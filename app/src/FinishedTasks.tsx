import { memo, useContext, useState } from "react";
import { ChevronDown, ChevronRight, SquareTerminal } from "lucide-react";
import type { FinishedTask } from "./bindings";
import type { Catalogued, Offer } from "./actions";
import { Menued } from "./Menus";
import { briefTitle, useOpenBrief } from "./Brief";
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
 * ever read as markup. **Brief** shows what it was sent, from the same record (#1494).
 * **Reopen** resumes its conversation as an ordinary chat with a tab; it is then no longer a
 * task, and the chat that asked is told nothing.
 *
 * **Changes** opens what the task changed, and no other task's, in a tab of its own (#1511,
 * V100-66); **Review changes** for a task that worked on its own branch, whose tab offers the
 * person's Merge and Discard. The report's line about what changed opens the same tab.
 *
 * **A row has a menu** (#1534): for a task on its own branch, the catalogue's Merge… and, under
 * the line, Discard branch…, which ask the Changes tab's own questions.
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
  onChanges,
  offers,
  onPress,
}: {
  /** The chat that asked for them, by the name its row has. */
  asker: string;
  /** The level its tasks are drawn at: one below its own. */
  level: number;
  tasks: readonly FinishedTask[];
  /** Takes these rows away, and forgets what their tasks said (#1520): their records stay. */
  onClear: (ids: string[]) => void;
  /** Reopens one as an ordinary chat; answers why not, where it could not. */
  onReopen: (task: FinishedTask) => Promise<string | undefined>;
  /** A row was opened to read its report: a task that failed has then been looked at, and
   *  its needs-you item goes (#1491). */
  onLook?: (task: FinishedTask) => void;
  /** Opens what a task changed, in a tab of its own; no Changes is offered without it. */
  onChanges?: (task: FinishedTask) => void;
  /** The catalogue a row's menu is drawn from (#1534); no menu without it. */
  offers?: Catalogued;
  onPress?: (offer: Offer) => void;
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
            onChanges={onChanges}
            offers={offers}
            onPress={onPress}
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
              title="Takes these rows away, and forgets what these tasks and this chat said to each other. Each brief and report stays on its dispatch record."
              onClick={() => onClear(folded.map((task) => task.id))}
            >
              Clear finished
            </button>
          </div>
        )}
        {open &&
          folded.map((task) => (
            <FinishedRow
              key={task.id}
              task={task}
              onReopen={onReopen}
              onChanges={onChanges}
              offers={offers}
              onPress={onPress}
            />
          ))}
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
  onChanges,
  offers,
  onPress,
}: {
  task: FinishedTask;
  onClear?: (ids: string[]) => void;
  onReopen: (task: FinishedTask) => Promise<string | undefined>;
  onLook?: (task: FinishedTask) => void;
  onChanges?: (task: FinishedTask) => void;
  offers?: Catalogued;
  onPress?: (offer: Offer) => void;
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
  /** Opens the brief it was sent (`Brief.tsx`), where the window around this row has one. */
  const openBrief = useOpenBrief();
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
  const row = (
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
        {openBrief !== undefined && (
          <button
            type="button"
            className="finished-brief"
            tabIndex={0}
            aria-label={briefTitle(task.name)}
            aria-haspopup="dialog"
            title="What this task was sent, as it was sent."
            onClick={() => openBrief({ dispatch: task.id, name: task.name })}
          >
            Brief
          </button>
        )}
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
        {waits === null && onChanges !== undefined && (
          <button
            type="button"
            className="finished-changes"
            tabIndex={0}
            aria-label={`${task.branch === null ? "Changes" : "Review changes"} of ${task.name}`}
            title={
              task.branch === null
                ? "Opens the files this task's edit tools wrote that are still uncommitted. Edits made by a shell command, and what it committed, are not listed."
                : "Opens what its own branch changed, with Merge and Discard."
            }
            onClick={() => onChanges(task)}
          >
            {task.branch === null ? "Changes" : "Review changes"}
          </button>
        )}
        {waits === null && onClear !== undefined && (
          <button
            type="button"
            className="finished-clear"
            tabIndex={0}
            aria-label={`Clear ${task.name}`}
            title="Takes this row away, and forgets what this task and its chat said to each other. Its brief and report stay on its dispatch record."
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
          {/* The task's own words for what it changed, as text; the line opens what purlis
              found it changed (#1511). */}
          {task.changed !== null &&
            (onChanges === undefined ? (
              <p className="report-text report-changed">Changed: {task.changed}</p>
            ) : (
              <p className="report-text report-changed">
                <button
                  type="button"
                  className="report-changed-link"
                  tabIndex={0}
                  title="Opens what purlis can tell this task changed"
                  onClick={() => onChanges(task)}
                >
                  Changed:
                </button>{" "}
                {task.changed}
              </p>
            ))}
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
  // Its menu (#1534): the catalogue's rows for this task, where it has any.
  if (offers === undefined || onPress === undefined) return row;
  return (
    <Menued on={{ on: "finished", id: task.id }} offers={offers} onPress={onPress}>
      {row}
    </Menued>
  );
}
