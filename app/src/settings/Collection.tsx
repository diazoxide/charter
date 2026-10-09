import { useEffect, useId, useRef, useState, type ReactNode, type RefObject } from "react";
import { Choice, Field, SettingActions, SettingRow } from "./components";
import type { Driven, EntryRefused, EntryRefusal } from "./driver";
import type { Collection, CollectionEntry, EntryField, Setting } from "./groups";

/**
 * **A collection, drawn** (ST-3, V91e–g): each entry a heading with its Remove over its own rows,
 * then Add, which opens an inline form of setting rows. Every write is the driver's `entry`
 * (`driver.ts`), which asks the core's one function for the collection against the text the
 * entries were drawn from (`collection.base`); nothing is checked here, and every label is the
 * core's. What the core refuses is said where it belongs: a field's refusal under that field, a
 * Remove's referrers under the entry (by its identity) with a link to the group each is changed
 * in, an Undo's at the head of the collection. The last add or remove offers its Undo at the
 * head, saying what it did.
 *
 * **An entry with a page of its own** (ST-4, V91e: a harness profile) is drawn on its
 * collection's group as a heading that opens the page, with its Remove; the page draws that entry
 * alone, its rows and its Rename, with no Add. **Rename** (V91k) opens a one-field form in place;
 * a name the core refuses is said under it, and what uses the entry under the entry, as for a
 * Remove.
 */
