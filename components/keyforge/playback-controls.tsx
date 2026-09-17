import { useEffect, useState, type CSSProperties } from "react";

type PlaybackControlsProps = {
  enabled: boolean;
  enabledPending: boolean;
  volumePending: boolean;
  volume: number;
  onEnabledChange: (enabled: boolean) => void;
  onVolumeChange: (volume: number) => void;
};

export function PlaybackControls({ enabled, enabledPending, volume, volumePending, onEnabledChange, onVolumeChange }: PlaybackControlsProps) {
  const [draftVolume, setDraftVolume] = useState(volume);
  useEffect(() => setDraftVolume(volume), [volume]);
  const percentage = Math.round(draftVolume * 100);
  return (
    <section className="controls-panel" aria-labelledby="controls-title">
      <div className="control-heading">
        <div><p className="eyebrow">Engine</p><h2 id="controls-title">{enabled ? "Running" : "Paused"}</h2></div>
        <label className="toggle-control">
          <input aria-label="Sound playback" checked={enabled} disabled={enabledPending} onChange={(event) => onEnabledChange(event.currentTarget.checked)} type="checkbox" />
          <span aria-hidden="true" className="toggle-track"><span /></span>
        </label>
      </div>
      <div className="volume-control">
        <div className="volume-label"><label htmlFor="master-volume">Output</label><output htmlFor="master-volume">{percentage}</output></div>
        <input aria-label="Volume" disabled={volumePending} id="master-volume" max="100" min="0" onChange={(event) => {
          const nextVolume = Number(event.currentTarget.value) / 100;
          setDraftVolume(nextVolume);
          onVolumeChange(nextVolume);
        }} style={{ "--volume": `${percentage}%` } as CSSProperties} type="range" value={percentage} />
      </div>
    </section>
  );
}
