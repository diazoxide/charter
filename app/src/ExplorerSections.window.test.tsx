import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { showTheExplorer, stripNamed } from "./test-strips";
import App from "./App";
import { forgetThisLaunch } from "./regions";
import { GLOBAL } from "./windowprefs";

/**
 * **Explorer in sections** (#1677, spec #1671 B-12): *Workspaces*, with the focused one marked;
 * the focused workspace's repos and branches; and *Files*, of where the next chat starts. Each
 * section folds, and the fold is kept in the layout file for the next launch.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";
const at = (workspace: string) => `${PLANE}/workspaces/${workspace}`;

type Asked = { cmd: string; args: Record<string, unknown> };

/** Two workspaces: alpha clones `svc` with one branch, beta clones `web` with none. */
function core() {
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
        workspaces: ["alpha", "beta"].map((name) => ({
          name,
          path: at(name),
          vision: "",
          todos: [],
          chats: [],
        })),
      };
    if (cmd === "workspace_panels") {
      const repo = a.workspace === "beta" ? "web" : "svc";
      return {
        workspace: a.workspace,
        repos: [repo],
        paths: { [repo]: `${at(String(a.workspace))}/${repo}` },
        absent: [],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["steward"],
        persona: "steward",
      };
    }
    if (cmd === "workspace_repos")
      return { workspace: a.workspace, repos: [], cache_refused: null };
    if (cmd === "worktree_list")
      return a.workspace === "beta" || a.repo === "web"
        ? []
        : [
            {
              piece: "one",
              path: `${at("alpha")}/.worktrees/svc/one`,
              branch: "one",
              wired: true,
              stale: false,
            },
          ];
    if (cmd === "branch_tree")
      return a.folder === ""
        ? {
            entries:
              a.piece === "one"
                ? [{ name: "lib.rs", kind: "file", ignored: false, refused: null }]
                : [{ name: "README.md", kind: "file", ignored: false, refused: null }],
            more: 0,
          }
        : { entries: [], more: 0 };
    if (cmd === "branch_status") return { changes: [], folders: [], more: 0, base: "main" };
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
  Reflect.deleteProperty(globalThis, GLOBAL);
});

const explorer = () => within(screen.getByRole("navigation", { name: "Explorer" }));
const workspacesTree = () => explorer().getByRole("tree", { name: "Workspaces of this project" });

