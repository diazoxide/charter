import { useCallback, useEffect, useState } from "react";
import {
  commands,
  type FirstRunFound,
  type ForgeRow,
  type HarnessRow,
  type TemplateChoice,
  type TemplateRow,
} from "./bindings";

/** The radio values that are not a template's id. Ids are lower-case words, so neither clashes. */
const FITS = ":fits";
const NONE = ":none";

function choiceOf(value: string): TemplateChoice {
  if (value === FITS) return { kind: "fits" };
  if (value === NONE) return { kind: "no-template" };
  return { kind: "named", id: value };
}
import { type ForgeAsk, ForgeQuestion } from "./ForgeQuestion";
import { Choice, Field, SettingActions, SettingRow } from "./settings/components";

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
 * **The project template is chosen here, and already chosen** (FR-17). It starts on the one that
 * fits the repo, which the core works out from the files at the repo's top level, so an operator
 * who does not look at it still gets one, and picking it costs no extra step. A template adds
 * personas, a review checklist and the commands every harness asks about first; it writes
 * nothing into the repo.
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
  forgeAsk,
}: {
  /** Asks the core to open this repo into the local project, laid out from `template`. */
  onOpenRepo: (path: string, template: TemplateChoice) => void;
  /** Shows the ordinary opener, for a project that already exists. */
  onOpenProject: () => void;
  /** Opens the local project with `<cli> auth login` running in a shell tab. */
  onSignInToForge: (row: ForgeRow) => void;
  /** Whether the core is cloning it right now, so it is not asked twice. */
  opening: boolean;
  /** Why the last attempt opened nothing — the core's words, all of them. */
  trouble?: string;
  /** Which forge the repo's repos are on, when its remote did not say (#839). */
  forgeAsk?: ForgeAsk;
}) {
  const [found, setFound] = useState<FirstRunFound>();
  const [typed, setTyped] = useState("");
  const [template, setTemplate] = useState(FITS);
  // The template that fits a typed path, by id, with the path it was asked about: `null` when
  // none fits. A folder picked in the dialog opens at once, so this is the typed path's alone,
  // and an answer about a path no longer typed is not shown.
  const [fitting, setFitting] = useState<{ path: string; id: string | null }>();
  const path = typed.trim();
  const fits = fitting !== undefined && fitting.path === path ? fitting.id : undefined;

  useEffect(() => {
    if (path === "") return;
    let gone = false;
    void commands
      .templateThatFits(path)
      .then((answer) => {
        if (!gone && answer.status === "ok") setFitting({ path, id: answer.data });
      })
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [path]);

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
        if (answer.status === "ok" && answer.data) onOpenRepo(answer.data, choiceOf(template));
      })
      .catch(() => undefined);
  }, [onOpenRepo, template]);

  return (
    <section className="opener first-run" aria-labelledby="first-run-heading">
      <h1 id="first-run-heading">Open a repo to start</h1>
      <p className="came-back">
        charter has nothing saved on this machine yet. A project is where charter keeps your
        workspaces, personas and memory: charter makes one for you, on this machine only, and opens
        your repo in a workspace of its own. Nothing is written into your repo.
      </p>

      {/* `tabIndex={0}` on every button, per `docs/ui-primitives.md` (charter-app#189). */}
      <SettingActions>
        <button type="button" tabIndex={0} disabled={opening} onClick={pick}>
          Open a repo…
        </button>
      </SettingActions>

      {/* The path and the template are rows of the settings set (DS-3d, #1176). */}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (typed.trim() && !opening) onOpenRepo(typed.trim(), choiceOf(template));
        }}
      >
        <SettingRow
          label="Or type the repo's path"
          help="A repo's folder on this machine."
          control={(ids) => (
            <Field
              ids={ids}
              kind="text"
              value={typed}
              placeholder="/path/to/repo"
              onChange={setTyped}
            />
          )}
        />
        <SettingActions>
          <button type="submit" tabIndex={0} disabled={!typed.trim() || opening}>
            Open
          </button>
        </SettingActions>
      </form>

      {found && found.templates.length > 0 && (
        <SettingRow
          label="Project template"
          help={
            "Personas, a review checklist and the commands charter asks you about before a chat " +
            "runs them, for the stack you work in. Nothing is written into your repo."
          }
          grouped
          control={(ids) => (
            <Choice
              ids={ids}
              kind="radio"
              options={[
                { value: FITS, label: "Fits the repo", says: fitsSays(fits, found.templates) },
                ...found.templates.map((one) => ({
                  value: one.id,
                  label: one.title,
                  says: one.summary,
                })),
                { value: NONE, label: "None", says: "Only the steward persona." },
              ]}
              value={template}
              onValueChange={setTemplate}
              disabled={opening}
            />
          )}
        />
      )}

      {opening && (
        <p className="came-back" role="status">
          Copying your repo into its workspace…
        </p>
      )}

      {forgeAsk && <ForgeQuestion ask={forgeAsk} />}

      {/* Verbatim: the sentence names the path and what was wrong with it. */}
      {trouble && (
        <p className="trouble said-in-full" role="alert">
          {trouble}
        </p>
      )}

      {found && (
        <>
          <h2 id="first-run-found">On this machine</h2>
          {found.harnesses.every((row) => !row.installed) && (
            <p className="came-back">
              No harness is installed on this machine. Once your repo is open, charter lists each
              one with its own installer, which runs in a shell tab when you press Install.
            </p>
          )}
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
                    <button type="button" tabIndex={0} onClick={() => onSignInToForge(row)}>
                      Sign in to {row.title}
                    </button>
                  </>
                )}
              </li>
            ))}
          </ul>
        </>
      )}

      <SettingActions>
        <button type="button" tabIndex={0} onClick={onOpenProject}>
          Open an existing project instead
        </button>
      </SettingActions>
    </section>
  );
}

/** What "Fits the repo" says it will pick: the template, once there is a path to look at. */
function fitsSays(fits: string | null | undefined, templates: TemplateRow[]): string {
  if (fits === undefined) return "charter picks by the files at the repo's top level.";
  if (fits === null) return "None fits this repo, so it opens with no template.";
  const title = templates.find((one) => one.id === fits)?.title ?? fits;
  return `${title}, by the files at the repo's top level.`;
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
