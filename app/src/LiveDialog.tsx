import { useEffect, useRef, useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { Radio } from "lucide-react";
import { commands, type LivePreview, type PlaneId } from "./bindings";
import { tellSaved } from "./saving";

/**
 * **Make a workspace LIVE or LOCAL** (charter-app#301, ADR 0051): what it publishes, where it
 * goes, and a yes before anything happens. Opened from the workspace's menu, the palette, and
 * its settings page — one dialog, so the three say the same thing.
 *
 * LIVE publishes the workspace's charter, memory and todos with the plane, and the plane is
 * saved at once: going LIVE is an explicit intent to publish. LOCAL stops publishing them (they
 * stay on disk) and saves the untracking; what was pushed before stays in history, and the
 * dialog says so rather than let "private" suggest otherwise.
 *
 * Whether the remote is public is not something charter can see, so the dialog names the
 * remote and says who reads it: whoever can read that repository.
 */
export function LiveDialog({
  plane,
  workspace,
  onClose,
  onDone,
}: {
  plane: PlaneId;
  workspace: string;
  onClose: () => void;
  onDone: (said: string[]) => void;
}) {
  const [read, setRead] = useState<LivePreview | null>(null);
  const [trouble, setTrouble] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const cancel = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    let gone = false;
    void commands
      .workspaceLivePreview(plane, workspace)
      .then((got) => {
        if (gone) return;
        if (got.status === "ok") setRead(got.data);
        else setTrouble(got.error);
      })
      .catch((err: unknown) => {
        if (!gone) setTrouble(String(err));
      });
    return () => {
      gone = true;
    };
  }, [plane, workspace]);

  const going = read === null ? undefined : !read.live;
  const word = going === false ? "local" : "live";

  const confirm = async () => {
    if (going === undefined) return;
    setBusy(true);
    setTrouble(null);
    try {
      const got = await commands.workspaceLive(plane, workspace, going);
      if (got.status === "ok") {
        tellSaved();
        onDone(got.data);
      } else {
        setTrouble(got.error);
      }
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
          <AlertDialog.Title>{`Make ${workspace} ${word}?`}</AlertDialog.Title>
          <AlertDialog.Description className="came-back">
            {going === false
              ? "Its charter, memory and todos stay on this machine and stop being committed."
              : "Its charter, memory and todos are committed with the plane, and every save publishes them."}
          </AlertDialog.Description>
          {read === null && trouble === null && <p className="pending">Reading the workspace…</p>}
          {read !== null && (
            <>
              {read.files.length > 0 && (
                <ul className="at-risk" aria-label="What it publishes">
                  {read.files.map((file) => (
                    <li key={file}>{file}</li>
                  ))}
                </ul>
              )}
              <p className="came-back">{whereText(read)}</p>
            </>
          )}
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
              disabled={busy || read === null}
              onClick={() => void confirm()}
            >
              {`Make ${word}`}
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

/** Where the files go, or that they stop going: said from the plane's mode and remote. */
function whereText(read: LivePreview): string {
  if (read.live) {
    return "It stops publishing them from now on. What was already pushed stays in the repository's history.";
  }
  if (read.mode === "off") {
    return "This plane's mode is off, so charter commits nothing; they are published when you commit and push them.";
  }
  if (read.mode === "commit") {
    return "This plane's mode is commit, so they are committed on this machine and published when you push.";
  }
  if (read.remote === null) {
    return "This plane has no remote charter can push to, so they are committed on this machine only.";
  }
  return `The next save pushes them to ${read.remote} — anyone who can read that repository will read them.`;
}

/**
 * **The LIVE mark** (charter-app#301): on a workspace's tab, the breadcrumb, the Explorer's row
 * for it and the Saving tab. Named for a screen reader, and titled for a pointer, because a
 * glyph alone says nothing about what LIVE means.
 */
export function LiveMark() {
  return (
    <span className="live-mark" role="img" aria-label="live" title="Live: published with the plane">
      <Radio aria-hidden="true" />
    </span>
  );
}
