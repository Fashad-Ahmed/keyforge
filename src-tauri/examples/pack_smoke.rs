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
    pack::{PackManager, RegisteredPack},
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

fn run_smoke(root: &Path) -> Result<(), SmokeError> {
    let manager =
        PackManager::open(root.to_path_buf()).map_err(|_| SmokeError::PackStorageUnavailable)?;
    let installed = manager
        .install_bundled_default()
        .map_err(|_| SmokeError::PackInstallFailed)?;
    let decoded = manager
        .decode(installed.id())
        .map_err(|_| SmokeError::PackDecodeFailed)?;
    let engine = AudioEngine::start().map_err(|_| SmokeError::AudioStartFailed)?;
    let handle = engine.handle();
    let smoke_result = (|| {
        wait_until_ready(&handle)?;
        let registered = decoded
            .register(&handle)
            .map_err(|_| SmokeError::PackRegistrationFailed)?;
        play_registered_pack(&handle, &registered)
    })();
    let shutdown_result = engine
        .shutdown()
        .map_err(|_| SmokeError::AudioShutdownFailed);

    smoke_result?;
    shutdown_result
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
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        match handle.status() {
            AudioEngineStatus::Ready => return Ok(()),
            AudioEngineStatus::Unavailable | AudioEngineStatus::Stopped => {
                return Err(SmokeError::AudioOutputUnavailable);
            }
            AudioEngineStatus::Starting | AudioEngineStatus::Recovering => {}
        }
        if Instant::now() >= deadline {
            return Err(SmokeError::AudioOutputUnavailable);
        }
        thread::sleep(READY_POLL_DELAY);
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
