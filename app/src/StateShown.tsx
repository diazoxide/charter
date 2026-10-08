import {
  Check,
  Circle,
  Ellipsis,
  Hand,
  Hourglass,
  MessageCircleQuestion,
  Minus,
  Pause,
  TriangleAlert,
  X,
  type LucideIcon,
} from "lucide-react";
import { useState } from "react";
import { useArrived } from "./lib/arrived";
import type { Shown, ShownShape } from "./shownState";
import { property } from "./theme/theme";

/**
 * Each shape as it is drawn. **No two share an outline** at the size of a row: one ring only
 * (working), and what is not known and what ended without a report are three dots and a
 * triangle, not two more rings a few pixels apart. `dot` is the same circle filled
 * (`.shown-state [data-shape="dot"]`), since an outline that small is no mark at all.
 */
export const SHAPES: Readonly<Record<ShownShape, LucideIcon>> = {
  ring: Circle,
  hand: Hand,
  hourglass: Hourglass,
  question: MessageCircleQuestion,
  tick: Check,
  cross: X,
  dash: Minus,
  triangle: TriangleAlert,
  dot: Circle,
  pause: Pause,
  dots: Ellipsis,
};

/**
 * **A state as a row shows it: its mark, then its word** (#1484, V100-3, V100-72).
 *
 * The word is always drawn, and it is the text: what a person reads is what a screen reader
 * reads, and the mark beside it is decoration. The colour is on the mark and never on the
 * word, which stays legible whatever a theme does with the state colours. A state that needs
 * you wears the hand the title bar's list wears, **and the hand knocks twice when the chat
 * starts needing you** (`arrived`): only on that change, never because a row was drawn.
 *
 * **The knock is played once per change** (#1499). The Chats list moves its rows, and an
 * element put back into the document plays its animation again, so a class left on for good
 * would knock for a chat that has needed you for an hour each time another row moved past it.
 * The class comes off when the animation ends, and goes on again at the next change.
 *
 * `changed` is for a row drawn for the first time in a state its chat came into while the row
 * was not on screen: a task that started needing you under a session that had folded by
 * itself. It did change, and nothing here saw it, so the list says so.
 */
export function StateShown({ shown, changed = false }: { shown: Shown; changed?: boolean }) {
  const Shape = SHAPES[shown.shape];
  // A state this row CHANGED to, not the one it was drawn in (`useArrived`).
  const arrived = useArrived(shown.kind) || changed;
  /** The state whose arrival has been played: its motion is over until the next change. */
  const [played, setPlayed] = useState<Shown["kind"]>();
  // Another state since: the next time it comes to this one is a change again.
  if (played !== undefined && played !== shown.kind) setPlayed(undefined);
  return (
    <span
      className={arrived && played !== shown.kind ? "shown-state arrived" : "shown-state"}
      data-state={shown.kind}
      onAnimationEnd={() => setPlayed(shown.kind)}
    >
      <span
        className="shape"
        data-shape={shown.shape}
        data-mark={shown.kind === "needs-you" ? "needs-you" : undefined}
        // The state's colour, by its token: on the mark only.
        style={{ color: `var(${property(shown.token)})` }}
        aria-hidden="true"
      >
        <Shape />
      </span>
      <span className="word">{shown.word}</span>
    </span>
  );
}
