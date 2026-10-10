import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * **Every empty view tab says what goes there, through `EmptyState`** (DS-3 #626 line 1, the
 * empty-states part; FR-19 #614).
 *
 * `EmptyState.tsx` is the one way to draw a view with nothing in it: a headline that says what
 * is missing, a line that says what goes there, and the tab's own button where it has one.
 * Before it, each tab wrote its own `<p className="none">No …</p>`, which said what was missing
 * and never what goes there. This fails when a view tab draws one that way again: a paragraph
 * whose words open with "No", "Nothing" or "None".
 *
 * **A search or filter that keeps nothing is not an empty state** (D-626-1): the view holds
 * things, and the box above the list is the way back to them. That line says what matched
 * nothing ("Nothing here matches …"), so a paragraph that says "matches" is let through.
 *
 * The view tabs are read from `Views.tsx`: every component it imports whose name ends in `Tab`
 * or `View`, so a tab added later is held too. A tab not moved yet is in `LATER`, with why.
 *
 * Read as text, as `settings/oldFormClasses.test.ts` and `Notice.guard.test.ts` read theirs.
 */

/** The view tabs left for a later change, each with why. Empty since Search stopped being a
 *  view tab (#1701): it is a side view only, and draws its empty states through `EmptyState`. */
const LATER: Record<string, string> = {};

/** The side views, read beside the view tabs (#1718 line 4): Changes and Search draw in the side
 *  since #1701, so `Views.tsx` no longer imports them, and their empty states are held all the
 *  same. */
const SIDE_VIEWS = ["ChangesView.tsx", "SearchTab.tsx"];

const SRC = join(process.cwd(), "src");

/** The source files of the view tabs `Views.tsx` draws, relative to `src/`. */
function viewTabs(views: string): string[] {
  return [...views.matchAll(/import\s*\{\s*(\w+)\b[^}]*\}\s*from\s*"\.\/([\w/]+)"/g)]
    .filter(([, name]) => /(Tab|View)$/.test(name))
    .map(([, , path]) => `${path}.tsx`);
}

/** What opens a paragraph's words: the text right after `<p …>`, or, where the paragraph opens
 *  with an expression, every string in it. */
function openings(inner: string): string[] {
  const text = inner.trim();
  if (!text.startsWith("{")) return [text];
  return [...text.matchAll(/"([^"]*)"|'([^']*)'|`([^`]*)`/g)].map(([, a, b, c]) => a ?? b ?? c);
}

/** The paragraphs in `source` drawn as an empty state by hand: their words, one per paragraph. */
function handDrawnEmpties(source: string): string[] {
  return [...source.matchAll(/<p\b[^>]*>((?:(?!<\/p>)[\s\S])*?)<\/p>/g)]
    .map(([, inner]) => inner)
    .filter((inner) => !/\bmatches\b/.test(inner))
    .filter((inner) => openings(inner).some((said) => /^(No|Nothing|None)\b/.test(said.trim())))
    .map((inner) => inner.replace(/\s+/g, " ").trim());
}

const read = (path: string) => readFileSync(join(SRC, path), "utf8");

describe("an empty view tab", () => {
  const tabs = [...viewTabs(read("Views.tsx")), ...SIDE_VIEWS];

  it("is read from every view tab Views.tsx draws", () => {
    // A reader that found none would pass on nothing.
    expect(tabs).toEqual(
      expect.arrayContaining([
        "SavingView.tsx",
        "TaskChangesTab.tsx",
        "ChatNetworkTab.tsx",
        "ChangesView.tsx",
        "SearchTab.tsx",
      ]),
    );
  });

  it("says what goes there through EmptyState, never a paragraph of its own", () => {
    const drawn = tabs
      .filter((path) => !(path in LATER))
      .flatMap((path) => handDrawnEmpties(read(path)).map((said) => `${path}: ${said}`));
    expect(drawn).toEqual([]);
  });

  it("is left for later only where it is a view tab still", () => {
    expect(Object.keys(LATER).filter((path) => !tabs.includes(path))).toEqual([]);
  });

  it("would be caught if one came back", () => {
    // The reader above is what the guard rests on, so it is held to the shapes a tab writes.
    expect(handDrawnEmpties(`<p className="none">No saves recorded yet.</p>`)).toEqual([
      "No saves recorded yet.",
    ]);
    expect(handDrawnEmpties(`<p>Nothing was refused in the last 30 days.</p>`)).toHaveLength(1);
    expect(
      handDrawnEmpties(`<p className="none">
          {own === null ? "No file is listed here." : "No changed file to show."}
        </p>`),
    ).toHaveLength(1);
    // A search that kept nothing, a field with no value yet, and a sentence that merely
    // contains the word are not empty states.
    expect(handDrawnEmpties(`<p className="none">Nothing here matches “{query}”.</p>`)).toEqual([]);
    expect(handDrawnEmpties(`<p className="none">Not reported yet.</p>`)).toEqual([]);
    expect(handDrawnEmpties(`<p>It holds no secrets.</p>`)).toEqual([]);
    expect(
      viewTabs(`import { VaultTab } from "./VaultTab";\nimport { Notice } from "./Notice";`),
    ).toEqual(["VaultTab.tsx"]);
  });
});
