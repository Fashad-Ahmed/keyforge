# KeyForge Milestone 1 — Secure Desktop Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a secure, testable Tauri 2 + Next.js + TypeScript desktop foundation that launches successfully, uses static export, exposes only an explicit Rust command surface, and contains no keyboard-capture or networking behavior yet.

**Architecture:** Next.js is a static-exported presentation layer loaded by Tauri. All native capabilities live in Rust behind narrowly-scoped Tauri commands. Raw OS input events will never be exposed to the frontend in later milestones; this milestone establishes that boundary before input or audio code exists.

**Tech Stack:** Tauri 2, Rust stable, Next.js, TypeScript, Tailwind CSS, pnpm, Vitest, React Testing Library, Cargo test, GitHub Actions.

## Global Constraints

- Working codename: `KeyForge`.
- Package manager: `pnpm`.
- Next.js must use `output: "export"`.
- Tauri `frontendDist` must be `../out`.
- No SSR, Server Actions, API routes, middleware, or runtime Next.js server.
- No telemetry, analytics SDK, account system, cloud service, or application networking.
- No global keyboard hooks in Milestone 1.
- No arbitrary plugin/script execution.
- Frontend may call only explicitly registered Tauri commands.
- Tauri capabilities must be least-privilege and scoped to the `main` window.
- Rust and JavaScript dependency lockfiles must be committed.
- All production code must be covered by automated tests appropriate to its layer.
- CI must run frontend tests, frontend build, Rust formatting, Rust linting, and Rust tests.
- Do not add dependencies unless directly required by this milestone.

---

## Repository Structure

```text
keyforge/
├── AGENTS.md
├── README.md
├── SECURITY.md
├── package.json
├── pnpm-lock.yaml
├── next.config.ts
├── tsconfig.json
├── vitest.config.ts
├── app/
│   ├── globals.css
│   ├── layout.tsx
│   └── page.tsx
├── components/
│   └── app-shell.tsx
├── lib/
│   ├── native/
│   │   ├── api.ts
│   │   └── api.test.ts
│   └── types/
│       └── app-info.ts
├── src-tauri/
│   ├── Cargo.lock
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── main.json
│   └── src/
│       ├── commands/
│       │   ├── app_info.rs
│       │   └── mod.rs
│       ├── lib.rs
│       └── main.rs
├── docs/
│   ├── architecture/
│   │   └── trust-boundaries.md
│   ├── product/
│   │   └── v1-scope.md
│   ├── security/
│   │   └── threat-model.md
│   └── superpowers/
│       └── plans/
│           └── 2026-08-18-secure-desktop-foundation.md
└── .github/
    └── workflows/
        └── ci.yml
```

## Interfaces Locked by This Plan

### Rust → frontend

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub platform: String,
}

#[tauri::command]
pub fn get_app_info() -> AppInfo;
```

### TypeScript

```ts
export type AppInfo = {
  name: string;
  version: string;
  platform: string;
};

export async function getAppInfo(): Promise<AppInfo>;
```

No other native command is exposed in Milestone 1.

---

### Task 1: Bootstrap the Tauri + Next.js Repository

**Files:**
- Create: entire initial repository
- Create: `next.config.ts`
- Create: `src-tauri/tauri.conf.json`
- Create: `.gitignore`

**Interfaces:**
- Consumes: none
- Produces: a Tauri 2 application whose frontend is a Next.js static export in `out/`

- [ ] **Step 1: Create the Next.js application**

Run:

```bash
pnpm create next-app@latest keyforge \
  --ts \
  --tailwind \
  --eslint \
  --app \
  --no-src-dir \
  --import-alias "@/*"

cd keyforge
```

When the CLI asks any question not represented by flags, keep the default except do not enable experimental features.

- [ ] **Step 2: Add Tauri 2**

Run:

```bash
pnpm add -D @tauri-apps/cli
pnpm tauri init
```

Use these values when prompted:

```text
App name: KeyForge
Window title: KeyForge
Web assets location: ../out
Dev server URL: http://localhost:3000
Frontend dev command: pnpm dev
Frontend build command: pnpm build
```

- [ ] **Step 3: Configure Next.js for static export**

Replace `next.config.ts` with:

```ts
import type { NextConfig } from "next";

const isProd = process.env.NODE_ENV === "production";
const internalHost = process.env.TAURI_DEV_HOST ?? "localhost";

const nextConfig: NextConfig = {
  output: "export",
  images: {
    unoptimized: true,
  },
  assetPrefix: isProd ? undefined : `http://${internalHost}:3000`,
};

