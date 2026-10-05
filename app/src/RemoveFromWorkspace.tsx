import { useRef, useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { commands, type PlaneId } from "./bindings";

/**
 * **Take a repo the workspace names, and this machine has not cloned, out of the workspace**
 * (#1228): a yes before anything is written. Asked from the explorer's "Not cloned here" row,
 * its menu, the bottom bar's menu, the palette, and Settings › Workspace › Repos — one
 * question, so each says the same thing.
 *
 * Only the repo's row in `workspace.json` goes (`drop_repo_membership`). Nothing is deleted,
 * because nothing is here to delete, and the question says so. It is still a question: the
 * workspace stops naming the repo for everyone who shares it, and there is no Undo. A cloned
 * repo is never asked about here — its removal deletes the clone and goes through
 * `drop_repo`'s guard — and the core refuses one that got cloned in between, in its own words.
 */
export function RemoveFromWorkspace({
  plane,
  workspace,
  repo,
  onClose,
  onDone,
}: {
  plane: PlaneId;
  workspace: string;
  repo: string;
  onClose: () => void;
  /** It is out of the workspace: what the core said about it. */
  onDone: (said: string[]) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [trouble, setTrouble] = useState<string | null>(null);
  const cancel = useRef<HTMLButtonElement>(null);

  const confirm = async () => {
    setBusy(true);
    setTrouble(null);
    try {
      const got = await commands.dropRepoMembership(plane, workspace, repo);
      if (got.status === "ok") onDone(got.data);
      else setTrouble(got.error);
    } catch (err: unknown) {
      setTrouble(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <AlertDialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="asking" />
        <AlertDialog.Content
          className="warning"
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            cancel.current?.focus();
          }}
        >
          <AlertDialog.Title>{`Remove ${repo} from ${workspace}?`}</AlertDialog.Title>
          <AlertDialog.Description className="came-back">
            {`${workspace} stops naming ${repo} in its workspace.json. Nothing is deleted: it is not cloned on this machine.`}
          </AlertDialog.Description>
          {trouble !== null && (
            <p className="trouble" role="alert">
              {trouble}
            </p>
          )}
          <div className="doing">
            <button
              type="button"
              className="ends-it"
              tabIndex={0}
              disabled={busy}
              onClick={() => void confirm()}
            >
              Remove from workspace
            </button>
            <AlertDialog.Cancel asChild>
              <button type="button" tabIndex={0} ref={cancel} onClick={onClose}>
                Cancel
              </button>
            </AlertDialog.Cancel>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
