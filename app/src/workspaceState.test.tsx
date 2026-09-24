import { afterEach, describe, expect, it } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { useWorkspaceState } from "./workspaceState";
import type { Panels as PanelsModel } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
});

/** The project these answers are for. A workspace name is only half an answer: two projects
 *  can both have an `alpha`, so every ask carries the project it is about. */
const PLANE = "/home/dev/plane";

const PANELS: PanelsModel = {
  workspace: "alpha",
  repos: ["svc", "tool"],
  absent: [],
  refused: [],
  todos: [],
  todos_refused: null,
  personas: ["steward"],
  persona: "steward",
  contributed: [],
};

function piece(name: string, on: Record<string, unknown> = {}) {
  return {
    piece: name,
    path: `/home/dev/plane/workspaces/alpha/.worktrees/svc/${name}`,
    branch: name,
    wired: true,
    stale: false,
    ...on,
  };
}

/** The core, counting what it was asked. */
function core(answer: (cmd: string, args: Record<string, unknown>) => unknown): {
  asked: { cmd: string; args: Record<string, unknown> }[];
} {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    return answer(cmd, (args ?? {}) as Record<string, unknown>);
  });
  return { asked };
}

const ORDINARY = (cmd: string, args: Record<string, unknown>) => {
  if (cmd === "workspace_panels") return PANELS;
  if (cmd === "workspace_repos") return { workspace: "alpha", repos: [], cache_refused: null };
  if (cmd === "worktree_list") return args.repo === "svc" ? [piece("one"), piece("two")] : [];
  return null;
};

