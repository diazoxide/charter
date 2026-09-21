import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { BottomBar } from "./BottomBar";
import type { Panels as PanelsModel, Piece, RepoState } from "./bindings";
import type { WorkspaceState } from "./workspaceState";

afterEach(cleanup);

const PANELS: PanelsModel = {
  workspace: "alpha",
  repos: ["svc", "tool"],
  absent: [],
  refused: [],
  todos: [],
  todos_refused: null,
  personas: ["steward"],
  persona: "steward",
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

const row = (name: string) => screen.getByTestId(`repo-${name}`);
const ci = (name: string) => screen.getByTestId(`ci-${name}`);

describe("the bottom bar", () => {
  it("shows each repo's branch, how far it is from its upstream, and what is uncommitted", () => {
    render(
      <BottomBar
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
    render(<BottomBar workspace="alpha" state={state({ repos: undefined, reading: true })} />);

    expect(row("svc")).toHaveTextContent("reading…");
  });

  // ---------------------------------------------------------------------------------------
  // Worktrees, which are the bottom bar's half of the pair (charter ADR 0038)
  // ---------------------------------------------------------------------------------------

  it("counts the worktrees cut off each clone", () => {
    render(
      <BottomBar
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
        workspace="alpha"
        state={state({ piecesRefused: { svc: "charter will not run git through a symlink" } })}
      />,
    );

    expect(screen.getByTestId("worktrees-svc")).toHaveTextContent("worktrees unreadable");
    expect(screen.getByTestId("worktrees-svc")).not.toHaveTextContent("no worktrees");
  });

  it("says a listing is still on its way rather than counting nothing", () => {
    render(<BottomBar workspace="alpha" state={state()} />);

    expect(screen.getByTestId("worktrees-svc")).toHaveTextContent("asking git");
  });

  // ---------------------------------------------------------------------------------------
  // CI, which is read and never fetched
  // ---------------------------------------------------------------------------------------

  it("shows the CI state the cache holds, with the change and how old the answer is", () => {
    render(
      <BottomBar
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
    render(<BottomBar workspace="alpha" state={state()} />);

    expect(screen.getByTestId("bottom-bar")).toHaveTextContent("never fetches");
  });

  it("shows a forge cache charter refused, once for the listing", () => {
    render(
      <BottomBar
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
        workspace="alpha"
        state={state({ panels: { ...PANELS, repos: ["svc"], absent: ["later"] } })}
      />,
    );

    expect(row("later")).toHaveTextContent("not cloned here");
  });

  it("says when the core refused the workspace outright", () => {
    render(
      <BottomBar
        workspace="ghost"
        state={state({ panels: undefined, trouble: "no workspace 'ghost'" })}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("ghost");
  });

  it("draws nothing about a workspace when none is focused", () => {
    render(<BottomBar workspace={undefined} state={state()} />);

    expect(screen.getByText("No workspace focused.")).toBeInTheDocument();
    expect(screen.queryByTestId("repo-svc")).not.toBeInTheDocument();
  });

  // ---------------------------------------------------------------------------------------
  // The rule, as far as a test can hold it (charter ADR 0038)
  // ---------------------------------------------------------------------------------------

  it("has nothing in it to press, because the bottom is what is true and not what you do", () => {
    render(
      <BottomBar
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
