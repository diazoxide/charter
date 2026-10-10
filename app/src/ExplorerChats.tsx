import { memo } from "react";
import { useChatsHere, useChatsSelect } from "./chatState";
import { helpersCountOf, helpersCountSaid, sameHelpersCount } from "./explorerTasks";

/**
 * **A chat's helpers as words on its row** (#1490): a row in the Chats list,
 * `3 helpers · 1 working`. Nothing for a chat with none. Reads its own chat's helpers off the
 * project's store (SC-3), so they redraw this and no row.
 *
 * The explorer drew chats' helpers and tasks as rows of its own until #1673; the Chats view is
 * the one place a chat is listed now, and this is what is left of it.
 */
export const HelpersSaid = memo(function HelpersSaid({ session }: { session: number }) {
  const count = useChatsSelect(
    useChatsHere(),
    (states) => helpersCountOf(states, session),
    sameHelpersCount,
  );
  if (count.total === 0) return null;
  return (
    <span className="helpers" title="The helpers its harness started inside this chat">
      {helpersCountSaid(count)}
    </span>
  );
});
