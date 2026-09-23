import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { Moved, OpenChat } from "./bindings";

/** What the picker draws. One profile, so picking is one click. */
const START_OPTIONS = {
  profiles: [
    {
      name: "claude",
      kind: "claude",
      shown: "claude",
      source: "built-in",
      is_default: true,
      approval: null,
    },
  ],
  refused: [],
  personas: ["steward"],
  persona: "steward",
  ignore_fix: null,
  declares_none: true,
};

/** Opens a chat the way the operator does now: New tab, then a row, then Start.
 *
 *  A harness starts only once a row is picked (ADR 0022) — the dialog shows even when one
 *  profile is available, because skipping it would bring back the harness nobody picked on
 *  a one-harness machine. Every test that wants a session goes through here, which is also
 *  what keeps that rule from being quietly removed: take the picker out of the path and
 *  every one of these fails.
 */
async function openAChat() {
  await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
  await userEvent.click(await screen.findByRole("button", { name: "Start" }));
}

/** What Tauri's own mocks put on the window: the registry `listen` hands its callback to,
 *  which is how a test fires an event the app is listening for. */
declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

/**
 * The window over a whole day: closed to the tray with everything still running, quit with a
 * warning about what that ends, and opened again on the chats it was left on.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
});

type Asked = { cmd: string; args: unknown };

/** A chat the core says it has open, with everything not under test left plain. */
function chat(one: Partial<OpenChat> & { session: number }): OpenChat {
  return {
    name: `ide.${one.session}`,
    cwd: "/home/dev/plane/workspaces/ide",
    harness: "claude",
    in_front: false,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
    pinned: false,
    ...one,
  };
}

/** The core, answering with `open` as the chats it already has. */
function core(
  open: OpenChat[] = [],
  wouldNot: [string, string][] = [],
  /** What each chat is doing, as the hooks would have reported it. */
  states: Moved[] = [],
): {
  asked: Asked[];
  /** Resolves once this core has answered both commands the window's `settled` is made of. */
  settled: Promise<unknown>;
  /** Fires the event the app is sent when something asks it to quit. */
  askToQuit: () => Promise<void>;
  /** Fires one `chat-moved`, the way the core pushes one. */
  move: (moved: Moved) => void;
} {
  const asked: Asked[] = [];
  const listeners = new Map<string, number>();
  let opened = 0;
  /** One-shot: resolved the moment this core answers `cmd`. */
  const answering = new Map<string, () => void>();
  const answered = (cmd: string) =>
    new Promise<void>((it) => answering.set(cmd, it)).then(() => answering.delete(cmd));
  /**
   * When the window is settled, as a fact to await rather than a thing to watch for
   * (charter-app#138).
   *
   * `App` calls itself settled once the restore is over and every project has reported what
   * it holds, and those two are `planes_to_restore` and `chats_that_would_not_start` — the
   * last answers of the two chains a launch starts. Both are this mock's to give, so a test
   * can await the answer going out instead of polling the window until it shows a sign of
   * having received it, and `act` finishes the rest.
   *
   * That matters because a poll carries a deadline and an awaited promise does not. The
   * deadline was one second, nothing in the quit path is timing-dependent, and on a run
   * building eighteen jsdom environments at once one second was not always enough — so the
   * test reported "the window did not quit" when what had happened was "the machine was
   * busy". A budget for a whole test belongs to the runner, which already has one.
   */
  const settled = Promise.all([
    answered("planes_to_restore"),
    answered("chats_that_would_not_start"),
  ]);
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    answering.get(cmd)?.();
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, handler);
      return 1;
    }
    if (cmd === "plane_at_launch")
      return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "start_chat") return { session: ++opened };
    if (cmd === "opened_chats") return open;
    if (cmd === "chat_states") return states;
    if (cmd === "chats_that_would_not_start") return wouldNot;
    if (cmd === "open_session") return open.length + ++opened;
    return null;
  });
  return {
    asked,
    settled,
    move: (moved: Moved) => {
      const handler = listeners.get("chat-moved");
      if (handler === undefined) throw new Error("the window is not listening for moves");
      window.__TAURI_INTERNALS__.runCallback(handler, {
        event: "chat-moved",
        id: 1,
        payload: moved,
      });
    },
    askToQuit: async () => {
      const handler = listeners.get("quit-asked");
      if (handler === undefined) throw new Error("the window is not listening for a quit");
      window.__TAURI_INTERNALS__.runCallback(handler, {
        event: "quit-asked",
        id: 1,
        payload: null,
      });
      await vi.waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    },
  };
}

