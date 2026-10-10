import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
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
import type { FinishedTask, Moved, OpenChat } from "./bindings";
import type { State } from "./chatState";
import { drawnWith } from "./cascade.testkit";
import { card, cardOf as cardOfRow, facts, theCard } from "./chatCard.testkit";
import { CARD_DELAY_MS, CARD_LEAVE_MS, CARD_SKIP_MS } from "./ChatsSection";
import { forgetThisLaunch } from "./regions";
import { GLOBAL } from "./windowprefs";
import type { Shown } from "./shownState";
import { stripNamed } from "./test-strips";

/**
 * **The Chats section, against the whole window** (#1447): every running chat of the project
 * in one tree in the left region, nested by which chat started which. A handoff has a tab; a
 * task is listed with none of its own, and its row shows it inside the tab of the session that
 * asked for it (#1486, and `TaskInTab.window.test.tsx` for what that tab then says).
 *
 * Nothing starts a task chat yet, so the ones here are the core's answer as a fixture: a chat
 * whose `from` says `task: true` and `tab: false`. The record that says so is `reopen.rs`'s
 * and `chats.rs`'s.
 */

const drawn = vi.hoisted(() => ({ marks: [] as (string | undefined)[] }));

vi.mock("./StateShown", async (original) => {
  const real = await original<typeof import("./StateShown")>();
  return {
    ...real,
    StateShown: (props: { shown: Shown }) => {
      drawn.marks.push(props.shown.word);
      return <real.StateShown {...props} />;
    },
  };
});

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

/** The tab in front, by its session's name. */
const frontTab = () =>
  within(stripNamed("Tabs"))
    .getAllByRole("tab")
    .filter((tab) => tab.getAttribute("aria-selected") === "true")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";

type Lineage = NonNullable<OpenChat["from"]>;

/** A chat working in `workspace`, as the core lists it. */
function chat(
  session: number,
  workspace: string,
  more: Partial<OpenChat> = {},
): OpenChat & { workspace: string } {
  return {
    session,
    name: String(session),
    cwd: `${PLANE}/workspaces/${workspace}`,
    harness: "claude",
    in_front: session === 1,
    resumed: null,
    fresh: null,
    profile: null,
    persona: "steward",
    unreported: null,
    card: null,
    guessed: null,
    pinned: false,
    label: null,
    from: null,
    ...more,
    workspace,
  };
}

/** Started by chat `asker`, which the operator saw as `name` in `workspace`. */
function by(
  asker: number,
  mode: "handoff" | "task",
  name = `steward ${asker}`,
  workspace = "alpha",
): Lineage {
  return {
    chat: asker,
    name,
    workspace,
    task: mode === "task",
    tab: mode === "handoff",
    reported: false,
    unreported: false,
  };
}

/** A fixture's chat as the core sends it: the workspace is where it is filed, not a field. */
function asListed(one: OpenChat & { workspace: string }): OpenChat {
  const sent: OpenChat & { workspace?: string } = { ...one };
  delete sent.workspace;
  return sent;
}

type Asked = { cmd: string; args: Record<string, unknown> };

/** What the pretend core says chats used (#1500): task 3 reported, every other none yet. */
function usedFor(a: Record<string, unknown>) {
  return {
    chats: ((a.chats as number[]) ?? []).map((session) =>
      session === 3
        ? { session, tokens: "12k in, 3k out", unsaid: null }
        : { session, tokens: null, unsaid: "not_yet" },
    ),
    finished: [],
    total: null,
  };
}

/**
 * The core, holding `open` chats in two workspaces. It answers `open_chat_tab` the way the
 * record does: the chat has a tab from then on, so a second window on it draws one.
 */
function core(open: (OpenChat & { workspace: string })[], finished: FinishedTask[] = []) {
  const asked: Asked[] = [];
  const listeners = new Map<string, number>();
  /** Every listener of an event: `plane-changed` has one per answer the window reads. */
  const everyListener = new Map<string, number[]>();
  const now = () => open.map(asListed);
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: a });
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, handler);
      everyListener.set(event, [...(everyListener.get(event) ?? []), handler]);
      return 1;
    }
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return now();
    if (cmd === "finished_tasks") return [...finished];
    // Both workspaces on the strip, so a test can focus either (#1655).
    if (cmd === "plane_pins") return { project: false, workspaces: ["alpha", "beta"], missing: [] };
    if (cmd === "open_chat_tab") {
      const one = open.find((chat) => chat.session === a.session);
      if (one?.from) one.from = { ...one.from, tab: true };
      return null;
    }
    if (cmd === "tab_shows") {
      // The record keeps what a session's tab shows on that session's own entry (#1486).
      const one = open.find((chat) => chat.session === a.session);
      if (one) one.shows = (a.shown as number | null) ?? null;
      return null;
    }
    if (cmd === "stopping_chats") return [];
    // Stop all tasks (#1498): the tasks at work below the session, deepest first.
    if (cmd === "all_tasks_ending") return { name: `steward ${String(a.session)}`, tasks: [3, 2] };
    if (cmd === "stop_all_tasks") return (a.tasks as number[]).length;
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: ["steward", "devops"],
        persona: "steward",
        unfiled: [],
        workspaces: ["alpha", "beta"].map((name) => ({
          name,
          path: `${PLANE}/workspaces/${name}`,
          vision: "",
          todos: [],
          colour: null,
          live: false,
          chats: open.filter((chat) => chat.workspace === name).map(asListed),
        })),
      };
    if (cmd === "workspace_panels")
      return {
        workspace: a.workspace,
        repos: [],
        paths: {},
        absent: [],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["steward", "devops"],
        persona: "steward",
      };
    if (cmd === "workspace_repos")
      return { workspace: a.workspace, repos: [], cache_refused: null };
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "alerts_everywhere") return [{ plane: PLANE, alerts: [], stopped: null }];
    if (cmd === "tasks_used") return usedFor(a);
    return null;
  });
  return {
    asked,
    /**
     * **The core says how one of its chats stands has changed** (#1484): `change` is made to
     * what it holds, and it says the sidebar's answer moved, as it does when a task's question
     * opens or is answered and when a report lands (`Held::rows_changed`). The window reads
     * the sidebar again; nothing else tells it.
     */
    rowsChange: async (session: number, change: (from: Lineage) => Lineage) => {
      const one = open.find((chat) => chat.session === session);
      if (one?.from == null) throw new Error(`chat ${session} was started by no chat`);
      one.from = change(one.from);
      const handlers = everyListener.get("plane-changed") ?? [];
      if (handlers.length === 0) throw new Error("the window is not listening for changes");
      await act(async () => {
        for (const handler of handlers)
          window.__TAURI_INTERNALS__.runCallback(handler, {
            event: "plane-changed",
            id: 1,
            payload: {
              plane: PLANE,
              changes: [{ kind: "chats", workspace: null, persona: null, path: "" }],
              answers: [{ answer: "sidebar" }],
            },
          });
        await Promise.resolve();
      });
    },
    /** The core says a chat another chat started has arrived. */
    arrive: (one: OpenChat & { workspace: string }) => {
      const handler = listeners.get("handoff-arrived");
      if (handler === undefined) throw new Error("the window is not listening for arrivals");
      act(() => {
        window.__TAURI_INTERNALS__.runCallback(handler, {
          event: "handoff-arrived",
          id: 1,
          payload: {
            plane: PLANE,
            session: one.session,
            name: one.name,
            label: one.label,
            from: one.from,
            workspace: one.workspace,
            persona: one.persona,
            harness: one.harness,
          },
        });
      });
    },
    /** The core says chat `session`'s stop is at `phase`. One that has ended is gone from
     *  what the core lists, as it is from the core. */
    stop: (session: number, phase: "stopping" | "stopped") => {
      const handler = listeners.get("chat-stop");
      if (handler === undefined) throw new Error("the window is not listening for stops");
      if (phase === "stopped")
        open.splice(
          open.findIndex((chat) => chat.session === session),
          1,
        );
      act(() => {
        window.__TAURI_INTERNALS__.runCallback(handler, {
          event: "chat-stop",
          id: 1,
          payload: { plane: PLANE, session, phase },
        });
      });
    },
    /** The core says chat `session` moved to `state`. */
    move: (
      session: number,
      state: State,
      at: number,
      queue: number[] = [],
      children: Moved["children"] = [],
      needs: Moved["needs"] = null,
      reports: string[] = [],
      stopped: Moved["stopped"] = null,
    ) => {
      const handler = listeners.get("chat-moved");
      if (handler === undefined) throw new Error("the window is not listening for moves");
      const moved: Moved = {
        plane: PLANE,
        session,
        state,
        needs_you: queue.includes(session),
        queue,
        moved_at: at,
        sequence: at,
        reports,
        refusals: [],
        children,
        needs,
        stopped,
      };
      act(() => {
        window.__TAURI_INTERNALS__.runCallback(handler, {
          event: "chat-moved",
          id: 1,
          payload: moved,
        });
      });
    },
  };
}

