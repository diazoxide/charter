import { keepFacet, keptFacet } from "./projectViews";

/**
 * **What Explorer keeps for each project** (spec #1671 B-11, #1686): its folded sections
 * (`explorerSections.ts`) and the rows folded or opened in its trees, so a relaunch draws the
 * project's Explorer as the person left it. One entry, `explorer`, in the project's place in
 * `layout.json` v2 (`projectViews.ts`):
 *
 * - **`closed`**: the sections folded on their headings (`explorerSections.ts` reads it);
 * - **`folded`**: the clones folded in *Repos and branches*, open until folded;
 * - **`opened`**: the folders opened in *Files*, closed until opened;
 * - **`shut`**: the cockpit's *Files* rows closed, open until closed.
 *
 * The last three are Explorer's own row keys (`Explorer.tsx`), kept as it wrote them and never
 * read as paths: a key that names nothing on screen any more opens or folds nothing. Each list
 * is left out while it is empty, and the whole entry while every one is; `closed` is kept empty,
 * since a project with none follows the machine's.
 *
 * **Bounded** ({@link EXPLORER_MOST_BYTES}): a person who opened hundreds of folders keeps the
 * ones opened last; the folders opened longest ago go first, then the clones folded longest ago,
 * then the cockpit's rows. A row let go only starts as it does by itself.
 */

/** The most one project's Explorer entry takes of the file, as written. Thirty-two projects of
 *  it are well past the projects' budget (`regions.PROJECTS_MOST_BYTES`): the projects changed
 *  longest ago are let go there first. */
export const EXPLORER_MOST_BYTES = 2 * 1024;

/** The longest key kept from the file: a folder's path inside a branch, with its workspace. */
const MOST_KEY = 1024;

/** Explorer's entry as the file holds it. */
export type ExplorerKept = {
  closed?: string[];
  folded?: string[];
  opened?: string[];
  shut?: string[];
};

const PARTS = ["closed", "folded", "opened", "shut"] as const;
type Part = (typeof PARTS)[number];

/** The rows folded and opened in Explorer's trees. */
export type TreeFolds = {
  folded: ReadonlySet<string>;
  opened: ReadonlySet<string>;
  shut: ReadonlySet<string>;
};

/** Explorer's entry for `project`, each list read as names and nothing else: a list that is
 *  not one is none, and an item that is not a name is skipped. */
export function keptExplorer(project: string | undefined): ExplorerKept {
  const held = keptFacet(project, "explorer");
  if (held === null || typeof held !== "object" || Array.isArray(held)) return {};
  const out: ExplorerKept = {};
  for (const part of PARTS) {
    const list = Object.prototype.hasOwnProperty.call(held, part)
      ? (held as Record<string, unknown>)[part]
      : undefined;
    if (!Array.isArray(list)) continue;
    out[part] = list.filter(
      (one): one is string => typeof one === "string" && one.length <= MOST_KEY,
    );
  }
  return out;
}

/** Keeps `parts` as Explorer's for `project`, beside what it kept of the rest: the lists in
 *  their order, empty ones left out, and the whole held to {@link EXPLORER_MOST_BYTES}. */
export function keepExplorer(project: string | undefined, parts: ExplorerKept): void {
  if (project === undefined) return;
  const was = keptExplorer(project);
  const next: ExplorerKept = {};
  for (const part of PARTS) {
    const list = part in parts ? parts[part] : was[part];
    // `closed` is kept empty too: every section open is the project's own, not the machine's.
    if (list !== undefined && (list.length > 0 || part === "closed")) next[part] = [...list];
  }
  within(next);
  keepFacet(project, "explorer", Object.keys(next).length === 0 ? undefined : next);
}

/** The lists let go of first, oldest first, when an entry is over its budget. */
const LET_GO: readonly Part[] = ["opened", "folded", "shut"];

function within(entry: ExplorerKept): void {
  const bytes = () => new TextEncoder().encode(JSON.stringify(entry, null, 2)).length;
  for (const part of LET_GO) {
    while (bytes() > EXPLORER_MOST_BYTES) {
      const list = entry[part];
      if (list === undefined) break;
      list.shift();
      if (list.length === 0) {
        entry[part] = undefined;
        break;
      }
    }
  }
}

/** The rows folded and opened in `project`'s Explorer, as last kept. */
export function keptTreeFolds(project: string | undefined): TreeFolds {
  const held = keptExplorer(project);
  return {
    folded: new Set(held.folded),
    opened: new Set(held.opened),
    shut: new Set(held.shut),
  };
}

/** Keeps the rows folded and opened in `project`'s Explorer: written only when they changed. */
export function keepTreeFolds(project: string | undefined, folds: TreeFolds): void {
  keepExplorer(project, {
    folded: [...folds.folded],
    opened: [...folds.opened],
    shut: [...folds.shut],
  });
}
