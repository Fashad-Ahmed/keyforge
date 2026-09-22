# KeyForge

KeyForge is a free, open-source, privacy-first keyboard sound engine for macOS, Windows, and Linux.

This repository currently contains Milestone 6: a secure Tauri 2 desktop foundation, a statically exported Next.js presentation layer, a native Rust audio engine, validated local sound-pack import, persistent controls, a product interface, and close-to-tray operation. macOS local key-sound playback is available; Windows and Linux input adapters remain explicitly unsupported.

## Architecture

- Next.js and TypeScript render the interface as static files in `out/`.
- Tauri loads those files directly; there is no production Next.js server.
- Native functionality belongs in Rust and crosses IPC only through explicitly registered commands.
- Custom commands are limited to reviewed coarse status and control IPC: `get_app_info`, `get_runtime_status`, `set_sound_enabled`, `set_master_volume`, `import_sound_pack`, and `select_sound_pack`.
- The main window has no built-in Tauri core permissions.
- The audio engine accepts validated, decoded PCM only; encoded audio bytes and file paths are outside its boundary.
- Its fixed mixer supports 32 simultaneous voices. Sound enabled, master volume, and selected pack ID are the only persisted product settings.
- The audio engine and pack manager are constructed by Rust during ordinary Tauri startup.
- The macOS input adapter converts native key-down events into internal `SoundEvent` categories only; raw key codes never cross the adapter boundary.
- Windows and Linux input adapters return unsupported status in this milestone.
- Sound packs contain data only: one strict JSON manifest and signed-16 PCM WAV files for normal, Space, Enter, Backspace, and Modifier groups.
- The bundled library separates recorded linear/tactile/clicky mechanical sounds
  from playful bubble-pop, rubber-duck, and cartoon-boing options; earlier
  procedural sounds are labeled separately. All audio is bundled locally, and
  new installs default to the recorded Keychron linear pack.
- Imports enforce a 16 MiB compressed archive limit, reject cross-platform traversal, decode before same-parent staging, and reject duplicate pack IDs without replacement.
- Installed audio is rewritten as canonical signed-16 PCM WAV; the audio registry receives decoded PCM only.
- Import uses a Rust-only native file picker. The selected filesystem path stays in Rust and never crosses IPC.
- Pack changes use prepare-then-commit activation: complete decode and registration precede one authoritative runtime swap, so a failed activation leaves the current sound active.
- Closing the main window uses close-to-tray behavior. The native tray can enable or disable sounds, show KeyForge, or quit.
- IPC errors are sanitized failures with stable variants rather than filesystem paths, decoder details, or operating-system errors.
- There is no application networking. Autostart, networking, updates, and Windows/Linux input hooks remain excluded.
- The application contains no telemetry, analytics, accounts, or runtime networking.

See [the trust-boundary documentation](docs/architecture/trust-boundaries.md) and [threat model](docs/security/threat-model.md) before adding native functionality.

## Prerequisites

- Node.js 22.23.2
- pnpm 10.33.2
- Rust 1.88.0 with `rustfmt` and `clippy`
- Tauri's platform prerequisites for your operating system

## Development

Install locked dependencies and launch the desktop application:

```bash
pnpm install --frozen-lockfile
pnpm tauri dev
```

The development asset server is restricted to `127.0.0.1`. Next.js telemetry is disabled by the committed project environment.

Use **Import local pack** to open the native ZIP picker. Import cancellation is harmless; valid packs are installed and activated locally. The main Tauri capability list remains empty because the picker is invoked by Rust rather than exposed as a frontend plugin API.

On macOS, local key-sound playback uses the operating system input monitoring/accessibility prompt. KeyForge does not store, transmit, log, or send raw key data to the frontend; the native adapter keeps raw key codes inside Rust and forwards only coarse sound categories internally.

To manually smoke-test the native audio engine during development, run:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml --example audio_smoke
```

Warning: this command emits a short tone through the default output device. It is a manual developer check, not a production integration path.

To manually validate the bundled pack import and playback path:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml --example pack_smoke
```

Warning: this developer command plays a short sequence through the default output device. It uses a test-owned temporary pack directory and is not part of production Tauri startup.

## Verification

```bash
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

`pnpm build` must produce `out/index.html`.

## Security

Never expose raw keyboard events or typed content to the frontend. Never persist or transmit typed content. New dependencies, IPC commands, and Tauri permissions require explicit justification and review.

Report vulnerabilities according to [SECURITY.md](SECURITY.md).
