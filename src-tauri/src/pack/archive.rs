use std::{
    collections::HashSet,
    fmt,
    io::{Cursor, Read, Seek},
    str,
};

use zip::{CompressionMethod, ZipArchive};

use super::{
    decoder::{decode_wav, DecodedAudio},
    manifest::MANIFEST_LIMIT_BYTES,
    parse_manifest, CanonicalSoundPath, PackDecodeError, PackInstallError, PackManifestError,
    ValidatedPack, MAX_DECODED_PACK_BYTES,
};

pub const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_ARCHIVE_ENTRIES: usize = 128;
pub const MAX_EXPANDED_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_WAV_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_COMPRESSION_RATIO: u64 = 100;

const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;
const DATA_DESCRIPTOR_SIGNATURE: u32 = 0x0807_4b50;
const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const LOCAL_HEADER_BYTES: usize = 30;
const CENTRAL_HEADER_BYTES: usize = 46;
const EOCD_BYTES: usize = 22;
const MAX_ZIP_COMMENT_BYTES: usize = u16::MAX as usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArchiveEntry {
    pub(crate) index: usize,
    name: String,
    pub(crate) expanded_size: u64,
    compressed_size: u64,
}

impl ArchiveEntry {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn expanded_size(&self) -> u64 {
        self.expanded_size
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArchiveInventory {
    manifest_index: usize,
    entries: Vec<ArchiveEntry>,
    wav_entries: Vec<ArchiveEntry>,
}

impl ArchiveInventory {
    pub(crate) fn manifest_index(&self) -> usize {
        self.manifest_index
    }

    pub(crate) fn wav_entries(&self) -> &[ArchiveEntry] {
        &self.wav_entries
    }

    pub(crate) fn entry(&self, name: &str) -> Option<&ArchiveEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackArchiveError {
    TooLarge,
    Invalid,
    Entries,
    Path,
    Duplicate,
    EntryType,
    Encrypted,
    Compression,
    EntryTooLarge,
    ExpandedTooLarge,
    CompressionRatio,
    MissingManifest,
    DuplicateManifest,
    Read,
    SizeMismatch,
}

impl fmt::Display for PackArchiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid sound-pack archive: {self:?}")
    }
}

impl std::error::Error for PackArchiveError {}

#[derive(Debug)]
struct RawEntry {
    name: Vec<u8>,
    flags: u16,
    compression: u16,
    crc32: u32,
    compressed_size: u64,
    expanded_size: u64,
    local_offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Manifest,
    SoundsDirectory,
    Wav,
}

pub(crate) fn inspect_archive<R: Read + Seek>(
    reader: R,
    source_size: u64,
) -> Result<ArchiveInventory, PackArchiveError> {
    let raw_bytes = snapshot_archive(reader, source_size)?;
    inspect_archive_snapshot(&raw_bytes)
}

fn snapshot_archive<R: Read>(reader: R, source_size: u64) -> Result<Vec<u8>, PackArchiveError> {
    if source_size > MAX_ARCHIVE_BYTES {
        return Err(PackArchiveError::TooLarge);
    }

    let capacity = usize::try_from(source_size).map_err(|_| PackArchiveError::TooLarge)?;
    let mut raw_bytes = Vec::with_capacity(capacity);
    reader
        .take(MAX_ARCHIVE_BYTES + 1)
        .read_to_end(&mut raw_bytes)
        .map_err(|_| PackArchiveError::Invalid)?;
    if raw_bytes.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err(PackArchiveError::TooLarge);
    }
    Ok(raw_bytes)
}

fn inspect_archive_snapshot(raw_bytes: &[u8]) -> Result<ArchiveInventory, PackArchiveError> {
    let raw_entries = preflight_zip_structure(raw_bytes)?;
    validate_raw_names(&raw_entries)?;

    let mut archive =
        ZipArchive::new(Cursor::new(raw_bytes)).map_err(|_| PackArchiveError::Invalid)?;
    if archive.len() != raw_entries.len() {
        return Err(PackArchiveError::Invalid);
    }

    let mut exact_names = HashSet::with_capacity(raw_entries.len());
    let mut folded_names = HashSet::with_capacity(raw_entries.len());
    let mut manifest_index = None;
    let mut entries = Vec::with_capacity(raw_entries.len());
    let mut wav_entries = Vec::new();
    let mut expanded_total = 0_u64;

    for (index, raw_entry) in raw_entries.iter().enumerate() {
        let file = archive
            .by_index_raw(index)
            .map_err(|_| PackArchiveError::Invalid)?;
        if file.name_raw() != raw_entry.name
            || file.compressed_size() != raw_entry.compressed_size
            || file.size() != raw_entry.expanded_size
        {
            return Err(PackArchiveError::Invalid);
        }
        let name = validated_ascii_name(file.name_raw())?;
        let kind = classify_name(name)?;

        if !exact_names.insert(name.to_owned()) || !folded_names.insert(name.to_ascii_lowercase()) {
            return if name == "manifest.json" {
                Err(PackArchiveError::DuplicateManifest)
            } else {
                Err(PackArchiveError::Duplicate)
            };
        }

        validate_entry_type(&file, kind)?;
        if file.encrypted() {
            return Err(PackArchiveError::Encrypted);
        }
        if !matches!(
            file.compression(),
            CompressionMethod::STORE | CompressionMethod::DEFLATE
        ) {
            return Err(PackArchiveError::Compression);
        }

        let expanded_size = file.size();
        let compressed_size = file.compressed_size();
        if kind != EntryKind::SoundsDirectory && expanded_size == 0 {
            return Err(PackArchiveError::Invalid);
        }
        if kind == EntryKind::Wav && expanded_size > MAX_WAV_ENTRY_BYTES {
            return Err(PackArchiveError::EntryTooLarge);
        }
        expanded_total = expanded_total
            .checked_add(expanded_size)
            .ok_or(PackArchiveError::ExpandedTooLarge)?;
        if expanded_total > MAX_EXPANDED_BYTES {
            return Err(PackArchiveError::ExpandedTooLarge);
        }
        validate_compression_ratio(expanded_size, compressed_size)?;

        if kind == EntryKind::SoundsDirectory {
            continue;
        }
        let entry = ArchiveEntry {
            index,
            name: name.to_owned(),
            expanded_size,
            compressed_size,
        };
        if kind == EntryKind::Manifest {
            if manifest_index.replace(index).is_some() {
                return Err(PackArchiveError::DuplicateManifest);
            }
        } else {
            wav_entries.push(entry.clone());
        }
        entries.push(entry);
    }

    let manifest_index = manifest_index.ok_or(PackArchiveError::MissingManifest)?;
    Ok(ArchiveInventory {
        manifest_index,
        entries,
        wav_entries,
    })
}

