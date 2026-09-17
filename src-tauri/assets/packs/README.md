# Bundled KeyForge Mechanical Packs

The four `keyforge-*.zip` archives are original, data-only sound packs authored by
KeyForge contributors and dedicated to the public domain under CC0-1.0.

The WAV files are synthesized offline by
`src-tauri/examples/generate_default_pack.rs`. The deterministic generator
uses fixed xorshift32 seeds, integer Q15 attack/decay/release envelopes, and
integer high-pass, band-pass, and low-pass filters to layer a sharp stem
contact, bottom-out thock, short case resonance, and stabilizer chatter. The
sound is transient rather than pitched: the generator contains no tonal
oscillator. It uses no
floating-point or platform `libm` operation, produces mono signed 16-bit PCM at
48,000 Hz, clamps every sample below 0.9 full scale, ends every WAV at exact
zero, and performs no network access. It is a developer example only: neither
`build.rs` nor production startup calls it.

## Identities and contents

- `keyforge-mechanical` — Classic Mechanical, version `1.1.0`
- `keyforge-deep-thock` — Deep Thock, version `1.0.0`
- `keyforge-crisp-click` — Crisp Click, version `1.0.0`
- `keyforge-soft-linear` — Soft Linear, version `1.0.0`

Each archive contains:
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
  --example generate_default_pack -- keyforge-mechanical \
  src-tauri/target/keyforge-mechanical.repro.zip
cmp src-tauri/assets/packs/keyforge-mechanical.zip \
  src-tauri/target/keyforge-mechanical.repro.zip
shasum -a 256 src-tauri/assets/packs/keyforge-mechanical.zip
```

The generator accepts one known profile ID and one relative output path, and uses create-new
semantics, so it never replaces an existing file. Its successful stdout is
only that relative path. The expected SHA-256 digests are:

```text
2bd991b0c9c9ad82e5821a7e4aa9576c35517ac733b59c271ebe5a9ddc0854bd  keyforge-crisp-click.zip
c801bbf63c34338b6edffa0d092b3e13ce4fc8b69d68ead61661559553aa056a  keyforge-deep-thock.zip
687f25f51fe0e454e1b53aaa53045be2493de8483d6c0e2664a3589ec5f304dd  keyforge-mechanical.zip
7431c8385d684d5dfe09f6c2537d12856cd9c3ab15611c6be0695ffc01c01d13  keyforge-soft-linear.zip
```
