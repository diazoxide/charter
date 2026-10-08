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
import { setChatsListPrefs } from "./chatsListPrefs";
import type { State } from "./chatState";
import { forgetThisLaunch } from "./regions";

/**
 * **The Chats list at fifty chats, against the whole window** (#1499, V100-47 to V100-50): the
 * order its sessions stand in and when it is held still, the folds it makes by itself, the
 * filter, the two lines of a row, its keys, and what a chat moving redraws.
 *
 * The harness is `ChatsSection.window.test.tsx`'s: the core's answers as fixtures, and its
 * events sent by hand.
 */

/** Every draw of a row, by its chat: a row draws its activity slot each time it is drawn. */
const drawn = vi.hoisted(() => ({ rows: [] as number[] }));

vi.mock("./ChatRowActivity", () => ({
  ChatRowActivity: ({ session }: { session: number }) => {
    drawn.rows.push(session);
    return null;
  },
}));

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
type Filed = OpenChat & { workspace: string };

/** A chat working in `workspace`, as the core lists it. */
function chat(session: number, workspace: string, more: Partial<OpenChat> = {}): Filed {
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

/** A task chat `asker` asked for, which the person saw as `name` in `workspace`. */
function taskOf(asker: number, workspace = "alpha", name = `steward ${asker}`): Lineage {
  return {
    chat: asker,
    name,
    workspace,
    task: true,
    tab: false,
    reported: false,
    unreported: false,
  };
}

function finished(id: string, asker: number, more: Partial<FinishedTask> = {}): FinishedTask {
  return {
    id,
    asker,
    name: `check ${id}`,
    persona: "devops",
    outcome: "done",
    folds: true,
    report: "All good.",
    changed: null,
    ended: "2026-10-08T12:04:30+00:00",
    place: "alpha",
    branch: null,
    reopens: true,
    ...more,
  };
}

/** A fixture's chat as the core sends it: the workspace is where it is filed, not a field. */
function asListed(one: Filed): OpenChat {
  const sent: OpenChat & { workspace?: string } = { ...one };
  delete sent.workspace;
  return sent;
}

type Asked = { cmd: string; args: Record<string, unknown> };

/** The core, holding `open` chats in two workspaces and `ended` finished tasks. */
function core(open: Filed[], ended: FinishedTask[] = []) {
  const asked: Asked[] = [];
  const listeners = new Map<string, number>();
  let sequence = 0;
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: a });
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, handler);
      return 1;
    }
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return open.map(asListed);
    if (cmd === "open_chat_tab") {
      const one = open.find((chat) => chat.session === a.session);
      if (one?.from) one.from = { ...one.from, tab: true };
      return null;
    }
    if (cmd === "stopping_chats") return [];
    if (cmd === "finished_tasks") return ended;
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
    asked: (cmd: string) => asked.filter((one) => one.cmd === cmd).map((one) => one.args),
    /** The core says chat `session` moved to `state`, with `queue` asking for the person. */
    move: (session: number, state: State, queue: number[] = []) => {
      const handler = listeners.get("chat-moved");
      if (handler === undefined) throw new Error("the window is not listening for moves");
      sequence += 1;
      const moved: Moved = {
        plane: PLANE,
        session,
        state,
        needs_you: queue.includes(session),
        queue,
        moved_at: sequence,
        sequence,
        reports: [],
        refusals: [],
        children: [],
        needs: null,
        stopped: null,
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
const theTree = () => screen.getByRole("tree", { name: "Chats of this project" });
/** The list the pointer rests over: the tree and what holds it. */
const list = () => {
  const found = theTree().closest<HTMLElement>(".chats-list");
  if (found === null) throw new Error("the tree is not in a list");
  return found;
};

/** Each row of the section as `level name`, top to bottom. */
const shape = () =>
  within(theTree())
    .getAllByRole("treeitem")
    .map(
      (row) => `${row.getAttribute("aria-level")} ${row.querySelector(".session")?.textContent}`,
    );

const row = (name: string) => {
  const found = within(theTree())
    .getAllByRole("treeitem")
    .find((one) => one.querySelector(".session")?.textContent === name);
  if (found === undefined) throw new Error(`no row is named ${name}`);
  return found;
};

/** Where a row says it stands among its siblings: `posinset/setsize`. */
const place = (name: string) =>
  `${row(name).getAttribute("aria-posinset")}/${row(name).getAttribute("aria-setsize")}`;

const box = () =>
  screen.getByRole("searchbox", { name: "Filter chats by name, persona, workspace or state" });
/**
 * Types into the filter's box, with the keyboard brought to it and no pointer: jsdom lays
 * nothing out, so a pointer pressed anywhere lands on the regions' divider, which takes the
 * keyboard as it would for a drag.
 */
const typed = async (text: string) => {
  act(() => box().focus());
  await userEvent.keyboard(text);
};
const hiddenSaid = () => document.querySelector(".chats-hidden")?.textContent;

/** Three sessions, the first with a task, and each made to do what a test says. */
const threeSessions = () => [
  chat(1, "alpha"),
  chat(2, "alpha"),
  chat(3, "beta"),
  chat(4, "alpha", { persona: "devops", from: taskOf(1) }),
];

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
  drawn.rows.length = 0;
});

