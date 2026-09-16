# KeyForge Milestone 4 — Local Key Sounds Design

**Date:** 2026-09-16
**Status:** Draft for review
**Scope:** First usable offline key-sound runtime, macOS input adapter first

## Goal

Make KeyForge usable as a local desktop keyboard sound engine: the native app starts the audio engine, loads the bundled mechanical sound pack, listens for local keyboard activity, converts native input into sanitized sound categories, and plays short key sounds without exposing raw key events or typed content to the frontend.

This milestone is the first product slice people can actually use. It remains offline, private, and native-first.

## Product Behavior

- The app starts with sound playback enabled by default when native startup succeeds.
- The bundled `keyforge-mechanical` pack is loaded and registered with the native audio engine.
- Local key presses trigger sounds from one of five sanitized groups: `normal`, `space`, `enter`, `backspace`, or `modifier`.
- `normal` sounds rotate through the bundled variants using Rust-owned deterministic state. Later milestones may replace this with stronger randomized selection.
- The frontend shows only coarse status and controls:
  - enabled or disabled;
  - master volume;
  - current audio/input readiness state.
- Users can toggle sounds on/off and adjust volume.
- No typed characters, key codes, native keyboard events, window titles, application names, device names, file paths, or pack internals are sent to the frontend.

## Platform Scope

Milestone 4 implements production keyboard input on macOS first because it is the current development platform and gives the fastest usable build.

The input subsystem must still be shaped for cross-platform support:

- `input/mod.rs` defines the common native interface.
- `input/event.rs` defines sanitized `SoundEvent`.
- `input/macos.rs` owns the first platform adapter.
- `input/windows.rs` and `input/linux.rs` are explicit unsupported stubs returning `UnsupportedPlatform` until their milestones are implemented.

Windows and Linux support are not skipped forever; they are intentionally separated so the first product build can ship sooner.

## Security and Privacy Properties

- Raw native keyboard events may exist only inside the platform input adapter.
- Raw typed content must never be stored, transmitted, logged, exposed over IPC, or represented in frontend state.
- The frontend must never receive raw key codes, characters, scan codes, modifier bitsets, native event payloads, active app names, or key history.
- Tauri IPC may expose only coarse runtime state and user controls.
- No telemetry, analytics, account system, networking, updater, marketplace, remote registry, or community functionality is added.
- Sound packs remain data only. This milestone only uses the already validated bundled pack.
- New Tauri permissions must be limited to explicit commands needed for app state and controls. No shell, filesystem, network, dialog, clipboard, notification, global shortcut, or updater permission is added.

## Native Runtime Architecture

Add `src-tauri/src/runtime/`:

```text
runtime/
├── mod.rs       Runtime owner, state, command-facing methods
├── selector.rs  SoundEvent to SampleId selection
└── state.rs     Sanitized runtime status snapshot
```

`KeyForgeRuntime` owns:

- `AudioEngine`;
- registered bundled `RegisteredPack`;
- input listener handle;
- enabled flag;
- current master volume;
- coarse input/audio status.

It exposes Rust methods used by Tauri commands:

```rust
pub fn snapshot(&self) -> RuntimeSnapshot;
pub fn set_enabled(&self, enabled: bool) -> RuntimeSnapshot;
pub fn set_volume(&self, volume: f32) -> Result<RuntimeSnapshot, RuntimeControlError>;
```

The runtime never exposes `SampleId`, file paths, decoded PCM, pack filenames, native keyboard events, or backend error strings over IPC.

## Sanitized Input Model

Add `src-tauri/src/input/`:

```text
input/
├── mod.rs
├── event.rs
├── macos.rs
├── windows.rs
└── linux.rs
```

The sanitized event enum is:

```rust
pub enum SoundEvent {
    Normal,
    Space,
    Enter,
    Backspace,
    Modifier,
}
```

The platform adapter may inspect native events only long enough to classify them into one of these variants or ignore them. It must not retain or emit raw native values. Key-up sounds are deferred.

The common interface is:

```rust
pub trait InputListener {
    fn stop(self) -> Result<(), InputError>;
}

pub fn start_listener(
    sink: impl Fn(SoundEvent) + Send + Sync + 'static,
) -> Result<Box<dyn InputListener + Send>, InputError>;
```

On unsupported platforms, `start_listener` returns `InputError::UnsupportedPlatform`.

## macOS Input Adapter

The macOS adapter uses a system event tap for key-down events only. It maps native events to sanitized categories and forwards only `SoundEvent` to the runtime sink.

It must fail closed when permission or event-tap setup is unavailable. The runtime should still start the frontend and audio state, but it reports input as unavailable and does not play sounds.

The adapter must not:

- log events;
- expose key codes over IPC;
- store key history;
- inspect focused application metadata;
- capture typed text.

## Tauri Commands

Add only these explicit commands:

```rust
get_runtime_status() -> RuntimeStatusDto
set_sound_enabled(enabled: bool) -> RuntimeStatusDto
set_master_volume(volume: f32) -> Result<RuntimeStatusDto, RuntimeCommandError>
```

`RuntimeStatusDto` contains only:

- `sound_enabled: bool`;
- `volume: f32`;
- `audio_status: "starting" | "ready" | "recovering" | "unavailable" | "stopped"`;
- `input_status: "starting" | "ready" | "unsupported" | "permission_denied" | "unavailable"`;
- `pack_name: string`;
- `pack_id: string`;
- group counts.

The existing app-info command remains unchanged.

## Frontend

The Next.js frontend remains static export only. It adds a compact control surface to the existing app shell:

- status row for audio/input;
- on/off toggle;
- volume slider;
- bundled pack summary.

The frontend does not subscribe to key events and does not implement sound selection. It invokes only the three explicit commands above.

## Testing Strategy

Use TDD task-by-task.

Rust tests:

- `SoundEvent` selectors choose only registered sample IDs and rotate normal variants deterministically.
- disabled runtime ignores input events.
- volume validation rejects non-finite and out-of-range values.
- runtime snapshots contain no raw/native fields.
- unsupported Windows/Linux adapters return `UnsupportedPlatform`.
- macOS event classification is tested through a pure function that accepts a small internal enum, not raw platform events in assertions.
- Tauri command DTO serialization contains only reviewed fields.

Frontend tests:

- status renders without raw/native fields;
- toggle invokes only `set_sound_enabled`;
- slider invokes only `set_master_volume`;
- static export policy remains enforced.

Security tests:

- capabilities include only reviewed command permissions;
- command allowlist matches the three runtime commands plus app-info;
- no filesystem, shell, network, dialog, updater, clipboard, notification, or global shortcut permission appears;
- no frontend API exposes key events or typed content names.

Manual verification:

- run `pnpm tauri dev` on macOS;
- grant accessibility/input monitoring permission if macOS prompts;
- confirm key presses produce the bundled mechanical sound;
- confirm toggling off stops sounds;
- confirm volume changes affect playback.

## Non-Goals

This milestone does not include:

- Windows or Linux production keyboard hooks;
- key-up sounds;
- custom pack import UI;
- pack selection UI beyond the bundled pack summary;
- persistent settings;
- tray behavior;
- launch at startup;
- updater or release signing;
- networking, telemetry, accounts, registry, marketplace, or community functionality;
- application-specific sound profiles;
- frontend key capture.

## Follow-Up Milestones

1. Windows input adapter.
2. Linux input adapter.
3. Persistent local settings.
4. System tray and start/stop ergonomics.
5. Custom sound-pack import UI.
