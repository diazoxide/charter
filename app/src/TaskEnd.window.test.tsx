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
import type { FinishedTask, OpenChat, TaskEnding } from "./bindings";
import { forgetKeyboard } from "./paneKeyboard";
import { forgetThisLaunch } from "./regions";

/**
 * **The person ends a task with Stop and get its report or Close now** (#1488, V100-5,
 * V100-18), against the whole window: the two rows on a task's row menu, the two buttons on
 * the breadcrumb's line while a tab shows the task, the second step every ending takes (in
 * place for an idle task, the one modal question for a task mid-turn), and the words its row
 * says afterwards. **Nothing here ends a task on one press**, and no close ends one at all.
 *
 * The core here is a fixture. It answers `task_ending` with what ending a task would do, as
 * the real one reads it from its own records, and takes `end_task`. What the chat that asked
 * is then told is the core's, and is tested there.
 */

vi.mock("./SessionPane", async () => {
  const { paneDrawn } = await import("./paneKeyboard");
  return {
    SessionPane: ({ plane, session }: { plane: string; session: number }) => {
      const pane = useRef<HTMLDivElement>(null);
      useEffect(() => paneDrawn(plane, session, () => pane.current?.focus()), [plane, session]);
      return (
        <div data-testid="pane" data-session={session} tabIndex={-1} ref={pane}>
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

function chat(session: number, more: Partial<OpenChat> = {}): Listed {
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
    workspace: "alpha",
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

/** What ending an idle task that can be asked for a report would do: nothing to ask about. */
function idle(name: string, more: Partial<TaskEnding> = {}): TaskEnding {
  return {
    name,
    working: false,
    no_report: null,
    below: [],
    stopping: false,
    reported: false,
    ...more,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

/**
 * The core: steward 1 dispatched talk (4), which dispatched deep (7), and sweep (5). `endings`
 * is what it answers `task_ending` with, by chat; a task not in it is idle.
 */
function core(
  endings: Record<number, Partial<TaskEnding>> = {},
  sweepHasATab = false,
  sweepRunsOn = "claude",
) {
  const open: Listed[] = [
    chat(1),
    chat(4, { persona: "devops", label: "talk", from: taskOf(1) }),
    chat(5, {
      persona: "devops",
      label: "sweep",
      harness: sweepRunsOn,
      from: taskOf(1, { tab: sweepHasATab }),
    }),
    chat(7, { persona: "devops", label: "deep", from: taskOf(4, { name: "talk" }) }),
  ];
  const asked: Asked[] = [];
  const listeners = new Map<string, number[]>();
  const finished: FinishedTask[] = [];
  /** What `end_task` is refused with, while the test says it is. */
  const refuses: { why?: string } = {};
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
    if (cmd === "task_ending") {
      const session = a.session as number;
      const one = open.find((chat) => chat.session === session);
      if (one === undefined || !one.from?.task)
        throw new Error("That chat is not a task, so it is not ended this way.");
      return idle(one.label ?? one.name, endings[session]);
    }
    if (cmd === "end_task") {
      if (refuses.why !== undefined) throw new Error(refuses.why);
      return null;
    }
    if (cmd === "close_chat_tab") {
      // The core's half of sending a task's tab back: it has no tab from here on.
      const one = open.find((chat) => chat.session === a.session);
      if (!one?.from?.task) throw new Error("Only a task goes back to the Chats list.");
      one.from = { ...one.from, tab: false };
      return null;
    }
    if (cmd === "finished_tasks") return [...finished];
    if (cmd === "stopping_chats") return [];
    if (cmd === "dispatch_grants_needed" || cmd === "vault_refusals") return [];
    if (cmd === "owed_restarts") return [];
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
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
            chats: open.map(asListed),
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
  return {
    asked,
    finished,
    refuses,
    /** The core says chat `session` is being stopped. */
    stopping: (session: number) => said("chat-stop", { plane: PLANE, session, phase: "stopping" }),
    /** The core says chat `session` has ended: it is gone from what the core lists. */
    ended: async (session: number) => {
      open.splice(
        open.findIndex((chat) => chat.session === session),
        1,
      );
      await said("chat-stop", { plane: PLANE, session, phase: "stopped" });
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

const onScreen = () =>
  screen.queryAllByTestId("pane").map((pane) => Number(pane.getAttribute("data-session")));

const ends = (asked: Asked[]) =>
  asked.filter((one) => one.cmd === "end_task").map((one) => one.args);

const STOP = "Stop and get its report: task talk";
const CLOSE = "Close now: task talk";
const STOP_SWEEP = "Stop and get its report: task sweep";
const CLOSE_SWEEP = "Close now: task sweep";

/** The second step, where it is asked: the group named by what it asks. */
const second = (says: string) => screen.findByRole("group", { name: says });

async function drawn(
  endings: Record<number, Partial<TaskEnding>> = {},
  sweepHasATab = false,
  sweepRunsOn = "claude",
) {
  const held = core(endings, sweepHasATab, sweepRunsOn);
  render(<App />);
  const tree = await section();
  await waitFor(() => expect(within(tree).getAllByRole("treeitem")).toHaveLength(4));
  return { ...held, tree };
}

/** Opens the menu of the row called `name`. */
async function menuOf(tree: HTMLElement, name: string) {
  fireEvent.contextMenu(row(tree, name));
  await screen.findAllByRole("menuitem");
  return screen.getAllByRole("menuitem").map((item) => item.getAttribute("aria-label"));
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

describe("a task's row menu", () => {
  it("offers Stop and get its report and Close now, and nothing of a chat's close", async () => {
    const { tree } = await drawn();

    const items = await menuOf(tree, "talk");

    expect(items).toContain(STOP);
    expect(items).toContain(CLOSE);
    // One vocabulary: a task is not stopped as a chat is, and is never smart-closed.
    for (const item of items)
      expect(item).not.toMatch(/^Stop chat|Smart close|^End chat|Close tab/);
  });

  it("keeps a session's own Stop rows on the session, which is no task", async () => {
    const { tree } = await drawn();

    const items = await menuOf(tree, "steward 1");

    expect(items).toContain("Stop chat steward 1");
    expect(items.filter((item) => item?.startsWith("Stop and get its report"))).toEqual([]);
    expect(items.filter((item) => item?.startsWith("Close now"))).toEqual([]);
  });

  it("does not offer Stop on a harness purlis types nothing into, and says why on the row", async () => {
    const { tree } = await drawn({}, false, "opencode");

    await menuOf(tree, "sweep");

    const stop = screen.getByRole("menuitem", { name: STOP_SWEEP });
    expect(stop).toHaveAttribute("aria-disabled", "true");
    expect(stop.getAttribute("title")).toContain("purlis does not type into");
    expect(stop.getAttribute("title")).toContain("Close now ends it.");
    expect(screen.getByRole("menuitem", { name: CLOSE_SWEEP })).not.toHaveAttribute(
      "aria-disabled",
    );
  });
});

describe("the second step, for a task that is not in the middle of a turn", () => {
  it("ends nothing on the press: it asks on the task's own row, with the keyboard on Keep", async () => {
    const { tree, asked } = await drawn();

    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: STOP_SWEEP }));

    const step = await second("Stop sweep and get its report?");
    // On its row, in the list, and no dialog: nothing else in the window is taken away.
    expect(row(tree, "sweep").closest("li")).toContainElement(step);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(
      within(step)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["Stop it", "Keep"]);
    expect(within(step).getByRole("button", { name: "Keep" })).toHaveFocus();
    expect(ends(asked)).toEqual([]);

    // A stray Return is Keep: nothing ends, and the keyboard is back on the row.
    await userEvent.keyboard("{Enter}");

    await waitFor(() => expect(screen.queryByRole("group", { name: /^Stop sweep/ })).toBeNull());
    expect(ends(asked)).toEqual([]);
    expect(row(tree, "sweep")).toHaveFocus();
  });

  it("does it on the answer: Stop it, or Close it", async () => {
    const { tree, asked } = await drawn();

    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: STOP_SWEEP }));
    await userEvent.click(
      within(await second("Stop sweep and get its report?")).getByRole("button", {
        name: "Stop it",
      }),
    );

    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 5, way: "report", below: false }]),
    );
    await waitFor(() => expect(screen.queryByRole("group", { name: /^Stop sweep/ })).toBeNull());

    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: CLOSE_SWEEP }));
    const step = await second("Close sweep now, with no report?");
    await waitFor(() => expect(within(step).getByRole("button", { name: "Keep" })).toHaveFocus());
    await userEvent.click(within(step).getByRole("button", { name: "Close it" }));

    await waitFor(() => expect(ends(asked)).toHaveLength(2));
    expect(ends(asked)[1]).toEqual({ plane: PLANE, session: 5, way: "now", below: false });
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("is Keep on Escape, and ends nothing", async () => {
    const { tree, asked } = await drawn();
    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: CLOSE_SWEEP }));
    await second("Close sweep now, with no report?");

    await userEvent.keyboard("{Escape}");

    await waitFor(() => expect(screen.queryByRole("group", { name: /^Close sweep/ })).toBeNull());
    expect(ends(asked)).toEqual([]);
  });

  it("asks a task that has reported only whether to close it, and for no second report", async () => {
    const { tree, asked } = await drawn({
      5: { reported: true, no_report: "'sweep' has reported already." },
    });

    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: STOP_SWEEP }));
    const step = await second("sweep has reported. Close it?");
    await userEvent.click(within(step).getByRole("button", { name: "Close it" }));

    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 5, way: "now", below: false }]),
    );
  });

  it("keeps the core's refusal where the answer was given, and the task as it was", async () => {
    const { tree, refuses } = await drawn();
    refuses.why = "That chat is not open any more.";
    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: CLOSE_SWEEP }));

    await userEvent.click(
      within(await second("Close sweep now, with no report?")).getByRole("button", {
        name: "Close it",
      }),
    );

    const step = await second("Close sweep now, with no report?");
    expect(await within(step).findByRole("alert")).toHaveTextContent(
      "That chat is not open any more.",
    );
    expect(row(tree, "sweep")).toBeTruthy();
  });
});

