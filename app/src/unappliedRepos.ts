import type { PlaneId } from "./bindings";

/**
 * **The ticks Settings › Repos has not applied yet, per project and workspace** (#1192): what
 * to clone and what to remove, against what is cloned. Held here and not in the group, which
 * Settings unmounts when another group is picked, so a switch and back finds them as they were
 * (D-1192-a: keeping them rather than asking before the switch, which would stop the person for
 * a draft they can still see and apply).
 *
 * Held as changes rather than as the ticked set, so a read of the disk settles them: a tick the
 * disk already agrees with (cloned or removed from somewhere else meanwhile) is dropped, and
 * nothing that was not a change is held over a later change on disk. Apply lets go of them all.
 * They live as long as the window (D-1192-b): the Settings tab cannot tell this row when it
 * closes yet.
 */
export type Unapplied = { readonly add: ReadonlySet<string>; readonly remove: ReadonlySet<string> };

const NONE: Unapplied = { add: new Set(), remove: new Set() };
const unapplied = new Map<string, Unapplied>();
const unappliedHeard = new Set<() => void>();

/** The key of a project's workspace in this store. */
export function unappliedKey(plane: PlaneId, workspace: string): string {
  return `${plane}\u0000${workspace}`;
}

/** The workspace's unapplied ticks: none, until one is held. The same object until it changes,
 *  as `useSyncExternalStore` needs. */
export function unappliedOf(key: string): Unapplied {
  return unapplied.get(key) ?? NONE;
}

/** Holds `now` as the workspace's unapplied ticks, and tells every row of it. */
export function holdUnapplied(key: string, now: Unapplied) {
  if (now.add.size === 0 && now.remove.size === 0) unapplied.delete(key);
  else unapplied.set(key, now);
  for (const hear of unappliedHeard) hear();
}

/** Hears every change to any workspace's ticks; returns the way to stop. */
export function hearUnapplied(hear: () => void) {
  unappliedHeard.add(hear);
  return () => void unappliedHeard.delete(hear);
}

/** For tests: forget every unapplied tick. */
export function forgetUnappliedRepos() {
  unapplied.clear();
}
