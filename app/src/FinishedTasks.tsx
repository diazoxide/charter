import { memo, useState } from "react";
import { ChevronDown, ChevronRight, SquareTerminal } from "lucide-react";
import type { FinishedTask } from "./bindings";
import { firstLine, foldedOf, qualifierOf, shownOf } from "./finished";
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
 * **Cleared rows go to Past tasks** (#1510): clearing takes a row off this list and leaves its
 * record, and the workspace's Past tasks view is where it is read from then. Clear finished
 * says so, on its button and in the line it leaves behind, with the one press that goes there.
 *
 * Not rows of the tree: there is no chat behind one to bring forward, so the arrows stop on the
 * chats and Tab reaches these, each a button of its own.
 */
export const FinishedTasks = memo(function FinishedTasks({
  asker,
  session,
  level,
  tasks,
  onClear,
  onReopen,
  onPastTasks,
}: {
  /** The chat that asked for them, by the name its row has. */
  asker: string;
  /** That chat, by its number: what {@link onPastTasks} is told. */
  session?: number;
  /** The level its tasks are drawn at: one below its own. */
  level: number;
  tasks: readonly FinishedTask[];
  /** Takes these rows away. The rows and nothing else: their records stay. */
  onClear: (ids: string[]) => void;
  /** Reopens one as an ordinary chat; answers why not, where it could not. */
  onReopen: (task: FinishedTask) => Promise<string | undefined>;
  /** Opens the Past tasks of the workspace chat `asker` works in. */
  onPastTasks?: (asker: number) => void;
}) {
  /** Whether the folded rows are drawn. This window's own, and folded to start with. */
  const [open, setOpen] = useState(false);
  /** How many rows this window cleared from under this chat: what the line left behind says,
   *  until the list is drawn anew. */
  const [cleared, setCleared] = useState(0);
  const clear = (ids: string[]) => {
    setCleared((was) => was + ids.length);
    onClear(ids);
  };
  const past = onPastTasks !== undefined && session !== undefined && (
    <button
      type="button"
      className="finished-past"
      // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
      tabIndex={0}
      aria-label={`See past tasks, from ${asker}`}
      title="Every task dispatched in this workspace that has ended, cleared or not."
      onClick={() => onPastTasks(session)}
    >
      See past tasks
    </button>
  );
  /* Where the cleared rows went, said where they were (#1510). */
  const went = cleared > 0 && (
    <p className="finished-went" role="status">
      {cleared === 1
        ? "Cleared 1 finished task. It stays in Past tasks."
        : `Cleared ${cleared} finished tasks. They stay in Past tasks.`}
      {past}
    </p>
  );
  if (tasks.length === 0)
    return went ? (
      <li role="none" className="finished-tasks" data-level={level}>
        {went}
      </li>
    ) : null;
  const { alone, folded } = foldedOf(tasks);
  return (
    <li role="none" className="finished-tasks" data-level={level}>
      <div role="group" aria-label={`Finished tasks of ${asker}`}>
        {alone.map((task) => (
          <FinishedRow key={task.id} task={task} onClear={clear} onReopen={onReopen} />
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
              title="Takes these rows away. They stay in Past tasks."
              onClick={() => clear(folded.map((task) => task.id))}
            >
              Clear finished
            </button>
          </div>
        )}
        {open &&
          folded.map((task) => <FinishedRow key={task.id} task={task} onReopen={onReopen} />)}
        {/* With the fold open, the way to the ones no longer listed here. */}
        {open && !went && past && <p className="finished-went">{past}</p>}
      </div>
      {went}
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
}: {
  task: FinishedTask;
  onClear?: (ids: string[]) => void;
  onReopen: (task: FinishedTask) => Promise<string | undefined>;
}) {
  const [shown, setShown] = useState(false);
  const [refused, setRefused] = useState<string>();
  const [busy, setBusy] = useState(false);
  const state = shownOf(task);
  const more = qualifierOf(task);
  /** Why Reopen does nothing, where it does nothing: said to a screen reader on the button,
   *  which stays in the Tab order, and in the opened row. A disabled button takes no focus,
   *  so its reason would reach nobody on a keyboard. */
  const cannot = task.reopens ? undefined : NO_CONVERSATION;
  const reopen = () => {
    if (cannot !== undefined || busy) return;
    setBusy(true);
    setRefused(undefined);
    void onReopen(task)
      .then(setRefused)
      .finally(() => setBusy(false));
  };
  return (
    <div className="finished-task" data-how={task.how}>
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
          {/* Cut short in a narrow sidebar (#1499), and whole here for a pointer that rests on
              it; a screen reader is told the text. */}
          <span className="session" title={task.name}>
            {task.name}
          </span>
          {/* How it ended, as every row says a state: the mark and the word a chat's row
            wears (#1484). The core's own word follows where it says more than that word
            does (blocked, closed by the person), so no end is said less exactly here. */}
          {state !== undefined && <StateShown shown={state} />}
          {more !== undefined && <span className="outcome">{more}</span>}
        </button>
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
        {onClear !== undefined && (
          <button
            type="button"
            className="finished-clear"
            tabIndex={0}
            aria-label={`Clear ${task.name}`}
            title="Takes this row away. It stays in Past tasks."
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
