/**
 * **A limit, said where it binds** (#1498, V100-54).
 *
 * A session's tab menu ends with how many of its tasks run against the limit in force for it,
 * `4 of 6 running`: the count a dispatch from it is decided over and the limit it is decided
 * by, both the core's (`OpenChat.tasks_running`, `OpenChat.tasks_limit`). Nothing is said
 * where nothing counts against the limit: a session whose tasks have all finished has nothing
 * a limit holds back. A refusal is said on the session's row (`ListedChat.atLimit`).
 *
 * **Where another limit binds, the footer says that one too** (#1540): its chain's, a
 * persona's, another workspace's, or its token figure, in the row's own words
 * (`AtLimit.row`). It is left out only where the footer's own numbers already say that very
 * limit: the chat's own running limit (`AtLimit.own`), at the footer's number, with as many
 * running. A chat nobody is at counts its handoffs too, and a held chat or one on the default
 * persona can be held to another number than the footer's: those are said.
 */
import type { ChatRow, ListedChat } from "./chatsTree";

/** `4 of 6 running`, for a chat the core gave both numbers for, with a task counted; and the
 *  limit that binds it where that is another than this count's (`at its chain's limit (16
 *  live)`). */
export function runningSaid(
  chat: Pick<ListedChat, "tasksLimit" | "tasksRunning" | "atLimit"> | undefined,
): string | undefined {
  const said = [countedSaid(chat), bindsSaid(chat)].filter((one) => one !== undefined);
  return said.length === 0 ? undefined : said.join(" · ");
}

function countedSaid(
  chat: Pick<ListedChat, "tasksLimit" | "tasksRunning"> | undefined,
): string | undefined {
  const limit = chat?.tasksLimit;
  const running = chat?.tasksRunning;
  if (limit === null || limit === undefined || running === null || running === undefined)
    return undefined;
  if (running === 0) return undefined;
  return `${running} of ${limit} running`;
}

function bindsSaid(
  chat: Pick<ListedChat, "tasksLimit" | "tasksRunning" | "atLimit"> | undefined,
): string | undefined {
  const at = chat?.atLimit;
  if (at === null || at === undefined) return undefined;
  const footerSaysIt =
    at.own && chat?.tasksLimit === at.limit && (chat.tasksRunning ?? 0) >= at.limit;
  return footerSaysIt ? undefined : at.row;
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
