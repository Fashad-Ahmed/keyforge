# KeyForge Milestone 3 — Secure Sound-Pack Format Design

**Date:** 2026-08-25
**Status:** Approved design
**Scope:** Native ZIP import, strict manifest validation, WAV decoding, managed installation, and one bundled default pack

## Goal

Build a production-quality Rust sound-pack subsystem that treats every pack as untrusted data, validates it under explicit resource limits, decodes only a narrow WAV subset, and passes only validated `PcmSample` values into the existing audio registry.

Milestone 3 establishes the complete file-to-PCM trust boundary before keyboard input, product UI, or production audio startup exists. It adds no application networking, telemetry, Tauri command, Tauri event, or Tauri capability.

## Decisions

- Accept local sound packs as ZIP archives.
- Validate while staging, then install with a same-filesystem atomic rename.
- Support WAV only, restricted to uncompressed signed 16-bit PCM.
- Use five fixed sound groups: `normal`, `space`, `enter`, `backspace`, and `modifier`.
- Require at least one WAV in every sound group.
- Treat variants within a group as an ordered set for later uniform random selection.
- Reject an import when its pack ID is already installed.
- Bundle one original compact mechanical pack with variants for all five groups.
- Subject the bundled pack to the same archive, manifest, decoder, and PCM validation as imported packs.
- Keep all paths, archive bytes, decoder details, and PCM data native.

## Non-Goals

Milestone 3 does not include:

- keyboard hooks, native input events, or the future `SoundEvent` model;
- runtime event-to-sound selection or random-number generation;
- production construction of `AudioEngine` or `PackManager` during Tauri startup;
- frontend pack lists, previews, selection, import controls, or file dialogs;
- Tauri commands, events, permissions, or capability changes;
- persistent selected-pack settings;
- replacing, updating, deleting, exporting, or downloading packs;
- FLAC, Ogg Vorbis, MP3, AAC, compressed WAV codecs, or streaming audio;
- pack signatures, a remote registry, marketplace, community features, or networking;
- executable scripts, binaries, dynamic libraries, macros, commands, plugins, or post-install actions;
- autostart, updater, release signing, or packaging changes.

M4 owns the sanitized input domain. M6 owns product UI, pack selection, file-dialog integration, persistent settings, and the production lifecycle of the audio and pack subsystems.

## Security Model

Sound-pack archives are fully attacker-controlled. Neither a familiar filename nor a bundled origin grants trust. The subsystem must remain safe when ZIP headers lie, paths use another platform's syntax, compressed data expands unexpectedly, JSON is adversarial, WAV chunks are malformed, or an import fails at any point.

The subsystem protects:

- the integrity of files outside KeyForge's managed pack root;
- bounded CPU, memory, disk, entry-count, and decoded-audio use;
- the data-only sound-pack invariant;
- deterministic pack identity across macOS, Windows, and Linux;
- the existing audio engine's decoded-PCM boundary;
- privacy by keeping pack internals native and offline.

The managed pack root is supplied by trusted native application code. The manager never accepts a destination directory from a pack manifest or the frontend.

## Dependency Choice

The implementation may add only the narrow direct dependencies needed for this boundary:

- `serde_json` for strict JSON parsing; `serde` is already a direct dependency;
- `zip` with default features disabled and only Stored/Deflate archive support;
- `hound` for RIFF/WAVE parsing and uncompressed PCM decoding.

Exact compatible patch versions will be locked in `src-tauri/Cargo.lock` during implementation. Features that enable additional compression algorithms, encryption, time handling, networking, logging, or unrelated media codecs must remain disabled.

No general media framework, async runtime, database, scripting engine, dynamic loader, network client, telemetry SDK, or application logging framework is justified for this milestone.

## Module Architecture

The subsystem lives under `src-tauri/src/pack/`:

```text
pack/
├── mod.rs       Public native API, types, limits, and orchestration
├── manifest.rs  Strict schema parsing and semantic validation
├── archive.rs   ZIP inventory, path rules, bounded reads, and staging
├── decoder.rs   WAV validation and conversion into PcmSample
└── storage.rs   Managed-root checks, staged commit, and stale-stage cleanup
```

The bundled archive and its provenance live under:

```text
src-tauri/assets/packs/keyforge-mechanical.zip
src-tauri/assets/packs/README.md
```

