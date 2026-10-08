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
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { ChatDoing, Doing, Moved, OpenChat } from "./bindings";
import { setChatsListPrefs } from "./chatsListPrefs";
import type { State } from "./chatState";
import { forgetThisLaunch } from "./regions";

/**
 * **A working chat's row says in one line what it is doing, against the whole window**
 * (#1493, V100-42, V100-71): the line coming, changing, standing in for the rest of the second
 * line beside the time and giving it back, in the present while a tool runs and in the past
 * once it is back, never changing what a row is made of, and never being there for a chat
 * nothing was heard from. And what one chat's line changing draws again.
 *
 * The harness is `ChatsList.window.test.tsx`'s: the core's answers as fixtures, and its
 * events sent by hand.
 */

/** Every draw of a chat's line or of its second line's contents, and of a row, by its chat. */
const drawn = vi.hoisted(() => ({ lines: [] as number[], rows: [] as number[] }));

vi.mock("./chatDoing", async (original) => {
  const real = await original<typeof import("./chatDoing")>();
  return {
    ...real,
    // The line asks this each time it is drawn (`ChatDoingLine`), and so does what holds the
    // second line's contents (`SecondLine`).
    useDoingSaid: (session: number) => {
      drawn.lines.push(session);
      return real.useDoingSaid(session);
    },
    // And a row asks this each time it is drawn.
    chatDoingId: (session: number) => {
      drawn.rows.push(session);
      return real.chatDoingId(session);
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
type Filed = OpenChat & { workspace: string };

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

function asListed(one: Filed): OpenChat {
  const sent: OpenChat & { workspace?: string } = { ...one };
  delete sent.workspace;
  return sent;
}

const doing = (kind: string, name: string | null = null, count = 0, over = false): Doing => ({
  kind,
  name,
  count,
  over,
});
/** The same, once a hook has said its tool came back. */
const done = (kind: string, name: string | null = null, count = 0) =>
  doing(kind, name, count, true);

/** The core, holding `open` chats in two workspaces, and answering `first` for their lines. */
function core(open: Filed[], first: ChatDoing[] = []) {
  const listeners = new Map<string, number>();
  let sequence = 0;
  let tellings = 100;
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, handler);
      return 1;
    }
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return open.map(asListed);
    if (cmd === "stopping_chats") return [];
    if (cmd === "finished_tasks") return [];
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
          chats: open.filter((one) => one.workspace === name).map(asListed),
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
    if (cmd === "chat_doings") return first;
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "alerts_everywhere") return [{ plane: PLANE, alerts: [], stopped: null }];
    return null;
  });
  const send = (event: string, payload: unknown) => {
    const handler = listeners.get(event);
    if (handler === undefined) throw new Error(`the window is not listening for ${event}`);
    act(() => {
      window.__TAURI_INTERNALS__.runCallback(handler, { event, id: 1, payload });
    });
  };
  return {
    /** The core says chat `session` moved to `state`. */
    move: (session: number, state: State, queue: number[] = []) => {
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
      send("chat-moved", moved);
    },
    /** The core says what chat `session` is doing now, or that it has no line. */
    tell: (session: number, what: Doing | null, numbered?: number, plane = PLANE) => {
      tellings += 1;
      send("chat-doing", { plane, session, sequence: numbered ?? tellings, doing: what });
    },
  };
}

const section = () => screen.findByRole("tree", { name: "Chats of this project" });
const theTree = () => screen.getByRole("tree", { name: "Chats of this project" });
const rows = () => within(theTree()).getAllByRole("treeitem");
const row = (name: string) => {
  const found = rows().find((one) => one.querySelector(".session")?.textContent === name);
  if (found === undefined) throw new Error(`no row is named ${name}`);
  return found;
};
const second = (name: string) => row(name).querySelector(".line.two");
const line = (name: string) => row(name).querySelector(".chat-doing");
const said = (name: string) => line(name)?.textContent;
const word = (name: string) => row(name).querySelector(".shown-state .word")?.textContent;

