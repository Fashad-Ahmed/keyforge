# macOS Menu-Bar-First Experience Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make KeyForge’s macOS everyday controls a compact menu-bar panel, with the existing full sound library available on demand and playback continuing after the panel is dismissed.

**Architecture:** Keep the existing single Tauri WebView, Rust runtime, validated settings, and static Next.js UI. Rust owns tray interaction, panel placement and lifecycle, while one narrow typed command lets the UI request only compact, manage, or dismiss presentation; the existing sound commands remain authoritative. Other platforms retain their existing ordinary window and tray behavior.

**Tech Stack:** Tauri 2.11, Rust, Next.js 16 static export, React, TypeScript, Tailwind CSS, Vitest, Rust unit tests.

**Spec:** `docs/superpowers/specs/2026-09-22-menu-bar-first-design.md`

## Global Constraints

- Keep the current static export to `out/`; do not add a Next.js server feature.
- Rust remains authoritative for playback, validated settings, pack selection, panel/window lifecycle, and tray actions.
- Add at most one presentation-only IPC command: a closed enum with `controls`, `manage`, and `dismiss`; it accepts no window label, path, key data, or arbitrary command.
- Do not add Tauri capabilities or permissions, runtime dependencies, application networking, telemetry, analytics, keyboard hooks, or pack execution.
- Raw keyboard events, key codes, typed content, filesystem paths, decoded audio, and device details never cross into frontend IPC.
- Apply menu-bar popover behavior only on macOS. Keep Windows and Linux tray/window behavior unchanged.
- Do not hide the Dock icon, add login-at-startup behavior, or include DMG packaging, code signing, notarization, updater, or release workflow work.
- Keep each task focused, use a failing test before implementation, run that task’s verification, and create one descriptive commit per completed task.
- Before frontend code changes, read the relevant installed Next.js guide under `node_modules/next/dist/docs/` as required by `AGENTS.md`.

## Review Focus

- **First launch with Input Monitoring denied:** keep the user-facing status visible instead of leaving the user with only an unexplained menu-bar icon; pin startup selection to the sanitized runtime status in Task 2.
- **Retina and secondary-display tray coordinates:** anchor and clamp the panel using physical coordinates on the monitor containing the icon; cover negative monitor origins and screen edges in Task 1 and manually inspect macOS placement in Task 4.
- **Dismissal while changing modes:** outside click/Escape dismisses only compact controls, never quits or disables audio; preserve manager state and do not dismiss during a native import dialog; pin transitions in Tasks 1–3.
- **Tray or window API failure:** playback and settings must remain unchanged, and a visible recoverable window remains available; pin the controller failure behavior in Task 2.
- **Non-macOS regressions:** click-to-open popover semantics must not compile into or change Windows/Linux behavior; keep platform-specific wiring behind `cfg(target_os = "macos")` and retain all three Rust CI targets in Task 4.

## File Map

- `src-tauri/src/panel.rs` — pure panel mode transitions and physical-coordinate placement, independently unit-tested.
- `src-tauri/src/commands/panel.rs` — the narrow typed Tauri presentation command and sanitized failure result.
- `src-tauri/src/commands/mod.rs` — register the panel command module.
- `src-tauri/src/tray.rs` — macOS left-click panel toggle; keep native enable/manage/quit menu actions.
- `src-tauri/src/lib.rs` — manage panel state, route window events, configure startup visibility, and register the command.
- `src-tauri/tauri.conf.json` — hidden/unadorned initial panel configuration, with Rust explicitly showing the ordinary window where required.
- `components/app-shell.tsx` — choose compact controls versus the existing manage/library view and call the presentation API.
- `components/keyforge/compact-controls.tsx` — compact accessible status, enabled switch, smooth volume input, selected sound control, and Manage sounds action.
- `components/keyforge/playback-controls.tsx` — optional class hook to reuse the existing debounced, accessible playback controls in the compact layout.
- `components/app-shell.test.tsx`, `components/keyforge/compact-controls.test.tsx`, and `components/keyforge/playback-controls.tsx` — native API mocking, user-visible mode/control tests, and reusable compact control styling.
- `lib/native/api.ts` and `lib/native/api.test.ts` — typed `setPanelPresentation` wrapper and exact IPC contract tests.
- `app/globals.css` — compact panel-specific layout and states without changing the broader responsive manage layout.
- `README.md`, `SECURITY.md`, `docs/architecture/trust-boundaries.md` — document the Mac panel lifecycle, presentation command, and unchanged trust boundary.
- `src-tauri/capabilities/main.json` — must remain unchanged; no new permission is needed.

