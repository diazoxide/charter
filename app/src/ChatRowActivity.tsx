import { memo } from "react";
import { DOING_ID, saidWhole, useDoingSaid } from "./chatDoing";

/**
 * **What a working chat is doing, in one line** (#1493, V100-42): "running cargo", "editing
 * Notice.tsx", "reading 3 files", "thinking". Dim, one line, cut short with an ellipsis where
 * it does not fit, and whole on hover. Nothing at all for a chat that is not working, or one
 * whose harness the app has heard nothing from (V100-71): no placeholder.
 *
 * The Chats list's row draws it (`ChatRowActivity`), and so may anything else that lists a
 * chat, a tab chip's menu for one: it is handed its chat and reads the rest itself, as the
 * state mark does, so a chat's line changing redraws this and nothing around it (SC-3).
 *
 * **The words are purlis's own** (`chatDoing.ts`), and the one name in them, a file's or a
 * program's, is text the core passed: shown as text, in its own direction (`bdi`), never as
 * markup.
 *
 * **Not a live region**: it changes several times a second and would chatter. It is out of
 * the accessibility tree where it stands, and what describes a row points at it (`id` is the
 * one a row's or a menu line's `aria-describedby` names), so a screen reader says it when asked.
 */
export const ChatDoingLine = memo(function ChatDoingLine({
  session,
  id,
}: {
  session: number;
  /** The id what it describes names: `chatDoingId` for the Chats list's row, and its own for
   *  any other surface that draws it beside that row (a tab's menu, #1551), as an id is one
   *  element's. */
  id?: string;
}) {
  const says = useDoingSaid(session);
  if (says === undefined) return null;
  return (
    <span className="chat-doing" id={id} aria-hidden="true" title={saidWhole(says)}>
      {says.words}
      {says.name !== undefined && (
        <>
          {" "}
          <bdi className="named">{says.name}</bdi>
        </>
      )}
    </span>
  );
});

/**
 * **What a chat is doing right now, on the second line of its row** (#1499, #1493): the one
 * place a row keeps for it, filled by {@link ChatDoingLine}.
 */
export const ChatRowActivity = memo(function ChatRowActivity({ session }: { session: number }) {
  // The id `chatDoingId` names, written out: that call is the row's own, made as the row is
  // drawn, and `ChatDoing.window.test.tsx` counts it as the row's draw (SC-3). This line is
  // drawn far more often than its row and must not call it.
  return <ChatDoingLine session={session} id={`${DOING_ID}${session}`} />;
});