export default nextConfig;
```

- [ ] **Step 4: Verify Tauri build configuration**

Ensure `src-tauri/tauri.conf.json` contains:

```json
{
  "build": {
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build",
    "devUrl": "http://localhost:3000",
    "frontendDist": "../out"
  }
}
```

Keep the remaining generated Tauri configuration unless it conflicts with a global constraint.

- [ ] **Step 5: Build the static frontend**

Run:

```bash
pnpm build
```

Expected:

```text
Exit code 0
out/index.html exists
```

Verify:

```bash
test -f out/index.html
```

- [ ] **Step 6: Validate Rust**

Run:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: exit code `0`.

- [ ] **Step 7: Commit**

```bash
git add .
git commit -m "chore: bootstrap Tauri Next.js desktop app"
```

---

### Task 2: Establish the Native Trust Boundary

**Files:**
- Create: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/commands/app_info.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `lib/types/app-info.ts`
- Create: `lib/native/api.ts`

**Interfaces:**
- Consumes: Tauri application from Task 1
- Produces: `get_app_info()` / `getAppInfo()` as the only native command exposed to the frontend

- [ ] **Step 1: Write the Rust unit test first**

Create `src-tauri/src/commands/app_info.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_has_expected_identity() {
        let info = build_app_info();
        assert_eq!(info.name, "KeyForge");
        assert!(!info.version.is_empty());
        assert!(!info.platform.is_empty());
    }
}
```

- [ ] **Step 2: Verify the Rust test fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml app_info_has_expected_identity
```

Expected: compilation failure because `build_app_info` is not defined.

- [ ] **Step 3: Implement the minimum Rust command**

Replace `src-tauri/src/commands/app_info.rs` with:

```rust
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub platform: String,
}

fn build_app_info() -> AppInfo {
    AppInfo {
        name: "KeyForge".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
    }
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    build_app_info()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_has_expected_identity() {
        let info = build_app_info();
        assert_eq!(info.name, "KeyForge");
        assert!(!info.version.is_empty());
        assert!(!info.platform.is_empty());
    }
}
```

Create `src-tauri/src/commands/mod.rs`:

```rust
pub mod app_info;
```

Update the Tauri builder in `src-tauri/src/lib.rs` so the only custom invoke command is:

```rust
.invoke_handler(tauri::generate_handler![
    commands::app_info::get_app_info
])
```

and declare:

```rust
mod commands;
```

- [ ] **Step 4: Run Rust tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all tests pass.

- [ ] **Step 5: Add the TypeScript contract**

Create `lib/types/app-info.ts`:

```ts
export type AppInfo = {
  name: string;
  version: string;
  platform: string;
};
```

Create `lib/native/api.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "@/lib/types/app-info";

export async function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}
```

Install the frontend API:

```bash
pnpm add @tauri-apps/api
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri lib package.json pnpm-lock.yaml
git commit -m "feat: establish explicit native command boundary"
```

---

### Task 3: Add Least-Privilege Tauri Capabilities

**Files:**
- Create: `src-tauri/capabilities/main.json`
- Modify: `src-tauri/tauri.conf.json`

**Interfaces:**
- Consumes: main Tauri window
- Produces: a single explicitly enabled capability for `main`

- [ ] **Step 1: Create the capability file**

Create `src-tauri/capabilities/main.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "main",
  "description": "Minimum permissions for the KeyForge main desktop window",
  "windows": ["main"],
  "platforms": ["linux", "macOS", "windows"],
  "permissions": ["core:default"]
}
```

- [ ] **Step 2: Explicitly enable only this capability**

In `src-tauri/tauri.conf.json`, set:

```json
{
  "app": {
    "security": {
      "capabilities": ["main"]
    }
  }
}
```

Preserve the rest of the generated `app` configuration.

- [ ] **Step 3: Confirm the application still compiles**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
pnpm build
```

Expected: both commands exit `0`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri
git commit -m "security: restrict desktop capabilities"
```

---

### Task 4: Add Frontend Tests and Minimal App Shell

**Files:**
- Create: `vitest.config.ts`
- Create: `vitest.setup.ts`
- Create: `lib/native/api.test.ts`
- Create: `components/app-shell.tsx`
- Create: `components/app-shell.test.tsx`
- Modify: `app/page.tsx`

**Interfaces:**
- Consumes: `getAppInfo(): Promise<AppInfo>`
- Produces: a minimal status screen proving Next.js ↔ Tauri IPC works

- [ ] **Step 1: Install test dependencies**

```bash
pnpm add -D vitest jsdom @testing-library/react @testing-library/jest-dom @testing-library/user-event
```

