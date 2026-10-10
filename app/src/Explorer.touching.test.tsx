import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { Explorer } from "./Explorer";
import { ChatsHere, fixedChats, nothingKnown } from "./chatState";
import type {
  ChatTouching,
  FolderEntry,
  OpenChat,
  Panels as PanelsModel,
  Piece,
  PlaneId,
} from "./bindings";
import type { Place } from "./pieceViews";
import { FADE_MS } from "./touching";

/**
 * **A live marker on the file a chat is touching** (FM-6, #1109): the render states — a file and
 * its folders marked with the chat's name, the mark fading once the chat goes quiet, a touch of
 * another project or of a chat working elsewhere marking nothing — in the explorer and in the
 * cockpit. The core is mocked; what it confines and sends is `hooks::tests`' and `touching.rs`'s.
 */

afterEach(() => {
  cleanup();
  clearMocks();
  vi.useRealTimers();
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

const chat = (session: number, name: string, cwd: string): OpenChat =>
  ({ session, name, cwd, harness: "claude-code", profile: null, persona: null }) as OpenChat;

const WORKING = chat(3, "fix login", ONE.path);
const ELSEWHERE = chat(4, "steward 4", "/plane/workspaces/alpha/svc");

const entry = (name: string, kind: FolderEntry["kind"] = "file"): FolderEntry => ({
  name,
  kind,
  ignored: false,
  refused: null,
});

function core() {
  mockIPC(
    (cmd, args) => {
      const a = args as Record<string, string | null>;
      if (cmd === "branch_tree") {
        if (a.folder === "")
          return { entries: [entry("src", "folder"), entry("README.md")], more: 0 };
        if (a.folder === "src") return { entries: [entry("lib.rs")], more: 0 };
        return { entries: [], more: 0 };
      }
      if (cmd === "branch_status") throw "not read here";
      if (cmd === "branch_ahead_behind") throw "not read here";
      if (cmd === "files_watch" || cmd === "branch_watch") return null;
      if (cmd === "project_icons_drawn") return null;
      if (cmd === "extension_icon_themes") return [];
      throw new Error(`unexpected ${cmd}`);
    },
    { shouldMockEvents: true },
  );
}

function draw(focus?: Place) {
  render(
    <ChatsHere.Provider value={fixedChats(nothingKnown)}>
      <Explorer
        plane={PLANE}
        workspace="alpha"
        state={{
          panels: PANELS,
          repos: { workspace: "alpha", repos: [], cache_refused: null },
          pieces: { svc: [ONE] },
          piecesRefused: {},
          reading: false,
        }}
        chats={[WORKING, ELSEWHERE]}
        spot={undefined}
        onPick={() => {}}
        offers={new Map()}
        onPress={() => {}}
        onReadAgain={() => {}}
        onOpenFile={() => {}}
        focus={focus}
        onFocus={() => {}}
      />
    </ChatsHere.Provider>,
  );
}

const touch = (session: number, path: string, plane: PlaneId = PLANE) =>
  act(() => emit("chat-touching", { plane, session, path } satisfies ChatTouching));

const rowOf = (id: string) => {
  const found = document.querySelector<HTMLElement>(`[data-row="${id}"]`);
  if (found === null) throw new Error(`no row ${id}`);
  return found;
};

/** What a row's live mark says, or nothing when it has none. */
const markOn = (id: string) =>
  rowOf(id).querySelector(".touch-mark")?.getAttribute("aria-label") ?? undefined;

async function openSrc() {
  await userEvent.click(rowOf("file:svc/one:"));
  await userEvent.click(await within(document.body).findByRole("treeitem", { name: /^src/ }));
  await screen.findByRole("treeitem", { name: /^lib\.rs/ });
}

describe("the file a chat is touching", () => {
  it("marks the file and every folder above it, naming the chat, then fades", async () => {
    core();
    draw();
    await openSrc();
    expect(document.querySelector(".touch-mark")).toBeNull();

    // The fade's clock, from before the touch it times.
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
    await touch(3, "src/lib.rs");

    const said = "fix login is working here now";
    expect(markOn("file:svc/one:src/lib.rs")).toBe(said);
    expect(markOn("file:svc/one:src")).toBe(said);
    expect(markOn("file:svc/one:")).toBe(said);
    expect(markOn("file:svc/one:README.md")).toBeUndefined();
    expect(
      rowOf("file:svc/one:src/lib.rs").querySelector(".touch-mark")?.getAttribute("title"),
    ).toBe(said);

    // Still there just before the fade, gone after it.
    await act(async () => {
      vi.advanceTimersByTime(FADE_MS - 100);
    });
    expect(markOn("file:svc/one:src/lib.rs")).toBe(said);
    await act(async () => {
      vi.advanceTimersByTime(200);
    });
    expect(document.querySelector(".touch-mark")).toBeNull();
  });

  it("marks nothing for a chat working elsewhere, or for another project", async () => {
    core();
    draw();
    await openSrc();

    // Chat 4 works in the repo's own folder, not this branch: its `src/lib.rs` is another file.
    await touch(4, "src/lib.rs");
    await touch(3, "src/lib.rs", "/another" as unknown as PlaneId);
    // A chat this window does not have.
    await touch(9, "src/lib.rs");

    expect(markOn("file:svc/one:src/lib.rs")).toBeUndefined();
    expect(markOn("file:svc/one:")).toBeUndefined();
  });

  it("marks a folder that is closed, so a touch deep in it still shows", async () => {
    core();
    draw();
    await userEvent.click(rowOf("file:svc/one:"));
    await screen.findByRole("treeitem", { name: /^src/ });

    await touch(3, "src/deep/inside/x.rs");

    expect(markOn("file:svc/one:src")).toBe("fix login is working here now");
  });

  it("marks the files in the branch's cockpit", async () => {
    core();
    draw({ workspace: "alpha", repo: "svc", piece: "one" });
    await screen.findByRole("treeitem", { name: /^README\.md/ });

    await touch(3, "README.md");

    expect(markOn("file:svc/one:README.md")).toBe("fix login is working here now");
  });
});
