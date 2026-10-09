import { useContext } from "react";
import type { Shown } from "./bindings";
import { useChatsHere, useChatsSelect, waitsOnItsPrompt } from "./chatState";
import { Notice, NoticeOf } from "./Notice";

/**
 * **A chat that is not on screen is stopped on a prompt** (reported 2026-10-09): its harness
 * asked the person's permission for a tool call, or asked them something, in a terminal nobody
 * is looking at, and everything it does waits on that answer. Said on the pane of the tab it
 * lives in — for a task, its session's tab — naming whose it is by its whole path (`NoticeOf`),
 * with **Show the task**, which switches the tab to it.
 *
 * - **Only for a chat that is not on screen.** On screen, the prompt is in its own pane, and a
 *   second line about it would say it twice. So nothing is drawn where no `NoticeOf` is given.
 * - **From two sources, one Notice.** While purlis holds a permission prompt on the chat's hook
 *   (about a minute, HP-6), the Notice says what it asks: the ask's line as the core summed it
 *   up (`asking::shown`: one line, every credential shape masked), drawn as text and nothing
 *   else, and "and N more" past the first. Past that hold, the harness's own prompt decides in
 *   the chat's pane, and the board still says the chat is stopped on it (`Moved.asking`,
 *   #1601): the Notice stays, without the prompt's words, which only the hold carried.
 * - **It answers nothing.** The person answers in the chat's own pane, where the harness shows
 *   the prompt in full, or from the title bar's needs-you list: purlis never approves a
 *   harness's prompt in the person's place.
 * - **It goes when the prompt does**: answered in the window, got past in the chat's pane (a
 *   tool of its own began after the prompt and one came back), the turn that asked ended, its
 *   next prompt, or the chat ended.
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
  const stopped = useChatsSelect(useChatsHere(), (states) => waitsOnItsPrompt(states, session));
  if (of === null) return null;
  const held = asks.filter((one) => one.session === session);
  const first = held[0];
  if (first === undefined && !stopped) return null;
  // Which chat it is comes from the core's lineage (`NoticeOf.task`), never from its name.
  const fixes = [
    { label: of.task === true ? "Show the task" : "Show it", onPress: of.onGo },
  ] as const;
  return (
    // The Notice says whose it is in its own sentence and has its own way to the chat, so the
    // shared "whose:" and "Go to it" are not drawn a second time.
    <NoticeOf.Provider value={null}>
      {first === undefined ? (
        <Notice
          cause={`task-permission:${session}:asked`}
          at="pane"
          tone="trouble"
          label={`${of.whose} is waiting on you`}
          fixes={fixes}
        >
          {of.whose} is stopped on a prompt and waiting on you. Answer it in its own pane.
        </Notice>
      ) : (
        <Notice
          cause={`task-permission:${session}:${first.ask}`}
          at="pane"
          tone="trouble"
          label={`${of.whose} is waiting on you for a permission`}
          fixes={fixes}
        >
          {of.whose} is waiting on you for a permission: “
          <span className="ask-says">{first.says}</span>”
          {held.length > 1 && ` and ${held.length - 1} more`}. Answer it in its own pane.
        </Notice>
      )}
    </NoticeOf.Provider>
  );
}
