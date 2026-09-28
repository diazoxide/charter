import { useEffect, useState, type FormEvent } from "react";
import Markdown from "react-markdown";
import { LoaderCircle } from "lucide-react";
import { EmptyState } from "./EmptyState";
import { COMPONENTS } from "./SessionRecordTab";
import { commands, type MemoryEdited, type MemoryView, type PlaneId } from "./bindings";
import {
  DRAFT,
  memoryKey,
  scopeWord,
  setDraft,
  TITLE_MAX,
  useDraft,
  type Draft,
  type MemoryRef,
} from "./memories";

/**
 * **One memory, in a tab of its own** (SI-9b, ADR 0065 Q2, Q3, Q10).
 *
 * Read, it is where the memory is — its store, its stamp and its file, under the tab's heading,
 * which is its title — and its body rendered as Markdown. The `# title` and `_stamp · kind_`
 * lines are the header's, so the body starts under them (`MemoryView.body`). **No HTML**, for
 * `SessionRecordTab`'s reason: a memory is a file a chat wrote, and raw HTML is dropped.
 *
 * Edited, the same tab is a title field capped at `TITLE_MAX` and the raw body, with Save and
 * Cancel. What is typed is kept outside the tab (`memories.Draft`), so looking at another tab
 * loses nothing. A save is checked against the file's text as this tab read it, and one the
 * file changed under is refused: the tab says so and offers **Reload** (what is on disk now,
 * dropping the edit) or **Overwrite**.
 *
 * Edit and Delete are the heading's buttons, the catalogue's `memory.edit:<key>` and
 * `memory.delete:<key>` rows (`Views.OWN_ROWS`), so the heading, a row's menu and the palette
 * are one verb. An edit asked for from outside arrives as a draft of `"wanted"`.
 *
 * A new memory's tab ({@link DRAFT} slug) reads nothing and is an editor from the start; its
 * Save is the store's own remember (`memory_create`), and its Cancel closes it.
 */