const section = () => screen.findByRole("tree", { name: "Chats of this project" });

/** Each row of the section as `level name`, top to bottom. */
const shape = (tree: HTMLElement) =>
  within(tree)
    .getAllByRole("treeitem")
    .map(
      (row) => `${row.getAttribute("aria-level")} ${row.querySelector(".session")?.textContent}`,
    );

const row = (tree: HTMLElement, name: string) => {
  const found = within(tree)
    .getAllByRole("treeitem")
    .find((one) => one.querySelector(".session")?.textContent === name);
  if (found === undefined) throw new Error(`no row is named ${name}`);
  return found;
};

/** What a row says its chat is doing: the word a person reads on it, and its mark's shape. */
const says = (on: HTMLElement) => ({
  word: on.querySelector(".shown-state .word")?.textContent,
  shape: on.querySelector(".shown-state .shape")?.getAttribute("data-shape"),
});

/** What row `name`'s card says (#1675). */
const cardOf = (tree: HTMLElement, name: string) => cardOfRow(row(tree, name));

const strip = () => stripNamed("Tabs");
const tabNames = () =>
  within(strip())
    .queryAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
  drawn.marks.length = 0;
  Reflect.deleteProperty(globalThis, GLOBAL);
});

describe("the Chats section", () => {
  it("lists every running chat of the project as a tree of which chat started which", async () => {
    core([
      chat(1, "alpha"),
      chat(2, "alpha", { from: by(1, "task"), label: "drop commons" }),
      chat(3, "alpha", { from: by(2, "task", "drop commons") }),
      chat(4, "beta", { persona: "devops" }),
    ]);
    render(<App />);
    const tree = await section();
    // Every workspace's, which the list shows on the person's word (#1655).
    await userEvent.click(await screen.findByRole("radio", { name: "All" }));

    await waitFor(() =>
      expect(shape(tree)).toEqual(["1 steward 1", "2 drop commons", "3 steward 3", "1 devops 4"]),
    );
    // Each row says who runs it and what it is doing, and its card says where (#1675).
    const first = row(tree, "steward 1");
    expect(first.querySelector('[title="steward"]')?.getAttribute("data-initials")).toBe("ST");
    expect(within(first).getByText("running (no detail from claude)")).toBeTruthy();
    expect((await cardOf(tree, "steward 1")).facts.Workspace).toBe("alpha");
    expect((await cardOf(tree, "devops 4")).facts.Workspace).toBe("beta");
  });
  it("nests a chat that works in another workspace under the chat that asked, and names its workspace", async () => {
    core([chat(1, "alpha"), chat(2, "beta", { persona: "devops", from: by(1, "task") })]);
    render(<App />);
    const tree = await section();

    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2"]));
    expect((await cardOf(tree, "devops 2")).facts.Workspace).toBe("beta");

    expect(screen.queryByRole("list", { name: /started in other workspaces/ })).toBeNull();
  });

  it("draws a chat handed off in the same workspace by its own row, with no badge", async () => {
    core([chat(1, "alpha"), chat(2, "alpha", { from: by(1, "handoff") })]);
    render(<App />);
    const tree = await section();
    // A handoff is a row of its own at the top (#1492).
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "1 steward 2"]));

    expect(screen.queryByRole("list", { name: /started in other workspaces/ })).toBeNull();
  });

  it("leaves a chat whose parent has closed at the top, saying where it came from", async () => {
    // Chat 7 handed off and was closed since; what it started is still running.
    core([
      chat(1, "alpha"),
      chat(8, "alpha", { from: by(7, "handoff", "release notes") }),
      chat(9, "beta", { from: by(8, "task", "steward 8") }),
    ]);
    render(<App />);
    const tree = await section();

    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "1 steward 8", "2 steward 9"]));
    expect((await cardOf(tree, "steward 8")).text).toContain("from release notes");
    // A chat drawn under its parent needs no note: the nesting says it.
    expect((await cardOf(tree, "steward 9")).text).not.toContain("from steward 8");
  });

  it("says a task whose asking chat has closed was asked by that chat, closed", async () => {
    // V100-64 (#1513): a task works on after the chat that asked closed, at the top.
    core([chat(1, "alpha"), chat(8, "alpha", { from: by(7, "task", "steward 7") })]);
    render(<App />);
    const tree = await section();

    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "1 steward 8"]));
    expect((await cardOf(tree, "steward 8")).text).toContain("asked by steward 7 (closed)");
  });

  it("says on the asking chat's row how many of its dispatches wait on memory (#1617)", async () => {
    core([
      chat(1, "alpha", { waiting_on_memory: 1 }),
      chat(2, "alpha", { from: by(1, "task"), label: "drop commons" }),
      chat(4, "beta", { persona: "devops", waiting_on_memory: 3 }),
    ]);
    render(<App />);
    const tree = await section();
    await userEvent.click(await screen.findByRole("radio", { name: "All" }));
    await waitFor(() => expect(shape(tree)).toHaveLength(3));

    const one = await screen.findByTestId("on-memory-1");
    expect(one.textContent).toBe("1 dispatch waits on memory");
    // Where they are named, on the line's hover.
    expect(one.getAttribute("title")).toContain("The Dispatches tab lists them under Not started.");
    expect((await screen.findByTestId("on-memory-4")).textContent).toBe(
      "3 dispatches wait on memory",
    );
    // A chat none of whose dispatches waits says nothing of memory.
    expect(screen.queryByTestId("on-memory-2")).toBeNull();
  });

  it("says a task's asker that waits to start after a launch is not open, not closed", async () => {
    core([
      chat(1, "alpha"),
      chat(8, "alpha", { from: { ...by(7, "task", "steward 7"), asker_waiting: true } }),
    ]);
    render(<App />);
    const tree = await section();

    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "1 steward 8"]));
    expect((await cardOf(tree, "steward 8")).text).toContain("asked by steward 7 (not open)");
  });

  it("marks the chat that needs you, and takes the mark off when it stops asking", async () => {
    const { move } = core([chat(1, "alpha"), chat(2, "alpha")]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(2));

    move(2, "waiting", 10, [2]);

    expect(says(row(tree, "steward 2"))).toEqual({ word: "needs you", shape: "hand" });
    expect(within(row(tree, "steward 1")).queryByText("needs you")).toBeNull();

    move(2, "running", 11, []);
    expect(says(row(tree, "steward 2"))).toEqual({ word: "working", shape: "ring" });
  });
});

/** One asker in alpha and six task chats under it, none of which the person has opened. */
const sixTasks = () => [
  chat(1, "alpha"),
  ...[2, 3, 4, 5, 6, 7].map((session) =>
    chat(session, session % 2 === 0 ? "alpha" : "beta", {
      persona: "devops",
      from: by(1, "task"),
    }),
  ),
];

