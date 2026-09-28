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
 * **SI-9c opens these too, and needs nothing else from here**: a workspace's Memory section and
 * the shared list carry `memory.open:<key>` on their rows (the core spells the key,
 * `memories::view_key`), and a `+` opens {@link draftView} with `doing.newMemory`.
 */

/** The view a memory is opened as. */
export const MEMORY_VIEW = "memory";

/** The slug of a memory not written yet — a new memory's tab (ADR 0065 Q9). No slug the core
 *  mints can be it: a slug starts with a letter or a digit. */
export const DRAFT = "+";

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