---

### Task 1: Define and Test Panel Geometry

**Files:**
- Create: `src-tauri/src/panel.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/panel.rs`

**Interfaces:**
- Produces `ScreenRect`, `PanelSize`, `PhysicalPoint`, and pure `panel_origin` geometry used by Task 2.
- All input/output coordinates are physical screen pixels. Tauri window-size conversion remains in Task 2’s native host.

- [ ] **Step 1: Write failing geometry tests.** Add a test for positioning below an icon on a single display:

```rust
#[test]
fn panel_origin_aligns_below_the_tray_icon() {
    let icon = ScreenRect::new(1380, 18, 24, 24);
    let monitor = ScreenRect::new(0, 0, 1440, 900);
    let size = PanelSize::new(380, 460);
    assert_eq!(panel_origin(icon, monitor, size, 6), PhysicalPoint::new(1024, 48));
}
```

- [ ] **Step 2: Run the geometry test and confirm it fails because the helper does not exist.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml panel::tests::panel_origin_aligns_below_the_tray_icon`

Expected: FAIL because `panel_origin` and its geometry types are not defined.

- [ ] **Step 3: Add the smallest pure geometry types.** Define `ScreenRect { x: i32, y: i32, width: u32, height: u32 }`, `PanelSize { width: u32, height: u32 }`, and `PhysicalPoint { x: i32, y: i32 }` in `src-tauri/src/panel.rs`; add `mod panel;` in `lib.rs`. Do not call Tauri APIs in this module.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScreenRect { pub(crate) x: i32, pub(crate) y: i32, pub(crate) width: u32, pub(crate) height: u32 }
```

- [ ] **Step 4: Add failing coordinate edge tests before writing the placement helper.** Cover left and right edge clamping, bottom edge clamping, a monitor with a negative x-origin, and a panel larger than the monitor. Keep the exact expected point in each assertion.

```rust
assert_eq!(panel_origin(ScreenRect::new(0, 18, 24, 24), ScreenRect::new(0, 0, 1440, 900), PanelSize::new(380, 460), 6), PhysicalPoint::new(0, 48));
assert_eq!(panel_origin(ScreenRect::new(1416, 18, 24, 24), ScreenRect::new(0, 0, 1440, 900), PanelSize::new(380, 460), 6), PhysicalPoint::new(1060, 48));
assert_eq!(panel_origin(ScreenRect::new(700, 870, 24, 20), ScreenRect::new(0, 0, 1440, 900), PanelSize::new(380, 460), 6), PhysicalPoint::new(344, 440));
assert_eq!(panel_origin(ScreenRect::new(-1200, 18, 24, 24), ScreenRect::new(-1600, 0, 1200, 900), PanelSize::new(380, 460), 6), PhysicalPoint::new(-1556, 48));
assert_eq!(panel_origin(ScreenRect::new(20, 18, 24, 24), ScreenRect::new(0, 0, 100, 80), PanelSize::new(380, 460), 6), PhysicalPoint::new(0, 0));
```

- [ ] **Step 5: Run the coordinate test and confirm it fails before the helper exists.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml panel::tests::panel_origin`

Expected: FAIL because `panel_origin` is not defined.

- [ ] **Step 6: Implement clamped physical geometry.** Use signed 64-bit intermediates for right/bottom arithmetic; align the panel’s right edge with the tray icon’s right edge; place it below the icon by the supplied gap; clamp each axis to the monitor bounds. If the panel is larger than a monitor dimension, anchor that axis to the monitor origin rather than overflowing or panicking.

```rust
pub(crate) fn panel_origin(
    icon: ScreenRect,
    monitor: ScreenRect,
    panel: PanelSize,
    gap: i32,
) -> PhysicalPoint
```

- [ ] **Step 7: Run all geometry tests and Rust formatting.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml panel::tests`

