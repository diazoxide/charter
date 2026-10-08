import { useEffect, useState } from "react";
import { GitCompare, LoaderCircle } from "lucide-react";
import { ChatAsk } from "./ChatAsk";
import { Loss } from "./DispatchesTab";
import { Notice } from "./Notice";
import {
  commands,
  type BranchMerge,
  type ChangedIn,
  type PlaneId,
  type TaskChanges,
  type TaskFile,
  type WorktreeLoss,
} from "./bindings";
import { discardSays } from "./dispatches";
import { pieceDiffTitle, pieceDiffView } from "./pieceViews";
import type { ViewRef } from "./tabs";
import {
  filesIn,
  mergeBlocked,
  mergeSays,
  ownSaid,
  placeOf,
  placeSaid,
  SHARED_SAID,
} from "./taskChanges";

/** A file's mark, as the explorer says it. */
const MARKS: Record<TaskFile["mark"], string> = {
  changed: "changed",
  added: "added",
  deleted: "deleted",
  renamed: "renamed",
};

/**
 * **What one task changed, and no other task's** (#1511, V100-66, V100-67): a view tab, opened
 * from **Changes** on a finished task's row and from its report's line about what changed.
 *
 * - **A task on its own branch** lists everything that branch changed since it was cut, and
 *   each file opens its comparison against where the branch started (`PieceDiff.tsx`). Once it
 *   has ended, the tab offers **Merge** into the branch it was cut from and **Discard branch**.
 * - **A task that worked in a folder other chats work in** lists only the files its own tools
 *   named that git finds changed there, each marked where a sibling named it too. Where purlis
 *   holds no such list, the tab says so and lists nothing: the folder's changes are never shown
 *   as the task's.
 *
 * **Merge and Discard are asked first, and each is the person's alone.** The question says
 * what would happen, from what the core read now; the answer hands that back, and the core
 * does nothing where the branch holds anything else by then. A merge is a fast-forward or a
 * refusal that says why, and changes nothing when refused. Both are commands of the window
 * alone: no chat reaches either.
 *
 * Everything the task wrote (its name, file names, its own words for what it changed) is drawn
 * as text.
 */
