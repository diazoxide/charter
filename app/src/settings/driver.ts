import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { extensionsChanged } from "../extensionsOn";
import { SETTINGS, usePlaneChanged } from "../planeChanged";
import { projectThemeChanged } from "../projectTheme";
import type {
  PlaneId,
  ProjectExtensions,
  ProjectTheme,
  SettingsEdit,
  SettingsStep,
  SettingsWhich,
} from "../bindings";
import { valueAt, type Control, type Group, type Saving, type Shown } from "./fileControls";
import type { FileSetting, SettingsFileId } from "./groups";

/**
 * **The settings driver** (SE-17's, generalised for SE-20): what every level whose settings are
 * kept in files shares — the Project level's `charter.toml` and `charter.local.toml`, the
 * Workspace level's `workspace.json`. A level says how its files are read, how one is written
 * and what it asks again once one changed; the driver does the rest, the same way for each:
 *
 * - **each change is written as its own keys**, through the core, one write at a time and each
 *   against the file as the last one left it; a write that would change nothing is not sent;
 * - **the last change can be undone**, by writing back exactly the keys it wrote (taking out a
 *   key that was absent), unless a value it would write back is one only the file can hold;
 * - **a refused write is kept by its setting**, in the core's words, and the files are read
 *   again so what is shown is what is on disk;
 * - **a file changed outside the tab** — by hand, by a chat, by another page — is read again, in
 *   the queue, so the next write is made against it rather than refused as changed on disk;
 * - **a value can be reset** (SE-18): its keys are taken out of the file it comes from, so the
 *   value beneath shows through — undone like any change;
 * - **a value can be moved** between a level's two files (SE-18, the Project level's "Shared /
 *   Only on this machine"), both files or neither, through the core. A move is made on a button
 *   and has no Undo; it ends the last change's Undo, whose keys it may have moved;
 * - **a file's whole text** (Edit as TOML, SE-19) is written in the same queue, against the
 *   text its edit began from; once written it is what the next change is made against, and the
 *   last change's Undo is gone, so an Undo never writes a key back over the text;
 * - **an entry of a collection is added or removed** (ST-3, the collection write seam) through the
 *   core's one function for that collection, which checks the whole entry and answers by field, by
 *   what uses the entry, and for the whole file ({@link EntryRefusal}). **It is sent against the
 *   text the entries were drawn from** ({@link EntryOp}'s `base`, taken when the person pressed
 *   the button), never the text as it stands when the write's turn comes, so a remove queued
 *   behind another write is refused rather than made to the entry now in its place. **Its Undo**
 *   (D-ST3-i, as amended) is what the level answers ({@link EntryUndo}): an add is undone by the
 *   inverse operation through the same core function — a remove of what was added, refused for
 *   what uses it — and a remove by writing the file's text back exactly as it was, against the
 *   text the remove left, so the entry is back where it was, as it was spelled, with its comment.
 *
 * **Nothing here loosens what the level says may not be loosened**: its `mayUndo` is asked of
 * every Undo, every reset and every move, and one it refuses is not offered (D-SE17g).
 */

/**
 * **One add or remove in a collection** (ST-3): `collection` is the level's name for it
 * (`forges`); an add carries each field of the Add form as typed, by the entry's own key, and a
 * remove the entry's identity, which the core gave it. `base` is the text of the collection's
 * file the entries were drawn from (`null`: not there), which the core checks it against.
 */
export type EntryOp =
  | { collection: string; base: string | null; add: Readonly<Record<string, string>> }
  | { collection: string; base: string | null; remove: string }
  /** A rename of the entry `rename` to `to` (ST-4, V91k): refused, naming them, while
   *  something uses the entry. Its Undo is the rename back. */
  | { collection: string; base: string | null; rename: string; to: string }
  /** A confirm of the entry `confirm` (#1341): your own sandbox host, which reaches nothing
   *  until Settings confirms it on this machine. It has no Undo. */
  | { collection: string; base: string | null; confirm: string };

