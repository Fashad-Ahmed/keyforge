use std::{collections::HashSet, fmt};

use serde::{Deserialize, Serialize};

pub const MANIFEST_LIMIT_BYTES: usize = 64 * 1024;
pub const MAX_GROUP_VARIANTS: usize = 16;
pub const MAX_PACK_SAMPLES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct PackId(String);

impl PackId {
    pub fn parse(value: &str) -> Result<Self, PackManifestError> {
        if !valid_slug(value, false) || reserved_windows_basename(value) {
            return Err(PackManifestError::Id);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PackVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl PackVersion {
    pub fn parse(value: &str) -> Result<Self, PackManifestError> {
        let mut components = value.split('.');
        let major = parse_version_component(components.next())?;
        let minor = parse_version_component(components.next())?;
        let patch = parse_version_component(components.next())?;
        if components.next().is_some() {
            return Err(PackManifestError::Version);
        }
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for PackVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct CanonicalSoundPath(String);

impl CanonicalSoundPath {
    pub fn parse(value: &str) -> Result<Self, PackManifestError> {
        let Some(stem) = value
            .strip_prefix("sounds/")
            .and_then(|path| path.strip_suffix(".wav"))
        else {
            return Err(PackManifestError::SoundPath);
        };
        if !valid_slug(stem, true) || reserved_windows_basename(stem) {
            return Err(PackManifestError::SoundPath);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SoundMap<T> {
    normal: Vec<T>,
    space: Vec<T>,
    enter: Vec<T>,
    backspace: Vec<T>,
    modifier: Vec<T>,
}

impl<T> SoundMap<T> {
    pub fn normal(&self) -> &[T] {
        &self.normal
    }

    pub fn space(&self) -> &[T] {
        &self.space
    }

    pub fn enter(&self) -> &[T] {
        &self.enter
    }

    pub fn backspace(&self) -> &[T] {
        &self.backspace
    }

    pub fn modifier(&self) -> &[T] {
        &self.modifier
    }

    pub fn total_len(&self) -> usize {
        self.normal.len()
            + self.space.len()
            + self.enter.len()
            + self.backspace.len()
            + self.modifier.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.normal
            .iter()
            .chain(&self.space)
            .chain(&self.enter)
            .chain(&self.backspace)
            .chain(&self.modifier)
    }

    pub fn try_map<U, E, F>(self, mut map: F) -> Result<SoundMap<U>, E>
    where
        F: FnMut(T) -> Result<U, E>,
    {
        Ok(SoundMap {
            normal: self
                .normal
                .into_iter()
                .map(&mut map)
                .collect::<Result<_, _>>()?,
            space: self
                .space
                .into_iter()
                .map(&mut map)
                .collect::<Result<_, _>>()?,
            enter: self
                .enter
                .into_iter()
                .map(&mut map)
                .collect::<Result<_, _>>()?,
            backspace: self
                .backspace
                .into_iter()
                .map(&mut map)
                .collect::<Result<_, _>>()?,
            modifier: self
                .modifier
                .into_iter()
                .map(map)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl<T> IntoIterator for SoundMap<T> {
    type Item = T;
    type IntoIter = std::iter::Chain<
        std::iter::Chain<
            std::iter::Chain<
                std::iter::Chain<std::vec::IntoIter<T>, std::vec::IntoIter<T>>,
                std::vec::IntoIter<T>,
            >,
            std::vec::IntoIter<T>,
        >,
        std::vec::IntoIter<T>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.normal
            .into_iter()
            .chain(self.space)
            .chain(self.enter)
            .chain(self.backspace)
            .chain(self.modifier)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedManifest {
    id: PackId,
    name: String,
    pack_version: PackVersion,
    sounds: SoundMap<CanonicalSoundPath>,
}

impl ValidatedManifest {
    pub fn id(&self) -> &PackId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn pack_version(&self) -> PackVersion {
        self.pack_version
    }

    pub fn sounds(&self) -> &SoundMap<CanonicalSoundPath> {
        &self.sounds
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackManifestError {
    TooLarge,
    Json,
    SchemaVersion,
    Id,
    Name,
    Version,
    EmptyGroup,
    TooManyVariants,
    TooManySamples,
    SoundPath,
    DuplicateReference,
}

impl fmt::Display for PackManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid sound-pack manifest: {self:?}")
    }
}

impl std::error::Error for PackManifestError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    schema_version: serde_json::Value,
    id: String,
    name: String,
    pack_version: String,
    sounds: RawSoundMap,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSoundMap {
    normal: Vec<String>,
    space: Vec<String>,
    enter: Vec<String>,
    backspace: Vec<String>,
    modifier: Vec<String>,
}

pub fn parse_manifest(bytes: &[u8]) -> Result<ValidatedManifest, PackManifestError> {
    if bytes.len() > MANIFEST_LIMIT_BYTES {
        return Err(PackManifestError::TooLarge);
    }
    let raw: RawManifest = serde_json::from_slice(bytes).map_err(|_| PackManifestError::Json)?;
    if raw.schema_version.as_i64() != Some(1) {
        return Err(PackManifestError::SchemaVersion);
    }

    let id = PackId::parse(&raw.id)?;
    validate_display_name(&raw.name)?;
    let pack_version = PackVersion::parse(&raw.pack_version)?;
    let sounds = validate_sound_map(raw.sounds)?;
    Ok(ValidatedManifest {
        id,
        name: raw.name.trim().to_owned(),
        pack_version,
        sounds,
    })
}

fn validate_sound_map(raw: RawSoundMap) -> Result<SoundMap<CanonicalSoundPath>, PackManifestError> {
    let mut total = 0_usize;
    let mut references = HashSet::new();
    let map_group =
        |group: Vec<String>, total: &mut usize, references: &mut HashSet<CanonicalSoundPath>| {
            if group.is_empty() {
                return Err(PackManifestError::EmptyGroup);
            }
            if group.len() > MAX_GROUP_VARIANTS {
                return Err(PackManifestError::TooManyVariants);
            }
            *total = total
                .checked_add(group.len())
                .ok_or(PackManifestError::TooManySamples)?;
            if *total > MAX_PACK_SAMPLES {
                return Err(PackManifestError::TooManySamples);
            }
            group
                .into_iter()
                .map(|path| {
                    let path = CanonicalSoundPath::parse(&path)?;
                    if !references.insert(path.clone()) {
                        return Err(PackManifestError::DuplicateReference);
                    }
                    Ok(path)
                })
                .collect()
        };
    Ok(SoundMap {
        normal: map_group(raw.normal, &mut total, &mut references)?,
        space: map_group(raw.space, &mut total, &mut references)?,
        enter: map_group(raw.enter, &mut total, &mut references)?,
        backspace: map_group(raw.backspace, &mut total, &mut references)?,
        modifier: map_group(raw.modifier, &mut total, &mut references)?,
    })
}

fn validate_display_name(value: &str) -> Result<(), PackManifestError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 80 {
        return Err(PackManifestError::Name);
    }
    if value.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '\u{2028}' | '\u{2029}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
            )
    }) {
        return Err(PackManifestError::Name);
    }
    Ok(())
}

fn valid_slug(value: &str, allow_underscore: bool) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    if !edge(bytes[0]) || !edge(bytes[bytes.len() - 1]) {
        return false;
    }
    bytes
        .iter()
        .all(|byte| edge(*byte) || *byte == b'-' || (allow_underscore && *byte == b'_'))
}

fn reserved_windows_basename(value: &str) -> bool {
    matches!(value, "con" | "prn" | "aux" | "nul")
        || value
            .strip_prefix("com")
            .or_else(|| value.strip_prefix("lpt"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}

fn parse_version_component(value: Option<&str>) -> Result<u32, PackManifestError> {
    let Some(value) = value else {
        return Err(PackManifestError::Version);
    };
    if value.is_empty() || (value != "0" && value.starts_with('0')) {
        return Err(PackManifestError::Version);
    }
    value.parse().map_err(|_| PackManifestError::Version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::test_support::valid_manifest_json;

    #[test]
    fn accepts_the_canonical_five_group_manifest() {
        let manifest = parse_manifest(&valid_manifest_json()).unwrap();
        assert_eq!(manifest.id().as_str(), "keyforge-mechanical");
        assert_eq!(manifest.name(), "KeyForge Mechanical");
        assert_eq!(manifest.pack_version().to_string(), "1.0.0");
        assert_eq!(manifest.sounds().normal().len(), 2);
        assert_eq!(manifest.sounds().total_len(), 6);
    }

    #[test]
    fn rejects_unknown_duplicate_and_missing_fields() {
        assert_eq!(
            parse_manifest(br#"{"schema_version":1,"schema_version":1}"#),
            Err(PackManifestError::Json)
        );
        let unknown = String::from_utf8(valid_manifest_json()).unwrap().replace(
            "\"schema_version\": 1,",
            "\"schema_version\": 1, \"script\": \"run.sh\",",
        );
        assert_eq!(
            parse_manifest(unknown.as_bytes()),
            Err(PackManifestError::Json)
        );
        assert_eq!(parse_manifest(br#"{}"#), Err(PackManifestError::Json));
    }

    #[test]
    fn rejects_invalid_identity_version_and_display_name() {
        for id in ["", "Upper", "-leading", "trailing-", "con", "com1"] {
            assert_eq!(PackId::parse(id), Err(PackManifestError::Id));
        }
        for version in ["1", "1.0", "01.0.0", "1.0.0-beta", "4294967296.0.0"] {
            assert_eq!(PackVersion::parse(version), Err(PackManifestError::Version));
        }
        for name in ["", "  ", "line\nbreak", "unsafe\u{202e}name"] {
            assert_eq!(validate_display_name(name), Err(PackManifestError::Name));
        }
    }

    #[test]
    fn rejects_noncanonical_paths_and_windows_device_names() {
        for path in [
            "../escape.wav",
            "/absolute.wav",
            "sounds\\key.wav",
            "C:relative.wav",
            "sounds/CON.wav",
            "sounds/con.wav",
            "sounds/key.WAV",
            "sounds/a/b.wav",
        ] {
            assert_eq!(
                CanonicalSoundPath::parse(path),
                Err(PackManifestError::SoundPath)
            );
        }
    }

    #[test]
    fn rejects_group_and_reference_limit_violations() {
        let duplicate = valid_manifest_json()
            .windows("sounds/space-01.wav".len())
            .filter(|window| *window == b"sounds/space-01.wav")
            .count();
        assert_eq!(duplicate, 1);
        let repeated = String::from_utf8(valid_manifest_json())
            .unwrap()
            .replace("sounds/enter-01.wav", "sounds/space-01.wav");
        assert_eq!(
            parse_manifest(repeated.as_bytes()),
            Err(PackManifestError::DuplicateReference)
        );
    }

    #[test]
    fn rejects_non_v1_schema_versions() {
        for version in ["0", "2", "-1", "1.0"] {
            let json = String::from_utf8(valid_manifest_json()).unwrap().replace(
                "\"schema_version\": 1",
                &format!("\"schema_version\": {version}"),
            );
            assert_eq!(
                parse_manifest(json.as_bytes()),
                Err(PackManifestError::SchemaVersion)
            );
        }
    }

    #[test]
    fn requires_every_nonempty_sound_group() {
        for (group, entry) in [
            (
                "normal",
                "\"normal\": [\"sounds/normal-01.wav\", \"sounds/normal-02.wav\"],",
            ),
            ("space", "\"space\": [\"sounds/space-01.wav\"],"),
            ("enter", "\"enter\": [\"sounds/enter-01.wav\"],"),
            ("backspace", "\"backspace\": [\"sounds/backspace-01.wav\"],"),
            ("modifier", "\"modifier\": [\"sounds/modifier-01.wav\"]"),
        ] {
            let absent = String::from_utf8(valid_manifest_json())
                .unwrap()
                .replace(entry, "");
            assert_eq!(
                parse_manifest(absent.as_bytes()),
                Err(PackManifestError::Json)
            );

            let empty = String::from_utf8(valid_manifest_json()).unwrap().replace(
                entry,
                &format!(
                    "\"{group}\": []{}",
                    if group == "modifier" { "" } else { "," }
                ),
            );
            assert_eq!(
                parse_manifest(empty.as_bytes()),
                Err(PackManifestError::EmptyGroup)
            );
        }
    }

    #[test]
    fn enforces_group_and_total_reference_limits() {
        let variants = (1..=17)
            .map(|index| format!("\"sounds/normal-{index}.wav\""))
            .collect::<Vec<_>>()
            .join(",");
        let too_many_group = format!(
            r#"{{"schema_version":1,"id":"pack","name":"Pack","pack_version":"1.0.0","sounds":{{"normal":[{variants}],"space":["sounds/space.wav"],"enter":["sounds/enter.wav"],"backspace":["sounds/backspace.wav"],"modifier":["sounds/modifier.wav"]}}}}"#
        );
        assert_eq!(
            parse_manifest(too_many_group.as_bytes()),
            Err(PackManifestError::TooManyVariants)
        );

        let group = |prefix: &str| {
            (1..=13)
                .map(|index| format!("\"sounds/{prefix}-{index}.wav\""))
                .collect::<Vec<_>>()
                .join(",")
        };
        let too_many_total = format!(
            r#"{{"schema_version":1,"id":"pack","name":"Pack","pack_version":"1.0.0","sounds":{{"normal":[{}],"space":[{}],"enter":[{}],"backspace":[{}],"modifier":[{}]}}}}"#,
            group("normal"),
            group("space"),
            group("enter"),
            group("backspace"),
            group("modifier")
        );
        assert_eq!(
            parse_manifest(too_many_total.as_bytes()),
            Err(PackManifestError::TooManySamples)
        );
    }

    #[test]
    fn enforces_id_and_display_name_boundaries() {
        for (id, expected) in [
            ("a".to_string(), Ok(())),
            ("a".repeat(64), Ok(())),
            ("a".repeat(65), Err(PackManifestError::Id)),
        ] {
            assert_eq!(PackId::parse(&id).map(|_| ()), expected);
        }
        for (name, expected) in [
            ("a".to_string(), Ok(())),
            ("a".repeat(80), Ok(())),
            ("a".repeat(81), Err(PackManifestError::Name)),
        ] {
            assert_eq!(validate_display_name(&name), expected);
        }
    }

    #[test]
    fn rejects_every_windows_reserved_basename() {
        for basename in [
            "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
            "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
        ] {
            assert_eq!(PackId::parse(basename), Err(PackManifestError::Id));
            assert_eq!(
                CanonicalSoundPath::parse(&format!("sounds/{basename}.wav")),
                Err(PackManifestError::SoundPath)
            );
        }
    }
}
