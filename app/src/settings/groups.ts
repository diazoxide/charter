import type { ReactNode } from "react";
import type { Reset, RowIds } from "./components";

/**
 * **Settings groups are data** (V89b, the spec on #558): each group is declared once, with a
 * stable id, a label, a line of help and its settings, and the Settings tab draws whatever it is
 * handed. It extends Project settings' `Group`/`Control` description (`ProjectSettings.tsx`), so
 * the Project level (SE-17) declares its groups the same way, and SE-2's registry later supplies
 * this same shape without the layout changing.
 *
 * **The id is the group's address** (V89c): `you.text`, `you.editor`. A link that says "change X
 * in Settings" names it, so it never changes once shipped; a label can.
 */
export type SettingsGroup = {
  /** The stable address: `<level>.<group>`. */
  id: string;
  label: string;
  /** One line on what the group is for, said under its heading. */
  help: string;
  settings: readonly Setting[];
};

/**
 * One setting: its stable id, its label, its line of help, and its live half.
 *
 * `useControl` is a hook — the row calls it once per render, always the same one for a given
 * setting — and answers how to draw the control (handed the row's ids, which tie it to the label
 * and the help) and, when the value can go back, the reset. That is
 * how a setting whose value lives in a store of its own (a text size, your editor) reads it and
 * redraws when it changes elsewhere.
 */
export type Setting = {
  /** The stable address: `<group id>.<setting>`. */
  id: string;
  label: string;
  help: string;
  useControl: () => { control: (ids: RowIds) => ReactNode; reset?: Reset };
};

/** A level the switcher can offer, and the groups it holds (`CONTEXT.md`, **Level**). */
export type Level = "you" | "project" | "workspace" | "persona";

/** What the switcher calls each level, in its order. */
export const LEVELS: readonly { id: Level; label: string }[] = [
  { id: "you", label: "You" },
  { id: "project", label: "Project" },
  { id: "workspace", label: "Workspace" },
  { id: "persona", label: "Persona" },
];
