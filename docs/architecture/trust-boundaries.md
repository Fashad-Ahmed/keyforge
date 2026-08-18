# Trust Boundaries

## Boundary 1: Operating System → Rust Input Adapter

Future OS-specific adapters receive native keyboard events.

## Boundary 2: Rust Input Adapter → Event Sanitizer

Raw events may exist only transiently inside the native input subsystem.

The sanitizer converts raw events into an internal `SoundEvent`. It must not produce typed strings or retain text.

## Boundary 3: Rust Core → Next.js

Only explicit Tauri commands and non-sensitive state may cross IPC.

Raw keyboard events, key history, and typed content must never cross this boundary.

## Boundary 4: Sound-Pack Files → Pack Manager

All pack files are untrusted input.

The pack manager must validate paths, file types, sizes, metadata, and audio decoding before use.

## Boundary 5: Build System → Release Artifact

Release artifacts must come from controlled CI and later milestones will add checksums, SBOMs, provenance, malware scanning, and platform signing.
