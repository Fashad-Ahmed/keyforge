# Local Key Sounds Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the first usable KeyForge product slice: native startup loads the bundled pack, plays local macOS key sounds, and exposes only sanitized controls/status to the static frontend.

**Architecture:** Rust owns audio, pack loading, sound selection, and native input. The frontend remains presentation-only and calls explicit Tauri commands for coarse status, enable/disable, and volume. The macOS input adapter converts native key-down events into `SoundEvent` categories inside Rust; Windows/Linux return unsupported stubs for this milestone.

**Tech Stack:** Tauri 2, Rust 1.88.0, Next.js static export, TypeScript, Tailwind CSS, Vitest, Cargo tests. No new production dependency is planned for the first pass; macOS event-tap calls use local FFI.

**Spec:** `docs/superpowers/specs/2026-09-16-local-key-sounds-design.md`

## Global Constraints

- Read `AGENTS.md`, `SECURITY.md`, `docs/architecture/trust-boundaries.md`, `docs/security/threat-model.md`, and the spec before implementation.
- Work task-by-task with one focused commit per completed task.
- Use TDD for every behavior change and observe each new test fail for the intended reason before implementing it.
- Do not add networking, telemetry, analytics, accounts, updater, autostart, marketplace, community features, custom pack import UI, tray behavior, or persistent settings.
- Raw keyboard events may exist only inside the native input adapter.
- Raw typed content must never be stored, transmitted, logged, or exposed to the frontend.
- The frontend must never receive raw key codes, characters, scan codes, modifier masks, native event payloads, app names, device names, paths, key history, decoded PCM, or sample IDs.
- New Tauri IPC is limited to explicit reviewed commands: `get_runtime_status`, `set_sound_enabled`, and `set_master_volume`.
- Tauri capabilities must remain least-privilege; add no filesystem, shell, network, dialog, updater, clipboard, notification, global shortcut, or OS plugin permission.
- Sound packs remain data-only. This milestone uses only the validated bundled default pack.
- macOS is the only production input adapter in this milestone. Windows/Linux must return `UnsupportedPlatform`.

---

## File Structure

```text
src-tauri/src/
├── input/
│   ├── mod.rs          Common listener API and platform dispatch
│   ├── event.rs        Sanitized SoundEvent and pure classification helpers
│   ├── macos.rs        macOS event-tap adapter and stop handle
│   ├── windows.rs      Unsupported stub
│   └── linux.rs        Unsupported stub
├── runtime/
│   ├── mod.rs          KeyForgeRuntime owner and control methods
│   ├── selector.rs     RegisteredPack + SoundEvent sample selection
│   └── state.rs        Runtime snapshots and sanitized DTO source types
├── commands/
│   ├── app_info.rs
│   ├── mod.rs
│   └── runtime.rs      Tauri commands for status, toggle, and volume
└── lib.rs              Builds runtime state and registers reviewed commands

components/app-shell.tsx             Add controls
components/app-shell.test.tsx        Add UI behavior coverage
lib/native/api.ts                    Add runtime command wrappers
lib/native/api.test.ts               Add wrapper tests
lib/types/runtime.ts                 Add frontend-safe runtime types
security/audio-policy.test.ts        Update reviewed IPC handler allowlist
security/tauri-policy.test.ts        Keep no-permission policy asserted
docs/architecture/trust-boundaries.md
docs/security/threat-model.md
```

## Task 1: Sanitized Events and Sound Selection

**Files:**
- Create: `src-tauri/src/input/event.rs`
- Create: `src-tauri/src/input/mod.rs`
- Create: `src-tauri/src/runtime/selector.rs`
- Create: `src-tauri/src/runtime/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `input::SoundEvent`
- Produces: `runtime::selector::SoundSelector`
- Consumes: `pack::RegisteredPack`

- [ ] **Step 1: Write failing tests for sanitized event classification**

Create `src-tauri/src/input/event.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_only_sanitized_categories() {
        assert_eq!(classify_key_for_test(TestKey::Space), Some(SoundEvent::Space));
        assert_eq!(classify_key_for_test(TestKey::Enter), Some(SoundEvent::Enter));
        assert_eq!(classify_key_for_test(TestKey::Backspace), Some(SoundEvent::Backspace));
        assert_eq!(classify_key_for_test(TestKey::Modifier), Some(SoundEvent::Modifier));
        assert_eq!(classify_key_for_test(TestKey::Printable), Some(SoundEvent::Normal));
        assert_eq!(classify_key_for_test(TestKey::Ignored), None);
    }
}
```

- [ ] **Step 2: Run and verify red**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml input::event::tests::classifies_only_sanitized_categories
```

