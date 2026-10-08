import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  ClipboardCheck,
  LoaderCircle,
  SquareTerminal,
} from "lucide-react";
import { EmptyState } from "./EmptyState";
import { Notice } from "./Notice";
import {
  commands,
  type ChatStop,
  type PastSince,
  type PastTask,
  type PastTaskRead,
  type PastTasks,
  type PlaneId,
} from "./bindings";
import { qualifierOf, shownOf } from "./finished";
import { listen } from "./here";
import {
  askedBy,
  clipped,
  counted,
  endsOf,
  EVERY_PAST,
  localAt,
  mergedPast,
  narrows,
  personasOfPast,
  shownPast,
  zoneSaid,
  type PastFilter,
} from "./pastTasks";
import { PersonaMark } from "./PersonaMark";
import { StateShown } from "./StateShown";

/** What the view asks the window to do: the one thing that acts on a past task. */
export type PastTasksDoes = {
  /**
   * Reopens the past task `id` **by the finished row's own Reopen** (#1485): an ordinary chat
   * on its conversation, with a tab. Answers why not, where it could not.
   */
  reopen: (id: string) => Promise<string | undefined>;
};

/** What the list holds: the rows, and what the last whole read said of the store. */
type Held = {
  rows: PastTask[];
  /** What the next read is handed: the read before it. */
  since: PastSince;
  older: number;
  most: number;
  unread: number;
  undrawn: number;
};

/** How long after a task's chat ended the list is asked once more, where that read still found
 *  a task waiting for its chat to close: the event and the close are two moments. */
const ONCE_MORE_MS = 3000;

const NO_CONVERSATION = "It cannot be reopened: its harness named no conversation to resume.";
const REOPENED = "It was reopened already: a task is reopened once, and that chat carries it on.";

/**
 * **A workspace's past tasks, in a tab of their own** (#1510, V100-52): every task that was
 * asked from this workspace or worked in it and has ended, newest first, from the dispatch
 * records this machine keeps for this project. A task still running is not here; it is in the
 * Chats list. A task whose row was cleared there, or went when its session closed, is: this is
 * where it went.
 *
 * **Times and days are the person's own.** A record keeps UTC; the rows say local time and the
 * day boxes mean local days, so "today" finds what ended today where they are. The zone is
 * said once, above the table.
 *
 * A row says when it ended, its name, who asked whom, **how it ended in the word and the shape
 * every row says a state in** (`shownOf`, as its finished row did), how long it ran and where
 * it worked. Its chevron opens it: its report, what it said it changed and its brief, read for
 * that one row, **each drawn as text** (a text node in a `pre`, never markup) and clipped where
 * long, with a way to see all of it.
 *
 * **Reopen is the one thing that acts on a past task**, and it is the finished row's: offered
 * where the record names a conversation and the task was not reopened already, and the opened
 * row says the core's own reason where it would be refused (another harness, a folder gone).
 *
 * **It follows the tasks that end while it is open**: when a chat ends, and when the project
 * changes on disk, it asks for what was written since its last read, and is handed only that.
 *
 * Seams for work in flight: a past task's Activity (#1495) belongs beside its session record's
 * link; the brief drawn here moves into the Brief panel (#1494); and what it said it changed
 * is plain text until a task's changes have a view (#1511).
 */
