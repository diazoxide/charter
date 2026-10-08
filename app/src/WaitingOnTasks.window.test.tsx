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
import { forgetThisLaunch } from "./regions";
import type { Shown } from "./shownState";

/**
 * **A session waiting on its tasks says so, counts them, and is flagged only for what
 * matters** (#1491), against the whole window.
 *
 * What the operator saw: a session whose turn had ended while its tasks worked wore the mark
 * of a chat waiting on him. The core now keeps such a session out of the needs-you queue; this
 * is what the window then says of it: `waiting on 2 tasks` in both lists, the count on its
 * row, the hand for a task that failed and none for one that is done.
 *
 * The core's answers are fixtures: the chats it lists, the finished rows it reads from its
 * dispatch records, and the moves it sends.
 */

const drawn = vi.hoisted(() => ({ marks: [] as string[] }));

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

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";

type Lineage = NonNullable<OpenChat["from"]>;

/** A chat in workspace alpha, as the core lists it. */
function chat(session: number, more: Partial<OpenChat> = {}): OpenChat {
  return {
    session,
    name: String(session),
    cwd: `${PLANE}/workspaces/alpha`,
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
  };
}

/** A task chat `asker` dispatched, called `label`, as its record stands. */
function task(session: number, label: string, asker = 1, more: Partial<Lineage> = {}): OpenChat {
  return chat(session, {
    label,
    from: {
      chat: asker,
      name: `steward ${asker}`,
      workspace: "alpha",
      task: true,
      tab: false,
      reported: false,
      unreported: false,
      ...more,
    },
  });
}

