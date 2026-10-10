import { needsYouRows, stoppedRows, type Now, type Offer } from "./actions";

/** What the queue's rows are worded from, beside the queue itself. */
export type QueueNow = Pick<
  Now,
  | "tabs"
  | "nameOf"
  | "reportsTo"
  | "refusedIn"
  | "neededFor"
  | "listed"
  | "stoppedBelow"
  | "stopped"
>;

/** The catalogue's row for the oldest chat in the queue. */
const NEXT = "needs.next";

/**
 * **The catalogue with the needs-you queue put in** (#1034).
 *
 * The queue changes whenever a chat starts or stops asking for you, and the project view used to
 * read it at its top so that the catalogue could word these rows, which redrew every pane for a
 * change only the counts and the title bar's list show. So the view builds its catalogue with no
 * queue (`needsYou: []`, no `stopped`) and these rows go in when the queue is read, as the view
 * reports to the window, off the chats' store.
 *
 * `offers` is that queue-less catalogue. Its `needs.next` row stands where the queue's rows go,
 * which is where `catalogue` pushes them: the next chat's row, every queued chat's two rows, then
 * the rows of the chats whose Smart close stopped. The result is the catalogue `catalogue` would
 * have built with the queue (`queueRows.test.ts` holds it to that). A catalogue that was not built
 * (a project not in front has none) is handed back as it is.
 */
export function withQueue(offers: Offer[], queue: readonly number[], now: QueueNow): Offer[] {
  const at = offers.findIndex((offer) => offer.id === NEXT);
  if (at < 0) return offers;
  const was = offers[at];
  const [oldest] = queue;
  // The queue-less row is the one that says nothing needs you, in the words `quiet` gives it; with
  // a chat in the queue it shows that chat.
  const next: Offer =
    oldest === undefined
      ? was
      : { ...was, available: true, reason: "", does: { verb: "showChat", session: oldest } };
  const listed = now.listed ?? [];
  return [
    ...offers.slice(0, at),
    next,
    ...needsYouRows(
      queue,
      now.nameOf,
      now.tabs,
      now.reportsTo,
      now.refusedIn,
      now.neededFor,
      (session) => listed.some((chat) => chat.session === session),
      now.stoppedBelow,
    ),
    ...stoppedRows(now.stopped ?? {}, queue, now.nameOf, now.tabs),
    ...offers.slice(at + 1),
  ];
}