pub(crate) fn read_entry_bounded<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry: &ArchiveEntry,
    runtime_limit: u64,
) -> Result<Vec<u8>, PackArchiveError> {
    if entry.expanded_size > runtime_limit {
        return Err(PackArchiveError::SizeMismatch);
    }
    let mut file = archive
        .by_index(entry.index)
        .map_err(|_| PackArchiveError::Read)?;
    let capacity =
        usize::try_from(entry.expanded_size).map_err(|_| PackArchiveError::EntryTooLarge)?;
    let mut output = Vec::with_capacity(capacity);
    let mut buffer = [0_u8; 32 * 1024];
    let mut total = 0_u64;
    loop {
        let read = file.read(&mut buffer).map_err(|_| PackArchiveError::Read)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or(PackArchiveError::TooLarge)?;
        if total > runtime_limit || total > entry.expanded_size {
            return Err(PackArchiveError::SizeMismatch);
        }
        output.extend_from_slice(&buffer[..read]);
    }
    if total != entry.expanded_size {
        return Err(PackArchiveError::SizeMismatch);
    }
    Ok(output)
}

pub(crate) fn validate_archive<R: Read + Seek>(
    reader: R,
    source_size: u64,
) -> Result<ValidatedPack, PackInstallError> {
    let raw_bytes = snapshot_archive(reader, source_size)?;
    let inventory = inspect_archive_snapshot(&raw_bytes)?;
    let mut archive = ZipArchive::new(Cursor::new(raw_bytes.as_slice()))
        .map_err(|_| PackArchiveError::Invalid)?;

    let manifest_entry = inventory
        .entry("manifest.json")
        .ok_or(PackArchiveError::MissingManifest)?;
    let manifest_bytes =
        read_entry_bounded(&mut archive, manifest_entry, MANIFEST_LIMIT_BYTES as u64)?;
    let mut expanded_bytes = account_expanded_bytes(0, manifest_bytes.len() as u64)?;
    let manifest = parse_manifest(&manifest_bytes)?;

    let references = manifest
        .sounds()
        .iter()
        .map(CanonicalSoundPath::as_str)
        .collect::<HashSet<_>>();
    let wav_paths = inventory
        .wav_entries()
        .iter()
        .map(ArchiveEntry::name)
        .collect::<HashSet<_>>();
    if references.iter().any(|path| !wav_paths.contains(path)) {
        return Err(PackManifestError::MissingReference.into());
    }
    if wav_paths.iter().any(|path| !references.contains(path)) {
        return Err(PackManifestError::UnreferencedFile.into());
    }

    let mut decoded_bytes = 0_usize;
    let sounds = manifest.sounds().clone().try_map(|path| {
        let entry = inventory
            .entry(path.as_str())
            .ok_or(PackManifestError::MissingReference)?;
        let wav_bytes = read_entry_bounded(&mut archive, entry, MAX_WAV_ENTRY_BYTES)?;
        expanded_bytes = account_expanded_bytes(expanded_bytes, wav_bytes.len() as u64)?;
        let decoded = decode_wav(&wav_bytes)?;
        decoded_bytes = account_decoded_bytes(decoded_bytes, &decoded)?;
        Ok::<_, PackInstallError>(decoded)
    })?;

    Ok(ValidatedPack {
        manifest,
        sounds,
        expanded_bytes,
        decoded_bytes,
    })
}

fn account_expanded_bytes(total: u64, actual: u64) -> Result<u64, PackArchiveError> {
    let total = total
        .checked_add(actual)
        .ok_or(PackArchiveError::ExpandedTooLarge)?;
    if total > MAX_EXPANDED_BYTES {
        return Err(PackArchiveError::ExpandedTooLarge);
    }
    Ok(total)
}

fn account_decoded_bytes(total: usize, decoded: &DecodedAudio) -> Result<usize, PackDecodeError> {
    let total = total
        .checked_add(decoded.sample().byte_len())
        .ok_or(PackDecodeError::DecodedMemory)?;
    if total > MAX_DECODED_PACK_BYTES {
        return Err(PackDecodeError::DecodedMemory);
    }
    Ok(total)
}

fn validate_raw_names(entries: &[RawEntry]) -> Result<(), PackArchiveError> {
    let mut exact = HashSet::with_capacity(entries.len());
    let mut folded = HashSet::with_capacity(entries.len());
    for entry in entries {
        let name = validated_ascii_name(&entry.name)?;
        if name == "manifest.json" && exact.contains(name) {
            return Err(PackArchiveError::DuplicateManifest);
        }
        if !exact.insert(name.to_owned()) || !folded.insert(name.to_ascii_lowercase()) {
            return Err(PackArchiveError::Duplicate);
        }
    }
    Ok(())
}

fn validated_ascii_name(bytes: &[u8]) -> Result<&str, PackArchiveError> {
    if bytes.is_empty()
        || !bytes.is_ascii()
        || bytes
            .iter()
            .any(|byte| *byte == 0 || byte.is_ascii_control())
    {
        return Err(PackArchiveError::Path);
    }
    str::from_utf8(bytes).map_err(|_| PackArchiveError::Path)
}

