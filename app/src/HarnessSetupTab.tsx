import { useEffect, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { commands, type HarnessRow, type HarnessSetupFound, type PlaneId } from "./bindings";
import { askHarnessSetup } from "./harnessSetup";

/**
 * **No harness found** (FR-29, W10): what the first chat is on a machine with no harness
 * installed — W10's *"official installers in a shell tab, the harness's own login, local model
 * fallback"*.
 *
 * - **Official installers in a shell tab.** Each harness is listed with its vendor's own
 *   install command, shown before anything runs (`purlis_core::noharness::installer`). Install
 *   opens a shell tab at the project's root and the core types that command into it and runs it
 *   — one press, which ruling V65 allows only for a compiled-in installer shown word for word.
 *   The tab names the harness and never sends the words. The shell is the operator's, so the
 *   installer runs with exactly the authority it would have in their own terminal. Each one
 *   installs where charter looks, so Check again finds it with no restart.
 * - **The harness's own login.** Start a chat is the picker (ADR 0022), on the harness just
 *   installed (else the first one installed), and a harness that is
 *   not signed in asks for its own login in its own first screen. charter never carries one
 *   (ADR 0087).
 * - **A local model fallback.** A local model server already answering on this machine is
 *   named, with the harness that can use it and no account. Pointing that harness at it is
 *   MS-17's adapter (ADR 0087 §8); here it is said, not configured.
 *
 * **A tab, not a dialog** (the operator's 2026-09-23 ruling: new surfaces default to view
 * tabs). The shell it opens comes in front of it, and the operator comes back to it by its tab.
 * Nothing here runs on a timer: Check again is how the machine is looked at again.
 */
export function HarnessSetupTab({
  plane,
  cwd,
  workspace,
}: {
  plane: PlaneId;
  /** Where the first chat starts: the repo's clone. */
  cwd: string;
  /** The strip the installer's shell is filed on. */
  workspace: string;
}) {
  const [found, setFound] = useState<HarnessSetupFound>();
  /** The harness whose Install was pressed last: what Start a chat starts on, once it is in. */
  const [pressed, setPressed] = useState<string>();
  const [trouble, setTrouble] = useState<string>();
  /** Bumped by Check again, to look at the machine again. */
  const [asked, setAsked] = useState(0);
  const [looking, setLooking] = useState(true);

  useEffect(() => {
    let gone = false;
    void commands
      // Harnesses and local models only: the forge CLIs' sign-in checks are the first run's.
      .harnessSetupFound()
      .then((answer) => {
        if (gone) return;
        if (answer.status === "error") setTrouble(answer.error);
        else {
          setTrouble(undefined);
          setFound(answer.data);
        }
      })
      .catch((err: unknown) => {
        if (!gone) setTrouble(String(err));
      })
      .finally(() => {
        if (!gone) setLooking(false);
      });
    return () => {
      gone = true;
    };
  }, [asked]);

  if (found === undefined && trouble === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Looking at this machine…
      </p>
    );
  }

  const harnesses = found?.harnesses ?? [];
  const any = harnesses.some((row) => row.installed);
  const startOn =
    harnesses.find((row) => row.installed && row.name === pressed) ??
    harnesses.find((row) => row.installed);
  const local = found?.local_models ?? [];

  return (
    <div className="harness-setup">
      <h3>{any ? "A harness is installed" : "No harness found"}</h3>
      <p className="came-back">
        {any
          ? "Start a chat and pick it. A harness that is not signed in asks for its own login when its chat starts."
          : "A chat runs a harness, and none is installed on this machine. Install one with its own installer: Install runs that command in a shell tab. When it has finished, press Check again. Signing in is the harness's own first screen, when its chat starts."}
      </p>

      {/* Verbatim: the core's sentence. */}
      {trouble && (
        <p className="trouble said-in-full" role="alert">
          {trouble}
        </p>
      )}

      <ul className="harness-setup-rows" aria-label="Harnesses">
        {harnesses.map((row) => (
          <li key={row.name}>
            <span className="tab-name">{row.title}</span>: {says(row)}
            {!row.installed && (
              <>
                <code className="harness-installer">{row.installer}</code>
                <span className="came-back">from {row.installer_page}</span>
                {/* `tabIndex={0}` on every button, per `docs/ui-primitives.md` (#189). */}
                <button
                  type="button"
                  tabIndex={0}
                  onClick={() => {
                    setPressed(row.name);
                    askHarnessSetup({ plane, harness: row.name, way: "install", workspace });
                  }}
                >
                  Install {row.title}
                </button>
              </>
            )}
          </li>
        ))}
      </ul>

      {local.length > 0 ? (
        <section aria-labelledby="harness-setup-local">
          <h3 id="harness-setup-local">A model on this machine</h3>
          {local.map((one) => (
            <p className="came-back" key={one.title}>
              {one.title} is running on this machine, at <code>{one.base_url}</code>. {one.harness}{" "}
              can use it with no account: install {one.harness}, then add {one.title} as a provider
              in its config, as its providers page shows.
            </p>
          ))}
        </section>
      ) : (
        <p className="came-back">
          No account? opencode can run on a model served on this machine by Ollama or LM Studio;
          start one and press Check again.
        </p>
      )}

      <div className="doing">
        <button
          type="button"
          tabIndex={0}
          disabled={looking}
          onClick={() => {
            setLooking(true);
            setAsked((was) => was + 1);
          }}
        >
          Check again
        </button>
        {startOn && (
          <button
            type="button"
            tabIndex={0}
            onClick={() => askHarnessSetup({ plane, harness: startOn.name, way: "chat", cwd })}
          >
            Start a chat
          </button>
        )}
      </div>
    </div>
  );
}

function says(row: HarnessRow): string {
  if (!row.installed) return "not installed";
  return row.signed_in ? "ready" : "installed; it asks you to sign in when its chat starts";
}