Expected: compilation fails because `input`, `SoundEvent`, `TestKey`, and `classify_key_for_test` do not exist.

- [ ] **Step 3: Implement minimal sanitized event model**

Expose `pub mod input;` from `lib.rs`. Create `input/mod.rs` and implement `event.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundEvent {
    Normal,
    Space,
    Enter,
    Backspace,
    Modifier,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestKey {
    Printable,
    Space,
    Enter,
    Backspace,
    Modifier,
    Ignored,
}

#[cfg(test)]
pub fn classify_key_for_test(key: TestKey) -> Option<SoundEvent> {
    match key {
        TestKey::Printable => Some(SoundEvent::Normal),
        TestKey::Space => Some(SoundEvent::Space),
        TestKey::Enter => Some(SoundEvent::Enter),
        TestKey::Backspace => Some(SoundEvent::Backspace),
        TestKey::Modifier => Some(SoundEvent::Modifier),
        TestKey::Ignored => None,
    }
}
```

- [ ] **Step 4: Verify green**

Run the command from Step 2. Expected: test passes.

- [ ] **Step 5: Write failing selector tests**

Create `runtime/selector.rs` with tests that build a test pack mapping without touching audio devices:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{audio::SampleId, input::SoundEvent};

    fn selector() -> SoundSelector {
        SoundSelector::from_groups_for_test(
            [SampleId::from_raw_for_test(10), SampleId::from_raw_for_test(11), SampleId::from_raw_for_test(12)],
            [SampleId::from_raw_for_test(20)],
            [SampleId::from_raw_for_test(30)],
            [SampleId::from_raw_for_test(40)],
            [SampleId::from_raw_for_test(50)],
        )
    }

    #[test]
    fn rotates_normal_variants_without_exposing_keys() {
        let selector = selector();
        assert_eq!(selector.select(SoundEvent::Normal), Some(SampleId::from_raw_for_test(10)));
        assert_eq!(selector.select(SoundEvent::Normal), Some(SampleId::from_raw_for_test(11)));
        assert_eq!(selector.select(SoundEvent::Normal), Some(SampleId::from_raw_for_test(12)));
        assert_eq!(selector.select(SoundEvent::Normal), Some(SampleId::from_raw_for_test(10)));
    }

    #[test]
    fn selects_special_groups_directly() {
        let selector = selector();
        assert_eq!(selector.select(SoundEvent::Space), Some(SampleId::from_raw_for_test(20)));
        assert_eq!(selector.select(SoundEvent::Enter), Some(SampleId::from_raw_for_test(30)));
        assert_eq!(selector.select(SoundEvent::Backspace), Some(SampleId::from_raw_for_test(40)));
        assert_eq!(selector.select(SoundEvent::Modifier), Some(SampleId::from_raw_for_test(50)));
    }
}
```

- [ ] **Step 6: Run and verify red**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml runtime::selector::tests
```

Expected: compilation fails because `runtime`, `SoundSelector`, and test-only `SampleId::from_raw_for_test` do not exist.

- [ ] **Step 7: Implement selector and test-only SampleId constructor**

Add a test-only constructor to `src-tauri/src/audio/sample.rs`:

```rust
#[cfg(test)]
impl SampleId {
    pub(crate) fn from_raw_for_test(value: u64) -> Self {
        Self(value)
    }
}
```

Create `runtime/mod.rs` with `pub(crate) mod selector;`. Implement `SoundSelector` with `AtomicUsize` for normal rotation and `Vec<SampleId>` group storage. Keep all methods `pub(crate)`.

- [ ] **Step 8: Verify green and commit**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml input::event::tests runtime::selector::tests
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

Commit:

```bash
git add src-tauri/src/input src-tauri/src/runtime src-tauri/src/lib.rs src-tauri/src/audio/sample.rs
git commit -m "feat: add sanitized sound event selection"
```

## Task 2: Runtime Owner and Safe Command DTOs

