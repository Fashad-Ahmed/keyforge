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

const PROJECT_ROOT = process.cwd();

function projectPath(path: string): string {
  return resolve(PROJECT_ROOT, path);
}

function read(path: string): string {
  return readFileSync(projectPath(path), "utf8");
}

function filesWithExtension(directory: string, extension: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(projectPath(directory), {
    withFileTypes: true,
  })) {
    const relativePath = `${directory}/${entry.name}`;
    if (entry.isDirectory()) {
      files.push(...filesWithExtension(relativePath, extension));
    } else if (entry.isFile() && entry.name.endsWith(extension)) {
      files.push(relativePath);
    }
  }
  return files.sort();
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

function dependencyByName(
  dependencies: CargoDependency[],
  name: string,
): CargoDependency {
  const matching = dependencies.filter((dependency) => dependency.name === name);
  expect(matching).toHaveLength(1);
  return matching[0];
}

function workflowActions(workflow: string): string[] {
  return Array.from(
    workflow.matchAll(/^\s*-\s*uses:\s*([^\s#]+)/gm),
    (match) => match[1],
  );
}

it("keeps audio outside every Tauri IPC permission and handler surface", () => {
  const capabilityFiles = filesWithExtension("src-tauri/capabilities", ".json");
  expect(capabilityFiles).toEqual(["src-tauri/capabilities/main.json"]);
  for (const capabilityFile of capabilityFiles) {
    const capability = JSON.parse(read(capabilityFile)) as Capability;
    expect(capability.permissions).toEqual([]);
  }

  const rustFiles = filesWithExtension("src-tauri/src", ".rs");
  const handlerMacros: string[] = [];
  let invokeHandlerRegistrations = 0;
  for (const rustFile of rustFiles) {
    const rust = read(rustFile);
    for (const match of rust.matchAll(/\bgenerate_handler!\s*\[([\s\S]*?)\]/g)) {
      handlerMacros.push(match[1].trim());
    }
    invokeHandlerRegistrations += (
      rust.match(/\.invoke_handler\s*\(/g) ?? []
    ).length;
  }
  expect(handlerMacros).toEqual(["commands::app_info::get_app_info"]);
  expect(invokeHandlerRegistrations).toBe(1);
});

it("uses exactly the approved direct runtime dependencies", () => {
  const metadata = cargoMetadata();
  const keyforge = metadata.packages.find(
    (candidate) =>
      candidate.name === "keyforge" &&
      metadata.workspace_members.includes(candidate.id),
  );
  expect(keyforge).toBeDefined();
  const runtimeDependencies = keyforge!.dependencies.filter(
    (dependency) => dependency.kind === null,
  );
  expect(runtimeDependencies.map((dependency) => dependency.name).sort()).toEqual(
    ["cpal", "crossbeam-queue", "serde", "tauri"],
  );

  const cpal = dependencyByName(runtimeDependencies, "cpal");
  expect({
    name: cpal.name,
    req: cpal.req,
    kind: cpal.kind,
    optional: cpal.optional,
    uses_default_features: cpal.uses_default_features,
    features: cpal.features,
  }).toEqual({
    name: "cpal",
    req: "^0.18.1",
    kind: null,
    optional: false,
    uses_default_features: false,
    features: [],
  });
  const crossbeamQueue = dependencyByName(
    runtimeDependencies,
    "crossbeam-queue",
  );
  expect({
    name: crossbeamQueue.name,
    req: crossbeamQueue.req,
    kind: crossbeamQueue.kind,
    optional: crossbeamQueue.optional,
    uses_default_features: crossbeamQueue.uses_default_features,
    features: crossbeamQueue.features,
  }).toEqual({
    name: "crossbeam-queue",
    req: "^0.3.13",
    kind: null,
    optional: false,
    uses_default_features: true,
    features: [],
  });
});

it("keeps desktop CI immutable and fail-closed", () => {
  const workflow = read(".github/workflows/ci.yml").replace(/\r\n/g, "\n");
  expect(workflow).toMatch(
    /^ {4}strategy:\n {6}fail-fast: false\n {6}matrix:\n {8}os: \[ubuntu-24\.04, macos-15, windows-2025\]\n {4}runs-on: \$\{\{ matrix\.os \}\}$/m,
  );
  expect(
    workflow.match(/^\s*os:\s*\[[^\n]*\]\s*$/gm) ?? [],
  ).toHaveLength(1);
  expect(workflow).not.toMatch(/^\s*(?:include|exclude):/m);
  expect(workflow.match(/^permissions:/gm) ?? []).toHaveLength(1);
  expect(workflow).toMatch(/^permissions:\n  contents: read$/m);
  expect(workflow).toMatch(/^  NEXT_TELEMETRY_DISABLED: "1"$/m);
  expect(workflow).toMatch(/^          toolchain: 1\.88\.0$/m);
  expect(workflowActions(workflow)).toEqual([
    "actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
    "pnpm/action-setup@f40ffcd9367d9f12939873eb1018b921a783ffaa",
    "actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020",
    "actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
    "dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c",
  ]);
  expect(workflow).toMatch(
    /^      - name: Install Tauri Linux prerequisites\n        if: runner\.os == 'Linux'$/m,
  );
  expect(workflow).toMatch(/^            libasound2-dev \\$/m);
  expect(workflow).toMatch(
    /^      - if: runner\.os == 'Linux'\n        run: cargo fmt --manifest-path src-tauri\/Cargo\.toml -- --check$/m,
  );
  expect(workflow).toMatch(
    /^      - run: cargo metadata --locked --manifest-path src-tauri\/Cargo\.toml --no-deps --format-version 1$/m,
  );
  expect(workflow).toMatch(
    /^      - run: cargo clippy --locked --manifest-path src-tauri\/Cargo\.toml --all-targets -- -D warnings$/m,
  );
  expect(workflow).toMatch(
    /^      - run: cargo test --locked --manifest-path src-tauri\/Cargo\.toml --all-targets$/m,
  );
  expect(workflow).not.toMatch(/^\s*continue-on-error\s*:/m);
  expect(workflow).not.toMatch(
    /^\s*(?:actions|attestations|checks|contents|deployments|discussions|id-token|issues|packages|pages|pull-requests|repository-projects|security-events|statuses):\s*write\s*(?:#.*)?$/im,
  );
  expect(workflow).not.toMatch(/\bsecrets\b/i);
  expect(workflow).not.toMatch(/\b(?:actions\/)?upload-artifact\b/i);
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
