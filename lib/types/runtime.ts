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

export type RuntimeStatus = {
  audioStatus: RuntimeAudioStatus;
  groupCounts: RuntimeGroupCounts;
  inputStatus: RuntimeInputStatus;
  packId: string;
  packName: string;
  soundEnabled: boolean;
  volume: number;
};
