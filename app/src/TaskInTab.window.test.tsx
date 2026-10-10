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
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type {
  ChatBlocked,
  DispatchPending,
  FinishedTask,
  Moved,
  OpenChat,
  VaultRefused,
} from "./bindings";
import type { State } from "./chatState";
import { cardOf as cardOfRow } from "./chatCard.testkit";
import { forgetKeyboard } from "./paneKeyboard";
import { REFERENCE_TYPE } from "./references";
import { forgetThisLaunch } from "./regions";
import { stripNamed } from "./test-strips";

/**
 * **A task opens inside its session's tab** (#1486), against the whole window: pressing a task
 * swaps what the tab of the session that asked shows, and adds no tab. While a tab shows a
 * task, the pane's existing top line carries a breadcrumb and the tab's label says the task.
 *
 * The core here is a fixture: a task is a chat whose `from` says `task: true` and
 * `tab: false`, as `ChatsSection.window.test.tsx` has it.
 */

vi.mock("./SessionPane", async () => {
  const { invoke } = await import("@tauri-apps/api/core");
  const { paneDrawn } = await import("./paneKeyboard");
  return {
    /** A pane that takes the keyboard when the window gives it, as a terminal does, and sends
     *  what is typed in it to its own session, as a terminal does. */
    SessionPane: ({ plane, session }: { plane: string; session: number }) => {
      const pane = useRef<HTMLDivElement>(null);
      useEffect(() => paneDrawn(plane, session, () => pane.current?.focus()), [plane, session]);
      return (
        <div
          data-testid="pane"
          data-session={session}
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

/** A chat working in `workspace`, as the core lists it. */
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

/** A task of chat `asker`. */
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

/** A dispatch chat `session` asked for that no grant covers: held until the person answers. */
function held(session: number, id: number): DispatchPending {
  return {
    plane: PLANE,
    id,
    session,
    chat: `chat ${session}`,
    asking: "steward",
    target: "devops",
    brief: "Check why the deploy is red.",
    brief_cut: false,
    brief_lines: 1,
    levels: ["chat", "you", "project"],
    locked: null,
    never_unread: null,
    works_in: null,
    works_in_missing: false,
    allowed_in: [],
    works_with: "devops works with its own access: no vault; no hosts beyond the project's.",
    also: [],
    shown: "s0",
    task: null,
    task_cut: false,
    profile: null,
  };
}

/** A vault chat `session` was refused. */
function refusedVault(session: number): VaultRefused {
  return {
    plane: PLANE,
    session,
    vault: "prod",
    persona: "steward",
    tagged_for: "devops",
    dispatch_to: null,
    locked: null,
  };
}

/** What chat `session`'s sandbox blocked: its own work, with nothing to allow. */
function blocked(session: number): ChatBlocked {
  return {
    plane: PLANE,
    session,
    operation: "write",
    kind: "toolchain-cache",
    ours: false,
    harness: "claude",
    said: "a write to a toolchain's package cache",
    offer: "none",
    target: null,
    route: null,
    levels: [],
  };
}

/** Chat `session`'s sandbox blocked reaching `host`, which Allow names (#1508). */
function blockedOnHost(session: number, host = "registry.npmjs.org"): ChatBlocked {
  return {
    plane: PLANE,
    session,
    operation: "connect",
    kind: "host",
    ours: false,
    harness: "claude",
    said: "a connection to a host the project does not list",
    offer: "host",
    target: host,
    route: null,
    levels: ["chat", "you", "project"],
  };
}

/**
 * The core, holding `open` chats in two workspaces. It keeps what each session's tab shows
 * (`tab_shows`) on that chat, as the record does, and what it holds for the person: dispatches
 * waiting for a grant and vaults it refused, by chat.
 */
function core(open: Listed[]) {
  const asked: Asked[] = [];
  const listeners = new Map<string, number[]>();
  const dispatches = new Map<number, DispatchPending[]>();
  const refusals = new Map<number, VaultRefused[]>();
  /** The finished rows the core lists, as it reads them from its dispatch records (#1485). */
  const finished: FinishedTask[] = [];
  /** How many of the next `tab_shows` the core refuses. */
  const refusing = { shows: 0 };
  /** The tasks the core's next answer to several allows, where keeping fails part way. */
  const answering: { only?: number[] } = {};
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
    if (cmd === "open_chat_tab") {
      const one = open.find((chat) => chat.session === a.session);
      if (one?.from) one.from = { ...one.from, tab: true };
      return null;
    }
    if (cmd === "chat_in_front") {
      for (const one of open) one.in_front = one.session === a.session;
      return null;
    }
    if (cmd === "tab_shows") {
      if (refusing.shows > 0) {
        refusing.shows -= 1;
        throw new Error("the record could not be written");
      }
      const one = open.find((chat) => chat.session === a.session);
      if (one) one.shows = (a.shown as number | null) ?? null;
      return null;
    }
    if (cmd === "dispatch_grants_needed") return dispatches.get(a.session as number) ?? [];
    if (cmd === "allow_dispatch" || cmd === "keep_dispatch_blocked") {
      for (const [session, waiting] of dispatches)
        dispatches.set(
          session,
          waiting.filter((one) => one.id !== a.id),
        );
      return cmd === "allow_dispatch" ? { said: "Allowed for this chat." } : true;
    }
    if (cmd === "vault_refusals") return refusals.get(a.session as number) ?? [];
    if (cmd === "allow_refused_vault" || cmd === "keep_vault_blocked") {
      refusals.set(a.session as number, []);
      return { said: cmd === "allow_refused_vault" ? "Allowed." : "Kept blocked." };
    }
    // The core holds the name it is given and answers it: the chat's label from then on.
    if (cmd === "rename_chat") {
      const one = open.find((chat) => chat.session === a.session);
      if (one) one.label = a.label as string;
      return a.label;
    }
    if (cmd === "allow_sandbox_block_for_tasks") {
      const tasks = (a.seen as { task: number }[]).map((one) => one.task);
      const answered = answering.only ?? tasks;
      return {
        said:
          answered.length === tasks.length
            ? "Allowed for each of these tasks on its own."
            : `It was allowed for chat ${answered.join(" and ")} only, and not for the rest.`,
        answered,
      };
    }
    if (cmd === "keep_sandbox_block_for_tasks")
      return (a.seen as { task: number }[]).map((one) => one.task);
    if (cmd === "allow_sandbox_block") return { said: "Allowed for this chat." };
    if (cmd === "reference_into_chat") return { kind: "typed", text: "@src/main.rs" };
    if (cmd === "ask_chat_restart") return null;
    if (cmd === "restart_chat") {
      // The same chat on its conversation, under a new number. The core re-points what the
      // chats it asked for say of it only when the test says the list has caught up.
      const one = open.find((chat) => chat.session === a.session);
      if (!one) throw new Error(`no chat ${String(a.session)} is open`);
      one.session = (a.session as number) + 20;
      return { chat: asListed(one), notices: [], not_yet: null };
    }
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
    finished,
    refusing,
    answering,
    rowsChanged,
    /** The core says chat `session`'s stop has ended it: it is gone from what the core lists. */
    ended: async (session: number) => {
      open.splice(
        open.findIndex((chat) => chat.session === session),
        1,
      );
      await said("chat-stop", { plane: PLANE, session, phase: "stopped" });
    },
    /** The core says chat `session` moved to `state`, with `queue` asking for the person. */
    move: async (session: number, state: State, at: number, queue: number[] = []) => {
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
      };
      await said("chat-moved", moved);
    },
    /** Chat `session` asks to dispatch with no grant: the core holds it and says so. */
    holdDispatch: async (session: number, id = 70 + session) => {
      const one = held(session, id);
      dispatches.set(session, [...(dispatches.get(session) ?? []), one]);
      await said("dispatch-grant-needed", one).catch(() => undefined);
    },
    /** Chat `session` is refused a vault: the core holds it and says so. */
    refuseVault: async (session: number) => {
      const one = refusedVault(session);
      refusals.set(session, [one]);
      await said("chat-vault-refused", one).catch(() => undefined);
    },
    /** Chat `session`'s sandbox blocked something of its own. */
    block: (session: number) => said("chat-sandbox-blocked", blocked(session)),
    /** Chat `session`'s sandbox blocked reaching `host` (#1508). */
    blockHost: (session: number, host?: string) =>
      said("chat-sandbox-blocked", blockedOnHost(session, host)),
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

const rows = (tree: HTMLElement) => within(tree).getAllByRole("treeitem");

const strip = () => stripNamed("Tabs");
const tabNames = () =>
  within(strip())
    .queryAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

/** The tab of the session called `name`. */
const tab = (name: string) => {
  const found = within(strip())
    .getAllByRole("tab")
    .find((one) => one.querySelector(".tab-name")?.textContent === name);
  if (found === undefined) throw new Error(`no tab is ${name}'s`);
  return found;
};

/** The sessions the panes on screen show, left to right. */
const onScreen = () =>
  screen.queryAllByTestId("pane").map((pane) => Number(pane.getAttribute("data-session")));

/** The breadcrumb on screen, where a pane draws one. */
const crumbs = () => screen.queryByRole("navigation", { name: "Chat path" });

/** The pane that says a task it shows is not there to be drawn, where there is one. */
const away = () => screen.queryByTestId("task-away");

/** The hand a tab wears for a chat of its own that is waiting and is not on screen: a button
 *  on the tab's chip, beside the tab's own button (#1487), which goes to that chat. */
const handOn = (name: string) =>
  within(tab(name).closest(".tab") as HTMLElement).queryByRole("button", { name: /needs? you\./ });

/** What that hand says is waiting. */
const handSays = (name: string) =>
  handOn(name)
    ?.getAttribute("aria-label")
    ?.replace(/\. Go to .*$/, "");

/** A Notice on the pane, by its accessible name. */
const notice = (name: string | RegExp) => screen.queryByRole("status", { name });

/** What the breadcrumb reads, as a person reads it. */
const crumbsSay = () => crumbs()?.textContent?.replace(/\s+/g, " ").trim();

const commandsOf = (asked: Asked[], cmd: string) =>
  asked.filter((one) => one.cmd === cmd).map((one) => one.args);

/**
 * steward 1 in alpha dispatched three tasks: talk (4) and sweep (5) in alpha, probe (6) in
 * beta. talk dispatched deep (7). steward 2 is another session in alpha.
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
async function drawn(open: Listed[]) {
  const held = core(open);
  render(<App />);
  const tree = await section();
  await waitFor(() => expect(rows(tree)).toHaveLength(open.length));
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

describe("pressing a task", () => {
  it("shows it in its session's tab and adds no tab", async () => {
    const { tree, asked } = await drawn(withTasks());
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
    expect(onScreen()).toEqual([1]);

    await userEvent.click(row(tree, "talk"));

    await waitFor(() => expect(onScreen()).toEqual([4]));
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
    expect(tab("steward 1").getAttribute("aria-selected")).toBe("true");
    // The tab is not made an ordinary one: the core is told nothing of a tab for the task.
    expect(commandsOf(asked, "open_chat_tab")).toEqual([]);
    // Its row is the current one in the list.
    expect(row(tree, "talk").getAttribute("aria-current")).toBe("true");
    expect(row(tree, "steward 1").getAttribute("aria-current")).toBeNull();
    // And no row says "no tab" of a task: none of them has one, and that is how tasks are.
    expect(tree.textContent).not.toContain("no tab");
  });

  it("brings its session's tab forward when another tab is in front", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));

    await userEvent.click(row(tree, "sweep"));

    await waitFor(() => expect(onScreen()).toEqual([5]));
    expect(tab("steward 1").getAttribute("aria-selected")).toBe("true");
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
  });

  it("shows a task of a task in the tab of the session at the top", async () => {
    const { tree } = await drawn(withTasks());

    await userEvent.click(row(tree, "deep"));

    await waitFor(() => expect(onScreen()).toEqual([7]));
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
  });

  it("falls back to an ordinary tab for a task whose session has no tab in this window", async () => {
    // Chat 9 asked for it and has closed since: the task is at the top of the list.
    const { tree, asked } = await drawn([
      chat(1, "alpha"),
      chat(12, "alpha", { persona: "devops", label: "left behind", from: taskOf(9) }),
    ]);

    await userEvent.click(row(tree, "left behind"));

    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "left behind"]));
    expect(onScreen()).toEqual([12]);
    expect(commandsOf(asked, "open_chat_tab")).toEqual([{ plane: PLANE, session: 12 }]);
    expect(crumbs()).toBeNull();
  });

  it("never shows a handoff inside the tab of the chat it came from", async () => {
    // Chat 3 is a handoff from steward 1, and this window draws no tab for it (the record
    // says it has one, and the fixture's tab was closed): it is a session of its own.
    const { tree, asked } = await drawn([
      chat(1, "alpha"),
      chat(3, "alpha", {
        label: "moved work",
        from: { ...taskOf(1), task: false, tab: false },
      }),
    ]);

    await userEvent.click(row(tree, "moved work"));

    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "moved work"]));
    expect(crumbs()).toBeNull();
    expect(commandsOf(asked, "open_chat_tab")).toEqual([{ plane: PLANE, session: 3 }]);
  });
});

describe("a tab remembers which chat it shows", () => {
  it("shows the task again when the tab comes back to the front", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));
    await userEvent.click(tab("steward 1"));

    await waitFor(() => expect(onScreen()).toEqual([4]));
    expect(crumbsSay()).toContain("talk");
  });

  it("tells the core what each tab shows, and every tab comes back on it when the window is loaded again", async () => {
    const open = [...withTasks(), chat(8, "alpha", { label: "notes", from: taskOf(2) })];
    const first = core(open);
    const window = render(<App />);
    const tree = await section();
    await waitFor(() => expect(rows(tree)).toHaveLength(open.length));
    // Two tabs, each left on a task: the one in front and the one behind it.
    await userEvent.click(row(tree, "notes"));
    await waitFor(() => expect(onScreen()).toEqual([8]));
    await userEvent.click(row(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));
    await waitFor(() =>
      expect(commandsOf(first.asked, "tab_shows")).toEqual([
        { plane: PLANE, session: 2, shown: 8, beside: null },
        { plane: PLANE, session: 1, shown: 5, beside: null },
      ]),
    );
    // The chat in front is the tab's own, as it always was: the task is not a tab.
    expect(commandsOf(first.asked, "chat_in_front").at(-1)).toEqual({ plane: PLANE, session: 1 });
    window.unmount();
    forgetThisLaunch();

    // The same core: a reload, and a relaunch that put the chats back, both ask it.
    render(<App />);
    await section();

    await waitFor(() => expect(onScreen()).toEqual([5]));
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
    expect(tab("steward 1").getAttribute("aria-selected")).toBe("true");
    expect(crumbsSay()).toContain("sweep");
    // And the tab behind is on its task too.
    expect(tab("steward 2").querySelector(".tab-task")?.textContent).toBe("notes");
    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([8]));
  });

  it("says so to the core when a tab goes back to its own chat", async () => {
    const { tree, asked } = await drawn(withTasks());
    await userEvent.click(row(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));

    await userEvent.click(row(tree, "steward 1"));

    await waitFor(() => expect(onScreen()).toEqual([1]));
    await waitFor(() =>
      expect(commandsOf(asked, "tab_shows").at(-1)).toEqual({
        plane: PLANE,
        session: 1,
        shown: null,
        beside: null,
      }),
    );
  });

  it("comes back on the session's own chat when the task the record names is gone, or is not below it", async () => {
    // The record says steward 1's tab showed chat 5, which did not come back, and that
    // steward 2's showed chat 4, which is a task of steward 1 and not of steward 2.
    const open = withTasks().filter((one) => one.session !== 5);
    open[0].shows = 5;
    open[1].shows = 4;
    await drawn(open);

    expect(onScreen()).toEqual([1]);
    expect(crumbs()).toBeNull();
    expect(away()).toBeNull();
    expect(tab("steward 1").querySelector(".tab-task")).toBeNull();
    expect(tab("steward 2").querySelector(".tab-task")).toBeNull();
  });

  it("tells the core a tab shows its own chat where what the record said it showed was not put back", async () => {
    // The core takes the chat a tab shows for the one the person is looking at. A number
    // left in the record for a chat that is gone would have it take them to be looking at
    // nothing, and notify them about the session on their screen.
    const open = withTasks().filter((one) => one.session !== 5);
    open[0].shows = 5;
    open[1].shows = 4;
    const { asked } = await drawn(open);

    await waitFor(() =>
      expect(commandsOf(asked, "tab_shows")).toEqual([
        { plane: PLANE, session: 1, shown: null, beside: null },
        { plane: PLANE, session: 2, shown: null, beside: null },
      ]),
    );
    expect(open[0].shows).toBeNull();
    expect(open[1].shows).toBeNull();
  });

  it("says nothing to the core of a tab that came back on the task the record said it showed", async () => {
    const open = withTasks();
    open[0].shows = 5;
    const { asked } = await drawn(open);

    await waitFor(() => expect(onScreen()).toEqual([5]));
    expect(commandsOf(asked, "tab_shows")).toEqual([]);
  });

  it("says what a tab shows again where the core did not take it", async () => {
    const { tree, asked, refusing, open } = await drawn(withTasks());
    refusing.shows = 1;
    await userEvent.click(row(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));
    await waitFor(() => expect(commandsOf(asked, "tab_shows")).toHaveLength(1));
    expect(open[0].shows ?? null).toBeNull();

    // The next change to the tabs says it again, with what else changed.
    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));

    await waitFor(() =>
      expect(commandsOf(asked, "tab_shows")).toEqual([
        { plane: PLANE, session: 1, shown: 5, beside: null },
        { plane: PLANE, session: 1, shown: 5, beside: null },
      ]),
    );
    expect(open[0].shows).toBe(5);
  });
});

describe("a shown task that ends", () => {
  it("stays on screen as ended, with its name, how it ended and one way back, which has the keyboard", async () => {
    const { tree, ended } = await drawn(withTasks());
    await userEvent.click(row(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));

    await ended(5);

    // Nothing flipped: the tab still reads the task, the breadcrumb still says the path.
    const pane = await screen.findByTestId("task-away");
    expect(tab("steward 1").querySelector(".tab-task")?.textContent).toBe("sweep");
    expect(crumbsSay()).toBe("steward 1 › sweep · ended without a report");
    expect(pane.textContent).toContain("sweep has ended");
    // No terminal is drawn for it, so nothing typed can go anywhere.
    expect(onScreen()).toEqual([]);
    const back = within(pane).getByRole("button", { name: "Back to steward 1" });
    await waitFor(() => expect(document.activeElement).toBe(back));
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);

    await userEvent.click(back);

    await waitFor(() => expect(onScreen()).toEqual([1]));
    expect(away()).toBeNull();
    expect(crumbs()).toBeNull();
    await waitFor(() => expect(document.activeElement?.getAttribute("data-session")).toBe("1"));
  });

  it("says how it reported, for one that had", async () => {
    const open = withTasks();
    open[3].from = taskOf(1, { reported: true, outcome: "done" });
    const { tree, ended } = await drawn(open);
    await userEvent.click(row(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));

    await ended(5);

    await screen.findByTestId("task-away");
    expect(crumbsSay()).toBe("steward 1 › sweep · done");
  });

  /** A finished row of steward 1, as the core lists one for a task that ended. */
  const finishedRow = (name: string, more: Partial<FinishedTask>): FinishedTask => ({
    id: `01K6${name}`,
    asker: 1,
    name,
    persona: "devops",
    how: "done",
    outcome: "done",
    folds: true,
    report: "",
    changed: null,
    ended: "2026-10-08T12:04:30+00:00",
    place: "alpha",
    branch: null,
    reopens: true,
    not_reopened: null,
    chat: null,
    did_not_start: false,
    attempts: 0,
    waits: null,
    ...more,
  });

  it("shows the report of the task it was left on, as text, and says how it ended as its finished row does", async () => {
    // The pane last saw sweep still owing its report: the record is what says how it ended.
    const { tree, ended, finished } = await drawn(withTasks());
    await userEvent.click(row(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));

    finished.push(
      finishedRow("sweep", {
        chat: 5,
        how: "blocked",
        outcome: "blocked",
        folds: false,
        report: "The queue is locked.\n<b>Nothing</b> was changed.",
      }),
    );
    await ended(5);

    const pane = await screen.findByTestId("task-away");
    const report = await within(pane).findByRole("region", { name: "Report from sweep" });
    // Text, every character: a report is a chat's words and never markup.
    expect(report.textContent).toBe("The queue is locked.\n<b>Nothing</b> was changed.");
    expect(report.querySelector("b")).toBeNull();
    // The pane holds the report, so it no longer sends the person to the row for it.
    expect(pane.textContent).not.toContain("is on its row");

    // One vocabulary: the pane, the breadcrumb and the finished row say the same state, and
    // the core's own word is beside it where the row draws it.
    const group = await screen.findByRole("group", { name: "Finished tasks of steward 1" });
    const onRow = within(group)
      .getAllByRole("button")
      .find((one) => one.querySelector(".session")?.textContent === "sweep");
    const says = (on: Element | undefined) => ({
      word: on?.querySelector(".shown-state .word")?.textContent,
      shape: on?.querySelector(".shown-state .shape")?.getAttribute("data-shape"),
      more: on?.querySelector(".outcome")?.textContent?.trim(),
    });
    expect(says(onRow)).toEqual({ word: "failed", shape: "cross", more: "blocked" });
    expect(says(pane)).toEqual(says(onRow));
    expect(crumbsSay()).toBe("steward 1 › sweep · failed");
    expect(within(pane).getByRole("button", { name: "Back to steward 1" })).toBeTruthy();
  });

  it("never draws the report of another task of the same name, before its own row is read or after", async () => {
    // steward 1 dispatched "sweep" before, and that one failed: its row is there already.
    // The second "sweep" (chat 5) ends while a pane is left on it.
    const { tree, ended, finished, rowsChanged } = await drawn(withTasks());
    const earlier = finishedRow("sweep", {
      id: "01K6EARLIER",
      chat: 3,
      how: "failed",
      outcome: "failed",
      folds: false,
      report: "The earlier sweep failed.",
    });
    finished.push(earlier);
    await rowsChanged();
    const group = await screen.findByRole("group", { name: "Finished tasks of steward 1" });
    await userEvent.click(row(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));

    // It ends, and the core's finished rows do not hold it yet: one row is named sweep, and
    // it is the other task's.
    await ended(5);

    const pane = await screen.findByTestId("task-away");
    expect(pane.textContent).not.toContain("The earlier sweep failed.");
    expect(within(pane).queryByRole("region", { name: "Report from sweep" })).toBeNull();
    expect(pane.textContent).toContain("is on its row under steward 1 in the Chats list");
    expect(crumbsSay()).toBe("steward 1 › sweep · ended without a report");

    // Its own row arrives: its report, and how it ended.
    finished.push(finishedRow("sweep", { id: "01K6THIS", chat: 5, report: "This sweep is done." }));
    await rowsChanged();

    const report = await within(pane).findByRole("region", { name: "Report from sweep" });
    expect(report.textContent).toBe("This sweep is done.");
    expect(crumbsSay()).toBe("steward 1 › sweep · done");

    // Clear finished takes the folded rows, this task's among them. One row named sweep is
    // left, the other task's, and the pane does not take it up.
    finished.splice(0, finished.length, earlier);
    await rowsChanged();

    await waitFor(() =>
      expect(within(pane).queryByRole("region", { name: "Report from sweep" })).toBeNull(),
    );
    expect(within(group).getAllByRole("button", { name: /sweep/ }).length).toBeGreaterThan(0);
    expect(pane.textContent).not.toContain("The earlier sweep failed.");
    expect(crumbsSay()).not.toContain("failed");
  });

  it("draws no report for a row that has no chat's number: one from before this launch, or one that is not an ended task", async () => {
    const { tree, ended, finished } = await drawn(withTasks());
    await userEvent.click(row(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));

    // Reopened, its own row is cleared; what is left under that name carries no number.
    finished.push(finishedRow("sweep", { chat: null, report: "Not this pane's." }));
    await ended(5);

    const pane = await screen.findByTestId("task-away");
    await screen.findByRole("group", { name: "Finished tasks of steward 1" });
    expect(within(pane).queryByRole("region", { name: "Report from sweep" })).toBeNull();
    expect(pane.textContent).not.toContain("Not this pane's.");
  });

  it("is still what its tab shows when the tab comes back from behind", async () => {
    const { tree, ended } = await drawn(withTasks());
    await userEvent.click(row(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));
    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));

    await ended(5);
    await userEvent.click(tab("steward 1"));

    expect(await screen.findByTestId("task-away")).toBeTruthy();
    expect(crumbsSay()).toMatch(/^steward 1 › sweep · /);
  });
});

describe("a pane never shows a task without its breadcrumb", () => {
  /** Whatever is on screen: a task's terminal is there only under its breadcrumb. */
  const neverUnlabelled = (own: number) => {
    for (const session of onScreen()) if (session !== own) expect(crumbs()).not.toBeNull();
  };

  it("stops drawing a task of a task when the task between it and the session ends", async () => {
    const { tree, ended } = await drawn(withTasks());
    await userEvent.click(row(tree, "deep"));
    await waitFor(() => expect(onScreen()).toEqual([7]));

    // talk asked for deep, and talk ends. deep is still running.
    await ended(4);

    const pane = await screen.findByTestId("task-away");
    expect(onScreen()).toEqual([]);
    expect(pane.textContent).toContain("deep is still running, and this tab can no longer show it");
    // The path it had is still what the pane says, and the chat that is gone is no way.
    expect(crumbsSay()).toMatch(/^steward 1 › talk › deep · /);
    expect(within(crumbs() as HTMLElement).queryByRole("button", { name: "talk" })).toBeNull();
    expect(within(pane).getByRole("button", { name: "Back to steward 1" })).toBeTruthy();

    // Its own tab is the way to it now, and the first tab goes back to its session.
    await userEvent.click(
      within(pane).getByRole("button", { name: "Open deep in a tab of its own" }),
    );
    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "steward 2", "deep"]));
    expect(onScreen()).toEqual([7]);
    await userEvent.click(tab("steward 1"));
    await waitFor(() => expect(onScreen()).toEqual([1]));
    expect(tab("steward 1").querySelector(".tab-task")).toBeNull();
  });

  it("holds while the session is started again under a new number, whichever catches up first", async () => {
    const { tree, open, rowsChanged } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));
    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));

    // steward 1 is restarted from its tab's menu: it is chat 21 now. The tabs know at once;
    // the core's list still says talk is a task of chat 1.
    fireEvent.contextMenu(tab("steward 1"));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: /^Restart chat .*steward 1/ }),
    );
    await waitFor(() => expect(open[0].session).toBe(21));
    await userEvent.click(tab("steward 1"));

    // Between the two, the pane does not draw talk's terminal under a path it cannot say.
    await waitFor(() => expect(screen.getAllByRole("tab").length).toBeGreaterThan(0));
    neverUnlabelled(21);

    // The list catches up: talk and the rest are tasks of chat 21.
    for (const one of open) if (one.from?.chat === 1) one.from = { ...one.from, chat: 21 };
    await rowsChanged();

    await waitFor(() => expect(onScreen()).toEqual([4]));
    expect(crumbsSay()).toMatch(/› talk · /);
    expect(away()).toBeNull();
  });

  it("leaves no tab showing a task that was given a tab of its own", async () => {
    const { tree, ended } = await drawn(withTasks());
    await userEvent.click(row(tree, "deep"));
    await waitFor(() => expect(onScreen()).toEqual([7]));
    await ended(4);
    await screen.findByTestId("task-away");

    // Pressed from the list while its home is gone: it opens as a tab of its own.
    await userEvent.click(row(tree, "deep"));

    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "steward 2", "deep"]));
    await userEvent.click(tab("steward 1"));
    await waitFor(() => expect(onScreen()).toEqual([1]));
    expect(away()).toBeNull();
    expect(crumbs()).toBeNull();
  });
});

