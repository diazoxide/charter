import { useState } from "react";
import { Field, SettingActions, SettingRow } from "./settings/components";

/**
 * **The git identity form** (FX-3): the fix for the doctor's `git identity` row, which needs a
 * name and an email before it can be applied.
 *
 * The fix registry's ids are plain (`charter doctor --fix <id>`), and a fix that takes input is
 * marked in the core (`FixId::takes_input`). The window draws this form wherever such a fix is
 * offered — the Doctor row's Fix button today, and a Notice carrying the same id — and hands
 * what is typed to the core, which checks both fields and answers each one's refusal. The
 * refusal is drawn under its field, by the design system's `SettingRow`, and nothing is
 * written until both are right.
 */

/** The fix ids whose Fix opens a form instead of applying at once. */
export const FIXES_WITH_A_FORM: ReadonlySet<string> = new Set(["git-identity"]);

/** Each field's refusal, in the core's words; `undefined` when the input was taken. */
export type IdentityRefused = { name: string[]; email: string[] };

export function GitIdentityForm({
  submit,
  onCancel,
  busy,
}: {
  /** Hand the input to the core. Resolves with each field's refusal, or `undefined` once the
   *  fix was applied (its outcome is the caller's to say). */
  submit: (name: string, email: string) => Promise<IdentityRefused | undefined>;
  onCancel: () => void;
  /** Out of reach while something else is being fixed or checked. */
  busy?: boolean;
}) {
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [refused, setRefused] = useState<IdentityRefused>();
  const [sending, setSending] = useState(false);

  return (
    <form
      className="doctor-form"
      aria-label="Git identity"
      onSubmit={(event) => {
        event.preventDefault();
        setSending(true);
        void submit(name, email)
          .then(setRefused)
          .finally(() => setSending(false));
      }}
    >
      <SettingRow
        label="Name"
        help="The name your commits are made under: written to git's global user.name."
        error={refused?.name}
        control={(ids) => (
          <Field ids={ids} kind="text" value={name} onChange={setName} placeholder="Ann Example" />
        )}
      />
      <SettingRow
        label="Email"
        help="Written to git's global user.email."
        error={refused?.email}
        control={(ids) => (
          <Field
            ids={ids}
            kind="text"
            value={email}
            onChange={setEmail}
            placeholder="ann@example.com"
          />
        )}
      />
      {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). */}
      <SettingActions>
        <button type="submit" tabIndex={0} disabled={busy || sending}>
          {sending ? "Setting…" : "Set identity"}
        </button>
        <button type="button" tabIndex={0} onClick={onCancel}>
          Cancel
        </button>
      </SettingActions>
    </form>
  );
}