describe("the one question, for a task in the middle of a turn", () => {
  it("asks once, in V100-18's words, and ends nothing until it is answered", async () => {
    const { tree, asked } = await drawn({ 5: { working: true } });

    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: CLOSE_SWEEP }));

    const question = await screen.findByRole("alertdialog", { name: "Stop task 'sweep'?" });
    expect(question.textContent).toContain("It is working.");
    const answers = within(question)
      .getAllByRole("button")
      .map((button) => button.textContent);
    expect(answers).toEqual(["Cancel", "Close now", "Stop and get its report"]);
    // Stop and get its report is the default: it has the keyboard.
    expect(within(question).getByRole("button", { name: "Stop and get its report" })).toHaveFocus();
    expect(ends(asked)).toEqual([]);

    await userEvent.click(within(question).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(ends(asked)).toEqual([]);
  });

  it("does what was answered: Stop and get its report, or Close now", async () => {
    for (const [answer, way] of [
      ["Stop and get its report", "report"],
      ["Close now", "now"],
    ] as const) {
      const { tree, asked } = await drawn({ 5: { working: true } });
      await menuOf(tree, "sweep");
      await userEvent.click(screen.getByRole("menuitem", { name: STOP_SWEEP }));
      const question = await screen.findByRole("alertdialog", { name: "Stop task 'sweep'?" });

      await userEvent.click(within(question).getByRole("button", { name: answer }));

      await waitFor(() =>
        expect(ends(asked)).toEqual([{ plane: PLANE, session: 5, way, below: false }]),
      );
      await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
      cleanup();
      clearMocks();
    }
  });

  it("is never the standard close dialog: no Smart close, no End chat, and no session is closed", async () => {
    const { tree, asked } = await drawn({ 5: { working: true } });
    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: STOP_SWEEP }));

    const question = await screen.findByRole("alertdialog");

    expect(question.textContent).not.toMatch(/Smart close|End chat|session record/);
    expect(screen.queryByRole("alertdialog", { name: /^End chat/ })).toBeNull();
    await userEvent.click(within(question).getByRole("button", { name: "Close now" }));
    await waitFor(() => expect(ends(asked)).toHaveLength(1));
    const closes = asked.filter((one) =>
      [
        "close_session",
        "smart_close",
        "smart_close_offer",
        "close_chat_stopping",
        "stop_chat",
      ].includes(one.cmd),
    );
    expect(closes).toEqual([]);
  });

  it("keeps the core's refusal in the question, and the task as it was", async () => {
    const { tree, refuses } = await drawn({ 5: { working: true } });
    refuses.why = "That chat is not open any more.";
    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: CLOSE_SWEEP }));
    const question = await screen.findByRole("alertdialog");

    await userEvent.click(within(question).getByRole("button", { name: "Close now" }));

    expect(await within(question).findByRole("alert")).toHaveTextContent(
      "That chat is not open any more.",
    );
    // Behind the question, its row is still there.
    expect(tree.querySelectorAll(".session")).toHaveLength(4);
  });
});