describe("Explorer in sections (#1677)", () => {
  it("draws Workspaces, the focused workspace's repos and branches, and Files, each open", async () => {
    core();
    render(<App />);
    await showTheExplorer();
    await explorer().findByRole("tree", { name: "Workspaces of this project" });

    expect(
      explorer()
        .getAllByRole("button", { expanded: true })
        .filter((one) => one.closest("h2") !== null)
        .map((one) => one.textContent),
    ).toEqual(["Workspaces", "Repos and branches", expect.stringMatching(/^Files/)]);
    expect(explorer().getByRole("tree", { name: "Repos and branches" })).toBeVisible();
    expect(explorer().getByRole("tree", { name: /^Files of/ })).toBeVisible();
  });

  it("marks the focused workspace with aria-current, and only it", async () => {
    core();
    render(<App />);
    await showTheExplorer();
    const tree = await explorer().findByRole("tree", { name: "Workspaces of this project" });

    await within(tree).findByRole("treeitem", { name: /^beta/ });
    expect(within(tree).getByRole("treeitem", { name: /^alpha/ })).toHaveAttribute(
      "aria-current",
      "true",
    );
    expect(within(tree).getByRole("treeitem", { name: /^beta/ })).not.toHaveAttribute(
      "aria-current",
    );
  });

  it("focuses a workspace from the section exactly as its tab on the strip does", async () => {
    const asked = core();
    render(<App />);
    await showTheExplorer();
    const beta = await within(
      await explorer().findByRole("tree", {
        name: "Workspaces of this project",
      }),
    ).findByRole("treeitem", { name: /^beta/ });

    await userEvent.click(beta);

    // The strip moved, which is the axis (ADR 0036), and the explorer is beta's now.
    await waitFor(() =>
      expect(within(stripNamed("Workspaces")).getByRole("tab", { name: /beta/ })).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
    expect(within(workspacesTree()).getByRole("treeitem", { name: /^beta/ })).toHaveAttribute(
      "aria-current",
      "true",
    );
    await explorer().findByTestId("clone-web");
    // The extensions heard it, as they do from the strip (charter-app#343).
    expect(
      asked.some((one) => one.cmd === "workspace_focused" && one.args.workspace === "beta"),
    ).toBe(true);
  });

  it("draws the picked branch's files in Files, and each repo's own folder before one is picked", async () => {
    core();
    render(<App />);
    await showTheExplorer();
    await explorer().findByTestId("clone-svc");

    const before = explorer().getByRole("tree", { name: "Files of alpha" });
    expect(await within(before).findByRole("treeitem", { name: /^svc/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );

    await userEvent.click(await explorer().findByRole("treeitem", { name: /^one/ }));

    const after = await explorer().findByRole("tree", { name: "Files of one in svc" });
    expect(await within(after).findByRole("treeitem", { name: /^lib\.rs/ })).toBeVisible();
    // A branch is one row of Repos and branches: its files are drawn once, here.
    expect(
      within(explorer().getByRole("tree", { name: "Repos and branches" })).queryByRole("treeitem", {
        name: /^lib\.rs/,
      }),
    ).not.toBeInTheDocument();
  });

  it("folds a section on its heading, keeping it mounted, and keeps the fold for the next launch", async () => {
    const asked = core();
    render(<App />);
    await showTheExplorer();
    const tree = await explorer().findByRole("tree", { name: "Workspaces of this project" });
    const heading = explorer().getByRole("button", { name: "Workspaces" });

    await userEvent.click(heading);

    expect(heading).toHaveAttribute("aria-expanded", "false");
    // Hidden, never unmounted: the same element, out of sight and out of the tab order.
    expect(tree.isConnected).toBe(true);
    expect(tree).not.toBeVisible();
    expect(tree.querySelector('[tabindex="0"]')).toBeNull();
    await waitFor(() => expect(written(asked)?.explorer).toEqual({ closed: ["workspaces"] }));

    await userEvent.click(heading);

    expect(heading).toHaveAttribute("aria-expanded", "true");
    expect(tree).toBeVisible();
    // Every section open is the default, which the file leaves out.
    await waitFor(() => expect(written(asked)).not.toHaveProperty("explorer"));
  });

  it("launches with the sections the layout file says are folded", async () => {
    (globalThis as Record<string, unknown>)[GLOBAL] = {
      layout: {
        path: "layout.json",
        found: true,
        document: { version: 2, regions: [], explorer: { closed: ["files"] } },
        trouble: null,
      },
      theme: { path: "theme.json", found: false, document: null, trouble: null },
    };
    core();
    render(<App />);
    await showTheExplorer();
    await explorer().findByTestId("clone-svc");

    expect(explorer().getByRole("button", { name: /^Files/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(explorer().getByRole("button", { name: "Workspaces" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(explorer().getByTestId("files")).not.toBeVisible();
  });
  it("gives the keyboard to the focused workspace's row when its key shows Explorer", async () => {
    core();
    render(<App />);
    await screen.findByRole("tab", { name: "Explorer" });
    await explorer_ready();

    // jsdom is not a Mac, so the key is Ctrl's (`sideKeys.ts`).
    await userEvent.keyboard("{Control>}{Shift>}E{/Shift}{/Control}");

    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(workspacesTree()).getByRole("treeitem", { name: /^alpha/ }),
      ),
    );
  });
});

/** The explorer once the project is read, whether or not it is the view open. */
async function explorer_ready() {
  await screen.findByTestId("clone-svc");
}

/** The last layout the window wrote, as a document. */
function written(asked: Asked[]): Record<string, unknown> | undefined {
  const last = asked.filter((one) => one.cmd === "write_layout").at(-1);
  return last === undefined ? undefined : JSON.parse(String(last.args.text));
}
