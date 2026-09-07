use std::{
    collections::HashSet,
    ffi::OsStr,
    fmt,
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, MutexGuard,
    },
};

use crate::audio::PcmSample;

use super::{
    archive::{MAX_EXPANDED_BYTES, MAX_WAV_ENTRY_BYTES},
    decoder::decode_wav,
    manifest::MANIFEST_LIMIT_BYTES,
    parse_manifest, PackId, PackVersion, SoundMap, ValidatedManifest, ValidatedPack,
    MAX_DECODED_PACK_BYTES,
};

const MANIFEST_NAME: &str = "manifest.json";
const SOUNDS_DIRECTORY_NAME: &str = "sounds";
const STAGE_PREFIX: &str = ".keyforge-stage-";
const STAGE_MARKER_NAME: &str = ".keyforge-owned-stage";
const STAGE_MARKER_BYTES: &[u8] = b"keyforge-stage-v1\n";

static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);
static PACK_STORAGE_MUTATION: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackStorageError {
    RootCreate,
    InvalidRoot,
    RootIdentity,
    Mutation,
    StageCreate,
    Write,
    Verification,
    Cleanup,
    Commit,
    InstalledPack,
    DuplicateId,
    #[cfg(test)]
    InjectedForTest,
}

impl fmt::Display for PackStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::RootCreate => "failed to create the managed sound-pack root",
            Self::InvalidRoot => "invalid managed sound-pack root",
            Self::RootIdentity => "managed sound-pack root identity changed",
            Self::Mutation => "sound-pack storage mutation is unavailable",
            Self::StageCreate => "failed to create a sound-pack stage",
            Self::Write => "failed to write canonical sound-pack data",
            Self::Verification => "staged sound-pack verification failed",
            Self::Cleanup => "sound-pack stage cleanup failed",
            Self::Commit => "sound-pack commit failed",
            Self::InstalledPack => "installed sound pack is invalid",
            Self::DuplicateId => "sound-pack identifier is already installed",
            #[cfg(test)]
            Self::InjectedForTest => "injected sound-pack storage failure",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for PackStorageError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredPackMetadata {
    pub(crate) id: PackId,
    pub(crate) name: String,
    pub(crate) pack_version: PackVersion,
    pub(crate) variant_counts: [usize; 5],
}

