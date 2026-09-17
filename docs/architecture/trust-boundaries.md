# Trust Boundaries

## Boundary 1: Operating System → Rust Input Adapter

OS-specific adapters receive native keyboard events. In Milestone 4, macOS is implemented first; Windows and Linux return unsupported status.

## Boundary 2: Rust Input Adapter → Event Sanitizer

Raw events may exist only transiently inside the native input subsystem.

The sanitizer converts raw events into an internal `SoundEvent`. It must not produce typed strings, retain text, or expose key codes outside the adapter.

## Boundary 3: Rust Core → Next.js

Only explicit Tauri commands and non-sensitive state may cross IPC.

Raw keyboard events, key history, and typed content must never cross this boundary.

## Boundary 4: Sound-Pack Files → Pack Manager

All pack files are untrusted input.

The local ZIP path exists only inside native pack-manager calls. Raw archive names are lexically validated before any filesystem join, including cross-platform traversal and Windows device-name rejection. The importer enforces a 16 MiB compressed archive limit plus entry, expanded-data, duration, and decoded-memory limits.

Complete manifest validation and audio decoding happen before same-parent staging. Installed content contains canonical JSON and canonical signed-16 PCM WAV files only. Duplicate pack IDs are rejected without replacing existing files.

## Boundary 5: Pack Manager → Audio Registry

The pack manager passes decoded PCM only into the native audio registry, and only opaque `SampleId` values leave it. Encoded audio bytes, archive names, decoder errors, and filesystem paths never reach the engine.

There is no sound-pack IPC: no pack path, archive bytes, WAV bytes, PCM samples, or decoder diagnostics cross the Tauri boundary. There is no application networking.

## Boundary 6: Build System → Release Artifact

Release artifacts must come from controlled CI and later milestones will add checksums, SBOMs, provenance, malware scanning, and platform signing.
