"use client";

import { useCallback, useEffect, useState } from "react";

import { ActiveProfile } from "@/components/keyforge/active-profile";
import { CompactControls } from "@/components/keyforge/compact-controls";
import { OperationMessage } from "@/components/keyforge/operation-message";
import { PackLibrary } from "@/components/keyforge/pack-library";
import { PlaybackControls } from "@/components/keyforge/playback-controls";
import { StatusHeader } from "@/components/keyforge/status-header";
import {
  getAppInfo,
  getRuntimeStatus,
  importSoundPack,
  selectSoundPack,
  setPanelPresentation,
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
type Message = { kind: "error" | "success"; text: string } | null;

const BUNDLED_PACK_IDS = new Set([
  "keyforge-mechanical",
  "keyforge-deep-thock",
  "keyforge-crisp-click",
  "keyforge-soft-linear",
]);

export function AppShell() {
  const [runtime, setRuntime] = useState<NativeRuntime>({ state: "connecting" });
  const [soundRuntime, setSoundRuntime] = useState<SoundRuntime>({ state: "connecting" });
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [message, setMessage] = useState<Message>(null);
  const [presentation, setPresentation] = useState<"controls" | "manage">("controls");

  useEffect(() => {
    let isMounted = true;
    void getAppInfo().then(
      (info) => {
        if (!isMounted) return;
        setRuntime({ state: "ready", info });
        setPresentation(info.platform === "macos" && window.innerWidth < 500 ? "controls" : "manage");
      },
      () => {
        if (!isMounted) return;
        setRuntime({ state: "unavailable" });
        setPresentation("manage");
      },
    );
    void getRuntimeStatus().then(
      (status) => isMounted && setSoundRuntime({ state: "ready", status }),
      () => isMounted && setSoundRuntime({ state: "unavailable" }),
    );
    return () => { isMounted = false; };
  }, []);

  const isMacOS = runtime.state === "ready" && runtime.info.platform === "macos";

  useEffect(() => {
    if (!isMacOS) return;
    const syncPresentationToNativeSize = () => {
      setPresentation(window.innerWidth < 500 ? "controls" : "manage");
    };
    window.addEventListener("resize", syncPresentationToNativeSize);
    return () => window.removeEventListener("resize", syncPresentationToNativeSize);
  }, [isMacOS]);

  const changePanelPresentation = useCallback(async (request: "controls" | "manage" | "dismiss") => {
    try {
      await setPanelPresentation(request);
      if (request !== "dismiss") setPresentation(request);
    } catch {
      setMessage({ kind: "error", text: "The control panel could not be changed. The current view remains open." });
    }
  }, []);

  useEffect(() => {
    if (!isMacOS || presentation !== "controls") return;
    const dismissOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      void changePanelPresentation("dismiss");
    };
    window.addEventListener("keydown", dismissOnEscape);
    return () => window.removeEventListener("keydown", dismissOnEscape);
  }, [changePanelPresentation, isMacOS, presentation]);

  const soundStatus = soundRuntime.state === "ready" ? soundRuntime.status : undefined;
  const visiblePacks = soundStatus && soundStatus.packs.length > 0
    ? soundStatus.packs
    : soundStatus
      ? [{
          active: true,
          bundled: BUNDLED_PACK_IDS.has(soundStatus.packId),
          groupCounts: soundStatus.groupCounts,
          id: soundStatus.packId,
          name: soundStatus.packName,
        }]
      : [];

  if (isMacOS && presentation === "controls") {
    return (
      <CompactControls
        onEnabledChange={(enabled) => void updateEnabled(enabled)}
        onManageSounds={() => void changePanelPresentation("manage")}
        onSelectPack={(packId) => void selectPack(packId)}
        onVolumeChange={(volume) => void updateVolume(volume)}
        message={message}
        packs={visiblePacks}
        pendingAction={pendingAction}
        status={soundStatus}
      />
    );
  }

  async function updateEnabled(enabled: boolean) {
    setPendingAction("enabled");
    setMessage(null);
    try {
      setSoundRuntime({ state: "ready", status: await setSoundEnabled(enabled) });
    } catch {
      setMessage({ kind: "error", text: "Sound playback could not be changed." });
    } finally {
      setPendingAction(null);
    }
  }

  async function updateVolume(volume: number) {
    setPendingAction("volume");
    setMessage(null);
    try {
      setSoundRuntime({ state: "ready", status: await setMasterVolume(volume) });
    } catch {
      setMessage({ kind: "error", text: "Output volume could not be changed." });
    } finally {
      setPendingAction(null);
    }
  }

  async function importPack() {
    setPendingAction("import");
    setMessage(null);
    try {
      const outcome = await importSoundPack();
      if (outcome.status === "installed") {
        setSoundRuntime({ state: "ready", status: outcome.snapshot });
        setMessage({ kind: "success", text: "Pack installed and activated." });
      }
    } catch {
      setMessage({ kind: "error", text: "The pack could not be imported. No changes were made." });
    } finally {
      setPendingAction(null);
    }
  }

  async function selectPack(packId: string) {
    setPendingAction(`pack:${packId}`);
    setMessage(null);
    try {
      setSoundRuntime({ state: "ready", status: await selectSoundPack(packId) });
    } catch {
      setMessage({ kind: "error", text: "The pack could not be activated. Your current sound is still active." });
    } finally {
      setPendingAction(null);
    }
  }

  return (
    <main className="keyforge-shell">
      <div className="instrument-frame">
        <StatusHeader
          info={runtime.state === "ready" ? runtime.info : undefined}
          inputStatus={soundStatus?.inputStatus}
          nativeUnavailable={runtime.state === "unavailable"}
        />
        {soundStatus ? (
        <div className="instrument-body">
          {isMacOS ? (
            <div className="manager-toolbar">
              <span>Sound library</span>
              <button onClick={() => void changePanelPresentation("controls")} type="button">Back to controls</button>
            </div>
          ) : null}
          <div className="primary-grid">
              <div>
                <ActiveProfile audioStatus={soundStatus.audioStatus} groupCounts={soundStatus.groupCounts} packName={soundStatus.packName} />
                <span className="input-detail">Input: {soundStatus.inputStatus}</span>
              </div>
              <PlaybackControls
                enabled={soundStatus.soundEnabled}
                enabledPending={pendingAction === "enabled"}
                onEnabledChange={(enabled) => void updateEnabled(enabled)}
                onVolumeChange={(volume) => void updateVolume(volume)}
                volume={soundStatus.volume}
                volumePending={pendingAction === "volume"}
              />
            </div>
            <OperationMessage message={message} />
            <PackLibrary
              onImport={() => void importPack()}
              onSelect={(packId) => void selectPack(packId)}
              packs={visiblePacks}
              pendingAction={pendingAction}
            />
          </div>
        ) : (
          <div className="runtime-empty">
            <p>{soundRuntime.state === "unavailable" ? "Sound runtime unavailable" : "Connecting to local engine..."}</p>
          </div>
        )}
        <footer className="instrument-footer"><span>Private by design</span><span>Local processing · No telemetry</span></footer>
      </div>
    </main>
  );
}
