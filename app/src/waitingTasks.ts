import { createContext } from "react";
import type { FinishedTask, NeedsApproval } from "./bindings";

/**
 * **What a task that a launch could not start again offers on its row** (#1497, D-1497-14).
 *
 * Such a task is not ended: it is still recorded and is tried again at the next launch. Its
 * row is drawn under the chat that asked, by the finished-rows component, with the ways out
 * the window's own "did not start" line has. They are the window's to carry out, so the row
 * reads them from here and the lists between it and the window pass nothing on.
 */
export type WaitingTaskWays = {
  /** Tries to start it again, as the launch did. It stays a task either way. */
  retry: (task: FinishedTask) => Promise<void>;
  /** Opens the question that shows what its profile would run; the approval is its answer. */
  approve: (task: FinishedTask, approval: NeedsApproval) => void;
  /** Ends it, on the person's word: answers why not, where it could not be ended. */
  end: (task: FinishedTask) => Promise<string | undefined>;
};

export const WaitingTaskWaysContext = createContext<WaitingTaskWays | null>(null);