/** The entry an op is about, when it is about one already there: a remove's or a rename's. */
export function entryOf(op: EntryOp): string | undefined {
  return "remove" in op
    ? op.remove
    : "rename" in op
      ? op.rename
      : "confirm" in op
        ? op.confirm
        : undefined;
}

/** Something that uses an entry, which stops its removal, and the group it is changed in. */
export type EntryReferrer = { what: string; group: string | null };

/** Why an add or a remove wrote nothing: by field, by what uses the entry, and for the file. */
export type EntryRefusal = {
  fields: Readonly<Record<string, readonly string[]>>;
  referrers: readonly EntryReferrer[];
  reasons: readonly string[];
};

/**
 * **What undoes an add or a remove** (D-ST3-i, as amended): an operation through the core (the
 * Undo of an add: a remove of what was added, with its reference check), or the whole text of a
 * file written back over `base`, the text the change left (the Undo of a remove: exact — place,
 * spelling and comments — and refused if the file moved since; putting an entry back only
 * declares again, so it asks no reference check). A collection with no text to put back (the
 * machine store) undoes with operations only.
 */
export type EntryUndo =
  EntryOp | { restore: { file: SettingsFileId; base: string | null; text: string } };

/**
 * What an add or a remove answered: the level, the file it wrote, what was done (said beside
 * its Undo) and what undoes it; or why nothing was written.
 */
export type EntryWrote<T> =
  | { saved: T; file: SettingsFileId; said: string; undo: EntryUndo | undefined }
  | { refused: EntryRefusal };

/** A collection's last refused Remove, Rename or Undo: the entry it was for (none for an
 *  Undo), and which it was. */
export type EntryRefused = {
  collection: string;
  entry: string | undefined;
  /** What was refused, in the words the refusal is said in: `removed` or `renamed`. */
  verb?: "removed" | "renamed" | "confirmed";
  refusal: EntryRefusal;
};

/** A level's settings files, by which file each is. */
export type Files = Partial<Record<SettingsFileId, Shown>>;

/** What writing a level's file answered: the level as it now stands, or why nothing was written. */
export type Wrote<T> = { saved: T } | { refused: string[] };

/** What the driver answers once the level has been read: its files, and what to do with a setting. */
export type Driven<T> = {
  state: "read";
  /** The level as the last read or write left it. */
  now: T;
  files: Files;
  /** A value being written, by setting: shown until the write settles. */
  pending: Readonly<Record<string, string>>;
  /** Why a setting's last write was refused, by setting. */
  refused: Readonly<Record<string, readonly string[]>>;
  /** The setting the last change was made to, which offers its Undo — or the collection's id,
   *  after an add or a remove in it. */
  undoable: string | undefined;
  /** What the last add or remove did, said beside its Undo. */
  undoSaid: string | undefined;
  /** Adds, removes or renames an entry for the collection `id` (its group's id); answers why
   *  nothing was written, or `undefined` once it was. */
  entry: (id: string, op: EntryOp) => Promise<EntryRefusal | undefined>;
  /** The last refused Remove, Rename or Undo in a collection, until the next write or Undo. */
  entryRefused: EntryRefused | undefined;
  /** Writes `draft` to `file`: the file the value comes from, else the one picked for it. */
  write: (setting: FileSetting, draft: string, file?: SettingsFileId) => void;
  /** Writes `text` as the whole of `which`, whose text was `base` when the edit began
   *  (`null`: not there yet); answers why nothing was written, or `undefined` once it was. */
  writeRaw: (
    which: SettingsFileId,
    base: string | null,
    text: string,
  ) => Promise<readonly string[] | undefined>;
  undo: () => void;
  /** Takes the setting's keys out of the file its value comes from. */
  reset: (setting: FileSetting) => void;
  /** Moves the setting's value into `to`, out of the other file. */
  move: (setting: FileSetting, to: SettingsWhich) => void;
  /** Whether `edits` may be written by an Undo, a reset or a move (the level's `mayUndo`). */
  mayChange: (edits: readonly SettingsEdit[]) => boolean;
  /** Reads the files again, in the queue: after something outside the files changed them. */
  reread: () => void;
};

