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
import type { Moved, OpenChat } from "./bindings";
import type { State } from "./chatState";
import { forgetKeyboard } from "./paneKeyboard";
import { forgetThisLaunch } from "./regions";
import { GRACE_MS, REST_MS } from "./tabTasks";

/**
 * **A session's tab wears a chip of its tasks, which opens the menu to switch between them**
 * (#1487), against the whole window and a core that is a fixture, as
 * `TaskInTab.window.test.tsx` has it: a task is a chat whose `from` says `task: true`.
 *
 * What a pointer does over the real strip, and how wide a tab is, are the scenario spec's
 * (`e2e/specs/tab-chip.e2e.ts`): jsdom lays nothing out.
 */

/** Which tabs' chips were drawn, by their session's number: the render count (SC-3). */
const chips = vi.hoisted(() => ({ drawn: [] as number[] }));

vi.mock("./tabTasks", async (original) => {
  const real = await original<typeof import("./tabTasks")>();
  return {
    ...real,
    // A chip counts once each time it is drawn, and nothing else counts.
    countsOf: (...args: Parameters<typeof real.countsOf>) => {
      chips.drawn.push(args[0][0]?.session ?? -1);
      return real.countsOf(...args);
    },
  };
});

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

function core(open: Listed[]) {
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
    if (cmd === "vault_refusals") return [];
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
    /** Task `session` reported `outcome` to the chat that asked, and the core says so. */
    reported: async (session: number, outcome: string) => {
      const one = open.find((chat) => chat.session === session);
      if (!one?.from) throw new Error(`chat ${session} is no task`);
      one.from = { ...one.from, reported: true, outcome };
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
  };
}

const section = () => screen.findByRole("tree", { name: "Chats of this project" });
const treeRows = (tree: HTMLElement) => within(tree).getAllByRole("treeitem");
const treeRow = (tree: HTMLElement, name: string) => {
  const found = treeRows(tree).find((one) => one.querySelector(".session")?.textContent === name);
  if (found === undefined) throw new Error(`no row is named ${name}`);
  return found;
};

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

/** The chip on a session's tab: the button that says its counts and opens its menu. */
const chip = (name: string) =>
  within(cell(name)).queryByRole("button", { name: new RegExp(`^Tasks of ${name}`) });

/** The chip, which is there. */
const theChip = (name: string) => {
  const found = chip(name);
  if (found === null) throw new Error(`${name}'s tab wears no chip`);
  return found;
};

/**
 * A press of a chip, as a click and nothing before it. `userEvent` sends a pointer going down
 * first, at 0,0 where jsdom lays everything out, and the window's region dividers take that
 * as a press on a divider and take the keyboard: a thing no real pointer on a chip does.
 */
const press = (on: Element) => fireEvent.click(on);

/** The hand on a session's chip, where it wears one. */
const hand = (name: string) => within(cell(name)).queryByRole("button", { name: /needs? you\./ });

/** The menu that is open, if one is. */
const menu = () => screen.queryByRole("menu", { name: /^Tasks of / });

/** The open menu's rows, as a person reads each. */
const lines = () =>
  within(menu() as HTMLElement)
    .getAllByRole("menuitem")
    .map((item) => item.textContent?.replace(/\s+/g, " ").trim());

const line = (name: string) => {
  const found = within(menu() as HTMLElement)
    .getAllByRole("menuitem")
    .find((item) => item.querySelector(".name")?.textContent === name);
  if (found === undefined) throw new Error(`the menu has no line for ${name}`);
  return found;
};

/** The sessions the panes on screen show, left to right. */
const onScreen = () =>
  screen.queryAllByTestId("pane").map((pane) => Number(pane.getAttribute("data-session")));

const pane = (session: number) => {
  const found = screen.queryAllByTestId("pane").find((one) => one.dataset.session === `${session}`);
  if (found === undefined) throw new Error(`no pane shows ${session}`);
  return found;
};

const commandsOf = (asked: Asked[], cmd: string) =>
  asked.filter((one) => one.cmd === cmd).map((one) => one.args);

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
async function drawn(open: Listed[]) {
  const held = core(open);
  render(<App />);
  const tree = await section();
  await waitFor(() => expect(treeRows(tree)).toHaveLength(open.length));
  return { ...held, tree };
}

