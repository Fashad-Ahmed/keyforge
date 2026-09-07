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
const EXPECTED_PACK_FILES = [
  "src-tauri/src/pack/archive.rs",
  "src-tauri/src/pack/decoder.rs",
  "src-tauri/src/pack/manifest.rs",
  "src-tauri/src/pack/mod.rs",
  "src-tauri/src/pack/storage.rs",
  "src-tauri/src/pack/test_support.rs",
] as const;
const EXPECTED_ASSETS = [
  "src-tauri/assets/packs/README.md",
  "src-tauri/assets/packs/keyforge-mechanical.zip",
] as const;
const EXPECTED_PACK_DEPENDENCIES = [
  'hound = "3.5.1"',
  'serde_json = "1.0"',
  'zip = { version = "8.6.0", default-features = false, features = ["deflate-flate2-zlib-rs"] }',
] as const;
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
      if (hasRawIdentifier(source, "macro_export")) {
        violations.push(`${path}: exported macro declaration`);
      }
      continue;
    }

    const auditedSource = withoutApprovedFoundationDeclarations(path, source);

    for (const root of ["audio", "pack"] as const) {
      if (hasRawIdentifier(auditedSource, root)) {
        violations.push(`${path}: ${root} module reference`);
      }
    }

    if (hasRawIdentifier(auditedSource, "path")) {
      violations.push(`${path}: module path override`);
    }
    if (hasRawIdentifier(auditedSource, "include")) {
      violations.push(`${path}: include!`);
    }
    if (hasRawIdentifier(auditedSource, "macro_use")) {
      violations.push(`${path}: macro_use`);
    }

    for (const symbol of FORBIDDEN_STARTUP_SYMBOLS) {
      if (hasRawIdentifier(auditedSource, symbol)) {
        violations.push(`${path}: ${symbol}`);
      }
    }
  }

  return violations;
}

function withoutApprovedFoundationDeclarations(path: string, source: string): string {
  if (path !== `${NATIVE_SOURCE_ROOT}/lib.rs`) {
    return source;
  }

  const firstBlock = source.indexOf("{");
  const headerEnd = firstBlock === -1 ? source.length : firstBlock;
  const header = source
    .slice(0, headerEnd)
    .replace(/^pub mod audio;\r?$/m, "")
    .replace(/^pub mod pack;\r?$/m, "");
  return header + source.slice(headerEnd);
}

function hasRawIdentifier(source: string, identifier: string): boolean {
  return new RegExp(
    `(^|[^A-Za-z0-9_])${identifier}([^A-Za-z0-9_]|$)`,
  ).test(source);
}

function isFoundationalSource(path: string): boolean {
  return (
    path === `${NATIVE_SOURCE_ROOT}/audio.rs` ||
    path === `${NATIVE_SOURCE_ROOT}/pack.rs` ||
    path.startsWith(`${NATIVE_SOURCE_ROOT}/audio/`) ||
    path.startsWith(`${NATIVE_SOURCE_ROOT}/pack/`)
  );
}

function readProductionSources(
  directory = NATIVE_SOURCE_ROOT,
  logicalDirectory = directory,
): Map<string, string> {
  const sources = new Map<string, string>();
  const metadata = lstatSync(directory);
  if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
    throw new Error("unsafe source tree entry");
  }
  const logicalRoot = logicalDirectory.replaceAll("\\", "/").replace(/\/+$/, "");
  for (const entry of readdirSync(directory)) {
    const filesystemPath = join(directory, entry);
    const logicalPath = `${logicalRoot}/${entry}`;
    const entryMetadata = lstatSync(filesystemPath);
    if (entryMetadata.isSymbolicLink()) {
      throw new Error("unsafe source tree entry");
    }
    if (entryMetadata.isDirectory()) {
      for (const [childPath, source] of readProductionSources(
        filesystemPath,
        logicalPath,
      )) {
        sources.set(childPath, source);
      }
    } else if (entryMetadata.isFile() && entry.endsWith(".rs")) {
      sources.set(logicalPath, readFileSync(filesystemPath, "utf8"));
    } else if (!entryMetadata.isFile()) {
      throw new Error("unsafe source tree entry");
    }
  }
  return sources;
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

function enumerateFiles(directory: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(directory)) {
    const path = join(directory, entry);
    const metadata = lstatSync(path);
    if (metadata.isSymbolicLink() || (!metadata.isFile() && !metadata.isDirectory())) {
      throw new Error("unsafe reviewed tree entry");
    }
    if (metadata.isDirectory()) {
      files.push(...enumerateFiles(path));
    } else {
      files.push(path.replaceAll("\\", "/"));
    }
  }
  return files.sort();
}

