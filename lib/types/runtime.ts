export type RuntimeAudioStatus =
  | "starting"
  | "ready"
  | "recovering"
  | "unavailable"
  | "stopped";

export type RuntimeInputStatus =
  | "starting"
  | "ready"
  | "unsupported"
  | "permission_denied"
  | "unavailable";

export type RuntimeGroupCounts = {
  normal: number;
  space: number;
  enter: number;
  backspace: number;
  modifier: number;
};

export type PackSummary = {
  active: boolean;
  bundled: boolean;
  groupCounts: RuntimeGroupCounts;
  id: string;
  name: string;
};

export type RuntimeStatus = {
  audioStatus: RuntimeAudioStatus;
  groupCounts: RuntimeGroupCounts;
  inputStatus: RuntimeInputStatus;
  packId: string;
  packName: string;
  packs: PackSummary[];
  soundEnabled: boolean;
  volume: number;
};

export type ImportOutcome =
  | { status: "cancelled" }
  | { status: "installed"; snapshot: RuntimeStatus };
