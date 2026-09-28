# Trust Boundaries

## Boundary 1: Operating System → Rust Input Adapter

OS-specific adapters receive native keyboard events. In Milestone 4, macOS is implemented first; Windows and Linux return unsupported status.

## Boundary 2: Rust Input Adapter → Event Sanitizer

Raw events may exist only transiently inside the native input subsystem.

The sanitizer converts raw events into an internal `SoundEvent`. It must not produce typed strings, retain text, or expose key codes outside the adapter.

## Boundary 3: Rust Core → Next.js

Only explicit Tauri commands and non-sensitive state may cross IPC.

Raw keyboard events, key history, and typed content must never cross this boundary.

The reviewed product snapshot contains coarse engine status, validated volume and enabled state, active pack identity, group counts, and sanitized pack summaries. Import uses a Rust-only native file picker, so paths never enter command arguments or responses. Commands return sanitized failures without IO, decoder, device, or path details. The presentation-only `set_panel_presentation` command accepts only the closed values `controls`, `manage`, or `dismiss`; it carries no keyboard events, key codes, typed content, paths, or audio data and cannot mutate playback settings. The frontend has no Tauri capabilities and no browser networking or persistence.

## Boundary 4: Sound-Pack Files → Pack Manager

All pack files are untrusted input.

The local ZIP path exists only inside native pack-manager calls. Raw archive names are lexically validated before any filesystem join, including cross-platform traversal and Windows device-name rejection. The importer enforces a 16 MiB compressed archive limit plus entry, expanded-data, duration, and decoded-memory limits.

Complete manifest validation and audio decoding happen before same-parent staging. Installed content contains canonical JSON and canonical signed-16 PCM WAV files only. Duplicate pack IDs are rejected without replacing existing files.

Pack selection uses prepare-then-commit activation. Rust decodes and registers the complete candidate selector before atomically swapping active runtime state. A failed candidate cannot partially replace the current pack.

## Boundary 5: Pack Manager → Audio Registry

The pack manager passes decoded PCM only into the native audio registry, and only opaque `SampleId` values leave it. Encoded audio bytes, archive names, decoder errors, and filesystem paths never reach the engine.

There is no sound-pack IPC: no pack path, archive bytes, WAV bytes, PCM samples, or decoder diagnostics cross the Tauri boundary. There is no application networking.

## Boundary 6: Product Settings → Local Filesystem

Sound enabled, master volume, and selected pack ID are the complete persisted setting set. Rust validates each value and writes versioned JSON through same-parent temporary storage and platform-safe replacement. Reads never execute content, and corrupt files are not overwritten automatically.

## Boundary 7: Window and Tray → Rust Runtime

On macOS, the tray opens a compact presentation of the existing WebView; Manage sounds switches that same window to the existing library/import presentation. Rust owns window size, position, visibility, focus dismissal, native tray actions, and explicit quit. Dismissing or closing the compact panel hides it without stopping playback. The presentation IPC can request only controls/manage/dismiss and cannot alter runtime settings. Windows and Linux retain ordinary window/tray behavior. The tray calls the same authoritative runtime mutation used by IPC. A tray construction failure degrades to an ordinary recoverable window instead of trapping the application.

## Boundary 8: Build System → Release Artifact

Release artifacts must come from controlled CI and later milestones will add checksums, SBOMs, provenance, malware scanning, and platform signing.

Autostart, networking, updates, and Windows/Linux input hooks remain excluded.