/**
 * Runs the window forward until something the core did is done, and no further.
 *
 * Each turn is one `act`: React commits whatever the last answer queued, and the microtask
 * queue is handed back so the next answer can arrive. The loop ends the moment the promise
 * the test named has resolved, and one more turn after it carries the window through what
 * that answer left queued.
 *
 * **It has no deadline of its own, and that is the point** (charter-app#138). A `waitFor`
 * does — one second by default — so a test written with one is racing the machine: under a
 * suite that builds forty jsdom environments at once, the window that had not quit yet was
 * reported as a window that would not quit. Nothing in that path is timing-dependent; only
 * the deadline was. Here the thing awaited is a fact the mock itself produced rather than a
 * sign a poll hoped to catch, and the only budget left is the runner's own — which is a
 * backstop, not a limit anything is expected to approach.
 */
async function untilTheCoreHas(done: Promise<unknown>) {
  let has = false;
  void done.then(() => (has = true));
  while (!has) await act(async () => {});
  await act(async () => {});
}

const tabs = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .getAllByRole("tab")
    // The NAME a tab carries, not everything drawn in it: a tab also says what its chat is
    // doing, and these tests are about which tabs exist.
    .map((tab) => tab.querySelector(".tab-name")?.textContent);
const panes = () => screen.getAllByTestId("pane").map((pane) => pane.textContent);
const of = (cmd: string, asked: Asked[]) => asked.filter((one) => one.cmd === cmd);

describe("what the window does with the chats the core already has", () => {
  it("shows a tab for each one instead of ending it", async () => {
    // This is the relaunch: the core put the record back before there was a window, so the
    // window's job is to draw what is already running — never to sweep it away.
    const { asked } = core([chat({ session: 7 }), chat({ session: 8 })]);

    render(<App />);

    await vi.waitFor(() => expect(tabs()).toEqual(["ide.7", "ide.8"]));
    expect(of("close_session", asked)).toEqual([]);
  });

  it("shows the one that was in front when the app was quit", async () => {
    core([chat({ session: 7 }), chat({ session: 8, in_front: true })]);

    render(<App />);

    await vi.waitFor(() => expect(panes()).toEqual(["session 8"]));
  });

  it("says a chat was resumed, and by which conversation", async () => {
    core([chat({ session: 7, resumed: "11111111-2222-4333-8444-555555555555", in_front: true })]);

    render(<App />);

    expect(await screen.findByText(/resumed/i)).toHaveTextContent("11111111");
  });

  it("says a chat came back as a new one, and why", async () => {
    // The honest half: a Codex chat has no conversation the app could have recorded, and the
    // window says that rather than letting it look like the chat it was.
    core([
      chat({
        session: 7,
        harness: "codex",
        fresh: "no conversation was recorded for it",
        in_front: true,
      }),
    ]);

    render(<App />);

    expect(await screen.findByText(/new chat/i)).toHaveTextContent(
      "no conversation was recorded for it",
    );
  });

  it("says nothing about a shell that was not resumed", async () => {
    // Every chat is a shell until the harness picker lands, and a shell has no conversation
    // to bring back. Explaining that on every relaunch, forever, is noise about the normal
    // case — the note is for a harness that could have been resumed and was not.
    core([
      chat({
        session: 7,
        harness: null,
        fresh: "charter has not measured how this program resumes",
        in_front: true,
      }),
    ]);

    render(<App />);

    await vi.waitFor(() => expect(tabs()).toHaveLength(1));
    expect(screen.queryByText(/new chat/i)).not.toBeInTheDocument();
  });

  it("names the chats this launch could not start, and says they are still recorded", async () => {
    // A chat whose directory has moved would otherwise just be a tab that is quietly not
    // there — and the operator has no way to know it is still coming back.
    core([chat({ session: 7, in_front: true })], [["ide.9", "no such file or directory"]]);

    render(<App />);

    const said = await screen.findByText(/did not start/i);
    expect(said).toHaveTextContent("ide.9");
    expect(said).toHaveTextContent("no such file or directory");
    expect(said).toHaveTextContent(/still recorded/i);
  });

  it("says nothing about either when a chat is one the operator just opened", async () => {
    const { asked } = core();
    render(<App />);

    await openAChat();

    expect(screen.queryByText(/resumed/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/new chat/i)).not.toBeInTheDocument();
    expect(of("start_chat", asked)).toHaveLength(1);
  });

  it("tells the core which chat is in front, so the next quit records it", async () => {
    const { asked } = core([chat({ session: 7, in_front: true }), chat({ session: 8 })]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toEqual(["ide.7", "ide.8"]));

    await userEvent.click(
      within(screen.getByRole("tablist", { name: "Tabs" })).getAllByRole("tab")[1],
    );

    await vi.waitFor(() =>
      expect(of("chat_in_front", asked).slice(-1)[0]?.args).toEqual({
        plane: "/home/dev/plane",
        session: 8,
      }),
    );
  });
});

