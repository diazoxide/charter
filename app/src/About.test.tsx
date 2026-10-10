import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { AboutCharter } from "./About";

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("About Charter", () => {
  it("says what it is reading while it reads, in sentence case (#630)", async () => {
    mockIPC(() => new Promise(() => undefined));
    render(<AboutCharter />);
    await userEvent.click(screen.getByTestId("title-about"));
    expect(await screen.findByText("Reading what this version brought…")).toBeTruthy();
  });

  it("says why what this version brought could not be read, and Read again asks again", async () => {
    let asks = 0;
    mockIPC((cmd) => {
      if (cmd !== "about_charter") throw new Error("not asked here");
      asks += 1;
      throw new Error("the changelog could not be read");
    });
    render(<AboutCharter />);
    await userEvent.click(screen.getByTestId("title-about"));
    expect(await screen.findByText(/could not read what this version brought/)).toBeTruthy();

    // NO-8's follow-up (#1296): the dialog reads once, so its refusal offers the retry.
    await userEvent.click(screen.getByRole("button", { name: "Read again" }));
    await waitFor(() => expect(asks).toBe(2));
    expect(await screen.findByText(/could not read what this version brought/)).toBeTruthy();
  });

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
