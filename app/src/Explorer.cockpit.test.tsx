import { afterEach, describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Explorer } from "./Explorer";
import { ChatsHere, fixedChats, nothingKnown } from "./chatState";
import type {
  AheadBehind,
  BranchStatus,
  OpenChat,
  Panels as PanelsModel,
  Piece,
  PlaneId,
} from "./bindings";
import type { Catalogued, Offer } from "./actions";
import type { Place } from "./pieceViews";
import type { WorkspaceState } from "./workspaceState";

/**
 * **The branch cockpit** (FM-5, #1108): the render states the real-app scenario
 * (`workspace-explorer.e2e.ts`) does not reach on its fixture — a branch ahead of and behind its
 * base, a refused read, a Merge the catalogue cannot run, the chats working in the branch. The
 * core is mocked; what it answers for a real branch is `a_branch_says_how_far_it_is_from_its_base.rs`'s.
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
  branch: "fix/one",
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

const STATUS: BranchStatus = {
  changes: [
    { path: "README.md", mark: "changed", from: null, uncommitted: true },
    { path: "src/new.rs", mark: "added", from: null, uncommitted: false },
  ],
  folders: [
    { folder: "", mark: "changed", count: 2 },
    { folder: "src", mark: "added", count: 1 },
  ],
  more: 0,
  base: "main",
};

const chat = (session: number, name: string, cwd: string): OpenChat =>
  ({ session, name, cwd, harness: "claude-code", profile: null, persona: null }) as OpenChat;

const WORKING = chat(3, "steward 3", "/plane/workspaces/alpha/.worktrees/svc/one/src");
const ELSEWHERE = chat(4, "steward 4", "/plane/workspaces/alpha/svc");

/** The core, answering how far the branch is with `apart`, or refusing with a string. */
function core(apart: AheadBehind | string) {
  mockIPC(
    (cmd) => {
      if (cmd === "branch_tree")
        return {
          entries: [{ name: "README.md", kind: "file", ignored: false, refused: null }],
          more: 0,
        };
      if (cmd === "branch_status") return STATUS;
      if (cmd === "branch_ahead_behind") {
        if (typeof apart === "string") throw apart;
        return apart;
      }
      if (cmd === "files_watch" || cmd === "branch_watch") return null;
      throw new Error(`unexpected ${cmd}`);
    },
    { shouldMockEvents: true },
  );
}

const FOCUS: Place = { workspace: "alpha", repo: "svc", piece: "one" };

function offer(id: string, title: string, available = true): Offer {
  return {
    id,
    title,
    available,
    reason: available ? "" : "purlis found no project, so it cannot reach a branch.",
    does: { verb: "openProject" },
  };
}

function draw({
  offers = new Map(),
  onPress = () => {},
  onFocus = () => {},
}: {
  offers?: Catalogued;
  onPress?: (offer: Offer) => void;
  onFocus?: (focus: Place | undefined) => void;
} = {}) {
  render(
    <ChatsHere.Provider value={fixedChats(nothingKnown)}>
      <Explorer
        plane={PLANE}
        workspaces={["alpha", "beta"]}
        workspace="alpha"
        state={STATE}
        chats={[WORKING, ELSEWHERE]}
        spot={undefined}
        onPick={() => {}}
        offers={offers}
        onPress={onPress}
        onReadAgain={() => {}}
        onOpenFile={() => {}}
        focus={FOCUS}
        onFocus={onFocus}
      />
    </ChatsHere.Provider>,
  );
}

const header = () => screen.getByRole("region", { name: "Branch fix/one" });

