# Secure Sound-Pack Format Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a native, fail-closed ZIP sound-pack importer that validates a strict five-group manifest, decodes a narrow signed-16-bit PCM WAV subset, installs canonical data atomically into managed storage, and supplies only validated PCM to the existing audio registry.

**Architecture:** A Rust-only `pack` subsystem separates manifest, archive, WAV, storage, and orchestration responsibilities. Imports are fully validated and decoded in memory before any staging directory is created; staged output is canonical JSON plus re-encoded WAV, verified again, and renamed within the managed root. The frontend, Tauri IPC surface, permissions, production startup, and networking remain unchanged.

**Tech Stack:** Rust 1.88.0, Tauri 2, `serde`/`serde_json`, `zip` 8.6 with default features disabled and `deflate-flate2-zlib-rs` only, `hound` 3.5.1, Vitest security-policy tests, pnpm.

**Spec:** `docs/superpowers/specs/2026-08-25-sound-pack-format-design.md`

## Global Constraints

- Read `AGENTS.md`, `SECURITY.md`, the design spec, `docs/architecture/trust-boundaries.md`, and `docs/security/threat-model.md` before Task 1. Their security invariants are mandatory.
- Work only in the isolated `codex/sound-pack-format` worktree. Do not implement on `main`.
- Use red-green-refactor TDD for every behavior change. Run each stated red command and observe the expected failure before writing production code.
- Make exactly one focused commit per completed task. Do not combine tasks or skip review gates.
- Raw keyboard events and typed content never enter this milestone.
- Add no Tauri command, event, capability, permission, frontend behavior, or production startup hook.
- Add no networking, telemetry, analytics, logging framework, account, scripting, executable-pack, dynamic-loading, database, or updater dependency.
- Accept ZIP input only. Accept only Stored or Deflate entries.
- Accept WAV only: RIFF/WAVE, format tag `1`, signed 16-bit integer PCM, mono/stereo, 8,000–96,000 Hz, non-empty, and at most two seconds.
- Enforce: 16 MiB source archive, 128 entries, 64 KiB manifest, 64 WAVs, 8 MiB expanded per WAV, 64 MiB expanded archive, 100:1 per-entry ratio, and 64 MiB decoded PCM per pack.
- Require exactly `normal`, `space`, `enter`, `backspace`, and `modifier`; each has 1–16 unique canonical WAV references and the pack has at most 64 references.
- Pack IDs and paths are lowercase ASCII and reject Windows reserved device basenames.
- Duplicate pack IDs are rejected; M3 never replaces or deletes installed packs.
- The bundled pack receives the same validation and installation path as an imported archive.
- Errors exposed by public native types are sanitized and contain no absolute paths, archive contents, decoder strings, usernames, or device information.
- Tests use test-owned temporary roots only and never a real application-data directory or real audio device.
- Before every Rust commit run `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, and the relevant locked Rust tests.
- Before every security-policy or documentation commit run `pnpm test` and `git diff --check`.

## File Map

### Create

- `src-tauri/src/pack/mod.rs` — public native pack API, common types, sanitized top-level errors, limits, and orchestration.
- `src-tauri/src/pack/manifest.rs` — strict schema parsing, pack identity/version rules, canonical sound paths, and fixed sound groups.
- `src-tauri/src/pack/decoder.rs` — RIFF preflight, bounded signed-16 PCM decoding, and canonical WAV re-encoding.
- `src-tauri/src/pack/archive.rs` — ZIP inventory, raw-name/type/compression checks, bounded entry reads, reference cross-checking, and pack decoding.
- `src-tauri/src/pack/storage.rs` — trusted managed-root validation, staging transaction, canonical writes, verification, commit, discovery, and scoped cleanup.
- `src-tauri/src/pack/test_support.rs` — test-only JSON, WAV, ZIP, byte-mutation, and temporary-root fixtures.
- `src-tauri/assets/packs/keyforge-mechanical.zip` — bundled data-only default pack.
- `src-tauri/assets/packs/README.md` — asset provenance, license, contents, and SHA-256 digest.
- `src-tauri/examples/generate_default_pack.rs` — deterministic developer-only asset generator; never called by production code.
- `src-tauri/examples/pack_smoke.rs` — developer-only install/decode/register/play smoke check.
- `security/pack-policy.test.ts` — fail-closed pack dependency, asset, IPC, capability, and documentation policy gates.

### Modify

- `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock` — approved narrow dependencies and locked graph.
- `src-tauri/src/lib.rs` — register the native Rust `pack` module only; do not alter `run()`.
- `src-tauri/src/audio/sample.rs` — atomic batch registry insertion and test-only accounting accessors.
- `src-tauri/src/audio/mod.rs` — public native batch registration API.
- `security/audio-policy.test.ts` — extend the complete dependency allowlist without weakening existing source/config/IPC/CI checks.
- `README.md` — describe Milestone 3 boundaries, supported pack format, and developer smoke command.
- `docs/architecture/trust-boundaries.md` — make the file-to-pack-manager and pack-manager-to-audio boundaries concrete.
- `docs/security/threat-model.md` — record traversal, archive bomb, malformed WAV, ambiguous path, and partial-install mitigations.

---

### Task 1: Lock the minimal pack dependency surface

**Files:**
- Modify: `security/audio-policy.test.ts`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

**Interfaces:**
- Consumes: existing `APPROVED_DEPENDENCIES`, `cargoMetadata()`, and Cargo source-policy checks.
- Produces: direct dependencies `hound`, `serde_json`, and `zip` with an exact reviewed feature record for all later tasks.

- [ ] **Step 1: Read the governing files and confirm the worktree**

Run:

```bash
git status --short --branch
sed -n '1,220p' AGENTS.md
sed -n '1,460p' docs/superpowers/specs/2026-08-25-sound-pack-format-design.md
sed -n '1,220p' SECURITY.md
sed -n '1,220p' docs/architecture/trust-boundaries.md
sed -n '1,220p' docs/security/threat-model.md
```

Expected: branch is `codex/sound-pack-format`, worktree is clean, and the documented restrictions match Global Constraints.

- [ ] **Step 2: Extend the policy expectation before Cargo.toml**

Add these complete records to `APPROVED_DEPENDENCIES` in name-sorted order:

```ts
{
  name: "hound",
  rename: null,
  source: CRATES_IO_SOURCE,
  req: "^3.5.1",
  kind: null,
  optional: false,
  uses_default_features: true,
  features: [],
  target: null,
  registry: null,
  path: null,
},
{
  name: "serde_json",
  rename: null,
  source: CRATES_IO_SOURCE,
  req: "^1.0",
  kind: null,
  optional: false,
  uses_default_features: true,
  features: [],
  target: null,
  registry: null,
  path: null,
},
{
  name: "zip",
  rename: null,
  source: CRATES_IO_SOURCE,
  req: "^8.6.0",
  kind: null,
  optional: false,
  uses_default_features: false,
  features: ["deflate-flate2-zlib-rs"],
  target: null,
  registry: null,
  path: null,
},
```

Replace index-based fixture mutation with name-based lookup so additions cannot make the test target the wrong crate:

```ts
function dependencyByName(
  dependencies: CargoDependency[],
  name: string,
): CargoDependency {
  const dependency = dependencies.find((candidate) => candidate.name === name);
  if (!dependency) {
    throw new Error(`missing dependency fixture: ${name}`);
  }
  return dependency;
}

