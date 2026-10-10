import type { ReactNode } from "react";
import { useChatsHere, useChatsSelect } from "./chatState";

/**
 * **What is drawn from the needs-you queue, read where it is drawn** (#1034).
 *
 * The queue is the project's: a chat that starts or stops asking for you changes it, and the
 * workspace tabs' counts, the show-more counts and the hands on the tab chips are drawn from it.
 * The project view used to read it at its top, which redrew every pane for a change none of them
 * shows. Each of those surfaces is drawn inside one of these instead, so a queue change redraws
 * it and nothing around it. `children` is called with the queue, oldest first.
 */
export function QueueRead({
  children,
}: {
  children: (queue: readonly number[]) => ReactNode;
}): ReactNode {
  const queue = useChatsSelect(useChatsHere(), (states) => states.needsYou);
  return children(queue);
}
