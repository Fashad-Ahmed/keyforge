import { readFileSync } from "node:fs";

import { expect, it } from "vitest";

type Capability = {
  permissions: string[];
};

type SecurityConfig = {
  identifier: string;
  app: {
    security: {
      csp: Record<string, string> | null;
      devCsp?: Record<string, string> | null;
    };
  };
};

function readJson<T>(relativePath: string): T {
  return JSON.parse(
    readFileSync(new URL(relativePath, import.meta.url), "utf8"),
  ) as T;
}

it("grants the main window no built-in Tauri permissions", () => {
  const capability = readJson<Capability>(
    "../src-tauri/capabilities/main.json",
  );

  expect(capability.permissions).toEqual([]);
});

it("restricts production webview content to local assets and Tauri IPC", () => {
  const config = readJson<SecurityConfig>("../src-tauri/tauri.conf.json");

  expect(config.app.security.csp).toEqual({
    "default-src": "'self'",
    "connect-src": "ipc: http://ipc.localhost",
    "img-src": "'self' data:",
    "style-src": "'self'",
  });
});

it("uses a bundle identifier that does not conflict with macOS app bundles", () => {
  const config = readJson<SecurityConfig>("../src-tauri/tauri.conf.json");

  expect(config.identifier).toBe("org.keyforge.desktop");
  expect(config.identifier.endsWith(".app")).toBe(false);
});
