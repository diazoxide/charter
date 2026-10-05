import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { FinishedMoving, VaultsToMove } from "./bindings";
import { VaultsWaitingNotice } from "./VaultsWaitingNotice";

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
  mockIPC((cmd) => {
    asked.push(cmd);
    if (cmd === "vaults_to_move") return waiting;
    if (cmd === "finish_moving_vaults") return finished;
    return null;
  });
  return { asked };
}

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("the notice about vaults that wait to move", () => {
  it("is not drawn when nothing waits", async () => {
    const { asked } = core(null, { left: null, said: [] });

    render(<VaultsWaitingNotice />);

    await waitFor(() => expect(asked).toContain("vaults_to_move"));
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("says how many vaults wait and that macOS asks once per secret, and copies nothing by itself", async () => {
    const { asked } = core(TWO, { left: null, said: [] });

    render(<VaultsWaitingNotice />);

    const notice = await screen.findByRole("status");
    expect(notice.textContent).toContain("2 vaults");
    expect(notice.textContent).toContain("5 secrets");
    expect(notice.textContent).toContain("keep working");
    expect(asked).not.toContain("finish_moving_vaults");
  });

  it("finishes moving them on the press, and goes once every one moved", async () => {
    const { asked } = core(TWO, { left: null, said: ["✓ vault 'ops': moved"] });
    render(<VaultsWaitingNotice />);

    await userEvent.click(await screen.findByRole("button", { name: "Finish moving 2 vaults" }));

    await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
    expect(asked.filter((cmd) => cmd === "finish_moving_vaults")).toHaveLength(1);
  });

  it("stays, says why, and offers the press again when a secret was not allowed", async () => {
    core(TWO, {
      left: { vaults: 1, items: 2 },
      said: [
        "✓ vault 'team': moved",
        "✗ vault 'ops': the user did not allow it; it still reads its secrets under the old name",
      ],
    });
    render(<VaultsWaitingNotice />);

    await userEvent.click(await screen.findByRole("button", { name: "Finish moving 2 vaults" }));

    const again = await screen.findByRole("button", { name: "Finish moving 1 vault" });
    const notice = screen.getByRole("status");
    expect(notice.textContent).toContain("did not allow");
    expect(notice.textContent).not.toContain("✓");
    expect(again).toBeTruthy();
  });

  it("can be dismissed", async () => {
    core(TWO, { left: null, said: [] });
    render(<VaultsWaitingNotice />);

    await userEvent.click(await screen.findByRole("button", { name: "Dismiss" }));

    expect(screen.queryByRole("status")).toBeNull();
  });
});
