import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type TasksUsed } from "./bindings";
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
 * open, and when the pointer comes onto a task's row.
 */

/** What one ask names: the session's own chat, its open tasks, and its ended ones. */
export type UsedAsk = {
  own: number | null;
  chats: number[];
  finished: string[];
};

/** Reads what an ask's chats used. Answers nothing where the core could not say. */
export type UsedReader = (ask: UsedAsk) => Promise<TasksUsed | undefined>;

/** The prefix of an ended task's line that carries its dispatch record's id (`PlaneView`). */
const FINISHED = "finished:";

/**
 * **The ask for a tab's menu**: every line of it, the session's own chat as `own` (counted in
 * the tokens and not as a task), each open task, and each ended task by its record, or by its
 * chat's number where it has no finished row yet.
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

/**
 * A line's tokens in `used`: its words (`15k in, 4k out`), `null` where its harness reported
 * none, `undefined` where it has not been read.
 */
export function tokensOfLine(used: TasksUsed | undefined, line: Line): string | null | undefined {
  if (used === undefined) return undefined;
  if (line.key.startsWith(FINISHED)) {
    const id = line.key.slice(FINISHED.length);
    return used.finished.find((one) => one.id === id)?.tokens;
  }
  if (line.session === undefined) return undefined;
  return used.chats.find((one) => one.session === line.session)?.tokens;
}

/**
 * **A task's tokens, as its hover says them**: the figure, or a dash where its harness reported
 * none (V100-71), never a zero. Nothing before it has been read.
 */
export function tokensSaid(tokens: string | null | undefined): string | undefined {
  if (tokens === undefined) return undefined;
  return tokens === null ? "Tokens: — (its harness reported none)" : `Tokens: ${tokens}`;
}

/** The window's reader: one ask of the core, through the project the chats are in. */
export async function readUsed(plane: string, ask: UsedAsk): Promise<TasksUsed | undefined> {
  try {
    const answer = await commands.tasksUsed(plane, ask.own, ask.chats, ask.finished);
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
 * **A task row's tokens on hover**: read as the pointer comes onto the row, once per coming,
 * and said on its title. `what` is the open task's number or the ended task's record.
 */
export function useTokensOnHover(what: { chat: number } | { finished: string }): {
  said: string | undefined;
  onPointerEnter: () => void;
} {
  const { plane } = useChatsHere();
  const [said, setSaid] = useState<{ key: string; said: string | undefined }>();
  const key = "chat" in what ? `chat:${what.chat}` : `finished:${what.finished}`;
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  const onPointerEnter = useCallback(() => {
    if (plane === undefined) return;
    const ask: UsedAsk =
      "chat" in what
        ? { own: null, chats: [what.chat], finished: [] }
        : { own: null, chats: [], finished: [what.finished] };
    void readUsed(plane, ask).then((used) => {
      if (!live.current || used === undefined) return;
      const tokens =
        "chat" in what
          ? used.chats.find((one) => one.session === what.chat)?.tokens
          : used.finished.find((one) => one.id === what.finished)?.tokens;
      setSaid({ key, said: tokensSaid(tokens) });
    });
    // `what` is read through `key`, which says all of it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [plane, key]);
  return { said: said?.key === key ? said.said : undefined, onPointerEnter };
}
