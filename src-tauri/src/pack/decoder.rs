#[cfg(test)]
mod tests {
    use super::{decode_wav, PackDecodeError};
    use crate::{
        audio::ChannelCount,
        pack::test_support::{
            append_riff_chunk, overwrite_le_u16, overwrite_le_u32, wav_bytes, wav_with_junk_chunk,
        },
    };

    #[test]
    fn decodes_mono_and_stereo_signed_16_pcm() {
        let mono = decode_wav(&wav_bytes(1, 48_000, &[-32_768, 0, 32_767])).unwrap();
        assert_eq!(mono.sample().sample_rate(), 48_000);
        assert_eq!(mono.sample().channels(), ChannelCount::Mono);
        assert_eq!(mono.sample().frame_count(), 3);
        assert_eq!(mono.sample().samples(), &[-1.0, 0.0, 32_767.0 / 32_768.0]);

        let stereo = decode_wav(&wav_bytes(2, 8_000, &[-1, 1, -2, 2])).unwrap();
        assert_eq!(stereo.sample().channels(), ChannelCount::Stereo);
        assert_eq!(stereo.sample().frame_count(), 2);
    }

    #[test]
    fn canonical_output_round_trips_without_metadata() {
        let decoded = decode_wav(&wav_with_junk_chunk()).unwrap();
        assert!(!decoded
            .canonical_wav()
            .windows(4)
            .any(|bytes| bytes == b"JUNK"));
        let second = decode_wav(decoded.canonical_wav()).unwrap();
        assert_eq!(decoded.sample(), second.sample());
    }

    #[test]
    fn rejects_non_pcm_and_non_16_bit_formats() {
        let mut float = wav_bytes(1, 48_000, &[0]);
        overwrite_le_u16(&mut float, 20, 3);
        let mut pcm_8 = wav_bytes(1, 48_000, &[0]);
        overwrite_le_u16(&mut pcm_8, 34, 8);
        let mut pcm_24 = wav_bytes(1, 48_000, &[0]);
        overwrite_le_u16(&mut pcm_24, 34, 24);
        let mut extensible = wav_bytes(1, 48_000, &[0]);
        overwrite_le_u16(&mut extensible, 20, 0xfffe);

        for bytes in [float, pcm_8, pcm_24, extensible] {
            assert_eq!(decode_wav(&bytes), Err(PackDecodeError::Format));
        }
    }

    #[test]
    fn rejects_bad_container_alignment_rate_and_duration() {
        assert_eq!(decode_wav(b"not-wave"), Err(PackDecodeError::Container));
        assert_eq!(
            decode_wav(&wav_bytes(3, 48_000, &[0, 0, 0])),
            Err(PackDecodeError::Channels)
        );
        assert_eq!(
            decode_wav(&wav_bytes(1, 7_999, &[0])),
            Err(PackDecodeError::SampleRate)
        );
        assert_eq!(
            decode_wav(&wav_bytes(1, 96_001, &[0])),
            Err(PackDecodeError::SampleRate)
        );
        assert_eq!(
            decode_wav(&wav_bytes(1, 48_000, &[])),
            Err(PackDecodeError::Empty)
        );
        assert_eq!(
            decode_wav(&wav_bytes(1, 48_000, &vec![0; 96_001])),
            Err(PackDecodeError::Duration)
        );
    }