describe("the order of the Chats list (V100-47)", () => {
  /** Session 3 needs the person, 2 is at work, 1 and its task are idle. */
  const settled = async () => {
    const held = core(threeSessions());
    render(<App />);
    await section();
    await waitFor(() => expect(shape()).toHaveLength(4));
    held.move(1, "waiting");
    held.move(4, "waiting");
    held.move(2, "running");
    held.move(3, "waiting", [3]);
    return held;
  };

  it("puts the sessions that need the person first, then the ones at work, then the idle", async () => {
    await settled();

    expect(shape()).toEqual(["1 steward 3", "1 steward 2", "1 steward 1", "2 devops 4"]);
    // Each row says where it now stands, to a screen reader.
    expect(place("steward 3")).toBe("1/3");
    expect(place("steward 2")).toBe("2/3");
    expect(place("steward 1")).toBe("3/3");
    expect(place("devops 4")).toBe("1/1");
  });

  it("puts a session whose task needs the person with the ones that need the person", async () => {
    const { move } = await settled();

    move(4, "waiting", [3, 4]);

    expect(shape()).toEqual(["1 steward 1", "2 devops 4", "1 steward 3", "1 steward 2"]);
  });

  it("does not move a row while the pointer rests over the list, and does when it leaves", async () => {
    const { move, asked } = await settled();
    const before = shape();

    await userEvent.hover(list());
    move(1, "waiting", [3, 1]);
    move(2, "waiting", [3, 1]);

    expect(shape()).toEqual(before);
    // The state itself is said at once: only the order waits.
    expect(within(row("steward 1")).getByRole("img", { name: "needs you" })).toBeTruthy();
    // No row moved under the click: what is pressed is what was under the pointer.
    await userEvent.click(row("devops 4"));
    await waitFor(() => expect(asked("open_chat_tab")).toEqual([{ plane: PLANE, session: 4 }]));
    expect(shape()).toEqual(before);

    act(() => row("devops 4").blur());
    await userEvent.unhover(list());

    expect(shape()).toEqual(["1 steward 1", "2 devops 4", "1 steward 3", "1 steward 2"]);
  });

  it("does not move a row while the keyboard is in the list, and does when it leaves", async () => {
    const { move } = await settled();
    const before = shape();

    act(() => row("steward 2").focus());
    move(1, "waiting", [3, 1]);

    expect(shape()).toEqual(before);
    // The arrows walk the rows as they stand.
    await userEvent.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(row("steward 1"));

    act(() => row("steward 1").blur());

    expect(shape()).toEqual(["1 steward 1", "2 devops 4", "1 steward 3", "1 steward 2"]);
  });

  it("groups the sessions by workspace where the person asked for that", async () => {
    await settled();

    act(() => setChatsListPrefs({ grouped: true }));

    expect(shape()).toEqual(["1 steward 2", "1 steward 1", "2 devops 4", "1 steward 3"]);
    expect([...theTree().querySelectorAll(".chats-group")].map((one) => one.textContent)).toEqual([
      "alpha",
      "beta",
    ]);
    // The names over them are not rows: the tree still counts three sessions.
    expect(place("steward 3")).toBe("3/3");
  });
});

