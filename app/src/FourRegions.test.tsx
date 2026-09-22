import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

/**
 * **The window is four regions** (charter ADR 0038), against the whole app, because three of
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
  let next = 0;
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: a });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "start_chat") {
      chats.push(chat(++next, String(a.name), a.cwd as string | null));
      return { session: next, wired: null };
    }
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
        absent: [],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["steward"],
        persona: "steward",
      };
    if (cmd === "workspace_repos")
      return { workspace: a.workspace, repos: [], cache_refused: null };
    if (cmd === "worktree_list") return a.workspace === "alpha" ? cut() : [];
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "chat_states")
      return waiting.map((session) => ({ session, state: "waiting", queue: waiting }));
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    return null;
  });
  return { asked };
}

beforeEach(() => globalThis.localStorage.clear());
afterEach(() => {
  cleanup();
  clearMocks();
});

/** Opens a chat where the operator is standing: New tab, a row, Start. */
async function openAChat() {
  await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
  await userEvent.click(await screen.findByRole("button", { name: "Start" }));
}

/** Focuses a workspace from the strip, which is the axis (charter ADR 0036). */
async function focus(workspace: string) {
  const tab = within(screen.getByRole("tablist", { name: "Workspaces" }))
    .getAllByRole("tab")
    .find((one) => one.querySelector(".workspace-name")?.textContent === workspace);
  if (!tab) throw new Error(`no ${workspace} on the strip`);
  await userEvent.click(tab);
}

/** Where the last chat was started. */
const startedIn = (asked: { cmd: string; args: Record<string, unknown> }[]) =>
  asked.filter((one) => one.cmd === "start_chat").map((one) => one.args.cwd);