Expected: PASS for alignment, edge clamping, negative monitor origins, and oversized-panel cases.

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Expected: PASS.

- [ ] **Step 8: Commit Task 1.**

```bash
git add src-tauri/src/panel.rs src-tauri/src/lib.rs
git commit -m "feat: model menu bar panel presentation"
```

---

### Task 2: Connect the Mac Tray, Window Lifecycle, and Narrow IPC

**Files:**
- Create: `src-tauri/src/commands/panel.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/panel.rs`
- Modify: `src-tauri/src/tray.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tauri.conf.json`
- Test: `src-tauri/src/panel.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/commands/panel.rs`

**Interfaces:**
- Consumes Task 1’s panel state and physical geometry.
- Produces `PanelView::{Controls, Manage}`, `PanelRequest::{Controls, Manage, Dismiss}`, `PanelEffect::{Show(PanelView), Hide, None}`, `PanelState`, a private `PanelHost` trait, and `#[tauri::command] set_panel_presentation(request: PanelRequest, ...) -> Result<(), PanelCommandError>`; `PanelCommandError` has only a sanitized `Unavailable` variant.
- `PanelRequest` deserializes only the exact snake-case strings `controls`, `manage`, and `dismiss`; `PanelState::default()` remembers Controls.
- The command is bound to Tauri’s injected `main` window and `PanelState`. It does not accept a window label, URL, position, path, duration, or arbitrary JavaScript.
- `PanelState` stores the last tray icon rectangle and its containing monitor only in memory; a Manage request reuses that Rust-owned anchor when recalculating size/position.
- Native transition helper signature: `dispatch_panel_request(state: &mut PanelState, host: &mut impl PanelHost, request: PanelRequest) -> Result<PanelEffect, PanelCommandError>`.
- Tray actions route through `PanelState` and one private panel host; enable/disable continues to use `KeyForgeRuntime::set_enabled`.

- [ ] **Step 1: Write failing dispatch and startup tests using a fake panel host.** Define `FakePanelHost { visible: bool, fail_next: bool, last_view: Option<PanelView>, last_position: Option<PhysicalPoint> }` and a `PanelHost` test implementation. Test Controls, Manage, Dismiss preserving the last view, tray-click toggle, focus loss in both modes, reuse of the latest anchor when changing from Controls to Manage, and a failed show preserving the current view. Add `should_show_window_on_startup(is_macos, input_ready, audio_ready, tray_available) -> bool`: macOS with every service ready returns false; macOS with denied/unavailable input, unavailable audio, or unavailable tray returns true; non-macOS returns true.

```rust
#[derive(Default)]
struct FakePanelHost {
    visible: bool,
    fail_next: bool,
    last_view: Option<PanelView>,
    last_position: Option<PhysicalPoint>,
}

trait PanelHost {
    fn is_visible(&self) -> Result<bool, PanelCommandError>;
    fn present(&mut self, view: PanelView) -> Result<(), PanelCommandError>;
    fn dismiss(&mut self) -> Result<(), PanelCommandError>;
}

#[test]
fn panel_request_deserialization_accepts_only_the_closed_values() {
    assert_eq!(serde_json::from_str::<PanelRequest>("\"manage\""), Ok(PanelRequest::Manage));
    assert!(serde_json::from_str::<PanelRequest>("\"open_url\"").is_err());
}

#[test]
fn failed_show_does_not_commit_the_new_panel_view() {
    let mut state = PanelState::default();
    let mut host = FakePanelHost { visible: false, fail_next: true, last_view: None, last_position: None };
    assert_eq!(dispatch_panel_request(&mut state, &mut host, PanelRequest::Manage), Err(PanelCommandError::Unavailable));
    assert_eq!(state.view(), PanelView::Controls);
}
```

