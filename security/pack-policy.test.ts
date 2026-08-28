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

    for (const symbol of FORBIDDEN_STARTUP_SYMBOLS) {
      if (new RegExp(`\\b${symbol}\\b`).test(source)) {
        violations.push(`${path}: ${symbol}`);
      }
    }

    for (const moduleName of privateModuleNames(source)) {
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

function privateModuleNames(source: string): string[] {
  return [...source.matchAll(/(?:^|\n)\s*mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;/g)].map(
    (match) => match[1],
  );
}

function privateModulePaths(parentPath: string, moduleName: string): string[] {
  const directory = parentPath.slice(0, parentPath.lastIndexOf("/"));
  return [
    `${directory}/${moduleName}.rs`,
    `${directory}/${moduleName}/mod.rs`,
  ];
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
