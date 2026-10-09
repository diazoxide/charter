import { StrictMode, useEffect, useRef } from "react";
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
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { FinishedTask, Moved, OpenChat, Shown } from "./bindings";
import type { State } from "./chatState";
import { forgetKeyboard } from "./paneKeyboard";
import { forgetThisLaunch } from "./regions";

/**
 * **A task stopped on its harness's permission prompt is said where the person is**
 * (reported 2026-10-09), against the whole window and a core that is a fixture, as
 * `TabChip.window.test.tsx` has it: a task is a chat whose `from` says `task: true`.
 *
 * The core says a chat's permission prompt the moment its hook holds it (`asks-changed`): the
 * session's tab wears the hand for it at once, and the session's pane says whose it is and
 * what it asks, with Show the task. Nothing here answers it.
 */

vi.mock("./SessionPane", async () => {
  const { invoke } = await import("@tauri-apps/api/core");
  const { paneDrawn } = await import("./paneKeyboard");
  return {
    SessionPane: ({ plane, session }: { plane: string; session: number }) => {
      const pane = useRef<HTMLDivElement>(null);
      useEffect(() => paneDrawn(plane, session, () => pane.current?.focus()), [plane, session]);
      return (
        <div
          data-testid="pane"
          data-session={session}
          data-chat-keyboard=""
          tabIndex={-1}
          ref={pane}
          onKeyDown={(event) => void invoke("send_input", { plane, session, text: event.key })}
        >
          session {session}
        </div>
      );
    },
  };
});

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";

type Lineage = NonNullable<OpenChat["from"]>;
type Listed = OpenChat & { workspace: string };

function chat(session: number, workspace: string, more: Partial<OpenChat> = {}): Listed {
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

function taskOf(asker: number, more: Partial<Lineage> = {}): Lineage {
  return {
    chat: asker,
    name: `steward ${asker}`,
    workspace: "alpha",
    task: true,
    tab: false,
    reported: false,
    unreported: false,
    ...more,
  };
}

function asListed(one: Listed): OpenChat {
  const sent: OpenChat & { workspace?: string } = { ...one };
  delete sent.workspace;
  return sent;
}

type Asked = { cmd: string; args: Record<string, unknown> };

function core(open: Listed[], finished: FinishedTask[] = []) {
  const asked: Asked[] = [];
  const listeners = new Map<string, number[]>();
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: a });
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, [...(listeners.get(event) ?? []), handler]);
      return 1;
    }
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return open.map(asListed);
    if (cmd === "tab_shows") {
      const one = open.find((chat) => chat.session === a.session);
      if (one) one.shows = (a.shown as number | null) ?? null;
      return null;
    }
    if (cmd === "dispatch_grants_needed") return [];
    // The Brief panel's read (#1494): this core holds no dispatch records.
    if (cmd === "task_brief") throw "This project holds no record of that task.";
    // The question a task asking its asker is paused on (#1551).
    if (cmd === "task_question") {
      const one = open.find((chat) => chat.session === a.session);
      return one?.from?.asking === true
        ? { task: one.label, asked: one.from.name, number: 2, question: "Which branch?" }
        : null;
    }
    if (cmd === "vault_refusals") return [];
    if (cmd === "owed_restarts") return [];
    if (cmd === "finished_tasks") return [...finished];
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
  const said = async (event: string, payload: unknown) => {
    const handlers = listeners.get(event) ?? [];
    if (handlers.length === 0) throw new Error(`the window is not listening for ${event}`);
    await act(async () => {
      for (const handler of handlers)
        window.__TAURI_INTERNALS__.runCallback(handler, { event, id: 1, payload });
      await Promise.resolve();
    });
  };
  /** The core says the rows changed, and the window reads its list again. */
  const rowsChanged = () =>
    said("plane-changed", {
      plane: PLANE,
      changes: [{ kind: "chats", workspace: null, persona: null, path: "" }],
      answers: [{ answer: "sidebar" }],
    });
  return {
    asked,
    open,
    rowsChanged,
    said,
    /** Task `session` reported `outcome` to the chat that asked, and the core says so. */
    reported: async (session: number, outcome: string) => {
      const one = open.find((chat) => chat.session === session);
      if (!one?.from) throw new Error(`chat ${session} is no task`);
      one.from = { ...one.from, reported: true, outcome };
      await rowsChanged();
    },
    /** Task `session` ended at its report: its chat is gone from the list, and `row` is its
     *  finished row under the chat that asked. */
    finishes: async (session: number, row: FinishedTask) => {
      open.splice(
        open.findIndex((chat) => chat.session === session),
        1,
      );
      finished.push(row);
      await said("chat-stop", { plane: PLANE, session, phase: "stopped" });
      await rowsChanged();
    },
    /** The core says chat `session`'s stop has ended it: it is gone from what the core lists. */
    ended: async (session: number) => {
      open.splice(
        open.findIndex((chat) => chat.session === session),
        1,
      );
      await said("chat-stop", { plane: PLANE, session, phase: "stopped" });
    },
    /** The core says chat `session` moved to `state`, with `queue` asking for the person. */
    move: async (
      session: number,
      state: State,
      at: number,
      queue: number[] = [],
      more: Partial<Moved> = {},
    ) => {
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
      await said("chat-moved", moved);
    },
  };
}