An optional deterministic offline asset generator may live under `tools/`. It is a developer tool, is never packaged as pack content, is never invoked by the application, and must have no production dependency path. The committed pack itself contains JSON and WAV data only.

### `PackManager`

`PackManager` owns the trusted managed-root path and serializes mutating operations within the process. Its native-only API is conceptually:

```rust
pub fn open(root: PathBuf) -> Result<Self, PackStorageError>;
pub fn install_zip(&self, source: &Path) -> Result<InstalledPack, PackInstallError>;
pub fn install_bundled_default(&self) -> Result<InstalledPack, PackInstallError>;
pub fn discover(&self) -> Result<Vec<InstalledPack>, PackStorageError>;
pub fn decode(&self, id: &PackId) -> Result<DecodedPack, PackLoadError>;
```

These signatures are illustrative; the implementation plan may refine ownership without broadening the boundary. The manager does not expose ZIP readers, decoder objects, absolute storage paths, or unvalidated manifest structures.

`InstalledPack` contains sanitized metadata only: opaque pack ID, display name, pack version, and per-group variant counts. `DecodedPack` owns validated metadata plus `PcmSample` values arranged by semantic group. Neither type is serializable for Tauri IPC in M3.

### Audio registry integration

The existing audio registry receives only `PcmSample`. M3 adds a native batch-registration operation so a decoded pack is registered atomically: either every sample receives an opaque `SampleId`, or the registry remains unchanged. Existing count, byte, identifier, and stopped-state limits still apply independently.

The result maps the five semantic groups to opaque `SampleId` values. It contains no filenames or native paths. No production caller registers a pack in M3; deterministic integration tests exercise the boundary. Pack switching and reclamation remain deferred until their lifecycle is designed.

## Manifest Format

Every archive contains exactly one root-level `manifest.json` using schema version `1`:

```json
{
  "schema_version": 1,
  "id": "keyforge-mechanical",
  "name": "KeyForge Mechanical",
  "pack_version": "1.0.0",
  "sounds": {
    "normal": [
      "sounds/normal-01.wav",
      "sounds/normal-02.wav"
    ],
    "space": ["sounds/space-01.wav"],
    "enter": ["sounds/enter-01.wav"],
    "backspace": ["sounds/backspace-01.wav"],
    "modifier": ["sounds/modifier-01.wav"]
  }
}
```

Serde structures use `deny_unknown_fields` at every object level. Unknown fields, duplicate JSON keys, missing fields, `null`, and type coercion are rejected. Because common JSON deserializers may otherwise accept the last duplicate key, duplicate-key rejection must occur during parsing rather than through a post-deserialization check.

### Identity and display rules

- `schema_version` must equal integer `1`.
- `id` is 1–64 ASCII bytes matching `[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?`.
- `id` must not equal a Windows reserved device basename: `con`, `prn`, `aux`, `nul`, `com1`–`com9`, or `lpt1`–`lpt9`.
- `name` is 1–80 Unicode scalar values after trimming leading/trailing whitespace.
- `name` rejects NUL, control characters, line/paragraph separators, and bidi override/isolate controls.
- `pack_version` is canonical `MAJOR.MINOR.PATCH`; each component is an unsigned 32-bit integer with no leading zero unless it is exactly `0`.
- Identity comparisons use the exact validated ASCII ID and never locale-dependent case conversion.

The pack version is descriptive in M3. It does not authorize replacement or imply update behavior.

### Sound-map rules

- `sounds` contains exactly `normal`, `space`, `enter`, `backspace`, and `modifier`.
- Every group contains 1–16 entries.
- A manifest contains no more than 64 total WAV references.
- Every reference is unique across the entire manifest.
- Array order is retained for deterministic inspection and testing.
- Weighting, conditions, aliases, globs, fallback inheritance, and arbitrary event names are forbidden.

## Canonical Archive Layout

Accepted regular-file entries are limited to:

```text
manifest.json
sounds/<filename>.wav
```

An explicit `sounds/` directory entry may be present. No other directory entry is accepted. Metadata folders such as `__MACOSX`, thumbnails, resource forks, hidden files, and unreferenced WAV files are rejected rather than ignored.

`<filename>` is 1–64 ASCII bytes before `.wav` and matches `[a-z0-9](?:[a-z0-9_-]{0,62}[a-z0-9])?`. The extension is exactly lowercase `.wav`. These rules make valid archive paths identical on case-sensitive and case-insensitive filesystems.

