import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

/**
 * The first run (FR-4, #603): a new machine reaches a working chat with no prompt about
 * accounts, cloud or telemetry.
 *
 * What the core does with the repo — the local plane in charter's own directory, the
 * workspace named after the repo, nothing written into it — is `charter_core::firstrun`'s
 * and is tested there against real directories. This is about the window: what it asks, what
 * it does not, and that the first chat starts in that workspace's clone.
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

const FOUND = {
  harnesses: [
    { name: "claude", title: "Claude Code", installed: true, signed_in: true },
    { name: "codex", title: "Codex", installed: true, signed_in: false },
    { name: "opencode", title: "opencode", installed: false, signed_in: false },
  ],
  forges: [
    { cli: "gh", title: "GitHub", installed: true, signed_in: false },
    { cli: "glab", title: "GitLab", installed: true, signed_in: false },
  ],
};

const ASK = {
  path: LOCAL,
  contributes: { plugins: [], env: [], starts: [], profiles: [], grants: [] },
  changes: [],
  first: true,
};

/** A new machine: nothing at launch, nothing to put back, nothing remembered. */
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
        opened: { plane: LOCAL, ask: null },
        workspace: "widget",
        cwd: CLONE,
        harness: null,
        instructions: 0,
      };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "start_chat") return { session: 1, label: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    return null;
  });
  return { asked, calls: (cmd: string) => asked.filter((one) => one.cmd === cmd) };
}

async function openRepoByPath(path: string) {
  const person = userEvent.setup();
  await person.type(await screen.findByLabelText("Or type the repo's path"), path);
  await person.click(screen.getByRole("button", { name: "Open" }));
  return person;
}

