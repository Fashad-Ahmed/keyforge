use std::{
    ffi::OsString,
    fmt,
    fs::OpenOptions,
    io::{Cursor, Write},
    path::{Component, PathBuf},
    process::ExitCode,
};

use hound::{SampleFormat, WavReader};
use zip::{write::SimpleFileOptions, CompressionMethod, System, ZipWriter};

const SAMPLE_RATE: u32 = 48_000;
const MAX_FRAMES: u32 = SAMPLE_RATE * 2;

struct Pack {
    id: &'static str,
    manifest: &'static [u8],
    normal_source: &'static [u8],
    space_source: &'static [u8],
}

const PACKS: [Pack; 6] = [
    Pack {
        id: "keyforge-switch-linear",
        manifest: include_bytes!("../assets/sound-manifests/keyforge-switch-linear.json"),
        normal_source: include_bytes!("../assets/packs/sources/keychron-k10-keypress.wav"),
        space_source: include_bytes!("../assets/packs/sources/keychron-linear-spacebar.wav"),
    },
    Pack {
        id: "keyforge-switch-tactile",
        manifest: include_bytes!("../assets/sound-manifests/keyforge-switch-tactile.json"),
        normal_source: include_bytes!("../assets/packs/sources/stav-tactile.wav"),
        space_source: include_bytes!("../assets/packs/sources/stav-tactile.wav"),
    },
    Pack {
        id: "keyforge-switch-clicky",
        manifest: include_bytes!("../assets/sound-manifests/keyforge-switch-clicky.json"),
        normal_source: include_bytes!("../assets/packs/sources/stav-clicky.wav"),
        space_source: include_bytes!("../assets/packs/sources/stav-clicky.wav"),
    },
    Pack {
        id: "keyforge-playful-bubble",
        manifest: include_bytes!("../assets/sound-manifests/keyforge-playful-bubble.json"),
        normal_source: include_bytes!("../assets/packs/sources/bubble-pop.wav"),
        space_source: include_bytes!("../assets/packs/sources/bubble-pop.wav"),
    },
    Pack {
        id: "keyforge-playful-duck",
        manifest: include_bytes!("../assets/sound-manifests/keyforge-playful-duck.json"),
        normal_source: include_bytes!("../assets/packs/sources/rubber-duck.wav"),
        space_source: include_bytes!("../assets/packs/sources/rubber-duck.wav"),
    },
    Pack {
        id: "keyforge-playful-boing",
        manifest: include_bytes!("../assets/sound-manifests/keyforge-playful-boing.json"),
        normal_source: include_bytes!("../assets/packs/sources/cartoon-boing.wav"),
        space_source: include_bytes!("../assets/packs/sources/cartoon-boing.wav"),
    },
];

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
            Self::Arguments => "expected one known pack ID and one relative output path",
            Self::Generate => "curated pack generation failed",
            Self::OutputExists => "output already exists",
            Self::Write => "curated pack write failed",
        };
        formatter.write_str(message)
    }
}

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
    let mut arguments = arguments.into_iter();
    let _program = arguments.next();
    let id = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or(GeneratorError::Arguments)?;
    let output = arguments.next().ok_or(GeneratorError::Arguments)?;
    if arguments.next().is_some() {
        return Err(GeneratorError::Arguments);
    }
    let pack = PACKS
        .iter()
        .find(|pack| pack.id == id)
        .ok_or(GeneratorError::Arguments)?;
    let output = parse_output_argument([OsString::from("generator"), output])?;
    let archive = generate_archive(pack)?;
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(GeneratorError::OutputExists);
        }
        Err(_) => return Err(GeneratorError::Write),
    };
    file.write_all(&archive)
        .map_err(|_| GeneratorError::Write)?;
    file.sync_all().map_err(|_| GeneratorError::Write)?;
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

fn generate_archive(pack: &Pack) -> Result<Vec<u8>, GeneratorError> {
    validate_source(pack.normal_source)?;
    validate_source(pack.space_source)?;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .system(System::Unix)
        .unix_permissions(0o100644);

    writer
        .start_file("manifest.json", options)
        .map_err(|_| GeneratorError::Generate)?;
    writer
        .write_all(pack.manifest)
        .map_err(|_| GeneratorError::Generate)?;

    for (name, source) in [
        ("sounds/backspace-01.wav", pack.normal_source),
        ("sounds/enter-01.wav", pack.normal_source),
        ("sounds/modifier-01.wav", pack.normal_source),
        ("sounds/normal-01.wav", pack.normal_source),
        ("sounds/space-01.wav", pack.space_source),
    ] {
        writer
            .start_file(name, options)
            .map_err(|_| GeneratorError::Generate)?;
        writer
            .write_all(source)
            .map_err(|_| GeneratorError::Generate)?;
    }

    writer
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|_| GeneratorError::Generate)
}