function finished(id: string, name: string, more: Partial<FinishedTask> = {}): FinishedTask {
  return {
    id,
    asker: 1,
    name,
    chat: null,
    did_not_start: false,
    attempts: 0,
    waits: null,
    persona: "steward",
    how: "done",
    outcome: "done",
    folds: true,
    report: `${name}: all good.`,
    changed: null,
    ended: "2026-10-08T12:04:30+00:00",
    place: "alpha",
    branch: null,
    reopens: true,
    not_reopened: null,
    ...more,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

/** The core, holding `open` chats in workspace alpha and `rows` finished under them. */
function core(open: OpenChat[], rows: FinishedTask[] = []) {
  const asked: Asked[] = [];
  const listeners = new Map<string, number>();
  const everyListener = new Map<string, number[]>();
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
    if (cmd === "opened_chats") return [...open];
    if (cmd === "stopping_chats") return [];
    if (cmd === "finished_tasks") return [...rows];
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: ["steward"],
        persona: "steward",
        unfiled: [],
        workspaces: [
          {
            name: "alpha",
            path: `${PLANE}/workspaces/alpha`,
            vision: "",
            todos: [],
            colour: null,
            live: false,
            chats: [...open],
          },
        ],
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
        personas: ["steward"],
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
  /** The core says the rows changed: the window reads the sidebar, and its finished rows. */
  const rowsChanged = async () => {
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
  };
  return {
    asked: (cmd: string) => asked.filter((one) => one.cmd === cmd).map((one) => one.args),
    /** The core's count of chat `session`'s running tasks is `count` from the next read. */
    running: (session: number, count: number) => {
      const one = open.find((chat) => chat.session === session);
      if (one !== undefined) one.tasks_running = count;
    },
    /** A task's record changes, as when its report lands, and the core says so. */
    reports: async (session: number, outcome: string) => {
      const one = open.find((chat) => chat.session === session);
      if (one?.from == null) throw new Error(`chat ${session} is no task`);
      one.from = { ...one.from, reported: true, outcome };
      await rowsChanged();
    },
    /** The core says chat `session` moved to `state`, with `queue` asking for the person. */
    move: (
      session: number,
      state: State,
      at: number,
      queue: number[] = [],
      more: Partial<Moved> = {},
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
        children: [],
        needs: null,
        stopped: null,
        ...more,
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

const row = (tree: HTMLElement, name: string) => {
  const found = within(tree)
    .getAllByRole("treeitem")
    .find((one) => one.querySelector(".session")?.textContent === name);
  if (found === undefined) throw new Error(`no row is named ${name}`);
  return found;
};

/** The explorer's row for the chat it calls `name`. */
const explorerRow = async (name: string) => {
  const explorer = await screen.findByRole("tree", { name: "Repos and branches" });
  const found = within(explorer)
    .getAllByRole("treeitem")
    .find((one) => one.querySelector(".session")?.textContent === name);
  if (found === undefined) throw new Error(`the explorer has no row named ${name}`);
  return found;
};

/** What a row says its chat is doing: its word, its mark's shape, and the mark's colour. */
const says = (on: HTMLElement) => ({
  word: on.querySelector(".shown-state .word")?.textContent,
  shape: on.querySelector(".shown-state .shape")?.getAttribute("data-shape"),
});
const colour = (on: HTMLElement) =>
  on.querySelector<HTMLElement>(".shown-state .shape")?.style.color;

/** What a row says of its tasks, or nothing where it says none. */
const count = (on: HTMLElement) => on.querySelector(".task-count")?.textContent;

const rows = async (names: number) => {
  const tree = await section();
  await waitFor(() => expect(within(tree).getAllByRole("treeitem")).toHaveLength(names));
  return tree;
};

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
  drawn.marks.length = 0;
});

describe("a session whose turn has ended while its tasks work", () => {
  it("says waiting on 2 tasks in both lists, in the working colour, and wears no hand", async () => {
    const { move } = core([chat(1), task(2, "check prod"), task(3, "check staging")]);
    render(<App />);
    const tree = await rows(3);
    move(2, "running", 1);
    move(3, "running", 2);

    // Its turn ends, and the core keeps it out of the queue: it waits on its tasks.
    move(1, "waiting", 3, []);

    const waiting = { word: "waiting on 2 tasks", shape: "hourglass" };
    expect(says(row(tree, "steward 1"))).toEqual(waiting);
    expect(says(await explorerRow("steward 1"))).toEqual(waiting);
    // The working colour, which a working task's ring wears too.
    expect(colour(row(tree, "steward 1"))).toBe("var(--state-running)");
    expect(colour(row(tree, "check prod"))).toBe("var(--state-running)");
    // No hand anywhere: not on its row, and nothing in the title bar's list.
    expect(tree.querySelector('[data-mark="needs-you"]')).toBeNull();
    expect(screen.queryByTestId("needs-you-button")).toBeNull();
  });

  it("counts down as they finish, and needs you only once the core says the session does", async () => {
    const { move, reports } = core([chat(1), task(2, "check prod"), task(3, "check staging")]);
    render(<App />);
    const tree = await rows(3);
    move(2, "running", 1);
    move(3, "running", 2);
    move(1, "waiting", 3, []);

    // One finishes as done: the word and the count change, and nothing asks for the person.
    await reports(3, "done");
    move(3, "waiting", 4, []);

    await waitFor(() =>
      expect(says(row(tree, "steward 1"))).toEqual({
        word: "waiting on 1 task",
        shape: "hourglass",
      }),
    );
    expect(count(row(tree, "steward 1"))).toBe("1 working · 1 done");
    expect(screen.queryByTestId("needs-you-button")).toBeNull();

    // The last one reports, the session reads it in a turn of its own and stops with nothing
    // below it: the core queues it, and only now does it wear the hand.
    await reports(2, "done");
    move(2, "waiting", 5, []);
    move(1, "running", 6, []);
    expect(says(row(tree, "steward 1"))).toEqual({ word: "working", shape: "ring" });
    move(1, "waiting", 7, [1]);

    expect(says(row(tree, "steward 1"))).toEqual({ word: "needs you", shape: "hand" });
    expect(count(row(tree, "steward 1"))).toBe("2 done");
    expect(await screen.findByTestId("needs-you-button")).toBeTruthy();
  });

  it("is not waiting on tasks when its only open tasks wait on the person: their hand rolls up", async () => {
    const { move } = core([chat(1), task(2, "check prod"), task(3, "check staging")]);
    render(<App />);
    const tree = await rows(3);
    // One asks the person, the other is at rest with its report owed: neither is working.
    move(2, "waiting", 1, [2]);
    move(3, "waiting", 2, [2]);

    move(1, "waiting", 3, [2]);

    expect(says(row(tree, "steward 1"))).toEqual({ word: "idle", shape: "pause" });
    expect(count(row(tree, "steward 1"))).toBe("2 waiting");
    // The task that needs you wears the hand, and the session's row leads to it.
    expect(says(row(tree, "check prod"))).toEqual({ word: "needs you", shape: "hand" });
    expect(tree.querySelector('.rolled-up[data-leads-to="2"]')).not.toBeNull();
  });

  it("is idle, with no count, when it has no tasks at all", async () => {
    const { move } = core([chat(1), chat(2)]);
    render(<App />);
    const tree = await rows(2);

    // Ignored until it asks again, say: the queue is what raises the hand.
    move(1, "waiting", 1, []);

    expect(says(row(tree, "steward 1"))).toEqual({ word: "idle", shape: "pause" });
    expect(count(row(tree, "steward 1"))).toBeUndefined();
  });

  it("counts a task two dispatches down, and a task with tasks of its own says the same of itself", async () => {
    const { move } = core([chat(1), task(2, "check prod"), task(3, "check one shard", 2)]);
    render(<App />);
    const tree = await rows(3);
    move(3, "running", 1);

    move(2, "waiting", 2, []);
    move(1, "waiting", 3, []);

    expect(says(row(tree, "steward 1")).word).toBe("waiting on 2 tasks");
    expect(says(row(tree, "check prod")).word).toBe("waiting on 1 task");
    expect(count(row(tree, "check prod"))).toBe("1 working");
  });
});

describe("the count on a session's row", () => {
  const FIVE = ["talk", "listen", "read"].map((name, at) =>
    finished(`01K6DONE${at}`, `live check ${name}`),
  );

  it("says how many are working and how many are done, from the rows the list draws", async () => {
    const { move } = core([chat(1), task(2, "check prod"), task(3, "check staging")], FIVE);
    render(<App />);
    const tree = await rows(3);
    move(2, "running", 1);
    move(3, "running", 2);

    await waitFor(() => expect(count(row(tree, "steward 1"))).toBe("2 working · 3 done"));
    // The same rows the finished fold under it counts.
    expect(await within(tree).findByRole("button", { name: /Finished \(3\)/ })).toBeTruthy();
    // A task with none of its own says nothing.
    expect(count(row(tree, "check prod"))).toBeUndefined();
  });

  it("says how many failed when any did, and never a part that is zero", async () => {
    core(
      [chat(1)],
      [
        ...FIVE,
        finished("01K6FAILED", "check staging", { how: "failed", outcome: "failed", folds: false }),
        finished("01K6LOST", "check prod", {
          how: "unreported",
          outcome: "ended without a report",
          folds: false,
        }),
      ],
    );
    render(<App />);
    const tree = await rows(1);

    await waitFor(() => expect(count(row(tree, "steward 1"))).toBe("2 failed · 3 done"));
  });

  it("says 6 of 6 tasks at the limit the core says is in force for it", async () => {
    const six = [2, 3, 4, 5, 6, 7].map((session) => task(session, `check ${session}`));
    const { move } = core([chat(1, { tasks_limit: 6, tasks_running: 6 }), ...six], FIVE);
    render(<App />);
    const tree = await rows(7);
    for (const one of six) move(one.session, "running", one.session);

    await waitFor(() => expect(count(row(tree, "steward 1"))).toBe("6 of 6 tasks · 3 done"));
  });

  it("is below its limit again when one of the six reports", async () => {
    const six = [2, 3, 4, 5, 6, 7].map((session) => task(session, `check ${session}`));
    const { reports, running } = core([chat(1, { tasks_limit: 6, tasks_running: 6 }), ...six]);
    render(<App />);
    const tree = await rows(7);
    await waitFor(() => expect(count(row(tree, "steward 1"))).toBe("6 of 6 tasks"));

    // The core counts one fewer against the limit once it has reported.
    running(1, 5);
    await reports(7, "done");

    await waitFor(() => expect(count(row(tree, "steward 1"))).toBe("5 working · 1 done"));
  });
});

describe("a limit, said where it binds (#1498)", () => {
  const SAID =
    "This chat already has 6 tasks that have not reported, which is as many as it may have at once. Close one or wait for one to report, or raise the limit in Settings › Project › Dispatch.";

  it("says at its task limit on a refused session's row, with the way to Settings, until a slot frees", async () => {
    const six = [2, 3, 4, 5, 6, 7].map((session) => task(session, `check ${session}`));
    const session = chat(1, {
      tasks_limit: 6,
      tasks_running: 6,
      at_limit: { limit: 6, row: "at its task limit (6)", said: SAID },
    });
    const { reports, running } = core([session, ...six]);
    render(<App />);
    await rows(7);

    const line = await screen.findByTestId("at-limit-1");
    expect(line.textContent).toContain("at its task limit (6)");
    // The whole sentence, which says which limit and where it is changed.
    expect(line.getAttribute("title")).toBe(SAID);
    const linked: unknown[] = [];
    const heard = (event: Event) => linked.push((event as CustomEvent).detail);
    window.addEventListener("charter-settings-link", heard);
    try {
      await userEvent.click(within(line).getByRole("button", { name: "Dispatch settings" }));
    } finally {
      window.removeEventListener("charter-settings-link", heard);
    }
    expect(linked).toEqual([{ plane: PLANE, link: { group: "project.dispatch" } }]);
    // A task's own row says nothing of its asker's limit.
    expect(screen.queryByTestId("at-limit-2")).toBeNull();

    // A slot frees: the core says no limit binds now, and the line goes.
    session.at_limit = null;
    running(1, 5);
    await reports(7, "done");

    await waitFor(() => expect(screen.queryByTestId("at-limit-1")).toBeNull());
  });

  it("ends the session's tab menu with how many of its tasks run against its limit", async () => {
    core([chat(1, { tasks_limit: 6, tasks_running: 2 }), task(2, "check prod"), task(3, "x")]);
    render(<App />);
    await rows(3);

    // Pressed plainly, as the chip's own window tests press it.
    fireEvent.click(await screen.findByRole("button", { name: /^Tasks of steward 1/ }));
    const menu = await screen.findByRole("menu", { name: /^Tasks of steward 1/ });

    expect(menu.querySelector(".tasks-menu-limits")?.textContent).toBe("2 of 6 running");
  });

  it("says nothing at the limit where no dispatch was refused", async () => {
    const six = [2, 3, 4, 5, 6, 7].map((session) => task(session, `check ${session}`));
    core([chat(1, { tasks_limit: 6, tasks_running: 6 }), ...six]);
    render(<App />);
    const tree = await rows(7);

    await waitFor(() => expect(count(row(tree, "steward 1"))).toBe("6 of 6 tasks"));
    expect(screen.queryByTestId("at-limit-1")).toBeNull();
  });
});

describe("what a finishing task does to its session's row", () => {
  const FAILED = finished("01K6FAILED", "check staging", {
    how: "failed",
    outcome: "failed",
    folds: false,
    report: "The cluster refused the login.\nNothing was changed.",
  });
  const failure = {
    kind: "task_failed" as const,
    id: FAILED.id,
    task: "check staging",
    how: "failed" as const,
    why: "The cluster refused the login.",
  };
  /** The title bar's item for a failure of chat 1's, pressed. */
  const go = async (task = "check staging") => {
    await userEvent.click(await screen.findByTestId("needs-you-button"));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: new RegExp(`^Go to steward 1: ${task} failed`) }),
    );
  };

  it("puts no hand on it for a task that finished as done", async () => {
    const { move, reports } = core([chat(1), task(2, "check prod")]);
    render(<App />);
    const tree = await rows(2);
    move(2, "running", 1);
    move(1, "waiting", 2, []);

    // The report lands: the core tells a move of the session's that only counts.
    await reports(2, "done");
    move(2, "waiting", 3, []);
    move(1, "waiting", 4, [], { reports: ["check prod"] });

    expect(says(row(tree, "steward 1")).shape).not.toBe("hand");
    expect(tree.querySelector('[data-mark="needs-you"]')).toBeNull();
    expect(screen.queryByTestId("needs-you-button")).toBeNull();
    await waitFor(() => expect(count(row(tree, "steward 1"))).toBe("1 done"));
  });

  it("puts the hand on it for a task that failed, though it is working and others still are", async () => {
    const { move } = core([chat(1), task(2, "check prod")], [FAILED]);
    render(<App />);
    const tree = await rows(2);
    move(2, "running", 1);

    // The session is mid-turn, reading the report it was typed a line about.
    move(1, "running", 2, [1], { needs: [failure] });

    expect(says(row(tree, "steward 1"))).toEqual({ word: "needs you", shape: "hand" });
    // The title bar's item says which task failed and why, in a few words.
    await userEvent.click(await screen.findByTestId("needs-you-button"));
    expect(
      await screen.findByRole("menuitem", {
        name: /^Go to steward 1: check staging failed: The cluster refused the login\./,
      }),
    ).toBeTruthy();
  });

  it("goes to the failed task's row from its item, and tells the core it was looked at", async () => {
    const { move, asked } = core([chat(1), task(2, "check prod")], [FAILED]);
    render(<App />);
    const tree = await rows(2);
    move(1, "running", 2, [1], { needs: [failure] });
    const failedRow = await within(tree).findByRole("button", { name: /^check staging/ });

    await go();

    // Shown first, and only then looked at: that one failure, by its record's id.
    await waitFor(() => expect(failedRow).toHaveFocus());
    await waitFor(() =>
      expect(asked("task_failure_seen")).toEqual([{ plane: PLANE, session: 1, id: FAILED.id }]),
    );

    // The core answers with the session out of the queue: the hand goes.
    move(1, "running", 3, [], { needs: null });
    expect(says(row(tree, "steward 1"))).toEqual({ word: "working", shape: "ring" });
    expect(screen.queryByTestId("needs-you-button")).toBeNull();
  });

  it("opens the session's folded row to get to the failed task's row", async () => {
    const { move } = core([chat(1), task(2, "check prod")], [FAILED]);
    render(<App />);
    const tree = await rows(2);
    await within(tree).findByRole("button", { name: /^check staging/ });
    // Folded: the task under it and its finished rows are not drawn.
    const twist = tree.querySelector<HTMLElement>('.twist[data-fold="open"]');
    if (twist === null) throw new Error("the session's row does not fold");
    await userEvent.click(twist);
    expect(within(tree).queryByRole("button", { name: /^check staging/ })).toBeNull();
    move(1, "running", 2, [1], { needs: [failure] });

    await go();

    await waitFor(() =>
      expect(within(tree).getByRole("button", { name: /^check staging/ })).toHaveFocus(),
    );

    // Opened once, for that asking: folded again by the person, it stays folded.
    const again = tree.querySelector<HTMLElement>('.twist[data-fold="open"]');
    if (again === null) throw new Error("the session's row does not fold");
    await userEvent.click(again);
    move(2, "running", 5, [1]);
    move(2, "waiting", 6, [1]);
    expect(within(tree).queryByRole("button", { name: /^check staging/ })).toBeNull();
  });

  it("clears the one failure it went to, and leaves the others their hand", async () => {
    const OTHER = finished("01K6OTHER", "check prod", {
      how: "failed",
      outcome: "failed",
      folds: false,
    });
    const { move, asked } = core([chat(1)], [OTHER, FAILED]);
    render(<App />);
    const tree = await rows(1);
    await within(tree).findByRole("button", { name: /^check staging/ });
    move(1, "running", 2, [1], {
      needs: [{ ...failure, id: OTHER.id, task: "check prod", why: "No route." }, failure],
    });

    // The item says the latest, and Go is that one's.
    await go();

    await waitFor(() =>
      expect(asked("task_failure_seen")).toEqual([{ plane: PLANE, session: 1, id: FAILED.id }]),
    );
  });

  it("says so, shows the session and keeps the hand when the failed task has no row to show", async () => {
    // Its finished row was never listed (a dispatch that did not start has none).
    const { move, asked } = core([chat(1), chat(2)], []);
    render(<App />);
    const tree = await rows(2);
    move(1, "running", 2, [1], {
      needs: [
        { ...failure, id: "not-started-1", how: "did_not_start" as const, why: "No profile." },
      ],
    });

    await userEvent.click(await screen.findByTestId("needs-you-button"));
    await userEvent.click(
      await screen.findByRole("menuitem", {
        name: /^Go to steward 1: check staging did not start/,
      }),
    );

    expect(await screen.findByText(/check staging has no row left to show/)).toBeTruthy();
    // Not looked at: nothing was shown, so the hand stays where it is.
    expect(asked("task_failure_seen")).toEqual([]);
    expect(says(row(tree, "steward 1"))).toEqual({ word: "needs you", shape: "hand" });
  });

  it("goes to the chat of a failed task that is still open, and then it is looked at", async () => {
    // A task that reported blocked stays open as the chat it is; so does one that failed,
    // for the moment before purlis ends it. The core says which chat it is.
    const { move, asked } = core([chat(1), task(2, "check prod")]);
    render(<App />);
    await rows(2);
    move(2, "waiting", 1, []);
    move(1, "running", 2, [1], {
      needs: [{ ...failure, id: "01K6BLOCKED", chat: 2, task: "check prod", why: "No access." }],
    });

    await go("check prod");

    await waitFor(() =>
      expect(asked("task_failure_seen")).toEqual([{ plane: PLANE, session: 1, id: "01K6BLOCKED" }]),
    );
    // Shown inside its session's tab, as a pressed task's row shows it.
    await waitFor(() => expect(screen.getByTestId("pane").textContent).toBe("session 2"));
  });

  it("is looked at when its finished row is opened to be read", async () => {
    const { move, asked } = core([chat(1)], [FAILED, finished("01K6DONE", "live check")]);
    render(<App />);
    const tree = await rows(1);
    move(1, "running", 2, [1], { needs: [failure] });

    await userEvent.click(await within(tree).findByRole("button", { name: /^check staging/ }));

    await waitFor(() =>
      expect(asked("task_failure_seen")).toEqual([{ plane: PLANE, session: 1, id: FAILED.id }]),
    );
    // A row that is no failure its session is flagged for tells the core nothing.
    await userEvent.click(await within(tree).findByRole("button", { name: /Finished \(1\)/ }));
    await userEvent.click(await within(tree).findByRole("button", { name: /^live check/ }));
    expect(asked("task_failure_seen")).toHaveLength(1);
  });
});

