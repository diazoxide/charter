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
    { cli: "gh", forge: "github", title: "GitHub", installed: true, signed_in: false },
    { cli: "glab", forge: "gitlab", title: "GitLab", installed: true, signed_in: false },
  ],
  templates: [
    { id: "go", title: "Go", summary: "A Go module." },
    { id: "rust", title: "Rust", summary: "A Cargo crate or workspace." },
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
        opened: {
          opened: { plane: LOCAL, ask: null },
          workspace: "widget",
          cwd: CLONE,
          harness: null,
          instructions: 0,
        },
        asks_forge: null,
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

describe("the first run's project template (FR-17)", () => {
  it("offers every template, starting on the one that fits the repo", async () => {
    core();
    render(<App />);

    const group = await screen.findByRole("radiogroup", { name: "Project template" });
    // What fits the repo, every template the core ships, and none.
    expect(within(group).getAllByRole("radio")).toHaveLength(4);
    expect(within(group).getByRole("radio", { name: "Fits the repo" })).toBeChecked();
    expect(within(group).getByRole("radio", { name: "Rust" })).not.toBeChecked();
    expect(within(group).getByRole("radio", { name: "None" })).not.toBeChecked();
    expect(within(group).getByText("A Cargo crate or workspace.")).toBeInTheDocument();
  });

  it("says which template fits the repo whose path is typed", async () => {
    const { calls } = core((cmd, args) =>
      cmd === "template_that_fits" ? (args.path === REPO ? "rust" : null) : undefined,
    );
    render(<App />);

    const group = await screen.findByRole("radiogroup", { name: "Project template" });
    await userEvent.setup().type(screen.getByLabelText("Or type the repo's path"), REPO);

    const fits = within(group).getByRole("radio", { name: "Fits the repo" });
    await waitFor(() => expect(fits).toHaveAccessibleDescription(/Rust/));
    expect(calls("template_that_fits").at(-1)?.args).toEqual({ path: REPO });
  });

  it("opens the repo with the template the operator picked", async () => {
    const { calls } = core();
    render(<App />);

    const group = await screen.findByRole("radiogroup", { name: "Project template" });
    await userEvent.setup().click(within(group).getByRole("radio", { name: "Rust" }));
    await openRepoByPath(REPO);

    await vi.waitFor(() =>
      expect(calls("open_repo").map((one) => one.args)).toEqual([
        { path: REPO, template: { kind: "named", id: "rust" }, forge: null },
      ]),
    );
  });

  it("opens the repo with no template when the operator picks none", async () => {
    const { calls } = core();
    render(<App />);

    const group = await screen.findByRole("radiogroup", { name: "Project template" });
    await userEvent.setup().click(within(group).getByRole("radio", { name: "None" }));
    await openRepoByPath(REPO);

    await vi.waitFor(() =>
      expect(calls("open_repo").map((one) => one.args)).toEqual([
        { path: REPO, template: { kind: "no-template" }, forge: null },
      ]),
    );
  });
});

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

    expect(calls("open_repo").map((one) => one.args)).toEqual([
      { path: REPO, template: { kind: "fits" }, forge: null },
    ]);
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
          opened: {
            opened: { plane: null, ask: ASK },
            workspace: "widget",
            cwd: CLONE,
            harness: null,
            instructions: 0,
          },
          asks_forge: null,
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
          opened: {
            opened: { plane: LOCAL, ask: null },
            workspace: "widget",
            cwd: CLONE,
            harness: "claude",
            instructions: 0,
          },
          asks_forge: null,
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
          opened: {
            opened: { plane: LOCAL, ask: null },
            workspace: "widget",
            cwd: CLONE,
            harness: "claude",
            instructions: 0,
          },
          asks_forge: null,
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
          opened: {
            opened: { plane: LOCAL, ask: null },
            workspace: "widget",
            cwd: CLONE,
            harness: "claude",
            instructions: 0,
          },
          asks_forge: null,
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
    // With no repo to read, the forge whose sign-in was pressed is the project's (#839).
    expect(calls("open_local_project")[0].args).toEqual({ forge: "gitlab" });
  });

  it("asks which forge when the repo's remote does not say, and opens it with the answer", async () => {
    const { calls } = core((cmd, args) => {
      if (cmd === "open_repo" && args.forge === null)
        return { opened: null, asks_forge: `${REPO} has no \`origin\` remote` };
      return undefined;
    });
    render(<App />);

    const person = await openRepoByPath(REPO);
    const question = await screen.findByRole("group", { name: "Which forge are its repos on?" });
    expect(question).toHaveTextContent(`${REPO} has no \`origin\` remote`);
    expect(calls("start_chat")).toHaveLength(0);
    await person.click(within(question).getByRole("button", { name: "GitLab" }));

    await vi.waitFor(() =>
      expect(calls("open_repo").map((one) => one.args)).toEqual([
        { path: REPO, template: { kind: "fits" }, forge: null },
        { path: REPO, template: { kind: "fits" }, forge: "gitlab" },
      ]),
    );
    expect(await screen.findByRole("dialog", { name: "Start a chat" })).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Which forge are its repos on?" })).toBeNull();
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
          opened: {
            opened: { plane: LOCAL, ask: null },
            workspace: "widget",
            cwd: CLONE,
            harness: "claude",
            instructions: 2,
          },
          asks_forge: null,
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
          opened: {
            opened: { plane: LOCAL, ask: null },
            workspace: "widget",
            cwd: CLONE,
            harness: "claude",
            instructions: 0,
          },
          asks_forge: null,
        };
      return undefined;
    });
    render(<App />);

    await openRepoByPath(REPO);

    // The chat's tab is drawn, which is when an offer would have been made beside it.
    const strip = screen.getByRole("tablist", { name: "Tabs" });
    await within(strip).findByRole("tab", { selected: true });
    await waitFor(() => expect(calls("start_chat")).toHaveLength(1));
    // The chat, and the first task offered beside it (FR-28), and nothing else.
    await within(strip).findByRole("tab", { name: /First task · widget/ });
    expect(within(strip).getAllByRole("tab")).toHaveLength(2);
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

