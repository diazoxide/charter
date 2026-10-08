/**
 * **A dispatch refused while nobody was there, in the title bar's needs-you list** (#1507,
 * decision V100-29): "<persona> wanted <persona> while you were away", how often and when,
 * with **Dismiss**, **Never for this pair** and **Allow from now on**.
 *
 * **An item of its own, attached to no chat.** The chat that asked was refused and told so; it
 * is not waiting on anyone. So this is no row of a chat: it has no Go, no tab shows a hand for
 * it, and the counts on the project and workspace tabs do not move. The title bar's list is
 * its only place.
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
 * - **a chat cannot move a row under the pointer.** The core lists pairs in the order they
 *   were first refused, the list drawn here is the one the menu was opened on
 *   (`NeedsYouMenu` holds it still), and a row that went while it was open is marked and
 *   stays in its place, with its answers off;
 * - **the keyboard never lands on Allow by itself.** Dismiss is the first answer, so it is
 *   where the arrows arrive; Allow from now on is the last.
 */
import * as Menu from "@radix-ui/react-dropdown-menu";
import { useState } from "react";
import type { AwayRefusal } from "./bindings";
import { ago } from "./BottomBar";

/** One refusal, as the window lists it: with the project it happened in. */
export type AwayItem = AwayRefusal & {
  plane: string;
  /** The project, named as its tab names it. */
  project: string;
};

/** What tells one item from another: its project, pair and workspace. */
export function awayKey(item: AwayItem): string {
  return `${item.plane}#away#${item.asking}#${item.target}#${item.workspace ?? ""}`;
}

/** The most of a name the item draws. */
const MOST_NAME = 40;

/** `text` held to `most` characters, said to be cut where it is. */
export function clipped(text: string, most: number = MOST_NAME): string {
  const chars = [...text];
  return chars.length <= most ? text : `${chars.slice(0, most).join("")}…`;
}

/** The item's headline. */
export function awaySaid(item: Pick<AwayItem, "asking" | "target">): string {
  return `${clipped(item.asking)} wanted ${clipped(item.target)} while you were away`;
}

/** How often and when, then the workspace and the project. */
export function awayWhere(item: AwayItem, now: number): string {
  const when = ago(Math.max(0, Math.round(now - item.latest)));
  const often =
    item.times === 1
      ? `once, ${when}`
      : item.times === 2
        ? `twice, last ${when}`
        : `${item.times} times, last ${when}`;
  return [often, item.workspace != null ? clipped(item.workspace) : undefined, item.project]
    .filter((part) => part !== undefined)
    .join(" · ");
}

/**
 * Where Allow from now on on `item` holds, as the button's name ends: the grant is limited to
 * the workspace the refused task would have worked in. A refusal at the project's root has no
 * workspace to limit it to, so there it holds in any.
 */
export function awayHolds(item: Pick<AwayItem, "workspace">): string {
  return item.workspace != null
    ? `for work in ${clipped(item.workspace)} only`
    : "in any workspace";
}

/** What a row that went while the list was open says in place of what Allow would allow. */
export const GONE = "No longer listed. Nothing here can be answered.";

/** The items, each a group of the needs-you menu with its three answers. */
export function AwayRows({
  items,
  gone,
  onAllow,
  onDismiss,
  onNever,
  now,
}: {
  items: readonly AwayItem[];
  /** Whether an item drawn is no longer one the core lists: drawn in place, answers off. */
  gone?: (item: AwayItem) => boolean;
  onAllow?: (item: AwayItem) => void;
  onDismiss?: (item: AwayItem) => void;
  onNever?: (item: AwayItem) => void;
  /** Now, in seconds since 1970; the moment the list was opened when it is not given. */
  now?: number;
}) {
  // The list is drawn when it is opened, so "2h ago" is as of the look.
  const [opened] = useState(() => Date.now() / 1000);
  const at = now ?? opened;
  return (
    <>
      {items.map((item) => {
        const said = awaySaid(item);
        const pair = `${clipped(item.asking)} chats dispatch to ${clipped(item.target)}`;
        const went = gone?.(item) ?? false;
        return (
          <Menu.Group
            key={awayKey(item)}
            className={`needs-you-row needs-you-permission needs-you-away${went ? " gone" : ""}`}
            aria-label={`${said} · ${awayWhere(item, at)}`}
          >
            <p className="needs-you-says">
              <span className="needs-you-name">{said}</span>
              <span className="needs-you-where">{awayWhere(item, at)}</span>
            </p>
            {/* What the press allows and reaches, before the press: the core's sentence, whole. */}
            <p className="needs-you-says needs-you-allows">{went ? GONE : item.allows}</p>
            {/* Dismiss first: where the keyboard arrives is never the grant. */}
            <Menu.Item
              className="more-tab needs-you-answer"
              disabled={went}
              aria-label={`Dismiss: ${said}`}
              title="Put this away. Nothing is allowed, and it is not listed again for a week of further refusals."
              onSelect={() => onDismiss?.(item)}
            >
              Dismiss
            </Menu.Item>
            <Menu.Item
              className="more-tab needs-you-answer"
              disabled={went}
              aria-label={`Never for this pair: ${clipped(item.asking)} chats never dispatch to ${clipped(item.target)}, for you on this machine`}
              title="No chat of that persona is asked or allowed for that persona on this machine, until you lift it in Settings › Project › Dispatch."
              onSelect={() => onNever?.(item)}
            >
              Never for this pair
            </Menu.Item>
            <Menu.Item
              className="more-tab needs-you-answer allows"
              disabled={went}
              aria-label={`Allow from now on: ${pair}, for you on this machine, ${awayHolds(item)}`}
              title={item.allows}
              onSelect={() => onAllow?.(item)}
            >
              Allow from now on
            </Menu.Item>
          </Menu.Group>
        );
      })}
    </>
  );
}