fn classify_name(name: &str) -> Result<EntryKind, PackArchiveError> {
    if name == "manifest.json" {
        Ok(EntryKind::Manifest)
    } else if name == "sounds/" {
        Ok(EntryKind::SoundsDirectory)
    } else {
        CanonicalSoundPath::parse(name)
            .map(|_| EntryKind::Wav)
            .map_err(|_| PackArchiveError::Path)
    }
}

fn validate_entry_type<R: Read>(
    file: &zip::read::ZipFile<'_, R>,
    kind: EntryKind,
) -> Result<(), PackArchiveError> {
    let mode_type = file.unix_mode().map(|mode| mode & 0o170000).unwrap_or(0);
    match kind {
        EntryKind::SoundsDirectory => {
            if !file.is_dir()
                || !matches!(mode_type, 0 | 0o040000)
                || file.size() != 0
                || file.compressed_size() != 0
            {
                return Err(PackArchiveError::EntryType);
            }
        }
        EntryKind::Manifest | EntryKind::Wav => {
            if !file.is_file() || !matches!(mode_type, 0 | 0o100000) {
                return Err(PackArchiveError::EntryType);
            }
        }
    }
    Ok(())
}

fn validate_compression_ratio(expanded: u64, compressed: u64) -> Result<(), PackArchiveError> {
    if expanded == 0 {
        return Ok(());
    }
    if compressed == 0 {
        return Err(PackArchiveError::CompressionRatio);
    }
    let maximum = compressed
        .checked_mul(MAX_COMPRESSION_RATIO)
        .ok_or(PackArchiveError::CompressionRatio)?;
    if expanded > maximum {
        return Err(PackArchiveError::CompressionRatio);
    }
    Ok(())
}

fn preflight_zip_structure(bytes: &[u8]) -> Result<Vec<RawEntry>, PackArchiveError> {
    let eocd = find_eocd(bytes)?;
    let disk = read_u16(bytes, eocd + 4)?;
    let central_disk = read_u16(bytes, eocd + 6)?;
    let entries_on_disk = read_u16(bytes, eocd + 8)?;
    let entry_count = read_u16(bytes, eocd + 10)?;
    let central_size = read_u32(bytes, eocd + 12)?;
    let central_offset = read_u32(bytes, eocd + 16)?;
    if disk != 0
        || central_disk != 0
        || entries_on_disk != entry_count
        || entry_count == u16::MAX
        || central_size == u32::MAX
        || central_offset == u32::MAX
    {
        return Err(PackArchiveError::Invalid);
    }
    let entry_count = usize::from(entry_count);
    if entry_count > MAX_ARCHIVE_ENTRIES {
        return Err(PackArchiveError::Entries);
    }
    let central_start = usize::try_from(central_offset).map_err(|_| PackArchiveError::Invalid)?;
    let central_size = usize::try_from(central_size).map_err(|_| PackArchiveError::Invalid)?;
    let central_end = central_start
        .checked_add(central_size)
        .ok_or(PackArchiveError::Invalid)?;
    if central_end != eocd {
        return Err(PackArchiveError::Invalid);
    }

    let mut entries = Vec::with_capacity(entry_count);
    let mut offset = central_start;
    let mut ranges = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        if read_u32(bytes, offset)? != CENTRAL_HEADER_SIGNATURE {
            return Err(PackArchiveError::Invalid);
        }
        let flags = read_u16(bytes, offset + 8)?;
        let compression = read_u16(bytes, offset + 10)?;
        let crc32 = read_u32(bytes, offset + 16)?;
        let compressed_size = read_u32(bytes, offset + 20)?;
        let expanded_size = read_u32(bytes, offset + 24)?;
        let name_len = usize::from(read_u16(bytes, offset + 28)?);
        let extra_len = usize::from(read_u16(bytes, offset + 30)?);
        let comment_len = usize::from(read_u16(bytes, offset + 32)?);
        let starting_disk = read_u16(bytes, offset + 34)?;
        let local_offset = read_u32(bytes, offset + 42)?;
        if starting_disk != 0
            || compressed_size == u32::MAX
            || expanded_size == u32::MAX
            || local_offset == u32::MAX
        {
            return Err(PackArchiveError::Invalid);
        }
        let variable_len = name_len
            .checked_add(extra_len)
            .and_then(|value| value.checked_add(comment_len))
            .ok_or(PackArchiveError::Invalid)?;
        let next = offset
            .checked_add(CENTRAL_HEADER_BYTES)
            .and_then(|value| value.checked_add(variable_len))
            .ok_or(PackArchiveError::Invalid)?;
        if next > central_end {
            return Err(PackArchiveError::Invalid);
        }
        let name_start = offset + CENTRAL_HEADER_BYTES;
        let name_end = name_start + name_len;
        let name = bytes
            .get(name_start..name_end)
            .ok_or(PackArchiveError::Invalid)?
            .to_vec();
        let raw = RawEntry {
            name,
            flags,
            compression,
            crc32,
            compressed_size: u64::from(compressed_size),
            expanded_size: u64::from(expanded_size),
            local_offset: u64::from(local_offset),
        };
        ranges.push(validate_local_entry(bytes, &raw, central_start)?);
        entries.push(raw);
        offset = next;
    }
    if offset != central_end {
        return Err(PackArchiveError::Invalid);
    }
    ranges.sort_unstable_by_key(|range| range.0);
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(PackArchiveError::Invalid);
    }
    Ok(entries)
}

