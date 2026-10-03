import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { wordsOutsideTheFirstHour } from "./firstHour";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

/**
 * The first task (FR-28, #621): the guided task FR-1 measures, offered beside the first chat.
 *
 * What a run is — its branch, the task typed and unsent, the diff command — is the core's
 * (`charter_core::firsttask`, `firsttask.rs`), and the CI run of the script is
 * `crates/charter-cli/tests/first_task_script.rs`. This is about the window: that the tab is
 * offered without asking anything, which profile each run starts on, and that a run's diff is
 * one press away.
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

const LOCAL = "/home/dev/.config/charter/local-plane";
const REPO = "/home/dev/widget";
const CLONE = `${LOCAL}/workspaces/widget/widget`;

const SIDEBAR = {
  root: LOCAL,
  workspaces: [
    { name: "widget", path: `${LOCAL}/workspaces/widget`, vision: "", todos: [], chats: [] },
  ],
  personas: ["steward"],
  persona: "steward",
  unfiled: [],
};

/** What the core says of opencode's missing capability: its card's line and label (HP-19). */
const OPENCODE_CANNOT_TYPE =
  "opencode cannot have a prompt typed in for you, because charter cannot tell when it has finished starting. See What opencode can do here.";

const profile = (name: string, kind: string, approval: string | null, isDefault = false) => ({
  name,
  kind,
  shown: name,
  source: "built-in",
  is_default: isDefault,
  approval,
  ready_to_type: kind !== "opencode",
  // Its harness's card at a glance (HP-19), as the core answers it for opencode.
  ...(kind === "opencode" && {
    harness: {
      name: "opencode",
      title: "opencode",
      label: "What opencode can do here",
      lines: [],
      cannot_type: OPENCODE_CANNOT_TYPE,
    },
  }),
});

const START_OPTIONS = {
  profiles: [
    profile("claude", "claude", null, true),
    profile("codex", "codex", "new"),
    profile("opencode", "opencode", null),
  ],
  refused: [],
  personas: ["steward"],
  persona: "steward",
  ignore_fix: null,
  declares_none: true,
};

const FOUND = {
  harnesses: [{ name: "claude", title: "Claude Code", installed: true, signed_in: true }],
  forges: [],
  templates: [],
};

/** A run as the core answers it, for run `run` on `harness`. */
function aRun(run: number, harness: string) {
  return {
    session: 10 + run,
    name: String(10 + run),
    label: `first task ${run}`,
    persona: "steward",
    harness,
    workspace: "widget",
    branch: `first-task-${run}`,
    folder: `${LOCAL}/workspaces/widget/.worktrees/widget/first-task-${run}`,
    diff: "git add -N -A && git diff 1a2b3c4d --",
  };
}

function core(answers: (cmd: string, args: Record<string, unknown>) => unknown = () => undefined) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    const answer = answers(cmd, given);
    if (answer !== undefined) return answer;
    if (cmd === "plane_at_launch") return { plane: null, from: null, why: null };
    if (cmd === "recent_planes") return { planes: [], dropped: [], forgetful: null };
    if (cmd === "first_run_found") return FOUND;
    if (cmd === "open_repo")
      return {
        opened: {
          opened: { plane: LOCAL, ask: null },
          workspace: "widget",
          cwd: CLONE,
          harness: "claude",
          instructions: 0,
        },
        asks_forge: null,
      };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "start_chat") return { session: 1, label: null };
    if (cmd === "first_task_run") {
      const run = given.run as number;
      return aRun(run, given.profile as string);
    }
    if (cmd === "approve_profile") return null;
    if (cmd === "open_session") return 30;
    if (cmd === "opened_chats") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    return null;
  });
  return { asked, calls: (cmd: string) => asked.filter((one) => one.cmd === cmd) };
}

