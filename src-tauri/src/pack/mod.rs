#[allow(dead_code)]
mod archive;
#[allow(dead_code)]
mod decoder;
mod manifest;
#[allow(dead_code)]
mod storage;

use std::fmt;

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
    Archive(PackArchiveError),
    Manifest(PackManifestError),
    Decode(PackDecodeError),
    Storage(PackStorageError),
    DuplicateId,
}

impl fmt::Display for PackInstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let category = match self {
            Self::Archive(_) => "archive",
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
            Self::Archive(error) => Some(error),
            Self::Manifest(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::DuplicateId => None,
        }
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
