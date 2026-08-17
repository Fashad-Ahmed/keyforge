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

<!-- BEGIN:nextjs-agent-rules -->

# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` (resolved from this file's directory; in monorepos the `next` package may not be visible from the repo root) before writing any code. Heed deprecation notices.

This block is written and re-added by `next dev` — verify at `node_modules/next/dist/server/lib/generate-agent-files.js`. Removing it from a diff only re-creates the uncommitted change; committing it with your work keeps the tree clean.

<!-- END:nextjs-agent-rules -->
