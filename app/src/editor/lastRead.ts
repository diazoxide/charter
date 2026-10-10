/**
 * **The line last read in each file's preview** (#1143, D-1143-2): what a tree row's *Open in
 * your editor* opens a file at, so the editor lands where the person was reading.
 *
 * The line is the one the preview's own *Open in your editor at line N* sends: where the cursor
 * is, or the line a jump brought into view. It is kept per file of a branch — project, workspace,
 * repo, piece and path — **in this window's memory only**: never on disk and never in a record,
 * so a relaunch starts every file at line 1 again, as a file never read does.
 *
 * Bounded, oldest first out: a person reads far fewer files in a sitting than the bound, and a
 * forgotten line only means the editor opens at line 1.
 */
import { useSyncExternalStore } from "react";
import type { Place } from "../pieceViews";

/** How many files' lines are kept at once. */
export const KEPT = 256;

const lines = new Map<string, number>();
const listeners = new Set<() => void>();

function keyOf(plane: string, place: Place, path: string): string {
  return [plane, place.workspace, place.repo, place.piece ?? "", path].join("\u0000");
}

/** The preview of `path` is at `line`. Line 1, or a line that is not one, forgets it. */
export function readAt(plane: string, place: Place, path: string, line: number): void {
  const key = keyOf(plane, place, path);
  const was = lines.get(key);
  const kept = Number.isInteger(line) && line > 1 ? line : undefined;
  if (was === kept) return;
  // Taken out first, so a line read again is the newest and the last to be dropped.
  lines.delete(key);
  if (kept !== undefined) {
    lines.set(key, kept);
    if (lines.size > KEPT) {
      const oldest = lines.keys().next().value;
      if (oldest !== undefined) lines.delete(oldest);
    }
  }
  for (const listener of listeners) listener();
}

/** The line last read in `path`'s preview, or 1 where it was not read. */
export function lastRead(plane: string, place: Place, path: string): number {
  return lines.get(keyOf(plane, place, path)) ?? 1;
}

/** {@link lastRead}, drawn again when it changes: what a tree row's menu offers. */
export function useLastRead(plane: string | undefined, place: Place, path: string): number {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => (plane === undefined ? 1 : lastRead(plane, place, path)),
  );
}

/** Forgets every line. For tests. */
export function forgetLastRead(): void {
  lines.clear();
  for (const listener of listeners) listener();
}
