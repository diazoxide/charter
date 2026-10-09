import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { FirstRun } from "./FirstRun";

/**
 * **The first run's folder dialog** (#1291): a dialog that could not open is said where the
 * first run says a refused open, and a cancel is not a failure and says nothing.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

function core(pick: () => string | null) {
  mockIPC((cmd) => {
    if (cmd === "pick_project") return pick();
    return null;
  });
}

function firstRun(trouble?: string) {
  const onOpenRepo = vi.fn();
  render(
    <FirstRun
      onOpenRepo={onOpenRepo}
      onOpenProject={vi.fn()}
      onSignInToForge={vi.fn()}
      opening={false}
      trouble={trouble}
    />,
  );
  return { onOpenRepo };
}

describe("the first run's Open a repo…", () => {
  it("says a folder dialog that could not open, and opens nothing", async () => {
    core(() => {
      throw "the folder dialog could not be opened";
    });
    const { onOpenRepo } = firstRun();

    await userEvent.click(screen.getByRole("button", { name: "Open a repo…" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "the folder dialog could not be opened",
    );
    expect(onOpenRepo).not.toHaveBeenCalled();
  });

  it("says nothing when the dialog is cancelled", async () => {
    let asked = 0;
    core(() => {
      asked += 1;
      return null;
    });
    const { onOpenRepo } = firstRun();

    await userEvent.click(screen.getByRole("button", { name: "Open a repo…" }));

    await vi.waitFor(() => expect(asked).toBe(1));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(onOpenRepo).not.toHaveBeenCalled();
  });

  it("opens the folder that was picked", async () => {
    core(() => "/home/dev/widget");
    const { onOpenRepo } = firstRun();

    await userEvent.click(screen.getByRole("button", { name: "Open a repo…" }));

    await vi.waitFor(() =>
      expect(onOpenRepo).toHaveBeenCalledWith("/home/dev/widget", expect.anything()),
    );
  });
});
