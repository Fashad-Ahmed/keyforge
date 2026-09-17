#[allow(dead_code)]
pub(crate) mod catalog;
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
use crate::settings::{AppSettings, SettingsHealth, SettingsStore};
use crate::{
    audio::{AudioEngine, AudioEngineHandle, AudioEngineStatus, VolumeError},
    input::{self, InputListener, InputStatus, SoundEvent},
    pack::{PackId, PackInstallError, PackManager},
};
use catalog::{PackCatalog, PackSummary};
use selector::SoundSelector;
pub(crate) use state::{PackActionError, RuntimeControlError, RuntimeSnapshot};
use state::{RuntimeAudioStatus, RuntimeGroupCounts, RuntimeInputStatus};

struct RuntimeInner {
    sound_enabled: bool,
    volume: ValidatedVolume,
    pack_id: String,
    pack_name: String,
    group_counts: RuntimeGroupCounts,
    selector: Option<Arc<SoundSelector>>,
}

pub(crate) struct KeyForgeRuntime {
    inner: Arc<Mutex<RuntimeInner>>,
    input_status: RuntimeInputStatus,
    #[allow(dead_code)]
    pack_manager: Option<PackManager>,
    settings_store: Option<SettingsStore>,
    _settings_health: SettingsHealth,
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
                pack_id: "keyforge-mechanical".to_string(),
                pack_name: "KeyForge Mechanical".to_string(),
                group_counts: RuntimeGroupCounts::new(3, 1, 1, 1, 1),
                selector: None,
            })),
            input_status: RuntimeInputStatus::Unavailable,
            pack_manager: None,
            settings_store: None,
            _settings_health: SettingsHealth::Unavailable,
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
        let settings_store = SettingsStore::open(app_data_dir.clone());
        let loaded_settings = settings_store.load();
        let settings = loaded_settings.settings().clone();
        let manager =
            PackManager::open(app_data_dir.join("packs")).map_err(|_| RuntimeStartError::Pack)?;
        let bundled = match manager.install_bundled_default() {
            Ok(installed) => installed,
            Err(PackInstallError::DuplicateId) => manager
                .discover()
                .map_err(|_| RuntimeStartError::Pack)?
                .into_iter()
                .find(|pack| pack.id().as_str() == "keyforge-mechanical")
                .ok_or(RuntimeStartError::Pack)?,
            Err(_) => return Err(RuntimeStartError::Pack),
        };
        let selected = manager
            .discover()
            .map_err(|_| RuntimeStartError::Pack)?
            .into_iter()
            .find(|pack| pack.id() == settings.selected_pack_id())
            .unwrap_or_else(|| bundled.clone());
        let (installed, decoded) = match manager.decode(selected.id()) {
            Ok(decoded) => (selected, decoded),
            Err(_) => {
                let decoded = manager
                    .decode(bundled.id())
                    .map_err(|_| RuntimeStartError::Pack)?;
                (bundled, decoded)
            }
        };
        let registered = decoded
            .register(&audio_handle)
            .map_err(|_| RuntimeStartError::Register)?;
        audio_handle
            .set_master_volume(settings.master_volume().get())
            .map_err(|_| RuntimeStartError::Audio)?;
        let selector = Arc::new(SoundSelector::from_registered_pack(&registered));
        let inner = Arc::new(Mutex::new(RuntimeInner {
            sound_enabled: settings.sound_enabled(),
            volume: settings.master_volume(),
            pack_id: installed.id().as_str().to_string(),
            pack_name: installed.name().to_string(),
            group_counts: RuntimeGroupCounts::new(
                installed.variant_counts().normal(),
                installed.variant_counts().space(),
                installed.variant_counts().enter(),
                installed.variant_counts().backspace(),
                installed.variant_counts().modifier(),
            ),
            selector: Some(selector.clone()),
        }));
        let input_inner = inner.clone();
        let input_audio = audio_handle.clone();
        let input_listener = input::start_listener_for_platform(Box::new(move |event| {
            play_sanitized_event(&input_inner, &input_audio, event);
        }));
        let (input_listener, input_status) = match input_listener {
            Ok(listener) => (Some(listener), RuntimeInputStatus::Ready),
            Err(error) => (
                None,
                RuntimeInputStatus::from(InputStatus::from_error(error)),
            ),
        };
        let runtime = Self {
            inner,
            input_status,
            pack_manager: Some(manager),
            settings_store: Some(settings_store),
            _settings_health: loaded_settings.health(),
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
                pack_id: "keyforge-mechanical".to_string(),
                pack_name: "KeyForge Mechanical".to_string(),
                group_counts: RuntimeGroupCounts::new(3, 1, 1, 1, 1),
                selector: None,
            })),
            input_status: RuntimeInputStatus::Ready,
            pack_manager: None,
            settings_store: None,
            _settings_health: SettingsHealth::Defaulted,
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
                pack_id: "keyforge-mechanical".to_string(),
                pack_name: "KeyForge Mechanical".to_string(),
                group_counts: RuntimeGroupCounts::new(1, 0, 0, 0, 0),
                selector: Some(Arc::new(SoundSelector::from_groups_for_test(
                    normal,
                    std::iter::empty(),
                    std::iter::empty(),
                    std::iter::empty(),
                    std::iter::empty(),
                ))),
            })),
            input_status: RuntimeInputStatus::Ready,
            pack_manager: None,
            settings_store: None,
            _settings_health: SettingsHealth::Defaulted,
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
            inner.group_counts.clone(),
            self.input_status,
            inner.pack_id.clone(),
            inner.pack_name.clone(),
            inner.sound_enabled,
            inner.volume,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn select_pack(&self, pack_id: &str) -> Result<RuntimeSnapshot, PackActionError> {
        let manager = self
            .pack_manager
            .as_ref()
            .ok_or(PackActionError::NotFound)?;
        let audio_handle = self
            .audio_handle
            .as_ref()
            .ok_or(PackActionError::ActivationFailed)?;
        let pack_id = PackId::parse(pack_id).map_err(|_| PackActionError::NotFound)?;
        let decoded = manager
            .decode(&pack_id)
            .map_err(|_| PackActionError::NotFound)?;
        let registered = decoded
            .register(audio_handle)
            .map_err(|_| PackActionError::ActivationFailed)?;
        let metadata = registered.metadata();
        let counts = metadata.variant_counts();
        let mut inner = self.inner.lock().expect("runtime state mutex poisoned");
        inner.pack_id = metadata.id().as_str().to_owned();
        inner.pack_name = metadata.name().to_owned();
        inner.group_counts = RuntimeGroupCounts::new(
            counts.normal(),
            counts.space(),
            counts.enter(),
            counts.backspace(),
            counts.modifier(),
        );
        inner.selector = Some(Arc::new(SoundSelector::from_registered_pack(&registered)));
        let settings = AppSettings::new(inner.sound_enabled, inner.volume.get(), &inner.pack_id)
            .map_err(|_| PackActionError::PersistenceFailed)?;
        drop(inner);
        if let Some(store) = &self.settings_store {
            store
                .save(&settings)
                .map_err(|_| PackActionError::PersistenceFailed)?;
        }
        Ok(self.snapshot())
    }

    #[allow(dead_code)]
    pub(crate) fn catalog(&self) -> Result<PackCatalog, PackActionError> {
        let manager = self
            .pack_manager
            .as_ref()
            .ok_or(PackActionError::NotFound)?;
        let active_id = self
            .inner
            .lock()
            .expect("runtime state mutex poisoned")
            .pack_id
            .clone();
        let summaries = manager
            .discover()
            .map_err(|_| PackActionError::ActivationFailed)?
            .into_iter()
            .map(|pack| {
                let counts = pack.variant_counts();
                PackSummary::new(
                    pack.id().as_str(),
                    pack.name(),
                    pack.id().as_str() == "keyforge-mechanical",
                    pack.id().as_str() == active_id,
                    RuntimeGroupCounts::new(
                        counts.normal(),
                        counts.space(),
                        counts.enter(),
                        counts.backspace(),
                        counts.modifier(),
                    ),
                )
            })
            .collect();
        Ok(PackCatalog::new(summaries, &active_id))
    }

    pub(crate) fn set_enabled(
        &self,
        enabled: bool,
    ) -> Result<RuntimeSnapshot, RuntimeControlError> {
        let mut inner = self.inner.lock().expect("runtime state mutex poisoned");
        inner.sound_enabled = enabled;
        drop(inner);
        self.persist_current_settings()?;
        Ok(self.snapshot())
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
        self.persist_current_settings()?;
        Ok(self.snapshot())
    }

    fn persist_current_settings(&self) -> Result<(), RuntimeControlError> {
        let Some(store) = &self.settings_store else {
            return Ok(());
        };
        let inner = self.inner.lock().expect("runtime state mutex poisoned");
        let settings = AppSettings::new(inner.sound_enabled, inner.volume.get(), &inner.pack_id)
            .map_err(|_| RuntimeControlError::PersistenceFailed)?;
        drop(inner);
        store
            .save(&settings)
            .map_err(|_| RuntimeControlError::PersistenceFailed)
    }

    #[cfg(test)]
    pub(crate) fn handle_sound_event(&self, event: SoundEvent) {
        let inner = self.inner.lock().expect("runtime state mutex poisoned");
        if !inner.sound_enabled {
            return;
        }
        let Some(sample_id) = inner
            .selector
            .as_ref()
            .and_then(|selector| selector.select(event))
        else {
            return;
        };
        drop(inner);
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
    audio_handle: &AudioEngineHandle,
    event: SoundEvent,
) {
    let inner = inner.lock().expect("runtime state mutex poisoned");
    if !inner.sound_enabled {
        return;
    }
    let sample_id = inner
        .selector
        .as_ref()
        .and_then(|selector| selector.select(event));
    drop(inner);
    if let Some(sample_id) = sample_id {
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
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;
    use crate::{audio::SampleId, input::SoundEvent};

    static SETTINGS_TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn runtime_with_settings_store() -> (KeyForgeRuntime, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "keyforge-runtime-settings-test-{}-{}",
            std::process::id(),
            SETTINGS_TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let mut runtime = KeyForgeRuntime::new_for_test();
        runtime.settings_store = Some(SettingsStore::open(path.clone()));
        (runtime, path)
    }

    #[test]
    fn disabled_runtime_ignores_sound_events() {
        let runtime = KeyForgeRuntime::new_with_test_samples([SampleId::from_raw_for_test(7)]);
        runtime.set_enabled(false).unwrap();

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

    #[test]
    fn failed_pack_selection_preserves_the_active_pack() {
        let runtime = KeyForgeRuntime::new_for_test();
        let before = runtime.snapshot();

        assert_eq!(
            runtime.select_pack("missing-pack"),
            Err(PackActionError::NotFound)
        );
        assert_eq!(runtime.snapshot().pack_id, before.pack_id);
    }

    #[test]
    fn persists_enabled_state() {
        let (runtime, path) = runtime_with_settings_store();

        runtime.set_enabled(false).unwrap();

        assert!(!SettingsStore::open(path.clone())
            .load()
            .settings()
            .sound_enabled());
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn persists_volume_and_rejects_invalid_values_before_writing() {
        let (runtime, path) = runtime_with_settings_store();
        runtime.set_volume(0.25).unwrap();
        assert_eq!(
            SettingsStore::open(path.clone())
                .load()
                .settings()
                .master_volume()
                .get(),
            0.25
        );

        assert_eq!(
            runtime.set_volume(f32::NAN),
            Err(RuntimeControlError::InvalidVolume)
        );
        assert_eq!(
            SettingsStore::open(path.clone())
                .load()
                .settings()
                .master_volume()
                .get(),
            0.25
        );
        fs::remove_dir_all(path).unwrap();
    }
}
