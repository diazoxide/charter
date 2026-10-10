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
import type { FinishedTask, InboxUpdate, Moved, Need, OpenChat, UpdateNoted } from "./bindings";
import type { State } from "./chatState";
import { setChatsListPrefs } from "./chatsListPrefs";
import { forgetThisLaunch } from "./regions";
import { stripNamed } from "./test-strips";

/**
 * **While you were away: one summary in place of a pile of Notices** (#1514, V100-65), against
 * the whole window and a core that is a fixture: the chats it lists, the finished rows it reads
 * from its dispatch records, and the moves it sends.
 *
 * Away is the window losing the focus, being hidden, or getting no input, and back is it
 * getting the focus, being shown, or the first input again. The clock is faked (`Date` only,
 * so the window's own timers and the waits here still run).
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane" data-session={session}>
      session {session}
    </div>
  ),
}));

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";
/** A second project in the same window. */
const TWO = "/home/dev/two";

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

function task(session: number, label: string, asker = 1): OpenChat {
  return chat(session, {
    label,
    persona: "devops",
    from: {
      chat: asker,
      name: `steward ${asker}`,
      workspace: "alpha",
      task: true,
      tab: false,
      reported: false,
      unreported: false,
    },
  });
}

/** The window's clock: when the person left, and so on from there. */
const LEFT = Date.parse("2026-10-09T10:00:00Z");
const MINUTE = 60 * 1000;
const iso = (at: number) => new Date(at).toISOString();

function finished(id: string, name: string, more: Partial<FinishedTask> = {}): FinishedTask {
  return {
    id,
    asker: 1,
    name,
    chat: null,
    did_not_start: false,
    attempts: 0,
    waits: null,
    persona: "devops",
    how: "done",
    outcome: "done",
    folds: true,
    report: `${name}: all good.`,
    changed: null,
    ended: iso(LEFT + 10 * MINUTE),
    place: "alpha",
    branch: null,
    reopens: true,
    not_reopened: null,
    ...more,
  };
}

const SEVEN_DONE = ["a", "b", "c", "d", "e", "f", "g"].map((name, at) =>
  finished(`01K7DONE${at}`, `check ${name}`),
);
const FAILED = finished("01K7FAILED", "check staging", {
  how: "failed",
  outcome: "failed",
  folds: false,
  report: "The cluster refused the login.",
});
const failure: Need = {
  kind: "task_failed",
  task: "check staging",
  how: "failed",
  why: "The cluster refused the login.",
  id: FAILED.id,
  chat: null,
};

type Asked = { cmd: string; args: Record<string, unknown> };

type Project = { open: OpenChat[]; rows: FinishedTask[] };

/** The core, holding `open` and `rows` in the project at PLANE, and `other` at TWO when given:
 *  then the window is put back with both, PLANE in front. */
