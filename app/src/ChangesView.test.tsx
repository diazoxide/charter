import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { ChangesView } from "./ChangesView";
import { catalogue, catalogued, type Catalogued } from "./actions";
import { noTabs } from "./tabs";
import type { FactColumn, Panels as PanelsModel, Piece, RepoState } from "./bindings";
import type { WorkspaceState } from "./workspaceState";

afterEach(cleanup);

const PANELS: PanelsModel = {
  workspace: "alpha",
  repos: ["svc", "tool"],
  paths: { svc: "/p/svc", tool: "/p/tool" },
  absent: [],
  refused: [],
  todos: [],
  todos_refused: null,
  personas: ["steward"],
  persona: "steward",
  sessions: [],
  contributed: [],
};

function repo(name: string, on: Partial<RepoState> = {}): RepoState {
  return {
    name,
    branch: "main",
    unborn: false,
    detached: null,
    upstream: null,
    ahead: 0,
    behind: 0,
    tracked: 0,
    untracked: 0,
    unreadable: null,
    ci: null,
    change: null,
    sigil: null,
    fetched_seconds_ago: null,
    not_fetched: "nothing has fetched this checkout",
    ...on,
  };
}

function piece(name: string, on: Partial<Piece> = {}): Piece {
  return {
    piece: name,
    path: `/p/.worktrees/svc/${name}`,
    branch: name,
    wired: true,
    stale: false,
    said: "",
    ...on,
  };
}

/** What the one workspace ask has come back with, as the regions receive it. */
function state(on: Partial<WorkspaceState> = {}): WorkspaceState {
  return {
    panels: PANELS,
    repos: { workspace: "alpha", repos: [], cache_refused: null },
    pieces: {},
    piecesRefused: {},
    reading: false,
    ...on,
  };
}

/** No catalogue, so no menus: every test but the ones about the menu draws the bar it always
 *  drew. */
const NO_MENUS: Catalogued = new Map();

const row = (name: string) => screen.getByTestId(`repo-${name}`);
const ci = (name: string) => screen.getByTestId(`ci-${name}`);

