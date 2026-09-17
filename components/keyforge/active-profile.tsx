import type { RuntimeAudioStatus, RuntimeGroupCounts } from "@/lib/types/runtime";

type ActiveProfileProps = { audioStatus: RuntimeAudioStatus; groupCounts: RuntimeGroupCounts; packName: string };

export function ActiveProfile({ audioStatus, groupCounts, packName }: ActiveProfileProps) {
  const title = `${packName.replace(/^KeyForge\s+/u, "")} Precision`;
  return (
    <section className="profile-panel" aria-labelledby="profile-title">
      <p className="eyebrow">Loaded profile</p>
      <h2 id="profile-title">{title}</h2>
      <p className="profile-description">
        <span>{groupCounts.normal} primary voices. Local, responsive playback.</span>
        <span>On-device processing only.</span>
      </p>
      <dl className="runtime-readout" aria-label="Runtime details">
        <div>Audio: {audioStatus}</div>
        <div>Pack: {packName}</div>
        <div>Normal variants: {groupCounts.normal}</div>
      </dl>
    </section>
  );
}