/** A key pressed with the chord the window's own keys take here: Ctrl+Shift, off a Mac. */
const chord = (key: string, code?: string) =>
  fireEvent.keyDown(document.activeElement ?? document.body, {
    key,
    code,
    ctrlKey: true,
    shiftKey: true,
  });

/** Only the clocks a rest and a grace run on, so the window's own promises still settle. */
const fakeClocks = () =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  });

const pass = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

/** The pointer comes onto `on`, as a mouse does. */
const over = (on: Element, at = { clientX: 10, clientY: 10 }) =>
  fireEvent.pointerEnter(on, { pointerType: "mouse", ...at });
const off = (on: Element) => fireEvent.pointerLeave(on, { pointerType: "mouse" });

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
  forgetKeyboard();
  chips.drawn.length = 0;
});
afterEach(() => {
  vi.useRealTimers();
  cleanup();
  clearMocks();
});

describe("the chip on a session's tab", () => {
  it("is not worn by a tab with no tasks, which is drawn as it always was", async () => {
    // What a tab's cell holds in a window where nothing has a task.
    await drawn([chat(1, "alpha"), chat(2, "alpha")]);
    const shape = (name: string) =>
      [...cell(name).children].map((child) => `${child.tagName}.${child.className}`);
    const before = shape("steward 2");
    const said = tab("steward 2").textContent;
    cleanup();
    clearMocks();

    await drawn(withTasks());

    expect(chip("steward 2")).toBeNull();
    expect(cell("steward 2").querySelector(".tab-tasks")).toBeNull();
    expect(shape("steward 2")).toEqual(before);
    expect(tab("steward 2").textContent).toBe(said);
    // And the tab beside it, which has tasks, wears one.
    expect(chip("steward 1")).not.toBeNull();
  });

  it("counts its tasks by how they stand, each count with its state's shape", async () => {
    const { move, reported } = await drawn(withTasks());
    await move(4, "running", 1);
    await move(5, "running", 2);
    await reported(6, "failed");
    await reported(7, "done");

    const worn = theChip("steward 1");
    expect(worn.getAttribute("aria-label")).toBe("Tasks of steward 1: 2 working, 1 failed, 1 done");
    expect(
      [...worn.querySelectorAll("[data-count]")].map(
        (count) => `${count.getAttribute("data-count")} ${count.textContent}`,
      ),
    ).toEqual(["working 2", "failed 1", "done 1"]);
  });

  it("draws nothing for a count of none", async () => {
    await drawn(withTasks());

    const worn = theChip("steward 1");
    expect(worn.getAttribute("aria-label")).toBe("Tasks of steward 1: 4 working");
    expect(worn.querySelectorAll("[data-count]")).toHaveLength(1);
  });

  it("moves its counts as its tasks move, with nothing pressed", async () => {
    const { move, reported, ended } = await drawn(withTasks());
    expect(theChip("steward 1").getAttribute("aria-label")).toBe("Tasks of steward 1: 4 working");

    await reported(5, "done");
    expect(theChip("steward 1").getAttribute("aria-label")).toBe(
      "Tasks of steward 1: 3 working, 1 done",
    );

    // A task whose program ended while it still owed its report: ended without one.
    await move(6, "failed", 3);
    expect(theChip("steward 1").getAttribute("aria-label")).toBe(
      "Tasks of steward 1: 2 working, 1 failed, 1 done",
    );

    // And one that is gone from the list is counted no more.
    await ended(5);
    await waitFor(() =>
      expect(theChip("steward 1").getAttribute("aria-label")).toBe(
        "Tasks of steward 1: 2 working, 1 failed",
      ),
    );
  });

  it("wears the hand for a task that needs you off screen, and a press goes to that task", async () => {
    const { move } = await drawn(withTasks());
    expect(hand("steward 1")).toBeNull();

    await move(5, "waiting", 1, [5]);

    const worn = hand("steward 1");
    expect(worn?.getAttribute("aria-label")).toBe("sweep needs you. Go to sweep");
    // The plain mark the tab's own button wore is gone: the hand is the chip's, and pressed.
    expect(within(tab("steward 1")).queryByRole("img", { name: /needs you/ })).toBeNull();

    await userEvent.click(worn as HTMLElement);

    await waitFor(() => expect(onScreen()).toEqual([5]));
    expect(document.activeElement).toBe(pane(5));
    // It is on screen now, so the tab wears no hand for it. And the tab stays on it.
    expect(hand("steward 1")).toBeNull();
    await move(5, "running", 2, []);
    expect(onScreen()).toEqual([5]);
  });

  it("goes to the one that has waited longest, and says how many more wait", async () => {
    const { move } = await drawn(withTasks());
    await move(6, "waiting", 1, [6]);
    await move(4, "waiting", 2, [6, 4]);

    expect(hand("steward 1")?.getAttribute("aria-label")).toBe(
      "probe and 1 more need you. Go to probe",
    );
    await userEvent.click(hand("steward 1") as HTMLElement);

    await waitFor(() => expect(onScreen()).toEqual([6]));
    expect(hand("steward 1")?.getAttribute("aria-label")).toBe("talk needs you. Go to talk");
  });

  it("is still worn, with a line for it, by a tab showing a task that has ended", async () => {
    const { tree, ended } = await drawn([
      chat(1, "alpha"),
      chat(4, "alpha", { persona: "devops", label: "talk", from: taskOf(1) }),
    ]);
    await userEvent.click(treeRow(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await ended(4);
    await waitFor(() => expect(screen.queryByTestId("task-away")).not.toBeNull());

    // Its program ended owing its report: it failed, and is counted so.
    expect(theChip("steward 1").getAttribute("aria-label")).toBe("Tasks of steward 1: 1 failed");
    press(theChip("steward 1"));
    expect(lines()).toEqual([
      "steward 1 running (no detail from claude)",
      "talk, shown now ended without a report",
    ]);
    expect(line("talk").getAttribute("aria-current")).toBe("true");
  });
});

describe("the menu a chip opens", () => {
  it("lists the session's own chat first, then its tasks under who asked for them", async () => {
    const { move } = await drawn(withTasks());
    await move(1, "waiting", 1);
    await move(4, "running", 2);

    press(theChip("steward 1"));

    expect(lines()).toEqual([
      "steward 1, shown now idle",
      "talk working",
      "deep running (no detail from claude)",
      "sweep running (no detail from claude)",
      "probe in beta running (no detail from claude)",
    ]);
    const levels = within(menu() as HTMLElement)
      .getAllByRole("menuitem")
      .map((item) => item.getAttribute("data-level"));
    expect(levels).toEqual(["1", "2", "3", "2", "2"]);
    // Each wears its persona's mark, and the chat the tab shows now is marked.
    expect(line("steward 1").getAttribute("aria-current")).toBe("true");
    expect(line("talk").getAttribute("aria-current")).toBeNull();
    expect(line("talk").querySelector(".shown-state .word")?.textContent).toBe("working");
  });

  it("says how long a chat has been in its state, only where the window saw it begin", async () => {
    const { move } = await drawn(withTasks());
    await move(4, "running", 1);
    await move(5, "running", 2);
    fakeClocks();
    // The first word about talk told the window nothing of when it began. Its next does.
    await move(4, "waiting", 3);
    pass(125_000);

    press(theChip("steward 1"));

    expect(line("talk").querySelector(".since")?.textContent).toBe("2m");
    expect(line("sweep").querySelector(".since")).toBeNull();
    expect(line("probe").querySelector(".since")).toBeNull();
  });

  it("switches the tab to the row picked, closes, and puts the keyboard in that chat", async () => {
    const { asked } = await drawn(withTasks());
    press(theChip("steward 1"));

    await userEvent.click(line("sweep"));

    await waitFor(() => expect(onScreen()).toEqual([5]));
    expect(menu()).toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(pane(5)));
    // No tab was added, and the core was told of none.
    expect(within(strip()).getAllByRole("tab")).toHaveLength(2);
    expect(commandsOf(asked, "open_chat_tab")).toEqual([]);

    // And the menu marks where the tab is now.
    press(theChip("steward 1"));
    expect(line("sweep").getAttribute("aria-current")).toBe("true");
    expect(line("steward 1").getAttribute("aria-current")).toBeNull();
  });

  it("goes back to the session's own chat from its row", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(treeRow(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    press(theChip("steward 1"));
    await userEvent.click(line("steward 1"));

    await waitFor(() => expect(onScreen()).toEqual([1]));
    await waitFor(() => expect(document.activeElement).toBe(pane(1)));
  });

  it("folds done and cancelled into Finished (n), and leaves a failure a row of its own", async () => {
    const { reported } = await drawn(withTasks());
    await reported(5, "done");
    await reported(6, "cancelled");
    await reported(7, "failed");

    press(theChip("steward 1"));

    expect(lines()).toEqual([
      "steward 1, shown now running (no detail from claude)",
      "talk running (no detail from claude)",
      "deep failed",
      "Finished (2)",
    ]);
    const fold = within(menu() as HTMLElement).getByRole("menuitem", { name: "Finished (2)" });
    expect(fold.getAttribute("aria-expanded")).toBe("false");

    // Unfolding keeps the menu, and a finished task can still be gone to.
    await userEvent.click(fold);
    expect(lines()).toEqual([
      "steward 1, shown now running (no detail from claude)",
      "talk running (no detail from claude)",
      "deep failed",
      "Finished (2)",
      "sweep done",
      "probe in beta cancelled",
    ]);
    await userEvent.click(line("sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));
  });

  it("opens the fold by itself where the chat on screen is in it", async () => {
    const { tree, reported } = await drawn(withTasks());
    await userEvent.click(treeRow(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));
    await reported(5, "done");

    press(theChip("steward 1"));

    expect(line("sweep").getAttribute("aria-current")).toBe("true");
  });
});

describe("opening and closing the menu", () => {
  it("opens on a press of the chip, and closes on a second", async () => {
    await drawn(withTasks());

    press(theChip("steward 1"));
    expect(menu()).not.toBeNull();
    expect(theChip("steward 1").getAttribute("aria-expanded")).toBe("true");

    press(theChip("steward 1"));
    expect(menu()).toBeNull();
  });

  it("opens when the pointer has rested on the chip, and not a moment before", async () => {
    await drawn(withTasks());
    pane(1).focus();
    fakeClocks();

    over(theChip("steward 1"));
    pass(REST_MS - 1);
    expect(menu()).toBeNull();

    pass(1);
    expect(menu()).not.toBeNull();
    // It took no keyboard: whoever was typing in the chat goes on typing there.
    pass(50);
    expect(document.activeElement).toBe(pane(1));
  });

  it("never opens for a pointer that passes over the tab and the chip", async () => {
    await drawn(withTasks());
    fakeClocks();

    over(tab("steward 1"));
    pass(40);
    off(tab("steward 1"));
    over(theChip("steward 1"));
    pass(60);
    off(theChip("steward 1"));
    over(tab("steward 2"));
    pass(5_000);

    expect(menu()).toBeNull();
  });

  it("does not open for a pointer resting on the tab itself", async () => {
    await drawn(withTasks());
    fakeClocks();

    over(tab("steward 1"));
    pass(5_000);

    expect(menu()).toBeNull();
  });

  it("counts a rest from where the pointer stopped, not from where it came on", async () => {
    await drawn(withTasks());
    fakeClocks();
    const on = theChip("steward 1");

    over(on, { clientX: 10, clientY: 10 });
    pass(REST_MS - 50);
    fireEvent.pointerMove(on, { pointerType: "mouse", clientX: 30, clientY: 10 });
    pass(REST_MS - 50);
    expect(menu()).toBeNull();

    // A tremor is not a move: the rest it is in goes on.
    fireEvent.pointerMove(on, { pointerType: "mouse", clientX: 32, clientY: 11 });
    pass(50);
    expect(menu()).not.toBeNull();
  });

  it("closes a moment after the pointer has left both the chip and the menu", async () => {
    await drawn(withTasks());
    pane(1).focus();
    fakeClocks();
    over(theChip("steward 1"));
    pass(REST_MS);
    expect(menu()).not.toBeNull();

    // From the chip across the gap to the menu: it stays.
    off(theChip("steward 1"));
    pass(GRACE_MS - 1);
    over(menu() as HTMLElement);
    pass(5_000);
    expect(menu()).not.toBeNull();

    // Off the menu, and back in time: it stays. Off for good: it goes.
    off(menu() as HTMLElement);
    pass(GRACE_MS - 1);
    over(theChip("steward 1"));
    pass(GRACE_MS);
    expect(menu()).not.toBeNull();
    off(theChip("steward 1"));
    pass(GRACE_MS - 1);
    expect(menu()).not.toBeNull();
    pass(1);
    expect(menu()).toBeNull();
    // The keyboard is where it was all along.
    pass(50);
    expect(document.activeElement).toBe(pane(1));
  });

  it("keeps a menu the rest opened when the chip is then pressed", async () => {
    await drawn(withTasks());
    fakeClocks();
    over(theChip("steward 1"));
    pass(REST_MS);

    press(theChip("steward 1"));

    expect(menu()).not.toBeNull();
    // It has the keyboard now: its rows are walked with the arrows.
    expect(menu()?.contains(document.activeElement)).toBe(true);
  });

  it("closes on Escape, and the keyboard goes back to where it was", async () => {
    await drawn(withTasks());
    pane(1).focus();
    press(theChip("steward 1"));
    expect(menu()).not.toBeNull();

    await userEvent.keyboard("{Escape}");

    expect(menu()).toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(pane(1)));
  });

  it("closes on a press outside it", async () => {
    const { tree } = await drawn(withTasks());
    press(theChip("steward 1"));
    expect(menu()).not.toBeNull();

    await userEvent.click(tree);

    await waitFor(() => expect(menu()).toBeNull());
  });

  it("keeps an Escape that closes a rested menu from the chat that has the keyboard", async () => {
    const { asked } = await drawn(withTasks());
    pane(1).focus();
    fakeClocks();
    over(theChip("steward 1"));
    pass(REST_MS);
    pass(10);
    expect(menu()).not.toBeNull();

    fireEvent.keyDown(pane(1), { key: "Escape" });
    pass(10);

    expect(menu()).toBeNull();
    expect(commandsOf(asked, "send_input")).toEqual([]);
  });
});

describe("the keys for the chats inside a tab", () => {
  it("opens the tab's menu from the keyboard, on the chat it shows, and every row is reached", async () => {
    await drawn(withTasks());
    pane(1).focus();

    chord("J");

    await waitFor(() => expect(menu()).not.toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(line("steward 1")));
    // Down the rows, and Enter goes to the one the keyboard is on.
    await userEvent.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(line("talk"));
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}");
    expect(document.activeElement).toBe(line("probe"));
    await userEvent.keyboard("{Enter}");

    await waitFor(() => expect(onScreen()).toEqual([6]));
    expect(menu()).toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(pane(6)));
  });

  it("finds a row by the letters of its name", async () => {
    await drawn(withTasks());
    pane(1).focus();
    chord("J");
    await waitFor(() => expect(document.activeElement).toBe(line("steward 1")));

    await userEvent.keyboard("sw");

    expect(document.activeElement).toBe(line("sweep"));
  });

  it("gives the keyboard back to the chat when the menu it opened is left with Escape", async () => {
    await drawn(withTasks());
    pane(1).focus();
    chord("J");
    await waitFor(() => expect(document.activeElement).toBe(line("steward 1")));

    await userEvent.keyboard("{Escape}");

    expect(menu()).toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(pane(1)));
  });

  it("opens a tab's menu with Down on the tab", async () => {
    await drawn(withTasks());
    tab("steward 1").focus();

    await userEvent.keyboard("{ArrowDown}");

    await waitFor(() => expect(menu()).not.toBeNull());
    // And Down on a tab with no tasks opens nothing.
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(menu()).toBeNull());
    await userEvent.click(tab("steward 2"));
    tab("steward 2").focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(menu()).toBeNull();
  });

  it("goes to the next and the previous chat in the tab, round its ends, and back to its own", async () => {
    await drawn(withTasks());
    pane(1).focus();

    chord("}", "BracketRight");
    await waitFor(() => expect(onScreen()).toEqual([4]));
    await waitFor(() => expect(document.activeElement).toBe(pane(4)));
    chord("}", "BracketRight");
    await waitFor(() => expect(onScreen()).toEqual([7]));

    chord("{", "BracketLeft");
    await waitFor(() => expect(onScreen()).toEqual([4]));
    chord("{", "BracketLeft");
    await waitFor(() => expect(onScreen()).toEqual([1]));
    // Round the end: before the session's own chat is the last task.
    chord("{", "BracketLeft");
    await waitFor(() => expect(onScreen()).toEqual([6]));

    chord("H");
    await waitFor(() => expect(onScreen()).toEqual([1]));
    await waitFor(() => expect(document.activeElement).toBe(pane(1)));
    // No tab was added on the way.
    expect(within(strip()).getAllByRole("tab")).toHaveLength(2);
  });

  it("takes nothing from a chat: the same letters without Shift reach its terminal", async () => {
    const { asked } = await drawn(withTasks());
    pane(1).focus();

    for (const key of ["j", "h", "[", "]"]) {
      const reached = fireEvent.keyDown(pane(1), { key, ctrlKey: true });
      // Not prevented, which is what reaching the shell means.
      expect(reached).toBe(true);
    }

    expect(commandsOf(asked, "send_input").map((one) => one.text)).toEqual(["j", "h", "[", "]"]);
    expect(onScreen()).toEqual([1]);
    expect(menu()).toBeNull();
  });

  it("does nothing, and reaches no chat, on a tab with no tasks", async () => {
    const { asked } = await drawn(withTasks());
    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));
    pane(2).focus();

    chord("J");
    chord("}", "BracketRight");
    chord("H");

    expect(menu()).toBeNull();
    expect(onScreen()).toEqual([2]);
    expect(commandsOf(asked, "send_input")).toEqual([]);
  });
});

