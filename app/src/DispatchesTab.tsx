import { Fragment, useEffect, useState } from "react";
import { ChevronDown, ChevronRight, LoaderCircle, Send, UserRound } from "lucide-react";
import { EmptyState } from "./EmptyState";
import { Notice } from "./Notice";
import { commands, type DispatchRow, type Dispatches, type PlaneId } from "./bindings";
import {
  askersOf,
  EVERY_DISPATCH,
  NO_PERSONA,
  NO_PERSONA_SAID,
  personasOf,
  saidAt,
  shownDispatches,
  type DispatchFilter,
} from "./dispatches";

/** How often the list is read again while a dispatch in it is still running. */
const WHILE_RUNNING_MS = 5000;

/**
 * **A project's dispatches, in a tab of their own** (#1452): the running ones and the past ones
 * this machine still keeps, newest first, narrowed by persona and by the chat that asked.
 *
 * A row's task is its one control. It opens the persona chat while that chat is still open, else
 * the session record the chat wrote, and is plain text when neither is there. The brief and the
 * report are a row's own words and can be long, so they are drawn under the row on a press and
 * not in the table.
 *
 * **Cost is what the chat's harness reported**, relayed by the chat's status line: a figure a
 * chat can alter, so the column says *Cost (reported)* and nothing is decided by it. A row whose
 * harness reports none says so in words, never as a zero.
 */
