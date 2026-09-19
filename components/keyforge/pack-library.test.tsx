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
        pack("community-rain", "Community Rain", false),
      ]}
      pendingAction={null}
    />,
  );

  fireEvent.click(screen.getByRole("button", { name: "Enthusiast" }));
  expect(screen.getByRole("heading", { name: "Marble Thock" })).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Creamy Tactile" })).not.toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Arcade" })).not.toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Community Rain" })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Imported" }));
  expect(screen.getByRole("heading", { name: "Community Rain" })).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Marble Thock" })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Realistic" }));
  expect(screen.getByRole("heading", { name: "Creamy Tactile" })).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Community Rain" })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Creative" }));
  expect(screen.getByRole("heading", { name: "Arcade" })).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Creamy Tactile" })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "All" }));
  expect(screen.getAllByRole("article")).toHaveLength(4);
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
