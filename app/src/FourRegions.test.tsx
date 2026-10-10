import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { forgetThisLaunch } from "./regions";
import { GLOBAL } from "./windowprefs";
import { onAMac } from "./tabKeys";
import { findStripNamed, showTheExplorer, stripNamed } from "./test-strips";

/**
 * **The window is four regions** (ADR 0038), against the whole app, because three of
 * them only mean anything together: the explorer picks where the next chat goes, the bar
 * starts it there, and the right-hand side is where the queue went.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;
const BETA = `${PLANE}/workspaces/beta`;
const CUT = `${ALPHA}/.worktrees/svc`;

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
  persona_profiles: {},
  ignore_fix: null,
  declares_none: true,
};

function chat(session: number, name: string, cwd: string | null) {
  return {
    session,
    name,
    cwd,
    harness: "claude",
    in_front: false,
    resumed: null,
    fresh: null,
    profile: "claude",
    persona: "steward",
    unreported: null,
    guessed: null,
  };
}

function piece(name: string, on: Record<string, unknown> = {}) {
  return { piece: name, path: `${CUT}/${name}`, branch: name, wired: true, stale: false, ...on };
}

/** The core, filing chats by the directory each one works in, as it really does.
 *
 *  `cut` is asked afresh on every listing, so a test can remove a worktree mid-run. */
function core(
  waiting: number[] = [],
  cut: () => ReturnType<typeof piece>[] = () => [piece("one"), piece("two")],
) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  const chats: ReturnType<typeof chat>[] = [];
  // Branches cut from the window (GL-1), listed after the ones `cut` names.
  const madeHere: string[] = [];
  let next = 0;
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: a });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "start_chat") {
      const answer = () => {
        chats.push(chat(++next, String(a.name), a.cwd as string | null));
        return { session: next };
      };
      // A start the test is holding open, as a checkout off the main thread holds one.
      return startHeld === undefined ? answer() : startHeld.then(answer);
    }
    // The operator has pinned every workspace, so every one is on the strip and can be
    // clicked there: the strip draws what is pinned and the one you are in (ADR 0054).
    if (cmd === "plane_pins") return { project: false, workspaces: ["alpha", "beta"], missing: [] };
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: ["steward"],
        persona: "steward",
        unfiled: chats.filter((one) => one.cwd === null || !one.cwd.startsWith(`${PLANE}/`)),
        workspaces: [
          {
            name: "alpha",
            path: ALPHA,
            vision: "Ship it",
            todos: [],
            chats: chats.filter((one) => one.cwd?.startsWith(ALPHA)),
          },
          {
            name: "beta",
            path: BETA,
            vision: "Retire the importer",
            todos: [],
            chats: chats.filter((one) => one.cwd?.startsWith(BETA)),
          },
        ],
      };
    if (cmd === "workspace_panels")
      return {
        workspace: a.workspace,
        repos: a.workspace === "alpha" ? ["svc"] : [],
        paths: a.workspace === "alpha" ? { svc: `${ALPHA}/svc` } : {},
        absent: [],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["steward"],
        persona: "steward",
      };
    if (cmd === "workspace_repos")
      return {
        workspace: a.workspace,
        repos: a.workspace === "alpha" ? repos : [],
        cache_refused: null,
      };
    if (cmd === "worktree_list")
      return a.workspace === "alpha" ? [...cut(), ...madeHere.map((name) => piece(name))] : [];
    if (cmd === "worktree_add") {
      const name = (a.branch as string | null) ?? "chat-1";
      madeHere.push(name);
      return { piece: name, path: `${CUT}/${name}`, branch: name, warnings: [] };
    }
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "chat_states")
      return waiting.map((session) => ({ session, state: "waiting", queue: waiting, sequence: 1 }));
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "alerts_everywhere") return alerts;
    return null;
  });
  return { asked };
}

/** While set, `start_chat` answers only once it settles: a start still in flight. */
let startHeld: Promise<void> | undefined;

/** What the core says is wrong in every project it holds. A test sets it before `core()`. */
let alerts: unknown = [];

/** What git says of alpha's clones. A test sets it before `core()`. */
let repos: unknown[] = [];

/** One clone's git answer, clean unless the test says otherwise. */
function repoState(name: string, on: Record<string, unknown> = {}) {
  return {
    name,
    branch: "main",
    unborn: false,
    detached: null,
    upstream: null,
    ahead: 0,
    behind: 0,
    tracked: 0,
    untracked: 0,
    unreadable: null,
    ci: null,
    change: null,
    sigil: null,
    fetched_seconds_ago: null,
    not_fetched: "nothing fetched yet",
    ...on,
  };
}