export function CollectionView({
  id,
  collection,
  settings,
  driver,
  row,
  onGo,
  reachable,
  adding,
}: {
  /** The group's id: what the driver keeps the collection's Undo and refusals by. */
  id: string;
  collection: Collection;
  /** The group's settings the filter left. */
  settings: readonly Setting[];
  driver: Driven<unknown>;
  /** Draws one of the group's settings as a row. */
  row: (setting: Setting) => ReactNode;
  /** Opens a group: a referrer's link. */
  onGo: (group: string) => void;
  /** Whether {@link onGo} can open `group` from here: a link that cannot is not drawn. */
  reachable: (group: string) => boolean;
  /** Opens the Add form, as a picker's New… asked (ST-4): `then` is handed the name of what
   *  was added. A new `ask` opens it again. */
  adding?: Asked;
}) {
  const { noun } = collection;
  /** What the driver keeps this collection's Undo and refusals by. */
  const key = collection.home ?? id;
  const [removing, setRemoving] = useState<string>();
  const [renaming, setRenaming] = useState<string>();
  const addButton = useRef<HTMLButtonElement>(null);
  const whole = useRef<HTMLDivElement>(null);
  /** Where the focus goes once what had it is gone: Add, or the collection itself. */
  const settle = () => (addButton.current ?? whole.current)?.focus();
  const undoSaid = driver.undoable === key ? driver.undoSaid : undefined;
  const refused = driver.entryRefused?.collection === key ? driver.entryRefused : undefined;
  // An Undo's refusal, or a Remove's whose entry is no longer drawn as it was, is the head's.
  const atHead =
    refused !== undefined &&
    (refused.entry === undefined || !collection.entries.some((one) => one.id === refused.entry));
  const ofEntry = (entry: CollectionEntry) =>
    settings.filter((one) => entry.settings.includes(one.id));
  const mine = new Set(collection.entries.flatMap((entry) => entry.settings));

  /** Confirms the entry (#1341): your own sandbox host, which reaches nothing until it is. */
  const confirm = async (entry: CollectionEntry) => {
    setRemoving(entry.id);
    await driver.entry(key, {
      collection: collection.name,
      base: collection.base,
      confirm: entry.id,
    });
    setRemoving(undefined);
    settle();
  };

  const remove = async (entry: CollectionEntry) => {
    setRemoving(entry.id);
    // Sent against the text this entry was drawn from, whatever is queued before it.
    const refusal = await driver.entry(key, {
      collection: collection.name,
      base: collection.base,
      remove: entry.id,
    });
    setRemoving(undefined);
    if (refusal) return;
    // The entry is gone, and its page with it: the person lands where its Undo is.
    if (key !== id) onGo(key);
    // The focus goes to Add, which is always there.
    else settle();
  };

  return (
    <div className="ui-collection" ref={whole} tabIndex={-1}>
      {settings.filter((one) => !mine.has(one.id)).map(row)}
      {(undoSaid !== undefined || atHead) && (
        <div className="ui-collection-done" role="status">
          {undoSaid !== undefined && (
            <>
              <span>{undoSaid}</span>
              <button
                type="button"
                className="ui-setting-reset"
                // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
                tabIndex={0}
                onClick={() => {
                  driver.undo();
                  // On an entry's page, the Undo may take the page away (a rename back, an add
                  // undone): the person lands on the collection's group, where it is said.
                  if (key !== id) onGo(key);
                  else settle();
                }}
              >
                Undo
              </button>
            </>
          )}
          {refused && atHead && (
            <Refused noun={noun} refused={refused} onGo={onGo} reachable={reachable} />
          )}
        </div>
      )}
      {collection.entries.map((entry) => (
        <div key={entry.id} className="ui-collection-entry" role="group" aria-label={entry.label}>
          <div className="ui-collection-head">
            <h4>{entry.label}</h4>
            {entry.page !== undefined && reachable(entry.page) && (
              <button
                type="button"
                className="ui-setting-reset"
                tabIndex={0}
                aria-label={`Open ${entry.label}`}
                onClick={() => onGo(entry.page ?? "")}
              >
                Open
              </button>
            )}
            {collection.renames && (
              <button
                type="button"
                className="ui-setting-reset"
                tabIndex={0}
                disabled={removing !== undefined || renaming !== undefined}
                aria-label={`Rename ${entry.label}`}
                onClick={() => setRenaming(entry.id)}
              >
                Rename
              </button>
            )}
            {entry.confirm && (
              <button
                type="button"
                className="ui-setting-reset"
                tabIndex={0}
                disabled={removing !== undefined}
                aria-label={`Confirm ${entry.label}`}
                onClick={() => void confirm(entry)}
              >
                Confirm
              </button>
            )}
            <button
              type="button"
              className="ui-setting-reset"
              tabIndex={0}
              disabled={removing !== undefined}
              aria-label={`Remove ${entry.label}`}
              onClick={() => void remove(entry)}
            >
              Remove
            </button>
          </div>
          {renaming === entry.id && (
            <RenameForm
              id={key}
              collection={collection}
              entry={entry}
              driver={driver}
              onDone={(to) => {
                setRenaming(undefined);
                if (to !== undefined && collection.pageOf) onGo(collection.pageOf(to));
                else whole.current?.focus();
              }}
            />
          )}
          {refused?.entry === entry.id && (
            <Refused noun={noun} refused={refused} onGo={onGo} reachable={reachable} />
          )}
          {ofEntry(entry).map(row)}
        </div>
      ))}
      {collection.adds !== false && (
        <AddForm
          id={key}
          collection={collection}
          driver={driver}
          adding={addButton}
          whole={whole}
          asked={adding}
        />
      )}
    </div>
  );
}

/** A picker's New… asking for the Add form (ST-4): `then` is handed the added entry's name. */
export type Asked = { ask: number; then: (name: string) => void };

/** What a Remove, a Rename or an Undo was refused for: who uses the entry, each with its link,
 *  and why. A Rename refused only by field says so under its field, and nothing here. */
function Refused({
  noun,
  refused,
  onGo,
  reachable,
}: {
  noun: string;
  refused: EntryRefused;
  onGo: (group: string) => void;
  reachable: (group: string) => boolean;
}) {
  const { refusal } = refused;
  const users = refusal.referrers.length;
  if (users + refusal.reasons.length === 0) return null;
  return (
    <div className="ui-setting-error" role="alert">
      {users > 0 && (
        <p>{`This ${noun} is not ${refused.verb ?? "removed"} while ${users === 1 ? "this uses" : "these use"} it:`}</p>
      )}
      {refusal.referrers.map((one, at) => (
        <p key={at}>
          {one.what}
          {one.group !== null && reachable(one.group) && (
            <>
              {" "}
              <button
                type="button"
                className="ui-setting-reset"
                tabIndex={0}
                onClick={() => onGo(one.group ?? "")}
              >
                Fix it in Settings
              </button>
            </>
          )}
        </p>
      ))}
      {refusal.reasons.map((why, at) => (
        <p key={`reason-${at}`}>{why}</p>
      ))}
    </div>
  );
}

