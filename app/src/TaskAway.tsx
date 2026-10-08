import { useEffect, useRef } from "react";
import { shownState, type Shown } from "./shownState";
import { StateShown } from "./StateShown";
import type { Crumbs } from "./tabChats";

/** Why a pane cannot draw the task its tab shows. */
export type Away =
  /** The list of chats is not read yet: nothing is known of the task, so nothing is drawn. */
  | { why: "unread" }
  /** Its program has ended, or it was closed. */
  | { why: "ended"; crumbs: Crumbs }
  /** It is running, and no longer below this tab's session: a chat between them has ended, or
   *  it has a tab of its own. */
  | { why: "moved"; crumbs: Crumbs };

/**
 * **How a task that ended says how it ended**, from what its row last said of it (#1484's one
 * function, with its program over): `done`, `failed`, `cancelled` or `ended without a report`.
 */
export function endedState(crumbs: Crumbs) {
  const shown = crumbs.path[crumbs.path.length - 1];
  return shownState({
    board: "done",
    needsYou: false,
    task: { report: shown.report ?? "owed", outcome: shown.outcome, asking: null },
    harness: shown.harness,
  });
}

/**
 * **A pane whose tab shows a task that is not there to be drawn** (#1486, V100-37).
 *
 * **The pane never changes what it shows by itself.** A task the person was reading that ends,
 * or is closed from elsewhere, stays on screen as ended: its name in the breadcrumb over this,
 * how it ended, and one button back to the session's own chat. Going back is the person's
 * press. The keyboard is put on that button, on purpose: the terminal that had it is gone, and
 * a key typed must not land anywhere the person cannot see.
 *
 * **No terminal is drawn, so nothing typed reaches a chat.** That is also what a pane does for
 * a task that is still running and is no longer below this tab's session: the pane cannot say
 * the path to it any more, and typing into a chat under a path that is not true is the mistake
 * the breadcrumb exists to prevent.
 *
 * Its report is on its finished row in the Chats list, under the session that asked, which is
 * where the person reads and clears finished tasks; `report` draws it here too, as text, where
 * the window holds it.
 *
 * **It says how the task ended as its finished row says it** (#1485): `finished` is that row's
 * state and the core's own word beside it, where the row draws one. Without a finished row it
 * says what the task's own row last said of it (`endedState`).
 */
export function TaskAway({
  away,
  focused,
  report,
  finished,
  onBack,
  onOpenAlone,
}: {
  away: Exclude<Away, { why: "unread" }>;
  /** Whether this is the pane with the keyboard: its button takes it. */
  focused: boolean;
  /** The task's report, where the window holds it. Text, never markup. */
  report?: string;
  /** How its finished row says it ended, where it has one: the state, and the core's word
   *  beside it where that row draws one. */
  finished?: { state: Shown | undefined; more: string | undefined };
  /** Back to the session's own chat, in this pane. */
  onBack: () => void;
  /** Opens the task, which is still running, in a tab of its own. */
  onOpenAlone: () => void;
}) {
  const back = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (focused) back.current?.focus();
  }, [focused]);
  const [own] = away.crumbs.path;
  const task = away.crumbs.path[away.crumbs.path.length - 1];
  const ended = finished === undefined ? endedState(away.crumbs) : finished.state;
  return (
    <div className="pane task-away" data-testid="task-away" data-session={task.session}>
      <div className="task-away-says">
        {away.why === "ended" ? (
          <p>
            <strong>{task.name}</strong> has ended
            {ended === undefined ? "." : ":"} {ended !== undefined && <StateShown shown={ended} />}
            {finished?.more !== undefined && <span className="outcome"> {finished.more}</span>}
          </p>
        ) : (
          <p>
            <strong>{task.name}</strong> is still running, and this tab can no longer show it: the
            chat that asked for it has ended, or it has a tab of its own.
          </p>
        )}
        {away.why === "ended" && report === undefined && (
          <p className="honest">
            Its terminal went with its program. Its report, where it sent one, is on its row under{" "}
            {own.name} in the Chats list.
          </p>
        )}
        {report !== undefined && (
          <section aria-label={`Report from ${task.name}`}>
            <pre className="block-report-draft task-away-report">{report}</pre>
          </section>
        )}
      </div>
      <div className="task-away-ways">
        <button type="button" ref={back} tabIndex={0} onClick={onBack}>
          Back to {own.name}
        </button>
        {away.why === "moved" && (
          <button type="button" tabIndex={0} onClick={onOpenAlone}>
            Open {task.name} in a tab of its own
          </button>
        )}
      </div>
    </div>
  );
}
