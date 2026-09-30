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
 * What the core does with the repository — the local plane in charter's own directory, the
 * workspace named after the repository, nothing written into it — is `charter_core::firstrun`'s
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
  forge: { cli: "gh", installed: true, signed_in: false },
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
      return { opened: { plane: LOCAL, ask: null }, workspace: "widget", cwd: CLONE };
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
  await person.type(await screen.findByLabelText("Or type the repository's path"), path);
  await person.click(screen.getByRole("button", { name: "Open" }));
  return person;
}

describe("the first run", () => {
  it("asks for a repository and nothing about where the project goes", async () => {
    core();
    render(<App />);

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "Open a repository to start",
    );
    expect(screen.getByRole("button", { name: "Open a repository…" })).toBeInTheDocument();
    // The plane is charter's to place (W10): no folder for it, and no word of accounts, cloud
    // or telemetry on the way to a chat.
    expect(screen.queryByLabelText("Folder")).not.toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/account|cloud|telemetry|sign up/i);
  });

  it("says which harnesses are installed and signed in, and whether gh is", async () => {
    core();
    render(<App />);

    const found = await screen.findByRole("list", { name: "On this machine" });
    await waitFor(() => expect(within(found).getAllByRole("listitem")).toHaveLength(4));
    const rows = within(found)
      .getAllByRole("listitem")
      .map((row) => row.textContent);
    expect(rows).toEqual([
      "Claude Code: ready",
      "Codex: installed; it asks you to sign in when its chat starts",
      "opencode: not installed",
      "gh: installed, not signed in; only needed to work with GitHub",
    ]);
  });

  it("opens the repository into the local project and starts the first chat in its clone", async () => {
    const { calls } = core();
    render(<App />);

    const person = await openRepoByPath(REPO);

    expect(calls("open_repo").map((one) => one.args)).toEqual([{ path: REPO }]);
    // The first chat is asked for by itself: the harness picker, which ADR 0022 keeps.
    const picker = await screen.findByRole("dialog", { name: "Start a chat" });
    await person.click(within(picker).getByRole("button", { name: "Start" }));

    await vi.waitFor(() => expect(calls("start_chat")).toHaveLength(1));
    expect(calls("start_chat")[0].args).toMatchObject({ plane: LOCAL, cwd: CLONE });
    // In the workspace named after the repository, not the plane root.
    expect(calls("workspace_focused").map((one) => one.args.workspace)).toContain("widget");
  });

  it("asks the trust question first when the local project needs it, then starts the chat", async () => {
    const { calls } = core((cmd) => {
      if (cmd === "open_repo")
        return { opened: { plane: null, ask: ASK }, workspace: "widget", cwd: CLONE };
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
      "/home/dev/papers is not the top level of a git working tree, so there is no repository to open.";
    core((cmd) => {
      if (cmd === "open_repo") throw refusal;
      return undefined;
    });
    render(<App />);

    await openRepoByPath("/home/dev/papers");

    expect(await screen.findByRole("alert")).toHaveTextContent(refusal);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      "Open a repository to start",
    );
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
});