describe("useWorkspaceState", () => {
  it("reads the plane again when it changes on disk, and asks git nothing more", async () => {
    // charter-app#264. The plane moving on disk is a todo closed in a terminal, so the plane
    // read runs again — and `git status` per clone, five seconds each at worst, does not, nor
    // does `git worktree list` while the clone names are the same.
    // A fresh answer per read, as the core's is: the clone list is a new array each time.
    const { asked } = core((cmd, args) =>
      cmd === "workspace_panels" ? { ...PANELS, repos: [...PANELS.repos] } : ORDINARY(cmd, args),
    );
    const { result, rerender } = renderHook(
      ({ changed }: { changed: number }) => useWorkspaceState(PLANE, "alpha", 0, changed),
      { initialProps: { changed: 0 } },
    );
    await waitFor(() => expect(result.current.pieces.tool).toEqual([]));

    rerender({ changed: 1 });
    await waitFor(() =>
      expect(asked.filter((one) => one.cmd === "workspace_panels")).toHaveLength(2),
    );
    await new Promise((settle) => setTimeout(settle, 20));

    expect(asked.filter((one) => one.cmd === "workspace_repos")).toHaveLength(1);
    expect(asked.filter((one) => one.cmd === "worktree_list")).toHaveLength(2);
    expect(result.current.pieces.svc).toHaveLength(2);
  });

  it("asks git for the pieces once per clone, however often the window renders", async () => {
    // **The measurement #133 asked for, kept as a bound rather than a millisecond.** Every
    // one of these is a `git worktree list` subprocess in the core, so the thing worth
    // pinning is the COUNT: one per clone per focused workspace, and none at all for a
    // re-render. An effect that depended on a fresh array would make it one per clone per
    // render, which nothing on screen would show and every keystroke would pay for.
    const { asked } = core(ORDINARY);

    const { result, rerender } = renderHook(() => useWorkspaceState(PLANE, "alpha"));

    await waitFor(() => expect(result.current.pieces.svc).toHaveLength(2));
    rerender();
    rerender();
    rerender();
    await waitFor(() => expect(result.current.pieces.tool).toEqual([]));

    const lists = asked.filter((one) => one.cmd === "worktree_list");
    expect(lists.map((one) => one.args.repo)).toEqual(["svc", "tool"]);
    // And the two asks that were here before this record, still once each.
    expect(asked.filter((one) => one.cmd === "workspace_panels")).toHaveLength(1);
    expect(asked.filter((one) => one.cmd === "workspace_repos")).toHaveLength(1);
  });

  it("names its plane on every ask, because a workspace name is only half an answer", async () => {
    const { asked } = core(ORDINARY);

    const { result } = renderHook(() => useWorkspaceState(PLANE, "alpha"));

    await waitFor(() => expect(result.current.pieces.svc).toHaveLength(2));
    for (const one of asked) expect(one.args.plane).toBe(PLANE);
  });

  it("says why a clone's worktrees could not be listed rather than showing none", async () => {
    // "No worktrees" and "git would not answer" are different claims, and the second one is
    // the one an operator has to act on.
    core((cmd, args) => {
      if (cmd === "worktree_list" && args.repo === "svc")
        throw new Error("charter will not run git through a symlink");
      return ORDINARY(cmd, args);
    });

    const { result } = renderHook(() => useWorkspaceState(PLANE, "alpha"));

    await waitFor(() => expect(result.current.piecesRefused.svc).toMatch(/symlink/));
    expect(result.current.pieces.svc).toBeUndefined();
  });

  it("throws away a listing that arrives for a workspace no longer focused", async () => {
    // The asks are re-made on every focus change and the answers race. A piece of beta drawn
    // under alpha's clone is the app pointing the next chat at the wrong directory.
    let held: ((value: unknown) => void) | undefined;
    core((cmd, args) => {
      if (cmd === "workspace_panels")
        return { ...PANELS, workspace: args.workspace, repos: ["svc"] };
      if (cmd === "workspace_repos")
        return { workspace: args.workspace, repos: [], cache_refused: null };
      if (cmd === "worktree_list" && args.workspace === "alpha")
        return new Promise((resolve) => (held = resolve));
      return [piece("beta-one")];
    });

    const { result, rerender } = renderHook(
      ({ workspace }: { workspace: string }) => useWorkspaceState(PLANE, workspace),
      { initialProps: { workspace: "alpha" } },
    );
    await waitFor(() => expect(result.current.panels?.workspace).toBe("alpha"));

    rerender({ workspace: "beta" });
    await waitFor(() => expect(result.current.panels?.workspace).toBe("beta"));
    // alpha's listing lands now, after the focus has moved.
    held?.([piece("alpha-one")]);

    await waitFor(() => expect(result.current.pieces.svc?.[0]?.piece).toBe("beta-one"));
    expect(result.current.pieces.svc?.map((one) => one.piece)).not.toContain("alpha-one");
  });

  it("reads the workspace again when the window says it changed it (charter-app#174)", async () => {
    // The explorer's rows can remove a worktree now, and the tree they removed it from is
    // drawn out of this record. Without a way to ask again, the row stayed on screen until
    // the operator focused another workspace and came back — which looks exactly like a
    // removal that silently failed.
    let pieces = [piece("one"), piece("two")];
    const { asked } = core((cmd, args) => {
      if (cmd === "worktree_list") return args.repo === "svc" ? pieces : [];
      return ORDINARY(cmd, args);
    });

    const { result, rerender } = renderHook(
      ({ again }: { again: number }) => useWorkspaceState(PLANE, "alpha", again),
      { initialProps: { again: 0 } },
    );
    await waitFor(() => expect(result.current.pieces.svc).toHaveLength(2));

    // `one` is removed, and the window says so with the counter.
    pieces = [piece("two")];
    rerender({ again: 1 });

    await waitFor(() => expect(result.current.pieces.svc?.map((p) => p.piece)).toEqual(["two"]));
    // Everything is asked again, not only the listing: removing a piece also changes what
    // `git status` says about the clone it was cut from, which the bottom bar draws.
    expect(asked.filter((one) => one.cmd === "workspace_repos")).toHaveLength(2);
  });

  it("does not ask again for a counter that has not moved", async () => {
    // A counter in a dependency array is only as good as its stillness: one that changed per
    // render would be a `git worktree list` per clone per keystroke.
    const { asked } = core(ORDINARY);

    const { result, rerender } = renderHook(() => useWorkspaceState(PLANE, "alpha", 3));

    await waitFor(() => expect(result.current.pieces.svc).toHaveLength(2));
    rerender();
    rerender();

    expect(asked.filter((one) => one.cmd === "worktree_list")).toHaveLength(2);
    expect(asked.filter((one) => one.cmd === "workspace_panels")).toHaveLength(1);
  });

  it("asks nothing at all when no workspace is focused", async () => {
    const { asked } = core(ORDINARY);

    const { result } = renderHook(() => useWorkspaceState(PLANE, undefined));

    await waitFor(() => expect(result.current.panels).toBeUndefined());
    expect(asked).toEqual([]);
  });
});
