#[allow(dead_code)]
pub(crate) mod selector;
#[allow(dead_code)]
pub(crate) mod state;

use std::sync::Mutex;

use state::{RuntimeAudioStatus, RuntimeGroupCounts, RuntimeInputStatus};
pub(crate) use state::{RuntimeControlError, RuntimeSnapshot, ValidatedVolume};

struct RuntimeInner {
    sound_enabled: bool,
    volume: ValidatedVolume,
}

pub(crate) struct KeyForgeRuntime {
    inner: Mutex<RuntimeInner>,
    audio_status: RuntimeAudioStatus,
    input_status: RuntimeInputStatus,
    pack_id: String,
    pack_name: String,
    group_counts: RuntimeGroupCounts,
}

impl KeyForgeRuntime {
    #[cfg(test)]
    pub(crate) fn new_for_test() -> Self {
        Self {
            inner: Mutex::new(RuntimeInner {
                sound_enabled: true,
                volume: ValidatedVolume::new(1.0).unwrap(),
            }),
            audio_status: RuntimeAudioStatus::Ready,
            input_status: RuntimeInputStatus::Ready,
            pack_id: "keyforge-mechanical".to_string(),
            pack_name: "KeyForge Mechanical".to_string(),
            group_counts: RuntimeGroupCounts::new(3, 1, 1, 1, 1),
        }
    }

    pub(crate) fn snapshot(&self) -> RuntimeSnapshot {
        let inner = self.inner.lock().expect("runtime state mutex poisoned");
        RuntimeSnapshot::new(
            self.audio_status,
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
        let volume = ValidatedVolume::new(volume)?;
        let mut inner = self.inner.lock().expect("runtime state mutex poisoned");
        inner.volume = volume;
        drop(inner);
        Ok(self.snapshot())
    }
}
