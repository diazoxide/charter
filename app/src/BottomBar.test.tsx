/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { BottomBar } from "./BottomBar";
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

describe("the bottom bar", () => {
  it("shows each repo's branch, how far it is from its upstream, and what is uncommitted", () => {
    render(
      <BottomBar
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

  it("never draws a tree charter could not read as clean", () => {
    // The whole reason the core has a third state. "Clean" here is a lie that reads as
    // "nothing to do", which is exactly the wrong thing to tell someone in a hurry.
    render(
      <BottomBar
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
      <BottomBar
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
      <BottomBar
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
      <BottomBar
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({ repos: undefined, reading: true })}
      />,
    );

    expect(row("svc")).toHaveTextContent("reading…");
  });

  // ---------------------------------------------------------------------------------------
  // Worktrees, which are the bottom bar's half of the pair (ADR 0038)
  // ---------------------------------------------------------------------------------------

  it("counts the worktrees cut off each clone", () => {
    render(
      <BottomBar
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({ pieces: { svc: [piece("one"), piece("two")], tool: [] } })}
      />,
    );

    expect(screen.getByTestId("worktrees-svc")).toHaveTextContent("2 worktrees");
    expect(screen.getByTestId("worktrees-tool")).toHaveTextContent("no worktrees");
  });

  it("counts the worktrees a chat would run unwired or stale in", () => {
    // The two states that change what starting a chat in one MEANS. Counted here; the row a
    // person acts on is the explorer's.
    render(
      <BottomBar
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
      <BottomBar
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({ piecesRefused: { svc: "charter will not run git through a symlink" } })}
      />,
    );

    expect(screen.getByTestId("worktrees-svc")).toHaveTextContent("worktrees unreadable");
    expect(screen.getByTestId("worktrees-svc")).not.toHaveTextContent("no worktrees");
  });

  it("says a listing is still on its way rather than counting nothing", () => {
    render(<BottomBar offers={NO_MENUS} onPress={() => {}} workspace="alpha" state={state()} />);

    expect(screen.getByTestId("worktrees-svc")).toHaveTextContent("asking git");
  });

  // ---------------------------------------------------------------------------------------
  // CI, which is read and never fetched
  // ---------------------------------------------------------------------------------------

  it("shows the CI state the cache holds, with the change and how old the answer is", () => {
    render(
      <BottomBar
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
      <BottomBar
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
      <BottomBar
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
    render(<BottomBar offers={NO_MENUS} onPress={() => {}} workspace="alpha" state={state()} />);

    expect(screen.getByTestId("bottom-bar")).toHaveTextContent("never fetches");
  });

  it("shows a forge cache charter refused, once for the listing", () => {
    render(
      <BottomBar
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

  it("shows what charter would not read instead of a workspace with fewer repos in it", () => {
    render(
      <BottomBar
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
      <BottomBar
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
      <BottomBar
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
      <BottomBar offers={NO_MENUS} onPress={() => {}} workspace={undefined} state={state()} />,
    );

    expect(screen.getByText("No workspace focused.")).toBeInTheDocument();
    expect(screen.queryByTestId("repo-svc")).not.toBeInTheDocument();
  });

  // ---------------------------------------------------------------------------------------
  // Columns, a worktree tree, and pipelines that move (M6.6)
  // ---------------------------------------------------------------------------------------

  it("lays the repos out in named columns, one row per repo", () => {
    render(
      <BottomBar
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state({
          repos: {
            workspace: "alpha",
            cache_refused: null,
            repos: [repo("svc", { tracked: 3 }), repo("tool")],
          },
        })}
      />,
    );

    // The operator's "add tabs columns": a column question — which of these is dirty — is
    // answered by reading down, and a screen reader says which column a value is in.
    expect(screen.getAllByRole("columnheader").map((one) => one.textContent)).toEqual([
      "Repo",
      "Branch",
      "Changes",
      "Worktrees",
      "Pipeline",
    ]);
    expect(screen.getAllByRole("rowheader").map((one) => one.textContent)).toEqual(["svc", "tool"]);
    const cells = within(row("svc")).getAllByRole("cell");
    expect(cells.map((one) => one.className.split(" ")[0])).toEqual([
      "branch",
      "dirt",
      "worktrees",
      "ci",
    ]);
    expect(cells[1]).toHaveTextContent("3 changed");
  });

  it("gives every row the same five columns, whatever charter could read of it", () => {
    render(
      <BottomBar
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

    // A row one column short draws the next repo's pipeline under this one's worktrees.
    const width = (tr: HTMLElement) =>
      [...tr.children].reduce((sum, cell) => sum + Number(cell.getAttribute("colspan") ?? 1), 0);
    for (const name of ["svc", "tool", "gone", "later"]) expect(width(row(name))).toBe(5);
  });

  it("draws a clone's worktrees as a tree under its row, each with its branch and state", () => {
    render(
      <BottomBar
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
    const rows = within(tree).getAllByRole("listitem");
    expect(rows.map((one) => one.querySelector(".piece")?.textContent)).toEqual([
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
    // A clone with nothing cut off it has no tree — the row already says "no worktrees".
    expect(screen.queryByTestId("worktree-tree-tool")).not.toBeInTheDocument();
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
        <BottomBar
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
      <BottomBar
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
      <BottomBar
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
      <BottomBar
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

  /**
   * **No table cell in the bottom bar is ever a flex container.** A `<td>` given `display: flex`
   * stops being a table-cell, and the column it was laid out in goes with it — which is the whole
   * reason this region is a table. Everything inside a cell is laid out inline. jsdom lays out
   * nothing, so this reads the stylesheet, and it is what stops the obvious "just flex the icon
   * and the text" fix from quietly un-aligning every column.
   */
  describe("the bottom bar's stylesheet", () => {
    const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
      /\/\*[\s\S]*?\*\//g,
      "",
    );
    const rules = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)].map(([, selector, body]) => ({
      selectors: selector.split(",").map((one) => one.trim()),
      body,
    }));
    // What a cell is, in this region: the row's own cells, and the classes a cell carries.
    const CELL = /^\.state-bar (?:\.repo-row > (?:td|th)|\.repo|\.branch|\.dirt|\.worktrees|\.ci)$/;

    it("reaches the cells it styles", () => {
      expect(rules.some(({ selectors }) => selectors.some((one) => CELL.test(one)))).toBe(true);
    });

    it("never makes a cell a flex or grid container", () => {
      const flexed = rules
        .filter(({ body }) => /(?:^|[;\s])display\s*:\s*(?:inline-)?(?:flex|grid)/.test(body))
        .flatMap(({ selectors }) => selectors.filter((one) => CELL.test(one)));
      expect(flexed).toEqual([]);
    });

    it("draws the worktree tree's guides with borders only, like the explorer's", () => {
      const guides = rules.filter(({ selectors }) =>
        selectors.some((one) => /\.worktree-tree.*::(?:before|after)/.test(one)),
      );
      expect(guides.length).toBeGreaterThan(0);
      for (const { body } of guides)
        expect(body).not.toMatch(/(?:^|[;\s])background(?:-color)?\s*:/);
    });
  });

  // ---------------------------------------------------------------------------------------
  // The rule, as far as a test can hold it (ADR 0038)
  // ---------------------------------------------------------------------------------------

  it("has nothing in it to press, because the bottom is what is true and not what you do", () => {
    render(
      <BottomBar
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
      screen.getByTestId("bottom-bar").querySelectorAll("button, input, textarea, select, a[href]"),
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
        needsYou: [],
        nameOf: String,
      }),
    );
  const bar = (onPress: (id: string) => void = () => {}) =>
    render(
      <BottomBar
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
    ).toEqual(["New tab in svc", "Start new chats in svc"]);
  });

  it("hands the catalogue's offer back when a row is pressed", async () => {
    const pressed: string[] = [];
    bar((id) => pressed.push(id));

    rightClick(row("tool"));
    await screen.findByRole("menu");
    await userEvent.click(screen.getByRole("menuitem", { name: "New tab in tool" }));

    expect(pressed).toEqual(["clone.chat:tool"]);
  });

  it("has none on a repo nobody cloned, nor on the worktrees under a repo", () => {
    bar();

    rightClick(row("later"));
    rightClick(within(screen.getByTestId("worktree-tree-svc")).getByText("one"));

    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("still leaves nothing in the region to press", () => {
    bar();

    expect(
      screen.getByTestId("bottom-bar").querySelectorAll("button, input, textarea, select, a[href]"),
    ).toHaveLength(0);
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

  it("adds a heading for each column and a cell for each repo its facts file filled", () => {
    render(
      <BottomBar
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state()}
        columns={[column()]}
      />,
    );

    expect(screen.getByRole("columnheader", { name: "PRs" })).toBeInTheDocument();
    expect(screen.getByTestId("fact-prs-open-svc")).toHaveTextContent("2");
    // A repo the file did not name has an empty cell, never a borrowed value.
    expect(screen.getByTestId("fact-prs-open-tool")).toBeEmptyDOMElement();
  });

  it("dims a stale cell and says how old it is", () => {
    render(
      <BottomBar
        offers={NO_MENUS}
        onPress={() => {}}
        workspace="alpha"
        state={state()}
        columns={[column({ cells: [{ repo: "svc", value: "2", age_seconds: 7200, stale: true }] })]}
      />,
    );

    const cell = screen.getByTestId("fact-prs-open-svc");
    expect(cell).toHaveClass("stale");
    expect(cell).toHaveTextContent("2 · 2h ago");
  });

  it("draws the five columns it always drew when no extension adds one", () => {
    render(<BottomBar offers={NO_MENUS} onPress={() => {}} workspace="alpha" state={state()} />);

    expect(screen.getAllByRole("columnheader")).toHaveLength(5);
  });
});
