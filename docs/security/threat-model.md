# Threat Model

## Protected Assets

- user keystroke privacy
- integrity of local sound packs
- integrity of settings
- integrity of application binaries
- integrity of the release pipeline

## Threats

1. accidental key logging
2. malicious code disguised as a sound pack
3. archive path traversal
4. malformed audio causing crashes or resource exhaustion
5. compromised Rust or npm dependency
6. excessive Tauri permissions
7. compromised CI workflow
8. tampered release artifact
9. malicious contributor change
10. future updater or registry compromise

## Implemented Mitigations

- raw keyboard events remain in Rust
- frontend receives no raw key data
- V1 has no application networking
- packs are data-only: strict declarative JSON plus validated WAV audio
- explicit Tauri capabilities
- locked dependencies
- protected review workflow
- automated tests and static analysis
- bounded decoded-PCM registry memory
- bounded playback commands and 32 mixer voices
- allocation-free audio callback work
- private Rust-side output-device recovery without IPC exposure
- a 16 MiB compressed archive limit, 128-entry ceiling, 64 MiB expanded ceiling, 8 MiB per-WAV ceiling, and 100:1 compression-ratio limit
- cross-platform traversal rejection, including absolute paths, drive/UNC forms, Windows device basenames, case collisions, links, and special entries
- a fixed manifest schema with unknown-field, duplicate-key, duplicate-reference, missing-reference, and unreferenced-file rejection
- manual RIFF preflight followed by decoder validation for mono/stereo signed-16 PCM WAV, 8–96 kHz, at most two seconds
- a 64 MiB decoded-memory ceiling computed from actual `f32` PCM storage
- complete validation before same-parent staging, marker-owned exact cleanup, and revalidation before commit
- canonical JSON and canonical signed-16 PCM WAV re-encoding; archive permissions, comments, timestamps, and metadata are not copied
- duplicate pack IDs use a no-replace policy within the serialized manager process
- the bundled pack enters through the same validator and storage transaction as imported packs
- the audio registry receives decoded PCM only and returns opaque sample IDs
- no sound-pack IPC and no application networking

The current storage transaction serializes one manager process. Hostile same-user filesystem races across separate processes are outside this milestone and remain deferred to the production single-instance lifecycle. Later milestones must add dependency review automation, SBOM, provenance, artifact scanning, and signing.
