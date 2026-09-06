import {
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

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

    const tokens = rustTokens(source);
    violations.push(...sourceBypassViolations(path, tokens));
    violations.push(...moduleReferenceViolations(path, tokens));

    for (const symbol of FORBIDDEN_STARTUP_SYMBOLS) {
      if (tokens.includes(symbol)) {
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
  const metadata = lstatSync(directory);
  if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
    throw new Error("unsafe source tree entry");
  }
  for (const entry of readdirSync(directory)) {
    const path = `${directory}/${entry}`;
    const entryMetadata = lstatSync(path);
    if (entryMetadata.isSymbolicLink()) {
      throw new Error("unsafe source tree entry");
    }
    if (entryMetadata.isDirectory()) {
      for (const [childPath, source] of readProductionSources(path)) {
        sources.set(childPath, source);
      }
    } else if (entryMetadata.isFile() && path.endsWith(".rs")) {
      sources.set(path, readFileSync(path, "utf8"));
    } else if (!entryMetadata.isFile()) {
      throw new Error("unsafe source tree entry");
    }
  }
  return sources;
}

function sourceBypassViolations(path: string, tokens: string[]): string[] {
  const violations: string[] = [];
  for (let index = 0; index < tokens.length; index += 1) {
    if (tokens[index] !== "#" || tokens[index + 1] !== "[") {
      continue;
    }
    const end = matchingDelimiter(tokens, index + 1, "[", "]");
    if (end === undefined) {
      violations.push(`${path}: malformed attribute`);
      continue;
    }
    const attribute = tokens.slice(index + 2, end);
    if (
      (attribute[0] === "path" && attribute.includes("=")) ||
      (attribute[0] === "cfg_attr" && containsAssignment(attribute, "path"))
    ) {
      violations.push(`${path}: module path override`);
    }
    index = end;
  }

  const includeAliases = new Set(["include"]);
  for (let index = 0; index < tokens.length; index += 1) {
    if (tokens[index] !== "use") {
      continue;
    }
    const end = tokens.indexOf(";", index + 1);
    const importTokens = tokens.slice(index + 1, end === -1 ? tokens.length : end);
    if (!importTokens.includes("include")) {
      continue;
    }
    violations.push(`${path}: include import`);
    const asIndex = importTokens.lastIndexOf("as");
    if (asIndex !== -1 && isIdentifier(importTokens[asIndex + 1])) {
      includeAliases.add(importTokens[asIndex + 1]);
    }
  }
  for (let index = 0; index < tokens.length - 1; index += 1) {
    if (includeAliases.has(tokens[index]) && tokens[index + 1] === "!") {
      violations.push(`${path}: include!`);
    }
  }
  return violations;
}

function moduleReferenceViolations(path: string, tokens: string[]): string[] {
  const approvedDeclarations = new Set<number>();
  let braceDepth = 0;
  for (let index = 0; index < tokens.length; index += 1) {
    if (
      path === `${NATIVE_SOURCE_ROOT}/lib.rs` &&
      braceDepth === 0 &&
      tokens[index] === "pub" &&
      tokens[index + 1] === "mod" &&
      (tokens[index + 2] === "audio" || tokens[index + 2] === "pack") &&
      tokens[index + 3] === ";"
    ) {
      approvedDeclarations.add(index + 2);
    }
    if (tokens[index] === "{") {
      braceDepth += 1;
    } else if (tokens[index] === "}") {
      braceDepth -= 1;
    }
  }

  const violations: string[] = [];
  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index];
    if ((token === "audio" || token === "pack") && !approvedDeclarations.has(index)) {
      violations.push(`${path}: ${token} module reference`);
    }
  }
  return violations;
}

function containsAssignment(tokens: string[], identifier: string): boolean {
  return tokens.some(
    (token, index) => token === identifier && tokens[index + 1] === "=",
  );
}

function matchingDelimiter(
  tokens: string[],
  start: number,
  open: string,
  close: string,
): number | undefined {
  let depth = 0;
  for (let index = start; index < tokens.length; index += 1) {
    if (tokens[index] === open) {
      depth += 1;
    } else if (tokens[index] === close) {
      depth -= 1;
      if (depth === 0) {
        return index;
      }
    }
  }
  return undefined;
}