function core(open: OpenChat[], rows: FinishedTask[] = [], other?: Project) {
  const projects: Record<string, Project> = { [PLANE]: { open, rows } };
  if (other !== undefined) projects[TWO] = other;
  const asked: Asked[] = [];
  const everyListener = new Map<string, number[]>();
  let sequence = 0;
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: a });
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      everyListener.set(event, [...(everyListener.get(event) ?? []), handler]);
      return 1;
    }
    const here = projects[(a.plane as string | undefined) ?? PLANE] ?? projects[PLANE];
    if (cmd === "plane_at_launch")
      return other === undefined
        ? { plane: PLANE, from: PLANE, why: null }
        : { plane: null, from: null, why: null };
    if (cmd === "planes_to_restore")
      return {
        windows: other === undefined ? [] : [{ planes: [PLANE, TWO], active: 0 }],
        dropped: [],
        gone: [],
      };
    if (cmd === "open_plane") return { plane: a.path, ask: null };
    if (cmd === "opened_chats") return [...here.open];
    if (cmd === "stopping_chats") return [];
    if (cmd === "finished_tasks") return [...here.rows];
    if (cmd === "plane_sidebar")
      return {
        root: (a.plane as string | undefined) ?? PLANE,
        personas: ["steward", "devops"],
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
            chats: [...here.open],
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
        personas: ["steward", "devops"],
        persona: "steward",
      };
    if (cmd === "workspace_repos")
      return { workspace: a.workspace, repos: [], cache_refused: null };
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "dispatch_grants_needed") return [];
    if (cmd === "vault_refusals") return [];
    if (cmd === "owed_restarts") return [];
    if (cmd === "alerts_everywhere") return [{ plane: PLANE, alerts: [], stopped: null }];
    // The Inbox's updates (#1693): kept as the core keeps them, where a test gives a store.
    if (keeps !== null && cmd === "inbox_updates") return keeps.shown();
    if (keeps !== null && cmd === "note_inbox_updates") return keeps.note(a.noted as UpdateNoted[]);
    if (keeps !== null && cmd === "settle_inbox_updates")
      return keeps.settle(a.keys as string[] | null, a.how as string);
    return null;
  });
  const send = async (event: string, payload: unknown) => {
    const handlers = everyListener.get(event) ?? [];
    if (handlers.length === 0) throw new Error(`the window is not listening for ${event}`);
    await act(async () => {
      for (const handler of handlers)
        window.__TAURI_INTERNALS__.runCallback(handler, { event, id: 1, payload });
      await Promise.resolve();
    });
  };
  return {
    asked: (cmd: string) => asked.filter((one) => one.cmd === cmd).map((one) => one.args),
    commands: () => asked.map((one) => one.cmd),
    /** Tasks finished: their rows are read from the next read on, and the core says so. */
    finish: (...more: FinishedTask[]) => finishIn(PLANE, more),
    /** The same, in the project at `plane`. */
    finishIn: (plane: string, ...more: FinishedTask[]) => finishIn(plane, more),
    /** The core says chat `session` moved to `state`, with `queue` asking for the person. */
    move: (session: number, state: State, queue: number[], more: Partial<Moved> = {}) =>
      moveIn(PLANE, session, state, queue, more),
    moveIn: (plane: string, session: number, state: State, queue: number[]) =>
      moveIn(plane, session, state, queue, {}),
    /** The core keeps a dispatch of each of `asking`'s refused while nobody was there, last
     *  at `at`. */
    refuse: (asking: string[], at: number) =>
      send("dispatch-away-changed", {
        plane: PLANE,
        refused: asking.map((one) => ({
          asking: one,
          target: "devops",
          workspace: "alpha",
          latest: at / 1000,
          times: 1,
          allows: `${one} chats may dispatch to devops in alpha.`,
          nowhere: null,
          shown: `${one}>devops@alpha`,
        })),
      }),
  };
  async function finishIn(plane: string, more: FinishedTask[]) {
    projects[plane].rows.push(...more);
    await send("plane-changed", {
      plane,
      changes: [{ kind: "chats", workspace: null, persona: null, path: "" }],
      answers: [{ answer: "sidebar" }],
    });
  }
  async function moveIn(
    plane: string,
    session: number,
    state: State,
    queue: number[],
    more: Partial<Moved>,
  ) {
    sequence += 1;
    const moved: Moved = {
      plane,
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
      ...more,
    };
    await send("chat-moved", moved);
  }
}

/** The person leaves the window at `at`. */
const leave = (at = LEFT) =>
  act(() => {
    vi.setSystemTime(at);
    fireEvent.blur(window);
  });

/** The person comes back to it at `at`. */
const comeBack = (at: number) =>
  act(() => {
    vi.setSystemTime(at);
    fireEvent.focus(window);
  });

const summary = () => document.querySelector<HTMLElement>('[data-cause="away-summary"]');
const said = (notice: HTMLElement) =>
  notice.querySelector(".notice-says")?.textContent?.replace(/\s+/g, " ").trim();
const theSummary = () =>
  waitFor(() => {
    const found = summary();
    if (found === null) throw new Error("no summary stands");
    return found;
  });

const settle = () => act(() => new Promise((done) => setTimeout(done, 50)));
const onScreen = () =>
  screen.queryAllByTestId("pane").map((pane) => Number(pane.getAttribute("data-session")));

/** The window, drawn, with steward 1 and its task talk, and steward 2. */
async function drawn(rows: FinishedTask[] = []) {
  const held = core([chat(1), chat(2), task(4, "talk")], rows);
  render(<App />);
  const tree = await screen.findByRole("tree", { name: "Chats of this project" });
  await waitFor(() => expect(within(tree).getAllByRole("treeitem").length).toBeGreaterThan(1));
  await settle();
  return { ...held, tree };
}

/** While the person is away: seven tasks done, one failed, and two chats come to need them. */
async function whileAway(held: Awaited<ReturnType<typeof drawn>>) {
  await held.finish(...SEVEN_DONE, FAILED);
  await held.move(1, "running", [1], { needs: [failure] });
  await held.move(2, "waiting", [1, 2]);
  await held.move(4, "waiting", [1, 2, 4]);
}