function packDependencyViolations(cargoToml: string): string[] {
  return EXPECTED_PACK_DEPENDENCIES.filter(
    (record) => !cargoToml.split(/\r?\n/u).includes(record),
  );
}

function exactFileSetViolations(actual: string[], expected: readonly string[]): string[] {
  const expectedSet = new Set(expected);
  const actualSet = new Set(actual);
  return [
    ...actual.filter((path) => !expectedSet.has(path)),
    ...expected.filter((path) => !actualSet.has(path)),
  ].sort();
}

function nativeBoundaryViolations(capability: string, libSource: string): string[] {
  const violations: string[] = [];
  const parsed = JSON.parse(capability) as { permissions?: unknown };
  if (!Array.isArray(parsed.permissions) || parsed.permissions.length !== 0) {
    violations.push("capabilities must remain empty");
  }
  const handlers = [...libSource.matchAll(/generate_handler!\s*\[([^\]]*)\]/gu)];
  if (
    handlers.length !== 1 ||
    handlers[0]?.[1]?.trim() !== "commands::app_info::get_app_info"
  ) {
    violations.push("handler allowlist changed");
  }
  return violations;
}

function ciViolations(workflow: string): string[] {
  const required = [
    "os: [ubuntu-24.04, macos-15, windows-2025]",
    "cargo metadata --locked",
    "cargo fmt --manifest-path src-tauri/Cargo.toml -- --check",
    "cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings",
    "cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets",
  ];
  return required.filter((text) => !workflow.includes(text));
}

function missingDocumentationPhrases(documents: string[]): string[] {
  const combined = documents.join("\n").toLowerCase();
  const required = [
    "16 mib compressed archive limit",
    "cross-platform traversal",
    "canonical signed-16 pcm wav",
    "same-parent staging",
    "duplicate pack ids",
    "decoded pcm only",
    "no sound-pack ipc",
    "no application networking",
  ];
  return required.filter((phrase) => !combined.includes(phrase));
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

it("flags a forbidden audio root reference after a compiling lifetime", () => {
  const violations = auditFixture({
    "src-tauri/src/deferred/lifetime_launch.rs":
      "fn launch<'engine>() { let _ = crate::audio::AudioEngine::start(); }",
  });

  expect(violations).toContain(
    "src-tauri/src/deferred/lifetime_launch.rs: audio module reference",
  );
});

it("flags a forbidden pack root reference inside a compiling loop label", () => {
  const violations = auditFixture({
    "src-tauri/src/deferred/label_launch.rs":
      "fn launch() { 'launch: loop { let _ = crate::pack::PackManager::open(todo!()); break; } }",
  });

  expect(violations).toContain(
    "src-tauri/src/deferred/label_launch.rs: pack module reference",
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
      [
        `${root.replaceAll("\\", "/")}/main.rs`,
        `${root.replaceAll("\\", "/")}/nested/worker.rs`,
      ].sort(),
    );
  });
});

