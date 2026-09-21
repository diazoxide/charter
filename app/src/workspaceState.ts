import { useEffect, useState } from "react";
import {
  commands,
  type Panels as PanelsModel,
  type Piece,
  type PlaneId,
  type RepoStates,
} from "./bindings";

/**
 * Everything three regions need to know about ONE workspace, asked once.
 *
 * **Asked here and not in each region, because three of them now want the same answer.**
 * Until ADR 0038 the right-hand `Panels` asked for itself, and its own comment argued that
 * the asks did not belong further up. That was right while it was the only region reading a
 * workspace. It is wrong now: the explorer on the left, the state bar at the bottom and the
 * right-hand side all draw the same workspace, and three copies of this hook would be
 * `workspace_panels` three times, `workspace_repos` three times — and `workspace_repos` runs
 * `git status` once per clone, bounded at five seconds each.
 *
 * **What it costs, counted rather than assumed** (the standard #133 set). Per focused
 * workspace, not per render and not per keystroke:
 *
 * - one `workspace_panels` — a directory listing and a few small files;
 * - one `workspace_repos` — `git status` per clone, on a blocking thread in the core;
 * - one `worktree_list` **per clone** — `git worktree list --porcelain` per clone, which is
 *   the ask this record adds. It is the explorer's whole content, so it cannot be avoided;
 *   what it can be is bounded, and it is: the effect that makes these calls depends on the
 *   clone NAMES, which keep their identity for as long as the plane answers the same way, so
 *   focusing a workspace asks once and re-rendering asks not at all. That bound is pinned by
 *   a test rather than described — `workspaceState.test.ts`, "asks git for the pieces once
 *   per clone".
 *
 * A late answer for a workspace that is no longer focused is dropped three times over — by
 * the cleanup, by the name the answer itself carries, and by the state being keyed on the
 * workspace, so nothing left over from the last one is ever drawn under this one's heading.
 * Nothing is reset when the focus changes, because clearing state from inside an effect is a
 * render the window does not need; the stale answer is simply not this workspace's.
 */
export type WorkspaceState = {
  /** What the plane says, without git. */
  panels?: PanelsModel;
  /** What git says about each clone, and what the forge cache holds for it. */
  repos?: RepoStates;
  /** This workspace's pieces, by clone name. A clone whose listing has not come back is
   *  absent from the map, which is what `readingPieces` is for. */
  pieces: Record<string, Piece[]>;
  /** Why a clone's pieces could not be listed, by clone name. Shown, never dropped: a repo
   *  with no worktree rows would otherwise read as a repo nobody has cut one in. */
  piecesRefused: Record<string, string>;
  /** Whatever refused the workspace outright, in the core's own words. */
  trouble?: string;
  /** Whether the answer that runs git is still on its way. */
  reading: boolean;
};

/** Everything the core has said about ONE workspace. The name is part of the state, not
 *  beside it: that is what makes an answer for another workspace recognisable. */
type Answer = {
  workspace: string;
  panels?: PanelsModel;
  repos?: RepoStates;
  pieces: Record<string, Piece[]>;
  piecesRefused: Record<string, string>;
  trouble?: string;
  /** Whether the repos ask has come back, however it came back. */
  read?: boolean;
};

const NOTHING_YET = { pieces: {}, piecesRefused: {} };

export function useWorkspaceState(plane: PlaneId, workspace: string | undefined): WorkspaceState {
  const [answer, setAnswer] = useState<Answer>();

  useEffect(() => {
    if (workspace === undefined) return;
    let gone = false;
    // Merged onto what is already known about THIS workspace, and onto nothing when what is
    // held belongs to another one.
    const told = (what: Partial<Answer>) => {
      setAnswer((was) => ({
        ...(was?.workspace === workspace ? was : NOTHING_YET),
        workspace,
        ...what,
      }));
    };

    void commands
      .workspacePanels(plane, workspace)
      .then((said) => {
        if (gone) return;
        if (said.status === "error") {
          told({ trouble: said.error });
        } else if (said.data?.workspace === workspace) {
          // An `ok` answer with no body reads as `ok` all the same, and the window must not
          // throw inside a promise nothing is holding. It simply has nothing to draw.
          told({ panels: said.data });
        }
      })
      .catch((err: unknown) => {
        if (!gone) told({ trouble: String(err) });
      });

    void commands
      .workspaceRepos(plane, workspace)
      .then((said) => {
        if (gone) return;
        if (said.status === "error") {
          told({ read: true, trouble: said.error });
        } else if (said.data?.workspace === workspace) {
          told({ read: true, repos: said.data });
        }
      })
      .catch((err: unknown) => {
        if (!gone) told({ read: true, trouble: String(err) });
      });

    return () => {
      gone = true;
    };
  }, [plane, workspace]);

  const mine = answer?.workspace === workspace ? answer : undefined;
  /**
   * The clones to list pieces for.
   *
   * **The array off the plane's own answer, not a copy of it.** It is this effect's
   * dependency, and a fresh array per render would make the effect run per render — a git
   * subprocess per clone per keystroke. `panels` is one object held in state until the plane
   * is read again, so `panels.repos` keeps its identity across every render in between, and
   * the pieces are asked for once per focused workspace.
   */
  const clones = mine?.panels?.repos;

  useEffect(() => {
    if (workspace === undefined || clones === undefined) return;
    let gone = false;
    for (const repo of clones) {
      void commands
        .worktreeList(plane, workspace, repo)
        .then((said) => {
          if (gone) return;
          setAnswer((was) => {
            // Not this workspace's any more: dropped rather than merged onto whatever is
            // focused now, which is a different workspace's pieces under its name.
            if (was?.workspace !== workspace) return was;
            if (said.status === "error") {
              return { ...was, piecesRefused: { ...was.piecesRefused, [repo]: said.error } };
            }
            return { ...was, pieces: { ...was.pieces, [repo]: said.data ?? [] } };
          });
        })
        .catch((err: unknown) => {
          if (gone) return;
          setAnswer((was) =>
            was?.workspace === workspace
              ? { ...was, piecesRefused: { ...was.piecesRefused, [repo]: String(err) } }
              : was,
          );
        });
    }
    return () => {
      gone = true;
    };
  }, [clones, plane, workspace]);

  return {
    panels: mine?.panels,
    repos: mine?.repos,
    pieces: mine?.pieces ?? NOTHING_YET.pieces,
    piecesRefused: mine?.piecesRefused ?? NOTHING_YET.piecesRefused,
    trouble: mine?.trouble,
    // Reading until the answer that runs git has come back for THIS workspace.
    reading: workspace !== undefined && !mine?.read,
  };
}
