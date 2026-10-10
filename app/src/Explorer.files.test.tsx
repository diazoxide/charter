import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { Explorer } from "./Explorer";
import { ChatsHere, fixedChats, nothingKnown } from "./chatState";
import type {
  ExtensionTheme,
  FolderEntry,
  Panels as PanelsModel,
  Piece,
  PlaneId,
} from "./bindings";
import { forgetProjectThemes } from "./projectTheme";
import type { WorkspaceState } from "./workspaceState";
import type { Offer } from "./actions";

/**
 * **A branch's files in the explorer** (FM-1): the render states the real-app scenario
 * (`workspace-explorer.e2e.ts`) does not reach on its fixture — a file charter refuses, what git
 * ignores, and a folder read again when the core says it moved. The core is mocked here; what it
 * answers for a real branch is `a_branch_expands_into_its_files.rs`'s.
 */

afterEach(() => {
  cleanup();
  clearMocks();
  forgetProjectThemes();
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
function core(
  folders: Record<string, FolderEntry[]>,
  more: Record<string, number> = {},
  icons: { pick: string | null; offered: ExtensionTheme[] } = { pick: null, offered: [] },
) {
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
      if (cmd === "project_icons_drawn") return icons.pick;
      if (cmd === "extension_icon_themes") return icons.offered;
      throw new Error(`unexpected ${cmd}`);
    },
    { shouldMockEvents: true },
  );
  return asked;
}

