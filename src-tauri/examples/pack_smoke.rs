use std::{
    error::Error,
    ffi::OsStr,
    fmt, fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use keyforge_lib::{
    audio::{AudioEngine, AudioEngineHandle, AudioEngineStatus, SampleId},
    pack::{DecodedPack, PackManager, RegisteredPack},
};

const TEMP_ROOT_PREFIX: &str = "keyforge-pack-smoke-";
const PLAYBACK_DELAY: Duration = Duration::from_millis(180);
const READY_TIMEOUT: Duration = Duration::from_secs(5);
const READY_POLL_DELAY: Duration = Duration::from_millis(20);

#[derive(Debug)]
enum SmokeError {
    TempRootUnavailable,
    TempRootAlreadyExists,
    PackStorageUnavailable,
    PackInstallFailed,
    PackDecodeFailed,
    AudioStartFailed,
    AudioOutputUnavailable,
    PackRegistrationFailed,
    PlaybackFailed,
    AudioShutdownFailed,
    CleanupFailed,
}

impl fmt::Display for SmokeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::TempRootUnavailable => "pack smoke temporary storage unavailable",
            Self::TempRootAlreadyExists => "pack smoke temporary storage already exists",
            Self::PackStorageUnavailable => "pack smoke storage unavailable",
            Self::PackInstallFailed => "pack smoke bundled pack installation failed",
            Self::PackDecodeFailed => "pack smoke bundled pack decoding failed",
            Self::AudioStartFailed => "pack smoke audio startup failed",
            Self::AudioOutputUnavailable => "pack smoke audio output unavailable",
            Self::PackRegistrationFailed => "pack smoke sample registration failed",
            Self::PlaybackFailed => "pack smoke playback failed",
            Self::AudioShutdownFailed => "pack smoke audio shutdown failed",
            Self::CleanupFailed => "pack smoke temporary storage cleanup failed",
        };
        formatter.write_str(message)
    }
}

impl Error for SmokeError {}

struct TempRoot {
    root: PathBuf,
    parent: PathBuf,
    name: String,
    cleaned: bool,
}

