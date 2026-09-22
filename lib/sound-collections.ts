export const SOUND_COLLECTIONS = ["all", "real-switches", "playful", "earlier-synths", "imported"] as const;

export type SoundCollection = (typeof SOUND_COLLECTIONS)[number];

const REAL_SWITCH_PACK_IDS = new Set([
  "keyforge-switch-linear",
  "keyforge-switch-tactile",
  "keyforge-switch-clicky",
]);

const PLAYFUL_PACK_IDS = new Set([
  "keyforge-playful-bubble",
  "keyforge-playful-duck",
  "keyforge-playful-boing",
]);

const EARLIER_SYNTH_PACK_IDS = new Set([
  "keyforge-mechanical",
  "keyforge-deep-thock",
  "keyforge-crisp-click",
  "keyforge-soft-linear",
  "keyforge-creamy-tactile",
  "keyforge-silent-mechanical",
  "keyforge-buckling-spring",
  "keyforge-vintage-typewriter",
  "keyforge-marble-thock",
  "keyforge-poppy-tactile",
  "keyforge-clacky-aluminum",
  "keyforge-dampened-polycarbonate",
  "keyforge-retro-terminal",
  "keyforge-arcade",
  "keyforge-soft-office",
  "keyforge-sci-fi-console",
]);

const BUNDLED_PACK_IDS = new Set([
  ...REAL_SWITCH_PACK_IDS,
  ...PLAYFUL_PACK_IDS,
  ...EARLIER_SYNTH_PACK_IDS,
]);

export function isBundledPackId(packId: string): boolean {
  return BUNDLED_PACK_IDS.has(packId);
}

export function soundCollectionForPack(pack: { bundled: boolean; id: string }): Exclude<SoundCollection, "all"> {
  if (!pack.bundled) return "imported";
  if (REAL_SWITCH_PACK_IDS.has(pack.id)) return "real-switches";
  if (PLAYFUL_PACK_IDS.has(pack.id)) return "playful";
  return "earlier-synths";
}
