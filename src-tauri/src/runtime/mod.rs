#[allow(dead_code)]
pub(crate) mod selector;
#[allow(dead_code)]
pub(crate) mod state;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[cfg(test)]
use crate::audio::SampleId;
use crate::settings::ValidatedVolume;
use crate::{
    audio::{AudioEngine, AudioEngineHandle, AudioEngineStatus, VolumeError},
    input::{self, InputListener, InputStatus, SoundEvent},
    pack::{PackInstallError, PackManager},
};
use selector::SoundSelector;
use state::{RuntimeAudioStatus, RuntimeGroupCounts, RuntimeInputStatus};
pub(crate) use state::{RuntimeControlError, RuntimeSnapshot};

struct RuntimeInner {
    sound_enabled: bool,
    volume: ValidatedVolume,
}

pub(crate) struct KeyForgeRuntime {
    inner: Arc<Mutex<RuntimeInner>>,
    input_status: RuntimeInputStatus,
    pack_id: String,
    pack_name: String,
    group_counts: RuntimeGroupCounts,
    _selector: Option<Arc<SoundSelector>>,
    _audio_engine: Option<AudioEngine>,
    audio_handle: Option<AudioEngineHandle>,
    _input_listener: Option<InputListener>,
    #[cfg(test)]
    test_played_samples: Mutex<Vec<SampleId>>,
    #[cfg(test)]
    test_volume_updates: Mutex<Vec<f32>>,
}