    #[test]
    fn rejects_malformed_riff_chunk_structure() {
        let valid = wav_bytes(1, 48_000, &[0]);

        let mut wrong_riff = valid.clone();
        wrong_riff[..4].copy_from_slice(b"RIFX");
        assert_eq!(decode_wav(&wrong_riff), Err(PackDecodeError::Container));

        let mut wrong_wave = valid.clone();
        wrong_wave[8..12].copy_from_slice(b"AVI ");
        assert_eq!(decode_wav(&wrong_wave), Err(PackDecodeError::Container));

        let mut mismatched_size = valid.clone();
        overwrite_le_u32(&mut mismatched_size, 4, 0);
        assert_eq!(
            decode_wav(&mismatched_size),
            Err(PackDecodeError::Container)
        );

        let mut missing_fmt = valid.clone();
        missing_fmt[12..16].copy_from_slice(b"JUNK");
        assert_eq!(decode_wav(&missing_fmt), Err(PackDecodeError::Format));

        let mut duplicate_fmt = valid.clone();
        let fmt = valid[12..36].to_vec();
        duplicate_fmt.splice(36..36, fmt);
        let duplicate_fmt_size = (duplicate_fmt.len() - 8) as u32;
        overwrite_le_u32(&mut duplicate_fmt, 4, duplicate_fmt_size);
        assert_eq!(decode_wav(&duplicate_fmt), Err(PackDecodeError::Format));

        let mut missing_data = valid.clone();
        missing_data[36..40].copy_from_slice(b"JUNK");
        assert_eq!(decode_wav(&missing_data), Err(PackDecodeError::Container));

        let mut duplicate_data = valid.clone();
        append_riff_chunk(&mut duplicate_data, b"data", &[0, 0]);
        assert_eq!(decode_wav(&duplicate_data), Err(PackDecodeError::Container));

        let mut short_fmt = valid.clone();
        overwrite_le_u32(&mut short_fmt, 16, 12);
        assert_eq!(decode_wav(&short_fmt), Err(PackDecodeError::Format));

        let mut wrong_tag = valid.clone();
        overwrite_le_u16(&mut wrong_tag, 20, 2);
        assert_eq!(decode_wav(&wrong_tag), Err(PackDecodeError::Format));

        let mut wrong_alignment = valid.clone();
        overwrite_le_u16(&mut wrong_alignment, 32, 1);
        assert_eq!(decode_wav(&wrong_alignment), Err(PackDecodeError::Format));

        let mut wrong_byte_rate = valid.clone();
        overwrite_le_u32(&mut wrong_byte_rate, 28, 1);
        assert_eq!(decode_wav(&wrong_byte_rate), Err(PackDecodeError::Format));

        let mut partial_sample = valid.clone();
        partial_sample.push(0);
        partial_sample.push(0);
        overwrite_le_u32(&mut partial_sample, 40, 3);
        let partial_sample_size = (partial_sample.len() - 8) as u32;
        overwrite_le_u32(&mut partial_sample, 4, partial_sample_size);
        assert_eq!(decode_wav(&partial_sample), Err(PackDecodeError::Format));

        let mut partial_frame = wav_bytes(2, 48_000, &[0, 0]);
        partial_frame.truncate(partial_frame.len() - 2);
        overwrite_le_u32(&mut partial_frame, 40, 2);
        let partial_frame_size = (partial_frame.len() - 8) as u32;
        overwrite_le_u32(&mut partial_frame, 4, partial_frame_size);
        assert_eq!(
            decode_wav(&partial_frame),
            Err(PackDecodeError::IncompleteFrame)
        );

        let mut truncated_chunk = valid.clone();
        overwrite_le_u32(&mut truncated_chunk, 40, 4);
        assert_eq!(
            decode_wav(&truncated_chunk),
            Err(PackDecodeError::Container)
        );

        let mut trailing_bytes = valid;
        trailing_bytes.push(0);
        assert_eq!(decode_wav(&trailing_bytes), Err(PackDecodeError::Container));
    }

    #[test]
    fn accepts_exactly_two_seconds_of_complete_frames() {
        let decoded = decode_wav(&wav_bytes(2, 48_000, &vec![0; 192_000])).unwrap();
        assert_eq!(decoded.sample().frame_count(), 96_000);
    }
}
use std::{fmt, io::Cursor};

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

use crate::audio::{PcmSample, PcmSampleError};

pub const MIN_PACK_SAMPLE_RATE: u32 = 8_000;
pub const MAX_PACK_SAMPLE_RATE: u32 = 96_000;
pub const MAX_WAV_SECONDS: u32 = 2;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DecodedAudio {
    sample: PcmSample,
    canonical_wav: Vec<u8>,
}

