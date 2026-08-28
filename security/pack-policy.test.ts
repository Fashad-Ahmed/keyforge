import { existsSync, readFileSync, readdirSync } from "node:fs";

import { expect, it } from "vitest";

const NATIVE_SOURCE_ROOT = "src-tauri/src";
const FORBIDDEN_STARTUP_SYMBOLS = [
  "PackManager",
  "install_bundled_default",
  "install_zip",
  "AudioEngine",
  "AudioEngineHandle",
  "register_sample",
  "register_samples",
] as const;

function auditProductionSources(sources: ReadonlyMap<string, string>): string[] {
  const violations: string[] = [];

  for (const [path, source] of sources) {
    if (isFoundationalSource(path)) {
      continue;
    }

    violations.push(...sourceBypassViolations(path, source));

    for (const symbol of FORBIDDEN_STARTUP_SYMBOLS) {
      if (new RegExp(`\\b${symbol}\\b`).test(source)) {
        violations.push(`${path}: ${symbol}`);
      }
    }

  }

  return violations;
}

function isFoundationalSource(path: string): boolean {
  return (
    path === `${NATIVE_SOURCE_ROOT}/audio.rs` ||
    path === `${NATIVE_SOURCE_ROOT}/pack.rs` ||
    path.startsWith(`${NATIVE_SOURCE_ROOT}/audio/`) ||
    path.startsWith(`${NATIVE_SOURCE_ROOT}/pack/`)
  );
}

function readProductionSources(directory = NATIVE_SOURCE_ROOT): Map<string, string> {
  const sources = new Map<string, string>();
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = `${directory}/${entry.name}`;
    if (entry.isDirectory()) {
      for (const [childPath, source] of readProductionSources(path)) {
        sources.set(childPath, source);
      }
    } else if (entry.isFile() && entry.name.endsWith(".rs")) {
      sources.set(path, readFileSync(path, "utf8"));
    }
  }
  return sources;
}

function sourceBypassViolations(path: string, source: string): string[] {
  const violations: string[] = [];
  if (
    /#\s*\[\s*(?:path\s*=|cfg_attr\s*\([^\]]*\bpath\s*=)/.test(source)
  ) {
    violations.push(`${path}: module path override`);
  }
  if (/\binclude\s*!\s*\(/.test(source)) {
    violations.push(`${path}: include!`);
  }
  return violations;
}

function fixtureSources(overrides: Record<string, string> = {}) {
  return new Map<string, string>([
    ["src-tauri/src/main.rs", "fn main() {}"],
    [
      "src-tauri/src/lib.rs",
      "pub mod audio;\npub mod pack;\nmod commands;\npub fn run() {}",
    ],
    ["src-tauri/src/commands/mod.rs", "pub mod app_info;"],
    ["src-tauri/src/audio/mod.rs", "pub struct AudioEngine;"],
    ["src-tauri/src/pack/mod.rs", "pub struct PackManager;"],
    ...Object.entries(overrides),
  ]);
}

function auditFixture(overrides: Record<string, string> = {}) {
  return auditProductionSources(fixtureSources(overrides));
}

it("flags direct audio startup from main.rs", () => {
  const violations = auditFixture({
    "src-tauri/src/main.rs":
      "use keyforge_lib::audio::AudioEngine;\nfn main() { AudioEngine::start(); }",
  });

  expect(violations).toContain("src-tauri/src/main.rs: AudioEngine");
});

it("flags aliased audio startup imports", () => {
  const violations = auditFixture({
    "src-tauri/src/main.rs":
      "use keyforge_lib::audio::AudioEngine as Engine;\nfn main() { Engine::start(); }",
  });

  expect(violations).toContain("src-tauri/src/main.rs: AudioEngine");
});

it("flags alias integration in an arbitrary nonconventional Rust source", () => {
  const violations = auditFixture({
    "src-tauri/src/deferred/launch_sequence.rs":
      "use crate::audio::AudioEngine as Engine;\nfn start() { Engine::start(); }",
  });

  expect(violations).toContain(
    "src-tauri/src/deferred/launch_sequence.rs: AudioEngine",
  );
});

it("flags PackManager integration in a nonfoundational source", () => {
  const violations = auditFixture({
    "src-tauri/src/lib.rs": "mod startup;\npub fn run() {}",
    "src-tauri/src/startup.rs":
      "use crate::pack::PackManager;\nfn start() { PackManager::open(todo!()); }",
  });

  expect(violations).toContain("src-tauri/src/startup.rs: PackManager");
});

it("flags integration in a nested nonfoundational source", () => {
  const violations = auditFixture({
    "src-tauri/src/lib.rs": "mod startup;\npub fn run() {}",
    "src-tauri/src/startup.rs": "mod engine;",
    "src-tauri/src/startup/engine.rs":
      "use crate::audio::AudioEngine;\nfn start() { AudioEngine::start(); }",
  });

  expect(violations).toContain("src-tauri/src/startup/engine.rs: AudioEngine");
});

it("flags integration in a command source regardless of module visibility", () => {
  const violations = auditFixture({
    "src-tauri/src/lib.rs": "mod commands;\npub fn run() {}",
    "src-tauri/src/commands/mod.rs": "pub mod startup;",
    "src-tauri/src/commands/startup.rs":
      "use crate::pack::PackManager;\nfn start() { PackManager::open(todo!()); }",
  });

  expect(violations).toContain("src-tauri/src/commands/startup.rs: PackManager");
});

it("rejects module path override attributes without reading their paths", () => {
  const directPathViolations = auditFixture({
    "src-tauri/src/lib.rs":
      '#[path = "hidden-startup.rs"]\nmod startup;\npub fn run() {}',
  });
  const conditionalPathViolations = auditFixture({
    "src-tauri/src/lib.rs":
      '#[cfg_attr(target_os = "macos", path = "hidden-startup.rs")]\nmod startup;\npub fn run() {}',
  });

  expect(directPathViolations).toContain(
    "src-tauri/src/lib.rs: module path override",
  );
  expect(conditionalPathViolations).toContain(
    "src-tauri/src/lib.rs: module path override",
  );
});

it("rejects include macro code injection in an audited source", () => {
  const violations = auditFixture({
    "src-tauri/src/lib.rs": "mod startup;\npub fn run() {}",
    "src-tauri/src/startup.rs": 'include!("generated-startup.rs");',
  });

  expect(violations).toContain("src-tauri/src/startup.rs: include!");
});

it("does not scan foundational public module definitions", () => {
  expect(auditFixture()).toEqual([]);
});

it("keeps the sound-pack smoke path developer-only across production Rust sources", () => {
  expect(existsSync("src-tauri/examples/pack_smoke.rs")).toBe(true);

  expect(auditProductionSources(readProductionSources())).toEqual([]);
});
