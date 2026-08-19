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
  rename: string | null;
  req: string;
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

const REVIEWED_WORKFLOW_LINES = new Set([
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
  "      - name: Install Tauri Linux prerequisites",
  "        if: runner.os == 'Linux'",
  "        run: |",
  "      - uses: dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c",
  "        with:",
  "          toolchain: 1.88.0",
  "          components: rustfmt, clippy",
  "      - run: cargo metadata --locked --manifest-path src-tauri/Cargo.toml --no-deps --format-version 1",
  "      - if: runner.os == 'Linux'",
  "        run: cargo fmt --manifest-path src-tauri/Cargo.toml -- --check",
  "      - run: cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings",
  "      - run: cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets",
]);

const PROJECT_ROOT = process.cwd();
const ALLOWED_HANDLER = "tauri::generate_handler![commands::app_info::get_app_info]";
const ALLOWED_ACTIONS = [
  "actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
  "pnpm/action-setup@f40ffcd9367d9f12939873eb1018b921a783ffaa",
  "actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020",
  "actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
  "dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c",
];
const APPROVED_DEPENDENCIES = [
  {
    name: "cpal",
    rename: null,
    req: "^0.18.1",
    kind: null,
    optional: false,
    uses_default_features: false,
    features: [],
  },
  {
    name: "crossbeam-queue",
    rename: null,
    req: "^0.3.13",
    kind: null,
    optional: false,
    uses_default_features: true,
    features: [],
  },
  {
    name: "serde",
    rename: null,
    req: "^1.0",
    kind: null,
    optional: false,
    uses_default_features: true,
    features: ["derive"],
  },
  {
    name: "tauri",
    rename: null,
    req: "^2.11.3",
    kind: null,
    optional: false,
    uses_default_features: true,
    features: [],
  },
  {
    name: "tauri-build",
    rename: null,
    req: "^2.6.3",
    kind: "build",
    optional: false,
    uses_default_features: true,
    features: [],
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
  rename: string | null;
  req: string;
  uses_default_features: boolean;
}> {
  return dependencies
    .map((dependency) => ({
      name: dependency.name,
      rename: dependency.rename,
      req: dependency.req,
      kind: dependency.kind,
      optional: dependency.optional,
      uses_default_features: dependency.uses_default_features,
      features: [...dependency.features].sort(),
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
  let blockScalarIndentation: number | undefined;
  const lines = workflow.replace(/\r\n/g, "\n").split("\n");
  for (const [index, line] of lines.entries()) {
    const withoutComment = line.replace(/\s+#.*$/, "");
    if (withoutComment.trim() === "") {
      continue;
    }
    const indentation = withoutComment.length - withoutComment.trimStart().length;
    if (
      blockScalarIndentation !== undefined &&
      indentation > blockScalarIndentation
    ) {
      continue;
    }
    blockScalarIndentation = undefined;
    if (REVIEWED_WORKFLOW_LINES.has(withoutComment)) {
      if (withoutComment.endsWith("run: |")) {
        blockScalarIndentation = indentation;
      }
      continue;
    }
    if (/^\s*(?:-\s*)?["']/.test(withoutComment)) {
      fail(`quoted or escaped YAML keys are not allowed on line ${index + 1}`);
    }
    if (/(?:^|:\s*|-\s*)[&*!][A-Za-z_]/.test(withoutComment)) {
      fail(`YAML anchors, tags, and aliases are not allowed on line ${index + 1}`);
    }
    if (/[\[\]{}]/.test(withoutComment)) {
      fail(`flow collections are not allowed on line ${index + 1}`);
    }
    if (indentation === 0) {
      fail(`unknown top-level YAML key on line ${index + 1}`);
    }
    if (indentation === 2) {
      fail(`workflow jobs differ from the reviewed set on line ${index + 1}`);
    }
    fail(`unsupported YAML shape on line ${index + 1}`);
  }
}

function workflowEntries(workflow: string): WorkflowEntry[] {
  const entries: WorkflowEntry[] = [];
  for (const [index, line] of workflow.replace(/\r\n/g, "\n").split("\n").entries()) {
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
  const normalized = workflow.replace(/\r\n/g, "\n");
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
    req: "^0.12.0",
    kind: "dev",
    optional: false,
    uses_default_features: true,
    features: [],
  });
  expect(() => assertCargoDependencyPolicy(addition)).toThrow("complete records");

  const featureChange = approvedDependencyFixture();
  featureChange[3].features.push("devtools");
  expect(() => assertCargoDependencyPolicy(featureChange)).toThrow("complete records");
});

it("keeps desktop CI immutable and fail-closed", () => {
  assertWorkflowPolicy(read(".github/workflows/ci.yml"));
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
