import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { RelaunchQuestion } from "./bindings";
import { RelaunchAsk } from "./RelaunchAsk";

/**
 * The question a relaunch asks before it puts anything back (charter-app#250): reopen every
 * session, or start fresh.
 *
 * Two halves. The dialog on its own — what it says and what each way out of it answers — and
 * the window's launch path, where the property that matters is ORDER: nothing that starts a
 * chat is sent to the core before the operator has answered.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
});

const ONE = "/home/dev/charter";
const TWO = "/home/dev/ide";

const TWO_PROJECTS: RelaunchQuestion = {
  projects: [
    { plane: ONE, chats: 2, views: 1 },
    { plane: TWO, chats: 1, views: 0 },
  ],
  after_update: false,
};

const lastPart = (plane: string) => plane.split("/").pop() ?? plane;

function theDialog(question: RelaunchQuestion = TWO_PROJECTS) {
  const answered = vi.fn();
  render(<RelaunchAsk question={question} nameOf={lastPart} onAnswer={answered} />);
  return answered;
}

describe("the question a relaunch asks", () => {
  it("names how many chats were open, and in which projects", () => {
    theDialog();

    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent("3 chats and 1 view tab were open when charter last quit");
    expect(dialog).toHaveTextContent("charter2 chats, 1 view tab");
    expect(dialog).toHaveTextContent("ide1 chat");
  });

  it("says one chat in the singular", () => {
    theDialog({ projects: [{ plane: ONE, chats: 1, views: 0 }], after_update: false });

    expect(screen.getByRole("alertdialog")).toHaveTextContent(
      "1 chat was open when charter last quit",
    );
  });

  it("answers reopen-all from its first button, which has the keyboard", async () => {
    const answered = theDialog();
    const reopen = screen.getByRole("button", { name: "Reopen all sessions" });

    await waitFor(() => expect(reopen).toHaveFocus());
    await userEvent.click(reopen);

    expect(answered).toHaveBeenCalledWith("ReopenAll");
    expect(answered).toHaveBeenCalledTimes(1);
  });

  it("answers start-fresh only from its own button", async () => {
    const answered = theDialog();

    await userEvent.click(screen.getByRole("button", { name: "Start fresh" }));

    expect(answered).toHaveBeenCalledWith("StartFresh");
    expect(answered).toHaveBeenCalledTimes(1);
  });

  it("takes Escape as reopen-all, so a lost answer never discards work", async () => {
    const answered = theDialog();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Reopen all sessions" })).toHaveFocus(),
    );

    await userEvent.keyboard("{Escape}");

    expect(answered).toHaveBeenCalledWith("ReopenAll");
    expect(answered).not.toHaveBeenCalledWith("StartFresh");
  });

  it("puts both answers in the tab sequence, reopen first", () => {
    theDialog();

    const buttons = screen.getAllByRole("button");
    expect(buttons.map((button) => button.textContent)).toEqual([
      "Reopen all sessions",
      "Start fresh",
    ]);
    for (const button of buttons) expect(button).toHaveAttribute("tabindex", "0");
  });

  it("says why it is asking when charter restarted to install an update", () => {
    theDialog({ ...TWO_PROJECTS, after_update: true });

    expect(screen.getByRole("alertdialog")).toHaveTextContent(
      "charter restarted to install an update",
    );
  });
});

/** A core whose launch opened `ONE`, whose restore holds `TWO`, and which asks `question`. */
function core(question: RelaunchQuestion | null) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plugin:event|listen") return 1;
    if (cmd === "plane_at_launch") return { plane: ONE, from: ONE, why: null };
    if (cmd === "relaunch_ask") return question;
    if (cmd === "planes_to_restore") return { planes: [TWO], active: null, dropped: [] };
    if (cmd === "open_plane") return { plane: given.path, ask: null };
    if (cmd === "plane_sidebar")
      return { root: given.plane, workspaces: [], personas: [], persona: null, unfiled: [] };
    if (cmd === "opened_chats") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "recent_planes") return { planes: [], dropped: [], forgetful: null };
    return null;
  });
  return {
    sent: (cmd: string) => asked.filter((call) => call.cmd === cmd),
    order: () => asked.map((call) => call.cmd),
  };
}

describe("a launch with something to put back", () => {
  it("starts nothing, and restores no project, while the question is up", async () => {
    const calls = core(TWO_PROJECTS);
    render(<App />);

    await screen.findByRole("alertdialog");

    expect(calls.sent("relaunch")).toEqual([]);
    expect(calls.sent("open_plane")).toEqual([]);
    expect(calls.sent("planes_to_restore")).toEqual([]);
  });

  it("puts everything back after reopen-all, and only then restores the other projects", async () => {
    const calls = core(TWO_PROJECTS);
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "Reopen all sessions" }));

    await waitFor(() => expect(calls.sent("open_plane")).toHaveLength(1));
    expect(calls.sent("relaunch")).toEqual([{ cmd: "relaunch", args: { choice: "ReopenAll" } }]);
    const order = calls.order();
    expect(order.indexOf("relaunch")).toBeLessThan(order.indexOf("open_plane"));
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("tells the core to start fresh, and still restores the projects themselves", async () => {
    const calls = core(TWO_PROJECTS);
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "Start fresh" }));

    await waitFor(() => expect(calls.sent("open_plane")).toHaveLength(1));
    expect(calls.sent("relaunch")).toEqual([{ cmd: "relaunch", args: { choice: "StartFresh" } }]);
    const order = calls.order();
    expect(order.indexOf("relaunch")).toBeLessThan(order.indexOf("open_plane"));
  });

  it("takes an Escape as reopen-all", async () => {
    const calls = core(TWO_PROJECTS);
    render(<App />);
    await screen.findByRole("alertdialog");

    await userEvent.keyboard("{Escape}");

    await waitFor(() => expect(calls.sent("relaunch")).toHaveLength(1));
    expect(calls.sent("relaunch")[0].args).toEqual({ choice: "ReopenAll" });
  });
});

describe("a launch with nothing to put back", () => {
  it("asks nothing, and still lets the core put the launch's project back", async () => {
    const calls = core(null);
    render(<App />);

    await waitFor(() => expect(calls.sent("open_plane")).toHaveLength(1));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(calls.sent("relaunch")).toEqual([{ cmd: "relaunch", args: { choice: "ReopenAll" } }]);
  });
});