describe("the folds the Chats list makes by itself (V100-48)", () => {
  const twoTasks = () => [
    chat(1, "alpha"),
    chat(2, "alpha", { persona: "devops", from: taskOf(1) }),
    chat(3, "alpha", { persona: "devops", from: taskOf(1) }),
    chat(4, "alpha"),
  ];
  const up = async (ended: FinishedTask[] = []) => {
    const held = core(twoTasks(), ended);
    render(<App />);
    await section();
    await waitFor(() => expect(shape()).toHaveLength(4));
    held.move(2, "running");
    held.move(3, "running");
    return held;
  };

  it("keeps a session open while a task under it works, and folds it when all are over", async () => {
    const { move } = await up();
    expect(row("steward 1")).toHaveAttribute("aria-expanded", "true");

    // One of them ends, owing its report: the other still works.
    move(2, "done");
    expect(row("steward 1")).toHaveAttribute("aria-expanded", "true");
    move(3, "done");

    expect(row("steward 1")).toHaveAttribute("aria-expanded", "false");
    expect(shape()).toEqual(["1 steward 1", "1 steward 4"]);
  });

  it("opens a session it folded when a task under it needs the person", async () => {
    const { move } = await up();
    move(2, "done");
    move(3, "done");
    expect(shape()).toHaveLength(2);

    move(3, "waiting", [3]);

    expect(row("steward 1")).toHaveAttribute("aria-expanded", "true");
    expect(shape()).toEqual(["1 steward 1", "2 devops 2", "2 devops 3", "1 steward 4"]);
  });

  it("keeps a fold set by hand when it would have opened the session by itself", async () => {
    const { move } = await up();

    fireEvent.keyDown(row("steward 1"), { key: "ArrowLeft" });
    expect(row("steward 1")).toHaveAttribute("aria-expanded", "false");
    move(2, "done");
    move(3, "done");
    move(3, "waiting", [3]);

    expect(row("steward 1")).toHaveAttribute("aria-expanded", "false");
    expect(shape()).toEqual(["1 steward 1", "1 steward 4"]);
  });

  it("keeps a session opened by hand open when it would have folded it by itself", async () => {
    const { move } = await up();
    fireEvent.keyDown(row("steward 1"), { key: "ArrowLeft" });
    fireEvent.keyDown(row("steward 1"), { key: "ArrowRight" });

    move(2, "done");
    move(3, "done");

    expect(row("steward 1")).toHaveAttribute("aria-expanded", "true");
    expect(shape()).toHaveLength(4);
  });

  it("says on a folded session's row how its finished tasks ended", async () => {
    core(
      [chat(1, "alpha")],
      ["a", "b", "c", "d", "e"].map((id) => finished(id, 1)),
    );
    render(<App />);
    await section();

    // Five done and nothing at work: folded by itself, with the count in the state's mark.
    const summary = await within(theTree()).findByRole("img", { name: "5 done, folded" });
    expect(summary.querySelector('[data-shape="tick"]')?.textContent).toBe("5");
    expect(row("steward 1")).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("group", { name: "Finished tasks of steward 1" })).toBeNull();

    // Opened by hand, the finished tasks are under it, and the row no longer counts them.
    fireEvent.keyDown(row("steward 1"), { key: "ArrowRight" });
    const group = await screen.findByRole("group", { name: "Finished tasks of steward 1" });
    expect(within(group).getByRole("button", { name: "Finished (5)" })).toBeTruthy();
    expect(within(theTree()).queryByRole("img", { name: "5 done, folded" })).toBeNull();
  });

  it("does not fold a failure away: a session with a task that did not come out done stays open", async () => {
    core(
      [chat(1, "alpha")],
      [finished("a", 1), finished("b", 1, { outcome: "failed", folds: false })],
    );
    render(<App />);
    await section();

    expect(await screen.findByRole("group", { name: "Finished tasks of steward 1" })).toBeTruthy();
    expect(row("steward 1")).toHaveAttribute("aria-expanded", "true");
    // Folded by hand, it counts both ends.
    fireEvent.keyDown(row("steward 1"), { key: "ArrowLeft" });
    expect(within(theTree()).getByRole("img", { name: "1 done, 1 failed, folded" })).toBeTruthy();
  });
});