fn validate_local_entry(
    bytes: &[u8],
    entry: &RawEntry,
    central_start: usize,
) -> Result<(usize, usize), PackArchiveError> {
    let local = usize::try_from(entry.local_offset).map_err(|_| PackArchiveError::Invalid)?;
    if read_u32(bytes, local)? != LOCAL_HEADER_SIGNATURE {
        return Err(PackArchiveError::Invalid);
    }
    let local_flags = read_u16(bytes, local + 6)?;
    let local_compression = read_u16(bytes, local + 8)?;
    let local_crc = read_u32(bytes, local + 14)?;
    let local_compressed = read_u32(bytes, local + 18)?;
    let local_expanded = read_u32(bytes, local + 22)?;
    let name_len = usize::from(read_u16(bytes, local + 26)?);
    let extra_len = usize::from(read_u16(bytes, local + 28)?);
    if local_flags != entry.flags || local_compression != entry.compression {
        return Err(PackArchiveError::Invalid);
    }
    let name_start = local
        .checked_add(LOCAL_HEADER_BYTES)
        .ok_or(PackArchiveError::Invalid)?;
    let name_end = name_start
        .checked_add(name_len)
        .ok_or(PackArchiveError::Invalid)?;
    if bytes.get(name_start..name_end) != Some(entry.name.as_slice()) {
        return Err(PackArchiveError::Invalid);
    }
    let data_start = name_end
        .checked_add(extra_len)
        .ok_or(PackArchiveError::Invalid)?;
    let compressed =
        usize::try_from(entry.compressed_size).map_err(|_| PackArchiveError::Invalid)?;
    let data_end = data_start
        .checked_add(compressed)
        .ok_or(PackArchiveError::Invalid)?;
    if data_end > central_start {
        return Err(PackArchiveError::Invalid);
    }

    let descriptor = entry.flags & 0x0008 != 0;
    let end = if descriptor {
        if (local_crc != 0 && local_crc != entry.crc32)
            || (local_compressed != 0 && u64::from(local_compressed) != entry.compressed_size)
            || (local_expanded != 0 && u64::from(local_expanded) != entry.expanded_size)
        {
            return Err(PackArchiveError::SizeMismatch);
        }
        validate_data_descriptor(bytes, data_end, entry, central_start)?
    } else {
        if local_crc != entry.crc32
            || u64::from(local_compressed) != entry.compressed_size
            || u64::from(local_expanded) != entry.expanded_size
        {
            return Err(PackArchiveError::SizeMismatch);
        }
        data_end
    };
    Ok((local, end))
}

fn validate_data_descriptor(
    bytes: &[u8],
    data_end: usize,
    entry: &RawEntry,
    central_start: usize,
) -> Result<usize, PackArchiveError> {
    if read_u32(bytes, data_end)? == DATA_DESCRIPTOR_SIGNATURE {
        let values_start = data_end.checked_add(4).ok_or(PackArchiveError::Invalid)?;
        let signed_end = data_end.checked_add(16).ok_or(PackArchiveError::Invalid)?;
        if descriptor_matches(bytes, values_start, signed_end, entry, central_start)? {
            return Ok(signed_end);
        }
    }

    let unsigned_end = data_end.checked_add(12).ok_or(PackArchiveError::Invalid)?;
    if descriptor_matches(bytes, data_end, unsigned_end, entry, central_start)? {
        return Ok(unsigned_end);
    }
    Err(PackArchiveError::SizeMismatch)
}

fn descriptor_matches(
    bytes: &[u8],
    values_start: usize,
    descriptor_end: usize,
    entry: &RawEntry,
    central_start: usize,
) -> Result<bool, PackArchiveError> {
    if descriptor_end > central_start {
        return Ok(false);
    }
    let crc = read_u32(bytes, values_start)?;
    let compressed = read_u32(bytes, values_start + 4)?;
    let expanded = read_u32(bytes, values_start + 8)?;
    Ok(crc == entry.crc32
        && u64::from(compressed) == entry.compressed_size
        && u64::from(expanded) == entry.expanded_size)
}

