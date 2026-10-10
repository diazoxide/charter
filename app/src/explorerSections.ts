import { useSyncExternalStore } from "react";
import { atCreation, onLayoutMovedAside, sayAboutThisMachine, type Reading } from "./windowprefs";

/**
 * **Which of Explorer's sections are folded** (#1677, spec #1671 B-12): *Workspaces*, the
 * focused workspace's *Repos and branches*, and *Files*. Each folds on its heading, as an
 * editor's explorer does, and stays folded on the next launch.
 *
 * **Kept in the layout file**, beside how the Chats list is drawn (`layout.json`, `regions.ts`),
 * under `explorer.closed`: the sections folded, and nothing when every one is open. It is how one
 * person likes their window on this machine, so it is the machine's and not a project's — a
 * section is a habit, where the view open on a side is about the project in front.
 * **Use the default layout opens every section again**, at once and in what the next write
 * carries, without writing the file it has just moved aside.
 */

/** The sections, in the order they are drawn. */
export const SECTIONS = ["workspaces", "repos", "files"] as const;
export type SectionId = (typeof SECTIONS)[number];

const NONE: ReadonlySet<SectionId> = new Set();

const isSection = (one: unknown): one is SectionId =>
  typeof one === "string" && (SECTIONS as readonly string[]).includes(one);

/** The folded sections a layout document holds, and what had to be put right to read them. */
export function loadExplorerSections(raw: unknown): {
  closed: ReadonlySet<SectionId>;
  said: string[];
} {
  const said: string[] = [];
  const held =
    raw !== null && typeof raw === "object" && !Array.isArray(raw)
      ? (raw as { explorer?: unknown }).explorer
      : undefined;
  if (held === undefined) return { closed: NONE, said };
  if (held === null || typeof held !== "object" || Array.isArray(held)) {
    said.push(
      `"explorer" ${JSON.stringify(held)} is not Explorer's sections, so every one is open`,
    );
    return { closed: NONE, said };
  }
  const list = (held as { closed?: unknown }).closed;
  if (list === undefined) return { closed: NONE, said };
  if (!Array.isArray(list)) {
    said.push(
      `"explorer.closed" ${JSON.stringify(list)} is not a list of sections, so every section is open`,
    );
    return { closed: NONE, said };
  }
  const unknown = list.filter((one) => !isSection(one));
  if (unknown.length > 0)
    said.push(
      `"explorer.closed" names ${unknown.map((one) => JSON.stringify(one)).join(", ")}, which Explorer has no section called`,
    );
  return { closed: new Set(list.filter(isSection)), said };
}

/** The folded sections as the layout file writes them, or nothing when every one is open. */
export function explorerSectionsDocument(
  closed: ReadonlySet<SectionId> = closedSections(),
): { closed: SectionId[] } | undefined {
  if (closed.size === 0) return undefined;
  return { closed: SECTIONS.filter((one) => closed.has(one)) };
}

let changed: ReadonlySet<SectionId> | undefined;
let started: ReadonlySet<SectionId> | undefined;
const listeners = new Set<(closed: ReadonlySet<SectionId>) => void>();
/** What draws the sections, told on every change; {@link listeners} also hear only the
 *  person's, which are written. */
const drawers = new Set<() => void>();

function startingSections(layout: Reading = atCreation().layout): ReadonlySet<SectionId> {
  if (started !== undefined) return started;
  // A layout file purlis refused is said once, by `regions.ts`, and is every section open here.
  const { closed, said } =
    layout.found && layout.trouble === null
      ? loadExplorerSections(layout.document)
      : { closed: NONE, said: [] };
  if (said.length > 0) {
    const where = layout.path || "the layout file";
    sayAboutThisMachine("explorer", {
      severity: "warn",
      detail: `${where}: ${said.join("; ")}`,
      remedy: `fix ${where}, or fold or open a section of Explorer, which rewrites it`,
    });
  }
  started = closed;
  return closed;
}

/** The sections folded now. */
export function closedSections(): ReadonlySet<SectionId> {
  return changed ?? startingSections();
}

function become(now: ReadonlySet<SectionId>): void {
  changed = now;
  sayAboutThisMachine("explorer", undefined);
  for (const listener of listeners) listener(now);
  for (const draw of drawers) draw();
}

/** Folds a section, or opens it; tells every listener when that changed anything. */
export function setSectionOpen(section: SectionId, open: boolean): void {
  const was = closedSections();
  if (open !== was.has(section)) return;
  const now = new Set(was);
  if (open) now.delete(section);
  else now.add(section);
  become(now);
}

/** Calls `listener` whenever a section folds or opens. Answers the way to stop. */
export function onExplorerSections(listener: (closed: ReadonlySet<SectionId>) => void): () => void {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

/** {@link closedSections}, for a component that redraws when a section folds or opens. */
export function useClosedSections(): ReadonlySet<SectionId> {
  return useSyncExternalStore((draw) => {
    drawers.add(draw);
    return () => void drawers.delete(draw);
  }, closedSections);
}

/** Forgets what this launch folded and read, as a new launch would. For tests. */
export function forgetExplorerSections(): void {
  changed = undefined;
  started = undefined;
}

// Use the default layout: every section open, drawn at once. Not written: the file was just moved
// aside, and the next change the person makes writes a new one without them.
onLayoutMovedAside(() => {
  if (closedSections().size === 0) return;
  changed = NONE;
  for (const draw of drawers) draw();
});