describe("the first run", () => {
  it("asks for a repo, says what a project is, and nothing about where it goes", async () => {
    core();
    render(<App />);

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "Open a repo to start",
    );
    expect(screen.getByRole("button", { name: "Open a repo…" })).toBeInTheDocument();
    // The plane is charter's to place (W10): no folder for it, and no word of accounts, cloud
    // or telemetry on the way to a chat.
    expect(screen.queryByLabelText("Folder")).not.toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/account|cloud|telemetry|sign up/i);
    // ADR 0072's words: what a project is, and no "plane" or "repository" on the way in.
    expect(
      screen.getByText(/A project is where charter keeps your workspaces, personas and memory/),
    ).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/plane|repository/i);
  });

  it("says which harnesses are installed and signed in, and whether gh and glab are", async () => {
    core();
    render(<App />);

    const found = await screen.findByRole("list", { name: "On this machine" });
    await waitFor(() => expect(within(found).getAllByRole("listitem")).toHaveLength(5));
    const rows = within(found)
      .getAllByRole("listitem")
      .map((row) => row.textContent);
    expect(rows).toEqual([
      "Claude Code: ready",
      "Codex: installed; it asks you to sign in when its chat starts",
      "opencode: not installed",
      "gh: installed, not signed in; only needed to work with GitHub Sign in to GitHub",
      "glab: installed, not signed in; only needed to work with GitLab Sign in to GitLab",
    ]);
  });

  it("opens the repo into the local project and starts the first chat in its clone", async () => {
    const { calls } = core();
    render(<App />);

    const person = await openRepoByPath(REPO);

    expect(calls("open_repo").map((one) => one.args)).toEqual([{ path: REPO }]);
    // The first chat is asked for by itself: the harness picker, which ADR 0022 keeps.
    const picker = await screen.findByRole("dialog", { name: "Start a chat" });
    await person.click(within(picker).getByRole("button", { name: "Start" }));

    await vi.waitFor(() => expect(calls("start_chat")).toHaveLength(1));
    expect(calls("start_chat")[0].args).toMatchObject({ plane: LOCAL, cwd: CLONE });
    // In the workspace named after the repo, not the project's root.
    expect(calls("workspace_focused").map((one) => one.args.workspace)).toContain("widget");
  });

  it("asks the trust question first when the local project needs it, then starts the chat", async () => {
    const { calls } = core((cmd) => {
      if (cmd === "open_repo")
        return {
          opened: { plane: null, ask: ASK },
          workspace: "widget",
          cwd: CLONE,
          harness: null,
          instructions: 0,
        };
      if (cmd === "approve_plane") return LOCAL;
      return undefined;
    });
    render(<App />);

    const person = await openRepoByPath(REPO);
    const asking = await screen.findByRole("dialog", { name: "Open this project?" });
    await person.click(within(asking).getByRole("button", { name: "Open project" }));

    const picker = await screen.findByRole("dialog", { name: "Start a chat" });
    await person.click(within(picker).getByRole("button", { name: "Start" }));
    await vi.waitFor(() => expect(calls("start_chat")).toHaveLength(1));
    expect(calls("start_chat")[0].args).toMatchObject({ plane: LOCAL, cwd: CLONE });
  });

  it("shows the core's refusal in full and stays on the first run", async () => {
    const refusal =
      "/home/dev/papers is not the top level of a git repo, so there is nothing to open.";
    core((cmd) => {
      if (cmd === "open_repo") throw refusal;
      return undefined;
    });
    render(<App />);

    await openRepoByPath("/home/dev/papers");

    expect(await screen.findByRole("alert")).toHaveTextContent(refusal);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Open a repo to start");
  });

  it("still opens a project that exists, one press away", async () => {
    core();
    render(<App />);

    await userEvent.click(
      await screen.findByRole("button", { name: "Open an existing project instead" }),
    );

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "You have not opened a project yet",
    );
    expect(screen.getByLabelText("Or type a path")).toBeInTheDocument();
  });

  it("is not shown on a machine that remembers a project", async () => {
    core((cmd) => {
      if (cmd === "recent_planes")
        return {
          planes: [{ name: "plane", path: "/home/dev/plane", approved: true }],
          dropped: [],
          forgetful: null,
        };
      return undefined;
    });
    render(<App />);

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "You have not opened a project yet",
    );
  });

  it("starts the first chat without the picker when exactly one harness is signed in", async () => {
    // W10's interrupt budget: nothing to pick, so nothing is asked (ADR 0022 still shows the
    // picker whenever there is a choice or a command to approve).
    const { calls } = core((cmd) => {
      if (cmd === "open_repo")
        return {
          opened: { plane: LOCAL, ask: null },
          workspace: "widget",
          cwd: CLONE,
          harness: "claude",
          instructions: 0,
        };
      return undefined;
    });
    render(<App />);

    await openRepoByPath(REPO);

    await vi.waitFor(() => expect(calls("start_chat")).toHaveLength(1));
    expect(calls("start_chat")[0].args).toMatchObject({
      plane: LOCAL,
      cwd: CLONE,
      profile: "claude",
    });
    expect(screen.queryByRole("dialog", { name: "Start a chat" })).not.toBeInTheDocument();
  });

  it("starts the first chat on a branch of its own and says which (GL-1)", async () => {
    // Nothing was asked, so the default stands, and the pane is the only place the operator
    // learns the chat is not on the repo's own branch (ADR 0072 §4).
    const line = "On branch chat-1 in widget, a branch of its own cut from main.";
    const { calls } = core((cmd) => {
      if (cmd === "open_repo")
        return {
          opened: { plane: LOCAL, ask: null },
          workspace: "widget",
          cwd: CLONE,
          harness: "claude",
          instructions: 0,
        };
      if (cmd === "start_chat") return { session: 1, label: null, notices: [line] };
      return undefined;
    });
    render(<App />);

    await openRepoByPath(REPO);

    await vi.waitFor(() => expect(calls("start_chat")).toHaveLength(1));
    expect(calls("start_chat")[0].args).toMatchObject({
      cwd: CLONE,
      boxes: { show_footer: false, new_branch: true },
    });
    expect(
      await screen.findByRole("status", { name: "What this chat's start found" }),
    ).toHaveTextContent(line);
  });

  it("still asks when the one signed-in harness's command has to be approved first", async () => {
    const { calls } = core((cmd) => {
      if (cmd === "open_repo")
        return {
          opened: { plane: LOCAL, ask: null },
          workspace: "widget",
          cwd: CLONE,
          harness: "claude",
          instructions: 0,
        };
      if (cmd === "start_options")
        return {
          ...START_OPTIONS,
          profiles: [{ ...START_OPTIONS.profiles[0], approval: "new" }],
        };
      return undefined;
    });
    render(<App />);

    await openRepoByPath(REPO);

    expect(await screen.findByRole("dialog", { name: "Start a chat" })).toBeInTheDocument();
    expect(calls("start_chat")).toHaveLength(0);
  });

  it("offers GitHub's own sign-in in a shell tab, and asks nothing", async () => {
    const { calls } = core((cmd) => {
      if (cmd === "open_local_project") return { plane: LOCAL, ask: null };
      if (cmd === "open_session") return 7;
      return undefined;
    });
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "Sign in to GitHub" }));

    await vi.waitFor(() => expect(calls("send_input")).toHaveLength(1));
    expect(calls("open_session")[0].args).toMatchObject({
      plane: LOCAL,
      program: null,
      cwd: LOCAL,
    });
    expect(calls("send_input")[0].args).toEqual({
      plane: LOCAL,
      session: 7,
      text: "gh auth login\n",
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("offers GitLab's own sign-in the same way, with glab", async () => {
    const { calls } = core((cmd) => {
      if (cmd === "open_local_project") return { plane: LOCAL, ask: null };
      if (cmd === "open_session") return 7;
      return undefined;
    });
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "Sign in to GitLab" }));

    await vi.waitFor(() => expect(calls("send_input")).toHaveLength(1));
    expect(calls("send_input")[0].args).toEqual({
      plane: LOCAL,
      session: 7,
      text: "glab auth login\n",
    });
  });
});

