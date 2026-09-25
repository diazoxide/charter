import { readFileSync } from "node:fs";
import { join } from "node:path";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { forgetThisLaunch } from "./regions";
import { sequenceIn } from "./tabSequence";

/**
 * **Where Tab goes in the window outside its dialogs** (charter-app#189).
 *
 * #186 made every modal reachable; this is the rest of the window, where the question is not
 * one attribute but an order. The WAI-ARIA answer, which is what this file holds the window to:
 *
 * - **each strip and each list is ONE Tab stop** — a roving `tabindex`: the selected tab says
 *   `0`, every other says `-1`, and the arrows move inside it (Authoring Practices, "Tabs" and
 *   "Listbox"/"Tree View" keyboard interaction);
 * - **every other control says `tabIndex={0}`**, because WebKit leaves a `<button>` whose
 *   `tabindex` is not written down out of the sequence (`docs/ui-primitives.md`);
 * - **the order is the order the window is drawn in**, top to bottom and left to right.
 *
 * jsdom is not WebKit, so what is asserted is what the engine is handed — the attributes and
 * the document order — through `inWebKitsTabSequence` (`tabSequence.ts`), the engine's own rule written
 * down. Whether WebKit then tabs along that sequence is the one step only a person pressing Tab
 * in the real window can confirm, and a scenario cannot: WebDriver performs no default action.
 */

vi.mock("./SessionPane", async (real) => {
  // The real module's keyboard rule, around a stand-in for xterm: jsdom cannot draw a terminal,
  // and what is under test is what happens to a key delivered inside one.
  const actual = await real<typeof import("./SessionPane")>();
  return {
    ...actual,
    SessionPane: ({ session, focused }: { session: number; focused: boolean }) => (
      <div
        className={focused ? "pane focused" : "pane"}
        data-testid="pane"
        data-chat-keyboard=""
        onKeyDownCapture={actual.leavesTheChat}
      >
        <textarea aria-label={`Terminal ${session}`} tabIndex={0} />
      </div>
    ),
  };
});

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;
const BETA = `${PLANE}/workspaces/beta`;
const CUT = `${ALPHA}/.worktrees/svc`;

function chat(session: number, name: string, inFront = false) {
  return {
    session,
    name,
    cwd: ALPHA,
    harness: "claude",
    in_front: inFront,
    resumed: null,
    fresh: null,
    profile: "claude",
    persona: "steward",
    unreported: null,
    pinned: false,
  };
}

/** A plane with two workspaces, three chats in the first and one worktree cut there. */
function core({
  waiting = [] as number[],
  chats = [chat(1, "one"), chat(2, "two", true), chat(3, "three")],
} = {}) {
  /** Every command the window sent, by name. */
  const asked: string[] = [];
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push(cmd);
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return chats;
    // The operator has pinned every workspace, so every one is on the strip and can be
    // clicked there: the strip draws what is pinned and the one you are in (ADR 0054).
    if (cmd === "plane_pins") return { project: false, workspaces: ["alpha", "beta"], missing: [] };
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: ["steward"],
        persona: "steward",
        unfiled: [],
        workspaces: [
          { name: "alpha", path: ALPHA, vision: "", todos: [], chats },
          { name: "beta", path: BETA, vision: "", todos: [], chats: [] },
        ],
      };
    if (cmd === "workspace_panels")
      return {
        workspace: a.workspace,
        repos: a.workspace === "alpha" ? ["svc"] : [],
        paths: {},
        absent: [],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["steward"],
        persona: "steward",
      };
    if (cmd === "workspace_repos")
      return { workspace: a.workspace, repos: [], cache_refused: null };
    if (cmd === "worktree_list")
      return a.workspace === "alpha"
        ? [{ piece: "one", path: `${CUT}/one`, branch: "one", wired: true, stale: false }]
        : [];
    if (cmd === "chat_states")
      return waiting.map((session) => ({ session, state: "waiting", queue: waiting, sequence: 1 }));
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "alerts_everywhere") return [{ plane: PLANE, alerts: [], stopped: null }];
    return null;
  });
  return { asked };
}

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
});

/** The tabs of one strip, in the order it draws them. */
const tabsOf = (strip: string) =>
  within(screen.getByRole("tablist", { name: strip })).getAllByRole("tab");