describe("a task that has a tab of its own", () => {
  /** The tab called `name`, on the strip. */
  const tabOf = (name: string) =>
    within(screen.getByRole("tablist", { name: "Tabs" }))
      .queryAllByRole("tab")
      .find((one) => one.querySelector(".tab-name")?.textContent === name);
  /** The close on the strip of the tab called `name`. */
  const closeOf = (name: string) => {
    const close = tabOf(name)?.closest(".tab")?.querySelector<HTMLElement>("button.closer");
    if (!close) throw new Error(`no tab called ${name} has a close`);
    return close;
  };
  const ending = [
    "end_task",
    "close_session",
    "smart_close",
    "smart_close_offer",
    "close_chat_stopping",
    "stop_chat",
  ];

  it("ends nothing when its tab is closed: the tab goes, and the task is back in the list", async () => {
    // Mid-turn, which is where a close that ended it would lose the most.
    const { tree, asked } = await drawn({ 5: { working: true } }, true);
    expect(tabOf("sweep")).toBeDefined();
    // The cross says what it does, and is not drawn as one that ends a chat.
    expect(closeOf("sweep").getAttribute("aria-label")).toBe("Send sweep back to the Chats list");
    expect(closeOf("sweep").className).toContain("keeps");

    await userEvent.click(closeOf("sweep"));

    // No question of any kind: there is nothing to lose.
    await waitFor(() => expect(tabOf("sweep")).toBeUndefined());
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(asked.filter((one) => one.cmd === "close_chat_tab").map((one) => one.args)).toEqual([
      { plane: PLANE, session: 5 },
    ]);
    expect(asked.filter((one) => ending.includes(one.cmd))).toEqual([]);
    // Still a task of steward 1, still listed, and its row opens it again.
    expect(row(tree, "sweep")).toBeTruthy();
    await userEvent.click(row(tree, "sweep"));
    await waitFor(() => expect(onScreen()).toEqual([5]));
  });

  it("is sent back by the close shortcut too, and a session's own close is as it was", async () => {
    const { tree, asked } = await drawn({}, true);

    act(() => tabOf("sweep")?.focus());
    await userEvent.keyboard("{Delete}");

    await waitFor(() => expect(tabOf("sweep")).toBeUndefined());
    expect(asked.filter((one) => ending.includes(one.cmd))).toEqual([]);
    expect(row(tree, "sweep")).toBeTruthy();
    // A session is not a task: its close still asks the close question.
    await userEvent.click(closeOf("steward 1"));
    expect(await screen.findByRole("alertdialog", { name: "End chat steward 1?" })).toBeTruthy();
  });
});

