import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

/**
 * The palette against the whole window: every action it lists reaching what the window
 * already does, and the bar's buttons reaching the same rows.
 *
 * **The point of this file is that there is one list.** `actions.test.ts` is about what the
 * catalogue says; `Palette.test.tsx` is about the surface. Here the two meet the real `App`,
 * which is the only place a second list could hide — and the test that would catch it is the
 * one that runs an action both ways and compares what the window became.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session, focused }: { session: number; focused: boolean }) => (
    <div data-testid="pane" data-focused={String(focused)}>
      session {session}
    </div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
});

/** Where the chat this plane opens works: a piece, so the worktree rows have a subject. */
const CWD = "/home/dev/plane/workspaces/alpha/svc/charter/fix-it";

const PIECE = {
  workspace: "alpha",
  repo: "svc",
  piece: "fix-it",
  branch: "charter/fix-it",
  wired: false,
  stale: false,
};

const SIDEBAR = {
  root: "/home/dev/plane",
  workspaces: [
    {
      name: "alpha",
      path: "/home/dev/plane/workspaces/alpha",
      vision: "Ship it",
      todos: [],
      chats: [
        {
          session: 1,
          name: "1",
          cwd: CWD,
          harness: "claude",
          in_front: true,
          resumed: null,
          fresh: null,
          profile: "claude",
          persona: "steward",
        },
      ],
    },
    {
      name: "beta",
      path: "/home/dev/plane/workspaces/beta",
      vision: "Later",
      todos: [],
      chats: [],
    },
  ],
  personas: ["steward"],
  persona: "steward",
  unfiled: [],
};

const START_OPTIONS = {
  profiles: [
    {
      name: "claude",
      kind: "claude",
      shown: "claude",
      source: "built-in",
      is_default: true,
      approval: null,
    },
  ],
  refused: [],
  personas: ["steward"],
  persona: "steward",
  ignore_fix: null,
  declares_none: true,
};

/** The core, answering every command the window sends, and recording what it was asked. */
function core(over: (cmd: string, args: unknown) => unknown = () => undefined) {
  const asked: { cmd: string; args: unknown }[] = [];
  let opened = 0;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    const mine = over(cmd, args);
    if (mine !== undefined) return mine;
    if (cmd === "plane_at_launch")
      return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "running_sessions") return [];
    if (cmd === "opened_chats") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "start_chat") return { session: ++opened, wired: null };
    if (cmd === "worktree_of_chat") return PIECE;
    return null;
  });
  return { asked };
}

async function openAChat() {
  await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
  await userEvent.click(await screen.findByRole("button", { name: "Start" }));
}

/** Opens the palette from wherever the keyboard is, types, and presses Enter. */
async function palette(typed: string) {
  await userEvent.keyboard("{F2}");
  await screen.findByRole("dialog", { name: "Command palette" });
  if (typed) await userEvent.keyboard(typed);
}

async function runFromPalette(typed: string) {
  await palette(typed);
  await userEvent.keyboard("{Enter}");
}

const rowTitles = () =>
  screen.getAllByRole("option").map((row) => row.querySelector(".palette-title")?.textContent);

const tabNames = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

/**
 * What the window ASKED the core, as a sequence two routes can be compared on.
 *
 * Tauri's own event plumbing is filtered out, and so is the cold-start marker: both carry a
 * handler id that is new every render and land whenever a frame happens to, which is noise
 * about the test runner rather than about the action.
 */
const commandsSent = (asked: { cmd: string; args: unknown }[]) =>
  asked.filter(({ cmd }) => !cmd.startsWith("plugin:") && cmd !== "first_frame");

/** What the window IS: its tabs, its panes, and which pane has the keyboard. */
const arrangement = () => ({
  tabs: tabNames(),
  panes: screen
    .getAllByTestId("pane")
    .map((pane) => `${pane.textContent} focused=${pane.dataset.focused}`),
});

