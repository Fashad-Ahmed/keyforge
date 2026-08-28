# Bundled KeyForge Mechanical Pack

`keyforge-mechanical.zip` is an original, data-only sound pack authored by
KeyForge contributors and dedicated to the public domain under CC0-1.0.

The WAV files are synthesized offline by
`src-tauri/examples/generate_default_pack.rs`. The deterministic generator
uses fixed xorshift32 seeds to layer short noise bursts, damped click
impulses, and low-amplitude resonant tails. It produces mono signed 16-bit PCM
at 48,000 Hz, clamps every sample below 0.9 full scale, and performs no
network access. It is a developer example only: neither `build.rs` nor
production startup calls it.

## Identity and contents

- Manifest ID: `keyforge-mechanical`
- Display name: `KeyForge Mechanical`
- Pack version: `1.0.0`
- `manifest.json`
- `sounds/backspace-01.wav`
- `sounds/enter-01.wav`
- `sounds/modifier-01.wav`
- `sounds/normal-01.wav`
- `sounds/normal-02.wav`
- `sounds/normal-03.wav`
- `sounds/space-01.wav`

The ZIP stores those eight entries in the listed lexicographic order with
Stored compression, the fixed ZIP epoch timestamp, regular-file mode
`100644`, and no archive or entry comments. It contains no directory entry,
script, executable, library, macro, command, hidden metadata, or unreferenced
file.

## Deterministic reproduction

From the repository root, choose a relative output path that does not exist:

```sh
cargo run --locked --manifest-path src-tauri/Cargo.toml \
  --example generate_default_pack -- \
  src-tauri/target/keyforge-mechanical.repro.zip
cmp src-tauri/assets/packs/keyforge-mechanical.zip \
  src-tauri/target/keyforge-mechanical.repro.zip
shasum -a 256 src-tauri/assets/packs/keyforge-mechanical.zip
```

The generator accepts exactly one relative output path and uses create-new
semantics, so it never replaces an existing file. Its successful stdout is
only that relative path. The expected SHA-256 digest is:

```text
05d2816c9f3d0dcadfeb03a55a2ce98a2a4c8992541fb19529212e868a47dadd
```