describe("the Changes view", () => {
  it("shows each repo's branch, how far it is from its upstream, and what is uncommitted", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [
              repo("svc", {
                upstream: "origin/main",
                ahead: 2,
                behind: 1,
                tracked: 3,
                untracked: 1,
              }),
              repo("tool"),
            ],
          },
        })}
      />,
    );

    expect(row("svc")).toHaveTextContent("main");
    expect(row("svc")).toHaveTextContent("origin/main");
    expect(row("svc")).toHaveTextContent("2 ahead, 1 behind");
    expect(row("svc")).toHaveTextContent("3 changed, 1 untracked");
    expect(row("tool")).toHaveTextContent("clean");
  });

  it("never draws a tree purlis could not read as clean", () => {
    // The whole reason the core has a third state. "Clean" here is a lie that reads as
    // "nothing to do", which is exactly the wrong thing to tell someone in a hurry.
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [
              repo("svc", {
                branch: null,
                unreadable:
                  "charter could not read the working tree at /p/svc — fatal: not a git " +
                  "repository. That is not the same as it being clean",
              }),
            ],
          },
        })}
      />,
    );

    expect(row("svc")).toHaveTextContent("could not read");
    expect(row("svc").querySelector(".dirt")).toBeNull();
    expect(within(row("svc")).getByRole("alert")).toBeInTheDocument();
  });

  it("says a branch has no commits yet rather than drawing it like any other", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [repo("svc", { unborn: true })],
          },
        })}
      />,
    );

    expect(row("svc")).toHaveTextContent("no commits yet");
  });

  it("names the commit a detached checkout sits on", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [repo("svc", { branch: null, detached: "1a2b3c4" })],
          },
        })}
      />,
    );

    expect(row("svc")).toHaveTextContent("detached at 1a2b3c4");
  });

  it("says the git answer is still coming rather than drawing a repo as unread", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({ repos: undefined, reading: true })}
      />,
    );

    expect(row("svc")).toHaveTextContent("reading…");
  });

  // ---------------------------------------------------------------------------------------
  // Worktrees, which are the Changes view's half of the pair (ADR 0038)
  // ---------------------------------------------------------------------------------------

  it("counts the worktrees cut off each clone", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({ pieces: { svc: [piece("one"), piece("two")], tool: [] } })}
      />,
    );

    expect(screen.getByTestId("worktrees-svc")).toHaveTextContent("2 branches");
    expect(screen.getByTestId("worktrees-tool")).toHaveTextContent("no branches");
  });

  it("counts the worktrees a chat would run unwired or stale in", () => {
    // The two states that change what starting a chat in one MEANS. Counted here; the row a
    // person acts on is the explorer's.
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          pieces: { svc: [piece("one", { wired: false }), piece("two", { stale: true })] },
        })}
      />,
    );

    expect(screen.getByTestId("worktrees-svc")).toHaveTextContent("1 unwired");
    expect(screen.getByTestId("worktrees-svc")).toHaveTextContent("1 stale");
  });

  it("never says a clone has no worktrees when git would not answer", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({ piecesRefused: { svc: "purlis will not run git through a symlink" } })}
      />,
    );

    expect(screen.getByTestId("worktrees-svc")).toHaveTextContent("branches unreadable");
    expect(screen.getByTestId("worktrees-svc")).not.toHaveTextContent("no branches");
  });

  it("says a listing is still on its way rather than counting nothing", () => {
    render(<ChangesView offers={NO_MENUS} onPress={() => {}} workspace="alpha" state={state()} />);

    expect(screen.getByTestId("worktrees-svc")).toHaveTextContent("asking git");
  });

  // ---------------------------------------------------------------------------------------
  // CI, which is read and never fetched
  // ---------------------------------------------------------------------------------------

  it("shows the CI state the cache holds, with the change and how old the answer is", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [
              repo("svc", {
                ci: "failed",
                change: 41,
                sigil: "#",
                fetched_seconds_ago: 120,
                not_fetched: null,
              }),
            ],
          },
        })}
      />,
    );

    expect(ci("svc")).toHaveTextContent("failed");
    expect(ci("svc")).toHaveTextContent("#41");
    // The age is part of the claim: a two-hour-old "success" is not the same as a fresh one.
    expect(ci("svc")).toHaveTextContent("2m ago");
  });

  it("says why there is no CI state rather than leaving the cell blank", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [repo("svc", { not_fetched: "the last fetch was for a different branch" })],
          },
        })}
      />,
    );

    expect(ci("svc")).toHaveTextContent("not fetched");
    expect(ci("svc")).toHaveTextContent("different branch");
  });

  it("tells a fetch that named no pipeline from no fetch at all", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [repo("svc", { fetched_seconds_ago: 30, not_fetched: null })],
          },
        })}
      />,
    );

    expect(ci("svc")).toHaveTextContent("no pipeline recorded");
    expect(ci("svc")).not.toHaveTextContent("not fetched");
  });

  it("says the app reads CI state and does not fetch it", () => {
    render(<ChangesView offers={NO_MENUS} onPress={() => {}} workspace="alpha" state={state()} />);

    expect(screen.getByTestId("changes-view")).toHaveTextContent("never fetches");
  });

  it("shows a forge cache charter refused, once for the listing", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          repos: {
            workspace: "alpha",
            repos: [],
            cache_refused: "/p/.charter/cache/glstate.json is reached through a symlink",
          },
        })}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("symlink");
  });

  // ---------------------------------------------------------------------------------------
  // Refusals, which are shown
  // ---------------------------------------------------------------------------------------

  it("shows what purlis would not read instead of a workspace with fewer repos in it", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          panels: {
            ...PANELS,
            repos: ["svc"],
            refused: [
              ["alias", "'alias' is reached through a symlink, and a worktree path may not be"],
            ],
          },
        })}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("alias");
  });

  it("shows a repo the manifest names that nobody has cloned", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({ panels: { ...PANELS, repos: ["svc"], absent: ["later"] } })}
      />,
    );

    expect(row("later")).toHaveTextContent("not cloned here");
  });

  it("says when the core refused the workspace outright", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="ghost"
        state={state({ panels: undefined, trouble: "no workspace 'ghost'" })}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("ghost");
  });

  it("draws nothing about a workspace when none is focused", () => {
    render(
      <ChangesView offers={NO_MENUS} onPress={() => {}} workspace={undefined} state={state()} />,
    );

    expect(screen.getByText("No workspace focused")).toBeInTheDocument();
    // FR-19 (#614): it says what goes here, not only that nothing does.
    expect(screen.getByTestId("changes-empty")).toHaveTextContent(/Focus a workspace/);
    expect(screen.queryByTestId("repo-svc")).not.toBeInTheDocument();
  });

  it("says what a workspace with no repos would list here", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({ panels: { ...PANELS, repos: [], absent: [] } })}
      />,
    );

    expect(screen.getByTestId("changes-empty")).toHaveTextContent("No repos in this workspace");
    expect(screen.getByTestId("changes-empty")).toHaveTextContent(
      /the branch it is on, its changes/,
    );
    expect(screen.queryByRole("tree")).not.toBeInTheDocument();
  });

  // ---------------------------------------------------------------------------------------
  // Columns, a worktree tree, and pipelines that move (M6.6)
  // ---------------------------------------------------------------------------------------

  it("draws each repo as a tree: its heading with the branch, then its changes, branches and pipeline", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          pieces: { svc: [piece("one")], tool: [] },
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [repo("svc", { tracked: 3, upstream: "origin/main" }), repo("tool")],
          },
        })}
      />,
    );

    // An editor's source-control view (#1701), and not a table that scrolls sideways in a side:
    // one heading per repo, and what is true of it as rows under it.
    const tree = screen.getByRole("tree", { name: "Repos of alpha" });
    const headings = within(tree)
      .getAllByRole("treeitem")
      .filter((item) => item.getAttribute("aria-level") === "1");
    expect(headings.map((one) => one.querySelector(".repo")?.textContent)).toEqual(["svc", "tool"]);
    expect(headings[0].querySelector(".branch")).toHaveTextContent("main");
    expect(headings[0].querySelector(".upstream")).toHaveTextContent("origin/main");
    expect(within(tree).queryAllByRole("columnheader")).toEqual([]);

    const under = within(row("svc"))
      .getAllByRole("treeitem")
      .map((item) => [item.getAttribute("aria-level"), item.className.split(" ")[0]]);
    expect(under).toEqual([
      ["1", "repo-row"],
      ["2", "changes"],
      ["2", "worktrees"],
      ["3", "piece"],
      ["2", "ci"],
    ]);
    expect(within(row("svc")).getAllByRole("treeitem")[1]).toHaveTextContent("3 changed");
  });

  it("draws every repo's rows whatever charter could read of it", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          panels: { ...PANELS, repos: ["svc", "tool", "gone"], absent: ["later"] },
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [repo("svc"), repo("tool", { unreadable: "could not read it" })],
          },
        })}
      />,
    );

    // A repo git has not answered for yet still says so on each of its rows; one nobody
    // cloned is a heading that says it, with nothing under it to read.
    expect(row("gone")).toHaveTextContent("not read");
    expect(screen.getByTestId("ci-gone")).toHaveTextContent("not read");
    expect(row("tool")).toHaveTextContent("could not read it");
    expect(within(row("later")).getAllByRole("treeitem")).toHaveLength(1);
    expect(row("later")).toHaveTextContent("not cloned here");
  });

  it("draws a clone's worktrees as rows under its Branches row, each with its branch and state", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          pieces: {
            svc: [
              piece("one", { branch: "fix/login" }),
              piece("two", { wired: false }),
              piece("three", { stale: true, wired: false }),
            ],
            tool: [],
          },
          repos: { workspace: "alpha", cache_refused: null, repos: [repo("svc"), repo("tool")] },
        })}
      />,
    );

    const tree = screen.getByTestId("worktree-tree-svc");
    // The shared tree's level (#1682): a `role="group"` under the row it hangs from, whose
    // guide is the `.tree` style's straight line and never the bottom bar's elbows.
    expect(tree).toHaveAttribute("role", "group");
    expect(tree.closest(".tree")).not.toBeNull();
    const rows = within(tree).getAllByRole("treeitem");
    expect(rows.map((one) => one.querySelector(".piece-name")?.textContent)).toEqual([
      "one",
      "two",
      "three",
    ]);
    expect(rows[0]).toHaveTextContent("fix/login");
    // `two` is on a branch called `two`: the name already says it, so it is not said twice.
    expect(rows[1].querySelector(".branch")).toBeNull();
    expect(rows[1]).toHaveTextContent("unwired");
    // Stale says stale and nothing else: a directory that is gone is not also "unwired".
    expect(rows[2]).toHaveTextContent("stale");
    expect(rows[2]).not.toHaveTextContent("unwired");
    // A clone with nothing cut off it has no rows under Branches — that row says "no branches".
    expect(screen.queryByTestId("worktree-tree-tool")).not.toBeInTheDocument();
  });

  // ---------------------------------------------------------------------------------------
  // The keyboard: one stop, the tree's arrows (#1701 line 4)
  // ---------------------------------------------------------------------------------------

  it("is one Tab stop, on its first row, which is where ⌃⇧G lands", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          pieces: { svc: [piece("one")], tool: [] },
          repos: { workspace: "alpha", cache_refused: null, repos: [repo("svc"), repo("tool")] },
        })}
      />,
    );

    // `giveViewTheKeyboard` focuses the tree's `[tabindex="0"]`: there is exactly one.
    const tree = screen.getByRole("tree");
    const stops = tree.querySelectorAll('[tabindex="0"]');
    expect(stops).toHaveLength(1);
    expect(stops[0]).toBe(within(row("svc")).getAllByRole("treeitem")[0]);
  });

  it("steps down the rows, into a repo's rows and back out to its heading", async () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          pieces: { svc: [piece("one")], tool: [] },
          repos: { workspace: "alpha", cache_refused: null, repos: [repo("svc"), repo("tool")] },
        })}
      />,
    );
    const items = within(row("svc")).getAllByRole("treeitem");

    await userEvent.tab();
    expect(items[0]).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(items[1]).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(items[0]).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    expect(items[1]).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(screen.getByTestId("ci-tool")).toHaveFocus();
    // And Tab leaves: the tree is one stop, not one per row.
    await userEvent.tab();
    expect(screen.getByRole("tree")).not.toContainElement(document.activeElement as HTMLElement);
  });

  it.each([
    ["success", "lucide-circle-check", null],
    ["failed", "lucide-circle-x", null],
    ["running", "lucide-loader-circle", "spinning"],
    ["pending", "lucide-clock", "breathing"],
    ["manual", "lucide-hand", null],
    ["canceled", "lucide-circle-slash", null],
    ["skipped", "lucide-skip-forward", null],
  ])(
    "marks a %s pipeline with its own shape, moving only if it is still going",
    (ci, mark, moves) => {
      render(
        <ChangesView
          offers={NO_MENUS}
          onPress={() => {}}
          workspace="alpha"
          state={state({
            repos: {
              workspace: "alpha",
              cache_refused: null,
              repos: [repo("svc", { ci, fetched_seconds_ago: 60, not_fetched: null })],
            },
          })}
        />,
      );

      const cell = screen.getByTestId("ci-svc");
      expect(cell).toHaveTextContent(ci);
      const svg = cell.querySelector(`svg.${mark}`);
      expect(svg).not.toBeNull();
      // Running and queued are different claims, so they move differently (M7.2): a queued
      // run that spun would be drawn as one that is working. And nothing else moves at all —
      // including a settle, because this row was drawn already finished rather than seen to.
      for (const motion of ["spinning", "breathing", "settling"]) {
        expect(svg?.classList.contains(motion), motion).toBe(motion === moves);
      }
    },
  );

  it("settles a pipeline's mark when the run finishes on screen, and only then", () => {
    const bar = (ci: string) => (
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [repo("svc", { ci, fetched_seconds_ago: 60, not_fetched: null })],
          },
        })}
      />
    );
    const { rerender } = render(bar("running"));
    expect(screen.getByTestId("ci-svc").querySelector(".settling")).toBeNull();

    // The run finishes while the row is up: the new mark is drawn settling into place.
    rerender(bar("success"));
    const settled = screen.getByTestId("ci-svc").querySelector("svg.lucide-circle-check");
    expect(settled?.classList.contains("settling")).toBe(true);
    expect(settled?.classList.contains("spinning")).toBe(false);
  });

  it("does not settle a mark the row was first drawn with", () => {
    // A bar opened on a workspace whose pipelines all finished an hour ago is a row of
    // settled answers, and a row of them all growing into place at once is decoration.
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [repo("svc", { ci: "success", fetched_seconds_ago: 60, not_fetched: null })],
          },
        })}
      />,
    );
    expect(screen.getByTestId("ci-svc").querySelector(".settling")).toBeNull();
  });

  it("draws an answer it does not recognise as a fetch that names nothing, and keeps still", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [repo("svc", { ci: "exploded", fetched_seconds_ago: 60, not_fetched: null })],
          },
        })}
      />,
    );

    const cell = screen.getByTestId("ci-svc");
    expect(cell.querySelector("svg.lucide-circle-dashed")).not.toBeNull();
    expect(cell.querySelector(".spinning")).toBeNull();
  });

  // ---------------------------------------------------------------------------------------
  // The rule, as far as a test can hold it (ADR 0038)
  // ---------------------------------------------------------------------------------------

  it("has nothing in it to press, because Changes is what is true and not what you do", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          pieces: { svc: [piece("one")], tool: [] },
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [repo("svc"), repo("tool")],
          },
        })}
      />,
    );

    expect(
      screen
        .getByTestId("changes-view")
        .querySelectorAll("button, input, textarea, select, a[href]"),
    ).toHaveLength(0);
  });
});