describe("a task chat", () => {
  it("is listed under its asker and adds no tab: six of them, six rows, one tab", async () => {
    core(sixTasks());
    render(<App />);
    const tree = await section();

    await waitFor(() =>
      expect(shape(tree)).toEqual([
        "1 steward 1",
        "2 devops 2",
        "2 devops 3",
        "2 devops 4",
        "2 devops 5",
        "2 devops 6",
        "2 devops 7",
      ]),
    );
    expect(tabNames()).toEqual(["steward 1"]);
    expect(row(tree, "devops 4").getAttribute("data-tab")).toBe("false");
    expect(row(tree, "steward 1").getAttribute("data-tab")).toBe("true");
  });

  it("says its tokens in its row's card, read once the pointer rests and only for that row (#1500)", async () => {
    const { asked } = core(sixTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(row(tree, "devops 3")).toBeInTheDocument());
    const reads = () => asked.filter((one) => one.cmd === "tasks_used").map((one) => one.args);

    // A pointer passing over a row brings up no card, and reads nothing.
    fireEvent.pointerEnter(row(tree, "devops 4"));
    fireEvent.pointerLeave(row(tree, "devops 4"));
    // One that rests reads that row's figure, for a hover: no time, no total.
    await userEvent.hover(row(tree, "devops 3"));
    await waitFor(() => expect(card()).toHaveTextContent("Tokens: 12k in, 3k out"), {
      timeout: 3000,
    });
    expect(reads()).toEqual([
      { plane: PLANE, scope: "hover", own: null, chats: [3], finished: [] },
    ]);
    await userEvent.unhover(row(tree, "devops 3"));

    await userEvent.hover(row(tree, "devops 5"));
    await waitFor(() => expect(card()).toHaveTextContent("Tokens: — (nothing reported yet)"), {
      timeout: 3000,
    });
    await userEvent.unhover(row(tree, "devops 5"));
    // The session's own row is no task, and says no tokens.
    expect((await cardOf(tree, "steward 1")).text).not.toContain("Tokens");
    expect(reads().map((one) => one.chats)).toEqual([[3], [5]]);
  });

  it("is shown inside its asker's tab when its row is clicked, and no tab is added", async () => {
    const { asked } = core(sixTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(7));

    await userEvent.click(row(tree, "devops 4"));

    await waitFor(() => expect(screen.getByTestId("pane").textContent).toBe("session 4"));
    expect(tabNames()).toEqual(["steward 1"]);
    expect(frontTab()).toEqual(["steward 1"]);
    // No tab of its own was made for it: the core is told of none, and its row still says so.
    expect(asked.filter((one) => one.cmd === "open_chat_tab")).toEqual([]);
    expect(row(tree, "devops 4").getAttribute("data-tab")).toBe("false");

    // A second click changes nothing, and another task takes the first one's place.
    await userEvent.click(row(tree, "devops 4"));
    expect(screen.getByTestId("pane").textContent).toBe("session 4");
    await userEvent.click(row(tree, "devops 6"));
    await waitFor(() => expect(screen.getByTestId("pane").textContent).toBe("session 6"));
    expect(tabNames()).toEqual(["steward 1"]);
  });

  it("is shown in its asker's tab when it works in another workspace, on the asker's strip", async () => {
    core(sixTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(7));

    await userEvent.click(row(tree, "devops 3"));

    // Chat 3 works in beta. It is a task of steward 1 in alpha: the strip stays alpha's.
    await waitFor(() => expect(screen.getByTestId("pane").textContent).toBe("session 3"));
    expect(tabNames()).toEqual(["steward 1"]);
  });

  it("is still listed, and still what its asker's tab shows, after the window is loaded again", async () => {
    // The same core across both windows: a reload, and a relaunch that put the chats back,
    // both ask it what is open and which chat was in front.
    const open = sixTasks();
    const { asked } = core(open);
    const first = render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(7));
    await userEvent.click(row(tree, "devops 4"));
    await waitFor(() => expect(screen.getByTestId("pane").textContent).toBe("session 4"));
    // What the tab shows is the record's, on the session's own entry: the core was told.
    await waitFor(() =>
      expect(asked.filter((one) => one.cmd === "tab_shows").at(-1)?.args).toEqual({
        plane: PLANE,
        session: 1,
        shown: 4,
        beside: null,
      }),
    );
    first.unmount();
    forgetThisLaunch();

    render(<App />);
    const again = await section();

    await waitFor(() =>
      expect(shape(again)).toEqual([
        "1 steward 1",
        "2 devops 2",
        "2 devops 3",
        "2 devops 4",
        "2 devops 5",
        "2 devops 6",
        "2 devops 7",
      ]),
    );
    expect(tabNames()).toEqual(["steward 1"]);
    await waitFor(() => expect(screen.getByTestId("pane").textContent).toBe("session 4"));
    expect(row(again, "devops 2").getAttribute("data-tab")).toBe("false");
  });

  it("comes back as the tab it was, for a task an earlier purlis opened as a tab of its own", async () => {
    // The record says this task has a tab: one was opened for it before tasks lived inside
    // their session's tab, or its session has gone. It is still a tab.
    core([
      chat(1, "alpha"),
      chat(4, "alpha", { persona: "devops", from: { ...by(1, "task"), tab: true } }),
    ]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(2));

    expect(tabNames()).toEqual(["steward 1", "devops 4"]);
    await userEvent.click(row(tree, "devops 4"));
    await waitFor(() => expect(frontTab()).toEqual(["devops 4"]));
    expect(tabNames()).toEqual(["steward 1", "devops 4"]);
  });

  it("arrives as a row and no tab, where a handoff arrives as a tab", async () => {
    const open = [chat(1, "alpha")];
    const { arrive } = core(open);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1"]));

    open.push(chat(2, "alpha", { persona: "devops", from: by(1, "task") }));
    arrive(open[1]);
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2"]));
    expect(tabNames()).toEqual(["steward 1"]);

    open.push(chat(3, "alpha", { persona: "devops", from: by(1, "handoff") }));
    arrive(open[2]);
    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "devops 3"]));
    // And as a row of its own at the top: it is not a task of the chat it came from (#1492).
    expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2", "1 devops 3"]);
    expect((await cardOf(tree, "devops 3")).text).toContain("from steward 1");
  });
});

describe("a chat moving, with fifty chats listed", () => {
  it("redraws that chat's marks and no other row's", async () => {
    const { move } = core(
      Array.from({ length: 50 }, (_, at) =>
        chat(at + 1, at % 2 === 0 ? "alpha" : "beta", at === 0 ? {} : { from: by(1, "task") }),
      ),
    );
    // Not under StrictMode, which draws everything twice: what is counted is draws.
    renderBare(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(50));
    for (let turn = 0; turn < 10; turn += 1) await act(async () => {});
    drawn.marks.length = 0;

    move(31, "running", 10);

    // Chat 31 is a task chat with no tab: its row here draws its state, and forty-nine other
    // rows draw nothing. The explorer draws no chat (#1673).
    expect(drawn.marks).toEqual(["working"]);
    expect(within(row(tree, "steward 31")).getByText("working")).toBeTruthy();
  });
});

/**
 * 1 in alpha asked 2 in alpha for a task, which asked the task chat 3 in beta. 4 is on its own.
 * 2 was given a tab of its own, so it is where 3 opens. A handoff would not be under 1 (#1492).
 */
const threeDeep = () => [
  chat(1, "alpha"),
  chat(2, "alpha", { from: { ...by(1, "task"), tab: true }, label: "drop commons" }),
  chat(3, "beta", { persona: "devops", from: by(2, "task", "drop commons") }),
  chat(4, "alpha"),
];

/** The hand a row wears for a chat below it: a button beside the row, in the row's item. */
const rolledUp = (tree: HTMLElement, name: string) =>
  row(tree, name).closest("li")?.querySelector<HTMLElement>("button.rolled-up") ?? null;

/** The same, where a test is about pressing it. */
const theRolledUp = (tree: HTMLElement, name: string) => {
  const found = rolledUp(tree, name);
  if (found === null) throw new Error(`${name} wears no hand for a chat below it`);
  return found;
};

describe("the needs-you mark rolling up the tree (#1448)", () => {
  it("is on the chat that needs you and on every row above it, and on no other", async () => {
    const { move } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    move(3, "waiting", 10, [3]);

    expect(says(row(tree, "devops 3"))).toEqual({ word: "needs you", shape: "hand" });
    expect(rolledUp(tree, "drop commons")?.getAttribute("aria-label")).toBe(
      "Go to devops 3 below drop commons, which needs you",
    );
    expect(rolledUp(tree, "steward 1")?.getAttribute("aria-label")).toBe(
      "Go to devops 3 below steward 1, which needs you",
    );
    expect(rolledUp(tree, "steward 4")).toBeNull();
    expect(within(row(tree, "steward 4")).queryByText("needs you")).toBeNull();
    // Its own row wears the mark and no button: the row itself goes to it.
    expect(rolledUp(tree, "devops 3")).toBeNull();

    move(3, "running", 11, []);
    expect(rolledUp(tree, "steward 1")).toBeNull();
    expect(rolledUp(tree, "drop commons")).toBeNull();
  });

  it("still shows on a row folded again over a chat two levels down that needs you", async () => {
    const { move } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    expect(row(tree, "steward 1").getAttribute("aria-expanded")).toBe("true");
    expect(row(tree, "steward 4").getAttribute("aria-expanded")).toBeNull();

    // The grandchild asks (its path is opened, #1675), and the person folds its rows away.
    move(3, "waiting", 10, [3]);
    await userEvent.click(within(tree).getByTitle("Fold the chats under steward 1"));
    expect(shape(tree)).toEqual(["1 steward 1", "1 steward 4"]);
    expect(row(tree, "steward 1").getAttribute("aria-expanded")).toBe("false");

    expect(rolledUp(tree, "steward 1")?.getAttribute("data-leads-to")).toBe("3");
  });

  it("goes to the chat that needs you when the rolled-up mark is pressed", async () => {
    const { asked, move } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    await userEvent.click(within(tree).getByTitle("Fold the chats under steward 1"));
    move(3, "waiting", 10, [3]);

    const tabsBefore = tabNames();
    await userEvent.click(theRolledUp(tree, "steward 1"));

    // Chat 3 is a task in beta, asked for by drop commons in alpha: that tab comes forward
    // showing it, and no tab is added on either strip.
    await waitFor(() => expect(screen.getByTestId("pane").textContent).toBe("session 3"));
    expect(tabNames()).toEqual(tabsBefore);
    expect(frontTab()).toEqual(["drop commons"]);
    expect(asked.filter((one) => one.cmd === "open_chat_tab")).toEqual([]);
  });

  it("folds and opens a row from the keyboard", async () => {
    core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    row(tree, "drop commons").focus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(shape(tree)).toEqual(["1 steward 1", "2 drop commons", "1 steward 4"]);
    await userEvent.keyboard("{ArrowRight}");
    expect(shape(tree)).toHaveLength(4);
  });

  it("keeps a fold set by hand when the window is drawn again, as a reload draws it (#1459)", async () => {
    core(threeDeep());
    const first = render(<App />);
    let tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    row(tree, "drop commons").focus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(shape(tree)).toEqual(["1 steward 1", "2 drop commons", "1 steward 4"]);

    // The window again, from nothing: what a reload of the webview draws.
    first.unmount();
    forgetThisLaunch();
    render(<App />);
    tree = await section();

    await waitFor(() =>
      expect(shape(tree)).toEqual(["1 steward 1", "2 drop commons", "1 steward 4"]),
    );
    row(tree, "drop commons").focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(shape(tree)).toHaveLength(4);
  });

  it("reads the sidebar again when a task's turn ends, and not when one begins (#1468)", async () => {
    const { asked, move } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    const reads = () => asked.filter((one) => one.cmd === "plane_sidebar").length;
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    const before = reads();

    // A task's turn begins: no report can have come with it, so nothing is read again.
    move(3, "running", 10);
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(reads()).toBe(before);

    // Its turn ends, which is when a report it made is followed: the sidebar is read again.
    move(3, "waiting", 11);
    await waitFor(() => expect(reads()).toBeGreaterThan(before));
  });

  it("counts a task on the workspace of the tab it lives in, not the one it works in", async () => {
    const { move } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    move(3, "waiting", 10, [3]);

    // Chat 3 works in beta and is a task of drop commons, whose tab is on alpha's strip: that
    // tab wears it, so alpha's workspace tab counts it and beta's, which holds no tab, does not.
    expect(await screen.findByLabelText("1 chats need you in alpha")).toBeTruthy();
    expect(screen.queryByLabelText(/need you in beta/)).toBeNull();
    expect(
      screen.queryByRole("button", { name: /the strip is not showing, where 1 chat needs you$/ }),
    ).toBeNull();
  });

  it("goes to a task from the title bar's list, inside its asker's tab", async () => {
    const { move } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    move(3, "waiting", 10, [3]);

    await userEvent.click(await screen.findByRole("button", { name: "1 chat needs you" }));
    const tabsBefore = tabNames();
    await userEvent.click(await screen.findByRole("menuitem", { name: /^Go to devops 3/ }));

    await waitFor(() => expect(screen.getByTestId("pane").textContent).toBe("session 3"));
    expect(tabNames()).toEqual(tabsBefore);
    expect(frontTab()).toEqual(["drop commons"]);
  });
});