beforeEach(() => {
  startHeld = undefined;
  globalThis.localStorage.clear();
  // Every test is a launch: what an earlier one toggled is not this one's arrangement.
  forgetThisLaunch();
  alerts = [{ plane: PLANE, alerts: [], stopped: null }];
  repos = [];
});
afterEach(() => {
  cleanup();
  clearMocks();
});

/** Opens a chat where the operator is standing: New tab, a row, Start. */
async function openAChat() {
  await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
  await userEvent.click(await screen.findByRole("button", { name: "Start" }));
}

/** Focuses a workspace from the strip, which is the axis (ADR 0036). */
async function focus(workspace: string) {
  const tab = within(stripNamed("Workspaces"))
    .getAllByRole("tab")
    .find((one) => one.querySelector(".workspace-name")?.textContent === workspace);
  if (!tab) throw new Error(`no ${workspace} on the strip`);
  await userEvent.click(tab);
}

/** Where the last chat was started. */
const startedIn = (asked: { cmd: string; args: Record<string, unknown> }[]) =>
  asked.filter((one) => one.cmd === "start_chat").map((one) => one.args.cwd);

describe("the four regions", () => {
  it("draws every region, and each one is named for what it holds", async () => {
    core();
    render(<App />);

    // The left side opens on its Chats view (#1673, B-2), with the Explorer one press away.
    expect(await screen.findByRole("tabpanel", { name: "Chats" })).toBeInTheDocument();
    expect(await showTheExplorer()).toBeInTheDocument();
    expect(await screen.findByTestId("panels")).toBeInTheDocument();
    expect(await screen.findByLabelText("Repository state")).toBeInTheDocument();
    expect(stripNamed("Tabs")).toBeInTheDocument();
  });

  it("has exactly one thing called Workspaces, and it is the strip", async () => {
    // Two `nav[aria-label="Workspaces"]` broke a spec when the second appeared. The old left
    // sidebar was the second one; the explorer is not.
    core();
    render(<App />);
    // The strip itself, and not the explorer: the explorer draws as soon as the project does,
    // and the strip only once the plane has been read — so waiting on the explorer would ask
    // this question before the thing it is about exists.
    await findStripNamed("Workspaces");
    await screen.findByTestId("clone-svc");

    expect(screen.getAllByLabelText("Workspaces")).toHaveLength(1);
    expect(screen.getByLabelText("Workspaces")).toHaveAttribute("role", "tablist");
  });

  it("starts a chat in the workspace when nothing in the explorer is picked", async () => {
    const { asked } = core();
    render(<App />);
    await screen.findByTestId("clone-svc");

    await openAChat();

    expect(startedIn(asked)).toEqual([ALPHA]);
  });

  it("starts a chat in the worktree the explorer picked", async () => {
    // What makes the left a SELECTOR rather than a second listing. The path is the one the
    // core spelled in `worktree_list`; the window never joins one together.
    const { asked } = core();
    render(<App />);
    await screen.findByTestId("clone-svc");
    await showTheExplorer();

    await userEvent.click(await screen.findByRole("treeitem", { name: /^two/ }));
    await openAChat();

    expect(startedIn(asked)).toEqual([`${CUT}/two`]);
  });

  it("starts a chat in a clone picked from its menu, on the path the core spelled", async () => {
    // charter-app#174: a clone is picked one level up from a piece, from the heading's menu,
    // and the explorer marks it the way it marks a picked piece.
    const { asked } = core();
    render(<App />);
    const clone = await screen.findByTestId("clone-svc");
    await showTheExplorer();
    const heading = within(clone).getByRole("treeitem", { name: /^svc/ });

    fireEvent.contextMenu(heading);
    await userEvent.click(await screen.findByRole("menuitem", { name: "Start new chats in svc" }));
    await waitFor(() => expect(heading).toHaveAttribute("aria-current", "true"));
    await openAChat();

    expect(startedIn(asked)).toEqual([`${ALPHA}/svc`]);
  });

  it("opens the picker for a new tab in the clone from the Changes view's row too", async () => {
    const { asked } = core();
    render(<App />);
    const row = await screen.findByTestId("repo-svc");

    fireEvent.contextMenu(row);
    await userEvent.click(await screen.findByRole("menuitem", { name: "New tab in svc" }));
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));

    expect(startedIn(asked)).toEqual([`${ALPHA}/svc`]);
  });

  it("starts a chat in a clone on a new branch of its own unless the box is cleared (GL-1)", async () => {
    const { asked } = core();
    render(<App />);
    const boxes = () =>
      asked.filter((one) => one.cmd === "start_chat").map((one) => one.args.boxes);

    fireEvent.contextMenu(await screen.findByTestId("repo-svc"));
    await userEvent.click(await screen.findByRole("menuitem", { name: "New tab in svc" }));
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));
    await waitFor(() => expect(boxes()).toHaveLength(1));
    fireEvent.contextMenu(await screen.findByTestId("repo-svc"));
    await userEvent.click(await screen.findByRole("menuitem", { name: "New tab in svc" }));
    await userEvent.click(await screen.findByRole("checkbox", { name: /new branch in svc/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));
    await waitFor(() => expect(boxes()).toHaveLength(2));
    await openAChat();

    expect(boxes()).toEqual([
      { show_footer: false, new_branch: true, without_sandbox: null },
      { show_footer: false, new_branch: false, without_sandbox: null },
      // The workspace's own directory is in no repo: nothing to cut, and nothing asked for.
      { show_footer: false, new_branch: false, without_sandbox: null },
    ]);
  });

  it("cuts a new branch from a repo's menu, and new chats start on it (GL-1)", async () => {
    const { asked } = core();
    render(<App />);
    await showTheExplorer();
    const heading = within(await screen.findByTestId("clone-svc")).getByRole("treeitem", {
      name: /^svc/,
    });

    fireEvent.contextMenu(heading);
    await userEvent.click(await screen.findByRole("menuitem", { name: "New branch in svc…" }));
    await userEvent.type(
      within(await screen.findByRole("dialog", { name: "New branch" })).getByRole("textbox", {
        name: /^Name/,
      }),
      "spike{Enter}",
    );
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "New branch" })).not.toBeInTheDocument(),
    );
    await screen.findByTestId("piece-svc-spike");
    await openAChat();

    expect(asked.find(({ cmd }) => cmd === "worktree_add")?.args).toMatchObject({
      plane: PLANE,
      workspace: "alpha",
      repo: "svc",
      branch: "spike",
    });
    expect(startedIn(asked)).toEqual([`${CUT}/spike`]);
  });

  it("starts one chat however often Start is pressed while it is starting (GL-1)", async () => {
    // Off the main thread, a start takes as long as its checkout, and a second press of
    // Start in that time used to start a second chat.
    let release = () => {};
    startHeld = new Promise((done) => (release = done));
    const { asked } = core();
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));
    const starting = await screen.findByRole("button", { name: "Starting…" });
    expect(starting).toBeDisabled();
    fireEvent.click(starting);
    release();
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Start a chat" })).not.toBeInTheDocument(),
    );

    expect(asked.filter((one) => one.cmd === "start_chat")).toHaveLength(1);
  });

  it("starts only that tab in the clone, and the next New tab where it always did", async () => {
    // The review of #174: `New tab in svc` had set the explorer's pick, which made it the
    // same row as `Start new chats in svc` under another name.
    const { asked } = core();
    render(<App />);
    fireEvent.contextMenu(await screen.findByTestId("repo-svc"));
    await userEvent.click(await screen.findByRole("menuitem", { name: "New tab in svc" }));
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));
    await waitFor(() => expect(startedIn(asked)).toHaveLength(1));

    await openAChat();

    expect(startedIn(asked)).toEqual([`${ALPHA}/svc`, ALPHA]);
  });

  it("does not carry a pick into another workspace", async () => {
    // A pick belongs to the workspace it was made in. Carrying it across would point the
    // next chat at a directory in the workspace the operator has just left.
    const { asked } = core();
    render(<App />);
    await screen.findByTestId("clone-svc");
    await showTheExplorer();
    await userEvent.click(await screen.findByRole("treeitem", { name: /^two/ }));

    await focus("beta");
    await openAChat();

    expect(startedIn(asked)).toEqual([BETA]);
  });

  it("brings the pick back when its workspace is focused again", async () => {
    // Set aside, not thrown away — the same courtesy the chat strip does with the tab that
    // was in front on it.
    const { asked } = core();
    render(<App />);
    await screen.findByTestId("clone-svc");
    await showTheExplorer();
    await userEvent.click(await screen.findByRole("treeitem", { name: /^two/ }));

    await focus("beta");
    await focus("alpha");
    await openAChat();

    expect(startedIn(asked)).toEqual([`${CUT}/two`]);
  });

  it("drops a pick whose worktree git no longer lists", async () => {
    // The worktree was removed while it was picked. Starting the next chat in a directory
    // that is not there would make the operator read a refusal charter could see coming.
    let cut = [piece("one"), piece("two")];
    const { asked } = core([], () => cut);

    render(<App />);
    await screen.findByTestId("clone-svc");
    await showTheExplorer();
    await userEvent.click(await screen.findByRole("treeitem", { name: /^two/ }));

    cut = [piece("one")];
    await focus("beta");
    await focus("alpha");
    await screen.findByTestId("clone-svc");
    await openAChat();

    expect(startedIn(asked)).toEqual([ALPHA]);
  });

  it("puts the needs-you queue in the title bar, and not on the right (charter-app#249)", async () => {
    core([7]);
    render(<App />);

    const hand = await screen.findByRole("button", { name: "1 chat needs you" });
    expect(screen.getByTestId("title-bar")).toContainElement(hand);
    expect(within(await screen.findByTestId("panels")).queryByLabelText("Needs you")).toBeNull();
  });

  it("puts a region away and brings it back", async () => {
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });
    const chats = screen.getByTestId("chats-section");

    await userEvent.click(screen.getByRole("button", { name: "Navigation", pressed: true }));
    expect(screen.queryByRole("tabpanel", { name: "Chats" })).not.toBeInTheDocument();
    // The centre is still there: a window with no panes is not a state a button can reach.
    expect(stripNamed("Tabs")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Navigation", pressed: false }));
    expect(await screen.findByRole("tabpanel", { name: "Chats" })).toBeInTheDocument();
    // Hidden while it was away, never unmounted (#1673): the same list came back.
    expect(screen.getByTestId("chats-section")).toBe(chats);
  });

  it("can put the right-hand side away too", async () => {
    core();
    render(<App />);
    await screen.findByTestId("panels");

    await userEvent.click(screen.getByRole("button", { name: "Attention", pressed: true }));

    expect(screen.queryByTestId("panels")).not.toBeInTheDocument();
  });

  it("gives every region in the arrangement its own way back, on the status line", async () => {
    // The buttons are drawn from the arrangement, so a region added to the catalogue cannot
    // arrive with nowhere to bring it back from — which a list written out by hand allowed.
    //
    // **And they are on the status line, not on the bar** (charter-app#193, asked for twice).
    // The `within` is over the line itself rather than over `.regions-doing` for that reason:
    // the class would pass wherever the row was moved to, and where it is IS the claim.
    //
    // **Read by `aria-label` and not by text**, because there is no text — *"just small icons
    // without texts, texts only with tooltips"*. That the name survived losing the words is
    // the whole risk in that instruction, and it is what this line asserts.
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });

    expect(
      within(screen.getByTestId("status-line"))
        .getAllByRole("button")
        .filter((one) => one.getAttribute("aria-pressed") !== null)
        .map((one) => one.getAttribute("aria-label")),
    ).toEqual(["Navigation", "Attention"]);
    expect(document.querySelector("header.bar .regions-doing")).toBeNull();
  });

  it("keeps a chat in another workspace running while the explorer is used", async () => {
    // #125's guarantee, one scope down: nothing in this region ends anything.
    const { asked } = core();
    render(<App />);
    await screen.findByTestId("clone-svc");
    await openAChat();
    await showTheExplorer();

    await userEvent.click(await screen.findByRole("treeitem", { name: /^one/ }));
    await focus("beta");
    await focus("alpha");

    expect(asked.filter((one) => one.cmd === "close_session")).toEqual([]);
    expect(screen.getAllByTestId("pane")).toHaveLength(1);
  });
});

