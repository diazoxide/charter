import { useId, useState } from "react";
import type { Shown } from "./fileControls";

/**
 * **Edit as TOML** (SE-19, #1169; the spec on #558, V89d): a file of a level, as its whole text.
 * One link per file sits at the foot of the nav; pressing it shows the file's text in the right
 * column, in place of a group, with an explicit Save and Discard. Nothing is written as it is
 * typed: half a table header is not a file to write.
 *
 * **Nothing here decides what a file may say.** Save sends the whole text against the text the
 * edit began from, and the core checks it with the rules it reads the file with
 * (`charter_core::settings::save`): text that does not parse, or that the next read would refuse,
 * is refused in the core's words and nothing is written.
 *
 * **A file changed outside the tab** — by hand, by a chat — is read again by the level's driver.
 * While nothing is typed the editor shows the file as it now is. An edit under way is kept, and
 * says the file moved under it: its Save is still made against the text it began from, which
 * the core refuses as changed on disk, so an outside edit is never overwritten unseen; Discard
 * takes up the file as it now is.
 */

/** One file a level offers as raw TOML. */
export type RawFile = {
  /** Which file, as the level names it: `shared` or `local`. */
  id: string;
  /** The file as it was last read. */
  file: Shown;
  /** The sentence on where the file is kept and who sees it. */
  kept: string;
  /** Writes `text` over the file whose text was `base` (`null`: not there yet). */
  save: (base: string | null, text: string) => Promise<{ saved: true } | { refused: string[] }>;
};

/**
 * What is typed into one file's editor, kept by the tab so it survives a look at a group: the
 * text, the file's text it began from (`base`, `null` for a file not there yet), and the text
 * the file was read as when it was last typed (`seen`).
 */
export type RawDraft = { base: string | null; text: string; seen: string };

/** The file's name, without its folder. */
export function named(file: Shown): string {
  return file.file.split("/").pop() ?? file.file;
}

/** The links at the foot of the nav: one per file, the one on screen marked current. */
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
        Edit as TOML
      </p>
      {files.map((one) => (
        <button
          key={one.id}
          type="button"
          // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
          tabIndex={0}
          aria-label={`Edit ${named(one.file)} as TOML`}
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
  onDraft: (to: RawDraft | undefined) => void;
}) {
  const heading = useId();
  const [saving, setSaving] = useState(false);
  const [refused, setRefused] = useState<readonly string[]>();
  const { file } = raw;
  const name = named(file);
  const base = file.exists ? file.text : null;
  const changed = (one: RawDraft) => one.text !== (one.base ?? "");
  // A draft with nothing typed in it is the file as it was once read: once the file reads
  // differently — written here, or outside the tab — the editor shows it as it now is.
  const live = draft && (changed(draft) || draft.seen === file.text) ? draft : undefined;
  const text = live?.text ?? file.text;
  const dirty = live !== undefined && changed(live);
  const moved = dirty && live.base !== base;

  const save = async () => {
    if (!dirty) return;
    setSaving(true);
    const said = await raw
      .save(live.base, live.text)
      .catch((err: unknown) => ({ refused: [String(err)] }));
    setSaving(false);
    if ("refused" in said) {
      setRefused(said.refused);
      return;
    }
    setRefused(undefined);
    // What was written is what the file now holds, shown until the level has read it again.
    onDraft({ base: live.text, text: live.text, seen: file.text });
  };

  return (
    <section className="ui-setting-group ui-raw" aria-labelledby={heading}>
      <h3 id={heading}>{name}</h3>
      <p className="ui-setting-help">
        The whole file, comments and all. {raw.kept}
        {!file.exists && " Not created yet: the first save creates it."} Nothing is written until
        you save, and charter refuses text it would not read.
      </p>
      {moved && (
        <p className="ui-setting-help" role="status">
          {name} changed on disk since this edit began. Saving it is refused so the change is not
          lost; Discard shows the file as it now is.
        </p>
      )}
      <textarea
        className="ui-field ui-raw-text"
        value={text}
        spellCheck={false}
        autoComplete="off"
        rows={Math.max(12, text.split("\n").length + 1)}
        aria-label={`${name}, as TOML`}
        onChange={(event) =>
          onDraft({
            base: dirty ? live.base : base,
            text: event.currentTarget.value,
            seen: file.text,
          })
        }
      />
      <div className="ui-raw-actions">
        <button type="button" tabIndex={0} disabled={!dirty || saving} onClick={() => void save()}>
          {saving ? "Saving…" : `Save ${name}`}
        </button>
        <button
          type="button"
          tabIndex={0}
          disabled={!dirty || saving}
          onClick={() => {
            setRefused(undefined);
            onDraft(undefined);
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
