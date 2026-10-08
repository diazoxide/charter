import { memo, useState } from "react";
import { ChevronDown, ChevronRight, SquareTerminal } from "lucide-react";
import type { FinishedTask } from "./bindings";
import { firstLine, foldedOf, shownOf } from "./finished";
import { PersonaMark } from "./PersonaMark";
import { StateShown } from "./StateShown";

/**
 * **A chat's finished tasks, under its row in the Chats section** (#1485).
 *
 * A task ends at its report, and its row stays here as a finished entry. The ones that came
 * out done, and the ones the asking chat cancelled, fold into one line, **Finished (n)**, with
 * **Clear finished**. Every other end is a row of its own until it is cleared, so a failure is
 * never behind a count.
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
}) {
  /** Whether the folded rows are drawn. This window's own, and folded to start with. */
  const [open, setOpen] = useState(false);
  if (tasks.length === 0) return null;
  const { alone, folded } = foldedOf(tasks);
  return (
    <li role="none" className="finished-tasks" data-level={level}>
      <div role="group" aria-label={`Finished tasks of ${asker}`}>
        {alone.map((task) => (
          <FinishedRow key={task.id} task={task} onClear={onClear} onReopen={onReopen} />
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

/** One finished task: its name and how it ended, its report on a press, and Reopen. A row
 *  that stands alone has a Clear of its own; a folded one is cleared with its fold. */
function FinishedRow({
  task,
  onClear,
  onReopen,
}: {
  task: FinishedTask;
  onClear?: (ids: string[]) => void;
  onReopen: (task: FinishedTask) => Promise<string | undefined>;
}) {
  const [shown, setShown] = useState(false);
  const [refused, setRefused] = useState<string>();
  const [busy, setBusy] = useState(false);
  const state = shownOf(task);
  const reopen = () => {
    setBusy(true);
    setRefused(undefined);
    void onReopen(task)
      .then(setRefused)
      .finally(() => setBusy(false));
  };
  return (
    <div className="finished-task" data-outcome={task.outcome}>
      <div className="finished-line">
        <button
          type="button"
          className="finished-name"
          tabIndex={0}
          aria-expanded={shown}
          title={firstLine(task.report) || undefined}
          onClick={() => setShown((was) => !was)}
        >
          {task.persona === null ? (
            <SquareTerminal className="node-icon" aria-hidden="true" />
          ) : (
            <PersonaMark persona={task.persona} />
          )}
          <span className="session">{task.name}</span>
          {/* How it ended, as every row says a state: the mark and the word a chat's row
            wears (#1484). The core's own word follows where it says more than that word
            does (blocked, closed by the person), so no end is said less exactly here. */}
          {state !== undefined && <StateShown shown={state} />}
          {task.outcome !== state?.word && <span className="outcome">{task.outcome}</span>}
        </button>
        <button
          type="button"
          className="finished-reopen"
          tabIndex={0}
          disabled={!task.reopens || busy}
          aria-label={`Reopen ${task.name}`}
          title={
            task.reopens
              ? "Resumes its conversation as an ordinary chat with a tab. It is no longer a task: it sends no report, and the chat that asked is not told."
              : "Its harness named no conversation to resume."
          }
          onClick={reopen}
        >
          Reopen
        </button>
        {onClear !== undefined && (
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
      {shown && (
        <div className="finished-report" role="region" aria-label={`Report of ${task.name}`}>
          {/* Text nodes, every one: a report is a chat's words, and is never markup here. */}
          <p className="report-text">{task.report}</p>
          {task.changed !== null && (
            <p className="report-text report-changed">Changed: {task.changed}</p>
          )}
          <p className="report-where">
            {task.place}
            {task.branch !== null && ` · own branch ${task.branch}`}
          </p>
        </div>
      )}
    </div>
  );
}
