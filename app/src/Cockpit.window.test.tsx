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
import { forgetThisLaunch } from "./regions";

/**
 * **The branch cockpit, against the whole window** (FM-5, #1108): focusing a branch from its
 * row's menu, the focus told to the core so the record keeps it with the window's views, and a
 * focus the record put back drawn as the cockpit. The cockpit's own render states are
 * `Explorer.cockpit.test.tsx`'s; the record is `reopen.rs`'s and `chats.rs`'s.
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

/** How the clone's branch listing answers: with its one branch, never, or refused. */
type Listing = "answers" | "pending" | "refused";

/** One workspace with one clone and one branch, and the focus the record put back. */
function core({
  focus = null,
  listing = "answers",
}: { focus?: Record<string, unknown> | null; listing?: Listing } = {}) {
  const asked: Asked[] = [];
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: a });
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
    if (cmd === "worktree_list" && listing === "pending") return new Promise(() => {});
    if (cmd === "worktree_list" && listing === "refused")
      return Promise.reject(new Error("git could not list the branches"));
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
    if (cmd === "reopened_focus") return focus;
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

const ONE = { workspace: "alpha", repo: "svc", piece: "one" };
const said = (asked: Asked[]) =>
  asked.filter((one) => one.cmd === "window_focus").map((one) => one.args.focus);

describe("the branch cockpit in the window (FM-5)", () => {
  it("focuses a branch from its row's menu, and tells the core, and steps back out on Esc", async () => {
    const asked = core();
    render(<App />);
    const tree = await screen.findByRole("tree", { name: "Repos and branches" });

    fireEvent.contextMenu(await within(tree).findByRole("treeitem", { name: /^one/ }));
    const menu = await screen.findByRole("menu");
    await userEvent.click(within(menu).getByRole("menuitem", { name: "Focus on branch one" }));

    const head = await screen.findByRole("region", { name: "Branch one" });
    await within(head).findByText("3 ahead, 0 behind main");
    await waitFor(() => expect(said(asked).at(-1)).toEqual(ONE));

    screen
      .getByRole("tree", { name: "Chats and files of one" })
      .querySelector<HTMLElement>("[data-row]")
      ?.focus();
    await userEvent.keyboard("{Escape}");

    await screen.findByRole("tree", { name: "Repos and branches" });
    await waitFor(() => expect(said(asked).at(-1)).toBeNull());
  });

  describe("a focus whose branch was removed (#1152)", () => {
    const GONE = { workspace: "alpha", repo: "svc", piece: "gone" };
    /** Every answer the first draw asked for, landed. */
    const settle = async () => {
      for (let turn = 0; turn < 20; turn += 1) await act(async () => {});
    };

    it("is forgotten once the listing answers without the branch", async () => {
      const asked = core({ focus: GONE });
      render(<App />);

      await screen.findByRole("tree", { name: "Repos and branches" });
      await waitFor(() => expect(said(asked).at(-1)).toBeNull());
      // And nothing tells the core it again.
      await settle();
      expect(said(asked).at(-1)).toBeNull();
    });

    it("is kept while the listing has not answered", async () => {
      const asked = core({ focus: GONE, listing: "pending" });
      render(<App />);

      await waitFor(() => expect(said(asked)).toContainEqual(GONE));
      await settle();
      expect(said(asked)).not.toContain(null);
    });

    it("is kept when the listing was refused, which says nothing of the branch", async () => {
      const asked = core({ focus: GONE, listing: "refused" });
      render(<App />);

      await waitFor(() => expect(said(asked)).toContainEqual(GONE));
      await waitFor(() => expect(asked.some((one) => one.cmd === "worktree_list")).toBe(true));
      await settle();
      expect(said(asked)).not.toContain(null);
    });
  });

  it("draws the focus the record put back as the cockpit", async () => {
    core({ focus: ONE });
    render(<App />);

    await screen.findByRole("region", { name: "Branch one" });
    expect(screen.queryByRole("tree", { name: "Repos and branches" })).toBeNull();
  });
});
