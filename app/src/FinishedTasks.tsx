import { memo, useContext, useState } from "react";
import { ChevronDown, ChevronRight, SquareTerminal } from "lucide-react";
import type { FinishedTask } from "./bindings";
import { firstLine, foldedOf, qualifierOf, shownOf } from "./finished";
import { PersonaMark } from "./PersonaMark";
import { StateShown } from "./StateShown";
import { useTokensOnHover } from "./tasksUsed";
import { WaitingTaskWaysContext } from "./waitingTasks";

/**
 * **A chat's finished tasks, under its row in the Chats section** (#1485).
 *
 * A task ends at its report, and its row stays here as a finished entry. The ones that came
 * out done, and the ones the asking chat cancelled, fold into one line, **Finished (n)**, with
 * **Clear finished**. Every other end is a row of its own until it is cleared, so a failure is
 * never behind a count.
 *
 * **A task that did not start is one of those rows** (#1497): failed, with the reason shown
 * under it at once, as text. It is never a banner across the window. One the chat that asked
 * tried more than once is one row, which says how often.
 *
 * **A task a launch could not start again is drawn here too, and is not ended** (`waits`): it
 * is still recorded and is tried again at the next launch. Its row offers Try to start again,
 * Review and approve… where its profile waits on that, and End task, which is the only thing
 * that ends it. It has no Reopen and no Clear.
 *
 * **A finished task cannot be typed into**: its program has ended. Pressing its row shows its
 * report, in place, **as text**: every word of it is a text node, so nothing a task wrote is
 * ever read as markup. **Reopen** resumes its conversation as an ordinary chat with a tab; it
 * is then no longer a task, and the chat that asked is told nothing.
 *
 * Not rows of the tree: there is no chat behind one to bring forward, so the arrows stop on the
 * chats and Tab reaches these, each a button of its own.
 */
export const FinishedTasks = memo(function FinishedTasks({
  asker,
  level,
  tasks,
  onClear,
  onReopen,
  onLook,
}: {
  /** The chat that asked for them, by the name its row has. */
  asker: string;
  /** The level its tasks are drawn at: one below its own. */
  level: number;
  tasks: readonly FinishedTask[];
  /** Takes these rows away. The rows and nothing else: their records stay. */
  onClear: (ids: string[]) => void;
  /** Reopens one as an ordinary chat; answers why not, where it could not. */
  onReopen: (task: FinishedTask) => Promise<string | undefined>;
  /** A row was opened to read its report: a task that failed has then been looked at, and
   *  its needs-you item goes (#1491). */
  onLook?: (task: FinishedTask) => void;
}) {
  /** Whether the folded rows are drawn. This window's own, and folded to start with. */
  const [open, setOpen] = useState(false);
  if (tasks.length === 0) return null;
  const { alone, folded } = foldedOf(tasks);
  return (
    <li role="none" className="finished-tasks" data-level={level}>
      <div role="group" aria-label={`Finished tasks of ${asker}`}>
        {alone.map((task) => (
          <FinishedRow
            key={task.id}
            task={task}
            onClear={onClear}
            onReopen={onReopen}
            onLook={onLook}
          />
        ))}
        {folded.length > 0 && (
          <div className="finished-fold">
            <button
              type="button"
              className="finished-count"
              // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
              tabIndex={0}
              aria-expanded={open}
              onClick={() => setOpen((was) => !was)}
            >
              {open ? <ChevronDown aria-hidden="true" /> : <ChevronRight aria-hidden="true" />}
              Finished ({folded.length})
            </button>
            <button
              type="button"
              className="finished-clear"
              tabIndex={0}
              title="Takes these rows away. Their dispatch records stay."
              onClick={() => onClear(folded.map((task) => task.id))}
            >
              Clear finished
            </button>
          </div>
        )}
        {open &&
          folded.map((task) => <FinishedRow key={task.id} task={task} onReopen={onReopen} />)}
      </div>
    </li>
  );
});

/** Why a finished task has no Reopen. */
const NO_CONVERSATION = "It cannot be reopened: its harness named no conversation to resume.";

/** One finished task: its name and how it ended, its report on a press, and Reopen. A row
 *  that stands alone has a Clear of its own; a folded one is cleared with its fold. */