impl DecodedAudio {
    pub(crate) fn sample(&self) -> &PcmSample {
        &self.sample
    }

    pub(crate) fn into_sample(self) -> PcmSample {
        self.sample
    }

    pub(crate) fn canonical_wav(&self) -> &[u8] {
        &self.canonical_wav
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackDecodeError {
    Container,
    Format,
    Channels,
    SampleRate,
    Empty,
    Duration,
    IncompleteFrame,
    Sample,
    Encode,
    DecodedMemory,
}

impl fmt::Display for PackDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Container => "invalid WAV container",
            Self::Format => "unsupported WAV format",
            Self::Channels => "unsupported WAV channel count",
            Self::SampleRate => "unsupported WAV sample rate",
            Self::Empty => "empty WAV data",
            Self::Duration => "WAV duration exceeds the limit",
            Self::IncompleteFrame => "WAV data ends with an incomplete frame",
            Self::Sample => "invalid WAV sample data",
            Self::Encode => "failed to canonicalize WAV data",
            Self::DecodedMemory => "decoded WAV data exceeds the memory limit",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for PackDecodeError {}

struct ExpectedWav {
    channels: u16,
    sample_rate: u32,
    sample_count: usize,
}

pub(crate) fn decode_wav(bytes: &[u8]) -> Result<DecodedAudio, PackDecodeError> {
    let expected = preflight_riff(bytes)?;
    let mut reader = WavReader::new(Cursor::new(bytes)).map_err(|_| PackDecodeError::Container)?;
    let spec = reader.spec();
    if spec.sample_format != SampleFormat::Int
        || spec.bits_per_sample != 16
        || spec.channels != expected.channels
        || spec.sample_rate != expected.sample_rate
    {
        return Err(PackDecodeError::Format);
    }

    let values: Vec<i16> = reader
        .samples::<i16>()
        .collect::<Result<_, _>>()
        .map_err(|_| PackDecodeError::Sample)?;
    if values.len() != expected.sample_count {
        return Err(PackDecodeError::Sample);
    }
    let pcm_values = values
        .iter()
        .map(|value| f32::from(*value) / 32_768.0)
        .collect();
    let sample =
        PcmSample::new(spec.sample_rate, spec.channels, pcm_values).map_err(map_pcm_error)?;
    let canonical_wav = encode_canonical(spec.channels, spec.sample_rate, &values)?;
    Ok(DecodedAudio {
        sample,
        canonical_wav,
    })
}

fn preflight_riff(bytes: &[u8]) -> Result<ExpectedWav, PackDecodeError> {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(PackDecodeError::Container);
    }
    let riff_size = usize::try_from(le_u32(bytes, 4)?).map_err(|_| PackDecodeError::Container)?;
    if riff_size.checked_add(8) != Some(bytes.len()) {
        return Err(PackDecodeError::Container);
    }

    let mut offset = 12_usize;
    let mut format = None;
    let mut data_length = None;
    while offset < bytes.len() {
        let chunk_id = bytes
            .get(offset..offset + 4)
            .ok_or(PackDecodeError::Container)?;
        let chunk_length =
            usize::try_from(le_u32(bytes, offset + 4)?).map_err(|_| PackDecodeError::Container)?;
        let data_start = offset.checked_add(8).ok_or(PackDecodeError::Container)?;
        let data_end = data_start
            .checked_add(chunk_length)
            .ok_or(PackDecodeError::Container)?;
        let chunk = bytes
            .get(data_start..data_end)
            .ok_or(PackDecodeError::Container)?;

        match chunk_id {
            b"fmt " => {
                if format.is_some() || chunk_length != 16 {
                    return Err(PackDecodeError::Format);
                }
                format = Some(parse_format(chunk)?);
            }
            b"data" if data_length.replace(chunk_length).is_some() => {
                return Err(PackDecodeError::Container);
            }
            b"data" => {}
            _ => {}
        }

        offset = data_end
            .checked_add(chunk_length % 2)
            .ok_or(PackDecodeError::Container)?;
        if offset > bytes.len() {
            return Err(PackDecodeError::Container);
        }
    }

    let (channels, sample_rate, block_align) = format.ok_or(PackDecodeError::Format)?;
    let data_length = data_length.ok_or(PackDecodeError::Container)?;
    if data_length == 0 {
        return Err(PackDecodeError::Empty);
    }
    if !data_length.is_multiple_of(2) {
        return Err(PackDecodeError::Format);
    }
    let block_align = usize::from(block_align);
    if !data_length.is_multiple_of(block_align) {
        return Err(PackDecodeError::IncompleteFrame);
    }
    let frames = data_length / block_align;
    let maximum_frames = usize::try_from(sample_rate)
        .ok()
        .and_then(|rate| rate.checked_mul(MAX_WAV_SECONDS as usize))
        .ok_or(PackDecodeError::Duration)?;
    if frames > maximum_frames {
        return Err(PackDecodeError::Duration);
    }
    let sample_count = frames
        .checked_mul(usize::from(channels))
        .ok_or(PackDecodeError::DecodedMemory)?;
    Ok(ExpectedWav {
        channels,
        sample_rate,
        sample_count,
    })
}

fn parse_format(chunk: &[u8]) -> Result<(u16, u32, u16), PackDecodeError> {
    if le_u16(chunk, 0)? != 1 || le_u16(chunk, 14)? != 16 {
        return Err(PackDecodeError::Format);
    }
    let channels = le_u16(chunk, 2)?;
    if !(1..=2).contains(&channels) {
        return Err(PackDecodeError::Channels);
    }
    let sample_rate = le_u32(chunk, 4)?;
    if !(MIN_PACK_SAMPLE_RATE..=MAX_PACK_SAMPLE_RATE).contains(&sample_rate) {
        return Err(PackDecodeError::SampleRate);
    }
    let block_align = le_u16(chunk, 12)?;
    let expected_block_align = channels.checked_mul(2).ok_or(PackDecodeError::Format)?;
    if block_align != expected_block_align {
        return Err(PackDecodeError::Format);
    }
    let expected_byte_rate = sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or(PackDecodeError::Format)?;
    if le_u32(chunk, 8)? != expected_byte_rate {
        return Err(PackDecodeError::Format);
    }
    Ok((channels, sample_rate, block_align))
}

fn le_u16(bytes: &[u8], offset: usize) -> Result<u16, PackDecodeError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(PackDecodeError::Container)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn le_u32(bytes: &[u8], offset: usize) -> Result<u32, PackDecodeError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(PackDecodeError::Container)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn encode_canonical(
    channels: u16,
    sample_rate: u32,
    values: &[i16],
) -> Result<Vec<u8>, PackDecodeError> {
    let mut cursor = Cursor::new(Vec::new());
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    {
        let mut writer = WavWriter::new(&mut cursor, spec).map_err(|_| PackDecodeError::Encode)?;
        for value in values {
            writer
                .write_sample(*value)
                .map_err(|_| PackDecodeError::Encode)?;
        }
        writer.finalize().map_err(|_| PackDecodeError::Encode)?;
    }
    Ok(cursor.into_inner())
}

fn map_pcm_error(error: PcmSampleError) -> PackDecodeError {
    match error {
        PcmSampleError::SampleRate => PackDecodeError::SampleRate,
        PcmSampleError::Channels => PackDecodeError::Channels,
        PcmSampleError::Empty => PackDecodeError::Empty,
        PcmSampleError::IncompleteFrame => PackDecodeError::IncompleteFrame,
        PcmSampleError::NonFinite | PcmSampleError::OutOfRange => PackDecodeError::Sample,
        PcmSampleError::TooLong => PackDecodeError::Duration,
    }
}
