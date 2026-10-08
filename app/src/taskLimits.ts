/**
 * **A limit, said where it binds** (#1498, V100-54).
 *
 * A session's tab menu ends with how many of its tasks run against the limit in force for it,
 * `4 of 6 running`: the count a dispatch from it is decided over and the limit it is decided
 * by, both the core's (`OpenChat.tasks_running`, `OpenChat.tasks_limit`). Nothing is said
 * where nothing counts against the limit: a session whose tasks have all finished has nothing
 * a limit holds back. A refusal is said on the session's row (`ListedChat.atLimit`).
 */
import type { ChatRow, ListedChat } from "./chatsTree";

/** `4 of 6 running`, for a chat the core gave both numbers for, with a task counted. */
export function runningSaid(
  chat: Pick<ListedChat, "tasksLimit" | "tasksRunning"> | undefined,
): string | undefined {
  const limit = chat?.tasksLimit;
  const running = chat?.tasksRunning;
  if (limit === null || limit === undefined || running === null || running === undefined)
    return undefined;
  if (running === 0) return undefined;
  return `${running} of ${limit} running`;
}

/** Each tab's footer, by tab: what its own session (the chat at the top of its list) says. */
export function runningByTab(
  rowsByTab: ReadonlyMap<number, readonly ChatRow[]>,
): ReadonlyMap<number, string> {
  const said = new Map<number, string>();
  for (const [tab, rows] of rowsByTab) {
    const own = rows.find((row) => row.level === 1);
    const words = runningSaid(own);
    if (words !== undefined) said.set(tab, words);
  }
  return said;
}