function rustTokens(source: string): string[] {
  const tokens: string[] = [];
  let index = 0;
  while (index < source.length) {
    const character = source[index];
    if (/\s/.test(character)) {
      index += 1;
    } else if (source.startsWith("//", index)) {
      index = source.indexOf("\n", index + 2);
      if (index === -1) {
        break;
      }
    } else if (source.startsWith("/*", index)) {
      index = skipBlockComment(source, index);
    } else {
      const rawStringEnd = rawStringEndIndex(source, index);
      if (rawStringEnd !== undefined) {
        index = rawStringEnd;
      } else if (character === '"' || character === "'") {
        index = skipQuotedLiteral(source, index, character);
      } else if (isIdentifierStart(character)) {
        const start = index;
        index += 1;
        while (index < source.length && isIdentifierPart(source[index])) {
          index += 1;
        }
        tokens.push(source.slice(start, index));
      } else {
        tokens.push(character);
        index += 1;
      }
    }
  }
  return tokens;
}

function skipBlockComment(source: string, index: number): number {
  let depth = 1;
  index += 2;
  while (index < source.length && depth > 0) {
    if (source.startsWith("/*", index)) {
      depth += 1;
      index += 2;
    } else if (source.startsWith("*/", index)) {
      depth -= 1;
      index += 2;
    } else {
      index += 1;
    }
  }
  return index;
}

function rawStringEndIndex(source: string, start: number): number | undefined {
  let index = start;
  if (source[index] === "b") {
    index += 1;
  }
  if (source[index] !== "r") {
    return undefined;
  }
  index += 1;
  let hashes = 0;
  while (source[index] === "#") {
    hashes += 1;
    index += 1;
  }
  if (source[index] !== '"') {
    return undefined;
  }
  const terminator = `"${"#".repeat(hashes)}`;
  const end = source.indexOf(terminator, index + 1);
  return end === -1 ? source.length : end + terminator.length;
}

function skipQuotedLiteral(source: string, index: number, quote: string): number {
  index += 1;
  while (index < source.length) {
    if (source[index] === "\\") {
      index += 2;
    } else if (source[index] === quote) {
      return index + 1;
    } else {
      index += 1;
    }
  }
  return index;
}

function isIdentifierStart(value: string | undefined): boolean {
  return value !== undefined && /[A-Za-z_]/.test(value);
}

function isIdentifierPart(value: string | undefined): boolean {
  return value !== undefined && /[A-Za-z0-9_]/.test(value);
}

