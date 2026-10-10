import { memo } from "react";
import { saidWhole, useDoingSaid } from "./chatDoing";

/**
 * **What a working chat is doing, in one line** (#1493, V100-42): "running cargo", "editing
 * Notice.tsx", "reading 3 files", "thinking". Dim, one line, cut short with an ellipsis where
 * it does not fit, and whole on hover. Nothing at all for a chat that is not working, or one
 * whose harness the app has heard nothing from (V100-71): no placeholder.
 *
 * A tab chip's menu draws it, and a Chats row's card (#1675), and so may anything else that
 * lists a chat: it is handed its chat and reads the rest itself, as the state mark does, so a
 * chat's line changing redraws this and nothing around it (SC-3). One component for both
 * (#1687): the card's is {@link ChatDoingLine}'s `heard` line.
 *
 * **The words are purlis's own** (`chatDoing.ts`), and the one name in them, a file's or a
 * program's, is text the core passed: shown as text, in its own direction (`bdi`), never as
 * markup.
 *
 * **Not a live region**: it changes several times a second and would chatter. In a menu it is
 * out of the accessibility tree where it stands, and what describes the menu's line points at
 * it (`id` is the one its `aria-describedby` names), so a screen reader says it when asked. In a
 * card it is `heard`: a line of its own, read with the card that describes the row, wrapped
 * whole where it is long, so it needs no tooltip.
 */
export const ChatDoingLine = memo(function ChatDoingLine({
  session,
  id,
  heard = false,
}: {
  session: number;
  /** The id what it describes names: its own for each surface that draws it (a tab's menu,
   *  #1551), as an id is one element's. */
  id?: string;
  /** Drawn as a line a screen reader reads where it stands, a paragraph of a card, rather
   *  than as words out of the tree that something else is described by. */
  heard?: boolean;
}) {
  const says = useDoingSaid(session);
  if (says === undefined) return null;
  const words = (
    <>
      {says.words}
      {says.name !== undefined && (
        <>
          {" "}
          <bdi className="named">{says.name}</bdi>
        </>
      )}
    </>
  );
  if (heard) return <p className="chat-doing">{words}</p>;
  return (
    <span className="chat-doing" id={id} aria-hidden="true" title={saidWhole(says)}>
      {words}
    </span>
  );
});
