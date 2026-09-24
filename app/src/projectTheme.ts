import { useEffect, useSyncExternalStore } from "react";
import { commands, type PlaneId } from "./bindings";

/**
 * **The theme each project this window holds draws** (charter-app#273, ADR 0048), kept as
 * `extensionsOn.ts` keeps what each has on, and for the same reason: the answer is the core's
 * (`extension::project::theme::resolve`, asked with `project_theme_drawn` — the record and the
 * project's two files, no extension's directory), asked once per project and again after a save.
 *
 * `null` is an answer: the project picked nothing, and the window keeps its own theme. A question
 * that failed is read as that too — a project whose theme cannot be asked must not hold the
 * window's theme back.
 */

const known = new Map<PlaneId, string | null>();
/** The newest question out per project: an answer to an older one is dropped. */
const latest = new Map<PlaneId, number>();
const listeners = new Set<() => void>();

function ask(plane: PlaneId) {
  const mine = (latest.get(plane) ?? 0) + 1;
  latest.set(plane, mine);
  void commands
    .projectThemeDrawn(plane)
    .then((said) => (said.status === "ok" ? (said.data ?? null) : null))
    .catch(() => null)
    .then((drawn) => {
      if (latest.get(plane) !== mine) return;
      known.set(plane, drawn);
      for (const listener of listeners) listener();
    });
}

/** Ask `plane` again — after its settings were saved, or an extension was approved or removed. */
export function projectThemeChanged(plane?: PlaneId) {
  for (const one of plane === undefined ? [...latest.keys()] : [plane]) ask(one);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** What `plane` draws, as its file holds it; `null` when it picked nothing; `undefined` until it
 *  has said. Redraws when it changes, asking once. */
export function useProjectTheme(plane: PlaneId | undefined): string | null | undefined {
  useEffect(() => {
    if (plane !== undefined && !latest.has(plane)) ask(plane);
  }, [plane]);
  return useSyncExternalStore(subscribe, () =>
    plane === undefined ? undefined : known.get(plane),
  );
}

/** For tests: forget every answer. */
export function forgetProjectThemes() {
  known.clear();
  latest.clear();
}
