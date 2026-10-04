import type { ReactNode } from "react";
import type { Reset, RowIds } from "./components";

/**
 * **Settings groups are data** (V89b, the spec on #558): each group is declared once, with a
 * stable id, a label, a line of help and its settings, and the Settings tab draws whatever it is
 * handed. The group's half of that shape is what every level shares.
 *
 * **A setting's half is not yet the shape the file-backed levels need.** Project settings'
 * `Control` (`ProjectSettings.tsx`) is declarative: a `kind`, `read(file)` and `edits(draft,
 * file)`, which is what one tab-level driver needs for the per-key write, Undo and the error said
 * beside the setting, and what SE-2's registry will supply. SE-17 adds that as a second variant
 * of {@link Setting}. The one here, `useControl`, is the escape hatch for a value kept in a
 * store of its own rather than in a settings file, which is all the You level has.
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
 * One setting: its stable id, its label, its line of help, and its live half — today only the
 * escape hatch for a value in a store of its own (above).
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
  useControl: () => {
    control: (ids: RowIds) => ReactNode;
    reset?: Reset;
    /** The control is a group of controls (a radio group): see `SettingRow`'s `grouped`. */
    grouped?: boolean;
  };
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