const featureChange = approvedDependencyFixture();
dependencyByName(featureChange, "tauri").features.push("devtools");
expect(() => assertCargoDependencyPolicy(featureChange)).toThrow(
  "complete records",
);
```

Add a pure fixture assertion proving ZIP defaults or extra codecs fail:

```ts
it("rejects broadened sound-pack dependency features", () => {
  const defaults = approvedDependencyFixture();
  dependencyByName(defaults, "zip").uses_default_features = true;
  expect(() => assertCargoDependencyPolicy(defaults)).toThrow("complete records");

  const codec = approvedDependencyFixture();
  dependencyByName(codec, "zip").features.push("zstd");
  expect(() => assertCargoDependencyPolicy(codec)).toThrow("complete records");
});
```

- [ ] **Step 3: Run the dependency policy test and verify RED**

Run:

```bash
pnpm test -- security/audio-policy.test.ts
```

Expected: FAIL in `uses exactly the approved complete direct dependency records` because Cargo metadata does not yet contain `hound`, `serde_json`, or `zip`. The new pure fixture test itself must pass.

- [ ] **Step 4: Add only the approved dependencies**

Add to `[dependencies]` in `src-tauri/Cargo.toml`:

```toml
hound = "3.5.1"
serde_json = "1.0"
zip = { version = "8.6.0", default-features = false, features = ["deflate-flate2-zlib-rs"] }
```

Keep the existing `serde = { version = "1.0", features = ["derive"] }`. Do not enable the broad `zip` feature named `deflate`, because it also enables the unnecessary Zopfli writer.

- [ ] **Step 5: Resolve and inspect the lockfile**

Run:

```bash
cargo check --manifest-path src-tauri/Cargo.toml --all-targets
cargo metadata --locked --manifest-path src-tauri/Cargo.toml --format-version 1 > /tmp/keyforge-pack-metadata.json
cargo tree --locked --manifest-path src-tauri/Cargo.toml -e features -i zip
cargo tree --locked --manifest-path src-tauri/Cargo.toml -e features -i hound
```

Expected: Cargo resolves stable `zip` 8.6.x compatible with Rust 1.88, `hound` 3.5.1, and `serde_json` 1.x. ZIP shows only the selected Deflate/flate2 zlib-rs path plus necessary internal features; no AES, Bzip2, Deflate64, LZMA, PPMd, time, XZ, Zstandard, networking, telemetry, or logging framework is enabled.

- [ ] **Step 6: Run GREEN verification**

Run:

```bash
pnpm test -- security/audio-policy.test.ts
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
git diff --check
```

Expected: policy test passes, Rust verification passes with 50 existing tests, and the diff is whitespace-clean.

- [ ] **Step 7: Commit**

```bash
git add security/audio-policy.test.ts src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build: lock sound-pack dependencies"
```

---

### Task 2: Implement the strict versioned manifest

**Files:**
- Create: `src-tauri/src/pack/mod.rs`
- Create: `src-tauri/src/pack/manifest.rs`
- Create: `src-tauri/src/pack/test_support.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `serde::Deserialize`, `serde_json`.
- Produces: `PackId`, `PackVersion`, `CanonicalSoundPath`, `SoundMap<T>`, `ValidatedManifest`, `PackManifestError`, and `parse_manifest(&[u8])`.

- [ ] **Step 1: Register an empty native module and write focused manifest tests**

In `src-tauri/src/lib.rs`, add only:

```rust
pub mod audio;
pub mod pack;
mod commands;
```

Create `src-tauri/src/pack/mod.rs`:

```rust
mod manifest;

#[cfg(test)]
mod test_support;

pub use manifest::{
    CanonicalSoundPath, PackId, PackManifestError, PackVersion, SoundMap,
    ValidatedManifest,
};
```

Create test helper `valid_manifest_json()` in `test_support.rs` returning this exact JSON as bytes:

```rust
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
```

In `manifest.rs`, add tests named:

```rust
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
    let unknown = String::from_utf8(valid_manifest_json())
        .unwrap()
        .replace("\"schema_version\": 1,", "\"schema_version\": 1, \"script\": \"run.sh\",");
    assert_eq!(parse_manifest(unknown.as_bytes()), Err(PackManifestError::Json));
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
```

Add table-driven cases for schema versions `0`, `2`, `-1`, and `1.0`; every absent or empty group; 17 entries in one group; 65 total entries; IDs of 1, 64, and 65 bytes; names of 1, 80, and 81 scalar values; and every reserved basename from the spec.

- [ ] **Step 2: Run manifest tests and verify RED**

Run:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::manifest::tests -- --nocapture
```

Expected: compilation FAILS because the manifest types and functions do not exist. This is the required failing-test observation.

- [ ] **Step 3: Implement the manifest types and validation**

Use these public shapes in `manifest.rs`:

```rust
use std::{collections::HashSet, fmt};

use serde::{Deserialize, Serialize};