/** The first run, through to its first chat, and the first task's tab opened. */
async function openTheFirstTask() {
  const person = userEvent.setup();
  await person.type(await screen.findByLabelText("Or type the repo's path"), REPO);
  await person.click(screen.getByRole("button", { name: "Open" }));
  const strip = screen.getByRole("tablist", { name: "Tabs" });
  await person.click(await within(strip).findByRole("tab", { name: /First task · widget/ }));
  const pane = await screen.findByRole("region", { name: "First task · widget" });
  return { person, pane, strip };
}

/** Back to the first task's tab, which a run's chat opening in front left behind. */
async function backToTheTask(person: ReturnType<typeof userEvent.setup>, strip: HTMLElement) {
  await person.click(await within(strip).findByRole("tab", { name: /First task · widget/ }));
  return screen.findByRole("region", { name: "First task · widget" });
}

describe("the first task", () => {
  it("is offered in a tab beside the first chat, and asks nothing", async () => {
    const { calls } = core();
    render(<App />);
    const person = userEvent.setup();
    await person.type(await screen.findByLabelText("Or type the repo's path"), REPO);
    await person.click(screen.getByRole("button", { name: "Open" }));

    await waitFor(() => expect(calls("start_chat")).toHaveLength(1));
    const strip = screen.getByRole("tablist", { name: "Tabs" });
    const offer = await within(strip).findByRole("tab", { name: /First task · widget/ });
    // Beside the chat, not in front of it: the chat is what the operator came for.
    expect(offer).toHaveAttribute("aria-selected", "false");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(calls("first_task_run")).toHaveLength(0);
  });

  it("starts run 1 on the project's default profile, in the repo's clone", async () => {
    const { calls } = core();
    render(<App />);
    const { person, pane, strip } = await openTheFirstTask();

    const run1 = within(pane).getByRole("radiogroup", { name: "First chat" });
    await waitFor(() => expect(within(run1).getByRole("radio", { name: "claude" })).toBeChecked());
    await person.click(within(pane).getByRole("button", { name: "Start the first chat" }));

    await waitFor(() => expect(calls("first_task_run")).toHaveLength(1));
    expect(calls("first_task_run")[0].args).toEqual({
      plane: LOCAL,
      cwd: CLONE,
      profile: "claude",
      persona: "steward",
      run: 1,
      columns: 80,
      rows: 24,
    });
    // Its chat opens in front, where the typed task waits for the operator's Enter.
    expect(await within(strip).findByRole("tab", { name: /first task 1/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    const again = await backToTheTask(person, strip);
    expect(within(again).getByText(/Started on the branch first-task-1/)).toBeInTheDocument();
  });

  it("starts run 2 on another harness than run 1's, approving its command on the press", async () => {
    const { asked, calls } = core();
    render(<App />);
    const { person, pane: first, strip } = await openTheFirstTask();
    await person.click(within(first).getByRole("button", { name: "Start the first chat" }));
    await waitFor(() => expect(calls("first_task_run")).toHaveLength(1));
    const pane = await backToTheTask(person, strip);

    const run2 = within(pane).getByRole("radiogroup", { name: "Second chat" });
    expect(within(run2).getByRole("radio", { name: "codex" })).toBeChecked();
    // The picker's own sentence, command and mark are in front of the operator before the press
    // (V69).
    const sentence = within(pane).getByRole("alert");
    expect(sentence).toHaveTextContent(
      "charter has not run this profile before. It would run: codex",
    );
    expect(within(sentence).getByText("codex").tagName).toBe("CODE");
    expect(within(run2).getByText("new")).toHaveClass("needs-approval");
    // It says what the second run starts knowing.
    expect(
      within(pane).getByText(/lesson the first chat recorded is in its memory/),
    ).toBeInTheDocument();
    await person.click(
      within(pane).getByRole("button", { name: "Approve and start the second chat" }),
    );

    await waitFor(() => expect(calls("first_task_run")).toHaveLength(2));
    expect(calls("approve_profile")[0].args).toEqual({
      plane: LOCAL,
      name: "codex",
      shown: "codex",
    });
    const order = asked.filter(
      (one) => one.cmd === "approve_profile" || one.cmd === "first_task_run",
    );
    expect(order.map((one) => one.cmd)).toEqual([
      "first_task_run",
      // Run 2's profile is approved before its run, and only on the press.
      "approve_profile",
      "first_task_run",
    ]);
    expect(calls("first_task_run")[1].args).toMatchObject({ profile: "codex", run: 2 });
  });

  it("uses only the first hour's words", async () => {
    core();
    render(<App />);
    const { pane } = await openTheFirstTask();
    await within(pane).findAllByRole("radio", { name: "codex" });

    // ADR 0072 §3: the whole pane, but the commands it shows and the picker's approval
    // sentence, whose words are the operator's ruling (V69).
    const said = pane.cloneNode(true) as HTMLElement;
    said.querySelectorAll("code, .approve").forEach((one) => one.remove());
    expect(wordsOutsideTheFirstHour(said.textContent ?? "")).toEqual([]);
  });

  it("says when a command changed since it was approved, as the picker does", async () => {
    core((cmd) =>
      cmd === "start_options"
        ? {
            ...START_OPTIONS,
            profiles: [
              profile("claude", "claude", null, true),
              profile("codex", "codex", "changed"),
            ],
          }
        : undefined,
    );
    render(<App />);
    const { pane } = await openTheFirstTask();

    const run1 = within(pane).getByRole("radiogroup", { name: "First chat" });
    await userEvent.setup().click(within(run1).getByRole("radio", { name: "codex" }));

    const sentence = await within(pane).findByText(/as it now stands/);
    expect(sentence).toHaveTextContent(
      "charter has not run this profile as it now stands. It would run: codex",
    );
    expect(within(run1).getByText("changed")).toHaveClass("needs-approval");
    expect(
      within(pane).getByRole("button", { name: "Approve and start the first chat" }),
    ).toBeEnabled();
  });

  it("never offers a profile charter cannot type the task into", async () => {
    core();
    render(<App />);
    const { pane } = await openTheFirstTask();

    const run1 = within(pane).getByRole("radiogroup", { name: "First chat" });
    const opencode = within(run1).getByRole("radio", { name: "opencode" });
    expect(opencode).toBeDisabled();
    // The off control says what its harness lacks, in its card's line and label (HP-19, ADR
    // 0072 §3): beside it, and as its tooltip.
    expect(within(run1).getByText(OPENCODE_CANNOT_TYPE)).toBeInTheDocument();
    expect(opencode).toHaveAttribute("title", OPENCODE_CANNOT_TYPE);
  });

  it("shows a run's diff in a shell in its branch's folder", async () => {
    const { calls } = core();
    render(<App />);
    const { person, pane: first, strip } = await openTheFirstTask();
    await person.click(within(first).getByRole("button", { name: "Start the first chat" }));
    await waitFor(() => expect(calls("first_task_run")).toHaveLength(1));
    const pane = await backToTheTask(person, strip);

    await person.click(await within(pane).findByRole("button", { name: "Show its diff" }));

    await waitFor(() => expect(calls("send_input")).toHaveLength(1));
    expect(calls("open_session")[0].args).toMatchObject({
      plane: LOCAL,
      program: null,
      cwd: `${LOCAL}/workspaces/widget/.worktrees/widget/first-task-1`,
    });
    expect(calls("send_input")[0].args).toMatchObject({
      text: "git add -N -A && git diff 1a2b3c4d --\n",
    });
  });

  it("says in full why a run did not start", async () => {
    const why = "Profile 'claude' runs a program charter has not measured.";
    core((cmd) => {
      if (cmd === "first_task_run") throw why;
      return undefined;
    });
    render(<App />);
    const { person, pane } = await openTheFirstTask();

    await person.click(within(pane).getByRole("button", { name: "Start the first chat" }));

    expect(await within(pane).findByText(why)).toHaveAttribute("role", "alert");
    expect(within(pane).getByRole("button", { name: "Start the first chat" })).toBeEnabled();
  });
});
