import { useSyncExternalStore } from "react";
import { atCreation, sayAboutThisMachine } from "./windowprefs";

/**
 * **One preference kept in the layout file** (#1686): how one person likes their window on
 * this machine, beside the arrangement (`layout.json`, `regions.ts`, the file's one writer).
 * Your editor, how the Chats list is drawn and Explorer's folded sections are each one of these.
 *
 * Every such preference is the same store: read from the file the window was handed, once, the
 * first time it is asked for; what had to be put right to read it said in the alerts drawer
 * under its own subject, with the file's path and the way to fix it, until the person changes
 * it; a change told to its listeners, which is what writes the file; and forgotten, for tests,
 * as a new launch would. A file purlis refused is said once, by `regions.ts`, and is the
 * default here.
 *
 * `regions.ts` takes each one's {@link LayoutPref.written} into the document, listens to it and
 * forgets it, from one list: a new preference is one more row there.
 */
export type LayoutPref<T, K extends string = string> = {
  /** Its key in the layout document, which is also the alert's subject. */
  readonly key: K;
  /** What it is now. */
  value(): T;
  /** Changes it; tells every listener, which writes the file, when it changed. */
  set(to: T): void;
  /** Changes it without telling the listeners: what draws it is drawn again, and nothing is
   *  written. For a value another writer carries, or one the file was just moved aside for. */
  keep(to: T): void;
  /** Calls `listener` whenever it is {@link set} to something else. Answers the way to stop. */
  on(listener: (value: T) => void): () => void;
  /** {@link value}, for a component that redraws when it changes. */
  use(): T;
  /** What the layout document holds of it: nothing where it is the default, which the file
   *  leaves out. */
  written(): unknown;
  /** Forgets what this launch chose and read, as a new launch would. For tests. */
  forget(): void;
};

export type LayoutPrefSpec<T, K extends string = string> = {
  key: K;
  /** What it is where the file says nothing of it, or purlis refused the file. */
  fallback: T;
  /** It as a layout document holds it, and what had to be put right to read it. */
  load: (document: unknown) => { value: T; said: string[] };
  /** Whether two values are the same, so a set to what it is tells nobody. */
  same: (one: T, other: T) => boolean;
  /** What the document holds of `value`, or nothing where it is the default. */
  written: (value: T) => unknown;
  /** How to put the file right, given where it is: the alert's remedy. */
  remedy: (where: string) => string;
  /** Where in Settings it is changed, for the alert's link. */
  settings?: string;
  /** What else a new launch would not know, forgotten with it: a store that keeps more than
   *  the one value. */
  forgotten?: () => void;
};

export function layoutPref<T, K extends string = string>(
  spec: LayoutPrefSpec<T, K>,
): LayoutPref<T, K> {
  /** What this launch changed it to, if anything. */
  let changed: { value: T } | undefined;
  /** What the launch started from, read once. */
  let started: { value: T } | undefined;
  const listeners = new Set<(value: T) => void>();
  const drawers = new Set<() => void>();

  const starting = (): T => {
    if (started !== undefined) return started.value;
    const layout = atCreation().layout;
    const { value, said } =
      layout.found && layout.trouble === null
        ? spec.load(layout.document)
        : { value: spec.fallback, said: [] };
    if (said.length > 0) {
      const where = layout.path || "the layout file";
      sayAboutThisMachine(spec.key, {
        severity: "warn",
        detail: `${where}: ${said.join("; ")}`,
        remedy: spec.remedy(where),
        ...(spec.settings !== undefined ? { settings: spec.settings } : {}),
      });
    }
    started = { value };
    return value;
  };

  const value = (): T => (changed !== undefined ? changed.value : starting());

  const keep = (to: T) => {
    changed = { value: to };
    sayAboutThisMachine(spec.key, undefined);
    for (const draw of drawers) draw();
  };

  const subscribe = (draw: () => void) => {
    drawers.add(draw);
    return () => void drawers.delete(draw);
  };

  return {
    key: spec.key,
    value,
    set(to) {
      if (spec.same(value(), to)) return;
      keep(to);
      for (const listener of listeners) listener(to);
    },
    keep,
    on(listener) {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
    use: () => useSyncExternalStore(subscribe, value),
    written: () => spec.written(value()),
    forget() {
      changed = undefined;
      started = undefined;
      spec.forgotten?.();
    },
  };
}
