import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * **The window's views take their form controls from the set** (DS-3, #626 line 2; D-626-5).
 *
 * The settings set (`settings/components.tsx`, ADR 0037 as amended) is the one house set, and
 * `oldFormClasses.test.ts` keeps the classes it replaced from coming back. This is the other
 * half: what "a view uses only the set" means, checked.
 *
 * **What the set means for a view (D-626-5).** A box, a pick or a tick that is part of a form is
 * a `Field` or a `Choice` in a `SettingRow`, so it has the set's label, help, keyboard and look.
 * The set does not draw buttons: a `<button>` is native HTML that already does the job
 * (`docs/ui-primitives.md`, *The rule*), and a form's or a question's buttons are the caller's,
 * in `SettingActions` or the `AnswerBar`. So the guard is about the three native form
 * controls, `<input>`, `<select>` and `<textarea>`: outside the set, a file draws one only
 * where it is named below, with how many and why.
 *
 * Two kinds of entry, both exact about the file and about the count:
 * - **Not a form row** (`STANDS`): a box that is the surface itself rather than a row of a form,
 *   such as a list's filter, the palette's query, a tab renamed in place, a reply box, a raw file
 *   editor, or a secret's value box, which the set's `Field` cannot hold because a secret is
 *   never kept in React state (`docs/ui-primitives.md`, the native box in a row's slot).
 * - **Debt** (`DEBT`): a form control that should be a `Field` or a `Choice`, named so that
 *   moving it is a visible change, and so that a new one cannot arrive unnoticed.
 *
 * An exact count and not an upper bound, so an entry cannot outlive what it names: a file that
 * draws fewer than its entry says, or none, fails here until the entry is lowered or dropped. A
 * train that pays a debt off (DS-8's first pass moved Close a chat's radios onto the set) lowers
 * its entry in the same change, which is the visible change this guard is for.
 */

const SRC = join(process.cwd(), "src");

/** The set itself: the one file that draws the native controls for everybody else. */
const THE_SET = "settings/components.tsx";

const FILTER = "a list's own filter or search box, drawn over the list it narrows: not a form row";
const SECRET =
  "a secret's value box: native so the value never sits in React state, which a Field would keep";

/** Native form controls that are the surface itself, not a row of a form. */
const STANDS: Record<string, { count: number; why: string }> = {
  "AnswerQuestion.tsx": { count: 1, why: "the reply box a chat's question is answered in" },
  "ChatsSection.tsx": { count: 1, why: FILTER },
  "DispatchesTab.tsx": { count: 4, why: "the dispatch list's four filters: " + FILTER },
  "Explorer.tsx": { count: 1, why: FILTER },
  "Inbox.tsx": { count: 1, why: "a reply typed into an Inbox row, a stop of the list it is in" },
  "FindBar.tsx": { count: 1, why: "the terminal's find box, its own bar over the pane" },
  "MemoryArchiveTab.tsx": { count: 1, why: FILTER },
  "Palette.tsx": { count: 1, why: "the palette's query, a combobox over its own list" },
  "PanelList.tsx": { count: 1, why: FILTER },
  "Panels.tsx": { count: 1, why: "a todo typed straight into its list, Enter adds it" },
  "PersonaMarkPicker.tsx": { count: 1, why: "the mark picker's own hex box beside its swatches" },
  "SearchTab.tsx": { count: 2, why: "the Search view's query and its scope, the view itself" },
  "TabRename.tsx": { count: 1, why: "a tab renamed in place, on the strip" },
  "VaultSignIn.tsx": { count: 1, why: SECRET },
  "VaultTab.tsx": { count: 3, why: `${FILTER}; and two value boxes: ${SECRET}` },
  "settings/RawToml.tsx": { count: 1, why: "the raw file editor, a whole file in one box" },
  "settings/SettingsTab.tsx": {
    count: 1,
    why: "the colour well, which no Field kind draws, in the row's control slot",
  },
};

/** Form controls that should be the set's `Field` or `Choice`, and are not yet. */
const DEBT: Record<string, { count: number; why: string }> = {
  "ChatsSection.tsx": {
    count: 1,
    why: "the state chips are native checkboxes under roving focus: a `Choice` checks (#1687)",
  },
  "DispatchGrantNotice.tsx": {
    count: 2,
    why: "two native radios for the grant's reach: a `Choice` radio, as Close a chat's became",
  },
  "SandboxBlockNotice.tsx": {
    count: 1,
    why: "the host to allow is a bare input in a Notice: a Field once Notices take a form row",
  },
  "settings/DispatchGrants.tsx": {
    count: 5,
    why: "a grant's four picks and the workspace are native selects: `Choice` select",
  },
  "settings/GrantedList.tsx": {
    count: 1,
    why: "the folder to add is a bare input beside its list: a Field in the row",
  },
  "settings/dispatch.tsx": {
    count: 2,
    why: "the limit box and the override pick: a Field and a `Choice` select",
  },
};

/** Every file under `dir` whose name `keep` takes. */
function files(dir: string, keep: (name: string) => boolean): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return files(path, keep);
    return keep(entry.name) ? [path] : [];
  });
}