/**
 * **The left side's activity bar, in the window** (#1673, ADR 0038 as amended 2026-10-10): what
 * a person presses and sees. The bar's own rules are `RegionFrame.test.tsx`'s; the press's are
 * `regions.test.ts`'s.
 */
describe("the left side's activity bar", () => {
  const bar = () => screen.getByRole("tablist", { name: "Navigation" });
  const tab = (name: string) => within(bar()).getByRole("tab", { name });
  /** The chord with the platform's command key: ⌘ on a Mac, Ctrl elsewhere. */
  const chord = (key: string, shift = false) =>
    fireEvent.keyDown(document.body, {
      key,
      shiftKey: shift,
      ...(onAMac() ? { metaKey: true } : { ctrlKey: true }),
    });

  it("switches the side between Chats and Explorer, one at a time", async () => {
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });
    expect(tab("Chats")).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByRole("navigation", { name: "Explorer" })).toBeNull();

    await userEvent.click(tab("Explorer"));

    expect(await screen.findByRole("navigation", { name: "Explorer" })).toBeInTheDocument();
    expect(screen.queryByRole("tabpanel", { name: "Chats" })).toBeNull();
    expect(tab("Explorer")).toHaveAttribute("aria-selected", "true");
  });

  it("puts the side away on a press of the open view, keeps the bar, and brings it back", async () => {
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });
    const chats = screen.getByTestId("chats-section");

    await userEvent.click(tab("Chats"));

    expect(screen.queryByRole("tabpanel")).toBeNull();
    expect(bar()).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Navigation", pressed: false })).toBeInTheDocument();

    await userEvent.click(tab("Chats"));

    expect(await screen.findByRole("tabpanel", { name: "Chats" })).toBeInTheDocument();
    expect(screen.getByTestId("chats-section")).toBe(chats);
  });

  it("counts the chats that need you on the Chats tab, while the side is away too", async () => {
    core([7]);
    render(<App />);
    await screen.findByRole("button", { name: "1 chat needs you" });
    await waitFor(() => expect(tab("Chats")).toHaveAccessibleDescription("1 chat needs you"));
    expect(tab("Chats").querySelector('.activity-count[data-tone="needs-you"]')).toHaveTextContent(
      "1",
    );

    await userEvent.click(tab("Chats"));

    expect(screen.queryByRole("tabpanel")).toBeNull();
    expect(tab("Chats")).toHaveAccessibleDescription("1 chat needs you");
  });

  it("draws no count when nothing needs you", async () => {
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });

    expect(tab("Chats").querySelector(".activity-count")).toBeNull();
  });

  it("answers its keys: the side away and back, then Explorer and Chats shown", async () => {
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });

    chord("b");
    await waitFor(() => expect(screen.queryByRole("tabpanel")).toBeNull());
    chord("b");
    expect(await screen.findByRole("tabpanel", { name: "Chats" })).toBeInTheDocument();

    chord("E", true);
    expect(await screen.findByRole("navigation", { name: "Explorer" })).toBeInTheDocument();
    // The key gives the view the keyboard, as an editor's does.
    await waitFor(() =>
      expect(document.activeElement?.closest('[role="tabpanel"]')).toHaveAccessibleName("Explorer"),
    );

    chord("C", true);
    expect(await screen.findByRole("tabpanel", { name: "Chats" })).toBeInTheDocument();
  });

  it("comes back as it was left when the project is opened again", async () => {
    core();
    const { unmount } = render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });
    await userEvent.click(tab("Explorer"));
    await screen.findByRole("navigation", { name: "Explorer" });
    unmount();

    core();
    render(<App />);

    expect(await screen.findByRole("navigation", { name: "Explorer" })).toBeInTheDocument();
  });
});

