import { useCallback, useEffect, useMemo, useState } from "react";
import { commands, type FinishedTask, type PlaneId } from "./bindings";

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