/** The fields' values as the form opens. */
function initial(fields: readonly EntryField[]): Record<string, string> {
  return Object.fromEntries(fields.map((one) => [one.field, one.initial ?? ""]));
}

/** What is said under a field: the core's refusal of the Add pressed, else its check of the
 *  text as typed (#1405). */
function fieldSaid(
  refused: readonly string[] | undefined,
  checked: string | undefined,
): readonly string[] | undefined {
  if (refused !== undefined && refused.length > 0) return refused;
  return checked === undefined ? undefined : [checked];
}

/**
 * **Add**: a button, which opens the form in place — one setting row per field — with Add and
 * Cancel under it. The form is closed only once the core wrote the entry; a refusal keeps it
 * open with what was typed, each field's refusal under its field.
 */
function AddForm({
  id,
  collection,
  driver,
  adding,
  whole,
  asked,
}: {
  id: string;
  collection: Collection;
  driver: Driven<unknown>;
  adding: RefObject<HTMLButtonElement | null>;
  whole: RefObject<HTMLDivElement | null>;
  asked?: Asked;
}) {
  const { noun, fields } = collection;
  const [values, setValues] = useState<Record<string, string> | undefined>(() =>
    asked === undefined ? undefined : initial(fields),
  );
  // A New… pressed again while the page is drawn opens the form again.
  const [answered, setAnswered] = useState(asked?.ask);
  if (asked !== undefined && asked.ask !== answered) {
    setAnswered(asked.ask);
    setValues(initial(fields));
  }
  const [refusal, setRefusal] = useState<EntryRefusal>();
  // What a field's own check said of what is in it now (#1405), and the text each was asked
  // for, so an answer to text typed over since is dropped.
  const [checked, setChecked] = useState<Readonly<Record<string, string>>>({});
  const asking = useRef<Record<string, string>>({});
  const [sending, setSending] = useState(false);
  const form = useId();
  const open = values !== undefined;
  // The first field takes the focus as the form is drawn; Add takes it back once it closes, or
  // the collection itself when Add is not there.
  const opened = useRef(false);
  useEffect(() => {
    if (open)
      document.getElementById(form)?.querySelector<HTMLElement>("input, select, textarea")?.focus();
    else if (opened.current) (adding.current ?? whole.current)?.focus();
    opened.current = open;
  }, [open, form, adding, whole]);
  const close = () => {
    setValues(undefined);
    setRefusal(undefined);
    setChecked({});
    asking.current = {};
  };
  if (values === undefined)
    return (
      <div className="ui-collection-add">
        <button
          ref={adding}
          type="button"
          className="ui-setting-reset"
          tabIndex={0}
          onClick={() => setValues(initial(fields))}
        >
          {`Add ${noun}`}
        </button>
      </div>
    );
  const set = (one: EntryField) => (to: string) => {
    setValues((was) => (was === undefined ? was : { ...was, [one.field]: to }));
    const check = one.check;
    if (check === undefined) return;
    // The field is checked anew, so what was said of the text it held goes.
    asking.current[one.field] = to;
    setRefusal((was) =>
      was === undefined ? was : { ...was, fields: { ...was.fields, [one.field]: [] } },
    );
    const said = (why: string | null) =>
      setChecked((was) => {
        const rest = Object.fromEntries(
          Object.entries(was).filter(([field]) => field !== one.field),
        );
        return why == null ? rest : { ...rest, [one.field]: why };
      });
    // An empty field is not refused while it is typed in: Add says what it needs.
    if (to.trim() === "") return said(null);
    check(to).then(
      (why) => {
        if (asking.current[one.field] === to) said(why);
      },
      () => undefined,
    );
  };
  const send = async () => {
    setSending(true);
    const said = await driver.entry(id, {
      collection: collection.name,
      base: collection.base,
      add: values,
    });
    setSending(false);
    if (said) setRefusal(said);
    else {
      close();
      // What New… asked for is picked once it is there.
      if (asked !== undefined) asked.then((values.name ?? "").trim());
    }
  };
  return (
    <form
      id={form}
      className="ui-collection-form"
      aria-label={`New ${noun}`}
      onSubmit={(event) => {
        event.preventDefault();
        void send();
      }}
      onKeyDown={(event) => {
        if (event.key !== "Escape") return;
        event.preventDefault();
        event.stopPropagation();
        close();
      }}
    >
      {fields.map((one) => (
        <SettingRow
          key={one.field}
          label={one.label}
          help={one.help}
          error={fieldSaid(refusal?.fields[one.field], checked[one.field])}
          control={(ids) =>
            one.kind === "choice" ? (
              <Choice
                kind="select"
                ids={ids}
                options={(one.choices ?? []).map((choice) => ({ value: choice, label: choice }))}
                value={values[one.field] ?? ""}
                onValueChange={set(one)}
              />
            ) : (
              <Field
                kind={one.kind === "lines" ? "list" : "text"}
                ids={ids}
                value={values[one.field] ?? ""}
                onChange={set(one)}
              />
            )
          }
        />
      ))}
      {refusal && refusal.reasons.length > 0 && (
        <div className="ui-setting-error" role="alert">
          {refusal.reasons.map((why, at) => (
            <p key={at}>{why}</p>
          ))}
        </div>
      )}
      <SettingActions>
        <button type="submit" tabIndex={0} disabled={sending}>
          {`Add ${noun}`}
        </button>
        <button type="button" tabIndex={0} onClick={close}>
          Cancel
        </button>
      </SettingActions>
    </form>
  );
}