/**
 * **Search and Changes on the left** (#1676, B-2, B-7): two more views on the navigation
 * region's bar. Search is the Search tab's content in the side; Changes is what the bottom
 * region drew, which is gone, so the terminals have the window's whole height.
 */
describe("the Search and Changes views", () => {
  const bar = () => screen.getByRole("tablist", { name: "Navigation" });
  const tab = (name: string) => within(bar()).getByRole("tab", { name });

  it("holds every view of the left side, in the bar's order", async () => {
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });

    expect(
      within(bar())
        .getAllByRole("tab")
        .map((one) => one.getAttribute("aria-label")),
    ).toEqual(["Chats", "Explorer", "Search", "Changes"]);
  });

  it("shows Search on its key, with the box given the keyboard", async () => {
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });

    fireEvent.keyDown(document.body, {
      key: "F",
      shiftKey: true,
      ...(onAMac() ? { metaKey: true } : { ctrlKey: true }),
    });

    const view = await screen.findByRole("tabpanel", { name: "Search" });
    const box = within(view).getByRole("searchbox", { name: "Search the files" });
    await waitFor(() => expect(box).toHaveFocus());
    // A view in the side, not a tab in the centre: the tab strip gained nothing.
    expect(within(stripNamed("Tabs")).queryByRole("tab", { name: /^Search/ })).toBeNull();
  });

  it("keeps what was typed when the side is switched away and back", async () => {
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });
    await userEvent.click(tab("Search"));
    const box = within(await screen.findByRole("tabpanel", { name: "Search" })).getByRole(
      "searchbox",
    );
    // `change` rather than typed keys: jsdom lays nothing out, so the panels' separator takes
    // a pointer press anywhere in the group (as `ChatsList.window.test.tsx` meets it too).
    fireEvent.change(box, { target: { value: "needle" } });

    await userEvent.click(tab("Chats"));
    await userEvent.click(tab("Search"));

    expect(
      within(await screen.findByRole("tabpanel", { name: "Search" })).getByRole("searchbox"),
    ).toHaveValue("needle");
  });

  it("searches where the window is when its tab opens it, as its key does", async () => {
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });
    const scope = () =>
      within(screen.getByRole("tabpanel", { name: "Search" })).getByRole("combobox", {
        name: "Where to search",
      });
    await userEvent.click(tab("Search"));
    await waitFor(() => expect(scope()).toHaveDisplayValue("Workspace alpha"));

    await userEvent.click(tab("Chats"));
    await focus("beta");
    await userEvent.click(tab("Search"));

    await waitFor(() => expect(scope()).toHaveDisplayValue("Workspace beta"));
  });

  it("shows Changes on ⌃⇧G, holding the repos, their branches and pipelines", async () => {
    repos = [repoState("svc", { tracked: 3, untracked: 1 })];
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });

    fireEvent.keyDown(document.body, { key: "G", shiftKey: true, ctrlKey: true });

    const view = await screen.findByRole("tabpanel", { name: "Changes" });
    const state = within(view).getByLabelText("Repository state");
    await waitFor(() => expect(within(state).getByTestId("repo-svc")).toHaveTextContent("main"));
    expect(within(state).getByTestId("repo-svc")).toHaveTextContent("3 changed, 1 untracked");
    expect(within(state).getByTestId("ci-svc")).toHaveTextContent("not fetched");
  });

  it("counts the uncommitted files on the Changes tab, while the side is away too", async () => {
    repos = [
      repoState("svc", { tracked: 3, untracked: 1 }),
      repoState("tool", { tracked: 1 }),
      // A tree purlis could not read is not counted as clean, nor as anything.
      repoState("gone", { unreadable: "permission denied" }),
    ];
    core();
    render(<App />);
    await waitFor(() => expect(tab("Changes")).toHaveAccessibleDescription("5 uncommitted files"));
    expect(tab("Changes").querySelector(".activity-count")).toHaveTextContent("5");
    // Not a question for the person: the plain count's colours, never the needs-you ones.
    expect(tab("Changes").querySelector('.activity-count[data-tone="needs-you"]')).toBeNull();

    await userEvent.click(tab("Chats"));

    expect(screen.queryByRole("tabpanel")).toBeNull();
    expect(tab("Changes")).toHaveAccessibleDescription("5 uncommitted files");
  });

  it("draws no count on Changes when every tree is clean", async () => {
    repos = [repoState("svc")];
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });
    await userEvent.click(tab("Changes"));
    await within(await screen.findByRole("tabpanel", { name: "Changes" })).findByTestId("repo-svc");

    expect(tab("Changes").querySelector(".activity-count")).toBeNull();
  });

  it("has no bottom region any more: what it showed is in Changes alone", async () => {
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });

    expect(screen.queryByTestId("region-bottom")).toBeNull();
    expect(screen.getAllByLabelText("Repository state")).toHaveLength(1);
    expect(screen.getByLabelText("Repository state").closest('[role="tabpanel"]')).toHaveAttribute(
      "data-view",
      "changes",
    );
    expect(screen.queryByRole("button", { name: "State" })).toBeNull();
  });
});

