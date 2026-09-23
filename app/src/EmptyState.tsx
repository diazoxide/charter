import type { ReactNode } from "react";
import clsx from "clsx";

/**
 * What a surface says when there is nothing in it, and what to do about it.
 *
 * **One component with several users, which is the operator's own instruction** — *"make some
 * beautiful empty state component reusable"*. It is used by the centre when a window holds no
 * chats, by the centre again when this workspace holds none, and by every panel's list
 * (`PanelList.tsx`). Three callers on the day it was written, which is the bar `docs/
 * design-system.md` sets for a shared component rather than a second copy of a paragraph.
 *
 * # Why the action is a node and not an offer id
 *
 * The button on an empty state is a row of the catalogue — `chat.new` for the centre — and
 * `Doer` in `PlaneView.tsx` is the one thing that draws a catalogue row as a button. This
 * component takes the drawn button rather than the id for two reasons, and the second is the
 * real one:
 *
 * - `Doer` lives beside the window that owns the catalogue, and importing it here would point
 *   a leaf component back at the screen that renders it.
 * - **An empty state with no way out is a legitimate empty state.** A panel with no todos
 *   offers nothing, because charter has no verb that writes a todo; saying so is the whole of
 *   what that panel can do. An `offer` prop would make the absence look like an omission.
 *
 * # It is centred, and that is the whole of the layout it asks for
 *
 * `size="page"` is the middle of whatever box it is in — the centre region's, which is what the
 * operator asked for: *"lets make open new tab button on empty page center"*. `size="panel"` is
 * the same thing at a sidebar's width, with the type scaled down and the padding cut, because a
 * 260 px column cannot carry a page's empty state without the headline wrapping to three lines
 * — which is the defect one region over that this change exists to fix.
 */
export function EmptyState({
  headline,
  body,
  mark: Mark,
  action,
  size = "page",
  testid,
}: {
  /** The one line. A claim about what is true, never an instruction. */
  headline: string;
  /** A sentence under it, where there is more to say than the headline. */
  body?: string;
  /** The glyph above it, where one helps. Lucide hides a nameless icon from assistive
   *  technology by itself, so it is decoration on top of the headline and never instead. */
  mark?: React.ComponentType<{ className?: string }>;
  /** The way out, already drawn — see the docstring. */
  action?: ReactNode;
  size?: "page" | "panel";
  testid?: string;
}) {
  return (
    <div className={clsx("empty-state", `empty-state-${size}`)} data-testid={testid}>
      {Mark && <Mark className="empty-state-mark" />}
      <p className="empty-state-headline">{headline}</p>
      {body !== undefined && body !== "" && <p className="empty-state-body">{body}</p>}
      {action !== undefined && <div className="empty-state-action">{action}</div>}
    </div>
  );
}