The filename stem must not equal a Windows reserved device basename (`con`, `prn`, `aux`, `nul`, `com1`–`com9`, or `lpt1`–`lpt9`).

Path validation operates on the original archive name before any filesystem join. It rejects:

- non-UTF-8 names;
- backslashes, NUL bytes, controls, and empty components;
- `.` and `..` components;
- leading slash, repeated slash, trailing slash outside the one allowed directory entry, and platform root prefixes;
- Windows drive prefixes, UNC forms, NT device paths, and alternate-data-stream colons;
- Unicode lookalikes because accepted paths are ASCII only;
- duplicate exact paths and paths that collide under ASCII case-folding;
- symbolic links, hard links, device nodes, FIFOs, sockets, and any entry not known to be a regular file or the allowed directory;
- encrypted entries, multi-disk archives, and unsupported compression methods.

Only Stored and Deflate compression are accepted. Archive and entry comments are either empty or ignored as inert metadata; they are never copied into installed state or treated as instructions.

A data descriptor is accepted only when the central directory supplies internally consistent bounded sizes. Actual streamed bytes remain authoritative and cannot exceed either the declared size or runtime ceilings.

## Resource Limits

All limits are checked with overflow-safe arithmetic and enforced against actual bytes read, not only attacker-controlled ZIP metadata.

| Resource | Limit |
| --- | ---: |
| Source ZIP file | 16 MiB |
| Central-directory entries, including `sounds/` | 128 |
| Manifest bytes | 64 KiB |
| Regular WAV files | 64 |
| Expanded bytes per WAV entry | 8 MiB |
| Expanded bytes for the complete archive | 64 MiB |
| Compression ratio per entry | 100:1 |
| Decoded duration per WAV | 2 seconds |
| Decoded PCM for one pack | 64 MiB |
| Channels | 1 or 2 |
| Sample rate | 8,000–96,000 Hz |
| Bits per sample | exactly 16 |

Zero-length files are rejected. Declared sizes that exceed limits fail before extraction; reads that exceed their declared sizes or a runtime ceiling fail immediately. Integer addition, multiplication, frame counts, byte counts, and duration calculations use checked arithmetic.

The audio registry applies its own 512-sample, 128 MiB, 8–192 kHz, mono/stereo, finite-value, amplitude, and ten-second limits after decoding. The stricter pack limits remain independent defense in depth.

## WAV Contract

The decoder accepts only RIFF/WAVE files with:

- a well-formed RIFF size and chunk layout;
- WAVE format;
- uncompressed integer PCM format tag `1`;
- exactly 16 bits per sample;
- one or two channels;
- a sample rate from 8,000 through 96,000 Hz;
- consistent block alignment and byte rate;
- at least one complete frame and at most two seconds of frames;
- exactly one usable format description and one bounded data stream;
- no truncated sample, partial frame, or trailing structure that contradicts declared sizes.

Samples convert deterministically to interleaved `f32` in `-1.0..=1.0` and then pass through `PcmSample::new`. Compressed WAV, extensible formats, IEEE float, unsigned 8-bit PCM, 24/32-bit PCM, more than two channels, and non-audio RIFF forms are rejected.

Metadata chunks may be skipped only through the bounded decoder. They are never retained or exposed. The decoder must not allocate based solely on an untrusted declared length.

## Validation and Installation Flow

Import follows one fail-closed transaction:

1. Open the source path read-only and reject a source larger than 16 MiB.
2. Parse ZIP structure without writing files.
3. Inventory all entries, validate original names and file types, enforce count and declared-size limits, and reject duplicates.
4. Read `manifest.json` through a 64 KiB limiting reader and strictly validate syntax and semantics.
5. Cross-check that every manifest reference exists once and every regular archive file is referenced or is the manifest.
6. Stream each WAV through per-entry and aggregate byte counters, decode it, construct `PcmSample`, and enforce aggregate decoded-memory limits.
7. Only after the complete archive and all decoded samples are valid, create a unique staging directory inside the managed pack root.
8. Write a canonical manifest and deterministically re-encode the validated samples as canonical signed 16-bit PCM WAV files with create-new semantics. No archive-provided metadata chunks, permissions, timestamps, comments, or unused bytes are installed.
9. Close writes, reopen staged files through the installed-pack loader, and verify the same manifest and WAV contract.
10. While holding the manager's mutation guard, recheck that the final pack ID is absent and rename the staging directory to `<managed-root>/<pack-id>` on the same filesystem.
11. Return sanitized `InstalledPack` metadata.