describe("the palette reaching what the window can do", () => {
  it("opens on a keystroke and lists the window's actions", async () => {
    core();
    render(<App />);
    await openAChat();

    await palette("");

    expect(rowTitles()).toEqual(
      expect.arrayContaining([
        "New tab",
        "Split right",
        "Split down",
        "Close pane",
        "Close tab 1",
        "Focus workspace beta",
        "Merge this chat's worktree into its clone",
        "Remove this chat's worktree",
        "Quit charter",
      ]),
    );
  });

  it("narrows to the one row as you type, and Enter runs it", async () => {
    core();
    render(<App />);
    await openAChat();

    await palette("split right");
    expect(rowTitles()).toEqual(["Split right"]);

    await userEvent.keyboard("{Enter}");
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));

    expect(screen.getAllByTestId("pane").map((pane) => pane.textContent)).toEqual([
      "session 1",
      "session 2",
    ]);
  });

  it("starts a chat, through the picker and no other way", async () => {
    // The palette opens the question ADR 0022 insists on; it never answers it.
    const { asked } = core();
    render(<App />);

    await runFromPalette("new tab");

    expect(await screen.findByRole("dialog", { name: /Start a chat/i })).toBeInTheDocument();
    expect(asked.map(({ cmd }) => cmd)).not.toContain("start_chat");
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));
    expect(tabNames()).toEqual(["1"]);
  });

  it("switches tab", async () => {
    core();
    render(<App />);
    await openAChat();
    await openAChat();
    expect(screen.getAllByTestId("pane").map((pane) => pane.textContent)).toEqual(["session 2"]);

    await runFromPalette("switch to tab 1");

    expect(screen.getAllByTestId("pane").map((pane) => pane.textContent)).toEqual(["session 1"]);
  });

  it("switches workspace, which is what the panels follow", async () => {
    core();
    render(<App />);
    await screen.findByTestId("panels");
    expect(await screen.findByLabelText("Workspace alpha")).toBeInTheDocument();

    await runFromPalette("focus workspace beta");

    expect(await screen.findByLabelText("Workspace beta")).toBeInTheDocument();
  });

  it("closes a chat", async () => {
    const { asked } = core();
    render(<App />);
    await openAChat();

    await runFromPalette("close tab 1");

    expect(screen.queryAllByTestId("pane")).toEqual([]);
    expect(asked.filter(({ cmd }) => cmd === "close_session").map(({ args }) => args)).toEqual([
      { plane: "/home/dev/plane", session: 1 },
    ]);
  });

  it("closes a pane", async () => {
    const { asked } = core();
    render(<App />);
    await openAChat();
    await userEvent.click(screen.getByRole("button", { name: "Split down" }));
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));

    await runFromPalette("close pane");

    expect(asked.filter(({ cmd }) => cmd === "close_session").map(({ args }) => args)).toEqual([
      { plane: "/home/dev/plane", session: 2 },
    ]);
  });

  it("shows the chat that needs you, and says so when none does", async () => {
    core();
    render(<App />);
    await openAChat();

    await palette("needs you");
    const row = screen.getByRole("option", { name: /Show the chat that needs you/ });

    // Listed, and refused with its reason — not dropped, which would leave the operator
    // wondering whether the palette had the row at all.
    expect(row).toHaveAttribute("aria-disabled", "true");
    expect(within(row).getByText("Nothing needs you.")).toBeInTheDocument();
  });

  it("merges a chat's worktree, and says what landed", async () => {
    const { asked } = core((cmd) =>
      cmd === "worktree_merge"
        ? { branch: "charter/fix-it", was: "abc1234", now: "def5678" }
        : undefined,
    );
    render(<App />);
    await openAChat();

    await runFromPalette("merge this chat");

    expect(await screen.findByRole("status")).toHaveTextContent(
      "charter/fix-it landed: abc1234 → def5678",
    );
    expect(asked.find(({ cmd }) => cmd === "worktree_merge")?.args).toMatchObject({
      plane: "/home/dev/plane",
      workspace: "alpha",
      repo: "svc",
      piece: "fix-it",
    });
  });

  it("shows a removal's refusal in the core's own words, and never forces on its own", async () => {
    const said = "svc/fix-it has uncommitted changes; commit or stash them, or pass --force";
    const { asked } = core((cmd) => {
      if (cmd !== "worktree_remove") return undefined;
      throw new Error(said);
    });
    render(<App />);
    await openAChat();

    await runFromPalette("remove this chat");

    // Verbatim, beside the rows rather than behind them, and the palette is still up.
    const alert = await within(
      await screen.findByRole("dialog", { name: "Command palette" }),
    ).findByRole("alert");
    expect(alert).toHaveTextContent(said);
    expect(
      asked.filter(({ cmd }) => cmd === "worktree_remove").map(({ args }) => args),
    ).toMatchObject([{ force: false }]);
  });

  it("offers to discard only once that refusal has been read", async () => {
    let refuse = true;
    const { asked } = core((cmd) => {
      if (cmd !== "worktree_remove") return undefined;
      if (refuse) throw new Error("svc/fix-it has uncommitted changes");
      return null;
    });
    render(<App />);
    await openAChat();

    await palette("discard");
    expect(screen.queryAllByRole("option")).toHaveLength(0);
    await userEvent.keyboard("{Escape}");

    await runFromPalette("remove this chat");
    await within(await screen.findByRole("dialog", { name: "Command palette" })).findByRole(
      "alert",
    );
    refuse = false;
    // The palette stayed open, and the answer to the sentence is now a row in it.
    await userEvent.clear(screen.getByRole("combobox"));
    await userEvent.keyboard("discard{Enter}");

    await vi.waitFor(() =>
      expect(
        asked.filter(({ cmd }) => cmd === "worktree_remove").map(({ args }) => args),
      ).toMatchObject([{ force: false }, { force: true }]),
    );
  });
});