function isIdentifier(value: string | undefined): value is string {
  return value !== undefined && /^[A-Za-z_][A-Za-z0-9_]*$/.test(value);
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

function withTempTree(action: (root: string) => void) {
  const root = mkdtempSync(join(tmpdir(), "keyforge-pack-policy-"));
  try {
    action(root);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
}

function smokeOrderViolations(source: string): string[] {
  const start = source.indexOf("AudioEngine::start");
  const register = source.indexOf(".register(&handle)");
  const ready = source.indexOf("wait_until_ready(&handle)");
  return start < register && register < ready ? [] : ["pack smoke lifecycle order"];
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

it("forbids a lib.rs call through an excluded audio wrapper", () => {
  const violations = auditFixture({
    "src-tauri/src/audio/mod.rs": "pub fn boot() {}",
    "src-tauri/src/lib.rs": "pub mod audio;\npub fn run() { audio::boot(); }",
  });

  expect(violations).toContain("src-tauri/src/lib.rs: audio module reference");
});

it("forbids audio re-exports and aliases outside the excluded foundation", () => {
  const violations = auditFixture({
    "src-tauri/src/audio/mod.rs": "pub fn boot() {}",
    "src-tauri/src/lib.rs":
      "pub mod audio;\npub use crate::audio::boot as start_sound;\npub fn run() { start_sound(); }",
  });

  expect(violations).toContain("src-tauri/src/lib.rs: audio module reference");
});

it("forbids crate audio aliases in audited startup sources", () => {
  const violations = auditFixture({
    "src-tauri/src/startup.rs": "use crate::audio as native_audio;",
  });

  expect(violations).toContain("src-tauri/src/startup.rs: audio module reference");
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

it("rejects path overrides through comments and balanced cfg_attr attributes", () => {
  const directViolations = auditFixture({
    "src-tauri/src/startup.rs": '#[path/* comment */= "hidden.rs"] mod hidden;',
  });
  const conditionalViolations = auditFixture({
    "src-tauri/src/startup.rs":
      '#[cfg_attr(feature = "x", doc = "]", path /* comment */ = "hidden.rs")] mod hidden;',
  });

  expect(directViolations).toContain(
    "src-tauri/src/startup.rs: module path override",
  );
  expect(conditionalViolations).toContain(
    "src-tauri/src/startup.rs: module path override",
  );
});

it("rejects include macro code injection in an audited source", () => {
  const violations = auditFixture({
    "src-tauri/src/lib.rs": "mod startup;\npub fn run() {}",
    "src-tauri/src/startup.rs": 'include!("generated-startup.rs");',
  });

  expect(violations).toContain("src-tauri/src/startup.rs: include!");
});

it("rejects direct and imported include macro injection through whitespace", () => {
  const directViolations = auditFixture({
    "src-tauri/src/startup.rs": 'include /* comment */ ! ("generated.rs");',
  });
  const aliasedViolations = auditFixture({
    "src-tauri/src/startup.rs": "use std::include as inject;\ninject ! ();",
  });

  expect(directViolations).toContain("src-tauri/src/startup.rs: include!");
  expect(aliasedViolations).toContain("src-tauri/src/startup.rs: include!");
});

it("allows inert include_str and include_bytes macros", () => {
  expect(
    auditFixture({
      "src-tauri/src/startup.rs":
        'let text = include_str!("note.txt");\nlet bytes = include_bytes!("note.bin");',
    }),
  ).toEqual([]);
});

it("collects the exact nested Rust source set", () => {
  withTempTree((root) => {
    mkdirSync(join(root, "nested"));
    writeFileSync(join(root, "main.rs"), "fn main() {}");
    writeFileSync(join(root, "nested", "worker.rs"), "fn worker() {}");
    writeFileSync(join(root, "nested", "ignored.txt"), "ignored");

    expect([...readProductionSources(root).keys()].sort()).toEqual(
      [join(root, "main.rs"), join(root, "nested", "worker.rs")].sort(),
    );
  });
});

it.runIf(process.platform !== "win32")(
  "rejects symlinked Rust files without following them",
  () => {
    withTempTree((root) => {
      const target = join(root, "target.rs");
      writeFileSync(target, "fn hidden() {}");
      symlinkSync(target, join(root, "linked.rs"));

      expect(() => readProductionSources(root)).toThrow("unsafe source tree entry");
    });
  },
);

it("rejects symlinked directories without following them", () => {
  withTempTree((root) => {
    const target = join(root, "target");
    mkdirSync(target);
    writeFileSync(join(target, "hidden.rs"), "fn hidden() {}");
    symlinkSync(target, join(root, "linked"), process.platform === "win32" ? "junction" : "dir");

    expect(() => readProductionSources(root)).toThrow("unsafe source tree entry");
  });
});

it("keeps pack smoke registration before waiting for readiness", () => {
  const smokeSource = readFileSync("src-tauri/examples/pack_smoke.rs", "utf8");

  expect(smokeOrderViolations(smokeSource)).toEqual([]);
  expect(
    smokeOrderViolations(
      "AudioEngine::start(); wait_until_ready(&handle); decoded.register(&handle);",
    ),
  ).toEqual(["pack smoke lifecycle order"]);
});

it("does not scan foundational public module definitions", () => {
  expect(auditFixture()).toEqual([]);
});

it("keeps the sound-pack smoke path developer-only across production Rust sources", () => {
  expect(existsSync("src-tauri/examples/pack_smoke.rs")).toBe(true);

  expect(auditProductionSources(readProductionSources())).toEqual([]);
});
