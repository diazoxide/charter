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
import type { Moved, OpenChat } from "./bindings";
import type { State } from "./chatState";
import { forgetThisLaunch } from "./regions";
import type { Shown } from "./shownState";

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
  within(screen.getByRole("tablist", { name: "Tabs" }))
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

/**
 * The core, holding `open` chats in two workspaces. It answers `open_chat_tab` the way the
 * record does: the chat has a tab from then on, so a second window on it draws one.
 */
function core(open: (OpenChat & { workspace: string })[]) {
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
    if (cmd === "open_chat_tab") {
      const one = open.find((chat) => chat.session === a.session);
      if (one?.from) one.from = { ...one.from, tab: a.opened !== false };
      return null;
    }
    if (cmd === "tab_shows") {
      // The record keeps what a session's tab shows on that session's own entry (#1486).
      const one = open.find((chat) => chat.session === a.session);
      if (one) one.shows = (a.shown as number | null) ?? null;
      return null;
    }
    if (cmd === "stopping_chats") return [];
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

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
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
});

describe("the Chats section", () => {
  it("lists every running chat of the project as a tree of which chat started which", async () => {
    core([
      chat(1, "alpha"),
      chat(2, "alpha", { from: by(1, "handoff"), label: "drop commons" }),
      chat(3, "alpha", { from: by(2, "handoff", "drop commons") }),
      chat(4, "beta", { persona: "devops" }),
    ]);
    render(<App />);
    const tree = await section();

    await waitFor(() =>
      expect(shape(tree)).toEqual(["1 steward 1", "2 drop commons", "3 steward 3", "1 devops 4"]),
    );
    // Each row says who runs it, where, and what it is doing.
    const first = row(tree, "steward 1");
    expect(first.querySelector('[title="steward"]')?.getAttribute("data-initials")).toBe("ST");
    expect(first.querySelector(".workspace")?.textContent).toBe("alpha");
    expect(within(first).getByText("running (no detail from claude)")).toBeTruthy();
    expect(row(tree, "devops 4").querySelector(".workspace")?.textContent).toBe("beta");
  });
  it("nests a chat that works in another workspace under the chat that asked, and names its workspace", async () => {
    core([chat(1, "alpha"), chat(2, "beta", { persona: "devops", from: by(1, "handoff") })]);
    render(<App />);
    const tree = await section();

    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2"]));
    expect(row(tree, "devops 2").querySelector(".workspace")?.textContent).toBe("beta");

    // And under its asker's row in alpha's explorer, wearing the workspace it went to.
    const started = await screen.findByRole("list", {
      name: "Chats steward 1 started in other workspaces",
    });
    expect(within(started).getByText("devops 2")).toBeTruthy();
    expect(started.querySelector(".elsewhere")?.textContent).toBe("beta");
  });

  it("draws a chat started in the same workspace by its own row in the explorer, with no badge", async () => {
    core([chat(1, "alpha"), chat(2, "alpha", { from: by(1, "handoff") })]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "2 steward 2"]));

    const explorer = await screen.findByRole("tree", { name: "Repos and branches" });
    await within(explorer).findByRole("treeitem", { name: /steward 2/ });
    expect(screen.queryByRole("list", { name: /started in other workspaces/ })).toBeNull();
    expect(explorer.querySelector(".elsewhere")).toBeNull();
  });

  it("leaves a chat whose parent has closed at the top, saying where it came from", async () => {
    // Chat 7 handed off and was closed since; what it started is still running.
    core([
      chat(1, "alpha"),
      chat(8, "alpha", { from: by(7, "handoff", "release notes") }),
      chat(9, "beta", { from: by(8, "handoff", "steward 8") }),
    ]);
    render(<App />);
    const tree = await section();

    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "1 steward 8", "2 steward 9"]));
    expect(row(tree, "steward 8").querySelector(".from")?.textContent).toBe("from release notes");
    // A chat drawn under its parent needs no note: the nesting says it.
    expect(row(tree, "steward 9").querySelector(".from")).toBeNull();
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
    expect(shape(tree)).toEqual(["1 steward 1", "2 devops 2", "2 devops 3"]);
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

    // Chat 31 is a task chat in alpha with no tab: its row here, and its row where it works
    // in alpha's explorer. Forty-nine other rows draw nothing.
    expect(drawn.marks).toEqual(["working", "working"]);
    expect(within(row(tree, "steward 31")).getByText("working")).toBeTruthy();
  });
});

