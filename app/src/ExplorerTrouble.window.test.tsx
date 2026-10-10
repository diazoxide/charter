import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { showTheChanges, showTheExplorer } from "./test-strips";
import App from "./App";
import { forgetThisLaunch } from "./regions";

/**
 * **The explorer's refused reads have their way out** (NO-4, V91b): a workspace whose state
 * could not be read, or a clone whose branches could not be listed, says so as a Notice with
 * **Read again**, and the line goes once a read succeeds. The bottom bar draws the same refusal
 * and stays unpressable (ADR 0038): its way out is this one, on the same read.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;

type Asked = { cmd: string; args: Record<string, unknown> };

/** One workspace with one clone and one branch, whose reads are refused the first times. */
function core({ repos = 0, branches = 0 }: { repos?: number; branches?: number } = {}) {
  const asked: Asked[] = [];
  let refuseRepos = repos;
  let refuseBranches = branches;
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: a });
    if (cmd === "workspace_repos" && refuseRepos > 0) {
      refuseRepos--;
      throw REPOS_REFUSED;
    }
    if (cmd === "worktree_list" && refuseBranches > 0) {
      refuseBranches--;
      throw BRANCHES_REFUSED;
    }
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: ["steward"],
        persona: "steward",
        unfiled: [],
        workspaces: [{ name: "alpha", path: ALPHA, vision: "", todos: [], chats: [] }],
      };
    if (cmd === "workspace_panels")
      return {
        workspace: a.workspace,
        repos: ["svc"],
        paths: { svc: `${ALPHA}/svc` },
        absent: [],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["steward"],
        persona: "steward",
      };
    if (cmd === "workspace_repos")
      return { workspace: a.workspace, repos: [], cache_refused: null };
    if (cmd === "worktree_list")
      return [
        {
          piece: "one",
          path: `${ALPHA}/.worktrees/svc/one`,
          branch: "one",
          wired: true,
          stale: false,
        },
      ];
    if (cmd === "branch_tree")
      return a.folder === "" && a.piece === "one"
        ? {
            entries: [
              { name: "src", kind: "folder", ignored: false, refused: null },
              { name: "README.md", kind: "file", ignored: false, refused: null },
            ],
            more: 0,
          }
        : { entries: [], more: 0 };
    if (cmd === "branch_status") return { changes: [], folders: [], more: 0, base: "main" };
    if (cmd === "branch_ahead_behind") return { ahead: 3, behind: 0, base: "main" };
    if (cmd === "reopened_focus") return null;
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "alerts_everywhere") return [{ plane: PLANE, alerts: [], stopped: null }];
    return null;
  });
  return asked;
}

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
});

const REPOS_REFUSED = "git status timed out in svc";
const BRANCHES_REFUSED = "purlis will not run git through a symlink";
const count = (asked: Asked[], cmd: string) => asked.filter((one) => one.cmd === cmd).length;
const explorer = () => screen.getByTestId("explorer");

describe("a read the explorer was refused", () => {
  it("offers Read again, and goes once the workspace reads", async () => {
    const asked = core({ repos: 1 });
    render(<App />);
    await showTheExplorer();

    const notice = (
      await within(await screen.findByTestId("explorer")).findByText(REPOS_REFUSED)
    ).closest("[data-cause]") as HTMLElement;
    expect(notice.getAttribute("data-cause")).toBe("workspace-read:alpha");
    // The Changes view says it too, and has nothing to press (ADR 0038, #1676).
    const changes = await showTheChanges();
    expect(within(changes).getByText(REPOS_REFUSED)).toBeVisible();
    expect(within(changes).queryAllByRole("button")).toEqual([]);
    await showTheExplorer();
    const before = count(asked, "workspace_repos");

    await userEvent.click(within(notice).getByRole("button", { name: "Read again" }));

    await waitFor(() => expect(count(asked, "workspace_repos")).toBeGreaterThan(before));
    await waitFor(() => expect(screen.queryByText(REPOS_REFUSED)).toBeNull());
  });

  it("offers Read again on a clone whose branches could not be listed", async () => {
    const asked = core({ branches: 1 });
    render(<App />);
    await showTheExplorer();

    const notice = (
      await within(await screen.findByTestId("explorer")).findByText(/through a symlink/)
    ).closest("[data-cause]") as HTMLElement;
    expect(notice.getAttribute("data-cause")).toBe("branches-read:alpha/svc");
    const before = count(asked, "worktree_list");

    await userEvent.click(within(notice).getByRole("button", { name: "Read again" }));

    await waitFor(() => expect(count(asked, "worktree_list")).toBeGreaterThan(before));
    await within(explorer()).findByRole("treeitem", { name: /^one/ });
    expect(within(explorer()).queryByText(/through a symlink/)).toBeNull();
  });
});