describe("the breadcrumb", () => {
  it("is not drawn while a tab shows its session's own chat", async () => {
    await drawn(withTasks());
    expect(onScreen()).toEqual([1]);
    expect(crumbs()).toBeNull();
  });

  it("says the session, the task and the task's state, in the pane's existing top line", async () => {
    const { tree, move } = await drawn(withTasks());
    const corner = () => document.querySelector(".pane-corner.at-start");
    const rowsBefore = corner()?.children.length;

    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));
    await move(4, "running", 10);

    expect(crumbsSay()).toBe("steward 1 › talk · working");
    // In the line the gauge and the harness's name are in: the pane gains no row.
    expect(crumbs()?.parentElement?.className).toBe("pane-chips");
    expect(corner()?.children.length).toBe(rowsBefore);
    // The state is the one the task's row says, by the one function.
    expect(within(row(tree, "talk")).getByText("working")).toBeTruthy();
  });

  it("goes back to the session's own chat when the session's name is pressed", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await userEvent.click(
      within(crumbs() as HTMLElement).getByRole("button", { name: "steward 1" }),
    );

    await waitFor(() => expect(onScreen()).toEqual([1]));
    expect(crumbs()).toBeNull();
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
  });

  it("shows the whole path for a task of a task, and each name in it goes to that chat", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(row(tree, "deep"));
    await waitFor(() => expect(onScreen()).toEqual([7]));

    expect(crumbsSay()).toMatch(/^steward 1 › talk › deep · /);
    // The chat shown is where the person is: it is not a way to anywhere.
    expect(within(crumbs() as HTMLElement).queryByRole("button", { name: "deep" })).toBeNull();

    await userEvent.click(within(crumbs() as HTMLElement).getByRole("button", { name: "talk" }));

    await waitFor(() => expect(onScreen()).toEqual([4]));
    expect(crumbsSay()).toMatch(/^steward 1 › talk · /);
  });

  it("says the workspace of a task that works in another one, which still opens in its asker's tab", async () => {
    const { tree } = await drawn(withTasks());

    await userEvent.click(row(tree, "probe"));

    await waitFor(() => expect(onScreen()).toEqual([6]));
    // The strip is still alpha's, where the session that asked is: beta's gained no tab.
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
    expect(crumbsSay()).toMatch(/^steward 1 › probe in beta · /);
  });

  it("says nothing of a workspace for a task that works where its session does", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));
    expect(crumbsSay()).not.toContain(" in ");
  });
});