export function DispatchesTab({
  plane,
  changed,
  onShowChat,
  onOpenRecord,
}: {
  plane: PlaneId;
  /** Bumped when the project changes on disk: the tab reads again. */
  changed: number;
  /** Bring the chat in `session` to the front. */
  onShowChat: (session: number) => void;
  /** Open the session record at `path`, called `title`. */
  onOpenRecord: (path: string, title: string) => void;
}) {
  const [said, setSaid] = useState<{ read?: Dispatches; trouble?: string }>();
  const [filter, setFilter] = useState<DispatchFilter>(EVERY_DISPATCH);
  const [read, setRead] = useState<string>();
  const [again, setAgain] = useState(0);
  useEffect(() => {
    let gone = false;
    void commands
      .dispatches(plane)
      .then((answer) => {
        if (gone) return;
        setSaid(answer.status === "error" ? { trouble: answer.error } : { read: answer.data });
      })
      .catch((err: unknown) => {
        if (!gone) setSaid({ trouble: String(err) });
      });
    return () => {
      gone = true;
    };
  }, [plane, changed, again]);
  // A running dispatch's time, its needs-you count and its end are the app's own writes, which
  // no change in the project's files announces: read again while one is running.
  const running = said?.read?.rows.some((row) => row.outcome === "running") ?? false;
  useEffect(() => {
    if (!running) return;
    const timer = window.setInterval(() => setAgain((was) => was + 1), WHILE_RUNNING_MS);
    return () => window.clearInterval(timer);
  }, [running]);

  if (said === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading the dispatches…
      </p>
    );
  }
  if (said.trouble !== undefined) {
    return (
      <Notice
        cause="dispatches-unread"
        tone="trouble"
        fixes={[{ label: "Read again", onPress: () => setAgain((was) => was + 1) }]}
      >
        {`purlis could not read this project's dispatches: ${said.trouble}`}
      </Notice>
    );
  }
  const rows = said.read?.rows ?? [];
  const undrawn = said.read?.undrawn ?? 0;
  /* Counted and never shown: a record holding characters that draw as nothing or turn the
     words around them, or more text than purlis ever stores, is not one the app wrote. */
  const refused = undrawn > 0 && (
    <p className="none" data-testid="dispatches-undrawn">
      {undrawn === 1
        ? "1 record purlis will not draw: it holds text purlis refuses to put on the screen."
        : `${undrawn} records purlis will not draw: they hold text purlis refuses to put on the screen.`}
    </p>
  );
  if (rows.length === 0) {
    if (refused) return refused;
    return (
      <EmptyState
        mark={Send}
        headline="No dispatches yet"
        body="When a chat hands work to another chat, it is listed here with its brief and its report."
        testid="dispatches-empty"
      />
    );
  }
  const shown = shownDispatches(rows, filter);
  return (
    <>
      <div className="vault-tools dispatches-tools">
        <select
          className="search-scope"
          role="combobox"
          // #190: WebKit leaves a control out of the tab sequence without `tabIndex`.
          tabIndex={0}
          aria-label="Filter by persona"
          value={filter.persona}
          onChange={(e) => setFilter({ ...filter, persona: e.target.value })}
        >
          <option value="">Every persona</option>
          {personasOf(rows).map((persona) => (
            <option key={persona} value={persona}>
              {persona === NO_PERSONA ? NO_PERSONA_SAID : persona}
            </option>
          ))}
        </select>
        <select
          className="search-scope"
          role="combobox"
          tabIndex={0}
          aria-label="Filter by asking chat"
          value={filter.asker}
          onChange={(e) => setFilter({ ...filter, asker: e.target.value })}
        >
          <option value="">Every asking chat</option>
          {askersOf(rows).map((asker) => (
            <option key={asker.key} value={asker.key}>
              {asker.name}
            </option>
          ))}
        </select>
      </div>
      {shown.length === 0 ? (
        // Not the empty state: the project has dispatches, and the filters keep none of them.
        <p className="none">No dispatch matches these filters.</p>
      ) : (
        <table className="vault-secrets dispatches" aria-label="Dispatches">
          <thead>
            <tr>
              <th scope="col">Task</th>
              <th scope="col">Persona</th>
              <th scope="col">Asked by</th>
              <th scope="col">Where</th>
              <th scope="col">Outcome</th>
              <th scope="col">Duration</th>
              <th scope="col">Needed you</th>
              {/* What the chat's harness reported, not a figure purlis measured. */}
              <th scope="col">Cost (reported)</th>
            </tr>
          </thead>
          <tbody>
            {shown.map((row) => (
              <Fragment key={row.id}>
                <tr data-testid={`dispatch-${row.id}`} data-outcome={row.outcome}>
                  <td>
                    <span className="dispatch-task">
                      <button
                        type="button"
                        className="dispatch-more"
                        tabIndex={0}
                        aria-expanded={read === row.id}
                        aria-label={
                          read === row.id
                            ? `Hide the brief and report of ${row.task}`
                            : `Show the brief and report of ${row.task}`
                        }
                        onClick={() => setRead(read === row.id ? undefined : row.id)}
                      >
                        {read === row.id ? (
                          <ChevronDown className="node-icon" aria-hidden="true" />
                        ) : (
                          <ChevronRight className="node-icon" aria-hidden="true" />
                        )}
                      </button>
                      <Task row={row} onShowChat={onShowChat} onOpenRecord={onOpenRecord} />
                    </span>
                  </td>
                  <td>
                    {row.persona === null ? (
                      NO_PERSONA_SAID
                    ) : (
                      <span className="dispatch-persona">
                        <PersonaMark persona={row.persona} className="node-icon" />
                        {row.persona}
                      </span>
                    )}
                  </td>
                  <td>{row.by_person ? `you, from ${row.asker}` : row.asker}</td>
                  <td title={row.folder ?? undefined}>{row.place}</td>
                  <td>{row.outcome}</td>
                  <td>{row.duration}</td>
                  <td>{row.needed_you}</td>
                  <td title={row.tokens ?? undefined}>{row.cost ?? "not reported"}</td>
                </tr>
                {read === row.id && (
                  <tr className="dispatch-read">
                    <td colSpan={8}>
                      <p className="dispatch-facts">
                        {[
                          row.mode === "task" ? "Task" : "Handoff",
                          `started ${saidAt(row.started)}`,
                          row.ended === null ? "still running" : `ended ${saidAt(row.ended)}`,
                          `asked by ${row.asker}`,
                          row.asker_persona === null ? NO_PERSONA_SAID : `as ${row.asker_persona}`,
                        ].join(" · ")}
                      </p>
                      <h3>Brief</h3>
                      <pre>{row.brief}</pre>
                      <h3>Report</h3>
                      {row.report === null ? (
                        <p className="none">
                          {row.outcome === "running" ? "Not reported yet." : "It ended with none."}
                        </p>
                      ) : (
                        <pre>{row.report}</pre>
                      )}
                    </td>
                  </tr>
                )}
              </Fragment>
            ))}
          </tbody>
        </table>
      )}
      {refused}
    </>
  );
}

/** A row's task: what opens its chat, else its session record, else its name alone. */
function Task({
  row,
  onShowChat,
  onOpenRecord,
}: {
  row: DispatchRow;
  onShowChat: (session: number) => void;
  onOpenRecord: (path: string, title: string) => void;
}) {
  const { open_session: session, session_record: record } = row;
  if (session !== null) {
    return (
      <button
        type="button"
        className="vault-secret"
        tabIndex={0}
        title="Show its chat"
        onClick={() => onShowChat(session)}
      >
        {row.task}
      </button>
    );
  }
  if (record !== null) {
    return (
      <button
        type="button"
        className="vault-secret"
        tabIndex={0}
        title="Open its session record"
        onClick={() => onOpenRecord(record, row.task)}
      >
        {row.task}
      </button>
    );
  }
  return (
    <span className="dispatch-gone" title="Its chat is closed and wrote no session record">
      {row.task}
    </span>
  );
}

/**
 * A persona's mark, **standing in for the shared component** #1449 builds: the same props
 * (`persona`, `className`), so the swap is this function for that import. It draws the glyph a
 * persona's tab carries today.
 */
function PersonaMark({ persona, className }: { persona: string; className?: string }) {
  return (
    <span className="persona-mark" data-persona={persona} title={persona} aria-hidden="true">
      <UserRound className={className} />
    </span>
  );
}
