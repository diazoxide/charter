import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import { Archive, LoaderCircle, Search } from "lucide-react";
import { EmptyState } from "./EmptyState";
import { COMPONENTS } from "./SessionRecordTab";
import { commands, type ArchivedMemory, type MemoryScope, type PlaneId } from "./bindings";
import { archiveWhere, memoryOf, scopeKey, scopeWord, unnumbered } from "./memories";
import { PAGE, snippetOf } from "./PanelList";
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
 *
 * **A numbered name is offered its un-numbered one, never given it** (#1191, D-1191-1). When
 * the archive holds both `freeze` and `freeze-2`, the second may be a `freeze` that archiving
 * had to number, or a memory that was called `freeze-2` in the store: the archive keeps no
 * record of which. So *Restore memory* still puts it back under its archived name, and a
 * second button, *Restore as freeze* ({@link unnumbered}), asks the core for the other name.
 * The tab never picks the name itself: whichever comes back as `freeze` leaves the other unable
 * to, and that is the reader's call. A store that already holds the name refuses it, moves
 * nothing, and the refusal is shown as any other is.
 *
 * **A long archive is searched and paged the way a memory list is** (`PanelList`, #1191): one
 * page of rows, then *Show more*, and a search box once the archive holds more than a page. The
 * search reads an archived memory's title, stamp and text, and a typed search shows every match;
 * a row found by its text shows the part that matched under it, as a memory list's does.
 * A search that hides the chosen row keeps its text drawn below: what the reader opened stays
 * open until they choose again or restore it.
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
  const [query, setQuery] = useState("");
  const [limit, setLimit] = useState(PAGE);
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
  // Against the whole archive, never the matches: a box that vanished as a search narrowed the
  // list would take away the control being used (`PanelList`'s rule).
  const searchable = archived.length > PAGE;
  const wanted = searchable ? query.trim().toLowerCase() : "";
  const matching =
    wanted === ""
      ? archived
      : archived.filter((one) =>
          `${one.title} ${one.stamp} ${one.body}`.toLowerCase().includes(wanted),
        );
  const drawn = wanted === "" ? matching.slice(0, limit) : matching;
  const more = matching.length - drawn.length;

  // The un-numbered name the chosen memory is offered, beside its archived one.
  const offered =
    shown === undefined
      ? undefined
      : unnumbered(
          shown.archived,
          archived.filter((one) => one !== shown).map((one) => one.archived),
        );

  /** Restore `memory` under its archived name, or under `as` when the reader chose that. */
  const restore = async (memory: ArchivedMemory, as?: string) => {
    setRestoring(true);
    setOutcome(undefined);
    try {
      const answer = await commands.memoryUnarchive(plane, scope, memory.archived, as ?? null);
      if (answer.status === "error") {
        setOutcome({ refused: `purlis could not restore ${memory.title}: ${answer.error}` });
      } else {
        const where = `${memory.title} is back in ${memoryOf(scope)}`;
        // Which name it came back under, whenever there were two it could have had.
        setOutcome({
          back:
            as === undefined && offered === undefined
              ? `${where}.`
              : `${where} as ${answer.data.slug}.`,
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
      {searchable && (
        <div className="panel-search">
          <Search className="node-icon" />
          <input
            type="search"
            value={query}
            aria-label={`Search archived memories in ${scopeWord(scope)}`}
            placeholder={`Search ${archived.length}`}
            onChange={(e) => {
              setQuery(e.target.value);
              setLimit(PAGE);
            }}
          />
        </div>
      )}
      {archived.length === 0 ? (
        <EmptyState
          mark={Archive}
          headline="Nothing archived"
          body={`A memory you delete from ${memoryOf(scope)} lands here, and Restore memory puts it back.`}
          testid="archive-empty"
        />
      ) : matching.length === 0 ? (
        // Not the empty state: the archive holds memories, and the search found none of them.
        <p className="none">Nothing here matches “{query.trim()}”.</p>
      ) : (
        <ul
          className="panel-rows memory-archive-rows"
          aria-label={`Archived memories in ${scopeWord(scope)}`}
        >
          {drawn.map((memory) => {
            const snippet = snippetOf(
              {
                key: memory.archived,
                text: memory.title,
                note: memory.stamp,
                mark: "",
                tone: "",
                detail: { kind: "text", text: memory.body },
                runs: null,
                actions: [],
              },
              wanted,
            );
            return (
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
                {snippet !== undefined && (
                  <p className="row-snippet" data-testid={`row-snippet-${memory.archived}`}>
                    {snippet.before}
                    <mark>{snippet.match}</mark>
                    {snippet.after}
                  </p>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {more > 0 && (
        <button
          type="button"
          className="panel-more"
          // #190: WebKit leaves a button out of the tab sequence without this.
          tabIndex={0}
          onClick={() => setLimit((was) => was + PAGE)}
        >
          {`Show ${Math.min(more, PAGE)} more`}
          <span className="note">{` · ${more} left`}</span>
        </button>
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
            {offered !== undefined && (
              <button
                type="button"
                tabIndex={0}
                disabled={restoring}
                onClick={() => void restore(shown, offered)}
              >
                {`Restore as ${offered}`}
              </button>
            )}
          </SettingActions>
        </section>
      )}
    </div>
  );
}