describe("where purlis may not type into the task", () => {
  const why =
    "'sweep' is showing a prompt that is yours to answer, and purlis types nothing into a chat that is.";

  it("says why in the modal question, offers Close now alone, and leaves the keyboard on Cancel", async () => {
    const { tree, asked } = await drawn({ 5: { working: true, no_report: why } });

    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: STOP_SWEEP }));

    const question = await screen.findByRole("alertdialog", { name: "Stop task 'sweep'?" });
    expect(question.textContent).toContain(why);
    expect(question.textContent).toContain("Close now ends its program without a report.");
    expect(
      within(question)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["Cancel", "Close now"]);
    expect(within(question).getByRole("button", { name: "Cancel" })).toHaveFocus();
    expect(ends(asked)).toEqual([]);

    await userEvent.click(within(question).getByRole("button", { name: "Close now" }));

    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 5, way: "now", below: false }]),
    );
  });

  it("says why in place for an idle task, and offers only to close it", async () => {
    const { tree, asked } = await drawn({ 5: { no_report: why } });

    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: STOP_SWEEP }));

    const step = await second(`${why} Close it now, with no report?`);
    expect(
      within(step)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["Close it", "Keep"]);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    await userEvent.click(within(step).getByRole("button", { name: "Close it" }));

    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 5, way: "now", below: false }]),
    );
  });

  it("offers no Stop in the question for a task already being stopped", async () => {
    const { tree } = await drawn({ 5: { working: true, stopping: true } });

    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: CLOSE_SWEEP }));

    const question = await screen.findByRole("alertdialog", { name: "Stop task 'sweep'?" });
    expect(question.textContent).toContain("It is being stopped already");
    expect(
      within(question)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["Cancel", "Close now"]);
  });
});