/**
 * Right-click on a repo's row (charter-app#174, the second half).
 *
 * **A menu is not a control, and the region stays unpressable**: the row gains no button, the
 * menu is drawn in a portal outside the region, and the rows it lists are about where a chat
 * starts — nothing in them touches the repo the region is reading. The worktree rows under a
 * repo stay without one: `Remove worktree` down here would be the amendment to ADR 0038 the
 * docstring says it would be.
 */
describe("a repo row's menu", () => {
  const offers = () =>
    catalogued(
      catalogue({
        tabs: noTabs(),
        workspaces: ["alpha"],
        focused: "alpha",
        plane: "/plane",
        clones: [
          { repo: "svc", path: "/p/svc" },
          { repo: "tool", path: "/p/tool" },
        ],
        pieces: [{ workspace: "alpha", repo: "svc", piece: "one" }],
        absent: ["later"],
        needsYou: [],
        nameOf: String,
      }),
    );
  const bar = (onPress: (id: string) => void = () => {}) =>
    render(
      <ChangesView
        workspace="alpha"
        offers={offers()}
        onPress={(offer) => onPress(offer.id)}
        state={state({
          panels: { ...PANELS, absent: ["later"] },
          pieces: { svc: [piece("one")], tool: [] },
        })}
      />,
    );
  const rightClick = (el: Element) =>
    el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));

  it("offers the clone's own rows, which are the explorer's clone rows", async () => {
    bar();

    rightClick(within(row("svc")).getByText("svc"));

    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((one) => one.getAttribute("aria-label")),
    ).toEqual([
      "Focus on repo svc",
      "New tab in svc",
      "New branch in svc…",
      "Start new chats in svc",
    ]);
  });

  it("hands the catalogue's offer back when a row is pressed", async () => {
    const pressed: string[] = [];
    bar((id) => pressed.push(id));

    rightClick(within(row("tool")).getByText("tool"));
    await screen.findByRole("menu");
    await userEvent.click(screen.getByRole("menuitem", { name: "New tab in tool" }));

    expect(pressed).toEqual(["clone.chat:tool"]);
  });

  it("offers Clone on a repo nobody cloned here, which the explorer's row offers too (#1215)", async () => {
    const pressed: string[] = [];
    bar((id) => pressed.push(id));

    rightClick(within(row("later")).getByText("later"));
    await screen.findByRole("menu");
    await userEvent.click(screen.getByRole("menuitem", { name: "Clone later" }));

    expect(pressed).toEqual(["absent.clone:later"]);
  });

  it("has none on the worktrees under a repo", () => {
    bar();

    rightClick(within(screen.getByTestId("worktree-tree-svc")).getByRole("treeitem"));

    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens on the row the keyboard is on, with Shift+F10", async () => {
    bar();

    await userEvent.tab();
    expect(within(row("svc")).getAllByRole("treeitem")[0]).toHaveFocus();
    await userEvent.keyboard("{Shift>}{F10}{/Shift}");

    expect(
      within(await screen.findByRole("menu")).getByRole("menuitem", { name: "New tab in svc" }),
    ).toBeInTheDocument();
  });

  it("is on a repo's own rows under its heading too", async () => {
    bar();

    rightClick(screen.getByTestId("ci-svc"));

    expect(
      within(await screen.findByRole("menu")).getByRole("menuitem", { name: "New tab in svc" }),
    ).toBeInTheDocument();
  });

  it("still leaves nothing in the region to press", () => {
    bar();

    expect(
      screen
        .getByTestId("changes-view")
        .querySelectorAll("button, input, textarea, select, a[href]"),
    ).toHaveLength(0);
  });
});