describe("the filter over the Chats list (V100-49)", () => {
  const four = () => [
    chat(1, "alpha"),
    chat(2, "beta", { persona: "devops", label: "live check talk", from: taskOf(1) }),
    chat(3, "alpha", { label: "release notes" }),
    chat(4, "alpha"),
  ];
  const up = async () => {
    const held = core(four());
    render(<App />);
    await section();
    await waitFor(() => expect(shape()).toHaveLength(4));
    for (const session of [1, 2, 3, 4]) held.move(session, "waiting");
    return held;
  };

  it("finds a task by its name, and keeps the session that asked for it above it", async () => {
    await up();

    await typed("talk");
    expect(shape()).toEqual(["1 steward 1", "2 live check talk"]);
    expect(hiddenSaid()).toBe("The filter hides 2 of 4 chats.");
    // What is drawn is what a screen reader counts.
    expect(place("steward 1")).toBe("1/1");
    expect(place("live check talk")).toBe("1/1");
  });

  it("finds a task by its persona, its workspace and its state's word", async () => {
    const { move } = await up();

    await typed("devops");
    expect(shape()).toEqual(["1 steward 1", "2 live check talk"]);

    await userEvent.clear(box());
    await typed("beta");
    expect(shape()).toEqual(["1 steward 1", "2 live check talk"]);

    move(2, "running");
    await userEvent.clear(box());
    await typed("working");
    expect(shape()).toEqual(["1 steward 1", "2 live check talk"]);
  });

  it("has a chip for the chats that need the person and one for the ones at work", async () => {
    const { move } = await up();
    move(3, "waiting", [3]);
    move(2, "running", [3]);

    await userEvent.click(screen.getByRole("checkbox", { name: "needs you" }));
    expect(shape()).toEqual(["1 release notes"]);

    await userEvent.click(screen.getByRole("checkbox", { name: "working" }));
    expect(shape()).toEqual(["1 release notes", "1 steward 1", "2 live check talk"]);

    await userEvent.click(screen.getByRole("checkbox", { name: "needs you" }));
    expect(shape()).toEqual(["1 steward 1", "2 live check talk"]);
  });

  it("is cleared by Escape, in its box and in the list", async () => {
    await up();
    await typed("talk");
    expect(shape()).toHaveLength(2);

    await userEvent.keyboard("{Escape}");

    expect(box()).toHaveValue("");
    expect(shape()).toHaveLength(4);
    expect(hiddenSaid()).toBe("");

    await userEvent.click(screen.getByRole("checkbox", { name: "working" }));
    expect(hiddenSaid()).toBe("No chat matches the filter.");
    await userEvent.click(screen.getByRole("checkbox", { name: "working" }));
    await typed("notes");
    fireEvent.keyDown(row("release notes"), { key: "Escape" });
    expect(shape()).toHaveLength(4);
  });

  it("opens a session it folded by itself over a task the filter asks for", async () => {
    const { move } = await up();
    move(2, "done");
    expect(shape()).toEqual(["1 steward 1", "1 release notes", "1 steward 4"]);

    await typed("talk");

    expect(shape()).toEqual(["1 steward 1", "2 live check talk"]);
  });

  it("is in the palette too: a task with no tab is found there, and shown from there", async () => {
    const { asked } = await up();

    await userEvent.keyboard("{F2}");
    await userEvent.type(screen.getByRole("combobox"), "live check");
    await userEvent.click(
      await screen.findByRole("option", {
        name: /Show task live check talk in beta, asked by steward 1/,
      }),
    );

    await waitFor(() => expect(asked("open_chat_tab")).toEqual([{ plane: PLANE, session: 2 }]));
  });
});