export function PastTasksTab({
  plane,
  workspace,
  changed,
  does,
  onOpenRecord,
}: {
  plane: PlaneId;
  /** The workspace whose tasks these are. */
  workspace: string;
  /** Bumped when the project changes on disk: what was written since is read. */
  changed: number;
  does?: PastTasksDoes;
  /** Open the session record at `path`, called `title`. */
  onOpenRecord: (path: string, title: string) => void;
}) {
  const [held, setHeld] = useState<Held>();
  const [trouble, setTrouble] = useState<string>();
  const [filter, setFilter] = useState<PastFilter>(EVERY_PAST);
  /** The rows that are opened, each with what was read for it. */
  const [opened, setOpened] = useState<
    Readonly<Record<string, { read?: PastTaskRead; trouble?: string }>>
  >({});
  /** A Reopen's refusal, by the row it was pressed on. */
  const [refused, setRefused] = useState<Readonly<Record<string, string>>>({});
  const [busy, setBusy] = useState<string>();

  /** The read before this one, for the next to follow; and the reads in turn, so each follows
   *  the one before it. */
  const last = useRef<PastSince | undefined>(undefined);
  const turn = useRef<Promise<void>>(Promise.resolve());
  const gone = useRef(false);
  const read = useCallback(
    (whole: boolean): Promise<PastTasks | undefined> => {
      const mine = turn.current.then(async () => {
        const since = whole ? null : (last.current ?? null);
        const answer = await commands
          .pastTasks(plane, workspace === "" ? null : workspace, since)
          .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
        if (gone.current) return undefined;
        if (answer.status === "error") {
          // A later read that failed leaves the list as it was: the next one follows the same
          // read. Only a list that was never read says so.
          if (since === null) setTrouble(answer.error);
          return undefined;
        }
        const said = answer.data;
        const next = { at: said.read_at, also: said.waiting };
        last.current = next;
        setTrouble(undefined);
        setHeld((was) =>
          said.whole || was === undefined
            ? {
                rows: said.rows,
                since: next,
                older: said.older,
                most: said.most,
                unread: said.unread,
                undrawn: said.undrawn,
              }
            : { ...was, rows: mergedPast(was.rows, said.rows), since: next },
        );
        return said;
      });
      turn.current = mine.then(() => undefined);
      return mine;
    },
    [plane, workspace],
  );

  // The whole list, once: when the view opens, and when it is asked to read again.
  const [again, setAgain] = useState(0);
  useEffect(() => {
    gone.current = false;
    last.current = undefined;
    void read(true);
    return () => {
      gone.current = true;
    };
  }, [read, again]);

  // **What was written since**, when the project changes on disk: a session record written
  // names itself on its task's record, and a task that ended wrote its own.
  const seen = useRef(changed);
  useEffect(() => {
    if (seen.current === changed) return;
    seen.current = changed;
    void read(false);
  }, [changed, read]);

  // And when a chat of this project has ended: a task is past once its chat has closed. One
  // more look a moment later where the read still found one waiting to close.
  useEffect(() => {
    let off = false;
    let stop: (() => void) | undefined;
    let timer: number | undefined;
    void (async () => {
      try {
        const unlisten = await listen<ChatStop>("chat-stop", (event) => {
          if (off || event.payload.plane !== plane || event.payload.phase === "stopping") return;
          void read(false).then((said) => {
            if (off || said === undefined || said.waiting.length === 0) return;
            window.clearTimeout(timer);
            timer = window.setTimeout(() => void read(false), ONCE_MORE_MS);
          });
        });
        if (off) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in: a unit test, or a webview being torn down.
      }
    })();
    return () => {
      off = true;
      window.clearTimeout(timer);
      stop?.();
    };
  }, [plane, read]);

  const open = (row: PastTask) => {
    if (opened[row.id] !== undefined) {
      setOpened((was) => without(was, row.id));
      return;
    }
    setOpened((was) => ({ ...was, [row.id]: {} }));
    void commands
      .pastTask(plane, row.id)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }))
      .then((answer) => {
        if (gone.current) return;
        setOpened((was) =>
          was[row.id] === undefined
            ? was
            : {
                ...was,
                [row.id]:
                  answer.status === "error" ? { trouble: answer.error } : { read: answer.data },
              },
        );
      });
  };

  const reopen = (row: PastTask) => {
    if (does === undefined || !row.reopens || busy !== undefined) return;
    setBusy(row.id);
    setRefused((was) => without(was, row.id));
    void does
      .reopen(row.id)
      .then((why) => {
        if (gone.current) return;
        if (why !== undefined) setRefused((was) => ({ ...was, [row.id]: why }));
        // Its chat is starting, or it was refused: either way the row is read again.
        void read(false);
      })
      .finally(() => {
        if (!gone.current) setBusy(undefined);
      });
  };

  if (trouble !== undefined && held === undefined) {
    return (
      <Notice
        cause="past-tasks-unread"
        tone="trouble"
        fixes={[{ label: "Read again", onPress: () => setAgain((was) => was + 1) }]}
      >
        {`purlis could not read this workspace's past tasks: ${trouble}`}
      </Notice>
    );
  }
  if (held === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading the past tasks…
      </p>
    );
  }

  const { rows } = held;
  /* What the list does not show, said and never silent: the tasks past its bound, the files
     that did not read, and the records purlis will not draw. The last two are counted across
     the project's store, since a record that cannot be read names no workspace. */
  const notShown = (held.older > 0 || held.unread > 0 || held.undrawn > 0) && (
    <div className="past-not-shown">
      {held.older > 0 && (
        <p className="none" data-testid="past-older">
          {`The newest ${held.most} are listed. ${counted(held.older, "older task is", "older tasks are")} not listed, and not searched.`}
        </p>
      )}
      {held.unread > 0 && (
        <p className="none" data-testid="past-unread">
          {`${counted(held.unread, "record", "records")} could not be read, in this project's store on this machine.`}
        </p>
      )}
      {held.undrawn > 0 && (
        <p className="none" data-testid="past-undrawn">
          {held.undrawn === 1
            ? "1 record purlis will not draw: it holds text purlis refuses to put on the screen."
            : `${held.undrawn} records purlis will not draw: they hold text purlis refuses to put on the screen.`}
        </p>
      )}
    </div>
  );
  if (rows.length === 0) {
    return (
      <>
        <EmptyState
          mark={ClipboardCheck}
          headline="No past tasks yet"
          body="When a task dispatched in this workspace has ended, it is listed here with its report and its brief. Tasks still running are in the Chats list."
          testid="past-tasks-empty"
        />
        {notShown}
      </>
    );
  }
  const shown = shownPast(rows, filter);
  const set = (part: Partial<PastFilter>) => setFilter((was) => ({ ...was, ...part }));
  return (
    <>
      <div className="vault-tools past-tools" role="search" aria-label="Narrow the past tasks">
        <input
          type="search"
          className="past-text"
          // #190: WebKit leaves a control out of the tab sequence without `tabIndex`.
          tabIndex={0}
          aria-label="Search by task name"
          placeholder="Task name"
          value={filter.text}
          onChange={(e) => set({ text: e.target.value })}
        />
        <select
          className="search-scope"
          role="combobox"
          tabIndex={0}
          aria-label="Filter by persona"
          title="A persona that asked for a task, or that ran one"
          value={filter.persona}
          onChange={(e) => set({ persona: e.target.value })}
        >
          <option value="">Every persona</option>
          {personasOfPast(rows).map((persona) => (
            <option key={persona} value={persona}>
              {persona}
            </option>
          ))}
        </select>
        <select
          className="search-scope"
          role="combobox"
          tabIndex={0}
          aria-label="Filter by how it ended"
          value={filter.how}
          onChange={(e) => set({ how: e.target.value })}
        >
          <option value="">Every end</option>
          {endsOf(rows).map((end) => (
            <option key={end.how} value={end.how}>
              {end.said}
            </option>
          ))}
        </select>
        {/* The person's own days, as the rows say a time: the zone is in the heading. */}
        <label className="past-day">
          From
          <input
            type="date"
            tabIndex={0}
            aria-label="Ended on or after"
            value={filter.from}
            onChange={(e) => set({ from: e.target.value })}
          />
        </label>
        <label className="past-day">
          To
          <input
            type="date"
            tabIndex={0}
            aria-label="Ended on or before"
            value={filter.to}
            onChange={(e) => set({ to: e.target.value })}
          />
        </label>
        {narrows(filter) && (
          <button
            type="button"
            className="past-all"
            tabIndex={0}
            onClick={() => setFilter(EVERY_PAST)}
          >
            Show every task
          </button>
        )}
      </div>
      <p className="past-count">
        <span role="status">
          {narrows(filter)
            ? `${shown.length} of ${counted(rows.length, "past task", "past tasks")}`
            : counted(rows.length, "past task", "past tasks")}
        </span>
        {/* Said once, for every time and day below: the records keep UTC, and the person
            reads their own clock. */}
        <span className="past-zone" data-testid="past-zone">
          {` · Times and days are your local time (${zoneSaid()}).`}
        </span>
      </p>
      {shown.length === 0 ? (
        // Not the empty state: the workspace has past tasks, and the narrowing keeps none.
        <p className="none">No past task matches.</p>
      ) : (
        <table className="vault-secrets dispatches past-tasks" aria-label="Past tasks">
          <thead>
            <tr>
              <th scope="col">Ended</th>
              <th scope="col">Task</th>
              <th scope="col">Asked by</th>
              <th scope="col">Ran as</th>
              <th scope="col">How it ended</th>
              <th scope="col">Took</th>
              <th scope="col">Where</th>
              <th scope="col" aria-label="Reopen" />
            </tr>
          </thead>
          <tbody>
            {shown.map((row) => (
              <Fragment key={row.id}>
                <PastRow
                  row={row}
                  open={opened[row.id] !== undefined}
                  busy={busy === row.id}
                  canAct={does !== undefined}
                  onOpen={() => open(row)}
                  onReopen={() => reopen(row)}
                />
                {(refused[row.id] !== undefined || row.not_reopened !== null) && (
                  <tr className="dispatch-read past-note">
                    <td colSpan={8}>
                      {refused[row.id] !== undefined && (
                        <p className="trouble" role="alert">
                          {refused[row.id]}
                        </p>
                      )}
                      {/* The last Reopen started a chat that ended at once: the core's
                          sentence, kept until the next try. */}
                      {row.not_reopened !== null && <p className="none">{row.not_reopened}</p>}
                    </td>
                  </tr>
                )}
                {opened[row.id] !== undefined && (
                  <tr className="dispatch-read">
                    <td colSpan={8}>
                      <Opened row={row} said={opened[row.id]} onOpenRecord={onOpenRecord} />
                    </td>
                  </tr>
                )}
              </Fragment>
            ))}
          </tbody>
        </table>
      )}
      {notShown}
    </>
  );
}

