use std::{
    ffi::OsString,
    fmt,
    fs::OpenOptions,
    io::{Cursor, Write},
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

use hound::{SampleFormat, WavSpec, WavWriter};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const SAMPLE_RATE: u32 = 48_000;
const PEAK_LIMIT: f32 = 0.88;
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
    body_hz: f32,
    kind: SoundKind,
    delayed_impulse: Option<usize>,
}

const VOICES: [Voice; 7] = [
    Voice {
        path: "sounds/backspace-01.wav",
        seed: 0x8a31_37d2,
        frames: 1_056,
        body_hz: 1_120.0,
        kind: SoundKind::Backspace,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/enter-01.wav",
        seed: 0xe713_40c5,
        frames: 1_920,
        body_hz: 690.0,
        kind: SoundKind::Enter,
        delayed_impulse: Some(672),
    },
    Voice {
        path: "sounds/modifier-01.wav",
        seed: 0x4d02_b981,
        frames: 864,
        body_hz: 1_460.0,
        kind: SoundKind::Modifier,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/normal-01.wav",
        seed: 0x1357_9bdf,
        frames: 1_440,
        body_hz: 920.0,
        kind: SoundKind::Normal,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/normal-02.wav",
        seed: 0x2468_ace1,
        frames: 1_536,
        body_hz: 1_010.0,
        kind: SoundKind::Normal,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/normal-03.wav",
        seed: 0x6c8e_9cf3,
        frames: 1_344,
        body_hz: 850.0,
        kind: SoundKind::Normal,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/space-01.wav",
        seed: 0xb529_7a4d,
        frames: 2_160,
        body_hz: 520.0,
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
    let (noise_gain, noise_decay, body_gain, body_decay, click_gain) = match voice.kind {
        SoundKind::Normal => (0.34, 115.0, 0.22, 50.0, 0.38),
        SoundKind::Space => (0.25, 72.0, 0.27, 36.0, 0.28),
        SoundKind::Enter => (0.31, 82.0, 0.30, 43.0, 0.35),
        SoundKind::Backspace => (0.29, 130.0, 0.18, 65.0, 0.42),
        SoundKind::Modifier => (0.23, 150.0, 0.15, 75.0, 0.35),
    };
    let mut state = voice.seed;

    (0..voice.frames)
        .map(|index| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let noise = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let time = index as f32 / SAMPLE_RATE as f32;
            let noise_attack = (index as f32 / 12.0).min(1.0);
            let noise_layer = noise * noise_gain * noise_attack * (-time * noise_decay).exp();
            let resonant_tail = (std::f32::consts::TAU * voice.body_hz * time).sin()
                * body_gain
                * (-time * body_decay).exp();
            let click_phase = std::f32::consts::TAU * 6_200.0 * time;
            let click = if index < 36 {
                click_phase.sin() * click_gain * (1.0 - index as f32 / 36.0).powi(2)
            } else {
                0.0
            };
            let delayed_click = voice.delayed_impulse.map_or(0.0, |delay| {
                if index < delay {
                    return 0.0;
                }
                let delayed_index = index - delay;
                if delayed_index >= 64 {
                    return 0.0;
                }
                let delayed_time = delayed_index as f32 / SAMPLE_RATE as f32;
                (std::f32::consts::TAU * 4_700.0 * delayed_time).sin()
                    * click_gain
                    * 0.72
                    * (1.0 - delayed_index as f32 / 64.0).powi(2)
            });
            let value = (noise_layer + resonant_tail + click + delayed_click)
                .clamp(-PEAK_LIMIT, PEAK_LIMIT);
            (value * i16::MAX as f32).round() as i16
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        io::{Cursor, Read},
    };

    use hound::{SampleFormat, WavReader};
    use zip::{CompressionMethod, ZipArchive};

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

    #[test]
    fn generation_is_byte_for_byte_reproducible_and_data_only() {
        let first = generate_archive().unwrap();
        let second = generate_archive().unwrap();

        assert_eq!(first, second);

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
            let peak = samples.into_iter().map(i16::unsigned_abs).max().unwrap();
            assert!(peak > 1_638);
            assert!(peak < 29_490);
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