describe("a row of the Chats list (V100-19, V100-50)", () => {
  const rows = () => [
    chat(1, "alpha"),
    chat(2, "alpha", { persona: "devops", from: taskOf(1) }),
    chat(3, "beta", { persona: "devops", from: taskOf(1) }),
    chat(4, "alpha", {
      persona: "devops",
      from: taskOf(1),
      cwd: `${PLANE}/workspaces/alpha/.worktrees/app/fix-login`,
    }),
  ];
  const up = async () => {
    const held = core(rows());
    render(<App />);
    await section();
    await waitFor(() => expect(shape()).toHaveLength(4));
    return held;
  };
  const second = (name: string) => row(name).querySelector(".line.two");

  it("is two lines: the mark, the name and the state, then where it works", async () => {
    const { move } = await up();
    move(1, "running");

    const first = row("steward 1").querySelector(".line.one");
    expect(first?.querySelector(".persona-mark")).not.toBeNull();
    expect(first?.querySelector(".session")?.textContent).toBe("steward 1");
    expect(first?.querySelector(".shown-state .word")?.textContent).toBe("working");
    expect(second("steward 1")?.querySelector(".workspace")?.textContent).toBe("alpha");
  });

  it("says the whole name on hover and to a screen reader, whatever the row cuts short", async () => {
    await up();

    expect(row("steward 1").querySelector(".session")).toHaveAttribute("title", "steward 1");
    expect(row("steward 1")).toHaveAccessibleName(/^steward 1/);
  });

  it("says a task's workspace only when it is not its asker's, and its own branch where it has one", async () => {
    await up();

    expect(second("devops 2")?.querySelector(".workspace")).toBeNull();
    expect(second("devops 3")?.querySelector(".workspace")?.textContent).toBe("beta");
    expect(second("devops 4")?.querySelector(".own-branch")?.textContent).toBe(
      "own branch fix-login",
    );
  });

  it("says how long a chat has been in its state, once the window has seen it come into it", async () => {
    const { move } = await up();

    // The first the window hears of it: how long it has been so is not known, and not said.
    move(2, "running");
    expect(row("devops 2").querySelector(".since")).toBeNull();

    move(2, "waiting");

    expect(row("devops 2").querySelector(".since")?.textContent).toBe("just now");
    expect(row("devops 3").querySelector(".since")).toBeNull();
  });

  it("wears no badge for having no tab, and says nothing of tabs", async () => {
    await up();

    expect(theTree().querySelector(".no-tab")).toBeNull();
    expect(theTree().textContent).not.toMatch(/no tab/i);
    for (const one of within(theTree()).getAllByRole("treeitem"))
      expect(one.getAttribute("title") ?? "").not.toMatch(/tab/i);
  });

  it("drops the second line on one line, and keeps what it said as the row's tooltip", async () => {
    await up();

    act(() => setChatsListPrefs({ lines: 1 }));

    expect(theTree().querySelector(".line.two")).toBeNull();
    expect(row("devops 3")).toHaveAttribute("title", "beta");
    expect(row("devops 4")).toHaveAttribute("title", "own branch fix-login");
    expect(row("devops 2")).not.toHaveAttribute("title");
    // The first line is as it was.
    expect(row("devops 3").querySelector(".line.one .session")?.textContent).toBe("devops 3");
  });
});

