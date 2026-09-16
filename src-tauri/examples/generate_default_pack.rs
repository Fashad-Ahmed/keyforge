use std::{
    ffi::OsString,
    fmt,
    fs::OpenOptions,
    io::{Cursor, Write},
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

use hound::{SampleFormat, WavSpec, WavWriter};
use zip::{write::SimpleFileOptions, CompressionMethod, System, ZipWriter};

const SAMPLE_RATE: u32 = 48_000;
const Q15_ONE: i64 = 32_767;
const PEAK_LIMIT: i64 = 28_800;
const MANIFEST: &[u8] = br#"{
  "schema_version": 1,
  "id": "keyforge-mechanical",
  "name": "KeyForge Mechanical",
  "pack_version": "1.0.0",
  "sounds": {
    "normal": [
      "sounds/normal-01.wav",
      "sounds/normal-02.wav",
      "sounds/normal-03.wav"
    ],
    "space": ["sounds/space-01.wav"],
    "enter": ["sounds/enter-01.wav"],
    "backspace": ["sounds/backspace-01.wav"],
    "modifier": ["sounds/modifier-01.wav"]
  }
}"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeneratorError {
    Arguments,
    Generate,
    OutputExists,
    Write,
}

impl fmt::Display for GeneratorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Arguments => "expected one relative output path",
            Self::Generate => "default pack generation failed",
            Self::OutputExists => "output already exists",
            Self::Write => "default pack write failed",
        };
        formatter.write_str(message)
    }
}

#[derive(Clone, Copy)]
enum SoundKind {
    Normal,
    Space,
    Enter,
    Backspace,
    Modifier,
}

#[derive(Clone, Copy)]
struct Voice {
    path: &'static str,
    seed: u32,
    frames: usize,
    body_hz: u32,
    kind: SoundKind,
    delayed_impulse: Option<usize>,
}

const VOICES: [Voice; 7] = [
    Voice {
        path: "sounds/backspace-01.wav",
        seed: 0x8a31_37d2,
        frames: 1_056,
        body_hz: 1_120,
        kind: SoundKind::Backspace,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/enter-01.wav",
        seed: 0xe713_40c5,
        frames: 1_920,
        body_hz: 690,
        kind: SoundKind::Enter,
        delayed_impulse: Some(672),
    },
    Voice {
        path: "sounds/modifier-01.wav",
        seed: 0x4d02_b981,
        frames: 864,
        body_hz: 1_460,
        kind: SoundKind::Modifier,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/normal-01.wav",
        seed: 0x1357_9bdf,
        frames: 1_440,
        body_hz: 920,
        kind: SoundKind::Normal,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/normal-02.wav",
        seed: 0x2468_ace1,
        frames: 1_536,
        body_hz: 1_010,
        kind: SoundKind::Normal,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/normal-03.wav",
        seed: 0x6c8e_9cf3,
        frames: 1_344,
        body_hz: 850,
        kind: SoundKind::Normal,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/space-01.wav",
        seed: 0xb529_7a4d,
        frames: 2_160,
        body_hz: 520,
        kind: SoundKind::Space,
        delayed_impulse: Some(960),
    },
];

fn main() -> ExitCode {
    match run(std::env::args_os()) {
        Ok(output) => {
            println!("{}", output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: impl IntoIterator<Item = OsString>) -> Result<PathBuf, GeneratorError> {
    let output = parse_output_argument(arguments)?;
    let archive = generate_archive()?;
    write_archive(&output, &archive)?;
    Ok(output)
}

fn parse_output_argument(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<PathBuf, GeneratorError> {
    let mut arguments = arguments.into_iter();
    let _program = arguments.next();
    let output = PathBuf::from(arguments.next().ok_or(GeneratorError::Arguments)?);
    if arguments.next().is_some()
        || output.as_os_str().is_empty()
        || output.to_str().is_none()
        || output.is_absolute()
        || output
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(GeneratorError::Arguments);
    }
    Ok(output)
}

fn write_archive(destination: &Path, archive: &[u8]) -> Result<(), GeneratorError> {
    let mut output = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(GeneratorError::OutputExists);
        }
        Err(_) => return Err(GeneratorError::Write),
    };
    output
        .write_all(archive)
        .map_err(|_| GeneratorError::Write)?;
    output.sync_all().map_err(|_| GeneratorError::Write)
}

fn generate_archive() -> Result<Vec<u8>, GeneratorError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .system(System::Unix)
        .unix_permissions(0o100644);

    writer
        .start_file("manifest.json", options)
        .map_err(|_| GeneratorError::Generate)?;
    writer
        .write_all(MANIFEST)
        .map_err(|_| GeneratorError::Generate)?;

    for voice in VOICES {
        let samples = mechanical_sample(voice);
        let wav = encode_wav(&samples)?;
        writer
            .start_file(voice.path, options)
            .map_err(|_| GeneratorError::Generate)?;
        writer
            .write_all(&wav)
            .map_err(|_| GeneratorError::Generate)?;
    }

    writer
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|_| GeneratorError::Generate)
}

