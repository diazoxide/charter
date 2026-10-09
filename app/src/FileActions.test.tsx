import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
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
import { forgetYourEditor, setYourEditor } from "./yourEditor";
import { stripNamed } from "./test-strips";

/**
 * **A file or folder row's actions, against the whole window** (FM-10, #1113): each row of its
 * menu reaches the core as the branch and the path inside it — never a directory the window
 * joined — and what the core answers is said. The rows themselves are `actions.test.ts`'s; what
 * the core does with the path is `piecefiles.rs`'s and `files::place`'s.
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

/** One workspace with one clone and one branch, holding `src/` and `README.md`. */
function core({ refuse }: { refuse?: string } = {}) {
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
    if (cmd === "copy_branch_path" || cmd === "reveal_branch_path") {
      if (refuse !== undefined) throw refuse;
      return null;
    }
    if (cmd === "open_shell_in_branch") return 41;
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
  forgetYourEditor();
});

const ONE = { workspace: "alpha", repo: "svc", piece: "one" };
const asks = (asked: Asked[], cmd: string) =>
  asked.filter((one) => one.cmd === cmd).map((one) => one.args);

/** Opens the branch's files, then the menu on `name`'s row, and presses `row` in it. */
async function fromTheMenu(name: string, row: string | RegExp) {
  const tree = await screen.findByRole("tree", { name: "Repos and branches" });
  const files = await waitFor(() => {
    const found = tree.querySelector<HTMLElement>('[data-row="file:svc/one:"]');
    if (found === null) throw new Error("the branch's Files row is not drawn yet");
    return found;
  });
  if (files.getAttribute("aria-expanded") !== "true") await userEvent.click(files);
  fireEvent.contextMenu(
    await within(tree).findByRole("treeitem", { name: new RegExp(`^${name}`) }),
  );
  const menu = await screen.findByRole("menu");
  await userEvent.click(within(menu).getByRole("menuitem", { name: row }));
}

describe("a file or folder row's actions (FM-10)", () => {
  it("copies a path through the core, by branch and path, and says so", async () => {
    const asked = core();
    render(<App />);

    await fromTheMenu("README.md", "Copy absolute path");

    await waitFor(() =>
      expect(asks(asked, "copy_branch_path")).toEqual([
        { plane: PLANE, ...ONE, path: "README.md", absolute: true },
      ]),
    );
    expect(await screen.findByText("Copied the absolute path of README.md.")).toBeInTheDocument();
  });

  it("says the core's refusal in its own words", async () => {
    core({ refuse: "'README.md' is not in the branch's folder any more" });
    render(<App />);

    await fromTheMenu("README.md", "Copy relative path");

    expect(
      await screen.findByText("'README.md' is not in the branch's folder any more"),
    ).toBeInTheDocument();
  });

  it("reveals a folder by branch and path", async () => {
    const asked = core();
    render(<App />);

    await fromTheMenu("src", /^Reveal in /);

    await waitFor(() =>
      expect(asks(asked, "reveal_branch_path")).toEqual([{ plane: PLANE, ...ONE, path: "src" }]),
    );
  });

  it("opens a shell tab in a folder the core resolves, and draws its tab", async () => {
    const asked = core();
    render(<App />);

    await fromTheMenu("src", "Open a shell tab here");

    await waitFor(() =>
      expect(asks(asked, "open_shell_in_branch")).toEqual([
        expect.objectContaining({ plane: PLANE, ...ONE, folder: "src", name: "shell 1" }),
      ]),
    );
    expect(asks(asked, "open_session")).toEqual([]);
    expect(
      await within(stripNamed("Tabs")).findByRole("tab", {
        name: /shell 1/,
      }),
    ).toBeInTheDocument();
  });

  it("opens a file in your editor at its first line, or asks for an editor first", async () => {
    const asked = core();
    render(<App />);

    await fromTheMenu("README.md", "Open in your editor");
    expect(await screen.findByText("Choose your editor in Settings first.")).toBeInTheDocument();

    setYourEditor("zed");
    await fromTheMenu("README.md", "Open in your editor");
    await waitFor(() =>
      expect(asks(asked, "open_in_your_editor")).toEqual([
        { plane: PLANE, ...ONE, path: "README.md", line: 1, editor: "zed" },
      ]),
    );
  });
});