impl TempRoot {
    fn create() -> Result<Self, SmokeError> {
        let parent = std::env::temp_dir();
        let name = format!("{TEMP_ROOT_PREFIX}{}", std::process::id());
        let root = parent.join(&name);

        if root
            .try_exists()
            .map_err(|_| SmokeError::TempRootUnavailable)?
        {
            return Err(SmokeError::TempRootAlreadyExists);
        }
        fs::create_dir(&root).map_err(|_| SmokeError::TempRootUnavailable)?;

        Ok(Self {
            root,
            parent,
            name,
            cleaned: false,
        })
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn cleanup(&mut self) -> Result<(), SmokeError> {
        if self.cleaned {
            return Ok(());
        }
        self.remove_exact_root()?;
        self.cleaned = true;
        Ok(())
    }

    fn remove_exact_root(&self) -> Result<(), SmokeError> {
        if !self.name.starts_with(TEMP_ROOT_PREFIX)
            || self.root.parent() != Some(self.parent.as_path())
            || self.root.file_name() != Some(OsStr::new(&self.name))
            || self.root != self.parent.join(&self.name)
        {
            return Err(SmokeError::CleanupFailed);
        }
        fs::remove_dir_all(&self.root).map_err(|_| SmokeError::CleanupFailed)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn main() -> Result<(), SmokeError> {
    let mut temp_root = TempRoot::create()?;
    let smoke_result = run_smoke(temp_root.path());
    let cleanup_result = temp_root.cleanup();

    match (smoke_result, cleanup_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

trait SmokeLifecycleBackend {
    fn start(&mut self) -> Result<(), SmokeError>;
    fn register(&mut self) -> Result<(), SmokeError>;
    fn wait_until_ready(&mut self) -> Result<(), SmokeError>;
    fn play(&mut self) -> Result<(), SmokeError>;
    fn shutdown(&mut self) -> Result<(), SmokeError>;
}

struct ProductionSmokeBackend {
    decoded: Option<DecodedPack>,
    engine: Option<AudioEngine>,
    handle: Option<AudioEngineHandle>,
    registered: Option<RegisteredPack>,
}

impl ProductionSmokeBackend {
    fn new(decoded: DecodedPack) -> Self {
        Self {
            decoded: Some(decoded),
            engine: None,
            handle: None,
            registered: None,
        }
    }
}

impl SmokeLifecycleBackend for ProductionSmokeBackend {
    fn start(&mut self) -> Result<(), SmokeError> {
        let engine = AudioEngine::start().map_err(|_| SmokeError::AudioStartFailed)?;
        self.handle = Some(engine.handle());
        self.engine = Some(engine);
        Ok(())
    }

    fn register(&mut self) -> Result<(), SmokeError> {
        let decoded = self
            .decoded
            .take()
            .ok_or(SmokeError::PackRegistrationFailed)?;
        let handle = self
            .handle
            .as_ref()
            .ok_or(SmokeError::PackRegistrationFailed)?;
        self.registered = Some(
            decoded
                .register(handle)
                .map_err(|_| SmokeError::PackRegistrationFailed)?,
        );
        Ok(())
    }

    fn wait_until_ready(&mut self) -> Result<(), SmokeError> {
        let handle = self
            .handle
            .as_ref()
            .ok_or(SmokeError::AudioOutputUnavailable)?;
        wait_until_ready(handle)
    }

    fn play(&mut self) -> Result<(), SmokeError> {
        let handle = self.handle.as_ref().ok_or(SmokeError::PlaybackFailed)?;
        let registered = self.registered.as_ref().ok_or(SmokeError::PlaybackFailed)?;
        play_registered_pack(handle, registered)
    }

    fn shutdown(&mut self) -> Result<(), SmokeError> {
        let engine = self.engine.take().ok_or(SmokeError::AudioShutdownFailed)?;
        let result = engine
            .shutdown()
            .map_err(|_| SmokeError::AudioShutdownFailed);
        self.handle = None;
        self.registered = None;
        result
    }
}

fn run_audio_lifecycle(backend: &mut impl SmokeLifecycleBackend) -> Result<(), SmokeError> {
    backend.start()?;
    let smoke_result = (|| {
        backend.register()?;
        backend.wait_until_ready()?;
        backend.play()
    })();
    let shutdown_result = backend.shutdown();

    smoke_result.and(shutdown_result)
}

fn run_smoke(root: &Path) -> Result<(), SmokeError> {
    let manager =
        PackManager::open(root.to_path_buf()).map_err(|_| SmokeError::PackStorageUnavailable)?;
    let installed = manager
        .install_bundled_default()
        .map_err(|_| SmokeError::PackInstallFailed)?;
    let decoded = manager
        .decode(installed.id())
        .map_err(|_| SmokeError::PackDecodeFailed)?;
    let mut backend = ProductionSmokeBackend::new(decoded);
    run_audio_lifecycle(&mut backend)
}

fn play_registered_pack(
    handle: &AudioEngineHandle,
    registered: &RegisteredPack,
) -> Result<(), SmokeError> {
    let sounds = registered.sounds();

    play_normal_variants(handle, sounds.normal())?;
    play_group(handle, sounds.space(), "space")?;
    play_group(handle, sounds.enter(), "enter")?;
    play_group(handle, sounds.backspace(), "backspace")?;
    play_group(handle, sounds.modifier(), "modifier")?;

    println!("pack smoke complete");
    Ok(())
}

fn wait_until_ready(handle: &AudioEngineHandle) -> Result<(), SmokeError> {
    let started = Instant::now();
    wait_for_ready_with(|| handle.status(), || started.elapsed(), thread::sleep)
}

fn wait_for_ready_with<Status, Elapsed, Sleep>(
    mut status: Status,
    mut elapsed: Elapsed,
    mut sleep: Sleep,
) -> Result<(), SmokeError>
where
    Status: FnMut() -> AudioEngineStatus,
    Elapsed: FnMut() -> Duration,
    Sleep: FnMut(Duration),
{
    let mut has_polled = false;
    loop {
        if has_polled && elapsed() >= READY_TIMEOUT {
            return Err(SmokeError::AudioOutputUnavailable);
        }
        match status() {
            AudioEngineStatus::Ready => return Ok(()),
            AudioEngineStatus::Unavailable | AudioEngineStatus::Stopped => {
                return Err(SmokeError::AudioOutputUnavailable);
            }
            AudioEngineStatus::Starting | AudioEngineStatus::Recovering => {}
        }
        if elapsed() >= READY_TIMEOUT {
            return Err(SmokeError::AudioOutputUnavailable);
        }
        sleep(READY_POLL_DELAY);
        has_polled = true;
    }
}

fn play_normal_variants(
    handle: &AudioEngineHandle,
    samples: &[SampleId],
) -> Result<(), SmokeError> {
    for (index, sample) in samples.iter().enumerate() {
        println!("playing normal {}/{}", index + 1, samples.len());
        play_sample(handle, *sample)?;
    }
    Ok(())
}

fn play_group(
    handle: &AudioEngineHandle,
    samples: &[SampleId],
    label: &str,
) -> Result<(), SmokeError> {
    for sample in samples {
        println!("playing {label}");
        play_sample(handle, *sample)?;
    }
    Ok(())
}

fn play_sample(handle: &AudioEngineHandle, sample: SampleId) -> Result<(), SmokeError> {
    handle
        .play(sample)
        .map_err(|_| SmokeError::PlaybackFailed)?;
    thread::sleep(PLAYBACK_DELAY);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        run_audio_lifecycle, wait_for_ready_with, AudioEngineStatus, SmokeError,
        SmokeLifecycleBackend, READY_TIMEOUT,
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum LifecycleCall {
        Start,
        Register,
        Wait,
        Play,
        Shutdown,
    }

    struct FakeLifecycleBackend {
        calls: Vec<LifecycleCall>,
        failure: Option<LifecycleCall>,
    }

    impl FakeLifecycleBackend {
        fn succeeds() -> Self {
            Self {
                calls: Vec::new(),
                failure: None,
            }
        }

        fn fails_at(failure: LifecycleCall) -> Self {
            Self {
                calls: Vec::new(),
                failure: Some(failure),
            }
        }

        fn record(&mut self, call: LifecycleCall, error: SmokeError) -> Result<(), SmokeError> {
            self.calls.push(call);
            if self.failure == Some(call) {
                Err(error)
            } else {
                Ok(())
            }
        }
    }

    impl SmokeLifecycleBackend for FakeLifecycleBackend {
        fn start(&mut self) -> Result<(), SmokeError> {
            self.record(LifecycleCall::Start, SmokeError::AudioStartFailed)
        }

        fn register(&mut self) -> Result<(), SmokeError> {
            self.record(LifecycleCall::Register, SmokeError::PackRegistrationFailed)
        }

        fn wait_until_ready(&mut self) -> Result<(), SmokeError> {
            self.record(LifecycleCall::Wait, SmokeError::AudioOutputUnavailable)
        }

        fn play(&mut self) -> Result<(), SmokeError> {
            self.record(LifecycleCall::Play, SmokeError::PlaybackFailed)
        }

        fn shutdown(&mut self) -> Result<(), SmokeError> {
            self.record(LifecycleCall::Shutdown, SmokeError::AudioShutdownFailed)
        }
    }

    #[test]
    fn runs_the_real_smoke_lifecycle_in_required_order() {
        let mut backend = FakeLifecycleBackend::succeeds();

        let result = run_audio_lifecycle(&mut backend);

        assert!(result.is_ok());
        assert_eq!(
            backend.calls,
            [
                LifecycleCall::Start,
                LifecycleCall::Register,
                LifecycleCall::Wait,
                LifecycleCall::Play,
                LifecycleCall::Shutdown,
            ]
        );
    }

    #[test]
    fn shuts_down_after_registration_fails() {
        let mut backend = FakeLifecycleBackend::fails_at(LifecycleCall::Register);

        let result = run_audio_lifecycle(&mut backend);

        assert!(matches!(result, Err(SmokeError::PackRegistrationFailed)));
        assert_eq!(
            backend.calls,
            [
                LifecycleCall::Start,
                LifecycleCall::Register,
                LifecycleCall::Shutdown,
            ]
        );
    }

    #[test]
    fn shuts_down_after_waiting_for_ready_fails() {
        let mut backend = FakeLifecycleBackend::fails_at(LifecycleCall::Wait);

        let result = run_audio_lifecycle(&mut backend);

        assert!(matches!(result, Err(SmokeError::AudioOutputUnavailable)));
        assert_eq!(
            backend.calls,
            [
                LifecycleCall::Start,
                LifecycleCall::Register,
                LifecycleCall::Wait,
                LifecycleCall::Shutdown,
            ]
        );
    }

    #[test]
    fn shuts_down_after_playback_fails() {
        let mut backend = FakeLifecycleBackend::fails_at(LifecycleCall::Play);

        let result = run_audio_lifecycle(&mut backend);

        assert!(matches!(result, Err(SmokeError::PlaybackFailed)));
        assert_eq!(
            backend.calls,
            [
                LifecycleCall::Start,
                LifecycleCall::Register,
                LifecycleCall::Wait,
                LifecycleCall::Play,
                LifecycleCall::Shutdown,
            ]
        );
    }

    #[test]
    fn rejects_ready_exactly_at_the_deadline_after_a_previous_poll() {
        let mut statuses = [AudioEngineStatus::Starting, AudioEngineStatus::Ready].into_iter();
        let mut elapsed = [Duration::ZERO, READY_TIMEOUT].into_iter();
        let mut sleeps = 0;

        let result = wait_for_ready_with(
            || statuses.next().unwrap(),
            || elapsed.next().unwrap(),
            |_| sleeps += 1,
        );

        assert!(matches!(result, Err(SmokeError::AudioOutputUnavailable)));
        assert_eq!(sleeps, 1);
    }
}
