import { useCallback, useEffect, useState } from "react";
import { commands, type FirstRunFound, type ForgeRow, type HarnessRow } from "./bindings";

/**
 * What a machine that has never opened a project sees (FR-4, #603).
 *
 * **One question, and it is not where the plane goes** (W10). charter keeps a local project in
 * its own directory, with no remote, and the operator is asked only for the repo to work on.
 * That repo becomes a workspace named after it, and the first chat starts in its clone. The
 * copy is ADR 0072's: a code repo is a "repo", the plane is a "project", and the line that
 * says what a project is is the ADR's own. Sharing the project with a team comes later, and nothing here mentions accounts, cloud
 * or telemetry: none of them stands between a new machine and a working chat (G5, C2, B2).
 *
 * **What the machine has is shown, never asked about.** Which harnesses are installed and
 * signed in, and whether `gh` and `glab` are: a harness that is not signed in still starts, and asks for
 * its own login in its own first screen, which is where that question belongs.
 *
 * **Signing in to a forge is offered, not asked** (W10: "forge CLI auth detected and offered").
 * The first run comes before a repo is chosen, so which forge the project will use is not known
 * yet, and both CLIs are listed. A button beside a CLI's row, when it is installed and not
 * signed in, opens the local project and runs `<cli> auth login` in a shell tab there — the
 * CLI's own login, in a tab the operator can leave, so it adds nothing to the interrupt budget.
 *
 * The path box is there for the opener's reason: a native folder dialog cannot be driven by
 * the scenario tests, and an operator who knows the path types it faster than they click.
 */
export function FirstRun({
  onOpenRepo,
  onOpenProject,
  onSignInToForge,
  opening,
  trouble,
}: {
  /** Asks the core to open this repo into the local project. */
  onOpenRepo: (path: string) => void;
  /** Shows the ordinary opener, for a project that already exists. */
  onOpenProject: () => void;
  /** Opens the local project with `<cli> auth login` running in a shell tab. */
  onSignInToForge: (cli: string) => void;
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
      // A machine charter could not look at still opens a repo.
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
      <h1 id="first-run-heading">Open a repo to start</h1>
      <p className="came-back">
        charter has nothing saved on this machine yet. A project is where charter keeps your
        workspaces, personas and memory: charter makes one for you, on this machine only, and opens
        your repo in a workspace of its own. Nothing is written into your repo.
      </p>

      {/* `tabIndex={0}` on every button, per `docs/ui-primitives.md` (charter-app#189). */}
      <div className="doing">
        <button type="button" tabIndex={0} disabled={opening} onClick={pick}>
          Open a repo…
        </button>
      </div>

      <form
        className="by-path"
        onSubmit={(event) => {
          event.preventDefault();
          if (typed.trim() && !opening) onOpenRepo(typed.trim());
        }}
      >
        <label htmlFor="first-run-path">Or type the repo&apos;s path</label>
        <input
          id="first-run-path"
          type="text"
          value={typed}
          autoComplete="off"
          spellCheck={false}
          placeholder="/path/to/repo"
          onChange={(event) => setTyped(event.target.value)}
        />
        <button type="submit" tabIndex={0} disabled={!typed.trim() || opening}>
          Open
        </button>
      </form>

      {opening && (
        <p className="came-back" role="status">
          Copying your repo into its workspace…
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
            {found.forges.map((row) => (
              <li key={row.cli}>
                <span className="tab-name">{row.cli}</span>: {forgeSays(row)}
                {row.installed && !row.signed_in && (
                  <>
                    {" "}
                    <button type="button" tabIndex={0} onClick={() => onSignInToForge(row.cli)}>
                      Sign in to {row.title}
                    </button>
                  </>
                )}
              </li>
            ))}
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
  const why = `only needed to work with ${row.title}`;
  if (!row.installed) return `not installed; ${why}`;
  return row.signed_in ? "signed in" : `installed, not signed in; ${why}`;
}
