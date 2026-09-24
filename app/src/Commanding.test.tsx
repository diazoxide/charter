import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { forgetExtensionThemes } from "./Extensions";
import { forgetExtensionsOn } from "./extensionsOn";
import { forgetProjectThemes } from "./projectTheme";
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
    if (cmd === "start_chat") return { session: ++opened };
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
 * The reads the window makes about the plane on its own initiative.
 *
 * Every one of them is an effect that chains off another effect's answer — the launch says
 * which plane, the sidebar says which workspaces, the panels read the focused one — so where
 * they land in a sequence is about how many awaits the chain took, not about what an action
 * did. No action performs any of them.
 */
const WATCHING = new Set([
  "plane_at_launch",
  "plane_sidebar",
  // What the machine store says is pinned here. Asked off the sidebar's answer, so where it
  // lands in a sequence is about how many awaits the chain took (ADR 0039).
  "plane_pins",
  "opened_chats",
  "chats_that_would_not_start",
  "chat_states",
  "workspace_panels",
  "workspace_repos",
  "worktree_of_chat",
]);

/**
 * What the window ASKED THE CORE TO DO, as a sequence two routes can be compared on.
 *
 * Tauri's own event plumbing is filtered out, and so is the cold-start marker: both carry a
 * handler id that is new every render and land whenever a frame happens to, which is noise
 * about the test runner rather than about the action. So are the plane reads above, for the
 * same reason — what is left is the action itself, which is what "one action, two doorways"
 * is a claim about.
 */
const commandsSent = (asked: { cmd: string; args: unknown }[]) =>
  asked.filter(
    ({ cmd }) => !cmd.startsWith("plugin:") && cmd !== "first_frame" && !WATCHING.has(cmd),
  );

/** What the window IS: its tabs, its panes, and which pane has the keyboard. */
const arrangement = () => ({
  tabs: tabNames(),
  panes: screen
    .getAllByTestId("pane")
    .map((pane) => `${pane.textContent} focused=${pane.dataset.focused}`),
});

/**
 * Answers the question charter asks before it ends a chat (`EndingChat.tsx`).
 *
 * **Every route to ending one asks first** — a tab's `×`, a pane's `×` and the palette's
 * rows all go through one place (the operator's *"closing session should ask confirmation"*).
 * A test that presses one of them and does not answer is a test of a dialog still on screen.
 *
 * `name` is the row's own words, which are the dialog's too, so this presses the answer that
 * belongs to the chat under test rather than whichever button happens to be second.
 */
async function answerTheAsk(name: string) {
  const asking = await screen.findByRole("alertdialog");
  await userEvent.click(within(asking).getByRole("button", { name }));
}

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
        "End this pane's chat",
        "End chat steward 1",
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
    expect(tabNames()).toEqual(["steward 1"]);
  });

  it("switches tab", async () => {
    core();
    render(<App />);
    await openAChat();
    await openAChat();
    expect(screen.getAllByTestId("pane").map((pane) => pane.textContent)).toEqual(["session 2"]);

    await runFromPalette("switch to tab steward 1");

    expect(screen.getAllByTestId("pane").map((pane) => pane.textContent)).toEqual(["session 1"]);
  });

  it("switches workspace, which is what the regions follow", async () => {
    core();
    render(<App />);
    await screen.findByTestId("panels");
    // The right-hand region names the workspace whose todos and personas it is drawing
    // (ADR 0038 renamed it: it is what is asking for you, not the workspace).
    expect(await screen.findByLabelText("Attention · alpha")).toBeInTheDocument();

    await runFromPalette("focus workspace beta");

    expect(await screen.findByLabelText("Attention · beta")).toBeInTheDocument();
  });

  it("closes a chat", async () => {
    const { asked } = core();
    render(<App />);
    await openAChat();

    await runFromPalette("end chat steward 1");
    await answerTheAsk("End chat steward 1");

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

    await runFromPalette("end this pane");
    await answerTheAsk("End this pane's chat");

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
    for (const words of [
      "New tab",
      "Split right",
      "Split down",
      "End this pane's chat",
      "End chat steward 1",
    ]) {
      expect(rows).toContain(words);
      expect(screen.getByRole("button", { name: words })).toBeInTheDocument();
    }
  });

  it("lists a row that cannot run, with its reason, where the button for it has gone", async () => {
    // **This used to be about a disabled `Split right` button on the bar**, and the bar has
    // no such button any more: the splits and the pane close are on the panes now, and with
    // no chat in front there are no panes to put them on. That is right, and it is also the
    // moment the guarantee could have been lost — *"an operator cannot ask about an option
    // they cannot see"* (`actions.ts`).
    //
    // It is not lost, and this is where it now lives: the palette lists every unavailable
    // row with the reason it cannot run. The palette is also the keyboard's route to these
    // three, which is the other half of moving them onto a hover surface.
    core();
    render(<App />);
    await screen.findByTestId("empty-window");

    await palette("split right");

    const row = screen.getByRole("option", { name: /Split right/ });
    expect(row).toHaveAttribute("aria-disabled", "true");
    expect(row).toHaveTextContent("No chat is in front, so there is no pane to split.");
    expect(screen.queryByRole("button", { name: "Split right" })).toBeNull();
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
    // Not vacuous: filtering the plane reads out must leave the action itself behind, or
    // the comparison below would pass by comparing two empty lists.
    expect(askedByButton.map(({ cmd }) => cmd)).toContain("start_chat");

    cleanup();
    clearMocks();
    // A second window, as the first was: what the first learned about its project's extensions
    // and themes is module state (`extensionsOn.ts`, `Extensions.tsx`, `projectTheme.ts`) that a
    // new window has not.
    forgetExtensionsOn();
    forgetExtensionThemes();
    forgetProjectThemes();

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