describe("the branch cockpit", () => {
  it("narrows the explorer to the branch: its state, then its files, and no chat (#1673)", async () => {
    core({ ahead: 2, behind: 1, base: "main" });
    draw();

    expect(screen.queryByRole("tree", { name: "Repos and branches" })).toBeNull();
    expect(within(header()).getByRole("heading", { name: "fix/one" })).toBeTruthy();
    await within(header()).findByText("2 ahead, 1 behind main");
    await within(header()).findByText("2 changes");

    const tree = screen.getByRole("tree", { name: "Files of fix/one" });
    const rows = within(tree).getAllByRole("treeitem");
    // Its files, open with nothing clicked. The chat working in it is the Chats view's.
    expect(rows.some((row) => row.textContent?.includes("steward"))).toBe(false);
    expect(rows[0].textContent).toContain("Files");
    expect(rows[0].getAttribute("aria-expanded")).toBe("true");
    await within(tree).findByRole("treeitem", { name: /README\.md/ });
  });

  it("keeps the Workspaces section above the cockpit, the focused one current (#1677)", async () => {
    core({ ahead: 0, behind: 0, base: "main" });
    draw();
    const listed = await screen.findByRole("tree", { name: "Workspaces of this project" });

    expect(within(listed).getByRole("treeitem", { name: /^alpha/ })).toHaveAttribute(
      "aria-current",
      "true",
    );
    expect(screen.queryByRole("tree", { name: "Repos and branches" })).not.toBeInTheDocument();
  });

  it("says a branch with no base recorded has none, rather than a count", async () => {
    core({ ahead: 0, behind: 0, base: null });
    draw();

    await within(header()).findByText("No base recorded");
  });

  it("says a count the core stopped past its cap as that many or more (#1152)", async () => {
    core({ ahead: 10_001, behind: 1234, base: "main" });
    draw();

    await within(header()).findByText("10,000+ ahead, 1,234 behind main");
  });

  it("says a count of exactly the cap as it is (#1152)", async () => {
    core({ ahead: 3, behind: 10_000, base: "main" });
    draw();

    await within(header()).findByText("3 ahead, 10,000 behind main");
  });

  it("says why it could not read how far the branch is", async () => {
    core("charter could not read the branch: it took too long");
    draw();

    await within(header()).findByText(/it took too long/);
  });

  it("runs the explorer's own Merge and Done rows from its header", async () => {
    core({ ahead: 1, behind: 0, base: "main" });
    const merge = offer("worktree.merge:svc/one", "Merge branch fix/one into svc");
    const done = offer("worktree.done:svc/one", "Mark branch fix/one done", false);
    const onPress = vi.fn();
    draw({
      offers: new Map([
        [merge.id, merge],
        [done.id, done],
      ]),
      onPress,
    });

    await userEvent.click(within(header()).getByRole("button", { name: "Merge" }));
    const doneButton = within(header()).getByRole("button", { name: "Done" });

    expect(onPress).toHaveBeenCalledWith(merge);
    // A row the catalogue cannot run is drawn, disabled, with why.
    expect(doneButton).toHaveProperty("disabled", true);
    expect(doneButton.getAttribute("title")).toContain("no project");
  });

  it("steps back out on Esc and on the breadcrumb", async () => {
    core({ ahead: 0, behind: 0, base: "main" });
    const onFocus = vi.fn();
    draw({ onFocus });

    const crumbs = screen.getByRole("navigation", { name: "Breadcrumb" });
    expect(within(crumbs).getByText("fix/one").getAttribute("aria-current")).toBe("location");
    await userEvent.click(within(crumbs).getByRole("button", { name: "alpha" }));
    expect(onFocus).toHaveBeenLastCalledWith(undefined);

    onFocus.mockClear();
    screen.getByRole("treeitem", { name: /^Files/ }).focus();
    await userEvent.keyboard("{Escape}");
    expect(onFocus).toHaveBeenLastCalledWith(undefined);
  });

  it("moves between its rows with the arrows", async () => {
    core({ ahead: 0, behind: 0, base: "main" });
    draw();
    const tree = screen.getByRole("tree", { name: "Files of fix/one" });
    const filesRow = within(tree).getByRole("treeitem", { name: /^Files/ });
    const readme = await within(tree).findByRole("treeitem", { name: /README\.md/ });

    filesRow.focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(readme);
    await userEvent.keyboard("{ArrowUp}");
    expect(document.activeElement).toBe(filesRow);
  });

  it("closes its files on ArrowLeft, and they stay closed", async () => {
    core({ ahead: 0, behind: 0, base: "main" });
    const cockpit = (chats: OpenChat[]) => (
      <ChatsHere.Provider value={fixedChats(nothingKnown)}>
        <Explorer
          plane={PLANE}
          workspace="alpha"
          state={STATE}
          chats={chats}
          spot={undefined}
          onPick={() => {}}
          offers={new Map()}
          onPress={() => {}}
          onReadAgain={() => {}}
          onOpenFile={() => {}}
          focus={FOCUS}
          onFocus={() => {}}
        />
      </ChatsHere.Provider>
    );
    const { rerender } = render(cockpit([WORKING, ELSEWHERE]));
    const tree = screen.getByRole("tree", { name: "Files of fix/one" });
    const filesRow = () => within(tree).getByRole("treeitem", { name: /^Files/ });
    await within(tree).findByRole("treeitem", { name: /README\.md/ });

    filesRow().focus();
    await userEvent.keyboard("{ArrowLeft}");

    expect(filesRow().getAttribute("aria-expanded")).toBe("false");
    // Drawn again with what the window now says — a chat ended — and still closed.
    rerender(cockpit([ELSEWHERE]));
    expect(filesRow().getAttribute("aria-expanded")).toBe("false");
    expect(within(tree).queryByRole("treeitem", { name: /README/ })).toBeNull();
  });

  it("lands the keyboard on the branch's row when Esc steps back out", async () => {
    core({ ahead: 0, behind: 0, base: "main" });
    function Focusable() {
      const [focus, setFocus] = useState<Place | undefined>(FOCUS);
      return (
        <Explorer
          plane={PLANE}
          workspace="alpha"
          state={STATE}
          chats={[WORKING, ELSEWHERE]}
          spot={undefined}
          onPick={() => {}}
          offers={new Map()}
          onPress={() => {}}
          onReadAgain={() => {}}
          onOpenFile={() => {}}
          focus={focus}
          onFocus={setFocus}
        />
      );
    }
    render(
      <ChatsHere.Provider value={fixedChats(nothingKnown)}>
        <Focusable />
      </ChatsHere.Provider>,
    );

    screen.getByRole("treeitem", { name: /^Files/ }).focus();
    await userEvent.keyboard("{Escape}");

    await screen.findByRole("tree", { name: "Repos and branches" });
    expect((document.activeElement as HTMLElement | null)?.dataset.row).toBe("piece:svc/one");
  });

  it("stays when an open row menu takes the Esc", async () => {
    core({ ahead: 0, behind: 0, base: "main" });
    const onFocus = vi.fn();
    draw({ onFocus });
    const tree = screen.getByRole("tree", { name: "Files of fix/one" });

    fireEvent.contextMenu(await within(tree).findByRole("treeitem", { name: /README\.md/ }));
    const menu = await screen.findByRole("menu");
    within(menu).getAllByRole("menuitem")[0].focus();
    await userEvent.keyboard("{Escape}");

    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(onFocus).not.toHaveBeenCalled();
    expect(screen.getByRole("tree", { name: "Files of fix/one" })).toBeTruthy();
  });

  it("is drawn while its repo is still listed, and not once the repo is gone, refused or lists no such branch", () => {
    core({ ahead: 0, behind: 0, base: "main" });
    const drawWith = (state: WorkspaceState) =>
      render(
        <ChatsHere.Provider value={fixedChats(nothingKnown)}>
          <Explorer
            plane={PLANE}
            workspace="alpha"
            state={state}
            chats={[]}
            spot={undefined}
            onPick={() => {}}
            offers={new Map()}
            onPress={() => {}}
            onReadAgain={() => {}}
            focus={FOCUS}
            onFocus={() => {}}
          />
        </ChatsHere.Provider>,
      );
    const cockpitDrawn = () => screen.queryByRole("region", { name: /^Branch / }) !== null;

    drawWith({ ...STATE, pieces: {}, piecesRefused: { svc: "git refused" } });
    expect(cockpitDrawn()).toBe(false);
    cleanup();
    drawWith({ ...STATE, panels: { ...PANELS, repos: [] }, pieces: {} });
    expect(cockpitDrawn()).toBe(false);
    cleanup();
    drawWith({ ...STATE, pieces: { svc: [] } });
    expect(cockpitDrawn()).toBe(false);
    cleanup();
    // Its repo's branches still on their way: the cockpit, named by its folder until then.
    drawWith({ ...STATE, pieces: {} });
    expect(cockpitDrawn()).toBe(true);
  });
});

