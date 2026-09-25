import { useId, useRef, useState } from "react";
import * as Checkbox from "@radix-ui/react-checkbox";
import * as Dialog from "@radix-ui/react-dialog";
import type { PlaneId } from "./bindings";
import { RepoPicker } from "./RepoPicker";

/**
 * Making a workspace, asked where the answer is given.
 *
 * **The prompt IS the prompt** (ADR 0035, and `ApprovePlane` says the same thing about
 * the trust ask). The CLI's shape here is `charter workspace create <name> --vision "…"`; the
 * window has a person looking at it, so it asks for the two things that command takes and
 * nothing is copied from the printed-command shape.
 *
 * **This dialog validates nothing.** What a workspace may be called is
 * `contain::workspace_name_ok`, reached through `workspace_create` → `wscmd::create` →
 * `wscmd::ensure`, which is the same path `charter workspace create` takes — so the app refuses
 * exactly the names a terminal refuses and says the same sentence about them. A check written
 * here would be a second answer to "what may a workspace be called", and the two would drift
 * the first time either moved. The only thing the button asks of the box is that it has
 * something in it, which is not a rule about names: it is the difference between a question
 * that has been answered and one that has not.
 *
 * A vision is optional and says so. It is the one field that is easier to fill now than later —
 * `workspace.md` is the living charter a fork inherits, and charter nags about an empty one on
 * every command — but a workspace with no vision is a workspace, and the core's own line
 * explains how to add one afterwards.
 *
 * **Its repos are picked from what the operator's own forge login reaches** (ADR 0055), and
 * cloned after the workspace is made — the dialog closes at once and each repo lands on its
 * own. Picking none is a workspace with no repos, which is still a workspace.
 */
export function NewWorkspace({
  /** What the plane is called, so the dialog says where the workspace is going. */
  plane,
  /** Which plane, for the repo picker to ask about. */
  planeId,
  /** Why the last attempt made nothing — **the core's sentence, unchanged**. */
  trouble,
  /** Whether charter is making it right now, so the answer cannot be given twice. */
  making,
  onCreate,
  onCancel,
}: {
  plane: string;
  planeId: PlaneId;
  trouble?: string;
  making: boolean;
  /** `live`: born LIVE, its charter, memory and todos published with the plane (charter-app#301). */
  onCreate: (name: string, vision: string, live: boolean, repos: string[]) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [vision, setVision] = useState("");
  const [live, setLive] = useState(false);
  const [repos, setRepos] = useState<ReadonlySet<string>>(new Set());
  const liveId = useId();
  const nameId = useId();
  const visionId = useId();
  // The name box, focused by the dialog itself: it is the one thing that has to be answered,
  // and a dialog that opens with the keyboard somewhere else is a dialog you have to click at.
  const box = useRef<HTMLInputElement>(null);
  const ready = name.trim() !== "" && !making;
  const create = () => {
    if (ready) onCreate(name.trim(), vision, live, [...repos].sort());
  };
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
          aria-labelledby="new-workspace"
          // A click outside answers nothing, which is what every dialog in this window does
          // (`docs/ui-primitives.md`). Escape is Cancel, and nothing is made.
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            box.current?.focus();
          }}
        >
          <Dialog.Title id="new-workspace">New workspace</Dialog.Title>
          <p className="where">
            in <code>{plane}</code>
          </p>

          <form
            className="asks"
            onSubmit={(event) => {
              event.preventDefault();
              create();
            }}
          >
            <label htmlFor={nameId}>Name</label>
            <input
              id={nameId}
              ref={box}
              value={name}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setName(event.target.value)}
            />
            {/* charter's own alphabet, said once and not enforced here. It is guidance for
                somebody typing, and the core is what refuses. */}
            <p className="came-back">
              Letters, digits, <code>.</code>, <code>_</code> and <code>-</code>. It becomes a
              directory under <code>workspaces/</code>.
            </p>

            <label htmlFor={visionId}>What it is for (optional)</label>
            <textarea
              id={visionId}
              rows={3}
              value={vision}
              onChange={(event) => setVision(event.target.value)}
            />
            <p className="came-back">
              Recorded in <code>workspace.md</code>, the living charter a fork inherits. You can add
              it later with <code>charter workspace vision</code>.
            </p>

            <RepoPicker plane={planeId} picked={repos} onPicked={setRepos} />
            <p className="came-back">
              Optional. Each one is cloned into the workspace after it is made, and you can add or
              remove repos later in its settings.
            </p>

            {/* LOCAL unless ticked: publishing is the operator's choice, never a default. */}
            <div className="choice">
              <Checkbox.Root
                id={liveId}
                className="box"
                checked={live}
                onCheckedChange={(next) => setLive(next === true)}
                tabIndex={0}
              >
                <Checkbox.Indicator className="box-mark">✓</Checkbox.Indicator>
              </Checkbox.Root>
              <label className="who" htmlFor={liveId}>
                Live
              </label>
            </div>
            <p className="came-back">
              A live workspace&apos;s charter, memory and todos are committed with the plane and
              published by every save — ticked, the plane is saved as soon as it is made, the way
              the Saving tab says this plane saves (and not at all while it has not been told). Left
              unticked, they stay on this machine.
            </p>

            {/* Verbatim, and in the dialog rather than behind it: the operator is still
                answering, and a refusal they cannot see beside the box is one they cannot act
                on. */}
            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}

            {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). The name
                box is this scope's first edge and `Cancel` is its last, so `Create workspace`
                sat between them — where Radix's focus scope does nothing and WebKit will not
                tab to a `<button>` whose `tabindex` is not written down. The two text boxes
                were reachable and the one that acts on them was not. */}
            <div className="doing">
              <button type="submit" tabIndex={0} disabled={!ready}>
                Create workspace
              </button>
              <button type="button" tabIndex={0} onClick={onCancel}>
                Cancel
              </button>
            </div>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
