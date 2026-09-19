export const SOUND_COLLECTIONS = ["all", "realistic", "enthusiast", "creative", "imported"] as const;

export type SoundCollection = (typeof SOUND_COLLECTIONS)[number];

const REALISTIC_PACK_IDS = new Set([
  "keyforge-mechanical",
  "keyforge-deep-thock",
  "keyforge-crisp-click",
  "keyforge-soft-linear",
  "keyforge-creamy-tactile",
  "keyforge-silent-mechanical",
  "keyforge-buckling-spring",
  "keyforge-vintage-typewriter",
]);

const ENTHUSIAST_PACK_IDS = new Set([
  "keyforge-marble-thock",
  "keyforge-poppy-tactile",
  "keyforge-clacky-aluminum",
  "keyforge-dampened-polycarbonate",
]);

const CREATIVE_PACK_IDS = new Set([
  "keyforge-retro-terminal",
  "keyforge-arcade",
  "keyforge-soft-office",
  "keyforge-sci-fi-console",
]);

const BUNDLED_PACK_IDS = new Set([
  ...REALISTIC_PACK_IDS,
  ...ENTHUSIAST_PACK_IDS,
  ...CREATIVE_PACK_IDS,
]);

export function isBundledPackId(packId: string): boolean {
  return BUNDLED_PACK_IDS.has(packId);
}

export function soundCollectionForPack(pack: { bundled: boolean; id: string }): Exclude<SoundCollection, "all"> {
  if (!pack.bundled) return "imported";
  if (ENTHUSIAST_PACK_IDS.has(pack.id)) return "enthusiast";
  if (CREATIVE_PACK_IDS.has(pack.id)) return "creative";
  return "realistic";
}
