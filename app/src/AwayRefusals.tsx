/**
 * **A dispatch refused while nobody was there, an update in the Inbox** (#1507, decision
 * V100-29; #1693): "<persona> wanted <persona> while you were away", how often, with
 * **Never for this pair**, **Allow from now on** and **Dismiss**.
 *
 * **An item of its own, attached to no chat.** The chat that asked was refused and told so; it
 * is not waiting on anyone. So this is no ask and no row of a chat: it has no Go, no tab shows
 * a hand for it, and the counts on the project and workspace tabs and the title bar do not move.
 * The Inbox's updates are its only place.
 *
 * **Allow from now on is a standing grant in one press**, offered because a chat was refused.
 * So:
 *
 * - the item says what the grant allows and what it makes reachable before the press, in the
 *   core's own sentence (`allows`), and the button's name says the pair and the scope: the
 *   workspace the refused task would have worked in, and no other (#1505), or any workspace
 *   where that was the project's root (`awayHolds`);
 * - it offers nothing wider: no project-level grant and no "any persona" is reachable here;
 * - **every word drawn is the app's own**: the two personas, the workspace, the count and the
 *   time. Nothing a chat wrote is shown: the core keeps neither the brief nor the task's name;
 * - **a chat cannot move a row under the pointer and have it granted.** The Inbox lists what
 *   comes to it as it comes, so a row can move; a grant pressed within `SETTLE_MS` of its row
 *   being drawn or moved is not sent, and the row says why (`Inbox.tsx`), as a sandbox block's
 *   Notice does when a host joins it;
 * - **the keyboard never lands on Allow by itself.** It comes in on the update, never on a
 *   button, and Allow from now on is the last of the answers the arrows reach.
 */
import type { AwayRefusal } from "./bindings";
import type { UpdateRow } from "./Inbox";
/** The most of a name the item draws. */
const MOST_NAME = 40;

/** `text` held to `most` characters, said to be cut where it is. */
export function clipped(text: string, most: number = MOST_NAME): string {
  const chars = [...text];
  return chars.length <= most ? text : `${chars.slice(0, most).join("")}…`;
}

/** The item's headline. */
export function awaySaid(item: Pick<AwayRefusal, "asking" | "target">): string {
  return `${clipped(item.asking)} wanted ${clipped(item.target)} while you were away`;
}

/**
 * Where Allow from now on on `item` holds, as the button's name ends: the grant is limited to
 * the workspace the refused task would have worked in. A refusal at the project's root has no
 * workspace to limit it to, so there it holds in any.
 */
export function awayHolds(item: Pick<AwayRefusal, "workspace" | "nowhere">): string {
  if (item.workspace != null && item.nowhere != null)
    return `which keeps nothing: ${clipped(item.workspace)} is not a workspace of this project now`;
  return item.workspace != null
    ? `for work in ${clipped(item.workspace)} only`
    : "in any workspace";
}

/** What a row that went while the list was open says in place of what Allow would allow. */
export const GONE = "No longer listed. Nothing here can be answered.";

/** How often a pair was refused, and for which workspace. */
export function oftenSaid(item: Pick<AwayRefusal, "times" | "workspace">): string {
  const often =
    item.times === 1
      ? "Refused once"
      : item.times === 2
        ? "Refused twice"
        : `Refused ${item.times} times`;
  return item.workspace != null ? `${often}, for work in ${clipped(item.workspace)}` : often;
}

/**
 * **What a refused dispatch's update offers in the Inbox** (#1693): how often and what Allow
 * from now on allows, before its answers; **Never for this pair**, then **Allow from now on**,
 * each the person's answer for the one pair the item names (`answer`); and Dismiss, which grants
 * nothing. `put` puts the update away once it is answered.
 */
export function awayRow(
  item: AwayRefusal,
  answer: (item: AwayRefusal, how: "allow" | "dismiss" | "never") => void,
  put: () => void,
): Omit<UpdateRow, "update"> {
  const pair = `${clipped(item.asking)} chats dispatch to ${clipped(item.target)}`;
  return {
    more: `${oftenSaid(item)}. ${item.allows}`,
    answers: [
      {
        label: "Never for this pair",
        name: `Never for this pair: ${clipped(item.asking)} chats never dispatch to ${clipped(item.target)}, for you on this machine`,
        title:
          "No chat of that persona is asked or allowed for that persona on this machine, until you lift it in Settings › Project › Dispatch.",
        press: () => {
          answer(item, "never");
          put();
        },
      },
      {
        label: "Allow from now on",
        name: `Allow from now on: ${pair}, for you on this machine, ${awayHolds(item)}`,
        title: item.allows,
        allows: true,
        press: () => {
          answer(item, "allow");
          put();
        },
      },
    ],
    dismiss: () => {
      answer(item, "dismiss");
      put();
    },
    dismissSays:
      "Put this away. Nothing is allowed, and it is not listed again for a week of further refusals.",
  };
}
