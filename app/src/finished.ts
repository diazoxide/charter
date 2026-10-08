import { useCallback, useEffect, useMemo, useState } from "react";
import { commands, type FinishedTask, type PlaneId } from "./bindings";
import { shownState, type Shown, type TaskFacts } from "./shownState";

/**
 * **A session's finished tasks** (#1485): the tasks a chat asked for that have ended. A task
 * ends at its report: purlis ends its program, and what stays is a row under the chat that
 * asked, with how it ended and its report.
 *
 * The rows are the core's (`finished_tasks`), read from the dispatch records and so the same
 * after the app is started again. This file reads them and says which fold:
 *
 * - **done and cancelled fold into one line**, "Finished (n)", with Clear finished;
 * - **every other end stays a row of its own** until it is cleared: failed, blocked, ended
 *   without a report, closed by the person. A failure is never hidden behind a count.
 *
 * Clearing takes rows away and nothing else: the records stay, and the Dispatches tab still
 * lists them.
 */

/** A chat's finished tasks, as its rows are drawn: the ones that stand alone, oldest first,
 *  then the ones folded into the one line. */
export type Folded = { alone: FinishedTask[]; folded: FinishedTask[] };

/** Which of `tasks` fold, by the core's own word for each (`FinishedTask.folds`). */
export function foldedOf(tasks: readonly FinishedTask[]): Folded {
  return {
    alone: tasks.filter((task) => !task.folds),
    folded: tasks.filter((task) => task.folds),
  };
}

/** The finished tasks of each asking chat, by its number, in the order the core lists them. */
export function byAsker(tasks: readonly FinishedTask[]): ReadonlyMap<number, FinishedTask[]> {
  const by = new Map<number, FinishedTask[]>();
  for (const task of tasks) by.set(task.asker, [...(by.get(task.asker) ?? []), task]);
  return by;
}

/** A report's first line, which a finished row's tooltip says. */
export function firstLine(report: string): string {
  return report.split("\n").find((line) => line.trim() !== "") ?? "";
}

/**
 * What the record of a task that ended `how` says of it, as a chat's row is told it
 * (`TaskFacts`). The two ends where purlis wrote the report in the task's place sent none
 * themselves: one whose program ended owing it, and one the person stopped (the record's
 * `stopped`). A value a later core sends that this window does not know is handed on as it is,
 * and is not guessed at.
 */
function recordOf(how: FinishedTask["how"]): TaskFacts {
  if (how === "unreported") return { report: "failed", outcome: null, asking: null };
  if (how === "stopped_by_person") return { report: "failed", outcome: "stopped", asking: null };
  return { report: "sent", outcome: how, asking: null };
}

/**
 * **A finished task's state, as every row says a state** (#1484): the word and the shape the
 * one function gives a task with this record, so a task reads the same on its finished row as
 * it did on its chat's row in the moment before its program was ended. Nothing is asked of a
 * board: its program has ended.
 */
export function shownOf(task: Pick<FinishedTask, "how">): Shown | undefined {
  return shownState({
    board: undefined,
    needsYou: false,
    task: recordOf(task.how),
    harness: null,
  });
}

/**
 * The core's own words for how a task ended (`FinishedTask.outcome`), where they say more
 * than its state's word does (a blocked task, one the person closed): what a row draws beside
 * the state. Nothing where the
 * state's word is the core's.
 */
export function qualifierOf(task: Pick<FinishedTask, "how" | "outcome">): string | undefined {
  return task.outcome === shownOf(task)?.word ? undefined : task.outcome;
}

/**
 * **The finished row of the task whose chat was `session`, which chat `asker` asked for**
 * (#1486's ended view reads it), or nothing. **By the number its chat had when it ended, and
 * never by its name**: a chat dispatches again under one name as a matter of course, and a
 * pane must not draw another task's report. A row with no number (a task that finished before
 * this app was started, or an entry that is not an ended task at all) is no pane's.
 */
export function finishedOf(
  finished: ReadonlyMap<number, readonly FinishedTask[]>,
  asker: number,
  session: number,
): FinishedTask | undefined {
  return (finished.get(asker) ?? []).find((task) => task.chat === session);
}

const NONE: readonly FinishedTask[] = [];

/**
 * The finished tasks of `plane`'s open chats, by asking chat, read when the view mounts, each
 * time `changed` changes (the window's own sign that a chat started, ended or closed), and
 * again on `read`. A list that cannot be read is no list: nothing is drawn as finished that the
 * core did not say is.
 *
 * `settled` says the rows are the ones read for this `changed`, or that the read for it has
 * failed: what `useRowsUntilRead` waits for.
 */
export function useFinishedTasks(
  plane: PlaneId,
  changed: unknown,
): { finished: ReadonlyMap<number, FinishedTask[]>; read: () => void; settled: boolean } {
  const [known, setKnown] = useState<{
    plane?: PlaneId;
    tasks: readonly FinishedTask[];
    /** The `changed` these were read for, or the last one a read was answered for. */
    readFor?: unknown;
  }>({ tasks: NONE });
  const [asked, setAsked] = useState(0);
  useEffect(() => {
    let gone = false;
    // A read that gave no list leaves the rows as they were, and is still an answer.
    const unread = () => {
      if (!gone) setKnown((was) => ({ ...was, readFor: changed }));
    };
    void commands
      .finishedTasks(plane)
      .then((answer) => {
        if (gone) return;
        if (answer.status !== "ok" || !Array.isArray(answer.data)) return unread();
        setKnown({ plane, tasks: answer.data, readFor: changed });
      })
      // A list that cannot be read shows none.
      .catch(unread);
    return () => {
      gone = true;
    };
  }, [plane, changed, asked]);
  const tasks = known.plane === plane ? known.tasks : NONE;
  const finished = useMemo(() => byAsker(tasks), [tasks]);
  const read = useCallback(() => setAsked((count) => count + 1), []);
  return { finished, read, settled: known.readFor === changed };
}

/**
 * **A task's row stays until the finished row that replaces it is read.** A task that ends
 * leaves the list of chats one command before its finished row arrives, so the rows below it
 * would move up and then down again within a moment. `rows` is handed on as it is, but for
 * that moment: while a task that was listed is gone from it and the finished rows are not
 * `settled` for it yet, the list as it last stood is drawn. Nothing else is held: a chat that
 * arrives, a rename and a chat that is not a task going are drawn at once.
 */
export function useRowsUntilRead<Row extends { session: number; mode: string | null }>(
  rows: readonly Row[],
  settled: boolean,
): readonly Row[] {
  const [shown, setShown] = useState(rows);
  if (shown === rows) return rows;
  const listed = new Set(rows.map((row) => row.session));
  const waits = !settled && shown.some((row) => row.mode === "task" && !listed.has(row.session));
  if (waits) return shown;
  setShown(rows);
  return rows;
}