describe("an anonymous helper", () => {
  it("still shows as a helper row under its chat in the explorer", async () => {
    const { move } = core([chat(1, "alpha"), chat(2, "alpha", { from: by(1, "handoff") })]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(2));

    move(1, "running", 10, [], [{ agent: "thread-7", state: "running" }]);

    const helpers = await screen.findByRole("list", { name: "Helpers of steward 1" });
    expect(within(helpers).getByText("helper thread-7")).toBeTruthy();
  });
});

/** 1 in alpha started 2 in alpha, which started the task chat 3 in beta. 4 is on its own. */
const threeDeep = () => [
  chat(1, "alpha"),
  chat(2, "alpha", { from: by(1, "handoff"), label: "drop commons" }),
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

  it("still shows on a folded row when a chat two levels down needs you", async () => {
    const { move } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));
    expect(row(tree, "steward 1").getAttribute("aria-expanded")).toBe("true");
    expect(row(tree, "steward 4").getAttribute("aria-expanded")).toBeNull();

    await userEvent.click(within(tree).getByTitle("Fold the chats under steward 1"));
    expect(shape(tree)).toEqual(["1 steward 1", "1 steward 4"]);
    expect(row(tree, "steward 1").getAttribute("aria-expanded")).toBe("false");

    // The grandchild asks while its row, and its parent's, are folded away.
    move(3, "waiting", 10, [3]);

    expect(rolledUp(tree, "steward 1")?.getAttribute("data-leads-to")).toBe("3");
    expect(shape(tree)).toEqual(["1 steward 1", "1 steward 4"]);
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

    fireEvent.contextMenu(row(tree, "drop commons"));
    expect(await screen.findByRole("menuitem", { name: "Stop chat drop commons" })).toBeTruthy();
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Stop chat drop commons and everything below it" }),
    );

    const question = await screen.findByRole("alertdialog", {
      name: "Stop chat drop commons and everything below it?",
    });
    expect(question.textContent).toContain("drop commons and the 1 chat below it end");
    expect(stops(asked)).toEqual([]);

    await userEvent.click(within(question).getByRole("button", { name: "Stop 2 chats" }));

    await waitFor(() => expect(stops(asked)).toEqual([{ plane: PLANE, session: 2, below: true }]));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("stops nothing when the question is cancelled", async () => {
    const { asked } = core(threeDeep());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(4));

    fireEvent.contextMenu(row(tree, "drop commons"));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Stop chat drop commons" }));
    const question = await screen.findByRole("alertdialog", { name: "Stop chat drop commons?" });
    expect(question.textContent).toContain("one short turn to write what it did");
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

    fireEvent.contextMenu(within(strip()).getByRole("tab", { name: /steward 1/ }));
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
    expect(tabNames()).toContain("drop commons");

    stop(2, "stopping");
    expect(row(tree, "drop commons").querySelector(".stopping")?.textContent).toBe("Stopping…");
    // Pressed again, it ends without waiting, and the row says so.
    fireEvent.contextMenu(row(tree, "drop commons"));
    expect(await screen.findByRole("menuitem", { name: "End chat drop commons now" })).toBeTruthy();
    await userEvent.keyboard("{Escape}");

    stop(2, "stopped");

    await waitFor(() => expect(shape(tree)).toEqual(["1 steward 1", "1 devops 3", "1 steward 4"]));
    expect(tabNames()).not.toContain("drop commons");
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
    stop(2, "stopping");

    fireEvent.contextMenu(row(tree, "drop commons"));
    await userEvent.click(
      await screen.findByRole("menuitem", {
        name: "Stop chat drop commons and everything below it",
      }),
    );
    const question = await screen.findByRole("alertdialog", {
      name: "Stop chat drop commons and everything below it?",
    });
    expect(question.textContent).toContain("drop commons is being stopped already.");
    await userEvent.click(within(question).getByRole("button", { name: "Stop 1 chat" }));

    await waitFor(() =>
      expect(asked.filter((one) => one.cmd === "stop_chat").map((one) => one.args)).toEqual([
        { plane: PLANE, session: 2, below: true },
      ]),
    );
  });
});

/** The explorer's row for the chat it calls `name`. */
const explorerRow = async (name: string) => {
  const explorer = await screen.findByRole("tree", { name: "Repos and branches" });
  const found = within(explorer)
    .getAllByRole("treeitem")
    .find((one) => one.querySelector(".session")?.textContent === name);
  if (found === undefined) throw new Error(`the explorer has no row named ${name}`);
  return found;
};

/** A task of chat 1 as its record stands: how it reported, or whom it is asking. */
const taskOf = (more: Partial<Lineage>): Lineage => ({ ...by(1, "task"), ...more });

