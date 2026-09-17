# Product UI, Pack Library, Settings, and Tray Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a polished local-first KeyForge control panel with persisted settings, secure pack import/selection, and close-to-tray operation.

**Architecture:** Rust remains authoritative for settings, pack installation and activation, runtime state, native file selection, tray state, and window lifecycle. The statically exported Next.js frontend renders the Precision Instrument UI and communicates only through explicit commands returning sanitized product snapshots.

**Tech Stack:** Tauri 2.11.3, Rust 1.88.0, CPAL 0.18.1, Next.js 16.3.1 static export, React, TypeScript, Tailwind CSS, Vitest, pnpm.

**Spec:** `docs/superpowers/specs/2026-09-17-product-ui-tray-design.md`

## Global Constraints

- Never persist, transmit, log, or expose typed content, native key events, or raw key codes.
- Never return import paths, installed paths, audio samples, registry IDs, device identity, or internal IO errors over IPC.
- No application networking, telemetry, analytics, account, updater, autostart, or community functionality.
- Sound packs remain declarative data and validated audio only.
- Next.js remains a static export with no SSR, API routes, Server Actions, middleware, runtime server, browser persistence, remote fonts, or runtime fetch.
- Keep Tauri capabilities least-privilege; frontend code must not receive dialog, filesystem, shell, process, HTTP, event, clipboard, notification, updater, or autostart access.
- Windows and Linux input adapters remain explicitly unsupported in this milestone.
- Every behavior change follows red-green-refactor, including an observed failing-test run.
- End every task with its focused verification and one narrow git commit.

## File Map

- `src-tauri/src/settings/mod.rs`: validated settings model and store orchestration.
- `src-tauri/src/settings/storage.rs`: same-directory temporary write and platform-safe replacement.
- `src-tauri/src/runtime/catalog.rs`: sanitized pack summaries and activation preparation.
- `src-tauri/src/runtime/lifecycle.rs`: close/show/quit state machine independent of Tauri handles.
- `src-tauri/src/runtime/state.rs`: serializable product snapshot and public error variants.
- `src-tauri/src/runtime/mod.rs`: coordinates settings, audio, pack catalog, selector, input, and mutations.
- `src-tauri/src/commands/runtime.rs`: thin explicit IPC adapters only.
- `src-tauri/src/tray.rs`: native menu construction and event routing.
- `src-tauri/src/lib.rs`: application setup, managed state, close interception, exact handler list.
- `lib/types/runtime.ts`: exact TypeScript mirror of sanitized Rust response types.
- `lib/native/api.ts`: typed invoke wrappers.
- `components/app-shell.tsx`: state orchestration and accessible dashboard composition.
- `components/keyforge/*.tsx`: focused Precision Instrument UI units.
- `app/globals.css`: visual tokens, glass surfaces, focus, responsive, and reduced-motion rules.
- `security/product-policy.test.ts`: dependency, IPC, capability, persistence, and static-frontend guards.

---

### Task 1: Validated Persistent Settings

**Files:**
- Create: `src-tauri/src/settings/mod.rs`
- Create: `src-tauri/src/settings/storage.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Produces: `SettingsStore::open(PathBuf)`, `SettingsStore::load() -> SettingsLoad`, `SettingsStore::save(&AppSettings) -> Result<(), SettingsError>`.
- Produces: `AppSettings { sound_enabled: bool, master_volume: ValidatedVolume, selected_pack_id: PackId }` and `SettingsHealth::{Ready, Defaulted, Corrupt}`.

- [ ] **Step 1: Write failing model and store tests**

Add tests proving missing files yield defaults, valid JSON round-trips, unknown fields and schema versions are rejected, NaN/out-of-range volume cannot be constructed, invalid pack IDs are rejected through the existing `PackId`, corrupt settings are not overwritten on load, and a failed replacement leaves the previous file readable.

```rust
#[test]
fn corrupt_settings_default_without_overwriting_source() {
    let root = TestRoot::new();
    root.write("settings.json", b"{broken");
    let loaded = SettingsStore::open(root.path()).load();
    assert_eq!(loaded.health(), SettingsHealth::Corrupt);
    assert_eq!(loaded.settings(), &AppSettings::default());
    assert_eq!(root.read("settings.json"), b"{broken");
}
```

- [ ] **Step 2: Verify red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml settings::`

