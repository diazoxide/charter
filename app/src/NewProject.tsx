import { useCallback, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { commands } from "./bindings";
import { type ForgeAsk, ForgeQuestion } from "./ForgeQuestion";
import { Choice, Field, SettingActions, SettingRow } from "./settings/components";

/**
 * Making a new project — a plane charter scaffolds — and the one decision it asks about.
 *
 * **The prompt IS the prompt** (ADR 0035). The CLI's shape is `mkdir`, `cd`, `charter
 * init`, because a hook blocked on stdin hangs a turn and `util.py` reads none; here there is a
 * window and a person looking at it, so the directory is picked and the question is asked where
 * the answer is given. Nothing is copied from the printed two-command shape.
 *
 * **The default writes nothing into anybody's repository** (ADR 0035, spec decision 27).
 * `charter init` used to scaffold the plane into whatever directory it was run in, and offer to
 * clone that repo as the first one. The opener destroyed the case that rested on: a directory
 * chosen in a file dialog has nobody standing in it, and writing `charter.toml`, `personas/`
 * and a block of rules into a tracked `.gitignore` is a write nobody typed. So a directory that
 * is the top of a git repository is refused, in the core's own four-line sentence, which names
 * the plane-beside-it shape and the flag that asks for the old one.
 *
 * **"Make this repo itself the plane" is that flag, as a box.** charter's own plane is a
 * repository, which is why the option is here at all — and it is never the default, and never
 * ticked for the operator.
 *
 * **Adopting a repository is the other half, and it is the one ADR 0035 calls the default**
 * (charter-app#175). The record's sentence is that `charter init` on an existing repo *"adopts
 * that repo as the plane's first clone and makes the plane beside it"*, so this asks for two
 * directories rather than showing a refusal about them: where the plane goes, and which repo
 * it starts with. The repo is cloned into `workspaces/<default>/<name>/` with charter's git
 * policy applied to the clone, and **nothing is written into the repo itself** — it is read,
 * and only read.
 *
 * The two answers are separate boxes because they are separate answers. Deriving the plane's
 * directory from the repo's (`../<name>-plane`) would put charter's guess where the operator's
 * decision belongs, on the one field this dialog exists to collect.
 *
 * Whatever is scaffolded is then opened **through the trust gate** — see `create_project`. A
 * plane charter has just made is still a plane this machine has approved nothing about, so the
 * ordinary end of this dialog is the approval dialog, on the same path a recents row takes.
 *
 * **All of the above is under Advanced now** (FR-4, #603, ruling W10). The default asks one
 * thing, the repository, and opens it into this machine's local project in a workspace named
 * after it — the first run's own path, so nobody is asked where a plane goes before they have a
 * reason to care. The two-directory form is a `<details>`, closed, because the browser has a
 * collapsible (`docs/ui-primitives.md`).
 *
 * **Every answer is drawn from the settings set** (DS-3c, #1175; ADR 0037's 2026-10-04
 * amendment): a {@link SettingRow} holding a {@link Field} or the box as a {@link Choice}, so a
 * line of help is its box's own description. A `Browse…` sits in its box's row, beside it.
 */
export function NewProject({
  /** Why the last attempt made nothing — **the core's lines, unchanged and all of them**. */
  trouble,
  /** Which forge the project's repos are on, when the repo's remote did not say (#839). */
  forgeAsk,
  /** The form changed, so a pending question about the forge is about a form that is gone. */
  onEdit,
  /** Whether charter is making it right now, so the answer cannot be given twice. */
  making,
  onCreate,
  onOpenRepo,
  opening,
  onCancel,
}: {
  trouble?: string;
  forgeAsk?: ForgeAsk;
  onEdit?: () => void;
  making: boolean;
  onCreate: (path: string, planeIsThisRepo: boolean, adopt: string) => void;
  /** Opens a repo into this machine's local project (FR-4). */
  onOpenRepo: (path: string) => void;
  /** Whether that is happening right now. */
  opening: boolean;
  onCancel: () => void;
}) {
  // A question about the forge is about the answers that were sent. Once one of them changes,
  // the next press asks again rather than answering for a form nobody sent (#848 review).
  const [repo, keepRepo] = useState("");
  const [path, keepPath] = useState("");
  const [adopt, keepAdopt] = useState("");
  const [planeIsThisRepo, keepPlaneIsThisRepo] = useState(false);
  const setRepo = (value: string) => {
    keepRepo(value);
    onEdit?.();
  };
  const setPath = (value: string) => {
    keepPath(value);
    onEdit?.();
  };
  const setAdopt = (value: string) => {
    keepAdopt(value);
    onEdit?.();
  };
  const setPlaneIsThisRepo = (value: boolean) => {
    keepPlaneIsThisRepo(value);
    onEdit?.();
  };
  const ready = path.trim() !== "" && !making;

  // One picker, told where to put its answer. Both fields ask the same question of the same
  // file dialog — which directory — and two copies of it would be two places to fix the day
  // a cancelled pick stops answering null.
  const pick = useCallback((into: (chosen: string) => void) => {
    void commands
      .pickProject()
      .then((answer) => {
        // A cancelled dialog is null and is not a failure: nothing is said and nothing moves.
        if (answer.status === "ok" && answer.data) into(answer.data);
      })
      .catch(() => undefined);
  }, []);

  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content
          className="warning"
          aria-labelledby="new-project"
          onInteractOutside={(e) => e.preventDefault()}
          // Radix focuses the first box as it opens, which is the repo: nothing to override.
        >
          <Dialog.Title id="new-project">New project</Dialog.Title>
          <p className="came-back">
            Pick a repo. charter opens it in a workspace of its own, in the project it keeps on this
            machine. Nothing is written into your repo.
          </p>

          <form
            onSubmit={(event) => {
              event.preventDefault();
              if (repo.trim() !== "" && !opening) onOpenRepo(repo.trim());
            }}
          >
            <SettingRow
              label="Repo"
              control={(ids) => (
                <>
                  <Field
                    kind="text"
                    ids={ids}
                    value={repo}
                    placeholder="/where/the/repo/is"
                    onChange={setRepo}
                  />
                  {/* `tabIndex={0}`, per `docs/ui-primitives.md` (charter-app#186). */}
                  <button
                    type="button"
                    tabIndex={0}
                    aria-label="Browse for the repo"
                    onClick={() => pick(setRepo)}
                  >
                    Browse…
                  </button>
                </>
              )}
            />
            {/* The core's refusal, all of it, for whichever form was sent last. */}
            {trouble && (
              <p className="trouble said-in-full" role="alert">
                {trouble}
              </p>
            )}
            {/* Asked for whichever form was sent last, as the refusal is (#839). */}
            {forgeAsk && <ForgeQuestion ask={forgeAsk} />}
            <SettingActions>
              <button type="submit" tabIndex={0} disabled={repo.trim() === "" || opening}>
                Open repo
              </button>
              <button type="button" tabIndex={0} onClick={onCancel}>
                Cancel
              </button>
            </SettingActions>
          </form>

          {/* The two-directory form (ADR 0035), for a project of its own somewhere the operator
              chooses. Closed until asked for. */}
          <details className="advanced">
            <summary tabIndex={0}>Advanced</summary>
            <p className="came-back">
              A project of its own, in a folder you choose, holding workspaces, personas and the
              repos work happens in.
            </p>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                // The box and the adopt field are two answers to one question — which
                // repository this plane starts from — so a ticked box sends no repo, rather
                // than sending both and letting the core rank them.
                if (ready)
                  onCreate(path.trim(), planeIsThisRepo, planeIsThisRepo ? "" : adopt.trim());
              }}
            >
              <SettingRow
                label="Folder"
                help="It does not have to exist yet. charter makes it, and writes the plane into it."
                control={(ids) => (
                  <>
                    <Field
                      kind="text"
                      ids={ids}
                      value={path}
                      placeholder="/where/it/goes"
                      onChange={setPath}
                    />
                    {/* `tabIndex={0}`, per `docs/ui-primitives.md` (charter-app#186): WebKit
                      leaves a `<button>` out of the tab sequence unless its `tabindex` is
                      written down, and the folder box beside this one is the scope's first
                      edge, so nothing reached this at all.

                      **The name says which box it fills**, because there are two of them. Two
                      buttons reading `Browse…` announce identically and pick different
                      directories, which is a question a screen reader cannot answer and a
                      sighted operator answers only from where the button sits. */}
                    <button
                      type="button"
                      tabIndex={0}
                      aria-label="Browse for the folder"
                      onClick={() => pick(setPath)}
                    >
                      Browse…
                    </button>
                  </>
                )}
              />

              {/* ADR 0035's default, as a second directory rather than a refusal about one. */}
              <SettingRow
                label="Repository to adopt"
                help={
                  <>
                    Optional: the plane goes in the folder above and this repo becomes its first
                    clone, in <code>workspaces/</code>. Nothing is written into the repo — it is
                    read, and only read. Leave it empty for a plane with no clones yet.
                  </>
                }
                control={(ids) => (
                  <>
                    <Field
                      kind="text"
                      ids={ids}
                      value={adopt}
                      placeholder="/where/the/repo/is"
                      disabled={planeIsThisRepo}
                      onChange={setAdopt}
                    />
                    {/* `tabIndex={0}` here too: this button was written after the sweep that
                      put the attribute on every other one (charter-app#186), which is exactly
                      how a fixed class of defect comes back. */}
                    <button
                      type="button"
                      tabIndex={0}
                      aria-label="Browse for the repository to adopt"
                      disabled={planeIsThisRepo}
                      onClick={() => pick(setAdopt)}
                    >
                      Browse…
                    </button>
                  </>
                )}
              />

              {/* The one decision, and it is the operator's: the set's toggle, a Radix
                checkbox in the tab sequence (`docs/ui-primitives.md`, charter-app#186), because
                it is the one answer on this dialog that writes into a repository the operator
                already has. */}
              <SettingRow
                label="Make this repo itself the plane"
                help={
                  <>
                    Only for a folder that is the top of a git repository, and only when you mean
                    it: it writes <code>charter.toml</code>, <code>personas/</code>,{" "}
                    <code>workspaces/</code> and charter&rsquo;s rules into that repository&rsquo;s
                    tracked <code>.gitignore</code>. charter&rsquo;s own plane is one of these. Left
                    unticked, charter writes nothing into a repository and says how to make a plane
                    beside it.
                  </>
                }
                control={(ids) => (
                  <Choice
                    kind="toggle"
                    ids={ids}
                    checked={planeIsThisRepo}
                    onCheckedChange={setPlaneIsThisRepo}
                  />
                )}
              />

              {/* `init`'s refusal in a repository is four lines — what it will not do, the
                commands that make a plane beside the repo, what asking for the old shape would
                write, and where the decision is recorded — and it is drawn in full above, where
                every refusal this dialog is given goes. */}

              {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). Only the
                folder box was in WebKit's tab sequence here: it is this scope's first edge and
                `Cancel` is its last, so `Browse…`, the checkbox and `Create project` were all
                in the middle, where neither the engine nor Radix reaches. */}
              <SettingActions>
                <button type="submit" tabIndex={0} disabled={!ready}>
                  Create project
                </button>
              </SettingActions>
            </form>
          </details>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