function FinishedRow({
  task,
  onClear,
  onReopen,
  onLook,
}: {
  task: FinishedTask;
  onClear?: (ids: string[]) => void;
  onReopen: (task: FinishedTask) => Promise<string | undefined>;
  onLook?: (task: FinishedTask) => void;
}) {
  // A task that did not start says why at once (#1497): its report is purlis's one sentence,
  // and the reason is the whole of what there is to know about it.
  const [shown, setShown] = useState(task.did_not_start);
  const [refused, setRefused] = useState<string>();
  const [busy, setBusy] = useState(false);
  /** Whether End task was pressed once and waits for the second press. */
  const [ending, setEnding] = useState(false);
  const ways = useContext(WaitingTaskWaysContext);
  const waits = task.waits;
  const state = shownOf(task);
  const more = qualifierOf(task);
  /** Why Reopen does nothing, where it does nothing: said to a screen reader on the button,
   *  which stays in the Tab order, and in the opened row. A disabled button takes no focus,
   *  so its reason would reach nobody on a keyboard. */
  const cannot = task.reopens ? undefined : NO_CONVERSATION;
  // What it used, kept when it ended (#1500): on its name's hover, read as the pointer comes
  // on. A task still waiting to start has no record to read.
  const used = useTokensOnHover({ finished: task.id });
  const hover = [firstLine(task.report), waits === null ? (used.said ?? "") : ""]
    .filter((one) => one !== "")
    .join("\n");
  const doing = (what: Promise<string | undefined>) => {
    setBusy(true);
    setRefused(undefined);
    void what.then(setRefused).finally(() => {
      setBusy(false);
      setEnding(false);
    });
  };
  const reopen = () => {
    if (cannot !== undefined || busy) return;
    setBusy(true);
    setRefused(undefined);
    void onReopen(task)
      .then(setRefused)
      .finally(() => setBusy(false));
  };
  return (
    <div className="finished-task" data-how={task.how} data-task-id={task.id}>
      <div className="finished-line">
        <button
          type="button"
          className="finished-name"
          tabIndex={0}
          aria-expanded={shown}
          title={hover || undefined}
          onPointerEnter={waits === null ? used.onPointerEnter : undefined}
          onPointerLeave={waits === null ? used.onPointerLeave : undefined}
          onClick={() => {
            // Opened to be read: looked at.
            if (!shown) onLook?.(task);
            setShown((was) => !was);
          }}
        >
          {task.persona === null ? (
            <SquareTerminal className="node-icon" aria-hidden="true" />
          ) : (
            <PersonaMark persona={task.persona} />
          )}
          {/* Cut short in a narrow sidebar (#1499), and whole here for a pointer that rests on
              it; a screen reader is told the text. */}
          <span
            className="session"
            title={
              waits === null && used.said !== undefined ? `${task.name}\n${used.said}` : task.name
            }
          >
            {task.name}
          </span>
          {/* How it ended, as every row says a state: the mark and the word a chat's row
            wears (#1484). The core's own word follows where it says more than that word
            does (blocked), so no end is said less exactly here. */}
          {state !== undefined && <StateShown shown={state} />}
          {more !== undefined && <span className="outcome">{more}</span>}
          {task.attempts > 1 && <span className="outcome">tried {task.attempts} times</span>}
        </button>
        {/* Still a task (`waits`): the ways out a chat that did not start has, and never
          Reopen or Clear, which are for one that has ended. */}
        {waits !== null && ways !== null && (
          <>
            <button
              type="button"
              className="finished-reopen"
              tabIndex={0}
              disabled={busy}
              aria-label={`Try to start ${task.name} again`}
              title="Starts it again as the launch tried to. It stays a task of the chat that asked."
              onClick={() => doing(ways.retry(task).then(() => undefined))}
            >
              Try to start again
            </button>
            {waits.approval !== null && (
              <button
                type="button"
                className="finished-reopen"
                tabIndex={0}
                disabled={busy}
                aria-label={`Review and approve what ${task.name} would run`}
                onClick={() => waits.approval !== null && ways.approve(task, waits.approval)}
              >
                Review and approve…
              </button>
            )}
            {ending ? (
              <>
                <button
                  type="button"
                  className="finished-clear"
                  tabIndex={0}
                  disabled={busy}
                  aria-label={`End ${task.name} now`}
                  onClick={() => doing(ways.end(task))}
                >
                  End it
                </button>
                <button
                  type="button"
                  className="finished-clear"
                  tabIndex={0}
                  disabled={busy}
                  aria-label={`Keep ${task.name}`}
                  onClick={() => setEnding(false)}
                >
                  Keep
                </button>
              </>
            ) : (
              <button
                type="button"
                className="finished-clear"
                tabIndex={0}
                disabled={busy}
                aria-label={`End task ${task.name}`}
                title="Ends the task: the chat that asked is told it failed and why, and it is not tried again. Its conversation can still be reopened as an ordinary chat."
                onClick={() => setEnding(true)}
              >
                End task
              </button>
            )}
          </>
        )}
        {/* Reopen and Clear are for a task that has ended. */}
        {waits === null && (
          <button
            type="button"
            className="finished-reopen"
            tabIndex={0}
            aria-disabled={cannot !== undefined || busy || undefined}
            aria-label={`Reopen ${task.name}`}
            aria-description={cannot}
            title={
              cannot ??
              "Resumes its conversation as an ordinary chat with a tab. It is no longer a task: it sends no report, and the chat that asked is not told."
            }
            onClick={reopen}
          >
            Reopen
          </button>
        )}
        {waits === null && onClear !== undefined && (
          <button
            type="button"
            className="finished-clear"
            tabIndex={0}
            aria-label={`Clear ${task.name}`}
            title="Takes this row away. Its dispatch record stays."
            onClick={() => onClear([task.id])}
          >
            Clear
          </button>
        )}
      </div>
      {refused !== undefined && (
        <p className="trouble" role="alert">
          {refused}
        </p>
      )}
      {/* The last Reopen started a chat that ended at once: the core's sentence, kept on the
          row until the next try. */}
      {task.not_reopened !== null && <p className="finished-note">{task.not_reopened}</p>}
      {shown && (
        <div className="finished-report" role="region" aria-label={`Report of ${task.name}`}>
          {/* Text nodes, every one: a report is a chat's words, and is never markup here. */}
          <p className="report-text">{task.report}</p>
          {task.changed !== null && (
            <p className="report-text report-changed">Changed: {task.changed}</p>
          )}
          {waits !== null && (
            <p className="report-text">
              It is still recorded, and will be tried again at the next launch.
            </p>
          )}
          <p className="report-where">
            {task.place}
            {task.branch !== null && ` · own branch ${task.branch}`}
          </p>
          {cannot !== undefined && <p className="report-where">{cannot}</p>}
        </div>
      )}
    </div>
  );
}