describe("the tab's label", () => {
  it("says the session and then the task, while the tab shows a task", async () => {
    const { tree } = await drawn(withTasks());
    expect(tab("steward 1").querySelector(".tab-task")).toBeNull();

    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    const label = tab("steward 1");
    expect(label.querySelector(".tab-name")?.textContent).toBe("steward 1");
    expect(label.querySelector(".tab-task")?.textContent).toBe("talk");
    // Read aloud, the separator is the words it stands for, and the tab says what a press does.
    expect(label.textContent).toContain(", showing task talk");
    expect(label.querySelector(".tab-task-sep")?.getAttribute("aria-hidden")).toBe("true");
    expect(label.getAttribute("aria-description")).toBe("Press to go back to steward 1");
    // The state mark is the session's, and stands beside the session's name, before the task's.
    const mark = label.querySelector(".state");
    expect(
      (mark?.compareDocumentPosition(label.querySelector(".tab-task") as Node) ?? 0) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    // And the other session's tab says only its own name.
    expect(tab("steward 2").querySelector(".tab-task")).toBeNull();
    expect(tab("steward 2").getAttribute("aria-description")).toBeNull();
  });

  it("goes back to the session's own chat when the tab is pressed while it is in front", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await userEvent.click(tab("steward 1"));

    await waitFor(() => expect(onScreen()).toEqual([1]));
    expect(tab("steward 1").querySelector(".tab-task")).toBeNull();
    expect(crumbs()).toBeNull();
  });

  it("goes back at once when the tab in front is pressed from the keyboard", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    tab("steward 1").focus();
    await userEvent.keyboard("{Enter}");

    expect(onScreen()).toEqual([1]);
  });

  it("stays on the task when the tab is pressed to bring it to the front", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));
    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));

    // One press brings it forward, on the task it was left on. Only a second goes back.
    await userEvent.click(tab("steward 1"));
    await waitFor(() => expect(onScreen()).toEqual([4]));
    await new Promise((resolve) => setTimeout(resolve, 400));
    expect(onScreen()).toEqual([4]);
  });

  it("stays on the task when the tab in front is double-clicked to rename it", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await userEvent.dblClick(tab("steward 1"));

    // The name is open for editing, and it is the session's name: the tab is the session.
    const box = await within(strip()).findByRole("textbox", { name: /^Rename chat/ });
    expect((box as HTMLInputElement).value).toBe("steward 1");
    await new Promise((resolve) => setTimeout(resolve, 400));
    expect(onScreen()).toEqual([4]);
    expect(crumbsSay()).toMatch(/^steward 1 › talk · /);
  });

  it("stays on the task when the tab in front is dragged along the strip", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    // Picked up, carried past the tab beside it and put down, from the keyboard: the drag's
    // own keys are not a press of the tab.
    const dragged = tab("steward 1");
    dragged.focus();
    await userEvent.keyboard("{Shift>}[Space]{/Shift}");
    await userEvent.keyboard("{ArrowRight}");
    await userEvent.keyboard("[Space]");
    await new Promise((resolve) => setTimeout(resolve, 400));

    expect(onScreen()).toEqual([4]);
    expect(tab("steward 1").querySelector(".tab-task")?.textContent).toBe("talk");
  });
});