export function MemoryTab({
  plane,
  at,
  changed,
  onSaved,
  onClose,
}: {
  plane: PlaneId;
  at: MemoryRef;
  /** Bumped when the plane changes on disk or a memory is written: the tab reads again. */
  changed: number;
  /** A save or a create wrote the memory: the tab's name, key and every list follow it. */
  onSaved: (memory: MemoryView) => void;
  /** The tab asks to be closed: a new memory's Cancel. */
  onClose: () => void;
}) {
  const key = memoryKey(at);
  const isNew = at.slug === DRAFT;
  const [said, setSaid] = useState<{ memory?: MemoryView | null; trouble?: string }>();
  const draft = useDraft(plane, key);
  const [saving, setSaving] = useState(false);
  const [refused, setRefused] = useState<string>();

  const { scope, slug } = at;
  useEffect(() => {
    if (isNew) return;
    let gone = false;
    void commands
      .memoryRead(plane, scope, slug)
      .then((answer) => {
        if (gone) return;
        setSaid(
          answer.status === "error" ? { trouble: answer.error } : { memory: answer.data ?? null },
        );
      })
      .catch((err: unknown) => {
        if (!gone) setSaid({ trouble: String(err) });
      });
    return () => {
      gone = true;
    };
    // `scope` by value: a fresh object with the same store must not read again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [plane, key, isNew, changed]);

  const memory = said?.memory ?? undefined;
  // An edit asked for before the read arrived — or a new memory, which is only ever an editor
  // — becomes a draft of what there is to edit.
  useEffect(() => {
    if (isNew && draft === undefined) setDraft(plane, key, { title: "", body: "", base: "" });
    else if (draft === "wanted" && memory !== undefined)
      setDraft(plane, key, { title: memory.title, body: memory.body, base: memory.text });
  }, [draft, isNew, key, memory, plane]);

  const editing = typeof draft === "object" ? draft : undefined;

  if (editing !== undefined) {
    const change = (to: Partial<Draft>) => setDraft(plane, key, { ...editing, ...to });
    const answered = (outcome: MemoryEdited) => {
      if (outcome.kind === "stale") {
        change({ stale: { now: outcome.now } });
        return;
      }
      saved(outcome.memory);
    };
    const saved = (written: MemoryView) => {
      setSaid({ memory: written });
      setDraft(plane, key, undefined);
      onSaved(written);
    };
    const save = async (overwrite: boolean) => {
      setSaving(true);
      setRefused(undefined);
      try {
        if (isNew) {
          const answer = await commands.memoryCreate(plane, scope, editing.title, editing.body);
          if (answer.status === "error") setRefused(answer.error);
          else saved(answer.data);
        } else {
          const answer = await commands.memoryEdit(
            plane,
            scope,
            slug,
            editing.title,
            editing.body,
            editing.base,
            overwrite,
          );
          if (answer.status === "error") setRefused(answer.error);
          else answered(answer.data);
        }
      } catch (err: unknown) {
        setRefused(String(err));
      } finally {
        setSaving(false);
      }
    };
    const submit = (event: FormEvent) => {
      event.preventDefault();
      void save(false);
    };
    const stale = editing.stale;
    return (
      <form className="memory-editor" onSubmit={submit} aria-label={`Editing ${key}`}>
        <Meta at={at} memory={memory} />
        <div className="settings-field">
          <label htmlFor={`${key}-title`}>Title</label>
          <input
            id={`${key}-title`}
            type="text"
            value={editing.title}
            maxLength={TITLE_MAX}
            spellCheck={false}
            placeholder={isNew ? "The body's first line, when left empty" : undefined}
            aria-describedby={`${key}-title-count`}
            onChange={(event) => change({ title: event.target.value })}
          />
          <span className="note" id={`${key}-title-count`}>
            {`${editing.title.length} / ${TITLE_MAX}`}
          </span>
        </div>
        <div className="settings-field">
          <label htmlFor={`${key}-body`}>Body</label>
          <textarea
            id={`${key}-body`}
            value={editing.body}
            spellCheck={false}
            rows={Math.max(8, editing.body.split("\n").length + 1)}
            onChange={(event) => change({ body: event.target.value })}
          />
        </div>
        {stale !== undefined && (
          <div className="trouble memory-stale" role="alert">
            {stale.now === null
              ? "This memory is not there any more — it was archived or removed since you opened it. Nothing was saved."
              : "This memory changed on disk since you opened it, so nothing was saved."}{" "}
            <span className="settings-actions">
              <button
                type="button"
                tabIndex={0}
                onClick={() => {
                  setDraft(plane, key, undefined);
                  if (stale.now !== undefined) setSaid({ memory: stale.now });
                }}
              >
                Reload
              </button>
              {stale.now !== null && (
                <button
                  type="button"
                  tabIndex={0}
                  disabled={saving}
                  onClick={() => void save(true)}
                >
                  Overwrite
                </button>
              )}
            </span>
          </div>
        )}
        {refused !== undefined && (
          <p className="trouble" role="alert">
            {refused}
          </p>
        )}
        <div className="settings-actions">
          <button type="submit" tabIndex={0} disabled={saving || stale !== undefined}>
            Save
          </button>
          <button
            type="button"
            tabIndex={0}
            onClick={() => {
              setDraft(plane, key, undefined);
              setRefused(undefined);
              if (isNew) onClose();
            }}
          >
            Cancel
          </button>
        </div>
      </form>
    );
  }

  if (said === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading the memory…
      </p>
    );
  }
  if (said.trouble !== undefined) {
    return (
      <p className="trouble" role="alert">
        {said.trouble}
      </p>
    );
  }
  if (memory === undefined) {
    // **Not an error**: a tab the last launch left open can name a memory archived since.
    return (
      <EmptyState
        headline="This memory is not here any more"
        body={`Nothing is called ${slug} in ${scopeWord(scope)}'s memory now.`}
        testid="view-gone"
      />
    );
  }
  return (
    <>
      <Meta at={at} memory={memory} />
      <article className="memory-body release-notes" data-testid="memory-body">
        <Markdown skipHtml components={COMPONENTS}>
          {memory.body}
        </Markdown>
      </article>
    </>
  );
}

/** Where a memory is: its store as a badge, its stamp and its file. */
function Meta({ at, memory }: { at: MemoryRef; memory?: MemoryView }) {
  return (
    <p className="memory-meta" data-testid="memory-meta">
      <span className="memory-scope">{memory?.place ?? scopeWord(at.scope)}</span>
      {memory === undefined ? (
        <span>not written yet</span>
      ) : (
        <>
          {memory.stamp !== "" && <span>{memory.stamp}</span>}
          <code>{memory.path}</code>
        </>
      )}
    </p>
  );
}