fn find_eocd(bytes: &[u8]) -> Result<usize, PackArchiveError> {
    if bytes.len() < EOCD_BYTES {
        return Err(PackArchiveError::Invalid);
    }
    let search_start = bytes
        .len()
        .saturating_sub(EOCD_BYTES + MAX_ZIP_COMMENT_BYTES);
    for offset in (search_start..=bytes.len() - EOCD_BYTES).rev() {
        if read_u32(bytes, offset)? != EOCD_SIGNATURE {
            continue;
        }
        let comment_len = usize::from(read_u16(bytes, offset + 20)?);
        if offset
            .checked_add(EOCD_BYTES)
            .and_then(|value| value.checked_add(comment_len))
            == Some(bytes.len())
        {
            return Ok(offset);
        }
    }
    Err(PackArchiveError::Invalid)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, PackArchiveError> {
    let value = bytes
        .get(offset..offset.checked_add(2).ok_or(PackArchiveError::Invalid)?)
        .ok_or(PackArchiveError::Invalid)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, PackArchiveError> {
    let value = bytes
        .get(offset..offset.checked_add(4).ok_or(PackArchiveError::Invalid)?)
        .ok_or(PackArchiveError::Invalid)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        io::{self, Cursor, Read, Seek, SeekFrom},
        rc::Rc,
    };

    use zip::{CompressionMethod, ZipArchive};

    use super::*;
    use crate::pack::test_support::{
        add_data_descriptor, add_signatureless_data_descriptor_with_magic_crc, central_offsets,
        corrupt_central_signature, corrupt_data_descriptor_expanded_size, find_eocd,
        mutate_entry_compression, mutate_entry_flags, mutate_entry_name, mutate_entry_sizes,
        mutate_entry_unix_mode, mutate_eocd_disk_markers, overwrite_le_u32,
        pack_with_decoded_bytes, physically_truncate_central_directory_record, single_entry_zip,
        valid_manifest_json, valid_pack_entries, valid_pack_zip, wav_bytes,
        zip_missing_referenced_wav, zip_with_bad_late_wav, zip_with_duplicate_manifest_reference,
        zip_with_entries, zip_with_entries_and_comments, zip_with_extra_name,
        zip_with_unreferenced_wav,
    };
    use crate::pack::{
        decoder::{decode_wav, PackDecodeError},
        PackManifestError,
    };

    #[test]
    fn validates_every_reference_and_decodes_every_group() {
        let bytes = valid_pack_zip();
        let pack = validate_archive(Cursor::new(bytes.clone()), bytes.len() as u64).unwrap();
        assert_eq!(pack.manifest().id().as_str(), "keyforge-mechanical");
        assert_eq!(pack.sounds().normal().len(), 2);
        assert_eq!(pack.sounds().total_len(), 6);
        assert_eq!(pack.decoded_bytes(), 24);
        assert_eq!(
            pack.expanded_bytes(),
            valid_pack_entries()
                .iter()
                .map(|(_, contents)| contents.len() as u64)
                .sum::<u64>()
        );

        let first_values = pack
            .sounds()
            .iter()
            .map(|audio| audio.sample().samples()[0])
            .collect::<Vec<_>>();
        assert_eq!(
            first_values,
            [1.0, 2.0, 3.0, 4.0, 5.0, 6.0].map(|value| value / 32_768.0)
        );
    }

    #[test]
    fn validation_uses_one_immutable_archive_snapshot() {
        let first = valid_pack_zip();
        let mut second_entries = valid_pack_entries();
        second_entries[1].0 = "sounds\\normal-01.wav".to_owned();
        second_entries[1].1 = wav_bytes(1, 48_000, &[42]);
        let second = zip_with_entries(CompressionMethod::Stored, second_entries);
        assert_eq!(first.len(), second.len());
        assert_eq!(inspect(&second), Err(PackArchiveError::Path));

        let pack = validate_archive(
            SwappingReader::new(first.clone(), second),
            first.len() as u64,
        )
        .unwrap();

        assert_eq!(
            pack.sounds().normal()[0].sample().samples(),
            &[1.0 / 32_768.0]
        );
    }

    #[test]
    fn rejects_missing_extra_and_duplicate_references() {
        assert_eq!(
            validate_test_zip(zip_missing_referenced_wav()),
            Err(PackInstallError::Manifest(
                PackManifestError::MissingReference
            ))
        );
        assert_eq!(
            validate_test_zip(zip_with_unreferenced_wav()),
            Err(PackInstallError::Manifest(
                PackManifestError::UnreferencedFile
            ))
        );
        assert_eq!(
            validate_test_zip(zip_with_duplicate_manifest_reference()),
            Err(PackInstallError::Manifest(
                PackManifestError::DuplicateReference
            ))
        );
    }

    #[test]
    fn returns_no_decoded_pack_when_the_last_wav_fails() {
        assert_eq!(
            validate_test_zip(zip_with_bad_late_wav()),
            Err(PackInstallError::Decode(PackDecodeError::Container))
        );
    }

    #[test]
    fn decoded_memory_limit_uses_actual_f32_storage() {
        let bytes = pack_with_decoded_bytes(MAX_DECODED_PACK_BYTES + 4);
        assert!(bytes.len() as u64 <= MAX_ARCHIVE_BYTES);
        assert_eq!(
            validate_test_zip(bytes),
            Err(PackInstallError::Decode(PackDecodeError::DecodedMemory))
        );
    }

    #[test]
    fn aggregate_accounting_rejects_integer_overflow() {
        let decoded = decode_wav(&wav_bytes(1, 48_000, &[1])).unwrap();
        assert_eq!(account_decoded_bytes(0, &decoded), Ok(4));
        assert_eq!(
            account_decoded_bytes(usize::MAX - 3, &decoded),
            Err(PackDecodeError::DecodedMemory)
        );
        assert_eq!(
            account_expanded_bytes(u64::MAX, 1),
            Err(PackArchiveError::ExpandedTooLarge)
        );
    }

    #[test]
    fn top_level_error_text_is_sanitized() {
        assert_eq!(
            PackInstallError::Archive(PackArchiveError::Path).to_string(),
            "sound-pack installation failed: archive"
        );
        assert_eq!(
            PackInstallError::Manifest(PackManifestError::Json).to_string(),
            "sound-pack installation failed: manifest"
        );
        assert_eq!(
            PackInstallError::Decode(PackDecodeError::Container).to_string(),
            "sound-pack installation failed: decode"
        );
    }

    #[test]
    fn inventories_stored_and_deflated_canonical_layouts() {
        for compression in [CompressionMethod::Stored, CompressionMethod::Deflated] {
            let bytes = zip_with_entries(compression, valid_pack_entries());
            let inventory =
                inspect_archive(Cursor::new(bytes.clone()), bytes.len() as u64).unwrap();
            assert_eq!(inventory.manifest_index(), 0);
            assert_eq!(inventory.wav_entries().len(), 6);
            let normal = inventory.entry("sounds/normal-01.wav").unwrap();
            assert_eq!(normal.name(), "sounds/normal-01.wav");
            assert!(normal.expanded_size() > 0);
        }
    }

    #[test]
    fn rejects_cross_platform_path_attacks() {
        for name in [
            "../escape.wav",
            "/absolute.wav",
            "sounds\\escape.wav",
            "C:relative.wav",
            "C:/absolute.wav",
            "//server/share.wav",
            "sounds//key.wav",
            "sounds/./key.wav",
            "sounds/key.WAV",
            "sounds/con.wav",
            "__MACOSX/junk",
            "other/",
        ] {
            let bytes = zip_with_extra_name(name);
            assert_eq!(
                inspect_archive(Cursor::new(bytes.clone()), bytes.len() as u64),
                Err(PackArchiveError::Path),
                "accepted unsafe name {name:?}"
            );
        }
    }

    #[test]
    fn rejects_non_ascii_raw_names_before_decoding() {
        let mut bytes = single_entry_zip("sounds/key.wav", vec![1]);
        let mut name = b"sounds/key.wav".to_vec();
        name[7] = 0xff;
        mutate_entry_name(&mut bytes, 0, &name);
        assert_eq!(inspect(&bytes), Err(PackArchiveError::Path));
    }

    #[test]
    fn rejects_links_special_types_encryption_and_unsupported_compression() {
        for mode in [0o120777, 0o060644, 0o020644, 0o010644, 0o140644] {
            let mut bytes = single_entry_zip("manifest.json", valid_manifest_json());
            mutate_entry_unix_mode(&mut bytes, 0, mode);
            assert_eq!(inspect(&bytes), Err(PackArchiveError::EntryType));
        }

        let mut encrypted = single_entry_zip("manifest.json", valid_manifest_json());
        mutate_entry_flags(&mut encrypted, 0, 0x0001);
        assert_eq!(inspect(&encrypted), Err(PackArchiveError::Encrypted));

        let mut bzip2 = single_entry_zip("manifest.json", valid_manifest_json());
        mutate_entry_compression(&mut bzip2, 0, 12);
        assert_eq!(inspect(&bzip2), Err(PackArchiveError::Compression));
    }

    #[test]
    fn enforces_source_size_at_the_exact_boundary() {
        let entries = vec![
            ("manifest.json".to_owned(), vec![1]),
            (
                "sounds/one.wav".to_owned(),
                vec![1; MAX_WAV_ENTRY_BYTES as usize],
            ),
            (
                "sounds/two.wav".to_owned(),
                vec![2; MAX_WAV_ENTRY_BYTES as usize - 1024],
            ),
        ];
        let unpadded = zip_with_entries(CompressionMethod::Stored, entries.clone());
        let padding = MAX_ARCHIVE_BYTES as usize - unpadded.len();
        assert!(padding <= usize::from(u16::MAX));
        let comment = "x".repeat(padding);
        let bytes = zip_with_entries_and_comments(
            CompressionMethod::Stored,
            entries,
            false,
            Some(&comment),
            None,
        );
        assert_eq!(bytes.len() as u64, MAX_ARCHIVE_BYTES);
        assert!(inspect_archive(Cursor::new(bytes.clone()), bytes.len() as u64).is_ok());

        let mut over_limit = bytes;
        over_limit.push(0);
        assert_eq!(
            inspect_archive(Cursor::new(over_limit), MAX_ARCHIVE_BYTES),
            Err(PackArchiveError::TooLarge)
        );

        let small = zip_with_entries(CompressionMethod::Stored, valid_pack_entries());
        assert_eq!(
            inspect_archive(Cursor::new(small), MAX_ARCHIVE_BYTES + 1),
            Err(PackArchiveError::TooLarge)
        );
    }

    #[test]
    fn bounds_snapshot_reads_when_seek_metadata_lies() {
        let bytes_read = Rc::new(Cell::new(0));
        let reader = LyingLengthReader {
            remaining: MAX_ARCHIVE_BYTES as usize + 4096,
            reported_end: 1,
            bytes_read: Rc::clone(&bytes_read),
        };

        assert_eq!(inspect_archive(reader, 1), Err(PackArchiveError::TooLarge));
        assert_eq!(bytes_read.get() as u64, MAX_ARCHIVE_BYTES + 1);
    }

    #[test]
    fn enforces_entry_count_at_128() {
        let archive = |count: usize| {
            let mut entries = vec![("manifest.json".to_owned(), valid_manifest_json())];
            entries.extend(
                (0..count - 1).map(|index| (format!("sounds/key-{index:03}.wav"), vec![1])),
            );
            zip_with_entries(CompressionMethod::Stored, entries)
        };
        let at_limit = archive(MAX_ARCHIVE_ENTRIES);
        assert!(inspect(&at_limit).is_ok());
        let over_limit = archive(MAX_ARCHIVE_ENTRIES + 1);
        assert_eq!(inspect(&over_limit), Err(PackArchiveError::Entries));
    }

    #[test]
    fn enforces_wav_entry_size_at_eight_mebibytes() {
        let at_limit = zip_with_entries(
            CompressionMethod::Stored,
            vec![
                ("manifest.json".to_owned(), valid_manifest_json()),
                (
                    "sounds/key.wav".to_owned(),
                    vec![0x5a; MAX_WAV_ENTRY_BYTES as usize],
                ),
            ],
        );
        assert!(inspect(&at_limit).is_ok());

        let over_limit = zip_with_entries(
            CompressionMethod::Stored,
            vec![
                ("manifest.json".to_owned(), valid_manifest_json()),
                (
                    "sounds/key.wav".to_owned(),
                    vec![0x5a; MAX_WAV_ENTRY_BYTES as usize + 1],
                ),
            ],
        );
        assert_eq!(inspect(&over_limit), Err(PackArchiveError::EntryTooLarge));
    }

    #[test]
    fn enforces_expanded_aggregate_at_64_mebibytes() {
        let archive = |extra: usize| {
            let manifest = valid_manifest_json();
            let mut entries = vec![("manifest.json".to_owned(), manifest.clone())];
            let expanded = MAX_EXPANDED_BYTES as usize + extra - manifest.len();
            let mut remaining = expanded;
            for index in 0..8 {
                let len = remaining.min(MAX_WAV_ENTRY_BYTES as usize);
                let mut state = 0x9e37_79b9_u32 ^ index as u32;
                let seed: Vec<u8> = (0..1024 * 1024)
                    .map(|_| {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        (state & 1) as u8
                    })
                    .collect();
                let mut payload = Vec::with_capacity(len);
                while payload.len() < len {
                    let take = (len - payload.len()).min(seed.len());
                    payload.extend_from_slice(&seed[..take]);
                }
                entries.push((format!("sounds/key-{index}.wav"), payload));
                remaining -= len;
            }
            assert_eq!(remaining, 0);
            zip_with_entries(CompressionMethod::Deflated, entries)
        };
        let at_limit = archive(0);
        assert!(inspect(&at_limit).is_ok());
        let over_limit = archive(1);
        assert_eq!(
            inspect(&over_limit),
            Err(PackArchiveError::ExpandedTooLarge)
        );
    }

    #[test]
    fn enforces_compression_ratio_at_100_to_1() {
        let mut at_limit = single_entry_zip("manifest.json", vec![0; 200]);
        let central = central_offsets(&at_limit)[0];
        let compressed =
            u32::from_le_bytes(at_limit[central + 20..central + 24].try_into().unwrap());
        assert!(compressed > 0);
        mutate_entry_sizes(&mut at_limit, 0, compressed, compressed * 100);
        assert!(inspect(&at_limit).is_ok());

        let mut over_limit = single_entry_zip("manifest.json", vec![0; 200]);
        let central = central_offsets(&over_limit)[0];
        let compressed =
            u32::from_le_bytes(over_limit[central + 20..central + 24].try_into().unwrap());
        mutate_entry_sizes(&mut over_limit, 0, compressed, compressed * 100 + 1);
        assert_eq!(
            inspect(&over_limit),
            Err(PackArchiveError::CompressionRatio)
        );

        let mut zero_compressed = single_entry_zip("manifest.json", vec![1]);
        mutate_entry_sizes(&mut zero_compressed, 0, 0, 1);
        assert_eq!(
            inspect(&zero_compressed),
            Err(PackArchiveError::CompressionRatio)
        );
    }

    #[test]
    fn rejects_exact_duplicate_case_collision_and_duplicate_manifest() {
        let mut duplicate = zip_with_entries(
            CompressionMethod::Stored,
            vec![
                ("manifest.json".to_owned(), valid_manifest_json()),
                ("sounds/one.wav".to_owned(), vec![1]),
                ("sounds/two.wav".to_owned(), vec![2]),
            ],
        );
        mutate_entry_name(&mut duplicate, 2, b"sounds/one.wav");
        assert_eq!(inspect(&duplicate), Err(PackArchiveError::Duplicate));

        let mut collision = zip_with_entries(
            CompressionMethod::Stored,
            vec![
                ("manifest.json".to_owned(), valid_manifest_json()),
                ("sounds/key.wav".to_owned(), vec![1]),
                ("sounds/KEY.wav".to_owned(), vec![2]),
            ],
        );
        mutate_entry_name(&mut collision, 2, b"sounds/KEY.wav");
        assert_eq!(inspect(&collision), Err(PackArchiveError::Duplicate));

        let mut duplicate_manifest = zip_with_entries(
            CompressionMethod::Stored,
            vec![
                ("manifest.json".to_owned(), valid_manifest_json()),
                ("manifest.jsox".to_owned(), valid_manifest_json()),
            ],
        );
        mutate_entry_name(&mut duplicate_manifest, 1, b"manifest.json");
        assert_eq!(
            inspect(&duplicate_manifest),
            Err(PackArchiveError::DuplicateManifest)
        );
    }

    #[test]
    fn requires_exactly_one_manifest_and_accepts_optional_sounds_directory() {
        let missing = single_entry_zip("sounds/key.wav", vec![1]);
        assert_eq!(inspect(&missing), Err(PackArchiveError::MissingManifest));

        let bytes = zip_with_entries_and_comments(
            CompressionMethod::Stored,
            valid_pack_entries(),
            true,
            None,
            None,
        );
        let inventory = inspect(&bytes).unwrap();
        assert_eq!(inventory.wav_entries().len(), 6);
        assert!(inventory.entry("sounds/").is_none());
    }

    #[test]
    fn rejects_directory_entries_that_contain_data() {
        for declared_expanded_size in [1, 0] {
            let mut entries = valid_pack_entries();
            let directory_index = entries.len();
            entries.push(("sounds/".to_owned(), vec![1]));
            let mut bytes = zip_with_entries(CompressionMethod::Stored, entries);
            mutate_entry_unix_mode(&mut bytes, directory_index, 0o040755);
            mutate_entry_sizes(&mut bytes, directory_index, 1, declared_expanded_size);

            assert_eq!(inspect(&bytes), Err(PackArchiveError::EntryType));
        }
    }

    #[test]
    fn archive_and_entry_comments_are_inert() {
        let bytes = zip_with_entries_and_comments(
            CompressionMethod::Stored,
            valid_pack_entries(),
            false,
            Some("do not execute: private/archive/path"),
            Some("ignored entry metadata"),
        );
        let inventory = inspect(&bytes).unwrap();
        assert_eq!(inventory.wav_entries().len(), 6);
        assert!(inventory
            .wav_entries()
            .iter()
            .all(|entry| !entry.name().contains("ignored")));
    }

    #[test]
    fn accepts_consistent_data_descriptor_and_bounded_read() {
        let bytes = add_data_descriptor(single_entry_zip("manifest.json", valid_manifest_json()));
        let inventory = inspect(&bytes).unwrap();
        let entry = inventory.entry("manifest.json").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(
            read_entry_bounded(&mut archive, entry, 64 * 1024).unwrap(),
            valid_manifest_json()
        );
    }

    #[test]
    fn accepts_signatureless_descriptor_when_crc_equals_signature_magic() {
        let bytes = add_signatureless_data_descriptor_with_magic_crc(single_entry_zip(
            "manifest.json",
            valid_manifest_json(),
        ));

        assert!(inspect(&bytes).is_ok());
    }

    #[test]
    fn rejects_inconsistent_descriptor_and_central_sizes() {
        let mut bytes =
            add_data_descriptor(single_entry_zip("manifest.json", valid_manifest_json()));
        corrupt_data_descriptor_expanded_size(&mut bytes, 1);
        assert_eq!(inspect(&bytes), Err(PackArchiveError::SizeMismatch));
    }

    #[test]
    fn rejects_multi_disk_markers_and_truncated_central_directory() {
        let mut multi_disk = single_entry_zip("manifest.json", valid_manifest_json());
        mutate_eocd_disk_markers(&mut multi_disk, 1, 1);
        assert_eq!(inspect(&multi_disk), Err(PackArchiveError::Invalid));

        let mut truncated = single_entry_zip("manifest.json", valid_manifest_json());
        corrupt_central_signature(&mut truncated);
        assert_eq!(inspect(&truncated), Err(PackArchiveError::Invalid));

        let mut bad_central_size = single_entry_zip("manifest.json", valid_manifest_json());
        let eocd = find_eocd(&bad_central_size);
        let declared =
            u32::from_le_bytes(bad_central_size[eocd + 12..eocd + 16].try_into().unwrap());
        overwrite_le_u32(&mut bad_central_size, eocd + 12, declared - 1);
        assert_eq!(inspect(&bad_central_size), Err(PackArchiveError::Invalid));
    }

    #[test]
    fn rejects_a_physically_truncated_central_directory_record() {
        let bytes = physically_truncate_central_directory_record(single_entry_zip(
            "manifest.json",
            valid_manifest_json(),
        ));

        assert_eq!(inspect(&bytes), Err(PackArchiveError::Invalid));
    }

    #[test]
    fn bounded_reads_enforce_runtime_declared_size_and_crc() {
        let bytes = single_entry_zip("manifest.json", valid_manifest_json());
        let inventory = inspect(&bytes).unwrap();
        let entry = inventory.entry("manifest.json").unwrap();

        let mut archive = ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
        assert_eq!(
            read_entry_bounded(&mut archive, entry, entry.expanded_size() - 1),
            Err(PackArchiveError::SizeMismatch)
        );

        let mut too_small = entry.clone();
        too_small.expanded_size -= 1;
        let mut archive = ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
        assert_eq!(
            read_entry_bounded(&mut archive, &too_small, MAX_WAV_ENTRY_BYTES),
            Err(PackArchiveError::SizeMismatch)
        );

        let mut too_large = entry.clone();
        too_large.expanded_size += 1;
        let mut archive = ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
        assert_eq!(
            read_entry_bounded(&mut archive, &too_large, MAX_WAV_ENTRY_BYTES),
            Err(PackArchiveError::SizeMismatch)
        );

        let mut corrupt = bytes;
        let central = central_offsets(&corrupt)[0];
        let local =
            u32::from_le_bytes(corrupt[central + 42..central + 46].try_into().unwrap()) as usize;
        let data = local
            + 30
            + u16::from_le_bytes(corrupt[local + 26..local + 28].try_into().unwrap()) as usize
            + u16::from_le_bytes(corrupt[local + 28..local + 30].try_into().unwrap()) as usize;
        corrupt[data] ^= 0xff;
        let inventory = inspect(&corrupt).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(corrupt)).unwrap();
        assert_eq!(
            read_entry_bounded(
                &mut archive,
                inventory.entry("manifest.json").unwrap(),
                64 * 1024
            ),
            Err(PackArchiveError::Read)
        );
    }

    #[test]
    fn bounded_read_rejects_declared_size_above_runtime_limit_before_io() {
        let bytes = single_entry_zip("manifest.json", valid_manifest_json());
        let inventory = inspect(&bytes).unwrap();
        let entry = inventory.entry("manifest.json").unwrap();
        let bytes_read = Rc::new(Cell::new(0));
        let reader = CountingReader {
            inner: Cursor::new(bytes),
            bytes_read: Rc::clone(&bytes_read),
        };
        let mut archive = ZipArchive::new(reader).unwrap();
        bytes_read.set(0);

        assert_eq!(
            read_entry_bounded(&mut archive, entry, entry.expanded_size() - 1),
            Err(PackArchiveError::SizeMismatch)
        );
        assert_eq!(bytes_read.get(), 0);
    }

    #[test]
    fn public_error_text_is_sanitized() {
        assert_eq!(
            PackArchiveError::Path.to_string(),
            "invalid sound-pack archive: Path"
        );
        assert!(!PackArchiveError::Read.to_string().contains('/'));
    }

    fn inspect(bytes: &[u8]) -> Result<ArchiveInventory, PackArchiveError> {
        inspect_archive(Cursor::new(bytes), bytes.len() as u64)
    }

    fn validate_test_zip(bytes: Vec<u8>) -> Result<ValidatedPack, PackInstallError> {
        validate_archive(Cursor::new(bytes.clone()), bytes.len() as u64)
    }

    struct LyingLengthReader {
        remaining: usize,
        reported_end: u64,
        bytes_read: Rc<Cell<usize>>,
    }

    struct CountingReader {
        inner: Cursor<Vec<u8>>,
        bytes_read: Rc<Cell<usize>>,
    }

    struct SwappingReader {
        first: Cursor<Vec<u8>>,
        second: Cursor<Vec<u8>>,
        swapped: bool,
    }

    impl SwappingReader {
        fn new(first: Vec<u8>, second: Vec<u8>) -> Self {
            Self {
                first: Cursor::new(first),
                second: Cursor::new(second),
                swapped: false,
            }
        }

        fn active(&mut self) -> &mut Cursor<Vec<u8>> {
            if self.swapped {
                &mut self.second
            } else {
                &mut self.first
            }
        }
    }

    impl Read for SwappingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.active().read(buffer)
        }
    }

    impl Seek for SwappingReader {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            if !self.swapped && position == SeekFrom::Start(0) {
                self.swapped = true;
            }
            self.active().seek(position)
        }
    }

    impl Read for CountingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let read = self.inner.read(buffer)?;
            self.bytes_read.set(self.bytes_read.get() + read);
            Ok(read)
        }
    }

    impl Seek for CountingReader {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            self.inner.seek(position)
        }
    }

    impl Read for LyingLengthReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let read = self.remaining.min(buffer.len());
            buffer[..read].fill(0);
            self.remaining -= read;
            self.bytes_read.set(self.bytes_read.get() + read);
            Ok(read)
        }
    }

    impl Seek for LyingLengthReader {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            match position {
                SeekFrom::End(0) => Ok(self.reported_end),
                SeekFrom::Start(0) => Ok(0),
                _ => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unsupported test seek",
                )),
            }
        }
    }
}