it("keeps logical policy keys normalized across Windows-like separators", () => {
  withTempTree((root) => {
    mkdirSync(join(root, "nested"));
    writeFileSync(join(root, "main.rs"), "fn main() {}");
    writeFileSync(join(root, "nested", "worker.rs"), "fn worker() {}");

    expect(
      [
        ...readProductionSources(
          root,
          String.raw`C:\workspace\keyforge\src-tauri\src`,
        ).keys(),
      ].sort(),
    ).toEqual(
      [
        "C:/workspace/keyforge/src-tauri/src/main.rs",
        "C:/workspace/keyforge/src-tauri/src/nested/worker.rs",
      ].sort(),
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

it("rejects crate-root macro wrappers exported from the audio foundation", () => {
  const violations = auditFixture({
    "src-tauri/src/audio/mod.rs":
      "#[macro_export]\nmacro_rules! boot_native { () => { let _ = $crate::audio::AudioEngine::start(); } }",
    "src-tauri/src/lib.rs":
      "pub mod audio;\npub mod pack;\npub fn run() { crate::boot_native!(); }",
  });

  expect(violations).toContain(
    "src-tauri/src/audio/mod.rs: exported macro declaration",
  );
});

it("rejects macro_use imports from the audio foundation", () => {
  const violations = auditFixture({
    "src-tauri/src/audio/mod.rs":
      "macro_rules! boot_native { () => { let _ = $crate::audio::AudioEngine::start(); } }",
    "src-tauri/src/lib.rs":
      "#[macro_use]\npub mod audio;\npub mod pack;\npub fn run() { boot_native!(); }",
  });

  expect(violations).toContain("src-tauri/src/lib.rs: macro_use");
});

it("rejects macro_use imports from the pack foundation", () => {
  const violations = auditFixture({
    "src-tauri/src/pack/mod.rs":
      "macro_rules! open_native { () => { let _ = $crate::pack::PackManager::open(todo!()); } }",
    "src-tauri/src/lib.rs":
      "pub mod audio;\n#[macro_use] pub mod pack;\npub fn run() { open_native!(); }",
  });

  expect(violations).toContain("src-tauri/src/lib.rs: macro_use");
});

it("rejects comment-separated macro exports in nested pack foundations", () => {
  const violations = auditFixture({
    "src-tauri/src/pack/export.rs":
      "#[ /* review */ macro_export /* review */ ]\nmacro_rules! open_native { () => { let _ = $crate::pack::PackManager::open(todo!()); } }",
    "src-tauri/src/lib.rs":
      "pub mod audio;\npub mod pack;\npub fn run() { crate::open_native!(); }",
  });

  expect(violations).toContain(
    "src-tauri/src/pack/export.rs: exported macro declaration",
  );
});

it("allows internal non-exported macros in foundational sources", () => {
  expect(
    auditFixture({
      "src-tauri/src/audio/internal.rs":
        "macro_rules! build_stream { () => { 1_u8 } }\nfn use_it() { let _ = build_stream!(); }",
    }),
  ).toEqual([]);
});

it("does not scan foundational public module definitions", () => {
  expect(auditFixture()).toEqual([]);
});

it("keeps the sound-pack smoke path developer-only across production Rust sources", () => {
  expect(existsSync("src-tauri/examples/pack_smoke.rs")).toBe(true);

  expect(auditProductionSources(readProductionSources())).toEqual([]);
});

it("keeps the reviewed pack source and asset sets exact", () => {
  expect(exactFileSetViolations(enumerateFiles("src-tauri/src/pack"), EXPECTED_PACK_FILES)).toEqual(
    [],
  );
  expect(exactFileSetViolations(enumerateFiles("src-tauri/assets/packs"), EXPECTED_ASSETS)).toEqual(
    [],
  );
  expect(
    enumerateFiles("src-tauri/assets/packs").every(
      (path) => path.endsWith(".md") || path === EXPECTED_ASSETS[1],
    ),
  ).toBe(true);
});

it("rejects hidden or executable-looking pack assets", () => {
  expect(
    exactFileSetViolations([...EXPECTED_ASSETS, "src-tauri/assets/packs/.hidden"], EXPECTED_ASSETS),
  ).toContain("src-tauri/assets/packs/.hidden");
  expect(EXPECTED_ASSETS.some((path) => /\.(?:exe|dll|dylib|so|js|sh)$/u.test(path))).toBe(false);
});

it("locks the exact direct sound-pack dependency records", () => {
  expect(packDependencyViolations(readFileSync("src-tauri/Cargo.toml", "utf8"))).toEqual([]);
  expect(
    packDependencyViolations(
      readFileSync("src-tauri/Cargo.toml", "utf8").replace(
        'features = ["deflate-flate2-zlib-rs"]',
        'features = ["deflate-flate2-zlib-rs", "bzip2"]',
      ),
    ),
  ).toContain(EXPECTED_PACK_DEPENDENCIES[2]);
});

it("keeps capabilities empty and the handler allowlist exact", () => {
  const capability = readFileSync("src-tauri/capabilities/main.json", "utf8");
  const libSource = readFileSync("src-tauri/src/lib.rs", "utf8");
  expect(nativeBoundaryViolations(capability, libSource)).toEqual([]);
  expect(nativeBoundaryViolations('{"permissions":["core:event:default"]}', libSource)).toContain(
    "capabilities must remain empty",
  );
  expect(
    nativeBoundaryViolations(
      capability,
      libSource.replace(
        "commands::app_info::get_app_info]",
        "commands::app_info::get_app_info, commands::packs::install]",
      ),
    ),
  ).toContain("handler allowlist changed");
});

it("keeps locked cross-platform Rust CI quality gates", () => {
  const workflow = readFileSync(".github/workflows/ci.yml", "utf8");
  expect(ciViolations(workflow)).toEqual([]);
  expect(ciViolations(workflow.replace("cargo test --locked", "cargo test"))).toContain(
    "cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets",
  );
});

it("documents the implemented sound-pack security boundary", () => {
  expect(
    missingDocumentationPhrases([
      readFileSync("README.md", "utf8"),
      readFileSync("docs/architecture/trust-boundaries.md", "utf8"),
      readFileSync("docs/security/threat-model.md", "utf8"),
    ]),
  ).toEqual([]);
  expect(missingDocumentationPhrases(["no application networking"])).toContain(
    "cross-platform traversal",
  );
});