describe("the four regions", () => {
  it("draws all four, and each one is named for what it holds", async () => {
    core();
    render(<App />);

    expect(await screen.findByLabelText("Explorer")).toBeInTheDocument();
    expect(await screen.findByTestId("panels")).toBeInTheDocument();
    expect(await screen.findByLabelText("Repository state")).toBeInTheDocument();
    expect(screen.getByRole("tablist", { name: "Tabs" })).toBeInTheDocument();
  });

  it("has exactly one thing called Workspaces, and it is the strip", async () => {
    // Two `nav[aria-label="Workspaces"]` broke a spec when the second appeared. The old left
    // sidebar was the second one; the explorer is not.
    core();
    render(<App />);
    // The strip itself, and not the explorer: the explorer draws as soon as the project does,
    // and the strip only once the plane has been read — so waiting on the explorer would ask
    // this question before the thing it is about exists.
    await screen.findByRole("tablist", { name: "Workspaces" });
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

    await userEvent.click(await screen.findByRole("button", { name: /^two/ }));
    await openAChat();

    expect(startedIn(asked)).toEqual([`${CUT}/two`]);
  });

  it("does not carry a pick into another workspace", async () => {
    // A pick belongs to the workspace it was made in. Carrying it across would point the
    // next chat at a directory in the workspace the operator has just left.
    const { asked } = core();
    render(<App />);
    await screen.findByTestId("clone-svc");
    await userEvent.click(await screen.findByRole("button", { name: /^two/ }));

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
    await userEvent.click(await screen.findByRole("button", { name: /^two/ }));

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
    await userEvent.click(await screen.findByRole("button", { name: /^two/ }));

    cut = [piece("one")];
    await focus("beta");
    await focus("alpha");
    await screen.findByTestId("clone-svc");
    await openAChat();

    expect(startedIn(asked)).toEqual([ALPHA]);
  });

  it("puts the needs-you queue on the right, not on the bar", async () => {
    core([]);
    render(<App />);

    const queue = await screen.findByLabelText("Needs you");
    expect(within(screen.getByTestId("panels")).getByLabelText("Needs you")).toBe(queue);
    expect(document.querySelector("header.bar .needs-you")).toBeNull();
  });

  it("puts a region away and brings it back", async () => {
    core();
    render(<App />);
    await screen.findByLabelText("Explorer");

    await userEvent.click(screen.getByRole("button", { name: "Explorer", pressed: true }));
    expect(screen.queryByTestId("explorer")).not.toBeInTheDocument();
    // The centre is still there: a window with no panes is not a state a button can reach.
    expect(screen.getByRole("tablist", { name: "Tabs" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Explorer", pressed: false }));
    expect(await screen.findByTestId("explorer")).toBeInTheDocument();
  });

  it("can put the bottom bar and the right-hand side away too", async () => {
    core();
    render(<App />);
    await screen.findByTestId("bottom-bar");

    await userEvent.click(screen.getByRole("button", { name: "State", pressed: true }));
    await userEvent.click(screen.getByRole("button", { name: "Attention", pressed: true }));

    expect(screen.queryByTestId("bottom-bar")).not.toBeInTheDocument();
    expect(screen.queryByTestId("panels")).not.toBeInTheDocument();
  });

  it("gives every region in the arrangement its own way back", async () => {
    // The buttons are drawn from the arrangement, so a region added to the catalogue cannot
    // arrive with nowhere to bring it back from — which a list written out by hand allowed.
    core();
    render(<App />);
    await screen.findByLabelText("Explorer");

    expect(
      within(document.querySelector(".regions-doing") as HTMLElement)
        .getAllByRole("button")
        .map((one) => one.textContent),
    ).toEqual(["Explorer", "Attention", "State"]);
  });

  it("keeps a chat in another workspace running while the explorer is used", async () => {
    // #125's guarantee, one scope down: nothing in this region ends anything.
    const { asked } = core();
    render(<App />);
    await screen.findByTestId("clone-svc");
    await openAChat();

    await userEvent.click(await screen.findByRole("button", { name: /^one/ }));
    await focus("beta");
    await focus("alpha");

    expect(asked.filter((one) => one.cmd === "close_session")).toEqual([]);
    expect(screen.getAllByTestId("pane")).toHaveLength(1);
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
  /** What the window reads before it renders. */
  const arrange = (regions: unknown) =>
    globalThis.localStorage.setItem("charter.layout", JSON.stringify({ regions }));

  const inSlot = (side: string) => within(screen.getByTestId(`region-${side}`));

  it("draws a region on the side the document names, with no JSX moved", async () => {
    arrange([
      { id: "explorer", side: "left", order: 0, collapsed: false },
      { id: "aside", side: "right", order: 0, collapsed: false },
      { id: "bottom", side: "right", order: 1, collapsed: false },
    ]);
    core();
    render(<App />);
    await screen.findByLabelText("Repository state");

    expect(inSlot("right").getByLabelText("Repository state")).toBeInTheDocument();
    expect(inSlot("bottom").queryByLabelText("Repository state")).not.toBeInTheDocument();
  });

  it("stacks two regions in one slot in the order the document gives them", async () => {
    arrange([
      { id: "explorer", side: "left", order: 0, collapsed: false },
      { id: "bottom", side: "left", order: -1, collapsed: false },
      { id: "aside", side: "right", order: 0, collapsed: false },
    ]);
    core();
    render(<App />);
    await screen.findByLabelText("Repository state");

    const drawn = inSlot("left").getAllByTestId(/^(explorer|bottom-bar)$/);
    expect(drawn.map((one) => one.dataset.testid)).toEqual(["bottom-bar", "explorer"]);
  });

  it("launches with a region away, and its slot is still in the group", async () => {
    // A slot that is put away is collapsed and never removed — charter-app#141's throw. What
    // the operator loses is the content, which is unmounted so it is out of the tab order too.
    arrange([
      { id: "explorer", side: "left", order: 0, collapsed: true },
      { id: "aside", side: "right", order: 0, collapsed: false },
      { id: "bottom", side: "bottom", order: 0, collapsed: false },
    ]);
    core();
    render(<App />);
    await screen.findByTestId("panels");

    expect(screen.queryByTestId("explorer")).not.toBeInTheDocument();
    expect(screen.getByTestId("region-left")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Explorer", pressed: false })).toBeInTheDocument();
  });

  it("writes what the operator did back where the next launch will read it", async () => {
    core();
    render(<App />);
    await screen.findByLabelText("Explorer");

    await userEvent.click(screen.getByRole("button", { name: "Explorer", pressed: true }));

    expect(JSON.parse(globalThis.localStorage.getItem("charter.layout") ?? "null")).toEqual({
      regions: [
        { id: "explorer", side: "left", order: 0, collapsed: true },
        { id: "aside", side: "right", order: 0, collapsed: false },
        { id: "bottom", side: "bottom", order: 0, collapsed: false },
      ],
    });
  });
});
