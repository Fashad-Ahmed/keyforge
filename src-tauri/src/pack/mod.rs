mod manifest;

#[cfg(test)]
mod test_support;

pub use manifest::{
    parse_manifest, CanonicalSoundPath, PackId, PackManifestError, PackVersion, SoundMap,
    ValidatedManifest,
};