/**
 * **Rename** (ST-4, V91k): the entry's name in one field, with Rename and Cancel under it. The
 * core decides: a name it refuses is said under the field and the form stays open; what uses
 * the entry is said under the entry. Once renamed the form closes, handing `onDone` the name.
 */
function RenameForm({
  id,
  collection,
  entry,
  driver,
  onDone,
}: {
  id: string;
  collection: Collection;
  entry: CollectionEntry;
  driver: Driven<unknown>;
  onDone: (to: string | undefined) => void;
}) {
  const [to, setTo] = useState(entry.name ?? "");
  const [refusal, setRefusal] = useState<EntryRefusal>();
  const [sending, setSending] = useState(false);
  const form = useId();
  useEffect(() => {
    document.getElementById(form)?.querySelector<HTMLElement>("input")?.focus();
  }, [form]);
  const send = async () => {
    setSending(true);
    const said = await driver.entry(id, {
      collection: collection.name,
      base: collection.base,
      rename: entry.id,
      to,
    });
    setSending(false);
    if (said) setRefusal(said);
    else onDone(to.trim());
  };
  return (
    <form
      id={form}
      className="ui-collection-form"
      aria-label={`Rename ${entry.label}`}
      onSubmit={(event) => {
        event.preventDefault();
        void send();
      }}
      onKeyDown={(event) => {
        if (event.key !== "Escape") return;
        event.preventDefault();
        event.stopPropagation();
        onDone(undefined);
      }}
    >
      <SettingRow
        label="New name"
        help={`What ${entry.label} is called from now on. Nothing may be using it.`}
        error={refusal?.fields.name}
        control={(ids) => <Field kind="text" ids={ids} value={to} onChange={setTo} />}
      />
      <SettingActions>
        <button type="submit" tabIndex={0} disabled={sending}>
          Rename
        </button>
        <button type="button" tabIndex={0} onClick={() => onDone(undefined)}>
          Cancel
        </button>
      </SettingActions>
    </form>
  );
}
