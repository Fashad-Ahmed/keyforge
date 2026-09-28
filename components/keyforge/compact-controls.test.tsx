import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { CompactControls } from "./compact-controls";
import type { RuntimeStatus } from "@/lib/types/runtime";

const status: RuntimeStatus = {
  audioStatus: "ready",
  groupCounts: { normal: 3, space: 1, enter: 1, backspace: 1, modifier: 1 },
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
      bundled: true,
      groupCounts: { normal: 2, space: 1, enter: 1, backspace: 1, modifier: 1 },
      id: "quiet-linear",
      name: "Quiet Linear",
    },
  ],
  soundEnabled: true,
  volume: 0.68,
};

function renderControls(overrides: Partial<React.ComponentProps<typeof CompactControls>> = {}) {
  const props: React.ComponentProps<typeof CompactControls> = {
    status,
    packs: status.packs,
    pendingAction: null,
    onEnabledChange: vi.fn(),
    onVolumeChange: vi.fn(),
    onSelectPack: vi.fn(),
    onManageSounds: vi.fn(),
    ...overrides,
  };
  render(<CompactControls {...props} />);
  return props;
}

describe("CompactControls", () => {
  it("exposes an accessible playback switch, continuous volume, and selected sound", () => {
    renderControls();

    expect(screen.getByRole("checkbox", { name: "Sound playback" })).toBeChecked();
    expect(screen.getByRole("slider", { name: "Volume" })).toHaveValue("680");
    expect(screen.getByRole("combobox", { name: "Sound profile" })).toHaveValue("keyforge-mechanical");
    expect(screen.getByText("KeyForge Mechanical")).toBeInTheDocument();
  });

  it("routes enablement, sound selection, volume, and manage actions to the supplied handlers", async () => {
    const props = renderControls();

    fireEvent.click(screen.getByRole("checkbox", { name: "Sound playback" }));
    fireEvent.change(screen.getByRole("combobox", { name: "Sound profile" }), {
      target: { value: "quiet-linear" },
    });
    fireEvent.change(screen.getByRole("slider", { name: "Volume" }), {
      target: { value: "375" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Manage sounds" }));

    expect(props.onEnabledChange).toHaveBeenCalledWith(false);
    expect(props.onSelectPack).toHaveBeenCalledWith("quiet-linear");
    await waitFor(() => expect(props.onVolumeChange).toHaveBeenCalledWith(0.375));
    expect(props.onManageSounds).toHaveBeenCalledOnce();
  });

  it("shows a safe unavailable state when runtime status is missing", () => {
    renderControls({ status: undefined });

    expect(screen.getByText("Sound engine unavailable" )).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Manage sounds" })).toBeEnabled();
    expect(screen.queryByRole("slider", { name: "Volume" })).not.toBeInTheDocument();
  });

  it("explains an empty installed library", () => {
    renderControls({ status: { ...status, packs: [] }, packs: [] });

    expect(screen.getByText("No installed sounds. Manage sounds to import one.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Manage sounds" })).toBeEnabled();
  });
});