describe("a task with tasks of its own still working", () => {
  it("asks once what becomes of them, and ends them with it unless the person keeps them", async () => {
    const { tree, asked } = await drawn({ 4: { below: ["deep"] } });

    await menuOf(tree, "talk");
    await userEvent.click(screen.getByRole("menuitem", { name: STOP }));

    const question = await screen.findByRole("alertdialog", { name: "Stop task 'talk'?" });
    expect(question.textContent).toContain("1 task it asked for is still working: deep.");
    // It is idle itself: the question is about what is below it.
    expect(question.textContent).not.toContain("It is working.");
    const too = within(question).getByRole("radio", { name: /^End them too/ });
    const keep = within(question).getByRole("radio", { name: /^Keep them working/ });
    expect(too).toBeChecked();
    await userEvent.click(
      within(question).getByRole("button", { name: "Stop and get its report" }),
    );
    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 4, way: "report", below: true }]),
    );
    expect(keep).not.toBeChecked();
  });

  it("keeps them where the person says so, and says what that means", async () => {
    const { tree, asked } = await drawn({ 4: { working: true, below: ["deep", "deeper"] } });
    await menuOf(tree, "talk");
    await userEvent.click(screen.getByRole("menuitem", { name: CLOSE }));
    const question = await screen.findByRole("alertdialog", { name: "Stop task 'talk'?" });
    expect(question.textContent).toContain("2 tasks it asked for are still working: deep, deeper.");

    await userEvent.click(within(question).getByRole("radio", { name: /^Keep them working/ }));
    expect(question.textContent).toContain(
      "They finish with nobody to report to, and stay in the Chats list marked as from talk.",
    );
    await userEvent.click(within(question).getByRole("button", { name: "Close now" }));

    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 4, way: "now", below: false }]),
    );
  });
});

describe("Delete on a task's row", () => {
  it("asks to stop a task mid-turn, in the task's own question", async () => {
    const { tree, asked } = await drawn({ 5: { working: true } });

    act(() => row(tree, "sweep").focus());
    await userEvent.keyboard("{Delete}");

    expect(await screen.findByRole("alertdialog", { name: "Stop task 'sweep'?" })).toBeTruthy();
    expect(screen.queryByRole("alertdialog", { name: /^Stop chat/ })).toBeNull();
    expect(ends(asked)).toEqual([]);
  });

  it("asks on the row for an idle task, and a second key that is not the answer ends nothing", async () => {
    const { tree, asked } = await drawn();

    act(() => row(tree, "sweep").focus());
    await userEvent.keyboard("{Delete}");

    const step = await second("Stop sweep and get its report?");
    expect(within(step).getByRole("button", { name: "Keep" })).toHaveFocus();
    expect(ends(asked)).toEqual([]);
    // Delete again, Return, Space: the keyboard is on Keep, and none of them stops the task.
    await userEvent.keyboard("{Delete}");
    expect(ends(asked)).toEqual([]);
    await userEvent.keyboard(" ");
    await waitFor(() => expect(screen.queryByRole("group", { name: /^Stop sweep/ })).toBeNull());
    expect(ends(asked)).toEqual([]);

    // Asked again and answered: it is stopped, and its report asked for.
    act(() => row(tree, "sweep").focus());
    await userEvent.keyboard("{Delete}");
    await userEvent.click(
      within(await second("Stop sweep and get its report?")).getByRole("button", {
        name: "Stop it",
      }),
    );
    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 5, way: "report", below: false }]),
    );
  });
});

