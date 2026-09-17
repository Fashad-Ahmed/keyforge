import { readFileSync } from "node:fs";
import { readdirSync } from "node:fs";
import { join } from "node:path";

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

function sourceFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory()
      ? sourceFiles(path)
      : /\.(?:ts|tsx)$/u.test(entry.name)
        ? [path]
        : [];
  });
}

it("keeps the static frontend free of network, persistence, and native path fields", () => {
  const sources = [...sourceFiles("app"), ...sourceFiles("components"), ...sourceFiles("lib")]
    .map((path) => readFileSync(path, "utf8"))
    .join("\n");

  expect(sources).not.toMatch(/\b(?:fetch|WebSocket|EventSource)\s*\(/u);
  expect(sources).not.toMatch(/\b(?:localStorage|sessionStorage|indexedDB|document\.cookie)\b/u);
  expect(readFileSync("lib/types/runtime.ts", "utf8")).not.toMatch(
    /\b(?:path|file|archive|sample|keyCode|rawEvent)\s*:/iu,
  );
});

it("documents product settings, import, activation, and tray boundaries", () => {
  const documents = [
    "README.md",
    "SECURITY.md",
    "docs/architecture/trust-boundaries.md",
    "docs/security/threat-model.md",
  ].map((path) => readFileSync(path, "utf8").toLowerCase()).join("\n");

  for (const phrase of [
    "sound enabled, master volume, and selected pack id",
    "rust-only native file picker",
    "prepare-then-commit activation",
    "close-to-tray",
    "sanitized failures",
    "autostart, networking, updates, and windows/linux input hooks remain excluded",
  ]) {
    expect(documents).toContain(phrase);
  }
});

it("uses a focused portrait product window", () => {
  const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")) as {
    app: { windows: Array<Record<string, unknown>> };
  };

  expect(config.app.windows).toEqual([
    expect.objectContaining({
      height: 800,
      minHeight: 680,
      minWidth: 600,
      width: 760,
    }),
  ]);
});
