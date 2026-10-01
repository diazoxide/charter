import { useId } from "react";
import type { ForgeWord } from "./bindings";

/** What the core asked, and what to do with the answer. */
export type ForgeAsk = {
  /** Why the repo's remote does not say which forge — the core's words. */
  why: string;
  /** Called with the forge the operator picked. */
  answer: (forge: ForgeWord) => void;
};

/**
 * Which forge a new project's repos are on, asked only when the repo's remote does not say
 * (#839). `charter init`'s rule, in the window: the forge comes from the `origin` of the repo
 * the project is made for, and a remote that is not on github.com or gitlab.com, or no remote
 * at all, is asked about rather than guessed. Inline and one press, so it is not one more
 * modal on the way to the first chat (W10's interrupt budget).
 */
export function ForgeQuestion({ ask }: { ask: ForgeAsk }) {
  const heading = useId();
  return (
    <div className="asks" role="group" aria-labelledby={heading}>
      <p id={heading}>Which forge are its repos on?</p>
      <p className="came-back">
        charter reads the forge from the remote of the repo the project is made for, and here it
        could not: {ask.why}. You can change it later in Project settings.
      </p>
      {/* `tabIndex={0}` on every button, per `docs/ui-primitives.md` (charter-app#189). */}
      <div className="doing">
        <button type="button" tabIndex={0} onClick={() => ask.answer("github")}>
          GitHub
        </button>
        <button type="button" tabIndex={0} onClick={() => ask.answer("gitlab")}>
          GitLab
        </button>
      </div>
    </div>
  );
}