describe("a task that needs you and is not on screen", () => {
  it("puts the hand on its session's tab, and switches nothing", async () => {
    const { move } = await drawn(withTasks());
    expect(handOn("steward 1")).toBeNull();

    await move(5, "waiting", 10, [5]);

    expect(handSays("steward 1")).toBe("sweep needs you");
    expect(handOn("steward 2")).toBeNull();
    // Nothing moved: the session's own chat is still what is on screen.
    expect(onScreen()).toEqual([1]);
  });

  it("puts the hand on the tab it was left showing in, once that tab is not in front", async () => {
    const { tree, move } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));
    // On screen in the tab in front: the pane is what says it, and the tab wears nothing.
    await move(4, "waiting", 10, [4]);
    expect(handOn("steward 1")).toBeNull();

    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));

    expect(handSays("steward 1")).toBe("talk needs you");
    // And the tab's own state mark is still the session's, not the task's.
    expect(within(tab("steward 1")).queryByRole("img", { name: "waiting on you" })).toBeNull();
  });

  it("puts the hand on the tab for the session's own chat, while the tab shows a task", async () => {
    const { tree, move } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await move(1, "waiting", 10, [1]);

    expect(handSays("steward 1")).toBe("steward 1 needs you");
  });

  it("is gone to from the title bar's list, which switches the tab to that task", async () => {
    const { move } = await drawn(withTasks());
    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));
    await move(5, "waiting", 10, [5]);

    await userEvent.click(await screen.findByRole("button", { name: "1 chat needs you" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: /^Go to sweep/ }));

    await waitFor(() => expect(onScreen()).toEqual([5]));
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
    expect(tab("steward 1").getAttribute("aria-selected")).toBe("true");
    // It is on screen now, so the tab wears no hand for it.
    expect(handOn("steward 1")).toBeNull();
  });

  it("is gone to from the hand on a row above it in the Chats list", async () => {
    const { tree, move } = await drawn(withTasks());
    await move(7, "waiting", 10, [7]);

    const hand = row(tree, "steward 1").closest("li")?.querySelector<HTMLElement>(".rolled-up");
    if (!hand) throw new Error("steward 1 wears no hand for the task below it");
    await userEvent.click(hand);

    await waitFor(() => expect(onScreen()).toEqual([7]));
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
  });

  it("leaves the tab where it is after the prompt is answered", async () => {
    const { tree, move } = await drawn(withTasks());
    await move(5, "waiting", 10, [5]);
    await userEvent.click(row(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));

    // Answered: the task works again, and later ends its turn with nothing to ask.
    await move(5, "running", 11, []);
    await move(5, "waiting", 12, []);
    await move(1, "running", 13, []);

    expect(onScreen()).toEqual([5]);
    expect(crumbsSay()).toMatch(/^steward 1 › sweep · /);
  });

  it("is worn by its session's tab, and counted on that tab's workspace, when it works in another", async () => {
    const { move } = await drawn(withTasks());
    await move(6, "waiting", 10, [6]);

    // probe works in beta, and is a task of steward 1 in alpha: that tab, on alpha's strip.
    expect(handSays("steward 1")).toBe("probe needs you");
    // And alpha's tab on the workspace strip counts it, where beta's has no tab that wears it.
    expect(screen.getByLabelText("1 chats need you in alpha")).toBeTruthy();
    expect(screen.queryByLabelText(/need you in beta/)).toBeNull();
  });
});

