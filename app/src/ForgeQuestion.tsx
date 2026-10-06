import type { ForgeWord } from "./bindings";
import { SettingActions, SettingRow } from "./settings/components";

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
 * at all, is asked about rather than guessed. Inline and one press. Inline does not make it
 * free: it is a group named by its question, so W10's interrupt budget counts it
 * (`interruptBudget.ts`), and on a new machine it is one of the three the first run can spend.
 */
export function ForgeQuestion({ ask }: { ask: ForgeAsk }) {
  // A row of the settings set (DS-3e): the question is its name, why it is asked its help, and
  // the answers its buttons. The whole row is the group the question names, so the why is in it.
  return (
    <div role="group" aria-label={QUESTION}>
      <SettingRow
        label={QUESTION}
        grouped
        help={
          "purlis reads the forge from the remote of the repo the project is made for, and here " +
          `it could not: ${ask.why}. You can change it later in Settings, at the Project level.`
        }
        control={() => (
          // `tabIndex={0}` on every button, per `docs/ui-primitives.md` (charter-app#189).
          <SettingActions>
            <button type="button" tabIndex={0} onClick={() => ask.answer("github")}>
              GitHub
            </button>
            <button type="button" tabIndex={0} onClick={() => ask.answer("gitlab")}>
              GitLab
            </button>
          </SettingActions>
        )}
      />
    </div>
  );
}

const QUESTION = "Which forge are its repos on?";