describe("a chat's state, as a word and a shape (#1484)", () => {
  it("tells a finished task from a chat waiting on the person by word and by shape, in both lists", async () => {
    const { move } = core([
      chat(1, "alpha"),
      chat(2, "alpha", { label: "talk", from: taskOf({ reported: true, outcome: "done" }) }),
      chat(3, "alpha", { label: "sweep", from: taskOf({ reported: true, outcome: "cancelled" }) }),
      chat(4, "alpha", { label: "probe", from: taskOf({ reported: true, outcome: "failed" }) }),
      chat(5, "alpha", { label: "lost", from: taskOf({ unreported: true, outcome: "failed" }) }),
      // Stopped by the person before it reported: purlis's own report, the record `stopped`.
      chat(6, "alpha", { label: "halt", from: taskOf({ unreported: true, outcome: "stopped" }) }),
    ]);
    render(<App />);
    const tree = await section();
    // Every task under it is over, so the session folded by itself (#1499): opened by hand.
    fireEvent.click(await within(tree).findByTitle("Show the chats under steward 1"));
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
      ["halt", { word: "cancelled", shape: "dash" }],
    ] as const;
    for (const [name, state] of expected) {
      expect(says(row(tree, name)), `${name} in the Chats list`).toEqual(state);
      expect(says(await explorerRow(name)), `${name} in the explorer`).toEqual(state);
    }
    // The word is the text, read once: the mark beside it is decoration.
    expect(within(row(tree, "talk")).getByText("done")).toBeTruthy();
    expect(within(row(tree, "talk")).queryByRole("img", { name: "done" })).toBeNull();
  });

  it("names a task with no tab the same in the Chats list and the explorer, never by its number", async () => {
    core([
      chat(1, "alpha"),
      chat(17, "alpha", { persona: "devops", label: "live check talk", from: by(1, "task") }),
      chat(18, "alpha", { persona: "devops", from: by(1, "task") }),
    ]);
    render(<App />);
    const tree = await section();

    await waitFor(() =>
      expect(shape(tree)).toEqual(["1 steward 1", "2 live check talk", "2 devops 18"]),
    );
    expect(tabNames()).toEqual(["steward 1"]);
    expect(await explorerRow("live check talk")).toBeTruthy();
    expect(await explorerRow("devops 18")).toBeTruthy();
    const explorer = await screen.findByRole("tree", { name: "Repos and branches" });
    const named = within(explorer)
      .getAllByRole("treeitem")
      .map((one) => one.querySelector(".session")?.textContent)
      .filter((name) => name !== undefined && name !== null);
    expect(named).not.toContain("17");
    expect(named).not.toContain("18");
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
      chat(2, "alpha", { label: "talk", from: by(1, "task") }),
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
    expect(says(await explorerRow("talk"))).toEqual(asking);

    // Answered, and it goes on inside the same turn.
    await rowsChange(2, (from) => ({ ...from, asking: null }));

    await waitFor(() => expect(says(row(tree, "talk"))).toEqual(working));
    expect(says(await explorerRow("talk"))).toEqual(working);
  });

  it("says done, never ended without a report, of a task whose program ends right after its report", async () => {
    const { move, rowsChange } = core([
      chat(1, "alpha"),
      chat(2, "alpha", { label: "talk", from: by(1, "task") }),
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
    expect(says(await explorerRow("talk"))).toEqual(done);
  });

  it("says needs you of a reported task the person has run again, as the title bar's list does", async () => {
    const { move } = core([
      chat(1, "alpha"),
      chat(2, "alpha", { label: "talk", from: taskOf({ reported: true, outcome: "done" }) }),
    ]);
    render(<App />);
    const tree = await section();
    // Its one task is over, so the session folded by itself (#1499): opened by hand.
    fireEvent.click(await within(tree).findByTitle("Show the chats under steward 1"));
    await waitFor(() => expect(shape(tree)).toHaveLength(2));
    const both = async (state: { word: string; shape: string }) => {
      expect(says(row(tree, "talk"))).toEqual(state);
      expect(says(await explorerRow("talk"))).toEqual(state);
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
      chat(2, "alpha", { label: "talk", from: taskOf({ asking: true }) }),
    ]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toEqual(["1 the release", "2 talk"]));

    expect(says(row(tree, "talk")).word).toBe("asking the release");
    expect(says(await explorerRow("talk")).word).toBe("asking the release");
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
    expect(says(await explorerRow("steward 1"))).toEqual(unheard);
  });

  it("says working in both lists once the chat's harness is heard", async () => {
    const { move } = core([chat(1, "alpha")]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(1));

    move(1, "running", 10);

    const working = { word: "working", shape: "ring" };
    expect(says(row(tree, "steward 1"))).toEqual(working);
    expect(says(await explorerRow("steward 1"))).toEqual(working);
  });
});