describe("a strip is one Tab stop", () => {
  it("the chat strip: the selected tab is the stop and every other tab says -1", async () => {
    core();
    render(<App />);
    await waitFor(() => expect(tabsOf("Tabs")).toHaveLength(3));

    const tabs = tabsOf("Tabs");
    expect(tabs.map((tab) => tab.getAttribute("tabindex"))).toEqual(["-1", "0", "-1"]);
    expect(tabs[1]).toHaveAttribute("aria-selected", "true");
  });

  it("the chat strip: the arrows, Home and End move along it, and Enter selects", async () => {
    core();
    render(<App />);
    await waitFor(() => expect(tabsOf("Tabs")).toHaveLength(3));
    const [one, two, three] = tabsOf("Tabs");

    two.focus();
    await userEvent.keyboard("{ArrowRight}");
    await waitFor(() => expect(three).toHaveFocus());
    // The stop follows the keyboard while it is inside, so Tab leaves from here.
    expect(three).toHaveAttribute("tabindex", "0");
    expect(two).toHaveAttribute("tabindex", "-1");
    // Moving is not selecting: the WAI-ARIA pattern with manual activation.
    expect(two).toHaveAttribute("aria-selected", "true");

    await userEvent.keyboard("{Home}");
    await waitFor(() => expect(one).toHaveFocus());
    await userEvent.keyboard("{End}");
    await waitFor(() => expect(three).toHaveFocus());
    await userEvent.keyboard("{ArrowLeft}");
    await waitFor(() => expect(two).toHaveFocus());
    await userEvent.keyboard("{ArrowLeft}");
    await waitFor(() => expect(one).toHaveFocus());

    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(tabsOf("Tabs")[0]).toHaveAttribute("aria-selected", "true"));
  });

  it("the chat strip: leaving it puts the stop back on the selected tab", async () => {
    core();
    render(<App />);
    await waitFor(() => expect(tabsOf("Tabs")).toHaveLength(3));
    const [one, two] = tabsOf("Tabs");

    two.focus();
    await userEvent.keyboard("{ArrowLeft}");
    await waitFor(() => expect(one).toHaveFocus());
    screen.getByRole("button", { name: "New tab" }).focus();
    // Whatever the keyboard did inside, coming back in lands on the selected tab.
    await waitFor(() => expect(two).toHaveAttribute("tabindex", "0"));
    expect(one).toHaveAttribute("tabindex", "-1");
  });

  it("the workspace strip: the focused workspace is the stop, and the arrows move", async () => {
    core();
    render(<App />);
    await waitFor(() => expect(tabsOf("Workspaces")).toHaveLength(2));
    const [alpha, beta] = tabsOf("Workspaces");
    expect(alpha).toHaveAttribute("aria-selected", "true");
    expect([alpha, beta].map((tab) => tab.getAttribute("tabindex"))).toEqual(["0", "-1"]);

    alpha.focus();
    await userEvent.keyboard("{ArrowRight}");
    await waitFor(() => expect(beta).toHaveFocus());
    await userEvent.keyboard(" ");
    await waitFor(() => expect(tabsOf("Workspaces")[1]).toHaveAttribute("aria-selected", "true"));
  });

  it("the project strip: the project in front is the stop", async () => {
    core();
    render(<App />);
    await waitFor(() => expect(tabsOf("Projects")).toHaveLength(1));
    expect(tabsOf("Projects")[0]).toHaveAttribute("tabindex", "0");
  });
});

describe("a terminal keeps its Tab", () => {
  it("Tab and Shift+Tab inside a chat's terminal are the shell's, and the window takes neither", async () => {
    core();
    render(<App />);
    const terminal = await screen.findByLabelText("Terminal 2");
    terminal.focus();

    for (const shiftKey of [false, true]) {
      const tab = new KeyboardEvent("keydown", {
        key: "Tab",
        shiftKey,
        bubbles: true,
        cancelable: true,
      });
      terminal.dispatchEvent(tab);
      // Unprevented and still here: what reaches xterm is the key, and xterm sends it on.
      expect(tab.defaultPrevented).toBe(false);
      expect(terminal).toHaveFocus();
    }
  });

  it("Ctrl+Tab leaves the terminal forwards and Ctrl+Shift+Tab backwards", async () => {
    core({ waiting: [3] });
    render(<App />);
    const terminal = await screen.findByLabelText("Terminal 2");
    terminal.focus();

    await userEvent.keyboard("{Control>}{Tab}{/Control}");
    // The next stop the window draws after the panes: the handle that resizes the centre
    // against the Attention region, which `react-resizable-panels` makes a keyboard control.
    await waitFor(() => expect(document.activeElement).toHaveAttribute("role", "separator"));

    terminal.focus();
    await userEvent.keyboard("{Control>}{Shift>}{Tab}{/Shift}{/Control}");
    // And the one before it: the focused pane's own controls, drawn in its top corner.
    await waitFor(() => expect(document.activeElement).toHaveAccessibleName(/End this pane|Close/));
  });
});