export function TaskChangesTab({
  plane,
  id,
  changed,
  onOpenView,
}: {
  plane: PlaneId;
  /** The dispatch's id. */
  id: string;
  /** Bumped when the project changes on disk: the tab reads again. */
  changed: number;
  onOpenView: (view: ViewRef, title: string) => void;
}) {
  const [said, setSaid] = useState<{ read?: TaskChanges; trouble?: string }>();
  const [again, setAgain] = useState(0);
  /** The merge being asked about, with the core's refusal of the last answer. */
  const [merging, setMerging] = useState<{ merge: BranchMerge; trouble?: string }>();
  /** The discard being asked about, with the core's refusal of the last answer. */
  const [discarding, setDiscarding] = useState<{ loss: WorktreeLoss; trouble?: string }>();
  /** What the last press could not do, or what it did, until it is put away. */
  const [told, setTold] = useState<{ tone: "news" | "trouble"; says: string }>();
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let gone = false;
    void commands
      .taskChanges(plane, id)
      .then((answer) => {
        if (gone) return;
        setSaid(answer.status === "error" ? { trouble: answer.error } : { read: answer.data });
      })
      .catch((err: unknown) => {
        if (!gone) setSaid({ trouble: String(err) });
      });
    return () => {
      gone = true;
    };
  }, [plane, id, changed, again]);

  const askToMerge = async () => {
    setTold(undefined);
    const answer = await commands
      .taskBranchMergeQuestion(plane, id)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (answer.status === "error") return setTold({ tone: "trouble", says: answer.error });
    // A merge the core would refuse is said, and not asked about: the press would change
    // nothing.
    const blocked = mergeBlocked(answer.data);
    if (blocked !== undefined) return setTold({ tone: "trouble", says: blocked });
    setMerging({ merge: answer.data });
  };
  const merge = async () => {
    if (merging === undefined) return;
    setBusy(true);
    const answer = await commands
      .taskBranchMerge(plane, id, merging.merge)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    setBusy(false);
    if (answer.status === "error") return setMerging({ ...merging, trouble: answer.error });
    setMerging(undefined);
    setTold({
      tone: "news",
      says: answer.data.folder_removed
        ? `${answer.data.branch} is merged, and its folder is removed: it held nothing else.`
        : `${answer.data.branch} is merged. Its folder is kept: it holds something git does not track.`,
    });
    setAgain((was) => was + 1);
  };
  const askToDiscard = async () => {
    setTold(undefined);
    const answer = await commands
      .dispatchWorktreeLoss(plane, id)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (answer.status === "error") return setTold({ tone: "trouble", says: answer.error });
    setDiscarding({ loss: answer.data });
  };
  const discard = async () => {
    if (discarding === undefined) return;
    setBusy(true);
    const answer = await commands
      .dispatchWorktreeDiscard(plane, id, discarding.loss)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    setBusy(false);
    if (answer.status === "error") return setDiscarding({ ...discarding, trouble: answer.error });
    setDiscarding(undefined);
    setAgain((was) => was + 1);
  };

  if (said === undefined)
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading what the task changed…
      </p>
    );
  if (said.trouble !== undefined || said.read === undefined)
    return (
      <Notice
        cause={`task-changes-unread:${id}`}
        tone="trouble"
        fixes={[{ label: "Read again", onPress: () => setAgain((was) => was + 1) }]}
      >
        {`purlis could not read what this task changed: ${said.trouble ?? "no answer"}`}
      </Notice>
    );
  const read = said.read;
  const own = read.own;
  return (
    <div className="task-changes" data-testid="task-changes">
      <p className="honest">{own === null ? SHARED_SAID : ownSaid(own)}</p>
      {read.running && (
        <p className="honest">It is still working: this is what it has changed so far.</p>
      )}
      {own?.acts === true && (
        <div className="task-changes-acts">
          <button type="button" tabIndex={0} onClick={() => void askToMerge()}>
            Merge…
          </button>
          <button type="button" tabIndex={0} onClick={() => void askToDiscard()}>
            Discard branch…
          </button>
        </div>
      )}
      {told !== undefined && (
        <Notice
          cause={`task-changes-told:${id}`}
          tone={told.tone}
          label={told.tone === "trouble" ? "Nothing was changed" : "Merged"}
          onDismiss={() => setTold(undefined)}
        >
          {told.says}
        </Notice>
      )}
      {read.unknown !== null && (
        <p className="honest" data-testid="task-changes-unknown">
          {read.unknown}
        </p>
      )}
      {read.unknown === null && filesIn(read) === 0 && (
        <p className="none">No changed file to show.</p>
      )}
      {read.places.map((place) => (
        <Place
          key={`${place.workspace}/${place.repo}/${place.piece ?? ""}`}
          place={place}
          onOpenView={onOpenView}
        />
      ))}
      {read.more && (
        <p className="honest">
          Its tools named more files than are kept: only the first are listed.
        </p>
      )}
      {read.elsewhere.length > 0 && (
        <>
          <h3>Named outside any repo</h3>
          <p className="honest">
            Its tools named these, and git has nothing to compare them against, so they are not said
            to have changed:
          </p>
          <pre>{read.elsewhere.join("\n")}</pre>
        </>
      )}
      {/* The task's own words, as it reported them: its claim, never a fact purlis checked. */}
      {read.said !== null && (
        <>
          <h3>What it says changed</h3>
          <pre>{read.said}</pre>
        </>
      )}
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
    </div>
  );
}

/** The files a task changed in one repo's folder or one branch's: each opens its comparison. */
function Place({
  place,
  onOpenView,
}: {
  place: ChangedIn;
  onOpenView: (view: ViewRef, title: string) => void;
}) {
  const where = placeOf(place);
  return (
    <section aria-label={`Changed in ${placeSaid(place)}`}>
      <h3>
        {placeSaid(place)}
        {place.base !== null && <span className="none"> · against {place.base}</span>}
      </h3>
      {place.unread !== null && <p className="trouble">{place.unread}</p>}
      <ul className="task-changes-files">
        {place.files.map((file) => (
          <li key={file.path} data-mark={file.mark}>
            <button
              type="button"
              className="vault-secret"
              tabIndex={0}
              title="Show what changed"
              onClick={() =>
                onOpenView(pieceDiffView(where, file.path), pieceDiffTitle(where, file.path))
              }
            >
              <GitCompare className="node-icon" aria-hidden="true" />
              {file.path}
            </button>
            <span className="outcome"> {MARKS[file.mark]}</span>
            {file.from !== null && <span className="none"> from {file.from}</span>}
            {file.uncommitted && <span className="none"> · not committed</span>}
            {file.also.length > 0 && (
              <span className="honest"> · also named by {file.also.join(", ")}</span>
            )}
          </li>
        ))}
      </ul>
      {place.more > 0 && <p className="none">{`… and ${place.more} more`}</p>}
    </section>
  );
}