/**
 * **A repo's own folder has a cockpit too, without Merge and Done** (#1152, D-1152-3): the
 * explorer narrowed to the clone itself — the branch it has checked out and how far that is from
 * its upstream, the chats working in it, and its files. Merge and Done act on a branch folder
 * purlis cut, so the header has neither, whatever the catalogue holds.
 */
describe("a repo's own folder's cockpit (#1152)", () => {
  const REPO: Place = { workspace: "alpha", repo: "svc", piece: null };
  const SVC = "/plane/workspaces/alpha/svc";
  const REPO_STATE: WorkspaceState = {
    ...STATE,
    panels: { ...PANELS, paths: { svc: SVC } },
    repos: {
      workspace: "alpha",
      cache_refused: null,
      repos: [
        {
          name: "svc",
          branch: "main",
          unborn: false,
          detached: null,
          upstream: "origin/main",
          ahead: 1,
          behind: 0,
          tracked: 0,
          untracked: 0,
          unreadable: null,
          ci: null,
          change: null,
          sigil: null,
          fetched_seconds_ago: null,
          not_fetched: "nothing has fetched this checkout",
        },
      ],
    },
  };
  const drawRepo = (state: WorkspaceState, offers: Catalogued = new Map()) =>
    render(
      <ChatsHere.Provider value={fixedChats(nothingKnown)}>
        <Explorer
          plane={PLANE}
          workspace="alpha"
          state={state}
          chats={[WORKING, ELSEWHERE]}
          spot={undefined}
          onPick={() => {}}
          offers={offers}
          onPress={() => {}}
          onReadAgain={() => {}}
          onOpenFile={() => {}}
          focus={REPO}
          onFocus={() => {}}
        />
      </ChatsHere.Provider>,
    );

  it("narrows the explorer to the clone: its branch, its files, and no Merge or Done", async () => {
    core({ ahead: 1, behind: 0, base: "origin/main" });
    const merge = offer("worktree.merge:svc/one", "Merge branch fix/one into svc");
    drawRepo(REPO_STATE, new Map([[merge.id, merge]]));

    const head = screen.getByRole("region", { name: "Repo svc" });
    expect(within(head).getByRole("heading", { name: "svc" })).toBeTruthy();
    await within(head).findByText("on main · 1 ahead, 0 behind origin/main");
    expect(within(head).queryByRole("button", { name: "Merge" })).toBeNull();
    expect(within(head).queryByRole("button", { name: "Done" })).toBeNull();

    const crumbs = screen.getByRole("navigation", { name: "Breadcrumb" });
    expect(within(crumbs).getByText("svc").getAttribute("aria-current")).toBe("location");

    const tree = screen.getByRole("tree", { name: "Files of svc" });
    const rows = within(tree).getAllByRole("treeitem");
    expect(rows.some((row) => row.textContent?.includes("steward"))).toBe(false);
    await within(tree).findByRole("treeitem", { name: /README\.md/ });
  });

  it("stands while the workspace holds the repo, and not once it is gone", () => {
    core({ ahead: 0, behind: 0, base: null });
    drawRepo(REPO_STATE);
    expect(screen.queryByRole("region", { name: "Repo svc" })).not.toBeNull();
    cleanup();

    drawRepo({ ...REPO_STATE, panels: { ...PANELS, repos: [] } });
    expect(screen.queryByRole("region", { name: "Repo svc" })).toBeNull();
  });
});