/** A window with everything drawn that can be: three chats, one asking, a worktree cut. */
async function theWholeWindow() {
  core({ waiting: [3, 1] });
  render(<App />);
  await screen.findByTestId("piece-svc-one");
  await screen.findByRole("button", { name: "2 chats need you" });
  await waitFor(() => expect(tabsOf("Tabs")).toHaveLength(3));
}

/** How a control reads to someone looking for it: its role, and the words it goes by. */
function said(el: HTMLElement): string {
  const role = el.getAttribute("role") ?? el.tagName.toLowerCase();
  const named = el.getAttribute("aria-label") ?? el.textContent ?? "";
  return `${role} ${named.trim()}`.trim();
}

describe("the window's tab order", () => {
  it("goes top to bottom, left to right, one stop per strip and per list", async () => {
    await theWholeWindow();
    const stops = sequenceIn(document.body).map(said);
    expect(stops).toEqual([
      // The title bar: the needs-you button first (charter-app#249), then the app's own two.
      "button 2 chats need you",
      "button About Charter — what this version brought",
      "button Updates — … channel, nothing new known",
      // The project strip: ONE stop for its tabs, then its own controls.
      "tab plane2",
      "button Open a project…",
      "button New project…",
      // The workspace strip.
      "tab alpha32",
      "button New workspace…",
      // The chat strip: the selected chat's tab, and the `+`. A tab's `×` is not a stop.
      "tab steward two",
      "button New tab",
      // The explorer, on the left by default: ONE stop, its current row.
      "treeitem alphathe workspace itself",
      // The handle between it and the centre — `react-resizable-panels`' keyboard resize.
      "separator",
      // The focused pane's own controls, drawn in its top corner, then its terminal. From
      // here Tab is the shell's, and Ctrl+Tab is the way on.
      "button Split right",
      "button Split down",
      "button End this pane's chat",
      "textarea Terminal 2",
      "separator",
      // Attention, on the right, has no stop of its own here: its panels are empty, and the
      // needs-you queue went to the title bar (charter-app#249).
      // The handle above the bottom region, which has no controls of its own.
      "separator",
      // The status line: the region toggles at its left, then Alerts and the doctor.
      "button Explorer",
      "button Attention",
      "button State",
      "button Alerts: none",
      "button Doctor",
    ]);
  });

  it("leaves no button to full keyboard access: each says whether it is a stop", async () => {
    await theWholeWindow();
    const unsaid = [...document.querySelectorAll<HTMLElement>("button, summary")]
      .filter((el) => !el.hasAttribute("tabindex"))
      .map(said);
    expect(unsaid).toEqual([]);
  });
});

/** The rows of a list-shaped region, in the order it draws them. */
const rowsIn = (region: HTMLElement) => [
  ...region.querySelectorAll<HTMLElement>("button, summary"),
];