fn encode_wav(samples: &[i16]) -> Result<Vec<u8>, GeneratorError> {
    let mut cursor = Cursor::new(Vec::new());
    let spec = WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::new(&mut cursor, spec).map_err(|_| GeneratorError::Generate)?;
    for sample in samples {
        writer
            .write_sample(*sample)
            .map_err(|_| GeneratorError::Generate)?;
    }
    writer.finalize().map_err(|_| GeneratorError::Generate)?;
    Ok(cursor.into_inner())
}

fn mechanical_sample(voice: Voice) -> Vec<i16> {
    let (noise_gain, noise_end, body_gain, body_end, click_gain, attack_frames, release_frames) =
        match voice.kind {
            SoundKind::Normal => (11_141, 512, 7_209, 2_048, 12_451, 12, 160),
            SoundKind::Space => (8_192, 2_048, 8_847, 4_096, 9_175, 18, 256),
            SoundKind::Enter => (10_158, 1_536, 9_830, 3_072, 11_469, 14, 224),
            SoundKind::Backspace => (9_503, 256, 5_898, 1_024, 13_762, 8, 144),
            SoundKind::Modifier => (7_536, 128, 4_915, 768, 11_469, 6, 128),
        };
    let mut state = voice.seed;

    (0..voice.frames)
        .map(|index| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let noise = i64::from(state >> 16) - 32_768;
            let noise_layer = scale_q15(
                scale_q15(noise, noise_gain),
                decay_envelope(index, voice.frames, noise_end),
            );
            let resonant_tail = scale_q15(
                scale_q15(
                    oscillator_sample(index, voice.body_hz, 0x4000_0000),
                    body_gain,
                ),
                decay_envelope(index, voice.frames, body_end),
            );
            let click = impulse(index, 36, 6_200, click_gain);
            let delayed_click = voice.delayed_impulse.map_or(0, |delay| {
                if index < delay {
                    return 0;
                }
                let delayed_index = index - delay;
                impulse(delayed_index, 64, 4_700, scale_q15(click_gain, 23_592))
            });
            let bounded_mix = (noise_layer + resonant_tail + click + delayed_click)
                .clamp(-PEAK_LIMIT, PEAK_LIMIT);
            let attack = attack_envelope(index, attack_frames);
            let release = release_envelope(index, voice.frames, release_frames);
            scale_q15(bounded_mix, scale_q15(attack, release)) as i16
        })
        .collect()
}

fn scale_q15(value: i64, gain: i64) -> i64 {
    value * gain / Q15_ONE
}

fn attack_envelope(index: usize, attack_frames: usize) -> i64 {
    if attack_frames <= 1 || index >= attack_frames - 1 {
        return Q15_ONE;
    }
    index as i64 * Q15_ONE / (attack_frames - 1) as i64
}

fn decay_envelope(index: usize, frames: usize, end_level: i64) -> i64 {
    if frames <= 1 {
        return end_level;
    }
    Q15_ONE - (Q15_ONE - end_level) * index as i64 / (frames - 1) as i64
}

fn release_envelope(index: usize, frames: usize, release_frames: usize) -> i64 {
    let release_start = frames.saturating_sub(release_frames);
    if index < release_start {
        return Q15_ONE;
    }
    let remaining = frames.saturating_sub(index + 1) as i64;
    let release_span = frames.saturating_sub(release_start + 1).max(1) as i64;
    let linear = remaining * Q15_ONE / release_span;
    scale_q15(linear, linear)
}

fn impulse(index: usize, frames: usize, frequency: u32, gain: i64) -> i64 {
    if index >= frames || frames <= 1 {
        return 0;
    }
    let remaining = (frames - index - 1) as i64;
    let linear = remaining * Q15_ONE / (frames - 1) as i64;
    let envelope = scale_q15(linear, linear);
    scale_q15(
        scale_q15(oscillator_sample(index, frequency, 0), gain),
        envelope,
    )
}

fn oscillator_sample(index: usize, frequency: u32, phase_offset: u32) -> i64 {
    let phase_step = (u64::from(frequency) << 32) / u64::from(SAMPLE_RATE);
    let phase = (index as u64 * phase_step) as u32;
    triangle_wave(phase.wrapping_add(phase_offset))
}

