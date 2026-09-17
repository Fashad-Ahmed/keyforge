import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";

import { AppShell } from "./app-shell";

const {
  getAppInfoMock,
  getRuntimeStatusMock,
  setMasterVolumeMock,
  setSoundEnabledMock,
} = vi.hoisted(() => ({
  getAppInfoMock: vi.fn(),
  getRuntimeStatusMock: vi.fn(),
  setMasterVolumeMock: vi.fn(),
  setSoundEnabledMock: vi.fn(),
}));

vi.mock("@/lib/native/api", () => ({
  getAppInfo: getAppInfoMock,
  getRuntimeStatus: getRuntimeStatusMock,
  setMasterVolume: setMasterVolumeMock,
  setSoundEnabled: setSoundEnabledMock,
}));

beforeEach(() => {
  getAppInfoMock.mockReset().mockResolvedValue({
    name: "KeyForge",
    version: "0.1.0",
    platform: "macos",
  });
  getRuntimeStatusMock.mockReset().mockResolvedValue({
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
  });
  setSoundEnabledMock.mockReset().mockImplementation((enabled: boolean) =>
    Promise.resolve({
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
      soundEnabled: enabled,
      volume: 1,
    }),
  );
  setMasterVolumeMock.mockReset().mockImplementation((volume: number) =>
    Promise.resolve({
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
      volume,
    }),
  );
});

it("renders native runtime information", async () => {
  render(<AppShell />);

  expect(
    screen.getByRole("heading", { level: 1, name: "KeyForge" }),
  ).toBeInTheDocument();
  expect(await screen.findByText("Version: 0.1.0")).toBeInTheDocument();
  expect(screen.getByText("Platform: macos")).toBeInTheDocument();
  expect(await screen.findByText("Audio: ready")).toBeInTheDocument();
  expect(screen.getByText("Input: ready")).toBeInTheDocument();
  expect(screen.getByText("Pack: KeyForge Mechanical")).toBeInTheDocument();
  expect(screen.getByText("Normal variants: 3")).toBeInTheDocument();
});

it("renders a non-sensitive status when native runtime information fails", async () => {
  getAppInfoMock.mockRejectedValue(new Error("sensitive native detail"));

  render(<AppShell />);

  expect(
    await screen.findByText("Native runtime unavailable"),
  ).toBeInTheDocument();
  expect(screen.queryByText("sensitive native detail")).not.toBeInTheDocument();
});

it("toggles sound playback through the native runtime", async () => {
  render(<AppShell />);

  const toggle = await screen.findByRole("checkbox", {
    name: "Sound playback",
  });
  fireEvent.click(toggle);

  expect(setSoundEnabledMock).toHaveBeenCalledWith(false);
});

it("updates master volume through the native runtime", async () => {
  render(<AppShell />);

  const slider = await screen.findByRole("slider", { name: "Volume" });
  fireEvent.change(slider, { target: { value: "25" } });

  expect(setMasterVolumeMock).toHaveBeenCalledWith(0.25);
});