describe("a list is one Tab stop", () => {
  it("the explorer: the current row is the stop, and Up, Down, Home and End move", async () => {
    await theWholeWindow();
    const explorer = screen.getByRole("navigation", { name: "Explorer" });
    const rows = rowsIn(explorer);
    // The workspace row, three chats working in it, the clone, its one worktree.
    expect(rows.map(said)).toEqual([
      expect.stringMatching(/^treeitem alpha/),
      expect.stringMatching(/^treeitem steward one/),
      expect.stringMatching(/^treeitem steward two/),
      expect.stringMatching(/^treeitem steward three/),
      "treeitem svc1",
      "treeitem one",
    ]);
    expect(rows.map((row) => row.getAttribute("tabindex"))).toEqual([
      "0",
      "-1",
      "-1",
      "-1",
      "-1",
      "-1",
    ]);

    rows[0].focus();
    await userEvent.keyboard("{ArrowDown}");
    await waitFor(() => expect(rows[1]).toHaveFocus());
    await userEvent.keyboard("{End}");
    await waitFor(() => expect(rows[5]).toHaveFocus());
    await userEvent.keyboard("{ArrowUp}");
    await waitFor(() => expect(rows[4]).toHaveFocus());
    await userEvent.keyboard("{Home}");
    await waitFor(() => expect(rows[0]).toHaveFocus());
    // And it is a tree (#238): Right goes into the workspace row, Left climbs back out. The
    // rest of the tree's keys are `Explorer.test.tsx`'s.
    await userEvent.keyboard("{ArrowRight}");
    await waitFor(() => expect(rows[1]).toHaveFocus());
    await userEvent.keyboard("{ArrowLeft}");
    await waitFor(() => expect(rows[0]).toHaveFocus());
  });

  it("the explorer: a picked worktree is where the keyboard comes back in", async () => {
    await theWholeWindow();
    const piece = within(screen.getByTestId("piece-svc-one")).getByRole("treeitem", {
      name: "one",
    });
    await userEvent.click(piece);
    await waitFor(() => expect(piece).toHaveAttribute("tabindex", "0"));
  });

  it("the needs-you list: its button is the stop, and the arrows move in the list it opens", async () => {
    await theWholeWindow();
    const hand = within(screen.getByTestId("title-bar")).getByRole("button", {
      name: "2 chats need you",
    });
    expect(hand).toHaveAttribute("tabindex", "0");

    hand.focus();
    await userEvent.keyboard("{Enter}");
    const items = await screen.findAllByRole("menuitem");
    // The oldest chat asking first, and the chats alone: their Ignore is no item (#248).
    expect(items.map((item) => item.querySelector(".needs-you-name")?.textContent)).toEqual([
      "steward three",
      "steward one",
    ]);
    await waitFor(() => expect(items[0]).toHaveFocus());
    await userEvent.keyboard("{ArrowDown}");
    await waitFor(() => expect(items[1]).toHaveFocus());

    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(hand).toHaveFocus());
  });
});

/**
 * **A pane's controls show when the keyboard is on them, not whenever their pane is focused.**
 *
 * The focused pane is the one the operator types in, and he asked for that corner to stay
 * clear (`App.css`, `.pane-doing`). jsdom computes no stylesheet, so the rules are read as
 * text, the way `ChatGauge.test.tsx` reads its own; `keyboard-reach.e2e.ts` computes them in
 * the real WebView.
 */
describe("a pane's controls", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
    /\/\*[\s\S]*?\*\//g,
    "",
  );
  /** Every rule as its selectors and its declarations. */
  const rules = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)].map(([, selectors, body]) => ({
    selectors: selectors.split(",").map((one) => one.trim()),
    body,
  }));
  /** The selectors that DRAW the controls: visible, and not faded out. */
  const drawing = rules
    .filter(
      (rule) => /visibility:\s*visible/.test(rule.body) && !/opacity:\s*0\s*;/.test(rule.body),
    )
    .flatMap((rule) => rule.selectors)
    .filter((selector) => selector.endsWith(".pane-doing") || selector.includes(".pane-doing:"));

  it("are drawn by hover and by the keyboard being on them, and by nothing else", () => {
    expect(drawing.sort()).toEqual([".pane-doing:focus-within", ".pane-frame:hover .pane-doing"]);
  });

  it("are not drawn because their pane is focused, or because its terminal has the keyboard", () => {
    for (const selector of drawing) {
      expect(selector).not.toContain(".focused");
      expect(selector).not.toContain(".pane-frame:focus-within");
    }
  });

  it("stay in the tab sequence on the focused pane, unseen and unclickable until reached", () => {
    const focused = rules.find((rule) =>
      rule.selectors.some((one) => one.includes(":has(.pane.focused)")),
    );
    expect(focused?.selectors).toEqual([
      ".pane-frame:has(.pane.focused):not(:hover) .pane-doing:not(:focus-within)",
    ]);
    // `visibility: visible` is what keeps them focusable; `opacity` is what hides them.
    expect(focused?.body).toMatch(/visibility:\s*visible/);
    expect(focused?.body).toMatch(/opacity:\s*0\s*;/);
    expect(focused?.body).toMatch(/pointer-events:\s*none/);
  });
});

