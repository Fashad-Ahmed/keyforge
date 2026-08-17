# KeyForge Agent Instructions

## Product

KeyForge is a privacy-first, open-source, cross-platform keyboard sound engine.

## Architecture

- Tauri 2 desktop shell.
- Next.js + TypeScript static-export frontend.
- Rust owns all native input, audio, pack validation, settings, and platform integration.
- Next.js is presentation only.
- No Next.js SSR, API routes, Server Actions, middleware, or runtime server.

## Security Invariants

1. Never persist typed key content.
2. Never transmit typed key content.
3. Raw native keyboard events must never be emitted to the frontend.
4. No telemetry or analytics SDK.
5. No application networking in V1.
6. Sound packs are data only: declarative metadata plus validated audio files.
7. Never execute scripts, binaries, dynamic libraries, macros, or commands from sound packs.
8. New Tauri permissions require explicit justification and review.
9. Prefer the minimum dependency surface.
10. Keep OS-specific input implementations behind a common Rust interface.

## Engineering Workflow

- Use TDD for behavior changes.
- Run frontend tests and build before committing frontend changes.
- Run `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` before committing Rust changes.
- Keep commits narrow and descriptive.
- Never bypass a failing security test to complete a feature.
