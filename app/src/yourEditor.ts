import type { YourEditor } from "./bindings";
import { layoutPref } from "./layoutPref";
import type { SettingsLink } from "./settings/links";

/**
 * **Your editor** (RC-20, ADR 0081 §3): which editor *Open in your editor* hands a file and a
 * line to, chosen in Settings.
 *
 * **Kept in the layout file, beside the text sizes** (`charter/layout.json`, `regions.ts`). It
 * is the operator's on this machine and could follow them to another: ADR 0081 §6 and ADR 0084
 * name it **Machine, syncable**, which is that file's tier, so it adds no store. A plane would
 * carry it to every clone.
 *
 * **One of four words, never a program.** The window sends the core which of
 * {@link EDITORS} it is, and the core builds the URL or reads `$VISUAL`/`$EDITOR` from its own
 * environment, so nothing written in this file is ever run.
 *
 * **None until the operator chooses.** A guess would open the wrong app, or nothing, with no
 * sentence to say why; *Open in your editor* asks for a choice instead.
 */

/**
 * **What *Open in your editor* says when no editor is chosen** (#1201, #1244): one sentence for
 * every place that opens a file in your editor — the file tab, the explorer's and the palette's
 * Open file — each with the link to {@link CHOOSE_EDITOR} beside it.
 */
export const NO_EDITOR = "Choose your editor in Settings first.";
/** Where an editor is chosen: Settings › You › Editor, its one setting focused (SE-22). */
export const CHOOSE_EDITOR: SettingsLink = { group: "you.editor", setting: "you.editor.yours" };

/** The editors, as Settings offers them, in that order. */
export const EDITORS: readonly { id: YourEditor; name: string; says: string }[] = [
  { id: "vscode", name: "VS Code", says: "Through its vscode:// links." },
  { id: "zed", name: "Zed", says: "Through its zed:// links." },
  {
    id: "idea",
    name: "A JetBrains IDE",
    says: "Through idea:// links: IntelliJ IDEA, or whichever JetBrains IDE took them.",
  },
  {
    id: "variable",
    name: "$VISUAL or $EDITOR",
    says: "Run with +line and the file, as purlis was started. Pick one that opens a window of its own, such as gvim or emacsclient -c: a terminal editor has no terminal here.",
  },
];

const KNOWN = new Set<string>(EDITORS.map((one) => one.id));

/** Your editor as a layout document holds it, and what had to be put right. */
export function loadEditor(raw: unknown): { editor: YourEditor | undefined; said: string[] } {
  const held =
    raw !== null && typeof raw === "object" && !Array.isArray(raw)
      ? (raw as { editor?: unknown }).editor
      : undefined;
  if (held === undefined) return { editor: undefined, said: [] };
  if (typeof held === "string" && KNOWN.has(held)) {
    return { editor: held as YourEditor, said: [] };
  }
  return {
    editor: undefined,
    said: [
      `"editor" ${JSON.stringify(held)} is not one of ${[...KNOWN].join(", ")}, so no editor is chosen`,
    ],
  };
}

/** The store (`layoutPref.ts`), under `editor`. */
export const YOUR_EDITOR = layoutPref<YourEditor | undefined, "editor">({
  key: "editor",
  fallback: undefined,
  load: (raw) => {
    const { editor, said } = loadEditor(raw);
    return { value: editor, said };
  },
  same: (one, other) => one === other,
  written: (editor) => editor,
  remedy: (where) => `fix ${where}, or choose your editor in Settings, which rewrites it`,
  settings: "you.editor",
});

/** Your editor, or nothing when none is chosen. */
export function yourEditor(): YourEditor | undefined {
  return YOUR_EDITOR.value();
}

/** Chooses your editor; tells every listener when it changed. */
export function setYourEditor(editor: YourEditor): void {
  YOUR_EDITOR.set(editor);
}

/** {@link yourEditor}, for a component that redraws when it changes. */
export function useYourEditor(): YourEditor | undefined {
  return YOUR_EDITOR.use();
}

/** Forgets what this launch chose and read, as a new launch would. For tests. */
export function forgetYourEditor(): void {
  YOUR_EDITOR.forget();
}