/** What a row is made of: its lines, and what stands in each, by class. */
const made = (name: string) =>
  [...row(name).children].map(
    (part) => `${part.className}[${[...part.children].map((one) => one.className).join(",")}]`,
  );

/** A session, a task of it where it works, one in another workspace, and a second session. */
const four = () => [
  chat(1, "alpha"),
  chat(2, "alpha", { persona: "devops", from: taskOf(1) }),
  chat(3, "beta", { persona: "devops", from: taskOf(1) }),
  chat(4, "beta"),
];

const up = async (first: ChatDoing[] = []) => {
  const held = core(four(), first);
  render(<App />);
  await section();
  await waitFor(() => expect(rows()).toHaveLength(4));
  return held;
};

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
  drawn.lines.length = 0;
  drawn.rows.length = 0;
  act(() => setChatsListPrefs({ lines: 2 }));
});

describe("a working chat's row says what it is doing (#1493)", () => {
  it("says it on the second line, in purlis's words, and changes as the chat works", async () => {
    const { move, tell } = await up();
    move(2, "running");
    expect(line("devops 2")).toBeNull();

    tell(2, doing("thinking"));
    expect(said("devops 2")).toBe("thinking");

    tell(2, doing("command", "cargo"));
    expect(said("devops 2")).toBe("running cargo");

    tell(2, doing("reading", "state.rs", 1));
    expect(said("devops 2")).toBe("reading state.rs");
    tell(2, doing("reading", null, 3));
    expect(said("devops 2")).toBe("reading 3 files");

    // One line of it, and one only.
    expect(row("devops 2").querySelectorAll(".chat-doing")).toHaveLength(1);
    expect(line("devops 2")?.closest(".line")).toBe(second("devops 2"));
    // The whole of it on hover, where the row cuts it short.
    expect(line("devops 2")).toHaveAttribute("title", "reading 3 files");
  });

  it("says a tool in the present while it runs and in the past once it has come back", async () => {
    const { move, tell } = await up();
    move(2, "running");

    tell(2, doing("command", "cargo"));
    expect(said("devops 2")).toBe("running cargo");
    tell(2, done("command", "cargo"));
    expect(said("devops 2")).toBe("ran cargo");
    expect(line("devops 2")).toHaveAttribute("title", "ran cargo");

    for (const [kind, name, count, now, over] of [
      ["command", null, 0, "running a command", "ran a command"],
      ["editing", "Notice.tsx", 0, "editing Notice.tsx", "edited Notice.tsx"],
      ["editing", null, 0, "editing a file", "edited a file"],
      ["reading", "a.rs", 1, "reading a.rs", "read a.rs"],
      ["reading", null, 3, "reading 3 files", "read 3 files"],
      ["searching", null, 0, "searching", "searched"],
      ["fetching", null, 0, "fetching a page", "fetched a page"],
      ["helper", null, 0, "waiting on a helper", "a helper finished"],
      ["dispatching", null, 0, "dispatching a task", "dispatched a task"],
      ["asking", null, 0, "asking a question", "asked a question"],
      ["reporting", null, 0, "writing its report", "wrote its report"],
      ["tool", null, 0, "using a tool", "used a tool"],
    ] as const) {
      tell(2, doing(kind, name, count));
      expect(said("devops 2"), kind).toBe(now);
      tell(2, done(kind, name, count));
      expect(said("devops 2"), kind).toBe(over);
    }
  });

  it("stands in for where it works while the chat works, keeps how long, and gives the line back", async () => {
    const { move, tell } = await up();
    // A task in another workspace: its second line says where it works.
    move(3, "running");
    move(3, "waiting");
    expect(second("devops 3")?.querySelector(".workspace")?.textContent).toBe("beta");
    expect(second("devops 3")?.querySelector(".since")).not.toBeNull();

    move(3, "running");
    tell(3, doing("editing", "Notice.tsx"));

    expect(said("devops 3")).toBe("editing Notice.tsx");
    expect(second("devops 3")?.querySelector(".workspace")).toBeNull();
    // How long it has been at work stays, after what it is doing: "editing Notice.tsx · now".
    const together = second("devops 3")?.querySelector(".doing-and-since");
    expect([...(together?.children ?? [])].map((one) => one.className)).toEqual([
      "chat-doing",
      "since",
    ]);
    expect(together?.querySelector(".since")?.textContent).toBe("just now");

    // The turn ends: the core takes the line away, and the board says it is no longer at work.
    tell(3, null);
    move(3, "waiting");

    expect(line("devops 3")).toBeNull();
    expect(second("devops 3")?.querySelector(".doing-and-since")).toBeNull();
    expect(second("devops 3")?.querySelector(".workspace")?.textContent).toBe("beta");
    expect(second("devops 3")?.querySelector(".since")).not.toBeNull();
  });

  it("is gone the moment the board says the turn ended, whichever word lands first", async () => {
    const { move, tell } = await up();
    move(2, "running");
    tell(2, doing("command", "cargo"));
    expect(line("devops 2")).not.toBeNull();

    // The board's word first: the line the window still holds is not drawn.
    move(2, "waiting");
    expect(line("devops 2")).toBeNull();

    // And a chat that needs the person wears none either.
    move(2, "running");
    expect(line("devops 2")).not.toBeNull();
    move(2, "waiting", [2]);
    expect(line("devops 2")).toBeNull();
  });

  it("takes a closed chat's line away, so the next chat with its number does not start with it", async () => {
    const { move, tell } = await up();
    move(2, "running");
    tell(2, doing("command", "cargo"));
    expect(said("devops 2")).toBe("running cargo");

    // The chat closes: the core says it has no line.
    tell(2, null);
    move(2, "done");
    // The number is at work again before anything new was said of it.
    move(2, "running");

    expect(line("devops 2")).toBeNull();
  });

  it("is made of the same two lines whatever it says: no row grows a line or loses one", async () => {
    const { move, tell } = await up();
    const names = ["steward 1", "devops 2", "devops 3", "steward 4"];
    for (const name of names) expect(second(name), name).not.toBeNull();
    expect(made("devops 2")).toHaveLength(2);
    expect(made("devops 2")[1]).toBe("line two[]");
    expect(made("devops 3")[1]).toBe("line two[workspace]");

    for (const session of [1, 2, 3, 4]) move(session, "running");
    tell(1, doing("command", "npm"));
    tell(2, doing("editing", "a_file_name_that_is_far_wider_than_a_sidebar.tsx"));
    tell(3, doing("helper"));

    // Each row is still a first line and a second, and the second holds one thing: what the
    // chat is doing, with the time beside it where there is one.
    for (const name of ["steward 1", "devops 2", "devops 3"]) {
      expect(row(name).children, name).toHaveLength(2);
      expect(made(name)[1], name).toBe("line two[doing-and-since]");
    }
    // Nothing is added beside a row either: the list has the same items.
    expect(theTree().querySelectorAll('li[role="none"]')).toHaveLength(4);
    expect(line("devops 2")?.children).toHaveLength(1);
    expect(line("devops 2")?.querySelector("bdi")?.textContent).toBe(
      "a_file_name_that_is_far_wider_than_a_sidebar.tsx",
    );

    for (const session of [1, 2, 3]) tell(session, null);
    for (const session of [1, 2, 3, 4]) move(session, "waiting");
    for (const session of [1, 2, 3, 4]) move(session, "running");
    for (const session of [1, 2, 3, 4]) move(session, "waiting");

    // Given back: what a row is made of is what it was, with the time in its state now said.
    for (const name of names) expect(row(name).children, name).toHaveLength(2);
    expect(made("devops 2")[1]).toBe("line two[since]");
    expect(made("devops 3")[1]).toBe("line two[workspace,since]");
  });

  it("says nothing on one line: there is no second line for it to stand in", async () => {
    const { move, tell } = await up();
    act(() => setChatsListPrefs({ lines: 1 }));
    move(2, "running");
    tell(2, doing("command", "cargo"));

    expect(second("devops 2")).toBeNull();
    expect(line("devops 2")).toBeNull();
    expect(row("devops 2").children).toHaveLength(1);
  });

  it("starts from what the core holds when the window opens in the middle of a turn", async () => {
    const { move } = await up([
      { plane: PLANE, session: 2, sequence: 7, doing: doing("command", "cargo") },
    ]);
    move(2, "running");

    await waitFor(() => expect(said("devops 2")).toBe("running cargo"));
  });

  it("does not take a telling older than the one it holds, or one of another project", async () => {
    const { move, tell } = await up();
    move(2, "running");
    tell(2, doing("command", "cargo"), 50);

    tell(2, doing("thinking"), 49);
    expect(said("devops 2")).toBe("running cargo");
    tell(2, doing("searching"), 60, "/home/dev/another");
    expect(said("devops 2")).toBe("running cargo");
  });
});

