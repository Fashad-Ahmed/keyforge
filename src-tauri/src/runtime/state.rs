use crate::settings::{SettingsValidationError, ValidatedVolume};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeAudioStatus {
    Starting,
    Ready,
    Recovering,
    Unavailable,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeInputStatus {
    Starting,
    Ready,
    Unsupported,
    PermissionDenied,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeGroupCounts {
    normal: usize,
    space: usize,
    enter: usize,
    backspace: usize,
    modifier: usize,
}

impl RuntimeGroupCounts {
    pub(crate) fn new(
        normal: usize,
        space: usize,
        enter: usize,
        backspace: usize,
        modifier: usize,
    ) -> Self {
        Self {
            normal,
            space,
            enter,
            backspace,
            modifier,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeSnapshot {
    pub(crate) audio_status: RuntimeAudioStatus,
    pub(crate) group_counts: RuntimeGroupCounts,
    pub(crate) input_status: RuntimeInputStatus,
    pub(crate) pack_id: String,
    pub(crate) pack_name: String,
    pub(crate) sound_enabled: bool,
    pub(crate) volume: f32,
}

impl RuntimeSnapshot {
    pub(crate) fn new(
        audio_status: RuntimeAudioStatus,
        group_counts: RuntimeGroupCounts,
        input_status: RuntimeInputStatus,
        pack_id: String,
        pack_name: String,
        sound_enabled: bool,
        volume: ValidatedVolume,
    ) -> Self {
        Self {
            audio_status,
            group_counts,
            input_status,
            pack_id,
            pack_name,
            sound_enabled,
            volume: volume.get(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        Self::new(
            RuntimeAudioStatus::Ready,
            RuntimeGroupCounts::new(3, 1, 1, 1, 1),
            RuntimeInputStatus::Ready,
            "keyforge-mechanical".to_string(),
            "KeyForge Mechanical".to_string(),
            true,
            ValidatedVolume::new(1.0).unwrap(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeControlError {
    InvalidVolume,
    AudioUnavailable,
    PersistenceFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PackActionError {
    NotFound,
    ActivationFailed,
    PersistenceFailed,
    DuplicatePack,
    InvalidPack,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "status", content = "snapshot", rename_all = "snake_case")]
pub(crate) enum ImportOutcome {
    Cancelled,
    Installed(RuntimeSnapshot),
}

impl From<SettingsValidationError> for RuntimeControlError {
    fn from(error: SettingsValidationError) -> Self {
        match error {
            SettingsValidationError::InvalidVolume | SettingsValidationError::InvalidPackId => {
                Self::InvalidVolume
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_serializes_only_reviewed_fields() {
        let snapshot = RuntimeSnapshot::for_test();
        let value = serde_json::to_value(&snapshot).unwrap();
        let keys = value
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec![
                "audioStatus",
                "groupCounts",
                "inputStatus",
                "packId",
                "packName",
                "soundEnabled",
                "volume",
            ]
        );
    }

    #[test]
    fn rejects_invalid_volume() {
        assert_eq!(
            ValidatedVolume::new(-0.1).map_err(RuntimeControlError::from),
            Err(RuntimeControlError::InvalidVolume)
        );
        assert_eq!(
            ValidatedVolume::new(1.1).map_err(RuntimeControlError::from),
            Err(RuntimeControlError::InvalidVolume)
        );
        assert_eq!(
            ValidatedVolume::new(f32::NAN).map_err(RuntimeControlError::from),
            Err(RuntimeControlError::InvalidVolume)
        );
        assert!(ValidatedVolume::new(0.5).is_ok());
    }
}
