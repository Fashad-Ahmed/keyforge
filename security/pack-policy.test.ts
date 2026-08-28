import { existsSync, readFileSync } from "node:fs";

import { expect, it } from "vitest";

it("keeps the sound-pack smoke path developer-only", () => {
  expect(existsSync("src-tauri/examples/pack_smoke.rs")).toBe(true);

  const nativeStartup = readFileSync("src-tauri/src/lib.rs", "utf8");
  expect(nativeStartup).not.toContain("PackManager::open");
  expect(nativeStartup).not.toContain("AudioEngine::start");
});