const section = () => screen.findByRole("tree", { name: "Chats of this project" });
const treeRows = (tree: HTMLElement) => within(tree).getAllByRole("treeitem");
const strip = () => screen.getByRole("tablist", { name: "Tabs" });

/** The tab of the session called `name`. */
const tab = (name: string) => {
  const found = within(strip())
    .getAllByRole("tab")
    .find((one) => one.querySelector(".tab-name")?.textContent === name);
  if (found === undefined) throw new Error(`no tab is ${name}'s`);
  return found;
};

/** The cell of the strip that session's tab is drawn in: the tab, its chip and its close. */
const cell = (name: string) => tab(name).closest<HTMLElement>(".tab") as HTMLElement;

/** The hand on a session's chip, where it wears one. */
const hand = (name: string) => within(cell(name)).queryByRole("button", { name: /needs? you\./ });

/** The sessions the panes on screen show, left to right. */
const onScreen = () =>
  screen.queryAllByTestId("pane").map((pane) => Number(pane.getAttribute("data-session")));

const pane = (session: number) => {
  const found = screen.queryAllByTestId("pane").find((one) => one.dataset.session === `${session}`);
  if (found === undefined) throw new Error(`no pane shows ${session}`);
  return found;
};

/**
 * steward 1 in alpha dispatched three tasks: talk (4) and sweep (5) in alpha, probe (6) in
 * beta. talk dispatched deep (7). steward 2 is another session in alpha, with no tasks.
 */
const withTasks = () => [
  chat(1, "alpha"),
  chat(2, "alpha"),
  chat(4, "alpha", { persona: "devops", label: "talk", from: taskOf(1) }),
  chat(5, "alpha", { persona: "devops", label: "sweep", from: taskOf(1) }),
  chat(6, "beta", { persona: "devops", label: "probe", from: taskOf(1) }),
  chat(7, "alpha", { persona: "devops", label: "deep", from: taskOf(4, { name: "talk" }) }),
];

/** The window, drawn, with every chat of `open` listed. */
async function drawn(open: Listed[], finished: FinishedTask[] = []) {
  const held = core(open, finished);
  render(<App />);
  const tree = await section();
  await waitFor(() => expect(treeRows(tree)).toHaveLength(open.length));
  return { ...held, tree };
}

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
  forgetKeyboard();
});
afterEach(() => {
  cleanup();
  clearMocks();
});

/** Chat `session`'s harness asked the person's permission to run `line`, held on its hook. */
function prompt(session: number, line: string, ask = `ask-${session}`): Shown {
  return {
    session,
    ask,
    says: line,
    options: [
      { id: "allow", label: "Allow", allows: true },
      { id: "deny", label: "Deny", allows: false },
    ],
  };
}

/** The core says the project's whole list of held prompts. */
const asks = (said: (event: string, payload: unknown) => Promise<void>, held: Shown[]) =>
  said("asks-changed", { plane: PLANE, asks: held });

/** The Notice on a pane about chat `session`'s prompt, if one is drawn. */
const promptNotice = (session: number) =>
  document.querySelector<HTMLElement>(`[data-cause^="task-permission:${session}:"]`);

