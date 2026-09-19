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
#[derive(Clone, Copy)]
struct Profile {
    id: &'static str,
    name: &'static str,
    version: &'static str,
    seed_mask: u32,
    contact_gain: i64,
    body_gain: i64,
    shell_gain: i64,
    body_smoothing: i64,
    duration_scale: i64,
    body_decay_power: u32,
}

const PROFILES: [Profile; 16] = [
    Profile {
        id: "keyforge-mechanical",
        name: "Classic Mechanical",
        version: "1.2.0",
        seed_mask: 0,
        contact_gain: Q15_ONE,
        body_gain: Q15_ONE,
        shell_gain: Q15_ONE,
        body_smoothing: Q15_ONE,
        duration_scale: Q15_ONE,
        body_decay_power: 3,
    },
    Profile {
        id: "keyforge-deep-thock",
        name: "Deep Thock",
        version: "1.1.0",
        seed_mask: 0xa391_6f2d,
        contact_gain: 20_500,
        body_gain: 32_767,
        shell_gain: 21_500,
        body_smoothing: 46_000,
        duration_scale: 49_152,
        body_decay_power: 1,
    },
    Profile {
        id: "keyforge-crisp-click",
        name: "Crisp Click",
        version: "1.1.0",
        seed_mask: 0x4c17_b8e3,
        contact_gain: 32_767,
        body_gain: 17_800,
        shell_gain: 29_500,
        body_smoothing: 13_000,
        duration_scale: 18_022,
        body_decay_power: 4,
    },
    Profile {
        id: "keyforge-soft-linear",
        name: "Soft Linear",
        version: "1.1.0",
        seed_mask: 0xd248_31ac,
        contact_gain: 13_800,
        body_gain: 18_500,
        shell_gain: 11_500,
        body_smoothing: 42_000,
        duration_scale: 23_593,
        body_decay_power: 3,
    },
    Profile {
        id: "keyforge-creamy-tactile",
        name: "Creamy Tactile",
        version: "1.0.0",
        seed_mask: 0x31d7_80a5,
        contact_gain: 18_200,
        body_gain: 30_500,
        shell_gain: 14_200,
        body_smoothing: 48_000,
        duration_scale: 38_200,
        body_decay_power: 2,
    },
    Profile {
        id: "keyforge-silent-mechanical",
        name: "Silent Mechanical",
        version: "1.0.0",
        seed_mask: 0x7b14_c269,
        contact_gain: 6_400,
        body_gain: 8_800,
        shell_gain: 3_800,
        body_smoothing: 58_000,
        duration_scale: 14_000,
        body_decay_power: 4,
    },
    Profile {
        id: "keyforge-buckling-spring",
        name: "Buckling Spring",
        version: "1.0.0",
        seed_mask: 0xbe62_39f1,
        contact_gain: 31_000,
        body_gain: 24_500,
        shell_gain: 32_767,
        body_smoothing: 11_000,
        duration_scale: 60_000,
        body_decay_power: 1,
    },
    Profile {
        id: "keyforge-vintage-typewriter",
        name: "Vintage Typewriter",
        version: "1.0.0",
        seed_mask: 0xe047_15bc,
        contact_gain: 32_767,
        body_gain: 20_500,
        shell_gain: 31_500,
        body_smoothing: 15_500,
        duration_scale: 75_000,
        body_decay_power: 2,
    },
    Profile {
        id: "keyforge-marble-thock",
        name: "Marble Thock",
        version: "1.0.0",
        seed_mask: 0x94a8_5ed3,
        contact_gain: 14_800,
        body_gain: 32_767,
        shell_gain: 10_500,
        body_smoothing: 62_000,
        duration_scale: 52_000,
        body_decay_power: 1,
    },
    Profile {
        id: "keyforge-poppy-tactile",
        name: "Poppy Tactile",
        version: "1.0.0",
        seed_mask: 0x1f83_d942,
        contact_gain: 20_000,
        body_gain: 24_000,
        shell_gain: 18_500,
        body_smoothing: 21_000,
        duration_scale: 24_500,
        body_decay_power: 1,
    },
    Profile {
        id: "keyforge-clacky-aluminum",
        name: "Clacky Aluminum",
        version: "1.0.0",
        seed_mask: 0x56cb_270e,
        contact_gain: 29_500,
        body_gain: 19_000,
        shell_gain: 32_767,
        body_smoothing: 10_500,
        duration_scale: 41_000,
        body_decay_power: 2,
    },
    Profile {
        id: "keyforge-dampened-polycarbonate",
        name: "Dampened Polycarbonate",
        version: "1.0.0",
        seed_mask: 0xc218_74a9,
        contact_gain: 12_500,
        body_gain: 23_500,
        shell_gain: 7_200,
        body_smoothing: 54_000,
        duration_scale: 30_000,
        body_decay_power: 4,
    },
    Profile {
        id: "keyforge-retro-terminal",
        name: "Retro Terminal",
        version: "1.0.0",
        seed_mask: 0x0ad5_b631,
        contact_gain: 27_500,
        body_gain: 13_000,
        shell_gain: 20_500,
        body_smoothing: 17_500,
        duration_scale: 21_500,
        body_decay_power: 4,
    },
    Profile {
        id: "keyforge-arcade",
        name: "Arcade",
        version: "1.0.0",
        seed_mask: 0xf463_1b87,
        contact_gain: 32_767,
        body_gain: 21_000,
        shell_gain: 27_500,
        body_smoothing: 14_000,
        duration_scale: 14_000,
        body_decay_power: 3,
    },
    Profile {
        id: "keyforge-soft-office",
        name: "Soft Office",
        version: "1.0.0",
        seed_mask: 0x6e29_a4dc,
        contact_gain: 7_800,
        body_gain: 12_500,
        shell_gain: 4_600,
        body_smoothing: 60_000,
        duration_scale: 19_000,
        body_decay_power: 4,
    },
    Profile {
        id: "keyforge-sci-fi-console",
        name: "Sci-Fi Console",
        version: "1.0.0",
        seed_mask: 0xa70c_e358,
        contact_gain: 12_000,
        body_gain: 4_000,
        shell_gain: 12_000,
        body_smoothing: 9_000,
        duration_scale: 18_000,
        body_decay_power: 2,
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
        frames: 2_400,
        body_hz: 1_120,
        kind: SoundKind::Backspace,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/enter-01.wav",
        seed: 0xe713_40c5,
        frames: 4_200,
        body_hz: 690,
        kind: SoundKind::Enter,
        delayed_impulse: Some(720),
    },
    Voice {
        path: "sounds/modifier-01.wav",
        seed: 0x4d02_b981,
        frames: 1_920,
        body_hz: 1_460,
        kind: SoundKind::Modifier,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/normal-01.wav",
        seed: 0x1357_9bdf,
        frames: 2_880,
        body_hz: 920,
        kind: SoundKind::Normal,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/normal-02.wav",
        seed: 0x2468_ace1,
        frames: 3_000,
        body_hz: 1_010,
        kind: SoundKind::Normal,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/normal-03.wav",
        seed: 0x6c8e_9cf3,
        frames: 2_640,
        body_hz: 850,
        kind: SoundKind::Normal,
        delayed_impulse: None,
    },
    Voice {
        path: "sounds/space-01.wav",
        seed: 0xb529_7a4d,
        frames: 4_800,
        body_hz: 520,
        kind: SoundKind::Space,
        delayed_impulse: Some(900),
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
    let (profile, output) = parse_arguments(arguments)?;
    let archive = generate_archive(profile)?;
    write_archive(&output, &archive)?;
    Ok(output)
}

fn parse_arguments(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<(Profile, PathBuf), GeneratorError> {
    let mut arguments = arguments.into_iter();
    let _program = arguments.next();
    let profile_id = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or(GeneratorError::Arguments)?;
    let output = arguments.next().ok_or(GeneratorError::Arguments)?;
    if arguments.next().is_some() {
        return Err(GeneratorError::Arguments);
    }
    let profile = profile_from_id(&profile_id)?;
    let output = parse_output_argument([OsString::from("generator"), output])?;
    Ok((profile, output))
}

fn profile_from_id(id: &str) -> Result<Profile, GeneratorError> {
    PROFILES
        .into_iter()
        .find(|profile| profile.id == id)
        .ok_or(GeneratorError::Arguments)
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

fn generate_archive(profile: Profile) -> Result<Vec<u8>, GeneratorError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .system(System::Unix)
        .unix_permissions(0o100644);

    writer
        .start_file("manifest.json", options)
        .map_err(|_| GeneratorError::Generate)?;
    writer
        .write_all(&manifest(profile))
        .map_err(|_| GeneratorError::Generate)?;

    for voice in VOICES {
        let samples = mechanical_sample(voice, profile);
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

fn manifest(profile: Profile) -> Vec<u8> {
    format!(
        "{{\n  \"schema_version\": 1,\n  \"id\": \"{}\",\n  \"name\": \"{}\",\n  \"pack_version\": \"{}\",\n  \"sounds\": {{\n    \"normal\": [\n      \"sounds/normal-01.wav\",\n      \"sounds/normal-02.wav\",\n      \"sounds/normal-03.wav\"\n    ],\n    \"space\": [\"sounds/space-01.wav\"],\n    \"enter\": [\"sounds/enter-01.wav\"],\n    \"backspace\": [\"sounds/backspace-01.wav\"],\n    \"modifier\": [\"sounds/modifier-01.wav\"]\n  }}\n}}",
        profile.id, profile.name, profile.version
    )
    .into_bytes()
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

fn mechanical_sample(voice: Voice, profile: Profile) -> Vec<i16> {
    let (contact_gain, body_gain, shell_gain, contact_frames, body_frames, release_frames) =
        match voice.kind {
            SoundKind::Normal => (19_200, 12_400, 6_800, 260, 1_900, 180),
            SoundKind::Space => (13_800, 17_400, 8_600, 340, 3_800, 300),
            SoundKind::Enter => (16_200, 15_800, 8_200, 300, 3_200, 260),
            SoundKind::Backspace => (20_400, 10_800, 7_800, 220, 1_700, 170),
            SoundKind::Modifier => (18_600, 9_400, 6_400, 190, 1_350, 150),
        };
    let frames = scale_frames(voice.frames, profile.duration_scale);
    let contact_frames = scale_frames(contact_frames, profile.duration_scale);
    let body_frames = scale_frames(body_frames, profile.duration_scale);
    let release_frames = scale_frames(release_frames, profile.duration_scale);
    let delayed_impulse = voice
        .delayed_impulse
        .map(|delay| scale_frames(delay, profile.duration_scale));
    let mut state = voice.seed ^ profile.seed_mask;
    let mut previous_noise = 0;
    let mut fast_filter = 0;
    let mut slow_filter = 0;
    let mut body_filter = 0;
    let base_smoothing = (SAMPLE_RATE / voice.body_hz).clamp(18, 72) as i64;
    let body_smoothing = scale_q15(base_smoothing, profile.body_smoothing).clamp(10, 90);
    let mut samples = Vec::with_capacity(frames);

    for index in 0..frames {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let noise = i64::from(state >> 16) - 32_768;
        fast_filter += (noise - fast_filter) / 3;
        slow_filter += (noise - slow_filter) / 14;
        body_filter += (noise - body_filter) / body_smoothing;

        let high_contact = noise - previous_noise;
        let shell = fast_filter - slow_filter;
        let contact = scale_q15(high_contact, impact_envelope(index, 0, contact_frames));
        let bottom_out = scale_q15(
            body_filter,
            impact_envelope_power(index, 18, body_frames, profile.body_decay_power),
        );
        let case_resonance = scale_q15(
            shell,
            impact_envelope(index, 42, body_frames.saturating_mul(2) / 3),
        );
        let stabilizer = delayed_impulse.map_or(0, |delay| {
            let first = scale_q15(
                high_contact,
                impact_envelope(index, delay, contact_frames.saturating_mul(3) / 4),
            );
            let second = scale_q15(
                shell,
                impact_envelope(index, delay + 96, contact_frames / 2),
            );
            scale_q15(first + second, 15_600)
        });
        previous_noise = noise;

        let mixed = scale_q15(scale_q15(contact, contact_gain), profile.contact_gain)
            + scale_q15(scale_q15(bottom_out, body_gain), profile.body_gain)
            + scale_q15(scale_q15(case_resonance, shell_gain), profile.shell_gain)
            + stabilizer;
        let attack = attack_envelope(index, 4);
        let release = release_envelope(index, frames, release_frames);
        samples.push(scale_q15(mixed, scale_q15(attack, release)));
    }

    normalize_samples(samples)
}

fn scale_frames(frames: usize, scale: i64) -> usize {
    ((frames as i64 * scale / Q15_ONE).max(1)) as usize
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

fn impact_envelope(index: usize, start: usize, frames: usize) -> i64 {
    impact_envelope_power(index, start, frames, 3)
}

fn impact_envelope_power(index: usize, start: usize, frames: usize, power: u32) -> i64 {
    if index < start || frames <= 1 {
        return 0;
    }
    let elapsed = index - start;
    if elapsed >= frames {
        return 0;
    }
    let remaining = (frames - elapsed - 1) as i128;
    let span = (frames - 1) as i128;
    (i128::from(Q15_ONE) * integer_power(remaining, power) / integer_power(span, power)) as i64
}

fn integer_power(value: i128, exponent: u32) -> i128 {
    (0..exponent).fold(1, |result, _| result * value)
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

fn normalize_samples(mut samples: Vec<i64>) -> Vec<i16> {
    let peak = samples.iter().map(|sample| sample.abs()).max().unwrap_or(1);
    if peak > PEAK_LIMIT {
        for sample in &mut samples {
            *sample = *sample * PEAK_LIMIT / peak;
        }
    }
    samples.into_iter().map(|sample| sample as i16).collect()
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        io::{Cursor, Read},
    };

    use hound::{SampleFormat, WavReader};
    use zip::{CompressionMethod, System, ZipArchive};

    use super::{
        generate_archive, mechanical_sample, parse_output_argument, profile_from_id, write_archive,
        GeneratorError, PROFILES, VOICES,
    };

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

    const COMMITTED_ARCHIVES: [&[u8]; 16] = [
        include_bytes!("../assets/packs/keyforge-mechanical.zip"),
        include_bytes!("../assets/packs/keyforge-deep-thock.zip"),
        include_bytes!("../assets/packs/keyforge-crisp-click.zip"),
        include_bytes!("../assets/packs/keyforge-soft-linear.zip"),
        include_bytes!("../assets/packs/keyforge-creamy-tactile.zip"),
        include_bytes!("../assets/packs/keyforge-silent-mechanical.zip"),
        include_bytes!("../assets/packs/keyforge-buckling-spring.zip"),
        include_bytes!("../assets/packs/keyforge-vintage-typewriter.zip"),
        include_bytes!("../assets/packs/keyforge-marble-thock.zip"),
        include_bytes!("../assets/packs/keyforge-poppy-tactile.zip"),
        include_bytes!("../assets/packs/keyforge-clacky-aluminum.zip"),
        include_bytes!("../assets/packs/keyforge-dampened-polycarbonate.zip"),
        include_bytes!("../assets/packs/keyforge-retro-terminal.zip"),
        include_bytes!("../assets/packs/keyforge-arcade.zip"),
        include_bytes!("../assets/packs/keyforge-soft-office.zip"),
        include_bytes!("../assets/packs/keyforge-sci-fi-console.zip"),
    ];

    #[test]
    fn generator_defines_the_complete_distinct_sound_library() {
        assert_eq!(
            PROFILES
                .into_iter()
                .map(|profile| (profile.id, profile.name))
                .collect::<Vec<_>>(),
            vec![
                ("keyforge-mechanical", "Classic Mechanical"),
                ("keyforge-deep-thock", "Deep Thock"),
                ("keyforge-crisp-click", "Crisp Click"),
                ("keyforge-soft-linear", "Soft Linear"),
                ("keyforge-creamy-tactile", "Creamy Tactile"),
                ("keyforge-silent-mechanical", "Silent Mechanical"),
                ("keyforge-buckling-spring", "Buckling Spring"),
                ("keyforge-vintage-typewriter", "Vintage Typewriter"),
                ("keyforge-marble-thock", "Marble Thock"),
                ("keyforge-poppy-tactile", "Poppy Tactile"),
                ("keyforge-clacky-aluminum", "Clacky Aluminum"),
                ("keyforge-dampened-polycarbonate", "Dampened Polycarbonate"),
                ("keyforge-retro-terminal", "Retro Terminal"),
                ("keyforge-arcade", "Arcade"),
                ("keyforge-soft-office", "Soft Office"),
                ("keyforge-sci-fi-console", "Sci-Fi Console"),
            ]
        );
        let archives = PROFILES
            .into_iter()
            .map(|profile| generate_archive(profile).unwrap())
            .collect::<Vec<_>>();

        for profile in PROFILES {
            assert_eq!(profile_from_id(profile.id).unwrap().id, profile.id);
        }
        for left in 0..archives.len() {
            for right in left + 1..archives.len() {
                assert_ne!(archives[left], archives[right]);
            }
        }
    }

    #[test]
    fn profiles_have_distinct_perceptual_envelopes() {
        let normal = VOICES
            .into_iter()
            .find(|voice| voice.path == "sounds/normal-01.wav")
            .unwrap();
        let classic = mechanical_sample(normal, PROFILES[0]);
        let thock = mechanical_sample(normal, PROFILES[1]);
        let click = mechanical_sample(normal, PROFILES[2]);
        let linear = mechanical_sample(normal, PROFILES[3]);

        assert!(thock.len() >= 3_600, "thock must have a long body decay");
        assert!(click.len() <= 1_800, "click must be short and immediate");
        assert!(linear.len() <= 2_200, "linear must be short and damped");
        assert!(classic.len() > linear.len(), "classic must outlast linear");

        let early_energy = |samples: &[i16]| {
            samples
                .iter()
                .take(64)
                .map(|sample| i64::from(*sample).pow(2))
                .sum::<i64>()
        };
        let total_energy = |samples: &[i16]| {
            samples
                .iter()
                .map(|sample| i64::from(*sample).pow(2))
                .sum::<i64>()
        };
        let early_share =
            |samples: &[i16]| early_energy(samples) as f64 / total_energy(samples).max(1) as f64;

        let thock_early_share = early_share(&thock);
        let click_early_share = early_share(&click);
        let linear_early_share = early_share(&linear);
        assert!(
            click_early_share > thock_early_share * 1.4,
            "click attack share {click_early_share:.3} must exceed thock {thock_early_share:.3}"
        );
        assert!(
            linear_early_share > thock_early_share * 1.35,
            "linear attack share {linear_early_share:.3} must exceed thock {thock_early_share:.3}"
        );
    }

    #[test]
    fn every_profile_pair_has_a_distinct_perceptual_signature() {
        let normal = VOICES
            .into_iter()
            .find(|voice| voice.path == "sounds/normal-01.wav")
            .unwrap();
        let signatures = PROFILES
            .into_iter()
            .map(|profile| {
                let samples = mechanical_sample(normal, profile);
                let total_energy = samples
                    .iter()
                    .map(|sample| i64::from(*sample).pow(2))
                    .sum::<i64>()
                    .max(1);
                let early_energy = samples
                    .iter()
                    .take(64)
                    .map(|sample| i64::from(*sample).pow(2))
                    .sum::<i64>();
                let crossings = samples
                    .windows(2)
                    .filter(|pair| pair[0].is_negative() != pair[1].is_negative())
                    .count();
                let rms = (total_energy as f64 / samples.len() as f64).sqrt();
                (
                    profile.id,
                    samples.len() as f64,
                    early_energy as f64 / total_energy as f64,
                    crossings as f64 / samples.len() as f64,
                    rms,
                )
            })
            .collect::<Vec<_>>();

        let mut overlaps = Vec::new();
        for left in 0..signatures.len() {
            for right in left + 1..signatures.len() {
                let (left_id, left_frames, left_attack, left_crossings, left_rms) =
                    signatures[left];
                let (right_id, right_frames, right_attack, right_crossings, right_rms) =
                    signatures[right];
                let duration_ratio = left_frames.max(right_frames) / left_frames.min(right_frames);
                let rms_ratio = left_rms.max(right_rms) / left_rms.min(right_rms).max(1.0);
                let separated = duration_ratio >= 1.15
                    || (left_attack - right_attack).abs() >= 0.08
                    || (left_crossings - right_crossings).abs() >= 0.08
                    || rms_ratio >= 1.3;

                if !separated {
                    overlaps.push(format!(
                        "{left_id} and {right_id}: duration {duration_ratio:.2}, attack delta {:.3}, crossing delta {:.3}, rms {rms_ratio:.2}",
                        (left_attack - right_attack).abs(),
                        (left_crossings - right_crossings).abs(),
                    ));
                }
            }
        }
        assert!(
            overlaps.is_empty(),
            "profiles are too similar:\n{}",
            overlaps.join("\n")
        );
    }

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
    fn mechanical_samples_do_not_have_tonal_resonant_tails() {
        for voice in VOICES {
            let samples = mechanical_sample(voice, PROFILES[0]);
            let lag = (super::SAMPLE_RATE / voice.body_hz) as usize;
            let start = 192.min(samples.len() / 4);
            let end = samples.len().saturating_sub(lag + 32);
            let first = &samples[start..end];
            let delayed = &samples[start + lag..end + lag];
            let dot = first
                .iter()
                .zip(delayed)
                .map(|(left, right)| f64::from(*left) * f64::from(*right))
                .sum::<f64>();
            let first_energy = first
                .iter()
                .map(|sample| f64::from(*sample).powi(2))
                .sum::<f64>();
            let delayed_energy = delayed
                .iter()
                .map(|sample| f64::from(*sample).powi(2))
                .sum::<f64>();
            let correlation = dot.abs() / (first_energy * delayed_energy).sqrt();

            assert!(
                correlation < 0.45,
                "{} has a tonal tail correlation of {correlation:.3}",
                voice.path
            );
        }
    }

    #[test]
    fn generated_archives_match_the_committed_assets() {
        for (profile, committed) in PROFILES.into_iter().zip(COMMITTED_ARCHIVES) {
            assert_eq!(generate_archive(profile).unwrap(), committed);
        }
    }

    #[test]
    fn generation_is_byte_for_byte_reproducible_and_data_only() {
        let first = generate_archive(PROFILES[0]).unwrap();
        let second = generate_archive(PROFILES[0]).unwrap();

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
        let bytes = generate_archive(PROFILES[0]).unwrap();
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
            assert!((1_920..=4_800).contains(&samples.len()));
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
        let archive = generate_archive(PROFILES[0]).unwrap();
        write_archive(&destination, &archive).unwrap();
        assert_eq!(
            write_archive(&destination, &archive),
            Err(GeneratorError::OutputExists)
        );
        std::fs::remove_file(destination).unwrap();
        std::fs::remove_dir(test_directory).unwrap();
    }
}
