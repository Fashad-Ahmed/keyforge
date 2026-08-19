import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(resolve(process.cwd(), path), "utf8");
}

it("keeps audio native and outside the Tauri IPC permission surface", () => {
  const rust = read("src-tauri/src/lib.rs");
  const capability = JSON.parse(read("src-tauri/capabilities/main.json")) as {
    permissions: string[];
  };
  expect(rust).toContain(
    "tauri::generate_handler![commands::app_info::get_app_info]",
  );
  expect(rust).not.toMatch(/generate_handler!\[[^\]]*audio/);
  expect(capability.permissions).toEqual([]);
});

it("uses only the approved minimal audio dependencies", () => {
  const cargo = read("src-tauri/Cargo.toml");
  expect(cargo).toContain(
    'cpal = { version = "0.18.1", default-features = false }',
  );
  expect(cargo).toContain('crossbeam-queue = "0.3.13"');
  expect(cargo).not.toMatch(/rodio|kira|symphonia|reqwest|tracing|log\s*=/);
});

it("compiles native audio on all desktop targets", () => {
  const workflow = read(".github/workflows/ci.yml");
  expect(workflow).toContain(
    "os: [ubuntu-24.04, macos-15, windows-2025]",
  );
  expect(workflow).toContain("libasound2-dev");
  expect(workflow).toContain(
    "cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets",
  );
});