**Files:**
- Create: `src-tauri/src/runtime/state.rs`
- Modify: `src-tauri/src/runtime/mod.rs`
- Create: `src-tauri/src/commands/runtime.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `KeyForgeRuntime`
- Produces: `RuntimeSnapshot`
- Produces Tauri commands listed in the spec

- [ ] **Step 1: Write failing runtime state tests**

Add tests to `runtime/state.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_serializes_only_reviewed_fields() {
        let snapshot = RuntimeSnapshot::for_test();
        let value = serde_json::to_value(&snapshot).unwrap();
        let keys = value.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
        assert_eq!(keys, vec![
            "audioStatus",
            "groupCounts",
            "inputStatus",
            "packId",
            "packName",
            "soundEnabled",
            "volume",
        ]);
    }

    #[test]
    fn rejects_invalid_volume() {
        assert_eq!(ValidatedVolume::new(-0.1), Err(RuntimeControlError::InvalidVolume));
        assert_eq!(ValidatedVolume::new(1.1), Err(RuntimeControlError::InvalidVolume));
        assert_eq!(ValidatedVolume::new(f32::NAN), Err(RuntimeControlError::InvalidVolume));
        assert!(ValidatedVolume::new(0.5).is_ok());
    }
}
```

- [ ] **Step 2: Run and verify red**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml runtime::state::tests
```

Expected: compilation fails because the state types do not exist.

- [ ] **Step 3: Implement sanitized state types**

Implement serializable enums and structs with `#[serde(rename_all = "camelCase")]` for structs and `#[serde(rename_all = "snake_case")]` for enum variants. Use only the fields from the spec.

- [ ] **Step 4: Verify green**

Run the command from Step 2. Expected: tests pass.

- [ ] **Step 5: Write failing command tests**

Add unit tests in `commands/runtime.rs` using `KeyForgeRuntime::new_for_test()`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_return_sanitized_status_and_apply_controls() {
        let runtime = KeyForgeRuntime::new_for_test();
        assert!(get_runtime_status_from(&runtime).sound_enabled);
        assert!(!set_sound_enabled_on(&runtime, false).sound_enabled);
        assert_eq!(set_master_volume_on(&runtime, 0.25).unwrap().volume, 0.25);
    }
}
```

- [ ] **Step 6: Run and verify red**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml commands::runtime::tests
```

Expected: compilation fails because the command helpers and test runtime do not exist.

- [ ] **Step 7: Implement test runtime and command wrappers**

Implement `KeyForgeRuntime::new_for_test()` without starting audio or input. Add `commands/runtime.rs` command functions and helper functions that take `&KeyForgeRuntime` for unit tests. Do not register the commands in Tauri yet.

- [ ] **Step 8: Verify green and commit**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml runtime::state::tests commands::runtime::tests
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

Commit:

```bash
git add src-tauri/src/runtime src-tauri/src/commands
git commit -m "feat: add sanitized runtime controls"
```

## Task 3: Frontend API, UI Controls, and IPC Policy

**Files:**
- Create: `lib/types/runtime.ts`
- Modify: `lib/native/api.ts`
- Modify: `lib/native/api.test.ts`
- Modify: `components/app-shell.tsx`
- Modify: `components/app-shell.test.tsx`
- Modify: `security/audio-policy.test.ts`
- Modify: `security/tauri-policy.test.ts`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/capabilities/main.json` only if Tauri requires explicit command permissions for app commands

**Interfaces:**
- Consumes: runtime commands from Task 2
- Produces: frontend controls for status, enabled toggle, and volume

- [ ] **Step 1: Write failing frontend API tests**

Extend `lib/native/api.test.ts` with tests that expect:

```ts
await getRuntimeStatus();
expect(invokeMock).toHaveBeenCalledWith("get_runtime_status");

await setSoundEnabled(false);
expect(invokeMock).toHaveBeenCalledWith("set_sound_enabled", { enabled: false });