describe("a Notice of a chat that is not the one its tab shows", () => {
  it("is drawn on the pane for the session's own chat while a task is shown, says whose it is, and answers for that chat", async () => {
    const { tree, asked, holdDispatch } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    // steward 1 asks to dispatch with no grant, while its tab shows talk.
    await holdDispatch(1);

    const held = await screen.findByRole("status", { name: "steward 1: Dispatch to devops" });
    expect(held.textContent).toMatch(/^steward 1: It runs as steward and wants to dispatch/);
    // The pane still shows talk: nothing was switched to show the question.
    expect(onScreen()).toEqual([4]);

    await userEvent.click(within(held).getByRole("button", { name: "Allow for this chat" }));

    // The answer is the held dispatch's own, which is steward 1's: never the shown chat's.
    await waitFor(() =>
      expect(commandsOf(asked, "allow_dispatch")).toEqual([
        { plane: PLANE, id: 71, level: "chat", also: [], shown: "s0" },
      ]),
    );
    expect(onScreen()).toEqual([4]);
  });

  it("goes to its chat on Go to it", async () => {
    const { tree, holdDispatch } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));
    await holdDispatch(1);
    const held = await screen.findByRole("status", { name: "steward 1: Dispatch to devops" });

    await userEvent.click(within(held).getByRole("button", { name: "Go to it" }));

    await waitFor(() => expect(onScreen()).toEqual([1]));
    // On its own pane it reads as it always did: no name before it, and no way to itself.
    const own = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(own.textContent).toMatch(/^This chat runs as steward and wants to dispatch/);
    expect(within(own).queryByRole("button", { name: "Go to it" })).toBeNull();
  });

  it("is drawn for a task while the tab shows the session's own chat, and names the task and its session", async () => {
    const { asked, holdDispatch } = await drawn(withTasks());
    expect(onScreen()).toEqual([1]);

    await holdDispatch(5);

    const held = await screen.findByRole("status", {
      name: "“sweep” (a task of “steward 1”): Dispatch to devops",
    });
    await userEvent.click(within(held).getByRole("button", { name: "Keep blocked" }));
    await waitFor(() =>
      expect(commandsOf(asked, "keep_dispatch_blocked")).toEqual([{ plane: PLANE, id: 75 }]),
    );
    expect(onScreen()).toEqual([1]);
  });

  it("is drawn for a vault the session's own chat was refused, and Allow is sent for that chat", async () => {
    const { tree, asked, refuseVault } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await refuseVault(1);

    const refused = await screen.findByRole("status", { name: "steward 1: Vault" });
    await userEvent.click(
      within(refused).getByRole("button", { name: "Allow steward to use this vault" }),
    );
    await waitFor(() =>
      expect(commandsOf(asked, "allow_refused_vault")).toEqual([
        { plane: PLANE, session: 1, vault: "prod" },
      ]),
    );
  });

  it("is drawn for a vault a task was refused, and Keep blocked is sent for the task", async () => {
    const { asked, refuseVault } = await drawn(withTasks());

    await refuseVault(5);

    const refused = await screen.findByRole("status", {
      name: "“sweep” (a task of “steward 1”): Vault",
    });
    await userEvent.click(within(refused).getByRole("button", { name: "Keep blocked" }));
    await waitFor(() =>
      expect(commandsOf(asked, "keep_vault_blocked")).toEqual([
        { plane: PLANE, session: 5, vault: "prod" },
      ]),
    );
  });

  it("is drawn for a sandbox block of a hidden chat, and dismissing it dismisses that chat's", async () => {
    const { tree, block } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await block(1);

    const said = await screen.findByRole("status", { name: "steward 1: Sandbox block" });
    expect(handSays("steward 1")).toBe("steward 1 needs you");
    await userEvent.click(within(said).getByRole("button", { name: "Dismiss" }));
    await waitFor(() => expect(notice(/Sandbox block$/)).toBeNull());
    expect(handOn("steward 1")).toBeNull();
  });

  it("puts the hand on the tab, in front or behind, until it is answered", async () => {
    const { tree, holdDispatch, refuseVault } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    // A held dispatch is not in the core's needs-you queue: the tab is what says it.
    await holdDispatch(1);
    await waitFor(() => expect(handSays("steward 1")).toBe("steward 1 needs you"));
    // And a task's refusal, on a tab that is behind and whose pane is not drawn at all.
    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));
    await refuseVault(5);
    await waitFor(() =>
      expect(handSays("steward 1")).toMatch(/^(steward 1|sweep|talk) and \d more need you$/),
    );

    // Answered where it is drawn: the hand for it goes.
    await userEvent.click(row(tree, "steward 1"));
    await waitFor(() => expect(onScreen()).toEqual([1]));
    const held = await screen.findByRole("status", { name: "Dispatch to devops" });
    await userEvent.click(within(held).getByRole("button", { name: "Keep blocked" }));
    await waitFor(() => expect(handSays("steward 1")).toBe("sweep needs you"));
  });

  it("does not carry what one chat's Notice was answered under another chat when the tab is switched", async () => {
    const { tree, holdDispatch } = await drawn(withTasks());
    await holdDispatch(1);
    const own = await screen.findByRole("status", { name: "Dispatch to devops" });
    await userEvent.click(within(own).getByRole("button", { name: "Allow for this chat" }));
    // steward 1's Notice says what the Allow answered, on steward 1's pane.
    await waitFor(() => expect(notice("Dispatch to devops")?.textContent).toContain("Allowed"));

    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    // talk asked for nothing and was answered nothing: its pane says nothing of a dispatch.
    expect(notice("Dispatch to devops")).toBeNull();
    // And when talk does ask, it is asked, not told what steward 1 was answered.
    await holdDispatch(4);
    const asks = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(asks.textContent).toContain("Nothing starts until you answer");
    expect(asks.textContent).not.toContain("Allowed");
  });
});

