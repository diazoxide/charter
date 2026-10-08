import {
  Check,
  Circle,
  Ellipsis,
  Hand,
  MessageCircleQuestion,
  Minus,
  Pause,
  TriangleAlert,
  X,
  type LucideIcon,
} from "lucide-react";
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
 */
export function StateShown({ shown }: { shown: Shown }) {
  const Shape = SHAPES[shown.shape];
  // A state this row CHANGED to, not the one it was drawn in (`useArrived`).
  const arrived = useArrived(shown.kind);
  return (
    <span className={arrived ? "shown-state arrived" : "shown-state"} data-state={shown.kind}>
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
