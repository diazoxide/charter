/**
 * **A dispatch refused while nobody was there, in the title bar's needs-you list** (#1507,
 * decision V100-29): "<persona> wanted <persona> while you were away", how often and when,
 * with **Allow from now on** and **Dismiss**.
 *
 * **An item of its own, attached to no chat.** The chat that asked was refused and told so; it
 * is not waiting on anyone. So this is no row of a chat: it has no Go, no tab shows a hand for
 * it, and the counts on the project and workspace tabs do not move. The title bar's list is
 * its only place.
 *
 * **Allow from now on is a standing grant in one press**, offered because a chat was refused.
 * So the item says exactly what it allows before the press, in the core's own sentence
 * (`allows`): one pair, for you, on this machine. The button's name and tooltip say it too. It
 * offers nothing wider: no project-level grant and no "any persona" is reachable from here.
 *
 * **Every word is drawn as text.** The two persona names are the app's own record; the task's
 * name is the one text a chat chose, held to a task name's rule by the core and clipped here.
 * The brief is never shown: the core does not keep it.
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

/** How often and when, then the task, the workspace and the project. */
export function awayWhere(item: AwayItem, now: number): string {
  const when = ago(Math.max(0, Math.round(now - item.latest)));
  const often =
    item.times === 1
      ? `once, ${when}`
      : item.times === 2
        ? `twice, last ${when}`
        : `${item.times} times, last ${when}`;
  return [
    often,
    item.task != null ? `task ${clipped(item.task)}` : undefined,
    item.workspace != null ? clipped(item.workspace) : undefined,
    item.project,
  ]
    .filter((part) => part !== undefined)
    .join(" · ");
}

/** The items, each a group of the needs-you menu with its two answers. */
export function AwayRows({
  items,
  onAllow,
  onDismiss,
  now,
}: {
  items: readonly AwayItem[];
  onAllow?: (item: AwayItem) => void;
  onDismiss?: (item: AwayItem) => void;
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
        return (
          <Menu.Group
            key={`${item.plane}#away#${item.asking}#${item.target}#${item.workspace ?? ""}`}
            className="needs-you-row needs-you-permission needs-you-away"
            aria-label={`${said} · ${awayWhere(item, at)}`}
          >
            <p className="needs-you-says">
              <span className="needs-you-name">{said}</span>
              <span className="needs-you-where">{awayWhere(item, at)}</span>
            </p>
            {/* What the press allows, before the press: the core's sentence, whole. */}
            <p className="needs-you-says needs-you-allows">{item.allows}</p>
            <Menu.Item
              className="more-tab needs-you-answer allows"
              aria-label={`Allow from now on: ${pair}, for you on this machine`}
              title={item.allows}
              onSelect={() => onAllow?.(item)}
            >
              Allow from now on
            </Menu.Item>
            <Menu.Item
              className="more-tab needs-you-answer"
              aria-label={`Dismiss: ${said}`}
              title="Take this off the list. Nothing is allowed."
              onSelect={() => onDismiss?.(item)}
            >
              Dismiss
            </Menu.Item>
          </Menu.Group>
        );
      })}
    </>
  );
}
