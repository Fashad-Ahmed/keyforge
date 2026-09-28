import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import type { PackSummary } from "@/lib/types/runtime";

import { PackLibrary } from "./pack-library";

const counts = { normal: 3, space: 1, enter: 1, backspace: 1, modifier: 1 };

function pack(id: string, name: string, bundled = true): PackSummary {
  return { active: false, bundled, groupCounts: counts, id, name };
}

it("filters the installed library by approved sound collection", () => {
  render(
    <PackLibrary
      onImport={vi.fn()}
      onSelect={vi.fn()}
      packs={[
        pack("keyforge-creamy-tactile", "Creamy Tactile"),
        pack("keyforge-marble-thock", "Marble Thock"),
        pack("keyforge-arcade", "Arcade"),
        pack("keyforge-switch-linear", "Linear Switch (Keychron K10)"),
        pack("keyforge-playful-bubble", "Bubble Pop"),
        pack("keyforge-playful-duck", "Rubber Duck"),
        pack("keyforge-playful-boing", "Cartoon Boing"),
        pack("community-rain", "Community Rain", false),
      ]}
      pendingAction={null}
    />,
  );

  fireEvent.click(screen.getByRole("button", { name: "Imported" }));
  expect(screen.getByRole("heading", { name: "Community Rain" })).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Rubber Duck" })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Real switches" }));
  expect(screen.getAllByRole("article")).toHaveLength(1);
  expect(screen.getByRole("heading", { name: "Linear Switch (Keychron K10)" })).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Community Rain" })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Playful" }));
  expect(screen.getByRole("heading", { name: "Rubber Duck" })).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Bubble Pop" })).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Cartoon Boing" })).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Linear Switch (Keychron K10)" })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Earlier synths" }));
  expect(screen.getByRole("heading", { name: "Creamy Tactile" })).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Linear Switch (Keychron K10)" })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "All" }));
  expect(screen.getAllByRole("article")).toHaveLength(8);
});

it("explains when the selected collection has no instruments", () => {
  render(
    <PackLibrary
      onImport={vi.fn()}
      onSelect={vi.fn()}
      packs={[pack("keyforge-mechanical", "Classic Mechanical")]}
      pendingAction={null}
    />,
  );

  fireEvent.click(screen.getByRole("button", { name: "Imported" }));

  expect(screen.getByRole("status")).toHaveTextContent("No imported instruments installed.");
});

it("shows only licensed recorded switch packs in Real switches", () => {
  render(
    <PackLibrary
      onImport={vi.fn()}
      onSelect={vi.fn()}
      packs={[
        { ...pack("keyforge-switch-linear", "Linear Switch (Keychron K10)"), groupCounts: { ...counts, normal: 1 } },
        pack("keyforge-switch-tactile", "Tactile Switch (StavSounds)"),
        pack("keyforge-switch-clicky", "Clicky Switch (StavSounds)"),
        pack("keyforge-mechanical", "Legacy procedural sound"),
        pack("keyforge-playful-duck", "Rubber Duck"),
      ]}
      pendingAction={null}
    />,
  );

  fireEvent.click(screen.getByRole("button", { name: "Real switches" }));
  expect(screen.getAllByRole("article")).toHaveLength(3);
  expect(screen.getByText("1 voice · Included")).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Legacy procedural sound" })).not.toBeInTheDocument();
});