/**
 * **A refusal down here has its way out without the explorer** (#1244): the explorer's Read
 * again is a Notice's button, and with the explorer hidden there was nothing to press. The
 * refusal line has a menu with the catalogue's Read again — a menu, not a control (#174), so
 * the region stays unpressable — and the palette has the same row while a refusal stands.
 */
describe("a refused read's menu (#1244)", () => {
  const offers = (readRefused: boolean) =>
    catalogued(
      catalogue({
        tabs: noTabs(),
        workspaces: ["alpha"],
        focused: "alpha",
        plane: "/plane",
        clones: [{ repo: "svc", path: "/p/svc" }],
        needsYou: [],
        nameOf: String,
        readRefused,
      }),
    );
  const rightClick = (el: Element) =>
    el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));

  it("offers Read again on the workspace's refusal, and hands the row back", async () => {
    const pressed: string[] = [];
    render(
      <ChangesView
        workspace="alpha"
        offers={offers(true)}
        onPress={(offer) => pressed.push(offer.id)}
        state={state({ trouble: "purlis could not read alpha: permission denied" })}
      />,
    );

    rightClick(screen.getByText("purlis could not read alpha: permission denied"));
    await screen.findByRole("menu");
    await userEvent.click(screen.getByRole("menuitem", { name: "Read the workspace again" }));

    expect(pressed).toEqual(["workspace.readagain"]);
  });

  it("offers it on the forge cache's refusal and on a repo it could not read", async () => {
    render(
      <ChangesView
        workspace="alpha"
        offers={offers(true)}
        onPress={() => {}}
        state={state({
          panels: { ...PANELS, repos: ["svc"] },
          repos: {
            workspace: "alpha",
            cache_refused: "purlis could not read the forge cache",
            repos: [repo("svc", { branch: null, unreadable: "could not read the tree" })],
          },
        })}
      />,
    );

    rightClick(screen.getByText("purlis could not read the forge cache"));
    expect(
      within(await screen.findByRole("menu")).getByRole("menuitem", {
        name: "Read the workspace again",
      }),
    ).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");

    rightClick(within(row("svc")).getByText("could not read the tree"));
    expect(
      within(await screen.findByRole("menu")).getByRole("menuitem", {
        name: "Read the workspace again",
      }),
    ).toBeInTheDocument();
  });

  it("is not on a repo's menu while nothing is refused", async () => {
    render(
      <ChangesView
        workspace="alpha"
        offers={offers(false)}
        onPress={() => {}}
        state={state({ panels: { ...PANELS, repos: ["svc"] } })}
      />,
    );

    rightClick(within(row("svc")).getByText("svc"));
    const menu = await screen.findByRole("menu");
    expect(within(menu).queryByRole("menuitem", { name: "Read the workspace again" })).toBeNull();
  });
});

