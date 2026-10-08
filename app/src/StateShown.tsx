import {
  Check,
  Circle,
  CircleDashed,
  CircleSlash,
  Dot,
  Hand,
  MessageCircleQuestion,
  Minus,
  Pause,
  X,
  type LucideIcon,
} from "lucide-react";
import type { Shown, ShownShape } from "./shownState";
import { property } from "./theme/theme";

/** Each shape as it is drawn. */
const SHAPES: Readonly<Record<ShownShape, LucideIcon>> = {
  ring: Circle,
  hand: Hand,
  question: MessageCircleQuestion,
  tick: Check,
  cross: X,
  dash: Minus,
  slash: CircleSlash,
  dot: Dot,
  pause: Pause,
  "broken-ring": CircleDashed,
};

/**
 * **A state as a row shows it: its mark, then its word** (#1484, V100-3, V100-72).
 *
 * The word is always drawn. The mark carries it as its accessible name, as a tab's state mark
 * does, and the drawn word is kept from being read a second time. The colour is on the mark and
 * never on the word, which stays legible whatever a theme does with the state colours. A state
 * that needs you wears the hand the title bar's list wears.
 */
export function StateShown({ shown }: { shown: Shown }) {
  const Shape = SHAPES[shown.shape];
  return (
    <span className="shown-state" data-state={shown.kind}>
      <span
        className="shape"
        data-shape={shown.shape}
        data-mark={shown.kind === "needs-you" ? "needs-you" : undefined}
        // The state's colour, by its token: on the mark only.
        style={{ color: `var(${property(shown.token)})` }}
        role="img"
        aria-label={shown.word}
      >
        <Shape aria-hidden="true" />
      </span>
      <span className="word" aria-hidden="true">
        {shown.word}
      </span>
    </span>
  );
}