describe("typing while a task is shown", () => {
  it("goes to that task and to no other chat, and the keyboard is in its pane after the switch", async () => {
    const { tree, asked } = await drawn(withTasks());

    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    // The switch put the keyboard in the task's pane: nothing is clicked before typing.
    await waitFor(() => expect(document.activeElement?.getAttribute("data-session")).toBe("4"));
    await userEvent.keyboard("ok");

    expect(commandsOf(asked, "send_input")).toEqual([
      { plane: PLANE, session: 4, text: "o" },
      { plane: PLANE, session: 4, text: "k" },
    ]);
  });

  it("goes to the session's own chat again once the tab is back on it", async () => {
    const { tree, asked } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));
    await userEvent.click(
      within(crumbs() as HTMLElement).getByRole("button", { name: "steward 1" }),
    );
    await waitFor(() => expect(onScreen()).toEqual([1]));

    await waitFor(() => expect(document.activeElement?.getAttribute("data-session")).toBe("1"));
    await userEvent.keyboard("y");

    expect(commandsOf(asked, "send_input")).toEqual([{ plane: PLANE, session: 1, text: "y" }]);
  });

  it("reaches no chat while the task shown has ended", async () => {
    const { tree, asked, ended } = await drawn(withTasks());
    await userEvent.click(row(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));
    await ended(5);
    await screen.findByTestId("task-away");

    await userEvent.keyboard("x");

    expect(commandsOf(asked, "send_input")).toEqual([]);
  });
});

describe("a file dropped on a tab", () => {
  const reference = {
    plane: PLANE,
    workspace: "alpha",
    repo: "svc",
    piece: null,
    path: "src/main.rs",
    folder: false,
  };
  const dropOn = (target: Element) =>
    fireEvent.drop(target, {
      dataTransfer: {
        types: [REFERENCE_TYPE],
        getData: (type: string) => (type === REFERENCE_TYPE ? JSON.stringify(reference) : ""),
      },
    });

  it("goes to the task the tab shows and reads, and the sentence names that task", async () => {
    const { tree, asked } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    dropOn(tab("steward 1").closest(".tab") as Element);

    await waitFor(() =>
      expect(commandsOf(asked, "reference_into_chat").map((sent) => sent.session)).toEqual([4]),
    );
    expect(
      await screen.findByText(/Typed @src\/main\.rs into talk\. Nothing was sent\./),
    ).toBeTruthy();
  });

  it("goes to the session's own chat while the tab shows it", async () => {
    const { asked } = await drawn(withTasks());

    dropOn(tab("steward 1").closest(".tab") as Element);

    await waitFor(() =>
      expect(commandsOf(asked, "reference_into_chat").map((sent) => sent.session)).toEqual([1]),
    );
  });
});

describe("closing, while a tab shows a task", () => {
  it("offers the session's close on the tab, which says what ends, asks first and ends the session, not the task", async () => {
    const { tree, asked } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    const close = tab("steward 1").closest(".tab")?.querySelector<HTMLElement>("button.closer");
    expect(close?.getAttribute("aria-label")).toBe("End chat steward 1");
    await userEvent.click(close as HTMLElement);

    const question = await screen.findByRole("alertdialog", { name: "End chat steward 1?" });
    expect(question.textContent).toContain(
      "This tab is showing talk, a task of steward 1. Closing the tab ends steward 1, not talk.",
    );
    expect(commandsOf(asked, "close_session")).toEqual([]);
    await userEvent.click(within(question).getByRole("button", { name: "Close" }));

    await waitFor(() =>
      expect(commandsOf(asked, "close_session")).toEqual([{ plane: PLANE, session: 1 }]),
    );
    expect(tabNames()).toEqual(["steward 2"]);
  });

  it("says nothing of a task in the close of a tab that shows its own chat", async () => {
    await drawn(withTasks());
    const close = tab("steward 1").closest(".tab")?.querySelector<HTMLElement>("button.closer");
    await userEvent.click(close as HTMLElement);
    const question = await screen.findByRole("alertdialog", { name: "End chat steward 1?" });
    expect(question.textContent).not.toContain("This tab is showing");
  });

  it("draws no close on the pane, and the only controls that end the task are the breadcrumb's two", async () => {
    const { tree } = await drawn(withTasks());
    // The session's own pane has its close in its corner, as it always had.
    expect(screen.getByRole("button", { name: "End this pane's chat" })).toBeTruthy();

    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    expect(screen.queryByRole("button", { name: "End this pane's chat" })).toBeNull();
    const ending = screen
      .queryAllByRole("button")
      .map((button) => button.getAttribute("aria-label") ?? button.textContent ?? "")
      .filter((name) => /\b(end|stop|close)\b/i.test(name) && /talk/.test(name));
    // The two ways a person ends a task (#1488), in words on the breadcrumb's line: neither
    // is a close, and nothing else on screen ends the task.
    expect(ending).toEqual(["Stop and get its report: task talk", "Close now: task talk"]);
    // The splits are the pane's still.
    expect(screen.getAllByRole("button", { name: /^Split/ })).toHaveLength(2);
  });
});

/** The chat `asker` handed its work off to: a session of its own, with a tab. */
const handoffFrom = (asker: number, more: Partial<Lineage> = {}): Lineage => ({
  ...taskOf(asker),
  task: false,
  tab: true,
  ...more,
});

/** The rows of the list as `level name`, top to bottom. */
const levels = (tree: HTMLElement) =>
  rows(tree).map(
    (one) => `${one.getAttribute("aria-level")} ${one.querySelector(".session")?.textContent}`,
  );

/** The name of the tab in front. */
const inFront = () =>
  within(strip())
    .getAllByRole("tab")
    .find((one) => one.getAttribute("aria-selected") === "true")
    ?.querySelector(".tab-name")?.textContent;

