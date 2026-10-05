import { useEffect, useId, useRef, useState, type ReactNode, type RefObject } from "react";
import { Choice, Field, SettingActions, SettingRow } from "./components";
import type { Driven, EntryRefusal } from "./driver";
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
 */
export function CollectionView({
  id,
  collection,
  settings,
  driver,
  row,
  onGo,
}: {
  /** The group's id: what the driver keeps the collection's Undo and refusals by. */
  id: string;
  collection: Collection;
  /** The group's settings the filter left. */
  settings: readonly Setting[];
  driver: Driven<unknown>;
  /** Draws one of the group's settings as a row. */
  row: (setting: Setting) => ReactNode;
  /** Opens a group of this level: a referrer's link. */
  onGo: (group: string) => void;
}) {
  const { noun } = collection;
  const [removing, setRemoving] = useState<string>();
  const adding = useRef<HTMLButtonElement>(null);
  const whole = useRef<HTMLDivElement>(null);
  /** Where the focus goes once what had it is gone: Add, or the collection itself. */
  const settle = () => (adding.current ?? whole.current)?.focus();
  const undoSaid = driver.undoable === id ? driver.undoSaid : undefined;
  const refused = driver.entryRefused?.collection === id ? driver.entryRefused : undefined;
  const ofEntry = (entry: CollectionEntry) =>
    settings.filter((one) => entry.settings.includes(one.id));
  const mine = new Set(collection.entries.flatMap((entry) => entry.settings));

  const remove = async (entry: CollectionEntry) => {
    setRemoving(entry.id);
    // Sent against the text this entry was drawn from, whatever is queued before it.
    const refusal = await driver.entry(id, {
      collection: collection.name,
      base: collection.base,
      remove: entry.id,
    });
    setRemoving(undefined);
    // The entry is gone: the focus goes to Add, which is always there.
    if (!refusal) settle();
  };

  return (
    <div className="ui-collection" ref={whole} tabIndex={-1}>
      {settings.filter((one) => !mine.has(one.id)).map(row)}
      {(undoSaid !== undefined || (refused && refused.entry === undefined)) && (
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
                  settle();
                }}
              >
                Undo
              </button>
            </>
          )}
          {refused && refused.entry === undefined && (
            <Refused noun={noun} refusal={refused.refusal} onGo={onGo} />
          )}
        </div>
      )}
      {collection.entries.map((entry) => (
        <div key={entry.id} className="ui-collection-entry" role="group" aria-label={entry.label}>
          <div className="ui-collection-head">
            <h4>{entry.label}</h4>
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
          {refused?.entry === entry.id && (
            <Refused noun={noun} refusal={refused.refusal} onGo={onGo} />
          )}
          {ofEntry(entry).map(row)}
        </div>
      ))}
      <AddForm id={id} collection={collection} driver={driver} adding={adding} whole={whole} />
    </div>
  );
}

/** What a Remove or an Undo was refused for: who uses the entry, each with its link, and why. */
function Refused({
  noun,
  refusal,
  onGo,
}: {
  noun: string;
  refusal: EntryRefusal;
  onGo: (group: string) => void;
}) {
  const users = refusal.referrers.length;
  return (
    <div className="ui-setting-error" role="alert">
      {users > 0 && (
        <p>{`This ${noun} is not removed while ${users === 1 ? "this uses" : "these use"} it:`}</p>
      )}
      {refusal.referrers.map((one, at) => (
        <p key={at}>
          {one.what}
          {one.group !== null && (
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
}: {
  id: string;
  collection: Collection;
  driver: Driven<unknown>;
  adding: RefObject<HTMLButtonElement | null>;
  whole: RefObject<HTMLDivElement | null>;
}) {
  const { noun, fields } = collection;
  const [values, setValues] = useState<Record<string, string>>();
  const [refusal, setRefusal] = useState<EntryRefusal>();
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
  const set = (field: string) => (to: string) =>
    setValues((was) => (was === undefined ? was : { ...was, [field]: to }));
  const send = async () => {
    setSending(true);
    const said = await driver.entry(id, {
      collection: collection.name,
      base: collection.base,
      add: values,
    });
    setSending(false);
    if (said) setRefusal(said);
    else close();
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
          error={refusal?.fields[one.field]}
          control={(ids) =>
            one.kind === "choice" ? (
              <Choice
                kind="select"
                ids={ids}
                options={(one.choices ?? []).map((choice) => ({ value: choice, label: choice }))}
                value={values[one.field] ?? ""}
                onValueChange={set(one.field)}
              />
            ) : (
              <Field
                kind={one.kind === "lines" ? "list" : "text"}
                ids={ids}
                value={values[one.field] ?? ""}
                onChange={set(one.field)}
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
