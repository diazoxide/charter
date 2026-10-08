import { memo } from "react";

/**
 * **What a chat is doing right now, on the second line of its row** (#1499): the one place a
 * row keeps for it. Nothing is drawn yet: the activity line arrives with #1493, which fills
 * this component and nothing else of the row.
 *
 * It is handed its chat and reads the rest itself, as the state mark does, so what it comes to
 * say redraws it and no row (SC-3).
 */
export const ChatRowActivity = memo(function ChatRowActivity({ session }: { session: number }) {
  void session;
  return null;
});