/**
 * FR-18a (#612): the agent instructions the repo carries — `CLAUDE.md`, `AGENTS.md` and
 * `.cursor/rules` — offered to the workspace's memory with a preview. What is found, what is
 * left out and what is written are `charter_core::repoinstructions`'s, tested against real
 * directories; this is about the window: the offer asks nothing (W10's interrupt budget), and
 * nothing is written without the preview's yes.
 */
describe("the repo's agent instructions", () => {
  const FILES = [
    {
      repo: "widget",
      file: "CLAUDE.md",
      text: "# Rules\n\nRun the tests first.\n",
      standing: { kind: "offered", caution: null },
    },
    {
      repo: "widget",
      file: ".cursor/rules/style.mdc",
      text: "Use tabs.\n",
      standing: { kind: "offered", caution: null },
    },
    {
      repo: "widget",
      file: "AGENTS.md",
      text: "",
      standing: {
        kind: "left-out",
        why: "line 1 holds what looks like a secret (credential assignment), and a secret never goes into memory",
      },
    },
  ];

  function withInstructions(extra: (cmd: string) => unknown = () => undefined) {
    return core((cmd) => {
      const instead = extra(cmd);
      if (instead !== undefined) return instead;
      if (cmd === "open_repo")
        return {
          opened: { plane: LOCAL, ask: null },
          workspace: "widget",
          cwd: CLONE,
          harness: "claude",
          instructions: 2,
        };
      if (cmd === "repo_instructions") return FILES;
      if (cmd === "import_instructions") return 2;
      return undefined;
    });
  }

  it("offers them in a tab beside the first chat, and asks nothing", async () => {
    const { calls } = withInstructions();
    render(<App />);

    await openRepoByPath(REPO);

    await vi.waitFor(() => expect(calls("start_chat")).toHaveLength(1));
    const strip = screen.getByRole("tablist", { name: "Tabs" });
    const offer = await within(strip).findByRole("tab", { name: /Memory from the repo · widget/ });
    // Beside the chat, not in front of it: the chat is what the operator came for.
    expect(offer).toHaveAttribute("aria-selected", "false");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(calls("import_instructions")).toHaveLength(0);
  });

  it("is not offered when the repo has none", async () => {
    const { calls } = core((cmd) => {
      if (cmd === "open_repo")
        return {
          opened: { plane: LOCAL, ask: null },
          workspace: "widget",
          cwd: CLONE,
          harness: "claude",
          instructions: 0,
        };
      return undefined;
    });
    render(<App />);

    await openRepoByPath(REPO);

    // The chat's tab is drawn, which is when an offer would have been made beside it.
    const strip = screen.getByRole("tablist", { name: "Tabs" });
    await within(strip).findByRole("tab", { selected: true });
    await waitFor(() => expect(calls("start_chat")).toHaveLength(1));
    expect(within(strip).getAllByRole("tab")).toHaveLength(1);
    expect(within(strip).queryByRole("tab", { name: /Memory from the repo/ })).toBeNull();
  });

  it("previews each file and writes only what is ticked, on the press", async () => {
    const { calls } = withInstructions();
    render(<App />);
    const person = await openRepoByPath(REPO);
    const strip = screen.getByRole("tablist", { name: "Tabs" });
    await person.click(
      await within(strip).findByRole("tab", { name: /Memory from the repo · widget/ }),
    );

    const pane = await screen.findByRole("region", { name: "Memory from the repo · widget" });
    // The whole text, before anything is written.
    expect(await within(pane).findByText(/Run the tests first\./)).toBeInTheDocument();
    expect(within(pane).getByText("Use tabs.")).toBeInTheDocument();
    // A file that cannot go into memory says why, and cannot be ticked.
    expect(within(pane).getByText(/a secret never goes into memory/)).toBeInTheDocument();
    expect(within(pane).queryByRole("checkbox", { name: "widget/AGENTS.md" })).toBeNull();
    expect(within(pane).getByText(/Nothing is written into your repo/)).toBeInTheDocument();
    expect(calls("import_instructions")).toHaveLength(0);

    await person.click(
      within(pane).getByRole("checkbox", { name: "widget/.cursor/rules/style.mdc" }),
    );
    await person.click(within(pane).getByRole("button", { name: "Add to memory" }));

    await vi.waitFor(() => expect(calls("import_instructions")).toHaveLength(1));
    expect(calls("import_instructions")[0].args).toEqual({
      plane: LOCAL,
      workspace: "widget",
      chosen: [{ repo: "widget", file: "CLAUDE.md", text: "# Rules\n\nRun the tests first.\n" }],
    });
  });

  it("uses only the first hour's words", async () => {
    withInstructions();
    render(<App />);
    const person = await openRepoByPath(REPO);
    const strip = screen.getByRole("tablist", { name: "Tabs" });
    await person.click(
      await within(strip).findByRole("tab", { name: /Memory from the repo · widget/ }),
    );

    const pane = await screen.findByRole("region", { name: "Memory from the repo · widget" });
    await within(pane).findByText(/Run the tests first\./);
    // ADR 0072 / V23: no "plane", "harness" or "repository" in what the tab says of its own.
    const own = [...pane.querySelectorAll("p, h2, button, label")]
      .map((one) => one.textContent ?? "")
      .join(" ");
    expect(own).not.toMatch(/plane|harness|repository/i);
  });

  it("shows invisible characters by their code point, and leaves a file with a caution unticked", async () => {
    const { calls } = withInstructions((cmd) =>
      cmd === "repo_instructions"
        ? [
            {
              repo: "widget",
              file: "CLAUDE.md",
              text: "Run\u200b the tests.\n",
              standing: {
                kind: "offered",
                caution: "line 1 holds an invisible character (U+200B)",
              },
            },
          ]
        : undefined,
    );
    render(<App />);
    const person = await openRepoByPath(REPO);
    const strip = screen.getByRole("tablist", { name: "Tabs" });
    await person.click(
      await within(strip).findByRole("tab", { name: /Memory from the repo · widget/ }),
    );

    const pane = await screen.findByRole("region", { name: "Memory from the repo · widget" });
    expect(await within(pane).findByText("U+200B", { selector: "mark" })).toBeInTheDocument();
    expect(within(pane).getByText(/holds an invisible character/)).toBeInTheDocument();
    const box = within(pane).getByRole("checkbox", { name: "widget/CLAUDE.md" });
    expect(box).toHaveAttribute("aria-checked", "false");
    expect(within(pane).getByRole("button", { name: "Add to memory" })).toBeDisabled();

    await person.click(box);
    await person.click(within(pane).getByRole("button", { name: "Add to memory" }));
    await waitFor(() => expect(calls("import_instructions")).toHaveLength(1));
    expect(calls("import_instructions")[0].args.chosen).toEqual([
      { repo: "widget", file: "CLAUDE.md", text: "Run\u200b the tests.\n" },
    ]);
  });

  it("reads the files again after a refusal, so the preview is what is on disk", async () => {
    const changed = "Has changed since. Look at it again, then add it.";
    // The file changes on disk at the moment the import is refused; React's development
    // double effect asks for the files twice on mount, so it is not a count of reads.
    let refused = false;
    const { calls } = withInstructions((cmd) => {
      if (cmd === "repo_instructions")
        return refused ? [{ ...FILES[0], text: "# Rules\n\nRun the new tests.\n" }] : [FILES[0]];
      if (cmd === "import_instructions") {
        refused = true;
        throw changed;
      }
      return undefined;
    });
    render(<App />);
    const person = await openRepoByPath(REPO);
    const strip = screen.getByRole("tablist", { name: "Tabs" });
    await person.click(
      await within(strip).findByRole("tab", { name: /Memory from the repo · widget/ }),
    );
    const pane = await screen.findByRole("region", { name: "Memory from the repo · widget" });
    await within(pane).findByText(/Run the tests first\./);

    await person.click(within(pane).getByRole("button", { name: "Add to memory" }));

    expect(await within(pane).findByRole("alert")).toHaveTextContent(changed);
    expect(await within(pane).findByText(/Run the new tests\./)).toBeInTheDocument();
    expect(calls("import_instructions")).toHaveLength(1);
  });
});
