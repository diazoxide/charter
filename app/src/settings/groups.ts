import type { ReactNode } from "react";
import type { SettingsEdit, SettingsStep, SettingsWhich } from "../bindings";
import type { Entry, Shown } from "./fileControls";
import type { Reset, RowIds } from "./components";

/**
 * **Settings groups are data** (V89b, the spec on #558): each group is declared once, with a
 * stable id, a label, a line of help and its settings, and the Settings tab draws whatever it is
 * handed. The group's half of that shape is what every level shares.
 *
 * **A setting is one of two shapes** ({@link Setting}). A value kept in a settings file is
 * *declared* ({@link FileSetting}): the key it is at, how its control is drawn, how it reads its
 * value out of the file and the edits a new value makes — Project settings' `Control`
 * (`fileControls.ts`) with the file it is kept in. One tab-level driver then writes it, undoes it
 * and says why a write was refused (`project.ts`), and SE-2's registry can supply the same
 * shape. A value kept in a store of its own — all the You level has — is *live*
 * ({@link LiveSetting}): a hook that draws its control, the escape hatch.
 *
 * **The id is the group's address** (V89c): `you.text`, `project.saving`. A link that says
 * "change X in Settings" names it, so it never changes once shipped; a label can.
 */
export type SettingsGroup = {
  /** The stable address: `<level>.<group>`. */
  id: string;
  label: string;
  /** One line on what the group is for, said under its heading. */
  help: string;
  settings: readonly Setting[];
  /** Sentences about what is in force in this group, the core's: drawn under its help. */
  notes?: readonly string[];
  /** The collection the group holds, when it holds one (ST-3): its entries, each with Remove,
   *  and an Add form. A group with a collection is offered even while it has no entry. */
  collection?: Collection;
  /** A page of the group before it in the nav (ST-4, V91e): one entry of that group's
   *  collection with many fields — a harness profile — drawn indented under it. */
  sub?: boolean;
};

/**
 * **A Settings collection** (ST-3, the spec on #1221, V91e): a list of entries in a settings file
 * — `[[forge]]` blocks — that the group adds to and takes from. Declared as data like the rest
 * of a group; the driver writes it (`driver.ts`'s `entry`), through the core's one function per
 * collection, which keeps every rule (V91l): the window checks nothing itself.
 *
 * - **Each entry** is a heading with its Remove, over the rows of its own settings (by id, from
 *   the group's `settings`, so the filter and the per-key writes are a group's as ever).
 * - **Add** opens an inline form of the {@link EntryField}s, as setting rows; the core's refusal
 *   of a field is said under that field, and the form keeps what was typed.
 * - **A refused Remove** names what uses the entry, each with a link to the group it is changed
 *   in; nothing cascades. A written add or remove has the one-level Undo, which is the inverse
 *   operation through the same core function (D-ST3-i).
 *
 * The entries, their identities and labels are the core's (`SettingsFile.entries`): the window
 * computes none of them.
 */
export type Collection = {
  /** The level's name for the collection, which its writes are sent as: `forges`. */
  name: string;
  /** What one entry is called, in Add and Remove's words: `forge`. */
  noun: string;
  /** The text of the collection's file the entries were drawn from (`null`: not there): what
   *  an Add or a Remove pressed now is sent against, so it is refused if the file moved. */
  base: string | null;
  entries: readonly CollectionEntry[];
  /** The Add form's fields, in order. */
  fields: readonly EntryField[];
  /** Whether Add is offered here (the default). An entry's own page holds that entry alone,
   *  with no Add: that is the collection's group's (ST-4). */
  adds?: boolean;
  /** Whether each entry offers Rename (ST-4, V91k): refused, naming them, while anything uses
   *  the entry. The field it renames is the Add form's `name`. */
  renames?: boolean;
  /** The group the collection's writes are kept under — its Undo and its refusals — when that
   *  is not the group drawing it: an entry's page keeps them with its collection's group, so a
   *  Remove there is undone where the person lands (ST-4). */
  home?: string;
  /** The address of the page an entry called `name` has, for a collection whose entries have
   *  pages of their own (ST-4): where a rename goes to. */
  pageOf?: (name: string) => string;
};

/** One entry of a collection. */
export type CollectionEntry = {
  /** The core's identity for it: what a remove is sent as. Opaque, and different once the entry
   *  moved or changed (D-ST3-j). */
  id: string;
  /** What the entry is called, by the core: its heading, and what Remove and its Undo name. */
  label: string;
  /** The ids of the group's settings that are this entry's keys. */
  settings: readonly string[];
  /** The address of the entry's own page, when it has one and is not drawn on it (ST-4): its
   *  heading opens it. */
  page?: string;
  /** The entry's name, as Rename starts from it (ST-4). */
  name?: string;
  /** Whether it waits for Confirm (#1341): your own sandbox host, not yet confirmed here. */
  confirm?: boolean;
};

