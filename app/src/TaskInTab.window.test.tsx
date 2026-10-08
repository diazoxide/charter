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
import type { ChatBlocked, DispatchPending, Moved, OpenChat, VaultRefused } from "./bindings";
import type { State } from "./chatState";
import { forgetKeyboard } from "./paneKeyboard";
import { REFERENCE_TYPE } from "./references";
import { forgetThisLaunch } from "./regions";

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

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
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

/** The hand a tab wears for a chat of its own that is waiting and is not on screen. */
const handOn = (name: string) => within(tab(name)).queryByRole("img", { name: /needs? you$/ });

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
        { plane: PLANE, session: 2, shown: 8 },
        { plane: PLANE, session: 1, shown: 5 },
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

    expect(within(tab("steward 1")).getByRole("img", { name: "sweep needs you" })).toBeTruthy();
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

    expect(within(tab("steward 1")).getByRole("img", { name: "talk needs you" })).toBeTruthy();
    // And the tab's own state mark is still the session's, not the task's.
    expect(within(tab("steward 1")).queryByRole("img", { name: "waiting on you" })).toBeNull();
  });

  it("puts the hand on the tab for the session's own chat, while the tab shows a task", async () => {
    const { tree, move } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await move(1, "waiting", 10, [1]);

    expect(within(tab("steward 1")).getByRole("img", { name: "steward 1 needs you" })).toBeTruthy();
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
    expect(within(tab("steward 1")).getByRole("img", { name: "probe needs you" })).toBeTruthy();
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
    expect(held.textContent).toMatch(/^steward 1: This chat runs as steward and wants to dispatch/);
    // The pane still shows talk: nothing was switched to show the question.
    expect(onScreen()).toEqual([4]);

    await userEvent.click(within(held).getByRole("button", { name: "Allow for this chat" }));

    // The answer is the held dispatch's own, which is steward 1's: never the shown chat's.
    await waitFor(() =>
      expect(commandsOf(asked, "allow_dispatch")).toEqual([
        { plane: PLANE, id: 71, level: "chat" },
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
      name: "sweep, a task of steward 1: Dispatch to devops",
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
      name: "sweep, a task of steward 1: Vault",
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
    expect(within(tab("steward 1")).getByRole("img", { name: "steward 1 needs you" })).toBeTruthy();
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
    await waitFor(() =>
      expect(
        within(tab("steward 1")).getByRole("img", { name: "steward 1 needs you" }),
      ).toBeTruthy(),
    );
    // And a task's refusal, on a tab that is behind and whose pane is not drawn at all.
    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));
    await refuseVault(5);
    await waitFor(() =>
      expect(
        within(tab("steward 1")).getByRole("img", {
          name: /^(steward 1|sweep|talk) and \d more need you$/,
        }),
      ).toBeTruthy(),
    );

    // Answered where it is drawn: the hand for it goes.
    await userEvent.click(row(tree, "steward 1"));
    await waitFor(() => expect(onScreen()).toEqual([1]));
    const held = await screen.findByRole("status", { name: "Dispatch to devops" });
    await userEvent.click(within(held).getByRole("button", { name: "Keep blocked" }));
    await waitFor(() =>
      expect(within(tab("steward 1")).getByRole("img", { name: "sweep needs you" })).toBeTruthy(),
    );
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

  it("draws no control that ends the task", async () => {
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
    expect(ending).toEqual([]);
    // The splits are the pane's still.
    expect(screen.getAllByRole("button", { name: /^Split/ })).toHaveLength(2);
  });
});
