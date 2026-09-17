import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";

import { AppShell } from "./app-shell";

const {
  getAppInfoMock,
  getRuntimeStatusMock,
  importSoundPackMock,
  selectSoundPackMock,
  setMasterVolumeMock,
  setSoundEnabledMock,
} = vi.hoisted(() => ({
  getAppInfoMock: vi.fn(),
  getRuntimeStatusMock: vi.fn(),
  importSoundPackMock: vi.fn(),
  selectSoundPackMock: vi.fn(),
  setMasterVolumeMock: vi.fn(),
  setSoundEnabledMock: vi.fn(),
}));

vi.mock("@/lib/native/api", () => ({
  getAppInfo: getAppInfoMock,
  getRuntimeStatus: getRuntimeStatusMock,
  importSoundPack: importSoundPackMock,
  selectSoundPack: selectSoundPackMock,
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
    packs: [
      {
        active: true,
        bundled: true,
        groupCounts: { normal: 3, space: 1, enter: 1, backspace: 1, modifier: 1 },
        id: "keyforge-mechanical",
        name: "KeyForge Mechanical",
      },
      {
        active: false,
        bundled: false,
        groupCounts: { normal: 2, space: 1, enter: 1, backspace: 1, modifier: 1 },
        id: "quiet-linear",
        name: "Quiet Linear",
      },
    ],
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
      packs: [],
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
      packs: [],
      soundEnabled: true,
      volume,
    }),
  );
  importSoundPackMock.mockReset().mockResolvedValue({ status: "cancelled" });
  selectSoundPackMock.mockReset().mockResolvedValue({
    audioStatus: "ready",
    groupCounts: { normal: 2, space: 1, enter: 1, backspace: 1, modifier: 1 },
    inputStatus: "ready",
    packId: "quiet-linear",
    packName: "Quiet Linear",
    packs: [
      {
        active: false,
        bundled: true,
        groupCounts: { normal: 3, space: 1, enter: 1, backspace: 1, modifier: 1 },
        id: "keyforge-mechanical",
        name: "KeyForge Mechanical",
      },
      {
        active: true,
        bundled: false,
        groupCounts: { normal: 2, space: 1, enter: 1, backspace: 1, modifier: 1 },
        id: "quiet-linear",
        name: "Quiet Linear",
      },
    ],
    soundEnabled: true,
    volume: 1,
  });
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

it("renders the approved private precision hierarchy and local library", async () => {
  render(<AppShell />);

  expect(await screen.findByText("INPUT ENGINE READY")).toBeInTheDocument();
  expect(screen.getByText("Mechanical Precision")).toBeInTheDocument();
  expect(screen.getByText("On-device processing only.")).toBeInTheDocument();
  expect(screen.getByText("Quiet Linear")).toBeInTheDocument();
});

it("opens native import without collecting a path", async () => {
  render(<AppShell />);

  fireEvent.click(await screen.findByRole("button", { name: "Import local pack" }));

  expect(importSoundPackMock).toHaveBeenCalledWith();
});

it("reconciles selected pack from the authoritative native snapshot", async () => {
  render(<AppShell />);

  fireEvent.click(await screen.findByRole("button", { name: "Select Quiet Linear" }));

  expect(selectSoundPackMock).toHaveBeenCalledWith("quiet-linear");
  expect(await screen.findByText("Quiet Linear Precision")).toBeInTheDocument();
});

it("shows a sanitized pack error without leaking native details", async () => {
  selectSoundPackMock.mockRejectedValue(new Error("/Users/private/secret.zip"));
  render(<AppShell />);

  fireEvent.click(await screen.findByRole("button", { name: "Select Quiet Linear" }));

  expect(await screen.findByText("The pack could not be activated. Your current sound is still active.")).toBeInTheDocument();
  expect(screen.queryByText(/secret\.zip/u)).not.toBeInTheDocument();
});
