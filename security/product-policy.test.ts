import { readFileSync } from "node:fs";

import { expect, it } from "vitest";

const EXPECTED_HANDLERS = [
  "commands::app_info::get_app_info",
  "commands::runtime::get_runtime_status",
  "commands::runtime::set_sound_enabled",
  "commands::runtime::set_master_volume",
  "commands::runtime::import_sound_pack",
  "commands::runtime::select_sound_pack",
].join(",");

it("keeps product IPC and capabilities exact", () => {
  const capability = JSON.parse(
    readFileSync("src-tauri/capabilities/main.json", "utf8"),
  ) as { permissions: unknown[] };
  const source = readFileSync("src-tauri/src/lib.rs", "utf8");
  const handlers = [...source.matchAll(/generate_handler!\s*\[([^\]]*)\]/gu)];

  expect(capability.permissions).toEqual([]);
  expect(handlers).toHaveLength(1);
  expect(handlers[0]?.[1]?.replace(/\s+/gu, "")).toBe(EXPECTED_HANDLERS);
});

it("does not accept archive paths over product IPC", () => {
  const source = readFileSync("src-tauri/src/commands/runtime.rs", "utf8");
  const command = source.match(
    /pub fn import_sound_pack\s*\(([^)]*)\)/u,
  );

  expect(command?.[1] ?? "").not.toMatch(/path|file|archive/iu);
});
