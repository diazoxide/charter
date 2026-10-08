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
 * **The finished row of the task called `name` that chat `asker` asked for** (#1486's ended
 * view reads it), or nothing. A finished row names its task and the chat that asked and not
 * the number its chat had, so it is found by those two; where two of a chat's finished tasks
 * have one name, nothing says which is meant, and none is answered.
 */
export function finishedOf(
  finished: ReadonlyMap<number, readonly FinishedTask[]>,
  asker: number,
  name: string,
): FinishedTask | undefined {
  const named = (finished.get(asker) ?? []).filter((task) => task.name === name);
  return named.length === 1 ? named[0] : undefined;
}

const NONE: readonly FinishedTask[] = [];

/**
 * The finished tasks of `plane`'s open chats, by asking chat, read when the view mounts, each
 * time `changed` changes (the window's own sign that a chat started, ended or closed), and
 * again on `read`. A list that cannot be read is no list: nothing is drawn as finished that the
 * core did not say is.
 */
export function useFinishedTasks(
  plane: PlaneId,
  changed: unknown,
): { finished: ReadonlyMap<number, FinishedTask[]>; read: () => void } {
  const [known, setKnown] = useState<{ plane?: PlaneId; tasks: readonly FinishedTask[] }>({
    tasks: NONE,
  });
  const [asked, setAsked] = useState(0);
  useEffect(() => {
    let gone = false;
    void commands
      .finishedTasks(plane)
      .then((answer) => {
        if (gone || answer.status !== "ok" || !Array.isArray(answer.data)) return;
        setKnown({ plane, tasks: answer.data });
      })
      // A list that cannot be read shows none.
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [plane, changed, asked]);
  const tasks = known.plane === plane ? known.tasks : NONE;
  const finished = useMemo(() => byAsker(tasks), [tasks]);
  const read = useCallback(() => setAsked((count) => count + 1), []);
  return { finished, read };
}
