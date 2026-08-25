use std::io::Cursor;

use hound::{SampleFormat, WavSpec, WavWriter};

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
