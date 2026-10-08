import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type Scope, type TasksUsed, type Unsaid } from "./bindings";
import { useChatsHere } from "./chatState";
import type { Line } from "./tabTasks";

/**
 * **What a session's tasks used** (#1500, V100-43): which chats a tab's menu asks the core
 * about, and how a task's tokens are said on hover. No money figure is ever drawn.
 *
 * The figures are the core's (`tasksused.rs`): a chat's tokens are what its harness reported
 * for its conversation, an ended task's what its record kept, and the total line is added up
 * and spelled there, so a line and the total cannot spell one number two ways. **Nothing here
 * polls**: a figure is read when the menu opens, when a chat of it changes state while it is
 * open, and when the pointer comes to rest on a task's row.
 */

/** What one ask names: the session's own chat, its open tasks, and its ended ones. */
export type UsedAsk = {
  own: number | null;
  chats: number[];
  finished: string[];
};

/** Reads what an ask's chats used, for the tab's menu. Answers nothing where the core could
 *  not say. */
export type UsedReader = (ask: UsedAsk) => Promise<TasksUsed | undefined>;

/** One chat's figure as the core answers it: its words, or why there are none. */
export type LineUsed = { tokens: string | null; unsaid: Unsaid | null };

/** The prefix of an ended task's line that carries its dispatch record's id (`PlaneView`). */
const FINISHED = "finished:";

/**
 * **The ask for a tab's menu**: every line of it, the session's own chat as `own` (said beside
 * the total, not in it), each open task, and each ended task by its record, or by its chat's
 * number where it has no finished row yet.
 */
export function askOf(lines: readonly Line[]): UsedAsk {
  const ask: UsedAsk = { own: null, chats: [], finished: [] };
  for (const line of lines) {
    if (line.key.startsWith(FINISHED)) ask.finished.push(line.key.slice(FINISHED.length));
    else if (line.row !== undefined && line.row.level === 1) ask.own = line.row.session;
    else if (line.session !== undefined) ask.chats.push(line.session);
  }
  return ask;
}

/** A line's figure in `used`, or `undefined` where it has not been read. */
export function tokensOfLine(used: TasksUsed | undefined, line: Line): LineUsed | undefined {
  if (used === undefined) return undefined;
  if (line.key.startsWith(FINISHED)) {
    const id = line.key.slice(FINISHED.length);
    return used.finished.find((one) => one.id === id);
  }
  if (line.session === undefined) return undefined;
  return used.chats.find((one) => one.session === line.session);
}

/** Why a figure is a dash, in words that are true of every case behind it. */
const UNSAID: Readonly<Record<Unsaid, string>> = {
  not_yet: "nothing reported yet",
  nothing: "nothing reported",
  not_known: "not known",
};

/**
 * **A task's tokens, as its hover says them**: the figure, or a dash and why (V100-71), never
 * a zero. Nothing before it has been read.
 */
export function tokensSaid(figure: LineUsed | undefined): string | undefined {
  if (figure === undefined) return undefined;
  if (figure.tokens !== null) return `Tokens: ${figure.tokens}`;
  return `Tokens: — (${UNSAID[figure.unsaid ?? "not_known"]})`;
}

/** The window's reader: one ask of the core, through the project the chats are in. */
export async function readUsed(
  plane: string,
  scope: Scope,
  ask: UsedAsk,
): Promise<TasksUsed | undefined> {
  try {
    const answer = await commands.tasksUsed(plane, scope, ask.own, ask.chats, ask.finished);
    // Anything that is not this answer (a test's catch-all, an older core) draws nothing.
    const found: unknown = answer.status === "ok" ? answer.data : undefined;
    return found !== null &&
      typeof found === "object" &&
      Array.isArray((found as TasksUsed).chats) &&
      Array.isArray((found as TasksUsed).finished)
      ? (found as TasksUsed)
      : undefined;
  } catch {
    // A read that failed draws nothing: an unknown is never drawn as a zero.
    return undefined;
  }
}

/**
 * How long the pointer stays on a row before its figure is read: a pointer running down the
 * list reads nothing, and the tooltip comes up later than this anyway.
 */
export const HOVER_READ_MS = 250;

/**
 * **A task row's tokens on hover**: read once the pointer has stayed on the row for
 * {@link HOVER_READ_MS}, one read at a time, and said on its title. Leaving the row first
 * reads nothing. `what` is the open task's number or the ended task's record.
 */
export function useTokensOnHover(what: { chat: number } | { finished: string }): {
  said: string | undefined;
  onPointerEnter: () => void;
  onPointerLeave: () => void;
} {
  const { plane } = useChatsHere();
  const [said, setSaid] = useState<{ key: string; said: string | undefined }>();
  const key = "chat" in what ? `chat:${what.chat}` : `finished:${what.finished}`;
  const waiting = useRef<number | undefined>(undefined);
  const reading = useRef(false);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
      window.clearTimeout(waiting.current);
    };
  }, []);
  const onPointerLeave = useCallback(() => {
    window.clearTimeout(waiting.current);
    waiting.current = undefined;
  }, []);
  const onPointerEnter = useCallback(() => {
    if (plane === undefined) return;
    window.clearTimeout(waiting.current);
    waiting.current = window.setTimeout(() => {
      waiting.current = undefined;
      if (reading.current) return;
      reading.current = true;
      const ask: UsedAsk = key.startsWith(FINISHED)
        ? { own: null, chats: [], finished: [key.slice(FINISHED.length)] }
        : { own: null, chats: [Number(key.slice("chat:".length))], finished: [] };
      void readUsed(plane, "hover", ask)
        .then((used) => {
          if (!live.current || used === undefined) return;
          const figure = key.startsWith(FINISHED) ? used.finished[0] : used.chats[0];
          setSaid({ key, said: tokensSaid(figure) });
        })
        .finally(() => {
          reading.current = false;
        });
    }, HOVER_READ_MS);
  }, [plane, key]);
  return { said: said?.key === key ? said.said : undefined, onPointerEnter, onPointerLeave };
}