impl KeyForgeRuntime {
    pub(crate) fn new_unavailable() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RuntimeInner {
                sound_enabled: true,
                volume: ValidatedVolume::new(1.0).expect("default volume is valid"),
            })),
            input_status: RuntimeInputStatus::Unavailable,
            pack_id: "keyforge-mechanical".to_string(),
            pack_name: "KeyForge Mechanical".to_string(),
            group_counts: RuntimeGroupCounts::new(3, 1, 1, 1, 1),
            _selector: None,
            _audio_engine: None,
            audio_handle: None,
            _input_listener: None,
            #[cfg(test)]
            test_played_samples: Mutex::new(Vec::new()),
            #[cfg(test)]
            test_volume_updates: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn start(app_data_dir: PathBuf) -> Self {
        Self::try_start(app_data_dir).unwrap_or_else(|_| Self::new_unavailable())
    }

    fn try_start(app_data_dir: PathBuf) -> Result<Self, RuntimeStartError> {
        let audio_engine = AudioEngine::start().map_err(|_| RuntimeStartError::Audio)?;
        let audio_handle = audio_engine.handle();
        let manager =
            PackManager::open(app_data_dir.join("packs")).map_err(|_| RuntimeStartError::Pack)?;
        let installed = match manager.install_bundled_default() {
            Ok(installed) => installed,
            Err(PackInstallError::DuplicateId) => manager
                .discover()
                .map_err(|_| RuntimeStartError::Pack)?
                .into_iter()
                .find(|pack| pack.id().as_str() == "keyforge-mechanical")
                .ok_or(RuntimeStartError::Pack)?,
            Err(_) => return Err(RuntimeStartError::Pack),
        };
        let decoded = manager
            .decode(installed.id())
            .map_err(|_| RuntimeStartError::Pack)?;
        let registered = decoded
            .register(&audio_handle)
            .map_err(|_| RuntimeStartError::Register)?;
        let selector = Arc::new(SoundSelector::from_registered_pack(&registered));
        let inner = Arc::new(Mutex::new(RuntimeInner {
            sound_enabled: true,
            volume: ValidatedVolume::new(1.0).expect("default volume is valid"),
        }));
        let input_inner = inner.clone();
        let input_selector = selector.clone();
        let input_audio = audio_handle.clone();
        let input_listener = input::start_listener_for_platform(Box::new(move |event| {
            play_sanitized_event(&input_inner, &input_selector, &input_audio, event);
        }));
        let (input_listener, input_status) = match input_listener {
            Ok(listener) => (Some(listener), RuntimeInputStatus::Ready),
            Err(error) => (
                None,
                RuntimeInputStatus::from(InputStatus::from_error(error)),
            ),
        };
        let counts = installed.variant_counts();
        let runtime = Self {
            inner,
            input_status,
            pack_id: installed.id().as_str().to_string(),
            pack_name: installed.name().to_string(),
            group_counts: RuntimeGroupCounts::new(
                counts.normal(),
                counts.space(),
                counts.enter(),
                counts.backspace(),
                counts.modifier(),
            ),
            _selector: Some(selector),
            _audio_engine: Some(audio_engine),
            audio_handle: Some(audio_handle),
            _input_listener: input_listener,
            #[cfg(test)]
            test_played_samples: Mutex::new(Vec::new()),
            #[cfg(test)]
            test_volume_updates: Mutex::new(Vec::new()),
        };
        Ok(runtime)
    }

    #[cfg(test)]
    pub(crate) fn new_for_test() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RuntimeInner {
                sound_enabled: true,
                volume: ValidatedVolume::new(1.0).unwrap(),
            })),
            input_status: RuntimeInputStatus::Ready,
            pack_id: "keyforge-mechanical".to_string(),
            pack_name: "KeyForge Mechanical".to_string(),
            group_counts: RuntimeGroupCounts::new(3, 1, 1, 1, 1),
            _selector: None,
            _audio_engine: None,
            audio_handle: None,
            _input_listener: None,
            test_played_samples: Mutex::new(Vec::new()),
            test_volume_updates: Mutex::new(Vec::new()),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_test_samples(normal: impl IntoIterator<Item = SampleId>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RuntimeInner {
                sound_enabled: true,
                volume: ValidatedVolume::new(1.0).unwrap(),
            })),
            input_status: RuntimeInputStatus::Ready,
            pack_id: "keyforge-mechanical".to_string(),
            pack_name: "KeyForge Mechanical".to_string(),
            group_counts: RuntimeGroupCounts::new(1, 0, 0, 0, 0),
            _selector: Some(Arc::new(SoundSelector::from_groups_for_test(
                normal,
                std::iter::empty(),
                std::iter::empty(),
                std::iter::empty(),
                std::iter::empty(),
            ))),
            _audio_engine: None,
            audio_handle: None,
            _input_listener: None,
            test_played_samples: Mutex::new(Vec::new()),
            test_volume_updates: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn snapshot(&self) -> RuntimeSnapshot {
        let inner = self.inner.lock().expect("runtime state mutex poisoned");
        RuntimeSnapshot::new(
            self.audio_status(),
            self.group_counts.clone(),
            self.input_status,
            self.pack_id.clone(),
            self.pack_name.clone(),
            inner.sound_enabled,
            inner.volume,
        )
    }

    pub(crate) fn set_enabled(&self, enabled: bool) -> RuntimeSnapshot {
        let mut inner = self.inner.lock().expect("runtime state mutex poisoned");
        inner.sound_enabled = enabled;
        drop(inner);
        self.snapshot()
    }

    pub(crate) fn set_volume(&self, volume: f32) -> Result<RuntimeSnapshot, RuntimeControlError> {
        let volume = ValidatedVolume::new(volume).map_err(RuntimeControlError::from)?;
        if let Some(handle) = &self.audio_handle {
            handle
                .set_master_volume(volume.get())
                .map_err(RuntimeControlError::from)?;
        }
        #[cfg(test)]
        self.test_volume_updates
            .lock()
            .expect("test volume mutex poisoned")
            .push(volume.get());
        let mut inner = self.inner.lock().expect("runtime state mutex poisoned");
        inner.volume = volume;
        drop(inner);
        Ok(self.snapshot())
    }

    #[cfg(test)]
    pub(crate) fn handle_sound_event(&self, event: SoundEvent) {
        let Some(selector) = &self._selector else {
            return;
        };
        let Some(sample_id) = selector.select(event) else {
            return;
        };
        if !self
            .inner
            .lock()
            .expect("runtime state mutex poisoned")
            .sound_enabled
        {
            return;
        }
        #[cfg(test)]
        self.test_played_samples
            .lock()
            .expect("test played samples mutex poisoned")
            .push(sample_id);
        if let Some(handle) = &self.audio_handle {
            let _ = handle.play(sample_id);
        }
    }

    fn audio_status(&self) -> RuntimeAudioStatus {
        self.audio_handle
            .as_ref()
            .map(|handle| RuntimeAudioStatus::from(handle.status()))
            .unwrap_or(RuntimeAudioStatus::Unavailable)
    }

    #[cfg(test)]
    pub(crate) fn played_samples_for_test(&self) -> Vec<SampleId> {
        self.test_played_samples
            .lock()
            .expect("test played samples mutex poisoned")
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn volume_updates_for_test(&self) -> Vec<f32> {
        self.test_volume_updates
            .lock()
            .expect("test volume mutex poisoned")
            .clone()
    }
}

