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
 * **What this dialog does NOT do is adopt the repo as the plane's first clone.** That is the
 * other half of ADR 0035's default and it needs `charter clone`, which applies charter's git
 * policy — a security-critical part that is not ported (spec decision 16), so `init
 * --clone-this-repo` refuses rather than clones in this charter too. The refusal the operator
 * reads is the core's, it names the three commands that finish the job, and the gap is filed
 * rather than half-implemented: a clone made without the policy is worse than one not made.
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
  onCreate: (path: string, planeIsThisRepo: boolean) => void;
  onCancel: () => void;
}) {
  const [path, setPath] = useState("");
  const [planeIsThisRepo, setPlaneIsThisRepo] = useState(false);
  const pathId = useId();
  const repoId = useId();
  const box = useRef<HTMLInputElement>(null);
  const ready = path.trim() !== "" && !making;

  const pick = useCallback(() => {
    void commands
      .pickProject()
      .then((answer) => {
        // A cancelled dialog is null and is not a failure: nothing is said and nothing moves.
        if (answer.status === "ok" && answer.data) setPath(answer.data);
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
              if (ready) onCreate(path.trim(), planeIsThisRepo);
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
              <button type="button" onClick={pick}>
                Browse…
              </button>
            </div>
            <p className="came-back">
              It does not have to exist yet. charter makes it, and writes the plane into it.
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
