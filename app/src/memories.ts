import { useSyncExternalStore } from "react";
import type { MemoryScope, MemoryView } from "./bindings";
import type { ViewRef } from "./tabs";

/**
 * **A memory in the window** (SI-9b, ADR 0065): one memory is one view tab —
 * `{ from: null, view: "memory", key: <its store and slug> }` — which reads it, renders it as
 * Markdown, edits it in place and archives it (`MemoryTab.tsx`).
 *
 * What opens one is a catalogue row, `memory.open:<key>`, so a memory row, the palette and a
 * row's menu are one verb (`actions.memoryOffers`). A single click previews it in the strip's
 * preview tab (`tabs.openPreview`); a double-click, or starting an edit, keeps it.
 *
 * **SI-9c opens these too**: a workspace's Memory section and the shared list carry
 * `memory.open:<key>` on their rows (the core spells the key, `memories::view_key`), and each
 * list's `+` is the catalogue row `memory.new:<`{@link scopeKey}`>`, which opens
 * {@link draftView} with `doing.newMemory`.
 */

/** The view a memory is opened as. */
export const MEMORY_VIEW = "memory";

/** The slug of a memory not written yet — a new memory's tab (ADR 0065 Q9). **A slug the core
 *  refuses** (`memstore::slug_ok`: `\\` is never part of one), so no memory file can be it — `+`,
 *  which it was, is a filename, and a hand-made `+.md` opened as a new memory's editor (SI-9d).
 *  The core spells the same thing (`memories::DRAFT`). */
export const DRAFT = "\\";

/** The longest title a memory takes (`memstore::TITLE_MAX`). */
export const TITLE_MAX = 72;

/** One memory: the store it is in and its slug ({@link DRAFT} for one not written yet). */
export type MemoryRef = { scope: MemoryScope; slug: string };

/**
 * What a memory's tab is keyed by: `workspace/<ws>/<slug>`, `persona/<name>/<slug>` or
 * `shared/<slug>`. **The core spells the same thing** (`memories::view_key`), for the rows it
 * sends, and both are tested against the same literals.
 */
export function memoryKey({ scope, slug }: MemoryRef): string {
  switch (scope.kind) {
    case "workspace":
      return `workspace/${scope.name}/${slug}`;
    case "persona":
      return `persona/${scope.name}/${slug}`;
    case "shared":
      return `shared/${slug}`;
  }
}

/**
 * What a store is named by in a catalogue row: `workspace/<ws>`, `persona/<name>` or `shared` —
 * a memory's key without its slug. `memory.new:<this>` makes one there (SI-9c, ADR 0065 Q9).
 */
export function scopeKey(scope: MemoryScope): string {
  return scope.kind === "shared" ? "shared" : `${scope.kind}/${scope.name}`;
}

/** The view the Personas panel's "shared" row opens: the shared store's own list (ADR 0065
 *  Q6), charter's built-in `shared-memory` view. A plane has one shared store, so no key. */
export const SHARED_MEMORY_VIEW: ViewRef = { from: null, view: "shared-memory", key: "" };

/** What the shared list's tab is called. */
export const SHARED_MEMORY_TITLE = "Shared memory";

/** The memory a key names, or `undefined` for one that names none. */
export function memoryRefOf(key: string): MemoryRef | undefined {
  const parts = key.split("/");
  if (parts[0] === "shared" && parts.length === 2 && parts[1] !== "") {
    return { scope: { kind: "shared" }, slug: parts[1] };
  }
  if ((parts[0] === "workspace" || parts[0] === "persona") && parts.length === 3) {
    const [kind, name, slug] = parts;
    if (name === "" || slug === "") return undefined;
    return { scope: { kind: kind as "workspace" | "persona", name }, slug };
  }
  return undefined;
}

/** The view one memory is opened as. */
export function memoryView(ref: MemoryRef): ViewRef {
  return { from: null, view: MEMORY_VIEW, key: memoryKey(ref) };
}

/** A new memory's tab, in edit mode from the start, for the store `scope` (ADR 0065 Q9). One
 *  per store: a second `+` brings the first draft forward. */
export function draftView(scope: MemoryScope): ViewRef {
  return memoryView({ scope, slug: DRAFT });
}

/** Whether `view` is a memory's tab. */
export function isMemory(view: ViewRef): boolean {
  return view.from === null && view.view === MEMORY_VIEW;
}

/** What a new memory's tab is called until it is saved. */
export const DRAFT_TITLE = "New memory";

/** The badge's word for a store: the workspace's or the persona's name, or `shared`. */
export function scopeWord(scope: MemoryScope): string {
  return scope.kind === "shared" ? "shared" : scope.name;
}

/** A store in a sentence: `shared memory`, or `<name>'s memory`. */
export function memoryOf(scope: MemoryScope): string {
  return scope.kind === "shared" ? "shared memory" : `${scope.name}'s memory`;
}

// ---------------------------------------------------------------------------------------------
// A store's archive (KN-4, D6).

/** The view a store's archive is opened as: one tab per store, keyed by {@link scopeKey}. */
export const ARCHIVE_VIEW = "memory-archive";

