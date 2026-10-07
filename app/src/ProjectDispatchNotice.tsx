import { useEffect, useState } from "react";
import { commands, type DispatchGrantsChanged, type PlaneId } from "./bindings";
import { Notice, type NoticeAction } from "./Notice";

/** A pair as the core spells it (`steward -> devops`), as a sentence says it. */
const said = (pair: string) => pair.replace(" -> ", " to ");

/**
 * **The project's dispatch grants changed** (#1437): the one-time Notice each teammate sees when
 * the committed settings change who may dispatch to whom, so a pulled change never silently
 * widens what chats do. **A pair a pull added covers no chat on this machine until someone here
 * allows it**: this Notice is where, all of them or one at a time (`acknowledge_dispatch_grants`,
 * which the core audits). A pair left unanswered still asks on the tab of the chat that needs it.
 *
 * It names what was added and what was taken away, and stands until it is answered. Grants taken
 * away need no yes: "Got it" records that they were read. "Review the grants" opens Settings,
 * where each is listed with Revoke. A grant allowed or revoked in this window is recorded there,
 * so it is never told back to the person who made it.
 */
export function ProjectDispatchNotice({
  plane,
  onReview,
}: {
  plane: PlaneId;
  /** Opens Settings where the dispatch grants are listed. */
  onReview: () => void;
}) {
  const [changed, setChanged] = useState<DispatchGrantsChanged>();

  useEffect(() => {
    let live = true;
    void commands
      .dispatchGrants(plane)
      .then((held) => {
        if (live && held.status === "ok") setChanged(held.data?.changed ?? undefined);
      })
      // A project whose grants cannot be read says nothing here: they still apply.
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [plane]);

  if (changed === undefined) return null;

  /** Allows `shown`, each as the core spelled it: every pair added, or one. */
  const allow = (shown: readonly string[]) =>
    void commands
      .acknowledgeDispatchGrants(plane, [...shown])
      .then((left) => {
        if (left.status === "ok") setChanged(left.data ?? undefined);
      })
      // Not recorded: it is told again next time, which is the safe way to be wrong.
      .catch(() => {});

  const review: NoticeAction = { label: "Review the grants", onPress: onReview };
  const all: NoticeAction =
    changed.added.length === 0
      ? { label: "Got it", onPress: () => allow(changed.now) }
      : {
          label: changed.added.length === 1 ? "Allow it" : `Allow all ${changed.added.length}`,
          onPress: () => allow(changed.added),
        };
  // One at a time, where there is more than one to choose between.
  const each: NoticeAction[] =
    changed.added.length > 1
      ? changed.added.map((pair) => ({
          label: `Allow ${said(pair)}`,
          onPress: () => allow([pair]),
        }))
      : [];

  return (
    <Notice
      cause="dispatch-grants"
      label="The project's dispatch grants changed"
      fixes={[all, ...each, review]}
    >
      <p>
        Which personas&apos; chats may dispatch to which has changed in the project&apos;s settings,
        which everyone who opens it follows.
        {changed.added.length > 0 && (
          <>
            {" "}
            Added: {changed.added.map(said).join(", ")}. What was added covers no chat on this
            machine until you allow it here.
          </>
        )}
        {changed.removed.length > 0 && <> Taken away: {changed.removed.map(said).join(", ")}.</>}
      </p>
    </Notice>
  );
}