describe("the keys of a row (#1499)", () => {
  const up = async () => {
    const held = core([
      chat(1, "alpha"),
      chat(2, "alpha", { persona: "devops", from: taskOf(1) }),
      chat(3, "alpha"),
    ]);
    render(<App />);
    await section();
    await waitFor(() => expect(shape()).toHaveLength(3));
    return held;
  };
  const said = () => document.querySelector(".chats-said")?.textContent;

  it("opens a row on Enter", async () => {
    const { asked } = await up();

    act(() => row("devops 2").focus());
    await userEvent.keyboard("{Enter}");

    await waitFor(() => expect(asked("open_chat_tab")).toEqual([{ plane: PLANE, session: 2 }]));
  });

  it("asks for a row beside the chat in front on Space, and says why that cannot be done yet", async () => {
    const { asked } = await up();

    act(() => row("devops 2").focus());
    await userEvent.keyboard(" ");

    expect(said()).toBe(
      "purlis cannot open a chat beside another yet. Press Enter to open it in front.",
    );
    // Space did not press the row: nothing was opened.
    expect(asked("open_chat_tab")).toEqual([]);
  });

  it("asks to stop a task on Delete, and stops nothing until the person says so", async () => {
    const { asked } = await up();

    act(() => row("devops 2").focus());
    await userEvent.keyboard("{Delete}");

    expect(await screen.findByRole("alertdialog", { name: "Stop chat devops 2?" })).toBeTruthy();
    expect(asked("stop_chat")).toEqual([]);
  });

  it("does nothing on Delete for a chat that is not a task", async () => {
    await up();

    act(() => row("steward 3").focus());
    await userEvent.keyboard("{Delete}");

    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("walks the rows with the arrows, through a fold and a filter", async () => {
    await up();

    act(() => row("steward 1").focus());
    await userEvent.keyboard("{ArrowLeft}");
    expect(row("steward 1")).toHaveAttribute("aria-expanded", "false");
    await userEvent.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(row("steward 3"));
    expect(place("steward 3")).toBe("2/2");

    await typed("devops");
    // A fold set by hand is kept under a filter: the row above the match is drawn, folded.
    expect(shape()).toEqual(["1 steward 1"]);
    act(() => row("steward 1").focus());
    await userEvent.keyboard("{ArrowRight}");
    await userEvent.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(row("devops 2"));
    expect(row("devops 2")).toHaveAttribute("aria-level", "2");
  });
});

describe("a chat moving, with fifty sessions and their tasks listed (SC-3)", () => {
  it("draws no row for a move that changes no order and no fold, and rows for one that does", async () => {
    const { move } = core([
      ...Array.from({ length: 50 }, (_, at) => chat(at + 1, at % 2 === 0 ? "alpha" : "beta")),
      ...Array.from({ length: 50 }, (_, at) =>
        chat(at + 51, "alpha", { persona: "devops", from: taskOf(at + 1) }),
      ),
    ]);
    // Not under StrictMode, which draws everything twice: what is counted is draws.
    renderBare(<App />);
    await section();
    await waitFor(() => expect(shape()).toHaveLength(100));
    for (let turn = 0; turn < 10; turn += 1) await act(async () => {});
    drawn.rows.length = 0;

    // Tasks start and end their turns, and a session too: each says so on its own mark.
    move(51, "running");
    move(52, "running");
    move(51, "waiting");
    move(7, "running");

    expect(within(row("devops 52")).getByRole("img", { name: "working" })).toBeTruthy();
    expect(within(row("devops 51")).getByRole("img", { name: "idle" })).toBeTruthy();
    expect(drawn.rows).toEqual([]);

    // Session 40 needs the person: it goes to the top, which is the one thing that moves rows.
    move(40, "waiting", [40]);

    expect(shape()[0]).toBe("1 steward 40");
    expect(drawn.rows.length).toBeGreaterThan(0);
  });
});