- [ ] **Step 2: Run the new dispatch test and verify it fails before the state machine/host exists.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml panel::tests::failed_show_does_not_commit_the_new_panel_view`

Expected: FAIL because the transition dispatcher, fake-host contract, and startup helper do not exist.

- [ ] **Step 3: Implement the panel request state machine and private native host.** Commit `PanelState` to a new view only after the host successfully applies that view. On each macOS tray click, derive and save a private anchor from the tray icon’s physical rectangle and its containing monitor’s physical origin/size; never accept geometry from JavaScript. Use `LogicalSize::new(360.0, 420.0)` for Controls and `LogicalSize::new(760.0, 800.0)` for Manage, scaling logical dimensions for physical placement. A Manage request repositions from the stored anchor using the new size. Clamp against the icon’s monitor, including non-primary monitors; if no monitor contains the tray rectangle or monitor enumeration fails, use the primary monitor and the same tested clamp helper. When startup needs to show Controls before a tray click, anchor at the primary monitor’s menu-bar edge. If no monitor bounds are available, show the recoverable manager at the platform’s default window position. Do not add an external geometry or windowing dependency.

```rust
let controls_size = LogicalSize::new(360.0, 420.0);
let manage_size = LogicalSize::new(760.0, 800.0);
```

- [ ] **Step 4: Implement presentation requests and sanitized failure handling.** On Controls/Manage, apply the matching size and position, make Controls undecorated, non-resizable, and always-on-top only while visible; make Manage use ordinary window decorations/resizing and clear always-on-top. On Dismiss, hide it and clear always-on-top. Return `Unavailable` if an operation fails; never mutate audio state or settings in this command. On non-macOS, the presentation command returns `Unavailable` without touching the window; the existing full UI never calls it.

- [ ] **Step 5: Route macOS tray clicks through the panel state.** On macOS set `show_menu_on_left_click(false)` and use only the left-button-up tray event to update the anchor and toggle the panel. Keep the native context menu’s enable/disable, Show KeyForge (opens Manage), and Quit items. Leave tray behavior on Linux and Windows unchanged using compile-time platform gates.

- [ ] **Step 6: Add native window lifecycle and startup behavior.** Dismiss a compact panel on focus loss; leave the manager open on focus loss; treat close as hide; keep explicit Quit as the only normal exit path. Configure the initial window hidden and undecorated, with a minimum size that permits the 360×420 Controls view. Explicitly restore ordinary decorations/resizing and show it during setup on non-macOS. On macOS, show compact controls during setup when input status is not Ready, audio is unavailable, or tray construction failed; otherwise remain in the menu bar. Do not persist onboarding history or add a new setting.

- [ ] **Step 7: Register the command and verify the capability file is unchanged.** Add only `set_panel_presentation` to the existing explicit command handler. The command accepts only the closed enum `{controls, manage, dismiss}`; on non-macOS it returns `Unavailable`. Do not change `src-tauri/capabilities/main.json`; do not add Tauri plugin permissions or frontend access to window-management APIs.

- [ ] **Step 8: Run focused Rust verification.**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Expected: PASS.

Run: `cargo test --manifest-path src-tauri/Cargo.toml panel::tests tray::tests commands::panel::tests`

Expected: PASS, including command argument and sanitized failure tests.

Run: `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: PASS with no new warnings.

- [ ] **Step 9: Commit Task 2.**

```bash
git add src-tauri/src/commands/panel.rs src-tauri/src/commands/mod.rs src-tauri/src/panel.rs src-tauri/src/tray.rs src-tauri/src/lib.rs src-tauri/tauri.conf.json
git commit -m "feat: open compact controls from mac menu bar"
```

---

### Task 3: Build and Test the Compact Frontend Controls

**Files:**
- Create: `components/keyforge/compact-controls.tsx`
- Create: `components/keyforge/compact-controls.test.tsx`
- Modify: `components/app-shell.tsx`
- Modify: `components/app-shell.test.tsx`
- Modify: `lib/native/api.ts`
- Modify: `lib/native/api.test.ts`
- Modify: `app/globals.css`

