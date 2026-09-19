# KeyForge Product UI, Pack Library, Settings, and Tray Design

**Date:** 2026-09-17  
**Milestone:** M6  
**Status:** Approved for implementation planning

## Objective

Turn the existing macOS sound runtime into a coherent daily-use desktop product without weakening KeyForge's privacy boundary. Users can control playback, persist preferences, import and select validated local sound packs, close the window while playback continues, and control the application from a native tray menu.

This milestone remains local-only. It adds no networking, telemetry, analytics, accounts, updater, autostart, community functionality, Windows input hook, or Linux input hook.

## User Experience

KeyForge opens to one focused control panel. The information hierarchy is:

1. privacy and native input status;
2. active sound profile;
3. playback enable and master volume;
4. installed local sound-pack library;
5. local pack import.

Closing the main window hides it while the native runtime continues. The tray menu contains:

- Enable Sounds / Disable Sounds;
- Show KeyForge;
- Quit.

The tray playback item and frontend playback control reflect the same Rust-owned state. Show KeyForge restores and focuses the existing main window. Quit is the only tray action that intentionally terminates the application.

## Visual Direction

The approved direction is **Precision Instrument**:

- near-black teal background;
- subtle technical grid;
- mint status illumination;
- restrained violet window accent;
- compact instrument-like layout;
- crisp, high-contrast typography;
- translucent surfaces used only where hierarchy benefits from them.

The design must not become a collection of nested glass cards. Glass is limited to control and library surfaces, while primary content remains visually quiet.

Motion is functional: a restrained runtime-status pulse, switch transitions, volume feedback, pack activation, and import progress. The interface honors reduced-motion preferences. Every control has a visible keyboard focus state, an accessible name, and sufficient contrast.

## Native Architecture

Rust remains the source of truth for runtime state, settings, pack installation, pack activation, tray state, window lifecycle, and native file selection. Next.js remains a static-export presentation layer.

### Settings Store

`SettingsStore` owns a versioned settings document in the platform application-data directory. V1 stores only:

- `sound_enabled: bool`;
- `master_volume: validated finite value from 0.0 through 1.0`;
- `selected_pack_id: validated pack ID`.

It never stores key events, key codes, typed content, device identifiers, audio-device names, archive source paths, or usage history.

Writes use a same-directory temporary file followed by atomic replacement. The store validates all loaded values before applying them. Missing settings produce defaults. Invalid, corrupted, or unsupported-version settings produce defaults and a sanitized settings status; the original invalid file is not silently overwritten during load.

At startup, settings are loaded before the runtime snapshot is made available. The selected installed pack is decoded and registered if valid. If it cannot be loaded, KeyForge activates the bundled default pack and reports a sanitized recovery state.

### Pack Catalog and Activation

The existing `PackManager` remains the only installation and validation boundary. The catalog exposes sanitized summaries containing only:

- pack ID;
- display name;
- whether the pack is bundled;
- whether the pack is active;
- sanitized group counts when useful to the interface.

No installed path, import source path, manifest internals, archive entry name, decoded sample, or registry identifier crosses IPC.

Import begins with a Rust-owned native file dialog restricted to `.zip`. Cancellation is a normal result and produces no error. A chosen archive is passed directly to `PackManager`; JavaScript never receives or supplies its filesystem path.

Successful installation does not partially activate a pack. Selection follows a prepare-then-commit sequence:

1. resolve the installed pack by validated ID;
2. decode every referenced audio file;
3. register the complete decoded set with the audio engine;
4. build a new selector;
5. atomically replace the active pack and selector;
6. persist the selected ID only after activation succeeds.

Any failure leaves the previous pack active. Duplicate IDs and invalid archives return sanitized, stable error codes suitable for UI copy.

### Runtime Coordination

`KeyForgeRuntime` coordinates settings, pack catalog, active selector, audio handle, input listener, and observable runtime status. All state transitions exposed to commands or tray actions go through runtime methods rather than mutating individual components.

Playback callbacks remain frontend-independent. The macOS adapter continues reducing native key codes to `SoundEvent` inside Rust. Raw events and key codes never enter settings, logs, tray code, IPC responses, or JavaScript.

Changing volume updates the live audio handle first and persists only a validated value. Changing the enabled state updates runtime playback and persists the result. Persistence failure is reported without exposing a path; the runtime must not claim the setting was durably saved.

## Tray and Window Lifecycle

The tray icon and menu are created by Rust during Tauri setup. Menu events call the same runtime methods used by IPC commands.

The main window close request is intercepted and converted to hide. This interception applies only during normal operation. An explicit Quit action marks shutdown intent before closing, allowing the application to terminate normally rather than hiding again.

The tray menu enable label and checked/unchecked state are refreshed after changes from either the tray or frontend. Tray failure must not disable audio playback or prevent the main window from opening; it produces a sanitized degraded status.