/** One field of a collection's Add form, written to the entry's key `field`. */
export type EntryField = {
  field: string;
  label: string;
  help: string;
  /** `text` is one line, `choice` a closed set, `lines` one entry per line. */
  kind: "text" | "choice" | "lines";
  choices?: readonly string[];
  /** What the field holds when the form opens. */
  initial?: string;
};

/** What every setting has: its stable address (`<group id>.<setting>`), label and help. */
type Named = { id: string; label: string; help: string };

/**
 * **Which settings file a value is kept in**: the project's `charter.toml` (`shared`) or
 * `charter.local.toml` (`local`), or a workspace's `workspace.json` (`workspace`, SE-20).
 */
export type SettingsFileId = SettingsWhich | "workspace";

/**
 * **A setting kept in a settings file** (SE-17): the key it is at and the file it is kept in,
 * how its control is drawn, how it reads its value out of the file, and the edits a new value
 * makes. Every value is text while it is typed or picked; `edits` is where it becomes a key, so
 * a setting whose value spans keys (a profile's environment) still writes only its own.
 *
 * `file` is the file a value goes to while no file holds one. A `movable` setting's value may be
 * kept in either of the project's two files (SE-18, V89d): its row offers "Shared / Only on this
 * machine", which moves the value between them, and the value in force is `charter.local.toml`'s
 * where it holds one (Local overrides Shared key by key).
 */
export type FileSetting = Named & {
  file: SettingsFileId;
  /** Kept in either `charter.toml` or `charter.local.toml`, as the person chooses (SE-18). Only a
   *  key both files' readers read is: never one only the Local file may hold (profiles, the
   *  environment passed to chats, the default profile), nor one only the Shared file may. */
  movable?: boolean;
  key: SettingsStep[];
  /** `text` is one line, `choice` a closed set, `lines` one entry per line, and `colour` a
   *  closed set whose `custom` pick is a `#rrggbb` of the operator's own (a workspace's). */
  kind: "text" | "choice" | "lines" | "colour";
  choices?: readonly string[];
  /** What a `choice`'s empty option says; none is offered without it. */
  unset?: string;
  /** What a `choice` shows for each of its values, when that is not the value itself. */
  labels?: Readonly<Record<string, string>>;
  read: (file: Shown) => string;
  /** The edits `draft` makes to `file`, the file it was typed over. */
  edits: (draft: string, file: Shown) => SettingsEdit[];
  /**
   * **Turned on here and never taken back here** — the sandbox (ADR 0067, D-SE17g): no Undo is
   * offered for it, since what it would write back is "not set" or a value read as less
   * confining, and its empty option is not drawn while a write of it is pending.
   */
  oneWay?: boolean;
  /**
   * **A picker over one of the project's collections** (ST-1, #1225): `choices` are what the
   * project has — undefined until the core has listed them — and the picker offers New… for it.
   */
  names?: Entry;
  /** The core's sentence about this key as the file stands — a value that names nothing — said
   *  beside the setting until it is replaced. */
  standing?: string;
};

/**
 * **A setting kept in a store of its own** (SE-16): `useControl` is a hook — the row calls it
 * once per render, always the same one for a given setting — answering how to draw the control
 * (handed the row's ids, which tie it to the label and the help) and, when the value can go
 * back, the reset. That is how a value in a store of its own (a text size, your editor) reads it
 * and redraws when it changes elsewhere.
 */
export type LiveSetting = Named & {
  useControl: () => {
    control: (ids: RowIds) => ReactNode;
    reset?: Reset;
    /** The control is a group of controls (a radio group): see `SettingRow`'s `grouped`. */
    grouped?: boolean;
    /** Why the last thing done here was refused, in the core's words: said beside it. */
    error?: readonly string[];
    /** Puts back what the last thing done here changed, while that is the one Undo on offer. */
    undo?: () => void;
  };
};

export type Setting = FileSetting | LiveSetting;

/** Whether `setting` is declared over a file, rather than drawn by a hook of its own. */
export function inAFile(setting: Setting): setting is FileSetting {
  return "edits" in setting;
}

/** A level the switcher can offer, and the groups it holds (`CONTEXT.md`, **Level**). */
export type Level = "you" | "project" | "workspace" | "persona";

/** What the switcher calls each level, in its order. */
export const LEVELS: readonly { id: Level; label: string }[] = [
  { id: "you", label: "You" },
  { id: "project", label: "Project" },
  { id: "workspace", label: "Workspace" },
  { id: "persona", label: "Persona" },
];