**Interfaces:**
- Consumes the current `RuntimeStatus`, pack summaries, `PlaybackControls`, and sound mutation callbacks.
- Produces `setPanelPresentation(request: "controls" | "manage" | "dismiss"): Promise<void>` in `lib/native/api.ts` and a `CompactControls` component with typed props for status, packs, pending action, callbacks, and manager navigation.
- The compact control calls only existing playback/pack commands plus the presentation command. It does not read keyboard input or browser storage.

- [ ] **Step 1: Write failing API contract tests.** Verify `setPanelPresentation("manage")` invokes exactly `set_panel_presentation` with `{ request: "manage" }`, and that a native rejection remains a rejection for the caller to handle with sanitized UI copy.

```ts
it("requests only the named manager presentation", async () => {
  await setPanelPresentation("manage");
  expect(invokeMock).toHaveBeenCalledWith("set_panel_presentation", { request: "manage" });
});
```

- [ ] **Step 2: Run the API test and confirm the wrapper is missing.**

Run: `pnpm test -- lib/native/api.test.ts`

Expected: FAIL because `setPanelPresentation` is not exported.

- [ ] **Step 3: Add the presentation API wrapper and make its contract test pass.** Define the request union in TypeScript and add no other native wrapper.

```ts
export type PanelPresentationRequest = "controls" | "manage" | "dismiss";
export function setPanelPresentation(request: PanelPresentationRequest): Promise<void> {
  return invoke<void>("set_panel_presentation", { request });
}
```

- [ ] **Step 4: Write failing compact-control component tests.** Define a `propsForTest(overrides)` fixture returning a ready `RuntimeStatus`, the `keyforge-mechanical` and `quiet-linear` `PackSummary` values, `pendingAction: null`, and `vi.fn()` callbacks for `onEnabledChange`, `onVolumeChange`, `onSelectPack`, and `onManageSounds`. Test accessible names and values for the enabled switch and volume slider; selected sound name and pack-choice control; Manage sounds action; and each callback. Verify unavailable status and an empty pack list render safe explanatory text rather than crashing.

```tsx
it("shows the selected sound and requests the selected local pack", async () => {
  const props = propsForTest();
  render(<CompactControls {...props} />);
  fireEvent.change(screen.getByRole("combobox", { name: "Sound profile" }), {
    target: { value: "quiet-linear" },
  });
  expect(props.onSelectPack).toHaveBeenCalledWith("quiet-linear");
});
```

- [ ] **Step 5: Run the component test and verify it fails because the component does not exist.**

Run: `pnpm test -- components/keyforge/compact-controls.test.tsx`

Expected: FAIL because `CompactControls` is not defined.

- [ ] **Step 6: Implement the smallest compact component.** Reuse `PlaybackControls` for the enable switch and existing debounced slider; add a labeled native `<select>` for validated installed pack IDs; show sanitized status and Manage sounds; accept callbacks rather than calling IPC inside the component.

```ts
type CompactControlsProps = {
  status?: RuntimeStatus;
  packs: PackSummary[];
  pendingAction: string | null;
  onEnabledChange: (enabled: boolean) => void;
  onVolumeChange: (volume: number) => void;
  onSelectPack: (packId: string) => void;
  onManageSounds: () => void;
};
```

- [ ] **Step 7: Add AppShell mode transitions and mode-specific rendering.** On macOS render compact controls by default; Manage sounds displays the existing full library/import UI. On non-macOS retain the existing full UI. Await `setPanelPresentation` before changing the frontend mode; display one sanitized message and keep the current view if native resize fails. Add an Escape handler only while compact controls are visible; it requests `dismiss` and does not observe any other key.

- [ ] **Step 8: Write and run AppShell transition tests.** Verify compact initial controls on macOS, Manage sounds reveals the existing library, Back to controls restores compact mode, Escape calls only `dismiss`, failed presentation keeps the current view, non-macOS remains on the current full UI, and no raw/native key information appears in component props or snapshots.

