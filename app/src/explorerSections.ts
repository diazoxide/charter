import { useSyncExternalStore } from "react";
import { layoutPref } from "./layoutPref";
import { onLayoutMovedAside } from "./windowprefs";
import { keepExplorer, keptExplorer } from "./explorerFolds";

/**
 * **Which of Explorer's sections are folded** (#1677, spec #1671 B-12): *Workspaces*, the
 * focused workspace's *Repos and branches*, and *Files*. Each folds on its heading, as an
 * editor's explorer does, and stays folded on the next launch.
 *
 * **Kept per project in the layout file** (B-11, #1686), as each project's arrangement is: under
 * `projects[path].explorer.closed` (`explorerFolds.ts`), the sections folded there, an empty list
 * when every one is open. **And for the machine**, under the top-level `explorer.closed` that
 * #1677 wrote: the person's last fold in any project, which is what a project that has none of
 * its own starts from, as a project with no arrangement starts from the machine's.
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
  closed: ReadonlySet<SectionId> = machineSections(),
): { closed: SectionId[] } | undefined {
  if (closed.size === 0) return undefined;
  return { closed: SECTIONS.filter((one) => closed.has(one)) };
}

const sameSections = (one: ReadonlySet<SectionId>, other: ReadonlySet<SectionId>) =>
  one.size === other.size && [...one].every((section) => other.has(section));

/** The machine's sections folded (`layoutPref.ts`), under `explorer`: the last fold in any
 *  project. */
export const EXPLORER_SECTIONS = layoutPref<ReadonlySet<SectionId>, "explorer">({
  key: "explorer",
  fallback: NONE,
  load: (raw) => {
    const { closed, said } = loadExplorerSections(raw);
    return { value: closed, said };
  },
  same: sameSections,
  written: (closed) => explorerSectionsDocument(closed),
  remedy: (where) => `fix ${where}, or fold or open a section of Explorer, which rewrites it`,
  forgotten: () => projectSets.clear(),
});

/** What draws the sections, told on every change, a project's or the machine's;
 *  {@link EXPLORER_SECTIONS}'s listeners hear only the machine's, which are written. */
const drawers = new Set<() => void>();

/** The machine's sections folded now: the last fold in any project. */
function machineSections(): ReadonlySet<SectionId> {
  return EXPLORER_SECTIONS.value();
}

/** Each project's own folded sections, as last handed out, by what they were read from: the
 *  same set while nothing changed, so `useSyncExternalStore` draws nothing again. */
const projectSets = new Map<string, { from: string; closed: ReadonlySet<SectionId> }>();

/** The sections folded now in `project`: its own, else the machine's. */
export function closedSections(project?: string): ReadonlySet<SectionId> {
  const own = keptExplorer(project).closed;
  if (project === undefined || own === undefined) return machineSections();
  const from = own.join("\n");
  const was = projectSets.get(project);
  if (was?.from === from) return was.closed;
  const closed: ReadonlySet<SectionId> = new Set(own.filter(isSection));
  projectSets.set(project, { from, closed });
  return closed;
}

/**
 * Folds a section, or opens it, in `project` (B-11): its own sections, and the machine's, which a
 * project with none of its own starts from. The project's entry is what writes the file then
 * (`projectViews.ts`), so the listeners here are told only of a fold with no project.
 */
export function setSectionOpen(section: SectionId, open: boolean, project?: string): void {
  const was = closedSections(project);
  if (open !== was.has(section)) return;
  const now = new Set(was);
  if (open) now.delete(section);
  else now.add(section);
  if (project === undefined) EXPLORER_SECTIONS.set(now);
  else {
    EXPLORER_SECTIONS.keep(now);
    // An empty list is kept: every section open here is this project's, whatever the machine's.
    keepExplorer(project, { closed: SECTIONS.filter((one) => now.has(one)) });
  }
  for (const draw of drawers) draw();
}

/** Calls `listener` whenever a section folds or opens. Answers the way to stop. */
export function onExplorerSections(listener: (closed: ReadonlySet<SectionId>) => void): () => void {
  return EXPLORER_SECTIONS.on(listener);
}

/** {@link closedSections}, for a component that redraws when a section folds or opens. */
export function useClosedSections(project?: string): ReadonlySet<SectionId> {
  return useSyncExternalStore(
    (draw) => {
      drawers.add(draw);
      return () => void drawers.delete(draw);
    },
    () => closedSections(project),
  );
}

/** Forgets what this launch folded and read, as a new launch would. For tests. */
export function forgetExplorerSections(): void {
  EXPLORER_SECTIONS.forget();
}

// Use the default layout: every section open, drawn at once. Not written: the file was just moved
// aside, and the next change the person makes writes a new one without them.
onLayoutMovedAside(() => {
  EXPLORER_SECTIONS.keep(NONE);
  projectSets.clear();
  for (const draw of drawers) draw();
});