function draw(onOpenFile = vi.fn(), onPress: (offer: Offer) => void = () => {}) {
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
        onPress={onPress}
        onReadAgain={() => {}}
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

  it("draws a file purlis will not open with its reason, and pressing it opens nothing", async () => {
    core({
      "one:": [
        entry("away.txt", {
          kind: "link",
          refused: "a link out of the branch's folder, so purlis does not follow it",
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
        entry(".env", { ignored: true, refused: "ignored by git, so purlis does not open it" }),
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

  it("draws what a folder held once it was watched, when it changed before the watch held", async () => {
    // #1427: a folder made in a branch while its folder was being opened. Read before the core
    // watched it, the folder was drawn without the new one, and no event ever came for a change
    // made before the watch: the tree stayed wrong until something else moved there.
    const disk: Record<string, FolderEntry[]> = { "one:": [entry("README.md")] };
    const asked: string[] = [];
    const watches: (() => void)[] = [];
    mockIPC(
      (cmd, args) => {
        const a = args as Record<string, string | null>;
        if (cmd === "branch_tree") {
          const key = `${a.piece ?? ""}:${a.folder}`;
          asked.push(key);
          return { entries: disk[key] ?? [], more: 0 };
        }
        // The core answers once the folder is watched, which takes a moment.
        if (cmd === "files_watch")
          return new Promise<null>((held) => watches.push(() => held(null)));
        if (cmd === "project_icons_drawn") return null;
        if (cmd === "extension_icon_themes") return [];
        throw new Error(`unexpected ${cmd}`);
      },
      { shouldMockEvents: true },
    );
    draw();
    await userEvent.click(row("file:svc/one:"));
    await vi.waitFor(() => expect(watches.length).toBeGreaterThan(0));

    // An agent makes a folder in the branch before the watch holds: no event will say so.
    disk["one:"] = [entry("README.md"), entry("shell-here", { kind: "folder" })];
    await act(async () => {
      for (const held of watches.splice(0)) held();
    });

    expect(await named("^shell-here")).toBeInTheDocument();
    expect(await named("^README.md")).toBeInTheDocument();
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
        entry(".env", { ignored: true, refused: "ignored by git, so purlis does not open it" }),
      ],
    });
    draw();
    await userEvent.click(row("file:svc/one:"));
    await userEvent.click(await screen.findByRole("button", { name: "Show ignored files" }));

    expect(await named("^\\.env")).toHaveTextContent("ignored by git, so purlis does not open it");
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

/** The icon a row is drawn with: its symbol's name in the icon theme (FM-3). */
const iconOf = (row: HTMLElement) => row.querySelector("svg.file-icon")?.getAttribute("data-icon");

describe("a branch's files are drawn with the project's icon theme (FM-3)", () => {
  const FOLDERS = {
    "one:": [entry("src", { kind: "folder" }), entry("README.md"), entry("notes.zzz")],
    "one:src": [entry("lib.rs")],
  };

  it("draws charter's own icons for a file's type and a folder's name", async () => {
    core(FOLDERS);
    draw();
    await userEvent.click(row("file:svc/one:"));
    const src = await named("^src");
    expect(iconOf(src)).toBe("folder-src");
    expect(iconOf(await named("^README.md"))).toBe("readme");
    expect(iconOf(await named("^notes.zzz"))).toBe("file");
    await userEvent.click(src);
    expect(iconOf(src)).toBe("folder-src-open");
    expect(iconOf(await named("^lib.rs"))).toBe("rust");
  });

  it("draws the icon theme an extension contributes when the project picks it", async () => {
    const square = { viewBox: "0 0 16 16", paths: [{ d: "M0 0h16v16H0z", tone: "icon.pink" }] };
    core(
      FOLDERS,
      {},
      {
        pick: "seti/Seti",
        offered: [
          {
            extension: "seti",
            name: "Seti",
            text: JSON.stringify({
              name: "Seti",
              symbols: { "seti-file": square, "seti-folder": square, "seti-md": square },
              file: "seti-file",
              folder: "seti-folder",
              extensions: { md: "seti-md" },
            }),
          },
        ],
      },
    );
    draw();
    await userEvent.click(row("file:svc/one:"));
    await vi.waitFor(async () => expect(iconOf(await named("^README.md"))).toBe("seti-md"));
    expect(iconOf(await named("^src"))).toBe("seti-folder");
    expect(iconOf(await named("^notes.zzz"))).toBe("seti-file");
  });
});

describe("a file or folder row's menu (FM-10)", () => {
  /** The rows of the menu a right-click on `name`'s row opens, by their names. */
  async function menuOf(name: string) {
    fireEvent.contextMenu(await named(`^${name}`));
    const menu = await screen.findByRole("menu");
    return within(menu)
      .getAllByRole("menuitem")
      .map((one) => one.getAttribute("aria-label"));
  }

  it("offers a file its paths, a reveal and your editor, and nothing that writes", async () => {
    core({ "one:": [entry("src", { kind: "folder" }), entry("README.md")] });
    draw();
    await userEvent.click(row("file:svc/one:"));

    const rows = await menuOf("README.md");

    expect(rows).toEqual([
      "Copy relative path",
      "Copy absolute path",
      expect.stringMatching(/^Reveal in /),
      "Open in your editor",
      "Start a chat here",
    ]);
  });

  it("drags a file or folder row as a reference to it, onto a chat (FM-9)", async () => {
    core({ "one:": [entry("src", { kind: "folder" }), entry("README.md")] });
    draw();
    await userEvent.click(row("file:svc/one:"));
    const set: Record<string, string> = {};
    const dataTransfer = { setData: (type: string, value: string) => (set[type] = value) };

    fireEvent.dragStart(await screen.findByRole("treeitem", { name: /README\.md/ }), {
      dataTransfer,
    });
    const file = JSON.parse(set["application/x-charter-reference"]) as unknown;
    fireEvent.dragStart(screen.getByRole("treeitem", { name: /^src/ }), { dataTransfer });
    const folder = JSON.parse(set["application/x-charter-reference"]) as unknown;

    const branch = { plane: PLANE, workspace: "alpha", repo: "svc", piece: "one" };
    expect(file).toEqual({ ...branch, path: "README.md", folder: false });
    expect(folder).toEqual({ ...branch, path: "src", folder: true });
  });

  it("offers a folder a shell tab there, and hands back the row with the branch and path", async () => {
    core({ "one:": [entry("src", { kind: "folder" })] });
    const pressed: Offer[] = [];
    draw(vi.fn(), (offer) => pressed.push(offer));
    await userEvent.click(row("file:svc/one:"));

    expect(await menuOf("src")).toContain("Open a shell tab here");
    await userEvent.click(screen.getByRole("menuitem", { name: "Open a shell tab here" }));

    expect(pressed.map((offer) => offer.does)).toEqual([
      {
        verb: "shellInFolder",
        at: { workspace: "alpha", repo: "svc", piece: "one", path: "src" },
      },
    ]);
  });

  it("gives the branch's own Files row its folder's absolute path, a reveal and a shell (#1143)", async () => {
    core({ "one:": [] });
    const pressed: Offer[] = [];
    draw(vi.fn(), (offer) => pressed.push(offer));

    fireEvent.contextMenu(row("file:svc/one:"));
    const menu = await screen.findByRole("menu");
    const rows = within(menu)
      .getAllByRole("menuitem")
      .map((one) => one.getAttribute("aria-label"));

    // No relative path: the folder's path relative to itself says nothing (D-1143-1).
    expect(rows).toEqual([
      "Copy absolute path",
      expect.stringMatching(/^Reveal in /),
      "Open a shell tab here",
    ]);
    await userEvent.click(within(menu).getByRole("menuitem", { name: "Open a shell tab here" }));
    expect(pressed.map((offer) => offer.does)).toEqual([
      { verb: "shellInFolder", at: { workspace: "alpha", repo: "svc", piece: "one", path: "" } },
    ]);
  });
});
