import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { BranchTree } from "./BranchTree";
import { ReferenceChats, type ChatsForReferences } from "../references";
import type {
  ChatTouching,
  FolderEntry,
  OpenChat,
  Panels as PanelsModel,
  Piece,
  PlaneId,
} from "../bindings";
import type { Place } from "../pieceViews";

/**
 * **The file tab's tree carries the live marker too** (#1154, FM-6's follow-up): a chat touching a
 * file of the branch marks the file and its folders, as the explorer's rows do. The tab asks
 * where its branch is and which chats are open only once a chat touches something, so a quiet
 * project costs it nothing. The core is mocked; the marks themselves are `Explorer.touching`'s.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/plane" as unknown as PlaneId;
const CUT: Place = { workspace: "alpha", repo: "svc", piece: "one" };
const REPO: Place = { workspace: "alpha", repo: "svc", piece: null };

const ONE: Piece = {
  piece: "one",
  path: "/plane/workspaces/alpha/.worktrees/svc/one",
  branch: "one",
  wired: true,
  stale: false,
  said: "",
};

const PANELS = {
  workspace: "alpha",
  repos: ["svc"],
  paths: { svc: "/plane/workspaces/alpha/svc" },
} as unknown as PanelsModel;

const chat = (session: number, name: string, cwd: string): OpenChat =>
  ({ session, name, cwd, harness: "claude-code", profile: null, persona: null }) as OpenChat;

const entry = (name: string, kind: FolderEntry["kind"] = "file"): FolderEntry => ({
  name,
  kind,
  ignored: false,
  refused: null,
});

/** The core as the tab asks it; returns what it was asked, by command. */
function core(open: OpenChat[]) {
  const asked: string[] = [];
  mockIPC(
    (cmd, args) => {
      const a = args as Record<string, string | null>;
      if (cmd === "branch_tree") {
        if (a.folder === "")
          return { entries: [entry("src", "folder"), entry("README.md")], more: 0 };
        return { entries: [], more: 0 };
      }
      if (cmd === "files_watch") return null;
      if (cmd === "project_icons_drawn") return null;
      if (cmd === "extension_icon_themes") return [];
      asked.push(cmd);
      if (cmd === "opened_chats") return open;
      if (cmd === "worktree_list") return [ONE];
      if (cmd === "workspace_panels") return PANELS;
      throw new Error(`unexpected ${cmd}`);
    },
    { shouldMockEvents: true },
  );
  return asked;
}

const touch = (session: number, path: string, plane: PlaneId = PLANE) =>
  act(() => emit("chat-touching", { plane, session, path } satisfies ChatTouching));

const markOn = (id: string) =>
  document.querySelector(`[data-row="${id}"] .touch-mark`)?.getAttribute("aria-label") ?? undefined;

describe("the file tab's live marker", () => {
  it("marks the file a chat in the branch touches, and the folders above it", async () => {
    const asked = core([chat(3, "fix login", ONE.path)]);
    render(<BranchTree plane={PLANE} place={CUT} onPick={() => {}} />);
    await screen.findByRole("treeitem", { name: /^README\.md/ });
    // Nothing touched yet: nothing asked beyond the tree.
    expect(asked).toEqual([]);

    await touch(3, "src/lib.rs");

    const said = "fix login is working here now";
    await screen.findByRole("img", { name: said });
    expect(markOn("file:svc/one:src")).toBe(said);
    expect(markOn("file:svc/one:README.md")).toBeUndefined();
    expect(asked.sort()).toEqual(["opened_chats", "worktree_list"]);

    // A second touch by a chat already known asks nothing again.
    await touch(3, "README.md");
    expect(markOn("file:svc/one:README.md")).toBe(said);
    expect(asked).toHaveLength(2);
  });

  it("names the chat as its tab does, where the window lends the names", async () => {
    core([chat(3, "7", ONE.path)]);
    const lent: ChatsForReferences = {
      plane: PLANE,
      chats: [{ session: 3, name: "fix login" }],
      hand: () => {},
    };
    render(
      <ReferenceChats.Provider value={lent}>
        <BranchTree plane={PLANE} place={CUT} onPick={() => {}} />
      </ReferenceChats.Provider>,
    );
    await screen.findByRole("treeitem", { name: /^README\.md/ });

    await touch(3, "README.md");

    await screen.findByRole("img", { name: "fix login is working here now" });
  });

  it("marks a repo's own folder by the clone's path", async () => {
    const asked = core([chat(4, "steward 4", PANELS.paths.svc)]);
    render(<BranchTree plane={PLANE} place={REPO} onPick={() => {}} />);
    await screen.findByRole("treeitem", { name: /^README\.md/ });

    await touch(4, "README.md");

    await screen.findByRole("img", { name: "steward 4 is working here now" });
    expect(asked.sort()).toEqual(["opened_chats", "workspace_panels"]);
  });

  it("marks nothing for a chat elsewhere or another project's touch", async () => {
    const asked = core([chat(4, "steward 4", "/plane/workspaces/alpha/svc")]);
    render(<BranchTree plane={PLANE} place={CUT} onPick={() => {}} />);
    await screen.findByRole("treeitem", { name: /^README\.md/ });

    await touch(3, "README.md", "/another" as unknown as PlaneId);
    expect(asked).toEqual([]);
    await touch(4, "README.md");
    // Let the asks answer, then see nothing marked.
    await act(async () => {
      await new Promise((done) => setTimeout(done, 20));
    });

    expect(asked.sort()).toEqual(["opened_chats", "worktree_list"]);
    expect(document.querySelector(".touch-mark")).toBeNull();
  });
});