describe("what needs you (#1448)", () => {
  it("draws no item for a report that reached the chat that asked", async () => {
    const { move } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    // The core's word when chat 2 reports to chat 1, which is open: chat 1 has a report to
    // read, and nothing is in the queue.
    move(1, "running", 10, [], [], null, ["drop commons"]);

    expect(screen.queryByRole("button", { name: /needs? you$/ })).toBeNull();
    expect(tree.querySelector('[data-mark="needs-you"]')).toBeNull();
  });

  it("says a report has nowhere to go, on the chat that wrote it", async () => {
    const { move } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    move(3, "running", 10, [3], [], [{ kind: "report_undelivered", asker: "drop commons" }]);

    await userEvent.click(await screen.findByRole("button", { name: "1 chat needs you" }));
    expect(
      await screen.findByRole("menuitem", {
        name: /^Go to devops 3: its report has nowhere to go because drop commons has closed or its program has ended/,
      }),
    ).toBeTruthy();
  });
});

describe("stopping a chat (#1448)", () => {
  const stops = (asked: Asked[]) =>
    asked.filter((one) => one.cmd === "stop_chat").map((one) => one.args);

  it("offers both stops on a row's menu, and asks before it stops anything", async () => {
    const { asked } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    // The session's own row: a task under it is not stopped as a chat is, and has its own two
    // ways to end (#1488), so the stops are a session's.
    fireEvent.contextMenu(row(tree, "steward 1"));
    expect(await screen.findByRole("menuitem", { name: "Stop chat steward 1" })).toBeTruthy();
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Stop chat steward 1 and everything below it" }),
    );

    const question = await screen.findByRole("alertdialog", {
      name: "Stop chat steward 1 and everything below it?",
    });
    expect(question.textContent).toContain("steward 1 and the 2 chats below it end");
    expect(stops(asked)).toEqual([]);

    await userEvent.click(within(question).getByRole("button", { name: "Stop 3 chats" }));

    await waitFor(() => expect(stops(asked)).toEqual([{ plane: PLANE, session: 1, below: true }]));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("stops nothing when the question is cancelled", async () => {
    const { asked } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    fireEvent.contextMenu(row(tree, "steward 1"));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Stop chat steward 1" }));
    const question = await screen.findByRole("alertdialog", { name: "Stop chat steward 1?" });
    // A session reports to nobody: it ends, and that is all the question has to say.
    expect(question.textContent).toContain("steward 1 ends. There is no undo.");
    await userEvent.click(within(question).getByRole("button", { name: "Cancel" }));

    expect(stops(asked)).toEqual([]);
    expect(shape(tree)).toHaveLength(4);
  });

  it("says a chat with nothing below it has nothing below it", async () => {
    core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    fireEvent.contextMenu(row(tree, "steward 4"));
    const below = await screen.findByRole("menuitem", {
      name: "Stop chat steward 4 and everything below it",
    });

    expect(below.getAttribute("aria-disabled")).toBe("true");
  });

  it("offers both stops on the chat's tab menu too", async () => {
    const { asked } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    // The session's own tab: a task's tab is named for its session too (#1489).
    fireEvent.contextMenu(within(strip()).getByRole("tab", { name: /^steward 1/ }));
    expect(
      await screen.findByRole("menuitem", { name: "Stop chat steward 1 and everything below it" }),
    ).toBeTruthy();
    await userEvent.click(screen.getByRole("menuitem", { name: "Stop chat steward 1" }));
    const question = await screen.findByRole("alertdialog", { name: "Stop chat steward 1?" });
    // The person started this one: nobody is waiting on what it would write.
    expect(question.textContent).toContain("steward 1 ends. There is no undo.");
    await userEvent.click(within(question).getByRole("button", { name: "Stop chat" }));

    await waitFor(() => expect(stops(asked)).toEqual([{ plane: PLANE, session: 1, below: false }]));
  });

  it("says a chat is stopping, and takes its row and its tab away when it has ended", async () => {
    const { asked, stop } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    expect(tabNames()).toContain("steward 1");

    stop(1, "stopping");
    expect(row(tree, "steward 1").querySelector(".stopping")?.textContent).toBe("Stopping…");
    // Pressed again, it ends without waiting, and the row says so.
    fireEvent.contextMenu(row(tree, "steward 1"));
    expect(await screen.findByRole("menuitem", { name: "End chat steward 1 now" })).toBeTruthy();
    await userEvent.keyboard("{Escape}");

    stop(1, "stopped");

    // Its tasks go on, and stand at the top now that the chat they hung from is gone.
    await waitFor(() =>
      expect(shape(tree)).toEqual(["1 drop commons", "2 devops 3", "1 steward 4"]),
    );
    expect(tabNames()).not.toContain("steward 1");
    // The core ended it: the window does not end it a second time.
    expect(asked.filter((one) => one.cmd === "close_session")).toEqual([]);
  });
});

describe("how the tree reads to a screen reader (#1448)", () => {
  it("says on a row that a chat below it needs you, and keeps the fold out of the tree", async () => {
    const { move } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    move(3, "waiting", 10, [3]);

    expect(row(tree, "steward 1").getAttribute("aria-description")).toBe(
      "devops 3 below it needs you",
    );
    expect(row(tree, "drop commons").getAttribute("aria-description")).toBe(
      "devops 3 below it needs you",
    );
    // The chat itself wears the mark, whose name says it; no description repeats it.
    expect(row(tree, "devops 3").getAttribute("aria-description")).toBeNull();
    expect(row(tree, "steward 4").getAttribute("aria-description")).toBeNull();
    // The fold is the pointer's: the row says expanded and folds on the arrows, so the button
    // is not a second thing to read.
    const fold = within(tree).getByTitle("Fold the chats under steward 1");
    expect(fold.getAttribute("aria-hidden")).toBe("true");
    expect(fold.getAttribute("tabindex")).toBe("-1");
    expect(within(tree).queryByRole("button", { name: /Fold the chats/ })).toBeNull();
  });
});

describe("what the chat that asked is shown of a stop (#1448)", () => {
  it("says the chat it started was stopped, and not that it reported back", async () => {
    const { move } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    // Chat 1 is waiting on you for its own turn's end, and the chat it started was stopped.
    move(1, "waiting", 10, [1], [], null, [], ["drop commons"]);

    await userEvent.click(await screen.findByRole("button", { name: "1 chat needs you" }));
    const item = await screen.findByRole("menuitem", { name: /^Go to steward 1/ });
    expect(item.getAttribute("aria-label")).toContain("steward 1: drop commons was stopped");
    expect(item.getAttribute("aria-label")).not.toContain("reported back");
  });

  it("offers everything below on a chat that is already stopping, and says what it adds", async () => {
    const { asked, stop } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    stop(1, "stopping");

    fireEvent.contextMenu(row(tree, "steward 1"));
    await userEvent.click(
      await screen.findByRole("menuitem", {
        name: "Stop chat steward 1 and everything below it",
      }),
    );
    const question = await screen.findByRole("alertdialog", {
      name: "Stop chat steward 1 and everything below it?",
    });
    expect(question.textContent).toContain("steward 1 is being stopped already.");
    await userEvent.click(within(question).getByRole("button", { name: "Stop 2 chats" }));

    await waitFor(() =>
      expect(asked.filter((one) => one.cmd === "stop_chat").map((one) => one.args)).toEqual([
        { plane: PLANE, session: 1, below: true },
      ]),
    );
  });
});

describe("stopping all of a session's tasks (#1498)", () => {
  const stopsAll = (asked: Asked[]) =>
    asked.filter((one) => one.cmd === "stop_all_tasks").map((one) => one.args);

  it("sits beside Stop with everything below it, asks once naming the count, and keeps the session", async () => {
    const { asked } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    fireEvent.contextMenu(row(tree, "steward 1"));
    await screen.findByRole("menuitem", { name: "Stop chat steward 1" });
    const names = screen
      .getAllByRole("menuitem")
      .map((one) => one.getAttribute("aria-label") ?? one.textContent);
    const below = names.findIndex((name) =>
      name?.startsWith("Stop chat steward 1 and everything below it"),
    );
    expect(names[below + 1]).toMatch(/^Stop all tasks of steward 1/);
    await userEvent.click(screen.getByRole("menuitem", { name: "Stop all tasks of steward 1" }));

    const question = await screen.findByRole("alertdialog", {
      name: "Stop all 2 tasks of steward 1?",
    });
    expect(question.textContent).toContain(
      "The 2 tasks at work below steward 1 end, deepest first.",
    );
    expect(question.textContent).toContain("steward 1 keeps running.");
    expect(stopsAll(asked)).toEqual([]);

    await userEvent.click(within(question).getByRole("button", { name: "Stop 2 tasks" }));

    // The tasks the question named, and no more; the session is not stopped.
    await waitFor(() =>
      expect(stopsAll(asked)).toEqual([{ plane: PLANE, session: 1, tasks: [3, 2] }]),
    );
    expect(asked.some((one) => one.cmd === "stop_chat")).toBe(false);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("stops nothing when the question is cancelled", async () => {
    const { asked } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    fireEvent.contextMenu(row(tree, "steward 1"));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Stop all tasks of steward 1" }),
    );
    const question = await screen.findByRole("alertdialog", {
      name: "Stop all 2 tasks of steward 1?",
    });
    await userEvent.click(within(question).getByRole("button", { name: "Cancel" }));

    expect(stopsAll(asked)).toEqual([]);
  });
});

/** The operator's five: one session in alpha and five tasks of it, two named by a label. */
const fiveTasks = () => [
  chat(1, "alpha"),
  chat(15, "alpha", { persona: "devops", label: "live check talk", from: by(1, "task") }),
  chat(16, "alpha", { persona: "devops", label: "live check queue", from: by(1, "task") }),
  chat(17, "alpha", { persona: "devops", from: by(1, "task") }),
  chat(18, "beta", { persona: "devops", from: by(1, "task") }),
  chat(19, "beta", { persona: "devops", from: by(1, "task") }),
];

describe("a chat's helpers (#1673)", () => {
  it("says in its row's card what it runs on, which the explorer's row said before", async () => {
    core([chat(1, "alpha", { profile: "work" }), chat(2, "alpha", { from: by(1, "handoff") })]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(2));

    expect((await cardOf(tree, "steward 1")).facts.Harness).toBe("work (claude)");
    expect((await cardOf(tree, "steward 2")).facts.Harness).toBe("claude");
  });

  it("are said in its row's card in the Chats list, which is the one list of chats", async () => {
    const { move } = core([chat(1, "alpha"), chat(2, "alpha", { from: by(1, "handoff") })]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(2));

    move(1, "running", 10, [], [{ agent: "thread-7", state: "running" }]);

    await userEvent.hover(row(tree, "steward 1"));
    const shown = await theCard();
    await waitFor(() => expect(shown).toHaveTextContent("1 helper · 1 working"));
    await userEvent.unhover(row(tree, "steward 1"));
    await waitFor(() => expect(card()).toBeNull());
    expect((await cardOf(tree, "steward 2")).text).not.toContain("helper");
  });
});

describe("a task's helpers (#1490)", () => {
  it("says a task's helpers on its own row", async () => {
    const { move } = core(fiveTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(6));

    move(
      15,
      "running",
      10,
      [],
      [
        { agent: "thread-1", state: "running" },
        { agent: "thread-2", state: "done" },
        { agent: "thread-3", state: "done" },
      ],
    );

    // In its card, and only in the card of a task that has some (#1675).
    expect((await cardOf(tree, "live check talk")).text).toContain("3 helpers · 1 working");
    expect((await cardOf(tree, "live check queue")).text).not.toContain("helper");
  });
});

/** A task of chat 1 as its record stands: how it reported, or whom it is asking. */
const taskOf = (more: Partial<Lineage>): Lineage => ({ ...by(1, "task"), ...more });

/**
 * The same task with a tab of its own, as one the person gave a tab has.
 */
const tabbed = (more: Partial<Lineage> = {}): Lineage => ({ ...taskOf(more), tab: true });

describe("a chat's state, as a word and a shape (#1484)", () => {
  it("tells a finished task from a chat waiting on the person by word and by shape", async () => {
    const { move } = core([
      chat(1, "alpha"),
      chat(2, "alpha", { label: "talk", from: tabbed({ reported: true, outcome: "done" }) }),
      chat(3, "alpha", { label: "sweep", from: tabbed({ reported: true, outcome: "cancelled" }) }),
      chat(4, "alpha", { label: "probe", from: tabbed({ reported: true, outcome: "failed" }) }),
      chat(5, "alpha", { label: "lost", from: tabbed({ unreported: true, outcome: "failed" }) }),
      // Stopped by the person before it reported: purlis's own report, the record `stopped`.
      chat(6, "alpha", {
        label: "halt",
        from: tabbed({ unreported: true, outcome: "closed_by_person" }),
      }),
    ]);
    render(<App />);
    const tree = await section();
    // Not every task under it came out done or cancelled, so the session stays open by
    // itself (#1499): a failure is not folded away.
    await waitFor(() => expect(shape(tree)).toHaveLength(6));

    // Every one of them has ended its turn: the board says the same of them all, and the
    // core keeps a task whose report was delivered out of the queue.
    for (const session of [1, 2, 3, 4]) move(session, "waiting", 10 + session, [1]);
    move(5, "failed", 20, [1]);
    move(6, "failed", 21, [1]);

    const expected = [
      ["steward 1", { word: "needs you", shape: "hand" }],
      ["talk", { word: "done", shape: "tick" }],
      ["sweep", { word: "cancelled", shape: "dash" }],
      ["probe", { word: "failed", shape: "cross" }],
      ["lost", { word: "ended without a report", shape: "triangle" }],
      ["halt", { word: "closed by you", shape: "octagon" }],
    ] as const;
    for (const [name, state] of expected) {
      expect(says(row(tree, name)), `${name} in the Chats list`).toEqual(state);
    }
    // The word is the text, read once: the mark beside it is decoration.
    expect(within(row(tree, "talk")).getByText("done")).toBeTruthy();
    expect(within(row(tree, "talk")).queryByRole("img", { name: "done" })).toBeNull();
  });

  it("names a task in the Chats list by its name, never by its number", async () => {
    core([
      chat(1, "alpha"),
      chat(17, "alpha", { persona: "devops", label: "live check talk", from: tabbed() }),
      chat(18, "alpha", { persona: "devops", from: tabbed() }),
      chat(19, "alpha", { persona: "devops", from: by(1, "task") }),
    ]);
    render(<App />);
    const tree = await section();

    await waitFor(() =>
      expect(shape(tree)).toEqual([
        "1 steward 1",
        "2 live check talk",
        "2 devops 18",
        "2 devops 19",
      ]),
    );
  });

  it("names a task with no tab by its name where the title bar says it cannot tell purlis it is waiting", async () => {
    core([
      chat(1, "alpha"),
      chat(17, "alpha", {
        persona: "devops",
        harness: "codex",
        label: "live check talk",
        unreported: "Codex never says when it stops mid-turn for your approval.",
        from: by(1, "task"),
      }),
    ]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(2));

    expect(
      await screen.findByRole("button", {
        name: "Nothing has asked for you, but live check talk can't tell purlis it's waiting",
      }),
    ).toBeTruthy();
  });

  it("says a task is asking the chat that dispatched it once the core says so, and stops when it is answered", async () => {
    const { move, rowsChange } = core([
      chat(1, "alpha"),
      chat(2, "alpha", { label: "talk", from: tabbed() }),
    ]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(2));
    move(2, "running", 10);
    const working = { word: "working", shape: "ring" };
    expect(says(row(tree, "talk"))).toEqual(working);

    // It asks from inside a running command: its board state does not move, and nothing but
    // the core's word that the rows changed has the window read them again.
    await rowsChange(2, (from) => ({ ...from, asking: true }));

    const asking = { word: "asking steward 1", shape: "question" };
    await waitFor(() => expect(says(row(tree, "talk"))).toEqual(asking));

    // Answered, and it goes on inside the same turn.
    await rowsChange(2, (from) => ({ ...from, asking: null }));

    await waitFor(() => expect(says(row(tree, "talk"))).toEqual(working));
  });

  it("says done, never ended without a report, of a task whose program ends right after its report", async () => {
    const { move, rowsChange } = core([
      chat(1, "alpha"),
      chat(2, "alpha", { label: "talk", from: tabbed() }),
    ]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(2));
    move(2, "running", 10);

    // The report lands, and the core says the rows changed, before the program is ended.
    await rowsChange(2, (from) => ({ ...from, reported: true, outcome: "done" }));
    await waitFor(() => expect(says(row(tree, "talk")).word).toBe("working"));
    move(2, "done", 11);
    // Its one task is over, so the session folded by itself (#1499): opened by hand.
    fireEvent.click(within(tree).getByTitle("Show the chats under steward 1"));

    const done = { word: "done", shape: "tick" };
    expect(says(row(tree, "talk"))).toEqual(done);
  });

  it("says needs you of a reported task the person has run again, as the title bar's list does", async () => {
    const { move } = core([
      chat(1, "alpha"),
      chat(2, "alpha", { label: "talk", from: tabbed({ reported: true, outcome: "done" }) }),
    ]);
    render(<App />);
    const tree = await section();
    // Its one task is over, so the session folded by itself (#1499): opened by hand.
    fireEvent.click(await within(tree).findByTitle("Show the chats under steward 1"));
    await waitFor(() => expect(shape(tree)).toHaveLength(2));
    const both = async (state: { word: string; shape: string }) => {
      expect(says(row(tree, "talk"))).toEqual(state);
    };

    // Its turn ended with the report: at rest, out of the queue.
    move(2, "waiting", 10, []);
    await both({ word: "done", shape: "tick" });
    expect(screen.queryByRole("button", { name: "1 chat needs you" })).toBeNull();

    // The person types in it: a new turn.
    move(2, "running", 11, []);
    await both({ word: "working", shape: "ring" });

    // That turn ends, and the core queues it: every surface says the person has the move.
    move(2, "waiting", 12, [2]);
    await both({ word: "needs you", shape: "hand" });
    await userEvent.click(await screen.findByRole("button", { name: "1 chat needs you" }));
    expect(await screen.findByRole("menuitem", { name: /^Go to talk/ })).toBeTruthy();
    // And the row above wears the hand that leads to it, which now points at a row that
    // wears one.
    expect(rolledUp(tree, "steward 1")?.getAttribute("aria-label")).toBe(
      "Go to talk below steward 1, which needs you",
    );
  });

  it("names the chat a task is asking as that chat's own row names it", async () => {
    core([
      chat(1, "alpha", { label: "the release" }),
      // Dispatched while that chat was still called steward 1.
      chat(2, "alpha", { label: "talk", from: tabbed({ asking: true }) }),
    ]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toEqual(["1 the release", "2 talk"]));

    expect(says(row(tree, "talk")).word).toBe("asking the release");
  });

  it("says what is not known of a chat whose harness sends nothing, and guesses nothing", async () => {
    core([
      chat(1, "alpha", {
        harness: "opencode",
        card: { name: "opencode", title: "opencode", label: "", lines: [], cannot_type: null },
      }),
    ]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(1));

    const unheard = { word: "running (no detail from opencode)", shape: "dots" };
    expect(says(row(tree, "steward 1"))).toEqual(unheard);
  });

  it("says working in both lists once the chat's harness is heard", async () => {
    const { move } = core([chat(1, "alpha")]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(1));

    move(1, "running", 10);

    const working = { word: "working", shape: "ring" };
    expect(says(row(tree, "steward 1"))).toEqual(working);
  });
});

/** Focuses a workspace from the strip, which is the axis, once the strip lists it. */
async function focusWorkspace(name: string) {
  const tab = await waitFor(() => {
    const found = within(stripNamed("Workspaces"))
      .getAllByRole("tab")
      .find((one) => one.querySelector(".workspace-name")?.textContent === name);
    if (found === undefined) throw new Error(`no ${name} on the workspace strip`);
    return found;
  });
  await userEvent.click(tab);
}

describe("the Chats section follows the focused workspace", () => {
  it("lists the focused workspace's chats, and another's once that one is focused", async () => {
    core([
      chat(1, "alpha"),
      // Asked for by alpha's chat and working in beta: it stays under the chat that asked.
      chat(2, "beta", { persona: "devops", from: by(1, "task") }),
      chat(3, "beta", { persona: "devops", in_front: false }),
    ]);
    render(<App />);
    const tree = await section();

    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2"]));

    await focusWorkspace("beta");
    await waitFor(() => expect(shape(tree)).toEqual(["1 devops 3"]));
  });

  it("lists every workspace's chats while All is picked, until Workspace is picked again", async () => {
    core([chat(1, "alpha"), chat(3, "beta", { persona: "devops" })]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1"]));

    await userEvent.click(screen.getByRole("radio", { name: "All" }));
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "1 devops 3"]));

    await userEvent.click(screen.getByRole("radio", { name: "Workspace" }));
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1"]));
  });

  it("says a chat in another workspace needs you, with a way to it, and never hides it silently", async () => {
    const { move } = core([chat(1, "alpha"), chat(3, "beta", { persona: "devops" })]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1"]));

    move(3, "waiting", 10, [3]);

    await screen.findByText("devops 3 needs you, in another workspace.");
    await userEvent.click(
      screen.getByRole("button", { name: "Go to devops 3, which needs you in another workspace" }),
    );
    // Its tab came forward, which focused its workspace: the list follows.
    await waitFor(() => expect(shape(tree)).toEqual(["1 devops 3"]));
    expect(screen.queryByText("devops 3 needs you, in another workspace.")).toBeNull();
  });
});

/** The scope switch (#1679), and the scope it is on. */
const scopeSwitch = () => screen.getByRole("radiogroup", { name: "Show the chats of" });
const scopeOn = () =>
  within(scopeSwitch())
    .getAllByRole("radio")
    .filter((one) => one.getAttribute("aria-checked") === "true")
    .map((one) => one.textContent);
const pickScope = (name: "This tab" | "Workspace" | "All") =>
  userEvent.click(within(scopeSwitch()).getByRole("radio", { name }));

describe("the Chats view's scope: this tab, the workspace, or all (#1679)", () => {
  /** Chat 1's tab in front, with its task working in beta; chat 3 in a tab of its own in
   *  alpha; chat 4 in beta. */
  const three = () => [
    chat(1, "alpha"),
    chat(2, "beta", { persona: "devops", from: by(1, "task") }),
    chat(3, "alpha", { in_front: false }),
    chat(4, "beta", { persona: "devops", in_front: false }),
  ];

  it("lists the focused workspace's chats until another scope is picked", async () => {
    core(three());
    render(<App />);
    const tree = await section();

    expect(scopeOn()).toEqual(["Workspace"]);
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2", "1 steward 3"]));
  });

  it("lists the chats of the tab in front, and follows the tab", async () => {
    core(three());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(3));

    await pickScope("This tab");
    expect(scopeOn()).toEqual(["This tab"]);
    // The session and the task it asked for, wherever that task works.
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2"]));

    await userEvent.click(within(strip()).getByRole("tab", { name: /steward 3/ }));
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 3"]));
  });

  it("lists every workspace's chats on All", async () => {
    core(three());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(3));

    await pickScope("All");
    await waitFor(() =>
      expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2", "1 steward 3", "1 devops 4"]),
    );
    await pickScope("Workspace");
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2", "1 steward 3"]));
  });

  it("remembers the scope picked for the project when the window opens it again", async () => {
    const { asked } = core(three());
    const { unmount } = render(<App />);
    await section();
    await pickScope("All");
    // Kept in the project's entry in layout.json v2 (B-11, #1696).
    const writes = () => asked.filter((one) => one.cmd === "write_layout");
    await waitFor(() => expect(writes().length).toBeGreaterThan(0));
    const document: unknown = JSON.parse(String(writes().at(-1)?.args.text));
    expect(document).toMatchObject({ projects: { [PLANE]: { chats: { scope: "all" } } } });
    unmount();
    forgetThisLaunch();
    // The next launch is handed the file the window wrote.
    (globalThis as Record<string, unknown>)[GLOBAL] = {
      layout: { path: "layout.json", found: true, document, trouble: null },
      theme: { path: "theme.json", found: false, document: null, trouble: null },
    };

    render(<App />);
    const tree = await section();
    expect(scopeOn()).toEqual(["All"]);
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
  });

  it("picks the next scope on a single arrow", async () => {
    // One Tab stop, as `Window.keyboard.test.tsx` walks it.
    core(three());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(3));
    const radios = within(scopeSwitch()).getAllByRole("radio");

    act(() => radios[1].focus());
    await userEvent.keyboard("{ArrowLeft}");

    await waitFor(() => expect(scopeOn()).toEqual(["This tab"]));
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2"]));
  });

  it("names a chat outside this tab that needs you at the top, with a way to it", async () => {
    const { move } = core(three());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(3));
    await pickScope("This tab");
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2"]));

    move(3, "waiting", 10, [3]);

    await screen.findByText("steward 3 needs you, outside this tab.");
    await userEvent.click(
      screen.getByRole("button", { name: "Go to steward 3, which needs you outside this tab" }),
    );
    // Its tab came forward: the list is that tab's, and nothing is hidden that needs you.
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 3"]));
    expect(screen.queryByText(/needs you, outside this tab/)).toBeNull();
  });

  it("stays on This tab when going to a task brings its tab forward, the tab read once it is in front (#1696)", async () => {
    const { move } = core([
      ...three(),
      chat(5, "alpha", { persona: "devops", from: by(3, "task") }),
    ]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    await pickScope("This tab");
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2"]));

    move(5, "waiting", 10, [5]);
    await screen.findByText("devops 5 needs you, outside this tab.");
    await userEvent.click(
      screen.getByRole("button", { name: "Go to devops 5, which needs you outside this tab" }),
    );

    // Its asker's tab came forward with it: This tab lists it, and is not widened.
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 3", "2 devops 5"]));
    expect(scopeOn()).toEqual(["This tab"]);
  });

  it("names a chat in another workspace that needs you, by its name", async () => {
    const { move } = core(three());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(3));

    move(4, "waiting", 10, [4]);
    move(2, "waiting", 11, [4, 2]);

    // Chat 2 is listed under the chat that asked it, so only chat 4 is hidden.
    await screen.findByText("devops 4 needs you, in another workspace.");
    expect(
      screen.getByRole("button", { name: "Go to devops 4, which needs you in another workspace" }),
    ).toBeTruthy();
  });

  it("names both what the filter hides and what the scope leaves out, Go going to the first", async () => {
    const { move } = core(three());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(3));
    move(4, "waiting", 10, [4]);
    move(3, "waiting", 11, [4, 3]);

    act(() =>
      screen
        .getByRole("searchbox", { name: "Filter chats by name, persona, workspace or state" })
        .focus(),
    );
    await userEvent.keyboard("devops");

    await waitFor(() =>
      expect(document.querySelector(".chats-hidden")?.textContent).toBe(
        "steward 3 needs you, and the filter hides it. The filter hides 1 of 3 chats." +
          " devops 4 needs you, in another workspace.",
      ),
    );
    expect(
      screen.getByRole("button", { name: "Go to steward 3, which needs you and the filter hides" }),
    ).toBeTruthy();
  });

  it("names on All only what the filter hides", async () => {
    const { move } = core(three());
    render(<App />);
    const tree = await section();
    await pickScope("All");
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    move(4, "waiting", 10, [4]);

    // Typed with the keyboard brought to the box: jsdom lays nothing out for a pointer.
    act(() =>
      screen
        .getByRole("searchbox", { name: "Filter chats by name, persona, workspace or state" })
        .focus(),
    );
    await userEvent.keyboard("steward");

    await screen.findByText(/devops 4 needs you, and the filter hides it\./);
    expect(
      screen.getByRole("button", { name: "Go to devops 4, which needs you and the filter hides" }),
    ).toBeTruthy();
  });
});

