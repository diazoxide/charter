import type { Ask } from "./bindings";

/**
 * What opening this project will put in force, and the question about it.
 *
 * **The prompt IS the prompt** (charter ADR 0035). charter's CLI asks by printing a second
 * command to type, because `util.py` has nothing that reads stdin and a hook blocked on stdin
 * hangs a turn — a constraint about the CLI and about nothing else. Here there is a window and
 * a person looking at it, so the question is asked where the answer is given, and nothing is
 * copied from the CLI's printed-command shape.
 *
 * What it shows is what charter can enumerate, and no more: the plugins the project's
 * committed settings enable, the environment they set, and the programs its reopen record
 * would start. It does not and cannot summarise the project's persona charters, its memory or
 * its todos, which are text a model will read and act on — charter has no model and makes no
 * judgements about the content of work. Said on screen, in the last line, rather than left for
 * whoever first assumes the dialog covered everything.
 *
 * A project that has changed what it contributes asks again, and the changes are charter's own
 * words for them (`machine::Change`) rather than a second description written here. Which
 * changes re-ask and which are only reported is `machine::Consent`'s decision and is taken
 * before this is drawn: a dialog that is up is a dialog that has to be answered.
 */
export function ApprovePlane({
  ask,
  onApprove,
  onCancel,
}: {
  ask: Ask;
  /** The operator's yes, carrying back the contribution they were shown — so the approval is
   *  for what was on screen and not for whatever the project says by the time it is clicked. */
  onApprove: (ask: Ask) => void;
  onCancel: () => void;
}) {
  const { contributes } = ask;
  const nothing =
    contributes.plugins.length === 0 &&
    contributes.env.length === 0 &&
    contributes.starts.length === 0 &&
    contributes.profiles.length === 0;
  return (
    <div className="asking">
      <div className="warning" role="dialog" aria-modal="true" aria-labelledby="approve-plane">
        <h2 id="approve-plane">
          {ask.first ? "Open this project?" : "This project has changed since you approved it"}
        </h2>
        {/* The root charter resolved, not the directory that was handed in: a picker pointed
            at a subfolder opens the project above it, and approving a directory you did not
            choose is the failure this dialog exists to prevent, arrived at from the friendly
            end. */}
        <p className="where">
          <code>{ask.path}</code>
        </p>

        {ask.changes.length > 0 && (
          <>
            <h3>What changed</h3>
            <ul className="changes">
              {ask.changes.map((change) => (
                <li key={change}>{change}</li>
              ))}
            </ul>
          </>
        )}

        <h3>{ask.first ? "What this project contributes" : "What it contributes now"}</h3>
        {nothing && (
          <p className="came-back">
            Nothing charter can enumerate: it enables no plugins, sets no environment, and its
            record names no chat to start.
          </p>
        )}
        {contributes.plugins.length > 0 && (
          <section>
            <h4>Plugins it enables in every chat</h4>
            <ul className="contributes">
              {contributes.plugins.map(([name, how]) => (
                <li key={name}>
                  <code>{name}</code>
                  {how && <span className="value"> {how}</span>}
                </li>
              ))}
            </ul>
          </section>
        )}
        {contributes.env.length > 0 && (
          <section>
            {/* Values and not only names: an `env` whose PATH gains a directory is a different
                grant from the one that was approved, and a list of names could not show it. */}
            <h4>Environment it sets on every harness</h4>
            <ul className="contributes">
              {contributes.env.map(([name, value]) => (
                <li key={name}>
                  <code>
                    {name}={value}
                  </code>
                </li>
              ))}
            </ul>
          </section>
        )}
        {contributes.starts.length > 0 && (
          <section>
            <h4>Programs opening it would start</h4>
            <ul className="contributes">
              {contributes.starts.map(([what]) => (
                <li key={what}>
                  <code>{what}</code>
                </li>
              ))}
            </ul>
          </section>
        )}
        {contributes.profiles.length > 0 && (
          <section>
            {/* Drawn, and deliberately drawn apart from the list above. The record chooses
                which of THIS machine's own harness profiles runs and never what it runs, and
                `profiletrust` shows a new or changed command line before it runs. */}
            <h4>Chats it would start on your own harness profiles</h4>
            <ul className="contributes">
              {contributes.profiles.map(([what]) => (
                <li key={what}>
                  <code>{what}</code>
                </li>
              ))}
            </ul>
          </section>
        )}

        <p className="came-back">
          charter can only list what it can read. A project&rsquo;s persona charters, memory and
          todos are text a model will read and act on, and charter makes no judgement about them.
        </p>

        <div className="doing">
          <button type="button" onClick={() => onApprove(ask)}>
            {ask.first ? "Open project" : "Open it anyway"}
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
