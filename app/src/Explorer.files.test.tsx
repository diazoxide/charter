import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { Explorer } from "./Explorer";
import { ChatsHere, fixedChats, nothingKnown } from "./chatState";
import type { FolderEntry, Panels as PanelsModel, Piece, PlaneId } from "./bindings";
import type { WorkspaceState } from "./workspaceState";

/**
 * **A branch's files in the explorer** (FM-1): the render states the real-app scenario
 * (`workspace-explorer.e2e.ts`) does not reach on its fixture — a file charter refuses, what git
 * ignores, and a folder read again when the core says it moved. The core is mocked here; what it
 * answers for a real branch is `a_branch_expands_into_its_files.rs`'s.
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

const entry = (name: string, on: Partial<FolderEntry> = {}): FolderEntry => ({
  name,
  kind: "file",
  ignored: false,
  refused: null,
  ...on,
});

/** The core, as the tree asks it: what each folder holds, by `<piece>:<folder>`. */
function core(folders: Record<string, FolderEntry[]>, more: Record<string, number> = {}) {
  const asked: string[] = [];
  mockIPC(
    (cmd, args) => {
      const a = args as Record<string, string | null>;
      if (cmd === "branch_tree") {
        const key = `${a.piece ?? ""}:${a.folder}`;
        asked.push(key);
        return { entries: folders[key] ?? [], more: more[key] ?? 0 };
      }
      if (cmd === "files_watch") return null;
      throw new Error(`unexpected ${cmd}`);
    },
    { shouldMockEvents: true },
  );
  return asked;
}

function draw(onOpenFile = vi.fn()) {
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
        onOpenFile={onOpenFile}
      />
    </ChatsHere.Provider>,
  );
  return onOpenFile;
}

const tree = () => screen.getByRole("tree", { name: "Repos and branches" });
const row = (id: string) => {
  const found = tree().querySelector<HTMLElement>(`[data-row="${id}"]`);
  if (found === null) throw new Error(`no row ${id}`);
  return found;
};
const named = (name: string) => within(tree()).findByRole("treeitem", { name: new RegExp(name) });