Expected: FAIL because `settings` and `SettingsStore` do not exist.

- [ ] **Step 3: Implement the minimal validated store**

Use a private serde wire type with `#[serde(deny_unknown_fields)]` and exact version `1`. Write `settings.json.tmp`, call `sync_all`, and replace `settings.json` through `storage::replace_file`. On Unix use same-directory `rename`; on Windows use the existing `windows-sys` filesystem API for replacement. Serialize only:

```json
{"version":1,"soundEnabled":true,"masterVolume":1.0,"selectedPackId":"keyforge-mechanical"}
```

Map all IO/parse failures to non-path-bearing `SettingsError::{Read, Invalid, Write}`.

- [ ] **Step 4: Verify green and quality**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check && cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings && cargo test --locked --manifest-path src-tauri/Cargo.toml settings::`

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src-tauri/src/settings
git commit -m "feat: persist validated local settings"
```

### Task 2: Sanitized Pack Catalog and Atomic Activation

**Files:**
- Create: `src-tauri/src/runtime/catalog.rs`
- Modify: `src-tauri/src/runtime/mod.rs`
- Modify: `src-tauri/src/runtime/state.rs`
- Modify: `src-tauri/src/runtime/selector.rs`
- Test: `src-tauri/src/runtime/catalog.rs`

**Interfaces:**
- Consumes: `SettingsStore`, `PackManager`, `AudioEngineHandle`, `SoundSelector`.
- Produces: `PackSummary`, `PackCatalog`, `PackActionError`, `ProductSnapshot` (replacing the narrower `RuntimeSnapshot`), `KeyForgeRuntime::select_pack(&PackId)` and `KeyForgeRuntime::install_pack(&Path)`.

- [ ] **Step 1: Write failing catalog and rollback tests**

Prove summaries serialize only `id`, `name`, `bundled`, `active`, and group counts. Test deterministic ID ordering. Introduce test doubles at the decode/register boundary and prove every failure preserves the previous active pack and selector.

```rust
#[test]
fn registration_failure_preserves_active_pack() {
    let runtime = RuntimeHarness::with_active("keyforge-mechanical");
    runtime.fail_registration_for("broken-pack");
    assert_eq!(runtime.select("broken-pack"), Err(PackActionError::ActivationFailed));
    assert_eq!(runtime.snapshot().pack_id, "keyforge-mechanical");
}
```

- [ ] **Step 2: Verify red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml runtime::catalog runtime::tests::registration_failure_preserves_active_pack`

Expected: FAIL on missing catalog and selection APIs.

- [ ] **Step 3: Implement prepare-then-commit activation**

Decode and register outside the runtime-state lock. Build a complete `Arc<SoundSelector>`, then take the lock once to replace `{ active_pack, selector }`. Persist the selected pack only after commit. If persistence fails, return `PersistenceFailed` while truthfully retaining the active in-memory pack and marking settings health degraded.

- [ ] **Step 4: Implement startup restoration**

Load settings before applying volume/enabled state. Attempt the selected installed pack; on failure activate `keyforge-mechanical` and expose `SettingsHealth::Defaulted`. Never delete the failed pack or corrupt settings automatically.

- [ ] **Step 5: Verify green**

Run: `cargo test --locked --manifest-path src-tauri/Cargo.toml runtime::`

Expected: all runtime/catalog tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/runtime
git commit -m "feat: activate installed sound packs atomically"
```

### Task 3: Persist Runtime Controls

**Files:**
- Modify: `src-tauri/src/runtime/mod.rs`
- Modify: `src-tauri/src/runtime/state.rs`
- Modify: `src-tauri/src/commands/runtime.rs`

**Interfaces:**
- Produces: `RuntimeMutationError::{InvalidVolume, AudioUnavailable, PersistenceFailed}`.
- Changes: `set_enabled` and `set_volume` return `Result<ProductSnapshot, RuntimeMutationError>`.