#[derive(Debug)]
enum RuntimeStartError {
    Audio,
    Pack,
    Register,
}

fn play_sanitized_event(
    inner: &Arc<Mutex<RuntimeInner>>,
    selector: &SoundSelector,
    audio_handle: &AudioEngineHandle,
    event: SoundEvent,
) {
    if !inner
        .lock()
        .expect("runtime state mutex poisoned")
        .sound_enabled
    {
        return;
    }
    if let Some(sample_id) = selector.select(event) {
        let _ = audio_handle.play(sample_id);
    }
}

impl From<AudioEngineStatus> for RuntimeAudioStatus {
    fn from(status: AudioEngineStatus) -> Self {
        match status {
            AudioEngineStatus::Starting => Self::Starting,
            AudioEngineStatus::Ready => Self::Ready,
            AudioEngineStatus::Recovering => Self::Recovering,
            AudioEngineStatus::Unavailable => Self::Unavailable,
            AudioEngineStatus::Stopped => Self::Stopped,
        }
    }
}

impl From<InputStatus> for RuntimeInputStatus {
    fn from(status: InputStatus) -> Self {
        match status {
            InputStatus::Ready => Self::Ready,
            InputStatus::Unsupported => Self::Unsupported,
            InputStatus::PermissionDenied => Self::PermissionDenied,
            InputStatus::Unavailable => Self::Unavailable,
        }
    }
}

impl From<VolumeError> for RuntimeControlError {
    fn from(error: VolumeError) -> Self {
        match error {
            VolumeError::Invalid => Self::InvalidVolume,
            VolumeError::Stopped => Self::AudioUnavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{audio::SampleId, input::SoundEvent};

    #[test]
    fn disabled_runtime_ignores_sound_events() {
        let runtime = KeyForgeRuntime::new_with_test_samples([SampleId::from_raw_for_test(7)]);
        runtime.set_enabled(false);

        runtime.handle_sound_event(SoundEvent::Normal);

        assert_eq!(runtime.played_samples_for_test(), Vec::<SampleId>::new());
    }

    #[test]
    fn enabled_runtime_plays_selected_sanitized_event() {
        let runtime = KeyForgeRuntime::new_with_test_samples([SampleId::from_raw_for_test(7)]);

        runtime.handle_sound_event(SoundEvent::Normal);

        assert_eq!(
            runtime.played_samples_for_test(),
            vec![SampleId::from_raw_for_test(7)]
        );
    }

    #[test]
    fn volume_update_reaches_playback_sink_and_snapshot() {
        let runtime = KeyForgeRuntime::new_with_test_samples([SampleId::from_raw_for_test(7)]);

        assert_eq!(runtime.set_volume(0.25).unwrap().volume, 0.25);

        assert_eq!(runtime.volume_updates_for_test(), vec![0.25]);
    }
}
