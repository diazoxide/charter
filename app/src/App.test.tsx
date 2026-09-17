import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("App", () => {
  it("shows the plane the core found", async () => {
    mockIPC((cmd) => (cmd === "plane_root" ? "/home/dev/plane" : undefined));

    render(<App />);

    expect(await screen.findByText("/home/dev/plane")).toBeInTheDocument();
  });

  it("says why there is no plane when the core finds none", async () => {
    mockIPC(() => {
      throw new Error("no charter.toml in /tmp or any directory above it");
    });

    render(<App />);

    expect(await screen.findByRole("alert")).toHaveTextContent("no charter.toml in /tmp");
  });
});