/**
 * **The arrangement, against the whole window** (`regions.ts`).
 *
 * The four regions used to be the shape of `PlaneView`'s JSX. They are a stored document now,
 * and these are the assertions that say the window really reads it — a layout that is only data
 * in one module is not configurable.
 */
describe("the window the stored arrangement asks for", () => {
  /** What the window reads before it renders: the layout file, handed to the page as it is
   *  created (`windowprefs.ts`). */
  const arrange = (regions: unknown) => {
    (globalThis as Record<string, unknown>)[GLOBAL] = {
      layout: {
        path: "layout.json",
        found: true,
        document: { version: 1, regions },
        trouble: null,
      },
      theme: { path: "theme.json", found: false, document: null, trouble: null },
    };
  };
  afterEach(() => {
    Reflect.deleteProperty(globalThis, GLOBAL);
  });

  const inSlot = (side: string) => within(screen.getByTestId(`region-${side}`));

  it("draws a region on the side the document names, with no JSX moved", async () => {
    arrange([
      { id: "explorer", side: "right", order: 0, collapsed: false },
      { id: "aside", side: "left", order: 0, collapsed: false },
    ]);
    core();
    render(<App />);
    await screen.findByTestId("panels");

    expect(inSlot("left").getByTestId("panels")).toBeInTheDocument();
    expect(inSlot("right").getByTestId("chats-section")).toBeInTheDocument();
  });

  it("stacks two regions in one slot in the order the document gives them", async () => {
    arrange([
      { id: "explorer", side: "left", order: 0, collapsed: false },
      { id: "aside", side: "left", order: -1, collapsed: false },
    ]);
    core();
    render(<App />);
    await screen.findByTestId("panels");

    const drawn = inSlot("left").getAllByTestId(/^(panels|chats-section)$/);
    expect(drawn.map((one) => one.dataset.testid)).toEqual(["panels", "chats-section"]);
  });

  it("draws a file that still places the bottom region, without it and without a word", async () => {
    // Every file before #1676 placed it. What it drew is the Changes view now.
    arrange([
      { id: "explorer", side: "left", order: 0, collapsed: false },
      { id: "aside", side: "right", order: 0, collapsed: false },
      { id: "bottom", side: "bottom", order: 0, collapsed: true },
    ]);
    core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });

    expect(screen.queryByTestId("region-bottom")).toBeNull();
    expect(screen.getAllByLabelText("Repository state")).toHaveLength(1);
  });

  it("launches with a region away, and its slot is still in the group", async () => {
    // A slot that is put away is collapsed and never removed — charter-app#141's throw. What
    // the operator loses is the content, which is unmounted so it is out of the tab order too.
    arrange([
      { id: "explorer", side: "left", order: 0, collapsed: true },
      { id: "aside", side: "right", order: 0, collapsed: false },
    ]);
    core();
    render(<App />);
    await screen.findByTestId("panels");

    // Its views are hidden, and still mounted (#1673): collapsing never unmounts a view.
    expect(screen.queryByRole("navigation", { name: "Explorer" })).not.toBeInTheDocument();
    expect(screen.queryByRole("tabpanel")).not.toBeInTheDocument();
    expect(screen.getByTestId("explorer")).toBeInTheDocument();
    expect(screen.getByTestId("region-left")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Navigation", pressed: false })).toBeInTheDocument();
  });

  it("writes what the operator did back where the next launch will read it", async () => {
    const { asked } = core();
    render(<App />);
    await screen.findByRole("tabpanel", { name: "Chats" });

    await userEvent.click(screen.getByRole("button", { name: "Navigation", pressed: true }));

    await vi.waitFor(() => expect(asked.some((one) => one.cmd === "write_layout")).toBe(true));
    const written = asked.filter((one) => one.cmd === "write_layout").at(-1);
    const arrangement = [
      { id: "navigation", side: "left", order: 0, collapsed: true },
      { id: "aside", side: "right", order: 0, collapsed: false },
    ];
    expect(JSON.parse(String(written?.args.text))).toEqual({
      version: 2,
      // The machine's, which a project with none of its own starts from, and the project's own
      // (#1673).
      regions: arrangement,
      projects: { [PLANE]: { regions: arrangement } },
      // The machine's text sizes share the file (charter-app#283), written as they stand.
      text: { window: 14, terminal: 13 },
    });
  });
});