pub const MANIFEST_LIMIT_BYTES: usize = 64 * 1024;
pub const MAX_GROUP_VARIANTS: usize = 16;
pub const MAX_PACK_SAMPLES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct PackId(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PackVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct CanonicalSoundPath(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SoundMap<T> {
    normal: Vec<T>,
    space: Vec<T>,
    enter: Vec<T>,
    backspace: Vec<T>,
    modifier: Vec<T>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedManifest {
    id: PackId,
    name: String,
    pack_version: PackVersion,
    sounds: SoundMap<CanonicalSoundPath>,
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
```

Deserialize through private `RawManifest` and `RawSoundMap` structures with `#[serde(deny_unknown_fields)]`. Use `serde_json::from_slice` directly into those structs so duplicate known fields are rejected by the derived struct visitor. Check `bytes.len()` before parsing.

Implement ASCII validation without a regex dependency:

```rust
fn valid_slug(value: &str, allow_underscore: bool) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    if !edge(bytes[0]) || !edge(bytes[bytes.len() - 1]) {
        return false;
    }
    bytes.iter().all(|byte| {
        edge(*byte) || *byte == b'-' || (allow_underscore && *byte == b'_')
    })
}

fn reserved_windows_basename(value: &str) -> bool {
    matches!(value, "con" | "prn" | "aux" | "nul")
        || value
            .strip_prefix("com")
            .or_else(|| value.strip_prefix("lpt"))
            .is_some_and(|suffix| matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
}
```

Parse versions by splitting into exactly three components and requiring `component == "0" || !component.starts_with('0')` before `u32::parse`. Validate names after `trim()` and reject `char::is_control`, U+2028, U+2029, U+202A–U+202E, and U+2066–U+2069.

`CanonicalSoundPath::parse` must split only the exact `sounds/<stem>.wav` shape, require lowercase ASCII, validate the stem with `valid_slug(stem, true)`, and reject reserved basenames. Do not call `Path::components`; archive paths use platform-independent lexical rules.

For `SoundMap<T>`, implement named accessors, `total_len()`, `iter()`, `try_map`, and `into_iter()` in the fixed group order. During conversion from raw strings, enforce each group's 1–16 size, aggregate 64 limit with checked addition, and global uniqueness through `HashSet<CanonicalSoundPath>`.

Implement sanitized `Display` for `PackManifestError` as `invalid sound-pack manifest: <variant>` and `std::error::Error`; never include rejected input.

- [ ] **Step 4: Run manifest tests and verify GREEN**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::manifest::tests -- --nocapture
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
git diff --check
```

Expected: all manifest cases and the full Rust suite pass; clippy emits no warnings.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/pack/mod.rs src-tauri/src/pack/manifest.rs src-tauri/src/pack/test_support.rs
git commit -m "feat: validate sound-pack manifests"
```

---

### Task 3: Decode and canonicalize the narrow WAV contract

**Files:**
- Create: `src-tauri/src/pack/decoder.rs`
- Modify: `src-tauri/src/pack/mod.rs`
- Modify: `src-tauri/src/pack/test_support.rs`

**Interfaces:**
- Consumes: `audio::PcmSample`, `audio::PcmSampleError`, `hound::{WavReader, WavWriter}`.
- Produces: `DecodedAudio`, `PackDecodeError`, `decode_wav(&[u8])`, and deterministic canonical signed-16 PCM WAV bytes.

- [ ] **Step 1: Add deterministic WAV fixture builders and failing tests**

Add `wav_bytes(channels, sample_rate, samples)` to `test_support.rs` using `hound::WavWriter<&mut Cursor<Vec<u8>>>` with `SampleFormat::Int` and 16 bits. Add byte helpers that overwrite little-endian RIFF fields by exact offset for malformed fixtures.

Write tests in `decoder.rs`:

```rust
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
    assert!(!decoded.canonical_wav().windows(4).any(|bytes| bytes == b"JUNK"));
    let second = decode_wav(decoded.canonical_wav()).unwrap();
    assert_eq!(decoded.sample(), second.sample());
}

#[test]
fn rejects_non_pcm_and_non_16_bit_formats() {
    for bytes in [float_wav(), pcm_8_wav(), pcm_24_wav(), extensible_pcm_wav()] {
        assert_eq!(decode_wav(&bytes), Err(PackDecodeError::Format));
    }
}

#[test]
fn rejects_bad_container_alignment_rate_and_duration() {
    assert_eq!(decode_wav(b"not-wave"), Err(PackDecodeError::Container));
    assert_eq!(decode_wav(&wav_bytes(3, 48_000, &[0, 0, 0])), Err(PackDecodeError::Channels));
    assert_eq!(decode_wav(&wav_bytes(1, 7_999, &[0])), Err(PackDecodeError::SampleRate));
    assert_eq!(decode_wav(&wav_bytes(1, 96_001, &[0])), Err(PackDecodeError::SampleRate));
    assert_eq!(decode_wav(&wav_bytes(1, 48_000, &[])), Err(PackDecodeError::Empty));
    assert_eq!(
        decode_wav(&wav_bytes(1, 48_000, &vec![0; 96_001])),
        Err(PackDecodeError::Duration)
    );
}
```

Add mutation cases for wrong `RIFF`, wrong `WAVE`, RIFF size mismatch, missing/duplicate `fmt `, missing/duplicate `data`, `fmt ` length other than 16, format tag other than 1, inconsistent block alignment, inconsistent byte rate, partial sample, partial frame, truncated chunk, trailing bytes outside declared RIFF, and exact two-second boundaries.

- [ ] **Step 2: Run decoder tests and verify RED**

Run:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::decoder::tests -- --nocapture
```

Expected: compilation FAILS because `decoder` and `decode_wav` do not exist.

- [ ] **Step 3: Implement RIFF preflight before calling hound**

Create these shapes:

```rust
use std::io::Cursor;

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
```

Add crate-private `sample(&self) -> &PcmSample`, `into_sample(self) -> PcmSample`, and `canonical_wav(&self) -> &[u8]` accessors. Do not expose encoded bytes outside the pack module.

Implement `preflight_riff(bytes)` manually:

```rust
fn le_u16(bytes: &[u8], offset: usize) -> Result<u16, PackDecodeError> {
    let value = bytes.get(offset..offset + 2).ok_or(PackDecodeError::Container)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn le_u32(bytes: &[u8], offset: usize) -> Result<u32, PackDecodeError> {
    let value = bytes.get(offset..offset + 4).ok_or(PackDecodeError::Container)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}
```

Require `RIFF`, exact `riff_size + 8 == bytes.len()` with checked conversion, and `WAVE`. Walk chunks from byte 12 using checked `8 + chunk_len + padding`. Require exactly one 16-byte `fmt ` chunk and exactly one `data` chunk. From `fmt ` require tag 1, channels 1/2, rate in range, bits 16, `block_align == channels * 2`, and `byte_rate == sample_rate * block_align`, all with checked arithmetic. Require data length non-zero, divisible by 2 and block alignment, and frame count at most `sample_rate * 2`.

This preflight is mandatory because `hound` also accepts WAVE_FORMAT_EXTENSIBLE, which the approved contract rejects.

- [ ] **Step 4: Decode and canonicalize**

Implement:

```rust
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
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PackDecodeError::Sample)?;
    let pcm_values = values
        .iter()
        .map(|value| f32::from(*value) / 32_768.0)
        .collect();
    let sample = PcmSample::new(spec.sample_rate, spec.channels, pcm_values)
        .map_err(map_pcm_error)?;
    let canonical_wav = encode_canonical(spec.channels, spec.sample_rate, &values)?;
    Ok(DecodedAudio { sample, canonical_wav })
}
```

`encode_canonical` writes to `Cursor<Vec<u8>>` through `WavWriter::new(&mut cursor, WavSpec { channels, sample_rate, bits_per_sample: 16, sample_format: SampleFormat::Int })`, writes every original `i16`, explicitly calls `finalize()`, then returns `cursor.into_inner()`. Do not reconstruct `i16` from `f32`; retaining the decoded integer vector makes canonicalization bit-exact.

Map `PcmSampleError` variants explicitly rather than formatting their text. Add sanitized `Display`/`Error` for `PackDecodeError`.

- [ ] **Step 5: Run decoder verification**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::decoder::tests -- --nocapture
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
git diff --check
```

Expected: all decoder boundary/mutation tests pass, clippy has no warnings, and no audio device is opened.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/pack/decoder.rs src-tauri/src/pack/mod.rs src-tauri/src/pack/test_support.rs
git commit -m "feat: validate and canonicalize PCM WAV"
```

---

### Task 4: Inventory ZIP archives and reject unsafe entries

**Files:**
- Create: `src-tauri/src/pack/archive.rs`
- Modify: `src-tauri/src/pack/mod.rs`
- Modify: `src-tauri/src/pack/test_support.rs`

**Interfaces:**
- Consumes: `zip::ZipArchive`, `zip::CompressionMethod`, `CanonicalSoundPath`.
- Produces: `ArchiveInventory`, `ArchiveEntry`, `PackArchiveError`, `inspect_archive`, and `read_entry_bounded`.

- [ ] **Step 1: Add ZIP fixture builders and failing inventory tests**

Add a test helper that writes `manifest.json`, optional `sounds/`, and named byte entries through `zip::ZipWriter<Cursor<Vec<u8>>>`. Its options must set only `CompressionMethod::Stored` or `CompressionMethod::Deflated` and regular-file Unix permissions `0o100644`.

Add archive tests for a valid Stored pack and valid Deflate pack, then table-driven failures:

```rust
#[test]
fn inventories_only_the_canonical_layout() {
    let bytes = zip_with_entries(CompressionMethod::Stored, valid_pack_entries());
    let inventory = inspect_archive(Cursor::new(bytes.clone()), bytes.len() as u64).unwrap();
    assert_eq!(inventory.manifest_index(), 0);
    assert_eq!(inventory.wav_entries().len(), 6);
    assert!(inventory.entry("sounds/normal-01.wav").is_some());
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
    ] {
        let bytes = zip_with_extra_name(name);
        assert_eq!(
            inspect_archive(Cursor::new(bytes.clone()), bytes.len() as u64),
            Err(PackArchiveError::Path)
        );
    }
}

#[test]
fn rejects_links_encryption_and_unsupported_compression() {
    assert_eq!(inspect_archive(symlink_zip(), symlink_zip_len()), Err(PackArchiveError::EntryType));
    assert_eq!(inspect_archive(encrypted_flag_zip(), encrypted_zip_len()), Err(PackArchiveError::Encrypted));
    assert_eq!(inspect_archive(bzip2_method_zip(), bzip2_zip_len()), Err(PackArchiveError::Compression));
}
```

Add exact-boundary tests for source bytes 16 MiB/16 MiB+1, entries 128/129, expanded aggregate 64 MiB/64 MiB+1, per-WAV 8 MiB/8 MiB+1, ratio 100:1/greater, duplicates, case-only collisions, non-ASCII raw names, invalid Unix file-type bits, missing/duplicate manifest, extra directories, archive comments, entry comments, a valid data descriptor, inconsistent descriptor/central sizes, multi-disk markers, and truncated central directories.

- [ ] **Step 2: Run archive tests and verify RED**

Run:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::archive::tests -- --nocapture
```

Expected: compilation FAILS because `archive` and inventory APIs do not exist.

- [ ] **Step 3: Implement raw-name, type, and metadata validation**

Use:

```rust
pub const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_ARCHIVE_ENTRIES: usize = 128;
pub const MAX_EXPANDED_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_WAV_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_COMPRESSION_RATIO: u64 = 100;

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
```

For each `ZipFile` obtained with `ZipArchive::by_index_raw` during inventory:

1. Read `name_raw()` and require non-empty ASCII bytes with no NUL/control.
2. Convert with `str::from_utf8` only after the ASCII check.
3. Accept exactly `manifest.json`, exactly `sounds/`, or a path accepted by `CanonicalSoundPath::parse`.
4. Require `sounds/` to be a directory and every other entry to be a regular file.
5. Inspect `unix_mode() & 0o170000`: accept absent/zero, `0o040000` only for `sounds/`, and `0o100000` only for files; reject all other types.
6. Reject `file.encrypted()`.
7. Accept only `CompressionMethod::STORE` or `CompressionMethod::DEFLATE`.
8. Enforce declared size, aggregate size, and `size <= compressed_size * 100` with checked multiplication. Non-empty expanded data with zero compressed bytes fails ratio validation.
9. Insert the exact ASCII name into both exact and ASCII-lowercase `HashSet`s; any collision fails.

Do not use `mangled_name`, `sanitized_name`, `enclosed_name`, `ZipArchive::extract`, or archive names as filesystem paths.

- [ ] **Step 4: Implement bounded reads**

Keep archive indices rather than open `ZipFile` handles. After inventory has rejected encryption and unsupported methods, reopen by index with `ZipArchive::by_index` and copy through a fixed 32 KiB stack buffer while counting with checked addition:

```rust
fn read_entry_bounded<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry: &ArchiveEntry,
    runtime_limit: u64,
) -> Result<Vec<u8>, PackArchiveError> {
    let mut file = archive.by_index(entry.index).map_err(|_| PackArchiveError::Read)?;
    let capacity = usize::try_from(entry.expanded_size)
        .map_err(|_| PackArchiveError::EntryTooLarge)?;
    let mut output = Vec::with_capacity(capacity);
    let mut buffer = [0_u8; 32 * 1024];
    let mut total = 0_u64;
    loop {
        let read = file.read(&mut buffer).map_err(|_| PackArchiveError::Read)?;
        if read == 0 {
            break;
        }
        total = total.checked_add(read as u64).ok_or(PackArchiveError::TooLarge)?;
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
```

ZIP CRC failures map to `Read`. Archive comments and entry comments remain inert and are never copied into inventory output.

A data descriptor is acceptable only when the central directory provides bounded sizes and the later actual read matches those sizes exactly. Multi-disk metadata is always rejected. Implement sanitized `Display` and `Error` for `PackArchiveError` without including archive names, raw ZIP errors, or source paths.

- [ ] **Step 5: Run archive verification**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::archive::tests -- --nocapture
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
git diff --check
```

Expected: all cross-platform and limit cases pass without filesystem writes.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/pack/archive.rs src-tauri/src/pack/mod.rs src-tauri/src/pack/test_support.rs
git commit -m "feat: reject unsafe sound-pack archives"
```

---

### Task 5: Validate complete archives into decoded packs

**Files:**
- Modify: `src-tauri/src/pack/archive.rs`
- Modify: `src-tauri/src/pack/mod.rs`
- Modify: `src-tauri/src/pack/test_support.rs`

**Interfaces:**
- Consumes: `parse_manifest`, `decode_wav`, `ArchiveInventory`, `SoundMap<CanonicalSoundPath>`.
- Produces: `ValidatedPack`, aggregate expanded/decoded accounting, and `validate_archive<R: Read + Seek>`.

- [ ] **Step 1: Write failing whole-pack tests**

Add:

```rust
#[test]
fn validates_every_reference_and_decodes_every_group() {
    let bytes = valid_pack_zip();
    let pack = validate_archive(Cursor::new(bytes.clone()), bytes.len() as u64).unwrap();
    assert_eq!(pack.manifest().id().as_str(), "keyforge-mechanical");
    assert_eq!(pack.sounds().normal().len(), 2);
    assert_eq!(pack.sounds().total_len(), 6);
    assert!(pack.decoded_bytes() > 0);
}

#[test]
fn rejects_missing_extra_and_duplicate_references() {
    assert_eq!(validate_test_zip(zip_missing_referenced_wav()), Err(PackInstallError::Manifest(PackManifestError::MissingReference)));
    assert_eq!(validate_test_zip(zip_with_unreferenced_wav()), Err(PackInstallError::Manifest(PackManifestError::UnreferencedFile)));
    assert_eq!(validate_test_zip(zip_with_bad_late_wav()), Err(PackInstallError::Decode(PackDecodeError::Container)));
}

#[test]
fn decoded_memory_limit_uses_actual_f32_storage() {
    let bytes = pack_with_decoded_bytes(MAX_DECODED_PACK_BYTES + 4);
    assert_eq!(validate_test_zip(bytes), Err(PackInstallError::Decode(PackDecodeError::DecodedMemory)));
}
```

Extend `PackManifestError` with `MissingReference` and `UnreferencedFile`. Add cases proving no decoded pack is returned if the last WAV fails and aggregate accounting uses checked addition of each `PcmSample::byte_len()`.

- [ ] **Step 2: Run whole-pack tests and verify RED**

Run:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::archive::tests::validates_every_reference_and_decodes_every_group -- --exact --nocapture
```

Expected: compilation FAILS because `ValidatedPack` and `validate_archive` do not exist.

- [ ] **Step 3: Implement complete validation**

Define in `pack/mod.rs`:

```rust
pub const MAX_DECODED_PACK_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ValidatedPack {
    manifest: ValidatedManifest,
    sounds: SoundMap<DecodedAudio>,
    expanded_bytes: u64,
    decoded_bytes: usize,
}

#[derive(Debug)]
pub enum PackInstallError {
    Archive(PackArchiveError),
    Manifest(PackManifestError),
    Decode(PackDecodeError),
    Storage(PackStorageError),
    DuplicateId,
}
```

Implement `From` conversions without embedding input text. In `validate_archive`:

1. Construct `ZipArchive`, then inventory it.
2. Read manifest with limit `MANIFEST_LIMIT_BYTES` and parse it.
3. Build the exact set of referenced canonical paths.
4. Require inventory WAV paths to equal that set; distinguish missing and unreferenced cases.
5. Iterate groups and variants in manifest order. Read each entry with the 8 MiB runtime limit and decode.
6. Add `sample.byte_len()` with `checked_add`; reject above 64 MiB.
7. Return `ValidatedPack` only after every variant succeeds.

Add a generic `SoundMap::try_map` or explicit fixed-order constructor so group identity and variant order cannot be lost through a map keyed by user data.

- [ ] **Step 4: Run complete validation GREEN**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::archive::tests -- --nocapture
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
git diff --check
```

Expected: complete pack validation, reference equality, late failure, and decoded-memory tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/pack/archive.rs src-tauri/src/pack/mod.rs src-tauri/src/pack/manifest.rs src-tauri/src/pack/test_support.rs
git commit -m "feat: validate complete sound packs"
```

---

### Task 6: Install canonical packs through a scoped staging transaction

**Files:**
- Create: `src-tauri/src/pack/storage.rs`
- Modify: `src-tauri/src/pack/mod.rs`
- Modify: `src-tauri/src/pack/manifest.rs`
- Modify: `src-tauri/src/pack/test_support.rs`

**Interfaces:**
- Consumes: `ValidatedPack`, canonical WAV bytes, validated ASCII pack IDs/paths.
- Produces: `PackStorage`, internal `StoredPackMetadata`, internal `StoredDecodedPack`, `PackStorageError`, `install_validated`, `discover`, `load_installed`, and scoped stale-stage cleanup.

- [ ] **Step 1: Write failing managed-root and transaction tests**

Use a `TestRoot` helper whose path is `std::env::temp_dir()/keyforge-pack-test-<pid>-<atomic-counter>`, created with `create_dir` and removed only by its own `Drop` after verifying the fixed prefix.

Add tests:

```rust
#[test]
fn installs_only_canonical_manifest_and_wavs() {
    let root = TestRoot::new();
    let storage = PackStorage::open(root.path().join("packs")).unwrap();
    let installed = storage.install_validated(validated_pack()).unwrap();
    assert_eq!(installed.id().as_str(), "keyforge-mechanical");
    assert!(installed.directory_for_test().join("manifest.json").is_file());
    assert!(!read(installed.directory_for_test().join("sounds/normal-01.wav"))
        .windows(4)
        .any(|bytes| bytes == b"JUNK"));
}

#[test]
fn duplicate_id_never_replaces_existing_files() {
    let root = TestRoot::new();
    let storage = PackStorage::open(root.path().join("packs")).unwrap();
    storage.install_validated(validated_pack()).unwrap();
    let before = snapshot_tree(storage.root_for_test());
    assert_eq!(storage.install_validated(validated_pack()), Err(PackStorageError::DuplicateId));
    assert_eq!(snapshot_tree(storage.root_for_test()), before);
}

#[test]
fn late_failure_removes_only_the_current_stage() {
    let root = TestRoot::new();
    let storage = PackStorage::open(root.path().join("packs")).unwrap();
    let unrelated = storage.root_for_test().join("do-not-delete");
    fs::create_dir(&unrelated).unwrap();
    let error = storage.install_validated_with_fault(validated_pack(), StorageFault::BeforeVerify);
    assert_eq!(error, Err(PackStorageError::InjectedForTest));
    assert!(unrelated.is_dir());
    assert!(stage_directories(storage.root_for_test()).is_empty());
}
```

Add cases for a symlink root, non-directory root, malformed installed directory, stage-name symlink, marker missing, marker mismatch, unknown sibling, cleanup containment, deterministic discovery sort, and installed-pack revalidation.

- [ ] **Step 2: Run storage tests and verify RED**

Run:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::storage::tests -- --nocapture
```

Expected: compilation FAILS because `PackStorage` does not exist.

- [ ] **Step 3: Implement canonical serialization and writes**

Define storage-private transfer types so filesystem code does not depend on Task 8's public manager API:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredPackMetadata {
    pub(crate) id: PackId,
    pub(crate) name: String,
    pub(crate) pack_version: PackVersion,
    pub(crate) variant_counts: [usize; 5],
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoredDecodedPack {
    pub(crate) metadata: StoredPackMetadata,
    pub(crate) sounds: SoundMap<PcmSample>,
}
```

Add `ValidatedManifest::canonical_json()` using a private `Serialize` view and `serde_json::to_vec_pretty`, then append one newline. Reparse the emitted bytes in a test and assert semantic equality.

`PackStorage::open` must create an absent root, inspect it with `symlink_metadata`, reject a symlink or non-directory, canonicalize it, and retain the canonical root. All later destination checks compare against that retained identity. Map I/O failures to stable `PackStorageError` variants whose `Display` strings contain no native path or raw OS message.

`write_validated_pack(stage, pack)` must:

- create `manifest.json`, `sounds/`, and each WAV through `OpenOptions::new().write(true).create_new(true)`;
- use only `PackId::as_str()` and `CanonicalSoundPath::as_str()` after validation;
- write `DecodedAudio::canonical_wav()` rather than source archive bytes;
- call `sync_all()` on each file after successful write;
- never copy archive permissions, timestamps, comments, extra fields, or metadata chunks.

Split each `CanonicalSoundPath` through its validated `file_name()` accessor and write to `stage.join("sounds").join(file_name)`; never pass an archive-owned string directly to a filesystem join.

- [ ] **Step 4: Implement stage ownership and exact cleanup**

Use an RAII guard:

```rust
struct StageGuard {
    root: PathBuf,
    path: PathBuf,
    committed: bool,
}

impl Drop for StageGuard {
    fn drop(&mut self) {
        if !self.committed
            && self.path.parent() == Some(self.root.as_path())
            && self
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(STAGE_PREFIX))
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
```

Create stage directories with `create_dir`, using `.<prefix><pid>-<AtomicU64>` and retrying on `AlreadyExists`. Immediately create a fixed marker file with create-new semantics. Do not accept stage names from callers.

Stale cleanup inspects direct children with `symlink_metadata`, rejects/fails closed on links, and removes only directories with both the exact reserved prefix and exact marker bytes. The production cleanup function returns a count and structured errors; it never deletes unknown entries.

- [ ] **Step 5: Implement verification and same-parent commit**

`load_installed(directory)` must reject a symlink directory, read only canonical `manifest.json` and referenced `sounds/*.wav`, reject extras, decode every WAV again, and return an internal `StoredDecodedPack`. After staging writes, call this loader and compare sanitized metadata and PCM to the in-memory validated pack.

While holding `PackStorage`'s mutation `Mutex<()>`:

1. Recheck final destination absence with `symlink_metadata`.
2. Call `fs::rename(stage, root.join(pack.id().as_str()))`.
3. Mark `StageGuard.committed = true` only after rename succeeds.
4. Return internal `StoredPackMetadata` with sanitized metadata and no public path. Task 8 wraps this value in the public native `InstalledPack` type.

Document in code that M3 serializes one manager process and does not claim no-replace atomicity against a hostile concurrent process.

- [ ] **Step 6: Run storage verification**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::storage::tests -- --nocapture
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
git diff --check
```

Expected: transaction, cleanup-containment, duplicate, discovery, and revalidation tests pass on the current platform; platform-neutral logic has no conditional path weakening.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/pack/storage.rs src-tauri/src/pack/mod.rs src-tauri/src/pack/manifest.rs src-tauri/src/pack/test_support.rs
git commit -m "feat: install validated packs atomically"
```

---

### Task 7: Add atomic audio-registry batch registration

**Files:**
- Modify: `src-tauri/src/audio/sample.rs`
- Modify: `src-tauri/src/audio/mod.rs`

**Interfaces:**
- Consumes: existing `PcmSample`, `SampleRegistry`, `RegisterSampleError`, registry limits, and shutdown state.
- Produces: `SampleRegistry::insert_batch` and `AudioEngineHandle::register_samples(Vec<PcmSample>) -> Result<Vec<SampleId>, RegisterSampleError>`.

- [ ] **Step 1: Write failing registry atomicity tests**

Add to `audio/sample.rs`:

```rust
#[test]
fn batch_insert_assigns_ordered_ids_and_accounts_memory_once() {
    let mut registry = SampleRegistry::default();
    let ids = registry
        .insert_batch(vec![mono(vec![0.1]).unwrap(), mono(vec![0.2, 0.3]).unwrap()])
        .unwrap();
    assert_eq!(ids, vec![SampleId(1), SampleId(2)]);
    assert_eq!(registry.sample_count_for_test(), 2);
    assert_eq!(registry.registered_bytes_for_test(), 12);
}

#[test]
fn failed_batch_leaves_registry_and_identifier_unchanged() {
    let limits = RegistryLimits { max_samples: 2, max_bytes: 8 };
    let mut registry = SampleRegistry::with_limits(limits);
    let before = registry.snapshot_for_test();
    assert_eq!(
        registry.insert_batch(vec![mono(vec![0.1]).unwrap(), mono(vec![0.2, 0.3]).unwrap()]),
        Err(RegisterSampleError::MemoryLimitExceeded)
    );
    assert_eq!(registry.snapshot_for_test(), before);
    assert_eq!(registry.insert(mono(vec![0.4]).unwrap()).unwrap(), SampleId(1));
}

#[test]
fn batch_preflights_identifier_exhaustion_without_partial_insert() {
    let mut registry = SampleRegistry::with_next_id_for_test(u64::MAX);
    assert_eq!(
        registry.insert_batch(vec![mono(vec![0.1]).unwrap(), mono(vec![0.2]).unwrap()]),
        Err(RegisterSampleError::IdentifierExhausted)
    );
    assert_eq!(registry.sample_count_for_test(), 0);
}
```

Add handle tests proving shutdown/poisoned registry failure leaves the registry unchanged and single registration still delegates correctly.

- [ ] **Step 2: Run batch tests and verify RED**

Run:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml audio::sample::tests::batch_insert_assigns_ordered_ids_and_accounts_memory_once -- --exact --nocapture
```

Expected: compilation FAILS because `insert_batch` does not exist.

- [ ] **Step 3: Implement all preflight checks before mutation**

Implement `insert_batch` in this order:

```rust
pub(crate) fn insert_batch(
    &mut self,
    samples: Vec<PcmSample>,
) -> Result<Vec<SampleId>, RegisterSampleError> {
    if samples.is_empty() {
        return Ok(Vec::new());
    }
    let final_count = self
        .samples
        .len()
        .checked_add(samples.len())
        .ok_or(RegisterSampleError::TooManySamples)?;
    if final_count > self.limits.max_samples {
        return Err(RegisterSampleError::TooManySamples);
    }
    let added_bytes = samples.iter().try_fold(0_usize, |total, sample| {
        total
            .checked_add(sample.byte_len())
            .ok_or(RegisterSampleError::MemoryLimitExceeded)
    })?;
    let final_bytes = self
        .registered_bytes
        .checked_add(added_bytes)
        .ok_or(RegisterSampleError::MemoryLimitExceeded)?;
    if final_bytes > self.limits.max_bytes {
        return Err(RegisterSampleError::MemoryLimitExceeded);
    }
    let first = self.next_id.ok_or(RegisterSampleError::IdentifierExhausted)?;
    let id_offset = u64::try_from(samples.len() - 1)
        .map_err(|_| RegisterSampleError::IdentifierExhausted)?;
    let last = first
        .checked_add(id_offset)
        .ok_or(RegisterSampleError::IdentifierExhausted)?;
    let next = last.checked_add(1);
    let ids: Vec<SampleId> = (first..=last).map(SampleId).collect();
    for (id, sample) in ids.iter().copied().zip(samples) {
        self.samples.insert(id, Arc::new(sample));
    }
    self.registered_bytes = final_bytes;
    self.next_id = next;
    Ok(ids)
}
```

Change `insert(sample)` to call `insert_batch(vec![sample])` and return its one ID. Add test-only getters rather than exposing accounting publicly.

- [ ] **Step 4: Add the handle API**

In `AudioEngineHandle`:

```rust
pub fn register_samples(
    &self,
    samples: Vec<PcmSample>,
) -> Result<Vec<SampleId>, RegisterSampleError> {
    if self.shared.shutdown.load(Ordering::Acquire) {
        return Err(RegisterSampleError::RegistryUnavailable);
    }
    self.shared
        .registry
        .lock()
        .map_err(|_| RegisterSampleError::RegistryUnavailable)?
        .insert_batch(samples)
}

pub fn register_sample(&self, sample: PcmSample) -> Result<SampleId, RegisterSampleError> {
    self.register_samples(vec![sample])?
        .into_iter()
        .next()
        .ok_or(RegisterSampleError::IdentifierExhausted)
}
```

Empty batch registration returns an empty vector while the engine is running and `RegistryUnavailable` after shutdown.

- [ ] **Step 5: Run audio verification**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml audio::sample::tests -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml audio::tests -- --nocapture
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
git diff --check
```

Expected: atomicity and prior single-registration behavior pass with no callback changes.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/audio/sample.rs src-tauri/src/audio/mod.rs
git commit -m "feat: register PCM samples atomically"
```

---

### Task 8: Expose the native PackManager orchestration boundary

**Files:**
- Modify: `src-tauri/src/pack/mod.rs`
- Modify: `src-tauri/src/pack/archive.rs`
- Modify: `src-tauri/src/pack/storage.rs`
- Modify: `src-tauri/src/pack/test_support.rs`

**Interfaces:**
- Consumes: `validate_archive`, `PackStorage`, `StoredPackMetadata`, `StoredDecodedPack`, and `AudioEngineHandle::register_samples`.
- Produces: `PackManager`, `SoundGroupCounts`, `InstalledPack`, `DecodedPack`, `RegisteredPack`, and sanitized install/load/register errors.

- [ ] **Step 1: Write failing end-to-end native API tests**

Add tests in `pack/mod.rs`:

```rust
#[test]
fn manager_installs_discovers_decodes_and_registers_without_paths() {
    let root = TestRoot::new();
    let source = root.write_file("import.zip", &valid_pack_zip());
    let manager = PackManager::open(root.path().join("managed")).unwrap();
    let installed = manager.install_zip(&source).unwrap();
    assert_eq!(installed.id().as_str(), "keyforge-mechanical");
    assert_eq!(manager.discover().unwrap(), vec![installed.clone()]);

    let decoded = manager.decode(installed.id()).unwrap();
    let handle = AudioEngineHandle::new_for_test();
    let registered = decoded.register(&handle).unwrap();
    assert_eq!(registered.sounds().normal().len(), 2);
    assert_eq!(registered.sounds().total_len(), 6);
}

#[test]
fn public_errors_are_sanitized() {
    let root = TestRoot::new();
    let source = root.write_file("private-name.zip", b"bad zip");
    let manager = PackManager::open(root.path().join("managed")).unwrap();
    let error = manager.install_zip(&source).unwrap_err();
    let display = error.to_string();
    assert!(!display.contains(root.path().to_string_lossy().as_ref()));
    assert!(!display.contains("private-name.zip"));
    assert_eq!(display, "sound-pack installation failed: invalid archive");
}

#[test]
fn manager_adds_no_tauri_or_frontend_boundary() {
    let source = include_str!("../lib.rs");
    assert!(!source.contains("PackManager::open"));
    assert!(!source.contains("install_zip"));
    assert!(!source.contains("register_samples"));
}
```

Add tests for source file over 16 MiB before ZIP parsing, nonexistent source sanitized as archive I/O, duplicate install unchanged, sorted discovery, corrupted installed pack load failure, and all-or-nothing registration when the audio registry lacks memory.

- [ ] **Step 2: Run manager tests and verify RED**

Run:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::tests -- --nocapture
```

Expected: compilation FAILS because `PackManager` and `DecodedPack::register` do not exist.

- [ ] **Step 3: Implement the public native types**

Use non-serializable types:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundGroupCounts {
    normal: usize,
    space: usize,
    enter: usize,
    backspace: usize,
    modifier: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPack {
    id: PackId,
    name: String,
    pack_version: PackVersion,
    variant_counts: SoundGroupCounts,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedPack {
    metadata: InstalledPack,
    sounds: SoundMap<PcmSample>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredPack {
    metadata: InstalledPack,
    sounds: SoundMap<SampleId>,
}

pub struct PackManager {
    storage: PackStorage,
}
```

Do not derive `Serialize`/`Deserialize` on these public types. Expose getters only for sanitized metadata, counts, and opaque IDs. Expose no `Path`, archive bytes, WAV bytes, PCM slice, or decoder object.

- [ ] **Step 4: Implement orchestration and atomic ID mapping**

`PackManager::install_zip`:

1. Open the native source path.
2. Read file metadata and enforce 16 MiB before `ZipArchive` construction.
3. Call `validate_archive(File, len)`.
4. Call `storage.install_validated(pack)`.

`PackManager::decode` delegates to strict installed-pack loading. `discover` delegates and sorts by exact pack ID.

Convert `StoredPackMetadata.variant_counts` from the fixed `[normal, space, enter, backspace, modifier]` order into `SoundGroupCounts`; do not reconstruct group identity from filenames or a hash map.

`DecodedPack::register` must flatten cloned/moved samples in fixed group order, save the five lengths, call `handle.register_samples` once, then split returned IDs by those lengths. If the returned ID count differs, return `PackRegisterError::RegistryInvariant` without a panic. The only public error text is `sound-pack registration failed: <sanitized variant>`.

- [ ] **Step 5: Run manager GREEN verification**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::tests -- --nocapture
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
git diff --check
```

Expected: native end-to-end tests pass, errors contain no test paths, and `run()` remains byte-for-byte unchanged except the earlier `pub mod pack;` line.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/pack/mod.rs src-tauri/src/pack/archive.rs src-tauri/src/pack/storage.rs src-tauri/src/pack/test_support.rs
git commit -m "feat: add native sound-pack manager"
```

---

### Task 9: Create and validate the bundled mechanical pack

**Files:**
- Create: `src-tauri/examples/generate_default_pack.rs`
- Create: `src-tauri/assets/packs/keyforge-mechanical.zip`
- Create: `src-tauri/assets/packs/README.md`
- Modify: `src-tauri/src/pack/mod.rs`
- Modify: `src-tauri/src/pack/test_support.rs`

**Interfaces:**
- Consumes: production archive validator and storage transaction.
- Produces: `BUNDLED_DEFAULT_PACK`, `PackManager::install_bundled_default`, original data-only default assets, and reproducible generation instructions.

- [ ] **Step 1: Write the bundled-pack test before the asset exists**

Add:

```rust
#[test]
fn bundled_default_uses_the_production_validator() {
    let root = TestRoot::new();
    let manager = PackManager::open(root.path().join("managed")).unwrap();
    let installed = manager.install_bundled_default().unwrap();
    assert_eq!(installed.id().as_str(), "keyforge-mechanical");
    assert!(installed.variant_counts().normal() >= 3);
    assert_eq!(installed.variant_counts().space(), 1);
    assert_eq!(installed.variant_counts().enter(), 1);
    assert_eq!(installed.variant_counts().backspace(), 1);
    assert_eq!(installed.variant_counts().modifier(), 1);
    let decoded = manager.decode(installed.id()).unwrap();
    assert_eq!(decoded.sounds().total_len(), installed.variant_counts().total());
}
```

Reference the future bytes as:

```rust
const BUNDLED_DEFAULT_PACK: &[u8] =
    include_bytes!("../../assets/packs/keyforge-mechanical.zip");
```

- [ ] **Step 2: Run the bundled test and verify RED**

Run:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::tests::bundled_default_uses_the_production_validator -- --exact --nocapture
```

Expected: compilation FAILS because `keyforge-mechanical.zip` does not exist.

- [ ] **Step 3: Implement a deterministic offline generator**

Create `generate_default_pack.rs` with:

- a fixed 48,000 Hz mono 16-bit format;
- 18–45 ms sounds, all below two seconds;
- a small deterministic xorshift32 generator seeded by a fixed constant per variant;
- layered short noise, damped click impulses, and a low-amplitude resonant tail;
- different envelopes for normal, Space, Enter, Backspace, and Modifier;
- explicit peak clamp below 0.9 to prevent harsh clipping;
- three normal variants and one WAV for each special group;
- the exact approved manifest;
- ZIP entries in lexicographic order, fixed Stored compression for deterministic bytes, regular mode `0o100644`, no comments, and no directory metadata beyond optional `sounds/`.

The generator takes exactly one output path argument, refuses extra arguments, creates the output with `create_new`, and never runs from `build.rs` or production startup. Its terminal output contains only the created relative path and SHA-256 must be computed separately with the platform tool documented in the asset README.

Core sample generation uses bounded vectors:

```rust
fn mechanical_sample(seed: u32, frames: usize, body_hz: f32) -> Vec<i16> {
    let mut state = seed;
    (0..frames)
        .map(|index| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let noise = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let time = index as f32 / 48_000.0;
            let attack = if index < 24 { index as f32 / 24.0 } else { 1.0 };
            let decay = (-time * 95.0).exp();
            let body = (std::f32::consts::TAU * body_hz * time).sin() * (-time * 48.0).exp();
            let value = (noise * 0.42 + body * 0.36) * attack * decay;
            (value.clamp(-0.9, 0.9) * i16::MAX as f32).round() as i16
        })
        .collect()
}
```

Use distinct seeds, frame counts, resonance frequencies, and a second delayed impulse for Enter/Space so the result is a short percussive mechanical sound rather than the M2 sine-wave smoke tone.

- [ ] **Step 4: Generate and inspect the archive**

Run:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml --example generate_default_pack -- src-tauri/assets/packs/keyforge-mechanical.zip
unzip -l src-tauri/assets/packs/keyforge-mechanical.zip
shasum -a 256 src-tauri/assets/packs/keyforge-mechanical.zip
```

Expected: exactly one manifest, three normal WAVs, and one WAV for each special group; no script/binary/hidden/extra entry. Record the displayed digest in `src-tauri/assets/packs/README.md` with authorship `KeyForge contributors`, license `CC0-1.0`, deterministic-generation command, and contents.

- [ ] **Step 5: Implement bundled installation through the same validator**

```rust
pub fn install_bundled_default(&self) -> Result<InstalledPack, PackInstallError> {
    let cursor = Cursor::new(BUNDLED_DEFAULT_PACK);
    let pack = validate_archive(cursor, BUNDLED_DEFAULT_PACK.len() as u64)?;
    self.storage.install_validated(pack).map_err(Into::into)
}
```

Do not add a trusted flag, relaxed limits, alternate parser, direct extraction, or direct PCM construction.

- [ ] **Step 6: Run bundled asset GREEN verification**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml pack::tests::bundled_default_uses_the_production_validator -- --exact --nocapture
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
git diff --check
```

Expected: the production validator accepts the committed archive; every Rust target including the generator compiles cleanly.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/examples/generate_default_pack.rs src-tauri/assets/packs/keyforge-mechanical.zip src-tauri/assets/packs/README.md src-tauri/src/pack/mod.rs src-tauri/src/pack/test_support.rs
git commit -m "feat: bundle validated mechanical sound pack"
```

---

### Task 10: Add the native pack smoke example

**Files:**
- Create: `src-tauri/examples/pack_smoke.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: `PackManager`, bundled default installation, `DecodedPack::register`, `AudioEngine`, and opaque sample IDs.
- Produces: a manual developer check that audibly plays bundled data without production Tauri integration.

- [ ] **Step 1: Write a compile-time contract test before the example exists**

Add a small policy test in a temporary new `security/pack-policy.test.ts`:

```ts
import { existsSync, readFileSync } from "node:fs";
import { expect, it } from "vitest";

it("keeps the sound-pack smoke path developer-only", () => {
  expect(existsSync("src-tauri/examples/pack_smoke.rs")).toBe(true);
  const nativeStartup = readFileSync("src-tauri/src/lib.rs", "utf8");
  expect(nativeStartup).not.toContain("PackManager::open");
  expect(nativeStartup).not.toContain("AudioEngine::start");
});
```

- [ ] **Step 2: Run the focused policy test and verify RED**

Run:

```bash
pnpm test -- security/pack-policy.test.ts
```

Expected: FAIL because `src-tauri/examples/pack_smoke.rs` does not exist.

- [ ] **Step 3: Implement the example with exact cleanup scope**

The example must:

1. Build a temp root named `keyforge-pack-smoke-<pid>` under `std::env::temp_dir()`.
2. Refuse to proceed if that exact path already exists.
3. Open `PackManager`, install the bundled pack, decode it, start `AudioEngine`, and atomically register all samples.
4. Wait for `AudioEngineStatus::Ready` for at most five seconds using a 20 ms developer-thread sleep.
5. Play each normal variant once, then Space, Enter, Backspace, and Modifier, with 180 ms between requests.
6. Shut down the engine explicitly.
7. Remove only the exact temp directory after checking its parent and `keyforge-pack-smoke-` prefix.
8. Return an error on every failure; never log paths, device identities, or decoder internals.

The example may print only sanitized progress labels such as `playing normal 1/3` and `pack smoke complete`. It must not be referenced from `src-tauri/src/lib.rs` or frontend files.

- [ ] **Step 4: Document the developer-only command**

Add this prose and command to README:

````markdown
To manually validate the bundled pack import and playback path:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml --example pack_smoke
```

Warning: this developer command plays a short sequence through the default output device. It uses a test-owned temporary pack directory and is not part of production Tauri startup.
````

- [ ] **Step 5: Run automated GREEN verification**

Run:

```bash
pnpm test -- security/pack-policy.test.ts
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
git diff --check
```

Expected: policy and Rust tests pass; compilation proves the example uses only public native APIs.

- [ ] **Step 6: Run the manual audible check**

Run:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml --example pack_smoke
```

Expected: a short mechanical sequence is audible, the process exits 0, and its exact temp directory is gone. If no audio device is available, report the structured unavailable status and do not claim audible verification.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/examples/pack_smoke.rs security/pack-policy.test.ts README.md
git commit -m "test: add native sound-pack smoke path"
```

---

### Task 11: Enforce the final pack security boundary and document it

**Files:**
- Modify: `security/pack-policy.test.ts`
- Modify: `security/audio-policy.test.ts`
- Modify: `README.md`
- Modify: `docs/architecture/trust-boundaries.md`
- Modify: `docs/security/threat-model.md`
- Inspect without changing unless necessary: `.github/workflows/ci.yml`
- Inspect without changing: `src-tauri/capabilities/main.json`
- Inspect without changing: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: the complete M3 implementation and current locked CI/IPC/capability policies.
- Produces: mutation-resistant static policy gates and accurate security documentation.

- [ ] **Step 1: Add failing documentation and asset policy expectations**

Extend `pack-policy.test.ts` with explicit allowlists:

```ts
const EXPECTED_PACK_FILES = [
  "src-tauri/src/pack/archive.rs",
  "src-tauri/src/pack/decoder.rs",
  "src-tauri/src/pack/manifest.rs",
  "src-tauri/src/pack/mod.rs",
  "src-tauri/src/pack/storage.rs",
  "src-tauri/src/pack/test_support.rs",
];

const EXPECTED_ASSETS = [
  "src-tauri/assets/packs/README.md",
  "src-tauri/assets/packs/keyforge-mechanical.zip",
];
```

Add tests that:

- recursively enumerate those roots and require exact reviewed files;
- reject pack-root extensions other than `.rs`, `.md`, and the one exact `.zip`;
- read `Cargo.toml`/locked metadata and require the exact three M3 direct dependency records;
- require ZIP defaults false and exact feature `deflate-flate2-zlib-rs`;
- require `src-tauri/capabilities/main.json` permissions to remain `[]`;
- require the only `generate_handler!` entry to remain `get_app_info`;
- require `lib.rs` not to start `AudioEngine` or `PackManager`;
- require CI to retain locked metadata, fmt, clippy `-D warnings`, and all-target tests on Ubuntu, macOS, and Windows;
- require README/trust-boundary/threat-model phrases describing ZIP limits, traversal rejection, canonical WAV, atomic staging, duplicate rejection, decoded-PCM-only audio, no IPC, and no networking.

Each parser/assertion must have a pure fixture mutation test demonstrating that an added capability, added handler, broadened ZIP feature, hidden asset, or removed CI command throws the intended policy error.

- [ ] **Step 2: Run policy tests and verify RED**

Run:

```bash
pnpm test -- security/pack-policy.test.ts
```

Expected: FAIL on missing M3 documentation phrases. Pure mutation tests pass, proving the guard is not tautological.

- [ ] **Step 3: Update architecture and threat documentation**

In `docs/architecture/trust-boundaries.md`, replace future-tense Boundary 4/5 text with:

- local ZIP path exists only inside native pack-manager calls;
- raw archive names are lexically validated before any filesystem join;
- complete pack validation and decoding happens before staging;
- installed content is canonical JSON and re-encoded signed-16 PCM WAV;
- only `PcmSample` crosses into audio and only opaque `SampleId` leaves it;
- no pack path, archive bytes, PCM, or decoder error crosses IPC.

In `docs/security/threat-model.md`, add concrete mitigations for:

- cross-platform traversal and Windows device paths;
- symlink/special-entry rejection;
- compressed, expanded, entry, duration, and decoded-memory limits;
- strict fixed schema and unknown-field rejection;
- RIFF preflight plus decoder validation;
- validation-before-staging and exact stage cleanup;
- canonical re-encoding and duplicate-ID no-replace policy;
- same-validator bundled assets;
- scoped limitation that hostile same-user filesystem races remain deferred to the production single-instance lifecycle.

Update README to say the repository contains Milestone 3, document the five groups and WAV/ZIP restrictions, and retain the statements that production startup/IPC/UI/input remain absent.

- [ ] **Step 4: Run full security and cross-platform-policy GREEN**

Run:

```bash
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
git diff --check
```

Expected: all frontend/security tests, static export, Rust formatting, clippy, and Rust tests pass. `out/index.html` exists. No CI or capability change is necessary unless a test proves the existing gate is insufficient.

- [ ] **Step 5: Audit the final dependency and boundary diff**

Run:

```bash
cargo tree --locked --manifest-path src-tauri/Cargo.toml -e features
git diff main...HEAD -- src-tauri/Cargo.toml src-tauri/Cargo.lock package.json pnpm-lock.yaml src-tauri/capabilities src-tauri/src/lib.rs .github/workflows/ci.yml
rg -n -i "reqwest|hyper|ureq|telemetry|analytics|sentry|segment|posthog|log::|tracing|Command::new|std::process|libloading|dlopen|LoadLibrary|generate_handler|emit\(" src-tauri/src src-tauri/examples package.json src-tauri/Cargo.toml
```

Expected: only approved archive/JSON/WAV dependencies were added; package files and capabilities are unchanged; `run()` still registers only `get_app_info`; the generator/example contain no process execution; no network, telemetry, logging, or dynamic-loading code appears.

- [ ] **Step 6: Commit**

```bash
git add security/pack-policy.test.ts security/audio-policy.test.ts README.md docs/architecture/trust-boundaries.md docs/security/threat-model.md
git commit -m "docs: codify sound-pack security boundary"
```

---

## Final Verification and Review Gate

- [ ] **Step 1: Confirm the worktree and commit sequence**

Run:

```bash
git status --short --branch
git log --oneline --decorate main..HEAD
git diff --check main...HEAD
```

Expected: clean `codex/sound-pack-format` worktree, one focused commit per task, and no whitespace errors.

- [ ] **Step 2: Run the complete locked verification suite from a clean tree**

Run exactly:

```bash
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

Expected: every command exits 0; Vitest reports all frontend/security tests passing; static export creates `out/index.html`; clippy has no warnings; all Rust library and example tests pass without an audio device.

- [ ] **Step 3: Run manual native verification**

Run:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml --example pack_smoke
pnpm tauri dev
```

Expected: the pack smoke installs, decodes, registers, and audibly plays the original mechanical sequence; its test-owned temporary directory is removed; Tauri launches using loopback development assets; the production window still exposes only sanitized `get_app_info`. Stop `pnpm tauri dev` cleanly after confirming launch.

- [ ] **Step 4: Request an independent final code review**

Review the entire `main...HEAD` diff against:

- `AGENTS.md`;
- `SECURITY.md`;
- the approved M3 design spec;
- `docs/architecture/trust-boundaries.md`;
- `docs/security/threat-model.md`.

Classify findings as Critical, Important, or Minor. Fix all Critical and Important findings with TDD and focused commits, then rerun the complete suite. Review specifically for traversal bypasses, ZIP bombs, size arithmetic overflow, symlink/special-file behavior, partial installs, unsafe cleanup, duplicate replacement, unsupported WAV acceptance, information leakage, dependency feature bloat, IPC/capability changes, networking, and Windows/Linux assumptions.

- [ ] **Step 5: Use the finishing workflow**

Invoke `superpowers:finishing-a-development-branch`. Do not merge, push, delete, or create a pull request without the user's explicit integration choice.