describe("one tree style, in the Chats tree and the explorer (#1672)", () => {
  it("draws a level of either as one step in and one straight guide", async () => {
    core([
      chat(1, "alpha"),
      chat(2, "alpha", { from: by(1, "task"), label: "drop commons" }),
      chat(3, "alpha", { from: by(2, "task", "drop commons") }),
    ]);
    render(<App />);
    const tree = await section();
    await waitFor(() =>
      expect(shape(tree)).toEqual(["1 steward 1", "2 drop commons", "3 steward 3"]),
    );
    // The Explorer view is mounted beside Chats and hidden while Chats is open (#1673).
    const explorer = await screen.findByRole("tree", { name: "Repos and branches", hidden: true });
    expect(tree).toHaveClass("tree");
    expect(explorer).toHaveClass("tree");

    // The Chats tree is flat: each row draws a guide per level above it.
    const item = (name: string) => row(tree, name).closest("li") as HTMLElement;
    expect(drawnWith(item("steward 1"), "--tree-depth")).toBe("0");
    expect(drawnWith(item("drop commons"), "--tree-depth")).toBe("1");
    expect(drawnWith(item("steward 3"), "--tree-depth")).toBe("2");
    expect(drawnWith(item("steward 3"), "background-image")).toContain("var(--tree-guide)");
    expect(drawnWith(item("steward 3"), "padding-inline-start")).toBe(
      "calc(var(--tree-depth) * var(--tree-indent))",
    );

    // The explorer is nested: a level is a group, and the group's edge is the guide. Its levels
    // were this fixture's chats until the explorer stopped drawing chats (#1673), and a chat
    // fixture has no clones, so the nested guide is held where there are levels to draw:
    // `Explorer.test.tsx` (the rules) and `editor/BranchTree.test.tsx` (a drawn tree).
  });
});

