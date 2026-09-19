import { invoke } from "@tauri-apps/api/core";

import type { AppInfo } from "@/lib/types/app-info";
import type { ImportOutcome, RuntimeStatus } from "@/lib/types/runtime";

export async function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}

export async function getRuntimeStatus(): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>("get_runtime_status");
}

export async function setSoundEnabled(enabled: boolean): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>("set_sound_enabled", { enabled });
}

export async function setMasterVolume(volume: number): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>("set_master_volume", { volume });
}

export async function importSoundPack(): Promise<ImportOutcome> {
  return invoke<ImportOutcome>("import_sound_pack");
}

export async function selectSoundPack(packId: string): Promise<RuntimeStatus> {
  return invoke<RuntimeStatus>("select_sound_pack", { packId });
}
