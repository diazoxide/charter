import { useEffect, useState } from "react";
import type { ProfileRow, StartOptions } from "./bindings";

/**
 * What a new chat asks before anything runs.
 *
 * **No harness starts until somebody picks a profile** (ADR 0022). It shows even when one
 * profile is available, because skipping it would bring back the harness nobody picked on a
 * one-harness machine, and one profile costs one Enter. Escape closes it having started
 * nothing — no harness ran, and no identity was recorded.
 *
 * A dialog rather than a pane that draws itself: the tmux frame put the selector in the
 * chat's own pane because tmux had already made the pane, and here there is no chat yet.
 * Nothing is opened until a row is picked, so cancelling leaves nothing to tear down.
 *
 * A profile whose command charter has not recorded running shows that command and asks. The
 * file is gitignored, so an edit to it leaves no diff for a reviewer to catch, and nothing
 * stops a chat editing plane config — which is why the ask is about the words that are
 * about to run and not about the profile's name.
 */
export function StartChat({
  options,
  trouble,
  onStart,
  onApprove,
  onCancel,
}: {
  options: StartOptions;
  /** Why the last attempt did not start, if it did not. */
  trouble?: string;
  onStart: (profile: string, persona: string | null) => void;
  onApprove: (profile: string) => void;
  onCancel: () => void;
}) {
  const [profile, setProfile] = useState<string | undefined>(
    () => options.profiles.find((p) => p.is_default)?.name ?? options.profiles[0]?.name,
  );
  const [persona, setPersona] = useState<string | null>(options.persona);
  const picked = options.profiles.find((p) => p.name === profile);

  // Escape starts nothing. Listened for on the window rather than the dialog, so it answers
  // wherever focus happens to be inside it.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onCancel]);

  return (
    <div className="asking">
      <div
        className="warning starting"
        role="dialog"
        aria-modal="true"
        aria-labelledby="start-chat"
      >
        <h2 id="start-chat">Start a chat</h2>

        {options.ignore_fix && (
          <p className="honest mid-turn" role="alert">
            git would carry <code>charter.local.toml</code>, so every profile it declares is refused
            until that is fixed: <code>{options.ignore_fix}</code>
          </p>
        )}
        {options.declares_none && !options.ignore_fix && (
          <p className="honest">
            {/* Said rather than shown as an empty list: a plane that declares nothing is the
                ordinary first state, not a fault, and the built-ins below still start. */}
            This plane declares no profiles of its own, so these are charter&apos;s built-ins.
            Declare your own in <code>charter.local.toml</code>, which stays on this machine.
          </p>
        )}

        <fieldset className="profiles">
          <legend>Harness</legend>
          {options.profiles.map((row) => (
            <Row key={row.name} row={row} picked={row.name === profile} onPick={setProfile} />
          ))}
        </fieldset>

        <fieldset className="personas">
          <legend>Persona</legend>
          <label>
            <input
              type="radio"
              name="persona"
              checked={persona === null}
              onChange={() => setPersona(null)}
            />
            <span className="who">none</span>
          </label>
          {options.personas.map((who) => (
            <label key={who}>
              <input
                type="radio"
                name="persona"
                checked={persona === who}
                onChange={() => setPersona(who)}
              />
              <span className="who">{who}</span>
              {who === options.persona && <span className="what">plane default</span>}
            </label>
          ))}
        </fieldset>

        {options.refused.length > 0 && (
          <details className="refused">
            {/* A missing profile is a row that is not in the list — easy to miss in a way a
                missing panel is not — so the ones charter will not use say why. */}
            <summary>{options.refused.length} refused</summary>
            <ul>
              {options.refused.map(([name, why]) => (
                <li key={name}>
                  <span className="who">{name}</span> {why}
                </li>
              ))}
            </ul>
          </details>
        )}

        {picked?.approval && (
          <p className="honest approve" role="alert">
            charter has not run this profile{" "}
            {picked.approval === "new" ? "before" : "as it now stands"}. It would run:{" "}
            <code>{picked.shown}</code>
          </p>
        )}
        {trouble && (
          <p className="honest mid-turn" role="alert">
            {trouble}
          </p>
        )}

        <div className="answer">
          {/* Cancel first and focused: starting a chat runs a command with nothing between
              the key and the exec, so it is never what a stray Return key finds. */}
          <button autoFocus onClick={onCancel}>
            Cancel
          </button>
          {picked?.approval ? (
            <button className="ends-it" onClick={() => onApprove(picked.name)} disabled={!picked}>
              Approve and start
            </button>
          ) : (
            <button onClick={() => profile && onStart(profile, persona)} disabled={!profile}>
              Start
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function Row({
  row,
  picked,
  onPick,
}: {
  row: ProfileRow;
  picked: boolean;
  onPick: (name: string) => void;
}) {
  return (
    <label>
      <input type="radio" name="profile" checked={picked} onChange={() => onPick(row.name)} />
      <span className="who">{row.name}</span>
      <span className="what">{row.kind}</span>
      {/* Already contained by the core: a profile is a file a chat can write, and a control
          byte in a command must never redraw this row. */}
      <code className="where">{row.shown}</code>
      <span className="from">{row.source}</span>
      {row.is_default && <span className="what">default</span>}
      {row.approval && <span className="what needs-approval">{row.approval}</span>}
    </label>
  );
}