export type Driver<T> = { state: "reading" } | { state: "trouble"; trouble: string } | Driven<T>;

/** How a level's files are read and written. */
export type Level<T> = {
  plane: PlaneId;
  /** Reads the level's files. */
  read: () => Promise<{ ok: T } | { trouble: string }>;
  /** The files in what was read. */
  files: (now: T) => Files;
  /** Writes `edits` to `which`, whose text was `base` (`null`: not there yet). */
  save: (
    which: SettingsFileId,
    base: string | null,
    edits: SettingsEdit[],
    now: T,
  ) => Promise<Wrote<T>>;
  /** Asks again what is in force, once the files were read or written. */
  inForce: () => void;
  /**
   * Whether `back` may be written by an Undo, a reset or a move (the keys a move takes out of its
   * file); every one may unless it says no.
   */
  mayUndo?: (back: readonly SettingsEdit[]) => boolean;
  /** Writes `text` as the whole of `which`, whose text was `base`: a level whose files are
   *  edited as text (SE-19) says how. */
  saveRaw?: (which: SettingsFileId, base: string | null, text: string, now: T) => Promise<Wrote<T>>;
  /** Moves the values at `paths` into `to`, out of the other file: both or neither. A level
   *  with one file has none. */
  move?: (to: SettingsWhich, paths: SettingsStep[][], now: T) => Promise<Wrote<T>>;
  /** Adds or removes a collection's entry through the core's function for it (ST-3). A level
   *  with no collection has none. */
  entry?: (op: EntryOp, now: T) => Promise<EntryWrote<T>>;
};

/**
 * The last change made here: the setting, its file, and what puts it back — the edits a key's
 * change is undone with, or the operation that undoes an add or a remove in a collection.
 */
type Change =
  | { setting: string; file: SettingsFileId; back: SettingsEdit[] }
  | { setting: string; undo: EntryUndo; said: string };

/**
 * **The file a setting's value comes from** at this level, or `undefined` when no file here
 * holds it. A movable setting's is `charter.local.toml` where that holds one: Local overrides
 * Shared key by key.
 */
export function heldIn(setting: FileSetting, files: Files): SettingsFileId | undefined {
  const holds = (which: SettingsFileId) => {
    const file = files[which];
    return file !== undefined && keysOf(setting, file).length > 0;
  };
  if (setting.movable) {
    if (holds("local")) return "local";
    if (holds("shared")) return "shared";
    return undefined;
  }
  return holds(setting.file) ? setting.file : undefined;
}

/** Every key `file` holds at or under the setting's key: what a reset takes out, and a move
 *  carries. A profile's environment is one key per variable. */
export function keysOf(setting: FileSetting, file: Shown): SettingsStep[][] {
  const at = JSON.stringify(setting.key).slice(0, -1);
  return file.fields
    .map((field) => field.path)
    .filter((path) => {
      const it = JSON.stringify(path);
      return it === `${at}]` || it.startsWith(`${at},`);
    });
}

/** `record` without `id`'s entry. */
function without<T>(record: Readonly<Record<string, T>>, id: string): Record<string, T> {
  const rest = { ...record };
  Reflect.deleteProperty(rest, id);
  return rest;
}

/**
 * **The driver for one level**, keyed by `target` (the project, or the project and workspace):
 * a new target starts over. The level's functions are read at the time they are used, so a
 * caller need not hold them still.
 */
