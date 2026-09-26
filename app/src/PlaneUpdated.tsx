import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";

import { commands, type PlaneId } from "./bindings";

/** The files each chat started on that have changed since, by session. */
export type PlaneUpdates = Readonly<Record<number, readonly string[]>>;

/**
 * Which of this project's chats are running on instructions the plane has changed since they
 * started (charter#369).
 *
 * A chat reads `CLAUDE.md`, its harness settings and sub-agents, and its persona's charter
 * once, when it starts. The Python charter said "control plane updated" in the transcript on
 * the next prompt; the ruling on #369 moved it here, onto the chat's tab, so the operator —
 * who is the one who can start it fresh — is the one told.
 *
 * Asked at the mount and whenever the core says the plane changed on disk (`changesOnDisk`,
 * from `usePlaneChanged`), which is when the answer can move. A window that cannot ask marks
 * nothing.
 */
export function usePlaneUpdated(plane: PlaneId, changesOnDisk: number): PlaneUpdates {
  const [updates, setUpdates] = useState<PlaneUpdates>({});
  useEffect(() => {
    let gone = false;
    void commands
      .chatsPlaneUpdated(plane)
      .then((answer) => {
        if (gone || answer.status !== "ok") return;
        setUpdates(Object.fromEntries((answer.data ?? []).map((one) => [one.session, one.files])));
      })
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [plane, changesOnDisk]);
  return updates;
}

/**
 * The mark on a chat's tab when the plane's instructions changed after it started.
 *
 * **Not a needs-you item.** Nothing is waiting on the operator: the chat works, on what it
 * read. So it is a quiet mark beside the pin, not a red count, and it adds nothing to the
 * queue. What it says is what the operator can do about it — start the chat fresh — and which
 * files moved, so they can judge whether that is worth it.
 */
export function PlaneUpdatedMark({ files }: { files?: readonly string[] }) {
  if (!files || files.length === 0) return null;
  return (
    <span
      className="plane-updated"
      role="img"
      aria-label="plane updated since this chat started"
      title={`Plane updated since this chat started: ${files.join(", ")}. It runs on what it read at its start until it is started fresh.`}
    >
      <RefreshCw />
    </span>
  );
}
