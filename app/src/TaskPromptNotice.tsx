import { useContext } from "react";
import type { Shown } from "./bindings";
import { Notice, NoticeOf } from "./Notice";

/**
 * **A chat that is not on screen is stopped on a permission prompt** (reported 2026-10-09): its
 * harness asked the person's permission for a tool call, in a terminal nobody is looking at,
 * and everything it does waits on that answer. Said on the pane of the tab it lives in — for
 * a task, its session's tab — naming whose it is by its whole path (`NoticeOf`) and what it
 * asks, with **Show the task**, which switches the tab to it.
 *
 * - **Only for a chat that is not on screen.** On screen, the prompt is in its own pane, and a
 *   second line about it would say it twice. So nothing is drawn where no `NoticeOf` is given.
 * - **What it asks is the harness's own words, inert.** The ask's line as the core summed it up
 *   (`asking::shown`: one line, every credential shape masked), drawn as text and nothing else:
 *   a chat's words are data, never markup or a link.
 * - **It answers nothing.** The person answers in the task's own pane, where the harness shows
 *   the prompt in full, or from the title bar's needs-you list: purlis never approves a
 *   harness's prompt in the person's place.
 * - **It goes when the ask does**: answered, withdrawn or timed out, the core's list no longer
 *   holds it.
 */
export function TaskPromptNotice({
  session,
  asks,
}: {
  session: number;
  /** The permission prompts this project's chats hold open on their hooks (HP-6), as the
   *  window last heard them (`permissionAsks.usePermissionAsks`). */
  asks: readonly Shown[];
}) {
  const of = useContext(NoticeOf);
  if (of === null) return null;
  const held = asks.filter((one) => one.session === session);
  const first = held[0];
  if (first === undefined) return null;
  // A task is named by its path (`taskAsks.whoseOf`); the session's own chat, hidden while
  // its tab shows a task, by its name alone.
  const task = of.whose.includes(" (a task of ");
  return (
    // The Notice says whose it is in its own sentence and has its own way to the chat, so the
    // shared "whose:" and "Go to it" are not drawn a second time.
    <NoticeOf.Provider value={null}>
      <Notice
        cause={`task-permission:${session}:${first.ask}`}
        at="pane"
        tone="trouble"
        label={`${of.whose} is waiting on you for a permission`}
        fixes={[{ label: task ? "Show the task" : "Show it", onPress: of.onGo }]}
      >
        {of.whose} is waiting on you for a permission: “
        <span className="ask-says">{first.says}</span>”
        {held.length > 1 && ` and ${held.length - 1} more`}. Answer it in its own pane.
      </Notice>
    </NoticeOf.Provider>
  );
}
