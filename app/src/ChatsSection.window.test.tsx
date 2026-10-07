import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
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

/**
 * **The Chats section, against the whole window** (#1447): every running chat of the project
 * in one tree in the left region, nested by which chat started which. A handoff has a tab; a
 * task chat is listed with none until its row is clicked.
 *
 * Nothing starts a task chat yet, so the ones here are the core's answer as a fixture: a chat
 * whose `from` says `task: true` and `tab: false`. The record that says so is `reopen.rs`'s
 * and `chats.rs`'s.
 */

const drawn = vi.hoisted(() => ({ marks: [] as (string | undefined)[] }));

vi.mock("./NeedsYou", async (original) => {
  const real = await original<typeof import("./NeedsYou")>();
  return {
    ...real,
    ChatMark: (props: { state: State | undefined }) => {
      drawn.marks.push(props.state);
      return <real.ChatMark {...props} />;
    },
  };
});

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

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
  const now = () => open.map(asListed);
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: a });
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, handler);
      return 1;
    }
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return now();
    if (cmd === "open_chat_tab") {
      const one = open.find((chat) => chat.session === a.session);
      if (one?.from) one.from = { ...one.from, tab: true };
      return null;
    }
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
    /** The core says chat `session` moved to `state`. */
    move: (
      session: number,
      state: State,
      at: number,
      queue: number[] = [],
      children: Moved["children"] = [],
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
        reports: [],
        refusals: [],
        children,
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
    expect(within(first).getByRole("img", { name: "unknown" })).toBeTruthy();
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

    expect(within(row(tree, "steward 2")).getByRole("img", { name: "needs you" })).toBeTruthy();
    expect(within(row(tree, "steward 1")).queryByRole("img", { name: "needs you" })).toBeNull();

    move(2, "running", 11, []);
    expect(within(row(tree, "steward 2")).queryByRole("img", { name: "needs you" })).toBeNull();
    expect(within(row(tree, "steward 2")).getByRole("img", { name: "running" })).toBeTruthy();
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

  it("opens as an ordinary tab, in front, when its row is clicked, and the core is told", async () => {
    const { asked } = core(sixTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(7));

    await userEvent.click(row(tree, "devops 4"));

    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "devops 4"]));
    const front = within(strip())
      .getAllByRole("tab")
      .filter((tab) => tab.getAttribute("aria-selected") === "true")
      .map((tab) => tab.querySelector(".tab-name")?.textContent);
    expect(front).toEqual(["devops 4"]);
    expect(screen.getByTestId("pane").textContent).toBe("session 4");
    expect(asked.filter((one) => one.cmd === "open_chat_tab").map((one) => one.args)).toEqual([
      { plane: PLANE, session: 4 },
    ]);
    await waitFor(() => expect(row(tree, "devops 4").getAttribute("data-tab")).toBe("true"));

    // A second click brings the same tab forward and opens no other.
    await userEvent.click(row(tree, "devops 4"));
    expect(tabNames()).toEqual(["steward 1", "devops 4"]);
    expect(asked.filter((one) => one.cmd === "open_chat_tab")).toHaveLength(1);
  });

  it("opens in the workspace it works in, when that is not the one in front", async () => {
    core(sixTasks());
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(7));

    await userEvent.click(row(tree, "devops 3"));

    // Chat 3 works in beta: the strip is beta's now, and holds its tab alone.
    await waitFor(() => expect(tabNames()).toEqual(["devops 3"]));
    expect(screen.getByTestId("pane").textContent).toBe("session 3");
  });

  it("is still listed, and the one opened still a tab, after the window is loaded again", async () => {
    // The same core across both windows: a reload, and a relaunch that put the chats back,
    // both ask it what is open.
    core(sixTasks());
    const first = render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(7));
    await userEvent.click(row(tree, "devops 4"));
    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "devops 4"]));
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
    expect(tabNames()).toEqual(["steward 1", "devops 4"]);
    expect(row(again, "devops 2").getAttribute("data-tab")).toBe("false");
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
    expect(drawn.marks).toEqual(["running", "running"]);
    expect(within(row(tree, "steward 31")).getByRole("img", { name: "running" })).toBeTruthy();
  });
});

describe("an anonymous helper", () => {
  it("still shows as a sub-agent row under its chat in the explorer", async () => {
    const { move } = core([chat(1, "alpha"), chat(2, "alpha", { from: by(1, "handoff") })]);
    render(<App />);
    const tree = await section();
    await waitFor(() => expect(shape(tree)).toHaveLength(2));

    move(1, "running", 10, [], [{ agent: "thread-7", state: "running" }]);

    const helpers = await screen.findByRole("list", { name: "Sub-agents of steward 1" });
    expect(within(helpers).getByText("sub-agent thread-7")).toBeTruthy();
  });
});
