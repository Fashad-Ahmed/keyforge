"use client";

import { useEffect, useState } from "react";

import {
  getAppInfo,
  getRuntimeStatus,
  setMasterVolume,
  setSoundEnabled,
} from "@/lib/native/api";
import type { AppInfo } from "@/lib/types/app-info";
import type { RuntimeStatus } from "@/lib/types/runtime";

type NativeRuntime =
  | { state: "connecting" }
  | { state: "ready"; info: AppInfo }
  | { state: "unavailable" };

type SoundRuntime =
  | { state: "connecting" }
  | { state: "ready"; status: RuntimeStatus }
  | { state: "unavailable" };

export function AppShell() {
  const [runtime, setRuntime] = useState<NativeRuntime>({
    state: "connecting",
  });
  const [soundRuntime, setSoundRuntime] = useState<SoundRuntime>({
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

    void getRuntimeStatus().then(
      (status) => {
        if (isMounted) {
          setSoundRuntime({ state: "ready", status });
        }
      },
      () => {
        if (isMounted) {
          setSoundRuntime({ state: "unavailable" });
        }
      },
    );

    return () => {
      isMounted = false;
    };
  }, []);

  const soundStatus =
    soundRuntime.state === "ready" ? soundRuntime.status : undefined;

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

      <section className="mt-4 rounded-xl border p-4">
        <h2 className="font-medium">Sound runtime</h2>
        {soundStatus ? (
          <>
            <dl className="mt-3 space-y-1 text-sm">
              <div>Audio: {soundStatus.audioStatus}</div>
              <div>Input: {soundStatus.inputStatus}</div>
              <div>Pack: {soundStatus.packName}</div>
              <div>Normal variants: {soundStatus.groupCounts.normal}</div>
            </dl>

            <div className="mt-5 space-y-4 text-sm">
              <label className="flex items-center gap-2">
                <input
                  checked={soundStatus.soundEnabled}
                  onChange={(event) => {
                    const enabled = event.currentTarget.checked;
                    void setSoundEnabled(enabled).then(
                      (status) => {
                        setSoundRuntime({ state: "ready", status });
                      },
                      () => {
                        setSoundRuntime({ state: "unavailable" });
                      },
                    );
                  }}
                  type="checkbox"
                />
                Sound playback
              </label>

              <label className="block">
                <span>Volume</span>
                <input
                  aria-label="Volume"
                  className="mt-2 block w-full"
                  max="100"
                  min="0"
                  onChange={(event) => {
                    const volume = Number(event.currentTarget.value) / 100;
                    void setMasterVolume(volume).then(
                      (status) => {
                        setSoundRuntime({ state: "ready", status });
                      },
                      () => {
                        setSoundRuntime({ state: "unavailable" });
                      },
                    );
                  }}
                  type="range"
                  value={Math.round(soundStatus.volume * 100)}
                />
              </label>
            </div>
          </>
        ) : soundRuntime.state === "unavailable" ? (
          <p className="mt-3 text-sm">Sound runtime unavailable</p>
        ) : (
          <p className="mt-3 text-sm">Connecting…</p>
        )}
      </section>
    </main>
  );
}
