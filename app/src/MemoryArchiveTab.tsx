import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import { Archive, LoaderCircle } from "lucide-react";
import { EmptyState } from "./EmptyState";
import { COMPONENTS } from "./SessionRecordTab";
import { commands, type ArchivedMemory, type MemoryScope, type PlaneId } from "./bindings";
import { archiveWhere, memoryOf, scopeKey, scopeWord } from "./memories";
import { SettingActions } from "./settings/components";

/**
 * **A store's archive, in a tab of its own** (KN-4, D6): what the window's Delete — and
 * `archive` on the command line — moved out of one store, to read and to put back.
 *
 * One tab per store (`memories.archiveView`), opened from the store's own list: a persona's
 * tab, the shared list, a workspace's Memory section, or the palette. It lists what the
 * store's archive holds by title and stamp; choosing one shows its text under the list,
 * **read-only** and rendered with no HTML, for `MemoryTab`'s reason. Nothing here edits an
 * archived memory: restoring it is how it becomes a memory again, and its own tab edits it
 * then.
 *
 * **Restore is an explicit button** and asks nothing first, because nothing is lost: the file
 * moves back under its name in the archive and its index line is appended (`memory_unarchive`,
 * the Delete's Undo). The core tells the project's model what it wrote, so every list showing
 * the store follows (FD-10); this tab reads the archive again at once as well.
 */
export function MemoryArchiveTab({
  plane,
  scope,
  changed,
}: {
  plane: PlaneId;
  scope: MemoryScope;
  /** Bumped when the project changes on disk or a memory is written: the tab reads again. */
  changed: number;
}) {
  const [said, setSaid] = useState<{ archived?: ArchivedMemory[]; trouble?: string }>();
  const [chosen, setChosen] = useState<string>();
  const [restoring, setRestoring] = useState(false);
  const [outcome, setOutcome] = useState<{ back?: string; refused?: string }>();
  const [again, setAgain] = useState(0);
  // **Focus follows a Restore** to the line that says what it did: the button pressed is gone
  // with the memory it restored, and focus left on nothing strands a keyboard and says
  // nothing to a screen reader.
  const saidBack = useRef<HTMLParagraphElement>(null);
  useEffect(() => {
    if (outcome?.back !== undefined) saidBack.current?.focus();
  }, [outcome?.back]);

  const store = scopeKey(scope);
  useEffect(() => {
    let gone = false;
    void commands
      .memoryArchived(plane, scope)
      .then((answer) => {
        if (gone) return;
        setSaid(answer.status === "error" ? { trouble: answer.error } : { archived: answer.data });
      })
      .catch((err: unknown) => {
        if (!gone) setSaid({ trouble: String(err) });
      });
    return () => {
      gone = true;
    };
    // `scope` by its key: a fresh object naming the same store must not read again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [plane, store, changed, again]);

  if (said === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading the archive…
      </p>
    );
  }
  if (said.trouble !== undefined) {
    return (
      <p className="trouble" role="alert">
        {`purlis could not read ${archiveWhere(scope)}: ${said.trouble}`}
      </p>
    );
  }

  const archived = said.archived ?? [];
  const shown = archived.find((one) => one.archived === chosen);

  const restore = async (memory: ArchivedMemory) => {
    setRestoring(true);
    setOutcome(undefined);
    try {
      const answer = await commands.memoryUnarchive(plane, scope, memory.archived, null);
      if (answer.status === "error") {
        setOutcome({ refused: `purlis could not restore ${memory.title}: ${answer.error}` });
      } else {
        setOutcome({
          back: `${memory.title} is back in ${memoryOf(scope)}.`,
        });
        setChosen(undefined);
        setAgain((was) => was + 1);
      }
    } catch (err: unknown) {
      setOutcome({ refused: `purlis could not restore ${memory.title}: ${String(err)}` });
    } finally {
      setRestoring(false);
    }
  };

  return (
    <div className="memory-archive">
      {outcome?.back !== undefined && (
        <p className="note" role="status" tabIndex={-1} ref={saidBack}>
          {outcome.back}
        </p>
      )}
      {archived.length === 0 ? (
        <EmptyState
          mark={Archive}
          headline="Nothing archived"
          body={`A memory you delete from ${memoryOf(scope)} lands here, and Restore memory puts it back.`}
          testid="archive-empty"
        />
      ) : (
        <ul
          className="panel-rows memory-archive-rows"
          aria-label={`Archived memories in ${scopeWord(scope)}`}
        >
          {archived.map((memory) => (
            <li key={memory.archived} className="panel-row">
              <button
                type="button"
                // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
                tabIndex={0}
                className="row"
                aria-pressed={memory.archived === chosen}
                onClick={() => {
                  setChosen(memory.archived === chosen ? undefined : memory.archived);
                  setOutcome(undefined);
                }}
              >
                <span className="row-text">{memory.title}</span>
                {memory.stamp !== "" && <span className="row-note">{` · ${memory.stamp}`}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
      {shown !== undefined && (
        <section className="memory-archived" aria-label={shown.title}>
          <p className="memory-meta" data-testid="archived-meta">
            <span className="memory-scope">archived</span>
            {shown.stamp !== "" && <span>{shown.stamp}</span>}
            <code>{shown.path}</code>
          </p>
          <article className="memory-body release-notes" data-testid="archived-body">
            <Markdown skipHtml components={COMPONENTS}>
              {shown.body}
            </Markdown>
          </article>
          {outcome?.refused !== undefined && (
            <p className="trouble" role="alert">
              {outcome.refused}
            </p>
          )}
          <SettingActions>
            <button
              type="button"
              tabIndex={0}
              disabled={restoring}
              onClick={() => void restore(shown)}
            >
              Restore memory
            </button>
          </SettingActions>
        </section>
      )}
    </div>
  );
}
