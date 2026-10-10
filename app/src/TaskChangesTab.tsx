import { useEffect, useState } from "react";
import { GitCompare, LoaderCircle } from "lucide-react";
import { ChatAsk } from "./ChatAsk";
import { EmptyState } from "./EmptyState";
import { Notice } from "./Notice";
import {
  commands,
  type ChangedIn,
  type PlaneId,
  type TaskChanges,
  type TaskFile,
} from "./bindings";
import { pieceDiffTitle, pieceDiffView } from "./pieceViews";
import type { ViewRef } from "./tabs";
import { useTaskBranchActs, type BranchTold } from "./taskBranchActs";
import {
  deleteSays,
  filesIn,
  leftSaid,
  ownSaid,
  pastTheCap,
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
 *   wrote that git finds changed there, each marked where another chat's tools wrote it too
 *   (#1534). Where purlis
 *   holds no such list, the tab says so and lists nothing: the folder's changes are never shown
 *   as the task's.
 *
 * **Merge and Discard are asked first, and each is the person's alone.** The question says
 * what would happen, from what the core read now; the answer hands that back, and the core
 * does nothing where the branch holds anything else by then. A merge is a fast-forward or a
 * refusal that says why, and changes nothing when refused. Both are commands of the window
 * alone: no chat reaches either. The asks are `taskBranchActs.tsx`'s, which the finished row's
 * menu and the Dispatches tab's row ask through too (#1534).
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
  /** The delete of a merged branch whose folder is gone being asked about (#1472): the commit
   *  it was shown at, with the core's refusal of the last answer. */
  const [deleting, setDeleting] = useState<{ tip: string; trouble?: string }>();
  /** What the last press could not do, or what it did, until it is put away. */
  const [told, setTold] = useState<BranchTold>();
  const [busy, setBusy] = useState(false);
  const acts = useTaskBranchActs({
    plane,
    told: setTold,
    changed: () => setAgain((was) => was + 1),
  });

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

  const askToMerge = () => {
    setTold(undefined);
    void acts.askToMerge(id);
  };
  const askToDiscard = () => {
    setTold(undefined);
    void acts.askToDiscard(id);
  };

  const deleteBranch = async () => {
    if (deleting === undefined) return;
    setBusy(true);
    const answer = await commands
      .taskBranchDelete(plane, id, deleting.tip)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    setBusy(false);
    if (answer.status === "error") return setDeleting({ ...deleting, trouble: answer.error });
    setDeleting(undefined);
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
      {own !== null && leftSaid(own) !== undefined && (
        <p className="honest" data-testid="task-changes-left">
          {leftSaid(own)}
        </p>
      )}
      {own?.left?.deletable === true && !read.running && (
        <div className="task-changes-acts">
          <button
            type="button"
            tabIndex={0}
            onClick={() => {
              setTold(undefined);
              if (own.left !== null) setDeleting({ tip: own.left.tip });
            }}
          >
            Delete branch…
          </button>
        </div>
      )}
      {own?.acts === true && (
        <div className="task-changes-acts">
          <button type="button" tabIndex={0} onClick={askToMerge}>
            Merge…
          </button>
          <button type="button" tabIndex={0} onClick={askToDiscard}>
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
        <EmptyState
          size="panel"
          mark={GitCompare}
          headline={
            own === null
              ? "No file its edit tools wrote is uncommitted here"
              : "No changed file to show"
          }
          body={
            own === null
              ? "A file its edit tools write in this folder is listed here while it is uncommitted."
              : `A file the task changes on ${own.branch ?? "its own branch"} is listed here, with its comparison against where the branch started.`
          }
          testid="task-changes-empty"
        />
      )}
      {read.places.map((place) => (
        <Place
          key={`${place.workspace}/${place.repo}/${place.piece ?? ""}`}
          place={place}
          shared={own === null}
          onOpenView={onOpenView}
        />
      ))}
      {read.more && (
        <p className="honest">
          Its edit tools wrote more files than are kept: only the first are listed.
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
      {acts.asking}
      {deleting !== undefined && own !== null && (
        <ChatAsk
          title="Delete this task's branch?"
          says={deleteSays(own)}
          answer="Delete"
          trouble={deleting.trouble}
          busy={busy}
          onAnswer={() => void deleteBranch()}
          onCancel={() => setDeleting(undefined)}
        />
      )}
    </div>
  );
}

/** The files a task changed in one repo's folder or one branch's: each opens its comparison. */
function Place({
  place,
  shared,
  onOpenView,
}: {
  place: ChangedIn;
  /** Whether it is a folder other chats work in: then `more` may hide a file of the task's. */
  shared: boolean;
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
              <span className="honest"> · also written by {file.also.join(", ")}</span>
            )}
          </li>
        ))}
      </ul>
      {place.more > 0 &&
        (shared ? (
          <p className="honest">{pastTheCap(place.more)}</p>
        ) : (
          <p className="none">{`… and ${place.more} more`}</p>
        ))}
    </section>
  );
}
