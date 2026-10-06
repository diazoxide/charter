import { useId, useRef, useState } from "react";
import type { Shown } from "./fileControls";

/**
 * **Edit as TOML** (SE-19, #1169; the spec on #558, V89d): a file of a level, as its whole text.
 * The Workspace level's `workspace.json` is the same, as **Edit as JSON** (NO-7, #1232): its
 * Save goes through the core's check of a manifest, against the text the edit began from.
 * One link per file sits at the foot of the nav; pressing it shows the file's text in the right
 * column, in place of a group, with an explicit Save and Discard. Nothing is written as it is
 * typed: half a table header is not a file to write.
 *
 * **Nothing here decides what a file may say.** Save sends the whole text against the text the
 * edit began from, and the core checks it with the rules it reads the file with
 * (`purlis_core::settings::save`): text that does not parse, or that the next read would refuse,
 * is refused in the core's words and nothing is written.
 *
 * **A file changed outside the tab** — by hand, by a chat — is read again by the level's driver.
 * While no edit is under way the editor shows the file as it now is. An edit is kept, and says
 * the file moved under it, for as long as it lasts — typed back to what it began from included:
 * every keystroke of it keeps the text it began from, its Save is made against that, which the
 * core refuses as changed on disk, so an outside edit is never overwritten unseen. Discard takes
 * up the file as it now is.
 *
 * **A save goes through the level's driver** (`Driven.writeRaw`), in the queue every setting's
 * write is in: what it wrote is what the next edit begins from, at once, and the last change's
 * Undo is gone.
 */

/** One file a level offers as its whole text. */
export type RawFile = {
  /** Which file, as the level names it: `shared`, `local` or `workspace`. */
  id: string;
  /** What the text is written in: a project's files are TOML, a workspace's manifest JSON. */
  as: "TOML" | "JSON";
  /** The file as it was last read. */
  file: Shown;
  /** The sentence on where the file is kept and who sees it. */
  kept: string;
  /** Writes `text` over the file whose text was `base` (`null`: not there yet); answers why
   *  nothing was written, or `undefined` once it was. */
  save: (base: string | null, text: string) => Promise<readonly string[] | undefined>;
};

/**
 * An edit of one file, kept by the tab so it survives a look at a group: the text, and the
 * file's text it began from (`base`, `null` for a file not there yet). It lasts until it is
 * saved or discarded.
 */
export type RawDraft = { base: string | null; text: string };

/** The file's name, without its folder. */
export function named(file: Shown): string {
  return file.file.split("/").pop() ?? file.file;
}

/** The links at the foot of the nav: one per file, the one on screen marked current, under
 *  what the level's files are written in (a level's files share one). */
export function RawLinks({
  files,
  editing,
  onEdit,
}: {
  files: readonly RawFile[];
  editing: string | undefined;
  onEdit: (id: string) => void;
}) {
  const label = useId();
  return (
    <div className="ui-settings-raw" role="group" aria-labelledby={label}>
      <p className="ui-settings-raw-label" id={label}>
        {`Edit as ${files[0]?.as ?? "TOML"}`}
      </p>
      {files.map((one) => (
        <button
          key={one.id}
          type="button"
          // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
          tabIndex={0}
          aria-current={one.id === editing ? "true" : undefined}
          onClick={() => onEdit(one.id)}
        >
          <code>{named(one.file)}</code>
        </button>
      ))}
    </div>
  );
}

/** One file's text, in the right column, with its Save and Discard. */
export function RawEditor({
  raw,
  draft,
  onDraft,
}: {
  raw: RawFile;
  draft: RawDraft | undefined;
  /** Sets the edit, or works it out from the edit as it now is. */
  onDraft: (
    to: RawDraft | undefined | ((now: RawDraft | undefined) => RawDraft | undefined),
  ) => void;
}) {
  const heading = useId();
  const box = useRef<HTMLTextAreaElement>(null);
  const [saving, setSaving] = useState(false);
  const [refused, setRefused] = useState<readonly string[]>();
  const { file } = raw;
  const name = named(file);
  const base = file.exists ? file.text : null;
  const text = draft?.text ?? file.text;
  const dirty = draft !== undefined && draft.text !== (draft.base ?? "");
  /** The file is no longer what the edit began from. */
  const moved = draft !== undefined && draft.base !== base;
  const focus = () => box.current?.focus();

  const save = async () => {
    if (!draft || !dirty) return;
    const sent = draft.text;
    setSaving(true);
    const why = await raw.save(draft.base, sent).catch((err: unknown) => [String(err)]);
    setSaving(false);
    setRefused(why);
    // Once written, the file is what was written: the edit is over, unless more was typed
    // while the save was on its way, which is then an edit of what was written.
    if (why === undefined)
      onDraft((now) => (now && now.text !== sent ? { base: sent, text: now.text } : undefined));
    focus();
  };

  return (
    <section className="ui-setting-group ui-raw" aria-labelledby={heading}>
      <h3 id={heading}>{name}</h3>
      <p className="ui-setting-help">
        {raw.as === "TOML" ? "The whole file, comments and all." : "The whole file."} {raw.kept}
        {!file.exists && " Not created yet: the first save creates it."} Nothing is written until
        you save, and purlis refuses text it would not read.
      </p>
      {moved && (
        <p className="ui-setting-help" role="status">
          {name} changed on disk since this edit began. Saving it is refused so the change is not
          lost; Discard shows the file as it now is.
        </p>
      )}
      <textarea
        ref={box}
        className="ui-field ui-raw-text"
        value={text}
        spellCheck={false}
        autoComplete="off"
        rows={Math.max(12, text.split("\n").length + 1)}
        aria-label={`${name}, as ${raw.as}`}
        // An edit keeps the text it began from for as long as it lasts.
        onChange={(event) =>
          onDraft({ base: draft ? draft.base : base, text: event.currentTarget.value })
        }
      />
      <div className="ui-raw-actions">
        <button type="button" tabIndex={0} disabled={!dirty || saving} onClick={() => void save()}>
          {saving ? "Saving…" : `Save ${name}`}
        </button>
        <button
          type="button"
          tabIndex={0}
          disabled={!(dirty || moved) || saving}
          onClick={() => {
            setRefused(undefined);
            onDraft(undefined);
            focus();
          }}
        >
          Discard
        </button>
      </div>
      {refused && (
        <div className="ui-setting-error" role="alert">
          <p>Nothing was saved:</p>
          {refused.map((why, at) => (
            <p key={at}>{why}</p>
          ))}
        </div>
      )}
    </section>
  );
}
