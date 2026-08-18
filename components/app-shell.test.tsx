import { render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";

import { AppShell } from "./app-shell";

const { getAppInfoMock } = vi.hoisted(() => ({
  getAppInfoMock: vi.fn(),
}));

vi.mock("@/lib/native/api", () => ({
  getAppInfo: getAppInfoMock,
}));

beforeEach(() => {
  getAppInfoMock.mockReset().mockResolvedValue({
    name: "KeyForge",
    version: "0.1.0",
    platform: "macos",
  });
});

it("renders native runtime information", async () => {
  render(<AppShell />);

  expect(
    screen.getByRole("heading", { level: 1, name: "KeyForge" }),
  ).toBeInTheDocument();
  expect(await screen.findByText("Version: 0.1.0")).toBeInTheDocument();
  expect(screen.getByText("Platform: macos")).toBeInTheDocument();
});

it("renders a non-sensitive status when native runtime information fails", async () => {
  getAppInfoMock.mockRejectedValue(new Error("sensitive native detail"));

  render(<AppShell />);

  expect(
    await screen.findByText("Native runtime unavailable"),
  ).toBeInTheDocument();
  expect(screen.queryByText("sensitive native detail")).not.toBeInTheDocument();
});