/** Every component file the window is drawn from: not tests, which render controls freely. */
const views = () => files(SRC, (name) => name.endsWith(".tsx") && !/\.test\.tsx$/.test(name));

/** How many native form controls `source` draws, comments left out: a doc comment that names
 *  `<input>` draws nothing. */
export function nativeControls(source: string): number {
  const code = source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");
  return [...code.matchAll(/<(input|select|textarea)(?=[\s/>])/g)].length;
}

/** Each file whose native form controls differ from what {@link STANDS} and {@link DEBT} name
 *  for it, either way, given what each view draws: a file over its entry, and an entry over its
 *  file, which is stale. */
function offTheList(drawn: ReadonlyMap<string, number>): string[] {
  const files = new Set([...drawn.keys(), ...Object.keys(STANDS), ...Object.keys(DEBT)]);
  return [...files].flatMap((file) => {
    const count = drawn.get(file) ?? 0;
    const named = (STANDS[file]?.count ?? 0) + (DEBT[file]?.count ?? 0);
    return count !== named ? [`${file}: ${count} native form controls, ${named} named`] : [];
  });
}

describe("the views take their form controls from the set (#626, D-626-5)", () => {
  it("draw a native input, select or textarea only where it is named, and as many as named", () => {
    const drawn = new Map(
      views()
        .map((path) => [relative(SRC, path), nativeControls(readFileSync(path, "utf8"))] as const)
        .filter(([file]) => file !== THE_SET),
    );
    expect(offTheList(drawn)).toEqual([]);
  });

  it("fails an entry its file no longer bears out, so none goes stale (#626)", () => {
    // A file paid off, still named: it draws none of what its entry says.
    expect(offTheList(new Map([["SandboxBlockNotice.tsx", 0]]))).toContain(
      "SandboxBlockNotice.tsx: 0 native form controls, 1 named",
    );
    // A named file the views no longer have at all.
    expect(offTheList(new Map()).some((line) => line.startsWith("SandboxBlockNotice.tsx: 0"))).toBe(
      true,
    );
    // And one drawn where nothing names it.
    expect(offTheList(new Map([["Fresh.tsx", 1]]))).toContain(
      "Fresh.tsx: 1 native form controls, 0 named",
    );
  });

  it("name only files that exist, each with a reason", () => {
    for (const [file, { count, why }] of [...Object.entries(STANDS), ...Object.entries(DEBT)]) {
      expect(existsSync(join(SRC, file)), file).toBe(true);
      expect(count, file).toBeGreaterThan(0);
      expect(why.length, file).toBeGreaterThan(20);
    }
  });

  it("finds the set drawing them, so a reader that found nothing would fail", () => {
    expect(nativeControls(readFileSync(join(SRC, THE_SET), "utf8"))).toBeGreaterThan(0);
  });

  it("counts what a view draws and not what a comment names", () => {
    expect(nativeControls(`<input type="text" />`)).toBe(1);
    expect(nativeControls(`<select\n  value={v}>`)).toBe(1);
    expect(nativeControls(`<textarea>`)).toBe(1);
    expect(nativeControls(`/** an <input> here */ <p />`)).toBe(0);
    expect(nativeControls(`// an <input> here\n<p />`)).toBe(0);
    expect(nativeControls(`<inputs />`)).toBe(0);
    expect(nativeControls(`const url = "https://x/<input"; <p />`)).toBe(0);
  });
});
