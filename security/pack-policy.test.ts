import { existsSync, readFileSync } from "node:fs";

import { expect, it } from "vitest";

const ENTRYPOINTS = ["src-tauri/src/main.rs", "src-tauri/src/lib.rs"];
const FORBIDDEN_STARTUP_SYMBOLS = [
  "PackManager",
  "install_bundled_default",
  "install_zip",
  "AudioEngine",
  "AudioEngineHandle",
  "register_sample",
  "register_samples",
] as const;

type SourceReader = (path: string) => string | undefined;

function auditStartupEntrypoints(readSource: SourceReader): string[] {
  const pending = [...ENTRYPOINTS];
  const visited = new Set<string>();
  const violations: string[] = [];

  while (pending.length > 0) {
    const path = pending.pop();
    if (path === undefined || visited.has(path)) {
      continue;
    }
    visited.add(path);

    const source = readSource(path);
    if (source === undefined) {
      continue;
    }

    violations.push(...sourceBypassViolations(path, source));

    for (const symbol of FORBIDDEN_STARTUP_SYMBOLS) {
      if (new RegExp(`\\b${symbol}\\b`).test(source)) {
        violations.push(`${path}: ${symbol}`);
      }
    }

    for (const moduleName of moduleNames(source)) {
      if (isCrateFoundation(path, moduleName)) {
        continue;
      }
      for (const modulePath of privateModulePaths(path, moduleName)) {
        if (readSource(modulePath) !== undefined) {
          pending.push(modulePath);
          break;
        }
      }
    }
  }

  return violations;
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

function moduleNames(source: string): string[] {
  return [
    ...source.matchAll(
      /(?:^|\n)\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;/g,
    ),
  ].map((match) => match[1]);
}

function privateModulePaths(parentPath: string, moduleName: string): string[] {
  const childBase = childModuleBase(parentPath);
  return [
    `${childBase}/${moduleName}.rs`,
    `${childBase}/${moduleName}/mod.rs`,
  ];
}

function childModuleBase(parentPath: string): string {
  const directory = parentPath.slice(0, parentPath.lastIndexOf("/"));
  if (parentPath.endsWith("/lib.rs") || parentPath.endsWith("/main.rs") || parentPath.endsWith("/mod.rs")) {
    return directory;
  }
  return `${directory}/${parentPath.slice(parentPath.lastIndexOf("/") + 1, -3)}`;
}

function isCrateFoundation(parentPath: string, moduleName: string): boolean {
  return (
    parentPath === "src-tauri/src/lib.rs" &&
    (moduleName === "audio" || moduleName === "pack")
  );
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
  const sources = fixtureSources(overrides);
  return auditStartupEntrypoints((path) => sources.get(path));
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

it("flags pack startup reached through a private module", () => {
  const violations = auditFixture({
    "src-tauri/src/lib.rs": "mod startup;\npub fn run() {}",
    "src-tauri/src/startup.rs":
      "use crate::pack::PackManager;\nfn start() { PackManager::open(todo!()); }",
  });

  expect(violations).toContain("src-tauri/src/startup.rs: PackManager");
});

it("flags integration in a private module nested below a file module", () => {
  const violations = auditFixture({
    "src-tauri/src/lib.rs": "mod startup;\npub fn run() {}",
    "src-tauri/src/startup.rs": "mod engine;",
    "src-tauri/src/startup/engine.rs":
      "use crate::audio::AudioEngine;\nfn start() { AudioEngine::start(); }",
  });

  expect(violations).toContain("src-tauri/src/startup/engine.rs: AudioEngine");
});

it("flags integration in a public child of a reachable private commands module", () => {
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

it("rejects reachable include macro code injection", () => {
  const violations = auditFixture({
    "src-tauri/src/lib.rs": "mod startup;\npub fn run() {}",
    "src-tauri/src/startup.rs": 'include!("generated-startup.rs");',
  });

  expect(violations).toContain("src-tauri/src/startup.rs: include!");
});

it("does not scan foundational public module definitions", () => {
  expect(auditFixture()).toEqual([]);
});

it("keeps the sound-pack smoke path developer-only in production entrypoints", () => {
  expect(existsSync("src-tauri/examples/pack_smoke.rs")).toBe(true);

  expect(
    auditStartupEntrypoints((path) =>
      existsSync(path) ? readFileSync(path, "utf8") : undefined,
    ),
  ).toEqual([]);
});