/**
 * **Delete closes the focused tab** (charter-app#239) — the optional key of the WAI-ARIA "Tabs"
 * pattern, and on a Mac the only keyboard way to close a tab short of the palette: a tab's `×`
 * is not a stop, and a Mac keyboard has no context-menu key to open the tab's menu with.
 *
 * It runs the row the `×` runs, so it asks what the `×` asks: ending a chat asks first, and a
 * view tab closes without asking (`ViewTabs.test.tsx`).
 */
describe("Delete on a focused tab", () => {
  /** Pretends the window runs on a Mac, or not, for the length of one test. */
  const onA = (platform: string) =>
    vi.spyOn(navigator, "platform", "get").mockReturnValue(platform);
  afterEach(() => vi.restoreAllMocks());

  it("asks before it ends the chat on the tab the keyboard is on, and Cancel keeps it", async () => {
    core();
    render(<App />);
    await waitFor(() => expect(tabsOf("Tabs")).toHaveLength(3));
    const [, two] = tabsOf("Tabs");
    two.focus();
    await userEvent.keyboard("{ArrowLeft}");

    await userEvent.keyboard("{Delete}");

    // The tab the keyboard is on, not the one in front.
    const asking = await screen.findByRole("alertdialog");
    expect(asking).toHaveTextContent("End chat steward one");
    await userEvent.click(within(asking).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(tabsOf("Tabs")).toHaveLength(3);
    // And the keyboard is back on the tab it pressed Delete on.
    await waitFor(() => expect(tabsOf("Tabs")[0]).toHaveFocus());
  });

  it("ends it once the question is answered", async () => {
    const { asked } = core();
    render(<App />);
    await waitFor(() => expect(tabsOf("Tabs")).toHaveLength(3));
    tabsOf("Tabs")[1].focus();

    await userEvent.keyboard("{Delete}");
    const asking = await screen.findByRole("alertdialog");
    await userEvent.click(within(asking).getByRole("button", { name: /End/ }));

    await waitFor(() => expect(tabsOf("Tabs")).toHaveLength(2));
    expect(asked).toContain("close_session");
    // The keyboard is not dropped on the page with the tab it was on: it is on the strip's stop,
    // so the next Delete or arrow works.
    await waitFor(() => expect(tabsOf("Tabs").find((tab) => tab.tabIndex === 0)).toHaveFocus());
  });

  it("closes a project from its tab, through the row its × runs", async () => {
    // A project with nothing open closes without a question; one with chats asks
    // ("closing a project" below).
    const { asked } = core({ chats: [] });
    render(<App />);
    await waitFor(() => expect(tabsOf("Projects")).toHaveLength(1));
    await screen.findByRole("button", { name: "New tab" });
    tabsOf("Projects")[0].focus();

    await userEvent.keyboard("{Delete}");

    await waitFor(() => expect(asked).toContain("close_plane"));
  });

  it("closes nothing on a workspace tab, which has no × and whose menu deletes it on disk", async () => {
    const { asked } = core();
    render(<App />);
    await waitFor(() => expect(tabsOf("Workspaces")).toHaveLength(2));
    const [alpha] = tabsOf("Workspaces");
    alpha.focus();

    const del = new KeyboardEvent("keydown", { key: "Delete", bubbles: true, cancelable: true });
    alpha.dispatchEvent(del);

    expect(del.defaultPrevented).toBe(false);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(tabsOf("Workspaces")).toHaveLength(2);
    expect(asked.filter((cmd) => cmd.startsWith("delete"))).toEqual([]);
  });

  it("is also Backspace on a Mac, whose key marked delete sends it", async () => {
    onA("MacIntel");
    core();
    render(<App />);
    await waitFor(() => expect(tabsOf("Tabs")).toHaveLength(3));
    tabsOf("Tabs")[1].focus();

    await userEvent.keyboard("{Backspace}");

    expect(await screen.findByRole("alertdialog")).toHaveTextContent("End chat steward two");
  });

  it("is not Backspace anywhere else, where Backspace on a tab means nothing", async () => {
    onA("Linux x86_64");
    core();
    render(<App />);
    await waitFor(() => expect(tabsOf("Tabs")).toHaveLength(3));
    tabsOf("Tabs")[1].focus();

    await userEvent.keyboard("{Backspace}");

    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("is left alone with a modifier held", async () => {
    core();
    render(<App />);
    await waitFor(() => expect(tabsOf("Tabs")).toHaveLength(3));
    tabsOf("Tabs")[1].focus();

    await userEvent.keyboard("{Shift>}{Delete}{/Shift}");

    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("never fires in a text field: Delete and Backspace in the palette edit its words", async () => {
    onA("MacIntel");
    core();
    render(<App />);
    await waitFor(() => expect(tabsOf("Tabs")).toHaveLength(3));
    tabsOf("Tabs")[1].focus();

    // ⌘K, because F2 on a chat tab renames it (charter-app#254).
    await userEvent.keyboard("{Meta>}k{/Meta}");
    const palette = await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("end{Backspace}{ArrowLeft}{Delete}");

    expect(within(palette).getByRole("combobox")).toHaveValue("e");
    expect(screen.queryByRole("alertdialog")).toBeNull();
    // Under the modal palette the strip is hidden from a role query, so it is counted as markup.
    expect(document.querySelectorAll('[aria-label="Tabs"] [role="tab"]')).toHaveLength(3);
  });

  it("never fires in a terminal: Delete and Backspace there are the shell's", async () => {
    onA("MacIntel");
    const { asked } = core();
    render(<App />);
    const terminal = await screen.findByLabelText("Terminal 2");
    terminal.focus();

    for (const key of ["Delete", "Backspace"]) {
      const press = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
      terminal.dispatchEvent(press);
      expect(press.defaultPrevented).toBe(false);
    }
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(tabsOf("Tabs")).toHaveLength(3);
    expect(asked).not.toContain("close_session");
  });
});

/**
 * **Closing a project that has chats open asks first**, as ending one chat does. The project's
 * `×` always ended every chat in it without a word, and #239 put that one key away — on a Mac,
 * the ordinary delete key. The question is on the `project.close:<plane>` row's verb, so the
 * `×`, Delete, the tab's menu and the palette all ask it.
 */
describe("closing a project", () => {
  /** Waits for the project to have said what it has open: before that, it would be asked about. */
  const settled = async () => {
    await waitFor(() => expect(tabsOf("Tabs")).toHaveLength(3));
    return tabsOf("Projects")[0];
  };

  it("asks first when chats are open, naming how many end and in which workspaces", async () => {
    const { asked } = core();
    render(<App />);
    await settled();

    await userEvent.click(screen.getByRole("button", { name: "Close project plane" }));

    const asking = await screen.findByRole("alertdialog");
    expect(asking).toHaveAccessibleName("Close project plane and end 3 chats?");
    expect(asking).toHaveTextContent("in alpha");
    expect(within(asking).getAllByRole("listitem")).toHaveLength(3);
    expect(asked).not.toContain("close_plane");
    // Cancel is first and focused: a stray Return keeps everything.
    expect(within(asking).getByRole("button", { name: "Cancel" })).toHaveFocus();
  });

  it("keeps everything on Cancel, and gives the keyboard back to the tab", async () => {
    const { asked } = core();
    render(<App />);
    const project = await settled();
    project.focus();

    await userEvent.keyboard("{Delete}");
    const asking = await screen.findByRole("alertdialog");
    await userEvent.click(within(asking).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(asked).not.toContain("close_plane");
    expect(tabsOf("Tabs")).toHaveLength(3);
    await waitFor(() => expect(tabsOf("Projects")[0]).toHaveFocus());
  });

  it("closes the project once the question is answered", async () => {
    const { asked } = core();
    render(<App />);
    const project = await settled();
    project.focus();

    await userEvent.keyboard("{Delete}");
    const asking = await screen.findByRole("alertdialog");
    await userEvent.click(within(asking).getByRole("button", { name: /end 3 chats/i }));

    await waitFor(() => expect(asked).toContain("close_plane"));
    expect(asked.filter((cmd) => cmd === "close_plane")).toHaveLength(1);
  });

  it("asks from the palette's row too, because the question is on the row's verb", async () => {
    const { asked } = core();
    render(<App />);
    await settled();

    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("Close project plane{Enter}");

    expect(await screen.findByRole("alertdialog")).toHaveAccessibleName(
      "Close project plane and end 3 chats?",
    );
    expect(asked).not.toContain("close_plane");
  });
});