/** What the tab for `name` says its chat is doing. */
function stateShown(name: string): string | null | undefined {
  const tab = within(screen.getByRole("tablist", { name: "Tabs" }))
    .getAllByRole("tab")
    .find((one) => one.querySelector(".tab-name")?.textContent === name);
  return tab?.querySelector("[data-state]")?.getAttribute("data-state");
}

describe("what the window is told about the chats", () => {
  /** A move, as the core pushes it: one chat, in one plane. */
  function moving(plane: string, session: number, state: string, queue: number[] = []): Moved {
    return { plane, session, state, needs_you: queue.includes(session), queue, moved_at: 1 };
  }

  it("takes a move in the plane it is showing", async () => {
    const { move } = core([chat({ session: 7, name: "ide.7" })]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toEqual(["ide.7"]));

    move(moving("/home/dev/plane", 7, "waiting", [7]));

    await vi.waitFor(() => expect(stateShown("ide.7")).toBe("waiting"));
  });

  it("leaves a chat alone when the move belongs to another plane", async () => {
    // Every plane numbers its chats from one, so "session 7 is waiting" is half a name. A
    // process holding two projects would otherwise paint one project's state onto the
    // other's chat 7 — and a chat that is waiting for you has no next event to correct it.
    //
    // The second move is what makes this a test rather than a hope: it is for THIS plane and
    // a different chat, so waiting for it proves the window had finished with the foreign one.
    const { move } = core([
      chat({ session: 7, name: "ide.7" }),
      chat({ session: 8, name: "ide.8" }),
    ]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toEqual(["ide.7", "ide.8"]));

    move(moving("/home/dev/another-plane", 7, "waiting", [7]));
    move(moving("/home/dev/plane", 8, "running"));

    await vi.waitFor(() => expect(stateShown("ide.8")).toBe("running"));
    expect(stateShown("ide.7")).toBe("unknown");
  });
});

