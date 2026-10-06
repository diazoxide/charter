import { useEffect, useRef, useState } from "react";
import {
  commands,
  type BranchChanged,
  type BranchStatus,
  type ChangeMark,
  type PlaneId,
} from "./bindings";
import { listen } from "./here";

/**
 * **What each branch the explorer has open changed** (FM-4, #1107): the markers on its files
 * and folders, and what "Changed only" collapses its tree to.
 *
 * A branch is named, never given as a directory: a repo and a piece, or no piece for the repo's
 * own folder. What it changed is `purlis_core::files::status`'s answer through `branch_status`:
 * against the branch it was cut from, committed or not, each folder rolling up what it holds.
 *
 * **Live, on a watch of the whole branch** (`branchwatch.rs`): the core listens to every branch
 * whose *Files* row is open and sends `branch-changed` when anything in it moves that git does
 * not ignore — an agent's first write into a folder nobody opened included. The status is then
 * read again: **one read at a time per branch**, and a move during a read asks for exactly one
 * more after it, so an agent writing a hundred files costs two reads, not a hundred.
 */

/** A branch, as the explorer names it. */
export type BranchRef = { repo: string; piece: string | null };

/** What a branch changed, or the sentence the core refused it with. Absent while it is read. */
export type StatusRead = { status?: BranchStatus; trouble?: string };

/** A branch's key within one workspace: a repo and a piece hold no `/`. */
export function branchKey(ref: BranchRef): string {
  return `${ref.repo}/${ref.piece ?? ""}`;
}

/** What one folder or file of a branch carries: its mark, and for a folder how many it holds. */
export type Marked = { mark: ChangeMark; count?: number; from?: string | null };

/** One name directly under a folder that leads to a change: a changed file, or a folder
 *  holding one. */
export type Seed = { name: string; folder: boolean; mark: ChangeMark };

/** A status, indexed once: each marked path's mark, and what each folder holds that leads to a
 *  change, folders first, in the order a person reads names. */
export type Indexed = {
  status: BranchStatus;
  marks: ReadonlyMap<string, Marked>;
  under: ReadonlyMap<string, readonly Seed[]>;
};

/** {@link Indexed} for a status: one pass over its changes. */
export function indexed(status: BranchStatus): Indexed {
  const marks = new Map<string, Marked>();
  for (const one of status.folders) marks.set(one.folder, { mark: one.mark, count: one.count });
  for (const one of status.changes) marks.set(one.path, { mark: one.mark, from: one.from });
  const under = new Map<string, Map<string, Seed>>();
  const add = (folder: string, seed: Seed) => {
    let here = under.get(folder);
    if (here === undefined) {
      here = new Map();
      under.set(folder, here);
    }
    const id = `${seed.folder ? "d" : "f"}${seed.name}`;
    if (!here.has(id)) here.set(id, seed);
  };
  for (const change of status.changes) {
    const steps = change.path.split("/");
    for (let at = 0; at < steps.length; at++) {
      const folder = steps.slice(0, at).join("/");
      const path = steps.slice(0, at + 1).join("/");
      const isFolder = at < steps.length - 1;
      add(folder, {
        name: steps[at],
        folder: isFolder,
        mark: isFolder ? (marks.get(path)?.mark ?? change.mark) : change.mark,
      });
    }
  }
  const sorted = new Map<string, Seed[]>();
  for (const [folder, seeds] of under) {
    sorted.set(
      folder,
      [...seeds.values()].sort(
        (a, b) =>
          Number(!a.folder) - Number(!b.folder) ||
          a.name.localeCompare(b.name, "en", { numeric: true }),
      ),
    );
  }
  return { status, marks, under: sorted };
}

/**
 * What each of `open` changed, by {@link branchKey}: read when a branch first appears in
 * `open`, and again whenever the core says something in it moved.
 *
 * @param open The branches whose *Files* row is open, in this workspace.
 */
export function useBranchStatus(
  plane: PlaneId | undefined,
  workspace: string | undefined,
  open: readonly BranchRef[],
): ReadonlyMap<string, StatusRead> {
  const [held, setHeld] = useState<{ workspace?: string; reads: Map<string, StatusRead> }>({
    reads: new Map(),
  });
  const asked = useRef<{ workspace?: string; keys: Set<string> }>({ keys: new Set() });
  const byKey = useRef(new Map<string, BranchRef>());
  const keys = open.map(branchKey).join("\0");
  const focused = useRef(workspace);
  /** Each branch's read: whether one is running, and whether another is wanted after it. */
  const flight = useRef(new Map<string, { again: boolean }>());
  const read = useRef<(ref: BranchRef) => void>(() => {});
  useEffect(() => {
    focused.current = workspace;
    read.current = (ref) => {
      if (plane === undefined || workspace === undefined) return;
      const key = `${workspace}\0${branchKey(ref)}`;
      const running = flight.current.get(key);
      if (running !== undefined) {
        running.again = true;
        return;
      }
      const now = { again: false };
      flight.current.set(key, now);
      const told = (got: StatusRead) => {
        flight.current.delete(key);
        if (focused.current === workspace) {
          setHeld((was) => {
            const reads = new Map(was.workspace === workspace ? was.reads : []);
            reads.set(branchKey(ref), got);
            return { workspace, reads };
          });
        }
        if (now.again) read.current(ref);
      };
      void commands
        .branchStatus(plane, workspace, ref.repo, ref.piece)
        .then((said) =>
          told(said.status === "error" ? { trouble: said.error } : { status: said.data }),
        )
        .catch((err: unknown) => told({ trouble: String(err) }));
    };
  });

  /** Whether the core listens to anything for this window, so a window that never opened a
   *  branch's files never asks it to listen to nothing. */
  const listening = useRef(false);
  useEffect(() => {
    const refs = keys === "" ? [] : keys.split("\0").map(unkey);
    const wanted =
      plane === undefined || workspace === undefined
        ? []
        : refs.map((ref) => ({ plane, workspace, ...ref }));
    if (wanted.length > 0 || listening.current) {
      listening.current = wanted.length > 0;
      void commands.branchWatch(wanted).catch(() => undefined);
    }
    if (plane === undefined || workspace === undefined) return;
    if (asked.current.workspace !== workspace) asked.current = { workspace, keys: new Set() };
    byKey.current = new Map(refs.map((ref) => [branchKey(ref), ref]));
    for (const ref of refs) {
      if (!asked.current.keys.has(branchKey(ref))) read.current(ref);
    }
    asked.current.keys = new Set(refs.map(branchKey));
  }, [keys, plane, workspace]);

  // Nothing listened to once the explorer is gone.
  useEffect(
    () => () => {
      if (listening.current) void commands.branchWatch([]).catch(() => undefined);
    },
    [],
  );

  // Something moved in an open branch: what it changed is read again.
  useEffect(() => {
    if (plane === undefined) return;
    let gone = false;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unlisten = await listen<BranchChanged>("branch-changed", (event) => {
          if (gone) return;
          for (const one of event.payload.branches) {
            if (one.plane !== plane || one.workspace !== focused.current) continue;
            const ref = byKey.current.get(branchKey(one));
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

const EMPTY: ReadonlyMap<string, StatusRead> = new Map();

function unkey(key: string): BranchRef {
  const slash = key.indexOf("/");
  const piece = key.slice(slash + 1);
  return { repo: key.slice(0, slash), piece: piece === "" ? null : piece };
}
