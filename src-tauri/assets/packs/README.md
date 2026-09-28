# Bundled KeyForge Sound Packs

The original sixteen `keyforge-*.zip` archives are procedural, data-only sound
packs authored by KeyForge contributors and dedicated to the public domain under
CC0-1.0. They are synthesized keyboard-like effects, not recordings of real
switches. The app calls this collection **Earlier synths** to make that clear.

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

## Recorded and playful collections

Six additional packs use distinct, locally bundled CC0 recordings: three
mechanical-keyboard examples (linear, tactile, and clicky) and three playful
effects (bubble pop, rubber duck, and cartoon boing). Each recording is included
as plain mono 48 kHz signed-16 PCM WAV data. The Freesound preview rendition was
used where the source download requires an account; it is not represented as a
full-resolution studio master. The samples are packaged into the strict WAV-only
sound-pack format and shipped with the app, so playback makes no network request.

Credits and source licenses:

- **Linear Switch (Keychron K10):** Normal letters use “Key_Press” by Mediasaur,
  [Freesound #788932](https://freesound.org/people/Mediasaur/sounds/788932/), CC0.
  Space uses “Keychron k10 space_bar” by Sadiquecat,
  [Freesound #789630](https://freesound.org/people/Sadiquecat/sounds/789630/), CC0.
- **Tactile Switch:** “Keyboard_Tactile_9” by StavSounds,
  [Freesound #766640](https://freesound.org/people/StavSounds/sounds/766640/), CC0.
- **Clicky Switch:** “Keyboard_Clicky_2” by StavSounds,
  [Freesound #766617](https://freesound.org/people/StavSounds/sounds/766617/), CC0.
- **Bubble Pop:** “Bubble_Pop” by arttim,
  [Freesound #733264](https://freesound.org/people/arttim/sounds/733264/), CC0.
- **Rubber Duck:** “Rubber Duck” by Slothfully_So,
  [Freesound #685067](https://freesound.org/people/Slothfully_So/sounds/685067/), CC0.
- **Cartoon Boing:** “Cartoon Boing.wav” by reelworldstudio,
  [Freesound #161122](https://freesound.org/people/reelworldstudio/sounds/161122/), CC0.

CC0 does not require credit; these courtesy credits preserve provenance for
users and downstream pack makers. The source WAVs are retained in `sources/`.
Each curated archive contains one `normal` voice and one voice for each special
key group. The linear pack uses the separate real spacebar recording for Space;
its other groups use the single-key Keychron press. Each other pack uses its
named sound throughout. The pack loader still validates every archive and WAV
using the same strict rules as imported packs.

Curated identities:

- `keyforge-switch-linear` — Linear Switch (Keychron K10), version `1.0.0`
- `keyforge-switch-tactile` — Tactile Switch (StavSounds), version `1.0.0`
- `keyforge-switch-clicky` — Clicky Switch (StavSounds), version `1.0.0`
- `keyforge-playful-bubble` — Bubble Pop, version `1.0.0`
- `keyforge-playful-duck` — Rubber Duck, version `1.0.0`
- `keyforge-playful-boing` — Cartoon Boing, version `1.0.0`

## Rebuild a curated archive

The source WAVs, manifests, and deterministic Rust packager are all committed.
From the repository root, choose a relative output path that does not exist:

```sh
cargo run --locked --manifest-path src-tauri/Cargo.toml \
  --example generate_curated_packs -- keyforge-playful-bubble \
  src-tauri/target/keyforge-playful-bubble.repro.zip
cmp src-tauri/assets/packs/keyforge-playful-bubble.zip \
  src-tauri/target/keyforge-playful-bubble.repro.zip
```

Replace `keyforge-playful-bubble` with any one of the six curated IDs above.
The generator uses create-new output semantics, stores only manifest and WAV
data, and never downloads or transforms audio at runtime.

## Identities and contents

- `keyforge-mechanical` — Classic Mechanical, version `1.2.0`
- `keyforge-deep-thock` — Deep Thock, version `1.1.0`
- `keyforge-crisp-click` — Crisp Click, version `1.1.0`
- `keyforge-soft-linear` — Soft Linear, version `1.1.0`
- `keyforge-creamy-tactile` — Creamy Tactile, version `1.0.0`
- `keyforge-silent-mechanical` — Silent Mechanical, version `1.0.0`
- `keyforge-buckling-spring` — Buckling Spring, version `1.0.0`
- `keyforge-vintage-typewriter` — Vintage Typewriter, version `1.0.0`
- `keyforge-marble-thock` — Marble Thock, version `1.0.0`
- `keyforge-poppy-tactile` — Poppy Tactile, version `1.0.0`
- `keyforge-clacky-aluminum` — Clacky Aluminum, version `1.0.0`
- `keyforge-dampened-polycarbonate` — Dampened Polycarbonate, version `1.0.0`
- `keyforge-retro-terminal` — Retro Terminal, version `1.0.0`
- `keyforge-arcade` — Arcade, version `1.0.0`
- `keyforge-soft-office` — Soft Office, version `1.0.0`
- `keyforge-sci-fi-console` — Sci-Fi Console, version `1.0.0`

Each of the sixteen procedural archives contains:
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
6b3673dc01750ce6e7a5cf4c21bb6ebc2491a607ee512c2db292abedafde2022  keyforge-arcade.zip
fd9dba12874c9f34bd16606a1bc85af181366b929b3874c2418d0d5bdd86ea31  keyforge-buckling-spring.zip
616c7cdaa90d27237a405e48ba7f559a3a98560839aaf1b2a51d45b08b2adaf0  keyforge-clacky-aluminum.zip
ae0cc32f351a74710c2997631345302de6acc39c48ee5bed542d159bcf6fc03b  keyforge-crisp-click.zip
aa4134dea8066cdfb01aa4aecf07003365d9fde24b9c237d8cb4ec5092e79415  keyforge-creamy-tactile.zip
48e0db38add57fc9f56b5344a2ff04280574621c8cb12893c5f332565d32546a  keyforge-dampened-polycarbonate.zip
85e43e45b9fc474b6b84e838caeb41756e81962340e1e7ecb32cc2c4152214e8  keyforge-deep-thock.zip
39fce9056b9fdc07d7c2b7a8f311e8f31222a9e52c043e263f17a7d8bb1613c6  keyforge-marble-thock.zip
13c748a8d48ff1a68dbd87b32918f68c685c3cbfffd967d94b079cf6f0e0b919  keyforge-mechanical.zip
a871a037207fc16a7831b9519fe330b0746f70abb143e31f366b93f2a168faf5  keyforge-poppy-tactile.zip
a578618dc6dbbd16524235ebf0073ae7df253159616fd99032c9ac049d4d13a3  keyforge-retro-terminal.zip
cee44f826e655470e340e3054603a11aa8ea6c087f4524ac4746da581bc251f8  keyforge-sci-fi-console.zip
2869d17262a3c66d81ee4bae6c5daaed3f081ab1ec4a58ad0a9ad63d8be0c4ab  keyforge-silent-mechanical.zip
add443b9e681d925860976b1ac1e2be98f0c1861adcedd309deef4b18f330fda  keyforge-soft-linear.zip
e40da6f1fb000ee2e777a707e2562aed0d10d4d11c5adea38ccd073f7ce6388c  keyforge-soft-office.zip
c115c3c38336a5b899799968ce5c07c06eb369404b32eaa9dd9c8417966beade  keyforge-vintage-typewriter.zip
840b5d2234ba7f191b4e2268859ae29370e41029120929ee4fd80ae417c0d536  keyforge-switch-clicky.zip
d007b72f1a15dfb1efc14a7ddef00fc4367492252418cc1f5639d3e7e8f1389c  keyforge-switch-linear.zip
025cb9c9e03862ba28220498d4a755f8096ab39c738ae8aa483351e2a5a252fe  keyforge-switch-tactile.zip
45c9d2bfce320d4f882f4d7780c8f8c5681a13dccc0220070845992ec255f79e  keyforge-playful-boing.zip
acf01d44efe22e7908d135018357f2e75365fe7162a05a5600f4251b2636d4fd  keyforge-playful-bubble.zip
cf344d1824adee10f278847ea7093b1a704373cb39067ddc1f4bb98f34f6e388  keyforge-playful-duck.zip
```