describe("a chat nothing was heard from (V100-71)", () => {
  it("has no line and no placeholder, whatever is said of it", async () => {
    const { move, tell } = await up();
    move(1, "running");
    const before = made("steward 4");

    // Chat 4's harness has sent nothing: the board knows no state for it.
    expect(word("steward 4")).toMatch(/^running \(no detail from /);
    tell(4, doing("command", "cargo"));

    expect(line("steward 4")).toBeNull();
    expect(made("steward 4")).toEqual(before);
    expect(second("steward 4")?.textContent).toBe("beta");
  });

  it("has no line while it waits for its first prompt, or once it has ended", async () => {
    const { move, tell } = await up();
    tell(2, doing("thinking"));
    for (const state of ["waiting", "done", "failed"] as const) {
      move(2, state);
      expect(line("devops 2"), state).toBeNull();
    }
  });
});

describe("what a chat can make its line read as (#1493)", () => {
  it("shows a plain name as text in its own direction, and no other name at all", async () => {
    const { move, tell } = await up();
    move(2, "running");
    tell(2, doing("editing", "Notice.tsx"));
    expect(line("devops 2")?.querySelector("bdi")?.textContent).toBe("Notice.tsx");

    // The core passes no such name. Were one to arrive, the window drops it by itself: markup,
    // a space, a letter that draws as a blank, a look-alike letter, a name too long.
    for (const name of [
      '<img src=x onerror="alert(1)">',
      "needs you",
      "Done.\u3164Now\u3164press\u3164Allow",
      "\u0430dmin.rs",
      "a\u202eb",
      "a".repeat(49),
    ]) {
      tell(2, doing("editing", name));
      expect(said("devops 2"), name).toBe("editing a file");
      expect(line("devops 2")?.children, name).toHaveLength(0);
      tell(2, doing("command", name));
      expect(said("devops 2"), name).toBe("running a command");
    }
  });

  it("says nothing for a kind it has no sentence for, and no name for a kind that has none", async () => {
    const { move, tell } = await up();
    move(2, "running");
    tell(2, doing("needs you", "Allow"));
    expect(line("devops 2")).toBeNull();

    tell(2, doing("helper", "steward"));
    expect(said("devops 2")).toBe("waiting on a helper");
  });

  it("is never a button, a link or a notice: it is words on the row's own second line", async () => {
    const { move, tell } = await up();
    move(2, "running");
    tell(2, doing("command", "cargo"));

    const shown = line("devops 2");
    expect(shown?.tagName).toBe("SPAN");
    expect(shown?.querySelector("button, a, [role]")).toBeNull();
    expect(shown?.closest('[role="alert"], [role="status"], .notice')).toBeNull();
  });
});

describe("what a screen reader is told of the line (#1493)", () => {
  it("is not announced as it changes, and is the row's description on demand", async () => {
    const { move, tell } = await up();
    move(2, "running");
    tell(2, doing("command", "cargo"));

    const shown = line("devops 2");
    // Not a live region, nor inside one: it would chatter.
    expect(shown?.closest("[aria-live], [role='status'], [role='alert'], [role='log']")).toBeNull();
    // Out of the row's name, which would otherwise change under a reader several times a second.
    expect(shown).toHaveAttribute("aria-hidden", "true");
    expect(row("devops 2")).toHaveAccessibleName(/^devops 2/);
    expect(row("devops 2")).not.toHaveAccessibleName(/cargo/);
    // And said when the row's description is asked for.
    expect(row("devops 2")).toHaveAccessibleDescription("running cargo");

    tell(2, done("reading", null, 3));
    expect(row("devops 2")).toHaveAccessibleDescription("read 3 files");

    tell(2, null);
    expect(row("devops 2")).not.toHaveAccessibleDescription();
  });
});

describe("fifty working tasks (SC-3)", () => {
  it("draws again only the second line of the chat whose line changed, and no row", async () => {
    const { move, tell } = core([
      ...Array.from({ length: 50 }, (_, at) => chat(at + 1, at % 2 === 0 ? "alpha" : "beta")),
      ...Array.from({ length: 50 }, (_, at) =>
        chat(at + 51, "alpha", { persona: "devops", from: taskOf(at + 1) }),
      ),
    ]);
    // Not under StrictMode, which draws everything twice: what is counted is draws.
    renderBare(<App />);
    await section();
    await waitFor(() => expect(rows()).toHaveLength(100));
    // Every task at work, each with a line.
    for (let task = 51; task <= 100; task += 1) move(task, "running");
    for (let task = 51; task <= 100; task += 1) tell(task, doing("thinking"));
    for (let turn = 0; turn < 10; turn += 1) await act(async () => {});
    expect(theTree().querySelectorAll(".chat-doing")).toHaveLength(50);
    /** The chats whose line, or second line's contents, were drawn since the last look. */
    const lines = () => [...new Set(drawn.lines)].sort((a, b) => a - b);
    drawn.lines.length = 0;
    drawn.rows.length = 0;

    tell(60, doing("command", "cargo"));

    expect(said("devops 60")).toBe("running cargo");
    // Chat 60's line and what holds it: two draws at most, both its own.
    expect(lines()).toEqual([60]);
    expect(drawn.lines.length).toBeLessThanOrEqual(2);
    expect(drawn.rows).toEqual([]);

    // The same thing said again draws nothing at all.
    drawn.lines.length = 0;
    tell(60, doing("command", "cargo"));
    expect(drawn.lines).toEqual([]);

    // The tool comes back: its own line again, and nothing else.
    tell(60, done("command", "cargo"));
    expect(said("devops 60")).toBe("ran cargo");
    expect(lines()).toEqual([60]);
    expect(drawn.rows).toEqual([]);

    // A burst across ten chats draws those ten chats' lines, and no row.
    drawn.lines.length = 0;
    for (let task = 71; task <= 80; task += 1) tell(task, doing("editing", `f${task}.rs`));
    expect(lines()).toEqual(Array.from({ length: 10 }, (_, at) => at + 71));
    expect(drawn.lines.length).toBeLessThanOrEqual(20);
    expect(drawn.rows).toEqual([]);

    // A line taken away draws its own chat's second line again and nothing else.
    drawn.lines.length = 0;
    tell(60, null);
    expect(line("devops 60")).toBeNull();
    expect(lines()).toEqual([60]);
    expect(drawn.rows).toEqual([]);
  });
});
