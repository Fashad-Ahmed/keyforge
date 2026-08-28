#[allow(dead_code)]
mod archive;
#[allow(dead_code)]
mod decoder;
mod manifest;
#[allow(dead_code)]
mod storage;

use std::{
    fmt,
    fs::File,
    io::Cursor,
    path::{Path, PathBuf},
};

use crate::audio::{AudioEngineHandle, PcmSample, RegisterSampleError, SampleId};

#[cfg(test)]
mod test_support;

pub use archive::PackArchiveError;
#[allow(unused_imports)]
pub(crate) use archive::{
    inspect_archive, read_entry_bounded, validate_archive, ArchiveEntry, ArchiveInventory,
};
pub use decoder::PackDecodeError;
pub use manifest::{
    parse_manifest, CanonicalSoundPath, PackId, PackManifestError, PackVersion, SoundMap,
    ValidatedManifest,
};
pub use storage::PackStorageError;
#[allow(unused_imports)]
pub(crate) use storage::{PackStorage, StoredDecodedPack, StoredPackMetadata};

pub const MAX_DECODED_PACK_BYTES: usize = 64 * 1024 * 1024;

const BUNDLED_DEFAULT_PACK: &[u8] = include_bytes!("../../assets/packs/keyforge-mechanical.zip");

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ValidatedPack {
    manifest: ValidatedManifest,
    sounds: SoundMap<decoder::DecodedAudio>,
    expanded_bytes: u64,
    decoded_bytes: usize,
}

#[allow(dead_code)]
impl ValidatedPack {
    pub(crate) fn manifest(&self) -> &ValidatedManifest {
        &self.manifest
    }

    pub(crate) fn sounds(&self) -> &SoundMap<decoder::DecodedAudio> {
        &self.sounds
    }

    pub(crate) fn expanded_bytes(&self) -> u64 {
        self.expanded_bytes
    }

    pub(crate) fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackInstallError {
    ArchiveIo,
    Archive(PackArchiveError),
    Manifest(PackManifestError),
    Decode(PackDecodeError),
    Storage(PackStorageError),
    DuplicateId,
}

impl fmt::Display for PackInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let category = match self {
            Self::ArchiveIo => "archive I/O",
            Self::Archive(PackArchiveError::TooLarge) => "archive too large",
            Self::Archive(_) => "invalid archive",
            Self::Manifest(_) => "manifest",
            Self::Decode(_) => "decode",
            Self::Storage(_) => "storage",
            Self::DuplicateId => "duplicate id",
        };
        write!(formatter, "sound-pack installation failed: {category}")
    }
}

