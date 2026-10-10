import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { FirstTaskTab, type FirstTaskDoes } from "./FirstTaskTab";

/**
 * The First task tab on its own (FR-28), for what it reads from the machine and the disk: which
 * harness's program is found (#1698), and which of its chats already started, after a relaunch
 * (#945, in #1091). The tab through the first run is `FirstTask.test.tsx`.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/.local/share/purlis/local-project";
const CLONE = `${PLANE}/workspaces/widget/widget`;

const profile = (name: string, kind: string, isDefault = false) => ({
  name,
  kind,
  shown: name,
  source: "built-in",
  is_default: isDefault,
  approval: null,
  ready_to_type: true,
  harness: null,
  sandbox: null,
});

const START_OPTIONS = {
  profiles: [profile("claude", "claude", true), profile("codex", "codex"), profile("pi", "pi")],
  refused: [],
  personas: ["steward"],
  persona: "steward",
  persona_profiles: {},
  ignore_fix: null,
  declares_none: true,
};

const harness = (name: string, title: string, installed: boolean) => ({
  name,
  title,
  installed,
  signed_in: installed,
  installer: `install ${name}`,
  installer_page: `https://example.invalid/${name}`,
});

/** The machine as the harness setup tab reads it: claude and pi found, codex not. */
const NO_CODEX = {
  harnesses: [
    harness("claude", "Claude Code", true),
    harness("codex", "Codex", false),
    harness("pi", "Pi", true),
  ],
  local_models: [],
};

const piece = (name: string, branch: string | null, stale = false) => ({
  piece: name,
  path: `${PLANE}/workspaces/widget/.worktrees/widget/${name}`,
  branch,
  wired: true,
  stale,
  said: "",
  unclaimed: null,
});

function machine({
  found = NO_CODEX as unknown,
  pieces = [] as unknown,
}: { found?: unknown; pieces?: unknown } = {}) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "harness_setup_found") return found;
    if (cmd === "worktree_list") return pieces;
    return null;
  });
  return { calls: (cmd: string) => asked.filter((one) => one.cmd === cmd) };
}

/** What the plane does for the tab, with no run started this launch. */
const DOES: FirstTaskDoes = {
  start: () => Promise.resolve("not in this test"),
  showDiff: () => {},
  runs: {},
};

const runSection = (name: string) => screen.getByRole("region", { name });

describe("the First task tab, on this machine (#1698)", () => {
  it("never suggests for the second chat a harness whose program is not installed", async () => {
    machine();
    render(<FirstTaskTab plane={PLANE} clone={CLONE} does={DOES} />);

    const run2 = await screen.findByRole("radiogroup", { name: "Second chat" });
    await waitFor(() => expect(within(run2).getByRole("radio", { name: "pi" })).toBeChecked());
    expect(within(run2).getByRole("radio", { name: "codex" })).not.toBeChecked();
  });

  it("says on its row that its program is not installed, and cannot be started", async () => {
    machine();
    render(<FirstTaskTab plane={PLANE} clone={CLONE} does={DOES} />);

    const run1 = await screen.findByRole("radiogroup", { name: "First chat" });
    const codex = within(run1).getByRole("radio", { name: "codex" });
    await waitFor(() => expect(codex).toBeDisabled());
    expect(within(run1).getByText("Codex is not installed on this machine.")).toBeInTheDocument();
    expect(codex).toHaveAttribute("title", "Codex is not installed on this machine.");
  });

  it("changes nothing when every program is installed", async () => {
    machine({
      found: {
        harnesses: [
          harness("claude", "Claude Code", true),
          harness("codex", "Codex", true),
          harness("pi", "Pi", true),
        ],
        local_models: [],
      },
    });
    render(<FirstTaskTab plane={PLANE} clone={CLONE} does={DOES} />);

    const run2 = await screen.findByRole("radiogroup", { name: "Second chat" });
    await waitFor(() => expect(within(run2).getByRole("radio", { name: "codex" })).toBeChecked());
    expect(within(run2).getByRole("radio", { name: "codex" })).toBeEnabled();
    expect(screen.queryByText(/is not installed/)).not.toBeInTheDocument();
  });

  it("offers every profile as before when the machine could not be read", async () => {
    mockIPC((cmd) => {
      if (cmd === "start_options") return START_OPTIONS;
      if (cmd === "harness_setup_found") throw "purlis could not look at this machine";
      if (cmd === "worktree_list") return [];
      return null;
    });
    render(<FirstTaskTab plane={PLANE} clone={CLONE} does={DOES} />);

    const run2 = await screen.findByRole("radiogroup", { name: "Second chat" });
    await waitFor(() => expect(within(run2).getByRole("radio", { name: "codex" })).toBeChecked());
    expect(within(run2).getByRole("radio", { name: "codex" })).toBeEnabled();
  });

  it("starts no built-in profile before the machine has been looked at", async () => {
    let answer: (found: unknown) => void = () => {};
    const look = new Promise((resolve) => (answer = resolve));
    mockIPC((cmd) => {
      if (cmd === "start_options") return START_OPTIONS;
      if (cmd === "harness_setup_found") return look;
      if (cmd === "worktree_list") return [];
      return null;
    });
    render(<FirstTaskTab plane={PLANE} clone={CLONE} does={DOES} />);

    const run1 = await screen.findByRole("radiogroup", { name: "First chat" });
    // A quick press while the look is out could start codex, whose program is missing.
    await waitFor(() => expect(within(run1).getByRole("radio", { name: "claude" })).toBeChecked());
    const start = within(runSection("First chat")).getByRole("button", {
      name: "Start the first chat",
    });
    expect(start).toBeDisabled();

    answer(NO_CODEX);
    await waitFor(() => expect(start).toBeEnabled());
  });

  it("judges only a built-in profile: one the project defines names its own command", async () => {
    mockIPC((cmd) => {
      if (cmd === "start_options")
        return {
          ...START_OPTIONS,
          profiles: [
            profile("claude", "claude", true),
            { ...profile("codex-mine", "codex"), source: "purlis.local.toml" },
          ],
        };
      if (cmd === "harness_setup_found") return NO_CODEX;
      if (cmd === "worktree_list") return [];
      return null;
    });
    render(<FirstTaskTab plane={PLANE} clone={CLONE} does={DOES} />);

    const run1 = await screen.findByRole("radiogroup", { name: "First chat" });
    await waitFor(() => expect(within(run1).getByRole("radio", { name: "claude" })).toBeChecked());
    expect(within(run1).getByRole("radio", { name: "codex-mine" })).toBeEnabled();
    expect(screen.queryByText(/is not installed/)).not.toBeInTheDocument();
  });
});

