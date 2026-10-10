import { atCreation, onLayoutMovedAside, type Reading } from "./windowprefs";

/**
 * **What the side views keep for each project, on this machine** (spec #1671 B-11, #1686,
 * #1696): beside each project's arrangement in `layout.json` v2, under `projects[path]`.
 *
 * A **facet** is one view's part of a project's entry: `explorer` (its folded sections and the
 * rows folded or opened in its trees) and `chats` (the Chats view's scope). This module keeps
 * them as values and knows nothing of what they mean: the view that owns a facet reads it,
 * checks it field by field and bounds it (`explorerSections.ts`, `explorerFolds.ts`,
 * `chatsScope.ts`). What it does know is where each one is: what this launch kept, else what
 * the file held for the project, else nothing.
 *
 * `regions.ts`, the one writer of the file, writes every project this launch kept something
 * in ({@link projectsTouched}), with each of its facets, beside its arrangement. The core keeps
 * every other project's entry as the file had it (`purlis_core::windowprefs::write_layout`).
 */

/** The facets, by the field each is written under in a project's entry. */
export type Facet = "explorer" | "chats";

const FACETS: readonly Facet[] = ["explorer", "chats"];

/** What this launch kept, by project, then by facet. `undefined` is "nothing kept", which a
 *  write leaves out. */
const kept = new Map<string, Map<Facet, unknown>>();
/** The projects this launch kept something in, the one kept in last, last. */
const touched = new Map<string, true>();
/** The file was moved aside this launch (Use the default layout): nothing it held is read. */
let movedAside = false;
const listeners = new Set<(project: string) => void>();

/** What the file holds for `project`'s `facet`, read with own properties only, so no project's
 *  name or facet reaches a prototype. */
function heldInFile(project: string, facet: Facet, layout: Reading = atCreation().layout): unknown {
  if (movedAside || !layout.found || layout.trouble !== null) return undefined;
  const own = (from: unknown, key: string): unknown =>
    from !== null &&
    typeof from === "object" &&
    !Array.isArray(from) &&
    Object.prototype.hasOwnProperty.call(from, key)
      ? (from as Record<string, unknown>)[key]
      : undefined;
  return own(own(own(layout.document, "projects"), project), facet);
}

/** What `project` keeps for `facet`: this launch's, else the file's, else nothing. Untrusted
 *  when it is the file's: its owner checks it. */
export function keptFacet(project: string | undefined, facet: Facet): unknown {
  if (project === undefined) return undefined;
  const ours = kept.get(project);
  if (ours?.has(facet)) return ours.get(facet);
  return heldInFile(project, facet);
}

/**
 * Keeps `value` as `project`'s `facet`, and tells every listener (`regions.ts` writes the file).
 * Nothing is told, and nothing written, when it is what was kept already.
 */
export function keepFacet(project: string | undefined, facet: Facet, value: unknown): void {
  if (project === undefined) return;
  if (JSON.stringify(value) === JSON.stringify(keptFacet(project, facet))) return;
  const ours = kept.get(project) ?? new Map<Facet, unknown>();
  ours.set(facet, value);
  kept.set(project, ours);
  touch(project);
  for (const listener of listeners) listener(project);
}

/** Marks `project` as one this launch changed, last. `regions.ts` calls it for an arrangement
 *  too, so one order says which project was changed longest ago. */
export function touch(project: string): void {
  touched.delete(project);
  touched.set(project, true);
}

/** The projects this launch kept something in, the one kept in last, last. */
export function projectsTouched(): string[] {
  return [...touched.keys()];
}

/** Every facet `project` keeps now, as its entry in the file writes them: none left empty. */
export function facetsOf(project: string): Partial<Record<Facet, unknown>> {
  const out: Partial<Record<Facet, unknown>> = {};
  for (const facet of FACETS) {
    const value = keptFacet(project, facet);
    if (value !== undefined) out[facet] = value;
  }
  return out;
}

/** Calls `listener` with the project whenever one of its facets changes. Answers the way to
 *  stop. */
export function onProjectViews(listener: (project: string) => void): () => void {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

/** Forgets what this launch kept, as a new launch would. For tests. */
export function forgetProjectViews(): void {
  kept.clear();
  touched.clear();
  movedAside = false;
}

// Use the default layout: no project keeps anything any more, from the file or from this launch.
// Each owner draws its default at once; nothing is written for it.
onLayoutMovedAside(() => {
  movedAside = true;
  kept.clear();
  touched.clear();
});
