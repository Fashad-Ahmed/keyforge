# KeyForge macOS Menu-Bar-First Experience

**Date:** 2026-09-22
**Status:** Approved for implementation planning
**Scope:** macOS control-surface redesign only

## Intent

KeyForge should feel like a small, useful menu-bar utility rather than a full desktop window users must keep open. The everyday path should let a Mac user open the menu-bar control, toggle sound, adjust volume, choose a sound, and return to typing. Playback must continue when the control surface is dismissed.

The existing full library/import UI remains available only when a user chooses **Manage sounds**. A signed/notarized downloadable DMG and GitHub release presentation are separate follow-up milestones; this design does not claim that the current application is yet ready for broad Mac distribution.

## User Experience

- KeyForge continues running in the background after its control surface is dismissed.
- Clicking the KeyForge menu-bar icon opens a compact control panel anchored near the icon.
- The compact panel presents, in order: engine/input status, one obvious enable toggle, a continuous volume slider with numeric feedback, the selected sound with a way to change it, and a **Manage sounds** action.
- The volume control must use a continuous native-backed value, support pointer, keyboard, and assistive-technology input, and update without a visible stepped/jittery effect. Values remain validated and persisted by Rust.
- Selecting a sound changes the active sound pack through the existing validated native command path. Raw keyboard events, key codes, and typed content remain entirely outside the panel and IPC.
- **Manage sounds** changes the same app window into the existing larger library/import experience. Returning to controls restores the compact presentation. This avoids introducing a second frontend window or a second settings authority.
- Clicking outside the compact panel or pressing Escape dismisses it without disabling sound or quitting KeyForge.
- The existing native context menu remains available for essential actions, including enable/disable, show/manage, and quit. Quit remains explicit and terminates the background runtime.
- The app may remain visible in the Dock; hiding the Dock icon is not part of this design. The menu-bar control, not Dock presence, becomes the primary day-to-day interaction.
- On non-macOS platforms, retain the current standard tray menu and ordinary application window behavior. Do not claim that every OS supports the same click-to-open panel semantics.

## Approaches Considered

1. **Native menu items only.** Lowest UI and WebView complexity, but cannot provide the smooth volume control, clear sound selection, or branded presentation the user wants.
2. **Tauri tray icon plus a compact anchored Tauri window (recommended).** Reuses the static Next.js/Tailwind interface and existing Rust commands while keeping macOS presentation compact. Tauri provides the tray-icon rectangle needed to place the panel; the implementation must verify physical/logical coordinates, display scaling, and screen-edge clamping on real macOS hardware.
3. **Native AppKit status item and popover.** Closest to native macOS conventions, but duplicates frontend presentation or requires a larger platform-specific bridge, diverging from the existing Tauri UI and making future cross-platform behavior less consistent.

## Architecture and Boundaries

- Keep the existing Tauri tray integration and explicit Rust runtime as the authorities for runtime state, audio, pack activation, persistence, lifecycle, and quitting.
- On macOS, configure the tray icon click to open the existing `main` WebView as a compact panel positioned from the tray icon bounds. Disable the default left-click native-menu behavior only where needed for this interaction; keep the context menu for alternate and essential actions.
- Reuse the existing static frontend and existing IPC commands for status, enablement, volume, pack listing/selection, and import. Add no command unless implementation proves an existing command cannot express a required, coarse action; any new command must be narrowly typed, documented, and tested.
- Use an explicit frontend presentation state for `compact-controls` and `manage-sounds`. The expanded view reuses the existing WebView and command surface rather than creating a second window or introducing a production web server.
- Rust owns show/hide, positioning, focus/lifecycle policy, settings writes, and native-menu actions. JavaScript requests only named operations and never receives a filesystem path, native input event, key code, or audio device detail.
- Preserve least-privilege Tauri capabilities. The panel does not need new filesystem, shell, network, or global-shortcut permissions.
- Coordinate conversion, display placement, focus dismissal, and menu behavior must be isolated behind small native helpers so they can be unit-tested without asserting desktop geometry in ordinary CI.

## Error and Lifecycle Behavior

- A tray-icon or positioning failure must not stop keyboard sound playback. Retain a recoverable way to show the ordinary application window, and report a sanitized degraded status.
- A failed panel show/hide operation must not change playback state or persisted settings.
- Closing/dismissing controls is not a quit request. The explicit native **Quit** action follows the existing lifecycle path and stops the runtime once.
- On app startup, show the panel only when required by first-run permission/onboarding UX; otherwise start unobtrusively in the menu bar. The implementation plan must state the exact first-launch behavior after checking the current macOS permission flow.
- During pack selection or import, do not dismiss the manager in a way that loses progress or the user’s place. Cancellation remains harmless and native failures remain sanitized.

## Visual and Interaction Direction

- The compact control should feel unmistakably like KeyForge: a small precision instrument for sound, not a shrunken copy of the full dashboard.
- Keep the hierarchy direct and use the current brand language selectively: dark instrument surface, clear mint runtime feedback, restrained accent color, readable control labels, and a custom-drawn or carefully styled smooth slider.
- Do not stack multiple cards or leave large empty grid regions in the compact panel. At compact size, show only controls used often; library management belongs in the expanded view.
- Respect reduced-motion settings, provide visible keyboard focus, accessible labels, and sufficient contrast. Motion should be limited to feedback for state changes.

## Testing and Acceptance

### Automated

- Rust tests cover tray click intent, show/hide state transitions, coordinate conversion/clamping, preservation of runtime state on window-operation failure, and explicit quit behavior.
- Frontend tests cover compact/manage view transitions, control values and labels, slider keyboard/accessibility behavior, and sanitized command calls.
- Existing security tests continue to prove no raw input or paths cross IPC, no new capabilities/networking/telemetry are introduced, and static export remains in use.
- Run the repository’s complete frontend and Rust verification suite plus macOS CI compilation.

### Manual macOS acceptance

- Launch from a clean install and complete the real macOS input permission flow.
- Verify a single click opens a compact panel adjacent to the menu-bar icon on supported display scales and monitor arrangements.
- Verify an outside click and Escape dismiss only the panel, and playback continues.
- Verify the enable toggle, smoothly changing volume, sound selection, and Manage sounds all reflect the Rust-owned runtime state.
- Verify the native context menu still provides a reliable route to controls and explicit Quit.
- Confirm the Dock behavior matches this design and that no new permission prompt is introduced by the menu-bar interaction itself.

## Explicit Exclusions

- Windows or Linux input adapters or platform-specific popovers.
- DMG production, code signing, notarization, auto-update, or release workflow changes.
- New keyboard hooks, raw-input IPC, telemetry, analytics, accounts, app networking, or additional Tauri permissions.
- Sound engine changes, pack format changes, pack execution, or sound content changes.
- Hiding the Dock icon or adding login-at-startup behavior.

## Open Implementation Decision

Determine how Tauri's macOS tray event geometry maps to window coordinates at Retina scale and across multiple displays. If the required panel anchoring cannot be made stable with the existing Tauri APIs, pause before substituting a native AppKit bridge; document the measured limitation and present the tradeoff for review.
