import { PlaybackControls } from "@/components/keyforge/playback-controls";
import type { PackSummary, RuntimeStatus } from "@/lib/types/runtime";

type CompactControlsProps = {
  status?: RuntimeStatus;
  packs: PackSummary[];
  pendingAction: string | null;
  message?: { kind: "error" | "success"; text: string } | null;
  onEnabledChange: (enabled: boolean) => void;
  onVolumeChange: (volume: number) => void;
  onSelectPack: (packId: string) => void;
  onManageSounds: () => void;
};

const INPUT_STATUS: Record<RuntimeStatus["inputStatus"], string> = {
  starting: "Input engine starting",
  ready: "Input engine ready",
  unsupported: "Input is unsupported",
  permission_denied: "Input permission needed",
  unavailable: "Input engine unavailable",
};

export function CompactControls({
  status,
  packs,
  pendingAction,
  message,
  onEnabledChange,
  onVolumeChange,
  onSelectPack,
  onManageSounds,
}: CompactControlsProps) {
  const activePack = packs.find((pack) => pack.id === status?.packId);

  return (
    <main className="compact-shell" aria-label="KeyForge sound controls">
      <header className="compact-header">
        <span className="compact-mark" aria-hidden="true">K</span>
        <div className="compact-brand">
          <p>KEYFORGE <span>/ LOCAL</span></p>
          <p className="compact-status">
            <span className={`status-dot${status?.inputStatus === "ready" ? " is-ready" : ""}`} aria-hidden="true" />
            {status ? INPUT_STATUS[status.inputStatus] : "Sound engine unavailable"}
          </p>
        </div>
      </header>

      {status ? (
        <>
          <PlaybackControls
            className="compact-playback-controls"
            enabled={status.soundEnabled}
            enabledPending={pendingAction === "enabled"}
            onEnabledChange={onEnabledChange}
            onVolumeChange={onVolumeChange}
            volume={status.volume}
            volumePending={pendingAction === "volume"}
          />

          <section className="compact-sound" aria-labelledby="compact-sound-title">
            <div className="compact-sound-heading">
              <h2 id="compact-sound-title">Sound profile</h2>
              {activePack ? <span>{activePack.groupCounts.normal} voices</span> : null}
            </div>
            {packs.length > 0 ? (
              <select
                aria-label="Sound profile"
                disabled={pendingAction !== null}
                onChange={(event) => onSelectPack(event.currentTarget.value)}
                value={activePack?.id ?? ""}
              >
                {!activePack ? <option value="">Choose a sound</option> : null}
                {packs.map((pack) => (
                  <option key={pack.id} value={pack.id}>{pack.name}</option>
                ))}
              </select>
            ) : (
              <p className="compact-empty">No installed sounds. Manage sounds to import one.</p>
            )}
          </section>
        </>
      ) : (
        <p className="compact-unavailable">The local sound engine could not be reached. Your typing stays private.</p>
      )}

      {message ? (
        <p className={`compact-message compact-message-${message.kind}`} role={message.kind === "error" ? "alert" : "status"}>
          {message.text}
        </p>
      ) : null}

      <button className="compact-manage-button" onClick={onManageSounds} type="button">
        Manage sounds <span aria-hidden="true">↗</span>
      </button>
    </main>
  );
}
