import type { RuntimeAudioStatus, RuntimeGroupCounts } from "@/lib/types/runtime";

type ActiveProfileProps = { audioStatus: RuntimeAudioStatus; groupCounts: RuntimeGroupCounts; packName: string };

export function ActiveProfile({ audioStatus, groupCounts, packName }: ActiveProfileProps) {
  const profile = packName.replace(/^KeyForge\s+/u, "");
  return (
    <section className="profile-panel" aria-labelledby="profile-title">
      <p className="eyebrow">Loaded profile</p>
      <h2 aria-label={`${profile} Precision`} id="profile-title">
        <span>{profile}</span>
        <span>Precision</span>
      </h2>
      <p className="profile-description">
        <span>{groupCounts.normal} voices · 48 kHz</span>
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