impl std::error::Error for PackInstallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ArchiveIo => None,
            Self::Archive(error) => Some(error),
            Self::Manifest(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::DuplicateId => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundGroupCounts {
    normal: usize,
    space: usize,
    enter: usize,
    backspace: usize,
    modifier: usize,
}

impl SoundGroupCounts {
    pub fn normal(&self) -> usize {
        self.normal
    }

    pub fn space(&self) -> usize {
        self.space
    }

    pub fn enter(&self) -> usize {
        self.enter
    }

    pub fn backspace(&self) -> usize {
        self.backspace
    }

    pub fn modifier(&self) -> usize {
        self.modifier
    }

    pub fn total(&self) -> usize {
        self.normal + self.space + self.enter + self.backspace + self.modifier
    }
}

impl From<[usize; 5]> for SoundGroupCounts {
    fn from(counts: [usize; 5]) -> Self {
        let [normal, space, enter, backspace, modifier] = counts;
        Self {
            normal,
            space,
            enter,
            backspace,
            modifier,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPack {
    id: PackId,
    name: String,
    pack_version: PackVersion,
    variant_counts: SoundGroupCounts,
}

impl InstalledPack {
    pub fn id(&self) -> &PackId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn pack_version(&self) -> PackVersion {
        self.pack_version
    }

    pub fn variant_counts(&self) -> &SoundGroupCounts {
        &self.variant_counts
    }
}

impl From<StoredPackMetadata> for InstalledPack {
    fn from(metadata: StoredPackMetadata) -> Self {
        Self {
            id: metadata.id,
            name: metadata.name,
            pack_version: metadata.pack_version,
            variant_counts: metadata.variant_counts.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedPack {
    metadata: InstalledPack,
    sounds: SoundMap<PcmSample>,
}

impl DecodedPack {
    pub fn metadata(&self) -> &InstalledPack {
        &self.metadata
    }

    pub fn register(self, handle: &AudioEngineHandle) -> Result<RegisteredPack, PackRegisterError> {
        let samples = self.sounds.iter().cloned().collect();
        let ids = handle.register_samples(samples)?;
        self.with_registered_ids(ids)
    }

    fn with_registered_ids(self, ids: Vec<SampleId>) -> Result<RegisteredPack, PackRegisterError> {
        if ids.len() != self.sounds.total_len() {
            return Err(PackRegisterError::RegistryInvariant);
        }
        let mut ids = ids.into_iter();
        let sounds = self
            .sounds
            .try_map(|_| ids.next().ok_or(PackRegisterError::RegistryInvariant))?;
        if ids.next().is_some() {
            return Err(PackRegisterError::RegistryInvariant);
        }
        Ok(RegisteredPack {
            metadata: self.metadata,
            sounds,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredPack {
    metadata: InstalledPack,
    sounds: SoundMap<SampleId>,
}

impl RegisteredPack {
    pub fn metadata(&self) -> &InstalledPack {
        &self.metadata
    }

    pub fn sounds(&self) -> &SoundMap<SampleId> {
        &self.sounds
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackLoadError {
    InvalidInstalledPack,
    StorageUnavailable,
}

impl fmt::Display for PackLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let category = match self {
            Self::InvalidInstalledPack => "installed pack is invalid",
            Self::StorageUnavailable => "storage unavailable",
        };
        write!(formatter, "sound-pack load failed: {category}")
    }
}

impl std::error::Error for PackLoadError {}

impl From<PackStorageError> for PackLoadError {
    fn from(error: PackStorageError) -> Self {
        if error == PackStorageError::InstalledPack {
            Self::InvalidInstalledPack
        } else {
            Self::StorageUnavailable
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackRegisterError {
    Registry(RegisterSampleError),
    RegistryInvariant,
}

impl fmt::Display for PackRegisterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let category = match self {
            Self::Registry(RegisterSampleError::TooManySamples) => "registry sample limit",
            Self::Registry(RegisterSampleError::MemoryLimitExceeded) => "registry memory limit",
            Self::Registry(RegisterSampleError::IdentifierExhausted) => {
                "registry identifiers exhausted"
            }
            Self::Registry(RegisterSampleError::RegistryUnavailable) => "registry unavailable",
            Self::RegistryInvariant => "registry invariant",
        };
        write!(formatter, "sound-pack registration failed: {category}")
    }
}

impl std::error::Error for PackRegisterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Registry(error) => Some(error),
            Self::RegistryInvariant => None,
        }
    }
}

impl From<RegisterSampleError> for PackRegisterError {
    fn from(error: RegisterSampleError) -> Self {
        Self::Registry(error)
    }
}

pub struct PackManager {
    storage: PackStorage,
}

impl PackManager {
    pub fn open(root: PathBuf) -> Result<Self, PackStorageError> {
        PackStorage::open(root).map(|storage| Self { storage })
    }

    pub fn install_zip(&self, source: &Path) -> Result<InstalledPack, PackInstallError> {
        let file = File::open(source).map_err(|_| PackInstallError::ArchiveIo)?;
        let source_size = file
            .metadata()
            .map_err(|_| PackInstallError::ArchiveIo)?
            .len();
        if source_size > archive::MAX_ARCHIVE_BYTES {
            return Err(PackArchiveError::TooLarge.into());
        }
        let pack = validate_archive(file, source_size)?;
        self.storage
            .install_validated(pack)
            .map(InstalledPack::from)
            .map_err(PackInstallError::from)
    }

    pub fn install_bundled_default(&self) -> Result<InstalledPack, PackInstallError> {
        let cursor = Cursor::new(BUNDLED_DEFAULT_PACK);
        let pack = validate_archive(cursor, BUNDLED_DEFAULT_PACK.len() as u64)?;
        self.storage
            .install_validated(pack)
            .map(InstalledPack::from)
            .map_err(PackInstallError::from)
    }

    pub fn discover(&self) -> Result<Vec<InstalledPack>, PackStorageError> {
        let mut packs = self
            .storage
            .discover()?
            .into_iter()
            .map(InstalledPack::from)
            .collect::<Vec<_>>();
        packs.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
        Ok(packs)
    }

    pub fn decode(&self, id: &PackId) -> Result<DecodedPack, PackLoadError> {
        let StoredDecodedPack { metadata, sounds } = self.storage.load_installed(id)?;
        Ok(DecodedPack {
            metadata: metadata.into(),
            sounds,
        })
    }
}

impl From<PackArchiveError> for PackInstallError {
    fn from(error: PackArchiveError) -> Self {
        Self::Archive(error)
    }
}

impl From<PackManifestError> for PackInstallError {
    fn from(error: PackManifestError) -> Self {
        Self::Manifest(error)
    }
}

impl From<PackDecodeError> for PackInstallError {
    fn from(error: PackDecodeError) -> Self {
        Self::Decode(error)
    }
}

impl From<PackStorageError> for PackInstallError {
    fn from(error: PackStorageError) -> Self {
        if error == PackStorageError::DuplicateId {
            Self::DuplicateId
        } else {
            Self::Storage(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, panic::catch_unwind};

    use crate::audio::{AudioEngineHandle, PcmSample, RegisterSampleError, SampleId};

    use super::{
        archive::MAX_ARCHIVE_BYTES,
        test_support::{valid_pack_zip, valid_pack_zip_with_identity, TestRoot},
        PackArchiveError, PackInstallError, PackLoadError, PackManager, PackRegisterError,
    };

    const MAX_REGISTERED_BYTES: usize = 128 * 1024 * 1024;

    #[test]
    fn bundled_default_uses_the_production_validator() {
        let root = TestRoot::new();
        let manager = PackManager::open(root.path().join("managed")).unwrap();

        let installed = manager.install_bundled_default().unwrap();

        assert_eq!(installed.id().as_str(), "keyforge-mechanical");
        assert_eq!(installed.variant_counts().normal(), 3);
        assert_eq!(installed.variant_counts().space(), 1);
        assert_eq!(installed.variant_counts().enter(), 1);
        assert_eq!(installed.variant_counts().backspace(), 1);
        assert_eq!(installed.variant_counts().modifier(), 1);
        let decoded = manager.decode(installed.id()).unwrap();
        assert_eq!(decoded.metadata(), &installed);
        assert_eq!(decoded.metadata().variant_counts().total(), 7);
    }

    #[test]
    fn manager_installs_discovers_decodes_and_registers_in_fixed_group_order() {
        let root = TestRoot::new();
        let source = root.write_file("import.zip", &valid_pack_zip());
        let manager = PackManager::open(root.path().join("managed")).unwrap();

        let installed = manager.install_zip(&source).unwrap();
        assert_eq!(installed.id().as_str(), "keyforge-mechanical");
        assert_eq!(installed.name(), "KeyForge Mechanical");
        assert_eq!(installed.pack_version().to_string(), "1.0.0");
        assert_eq!(installed.variant_counts().normal(), 2);
        assert_eq!(installed.variant_counts().space(), 1);
        assert_eq!(installed.variant_counts().enter(), 1);
        assert_eq!(installed.variant_counts().backspace(), 1);
        assert_eq!(installed.variant_counts().modifier(), 1);
        assert_eq!(installed.variant_counts().total(), 6);
        assert_eq!(manager.discover().unwrap(), vec![installed.clone()]);

        let decoded = manager.decode(installed.id()).unwrap();
        assert_eq!(decoded.metadata(), &installed);
        let handle = AudioEngineHandle::new_for_test();
        let registered = decoded.register(&handle).unwrap();
        assert_eq!(registered.metadata(), &installed);
        assert_eq!(
            registered.sounds().normal(),
            &[
                SampleId::from_raw_for_test(1),
                SampleId::from_raw_for_test(2),
            ]
        );
        assert_eq!(
            registered.sounds().space(),
            &[SampleId::from_raw_for_test(3)]
        );
        assert_eq!(
            registered.sounds().enter(),
            &[SampleId::from_raw_for_test(4)]
        );
        assert_eq!(
            registered.sounds().backspace(),
            &[SampleId::from_raw_for_test(5)]
        );
        assert_eq!(
            registered.sounds().modifier(),
            &[SampleId::from_raw_for_test(6)]
        );
        assert_eq!(registered.sounds().total_len(), 6);
    }

    #[test]
    fn public_install_errors_are_sanitized() {
        let root = TestRoot::new();
        let source = root.write_file("private-name.zip", b"bad zip");
        let manager = PackManager::open(root.path().join("managed")).unwrap();

        let error = manager.install_zip(&source).unwrap_err();
        let display = error.to_string();

        assert!(!display.contains(root.path().to_string_lossy().as_ref()));
        assert!(!display.contains("private-name.zip"));
        assert_eq!(display, "sound-pack installation failed: invalid archive");
    }

    #[test]
    fn source_size_is_rejected_before_invalid_zip_bytes_are_parsed() {
        let root = TestRoot::new();
        let source = root.write_file("oversized-private.zip", b"not a ZIP archive");
        fs::OpenOptions::new()
            .write(true)
            .open(&source)
            .unwrap()
            .set_len(MAX_ARCHIVE_BYTES + 1)
            .unwrap();
        let manager = PackManager::open(root.path().join("managed")).unwrap();

        let error = manager.install_zip(&source).unwrap_err();

        assert_eq!(error, PackInstallError::Archive(PackArchiveError::TooLarge));
        assert_eq!(
            error.to_string(),
            "sound-pack installation failed: archive too large"
        );
    }

    #[test]
    fn nonexistent_source_is_a_sanitized_archive_io_error() {
        let root = TestRoot::new();
        let source = root.path().join("private-missing.zip");
        let manager = PackManager::open(root.path().join("managed")).unwrap();

        let error = manager.install_zip(&source).unwrap_err();
        let display = error.to_string();

        assert_eq!(error, PackInstallError::ArchiveIo);
        assert_eq!(display, "sound-pack installation failed: archive I/O");
        assert!(!display.contains(root.path().to_string_lossy().as_ref()));
        assert!(!display.contains("private-missing.zip"));
    }

    #[test]
    fn duplicate_install_leaves_the_canonical_installed_pack_unchanged() {
        let root = TestRoot::new();
        let source = root.write_file("import.zip", &valid_pack_zip());
        let managed = root.path().join("managed");
        let manager = PackManager::open(managed.clone()).unwrap();
        let installed = manager.install_zip(&source).unwrap();
        let canonical_manifest = managed.join(installed.id().as_str()).join("manifest.json");
        let before = fs::read(&canonical_manifest).unwrap();

        assert_eq!(
            manager.install_zip(&source),
            Err(PackInstallError::DuplicateId)
        );

        assert_eq!(fs::read(canonical_manifest).unwrap(), before);
        assert_eq!(manager.discover().unwrap(), vec![installed]);
    }

    #[test]
    fn discovery_is_sorted_by_exact_pack_id() {
        let root = TestRoot::new();
        let zeta = root.write_file(
            "zeta.zip",
            &valid_pack_zip_with_identity("zeta-pack", "Zeta Pack"),
        );
        let alpha = root.write_file(
            "alpha.zip",
            &valid_pack_zip_with_identity("alpha-pack", "Alpha Pack"),
        );
        let manager = PackManager::open(root.path().join("managed")).unwrap();
        manager.install_zip(&zeta).unwrap();
        manager.install_zip(&alpha).unwrap();

        let ids = manager
            .discover()
            .unwrap()
            .into_iter()
            .map(|pack| pack.id().as_str().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(ids, ["alpha-pack", "zeta-pack"]);
    }

    #[test]
    fn corrupted_installed_pack_returns_a_sanitized_load_error() {
        let root = TestRoot::new();
        let source = root.write_file("import.zip", &valid_pack_zip());
        let managed = root.path().join("managed");
        let manager = PackManager::open(managed.clone()).unwrap();
        let installed = manager.install_zip(&source).unwrap();
        let private_wav = managed
            .join(installed.id().as_str())
            .join("sounds/normal-01.wav");
        fs::write(&private_wav, b"corrupt installed WAV").unwrap();

        let error = manager.decode(installed.id()).unwrap_err();
        let display = error.to_string();

        assert_eq!(error, PackLoadError::InvalidInstalledPack);
        assert_eq!(display, "sound-pack load failed: installed pack is invalid");
        assert!(!display.contains(root.path().to_string_lossy().as_ref()));
        assert!(!display.contains("normal-01.wav"));
    }

    #[test]
    fn registration_is_all_or_nothing_when_registry_memory_is_insufficient() {
        let root = TestRoot::new();
        let source = root.write_file("import.zip", &valid_pack_zip());
        let manager = PackManager::open(root.path().join("managed")).unwrap();
        let installed = manager.install_zip(&source).unwrap();
        let decoded = manager.decode(installed.id()).unwrap();
        let handle = AudioEngineHandle::new_for_test();
        fill_registry_leaving_one_float(&handle);

        let error = decoded.register(&handle).unwrap_err();

        assert_eq!(
            error,
            PackRegisterError::Registry(RegisterSampleError::MemoryLimitExceeded)
        );
        assert_eq!(
            error.to_string(),
            "sound-pack registration failed: registry memory limit"
        );
        handle
            .register_sample(PcmSample::new(48_000, 1, vec![0.0]).unwrap())
            .unwrap();
        assert_eq!(
            handle.register_sample(PcmSample::new(48_000, 1, vec![0.0]).unwrap()),
            Err(RegisterSampleError::MemoryLimitExceeded)
        );
    }

    #[test]
    fn mismatched_registry_ids_return_an_invariant_error_without_panicking() {
        let root = TestRoot::new();
        let source = root.write_file("import.zip", &valid_pack_zip());
        let manager = PackManager::open(root.path().join("managed")).unwrap();
        let installed = manager.install_zip(&source).unwrap();
        let decoded = manager.decode(installed.id()).unwrap();

        let result = catch_unwind(|| decoded.with_registered_ids(Vec::new()));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Err(PackRegisterError::RegistryInvariant));
    }

    #[test]
    fn manager_adds_no_tauri_frontend_or_startup_boundary() {
        let source = include_str!("../lib.rs");
        let handler_macro = concat!("generate_", "handler!");
        let invoke_handler = concat!("invoke_", "handler");
        let unchanged_run = format!(
            r#"pub fn run() {{
    tauri::Builder::default()
        .{invoke_handler}(tauri::{handler_macro}[commands::app_info::get_app_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}}"#
        );

        assert!(source.contains(&unchanged_run));
        assert!(!source.contains("PackManager::open"));
        assert!(!source.contains("install_zip"));
        assert!(!source.contains("register_samples"));
    }

    fn fill_registry_leaving_one_float(handle: &AudioEngineHandle) {
        let mut remaining_floats = (MAX_REGISTERED_BYTES / size_of::<f32>()) - 1;
        let mut samples = Vec::new();
        while remaining_floats > 0 {
            let (channels, float_count) = if remaining_floats > 1_920_000 {
                (2, remaining_floats.min(3_840_000) & !1)
            } else {
                (1, remaining_floats)
            };
            samples.push(PcmSample::new(192_000, channels, vec![0.0; float_count]).unwrap());
            remaining_floats -= float_count;
        }
        handle.register_samples(samples).unwrap();
    }
}