/**
 * **The status line**, against the whole window, because every question about it is a question
 * about where it is relative to everything else (`StatusLine.tsx` for what is on it and why).
 *
 * The operator asked for the project's directory to move out of the top-right corner into a
 * one-line bar at the very bottom. These are the assertions that it went there and that it is
 * not a fifth region; the component's own rules are in `StatusLine.test.tsx`.
 */
describe("the status line", () => {
  it("sits below every region, and outside the group that draws them", async () => {
    core();
    render(<App />);
    await screen.findByTestId("panels");

    const line = screen.getByTestId("status-line");
    const frame = document.querySelector(".region-frame");
    if (frame === null) throw new Error("no region frame");
    // Under the regions, not beside them.
    expect(frame.compareDocumentPosition(line) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // And not inside the panel group at all: it is the window's chrome, the way the project
    // strip above the regions is, and `StatusLine.tsx` argues why it is not in the arrangement.
    expect(document.querySelector(".regions")?.contains(line)).toBe(false);
  });

  it("carries the project's directory, which is no longer in the corner of the bar", async () => {
    core();
    render(<App />);
    await screen.findByTestId("status-line");

    expect(within(screen.getByTestId("status-line")).getByText(PLANE)).toBeInTheDocument();
    // The bar is the tab strip, the `+`, the splits and the region toggles, and nothing else.
    expect(document.querySelector("header.bar .plane")).toBeNull();
  });

  it("says which workspace the window is on, and follows the focus", async () => {
    core();
    render(<App />);
    await screen.findByTestId("clone-svc");

    const line = screen.getByTestId("status-line");
    expect(line).toHaveTextContent("alpha");

    await focus("beta");

    await waitFor(() => expect(line).toHaveTextContent("beta"));
  });

  it("counts what the workspace holds, off the answers the regions already asked for", async () => {
    // No second `workspace_panels` and no second `worktree_list`: the line reads the state the
    // three regions share (`useWorkspaceState`), which is what keeps `git status` per clone
    // from running twice for one focused workspace.
    const { asked } = core();
    render(<App />);
    await screen.findByTestId("clone-svc");

    // The mock's `alpha` holds one clone with two worktrees cut off it, on a plane of two
    // workspaces.
    const line = screen.getByTestId("status-line");
    await waitFor(() => expect(line).toHaveTextContent(/branches\s*2/));
    expect(line).toHaveTextContent(/ws\s*2/);

    const listings = asked.filter(
      (one) => one.cmd === "worktree_list" && one.args.workspace === "alpha",
    );
    expect(listings).toHaveLength(1);
  });

  it("cannot be put away, because it is not a region", async () => {
    // Every region in the arrangement gets a toggle drawn from it. The status line has none —
    // it is the frame, and a status line an operator can lose is one they will lose.
    core();
    render(<App />);
    await screen.findByTestId("panels");

    for (const name of ["Navigation", "Attention"]) {
      await userEvent.click(screen.getByRole("button", { name, pressed: true }));
    }

    expect(screen.queryByRole("button", { name: "Status" })).toBeNull();
    expect(screen.getByTestId("status-line")).toBeInTheDocument();
  });

  it("counts every project's alerts on the button and opens the drawer over the window", async () => {
    // The drawer is the window's, and the core is asked about every project at once: the
    // command names no plane, so nothing can wire it to the one in front by mistake.
    alerts = [
      {
        plane: PLANE,
        alerts: [
          {
            severity: "warn",
            subject: "reinit",
            detail: "1 workspace is behind the current layout: beta",
            way: { kind: "fix", id: "workspace-reinit" },
          },
        ],
        stopped: null,
      },
    ];
    const { asked } = core();
    render(<App />);

    const button = await screen.findByRole("button", { name: "Alerts: 1" });
    const before = asked.filter((one) => one.cmd === "alerts_everywhere").length;
    await userEvent.click(button);

    const drawer = await screen.findByRole("dialog", { name: "Alerts" });
    const plane = within(drawer).getByRole("region", { name: "Alerts in plane" });
    expect(plane).toHaveTextContent("1 workspace is behind the current layout: beta");
    expect(within(plane).getByRole("button", { name: "Reinit" })).toBeVisible();
    // Opening it asked again, so it lists what is true when it is looked at.
    await vi.waitFor(() =>
      expect(asked.filter((one) => one.cmd === "alerts_everywhere").length).toBeGreaterThan(before),
    );
    for (const one of asked.filter((a) => a.cmd === "alerts_everywhere"))
      expect(one.args).toEqual({});

    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("drops the count when purlis stopped looking in a project, and draws no zero", async () => {
    alerts = [{ plane: PLANE, alerts: [], stopped: "charter.toml is not valid TOML" }];
    core();
    render(<App />);

    const button = await screen.findByRole("button", { name: "Alerts: not counted" });
    expect(button).toBeEnabled();
    expect(button).not.toHaveTextContent("0");
  });

  it("draws no alerts section in the right-hand region any more", async () => {
    core();
    render(<App />);
    await screen.findByRole("button", { name: "Alerts: none" });

    expect(screen.queryByTestId("panel-alerts")).toBeNull();
  });
});