fn validate_source(bytes: &[u8]) -> Result<(), GeneratorError> {
    let reader = WavReader::new(Cursor::new(bytes)).map_err(|_| GeneratorError::Generate)?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != SAMPLE_RATE
        || spec.bits_per_sample != 16
        || spec.sample_format != SampleFormat::Int
        || reader.duration() == 0
        || reader.duration() > MAX_FRAMES
    {
        return Err(GeneratorError::Generate);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use zip::{CompressionMethod, ZipArchive};

    use super::{generate_archive, parse_output_argument, validate_source, GeneratorError, PACKS};

    const COMMITTED_ARCHIVES: [&[u8]; 6] = [
        include_bytes!("../assets/packs/keyforge-switch-linear.zip"),
        include_bytes!("../assets/packs/keyforge-switch-tactile.zip"),
        include_bytes!("../assets/packs/keyforge-switch-clicky.zip"),
        include_bytes!("../assets/packs/keyforge-playful-bubble.zip"),
        include_bytes!("../assets/packs/keyforge-playful-duck.zip"),
        include_bytes!("../assets/packs/keyforge-playful-boing.zip"),
    ];

    #[test]
    fn each_source_is_valid_short_mono_pcm_and_profiles_are_distinct() {
        let archives = PACKS
            .iter()
            .map(|pack| {
                validate_source(pack.normal_source).unwrap();
                validate_source(pack.space_source).unwrap();
                generate_archive(pack).unwrap()
            })
            .collect::<Vec<_>>();

        for left in 0..archives.len() {
            for right in left + 1..archives.len() {
                assert_ne!(archives[left], archives[right]);
            }
        }
    }

    #[test]
    fn curated_archives_reproduce_the_committed_assets() {
        for (pack, committed) in PACKS.iter().zip(COMMITTED_ARCHIVES) {
            assert_eq!(generate_archive(pack).unwrap(), committed, "{}", pack.id);
        }
    }

    #[test]
    fn archive_entries_are_audio_data_only_and_referenced_once() {
        let archive = generate_archive(&PACKS[0]).unwrap();
        let mut reader = ZipArchive::new(std::io::Cursor::new(archive)).unwrap();
        assert_eq!(reader.len(), 6);
        assert_eq!(reader.comment(), b"");
        for (index, name) in [
            "manifest.json",
            "sounds/backspace-01.wav",
            "sounds/enter-01.wav",
            "sounds/modifier-01.wav",
            "sounds/normal-01.wav",
            "sounds/space-01.wav",
        ]
        .into_iter()
        .enumerate()
        {
            let mut entry = reader.by_index_raw(index).unwrap();
            assert_eq!(entry.name(), name);
            assert_eq!(entry.compression(), CompressionMethod::Stored);
            assert_eq!(entry.unix_mode(), Some(0o100644));
            assert_eq!(entry.comment(), "");
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if name.ends_with(".wav") {
                assert_eq!(&bytes[..4], b"RIFF");
                assert_eq!(&bytes[8..12], b"WAVE");
            }
        }
    }

    #[test]
    fn linear_pack_uses_a_specific_recording_for_its_spacebar() {
        assert_ne!(PACKS[0].normal_source, PACKS[0].space_source);
        let mut archive =
            ZipArchive::new(std::io::Cursor::new(generate_archive(&PACKS[0]).unwrap())).unwrap();
        let mut normal = Vec::new();
        archive
            .by_name("sounds/normal-01.wav")
            .unwrap()
            .read_to_end(&mut normal)
            .unwrap();
        let mut space = Vec::new();
        archive
            .by_name("sounds/space-01.wav")
            .unwrap()
            .read_to_end(&mut space)
            .unwrap();
        assert_ne!(normal, space);
    }

    #[test]
    fn output_paths_must_remain_relative_and_unambiguous() {
        for invalid in ["", "/tmp/out.zip", "../out.zip", "a/../out.zip"] {
            assert_eq!(
                parse_output_argument(["generator".into(), invalid.into()]),
                Err(GeneratorError::Arguments)
            );
        }
    }
}
