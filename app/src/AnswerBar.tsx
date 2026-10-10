import type { ReactNode } from "react";

/**
 * **A question's answer bar** (#1210, D-1210-1): the one row every question dialog is answered
 * from — a way out and an act or two, and nothing to fill in, like the quit warning or a delete's
 * confirm. A *form* ends in `SettingActions` instead (V89j); this is its counterpart for a
 * question, and it is not a settings piece.
 *
 * **The row is the bar's; everything in it is the dialog's.** The buttons are native `<button>`s
 * (or Radix's own `Cancel` and `Action` around one) with their own `type`, `tabIndex={0}`,
 * `disabled` and `onClick`, in the order the dialog writes them. That order is the rule
 * (`docs/design-system.md`, *The answer bar*): **the way out first, then the acts, with the
 * answer that moves things on at the trailing edge**. An act that cannot be taken back says so
 * with `ends-it`, and it is never the one Return finds. Which button has the focus, and what
 * Escape does, stay with the dialog: the bar draws a row and handles no key.
 *
 * Drawn by `.answer` in `App.css`: a flex row at the trailing edge, with a gap, every button in
 * the bar's tokens. `answerBar.guard.test.ts` keeps any dialog from building the row by hand.
 */
export function AnswerBar({ children }: { children: ReactNode }) {
  return <div className="answer">{children}</div>;
}
