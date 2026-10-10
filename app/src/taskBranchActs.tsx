import { useState, type ReactNode } from "react";
import { ChatAsk } from "./ChatAsk";
import { Loss } from "./DispatchesTab";
import { commands, type BranchMerge, type PlaneId, type WorktreeLoss } from "./bindings";
import { discardSays } from "./dispatches";
import { mergeBlocked, mergeSays } from "./taskChanges";

/** What a press of Merge or Discard came to, said where it was pressed. */
export type BranchTold = { tone: "news" | "trouble"; says: string };

/**
 * **A task's Merge… and Discard…, asked first, wherever they are pressed** (#1511, #1534): the
 * task's Changes tab, the finished row's menu in the chats list and the Dispatches tab's row.
 * One set of asks, so each place asks the same question and the core is called the same way:
 *
 * - **Merge…** asks the core what a merge would do now (`task_branch_merge_question`). A merge
 *   it would refuse is said, and not asked about: the press would change nothing. Otherwise the
 *   question says what would happen, and the answer hands that back (`task_branch_merge`); the
 *   core does nothing where the branch holds anything else by then.
 * - **Discard…** asks the core what the folder holds (`dispatch_worktree_loss`), and the question
 *   names every path that would go; the answer hands that list back
 *   (`dispatch_worktree_discard`), and the core removes nothing where the folder holds other
 *   paths by then.
 *
 * Both are commands of the window alone: no chat reaches either. `told` hears what a press came
 * to (a merge done, or a refusal in the core's words); `changed` hears that the branch moved.
 */
export function useTaskBranchActs({
  plane,
  told,
  changed,
}: {
  plane: PlaneId;
  told: (said: BranchTold) => void;
  changed?: () => void;
}): {
  askToMerge: (id: string) => Promise<void>;
  askToDiscard: (id: string) => Promise<void>;
  /** The question being asked, where one is: drawn by whoever holds the asks. */
  asking: ReactNode;
} {
  /** The merge being asked about, with the core's refusal of the last answer. */
  const [merging, setMerging] = useState<{ id: string; merge: BranchMerge; trouble?: string }>();
  /** The discard being asked about, with the core's refusal of the last answer. */
  const [discarding, setDiscarding] = useState<{
    id: string;
    loss: WorktreeLoss;
    trouble?: string;
  }>();
  const [busy, setBusy] = useState(false);

  const askToMerge = async (id: string) => {
    const answer = await commands
      .taskBranchMergeQuestion(plane, id)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (answer.status === "error") return told({ tone: "trouble", says: answer.error });
    const blocked = mergeBlocked(answer.data);
    if (blocked !== undefined) return told({ tone: "trouble", says: blocked });
    setMerging({ id, merge: answer.data });
  };
  const merge = async () => {
    if (merging === undefined) return;
    setBusy(true);
    const answer = await commands
      .taskBranchMerge(plane, merging.id, merging.merge)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    setBusy(false);
    if (answer.status === "error") return setMerging({ ...merging, trouble: answer.error });
    setMerging(undefined);
    told({
      tone: "news",
      says: answer.data.folder_removed
        ? `${answer.data.branch} is merged, and its folder is removed: it held nothing else.`
        : `${answer.data.branch} is merged. Its folder is kept: it holds something git does not track.`,
    });
    changed?.();
  };
  const askToDiscard = async (id: string) => {
    const answer = await commands
      .dispatchWorktreeLoss(plane, id)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (answer.status === "error") return told({ tone: "trouble", says: answer.error });
    setDiscarding({ id, loss: answer.data });
  };
  const discard = async () => {
    if (discarding === undefined) return;
    setBusy(true);
    const answer = await commands
      .dispatchWorktreeDiscard(plane, discarding.id, discarding.loss)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    setBusy(false);
    if (answer.status === "error") return setDiscarding({ ...discarding, trouble: answer.error });
    setDiscarding(undefined);
    changed?.();
  };

  const asking = (
    <>
      {merging !== undefined && (
        <ChatAsk
          title="Merge this task's branch?"
          says={mergeSays(merging.merge)}
          answer="Merge"
          trouble={merging.trouble}
          busy={busy}
          onAnswer={() => void merge()}
          onCancel={() => setMerging(undefined)}
        />
      )}
      {discarding !== undefined && (
        <ChatAsk
          title="Discard this branch's folder?"
          says={discardSays(discarding.loss)}
          answer="Discard"
          trouble={discarding.trouble}
          busy={busy}
          onAnswer={() => void discard()}
          onCancel={() => setDiscarding(undefined)}
        >
          <Loss loss={discarding.loss} />
        </ChatAsk>
      )}
    </>
  );
  return { askToMerge, askToDiscard, asking };
}
