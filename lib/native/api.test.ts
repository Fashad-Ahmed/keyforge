import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

describe("getAppInfo", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("invokes the get_app_info command and returns its result", async () => {
    invokeMock.mockResolvedValue({
      name: "KeyForge",
      version: "0.1.0",
      platform: "macos",
    });

    const { getAppInfo } = await import("./api");
    const result = await getAppInfo();

    expect(invokeMock).toHaveBeenCalledOnce();
    expect(invokeMock).toHaveBeenCalledWith("get_app_info");
    expect(result).toEqual({
      name: "KeyForge",
      version: "0.1.0",
      platform: "macos",
    });
  });
});

describe("runtime controls", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("invokes the get_runtime_status command", async () => {
    const status = {
      audioStatus: "ready",
      groupCounts: {
        normal: 3,
        space: 1,
        enter: 1,
        backspace: 1,
        modifier: 1,
      },
      inputStatus: "ready",
      packId: "keyforge-mechanical",
      packName: "KeyForge Mechanical",
      soundEnabled: true,
      volume: 1,
    };
    invokeMock.mockResolvedValue(status);

    const { getRuntimeStatus } = await import("./api");

    await expect(getRuntimeStatus()).resolves.toEqual(status);
    expect(invokeMock).toHaveBeenCalledWith("get_runtime_status");
  });

  it("invokes the set_sound_enabled command with an explicit boolean", async () => {
    invokeMock.mockResolvedValue({ soundEnabled: false });

    const { setSoundEnabled } = await import("./api");

    await setSoundEnabled(false);
    expect(invokeMock).toHaveBeenCalledWith("set_sound_enabled", {
      enabled: false,
    });
  });

  it("invokes the set_master_volume command with an explicit volume", async () => {
    invokeMock.mockResolvedValue({ volume: 0.25 });

    const { setMasterVolume } = await import("./api");

    await setMasterVolume(0.25);
    expect(invokeMock).toHaveBeenCalledWith("set_master_volume", {
      volume: 0.25,
    });
  });
});
