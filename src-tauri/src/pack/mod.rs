#[allow(dead_code)]
mod archive;
#[allow(dead_code)]
mod decoder;
mod manifest;

#[cfg(test)]
mod test_support;

#[allow(unused_imports)]
pub(crate) use archive::{
    inspect_archive, read_entry_bounded, ArchiveEntry, ArchiveInventory, PackArchiveError,
};
pub use manifest::{
    parse_manifest, CanonicalSoundPath, PackId, PackManifestError, PackVersion, SoundMap,
    ValidatedManifest,
};