describe("the chat in front, as the Chats tree draws it (#1672)", () => {
  it("is drawn selected, and a row under the pointer is drawn differently", async () => {
    core([chat(1, "alpha"), chat(2, "alpha", { from: by(1, "handoff") })]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "1 steward 2"]));
    expect(tree).toHaveClass("tree");

    const front = row(tree, "steward 1");
    const other = row(tree, "steward 2");
    expect(front.getAttribute("aria-current")).toBe("true");
    // Selected is its own fill and an edge; under the pointer is the window's hover.
    expect(drawnWith(front, "background")).toBe("var(--list-selected)");
    expect(drawnWith(front, "box-shadow")).toContain("var(--list-selected-edge)");
    expect(drawnWith(other, "background", { hover: true })).toBe("var(--surface-hover)");
    expect(drawnWith(other, "box-shadow", { hover: true })).toBeUndefined();
    // And the pointer over the selected row does not take its selection away.
    expect(drawnWith(front, "background", { hover: true })).toBe("var(--list-selected)");

    // Another chat brought forward is the one drawn selected.
    await userEvent.click(other);
    await waitFor(() => expect(frontTab()).toEqual(["steward 2"]));
    await waitFor(() => expect(drawnWith(other, "background")).toBe("var(--list-selected)"));
    expect(drawnWith(front, "background")).not.toBe("var(--list-selected)");
  });

  it("rings the row the keyboard is on, which is not how it says selected", async () => {
    core([chat(1, "alpha"), chat(2, "alpha", { from: by(1, "handoff") })]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(2));
    const front = row(tree, "steward 1");
    const other = row(tree, "steward 2");

    // The keyboard on a row that is not selected: a ring, and no selection's fill or edge.
    expect(drawnWith(other, "outline", { focusVisible: true })).toBe("2px solid var(--focus-ring)");
    expect(drawnWith(other, "background", { focusVisible: true })).not.toBe("var(--list-selected)");
    // Selected and not focused: the fill and the edge, and no ring.
    expect(drawnWith(front, "outline")).toBeUndefined();
    // Both at once: each is still there.
    expect(drawnWith(front, "outline", { focusVisible: true })).toBe("2px solid var(--focus-ring)");
    expect(drawnWith(front, "background", { focusVisible: true })).toBe("var(--list-selected)");
  });
});