fn triangle_wave(phase: u32) -> i64 {
    let position = i64::from(phase >> 16);
    if position < 32_768 {
        position * 2 - 32_767
    } else {
        98_303 - position * 2
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        io::{Cursor, Read},
    };

    use hound::{SampleFormat, WavReader};
    use zip::{CompressionMethod, System, ZipArchive};

    use super::{generate_archive, parse_output_argument, write_archive, GeneratorError};

    const EXPECTED_ENTRIES: [&str; 8] = [
        "manifest.json",
        "sounds/backspace-01.wav",
        "sounds/enter-01.wav",
        "sounds/modifier-01.wav",
        "sounds/normal-01.wav",
        "sounds/normal-02.wav",
        "sounds/normal-03.wav",
        "sounds/space-01.wav",
    ];

    const COMMITTED_ARCHIVE: &[u8] = include_bytes!("../assets/packs/keyforge-mechanical.zip");

    #[test]
    fn synthesis_source_rejects_float_transcendental_math() {
        let source = include_str!("generate_default_pack.rs");
        let synthesis_start = source.find("fn mechanical_sample").unwrap();
        let synthesis_end = source[synthesis_start..]
            .find("\n#[cfg(test)]")
            .map(|offset| synthesis_start + offset)
            .unwrap();
        let synthesis = &source[synthesis_start..synthesis_end];

        for forbidden in [
            ".sin(", ".cos(", ".tan(", ".exp(", ".pow", ".sqrt(", "f32", "f64",
        ] {
            assert!(
                !synthesis.contains(forbidden),
                "synthesis contains nondeterministic float operation: {forbidden}"
            );
        }
    }

    #[test]
    fn generated_archive_matches_the_committed_asset() {
        assert_eq!(generate_archive().unwrap(), COMMITTED_ARCHIVE);
    }

    #[test]
    fn generation_is_byte_for_byte_reproducible_and_data_only() {
        let first = generate_archive().unwrap();
        let second = generate_archive().unwrap();

        assert_eq!(first, second);
        assert_eq!(
            central_directory_creator_systems(&first),
            vec![System::Unix as u8; EXPECTED_ENTRIES.len()]
        );

        let mut archive = ZipArchive::new(Cursor::new(first)).unwrap();
        assert_eq!(archive.comment(), b"");
        assert_eq!(archive.len(), EXPECTED_ENTRIES.len());
        for (index, expected_name) in EXPECTED_ENTRIES.into_iter().enumerate() {
            let entry = archive.by_index_raw(index).unwrap();
            assert_eq!(entry.name(), expected_name);
            assert_eq!(entry.compression(), CompressionMethod::Stored);
            assert_eq!(entry.unix_mode(), Some(0o100644));
            assert_eq!(entry.comment(), "");
        }
    }

    fn central_directory_creator_systems(archive: &[u8]) -> Vec<u8> {
        archive
            .windows(4)
            .enumerate()
            .filter_map(|(offset, signature)| {
                (signature == [0x50, 0x4b, 0x01, 0x02]).then(|| archive[offset + 5])
            })
            .collect()
    }

    #[test]
    fn generated_wavs_are_short_safe_fixed_format_pcm() {
        let bytes = generate_archive().unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();

        for name in EXPECTED_ENTRIES.into_iter().skip(1) {
            let mut entry = archive.by_name(name).unwrap();
            let mut wav = Vec::new();
            entry.read_to_end(&mut wav).unwrap();
            let mut reader = WavReader::new(Cursor::new(wav)).unwrap();
            let spec = reader.spec();
            assert_eq!(spec.channels, 1);
            assert_eq!(spec.sample_rate, 48_000);
            assert_eq!(spec.bits_per_sample, 16);
            assert_eq!(spec.sample_format, SampleFormat::Int);
            let samples = reader
                .samples::<i16>()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert!((864..=2_160).contains(&samples.len()));
            let peak = samples
                .iter()
                .map(|sample| sample.unsigned_abs())
                .max()
                .unwrap();
            assert!(peak > 1_638);
            assert!(peak < 29_490);
            assert_eq!(samples.last(), Some(&0), "{name} must end at zero");
            let tail_peak = samples[samples.len() - 16..]
                .iter()
                .map(|sample| sample.unsigned_abs())
                .max()
                .unwrap();
            assert!(tail_peak <= 512, "{name} tail peak {tail_peak} exceeds 512");
        }
    }

    #[test]
    fn cli_requires_one_relative_output_and_never_overwrites() {
        let output = parse_output_argument([
            OsString::from("generate_default_pack"),
            OsString::from("packs/default.zip"),
        ])
        .unwrap();
        assert_eq!(output, std::path::PathBuf::from("packs/default.zip"));
        assert_eq!(
            parse_output_argument([OsString::from("generate_default_pack")]),
            Err(GeneratorError::Arguments)
        );
        assert_eq!(
            parse_output_argument([
                OsString::from("generate_default_pack"),
                OsString::from("one.zip"),
                OsString::from("two.zip"),
            ]),
            Err(GeneratorError::Arguments)
        );
        assert_eq!(
            parse_output_argument([
                OsString::from("generate_default_pack"),
                OsString::from("/private/default.zip"),
            ]),
            Err(GeneratorError::Arguments)
        );

        let test_directory = std::env::temp_dir().join(format!(
            "keyforge-default-generator-test-{}",
            std::process::id()
        ));
        std::fs::create_dir(&test_directory).unwrap();
        let destination = test_directory.join("default.zip");
        let archive = generate_archive().unwrap();
        write_archive(&destination, &archive).unwrap();
        assert_eq!(
            write_archive(&destination, &archive),
            Err(GeneratorError::OutputExists)
        );
        std::fs::remove_file(destination).unwrap();
        std::fs::remove_dir(test_directory).unwrap();
    }
}