describe("an extension's repo columns (charter-app#340)", () => {
  const column = (over: Partial<FactColumn> = {}): FactColumn => ({
    extension: "prs",
    id: "open",
    title: "PRs",
    cells: [{ repo: "svc", value: "2", age_seconds: 30, stale: false }],
    ...over,
  });

  it("adds a row under each repo its facts file filled, named by the column", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state()}
        columns={[column()]}
      />,
    );

    expect(screen.getByTestId("fact-prs-open-svc")).toHaveTextContent("PRs 2");
    expect(screen.getByTestId("fact-prs-open-svc")).toHaveAttribute("role", "treeitem");
    expect(within(screen.getByTestId("fact-prs-open-svc")).getByText("PRs")).toHaveAttribute(
      "title",
      "From the extension prs",
    );
    // A repo the file did not name has no row, never a borrowed value.
    expect(screen.queryByTestId("fact-prs-open-tool")).not.toBeInTheDocument();
  });

  it("dims a stale cell and says how old it is", () => {
    render(
      <ChangesView
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state()}
        columns={[column({ cells: [{ repo: "svc", value: "2", age_seconds: 7200, stale: true }] })]}
      />,
    );

    const cell = screen.getByTestId("fact-prs-open-svc");
    expect(cell).toHaveClass("stale");
    expect(cell).toHaveTextContent("PRs 2 · 2h ago");
  });

  it("draws the rows it always drew when no extension adds one", () => {
    render(<ChangesView offers={NO_MENUS} onPress={() => {}} workspace="alpha" state={state()} />);

    expect(within(row("svc")).getAllByRole("treeitem")).toHaveLength(4);
  });
});