describe("a strip of many tabs", () => {
  /** Thirty sessions, each with one task. */
  const many = () =>
    Array.from({ length: 30 }, (_, at) => at + 1).flatMap((session) => [
      chat(session, "alpha"),
      chat(100 + session, "alpha", {
        persona: "devops",
        label: `task of ${session}`,
        from: taskOf(session),
      }),
    ]);

  async function settled() {
    const held = await drawn(many());
    await waitFor(() => expect(theChip("steward 30")).toBeTruthy());
    for (let turn = 0; turn < 10; turn += 1) await act(async () => {});
    chips.drawn.length = 0;
    return held;
  }

  it("redraws one tab's chip when one of its tasks changes state, and no other tab's", async () => {
    const { move, reported } = await settled();

    // A turn begins: the task is working, where nothing had been heard of it.
    await move(105, "running", 1);
    expect(new Set(chips.drawn)).toEqual(new Set([5]));
    expect(chips.drawn.length).toBeLessThanOrEqual(2);

    // A move that changes nothing a chip says draws no chip at all.
    chips.drawn.length = 0;
    await move(105, "running", 2);
    expect(chips.drawn).toEqual([]);

    // It starts waiting for the person: the queue is the project's, and still only the one
    // chip is drawn, for its hand.
    await move(105, "waiting", 3, [105]);
    expect(new Set(chips.drawn)).toEqual(new Set([5]));
    expect(hand("steward 5")).not.toBeNull();

    // It is answered and its turn ends: the hand goes, and one chip is drawn for that.
    chips.drawn.length = 0;
    await move(105, "waiting", 4, []);
    expect(new Set(chips.drawn)).toEqual(new Set([5]));
    expect(hand("steward 5")).toBeNull();

    // It reports: the list is read again, every tab's rows are built again, and the chips are
    // held on what they say, so one is drawn.
    chips.drawn.length = 0;
    await reported(105, "done");
    expect(new Set(chips.drawn)).toEqual(new Set([5]));
    expect(theChip("steward 5").getAttribute("aria-label")).toBe("Tasks of steward 5: 1 done");
    expect(theChip("steward 6").getAttribute("aria-label")).toBe("Tasks of steward 6: 1 working");
  });
});
