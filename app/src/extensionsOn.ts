import { useEffect, useSyncExternalStore } from "react";
import { commands, type PlaneId } from "./bindings";

/**
 * **Which extensions are on in each project this window holds** (charter-app#253, ADR 0048) —
 * and in each of its workspaces (charter-app#280).
 *
 * The window surveys this machine's extensions once — panels, views and themes — because a
 * survey re-hashes every installed extension's directory. What a project has on is a cheaper
 * question (`extensions_on`: the record and the project's two files, no directory read), and the
 * answer is the core's `extension::project::resolve`, the one function every consumer takes. A
 * workspace's `workspace.json` is a layer of that answer, between the project's Shared and Local
 * files, so the question is asked per project AND workspace: the focused workspace's answer is
 * what its panels and views are filtered by. `undefined` asks for the project with no workspace.
 *
 * **Until one has answered, nothing an extension contributes is drawn for it.** An extension
 * turned off must not flash on while the question is out; charter's own panels and views never
 * wait on this.
 */

type Where = { plane: PlaneId; workspace: string | undefined };

const whereOf = new Map<string, Where>();
const known = new Map<string, ReadonlySet<string>>();
/** The newest question out per project and workspace: an answer to an older one is dropped, so
 *  a save made while the first question was out is never overwritten by what the file said
 *  before it. */
const latest = new Map<string, number>();
const listeners = new Set<() => void>();

function keyOf(plane: PlaneId, workspace: string | undefined): string {
  return `${plane}\u0000${workspace ?? ""}`;
}

function tell() {
  for (const listener of listeners) listener();
}

function ask(plane: PlaneId, workspace: string | undefined) {
  const key = keyOf(plane, workspace);
  whereOf.set(key, { plane, workspace });
  const mine = (latest.get(key) ?? 0) + 1;
  latest.set(key, mine);
  void commands
    .extensionsOn(plane, workspace ?? null)
    .then((said) => (said.status === "ok" ? (said.data ?? []) : undefined))
    .catch(() => undefined)
    .then((ids) => {
      if (latest.get(key) !== mine) return;
      // A question that failed is not an answer: nothing is kept, so the next component to ask
      // asks again rather than a project going without its extensions until something is saved.
      if (ids === undefined) {
        latest.delete(key);
        return;
      }
      known.set(key, new Set(ids));
      tell();
    });
}

/** Ask `plane` again, for every workspace it was asked about — after its settings or a
 *  workspace's were saved, or an extension was approved. With no plane, every one. */
export function extensionsChanged(plane?: PlaneId) {
  // Every place ever asked about, including one whose last question failed: a save is exactly
  // when a failed answer should be asked for again.
  let asked = false;
  for (const where of [...whereOf.values()]) {
    if (plane === undefined || where.plane === plane) {
      ask(where.plane, where.workspace);
      asked = true;
    }
  }
  if (!asked && plane !== undefined) ask(plane, undefined);
}

/** What `plane` has on in `workspace`, or `undefined` until it has said. */
export function extensionsOnIn(
  plane: PlaneId | undefined,
  workspace?: string,
): ReadonlySet<string> | undefined {
  return plane === undefined ? undefined : known.get(keyOf(plane, workspace));
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** {@link extensionsOnIn}, for a component that redraws when it changes, asking once. */
export function useExtensionsOn(
  plane: PlaneId | undefined,
  workspace?: string,
): ReadonlySet<string> | undefined {
  useEffect(() => {
    if (plane !== undefined && !latest.has(keyOf(plane, workspace))) ask(plane, workspace);
  }, [plane, workspace]);
  return useSyncExternalStore(subscribe, () => extensionsOnIn(plane, workspace));
}

/** For tests: forget every answer. */
export function forgetExtensionsOn() {
  known.clear();
  latest.clear();
  whereOf.clear();
}
