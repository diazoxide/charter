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
 * the breadcrumb's line while a tab shows the task, the one question a task mid-turn asks, and
 * the words its row says afterwards.
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
function core(endings: Record<number, Partial<TaskEnding>> = {}, sweepHasATab = false) {
  const open: Listed[] = [
    chat(1),
    chat(4, { persona: "devops", label: "talk", from: taskOf(1) }),
    chat(5, { persona: "devops", label: "sweep", from: taskOf(1, { tab: sweepHasATab }) }),
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

const STOP = "Stop task talk and get its report";
const CLOSE = "Close task talk now";

async function drawn(endings: Record<number, Partial<TaskEnding>> = {}, sweepHasATab = false) {
  const held = core(endings, sweepHasATab);
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
    expect(items.filter((item) => item?.startsWith("Stop task"))).toEqual([]);
    expect(items.filter((item) => item?.startsWith("Close task"))).toEqual([]);
  });

  it("ends an idle task as pressed, with no question", async () => {
    const { tree, asked } = await drawn();

    await menuOf(tree, "sweep");
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Stop task sweep and get its report" }),
    );

    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 5, way: "report", below: false }]),
    );
    expect(screen.queryByRole("alertdialog")).toBeNull();

    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: "Close task sweep now" }));

    await waitFor(() => expect(ends(asked)).toHaveLength(2));
    expect(ends(asked)[1]).toEqual({ plane: PLANE, session: 5, way: "now", below: false });
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("ends a task that has reported with no question, and asks it for no second report", async () => {
    const { tree, asked } = await drawn({
      5: { reported: true, no_report: "'sweep' has reported already." },
    });

    await menuOf(tree, "sweep");
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Stop task sweep and get its report" }),
    );

    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 5, way: "now", below: false }]),
    );
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});

describe("the one question, for a task in the middle of a turn", () => {
  it("asks once, in V100-18's words, and ends nothing until it is answered", async () => {
    const { tree, asked } = await drawn({ 5: { working: true } });

    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: "Close task sweep now" }));

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
      await userEvent.click(
        screen.getByRole("menuitem", { name: "Stop task sweep and get its report" }),
      );
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
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Stop task sweep and get its report" }),
    );

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
    await userEvent.click(screen.getByRole("menuitem", { name: "Close task sweep now" }));
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
  /** The close on the strip of the tab called `name`. */
  const closeOf = (name: string) => {
    const tab = within(screen.getByRole("tablist", { name: "Tabs" }))
      .getAllByRole("tab")
      .find((one) => one.querySelector(".tab-name")?.textContent === name);
    const close = tab?.closest(".tab")?.querySelector<HTMLElement>("button.closer");
    if (!close) throw new Error(`no tab called ${name} has a close`);
    return close;
  };
  const standard = ["close_session", "smart_close", "smart_close_offer", "close_chat_stopping"];

  it("is never asked the standard close question: its tab's close is the task's own ending", async () => {
    const { asked } = await drawn({ 5: { working: true } }, true);

    await userEvent.click(closeOf("sweep"));

    const question = await screen.findByRole("alertdialog", { name: "Stop task 'sweep'?" });
    expect(screen.queryByRole("alertdialog", { name: /^End chat/ })).toBeNull();
    expect(question.textContent).not.toMatch(/Smart close|session record/);
    await userEvent.click(
      within(question).getByRole("button", { name: "Stop and get its report" }),
    );
    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 5, way: "report", below: false }]),
    );
    expect(asked.filter((one) => standard.includes(one.cmd))).toEqual([]);
  });

  it("is stopped with no question when it is idle, and the session's own close is as it was", async () => {
    const { asked } = await drawn({}, true);

    await userEvent.click(closeOf("sweep"));

    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 5, way: "report", below: false }]),
    );
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(asked.filter((one) => standard.includes(one.cmd))).toEqual([]);
    // A session is not a task: its close still asks the close question.
    await userEvent.click(closeOf("steward 1"));
    expect(await screen.findByRole("alertdialog", { name: "End chat steward 1?" })).toBeTruthy();
  });
});

describe("where purlis may not type into the task", () => {
  it("says why, offers Close now alone, and leaves the keyboard on Cancel", async () => {
    const why =
      "'sweep' is showing a prompt that is yours to answer, and purlis types nothing into a chat that is.";
    const { tree, asked } = await drawn({ 5: { no_report: why } });

    await menuOf(tree, "sweep");
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Stop task sweep and get its report" }),
    );

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

  it("closes an idle task now with no question, whatever it could have been asked", async () => {
    const { tree, asked } = await drawn({ 5: { no_report: "purlis has heard nothing." } });

    await menuOf(tree, "sweep");
    await userEvent.click(screen.getByRole("menuitem", { name: "Close task sweep now" }));

    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 5, way: "now", below: false }]),
    );
    expect(screen.queryByRole("alertdialog")).toBeNull();
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

  it("stops an idle task and gets its report, with no question", async () => {
    const { tree, asked } = await drawn();

    act(() => row(tree, "sweep").focus());
    await userEvent.keyboard("{Delete}");

    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 5, way: "report", below: false }]),
    );
    expect(screen.queryByRole("alertdialog")).toBeNull();
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
    // Words, not a cross: nothing here is drawn as the tab's close is.
    expect(buttons.map((button) => button.textContent)).toEqual(["Stop", "Close now"]);
    for (const button of buttons) {
      expect(button.querySelector("svg")).toBeNull();
      expect(button.className).not.toMatch(/closer|ends-a-chat/);
    }
    // Beside the path, in the pane's top line, and not among the pane's own controls.
    expect(group.closest(".pane-chips")).not.toBeNull();
    expect(group.closest(".pane-doing")).toBeNull();
    expect(screen.queryByRole("button", { name: "End this pane's chat" })).toBeNull();

    await userEvent.click(within(group).getByRole("button", { name: CLOSE }));

    await waitFor(() =>
      expect(ends(asked)).toEqual([{ plane: PLANE, session: 4, way: "now", below: false }]),
    );
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("asks first from there too when the task is mid-turn", async () => {
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

  it("says a task is being stopped, stops it no second time, and still closes it now", async () => {
    const { tree, stopping } = await drawn();
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    await stopping(4);

    await waitFor(() => expect(row(tree, "talk")).toHaveTextContent("Stopping…"));
    const group = screen.getByRole("group", { name: "End this task" });
    const stop = within(group).getByRole("button", { name: STOP });
    expect(stop).toHaveAttribute("aria-disabled", "true");
    expect(stop.getAttribute("title")).toContain("is being stopped already");
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