describe("being asked to quit", () => {
  it("names every session it is about to end", async () => {
    const { askToQuit } = core([chat({ session: 7 }), chat({ session: 8, harness: null })]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toHaveLength(2));

    await askToQuit();

    const dialog = within(screen.getByRole("dialog"));
    expect(dialog.getByText(/2 sessions will be ended/)).toBeInTheDocument();
    // `getAllBy`: a chat that reports no state is named twice on purpose — once in the list
    // of what is ending, and once in the sentence saying charter cannot tell if it is
    // mid-turn. This test is about the list.
    expect(within(dialog.getByRole("list")).getByText("ide.7")).toBeInTheDocument();
    expect(within(dialog.getByRole("list")).getByText("ide.8")).toBeInTheDocument();
  });

  /** What a hook would have reported, as the core hands it to the window. */
  function doing(session: number, state: string, needsYou = false): Moved {
    return {
      plane: "/home/dev/plane",
      session,
      state,
      needs_you: needsYou,
      queue: needsYou ? [session] : [],
      moved_at: 1,
    };
  }

  it("says which session is mid-turn, now that a hook can tell it", async () => {
    // This is the sentence M1.3 was written to replace. Until hooks landed the dialog said
    // charter "cannot yet tell"; a harness's own `UserPromptSubmit` now says so (spec
    // decision 3), and nothing is inferred from what the session printed (ADR 0018).
    const { askToQuit } = core(
      [chat({ session: 7, name: "ide.7" }), chat({ session: 8, name: "ide.8" })],
      [],
      [doing(7, "running"), doing(8, "waiting")],
    );
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toHaveLength(2));

    await askToQuit();

    const dialog = within(screen.getByRole("dialog"));
    expect(dialog.getByText(/ide\.7 is mid-turn and will be interrupted/)).toBeInTheDocument();
    expect(dialog.queryByText(/cannot yet tell/i)).not.toBeInTheDocument();
  });

  it("says plainly when nothing is mid-turn", async () => {
    const { askToQuit } = core([chat({ session: 7 })], [], [doing(7, "waiting")]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toHaveLength(1));

    await askToQuit();

    expect(
      within(screen.getByRole("dialog")).getByText("No session is mid-turn."),
    ).toBeInTheDocument();
  });

  it("still admits it cannot tell for a harness that reports no state", async () => {
    // The honest half of what M1.7 said, kept for the case that still deserves it. A Codex
    // chat reports nothing, and folding it into "no session is mid-turn" would be the app
    // claiming something it cannot see.
    const { askToQuit } = core([chat({ session: 7, name: "ide.7", harness: "codex" })]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toHaveLength(1));

    await askToQuit();

    expect(
      within(screen.getByRole("dialog")).getByText(
        /ide\.7 reports no state, so charter cannot tell whether it is mid-turn/,
      ),
    ).toBeInTheDocument();
  });

  it("quits when that is the answer", async () => {
    const { asked, askToQuit } = core([chat({ session: 7 })]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toHaveLength(1));
    await askToQuit();

    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: /quit/i }),
    );

    expect(of("quit", asked)).toHaveLength(1);
  });

  it("stays open when the answer is no, and ends nothing", async () => {
    const { asked, askToQuit } = core([chat({ session: 7 })]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toHaveLength(1));
    await askToQuit();

    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: /cancel/i }),
    );

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(of("quit", asked)).toEqual([]);
    expect(of("close_session", asked)).toEqual([]);
    // And the core is told, so the next Cmd-Q warns again instead of quitting outright.
    expect(of("quit_cancelled", asked)).toHaveLength(1);
  });

  it("takes Escape for the same answer as Cancel, and ends nothing", async () => {
    // The warning is a real modal now (`docs/ui-primitives.md`), and a modal with no way out
    // on the keyboard is the one thing a modal must not be. Escape gives the answer Cancel
    // gives — including telling the core, so the next Cmd-Q warns again rather than going
    // straight out.
    const { asked, askToQuit } = core([chat({ session: 7 })]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toHaveLength(1));
    await askToQuit();

    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(of("quit", asked)).toEqual([]);
    expect(of("close_session", asked)).toEqual([]);
    expect(of("quit_cancelled", asked)).toHaveLength(1);
  });

  it("warns rather than quitting while it is still finding out what is open", async () => {
    // A launch answers `opened_chats` after the window is already interactive. Quitting on
    // "no tabs yet" would end fifty chats the window had not drawn.
    const asked: Asked[] = [];
    const listeners = new Map<string, number>();
    let letTheChatsArrive = () => {};
    mockIPC(async (cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "plugin:event|listen") {
        const { event, handler } = args as { event: string; handler: number };
        listeners.set(event, handler);
        return 1;
      }
      if (cmd === "plane_at_launch")
        return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd !== "opened_chats") return null;
      await new Promise<void>((arrive) => (letTheChatsArrive = arrive));
      return [chat({ session: 7 })];
    });
    render(<App />);
    await vi.waitFor(() => expect(listeners.has("quit-asked")).toBe(true));

    window.__TAURI_INTERNALS__.runCallback(listeners.get("quit-asked") as number, {
      event: "quit-asked",
      id: 1,
      payload: null,
    });

    await vi.waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    expect(of("quit", asked)).toEqual([]);
    letTheChatsArrive();
  });

  it("quits straight away when there is nothing to end", async () => {
    // A warning listing nothing is a dialog in the way.
    const { asked, settled } = core();
    render(<App />);
    // And settled: the window asks its plane first and what it has open second, so "no tabs"
    // is only "nothing to end" once that second answer is back.
    //
    // **The precondition is awaited, not polled** (charter-app#138): `untilTheCoreHas` says
    // why, and `core`'s `settled` says which two answers the window's own `settled` is made
    // of. What the next line fires at is a window that has committed both, as a fact rather
    // than as something a poll caught in time.
    await untilTheCoreHas(settled);
    expect(screen.getByTestId("empty-window")).toBeInTheDocument();

    const listen = of("plugin:event|listen", asked).find(
      (one) => (one.args as { event: string }).event === "quit-asked",
    );
    window.__TAURI_INTERNALS__.runCallback((listen?.args as { handler: number }).handler, {
      event: "quit-asked",
      id: 1,
      payload: null,
    });

    // **Asserted, not awaited.** The window's answer to a quit is synchronous — the listener
    // reads the ref a layout effect filled and asks the core in the same turn — so `quit` is
    // in `asked` by the time `runCallback` returns. Waiting for it bought nothing when the
    // window quit, and when the window warned there was never going to be anything to wait
    // for: the wait just spent a second before saying so, in a sentence about a timeout
    // rather than about the window.
    expect(of("quit", asked)).toHaveLength(1);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