No autostart permission or plugin is added.

## IPC Boundary

The command surface remains explicit. Commands cover only:

- get the sanitized product/runtime snapshot and pack catalog;
- set playback enabled;
- set validated master volume;
- open the native import dialog and return a sanitized import result;
- select an installed pack by validated pack ID.

The frontend cannot provide a filesystem path. It cannot request arbitrary file reads, enumerate directories, access audio samples, select native devices, inspect input events, or execute pack content.

All returned errors use stable public variants such as cancelled, duplicate pack, invalid pack, unsupported audio, activation failed, persistence failed, and unavailable. Internal IO details and paths are not returned.

Tauri capabilities remain least-privilege. Any dialog or tray capability/dependency required by implementation must be locally scoped, explicitly documented, and guarded by security tests. No shell, process, HTTP, filesystem, event-broadcast, clipboard, notification, updater, or autostart permission is allowed.

## Frontend Structure

The static Next.js application renders:

- a privacy/input-status header;
- active-pack hero information;
- enabled toggle and accessible volume slider;
- installed-pack list with active state and select actions;
- import control and inline sanitized feedback;
- degraded-state guidance for audio, input permission, settings, and tray availability.

Frontend state is a view of Rust-owned state. After every mutation, the command response supplies the authoritative sanitized snapshot. Optimistic visuals may indicate an operation is pending, but must reconcile to the native response.

There are no API routes, Server Actions, middleware, SSR, production Next.js server, browser persistence, external fonts, remote images, analytics, or runtime fetches.

## Error and Recovery UX

The interface distinguishes these states without exposing sensitive detail:

- importing;
- installed and active;
- installed but not active;
- cancelled;
- duplicate pack;
- invalid or unsupported pack;
- activation failed while the previous pack remains active;
- settings unavailable or not persisted;
- audio unavailable;
- macOS input permission required;
- tray unavailable.

Errors appear next to the relevant action and remain dismissible. A failed pack change explicitly confirms that the previous pack is still active. Permission guidance explains only the required user action and does not claim permission was granted until the native adapter reports ready.

## Testing Strategy

Development follows TDD with observed failing tests before implementation.

Rust tests cover:

- settings defaults, validation, version rejection, corruption recovery, atomic replacement, and sanitized failures;
- startup restoration and fallback to the bundled pack;
- pack catalog sanitization;
- import cancellation, duplicate, invalid, and successful results;
- prepare-then-commit activation preserving the previous pack on every failure stage;
- synchronized tray/runtime enable state;
- close-to-tray, show, and explicit quit lifecycle logic through testable handlers;
- no raw input or filesystem paths in serializable command types;
- exact command registration and capability allowlists.

Frontend tests cover:

- approved information hierarchy and Precision Instrument presentation tokens;
- authoritative runtime reconciliation after mutations;
- accessible toggle, slider, pack selection, import, focus, and reduced-motion behavior;
- every sanitized status and error state;
- absence of browser persistence, networking, server-only Next.js features, and raw/native input fields.

Security policy tests continue scanning dependencies, capabilities, commands, production sources, and documentation. CI runs frontend tests, static export, Rust formatting, Clippy with warnings denied, and all Rust tests on the existing supported matrix.

Manual verification covers desktop launch, import through the native picker, pack activation, live enable/volume behavior, close-to-tray, tray show/toggle/quit, restart persistence, input-permission degradation, and visual/accessibility review.

## Documentation

Update README, trust-boundary documentation, threat model, and any permission rationale to describe:

- exactly what settings are persisted;
- the native-only file path boundary;
- atomic pack activation and fallback;
- close-to-tray behavior;
- the final command and capability allowlists;
- excluded functionality.

## Explicit Exclusions

- autostart or launch at login;
- updater;
- networking of any kind;
- telemetry or analytics;
- accounts or community features;
- remote pack discovery or download;
- arbitrary local folder browsing;
- sound-pack scripts, binaries, libraries, macros, or commands;
- Windows global input implementation;
- Linux global input implementation;
- audio-device selection;
- typed-content history or per-key logging.

## Acceptance Criteria

The milestone is complete only when:

- settings survive restart and invalid settings fail safely;
- local ZIP packs can be selected through a native picker, securely installed, and activated;
- activation failure preserves the working pack;
- closing the window keeps playback running in the tray;
- tray enable, show, and quit actions work and remain synchronized with the UI;
- the Precision Instrument UI is accessible, responsive, and statically exported;
- raw keyboard events, raw key codes, paths, samples, and internal identifiers never cross IPC;
- no networking, telemetry, analytics, autostart, updater, or new platform input hook is present;
- capabilities and dependencies pass explicit least-privilege review;
- all automated verification and the manual desktop checklist pass.