- [ ] **Step 1: Write failing durability tests**

Test successful enable/volume persistence, invalid volume rejection before audio/store mutation, audio failure before persistence, and persistence failure reflected in the returned error and subsequent snapshot.

- [ ] **Step 2: Verify red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml runtime::tests::persists_`

Expected: FAIL because controls do not use `SettingsStore`.

- [ ] **Step 3: Implement ordered mutations**

For volume: validate → update audio → update runtime → persist. For enabled: update runtime → persist. Return sanitized failures and retain accurate in-memory state. Never serialize an IO source error.

- [ ] **Step 4: Verify and commit**

Run: `cargo test --locked --manifest-path src-tauri/Cargo.toml runtime:: commands::runtime::`

```bash
git add src-tauri/src/runtime src-tauri/src/commands/runtime.rs
git commit -m "feat: persist runtime playback controls"
```

### Task 4: Native Pack Import Command

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/runtime/mod.rs`
- Modify: `src-tauri/src/runtime/state.rs`
- Modify: `src-tauri/src/commands/runtime.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/capabilities/main.json`
- Modify: `security/product-policy.test.ts`

**Interfaces:**
- Produces: `import_sound_pack`, `select_sound_pack(pack_id: String)`.
- Produces: `ImportOutcome::{Cancelled, Installed(ProductSnapshot)}` and sanitized `PackActionError`.

- [ ] **Step 1: Write failing command and boundary tests**

Assert cancellation is successful, a selected path is consumed only by Rust, duplicate/invalid archives return stable variants, successful import activates the pack, and serialized request/response types contain no `path`, `file`, `entry`, `sample`, or raw input fields. Add an exact handler and capability allowlist test.

- [ ] **Step 2: Verify red**

Run: `pnpm test security/product-policy.test.ts && cargo test --manifest-path src-tauri/Cargo.toml commands::runtime::`

Expected: FAIL because import/select commands do not exist.

- [ ] **Step 3: Add the native picker with minimum surface**

Use one reviewed native dialog dependency from Rust only, filter to `zip`, and do not import its JavaScript package. Keep `main.json` empty unless the selected implementation demonstrably requires a capability; if it does, add only the exact dialog-open permission and document why. Pass the returned native path directly to `KeyForgeRuntime::install_pack`.

- [ ] **Step 4: Register exact commands**

The custom handler list becomes exactly:

```rust
tauri::generate_handler![
    commands::app_info::get_app_info,
    commands::runtime::get_runtime_status,
    commands::runtime::set_sound_enabled,
    commands::runtime::set_master_volume,
    commands::runtime::import_sound_pack,
    commands::runtime::select_sound_pack,
]
```

- [ ] **Step 5: Verify and commit**

Run: `pnpm test security/product-policy.test.ts && cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings && cargo test --locked --manifest-path src-tauri/Cargo.toml commands::runtime::`

```bash
git add src-tauri security/product-policy.test.ts
git commit -m "feat: import and select local sound packs"
```

### Task 5: Tray and Window Lifecycle

