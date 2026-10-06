import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { act } from "react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { FinishedMoving, VaultsToMove } from "./bindings";
import { MOVED, VaultsWaitingNotice } from "./VaultsWaitingNotice";

/**
 * **The launch's keychain copy asks nothing; the person finishes what it left** (#1306).
 *
 * At launch the app copies keychain items to the new names with macOS's dialogs off, so an
 * update never opens a burst of prompts before a window is up. The vaults whose items would have
 * asked stay where they were, working, and this Notice offers to finish them: only that press
 * lets macOS ask.
 */

const TWO: VaultsToMove = { vaults: 2, items: 5 };

function core(waiting: VaultsToMove | null, finished: FinishedMoving): { asked: string[] } {
  const asked: string[] = [];
  mockIPC(
    (cmd) => {
      asked.push(cmd);
      if (cmd === "vaults_to_move") return waiting;
      if (cmd === "finish_moving_vaults") return finished;
      return null;
    },
    { shouldMockEvents: true },
  );
  return { asked };
}

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("the notice about vaults that wait to move", () => {
  it("is not drawn when nothing waits", async () => {
    const { asked } = core(null, { left: null, failed: 0 });

    render(<VaultsWaitingNotice />);

    await waitFor(() => expect(asked).toContain("vaults_to_move"));
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("says how many vaults wait and that macOS asks once per secret, and copies nothing by itself", async () => {
    const { asked } = core(TWO, { left: null, failed: 0 });

    render(<VaultsWaitingNotice />);

    const notice = await screen.findByRole("status");
    expect(notice.textContent).toContain("2 vaults");
    expect(notice.textContent).toContain("5 secrets");
    expect(notice.textContent).toContain("keep working");
    expect(asked).not.toContain("finish_moving_vaults");
  });

  it("finishes moving them on the press, and goes once every one moved", async () => {
    const { asked } = core(TWO, { left: null, failed: 0 });
    render(<VaultsWaitingNotice />);

    await userEvent.click(await screen.findByRole("button", { name: "Finish moving 2 vaults" }));

    await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
    expect(asked.filter((cmd) => cmd === "finish_moving_vaults")).toHaveLength(1);
  });

  it("stays with a short line, and offers the press again, when one could not be moved", async () => {
    core(TWO, { left: { vaults: 1, items: 2 }, failed: 1 });
    render(<VaultsWaitingNotice />);

    await userEvent.click(await screen.findByRole("button", { name: "Finish moving 2 vaults" }));

    const again = await screen.findByRole("button", { name: "Finish moving 1 vault" });
    const notice = screen.getByRole("status");
    expect(notice.textContent).toContain("One could not be moved");
    expect(notice.textContent).toContain("log");
    expect(again).toBeTruthy();
  });

  it("says a refused finish in one line", async () => {
    mockIPC((cmd) => {
      if (cmd === "vaults_to_move") return TWO;
      if (cmd === "finish_moving_vaults") throw "Nothing was moved; the app's log says why.";
      return null;
    });
    render(<VaultsWaitingNotice />);

    await userEvent.click(await screen.findByRole("button", { name: "Finish moving 2 vaults" }));

    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain("Nothing was moved"),
    );
    expect(screen.getByRole("button", { name: "Finish moving 2 vaults" })).toBeTruthy();
  });

  it("goes when another window's press moved every one", async () => {
    const { asked } = core(TWO, { left: null, failed: 0 });
    render(<VaultsWaitingNotice />);
    await screen.findByRole("status");

    await act(() => emit(MOVED, null));

    await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
    expect(asked).not.toContain("finish_moving_vaults");
  });

  it("says what still waits when another window's press moved only some", async () => {
    core(TWO, { left: null, failed: 0 });
    render(<VaultsWaitingNotice />);
    await screen.findByRole("button", { name: "Finish moving 2 vaults" });

    await act(() => emit(MOVED, { vaults: 1, items: 2 }));

    expect(await screen.findByRole("button", { name: "Finish moving 1 vault" })).toBeTruthy();
  });

  it("drops its trouble line when another window's press completes", async () => {
    core(TWO, { left: { vaults: 2, items: 5 }, failed: 1 });
    render(<VaultsWaitingNotice />);
    await userEvent.click(await screen.findByRole("button", { name: "Finish moving 2 vaults" }));
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain("could not be moved"),
    );

    await act(() => emit(MOVED, { vaults: 1, items: 2 }));

    await screen.findByRole("button", { name: "Finish moving 1 vault" });
    expect(screen.getByRole("status").textContent).not.toContain("could not be moved");
  });

  it("can be dismissed", async () => {
    core(TWO, { left: null, failed: 0 });
    render(<VaultsWaitingNotice />);

    await userEvent.click(await screen.findByRole("button", { name: "Dismiss" }));

    expect(screen.queryByRole("status")).toBeNull();
  });
});