Validation occurs before staging so invalid packs do not touch managed storage. A failure after staging begins removes only the exact staging directory created by the current operation. A failure never removes or modifies an installed pack.

M3 supports one mutating manager process at a time. Cross-process installation and single-instance application policy are deferred with the production lifecycle. The final duplicate check is still mandatory, but M3 does not claim no-replace atomicity against a hostile concurrent process.

## Managed Storage

The manager receives a trusted application-data pack root from native code. Opening it:

- creates the root when absent;
- rejects a non-directory root;
- rejects a root that is itself a symbolic link;
- canonicalizes and retains the root identity;
- creates staging directories directly under that root using a fixed `.keyforge-stage-` prefix plus process-local collision-safe identifiers;
- creates files and directories with application-owned defaults rather than archive permissions.

The attacker model for M3 is malicious pack input, not a hostile same-user process racing filesystem operations or replacing the managed root after it is opened. Cross-process hardening belongs with the later single-instance production lifecycle and must not be inferred from this design.

Every filesystem destination is produced from already validated ASCII components. No raw archive path is passed to `join`, `create_dir`, `File::create`, or `rename`.

Stale-stage cleanup considers only direct child directories with the exact reserved prefix and a KeyForge marker created with the staging directory. It verifies type and containment without following links. Unknown files, links, malformed stage names, installed pack directories, and paths outside the canonical root are untouched.

Discovery considers only direct child directories whose names are valid pack IDs. Each discovered directory is loaded through the strict installed-pack validator. Invalid directories produce structured native errors; they are not silently trusted or deleted.

## Bundled Default Pack

`keyforge-mechanical.zip` is compiled into the native binary as immutable bytes. Installation passes those bytes through the same archive inventory, manifest, decoder, staging, installed-pack verification, and duplicate policy used for a local ZIP.

The pack contains original short mechanical sounds for:

- multiple `normal` variants;
- Space;
- Enter;
- Backspace;
- Modifier.

The asset README records authorship, generation or recording method, license, manifest identity, and a SHA-256 digest for reviewer verification. The checksum documents provenance; it is not a signature or remote-update mechanism.

The pack archive contains no generator, script, binary, license executable, hidden metadata, or unreferenced file. CI validates the committed archive through the production parser.

## Errors and Privacy

Errors are typed and grouped by boundary:

- `PackArchiveError`: structure, path, entry type, compression, encryption, or archive limits;
- `PackManifestError`: JSON syntax, duplicate keys, schema, identity, sound map, or references;
- `PackDecodeError`: WAV container, format, frame, duration, or decoded-memory failures;
- `PackStorageError`: managed-root, staging, copy, verification, cleanup, or commit failures;
- `PackInstallError`: composes archive, manifest, decode, storage, and duplicate-ID outcomes;
- `PackLoadError`: installed-pack validation and decoded-loading outcomes.

Display strings are stable sanitized categories. They do not include absolute paths, archive entry contents, raw decoder diagnostics, file bytes, OS usernames, or managed-root locations. Native test-only access may inspect structured variants and sources.

No error is logged, transmitted, persisted as telemetry, or emitted over Tauri IPC. A future UI layer may map selected variants to generic user-facing messages without receiving native paths or decoder internals.

## Failure and Recovery Semantics

- All validation is fail closed.
- No individual file or sample is accepted independently of the complete pack.
- Validation failure creates no staging directory.
- Staging or verification failure removes only the current stage.
- Commit failure leaves existing installed packs unchanged.
- Duplicate IDs are rejected; M3 never replaces data.
- Installed corruption is reported and never auto-repaired from an untrusted import.
- Bundled-pack failure is reported like any other pack failure; it does not bypass checks.
- Panics are bugs, not validation responses. Malformed input must return errors.

## Cross-Platform Behavior

Validation uses platform-independent lexical rules before filesystem access. A pack accepted on one supported platform must have the same identity, inventory, manifest, and PCM result on macOS, Windows, and Linux.

Tests explicitly cover:

