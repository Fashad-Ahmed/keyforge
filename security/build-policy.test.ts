import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

import { expect, it } from "vitest";

type PackageJson = {
  scripts: {
    dev: string;
  };
};

type TauriConfig = {
  build: {
    devUrl: string;
  };
};

it("disables Next.js telemetry for project workflows", () => {
  const envFile = resolve(process.cwd(), ".env");

  expect(existsSync(envFile)).toBe(true);
  expect(readFileSync(envFile, "utf8")).toContain(
    "NEXT_TELEMETRY_DISABLED=1",
  );
});

it("uses the same IPv4 loopback origin across development tooling", async () => {
  const packageJson = JSON.parse(
    readFileSync(resolve(process.cwd(), "package.json"), "utf8"),
  ) as PackageJson;
  const tauriConfig = JSON.parse(
    readFileSync(resolve(process.cwd(), "src-tauri/tauri.conf.json"), "utf8"),
  ) as TauriConfig;
  const nextConfig = (await import("../next.config")).default;

  expect(packageJson.scripts.dev).toContain("--hostname 127.0.0.1");
  expect(tauriConfig.build.devUrl).toBe("http://127.0.0.1:3000");
  expect(nextConfig.assetPrefix).toBe("http://127.0.0.1:3000");
});