/** The tab that browses `scope`'s archive. */
export function archiveView(scope: MemoryScope): ViewRef {
  return { from: null, view: ARCHIVE_VIEW, key: scopeKey(scope) };
}

/** What a store's archive tab is called. */
export function archiveTitle(scope: MemoryScope): string {
  return scope.kind === "shared" ? "Archived shared memory" : `Archived memory · ${scope.name}`;
}

/** A store's archive in a sentence: `the shared archive`, or `<name>'s archive`. */
export function archiveWhere(scope: MemoryScope): string {
  return scope.kind === "shared" ? "the shared archive" : `${scope.name}'s archive`;
}

/** The store a {@link scopeKey} names, or `undefined` for one that names none. */
export function scopeOfKey(key: string): MemoryScope | undefined {
  if (key === "shared") return { kind: "shared" };
  const parts = key.split("/");
  if ((parts[0] === "workspace" || parts[0] === "persona") && parts.length === 2 && parts[1] !== "")
    return { kind: parts[0], name: parts[1] };
  return undefined;
}

/** The store whose archive `view` browses, when it is an archive's tab. */
export function archiveOf(view: ViewRef): MemoryScope | undefined {
  return view.from === null && view.view === ARCHIVE_VIEW ? scopeOfKey(view.key) : undefined;
}

/**
 * **The name a numbered archive restores under** (#1191, D-1191-1): the one archiving had to
 * number it away from, or `undefined` when it was never numbered.
 *
 * `archive_one` (memstore.rs) moves a memory into `archive/` under its own name, and adds `-2`
 * when `archive/` holds that name already, numbering from the name it tried last: the third is
 * `-2-3`. This is the exact inverse of that, so a `-<n>` the core never writes is left alone:
 * `release-2026` and `step-3` are names, not numbers. And it is offered only while `held` (the
 * archive's other names) still holds the name it was numbered away from, which is what tells a
 * numbered `freeze-2` from a memory its writer called `step-2`.
 *
 * Even then it is only likely, never known: a store can hold a `freeze-2` of its own (two
 * memories with one title), and the archive keeps no record of the name a file had. So the tab
 * offers this name beside the archived one and never picks it (`MemoryArchiveTab`); the
 * core refuses a name the store holds and moves nothing.
 */
export function unnumbered(archived: string, held: readonly string[]): string | undefined {
  const stem = (name: string) => name.replace(/\.md$/, "");
  let base = stem(archived);
  let next: number | undefined;
  for (;;) {
    const numbered = /^(.+)-([1-9]\d*)$/.exec(base);
    if (numbered === null) return undefined;
    const n = Number(numbered[2]);
    if (n < 2 || (next !== undefined && n !== next)) return undefined;
    base = numbered[1];
    if (n === 2) break;
    next = n - 1;
  }
  return held.some((name) => stem(name) === base) ? base : undefined;
}

// ---------------------------------------------------------------------------------------------
// An edit in progress.

/**
 * **An edit in progress, kept outside the tab** — the title and body typed so far, and the
 * file's text as it was read, which the save is checked against.
 *
 * Outside, because only the tab in front has panes on screen (`tabs.ts`): a draft held in the
 * tab's own state would be thrown away by looking at another tab. It lives as long as the
 * window does, and a relaunch starts every memory tab reading again.
 *
 * `"wanted"` is an edit asked for before the tab has read the memory — the row's Edit, which
 * opens the tab and asks in the same press. The tab fills it in when its read arrives.
 */
export type Draft = {
  title: string;
  body: string;
  /** The file's whole text when the edit began: `MemoryView.text`, or `""` for a new memory. */
  base: string;
  /** Set by a save the core refused as stale: what is on disk now, `null` when nothing is. */
  stale?: { now: MemoryView | null };
};

const drafts = new Map<string, Draft | "wanted">();
const listeners = new Set<() => void>();

function draftId(plane: string, key: string): string {
  return `${plane}\u0000${key}`;
}

function told() {
  for (const listener of listeners) listener();
}

/** Asks for the memory `key`'s tab to be in edit mode, whether or not it has read it yet. */
export function wantEdit(plane: string, key: string): void {
  const id = draftId(plane, key);
  if (drafts.has(id)) return;
  drafts.set(id, "wanted");
  told();
}

/** Sets, or with `undefined` ends, the edit of `key`. */
export function setDraft(plane: string, key: string, draft: Draft | undefined): void {
  const id = draftId(plane, key);
  if (draft === undefined) drafts.delete(id);
  else drafts.set(id, draft);
  told();
}

/** The edit of `key` in progress, if any — a {@link useDraft} without React. */
export function draftOf(plane: string, key: string): Draft | "wanted" | undefined {
  return drafts.get(draftId(plane, key));
}

/** The edit of `key` in progress, re-read whenever any edit changes. */
export function useDraft(plane: string, key: string): Draft | "wanted" | undefined {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => drafts.get(draftId(plane, key)),
  );
}

/** Forgets every edit in progress. For tests. */
export function forgetDrafts(): void {
  drafts.clear();
  told();
}
