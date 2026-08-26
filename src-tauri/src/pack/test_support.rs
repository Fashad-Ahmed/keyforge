use std::io::{Cursor, Write};

use hound::{SampleFormat, WavSpec, WavWriter};
use zip::{
    write::{FullFileOptions, SimpleFileOptions},
    CompressionMethod, ZipWriter,
};

pub(crate) fn valid_manifest_json() -> Vec<u8> {
    br#"{
      "schema_version": 1,
      "id": "keyforge-mechanical",
      "name": "KeyForge Mechanical",
      "pack_version": "1.0.0",
      "sounds": {
        "normal": ["sounds/normal-01.wav", "sounds/normal-02.wav"],
        "space": ["sounds/space-01.wav"],
        "enter": ["sounds/enter-01.wav"],
        "backspace": ["sounds/backspace-01.wav"],
        "modifier": ["sounds/modifier-01.wav"]
      }
    }"#
    .to_vec()
}

pub(crate) fn wav_bytes(channels: u16, sample_rate: u32, samples: &[i16]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::new(&mut cursor, spec).unwrap();
    for sample in samples {
        writer.write_sample(*sample).unwrap();
    }
    writer.finalize().unwrap();
    let mut bytes = cursor.into_inner();
    if channels > 2 {
        bytes.drain(36..60);
        overwrite_le_u32(&mut bytes, 16, 16);
        overwrite_le_u16(&mut bytes, 20, 1);
        let riff_size = (bytes.len() - 8) as u32;
        overwrite_le_u32(&mut bytes, 4, riff_size);
    }
    bytes
}

pub(crate) fn wav_with_junk_chunk() -> Vec<u8> {
    let mut bytes = wav_bytes(1, 48_000, &[0]);
    let junk = b"JUNK\x04\0\0\0junk";
    bytes.splice(36..36, junk.iter().copied());
    let riff_size = (bytes.len() - 8) as u32;
    overwrite_le_u32(&mut bytes, 4, riff_size);
    bytes
}