impl StoredPackMetadata {
    fn from_manifest(manifest: &ValidatedManifest) -> Self {
        let sounds = manifest.sounds();
        Self {
            id: manifest.id().clone(),
            name: manifest.name().to_owned(),
            pack_version: manifest.pack_version(),
            variant_counts: [
                sounds.normal().len(),
                sounds.space().len(),
                sounds.enter().len(),
                sounds.backspace().len(),
                sounds.modifier().len(),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoredDecodedPack {
    pub(crate) metadata: StoredPackMetadata,
    pub(crate) sounds: SoundMap<PcmSample>,
}

#[derive(Debug)]
pub(crate) struct PackStorage {
    root: PathBuf,
    root_fingerprint: RootFingerprint,
}

impl PackStorage {
    pub(crate) fn open(root: PathBuf) -> Result<Self, PackStorageError> {
        match fs::symlink_metadata(&root) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&root).map_err(|_| PackStorageError::RootCreate)?;
            }
            Err(_) => return Err(PackStorageError::InvalidRoot),
        }

        let metadata = fs::symlink_metadata(&root).map_err(|_| PackStorageError::InvalidRoot)?;
        if is_link_like(&metadata) || !metadata.is_dir() {
            return Err(PackStorageError::InvalidRoot);
        }
        let root = fs::canonicalize(&root).map_err(|_| PackStorageError::InvalidRoot)?;
        let metadata = fs::symlink_metadata(&root).map_err(|_| PackStorageError::InvalidRoot)?;
        let root_fingerprint =
            RootFingerprint::from_path(&root, &metadata).ok_or(PackStorageError::InvalidRoot)?;
        Ok(Self {
            root,
            root_fingerprint,
        })
    }

    pub(crate) fn install_validated(
        &self,
        pack: ValidatedPack,
    ) -> Result<StoredPackMetadata, PackStorageError> {
        let _mutation = self.mutation_guard()?;
        self.install_validated_using(pack, |_, _| Ok(()))
    }

    pub(crate) fn discover(&self) -> Result<Vec<StoredPackMetadata>, PackStorageError> {
        self.ensure_root_identity()?;
        let mut ids = Vec::new();
        let entries = fs::read_dir(&self.root).map_err(|_| PackStorageError::InstalledPack)?;
        for entry in entries {
            let entry = entry.map_err(|_| PackStorageError::InstalledPack)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Ok(id) = PackId::parse(name) else {
                continue;
            };
            let metadata =
                fs::symlink_metadata(entry.path()).map_err(|_| PackStorageError::InstalledPack)?;
            if is_link_like(&metadata) || !metadata.is_dir() {
                return Err(PackStorageError::InstalledPack);
            }
            ids.push(id);
        }
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids.into_iter()
            .map(|id| self.load_installed(&id).map(|pack| pack.metadata))
            .collect()
    }

    pub(crate) fn load_installed(
        &self,
        id: &PackId,
    ) -> Result<StoredDecodedPack, PackStorageError> {
        self.ensure_root_identity()?;
        let directory = self.root.join(id.as_str());
        load_installed_directory(&directory, id, InstalledLayout::Committed)
    }

    pub(crate) fn cleanup_stale_stages(&self) -> Result<usize, PackStorageError> {
        let _mutation = self.mutation_guard()?;
        self.ensure_root_identity()?;

        let mut owned_stages = Vec::new();
        let entries = fs::read_dir(&self.root).map_err(|_| PackStorageError::Cleanup)?;
        for entry in entries {
            let entry = entry.map_err(|_| PackStorageError::Cleanup)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if !name.starts_with(STAGE_PREFIX) {
                continue;
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|_| PackStorageError::Cleanup)?;
            if is_link_like(&metadata) {
                return Err(PackStorageError::Cleanup);
            }
            if !metadata.is_dir() {
                continue;
            }
            let marker = path.join(STAGE_MARKER_NAME);
            match fs::symlink_metadata(&marker) {
                Ok(metadata) if is_link_like(&metadata) => {
                    return Err(PackStorageError::Cleanup);
                }
                Ok(metadata) if metadata.is_file() => {
                    if exact_marker(&marker)? {
                        verify_direct_stage(&self.root, &path)?;
                        owned_stages.push(path);
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(PackStorageError::Cleanup),
            }
        }

        for stage in &owned_stages {
            remove_owned_stage(&self.root, stage, true)?;
        }
        Ok(owned_stages.len())
    }

    fn install_validated_using<F>(
        &self,
        pack: ValidatedPack,
        mut checkpoint: F,
    ) -> Result<StoredPackMetadata, PackStorageError>
    where
        F: FnMut(InstallCheckpoint, &Path) -> Result<(), PackStorageError>,
    {
        self.ensure_root_identity()?;
        let destination = self.root.join(pack.manifest().id().as_str());
        require_absent_destination(&destination)?;

        let mut stage = StageGuard::create(&self.root)?;
        checkpoint(InstallCheckpoint::StageCreated, stage.path())?;
        write_validated_pack(stage.path(), &pack)?;
        checkpoint(InstallCheckpoint::BeforeVerify, stage.path())?;

        let expected_metadata = StoredPackMetadata::from_manifest(pack.manifest());
        let staged =
            load_installed_directory(stage.path(), pack.manifest().id(), InstalledLayout::Stage)
                .map_err(|_| PackStorageError::Verification)?;
        #[cfg(test)]
        checkpoint(InstallCheckpoint::AfterReload, stage.path())?;
        let pcm_matches = staged.sounds.total_len() == pack.sounds().total_len()
            && staged
                .sounds
                .iter()
                .zip(pack.sounds().iter())
                .all(|(stored, validated)| stored == validated.sample());
        if staged.metadata != expected_metadata || !pcm_matches {
            return Err(PackStorageError::Verification);
        }

        stage.remove_marker()?;
        self.ensure_root_identity()?;
        require_absent_destination(&destination)?;

        // The process-wide mutex serializes every PackStorage mutation in this
        // process. It cannot make the absence check plus rename atomic against a
        // hostile same-user process; Unix may replace an empty raced-in directory,
        // while Windows normally rejects an occupied destination. M3 explicitly
        // defers cross-process/single-instance hardening.
        debug_assert_eq!(stage.path().parent(), Some(self.root.as_path()));
        debug_assert_eq!(destination.parent(), Some(self.root.as_path()));
        fs::rename(stage.path(), &destination).map_err(|_| PackStorageError::Commit)?;
        stage.mark_committed();
        Ok(staged.metadata)
    }

    fn ensure_root_identity(&self) -> Result<(), PackStorageError> {
        let metadata =
            fs::symlink_metadata(&self.root).map_err(|_| PackStorageError::RootIdentity)?;
        if is_link_like(&metadata) || !metadata.is_dir() {
            return Err(PackStorageError::RootIdentity);
        }
        let canonical = fs::canonicalize(&self.root).map_err(|_| PackStorageError::RootIdentity)?;
        let fingerprint = RootFingerprint::from_path(&self.root, &metadata)
            .ok_or(PackStorageError::RootIdentity)?;
        if canonical != self.root || fingerprint != self.root_fingerprint {
            return Err(PackStorageError::RootIdentity);
        }
        Ok(())
    }

    fn mutation_guard(&self) -> Result<MutexGuard<'static, ()>, PackStorageError> {
        PACK_STORAGE_MUTATION
            .lock()
            .map_err(|_| PackStorageError::Mutation)
    }

    #[cfg(test)]
    fn try_mutation_guard_for_test(&self) -> std::sync::TryLockResult<MutexGuard<'static, ()>> {
        PACK_STORAGE_MUTATION.try_lock()
    }

    #[cfg(test)]
    fn install_validated_with_fault(
        &self,
        pack: ValidatedPack,
        fault: StorageFault,
    ) -> Result<StoredPackMetadata, PackStorageError> {
        let _mutation = self.mutation_guard()?;
        let mut reached_comparison = false;
        let result = self.install_validated_using(pack, |point, stage| {
            if point == InstallCheckpoint::AfterReload {
                reached_comparison = true;
                return Ok(());
            }
            match (fault, point) {
                (StorageFault::ManifestAlreadyExists, InstallCheckpoint::StageCreated) => {
                    fs::write(stage.join(MANIFEST_NAME), b"occupied")
                        .map_err(|_| PackStorageError::InjectedForTest)
                }
                (StorageFault::BeforeVerify, InstallCheckpoint::BeforeVerify) => {
                    Err(PackStorageError::InjectedForTest)
                }
                (StorageFault::CorruptBeforeVerify, InstallCheckpoint::BeforeVerify) => fs::write(
                    stage.join(SOUNDS_DIRECTORY_NAME).join("normal-01.wav"),
                    b"not a WAV",
                )
                .map_err(|_| PackStorageError::InjectedForTest),
                (StorageFault::DifferentValidPcm, InstallCheckpoint::BeforeVerify) => fs::write(
                    stage.join(SOUNDS_DIRECTORY_NAME).join("normal-01.wav"),
                    crate::pack::test_support::wav_bytes(1, 48_000, &[42]),
                )
                .map_err(|_| PackStorageError::InjectedForTest),
                (StorageFault::DifferentValidMetadata, InstallCheckpoint::BeforeVerify) => {
                    let path = stage.join(MANIFEST_NAME);
                    let original =
                        fs::read(&path).map_err(|_| PackStorageError::InjectedForTest)?;
                    let original = std::str::from_utf8(&original)
                        .map_err(|_| PackStorageError::InjectedForTest)?;
                    let altered = original.replace(
                        "\"name\": \"KeyForge Mechanical\"",
                        "\"name\": \"Altered Mechanical\"",
                    );
                    if altered == original {
                        return Err(PackStorageError::InjectedForTest);
                    }
                    let altered = parse_manifest(altered.as_bytes())
                        .and_then(|manifest| manifest.canonical_json())
                        .map_err(|_| PackStorageError::InjectedForTest)?;
                    fs::write(path, altered).map_err(|_| PackStorageError::InjectedForTest)
                }
                _ => Ok(()),
            }
        });
        if matches!(
            fault,
            StorageFault::DifferentValidPcm | StorageFault::DifferentValidMetadata
        ) && !reached_comparison
        {
            Err(PackStorageError::InjectedForTest)
        } else {
            result
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallCheckpoint {
    StageCreated,
    BeforeVerify,
    #[cfg(test)]
    AfterReload,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageFault {
    ManifestAlreadyExists,
    BeforeVerify,
    CorruptBeforeVerify,
    DifferentValidPcm,
    DifferentValidMetadata,
}

fn require_absent_destination(destination: &Path) -> Result<(), PackStorageError> {
    match fs::symlink_metadata(destination) {
        Ok(_) => Err(PackStorageError::DuplicateId),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(PackStorageError::Commit),
    }
}

fn write_validated_pack(stage: &Path, pack: &ValidatedPack) -> Result<(), PackStorageError> {
    let manifest = pack
        .manifest()
        .canonical_json()
        .map_err(|_| PackStorageError::Write)?;
    write_new_and_sync(&stage.join(MANIFEST_NAME), &manifest)
        .map_err(|_| PackStorageError::Write)?;

    let sounds_directory = stage.join(SOUNDS_DIRECTORY_NAME);
    fs::create_dir(&sounds_directory).map_err(|_| PackStorageError::Write)?;
    if pack.manifest().sounds().total_len() != pack.sounds().total_len() {
        return Err(PackStorageError::Verification);
    }
    for (path, audio) in pack.manifest().sounds().iter().zip(pack.sounds().iter()) {
        let destination = sounds_directory.join(path.file_name());
        write_new_and_sync(&destination, audio.canonical_wav())
            .map_err(|_| PackStorageError::Write)?;
    }
    Ok(())
}

fn write_new_and_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstalledLayout {
    Stage,
    Committed,
}

fn load_installed_directory(
    directory: &Path,
    expected_id: &PackId,
    layout: InstalledLayout,
) -> Result<StoredDecodedPack, PackStorageError> {
    let directory_metadata =
        fs::symlink_metadata(directory).map_err(|_| PackStorageError::InstalledPack)?;
    if is_link_like(&directory_metadata) || !directory_metadata.is_dir() {
        return Err(PackStorageError::InstalledPack);
    }
    let canonical = fs::canonicalize(directory).map_err(|_| PackStorageError::InstalledPack)?;
    if canonical != directory {
        return Err(PackStorageError::InstalledPack);
    }

    let mut has_manifest = false;
    let mut has_sounds = false;
    let mut has_marker = false;
    for entry in fs::read_dir(directory).map_err(|_| PackStorageError::InstalledPack)? {
        let entry = entry.map_err(|_| PackStorageError::InstalledPack)?;
        let name = entry.file_name();
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|_| PackStorageError::InstalledPack)?;
        if name == OsStr::new(MANIFEST_NAME) {
            if has_manifest || is_link_like(&metadata) || !metadata.is_file() {
                return Err(PackStorageError::InstalledPack);
            }
            has_manifest = true;
        } else if name == OsStr::new(SOUNDS_DIRECTORY_NAME) {
            if has_sounds || is_link_like(&metadata) || !metadata.is_dir() {
                return Err(PackStorageError::InstalledPack);
            }
            has_sounds = true;
        } else if layout == InstalledLayout::Stage && name == OsStr::new(STAGE_MARKER_NAME) {
            if has_marker || is_link_like(&metadata) || !metadata.is_file() {
                return Err(PackStorageError::InstalledPack);
            }
            has_marker =
                exact_marker(&entry.path()).map_err(|_| PackStorageError::InstalledPack)?;
            if !has_marker {
                return Err(PackStorageError::InstalledPack);
            }
        } else {
            return Err(PackStorageError::InstalledPack);
        }
    }
    if !has_manifest
        || !has_sounds
        || (layout == InstalledLayout::Stage && !has_marker)
        || (layout == InstalledLayout::Committed && has_marker)
    {
        return Err(PackStorageError::InstalledPack);
    }

    let manifest_bytes =
        read_regular_file(&directory.join(MANIFEST_NAME), MANIFEST_LIMIT_BYTES as u64)?;
    let manifest = parse_manifest(&manifest_bytes).map_err(|_| PackStorageError::InstalledPack)?;
    let canonical_manifest = manifest
        .canonical_json()
        .map_err(|_| PackStorageError::InstalledPack)?;
    if manifest_bytes != canonical_manifest || manifest.id() != expected_id {
        return Err(PackStorageError::InstalledPack);
    }

    let sounds_directory = directory.join(SOUNDS_DIRECTORY_NAME);
    let mut expected_files = manifest
        .sounds()
        .iter()
        .map(|path| path.file_name())
        .collect::<HashSet<_>>();
    for entry in fs::read_dir(&sounds_directory).map_err(|_| PackStorageError::InstalledPack)? {
        let entry = entry.map_err(|_| PackStorageError::InstalledPack)?;
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|_| PackStorageError::InstalledPack)?;
        if is_link_like(&metadata) || !metadata.is_file() {
            return Err(PackStorageError::InstalledPack);
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(PackStorageError::InstalledPack);
        };
        if !expected_files.remove(name) {
            return Err(PackStorageError::InstalledPack);
        }
    }
    if !expected_files.is_empty() {
        return Err(PackStorageError::InstalledPack);
    }

    let mut expanded_bytes = manifest_bytes.len() as u64;
    let mut decoded_bytes = 0_usize;
    let sounds = manifest.sounds().clone().try_map(|path| {
        let bytes = read_regular_file(
            &sounds_directory.join(path.file_name()),
            MAX_WAV_ENTRY_BYTES,
        )?;
        if bytes.is_empty() {
            return Err(PackStorageError::InstalledPack);
        }
        expanded_bytes = expanded_bytes
            .checked_add(bytes.len() as u64)
            .ok_or(PackStorageError::InstalledPack)?;
        if expanded_bytes > MAX_EXPANDED_BYTES {
            return Err(PackStorageError::InstalledPack);
        }
        let decoded = decode_wav(&bytes).map_err(|_| PackStorageError::InstalledPack)?;
        if decoded.canonical_wav() != bytes {
            return Err(PackStorageError::InstalledPack);
        }
        decoded_bytes = decoded_bytes
            .checked_add(decoded.sample().byte_len())
            .ok_or(PackStorageError::InstalledPack)?;
        if decoded_bytes > MAX_DECODED_PACK_BYTES {
            return Err(PackStorageError::InstalledPack);
        }
        Ok(decoded.into_sample())
    })?;

    Ok(StoredDecodedPack {
        metadata: StoredPackMetadata::from_manifest(&manifest),
        sounds,
    })
}

fn read_regular_file(path: &Path, limit: u64) -> Result<Vec<u8>, PackStorageError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| PackStorageError::InstalledPack)?;
    if is_link_like(&metadata) || !metadata.is_file() || metadata.len() > limit {
        return Err(PackStorageError::InstalledPack);
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len()).map_err(|_| PackStorageError::InstalledPack)?,
    );
    File::open(path)
        .map_err(|_| PackStorageError::InstalledPack)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PackStorageError::InstalledPack)?;
    if bytes.len() as u64 != metadata.len() || bytes.len() as u64 > limit {
        return Err(PackStorageError::InstalledPack);
    }
    Ok(bytes)
}

