import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { DispatchPending, OpenChat, RelaunchQuestion } from "./bindings";
import { type Interrupts, countInterrupts, withinTheBudget } from "./interruptBudget";
import type { Ending } from "./QuitWarning";
import { stripNamed } from "./test-strips";
import { UpdateItem, type Updates } from "./Updates";

/**
 * **W10's interrupt budget past the first run** (DS-9, #1078): at most three prompts stand
 * between the operator and the next answered agent turn, on every way back to a chat and not
 * only a new machine's. `FirstRun.test.tsx` holds the first run's ways, the sign-in to a forge
 * first among them; this file holds the rest: a relaunch (reopen or fresh), a restart to
 * update, opening someone else's project, and a handoff.
 *
 * Each scenario renders the flow's real surfaces in the order the operator meets them and takes
 * its longest way: the answer that leaves the most still to ask, and the trust question where
 * the flow can raise it. No agent answers here (the pane is a stand-in), so every prompt counted
 * is one before that turn. `interruptBudget.ts` says what counts as a prompt and what it
 * cannot see.
 *
 * **Two ways go past the budget, and are not held here** but on #1078's checklist, since
 * holding them is a change to the flow and not to a test: a restart to update over a chat
 * mid-turn that then starts fresh asks a fourth prompt (the picker), and a relaunch whose
 * restore holds two projects that changed since they were approved asks the trust question
 * twice.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

let interrupts: Interrupts;
/** Prompts a scenario saw that the counter cannot, named by hand where they are seen. */
let byHand: string[];

beforeEach(() => {
  interrupts = countInterrupts();
  byHand = [];
});

afterEach(() => {
  interrupts.stop();
  const asked = [...interrupts.asked(), ...byHand];
  cleanup();
  clearMocks();
  withinTheBudget(asked);
});

const ONE = "/home/dev/plane";
const TWO = "/home/dev/teammate";

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
  persona_profiles: {},
  ignore_fix: null,
  declares_none: true,
};

/** What a teammate's project declares: a plugin and a start, so the trust question is asked. */
const CONTRIBUTES = {
  plugins: [["superpowers@market", "true"]],
  env: [],
  starts: [['{"program":"/bin/sh","args":[],"cwd":"/home/dev/teammate"}', ""]],
  profiles: [],
  grants: [],
};

/** The trust question for `path`: a first look, or an approved project that does more now. */
const asking = (path: string, first: boolean) => ({
  path,
  contributes: CONTRIBUTES,
  changes: first ? [] : ["the plugin superpowers@market is new"],
  first,
});

type Answers = (cmd: string, args: Record<string, unknown>) => unknown;

/** A core whose answers a scenario decides first, falling through to a window that comes up. */
function core(answers: Answers = () => undefined) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    const answer = answers(cmd, given);
    if (answer !== undefined) return answer;
    if (cmd === "plugin:event|listen") return 1;
    if (cmd === "plane_at_launch") return { plane: ONE, from: ONE, why: null };
    if (cmd === "relaunch_ask") return null;
    if (cmd === "planes_to_restore") return { windows: [], dropped: [] };
    if (cmd === "recent_planes")
      return {
        planes: [{ path: ONE, name: "plane", approved: true }],
        dropped: [],
        forgetful: null,
      };
    if (cmd === "plane_sidebar")
      return {
        root: given.plane,
        workspaces: [],
        personas: ["steward"],
        persona: "steward",
        unfiled: [],
      };
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "start_chat") return { session: 1, label: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    return null;
  });
  return { calls: (cmd: string) => asked.filter((one) => one.cmd === cmd) };
}

/** The question a relaunch asks, with two projects' chats to put back. */
const RELAUNCH = (after_update: boolean): RelaunchQuestion => ({
  projects: [
    { plane: ONE, chats: 2, views: 0 },
    { plane: TWO, chats: 1, views: 0 },
  ],
  after_update,
});

/**
 * A launch that asks `question`, and whose restore holds a project of a teammate's that has
 * started doing more since it was approved, so restoring it asks the trust question again.
 */
function relaunching(question: RelaunchQuestion) {
  return core((cmd, args) => {
    if (cmd === "relaunch_ask") return question;
    if (cmd === "planes_to_restore")
      return { windows: [{ planes: [TWO], active: null }], dropped: [] };
    if (cmd === "open_plane" && args.path === TWO) return { plane: null, ask: asking(TWO, false) };
    if (cmd === "approve_plane") return args.path;
    return undefined;
  });
}

