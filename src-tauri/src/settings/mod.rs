mod storage;

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(unix)]
use std::fs::File;

use serde::{Deserialize, Serialize};

use crate::pack::PackId;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
const SETTINGS_VERSION: u32 = 1;
const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ValidatedVolume(f32);

impl ValidatedVolume {
    pub(crate) fn new(value: f32) -> Result<Self, SettingsValidationError> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(SettingsValidationError::InvalidVolume)
        }
    }

    pub(crate) fn get(self) -> f32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppSettings {
    sound_enabled: bool,
    master_volume: ValidatedVolume,
    selected_pack_id: PackId,
}

impl AppSettings {
    pub(crate) fn new(
        sound_enabled: bool,
        master_volume: f32,
        selected_pack_id: &str,
    ) -> Result<Self, SettingsValidationError> {
        Ok(Self {
            sound_enabled,
            master_volume: ValidatedVolume::new(master_volume)?,
            selected_pack_id: PackId::parse(selected_pack_id)
                .map_err(|_| SettingsValidationError::InvalidPackId)?,
        })
    }

    pub(crate) fn sound_enabled(&self) -> bool {
        self.sound_enabled
    }

    pub(crate) fn master_volume(&self) -> ValidatedVolume {
        self.master_volume
    }

    pub(crate) fn selected_pack_id(&self) -> &PackId {
        &self.selected_pack_id
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self::new(true, 1.0, "keyforge-switch-linear").expect("default settings are valid")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsHealth {
    Ready,
    Defaulted,
    Corrupt,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SettingsLoad {
    settings: AppSettings,
    health: SettingsHealth,
}

impl SettingsLoad {
    pub(crate) fn settings(&self) -> &AppSettings {
        &self.settings
    }

    pub(crate) fn health(&self) -> SettingsHealth {
        self.health
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsValidationError {
    InvalidVolume,
    InvalidPackId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsError {
    Write,
}

#[derive(Debug, Clone)]
pub(crate) struct SettingsStore {
    directory: PathBuf,
}

impl SettingsStore {
    pub(crate) fn open(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub(crate) fn load(&self) -> SettingsLoad {
        let path = self.settings_path();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return SettingsLoad {
                    settings: AppSettings::default(),
                    health: SettingsHealth::Defaulted,
                };
            }
            Err(_) => {
                return SettingsLoad {
                    settings: AppSettings::default(),
                    health: SettingsHealth::Unavailable,
                };
            }
        };

        match serde_json::from_slice::<SettingsWire>(&bytes)
            .ok()
            .and_then(SettingsWire::validate)
        {
            Some(settings) => SettingsLoad {
                settings,
                health: SettingsHealth::Ready,
            },
            None => SettingsLoad {
                settings: AppSettings::default(),
                health: SettingsHealth::Corrupt,
            },
        }
    }

    pub(crate) fn save(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        fs::create_dir_all(&self.directory).map_err(|_| SettingsError::Write)?;
        let wire = SettingsWire::from(settings);
        let bytes = serde_json::to_vec(&wire).map_err(|_| SettingsError::Write)?;
        let temporary_path = self.temporary_path();
        let result = self.write_and_replace(&temporary_path, &bytes);
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    fn write_and_replace(&self, temporary_path: &Path, bytes: &[u8]) -> Result<(), SettingsError> {
        let mut temporary = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(temporary_path)
            .map_err(|_| SettingsError::Write)?;
        temporary
            .write_all(bytes)
            .and_then(|()| temporary.sync_all())
            .map_err(|_| SettingsError::Write)?;
        drop(temporary);
        storage::replace_file(temporary_path, &self.settings_path())
            .map_err(|_| SettingsError::Write)?;
        sync_directory(&self.directory).map_err(|_| SettingsError::Write)
    }

    fn settings_path(&self) -> PathBuf {
        self.directory.join(SETTINGS_FILE_NAME)
    }

    fn temporary_path(&self) -> PathBuf {
        self.directory.join(format!(
            "settings.json.{}.{}.tmp",
            std::process::id(),
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SettingsWire {
    version: u32,
    sound_enabled: bool,
    master_volume: f32,
    selected_pack_id: String,
}

impl SettingsWire {
    fn validate(self) -> Option<AppSettings> {
        if self.version != SETTINGS_VERSION {
            return None;
        }
        AppSettings::new(
            self.sound_enabled,
            self.master_volume,
            &self.selected_pack_id,
        )
        .ok()
    }
}

impl From<&AppSettings> for SettingsWire {
    fn from(settings: &AppSettings) -> Self {
        Self {
            version: SETTINGS_VERSION,
            sound_enabled: settings.sound_enabled(),
            master_volume: settings.master_volume().get(),
            selected_pack_id: settings.selected_pack_id().as_str().to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{AppSettings, SettingsHealth, SettingsStore};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "keyforge-settings-test-{}-{}",
                std::process::id(),
                TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn settings_path(&self) -> PathBuf {
            self.0.join("settings.json")
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn missing_settings_load_defaults() {
        let root = TestRoot::new();

        let loaded = SettingsStore::open(root.0.clone()).load();

        assert_eq!(loaded.health(), SettingsHealth::Defaulted);
        assert_eq!(loaded.settings(), &AppSettings::default());
    }

    #[test]
    fn first_launch_defaults_to_the_real_recorded_mechanical_pack() {
        assert_eq!(
            AppSettings::default().selected_pack_id().as_str(),
            "keyforge-switch-linear"
        );
    }

    #[test]
    fn valid_settings_round_trip() {
        let root = TestRoot::new();
        let store = SettingsStore::open(root.0.clone());
        let settings = AppSettings::new(false, 0.35, "keyforge-mechanical").unwrap();

        store.save(&settings).unwrap();
        let loaded = store.load();

        assert_eq!(loaded.health(), SettingsHealth::Ready);
        assert_eq!(loaded.settings(), &settings);
    }

    #[test]
    fn corrupt_settings_default_without_overwriting_source() {
        let root = TestRoot::new();
        fs::write(root.settings_path(), b"{broken").unwrap();

        let loaded = SettingsStore::open(root.0.clone()).load();

        assert_eq!(loaded.health(), SettingsHealth::Corrupt);
        assert_eq!(loaded.settings(), &AppSettings::default());
        assert_eq!(fs::read(root.settings_path()).unwrap(), b"{broken");
    }

    #[test]
    fn rejects_unknown_fields_and_schema_versions() {
        let root = TestRoot::new();
        let store = SettingsStore::open(root.0.clone());
        fs::write(
            root.settings_path(),
            br#"{"version":1,"soundEnabled":true,"masterVolume":1.0,"selectedPackId":"keyforge-mechanical","extra":true}"#,
        )
        .unwrap();
        assert_eq!(store.load().health(), SettingsHealth::Corrupt);

        fs::write(
            root.settings_path(),
            br#"{"version":2,"soundEnabled":true,"masterVolume":1.0,"selectedPackId":"keyforge-mechanical"}"#,
        )
        .unwrap();
        assert_eq!(store.load().health(), SettingsHealth::Corrupt);
    }

    #[test]
    fn rejects_invalid_volume_and_pack_id() {
        assert!(AppSettings::new(true, f32::NAN, "keyforge-mechanical").is_err());
        assert!(AppSettings::new(true, 1.1, "keyforge-mechanical").is_err());
        assert!(AppSettings::new(true, 0.5, "../escape").is_err());
    }
}