- `/absolute`, `../relative`, and repeated-separator paths;
- `C:\\`, `C:relative`, UNC, NT device, backslash, and colon forms;
- case-only duplicates and ASCII case-folding collisions;
- link and special-file Unix mode bits;
- reserved staging prefixes and filesystem type mismatches;
- rename behavior and cleanup containment using same-parent temporary directories.

Valid filenames avoid Windows reserved characters, trailing dots/spaces, case ambiguity, and Unicode normalization differences by construction.

## Test Strategy

All behavior changes follow red-green-refactor TDD. Every production behavior begins with a focused failing test, and the expected failure is observed before implementation. Tests use temporary directories under the test process's designated temporary root and never touch a real user application-data directory.

### Manifest tests

- minimal and multi-variant valid manifests;
- malformed JSON, duplicate keys, unknown fields, missing fields, and wrong types;
- unsupported schema versions and non-canonical pack versions;
- invalid IDs, empty or misleading names, control/bidi characters;
- missing, empty, oversized, duplicate, and unknown sound groups;
- duplicate references and invalid canonical paths.

### Archive tests

- valid Stored and Deflate archives;
- exact entry, file-count, compressed, expanded, per-entry, and ratio boundaries;
- actual reads exceeding declared or runtime limits;
- traversal attempts using Unix, Windows, mixed separators, drives, UNC, and device forms;
- duplicate, case-colliding, non-UTF-8, absolute, empty-component, and hidden paths;
- symlink, hard-link, device, FIFO, socket, encrypted, unsupported-method, truncated, and multi-disk inputs;
- extra files, extra directories, metadata folders, and unreferenced WAV files.

### Decoder tests

- valid mono/stereo 16-bit PCM at minimum and maximum rates and durations;
- deterministic conversion of minimum, zero, and maximum integer samples;
- wrong RIFF/WAVE tags, malformed chunk sizes, inconsistent alignment/rates, missing or duplicate required chunks;
- compressed, extensible, floating-point, 8/24/32-bit, zero-channel, and multichannel formats;
- empty, partial-frame, truncated, trailing-inconsistent, oversized, and over-duration data;
- aggregate decoded-memory enforcement with checked arithmetic.

### Storage and integration tests

- invalid imports leave no stage or installed directory;
- late write and verification failures clean only their own stage;
- duplicate IDs never replace installed files;
- stale-stage cleanup cannot follow links or leave the managed root;
- discovery revalidates installed packs;
- bundled archive passes the production validator;
- decoded pack batch registration is all-or-nothing and produces only opaque IDs;
- no test requires an actual audio device.

### Security policy and CI tests

- dependency allowlist and exact feature assertions for `zip` and `hound`;
- Cargo lockfile sources remain the reviewed registry with no Git/path/patch replacement;
- no network, telemetry, analytics, logging-framework, scripting, or dynamic-loading dependency is introduced;
- Tauri commands and capabilities remain unchanged;
- no frontend dependency or production frontend behavior changes;
- pack assets contain only the canonical manifest and referenced WAV files;
- Linux, Windows, and macOS CI run Rust formatting, clippy with warnings denied, and all tests.

Mutation-style fixture tests must change one property at a time so a passing rejection test proves the intended guard rather than failing earlier for an unrelated reason.

## Verification

The completed milestone must pass:

```bash
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

Manual verification will import and audibly play the bundled mechanical pack through a developer-only native example. The production Tauri app remains free of pack and audio startup behavior in M3.

## Completion Criteria

Milestone 3 is complete only when:

- valid ZIP packs install into managed storage and decode deterministically;
- traversal, special files, ambiguous paths, unsupported content, and resource exhaustion attempts are rejected;
- manifests use the strict fixed five-group schema;
- WAV decoding accepts only the documented PCM subset;
- failed imports leave no partial installed state;
- duplicate pack IDs do not replace existing data;
- the bundled original mechanical pack passes the same validator;
- only validated `PcmSample` values cross into the audio registry;
- batch registration is atomic;
- no raw paths, encoded bytes, decoded PCM, or decoder errors cross Tauri IPC;
- no Tauri command, event, permission, or capability is added;
- no networking, telemetry, analytics, account, or executable-pack behavior is added;
- frontend tests/build and Rust format/clippy/tests pass on the locked dependency graph;
- CI verifies the boundary on macOS, Windows, and Linux.
