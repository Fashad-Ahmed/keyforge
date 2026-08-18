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