pub(crate) fn overwrite_le_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn overwrite_le_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn append_riff_chunk(bytes: &mut Vec<u8>, id: &[u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(id);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    if !payload.len().is_multiple_of(2) {
        bytes.push(0);
    }
    let riff_size = (bytes.len() - 8) as u32;
    overwrite_le_u32(bytes, 4, riff_size);
}

pub(crate) fn valid_pack_entries() -> Vec<(String, Vec<u8>)> {
    vec![
        ("manifest.json".to_owned(), valid_manifest_json()),
        (
            "sounds/normal-01.wav".to_owned(),
            wav_bytes(1, 48_000, &[1]),
        ),
        (
            "sounds/normal-02.wav".to_owned(),
            wav_bytes(1, 48_000, &[2]),
        ),
        ("sounds/space-01.wav".to_owned(), wav_bytes(1, 48_000, &[3])),
        ("sounds/enter-01.wav".to_owned(), wav_bytes(1, 48_000, &[4])),
        (
            "sounds/backspace-01.wav".to_owned(),
            wav_bytes(1, 48_000, &[5]),
        ),
        (
            "sounds/modifier-01.wav".to_owned(),
            wav_bytes(1, 48_000, &[6]),
        ),
    ]
}

pub(crate) fn zip_with_entries(
    compression: CompressionMethod,
    entries: Vec<(String, Vec<u8>)>,
) -> Vec<u8> {
    zip_with_entries_and_comments(compression, entries, false, None, None)
}

pub(crate) fn zip_with_entries_and_comments(
    compression: CompressionMethod,
    entries: Vec<(String, Vec<u8>)>,
    include_sounds_directory: bool,
    archive_comment: Option<&str>,
    entry_comment: Option<&str>,
) -> Vec<u8> {
    assert!(matches!(
        compression,
        CompressionMethod::Stored | CompressionMethod::Deflated
    ));
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    if let Some(comment) = archive_comment {
        writer.set_comment(comment).unwrap();
    }
    if include_sounds_directory {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o040755);
        writer.add_directory("sounds/", options).unwrap();
    }
    for (index, (name, bytes)) in entries.into_iter().enumerate() {
        let options = FullFileOptions::default()
            .compression_method(compression)
            .unix_permissions(0o100644);
        let options = if index == 0 {
            if let Some(comment) = entry_comment {
                options.with_file_comment(comment)
            } else {
                options
            }
        } else {
            options
        };
        writer.start_file(name, options).unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

pub(crate) fn zip_with_extra_name(name: &str) -> Vec<u8> {
    let mut entries = valid_pack_entries();
    entries.push((name.to_owned(), vec![1]));
    zip_with_entries(CompressionMethod::Stored, entries)
}

pub(crate) fn single_entry_zip(name: &str, bytes: Vec<u8>) -> Vec<u8> {
    zip_with_entries(CompressionMethod::Stored, vec![(name.to_owned(), bytes)])
}

pub(crate) fn mutate_entry_name(bytes: &mut [u8], entry_index: usize, replacement: &[u8]) {
    let central = central_offsets(bytes)[entry_index];
    let name_len = le_u16(bytes, central + 28) as usize;
    assert_eq!(name_len, replacement.len());
    bytes[central + 46..central + 46 + name_len].copy_from_slice(replacement);
    let local = le_u32(bytes, central + 42) as usize;
    assert_eq!(le_u16(bytes, local + 26) as usize, replacement.len());
    bytes[local + 30..local + 30 + replacement.len()].copy_from_slice(replacement);
}

pub(crate) fn mutate_entry_flags(bytes: &mut [u8], entry_index: usize, flags: u16) {
    let central = central_offsets(bytes)[entry_index];
    overwrite_le_u16(bytes, central + 8, flags);
    let local = le_u32(bytes, central + 42) as usize;
    overwrite_le_u16(bytes, local + 6, flags);
}

pub(crate) fn mutate_entry_compression(bytes: &mut [u8], entry_index: usize, method: u16) {
    let central = central_offsets(bytes)[entry_index];
    overwrite_le_u16(bytes, central + 10, method);
    let local = le_u32(bytes, central + 42) as usize;
    overwrite_le_u16(bytes, local + 8, method);
}

pub(crate) fn mutate_entry_unix_mode(bytes: &mut [u8], entry_index: usize, mode: u16) {
    let central = central_offsets(bytes)[entry_index];
    overwrite_le_u16(bytes, central + 40, mode);
    bytes[central + 5] = 3;
}

pub(crate) fn mutate_entry_sizes(
    bytes: &mut [u8],
    entry_index: usize,
    compressed: u32,
    expanded: u32,
) {
    let central = central_offsets(bytes)[entry_index];
    overwrite_le_u32(bytes, central + 20, compressed);
    overwrite_le_u32(bytes, central + 24, expanded);
    let local = le_u32(bytes, central + 42) as usize;
    overwrite_le_u32(bytes, local + 18, compressed);
    overwrite_le_u32(bytes, local + 22, expanded);
}

pub(crate) fn add_data_descriptor(mut bytes: Vec<u8>) -> Vec<u8> {
    let offsets = central_offsets(&bytes);
    assert_eq!(offsets.len(), 1);
    let central = offsets[0];
    let local = le_u32(&bytes, central + 42) as usize;
    let flags = le_u16(&bytes, central + 8) | 0x0008;
    overwrite_le_u16(&mut bytes, central + 8, flags);
    overwrite_le_u16(&mut bytes, local + 6, flags);
    overwrite_le_u32(&mut bytes, local + 14, 0);
    overwrite_le_u32(&mut bytes, local + 18, 0);
    overwrite_le_u32(&mut bytes, local + 22, 0);
    let crc = le_u32(&bytes, central + 16);
    let compressed = le_u32(&bytes, central + 20);
    let expanded = le_u32(&bytes, central + 24);
    let mut descriptor = Vec::with_capacity(16);
    descriptor.extend_from_slice(&0x0807_4b50_u32.to_le_bytes());
    descriptor.extend_from_slice(&crc.to_le_bytes());
    descriptor.extend_from_slice(&compressed.to_le_bytes());
    descriptor.extend_from_slice(&expanded.to_le_bytes());
    bytes.splice(central..central, descriptor);
    let eocd = find_eocd(&bytes);
    overwrite_le_u32(&mut bytes, eocd + 16, (central + 16) as u32);
    bytes
}

pub(crate) fn add_signatureless_data_descriptor_with_magic_crc(bytes: Vec<u8>) -> Vec<u8> {
    let mut bytes = add_data_descriptor(bytes);
    let central_before = central_offsets(&bytes)[0];
    let local = le_u32(&bytes, central_before + 42) as usize;
    let name_len = le_u16(&bytes, local + 26) as usize;
    let extra_len = le_u16(&bytes, local + 28) as usize;
    let descriptor =
        local + 30 + name_len + extra_len + le_u32(&bytes, central_before + 20) as usize;
    assert_eq!(le_u32(&bytes, descriptor), 0x0807_4b50);

    bytes.drain(descriptor..descriptor + 4);
    let central = central_before - 4;
    overwrite_le_u32(&mut bytes, descriptor, 0x0807_4b50);
    overwrite_le_u32(&mut bytes, central + 16, 0x0807_4b50);
    let eocd = find_eocd(&bytes);
    overwrite_le_u32(&mut bytes, eocd + 16, central as u32);
    bytes
}

pub(crate) fn corrupt_data_descriptor_expanded_size(bytes: &mut [u8], expanded: u32) {
    let central = central_offsets(bytes)[0];
    let local = le_u32(bytes, central + 42) as usize;
    let name_len = le_u16(bytes, local + 26) as usize;
    let extra_len = le_u16(bytes, local + 28) as usize;
    let descriptor = local + 30 + name_len + extra_len + le_u32(bytes, central + 20) as usize;
    assert_eq!(le_u32(bytes, descriptor), 0x0807_4b50);
    overwrite_le_u32(bytes, descriptor + 12, expanded);
}

pub(crate) fn mutate_eocd_disk_markers(bytes: &mut [u8], disk: u16, central_disk: u16) {
    let eocd = find_eocd(bytes);
    overwrite_le_u16(bytes, eocd + 4, disk);
    overwrite_le_u16(bytes, eocd + 6, central_disk);
}

pub(crate) fn corrupt_central_signature(bytes: &mut [u8]) {
    let central = central_offsets(bytes)[0];
    bytes[central] ^= 0xff;
}

pub(crate) fn physically_truncate_central_directory_record(mut bytes: Vec<u8>) -> Vec<u8> {
    let eocd = find_eocd(&bytes);
    let central_size = le_u32(&bytes, eocd + 12);
    assert!(central_size > 0);
    bytes.remove(eocd - 1);
    let eocd = find_eocd(&bytes);
    overwrite_le_u32(&mut bytes, eocd + 12, central_size - 1);
    bytes
}

pub(crate) fn central_offsets(bytes: &[u8]) -> Vec<usize> {
    let eocd = find_eocd(bytes);
    let count = le_u16(bytes, eocd + 10) as usize;
    let mut offset = le_u32(bytes, eocd + 16) as usize;
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        assert_eq!(le_u32(bytes, offset), 0x0201_4b50);
        result.push(offset);
        let name = le_u16(bytes, offset + 28) as usize;
        let extra = le_u16(bytes, offset + 30) as usize;
        let comment = le_u16(bytes, offset + 32) as usize;
        offset += 46 + name + extra + comment;
    }
    result
}

pub(crate) fn find_eocd(bytes: &[u8]) -> usize {
    bytes
        .windows(4)
        .rposition(|window| window == [0x50, 0x4b, 0x05, 0x06])
        .unwrap()
}

fn le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