/** Answers the relaunch question, then the trust question the restore raises. */
async function relaunch(answer: "Reopen all sessions" | "Start fresh") {
  const person = userEvent.setup();
  const question = await screen.findByRole("alertdialog", { name: "Reopen your sessions?" });
  await person.click(within(question).getByRole("button", { name: answer }));
  const trust = await screen.findByRole("dialog", {
    name: "This project has changed since you approved it",
  });
  await person.click(within(trust).getByRole("button", { name: "Open it anyway" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  return person;
}

/** Starts a chat from a new tab, through the picker: the way to a turn after starting fresh. */
async function startAChat(person: ReturnType<typeof userEvent.setup>, start = "Start") {
  await person.click(await screen.findByRole("button", { name: "New tab" }));
  const picker = await screen.findByRole("dialog", { name: "Start a chat" });
  await person.click(within(picker).getByRole("button", { name: start }));
}

describe("a relaunch (#1078)", () => {
  it("costs two prompts when it reopens every chat: the question and a project's trust", async () => {
    const { calls } = relaunching(RELAUNCH(false));
    render(<App />);

    await relaunch("Reopen all sessions");

    expect(calls("relaunch").map((one) => one.args)).toEqual([{ choice: "ReopenAll" }]);
    // The reopened chats answer on their own: nothing more stands before a turn.
    expect(interrupts.asked()).toEqual([
      "Reopen your sessions?",
      "This project has changed since you approved it",
    ]);
  });

  it("costs three when it starts fresh, the last of them the chat's own picker", async () => {
    const { calls } = relaunching(RELAUNCH(false));
    render(<App />);

    const person = await relaunch("Start fresh");
    await startAChat(person);
    await vi.waitFor(() => expect(calls("start_chat")).toHaveLength(1));

    expect(calls("relaunch").map((one) => one.args)).toEqual([{ choice: "StartFresh" }]);
    expect(interrupts.asked()).toEqual([
      "Reopen your sessions?",
      "This project has changed since you approved it",
      "Start a chat",
    ]);
  });
});

/** One chat the window holds, as the update's warning is given it. */
function holding(name: string, state: Ending["state"]): Ending {
  return { key: `${ONE}/${name}`, project: "plane", name, harness: "claude", cwd: null, state };
}

describe("a restart to update (#1078)", () => {
  it("costs three prompts over a chat mid-turn when it reopens them, the update's own dialog among them", async () => {
    // Before the restart: the status line's button opens the update's dialog, and a chat
    // mid-turn is warned about. The dialog is counted although the operator opened it, as the
    // picker a New tab opens is: it is a dialog that waits for an answer (D-1078-2).
    const restart = vi.fn();
    const updates: Updates = {
      state: { kind: "installed", version: "0.2.0" },
      channel: "stable",
      check: () => {},
      install: () => {},
      choose: () => {},
      restart,
    };
    render(
      <UpdateItem
        updates={updates}
        chats={[holding("ide.1", "running"), holding("ide.2", "done")]}
      />,
    );
    const person = userEvent.setup();
    await person.click(screen.getByTestId("status-update"));
    await person.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Restart to update" }),
    );
    const warning = await screen.findByRole("alertdialog");
    await person.click(within(warning).getByRole("button", { name: "Restart now" }));
    expect(restart).toHaveBeenCalledTimes(1);
    const before = interrupts.asked();
    cleanup();

    // After it: the new version's launch asks what to put back, and the chats reopen.
    const { calls } = core((cmd) => (cmd === "relaunch_ask" ? RELAUNCH(true) : undefined));
    render(<App />);
    const question = await screen.findByRole("alertdialog", { name: "Reopen your sessions?" });
    expect(question).toHaveTextContent("purlis restarted to install an update");
    await person.click(within(question).getByRole("button", { name: "Reopen all sessions" }));
    await waitFor(() => expect(calls("relaunch")).toHaveLength(1));

    // The budget, spent to the last prompt: Start fresh here would ask a fourth, the picker
    // (#1078's checklist carries that finding).
    expect(before).toEqual(["Updates", "Restart to update"]);
    expect(interrupts.asked()).toEqual(["Updates", "Restart to update", "Reopen your sessions?"]);
  });
});

describe("opening someone else's project (#1078)", () => {
  it("costs two prompts: the trust question, then the first chat's picker", async () => {
    const { calls } = core((cmd, args) => {
      if (cmd === "plane_at_launch") return { plane: null, from: null, why: null };
      if (cmd === "open_plane") return { plane: null, ask: asking(TWO, true) };
      if (cmd === "approve_plane") return TWO;
      if (cmd === "start_options")
        return { ...START_OPTIONS, profiles: [{ ...START_OPTIONS.profiles[0], approval: "new" }] };
      if (cmd === "plane_sidebar" && args.plane === TWO)
        return { root: TWO, workspaces: [], personas: [], persona: null, unfiled: [] };
      return undefined;
    });
    render(<App />);

    const person = userEvent.setup();
    await person.type(await screen.findByLabelText("Or type a path"), TWO);
    await person.click(screen.getByRole("button", { name: "Open" }));
    const trust = await screen.findByRole("dialog", { name: "Open this project?" });
    await person.click(within(trust).getByRole("button", { name: "Open project" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // Its start command is new to this machine, so the picker is shown even with one harness.
    await startAChat(person, "Approve and start");
    await vi.waitFor(() => expect(calls("start_chat")).toHaveLength(1));

    expect(interrupts.asked()).toEqual(["Open this project?", "Start a chat"]);
  });
});

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

/** The chat the operator is reading when another chat hands work off. */
const READING: OpenChat = {
  session: 1,
  name: "steward 1",
  cwd: `${ONE}/workspaces/ide`,
  harness: "claude",
  in_front: true,
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
};

/** A dispatch to devops that no grant covers yet, held on the reading chat's tab. */
const HELD: DispatchPending = {
  plane: ONE,
  id: 7,
  session: 1,
  chat: "steward 1",
  asking: "steward",
  target: "devops",
  brief: "Check why the prod deploy is red.",
  brief_cut: false,
  brief_lines: 1,
  levels: ["chat", "you", "project"],
  locked: null,
  never_unread: null,
  works_in: null,
  works_in_missing: false,
  allowed_in: [],
  works_with: "devops works with its own access: vault team.",
  also: [],
  shown: "s0",
  task: null,
  task_cut: false,
  profile: null,
};

/**
 * A core holding the chat being read, which says when a handoff opened another; `held` is what
 * waits for a grant until an Allow takes it.
 */
function handingOff(held: DispatchPending[]) {
  const listeners = new Map<string, number>();
  let waiting = held;
  const { calls } = core((cmd, args) => {
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, handler);
      return 1;
    }
    if (cmd === "opened_chats") return [READING];
    // No sidebar, as `Handoff.test.tsx` has none: the strip then holds every chat of the project.
    if (cmd === "plane_sidebar") return null;
    if (cmd === "dispatch_grants_needed") return waiting;
    if (cmd === "allow_dispatch") {
      waiting = [];
      return null;
    }
    return undefined;
  });
  const arrive = (persona: string) => {
    const handler = listeners.get("handoff-arrived");
    if (handler === undefined) throw new Error("the window is not listening for handoffs");
    window.__TAURI_INTERNALS__.runCallback(handler, {
      event: "handoff-arrived",
      id: 1,
      payload: {
        plane: ONE,
        session: 2,
        name: "2",
        label: "deploy check",
        from: { name: "steward 1", workspace: "ide" },
        workspace: "ide",
        persona,
        harness: "claude",
      },
    });
  };
  return { calls, arrive };
}

/** The chat tab whose text ends in `name`: a persona's mark comes first. */
const tabNamed = (name: string) =>
  within(stripNamed("Tabs"))
    .getAllByRole("tab")
    .find((tab) => tab.textContent?.endsWith(name));

describe("a handoff (#1078)", () => {
  it("costs no prompt to the asking chat's own persona: the chat lands on a tab, behind", async () => {
    const { arrive } = handingOff([]);
    render(<App />);
    await waitFor(() => expect(tabNamed("steward 1")).toBeDefined());

    arrive("steward");

    await waitFor(() => expect(tabNamed("deploy check")).toBeDefined());
    expect(interrupts.asked()).toEqual([]);
  });

  it("costs one to another persona: the grant's Allow, which the counter cannot see", async () => {
    const { calls, arrive } = handingOff([HELD]);
    render(<App />);

    // A Notice is a `status` line, which the counter does not read, though nothing starts until
    // it is answered. It is counted here by hand (`interruptBudget.ts`, "What it cannot see").
    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    byHand.push("Dispatch to devops");
    await userEvent
      .setup()
      .click(within(notice).getByRole("button", { name: "Allow for this chat" }));
    await vi.waitFor(() => expect(calls("allow_dispatch")).toHaveLength(1));
    arrive("devops");

    await waitFor(() => expect(tabNamed("deploy check")).toBeDefined());
    expect([...interrupts.asked(), ...byHand]).toEqual(["Dispatch to devops"]);
  });
});