describe("a task the person asked for", () => {
  it("is counted on the session's row and is not what the session is waiting on", async () => {
    const theirs = task(2, "check prod", 1, { by_person: true });
    const { move } = core([chat(1), theirs]);
    render(<App />);
    const tree = await rows(2);
    move(2, "running", 1);

    move(1, "waiting", 2, []);

    // Its turn ended with nothing it asked for at work: idle here, and the core queues it.
    expect(says(row(tree, "steward 1"))).toEqual({ word: "idle", shape: "pause" });
    expect(count(row(tree, "steward 1"))).toBe("1 working");
    move(1, "waiting", 3, [1]);
    expect(says(row(tree, "steward 1"))).toEqual({ word: "needs you", shape: "hand" });
  });
});

describe("what a task changing state redraws (SC-3)", () => {
  it("is its own marks and its session's state, and no other chat's", async () => {
    const { move, reports } = core([
      chat(1),
      task(2, "check prod"),
      task(3, "check staging"),
      chat(8),
      chat(9),
    ]);
    render(<App />);
    const tree = await rows(5);
    move(2, "running", 1);
    move(3, "running", 2);
    move(1, "waiting", 3, []);
    move(8, "running", 4);
    move(9, "waiting", 5, [9]);
    await waitFor(() => expect(says(row(tree, "steward 1")).word).toBe("waiting on 2 tasks"));
    drawn.marks.length = 0;

    // A task's turn ends with its report still owed: it is idle, which is one fewer working
    // below its session. Its own state and its session's are drawn again, and no other's.
    move(2, "waiting", 6, [9]);

    expect(new Set(drawn.marks)).toEqual(new Set(["idle", "waiting on 1 task"]));
    expect(count(row(tree, "steward 1"))).toBe("1 working · 1 waiting");
    drawn.marks.length = 0;

    // It reports: its own state changes, and its session's count and state are read again
    // from its tasks. The two unrelated chats are not drawn.
    await reports(2, "done");

    await waitFor(() => expect(count(row(tree, "steward 1"))).toBe("1 working · 1 done"));
    expect(new Set(drawn.marks)).toEqual(new Set(["done", "waiting on 1 task"]));
  });
});