describe("one list, two surfaces", () => {
  it("draws the bar's buttons out of the catalogue, by the words the palette shows", async () => {
    // Every button here is a row. Remove a row from `catalogue` and its button goes with it —
    // which is the whole reason the bar cannot drift from the palette.
    core();
    render(<App />);
    await openAChat();

    await palette("");
    const rows = rowTitles();

    await userEvent.keyboard("{Escape}");
    for (const words of ["New tab", "Split right", "Split down", "Close pane", "Close tab 1"]) {
      expect(rows).toContain(words);
      expect(screen.getByRole("button", { name: words })).toBeInTheDocument();
    }
  });

  it("disables a button whose row cannot run, for the row's own reason", async () => {
    core();
    render(<App />);
    await screen.findByText(/No sessions/);

    expect(screen.getByRole("button", { name: "Split right" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Split right" })).toHaveAttribute(
      "title",
      "No chat is in front, so there is no pane to split.",
    );
  });

  it("ends in the same state whether a split came from the palette or from its button", async () => {
    // The test that would catch a second implementation: same action, two doorways, one
    // window afterwards — and the same question asked of the core.
    const fromTheButton = core();
    render(<App />);
    await openAChat();
    await userEvent.click(screen.getByRole("button", { name: "Split right" }));
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));
    const byButton = arrangement();
    const askedByButton = commandsSent(fromTheButton.asked);

    cleanup();
    clearMocks();

    const fromThePalette = core();
    render(<App />);
    await openAChat();
    await runFromPalette("split right");
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));
    const byPalette = arrangement();

    expect(byPalette).toEqual(byButton);
    expect(byPalette.panes).toEqual(["session 1 focused=false", "session 2 focused=true"]);
    // And the core was asked the same things, in the same order: the palette is a second
    // doorway to one action, not a second action that happens to look alike.
    expect(commandsSent(fromThePalette.asked)).toEqual(askedByButton);
  });
});

/**
 * The key the palette claimed, reaching the chat (charter-app#47).
 *
 * This is the only place the whole path is one thing: the keystroke, the catalogue's row,
 * the window's dispatcher, the session it picks and the bytes it writes. `actions.test.ts`
 * knows the row and `Palette.test.tsx` knows the chord; neither can tell whether what
 * arrives at the core is `F2`.
 */
describe("handing F2 to the chat in front", () => {
  /** What a terminal sends for an unmodified F2: SS3 Q. Spelled out here rather than
   *  imported, so the test fails if the constant changes rather than changing with it. */
  const F2_BYTES = "OQ";

  it("writes what the pane's own terminal would have written, to the session in front", async () => {
    const { asked } = core();
    render(<App />);
    await openAChat();

    await palette("");
    await userEvent.keyboard("{F2}");

    expect(commandsSent(asked)).toEqual(
      expect.arrayContaining([
        { cmd: "send_input", args: { plane: "/home/dev/plane", session: 1, text: F2_BYTES } },
      ]),
    );
    expect(screen.queryByRole("dialog", { name: "Command palette" })).not.toBeInTheDocument();
  });

  it("is the same whether the chord or the row ran it", async () => {
    // One mechanism with two doorways, which is the rule the bar's buttons follow too.
    const byChord = core();
    render(<App />);
    await openAChat();
    await palette("");
    await userEvent.keyboard("{F2}");
    const asChord = commandsSent(byChord.asked).filter(({ cmd }) => cmd === "send_input");

    cleanup();
    clearMocks();

    const byRow = core();
    render(<App />);
    await openAChat();
    await runFromPalette("Send F2");

    expect(commandsSent(byRow.asked).filter(({ cmd }) => cmd === "send_input")).toEqual(asChord);
  });

  it("sends nothing and says why when no chat is in front", async () => {
    const { asked } = core();
    render(<App />);

    await palette("");
    await userEvent.keyboard("{F2}");

    expect(commandsSent(asked).some(({ cmd }) => cmd === "send_input")).toBe(false);
    // Said where the operator is looking, and not only on the row further down the list.
    expect(document.querySelector(".held")?.textContent).toBe(
      "No chat is in front, so there is nowhere to send it.",
    );
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
  });
});
