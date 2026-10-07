import { useEffect, useState } from "react";
import { commands, type DispatchGrantsChanged, type PlaneId } from "./bindings";
import { Notice } from "./Notice";

/** A pair as the core spells it (`steward -> devops`), as a sentence says it. */
const said = (pair: string) => pair.replace(" -> ", " to ");

/**
 * **The project's dispatch grants changed** (#1437): the one-time Notice each teammate sees when
 * the committed settings change who may dispatch to whom, so a pulled change never silently
 * widens what chats do. The grants apply with no approval; this tells, and asks nothing.
 *
 * It names what was added and what was taken away, and stands until it is read: "Got it" records
 * the list as it was shown, on this machine (`acknowledge_dispatch_grants`), so a change made
 * after it was shown is told again. "Review the grants" opens Settings, where each is listed with
 * Revoke. A grant allowed or revoked in this window is recorded as seen there, so it is never
 * told back to the person who made it.
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

  const read = () =>
    void commands
      .acknowledgeDispatchGrants(plane, changed.now)
      .then((left) => {
        if (left.status === "ok") setChanged(left.data ?? undefined);
      })
      // Not recorded: it is told again next time, which is the safe way to be wrong.
      .catch(() => {});

  return (
    <Notice
      cause="dispatch-grants"
      label="The project's dispatch grants changed"
      fixes={[
        { label: "Got it", onPress: read },
        { label: "Review the grants", onPress: onReview },
      ]}
    >
      <p>
        Which personas&apos; chats may dispatch to which has changed in the project&apos;s settings,
        which everyone who opens it follows.
        {changed.added.length > 0 && <> Added: {changed.added.map(said).join(", ")}.</>}
        {changed.removed.length > 0 && <> Taken away: {changed.removed.map(said).join(", ")}.</>}
      </p>
    </Notice>
  );
}
