import { useCallback, useId, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import * as Checkbox from "@radix-ui/react-checkbox";
import { commands } from "./bindings";

/**
 * Making a new project — a plane charter scaffolds — and the one decision it asks about.
 *
 * **The prompt IS the prompt** (charter ADR 0035). The CLI's shape is `mkdir`, `cd`, `charter
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
 */
export function NewProject({
  /** Why the last attempt made nothing — **the core's lines, unchanged and all of them**. */
  trouble,
  /** Whether charter is making it right now, so the answer cannot be given twice. */
  making,
  onCreate,
  onCancel,
}: {
  trouble?: string;
  making: boolean;
  onCreate: (path: string, planeIsThisRepo: boolean, adopt: string) => void;
  onCancel: () => void;
}) {
  const [path, setPath] = useState("");
  const [adopt, setAdopt] = useState("");
  const [planeIsThisRepo, setPlaneIsThisRepo] = useState(false);
  const pathId = useId();
  const adoptId = useId();
  const repoId = useId();
  const box = useRef<HTMLInputElement>(null);
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
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            box.current?.focus();
          }}
        >
          <Dialog.Title id="new-project">New project</Dialog.Title>
          <p className="came-back">
            A project is a control plane: a directory of its own, holding workspaces, personas and
            the clones work happens in.
          </p>

          <form
            className="asks"
            onSubmit={(event) => {
              event.preventDefault();
              // The box and the adopt field are two answers to one question — which
              // repository this plane starts from — so a ticked box sends no repo, rather
              // than sending both and letting the core rank them.
              if (ready)
                onCreate(path.trim(), planeIsThisRepo, planeIsThisRepo ? "" : adopt.trim());
            }}
          >
            <label htmlFor={pathId}>Folder</label>
            <div className="picking">
              <input
                id={pathId}
                ref={box}
                value={path}
                autoComplete="off"
                spellCheck={false}
                placeholder="/where/it/goes"
                onChange={(event) => setPath(event.target.value)}
              />
              <button type="button" onClick={() => pick(setPath)}>
                Browse…
              </button>
            </div>
            <p className="came-back">
              It does not have to exist yet. charter makes it, and writes the plane into it.
            </p>

            {/* ADR 0035's default, as a second directory rather than a refusal about one. */}
            <label htmlFor={adoptId}>Repository to adopt</label>
            <div className="picking">
              <input
                id={adoptId}
                value={adopt}
                autoComplete="off"
                spellCheck={false}
                disabled={planeIsThisRepo}
                placeholder="/where/the/repo/is (optional)"
                onChange={(event) => setAdopt(event.target.value)}
              />
              <button type="button" disabled={planeIsThisRepo} onClick={() => pick(setAdopt)}>
                Browse…
              </button>
            </div>
            <p className="came-back">
              Optional, and the way ADR 0035 means a project to start: the plane goes in the folder
              above and this repository becomes its first clone, in <code>workspaces/</code>.
              Nothing is written into the repository — it is read, and only read. Leave it empty for
              a plane with no clones yet.
            </p>

            {/* The one decision, and it is the operator's. Radix's checkbox, per
                `docs/ui-primitives.md`; the label is `htmlFor` the control, which is native
                HTML doing what it already does. */}
            <div className="choice">
              <Checkbox.Root
                id={repoId}
                className="box"
                checked={planeIsThisRepo}
                onCheckedChange={(next) => setPlaneIsThisRepo(next === true)}
              >
                <Checkbox.Indicator className="box-mark">✓</Checkbox.Indicator>
              </Checkbox.Root>
              <label className="who" htmlFor={repoId}>
                Make this repo itself the plane
              </label>
            </div>
            <p className="came-back">
              Only for a folder that is the top of a git repository, and only when you mean it: it
              writes <code>charter.toml</code>, <code>personas/</code>, <code>workspaces/</code> and
              charter&rsquo;s rules into that repository&rsquo;s tracked <code>.gitignore</code>.
              charter&rsquo;s own plane is one of these. Left unticked, charter writes nothing into
              a repository and says how to make a plane beside it.
            </p>

            {/* Verbatim, and all of it. `init`'s refusal in a repository is four lines — what
                it will not do, the commands that make a plane beside the repo, what asking for
                the old shape would write, and where the decision is recorded — and an operator
                shown a summary of that can follow none of it. */}
            {trouble && (
              <p className="trouble said-in-full" role="alert">
                {trouble}
              </p>
            )}

            <div className="doing">
              <button type="submit" disabled={!ready}>
                Create project
              </button>
              <button type="button" onClick={onCancel}>
                Cancel
              </button>
            </div>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
