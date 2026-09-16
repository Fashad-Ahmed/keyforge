# KeyForge

KeyForge is a free, open-source, privacy-first keyboard sound engine for macOS, Windows, and Linux.

This repository currently contains Milestone 3: a secure Tauri 2 desktop foundation, a statically exported Next.js presentation layer, a native Rust audio engine, and a native sound-pack importer. Keyboard capture, production UI controls, autostart, updates, community features, and application networking are intentionally not implemented yet.

## Architecture

- Next.js and TypeScript render the interface as static files in `out/`.
- Tauri loads those files directly; there is no production Next.js server.
- Native functionality belongs in Rust and crosses IPC only through explicitly registered commands.
- The only custom command is `get_app_info`.
- The main window has no built-in Tauri core permissions.
- The audio engine accepts validated, decoded PCM only; encoded audio bytes and file paths are outside its boundary.
- Its fixed mixer supports 32 simultaneous voices, and master volume is validated and held only in memory.
- The audio engine and pack manager have no production IPC or UI integration in Milestone 3.
- `AudioEngine` and `PackManager` are not constructed during ordinary Tauri startup.
- Sound packs contain data only: one strict JSON manifest and signed-16 PCM WAV files for normal, Space, Enter, Backspace, and Modifier groups.
- Imports enforce a 16 MiB compressed archive limit, reject cross-platform traversal, decode before same-parent staging, and reject duplicate pack IDs without replacement.
- Installed audio is rewritten as canonical signed-16 PCM WAV; the audio registry receives decoded PCM only.
- There is no sound-pack IPC and no application networking.
- Milestone 4 owns sanitized input integration.
- Milestone 6 owns product UI and persistent volume.
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