describe("a handoff is a session of its own (#1492, V100-69)", () => {
  /** steward 1 asked talk (4) for a task, and handed work off to drop commons (3). */
  const moved = () => [
    chat(1, "alpha"),
    chat(3, "alpha", { label: "drop commons", from: handoffFrom(1) }),
    chat(4, "alpha", { persona: "devops", label: "talk", from: taskOf(1) }),
  ];

  /** What the card of row `name` says (#1675). */
  const cardOf = async (tree: HTMLElement, name: string) => (await cardOfRow(row(tree, name))).text;

  it("has its own tab with a close, and its own row at the top, under no chat", async () => {
    const { tree, asked } = await drawn(moved());

    expect(tabNames()).toEqual(["steward 1", "drop commons"]);
    const close = tab("drop commons").closest(".tab")?.querySelector("button.closer");
    expect(close?.getAttribute("aria-label")).toBe("End chat drop commons");
    // The task is under the chat that asked; the handoff is beside it.
    expect(levels(tree)).toEqual(["1 steward 1", "2 talk", "1 drop commons"]);
    expect(onScreen()).toEqual([1]);

    // Pressing its row brings its own tab forward. Nothing is shown inside steward 1's tab.
    await userEvent.click(row(tree, "drop commons").querySelector(".session") as HTMLElement);

    await waitFor(() => expect(onScreen()).toEqual([3]));
    expect(inFront()).toBe("drop commons");
    expect(tabNames()).toEqual(["steward 1", "drop commons"]);
    expect(crumbs()).toBeNull();
    expect(commandsOf(asked, "open_chat_tab")).toEqual([]);
    expect(commandsOf(asked, "tab_shows").filter((one) => one.shown === 3)).toEqual([]);

    // And a task of the chat it came from still opens in that chat's tab, not in this one.
    await userEvent.click(row(tree, "talk").querySelector(".session") as HTMLElement);
    await waitFor(() => expect(onScreen()).toEqual([4]));
    expect(inFront()).toBe("steward 1");
    expect(crumbsSay()).toContain("steward 1 › talk");
  });

  it("is in nobody's counts: it folds under no row and raises no hand but its own", async () => {
    const { tree, move } = await drawn([
      chat(1, "alpha"),
      chat(3, "alpha", { label: "drop commons", from: handoffFrom(1) }),
    ]);
    // Nothing is under the chat it came from: that row has nothing to fold or to sum up.
    expect(row(tree, "steward 1").getAttribute("aria-expanded")).toBeNull();
    expect(row(tree, "steward 1").closest("li")?.querySelector("button.twist")).toBeNull();
    expect(row(tree, "steward 1").querySelector(".below-summary")).toBeNull();

    await move(3, "waiting", 10, [3]);

    expect(within(row(tree, "drop commons")).getByText("needs you")).toBeTruthy();
    // The chat it came from is not waiting for anybody, on its row or on its tab.
    expect(row(tree, "steward 1").closest("li")?.querySelector("button.rolled-up")).toBeNull();
    expect(row(tree, "steward 1").getAttribute("aria-description")).toBeNull();
    expect(handOn("steward 1")).toBeNull();
    // Nor does that chat's tab offer it as one of its own chats.
    expect(within(tab("steward 1")).queryByText(/drop commons/)).toBeNull();
  });

  it("is named in the card of the chat it came from, which names that chat in its own", async () => {
    const { tree } = await drawn([
      ...moved(),
      chat(5, "alpha", { label: "release notes", from: handoffFrom(1) }),
    ]);

    // The newest first, and how many more, in the card and not on the row (#1675).
    expect(await cardOf(tree, "steward 1")).toContain("handed off to release notes and 1 more");
    expect(row(tree, "steward 1").textContent).not.toContain("handed off");
    // Each handed-off chat says where it came from, and hands nothing off itself.
    expect(await cardOf(tree, "drop commons")).toContain("from steward 1");
    expect(await cardOf(tree, "release notes")).toContain("from steward 1");
    expect(await cardOf(tree, "drop commons")).not.toContain("handed off");
    // A task says nothing of the kind: it is under the chat that asked.
    expect(await cardOf(tree, "talk")).not.toContain("from steward 1");
  });

  it("is gone to from the keyboard: the row's menu has a row for each chat the work went to", async () => {
    const { tree } = await drawn([
      ...moved(),
      chat(5, "alpha", { label: "release notes", from: handoffFrom(1) }),
    ]);

    fireEvent.contextMenu(row(tree, "steward 1"));

    // Every chat it went to, the newest first: the one the row names and the "1 more".
    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((item) => item.textContent)
        .filter((said) => said?.includes("handed off to")),
    ).toEqual([
      "Go to release notes (handed off to by steward 1)",
      "Go to drop commons (handed off to by steward 1)",
    ]);
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{Enter}");

    await waitFor(() => expect(onScreen()).toEqual([3]));
    expect(inFront()).toBe("drop commons");

    // A chat that handed nothing off has no such row.
    fireEvent.contextMenu(row(tree, "talk"));
    const other = await screen.findByRole("menu");
    expect(within(other).queryByText(/handed off to/)).toBeNull();
  });

  it("is still named in the card of the chat it came from while that chat works", async () => {
    const { tree, move } = await drawn(moved());

    await move(1, "running", 10);

    expect(within(row(tree, "steward 1")).getByText("working")).toBeTruthy();
    expect(await cardOf(tree, "steward 1")).toContain("handed off to drop commons");
    // Where it came from is always said.
    expect(await cardOf(tree, "drop commons")).toContain("from steward 1");
  });

  it("follows a rename of either chat", async () => {
    const { tree } = await drawn(moved());
    /** Renames the chat called `was`, on its tab, as a person does. */
    const rename = async (was: string, to: string) => {
      await userEvent.dblClick(tab(was));
      await within(strip()).findByRole("textbox", { name: /^Rename chat/ });
      await userEvent.keyboard(`{Control>}a{/Control}${to}{Enter}`);
      await waitFor(() => expect(tabNames()).toContain(to));
    };

    await rename("drop commons", "ship it");
    await waitFor(() => expect(row(tree, "ship it")).toBeTruthy());
    expect(await cardOf(tree, "steward 1")).toContain("handed off to ship it");

    await rename("steward 1", "lead");
    await waitFor(() => expect(row(tree, "lead")).toBeTruthy());
    expect(await cardOf(tree, "ship it")).toContain("from lead");
    expect(await cardOf(tree, "lead")).toContain("handed off to ship it");
  });
});

describe("a chat the person asked for from a tab (#1492, V100-70)", () => {
  /** In steward 1's tab the person asked devops (talk, 4); steward 1 dispatched sweep (5). */
  const asked = () => [
    chat(1, "alpha"),
    chat(2, "alpha"),
    chat(4, "alpha", { persona: "devops", label: "talk", from: taskOf(1, { by_person: true }) }),
    chat(5, "alpha", { persona: "devops", label: "sweep", from: taskOf(1) }),
  ];

  it("is a task of that session, nested under it and marked as the person's in its card", async () => {
    const { tree } = await drawn(asked());

    expect(levels(tree)).toEqual(["1 steward 1", "2 talk", "2 sweep", "1 steward 2"]);
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
    // In its card (#1675), and not on its one line.
    expect((await cardOfRow(row(tree, "talk"))).text).toContain("asked by you");
    expect(row(tree, "talk").textContent).not.toContain("asked by you");
    // A task the chat dispatched itself, and the session's own chat, are not marked.
    expect((await cardOfRow(row(tree, "sweep"))).text).not.toContain("asked by you");
    expect((await cardOfRow(row(tree, "steward 1"))).text).not.toContain("asked by you");
  });

  it("opens inside that session's tab, and its breadcrumb says whose it is after its state", async () => {
    const { tree } = await drawn(asked());

    await userEvent.click(row(tree, "talk"));

    await waitFor(() => expect(onScreen()).toEqual([4]));
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
    expect(inFront()).toBe("steward 1");
    expect(crumbsSay()).toBe("steward 1 › talk · running (no detail from claude) · asked by you");

    await userEvent.click(row(tree, "sweep"));

    await waitFor(() => expect(onScreen()).toEqual([5]));
    expect(crumbsSay()).toBe("steward 1 › sweep · running (no detail from claude)");
  });
});