Run: `pnpm test -- components/app-shell.test.tsx components/keyforge/compact-controls.test.tsx lib/native/api.test.ts`

Expected: PASS.

- [ ] **Step 9: Style and verify compact-only layout.** Add a 360-pixel-wide control surface with small direct hierarchy, comfortable hit targets, no oversized heading/grid or nested cards, and the existing smooth continuous volume affordance. Keep focus rings, reduced-motion behavior, sufficient contrast, and the full manager’s existing responsive rules.

- [ ] **Step 10: Run frontend checks and commit Task 3.**

Run: `pnpm test`

Expected: PASS for all frontend tests.

Run: `pnpm build`

Expected: PASS and emit static `out/` assets with no server routes.

```bash
git add components/app-shell.tsx components/app-shell.test.tsx components/keyforge/compact-controls.tsx components/keyforge/compact-controls.test.tsx lib/native/api.ts lib/native/api.test.ts app/globals.css
git commit -m "feat: add compact menu bar sound controls"
```

---

### Task 4: Document the Boundary and Complete Mac Acceptance

**Files:**
- Modify: `README.md`
- Modify: `SECURITY.md`
- Modify: `docs/architecture/trust-boundaries.md`
- Test: all existing frontend and Rust tests; `.github/workflows/ci.yml` cross-platform jobs

**Interfaces:**
- Consumes the final panel command name and lifecycle semantics from Tasks 2–3.
- Produces consistent user/security documentation, a verified full branch, and manual macOS acceptance evidence.

- [ ] **Step 1: Update the product documentation.** Say that macOS opens compact controls from the menu-bar icon; the library/import UI is on demand; closing the panel does not stop playback; and Windows/Linux retain their current window/tray UX. List `set_panel_presentation` as presentation-only IPC and explicitly state it carries no keyboard, path, or audio data.

- [ ] **Step 2: Review the complete diff against `AGENTS.md`, `SECURITY.md`, the trust-boundary document, and the approved design spec.** Confirm no new permission, dependency, network access, telemetry, browser storage, raw-input path, hidden startup setting, or DMG/release change slipped in. Confirm `main.json` is byte-for-byte unchanged from the base branch.

- [ ] **Step 3: Run complete repository verification.**

Run: `pnpm test`

Expected: PASS.

Run: `pnpm build`

Expected: PASS with static export.

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Expected: PASS.

Run: `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: PASS.

Run: `cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets`

Expected: PASS.

- [ ] **Step 4: Launch on macOS and complete the manual acceptance checklist.** Run `pnpm tauri dev`. Verify first launch with denied input permission shows a usable compact panel and sanitized permission status; a ready installation starts unobtrusively; one menu-bar click opens next to the icon on built-in and secondary Retina displays; the panel stays on-screen while adjusting controls; outside click and Escape dismiss without stopping audio; changing sound updates the active pack; Manage sounds opens the library/import UI and survives native file-picker focus; right-click menu actions still work; Quit exits; and tray/window failures leave a recoverable visible window without changing playback.

- [ ] **Step 5: Inspect CI results for macOS, Windows, and Linux.** Do not claim non-macOS behavior is preserved until those jobs pass. Resolve any failing check before proceeding.

- [ ] **Step 6: Commit the documentation and final acceptance updates.**

```bash
git add README.md SECURITY.md docs/architecture/trust-boundaries.md
git commit -m "docs: explain mac menu bar controls"
```

## Completion Criteria

- The compact Mac panel is the primary everyday surface; the existing manager is optional.
- Tray click, focus loss, Escape, explicit Quit, first-run permission state, and error recovery follow the state rules above.
- Frontend and native tests pass; static build succeeds; all Rust quality gates and three-platform CI pass.
- Manual macOS acceptance is recorded. If local macOS UI/display verification is unavailable, report it as a remaining release blocker rather than claiming completion.
- No DMG is produced in this milestone. A later distribution milestone must handle Developer ID signing, notarization credentials, universal architecture strategy, Gatekeeper verification, and GitHub release artifacts.
