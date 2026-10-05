import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Explorer } from "./Explorer";
import { ChatsHere, fixedChats, nothingKnown } from "./chatState";
import type { BranchStatus, FolderEntry, Panels as PanelsModel, Piece, PlaneId } from "./bindings";
import type { WorkspaceState } from "./workspaceState";

/**
 * **What a branch changed, in the explorer** (FM-4): the render states the real-app scenario
 * (`workspace-explorer.e2e.ts`) does not reach on its fixture — a file the branch deleted, a
 * rename's source, a folder's count, *Changed only* down several folders, and the filter keeping
 * the folders on the way to a match. The core is mocked; what it answers for a real branch is
 * `a_branch_marks_what_it_changed.rs`'s.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/plane" as unknown as PlaneId;

const PANELS: PanelsModel = {
  workspace: "alpha",
  repos: ["svc"],
  paths: {},
  absent: [],
  refused: [],
  todos: [],
  todos_refused: null,
  personas: [],
  persona: "steward",
  sessions: [],
  contributed: [],
};

const ONE: Piece = {
  piece: "one",
  path: "/plane/workspaces/alpha/.worktrees/svc/one",
  branch: "one",
  wired: true,
  stale: false,
  said: "",
};

const STATE: WorkspaceState = {
  panels: PANELS,
  repos: { workspace: "alpha", repos: [], cache_refused: null },
  pieces: { svc: [ONE] },
  piecesRefused: {},
  reading: false,
};

const file = (name: string): FolderEntry => ({ name, kind: "file", ignored: false, refused: null });
const folder = (name: string): FolderEntry => ({
  name,
  kind: "folder",
  ignored: false,
  refused: null,
});

/** A branch that changed README.md, added src/deep/new.rs, deleted src/gone.rs and renamed
 *  lib/old.rs to lib/new.rs. */
const CHANGED: BranchStatus = {
  changes: [
    { path: "README.md", mark: "changed", from: null, uncommitted: true },
    { path: "lib/new.rs", mark: "renamed", from: "lib/old.rs", uncommitted: false },
    { path: "src/deep/new.rs", mark: "added", from: null, uncommitted: true },
    { path: "src/gone.rs", mark: "deleted", from: null, uncommitted: true },
  ],
  folders: [
    { folder: "", mark: "changed", count: 4 },
    { folder: "lib", mark: "renamed", count: 1 },
    { folder: "src", mark: "changed", count: 2 },
    { folder: "src/deep", mark: "added", count: 1 },
  ],
  more: 0,
  base: "main",
};

/** The core: the branch's folders by `<folder>`, and what it changed. */
function core(folders: Record<string, FolderEntry[]>, status: { now: BranchStatus }) {
  const asked: string[] = [];
  mockIPC(
    (cmd, args) => {
      const a = args as Record<string, string | null>;
      if (cmd === "branch_tree") {
        asked.push(`tree:${a.folder}`);
        return { entries: folders[a.folder ?? ""] ?? [], more: 0 };
      }
      if (cmd === "branch_status") {
        asked.push("status");
        return status.now;
      }
      if (cmd === "files_watch" || cmd === "branch_watch") return null;
      throw new Error(`unexpected ${cmd}`);
    },
    { shouldMockEvents: true },
  );
  return asked;
}

function draw() {
  render(
    <ChatsHere.Provider value={fixedChats(nothingKnown)}>
      <Explorer
        plane={PLANE}
        workspace="alpha"
        state={STATE}
        chats={[]}
        spot={undefined}
        onPick={() => {}}
        onShowChat={() => {}}
        offers={new Map()}
        onPress={() => {}}
        onReadAgain={() => {}}
        onOpenFile={() => {}}
      />
    </ChatsHere.Provider>,
  );
}

const tree = () => screen.getByRole("tree", { name: "Repos and branches" });
const row = (id: string) => {
  const found = tree().querySelector<HTMLElement>(`[data-row="${id}"]`);
  if (found === null) throw new Error(`no row ${id}`);
  return found;
};
const named = (name: string) => within(tree()).findByRole("treeitem", { name: new RegExp(name) });
const drawnNames = () =>
  [...tree().querySelectorAll<HTMLElement>('[data-row^="file:svc/one:"]')].map(
    (one) => one.dataset.row,
  );

const TOP = {
  "": [folder("lib"), folder("src"), file("README.md")],
  lib: [file("new.rs")],
  src: [folder("deep"), file("keep.rs")],
};

describe("what a branch changed, in the explorer", () => {
  it("marks each file with what the branch did to it and each folder with how many it holds", async () => {
    core(TOP, { now: CHANGED });
    draw();
    await userEvent.click(row("file:svc/one:"));
    await userEvent.click(await named("^lib"));
    await userEvent.click(await named("^src"));

    expect(await named("^README.md")).toHaveAccessibleName("README.md changed");
    expect(await named("^new.rs")).toHaveAccessibleName("new.rs renamed from lib/old.rs");
    expect(await named("^src")).toHaveAccessibleName("src 2 changes");
    expect(row("file:svc/one:")).toHaveAccessibleName("Files 4 changes");
    expect(row("file:svc/one:src/keep.rs")).toHaveAccessibleName("keep.rs");
  });

  it("draws a file the branch deleted where it was, and it does not open", async () => {
    core(TOP, { now: CHANGED });
    draw();
    await userEvent.click(row("file:svc/one:"));
    await userEvent.click(await named("^src"));

    const gone = await named("^gone.rs");

    expect(gone).toHaveAttribute("aria-disabled", "true");
    expect(gone).toHaveTextContent("deleted on this branch");
    expect(gone.closest("li")).toHaveAttribute("data-mark", "deleted");
  });

  it("collapses to what the branch changed, every folder of it open, with no folder read", async () => {
    const asked = core(TOP, { now: CHANGED });
    draw();
    await userEvent.click(row("file:svc/one:"));
    await named("^README.md");

    await userEvent.click(screen.getByRole("button", { name: "Changed only" }));

    await waitFor(() =>
      expect(drawnNames()).toEqual([
        "file:svc/one:",
        "file:svc/one:lib",
        "file:svc/one:lib/new.rs",
        "file:svc/one:src",
        "file:svc/one:src/deep",
        "file:svc/one:src/deep/new.rs",
        "file:svc/one:src/gone.rs",
        "file:svc/one:README.md",
      ]),
    );
    expect(asked.filter((one) => one.startsWith("tree:"))).toEqual(["tree:"]);
  });

  it("says when a branch changed nothing", async () => {
    core(TOP, {
      now: { changes: [], folders: [], more: 0, base: "main" },
    });
    draw();
    await userEvent.click(row("file:svc/one:"));
    await named("^README.md");
    await userEvent.click(screen.getByRole("button", { name: "Changed only" }));

    expect(await screen.findByText("Nothing changed")).toBeInTheDocument();
  });

  it("counts the changes past the most the core marks under Changed only", async () => {
    core(TOP, { now: { ...CHANGED, more: 12_345 } });
    draw();
    await userEvent.click(row("file:svc/one:"));
    await named("^README.md");

    await userEvent.click(screen.getByRole("button", { name: "Changed only" }));

    expect(await screen.findByText("12,345 more changes not shown")).toBeInTheDocument();
  });
});