describe("the breadcrumb's line, while a tab shows a task", () => {
  it("has the only ending control of a pane showing a task: two words, and neither is a close mark", async () => {
    const { tree, asked } = await drawn();
    // The session's own pane has no such control.
    expect(screen.queryByRole("group", { name: "End this task" })).toBeNull();

    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    const group = screen.getByRole("group", { name: "End this task" });
    const buttons = within(group).getAllByRole("button");
    expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual([STOP, CLOSE]);
    // Words, not a cross: nothing here is drawn as the tab's close is. And each name begins
    // with the words drawn, so a voice command for what is on screen presses it.
    expect(buttons.map((button) => button.textContent)).toEqual(["Stop", "Close now"]);
    for (const button of buttons) {
      expect(button.getAttribute("aria-label")?.startsWith(button.textContent ?? "?")).toBe(true);
      expect(button.querySelector("svg")).toBeNull();
      expect(button.className).not.toMatch(/closer|ends-a-chat/);
    }
    // Beside the path, in the pane's top line, and not among the pane's own controls.
    expect(group.closest(".pane-chips")).not.toBeNull();
    expect(group.closest(".pane-doing")).toBeNull();
    expect(screen.queryByRole("button", { name: "End this pane's chat" })).toBeNull();

    // A press ends nothing: it is asked there, in the buttons' place, with Keep focused.
    await userEvent.click(within(group).getByRole("button", { name: CLOSE }));

    const step = await second("Close talk now, with no report?");
    expect(step.closest(".pane-chips")).not.toBeNull();
    expect(within(step).getByRole("button", { name: "Keep" })).toHaveFocus();
    expect(ends(asked)).toEqual([]);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    // Not on the task's row as well: one question, where the press was made.
    expect(row(tree, "talk").closest("li")?.querySelector(".task-end-confirm")).toBeNull();

    await userEvent.click(within(step).getByRole("button", { name: "Keep" }));

    // Kept: the two buttons are back, and nothing ended.
    await waitFor(() => expect(screen.getByRole("group", { name: "End this task" })).toBeTruthy());
    expect(ends(asked)).toEqual([]);

    await userEvent.click(
      within(screen.getByRole("group", { name: "End this task" })).getByRole("button", {
        name: CLOSE,
      }),
    );
    await userEvent.click(
      within(await second("Close talk now, with no report?")).getByRole("button", {
        name: "Close it",
      }),
    );
    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 4, way: "now", below: false }]),
    );
  });

  it("asks in the one question from there when the task is mid-turn", async () => {
    const { tree, asked } = await drawn({ 4: { working: true } });
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await userEvent.click(
      within(screen.getByRole("group", { name: "End this task" })).getByRole("button", {
        name: STOP,
      }),
    );

    const question = await screen.findByRole("alertdialog", { name: "Stop task 'talk'?" });
    expect(ends(asked)).toEqual([]);
    await userEvent.click(
      within(question).getByRole("button", { name: "Stop and get its report" }),
    );
    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 4, way: "report", below: false }]),
    );
  });

  it("says a task is being stopped, stops it no second time, and says why to whoever cannot see a tooltip", async () => {
    const { tree, asked, stopping } = await drawn();
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await stopping(4);

    await waitFor(() => expect(row(tree, "talk")).toHaveTextContent("Stopping…"));
    const group = screen.getByRole("group", { name: "End this task" });
    const stop = within(group).getByRole("button", { name: STOP });
    expect(stop).toHaveAttribute("aria-disabled", "true");
    expect(stop).toHaveAccessibleDescription(/is being stopped already/);
    await userEvent.click(stop);
    expect(screen.queryByRole("group", { name: /^Stop talk/ })).toBeNull();
    expect(ends(asked)).toEqual([]);
    expect(within(group).getByRole("button", { name: CLOSE })).not.toHaveAttribute("aria-disabled");
  });
});

describe("what its row says afterwards", () => {
  const ended = (how: FinishedTask["how"], outcome: string): FinishedTask => ({
    id: "01K6A",
    asker: 1,
    name: "talk",
    persona: "devops",
    how,
    outcome,
    folds: false,
    report: how === "stopped_by_person" ? "Moved two of five." : "",
    changed: null,
    ended: "2026-10-08T12:00:00Z",
    place: "alpha",
    branch: null,
    reopens: true,
    not_reopened: null,
  });

  it.each([
    ["closed_by_person", "closed by you", "octagon"],
    ["stopped_by_person", "stopped by you", "square"],
  ] as const)(
    "says %s in its own word and shape, on its finished row and on the pane left showing it",
    async (how, word, shape) => {
      const { tree, finished, ended: coreEnds } = await drawn();
      await userEvent.click(row(tree, "talk"));
      await waitFor(() => expect(onScreen()).toEqual([4]));

      finished.push(ended(how, word));
      await coreEnds(4);

      // The pane stays on the task, as ended, and says how in the row's word.
      const away = await screen.findByTestId("task-away");
      await waitFor(() => expect(away.querySelector(".shown-state .word")?.textContent).toBe(word));
      expect(away.querySelector(".shown-state .shape")?.getAttribute("data-shape")).toBe(shape);
      // Said once: no second word beside it, and never "cancelled".
      expect(away.querySelector(".outcome")).toBeNull();
      expect(away.textContent).not.toContain("cancelled");
      // No ending control is left: there is nothing to end.
      expect(screen.queryByRole("group", { name: "End this task" })).toBeNull();
      // Its finished row, under the session that asked: a row of its own, not in a fold.
      const list = await screen.findByRole("group", { name: "Finished tasks of steward 1" });
      expect(list.querySelector(".shown-state .word")?.textContent).toBe(word);
      expect(within(list).queryByRole("button", { name: /^Finished \(/ })).toBeNull();
    },
  );
});