/**
 * FR-29 (W10): a machine with no harness. The first chat is a tab with each harness's official
 * installer, run in a shell tab on a press; the harness's own login is its own first screen;
 * and a local model server already on the machine is named as the way in with no account.
 * Which installer, where it installs, and what counts as a local server are
 * `charter_core::noharness`'s, tested there.
 */
describe("no harness found (FR-29)", () => {
  const NOTHING = {
    ...FOUND,
    harnesses: [
      {
        name: "claude",
        title: "Claude Code",
        installed: false,
        signed_in: false,
        installer: "curl -fsSL https://claude.ai/install.sh | bash",
        installer_page: "https://code.claude.com/docs/en/setup",
      },
      {
        name: "codex",
        title: "Codex",
        installed: false,
        signed_in: false,
        installer: "curl -fsSL https://chatgpt.com/codex/install.sh | sh",
        installer_page: "https://github.com/openai/codex",
      },
      {
        name: "opencode",
        title: "opencode",
        installed: false,
        signed_in: false,
        installer: "curl -fsSL https://opencode.ai/install | bash",
        installer_page: "https://opencode.ai/docs/",
      },
    ],
    local_models: [],
  };

  /** A machine with no harness: `found` is what the next look at it finds. */
  function noHarness(found: () => unknown = () => NOTHING) {
    return core((cmd) => {
      if (cmd === "first_run_found") return found();
      // The setup tab looks at harnesses and local models only, never the forge CLIs.
      if (cmd === "harness_setup_found") {
        const all = found() as typeof NOTHING;
        return { harnesses: all.harnesses, local_models: all.local_models };
      }
      if (cmd === "start_options")
        return {
          ...START_OPTIONS,
          profiles: [
            ...START_OPTIONS.profiles,
            { ...START_OPTIONS.profiles[0], name: "codex", kind: "codex", is_default: false },
          ],
        };
      if (cmd === "open_repo")
        return {
          opened: {
            opened: { plane: LOCAL, ask: null },
            workspace: "widget",
            cwd: CLONE,
            harness: null,
            none_installed: true,
            instructions: 0,
          },
          asks_forge: null,
        };
      if (cmd === "open_session") return 7;
      return undefined;
    });
  }

  it("says on the first run that a harness is installed after the repo is open", async () => {
    noHarness();
    render(<App />);

    expect(await screen.findByText(/No harness is installed on this machine/)).toBeInTheDocument();
  });

  it("reaches a working chat: the official installer in a shell tab, then the picker", async () => {
    let installed = false;
    const { calls } = noHarness(() =>
      installed
        ? {
            ...NOTHING,
            harnesses: [
              NOTHING.harnesses[0],
              { ...NOTHING.harnesses[1], installed: true },
              NOTHING.harnesses[2],
            ],
          }
        : NOTHING,
    );
    render(<App />);

    const person = await openRepoByPath(REPO);

    // Not the picker: there is nothing on this machine it could start.
    const strip = await screen.findByRole("tablist", { name: "Tabs" });
    const setup = await within(strip).findByRole("tab", { name: /Set up a harness · widget/ });
    expect(setup).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByRole("dialog", { name: "Start a chat" })).not.toBeInTheDocument();
    const pane = await screen.findByRole("region", { name: "Set up a harness · widget" });
    expect(within(pane).getByRole("heading", { name: "No harness found" })).toBeInTheDocument();
    // Each vendor's own command, shown before it is run.
    expect(
      within(pane).getByText("curl -fsSL https://claude.ai/install.sh | bash"),
    ).toBeInTheDocument();
    expect(calls("open_session")).toHaveLength(0);

    await person.click(within(pane).getByRole("button", { name: "Install Codex" }));

    // The core types its own compiled-in command (V65): the window names the harness, never
    // the words, and nothing it sends is a runnable string.
    await vi.waitFor(() => expect(calls("type_installer")).toHaveLength(1));
    expect(calls("type_installer")[0].args).toEqual({ plane: LOCAL, session: 7, harness: "codex" });
    expect(calls("send_input")).toHaveLength(0);
    // At the project's root, not in the fresh clone, whose own shell hooks never run for it.
    expect(calls("open_session")[0].args).toMatchObject({
      plane: LOCAL,
      program: null,
      cwd: LOCAL,
    });
    const forgeLooks = calls("first_run_found").length;

    // The installer finished in its shell; the operator comes back and looks again.
    installed = true;
    await person.click(within(strip).getByRole("tab", { name: /Set up a harness · widget/ }));
    const back = await screen.findByRole("region", { name: "Set up a harness · widget" });
    await person.click(within(back).getByRole("button", { name: "Check again" }));
    await person.click(await within(back).findByRole("button", { name: "Start a chat" }));
    expect(calls("first_run_found")).toHaveLength(forgeLooks);

    // The picker, as every chat's start (ADR 0022), on the harness just installed: its own
    // login is its first screen.
    const picker = await screen.findByRole("dialog", { name: "Start a chat" });
    await person.click(within(picker).getByRole("button", { name: "Start" }));
    await vi.waitFor(() => expect(calls("start_chat")).toHaveLength(1));
    expect(calls("start_chat")[0].args).toMatchObject({
      plane: LOCAL,
      cwd: CLONE,
      profile: "codex",
    });
  });

  it("names a local model server already on the machine as the way in with no account", async () => {
    noHarness(() => ({
      ...NOTHING,
      local_models: [
        { title: "Ollama", base_url: "http://127.0.0.1:11434/v1", harness: "opencode" },
      ],
    }));
    render(<App />);

    await openRepoByPath(REPO);

    const pane = await screen.findByRole("region", { name: "Set up a harness · widget" });
    const local = await within(pane).findByRole("heading", { name: "A model on this machine" });
    expect(local.parentElement).toHaveTextContent(/Ollama is running on this machine/);
    expect(local.parentElement).toHaveTextContent("http://127.0.0.1:11434/v1");
    expect(local.parentElement).toHaveTextContent(/opencode/);
  });
});
