import { useEffect, useRef, useState } from "react";
import {
  commands,
  type BranchFolder,
  type FilesChanged,
  type FolderEntry,
  type PlaneId,
} from "./bindings";
import { listen } from "./here";

/**
 * **The folders of branches the explorer has expanded, read one level at a time** (FM-1,
 * #1103).
 *
 * A branch is named, never given as a directory: a workspace, a repo and a piece, or no piece
 * for the repo's own folder (#948). A folder is its path inside the branch, `""` for its top.
 * What each holds is `charter_core::files::tree`'s answer, through `branch_tree`.
 *
 * **What it costs.** One `branch_tree` per folder when it is expanded, and nothing for a
 * folder that is not: a branch of a hundred thousand files costs what its open folders hold.
 * Each expanded folder is watched, non-recursively (`files_watch`), and read again when the
 * core says it moved — an agent adding or removing a file — so the tree never needs a manual
 * refresh. A folder closed and opened again is read again, because it was not watched while
 * closed.
 */

/** One folder of a branch, as the explorer names it. */
export type BranchFolderRef = {
  repo: string;
  /** The piece, or `null` for the repo's own folder. */
  piece: string | null;
  /** Its path inside the branch, `""` for the branch's top. */
  folder: string;
};

/** What a folder holds — its first entries and how many more — or the sentence the core
 *  refused it with. Absent while it is read. */
export type FolderRead = { entries?: FolderEntry[]; more?: number; trouble?: string };

/** A folder's key within one workspace. A repo and a piece cannot hold a `/` or a `:`
 *  (`worktree::path_for` refuses both), so the key splits back exactly. */
export function folderKey(ref: BranchFolderRef): string {
  return `${ref.repo}/${ref.piece ?? ""}:${ref.folder}`;
}

/**
 * What each of `open` holds, by {@link folderKey}: read when a folder first appears in `open`,
 * and again whenever the core says it moved.
 *
 * @param open The folders the explorer has expanded and draws, in this workspace.
 */
export function useBranchFolders(
  plane: PlaneId | undefined,
  workspace: string | undefined,
  open: readonly BranchFolderRef[],
): ReadonlyMap<string, FolderRead> {
  const [held, setHeld] = useState<{ workspace?: string; reads: Map<string, FolderRead> }>({
    reads: new Map(),
  });
  /** The keys read already, so a re-render asks for nothing and a newly opened one is read. */
  const asked = useRef<{ workspace?: string; keys: Set<string> }>({ keys: new Set() });
  /** The folders open now, by key: what a `files-changed` is matched against. */
  const byKey = useRef(new Map<string, BranchFolderRef>());
  // Joined with NUL, which no name can hold: a folder an agent named with a line break stays
  // one folder.
  const keys = open.map(folderKey).join("\0");

  /** The workspace focused now: an answer for another one is dropped, not merged. */
  const focused = useRef(workspace);
  const read = useRef<(ref: BranchFolderRef) => void>(() => {});
  useEffect(() => {
    focused.current = workspace;
    read.current = (ref) => {
      if (plane === undefined || workspace === undefined) return;
      const key = folderKey(ref);
      const told = (got: FolderRead) => {
        if (focused.current !== workspace) return;
        setHeld((was) => {
          const reads = new Map(was.workspace === workspace ? was.reads : []);
          reads.set(key, got);
          return { workspace, reads };
        });
      };
      void commands
        .branchTree(plane, workspace, ref.repo, ref.piece, ref.folder)
        .then((said) =>
          told(
            said.status === "error"
              ? { trouble: said.error }
              : { entries: said.data.entries, more: said.data.more },
          ),
        )
        .catch((err: unknown) => told({ trouble: String(err) }));
    };
  });

  /** Whether the core is watching anything for this window, so a window that never opened a
   *  folder never asks it to watch nothing. */
  const watching = useRef(false);
  /** What the core is told to watch: the whole open set, which replaces what it had. */
  const watch = useRef((folders: BranchFolder[]) => {
    if (folders.length === 0 && !watching.current) return;
    watching.current = folders.length > 0;
    void commands.filesWatch(folders).catch(() => undefined);
  });

  // Each folder newly open is read, and the whole open set is what the core watches.
  useEffect(() => {
    if (plane === undefined || workspace === undefined) {
      watch.current([]);
      return;
    }
    const refs = keys === "" ? [] : keys.split("\0").map(unkey);
    if (asked.current.workspace !== workspace) asked.current = { workspace, keys: new Set() };
    const now = new Set(refs.map(folderKey));
    byKey.current = new Map(refs.map((ref) => [folderKey(ref), ref]));
    for (const ref of refs) {
      if (!asked.current.keys.has(folderKey(ref))) read.current(ref);
    }
    // A folder closed is forgotten, so opening it again reads it again.
    asked.current.keys = now;
    watch.current(refs.map((ref) => ({ plane, workspace, ...ref })));
  }, [keys, plane, workspace]);

  // Nothing watched once the explorer is gone.
  useEffect(() => {
    const unwatch = watch.current;
    return () => unwatch([]);
  }, []);

  // A folder that moved on disk is read again. Listened under the plane alone: focusing another
  // workspace changes what is matched, read when an event arrives, not the listener.
  useEffect(() => {
    if (plane === undefined) return;
    let gone = false;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unlisten = await listen<FilesChanged>("files-changed", (event) => {
          if (gone) return;
          for (const moved of event.payload.folders) {
            if (moved.plane !== plane || moved.workspace !== focused.current) continue;
            const ref = byKey.current.get(folderKey(moved));
            if (ref !== undefined) read.current(ref);
          }
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
  }, [plane]);

  return held.workspace === workspace ? held.reads : EMPTY;
}

const EMPTY: ReadonlyMap<string, FolderRead> = new Map();

/** A key back into the folder it names. */
function unkey(key: string): BranchFolderRef {
  const slash = key.indexOf("/");
  const colon = key.indexOf(":", slash);
  const piece = key.slice(slash + 1, colon);
  return {
    repo: key.slice(0, slash),
    piece: piece === "" ? null : piece,
    folder: key.slice(colon + 1),
  };
}