/** One session in alpha, with three tasks: one in beta on a branch of its own. */
const threeTasks = () => [
  chat(1, "alpha"),
  chat(2, "alpha", { persona: "devops", from: by(1, "task") }),
  chat(3, "beta", {
    persona: "devops",
    from: by(1, "task"),
    cwd: `${PLANE}/workspaces/beta/.worktrees/api/fix-the-login`,
  }),
  chat(4, "alpha", { persona: "devops", from: by(1, "task") }),
];

describe("a chat's row is one line, with a hover card (#1675)", () => {
  it("draws its state's mark, its persona's badge and its name, and nothing more", async () => {
    const { move } = core(threeTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    for (const session of [1, 2, 3, 4]) move(session, "running", 10 + session);

    for (const name of ["steward 1", "devops 3"]) {
      const one = row(tree, name);
      // One line, and no second line: the badge, the name and the state, which is drawn
      // first and heard after the name.
      expect(one.querySelector(".line.two")).toBeNull();
      const parts = [...(one.querySelector(".line.one")?.children ?? [])];
      expect(parts.map((part) => part.classList[0])).toEqual([
        "persona-mark",
        "session",
        "shown-state",
      ]);
      expect(drawnWith(parts[2], "order")).toBe("-1");
      expect(one).toHaveAccessibleName(`${name} working`);
      // The state is its mark on the row; its word is out of sight.
      const word = one.querySelector(".shown-state .word") as HTMLElement;
      expect(word.textContent).toBe("working");
      expect(drawnWith(word, "clip-path")).toBe("inset(50%)");
    }
    // An open row's tasks are its rows below it: no count, no workspace, no branch beside it.
    expect(row(tree, "steward 1").querySelector(".task-count")).toBeNull();
    expect(row(tree, "devops 3").textContent).not.toContain("beta");
    expect(row(tree, "devops 3").textContent).not.toContain("fix-the-login");
    // And the details are no native tooltip.
    expect(row(tree, "devops 3").getAttribute("title")).toBeNull();
  });

  it("shows its details in a card under a resting pointer, and takes it down when it leaves", async () => {
    const { move } = core(threeTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    move(3, "running", 10);

    await userEvent.hover(row(tree, "devops 3"));
    const shown = await theCard();

    expect(shown).toHaveTextContent("devops 3");
    expect(shown).toHaveTextContent("working");
    expect(facts(shown)).toMatchObject({
      Persona: "devops",
      Harness: "claude",
      Workspace: "beta",
      Branch: "fix-the-login",
    });
    // The tokens are read as the card comes up, for this task alone (#1500).
    await waitFor(() => expect(card()).toHaveTextContent("Tokens: 12k in, 3k out"));

    await userEvent.unhover(row(tree, "devops 3"));
    await waitFor(() => expect(card()).toBeNull());
  });

  it("keeps the card up while the pointer crosses to it, and on it (WCAG 1.4.13)", async () => {
    core(threeTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    await userEvent.hover(row(tree, "devops 3"));
    const shown = await theCard();
    // Off the row and onto the card, as a pointer reading it goes.
    await userEvent.unhover(row(tree, "devops 3"));
    await userEvent.hover(shown);
    await new Promise((done) => setTimeout(done, CARD_LEAVE_MS * 2));
    expect(card()).toBe(shown);

    await userEvent.unhover(shown);
    await waitFor(() => expect(card()).toBeNull());
  });

  it("says a session's tasks in its card", async () => {
    const { move } = core(threeTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    for (const session of [2, 3, 4]) move(session, "running", 10 + session);

    await userEvent.hover(row(tree, "steward 1"));
    expect(facts(await theCard()).Tasks).toBe("3 working");
  });

  it("opens the card from the keyboard, once the keyboard rests on the row, and Escape closes it", async () => {
    core(threeTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    act(() => row(tree, "devops 2").focus());
    const shown = await theCard();
    expect(shown).toHaveTextContent("devops 2");
    // What the card says is what the row is described by while it is up.
    expect(row(tree, "devops 2").getAttribute("aria-describedby")).toBe(shown.id);

    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(card()).toBeNull());
  });

  it("brings up no card for a press that focused the row, and does once the keyboard is used", async () => {
    core(threeTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    await userEvent.click(row(tree, "devops 2"));
    await act(() => new Promise((done) => setTimeout(done, 1200)));
    expect(card()).toBeNull();

    // The keyboard used, then a row focused: that is a rest of the keyboard.
    await userEvent.keyboard("{Shift}");
    act(() => row(tree, "devops 3").focus());
    expect(facts(await theCard())).toMatchObject({ Workspace: "beta" });
  });

  it("has one card up at a time: the keyboard's goes when the pointer brings up another (#1687)", async () => {
    core(threeTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    // The keyboard rests on one row, and its card comes up.
    await userEvent.keyboard("{Shift}");
    act(() => row(tree, "devops 2").focus());
    expect(await theCard()).toHaveTextContent("devops 2");

    // The pointer rests on another: its card, and that one alone.
    await userEvent.hover(row(tree, "devops 3"));
    await waitFor(() => expect(card()).toHaveTextContent("devops 3"));
    expect(screen.getAllByRole("tooltip")).toHaveLength(1);
    expect(row(tree, "devops 2").getAttribute("aria-describedby")).toBeNull();
  });

  it("brings the next row's card up at once while one is up, as an editor's hovers do (#1687)", async () => {
    core(threeTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    await userEvent.hover(row(tree, "devops 2"));
    expect(await theCard()).toHaveTextContent("devops 2");

    // From one row to the next: no second wait.
    await userEvent.unhover(row(tree, "devops 2"));
    await userEvent.hover(row(tree, "devops 3"));
    await new Promise((done) => setTimeout(done, CARD_DELAY_MS / 5));
    expect(card()).toHaveTextContent("devops 3");
    expect(screen.getAllByRole("tooltip")).toHaveLength(1);

    // Once every card has been down a while, a row is rested on again before its card comes.
    await userEvent.unhover(row(tree, "devops 3"));
    await waitFor(() => expect(card()).toBeNull());
    await new Promise((done) => setTimeout(done, CARD_SKIP_MS * 2));
    await userEvent.hover(row(tree, "devops 4"));
    await new Promise((done) => setTimeout(done, CARD_DELAY_MS / 5));
    expect(card()).toBeNull();
    expect(await theCard()).toHaveTextContent("devops 4");
  });

  it("still waits for the keyboard while a card is up: only the pointer skips the rest (#1687)", async () => {
    core(threeTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    await userEvent.hover(row(tree, "devops 2"));
    expect(await theCard()).toHaveTextContent("devops 2");

    // The keyboard comes to another row while that card is up: no card at once for it.
    await userEvent.keyboard("{Shift}");
    act(() => row(tree, "devops 3").focus());
    await new Promise((done) => setTimeout(done, CARD_DELAY_MS / 5));
    expect(card()?.textContent ?? "").not.toContain("devops 3");

    // It still comes, after the whole rest, and alone.
    await waitFor(() => expect(card()).toHaveTextContent("devops 3"));
    expect(screen.getAllByRole("tooltip")).toHaveLength(1);
  });

  it("shows a folded chat's tasks as a small count, and an open one's as its rows with guides", async () => {
    const { move } = core(threeTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    for (const session of [2, 3, 4]) move(session, "running", 10 + session);

    // Open: the tasks are rows a level in, each under the session's guide.
    expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2", "2 devops 3", "2 devops 4"]);
    const item = row(tree, "devops 3").closest("li") as HTMLElement;
    expect(drawnWith(item, "background-image")).toContain("var(--tree-guide)");
    expect(row(tree, "steward 1").querySelector(".task-count")).toBeNull();

    await userEvent.click(within(tree).getByTitle("Fold the chats under steward 1"));

    expect(shape(tree)).toEqual(["1 steward 1"]);
    const count = row(tree, "steward 1").querySelector(".task-count");
    expect(count?.querySelector('[aria-hidden="true"]')?.textContent).toBe("3");
    // Heard as what it counts.
    expect(count).toHaveTextContent("3 tasks");
  });

  it("opens the rows above a task that needed you before its row was listed", async () => {
    const open = [chat(1, "alpha"), chat(2, "alpha", { persona: "devops", from: by(1, "task") })];
    const { arrive, move } = core(open);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(2));
    await userEvent.click(within(tree).getByTitle("Fold the chats under steward 1"));
    expect(shape(tree)).toEqual(["1 steward 1"]);
    // The pointer leaves the list, which holds no arrival back then (V100-47).
    fireEvent.pointerLeave(tree.closest(".chats-list") as HTMLElement);

    // The board says a task needs you before the list has a row for it.
    move(3, "waiting", 10, [3]);
    open.push(chat(3, "alpha", { persona: "devops", from: by(1, "task") }));
    arrive(open[2]);

    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2", "2 devops 3"]));
  });

  it("opens the rows above a task that comes to need you, under a fold set by hand", async () => {
    const { move } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    await userEvent.click(within(tree).getByTitle("Fold the chats under steward 1"));
    expect(shape(tree)).toEqual(["1 steward 1", "1 steward 4"]);

    move(3, "waiting", 10, [3]);

    await waitFor(() =>
      expect(shape(tree)).toEqual(["1 steward 1", "2 drop commons", "3 devops 3", "1 steward 4"]),
    );
    // Opened once, for the asking: a fold set again afterwards is the person's, and holds.
    await userEvent.click(within(tree).getByTitle("Fold the chats under steward 1"));
    expect(shape(tree)).toEqual(["1 steward 1", "1 steward 4"]);
    move(3, "waiting", 11, [3]);
    expect(shape(tree)).toEqual(["1 steward 1", "1 steward 4"]);
    expect(rolledUp(tree, "steward 1")?.getAttribute("data-leads-to")).toBe("3");
  });
});