await setMasterVolume(0.25);
expect(invokeMock).toHaveBeenCalledWith("set_master_volume", { volume: 0.25 });
```

- [ ] **Step 2: Run and verify red**

Run:

```bash
pnpm test lib/native/api.test.ts
```

Expected: tests fail because the API functions do not exist.

- [ ] **Step 3: Implement frontend runtime types and API wrappers**

Create `lib/types/runtime.ts` with the exact DTO fields from the spec. Add the three API functions to `lib/native/api.ts`.

- [ ] **Step 4: Verify API green**

Run the command from Step 2. Expected: tests pass.

- [ ] **Step 5: Write failing UI tests**

Extend `components/app-shell.test.tsx` to assert the shell displays runtime status, toggles enabled state through `setSoundEnabled`, and changes volume through `setMasterVolume`.

- [ ] **Step 6: Run and verify UI red**

Run:

```bash
pnpm test components/app-shell.test.tsx
```

Expected: tests fail because the controls are absent.

- [ ] **Step 7: Implement compact controls**

Update `AppShell` to load app info and runtime status, display status, add an accessible checkbox/switch for enabled, and add an accessible range input for volume. Keep layout simple and avoid marketing copy.

- [ ] **Step 8: Register Tauri commands and update security policy tests**

Update `lib.rs` to manage `KeyForgeRuntime` with Tauri state and register:

```rust
tauri::generate_handler![
    commands::app_info::get_app_info,
    commands::runtime::get_runtime_status,
    commands::runtime::set_sound_enabled,
    commands::runtime::set_master_volume
]
```

Update `security/audio-policy.test.ts` reviewed handler allowlist to this exact command set. Keep capability permissions empty unless the Tauri build requires app command permissions; if it does, add only command-specific permissions and update `security/tauri-policy.test.ts` with exact justification.

- [ ] **Step 9: Verify and commit**

Run:

```bash
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml commands::runtime::tests runtime::state::tests
```

Commit:

```bash
git add app components lib security src-tauri/src/lib.rs src-tauri/src/commands src-tauri/capabilities/main.json
git commit -m "feat: add runtime status controls"
```

## Task 4: Native Runtime Startup and macOS Input Adapter

**Files:**
- Modify: `src-tauri/src/input/mod.rs`
- Create: `src-tauri/src/input/macos.rs`
- Create: `src-tauri/src/input/windows.rs`
- Create: `src-tauri/src/input/linux.rs`
- Modify: `src-tauri/src/runtime/mod.rs`
- Modify: `src-tauri/src/runtime/selector.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: docs if macOS permissions need manual notes

**Interfaces:**
- Consumes: `SoundEvent`, `SoundSelector`, `AudioEngine`, bundled pack API
- Produces: native runtime startup with local key-sound playback on macOS

- [ ] **Step 1: Write failing unsupported-platform tests**

In `input/mod.rs`, add tests behind non-mac cfg expectations or pure helpers proving unsupported platform status maps to `InputStatus::Unsupported`.

- [ ] **Step 2: Run and verify red**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml input::
```

Expected: compilation fails because listener types do not exist.

- [ ] **Step 3: Implement common listener API and unsupported stubs**

Define `InputError`, `InputStatus`, `InputListener`, and platform dispatch. Windows/Linux return `UnsupportedPlatform`.

- [ ] **Step 4: Write failing runtime input handling tests**

In `runtime/mod.rs`, add tests with `KeyForgeRuntime::new_for_test()` proving:

- disabled runtime ignores `SoundEvent`;
- enabled runtime attempts playback through a fake sink;
- volume update changes snapshot.

- [ ] **Step 5: Run and verify red**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml runtime::tests
```

Expected: tests fail because input event handling is absent.

- [ ] **Step 6: Implement runtime pack/audio startup**

Add production `KeyForgeRuntime::start()`:

- starts `AudioEngine`;
- creates a managed pack root under Tauri app data;
- installs or discovers the bundled default pack;
- decodes and registers it;
- builds `SoundSelector`;
- starts the platform input listener with a sink that calls `handle_sound_event`;
- reports coarse status on failure without exposing backend strings.

For testability, keep construction helpers that inject a fake playback sink without CPAL.

- [ ] **Step 7: Implement macOS event tap adapter**

In `input/macos.rs`, use local FFI to ApplicationServices/CoreFoundation:

- create a session event tap for key-down events only;
- classify the native key code into `SoundEvent`;
- forward only `SoundEvent` to the sink;
- run the tap on a named thread with a CFRunLoop;
- stop by signaling the run loop and joining the thread;
- return `PermissionDenied` or `Unavailable` when tap creation fails.

Keep raw key codes inside the module and never expose them outside the callback.

- [ ] **Step 8: Verify Rust and commit**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

Commit:

```bash
git add src-tauri/src/input src-tauri/src/runtime src-tauri/src/lib.rs docs
git commit -m "feat: play bundled pack from local key events"
```

## Task 5: Final Verification, Manual Launch, and PR

**Files:**
- No planned code files unless verification exposes a defect.

- [ ] **Step 1: Run complete verification**

Run:

```bash
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

- [ ] **Step 2: Launch locally**

Run:

```bash
pnpm tauri dev
```

On macOS, grant the requested input-monitoring/accessibility permission if prompted. Verify:

- app launches;
- status UI renders;
- key presses play bundled sounds;
- toggle disables playback;
- volume slider changes playback level.

- [ ] **Step 3: Push and create PR**

Push:

```bash
git push origin codex/local-key-sounds
```

Create PR:

```bash
gh pr create --base main --head codex/local-key-sounds --title "feat: play local key sounds on macOS" --body-file /tmp/keyforge-local-key-sounds-pr.md
```

The PR body must include summary, architecture, security properties, tests performed, plan deviations, excluded functionality, and follow-up milestone: Windows/Linux input adapters.