**Files:**
- Create: `src-tauri/src/runtime/lifecycle.rs`
- Create: `src-tauri/src/tray.rs`
- Modify: `src-tauri/src/runtime/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Produces: `LifecycleState::{Running, QuitRequested}` and `LifecycleAction::{HideWindow, ShowWindow, SetEnabled(bool), Quit}`.
- Produces: `tray::install(&tauri::AppHandle, Arc/State<KeyForgeRuntime>)` or equivalent Tauri-managed integration.

- [ ] **Step 1: Write failing pure lifecycle tests**

```rust
#[test]
fn close_hides_until_quit_is_requested() {
    let mut lifecycle = Lifecycle::default();
    assert_eq!(lifecycle.on_close_requested(), CloseDecision::Hide);
    lifecycle.request_quit();
    assert_eq!(lifecycle.on_close_requested(), CloseDecision::Allow);
}
```

Also prove Show targets only the named main window and tray toggle calls the same runtime enable method used by IPC.

- [ ] **Step 2: Verify red**

Run: `cargo test --manifest-path src-tauri/Cargo.toml lifecycle tray`

Expected: FAIL because lifecycle/tray modules do not exist.

- [ ] **Step 3: Implement tray and close interception**

Enable only Tauri's tray feature. Build three items: checked Enable Sounds, Show KeyForge, Quit. On close, prevent close and hide unless quit was requested. Show calls `show`, `unminimize`, and `set_focus`. Quit marks intent, removes/intercepts no further close, and calls application exit.

- [ ] **Step 4: Synchronize tray state**

Refresh checked state and label after successful frontend or tray mutations through one runtime observer/callback path. Tray construction failure sets `TrayStatus::Unavailable` but does not abort audio startup.

- [ ] **Step 5: Verify and commit**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check && cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings && cargo test --locked --manifest-path src-tauri/Cargo.toml lifecycle tray`

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src-tauri/src/tray.rs src-tauri/src/runtime
git commit -m "feat: keep KeyForge running in the tray"
```

### Task 6: Typed Product API and State Reconciliation

**Files:**
- Modify: `lib/types/runtime.ts`
- Modify: `lib/native/api.ts`
- Modify: `lib/native/api.test.ts`
- Modify: `components/app-shell.test.tsx`

**Interfaces:**
- Produces: `ProductSnapshot`, `PackSummary`, `ImportOutcome`, `PackActionError` TypeScript unions mirroring Rust.
- Produces: `importSoundPack()` and `selectSoundPack(packId)`.

- [ ] **Step 1: Write failing API contract tests**

Assert exact invoke names/arguments, cancellation handling, sanitized response keys, and absence of path/raw-event fields. Assert every mutation replaces frontend state with the returned authoritative snapshot.

- [ ] **Step 2: Verify red**

Run: `pnpm test lib/native/api.test.ts components/app-shell.test.tsx`

Expected: FAIL on missing types and wrappers.

- [ ] **Step 3: Implement exact types and wrappers**

```ts
export async function selectSoundPack(packId: string): Promise<ProductSnapshot> {
  return invoke<ProductSnapshot>("select_sound_pack", { packId });
}

