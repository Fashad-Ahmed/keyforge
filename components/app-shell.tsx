"use client";

import { useEffect, useState } from "react";

import { getAppInfo } from "@/lib/native/api";
import type { AppInfo } from "@/lib/types/app-info";

type RuntimeStatus =
  | { state: "connecting" }
  | { state: "ready"; info: AppInfo }
  | { state: "unavailable" };

export function AppShell() {
  const [runtime, setRuntime] = useState<RuntimeStatus>({
    state: "connecting",
  });

  useEffect(() => {
    let isMounted = true;

    void getAppInfo().then(
      (info) => {
        if (isMounted) {
          setRuntime({ state: "ready", info });
        }
      },
      () => {
        if (isMounted) {
          setRuntime({ state: "unavailable" });
        }
      },
    );

    return () => {
      isMounted = false;
    };
  }, []);

  return (
    <main className="min-h-screen p-8">
      <h1 className="text-3xl font-semibold">KeyForge</h1>
      <p className="mt-2 text-sm opacity-70">
        Privacy-first keyboard sound engine
      </p>

      <section className="mt-8 rounded-xl border p-4">
        <h2 className="font-medium">Native runtime</h2>
        {runtime.state === "ready" ? (
          <dl className="mt-3 space-y-1 text-sm">
            <div>Version: {runtime.info.version}</div>
            <div>Platform: {runtime.info.platform}</div>
          </dl>
        ) : runtime.state === "unavailable" ? (
          <p className="mt-3 text-sm">Native runtime unavailable</p>
        ) : (
          <p className="mt-3 text-sm">Connecting…</p>
        )}
      </section>
    </main>
  );
}