describe("a branch's files in the explorer", () => {
  it("reads a folder only when it is opened, and opens a file in its tab", async () => {
    const asked = core({
      "one:": [entry("src", { kind: "folder" }), entry("README.md")],
      "one:src": [entry("lib.rs")],
    });
    const opened = draw();
    expect(asked).toEqual([]);

    await userEvent.click(row("file:svc/one:"));
    await userEvent.click(await named("^src"));
    await userEvent.click(await named("^lib.rs"));

    expect(asked).toEqual(["one:", "one:src"]);
    expect(opened).toHaveBeenCalledWith(
      { workspace: "alpha", repo: "svc", piece: "one" },
      "src/lib.rs",
    );
  });

  it("opens a file of the repo's own folder as the repo's, with no branch folder named", async () => {
    core({ ":": [entry("README.md")] });
    const opened = draw();

    await userEvent.click(row("file:svc/:"));
    await userEvent.click(await named("^README.md"));

    expect(opened).toHaveBeenCalledWith(
      { workspace: "alpha", repo: "svc", piece: null },
      "README.md",
    );
  });

  it("draws a file charter will not open with its reason, and pressing it opens nothing", async () => {
    core({
      "one:": [
        entry("away.txt", {
          kind: "link",
          refused: "a link out of the branch's folder, so charter does not follow it",
        }),
      ],
    });
    const opened = draw();
    await userEvent.click(row("file:svc/one:"));

    const away = await named("^away.txt");
    await userEvent.click(away);

    expect(away).toHaveTextContent("a link out of the branch's folder");
    expect(away).toHaveAttribute("aria-disabled", "true");
    expect(opened).not.toHaveBeenCalled();
  });

  it("hides what git ignores until asked, then draws it dimmed and never opens it", async () => {
    core({
      "one:": [
        entry("target", { kind: "folder", ignored: true }),
        entry(".env", { ignored: true, refused: "ignored by git, so charter does not open it" }),
        entry("README.md"),
      ],
    });
    const opened = draw();
    await userEvent.click(row("file:svc/one:"));
    await named("^README.md");

    expect(within(tree()).queryByRole("treeitem", { name: /^\.env/ })).toBeNull();
    expect(within(tree()).queryByRole("treeitem", { name: /^target/ })).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "Show ignored files" }));
    const env = await named("^\\.env");
    await userEvent.click(env);

    expect(screen.getByRole("button", { name: "Show ignored files" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(env.closest("li")).toHaveAttribute("data-ignored", "true");
    expect(await named("^target")).toBeInTheDocument();
    expect(opened).not.toHaveBeenCalled();
  });

  it("reads an open folder again when the core says it moved", async () => {
    const folders: Record<string, FolderEntry[]> = { "one:": [entry("README.md")] };
    const asked = core(folders);
    draw();
    await userEvent.click(row("file:svc/one:"));
    await named("^README.md");

    folders["one:"] = [entry("README.md"), entry("new.rs")];
    await act(() =>
      emit("files-changed", {
        folders: [{ plane: PLANE, workspace: "alpha", repo: "svc", piece: "one", folder: "" }],
      }),
    );

    expect(await named("^new.rs")).toBeInTheDocument();
    expect(asked).toEqual(["one:", "one:"]);

    // And a file removed goes.
    folders["one:"] = [entry("new.rs")];
    await act(() =>
      emit("files-changed", {
        folders: [{ plane: PLANE, workspace: "alpha", repo: "svc", piece: "one", folder: "" }],
      }),
    );

    await vi.waitFor(() =>
      expect(within(tree()).queryByRole("treeitem", { name: /^README.md/ })).toBeNull(),
    );
  });

  it("opens a folder whose name holds a line break", async () => {
    const asked = core({
      "one:": [entry("a\nb", { kind: "folder" })],
      "one:a\nb": [entry("inside.md")],
    });
    draw();
    await userEvent.click(row("file:svc/one:"));
    // Found by its id: a selector cannot spell a line break as plainly.
    const folder = await vi.waitFor(() => {
      const found = [...tree().querySelectorAll<HTMLElement>("[data-row]")].find(
        (one) => one.dataset.row === "file:svc/one:a\nb",
      );
      if (found === undefined) throw new Error("no row for the folder yet");
      return found;
    });
    await userEvent.click(folder);

    expect(await named("^inside.md")).toBeInTheDocument();
    expect(asked).toEqual(["one:", "one:a\nb"]);
  });

  it("draws a folder's first entries and counts the rest it holds", async () => {
    core({ "one:": [entry("a.md")] }, { "one:": 12_345 });
    draw();

    await userEvent.click(row("file:svc/one:"));

    expect(await screen.findByText("12,345 more not shown")).toBeInTheDocument();
  });

  it("Home and End reach the ends of the tree from a file row", async () => {
    core({ "one:": [entry("a.md"), entry("b.md")] });
    draw();
    await userEvent.click(row("file:svc/one:"));
    await named("^b.md");

    row("file:svc/one:a.md").focus();
    await userEvent.keyboard("{End}");
    expect(row("file:svc/one:b.md")).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(row("root")).toHaveFocus();
  });

  it("says beside an ignored file why it does not open, once ignored files are shown", async () => {
    core({
      "one:": [
        entry(".env", { ignored: true, refused: "ignored by git, so charter does not open it" }),
      ],
    });
    draw();
    await userEvent.click(row("file:svc/one:"));
    await userEvent.click(await screen.findByRole("button", { name: "Show ignored files" }));

    expect(await named("^\\.env")).toHaveTextContent("ignored by git, so charter does not open it");
  });

  it("moves through files with the arrows and opens one with Enter", async () => {
    core({ "one:": [entry("a.md"), entry("b.md")] });
    const opened = draw();
    await userEvent.click(row("file:svc/one:"));
    await named("^b.md");

    row("file:svc/one:").focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(row("file:svc/one:a.md")).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}{Enter}");

    expect(opened).toHaveBeenCalledWith({ workspace: "alpha", repo: "svc", piece: "one" }, "b.md");
    await userEvent.keyboard("{ArrowLeft}");
    expect(row("file:svc/one:")).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(row("file:svc/one:")).toHaveAttribute("aria-expanded", "false");
  });
});