export async function importSoundPack(): Promise<ImportOutcome> {
  return invoke<ImportOutcome>("import_sound_pack");
}
```

Do not add localStorage, cookies, IndexedDB, fetch, event listeners for raw native events, or path fields.

- [ ] **Step 4: Verify and commit**

Run: `pnpm test lib/native/api.test.ts components/app-shell.test.tsx`

```bash
git add lib components/app-shell.test.tsx
git commit -m "feat: expose sanitized product controls"
```

### Task 7: Precision Instrument Product Interface

**Files:**
- Create: `components/keyforge/status-header.tsx`
- Create: `components/keyforge/active-profile.tsx`
- Create: `components/keyforge/playback-controls.tsx`
- Create: `components/keyforge/pack-library.tsx`
- Create: `components/keyforge/operation-message.tsx`
- Modify: `components/app-shell.tsx`
- Modify: `components/app-shell.test.tsx`
- Modify: `app/globals.css`
- Modify: `app/layout.tsx`

**Interfaces:**
- Consumes: Task 6 API and types.
- Produces: accessible, responsive Precision Instrument dashboard with no server/runtime web features.

- [ ] **Step 1: Write failing interaction and accessibility tests**

Cover connecting, ready, degraded audio, permission denied, pack list, active pack, toggling, volume, select pending/success/failure, import cancelled/success/failure, retry, keyboard labels, and authoritative rollback. Assert visible copy includes “On-device processing only” and never displays a path.

- [ ] **Step 2: Verify red**

Run: `pnpm test components/app-shell.test.tsx`

Expected: FAIL because the dashboard components and states are absent.

- [ ] **Step 3: Build the component hierarchy**

Keep `AppShell` responsible only for loading and mutation orchestration. Pass immutable status and callbacks to focused components. Disable only the control whose operation is pending; keep the working snapshot visible on errors.

- [ ] **Step 4: Implement the approved visual system**

Define CSS variables for obsidian/teal surfaces, mint status, violet frame accent, text hierarchy, borders, radii, and focus ring. Use a CSS grid background and restrained `backdrop-filter` only on controls/library. Add `@media (prefers-reduced-motion: reduce)` that removes status pulse and transitions. Use system fonts only.

- [ ] **Step 5: Verify responsive and static behavior**

Run: `pnpm test && pnpm build`

Expected: all tests pass; build reports `/` as static and creates `out/index.html`.

- [ ] **Step 6: Commit**

```bash
git add app components lib
git commit -m "feat: build Precision Instrument control panel"
```

### Task 8: Security Gates and Documentation

**Files:**
- Modify: `security/audio-policy.test.ts`
- Modify: `security/pack-policy.test.ts`
- Modify: `security/product-policy.test.ts`
- Modify: `README.md`
- Modify: `SECURITY.md`
- Modify: `docs/architecture/trust-boundaries.md`
- Modify: `docs/security/threat-model.md`
- Modify: `.github/workflows/ci.yml` only if the reviewed dependency requires an existing matrix prerequisite.

**Interfaces:**
- Produces: exact reviewed dependency/command/capability/file-set policies and current security documentation.

- [ ] **Step 1: Write failing policy assertions**

Require exact command names, exact capabilities, reviewed direct dependencies/features, no frontend path fields, no browser persistence/network APIs, no Next server features, no raw-input serialization, no pack execution surfaces, and docs containing settings/import/tray boundaries and exclusions.

- [ ] **Step 2: Verify red**

Run: `pnpm test security`

Expected: FAIL on missing final documentation/policy phrases.

- [ ] **Step 3: Update documentation and policies**

Document the three persisted settings, Rust-only picker path, atomic activation behavior, close-to-tray lifecycle, permissions rationale, sanitized failures, and excluded autostart/network/update/platform hooks. Keep dependency and file allowlists exact rather than prefix-based.

- [ ] **Step 4: Run the complete automated gate**

```bash
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

Expected: every command exits zero, all frontend/Rust tests pass, and `out/index.html` exists.

- [ ] **Step 5: Commit**

```bash
git add README.md SECURITY.md docs security .github/workflows/ci.yml
git commit -m "docs: codify product runtime security boundary"
```

### Task 9: Manual Acceptance, Review, and Pull Request

**Files:**
- Modify only files required by findings, with a failing regression test first for every behavior fix.

**Interfaces:**
- Produces: reviewed branch and unmerged pull request targeting `main`.

- [ ] **Step 1: Launch and exercise the product**

Run: `pnpm tauri dev`

Verify launch, Precision Instrument layout, permission guidance, live enable/volume, native import cancellation, valid import, invalid import, pack selection, close-to-tray, tray toggle, tray show, tray quit, and restart persistence. Confirm the previous pack continues after a failed activation.

- [ ] **Step 2: Perform an independent milestone review**

Review the entire diff against the spec, `AGENTS.md`, `SECURITY.md`, trust boundaries, and threat model. Classify Critical/Important/Minor findings. Specifically inspect raw-input leakage, path leakage, settings corruption, non-atomic activation, lock/callback hazards, dialog capability exposure, tray quit races, accidental networking/storage, dependency bloat, static-export incompatibility, accessibility, and Windows/Linux compilation assumptions.

- [ ] **Step 3: Fix all Critical and Important findings with TDD**

For each fix: add the focused failing test, run it and observe failure, implement the minimum correction, rerun the focused test, and create a narrow commit.

- [ ] **Step 4: Repeat the complete gate**

Run the six commands from Task 8 Step 4 and launch `pnpm tauri dev` once more. Do not claim completion if any command or manual acceptance item fails.

- [ ] **Step 5: Push and create the PR without merging**

```bash
git push -u origin codex/product-ui-tray
gh pr create --base main --head codex/product-ui-tray --title "feat: ship product UI, local packs, and tray controls"
```

The PR description must include summary, architecture, security properties, visual direction, tests, plan deviations, excluded functionality, and follow-up milestone “Windows Input Adapter.”