describe("a task stopped on a permission prompt", () => {
  it("puts the hand on its session's tab the moment its hook holds the prompt", async () => {
    const { said } = await drawn(withTasks());
    expect(hand("steward 1")).toBeNull();

    // The task's state has not moved: its harness has said nothing but the prompt.
    await asks(said, [prompt(5, "Run purlis persona where")]);

    await waitFor(() =>
      expect(hand("steward 1")?.getAttribute("aria-label")).toBe("sweep needs you. Go to sweep"),
    );
  });

  it("is said on its session's pane, whose it is and what it asks, and Show the task goes there", async () => {
    const { said } = await drawn(withTasks());
    await waitFor(() => expect(onScreen()).toEqual([1]));

    await asks(said, [prompt(5, "Run purlis persona where")]);

    const notice = await waitFor(() => {
      const found = promptNotice(5);
      if (found === null) throw new Error("no Notice for sweep's prompt");
      return found;
    });
    expect(notice.textContent).toContain(
      "“sweep” (a task of “steward 1”) is waiting on you for a permission: “Run purlis persona where”",
    );
    // It answers nothing: its only way out goes to the task, where the prompt is answered.
    const buttons = within(notice)
      .getAllByRole("button")
      .map((one) => one.textContent);
    expect(buttons).toEqual(["Show the task"]);

    fireEvent.click(within(notice).getByRole("button", { name: "Show the task" }));

    await waitFor(() => expect(onScreen()).toEqual([5]));
    expect(document.activeElement).toBe(pane(5));
    // On screen, the prompt is in its own pane: nothing says it a second time.
    expect(promptNotice(5)).toBeNull();
  });

  it("goes, with the hand, once the prompt is answered", async () => {
    const { said } = await drawn(withTasks());
    await asks(said, [prompt(5, "Run purlis persona where")]);
    await waitFor(() => expect(promptNotice(5)).not.toBeNull());

    await asks(said, []);

    await waitFor(() => expect(promptNotice(5)).toBeNull());
    expect(hand("steward 1")).toBeNull();
  });

  it("draws what the harness asks as text, never as markup", async () => {
    const { said } = await drawn(withTasks());

    await asks(said, [prompt(5, 'Run echo "<img src=x onerror=alert(1)>"')]);

    const notice = await waitFor(() => {
      const found = promptNotice(5);
      if (found === null) throw new Error("no Notice for sweep's prompt");
      return found;
    });
    expect(notice.querySelector("img")).toBeNull();
    expect(notice.textContent).toContain("<img src=x onerror=alert(1)>");
  });

  it("stays after purlis's hold of the prompt ends, from the board, until the task goes on", async () => {
    // #1601: purlis holds a permission prompt on the hook for about a minute. Past it the
    // harness's own prompt decides in the task's pane, and the person who comes back later
    // still has to be told the task waits on them.
    const { said, move } = await drawn(withTasks());
    await waitFor(() => expect(onScreen()).toEqual([1]));
    await move(5, "running", 1);
    await asks(said, [prompt(5, "Run purlis persona where")]);
    await waitFor(() => expect(promptNotice(5)).not.toBeNull());

    // The harness shows its own prompt and says it asks; purlis's hold ends.
    await move(5, "waiting", 2, [5], { asking: true });
    await asks(said, []);

    const notice = await waitFor(() => {
      const found = promptNotice(5);
      if (found === null) throw new Error("no Notice for sweep's prompt");
      return found;
    });
    // Without the prompt's words, which only the hold carried.
    expect(notice.textContent).toBe(
      "“sweep” (a task of “steward 1”) is stopped on a prompt and waiting on you. Answer it in its own pane.Show the task",
    );
    expect(
      within(notice)
        .getAllByRole("button")
        .map((one) => one.textContent),
    ).toEqual(["Show the task"]);
    expect(hand("steward 1")).not.toBeNull();

    // Answered in its pane: a tool of its came back and the core has it running again.
    await move(5, "running", 3);

    await waitFor(() => expect(promptNotice(5)).toBeNull());
  });

  it("goes when the task's turn ends or the task ends", async () => {
    const { said, move, ended } = await drawn(withTasks());
    await move(5, "waiting", 1, [5], { asking: true });
    await move(4, "waiting", 2, [4, 5], { asking: true });
    await waitFor(() => expect(promptNotice(5)).not.toBeNull());
    await waitFor(() => expect(promptNotice(4)).not.toBeNull());

    // Its turn ends: it waits on the person as any idle chat does, stopped on no prompt.
    await move(5, "waiting", 3, [4, 5]);
    await waitFor(() => expect(promptNotice(5)).toBeNull());

    await ended(4);
    await said("chat-moved", {
      plane: PLANE,
      session: 4,
      state: "done",
      needs_you: false,
      queue: [5],
      moved_at: 4,
      sequence: 4,
      reports: [],
      refusals: [],
      children: [],
    } satisfies Moved);
    await waitFor(() => expect(promptNotice(4)).toBeNull());
  });

  it("names the session's own chat by a field, whatever its name says", async () => {
    // #1601 item 4: the button was picked by looking for " (a task of " in the name.
    const open = withTasks();
    open[0] = chat(1, "alpha", { label: "notes (a task of “ops”)" });
    const { said } = await drawn(open);
    await asks(said, [prompt(5, "Run purlis persona where")]);
    const toTask = await waitFor(() => {
      const found = promptNotice(5);
      if (found === null) throw new Error("no Notice for sweep's prompt");
      return found;
    });
    fireEvent.click(within(toTask).getByRole("button", { name: "Show the task" }));
    await waitFor(() => expect(onScreen()).toEqual([5]));

    // Now the session's own chat is the one off screen, and it stops on a prompt.
    await asks(said, [prompt(1, "Run cargo test")]);

    const notice = await waitFor(() => {
      const found = promptNotice(1);
      if (found === null) throw new Error("no Notice for the session's own prompt");
      return found;
    });
    expect(
      within(notice)
        .getAllByRole("button")
        .map((one) => one.textContent),
    ).toEqual(["Show it"]);
  });
});