/** `all` without what it holds under `key`. */
function without<T>(all: Readonly<Record<string, T>>, key: string): Record<string, T> {
  return Object.fromEntries(Object.entries(all).filter(([one]) => one !== key));
}

/** One past task's row. */
function PastRow({
  row,
  open,
  busy,
  canAct,
  onOpen,
  onReopen,
}: {
  row: PastTask;
  open: boolean;
  busy: boolean;
  canAct: boolean;
  onOpen: () => void;
  onReopen: () => void;
}) {
  const state = shownOf(row);
  const more = qualifierOf(row);
  /** Why Reopen does nothing, where it does nothing: said on the button, which stays in the
   *  Tab order, as a finished row's is (#1485). */
  const cannot = row.reopened ? REOPENED : row.reopens ? undefined : NO_CONVERSATION;
  return (
    <tr data-testid={`past-${row.id}`} data-how={row.how}>
      <td>{row.ended === null ? "" : localAt(row.ended)}</td>
      <td>
        <span className="dispatch-task">
          <button
            type="button"
            className="dispatch-more"
            tabIndex={0}
            aria-expanded={open}
            aria-label={
              open
                ? `Hide the report and brief of ${row.name}`
                : `Show the report and brief of ${row.name}`
            }
            onClick={onOpen}
          >
            {open ? (
              <ChevronDown className="node-icon" aria-hidden="true" />
            ) : (
              <ChevronRight className="node-icon" aria-hidden="true" />
            )}
          </button>
          {/* A name is a chat's words: a text node. Its report's first line on a pointer
              that rests on it (V100-19). */}
          <span className="past-name" title={row.says === "" ? undefined : row.says}>
            {row.name}
          </span>
        </span>
      </td>
      <td>
        <span className="dispatch-persona">
          {row.asker_persona !== null && !row.by_person && (
            <PersonaMark persona={row.asker_persona} className="node-icon" />
          )}
          {askedBy(row)}
        </span>
      </td>
      <td>
        <span className="dispatch-persona">
          {row.persona === null ? (
            <>
              <SquareTerminal className="node-icon" aria-hidden="true" />
              No persona
            </>
          ) : (
            <>
              <PersonaMark persona={row.persona} className="node-icon" />
              {row.persona}
            </>
          )}
        </span>
      </td>
      <td>
        <span className="past-end">
          {/* The mark and the word a chat's row and a finished row say this end in (#1484);
              the core's own word beside it where it says more. */}
          {state !== undefined && <StateShown shown={state} />}
          {more !== undefined && <span className="outcome">{more}</span>}
          {row.reopened && <span className="outcome">reopened</span>}
        </span>
      </td>
      <td>{row.duration}</td>
      <td>
        {row.place}
        {row.branch !== null && <span className="own-branch">{` · own branch ${row.branch}`}</span>}
        {row.asked_from !== null && (
          <span className="own-branch">{` · asked from ${row.asked_from}`}</span>
        )}
      </td>
      <td>
        {canAct && (
          <button
            type="button"
            className="dispatch-discard"
            tabIndex={0}
            aria-disabled={cannot !== undefined || busy || undefined}
            aria-label={`Reopen ${row.name}`}
            aria-description={cannot}
            title={
              cannot ??
              "Resumes its conversation as an ordinary chat with a tab. It is no longer a task: it sends no report, and the chat that asked is not told."
            }
            onClick={cannot === undefined ? onReopen : undefined}
          >
            Reopen
          </button>
        )}
      </td>
    </tr>
  );
}