struct StageGuard {
    root: PathBuf,
    path: PathBuf,
    marker: MarkerState,
    committed: bool,
}

impl StageGuard {
    fn create(root: &Path) -> Result<Self, PackStorageError> {
        loop {
            let counter = STAGE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!("{STAGE_PREFIX}{}-{counter}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => {
                    let mut guard = Self {
                        root: root.to_owned(),
                        path,
                        marker: MarkerState::Pending,
                        committed: false,
                    };
                    write_new_and_sync(&guard.path.join(STAGE_MARKER_NAME), STAGE_MARKER_BYTES)
                        .map_err(|_| PackStorageError::StageCreate)?;
                    guard.marker = MarkerState::Present;
                    return Ok(guard);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(PackStorageError::StageCreate),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn remove_marker(&mut self) -> Result<(), PackStorageError> {
        let marker = self.path.join(STAGE_MARKER_NAME);
        if self.marker != MarkerState::Present
            || !exact_marker(&marker).map_err(|_| PackStorageError::Commit)?
        {
            return Err(PackStorageError::Commit);
        }
        fs::remove_file(marker).map_err(|_| PackStorageError::Commit)?;
        self.marker = MarkerState::RemovedForCommit;
        Ok(())
    }

    fn mark_committed(&mut self) {
        self.committed = true;
    }
}

impl Drop for StageGuard {
    fn drop(&mut self) {
        if !self.committed {
            let require_marker = self.marker == MarkerState::Present;
            let _ = remove_owned_stage(&self.root, &self.path, require_marker);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkerState {
    Pending,
    Present,
    RemovedForCommit,
}

fn exact_marker(marker: &Path) -> Result<bool, PackStorageError> {
    let metadata = fs::symlink_metadata(marker).map_err(|_| PackStorageError::Cleanup)?;
    if is_link_like(&metadata)
        || !metadata.is_file()
        || metadata.len() != STAGE_MARKER_BYTES.len() as u64
    {
        return Ok(false);
    }
    let mut bytes = Vec::with_capacity(STAGE_MARKER_BYTES.len());
    File::open(marker)
        .map_err(|_| PackStorageError::Cleanup)?
        .take(STAGE_MARKER_BYTES.len() as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PackStorageError::Cleanup)?;
    Ok(bytes == STAGE_MARKER_BYTES)
}

fn verify_direct_stage(root: &Path, stage: &Path) -> Result<(), PackStorageError> {
    if stage.parent() != Some(root)
        || !stage
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(STAGE_PREFIX))
    {
        return Err(PackStorageError::Cleanup);
    }
    let metadata = fs::symlink_metadata(stage).map_err(|_| PackStorageError::Cleanup)?;
    if is_link_like(&metadata) || !metadata.is_dir() {
        return Err(PackStorageError::Cleanup);
    }
    let canonical = fs::canonicalize(stage).map_err(|_| PackStorageError::Cleanup)?;
    if canonical != stage {
        return Err(PackStorageError::Cleanup);
    }
    Ok(())
}

fn remove_owned_stage(
    root: &Path,
    stage: &Path,
    require_marker: bool,
) -> Result<(), PackStorageError> {
    // Type, marker, and canonical-containment checks prevent link following in
    // the supported malicious-pack threat model. They are necessarily separate
    // path operations: a hostile same-user process could race them without the
    // later single-instance/platform-handle hardening that M3 explicitly defers.
    verify_direct_stage(root, stage)?;
    if require_marker && !exact_marker(&stage.join(STAGE_MARKER_NAME))? {
        return Err(PackStorageError::Cleanup);
    }
    remove_tree_without_following_links(stage)
}

fn remove_tree_without_following_links(path: &Path) -> Result<(), PackStorageError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| PackStorageError::Cleanup)?;
    if is_link_like(&metadata) || !metadata.is_dir() {
        return remove_link_or_file(path);
    }
    for entry in fs::read_dir(path).map_err(|_| PackStorageError::Cleanup)? {
        let entry = entry.map_err(|_| PackStorageError::Cleanup)?;
        let child = entry.path();
        let metadata = fs::symlink_metadata(&child).map_err(|_| PackStorageError::Cleanup)?;
        if !is_link_like(&metadata) && metadata.is_dir() {
            remove_tree_without_following_links(&child)?;
        } else {
            remove_link_or_file(&child)?;
        }
    }
    fs::remove_dir(path).map_err(|_| PackStorageError::Cleanup)
}

fn remove_link_or_file(path: &Path) -> Result<(), PackStorageError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(_) => fs::remove_dir(path).map_err(|_| PackStorageError::Cleanup),
    }
}

fn is_link_like(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RootFingerprint {
    device: u64,
    inode: u64,
}

#[cfg(unix)]
impl RootFingerprint {
    fn from_path(_: &Path, metadata: &Metadata) -> Option<Self> {
        use std::os::unix::fs::MetadataExt;

        Some(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RootFingerprint {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

#[cfg(windows)]
impl RootFingerprint {
    fn from_path(path: &Path, _: &Metadata) -> Option<Self> {
        use std::{ffi::c_void, mem::size_of, os::windows::io::AsRawHandle};

        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{
                FileIdInfo, GetFileInformationByHandleEx, FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_INFO,
                FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            },
        };

        let handle = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .ok()?;
        let mut information = FILE_ID_INFO::default();
        // SAFETY: `handle` remains open for the call, `information` is a valid
        // writable FILE_ID_INFO, and the buffer size matches its concrete type.
        let succeeded = unsafe {
            GetFileInformationByHandleEx(
                handle.as_raw_handle() as HANDLE,
                FileIdInfo,
                (&raw mut information).cast::<c_void>(),
                u32::try_from(size_of::<FILE_ID_INFO>()).ok()?,
            )
        };
        if succeeded == 0 {
            return None;
        }
        Some(Self {
            volume_serial_number: information.VolumeSerialNumber,
            file_id: information.FileId.Identifier,
        })
    }
}

#[cfg(not(any(unix, windows)))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RootFingerprint;

#[cfg(not(any(unix, windows)))]
impl RootFingerprint {
    fn from_path(_: &Path, _: &Metadata) -> Option<Self> {
        Some(Self)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        io::Cursor,
        path::{Path, PathBuf},
        sync::TryLockError,
    };

    use zip::CompressionMethod;

    use super::*;
    use crate::pack::{
        test_support::{
            valid_pack_entries, valid_pack_zip, wav_with_junk_chunk, zip_with_entries, TestRoot,
        },
        validate_archive, PackInstallError,
    };

    #[test]
    fn installs_only_canonical_manifest_and_wavs() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let installed = storage
            .install_validated(validated_pack_with_metadata_chunk())
            .unwrap();

        assert_eq!(installed.id.as_str(), "keyforge-mechanical");
        assert_eq!(installed.variant_counts, [2, 1, 1, 1, 1]);
        let directory = managed.join("keyforge-mechanical");
        assert_eq!(directory_entries(&directory), ["manifest.json", "sounds"]);
        assert_eq!(
            directory_entries(&directory.join("sounds")),
            [
                "backspace-01.wav",
                "enter-01.wav",
                "modifier-01.wav",
                "normal-01.wav",
                "normal-02.wav",
                "space-01.wav",
            ]
        );
        assert!(!fs::read(directory.join("sounds/normal-01.wav"))
            .unwrap()
            .windows(4)
            .any(|bytes| bytes == b"JUNK"));
    }

    #[test]
    fn creates_an_absent_root_and_rejects_non_directories() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        PackStorage::open(managed.clone()).unwrap();
        assert!(managed.is_dir());

        let file = root.path().join("not-a-directory");
        fs::write(&file, b"data").unwrap();
        assert_eq!(
            PackStorage::open(file).unwrap_err(),
            PackStorageError::InvalidRoot
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symlink_root_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let root = TestRoot::new();
        let target = root.path().join("target");
        fs::create_dir(&target).unwrap();
        let managed = root.path().join("packs");
        symlink(&target, &managed).unwrap();

        assert_eq!(
            PackStorage::open(managed).unwrap_err(),
            PackStorageError::InvalidRoot
        );
        assert!(target.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_redirected_managed_root_before_staging() {
        use std::os::unix::fs::symlink;

        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let original = root.path().join("packs-original");
        fs::rename(&managed, &original).unwrap();
        let outside = root.path().join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, &managed).unwrap();

        assert_eq!(
            storage.install_validated(validated_pack()),
            Err(PackStorageError::RootIdentity)
        );
        assert!(directory_entries(&outside).is_empty());
    }

    #[test]
    fn rejects_a_recreated_root_with_the_same_canonical_path() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        fs::rename(&managed, root.path().join("packs-original")).unwrap();
        fs::create_dir(&managed).unwrap();

        assert_eq!(
            storage.install_validated(validated_pack()),
            Err(PackStorageError::RootIdentity)
        );
        assert!(directory_entries(&managed).is_empty());
    }

    #[test]
    fn duplicate_id_never_replaces_existing_files() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        storage.install_validated(validated_pack()).unwrap();
        let before = snapshot_tree(&managed);

        assert_eq!(
            storage.install_validated(validated_pack()),
            Err(PackStorageError::DuplicateId)
        );
        assert_eq!(snapshot_tree(&managed), before);
        assert_eq!(
            PackInstallError::from(PackStorageError::DuplicateId),
            PackInstallError::DuplicateId
        );
    }

    #[test]
    fn process_mutex_serializes_distinct_storage_instances_for_the_same_root() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let first_storage = PackStorage::open(managed.clone()).unwrap();
        let second_storage = PackStorage::open(managed).unwrap();

        let first_guard = first_storage.mutation_guard().unwrap();
        assert!(matches!(
            second_storage.try_mutation_guard_for_test(),
            Err(TryLockError::WouldBlock)
        ));
        drop(first_guard);

        let second_guard = second_storage.mutation_guard().unwrap();
        drop(second_guard);
    }

    #[test]
    fn create_new_prevents_a_stage_file_from_being_replaced() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();

        assert_eq!(
            storage.install_validated_with_fault(
                validated_pack(),
                StorageFault::ManifestAlreadyExists,
            ),
            Err(PackStorageError::Write)
        );
        assert!(stage_directories(&managed).is_empty());
        assert!(!managed.join("keyforge-mechanical").exists());
    }

    #[test]
    fn late_failure_removes_only_the_current_stage() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let unrelated = managed.join("do-not-delete");
        fs::create_dir(&unrelated).unwrap();

        assert_eq!(
            storage.install_validated_with_fault(validated_pack(), StorageFault::BeforeVerify),
            Err(PackStorageError::InjectedForTest)
        );
        assert!(unrelated.is_dir());
        assert!(stage_directories(&managed).is_empty());
    }

