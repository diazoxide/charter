import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Panels } from "./Panels";
import type { Panels as PanelsModel, RepoState, RepoStates } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
});

const PANELS: PanelsModel = {
  workspace: "alpha",
  repos: ["svc", "tool"],
  absent: [],
  refused: [],
  todos: [
    { slug: "20260302-091400-review", title: "Review the rollout plan", stamp: "2026-03-02" },
  ],
  todos_refused: null,
  personas: ["devops", "steward"],
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

/** Answers both panel commands, with whatever this test wants them to say. */
function core(panels: PanelsModel | Error = PANELS, repos: Partial<RepoStates> = {}) {
  mockIPC((cmd) => {
    if (cmd === "workspace_panels") {
      if (panels instanceof Error) throw panels;
      return panels;
    }
    if (cmd === "workspace_repos") {
      return { workspace: "alpha", repos: [], cache_refused: null, ...repos };
    }
    return null;
  });
}

/** The project these panels are for. A workspace name is only half an answer: two
 *  projects can both have an `alpha`, so every ask carries the project it is about. */
const PLANE = "/home/dev/plane";

const row = (name: string) => screen.findByTestId(`repo-${name}`);
const ci = (name: string) => screen.findByTestId(`ci-${name}`);

describe("Panels", () => {
  it("draws the plane's own answer without waiting for git", async () => {
    // The two asks are separate so that the todo list is not behind `git status` on every
    // clone. Here the repo answer never arrives at all, and the rest still draws.
    mockIPC((cmd) => {
      if (cmd === "workspace_panels") return PANELS;
      return new Promise(() => {});
    });

    render(<Panels plane={PLANE} workspace="alpha" />);

    expect(await screen.findByText("Review the rollout plan")).toBeInTheDocument();
    expect(within(await screen.findByTestId("panel-personas")).getByText(/steward/)).toBeVisible();
    expect(await row("svc")).toHaveTextContent("reading…");
  });

  it("says which persona a chat started here would adopt", async () => {
    core();

    render(<Panels plane={PLANE} workspace="alpha" />);

    const personas = await screen.findByTestId("panel-personas");
    await waitFor(() => expect(personas).toHaveTextContent("steward · default"));
    expect(personas).toHaveTextContent("devops");
    expect(within(personas).getByText("devops")).not.toHaveTextContent("default");
  });

  it("shows each repo's branch, how far it is from its upstream, and what is uncommitted", async () => {
    core(PANELS, {
      repos: [
        repo("svc", { upstream: "origin/main", ahead: 2, behind: 1, tracked: 3, untracked: 1 }),
        repo("tool"),
      ],
    });

    render(<Panels plane={PLANE} workspace="alpha" />);

    const svc = await row("svc");
    await waitFor(() => expect(svc).toHaveTextContent("main"));
    expect(svc).toHaveTextContent("origin/main");
    expect(svc).toHaveTextContent("2 ahead, 1 behind");
    expect(svc).toHaveTextContent("3 changed, 1 untracked");
    expect(await row("tool")).toHaveTextContent("clean");
  });

  it("never draws a tree charter could not read as clean", async () => {
    // The whole reason the core has a third state. "Clean" here is a lie that reads as
    // "nothing to do", which is exactly the wrong thing to tell someone in a hurry.
    core(PANELS, {
      repos: [
        repo("svc", {
          branch: null,
          unreadable:
            "charter could not read the working tree at /p/svc — fatal: not a git repository. " +
            "That is not the same as it being clean",
        }),
      ],
    });

    render(<Panels plane={PLANE} workspace="alpha" />);

    const svc = await row("svc");
    await waitFor(() => expect(svc).toHaveTextContent("could not read"));
    // No dirt cell at all — not "clean", and not a count either, because every number on a
    // row charter could not read would be one it made up.
    expect(svc.querySelector(".dirt")).toBeNull();
    expect(within(svc).getByRole("alert")).toBeInTheDocument();
  });

  it("says a branch has no commits yet rather than drawing it like any other", async () => {
    core(PANELS, { repos: [repo("svc", { unborn: true })] });

    render(<Panels plane={PLANE} workspace="alpha" />);

    await waitFor(async () => expect(await row("svc")).toHaveTextContent("no commits yet"));
  });

  it("names the commit a detached checkout sits on", async () => {
    core(PANELS, { repos: [repo("svc", { branch: null, detached: "1a2b3c4" })] });

    render(<Panels plane={PLANE} workspace="alpha" />);

    await waitFor(async () => expect(await row("svc")).toHaveTextContent("detached at 1a2b3c4"));
  });

  // -------------------------------------------------------------------------------------
  // CI, which is read and never fetched
  // -------------------------------------------------------------------------------------

  it("shows the CI state the cache holds, with the change and how old the answer is", async () => {
    core(PANELS, {
      repos: [
        repo("svc", {
          ci: "failed",
          change: 41,
          sigil: "#",
          fetched_seconds_ago: 120,
          not_fetched: null,
        }),
      ],
    });

    render(<Panels plane={PLANE} workspace="alpha" />);

    const cell = await ci("svc");
    await waitFor(() => expect(cell).toHaveTextContent("failed"));
    expect(cell).toHaveTextContent("#41");
    // The age is part of the claim: a two-hour-old "success" is not the same as a fresh one.
    expect(cell).toHaveTextContent("2m ago");
  });

  it("says why there is no CI state rather than leaving the cell blank", async () => {
    // A blank cell reads as "fine". The reason is the whole point of the row.
    core(PANELS, {
      repos: [repo("svc", { not_fetched: "the last fetch was for a different branch" })],
    });

    render(<Panels plane={PLANE} workspace="alpha" />);

    const cell = await ci("svc");
    await waitFor(() => expect(cell).toHaveTextContent("not fetched"));
    expect(cell).toHaveTextContent("different branch");
  });

  it("tells a fetch that named no pipeline from no fetch at all", async () => {
    core(PANELS, { repos: [repo("svc", { fetched_seconds_ago: 30, not_fetched: null })] });

    render(<Panels plane={PLANE} workspace="alpha" />);

    const cell = await ci("svc");
    await waitFor(() => expect(cell).toHaveTextContent("no pipeline recorded"));
    expect(cell).not.toHaveTextContent("not fetched");
  });

  it("says the app reads CI state and does not fetch it", async () => {
    core();

    render(<Panels plane={PLANE} workspace="alpha" />);

    expect(
      await within(await screen.findByTestId("panel-ci")).findByText(/never fetches/),
    ).toBeVisible();
  });

  it("shows a forge cache charter refused, once for the listing", async () => {
    core(PANELS, {
      cache_refused: "/p/.charter/cache/glstate.json is reached through a symlink",
    });

    render(<Panels plane={PLANE} workspace="alpha" />);

    const panel = await screen.findByTestId("panel-ci");
    await waitFor(() => expect(within(panel).getByRole("alert")).toHaveTextContent("symlink"));
  });

  // -------------------------------------------------------------------------------------
  // Refusals, which are shown
  // -------------------------------------------------------------------------------------

  it("shows what charter would not read instead of a workspace with fewer repos in it", async () => {
    core({
      ...PANELS,
      repos: ["svc"],
      refused: [["alias", "'alias' is reached through a symlink, and a worktree path may not be"]],
    });

    render(<Panels plane={PLANE} workspace="alpha" />);

    const panel = await screen.findByTestId("panel-repos");
    await waitFor(() => expect(within(panel).getByRole("alert")).toHaveTextContent("alias"));
  });

  it("shows a repo the manifest names that nobody has cloned", async () => {
    core({ ...PANELS, repos: ["svc"], absent: ["later"] });

    render(<Panels plane={PLANE} workspace="alpha" />);

    await waitFor(async () => expect(await row("later")).toHaveTextContent("not cloned here"));
  });

  it("says why the todos could not be read rather than showing none", async () => {
    core({ ...PANELS, todos: [], todos_refused: "todos/ resolves outside the plane" });

    render(<Panels plane={PLANE} workspace="alpha" />);

    const panel = await screen.findByTestId("panel-todos");
    await waitFor(() => expect(within(panel).getByRole("alert")).toHaveTextContent("outside"));
  });

  it("says when the core refused the workspace outright", async () => {
    core(new Error("no workspace 'ghost'"));

    render(<Panels plane={PLANE} workspace="ghost" />);

    expect(await screen.findByRole("alert")).toHaveTextContent("ghost");
  });

  it("draws nothing about a workspace when none is focused", async () => {
    core();

    render(<Panels plane={PLANE} workspace={undefined} />);

    expect(await screen.findByText("No workspace focused.")).toBeInTheDocument();
  });

  it("throws away an answer that arrives for a workspace no longer focused", async () => {
    // The panels are re-asked on every focus change, and the answers race. One that names a
    // different workspace is another workspace's, and drawing it would put beta's repos
    // under alpha's heading.
    mockIPC((cmd) => {
      if (cmd === "workspace_panels") return { ...PANELS, workspace: "beta", repos: ["other"] };
      // This one IS alpha's, and it is what the test waits on: once it has been drawn, the
      // other answer has certainly arrived too, so what follows is about it being dropped
      // and not about it being slow.
      return { workspace: "alpha", repos: [], cache_refused: "the cache was not read" };
    });

    render(<Panels plane={PLANE} workspace="alpha" />);

    await waitFor(() =>
      expect(screen.getByTestId("panel-ci")).toHaveTextContent("the cache was not read"),
    );
    expect(screen.queryByTestId("repo-other")).not.toBeInTheDocument();
    expect(screen.getByTestId("panel-repos")).toHaveTextContent("Reading the plane…");
  });
});
