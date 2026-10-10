import { afterAll, afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { FolderEntry, PlaneId } from "../bindings";
import { expectEveryControlNamed, paidDebts } from "../a11y.testkit";
import { BranchTree } from "./BranchTree";
import { LightEditor, MergeViewer } from "./LightEditor";
import { PieceDiffTab } from "./PieceDiff";
import { PieceFileTab, PieceFilesTab } from "./PieceFiles";
import { gitHunks } from "./gitdiff.testkit";

/**
 * **Every control an editor tab draws has a name** (DS-6, #629): the light editor and the
 * comparison, a branch's files, one file, and what changed in it, each drawn with what the core
 * answers and held to `expectEveryControlNamed`. The rule and its own cases are
 * `a11y.testkit.ts` and `a11y.names.test.tsx`.
 */

/**
 * Nameless controls drawn by files this lane does not hold (DS-6's follow-ups), each as the
 * check writes it. It only shrinks: a debt no tab draws any more fails the last test.
 */
const DEBT: readonly string[] = [];

const found: string[] = [];
const held = (root: HTMLElement, where: string) =>
  found.push(...expectEveryControlNamed(root, DEBT, where));

const PLANE = "/plane" as unknown as PlaneId;
const CUT = { workspace: "alpha", repo: "svc", piece: "fix-it" };

const file = (name: string): FolderEntry => ({ name, kind: "file", ignored: false, refused: null });
const folder = (name: string): FolderEntry => ({ ...file(name), kind: "folder" });

/** The core as the editor tabs ask it: a branch of one folder and two files, `src/lib.rs`
 *  changed against main. */
function core() {
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, string>;
      switch (cmd) {
        case "branch_tree":
          return {
            entries: a.folder === "" ? [folder("src"), file("README.md")] : [file("lib.rs")],
            more: 0,
          };
        case "branch_status":
          return {
            changes: [{ path: "src/lib.rs", mark: "changed", from: null, uncommitted: true }],
            folders: [],
            more: 0,
            base: "main",
          };
        case "piece_file":
          return a.path === "README.md"
            ? { kind: "text", text: "# svc\n" }
            : { kind: "text", text: "pub fn one() {}\n" };
        case "what_changed":
          return {
            mark: "changed",
            from: null,
            uncommitted: true,
            base: "main",
            diff: {
              kind: "text",
              base: "one\ntwo\n",
              head: "one\nTWO\n",
              hunks: [{ oldStart: 2, oldLines: 1, newStart: 2, newLines: 1 }],
            },
          };
        case "worktree_list":
          return [];
        case "opened_chats":
          return [];
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("every control an editor tab draws has a name", () => {
  it("in the light editor", async () => {
    const { container } = render(<LightEditor path="src/lib.rs" text={"pub fn one() {}\n"} />);
    await waitFor(() => expect(container.querySelector(".cm-content")).not.toBeNull());

    held(container, "the light editor");
  });

  it("in the comparison", async () => {
    const { container } = render(
      <MergeViewer
        path="src/lib.rs"
        base={"one\ntwo\n"}
        head={"one\nTWO\n"}
        hunks={gitHunks("one\ntwo\n", "one\nTWO\n")}
      />,
    );
    await waitFor(() => expect(container.querySelector(".cm-mergeView")).not.toBeNull());

    held(container, "the comparison");
  });

  it("in a branch's tree", async () => {
    core();
    const { container } = render(
      <BranchTree plane={PLANE} place={CUT} onPick={() => undefined} />,
    );
    await screen.findByRole("treeitem", { name: "README.md" });

    held(container, "a branch's tree");
  });

  it("in a branch's files, with a file open in the preview", async () => {
    core();
    const { container } = render(
      <PieceFilesTab plane={PLANE} cut={CUT} onOpenView={() => undefined} />,
    );
    const tree = await screen.findByRole("tree", { name: "Files of fix-it" });
    await userEvent.click(await within(tree).findByRole("treeitem", { name: "src" }));
    await userEvent.click(await within(tree).findByRole("treeitem", { name: "lib.rs" }));
    await waitFor(() => expect(screen.getByTestId("light-editor")).toBeInTheDocument());

    held(container, "a branch's files");
  });

  it("in one file's tab", async () => {
    core();
    const { container } = render(
      <PieceFileTab plane={PLANE} cut={CUT} path="src/lib.rs" onOpenView={() => undefined} />,
    );
    await screen.findByRole("button", { name: "Show what changed" });

    held(container, "a file's tab");
  });

  it("in what changed in a file", async () => {
    core();
    const { container } = render(<PieceDiffTab plane={PLANE} cut={CUT} path="src/lib.rs" />);
    await waitFor(() => expect(container.querySelector(".cm-mergeView")).not.toBeNull());

    held(container, "what changed");
  });
});

afterAll(() => {
  // Run last, over what every tab above drew: a paid debt comes off the list.
  expect(paidDebts(DEBT, found), "debts no editor tab draws any more").toEqual([]);
});