    #[test]
    fn staged_files_are_redecoded_before_commit() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();

        assert_eq!(
            storage
                .install_validated_with_fault(validated_pack(), StorageFault::CorruptBeforeVerify),
            Err(PackStorageError::Verification)
        );
        assert!(stage_directories(&managed).is_empty());
        assert!(!managed.join("keyforge-mechanical").exists());
    }

    #[test]
    fn different_valid_staged_pcm_fails_post_reload_equivalence() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let unrelated = managed.join("do-not-delete");
        fs::create_dir(&unrelated).unwrap();
        fs::write(unrelated.join("sentinel"), b"keep").unwrap();

        assert_eq!(
            storage
                .install_validated_with_fault(validated_pack(), StorageFault::DifferentValidPcm,),
            Err(PackStorageError::Verification)
        );
        assert_eq!(fs::read(unrelated.join("sentinel")).unwrap(), b"keep");
        assert!(stage_directories(&managed).is_empty());
        assert!(!managed.join("keyforge-mechanical").exists());
    }

    #[test]
    fn different_valid_staged_metadata_fails_post_reload_equivalence() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let unrelated = managed.join("do-not-delete");
        fs::create_dir(&unrelated).unwrap();
        fs::write(unrelated.join("sentinel"), b"keep").unwrap();

        assert_eq!(
            storage.install_validated_with_fault(
                validated_pack(),
                StorageFault::DifferentValidMetadata,
            ),
            Err(PackStorageError::Verification)
        );
        assert_eq!(fs::read(unrelated.join("sentinel")).unwrap(), b"keep");
        assert!(stage_directories(&managed).is_empty());
        assert!(!managed.join("keyforge-mechanical").exists());
    }

    #[test]
    fn cleanup_requires_the_exact_owned_marker_and_ignores_unknown_siblings() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let missing = managed.join(format!("{STAGE_PREFIX}missing"));
        let mismatch = managed.join(format!("{STAGE_PREFIX}mismatch"));
        let unknown = managed.join("do-not-delete");
        fs::create_dir(&missing).unwrap();
        fs::create_dir(&mismatch).unwrap();
        fs::write(mismatch.join(STAGE_MARKER_NAME), b"not-keyforge").unwrap();
        fs::create_dir(&unknown).unwrap();

        assert_eq!(storage.cleanup_stale_stages(), Ok(0));
        assert!(missing.is_dir());
        assert!(mismatch.is_dir());
        assert!(unknown.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_rejects_a_stage_name_symlink_without_following_it() {
        use std::os::unix::fs::symlink;

        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let outside = root.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"keep").unwrap();
        symlink(&outside, managed.join(format!("{STAGE_PREFIX}link"))).unwrap();

        assert_eq!(
            storage.cleanup_stale_stages(),
            Err(PackStorageError::Cleanup)
        );
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep");
    }

    #[test]
    fn cleanup_removes_only_a_direct_owned_stage() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let stage = managed.join(format!("{STAGE_PREFIX}stale"));
        fs::create_dir(&stage).unwrap();
        fs::write(stage.join(STAGE_MARKER_NAME), STAGE_MARKER_BYTES).unwrap();
        fs::create_dir(stage.join("nested")).unwrap();
        fs::write(stage.join("nested/data"), b"discard").unwrap();
        let outside = root.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"keep").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, stage.join("nested/outside-link")).unwrap();

        assert_eq!(storage.cleanup_stale_stages(), Ok(1));
        assert!(!stage.exists());
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep");
    }

    #[test]
    fn discovery_is_sorted_and_ignores_unknown_siblings() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        storage
            .install_validated(validated_pack_with_id("zeta-pack"))
            .unwrap();
        storage
            .install_validated(validated_pack_with_id("alpha-pack"))
            .unwrap();
        fs::create_dir(managed.join("Not-A-Pack")).unwrap();
        fs::write(managed.join("notes.txt"), b"unknown").unwrap();

        let discovered = storage.discover().unwrap();
        assert_eq!(
            discovered
                .iter()
                .map(|metadata| metadata.id.as_str())
                .collect::<Vec<_>>(),
            ["alpha-pack", "zeta-pack"]
        );
    }

    #[test]
    fn discovery_fails_closed_for_a_malformed_valid_id_directory() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        fs::create_dir(managed.join("malformed-pack")).unwrap();

        assert_eq!(storage.discover(), Err(PackStorageError::InstalledPack));
    }

    #[test]
    fn installed_packs_are_revalidated_and_redecoded_on_load() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let installed = storage.install_validated(validated_pack()).unwrap();
        let first_load = storage.load_installed(&installed.id).unwrap();
        assert_eq!(first_load.metadata, installed);
        assert_eq!(first_load.sounds.total_len(), 6);

        fs::write(
            managed.join("keyforge-mechanical/sounds/modifier-01.wav"),
            b"not a WAV",
        )
        .unwrap();
        assert_eq!(
            storage.load_installed(&installed.id),
            Err(PackStorageError::InstalledPack)
        );
    }

    #[cfg(unix)]
    #[test]
    fn installed_loader_rejects_a_manifest_symlink_without_traversal() {
        use std::os::unix::fs::symlink;

        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let installed = storage.install_validated(validated_pack()).unwrap();
        let manifest = managed.join("keyforge-mechanical/manifest.json");
        let outside = root.path().join("outside-manifest.json");
        fs::rename(&manifest, &outside).unwrap();
        symlink(&outside, &manifest).unwrap();

        let error = storage.load_installed(&installed.id).unwrap_err();
        assert_sanitized_installed_error(error, root.path());
        assert!(outside.is_file());
    }

    #[cfg(unix)]
    #[test]
    fn installed_loader_rejects_a_sounds_directory_symlink_without_traversal() {
        use std::os::unix::fs::symlink;

        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let installed = storage.install_validated(validated_pack()).unwrap();
        let sounds = managed.join("keyforge-mechanical/sounds");
        let outside = root.path().join("outside-sounds");
        fs::rename(&sounds, &outside).unwrap();
        symlink(&outside, &sounds).unwrap();

        let error = storage.load_installed(&installed.id).unwrap_err();
        assert_sanitized_installed_error(error, root.path());
        assert!(outside.join("normal-01.wav").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn installed_loader_rejects_a_referenced_wav_symlink_without_traversal() {
        use std::os::unix::fs::symlink;

        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let installed = storage.install_validated(validated_pack()).unwrap();
        let wav = managed.join("keyforge-mechanical/sounds/normal-01.wav");
        let outside = root.path().join("outside-normal-01.wav");
        fs::rename(&wav, &outside).unwrap();
        symlink(&outside, &wav).unwrap();

        let error = storage.load_installed(&installed.id).unwrap_err();
        assert_sanitized_installed_error(error, root.path());
        assert!(outside.is_file());
    }

    #[test]
    fn installed_loader_rejects_a_noncanonical_manifest() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let installed = storage.install_validated(validated_pack()).unwrap();
        let directory = managed.join("keyforge-mechanical");
        fs::write(
            directory.join("manifest.json"),
            crate::pack::test_support::valid_manifest_json(),
        )
        .unwrap();
        assert_eq!(
            storage.load_installed(&installed.id),
            Err(PackStorageError::InstalledPack)
        );
    }

    #[test]
    fn installed_loader_rejects_an_extra_file() {
        let root = TestRoot::new();
        let managed = root.path().join("packs");
        let storage = PackStorage::open(managed.clone()).unwrap();
        let installed = storage.install_validated(validated_pack()).unwrap();
        let directory = managed.join("keyforge-mechanical");
        fs::write(directory.join("sounds/extra.wav"), b"extra").unwrap();
        assert_eq!(
            storage.load_installed(&installed.id),
            Err(PackStorageError::InstalledPack)
        );
    }

    #[test]
    fn storage_and_duplicate_install_errors_are_sanitized() {
        let root = TestRoot::new();
        let path_text = root.path().to_string_lossy();
        let storage = PackInstallError::from(PackStorageError::Write).to_string();
        let duplicate = PackInstallError::from(PackStorageError::DuplicateId).to_string();
        assert_eq!(storage, "sound-pack installation failed: storage");
        assert_eq!(duplicate, "sound-pack installation failed: duplicate id");
        assert!(!storage.contains(path_text.as_ref()));
        assert!(!duplicate.contains(path_text.as_ref()));
    }

    fn validated_pack() -> crate::pack::ValidatedPack {
        let bytes = valid_pack_zip();
        validate_archive(Cursor::new(bytes.clone()), bytes.len() as u64).unwrap()
    }

    fn validated_pack_with_metadata_chunk() -> crate::pack::ValidatedPack {
        let mut entries = valid_pack_entries();
        entries[1].1 = wav_with_junk_chunk();
        let bytes = zip_with_entries(CompressionMethod::Stored, entries);
        validate_archive(Cursor::new(bytes.clone()), bytes.len() as u64).unwrap()
    }

    fn validated_pack_with_id(id: &str) -> crate::pack::ValidatedPack {
        let mut entries = valid_pack_entries();
        entries[0].1 = String::from_utf8(entries[0].1.clone())
            .unwrap()
            .replace("keyforge-mechanical", id)
            .into_bytes();
        let bytes = zip_with_entries(CompressionMethod::Stored, entries);
        validate_archive(Cursor::new(bytes.clone()), bytes.len() as u64).unwrap()
    }

    fn directory_entries(path: &Path) -> Vec<String> {
        let mut entries = fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }

    fn stage_directories(root: &Path) -> Vec<PathBuf> {
        directory_entries(root)
            .into_iter()
            .filter(|name| name.starts_with(STAGE_PREFIX))
            .map(|name| root.join(name))
            .collect()
    }

    fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn visit(root: &Path, path: &Path, snapshot: &mut BTreeMap<PathBuf, Vec<u8>>) {
            let mut entries = fs::read_dir(path)
                .unwrap()
                .map(|entry| entry.unwrap())
                .collect::<Vec<_>>();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let path = entry.path();
                let relative = path.strip_prefix(root).unwrap().to_owned();
                let metadata = fs::symlink_metadata(&path).unwrap();
                if metadata.is_dir() {
                    snapshot.insert(relative, Vec::new());
                    visit(root, &path, snapshot);
                } else {
                    snapshot.insert(relative, fs::read(path).unwrap());
                }
            }
        }

        let mut snapshot = BTreeMap::new();
        visit(root, root, &mut snapshot);
        snapshot
    }

    fn assert_sanitized_installed_error(error: PackStorageError, root: &Path) {
        assert_eq!(error, PackStorageError::InstalledPack);
        let display = error.to_string();
        assert_eq!(display, "installed sound pack is invalid");
        assert!(!display.contains(root.to_string_lossy().as_ref()));
    }
}