/** A machine's updates, as the core keeps them (`inboxupdates`): `null` for a core that keeps
 *  none, as most tests here have. */
let keeps: ReturnType<typeof keeping> | null = null;

function keeping(kept: InboxUpdate[] = []) {
  const dismissed = new Set<string>();
  const shown = () =>
    kept.filter((one) => !dismissed.has(one.key)).sort((one, other) => other.at - one.at);
  return {
    kept,
    shown,
    note: (noted: UpdateNoted[]) => {
      for (const one of noted)
        if (!kept.some((was) => was.key === one.key)) kept.push({ ...one, read: false });
      return shown();
    },
    settle: (keys: string[] | null, how: string) => {
      for (const one of kept)
        if (keys === null || keys.includes(one.key)) {
          one.read = true;
          if (how === "dismissed") dismissed.add(one.key);
        }
      return shown();
    },
  };
}

beforeEach(() => {
  keeps = null;
  globalThis.localStorage.clear();
  forgetThisLaunch();
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(LEFT);
  vi.spyOn(document, "hasFocus").mockReturnValue(true);
});
afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("while you were away (#1514)", () => {
  it("sums up, on the way back, what happened while the window was not in use", async () => {
    const held = await drawn();
    await leave();
    await whileAway(held);
    expect(summary()).toBeNull();

    await comeBack(LEFT + 30 * MINUTE);

    const notice = await theSummary();
    expect(said(notice)).toBe("While you were away: 7 tasks done, 1 failed, 2 waiting on you");
    // Under the strip, one for the project; never focused, so nothing typed goes astray.
    expect(notice.closest(".pane-notices")).toBeNull();
    expect(notice.contains(document.activeElement)).toBe(false);
  });

  it("says nothing after a moment away", async () => {
    const held = await drawn();
    await leave();
    await whileAway(held);
    await comeBack(LEFT + 4 * MINUTE);
    await settle();
    expect(summary()).toBeNull();
  });

  it("counts only what happened while away", async () => {
    // Done before the person left, and a chat already waiting then: none of it is news.
    const earlier = finished("01K7EARLY", "check early", { ended: iso(LEFT - MINUTE) });
    const held = await drawn([earlier]);
    await held.move(2, "waiting", [2]);
    await leave();
    await held.finish(SEVEN_DONE[0]);
    await held.move(4, "waiting", [2, 4]);
    await comeBack(LEFT + 30 * MINUTE);
    // And after the person is back: not in the summary either.
    await held.finish(finished("01K7LATE", "check late", { ended: iso(LEFT + 31 * MINUTE) }));

    expect(said(await theSummary())).toBe("While you were away: 1 task done, 1 waiting on you");
  });

  it("counts a chat already waiting when the person left that asked them something new", async () => {
    // #1551: steward 1 waited on a failure's look; while away it ended a turn waiting on the
    // person too. steward 2 waited, and only went on waiting.
    const held = await drawn();
    await held.move(1, "running", [1], { needs: [failure] });
    await held.move(2, "waiting", [1, 2]);
    await leave();
    await held.move(1, "waiting", [1, 2], { needs: [failure] });
    await comeBack(LEFT + 30 * MINUTE);

    const notice = await theSummary();
    expect(said(notice)).toBe("While you were away: 1 chat waiting on you");
    expect(
      within(notice).getByRole("button", { name: "1 chat waiting on you: steward 1" }),
    ).toBeInTheDocument();
  });

  it("counts a waiting chat whose task failed while away as the failure alone", async () => {
    // #1551 review: steward 1 waited on the person when they left; its task failed meanwhile.
    // One event: one failure, and its question is no new wait.
    const held = await drawn();
    await held.move(1, "waiting", [1]);
    await leave();
    await held.finish(FAILED);
    await held.move(1, "waiting", [1], { needs: [failure] });
    await comeBack(LEFT + 30 * MINUTE);

    expect(said(await theSummary())).toBe("While you were away: 1 task failed");
  });

  it("counts a dispatch refused while nobody was there, and opens the Inbox it is answered in", async () => {
    // #1551: #1507's item joins the summary as a part of its own; #1693: it is an update in
    // the Inbox, answered there.
    const held = await drawn();
    await leave();
    await held.refuse(["steward"], LEFT + 10 * MINUTE);
    await comeBack(LEFT + 30 * MINUTE);

    const notice = await theSummary();
    expect(said(notice)).toBe("While you were away: 1 dispatch refused");
    await userEvent.click(
      within(notice).getByRole("button", { name: "1 dispatch refused: steward wanted devops" }),
    );

    // The Inbox, shown, and its updates, where its Allow, Never and Dismiss answer it.
    const inbox = await screen.findByRole("tabpanel", { name: "Inbox" });
    const updates = within(inbox).getByRole("region", { name: "Updates" });
    expect(within(updates).getByText("steward wanted devops while you were away")).toBeTruthy();
    expect(
      within(updates).getByRole("button", { name: /^Allow from now on: steward chats/ }),
    ).toBeTruthy();
  });

  it("draws several refused dispatches as one link to the Inbox, which lists each", async () => {
    const held = await drawn();
    await leave();
    await held.refuse(["steward", "lead"], LEFT + 10 * MINUTE);
    await comeBack(LEFT + 30 * MINUTE);

    const notice = await theSummary();
    expect(said(notice)).toBe("While you were away: 2 dispatches refused");
    const part = within(notice).getByRole("button", {
      name: "2 dispatches refused: steward wanted devops, lead wanted devops",
    });
    expect(part.getAttribute("aria-haspopup")).toBeNull();
    await userEvent.click(part);

    const inbox = await screen.findByRole("tabpanel", { name: "Inbox" });
    const updates = within(inbox).getByRole("region", { name: "Updates" });
    expect(within(updates).getByText("lead wanted devops while you were away")).toBeTruthy();
    await settle();
    expect(within(updates).getByText("steward wanted devops while you were away")).toBeTruthy();
  });

  it("goes from each part to the chats it counts", async () => {
    const held = await drawn();
    await leave();
    await whileAway(held);
    await comeBack(LEFT + 30 * MINUTE);
    const notice = await theSummary();

    // Several: a list of them, each going to its own.
    await userEvent.click(within(notice).getByRole("button", { name: "2 waiting on you" }));
    const waiting = await screen.findByRole("menu", { name: "2 waiting on you" });
    expect(
      within(waiting)
        .getAllByRole("menuitem")
        .map((one) => one.textContent),
    ).toEqual(["steward 2", "talk"]);
    // A task waiting on you is shown where it lives, inside its session's tab, where its
    // question is answered as it always is.
    await userEvent.click(within(waiting).getByRole("menuitem", { name: "talk" }));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await userEvent.click(within(notice).getByRole("button", { name: "7 tasks done" }));
    const done = await screen.findByRole("menu", { name: "7 tasks done" });
    expect(within(done).getAllByRole("menuitem")).toHaveLength(7);
    expect(within(done).getAllByRole("menuitem")[0].textContent).toBe(
      "check a, a task of steward 1",
    );
    await userEvent.keyboard("{Escape}");

    // One: straight to its finished row under the session that asked.
    await userEvent.click(
      within(notice).getByRole("button", {
        name: "1 failed: check staging, a task of steward 1",
      }),
    );
    await waitFor(() =>
      expect(within(held.tree).getByRole("button", { name: /^check staging/ })).toHaveFocus(),
    );
  });

  it("puts itself away on Dismiss, and nothing that waits for an answer", async () => {
    const held = await drawn();
    await leave();
    await whileAway(held);
    await comeBack(LEFT + 30 * MINUTE);
    const before = held.commands().length;

    await userEvent.click(within(await theSummary()).getByRole("button", { name: "Dismiss" }));

    expect(summary()).toBeNull();
    // The chats still need the person, on the title bar's list, as they did before.
    expect(screen.getByTestId("needs-you-button")).toHaveAccessibleName("3 chats need you");
    // And Dismiss told the core nothing: nothing was answered, ignored or looked at.
    // The asks registry reading its list again is a read, on its own timer (#1692), not an answer.
    await settle();
    expect(
      held
        .commands()
        .slice(before)
        .filter((cmd) => cmd !== "asks_waiting"),
    ).toEqual([]);
  });

  it("is never drawn with the setting off, and the window is as before", async () => {
    act(() => setChatsListPrefs({ away: false }));
    const held = await drawn();
    await leave();
    await whileAway(held);
    await comeBack(LEFT + 30 * MINUTE);
    await settle();

    expect(summary()).toBeNull();
    expect(screen.getByTestId("needs-you-button")).toHaveAccessibleName("3 chats need you");
    expect(within(held.tree).getByRole("button", { name: /^check staging/ })).toBeTruthy();
  });

  it("is drawn no more once what it counted has gone, and the next time away starts anew", async () => {
    const held = await drawn();
    await leave();
    await held.move(2, "waiting", [2]);
    await comeBack(LEFT + 30 * MINUTE);
    expect(said(await theSummary())).toBe("While you were away: 1 chat waiting on you");

    // The person answered it: nothing is waiting, and nothing is said.
    await held.move(2, "running", []);
    await waitFor(() => expect(summary()).toBeNull());

    await leave(LEFT + 40 * MINUTE);
    await held.finish(finished("01K7NEXT", "check next", { ended: iso(LEFT + 50 * MINUTE) }));
    await comeBack(LEFT + 60 * MINUTE);
    expect(said(await theSummary())).toBe("While you were away: 1 task done");
  });

  it("counts a window left in front with no input as away, from its last input", async () => {
    const held = await drawn();
    act(() => void fireEvent.pointerMove(window));
    await held.finish(finished("01K7EARLY", "check early", { ended: iso(LEFT + 2 * MINUTE) }));
    // Four minutes without a key or a pointer is not away.
    act(() => {
      vi.setSystemTime(LEFT + 4 * MINUTE);
      fireEvent.keyDown(window, { key: "a" });
    });
    await settle();
    expect(summary()).toBeNull();

    // Then nothing for half an hour, while the tasks worked on; the first key is the return.
    await whileAway(held);
    act(() => {
      vi.setSystemTime(LEFT + 40 * MINUTE);
      fireEvent.keyDown(window, { key: "a" });
    });
    // From the last input: the task that ended before it is not counted.
    expect(said(await theSummary())).toBe(
      "While you were away: 7 tasks done, 1 failed, 2 waiting on you",
    );
  });

  it("counts a hidden window as away until it is shown", async () => {
    const held = await drawn();
    const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    act(() => void document.dispatchEvent(new Event("visibilitychange")));
    await whileAway(held);
    visibility.mockReturnValue("visible");
    act(() => {
      vi.setSystemTime(LEFT + 30 * MINUTE);
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(said(await theSummary())).toBe(
      "While you were away: 7 tasks done, 1 failed, 2 waiting on you",
    );
  });

  it("counts a window that started without the focus as away from its start", async () => {
    vi.mocked(document.hasFocus).mockReturnValue(false);
    const held = await drawn();
    await held.finish(finished("01K7FIRST", "check first", { ended: iso(LEFT + MINUTE) }));
    // A pointer passing over the window behind another is not the person coming back to it.
    act(() => {
      vi.setSystemTime(LEFT + 2 * MINUTE);
      fireEvent.pointerMove(window);
    });
    await comeBack(LEFT + 30 * MINUTE);
    expect(said(await theSummary())).toBe("While you were away: 1 task done");
  });

  it("lets an old time away go, so nothing read later brings it back", async () => {
    const held = await drawn();
    await leave();
    await comeBack(LEFT + 30 * MINUTE);
    // Nothing happened; the person works on for a while.
    act(() => {
      vi.setSystemTime(LEFT + 31 * MINUTE);
      fireEvent.keyDown(window, { key: "a" });
    });
    // A record read now that ended inside that old time away is not news any more.
    await held.finish(finished("01K7OLD", "check old", { ended: iso(LEFT + 10 * MINUTE) }));
    await settle();
    expect(summary()).toBeNull();
  });
});

describe("while you were away, in a window of two projects (#1514)", () => {
  it("keeps a summary for each project, in front or not", async () => {
    const held = core([chat(1)], [], { open: [chat(1)], rows: [] });
    render(<App />);
    await waitFor(() => expect(projectNames()).toEqual(["plane*", "two"]));
    await settle();

    await leave();
    await held.finish(finished("01K7ONE", "check one"));
    await held.finishIn(TWO, finished("01K7TWO1", "check two"), finished("01K7TWO2", "check too"));
    await comeBack(LEFT + 30 * MINUTE);
    expect(said(await theSummary())).toBe("While you were away: 1 task done");

    await userEvent.click(projectTab("two"));
    await waitFor(() => expect(projectNames()).toEqual(["plane", "two*"]));
    expect(said(await theSummary())).toBe("While you were away: 2 tasks done");

    // Switching back finds the first still standing: a switch is no Dismiss.
    await userEvent.click(projectTab("plane"));
    await waitFor(() => expect(projectNames()).toEqual(["plane*", "two"]));
    expect(said(await theSummary())).toBe("While you were away: 1 task done");
  });
});

/** The projects on the strip, `*` on the one in front. */
const projectNames = () =>
  within(stripNamed("Projects"))
    .getAllByRole("tab")
    .map(
      (tab) =>
        `${tab.querySelector(".project-name")?.textContent}${
          tab.getAttribute("aria-selected") === "true" ? "*" : ""
        }`,
    );

/** One project's tab on the strip, by its name. */
function projectTab(name: string): HTMLElement {
  const tab = within(stripNamed("Projects"))
    .getAllByRole("tab")
    .find((one) => one.querySelector(".project-name")?.textContent === name);
  if (!tab) throw new Error(`no project tab called ${name}`);
  return tab;
}

/**
 * **The Inbox's updates** (#1693), in the window: what finished or failed, kept by the core a
 * day on this machine, listed after the asks, and Dismiss all and Mark all read on them alone.
 */
describe("the Inbox's updates (#1693)", () => {
  const openInbox = async () => {
    // Pressed only where it is not shown already: a press on the view in front puts it away.
    if (screen.queryByRole("tabpanel", { name: "Inbox" }) === null)
      await userEvent.click(
        within(screen.getByRole("tablist", { name: "Attention" })).getByRole("tab", {
          name: "Inbox",
        }),
      );
    return screen.findByRole("tabpanel", { name: "Inbox" });
  };

  it("notes a task that finished and one that failed, and lists both newest first", async () => {
    keeps = keeping();
    const done = finished("01K7DONE", "check alpha", { ended: iso(LEFT - 2 * MINUTE) });
    const failed = { ...FAILED, ended: iso(LEFT - MINUTE) };
    const held = await drawn([done, failed]);

    await waitFor(() =>
      expect(keeps?.kept.map((one) => [one.key, one.kind])).toEqual([
        ["task:01K7DONE", "task-done"],
        ["task:01K7FAILED", "task-failed"],
      ]),
    );
    // Noted once each, however often the rows are read again.
    await held.finish();
    await settle();
    const noted = held.asked("note_inbox_updates").flatMap((one) => one.noted as UpdateNoted[]);
    expect(noted.filter((one) => one.key === "task:01K7DONE")).toHaveLength(1);

    const updates = within(await openInbox()).getByRole("region", { name: "Updates" });
    expect(
      within(updates)
        .getAllByRole("listitem")
        .map((row) => row.getAttribute("data-kind")),
    ).toEqual(["task-failed", "task-done"]);
  });

  it("lists what the core kept across a relaunch, though no source says it now", async () => {
    keeps = keeping([
      {
        key: "doctor:git:fail",
        kind: "doctor",
        at: LEFT / 1000 - 3600,
        session: null,
        chain: [],
        says: "git: no identity is set",
        read: true,
      },
    ]);
    await drawn();
    const updates = within(await openInbox()).getByRole("region", { name: "Updates" });
    expect(within(updates).getByText("git: no identity is set")).toBeTruthy();
  });

  it("dismisses every update and marks every one read on the core, and answers no ask", async () => {
    keeps = keeping();
    const held = await drawn([{ ...FAILED, ended: iso(LEFT - MINUTE) }]);
    const inbox = await openInbox();
    await waitFor(() => within(inbox).getByRole("region", { name: "Updates" }));

    await userEvent.click(within(inbox).getByRole("button", { name: "Mark all read" }));
    await waitFor(() =>
      expect(held.asked("settle_inbox_updates")).toEqual([
        { plane: PLANE, keys: null, how: "read" },
      ]),
    );
    await userEvent.click(within(inbox).getByRole("button", { name: "Dismiss all" }));
    await waitFor(() =>
      expect(within(inbox).queryByRole("region", { name: "Updates" })).toBeNull(),
    );
    expect(held.asked("settle_inbox_updates").at(-1)).toEqual({
      plane: PLANE,
      keys: null,
      how: "dismissed",
    });
    expect(held.commands()).not.toContain("answer_ask");
  });

  it("goes from a task's update to its finished row, and marks that one read", async () => {
    keeps = keeping();
    const held = await drawn([{ ...FAILED, ended: iso(LEFT - MINUTE) }]);
    const updates = await waitFor(async () =>
      within(await openInbox()).getByRole("region", { name: "Updates" }),
    );
    await userEvent.click(within(updates).getByRole("button", { name: /^Go to chat/ }));
    await waitFor(() =>
      expect(held.asked("settle_inbox_updates")).toContainEqual({
        plane: PLANE,
        keys: ["task:01K7FAILED"],
        how: "read",
      }),
    );
  });
});