export function useSettingsDriver<T>(target: string, level: Level<T>): Driver<T> {
  const [now, setNow] = useState<{ ok: T } | { trouble: string }>();
  const [pending, setPending] = useState<Record<string, string>>({});
  const [refused, setRefused] = useState<Record<string, readonly string[]>>({});
  const [last, setLast] = useState<Change>();
  const [entryRefused, setEntryRefused] = useState<EntryRefused>();
  const latest = useRef(level);
  // The level's functions as of the last render, before any effect below uses them.
  useLayoutEffect(() => {
    latest.current = level;
  });
  /** The level as the last read or write left it, for the next write in the queue. */
  const held = useRef<T>(undefined);
  const queue = useRef<Promise<void>>(Promise.resolve());
  /** The newest read out: an answer to an older one is dropped. */
  const reading = useRef(0);
  const { plane } = level;

  const enqueue = useCallback((work: () => Promise<void>) => {
    queue.current = queue.current.then(work, work);
  }, []);

  const readFiles = useCallback((): Promise<void> => {
    const mine = ++reading.current;
    return latest.current
      .read()
      .catch((err: unknown) => ({ trouble: String(err) }))
      .then((said) => {
        if (reading.current !== mine) return;
        if ("ok" in said) held.current = said.ok;
        setNow(said);
        latest.current.inForce();
      });
    // The target is what a read is of.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target]);

  useEffect(() => {
    void readFiles();
  }, [readFiles]);
  const changes = usePlaneChanged([plane], SETTINGS);
  useEffect(() => {
    if (changes > 0) enqueue(readFiles);
  }, [changes, enqueue, readFiles]);

  /** What a write left the level as: what the next one is made against, and what is shown. */
  const took = useCallback(
    (saved: T) => {
      held.current = saved;
      setNow({ ok: saved });
      latest.current.inForce();
      // The window keeps of its surveyed panels, views and themes what is on here, and the
      // theme and colour it draws while this is in front.
      extensionsChanged(plane);
      projectThemeChanged(plane);
    },
    [plane],
  );

  /** Writes `edits` to `which` for `id`; answers whether the core wrote them. */
  const send = useCallback(
    async (id: string, which: SettingsFileId, edits: SettingsEdit[]): Promise<boolean> => {
      const was = held.current;
      const file = was === undefined ? undefined : latest.current.files(was)[which];
      if (was === undefined || file === undefined) return false;
      const said = await latest.current
        .save(which, file.exists ? file.text : null, edits, was)
        .catch((err: unknown): Wrote<T> => ({ refused: [String(err)] }));
      if ("refused" in said) {
        setRefused((before) => ({ ...before, [id]: said.refused }));
        // What is shown is what is on disk, which may not be what this tab last read.
        await readFiles();
        return false;
      }
      took(said.saved);
      setRefused((before) => without(before, id));
      return true;
    },
    [readFiles, took],
  );

  const mayChange = useCallback(
    (edits: readonly SettingsEdit[]) => latest.current.mayUndo?.(edits) ?? true,
    [],
  );

  /** Writes `edits` to `which` for `setting`, keeping what puts them back as the last change. */
  const change = useCallback(
    async (setting: FileSetting, which: SettingsFileId, file: Shown, edits: SettingsEdit[]) => {
      const back = edits.map((edit) => ({ path: edit.path, value: valueAt(file, edit.path) }));
      const undo = back.map((one) => ({ path: one.path, value: one.value ?? null }));
      // A value only the file itself can hold (a date, a float) cannot be written back.
      const undoable =
        !setting.oneWay && back.every((one) => one.value?.kind !== "other") && mayChange(undo);
      if (await send(setting.id, which, edits))
        setLast(undoable ? { setting: setting.id, file: which, back: undo } : undefined);
    },
    [mayChange, send],
  );

  const write = useCallback(
    (setting: FileSetting, draft: string, to?: SettingsFileId) => {
      // Any write ends what a collection's last refusal said.
      setEntryRefused(undefined);
      setPending((was) => ({ ...was, [setting.id]: draft }));
      enqueue(async () => {
        const now = held.current && latest.current.files(held.current);
        // Where the value is, at its turn; where it was picked to go, or its own file, if nowhere.
        const which = (now && heldIn(setting, now)) ?? to ?? setting.file;
        const file = now?.[which];
        if (file && setting.read(file) !== draft)
          await change(setting, which, file, setting.edits(draft, file));
        setPending((was) => without(was, setting.id));
      });
    },
    [change, enqueue],
  );

  const reset = useCallback(
    (setting: FileSetting) => {
      setPending((was) => ({ ...was, [setting.id]: "" }));
      // Any write ends what a collection's last refusal said.
      setEntryRefused(undefined);
      enqueue(async () => {
        const now = held.current && latest.current.files(held.current);
        const which = now && heldIn(setting, now);
        const file = which && now[which];
        const edits = file
          ? keysOf(setting, file).map((path): SettingsEdit => ({ path, value: null }))
          : [];
        if (which && file && edits.length > 0 && !setting.oneWay && mayChange(edits))
          await change(setting, which, file, edits);
        setPending((was) => without(was, setting.id));
      });
    },
    [change, enqueue, mayChange],
  );

  const move = useCallback(
    (setting: FileSetting, to: SettingsWhich) => {
      // Any write ends what a collection's last refusal said.
      setEntryRefused(undefined);
      enqueue(async () => {
        const was = held.current;
        const now = was && latest.current.files(was);
        const moveIt = latest.current.move;
        const from = now && heldIn(setting, now);
        const file = from && now[from];
        if (!was || !moveIt || !file || from === to) return;
        const paths = keysOf(setting, file);
        const out = paths.map((path): SettingsEdit => ({ path, value: null }));
        if (setting.oneWay || !mayChange(out)) return;
        setPending((before) => ({ ...before, [setting.id]: setting.read(file) }));
        const said = await moveIt(to, paths, was).catch((err: unknown): Wrote<T> => ({
          refused: [String(err)],
        }));
        if ("refused" in said) {
          setRefused((before) => ({ ...before, [setting.id]: said.refused }));
          await readFiles();
        } else {
          took(said.saved);
          setRefused((before) => without(before, setting.id));
          // The keys the last change wrote may be the ones moved: its Undo would write them back
          // into the file they left.
          setLast(undefined);
        }
        setPending((before) => without(before, setting.id));
      });
    },
    [enqueue, mayChange, readFiles, took],
  );

  /** Runs `op` for the collection `id`; answers why nothing was written. A refusal of a
   *  remove or an Undo is kept as the collection's, by the entry it was for. */
  const sendEntry = useCallback(
    async (id: string, op: EntryOp, undoing: boolean): Promise<EntryRefusal | undefined> => {
      const was = held.current;
      const write = latest.current.entry;
      if (was === undefined || write === undefined)
        return { fields: {}, referrers: [], reasons: ["Nothing here can be added to."] };
      const wrote = await write(op, was).catch((err: unknown): EntryWrote<T> => ({
        refused: { fields: {}, referrers: [], reasons: [String(err)] },
      }));
      if ("refused" in wrote) {
        if (undoing || entryOf(op) !== undefined)
          setEntryRefused({
            collection: id,
            entry: undoing ? undefined : entryOf(op),
            verb: "rename" in op ? "renamed" : "confirm" in op ? "confirmed" : "removed",
            refusal: wrote.refused,
          });
        // What is shown is what is on disk: a refusal may be the file having moved.
        await readFiles();
        return wrote.refused;
      }
      took(wrote.saved);
      setRefused((before) => without(before, id));
      // One level: an Undo is not itself undone.
      // A write with no Undo (a confirm) leaves none to offer.
      setLast(
        undoing || wrote.undo === undefined
          ? undefined
          : { setting: id, undo: wrote.undo, said: wrote.said },
      );
      return undefined;
    },
    [readFiles, took],
  );

  /** Writes a file's earlier text back for the collection `id`: the Undo of a remove. */
  const restore = useCallback(
    async (id: string, put: { file: SettingsFileId; base: string | null; text: string }) => {
      const was = held.current;
      const save = latest.current.saveRaw;
      if (was === undefined || save === undefined) return;
      const said = await save(put.file, put.base, put.text, was).catch(
        (err: unknown): Wrote<T> => ({ refused: [String(err)] }),
      );
      if ("refused" in said) {
        setEntryRefused({
          collection: id,
          entry: undefined,
          refusal: { fields: {}, referrers: [], reasons: said.refused },
        });
        await readFiles();
        return;
      }
      took(said.saved);
    },
    [readFiles, took],
  );

  const undo = useCallback(() => {
    const change = last;
    if (!change) return;
    setLast(undefined);
    setEntryRefused(undefined);
    enqueue(async () => {
      if (!("undo" in change)) await send(change.setting, change.file, change.back);
      else if ("restore" in change.undo) await restore(change.setting, change.undo.restore);
      else await sendEntry(change.setting, change.undo, true);
    });
  }, [enqueue, last, restore, send, sendEntry]);

  const entry = useCallback(
    (id: string, op: EntryOp) =>
      new Promise<EntryRefusal | undefined>((answer) => {
        setEntryRefused(undefined);
        enqueue(async () => answer(await sendEntry(id, op, false)));
      }),
    [enqueue, sendEntry],
  );

  const writeRaw = useCallback(
    (which: SettingsFileId, base: string | null, text: string) =>
      new Promise<readonly string[] | undefined>((answer) => {
        // Any write ends what a collection's last refusal said.
        setEntryRefused(undefined);
        enqueue(async () => {
          const was = held.current;
          const save = latest.current.saveRaw;
          if (was === undefined || save === undefined) {
            answer(["These settings are not kept as text, so nothing was saved."]);
            return;
          }
          const said = await save(which, base, text, was).catch((err: unknown): Wrote<T> => ({
            refused: [String(err)],
          }));
          if ("refused" in said) {
            // What is shown is what is on disk: a refusal may be the file having moved.
            await readFiles();
            answer(said.refused);
            return;
          }
          took(said.saved);
          // The keys an Undo would write back are the text's now.
          setLast(undefined);
          answer(undefined);
        });
      }),
    [enqueue, readFiles, took],
  );

  const reread = useCallback(() => enqueue(readFiles), [enqueue, readFiles]);

  if (now === undefined) return { state: "reading" };
  if ("trouble" in now) return { state: "trouble", trouble: now.trouble };
  return {
    state: "read",
    now: now.ok,
    files: level.files(now.ok),
    pending,
    refused,
    undoable: last?.setting,
    undoSaid: last && "said" in last ? last.said : undefined,
    entry,
    entryRefused,
    write,
    writeRaw,
    undo,
    reset,
    move,
    mayChange,
    reread,
  };
}

/** What the core says is in force at a level, which its groups are declared from. */
export type InForce = {
  extensions: ProjectExtensions;
  theme: ProjectTheme | undefined;
  saving?: Saving;
};

/** An old page's group asked about `file`: its controls, and what it says. */
export function asked(group: Group, file: Shown, { extensions, theme, saving }: InForce) {
  return {
    controls: group.controls(file, extensions.extensions, theme, saving),
    notes: [
      ...(group.note ? [group.note] : []),
      group.leftOut?.(extensions, theme, saving) ?? null,
      ...(group.notes?.(file, extensions.extensions, theme, saving) ?? []),
    ].filter((one): one is string => one !== null),
  };
}

/**
 * One of the old pages' controls as a setting of `group`, kept in `file`, whose help ends on
 * `kept`: where it is kept, and who sees it.
 */
export function fileSetting(
  group: string,
  file: SettingsFileId,
  control: Control,
  kept: string,
): FileSetting {
  // A control's id is the key it is at, as the old page keys its drafts by it.
  const path = JSON.parse(control.id) as SettingsStep[];
  const dotted = path.map((step) => ("key" in step ? step.key : String(step.index))).join(".");
  return {
    id: `${group}.${file === "local" ? "local." : ""}${dotted}`,
    label: control.label,
    help: [control.hint, kept].filter(Boolean).join(" "),
    file,
    key: path,
    kind: control.kind,
    choices: control.choices,
    unset: control.unset,
    labels: control.labels,
    options: control.options,
    status: control.status,
    turnOn: control.turnOn,
    read: control.read,
    edits: control.edits,
    names: control.names,
    resets: control.resets,
  };
}