Add to `package.json`:

```json
{
  "scripts": {
    "test": "vitest run",
    "test:watch": "vitest"
  }
}
```

- [ ] **Step 2: Configure Vitest**

Create `vitest.config.ts`:

```ts
import { defineConfig } from "vitest/config";
import path from "node:path";

export default defineConfig({
  test: {
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts"],
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "."),
    },
  },
});
```

Create `vitest.setup.ts`:

```ts
import "@testing-library/jest-dom/vitest";
```

- [ ] **Step 3: Test the native API wrapper**

Create `lib/native/api.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

describe("getAppInfo", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("calls only the get_app_info command", async () => {
    invokeMock.mockResolvedValue({
      name: "KeyForge",
      version: "0.1.0",
      platform: "macos",
    });

    const { getAppInfo } = await import("./api");
    const result = await getAppInfo();

    expect(invokeMock).toHaveBeenCalledWith("get_app_info");
    expect(result.name).toBe("KeyForge");
  });
});
```

- [ ] **Step 4: Run the test**

```bash
pnpm test
```

Expected: the API-wrapper test passes.

- [ ] **Step 5: Create the app shell**

Create `components/app-shell.tsx`:

```tsx
"use client";

import { useEffect, useState } from "react";
import { getAppInfo } from "@/lib/native/api";
import type { AppInfo } from "@/lib/types/app-info";

export function AppShell() {
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    void getAppInfo().then(setInfo);
  }, []);

  return (
    <main className="min-h-screen p-8">
      <h1 className="text-3xl font-semibold">KeyForge</h1>
      <p className="mt-2 text-sm opacity-70">
        Privacy-first keyboard sound engine
      </p>

      <section className="mt-8 rounded-xl border p-4">
        <h2 className="font-medium">Native runtime</h2>
        {info ? (
          <dl className="mt-3 space-y-1 text-sm">
            <div>Version: {info.version}</div>
            <div>Platform: {info.platform}</div>
          </dl>
        ) : (
          <p className="mt-3 text-sm">Connecting…</p>
        )}
      </section>
    </main>
  );
}
```

Replace `app/page.tsx` with:

```tsx
import { AppShell } from "@/components/app-shell";

export default function Home() {
  return <AppShell />;
}
```

- [ ] **Step 6: Add the app-shell test**

Create `components/app-shell.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { vi } from "vitest";
import { AppShell } from "./app-shell";

vi.mock("@/lib/native/api", () => ({
  getAppInfo: vi.fn().mockResolvedValue({
    name: "KeyForge",
    version: "0.1.0",
    platform: "macos",
  }),
}));

it("renders native runtime information", async () => {
  render(<AppShell />);

  expect(screen.getByText("KeyForge")).toBeInTheDocument();
  expect(await screen.findByText("Version: 0.1.0")).toBeInTheDocument();
  expect(screen.getByText("Platform: macos")).toBeInTheDocument();
});
```

- [ ] **Step 7: Verify frontend**

```bash
pnpm test
pnpm build
```

Expected: tests and static export pass.

- [ ] **Step 8: Commit**

```bash
git add .
git commit -m "test: add desktop shell contract tests"
```

---

### Task 5: Document Product and Security Boundaries

**Files:**
- Create: `AGENTS.md`
- Create: `SECURITY.md`
- Create: `docs/product/v1-scope.md`
- Create: `docs/architecture/trust-boundaries.md`
- Create: `docs/security/threat-model.md`

**Interfaces:**
- Consumes: approved product design
- Produces: rules Codex and contributors must follow in every later milestone

- [ ] **Step 1: Create `AGENTS.md`**

```markdown
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
```

- [ ] **Step 2: Create `SECURITY.md`**

```markdown
# Security Policy

## Supported Versions

Security fixes are provided for the latest released version.

## Reporting a Vulnerability

Do not open a public issue for suspected vulnerabilities.

Until a private security contact is configured, use GitHub Private Vulnerability Reporting when available for this repository.

Include:
- affected version or commit;
- operating system;
- reproduction steps;
- expected and observed behavior;
- impact;
- proof of concept when safe to provide.

## Security Guarantees We Intend to Preserve

- no storage of typed key content;
- no transmission of typed key content;
- no telemetry;
- no executable sound-pack content;
- least-privilege native capabilities;
- release artifacts built through controlled CI.
```

- [ ] **Step 3: Create `docs/product/v1-scope.md`**

Document exactly:

```markdown
# V1 Scope

## Included

- macOS, Windows, and Linux desktop targets
- system tray
- global keyboard-triggered sound playback
- mechanical, typewriter, terminal, and curated fun sound packs
- randomized normal-key samples
- special samples for Space, Enter, Backspace, and modifiers
- optional key-up sounds
- volume control
- launch at startup
- custom local sound-pack import
- offline operation
- no telemetry
- secure build and release pipeline

## Excluded

- user accounts
- cloud sync
- marketplace
- analytics
- remote pack registry
- arbitrary plugins
- executable sound-pack scripts
- community auto-downloads
```

- [ ] **Step 4: Create `docs/architecture/trust-boundaries.md`**

```markdown
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
```

- [ ] **Step 5: Create `docs/security/threat-model.md`**

```markdown
# Threat Model

## Protected Assets

- user keystroke privacy
- integrity of local sound packs
- integrity of settings
- integrity of application binaries
- integrity of the release pipeline

## Threats

1. accidental key logging
2. malicious code disguised as a sound pack
3. archive path traversal
4. malformed audio causing crashes or resource exhaustion
5. compromised Rust or npm dependency
6. excessive Tauri permissions
7. compromised CI workflow
8. tampered release artifact
9. malicious contributor change
10. future updater or registry compromise

## Initial Mitigations

- raw keyboard events remain in Rust
- frontend receives no raw key data
- V1 has no application networking
- packs will be data-only
- explicit Tauri capabilities
- locked dependencies
- protected review workflow
- automated tests and static analysis

Later milestones must add concrete pack parser limits, dependency review, SBOM, provenance, artifact scanning, and signing.
```

- [ ] **Step 6: Commit**

```bash
git add AGENTS.md SECURITY.md docs
git commit -m "docs: define product and security boundaries"
```

---

### Task 6: Add CI Quality and Security Baseline

**Files:**
- Create: `.github/workflows/ci.yml`
- Modify: `package.json` only if a missing script is required

**Interfaces:**
- Consumes: frontend and Rust tests
- Produces: a required CI workflow for every push and pull request

- [ ] **Step 1: Create CI workflow**

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read

jobs:
  frontend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: pnpm/action-setup@v4
        with:
          run_install: false

      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm

      - run: pnpm install --frozen-lockfile
      - run: pnpm test
      - run: pnpm build

  rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - run: cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
      - run: cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
      - run: cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 2: Run equivalent local checks**

```bash
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all commands exit `0`.

- [ ] **Step 3: Commit**

```bash
git add .github package.json pnpm-lock.yaml
git commit -m "ci: add frontend and Rust quality gates"
```

---

## Milestone 1 Acceptance Test

Run from repository root:

```bash
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri dev
```

The milestone is complete only when:

```text
[PASS] Next.js tests pass
[PASS] Next.js static export builds into out/
[PASS] Rust formatting passes
[PASS] Rust clippy passes with warnings denied
[PASS] Rust tests pass
[PASS] Tauri desktop window launches
[PASS] UI displays KeyForge native version/platform
[PASS] No application networking has been added
[PASS] No keyboard hooks have been added
[PASS] Only get_app_info is exposed as a custom command
[PASS] main window capability is explicit and least-privilege
[PASS] AGENTS.md documents non-negotiable privacy invariants
```

## Subsequent Milestones

Each receives a separate implementation plan:

1. **M2 — Audio Engine:** low-latency playback abstraction, sample preloading, concurrency, volume, deterministic tests.
2. **M3 — Sound-Pack Format:** safe manifest schema, file/type/size limits, traversal defense, decoder validation, bundled default pack.
3. **M4 — Sanitized Input Domain:** `SoundEvent` model and sanitizer tests with no OS hooks.
4. **M5 — macOS Input Adapter:** native global input integration and permission UX.
5. **M6 — Product UI + Tray:** enable/disable, pack selection, volume, startup behavior.
6. **M7 — Windows Input Adapter:** low-level Windows keyboard adapter behind the same interface.
7. **M8 — Linux Input Adapters:** X11 first, then Wayland-compatible strategy with documented limitations.
8. **M9 — Release Security:** dependency review, CodeQL, SBOM, checksums, attestations, malware scanning, release permissions.
9. **M10 — V1 Packaging:** macOS, Windows, Linux artifacts, signing/notarization strategy, release checklist.

## Self-Review

- Spec coverage for Milestone 1: complete.
- No keyboard-capture behavior appears before the input-domain milestone.
- No networking is introduced.
- Native IPC is explicit and minimal.
- Tauri static export requirements are satisfied.
- Frontend and Rust testing are independently enforceable.
- Security documentation is created before privileged features.
- Later independent subsystems are intentionally split into separate plans.
