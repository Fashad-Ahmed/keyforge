"use client";

import { useEffect, useState } from "react";

import { getAppInfo } from "@/lib/native/api";
import type { AppInfo } from "@/lib/types/app-info";

export function AppShell() {
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    void getAppInfo().then(setInfo);
  }, []);

  return (
    <main className="min-h-screen p-8">
      <h1 className="text-3xl font-semibold">KeyForge</h1>
      <p className="mt-2 text-sm opacity-70">
        Privacy-first keyboard sound engine
      </p>

      <section className="mt-8 rounded-xl border p-4">
        <h2 className="font-medium">Native runtime</h2>
        {info ? (
          <dl className="mt-3 space-y-1 text-sm">
            <div>Version: {info.version}</div>
            <div>Platform: {info.platform}</div>
          </dl>
        ) : (
          <p className="mt-3 text-sm">Connecting…</p>
        )}
      </section>
    </main>
  );
}
