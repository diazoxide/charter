import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";

import type { Offer } from "./actions";
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
 * Asked at the mount and whenever the core says a change on disk concerns the instructions
 * (`instructionsChanges`, from `usePlaneChanged(…, INSTRUCTIONS)`, FD-10d), which is when the
 * answer can move. A window that cannot ask marks nothing.
 */
export function usePlaneUpdated(plane: PlaneId, instructionsChanges: number): PlaneUpdates {
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
  }, [plane, instructionsChanges]);
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

/**
 * **The mark on the strip, as the way out it names** (NO-3): the same mark, drawn as a button
 * beside the tab rather than inside it — a button cannot sit in the tab's own — that presses the
 * catalogue's `tab.fresh:<id>`, the row the palette and the tab's menu list. So its accessible
 * name is the row's words, and the question it asks is the row's (`PlaneView`'s `ChatAsk`).
 *
 * **Not a Tab stop**, for `Closer`'s reason (charter-app#189): the strip is one stop, and a
 * keyboard reaches the same row from the palette or the tab's menu. So the tab is **described
 * by** it (`id`, #1246): a screen reader on the tab hears that the plane was updated, which a
 * mark outside the tab would otherwise never tell it.
 */
export function FreshMark({
  id,
  offer,
  files,
  onPress,
}: {
  /** What the tab's `aria-describedby` names. */
  id: string;
  offer?: Offer;
  files?: readonly string[];
  onPress: (offer: Offer) => void;
}) {
  if (!offer || !files || !freshMarkShown(offer, files)) return null;
  return (
    <button
      id={id}
      type="button"
      className="plane-updated fresh-mark"
      tabIndex={-1}
      aria-label={`${offer.title} — plane updated since this chat started`}
      title={`Plane updated since this chat started: ${files.join(", ")}. Press to start it fresh on what is there now.`}
      onClick={() => onPress(offer)}
    >
      <RefreshCw />
    </button>
  );
}

/** Whether a {@link FreshMark} is drawn: there is a Start fresh row, and files that moved. The
 *  tab asks the same, so it is never described by a mark that is not there. */
export function freshMarkShown(
  offer: Offer | undefined,
  files: readonly string[] | undefined,
): boolean {
  return offer !== undefined && files !== undefined && files.length > 0;
}