describe("what a task asks, on its session's tab (#1508)", () => {
  /** The clock the questions read, moved by hand: a press too soon after a question is drawn
   *  does nothing. */
  const clock = { now: 1_000_000 };
  beforeEach(() => {
    clock.now = 1_000_000;
    vi.spyOn(Date, "now").mockImplementation(() => clock.now);
  });
  afterEach(() => vi.restoreAllMocks());
  /** Time enough for a question to settle. */
  const settle = () => {
    clock.now += 5_000;
  };

  /** What the window sends back for task `task` blocked on the npm registry. */
  const seenOn = (task: number) => ({
    task,
    operation: "connect",
    kind: "host",
    what: "host",
    target: "registry.npmjs.org",
  });

  it("names a task two levels down by its whole path, each name quoted", async () => {
    const { holdDispatch } = await drawn(withTasks());
    expect(onScreen()).toEqual([1]);

    await holdDispatch(7);

    // Reachable from the session's tab without opening the task, and named from the core's
    // record of who asked whom.
    expect(
      await screen.findByRole("status", {
        name: "“deep” (a task of “steward 1” › “talk”): Dispatch to devops",
      }),
    ).toBeTruthy();
    expect(onScreen()).toEqual([1]);
  });

  it("asks once for three tasks blocked on one host, and one answer restarts all three", async () => {
    const { asked, blockHost, move } = await drawn(withTasks());

    await blockHost(4);
    // The same host as a grant matches it: one question.
    await blockHost(5, "Registry.npmjs.org:443");
    await blockHost(6);
    settle();

    const question = await screen.findByRole("status", { name: "Sandbox block for 3 tasks" });
    expect(question.textContent).toContain("3 tasks want to reach registry.npmjs.org");
    expect(question.textContent).toContain(
      "“talk” (a task of “steward 1”), “sweep” (a task of “steward 1”) and “probe” (a task of “steward 1”)",
    );
    // One question: no task asks it again on its own.
    expect(screen.queryAllByRole("status", { name: /Sandbox block$/ })).toEqual([]);

    await userEvent.click(
      within(question).getByRole("button", { name: "Allow for these 3 tasks" }),
    );

    // Each task with the block it was shown blocked on, exactly.
    await waitFor(() =>
      expect(commandsOf(asked, "allow_sandbox_block_for_tasks")).toEqual([
        {
          plane: PLANE,
          session: 1,
          seen: [seenOn(4), { ...seenOn(5), target: "Registry.npmjs.org:443" }, seenOn(6)],
          level: "chat",
        },
      ]),
    );
    // Nothing was allowed for the session that asked them.
    expect(commandsOf(asked, "allow_sandbox_block")).toEqual([]);
    await waitFor(() => expect(notice("Sandbox block for 3 tasks")).toBeNull());
    // What the answer did only says something, so it stands behind "+N more" while the
    // pane has two that ask (#1647): still drawn, and read there.
    expect(
      screen.queryByRole("status", { name: "Sandbox block", hidden: true })?.textContent,
    ).toContain("Allowed for each");

    // Each restarts on its conversation once its turn ends.
    for (const task of [4, 5, 6]) await move(task, "waiting", 20 + task);
    await waitFor(() =>
      expect(
        commandsOf(asked, "restart_chat")
          .map((one) => one.session)
          .sort(),
      ).toEqual([4, 5, 6]),
    );
  });

  it("does nothing on a press as the question forms, where a task's own Allow was a moment ago", async () => {
    const { asked, blockHost } = await drawn(withTasks());
    await blockHost(4);
    expect(
      await screen.findByRole("status", { name: "“talk” (a task of “steward 1”): Sandbox block" }),
    ).toBeTruthy();
    settle();

    // sweep is blocked the moment before the person presses talk's own Allow: the question
    // for both is under the pointer now, and pressing it allows nothing.
    await blockHost(5);
    const question = await screen.findByRole("status", { name: "Sandbox block for 2 tasks" });
    await userEvent.click(
      within(question).getByRole("button", { name: "Allow for these 2 tasks" }),
    );
    expect(question.textContent).toContain("nothing was answered");
    await userEvent.click(within(question).getByRole("button", { name: "Keep blocked" }));
    expect(commandsOf(asked, "allow_sandbox_block_for_tasks")).toEqual([]);
    expect(commandsOf(asked, "keep_sandbox_block_for_tasks")).toEqual([]);
    expect(commandsOf(asked, "allow_sandbox_block")).toEqual([]);
  });

  it("keeps the session's own block apart, and asks it on its own", async () => {
    const { asked, blockHost } = await drawn(withTasks());

    await blockHost(1);
    await blockHost(4);
    await blockHost(5);

    expect(await screen.findByRole("status", { name: "Sandbox block for 2 tasks" })).toBeTruthy();
    const own = await screen.findByRole("status", { name: "Sandbox block" });
    await userEvent.click(within(own).getByRole("button", { name: "Allow for this chat" }));
    await waitFor(() =>
      expect(commandsOf(asked, "allow_sandbox_block")).toEqual([
        {
          plane: PLANE,
          session: 1,
          shown: { operation: "connect", kind: "host", what: "host", target: "registry.npmjs.org" },
          level: "chat",
        },
      ]),
    );
    // The tasks' question still stands: the session's answer did not reach them.
    expect(notice("Sandbox block for 2 tasks")).toBeTruthy();
    expect(commandsOf(asked, "allow_sandbox_block_for_tasks")).toEqual([]);
  });

  it("says who joined after it was shown, and an answer pressed just then grants nothing", async () => {
    const { asked, blockHost } = await drawn(withTasks());
    await blockHost(4);
    await blockHost(5);
    await screen.findByRole("status", { name: "Sandbox block for 2 tasks" });
    settle();

    // probe is blocked too, the moment before the person presses.
    await blockHost(6);
    const question = await screen.findByRole("status", { name: "Sandbox block for 3 tasks" });
    expect(question.textContent).toContain(
      "“probe” (a task of “steward 1”) joined this question after it was first shown.",
    );
    await userEvent.click(
      within(question).getByRole("button", { name: "Allow for these 3 tasks" }),
    );

    expect(question.textContent).toContain("nothing was answered");
    expect(commandsOf(asked, "allow_sandbox_block_for_tasks")).toEqual([]);

    // Read, and answered again: it is the three it now shows.
    settle();
    await userEvent.click(
      within(question).getByRole("button", { name: "Allow for these 3 tasks" }),
    );
    await waitFor(() =>
      expect(
        commandsOf(asked, "allow_sandbox_block_for_tasks").map((one) =>
          (one.seen as { task: number }[]).map((seen) => seen.task),
        ),
      ).toEqual([[4, 5, 6]]),
    );
  });

  it("puts away only the tasks an answer allowed when keeping failed part way", async () => {
    const { answering, blockHost } = await drawn(withTasks());
    await blockHost(4);
    await blockHost(5);
    const question = await screen.findByRole("status", { name: "Sandbox block for 2 tasks" });
    settle();
    answering.only = [4];

    await userEvent.click(
      within(question).getByRole("button", { name: "Allow for these 2 tasks" }),
    );

    // sweep was not allowed: its block is still asked, on its own.
    expect(
      await screen.findByRole("status", { name: "“sweep” (a task of “steward 1”): Sandbox block" }),
    ).toBeTruthy();
    expect(notice("“talk” (a task of “steward 1”): Sandbox block")).toBeNull();
    // What the answer did only says something, so it stands behind "+N more" while the
    // pane has two that ask (#1647): still drawn, and read there.
    expect(
      screen.queryByRole("status", { name: "Sandbox block", hidden: true })?.textContent,
    ).toContain("allowed for chat 4 only");
  });

  it("asks a task blocked after the answer in a new question, and Keep blocked answers each listed", async () => {
    const { asked, blockHost } = await drawn(withTasks());
    await blockHost(4);
    await blockHost(5);
    const question = await screen.findByRole("status", { name: "Sandbox block for 2 tasks" });
    settle();

    await userEvent.click(within(question).getByRole("button", { name: "Keep blocked" }));
    await waitFor(() =>
      expect(commandsOf(asked, "keep_sandbox_block_for_tasks")).toEqual([
        { plane: PLANE, session: 1, seen: [seenOn(4), seenOn(5)] },
      ]),
    );
    await waitFor(() => expect(notice("Sandbox block for 2 tasks")).toBeNull());
    expect(screen.queryAllByRole("status", { name: /Sandbox block/ })).toEqual([]);

    // probe is blocked afterwards: it is asked on its own, and was answered nothing.
    await blockHost(6);
    expect(
      await screen.findByRole("status", {
        name: "“probe” (a task of “steward 1”): Sandbox block",
      }),
    ).toBeTruthy();
    expect(commandsOf(asked, "allow_sandbox_block_for_tasks")).toEqual([]);
  });
});