describe("the First task tab after a relaunch (#945)", () => {
  it("shows a chat whose branch is in the clone as started, and offers no second start", async () => {
    const { calls } = machine({
      pieces: [piece("first-task-1", "first-task-1"), piece("fix-login", "fix-login")],
    });
    render(<FirstTaskTab plane={PLANE} clone={CLONE} does={DOES} />);

    const first = runSection("First chat");
    expect(
      await within(first).findByText(/Started on the branch first-task-1/),
    ).toBeInTheDocument();
    expect(
      within(first).queryByRole("button", { name: "Start the first chat" }),
    ).not.toBeInTheDocument();
    // The second never started: its Start is offered.
    expect(
      within(runSection("Second chat")).getByRole("button", { name: "Start the second chat" }),
    ).toBeInTheDocument();
    expect(calls("worktree_list")[0].args).toEqual({
      plane: PLANE,
      workspace: "widget",
      repo: "widget",
    });
  });

  it("offers Start for a branch git no longer has a folder for", async () => {
    machine({ pieces: [piece("first-task-1", "first-task-1", true)] });
    render(<FirstTaskTab plane={PLANE} clone={CLONE} does={DOES} />);

    await screen.findByRole("radiogroup", { name: "First chat" });
    await waitFor(() =>
      expect(
        within(runSection("First chat")).getByRole("button", { name: "Start the first chat" }),
      ).toBeEnabled(),
    );
    expect(screen.queryByText(/Started on the branch/)).not.toBeInTheDocument();
  });

  it("keeps the run this launch started, with its diff, over what the disk says", async () => {
    machine({ pieces: [piece("first-task-1", "first-task-1")] });
    const run = {
      session: 11,
      name: "11",
      label: "first task 1",
      persona: "steward",
      harness: "claude",
      workspace: "widget",
      branch: "first-task-1",
      folder: `${PLANE}/workspaces/widget/.worktrees/widget/first-task-1`,
      diff: "git add -N -A && git diff 1a2b3c4d --",
    };
    render(
      <FirstTaskTab
        plane={PLANE}
        clone={CLONE}
        does={{ ...DOES, runs: { [CLONE]: { 1: run } } }}
      />,
    );

    const first = runSection("First chat");
    expect(within(first).getByText(/Started on the branch first-task-1/)).toBeInTheDocument();
    expect(within(first).getByRole("button", { name: "Show its diff" })).toBeInTheDocument();
  });
});
