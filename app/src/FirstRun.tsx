import { useCallback, useEffect, useState } from "react";
import { commands, type FirstRunFound, type ForgeRow, type HarnessRow } from "./bindings";

/**
 * What a machine that has never opened a project sees (FR-4, #603).
 *
 * **One question, and it is not where the plane goes** (W10). charter keeps a local project in
 * its own directory, with no remote, and the operator is asked only for the repository to work
 * on. That repository becomes a workspace named after it, and the first chat starts in its
 * clone. Sharing the project with a team comes later, and nothing here mentions accounts, cloud
 * or telemetry: none of them stands between a new machine and a working chat (G5, C2, B2).
 *
 * **What the machine has is shown, never asked about.** Which harnesses are installed and
 * signed in, and whether `gh` is: a harness that is not signed in still starts, and asks for
 * its own login in its own first screen, which is where that question belongs.
 *
 * The path box is there for the opener's reason: a native folder dialog cannot be driven by
 * the scenario tests, and an operator who knows the path types it faster than they click.
 */
export function FirstRun({
  onOpenRepo,
  onOpenProject,
  opening,
  trouble,
}: {
  /** Asks the core to open this repository into the local project. */
  onOpenRepo: (path: string) => void;
  /** Shows the ordinary opener, for a project that already exists. */
  onOpenProject: () => void;
  /** Whether the core is cloning it right now, so it is not asked twice. */
  opening: boolean;
  /** Why the last attempt opened nothing — the core's words, all of them. */
  trouble?: string;
}) {
  const [found, setFound] = useState<FirstRunFound>();
  const [typed, setTyped] = useState("");

  useEffect(() => {
    let gone = false;
    void commands
      .firstRunFound()
      .then((answer) => {
        if (!gone && answer.status === "ok") setFound(answer.data);
      })
      // A machine charter could not look at still opens a repository.
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, []);

  const pick = useCallback(() => {
    void commands
      .pickProject()
      .then((answer) => {
        if (answer.status === "ok" && answer.data) onOpenRepo(answer.data);
      })
      .catch(() => undefined);
  }, [onOpenRepo]);

  return (
    <section className="opener first-run" aria-labelledby="first-run-heading">
      <h1 id="first-run-heading">Open a repository to start</h1>
      <p className="came-back">
        charter keeps a project for you on this machine and opens the repository in a workspace of
        its own. Nothing is written into the repository.
      </p>

      {/* `tabIndex={0}` on every button, per `docs/ui-primitives.md` (charter-app#189). */}
      <div className="doing">
        <button type="button" tabIndex={0} disabled={opening} onClick={pick}>
          Open a repository…
        </button>
      </div>

      <form
        className="by-path"
        onSubmit={(event) => {
          event.preventDefault();
          if (typed.trim() && !opening) onOpenRepo(typed.trim());
        }}
      >
        <label htmlFor="first-run-path">Or type the repository&apos;s path</label>
        <input
          id="first-run-path"
          type="text"
          value={typed}
          autoComplete="off"
          spellCheck={false}
          placeholder="/path/to/repository"
          onChange={(event) => setTyped(event.target.value)}
        />
        <button type="submit" tabIndex={0} disabled={!typed.trim() || opening}>
          Open
        </button>
      </form>

      {opening && (
        <p className="came-back" role="status">
          Cloning the repository into its workspace…
        </p>
      )}

      {/* Verbatim: the sentence names the path and what was wrong with it. */}
      {trouble && (
        <p className="trouble said-in-full" role="alert">
          {trouble}
        </p>
      )}

      {found && (
        <>
          <h2 id="first-run-found">On this machine</h2>
          <ul className="first-run-found" aria-labelledby="first-run-found">
            {found.harnesses.map((row) => (
              <li key={row.name}>
                <span className="tab-name">{row.title}</span>: {harnessSays(row)}
              </li>
            ))}
            <li>
              <span className="tab-name">{found.forge.cli}</span>: {forgeSays(found.forge)}
            </li>
          </ul>
        </>
      )}

      <div className="doing">
        <button type="button" tabIndex={0} onClick={onOpenProject}>
          Open an existing project instead
        </button>
      </div>
    </section>
  );
}

function harnessSays(row: HarnessRow): string {
  if (!row.installed) return "not installed";
  return row.signed_in ? "ready" : "installed; it asks you to sign in when its chat starts";
}

function forgeSays(row: ForgeRow): string {
  if (!row.installed) return "not installed; only needed to work with GitHub";
  return row.signed_in ? "signed in" : "installed, not signed in; only needed to work with GitHub";
}
