/**
 * **A branch moved** (#1189): the window's `branch-changed`, heard for one branch only. The
 * light editor's tabs read again on it: the comparison tab its comparison, the file tabs
 * whether the branch changed the file they show.
 *
 * Nothing here asks the core to watch a branch (`branch_watch` sets the window's whole watched
 * set, which is the explorer's): a tab hears a branch the explorer watches.
 */
import { useEffect, useRef } from "react";
import type { BranchChanged, PlaneId } from "../bindings";
import { listen } from "../here";
import type { Place } from "../pieceViews";

/** Calls `heard` each time the window hears that `cut`'s branch moved. */
export function useBranchMoved(plane: PlaneId, cut: Place, heard: () => void) {
  const latest = useRef(heard);
  useEffect(() => {
    latest.current = heard;
  });
  const { workspace, repo, piece } = cut;
  useEffect(() => {
    let gone = false;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unlisten = await listen<BranchChanged>("branch-changed", (event) => {
          if (gone) return;
          const mine = event.payload.branches.some(
            (one) =>
              one.plane === plane &&
              one.workspace === workspace &&
              one.repo === repo &&
              one.piece === piece,
          );
          if (mine) latest.current();
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in: a unit test.
      }
    })();
    return () => {
      gone = true;
      stop?.();
    };
  }, [plane, workspace, repo, piece]);
}
