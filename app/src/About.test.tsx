import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { AboutCharter } from "./About";

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("About Charter", () => {
  it("ships each vendored asset's attribution and licence text (FM-3)", async () => {
    mockIPC(() => {
      throw new Error("not asked here");
    });
    render(<AboutCharter />);
    await userEvent.click(screen.getByTestId("title-about"));
    const notices = await screen.findByTestId("about-notices");
    expect(
      within(notices).getByText(/material-icon-theme 5\.39\.0, under the MIT licence/),
    ).toBeTruthy();
    expect(notices.querySelector("pre")?.textContent).toMatch(
      /^The MIT License \(MIT\)\nCopyright \(c\) 2025 Material Extensions/,
    );
  });
});
