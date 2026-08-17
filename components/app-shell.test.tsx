import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import { AppShell } from "./app-shell";

vi.mock("@/lib/native/api", () => ({
  getAppInfo: vi.fn().mockResolvedValue({
    name: "KeyForge",
    version: "0.1.0",
    platform: "macos",
  }),
}));

it("renders native runtime information", async () => {
  render(<AppShell />);

  expect(
    screen.getByRole("heading", { level: 1, name: "KeyForge" }),
  ).toBeInTheDocument();
  expect(await screen.findByText("Version: 0.1.0")).toBeInTheDocument();
  expect(screen.getByText("Platform: macos")).toBeInTheDocument();
});
