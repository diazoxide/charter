import { useEffect, useSyncExternalStore } from "react";
import { commands, type PlaneId } from "./bindings";

/**
 * **Which extensions are on in each project this window holds** (charter-app#253, ADR 0048).
 *
 * The window surveys this machine's extensions once — panels, views and themes — because a
 * survey re-hashes every installed extension's directory. What a project has on is a cheaper
 * question (`extensions_on`: the record and the project's two files, no directory read), and the
 * answer is the core's `extension::project::resolve`, the one function every consumer takes. So
 * each project asks it once, keeps it here, and the window keeps of its survey only what the
 * project in front has on.
 *
 * **Until a project has answered, nothing an extension contributes is drawn for it.** An
 * extension turned off in a project must not flash on while the question is out; charter's own
 * panels and views never wait on this.
 */

const known = new Map<PlaneId, ReadonlySet<string>>();
/** The newest question out per project: an answer to an older one is dropped, so a save made
 *  while the first question was out is never overwritten by what the file said before it. */
const latest = new Map<PlaneId, number>();
const listeners = new Set<() => void>();

function tell() {
  for (const listener of listeners) listener();
}

function ask(plane: PlaneId) {
  const mine = (latest.get(plane) ?? 0) + 1;
  latest.set(plane, mine);
  void commands
    .extensionsOn(plane)
    .then((said) => (said.status === "ok" ? (said.data ?? []) : []))
    .catch((): string[] => [])
    .then((ids) => {
      if (latest.get(plane) !== mine) return;
      known.set(plane, new Set(ids));
      tell();
    });
}

/** Ask `plane` again — after its settings were saved, or an extension was approved. */
export function extensionsChanged(plane?: PlaneId) {
  for (const one of plane === undefined ? [...latest.keys()] : [plane]) ask(one);
}

/** What `plane` has on, or `undefined` until it has said. */
export function extensionsOnIn(plane: PlaneId | undefined): ReadonlySet<string> | undefined {
  return plane === undefined ? undefined : known.get(plane);
}

/** {@link extensionsOnIn}, for a component that redraws when it changes, asking once. */
export function useExtensionsOn(plane: PlaneId | undefined): ReadonlySet<string> | undefined {
  useEffect(() => {
    if (plane !== undefined && !latest.has(plane)) ask(plane);
  }, [plane]);
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => extensionsOnIn(plane),
  );
}

/** For tests: forget every answer. */
export function forgetExtensionsOn() {
  known.clear();
  latest.clear();
}