/** An opened row: what was read for it. Every word of a chat's is a text node here. */
function Opened({
  row,
  said,
  onOpenRecord,
}: {
  row: PastTask;
  said: { read?: PastTaskRead; trouble?: string };
  onOpenRecord: (path: string, title: string) => void;
}) {
  const record = row.session_record;
  const facts = (
    <p className="dispatch-facts">
      {[
        `started ${localAt(row.started)}`,
        ...(row.ended === null ? [] : [`ended ${localAt(row.ended)}`]),
        `asked by ${askedBy(row)}`,
        ...(row.asker_persona === null ? [] : [`as ${row.asker_persona}`]),
      ].join(" · ")}
    </p>
  );
  if (said.trouble !== undefined) {
    return (
      <>
        {facts}
        <p className="trouble" role="alert">
          {said.trouble}
        </p>
      </>
    );
  }
  if (said.read === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading its report…
      </p>
    );
  }
  const { read } = said;
  return (
    <div className="past-read" role="region" aria-label={`Report and brief of ${row.name}`}>
      {facts}
      {/* Where its session record's link is, and where a link to its Activity (#1495) goes. */}
      <p className="past-links">
        {record === null ? (
          <span className="none">Its chat wrote no session record.</span>
        ) : (
          <button
            type="button"
            className="vault-secret"
            tabIndex={0}
            onClick={() => onOpenRecord(record, row.name)}
          >
            Open its session record
          </button>
        )}
      </p>
      <h3>Report</h3>
      <Clipped text={read.report} what="report" of={row.name} />
      {/* The task's own words for what it changed, as its claim: plain text until a task's
          changes have a view of their own (#1511). */}
      {(read.changed !== null || read.files.length > 0 || read.commits.length > 0) && (
        <>
          <h3>What it says changed</h3>
          {read.changed !== null && <Clipped text={read.changed} what="changes" of={row.name} />}
          {read.files.length > 0 && <pre>{`Files:\n${read.files.join("\n")}`}</pre>}
          {read.commits.length > 0 && <pre>{`Commits:\n${read.commits.join("\n")}`}</pre>}
        </>
      )}
      {/* Inline for now: the Brief panel (#1494) is where a brief is read once it lands. */}
      <h3>Brief</h3>
      <Clipped text={read.brief} what="brief" of={row.name} />
      {read.cannot_reopen !== null && <p className="none">{read.cannot_reopen}</p>}
    </div>
  );
}

/**
 * A chat's words, **as text**: one text node in a `pre`, so nothing in a report or a brief is
 * ever read as markup. Long text is clipped, and says so with the one press that shows it all.
 */
function Clipped({ text, what, of }: { text: string; what: string; of: string }) {
  const [all, setAll] = useState(false);
  const short = clipped(text);
  if (text === "") return <p className="none">{`It has no ${what}.`}</p>;
  if (short === undefined) return <pre>{text}</pre>;
  return (
    <>
      <pre data-clipped={!all || undefined}>{all ? text : `${short}…`}</pre>
      <button
        type="button"
        className="past-more"
        tabIndex={0}
        aria-expanded={all}
        aria-label={all ? `Show less of the ${what} of ${of}` : `Show all of the ${what} of ${of}`}
        onClick={() => setAll((was) => !was)}
      >
        {all ? "Show less" : "Show all"}
      </button>
    </>
  );
}
