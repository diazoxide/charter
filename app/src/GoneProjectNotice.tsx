import { useState } from "react";
import { commands, type GoneProject } from "./bindings";
import { Notice } from "./Notice";

/**
 * **A remembered project that is gone, with Locate… and Forget** (NO-5, #1237): drawn by the
 * window for a project the last quit had open, and by the opener for a recent.
 *
 * - **Forget** is the core's (ST-2): the project's recent, approval, pins and tabs go, so it no
 *   longer comes back at every launch.
 * - **Locate…** asks for a folder, and the core re-points the entry once it has checked a
 *   project is there (`locate_project`). What it found is handed to `onLocated`, which opens it
 *   through the trust gate like any open: the approval does not travel with the path.
 *
 * Neither is done without a press, because a disk that is unplugged may come back. A refusal
 * is said inside the Notice, which stays, so the operator can pick again.
 */
export function GoneProjectNotice({
  gone,
  cause,
  onLocated,
  onForgotten,
  onDismiss,
}: {
  gone: GoneProject;
  /** What the line is about, stably (`project-gone:<path>`). */
  cause: string;
  /** The core re-pointed the entry at this project. */
  onLocated: (found: string) => void;
  /** The core forgot the entry. */
  onForgotten: () => void;
  onDismiss: () => void;
}) {
  const [refused, setRefused] = useState<string>();

  const locate = async () => {
    const picked = await commands.pickProject().catch(() => null);
    // A cancelled dialog is null and is not a failure: nothing is said and nothing moves.
    if (picked === null || picked.status !== "ok" || !picked.data) return;
    const located = await commands
      .locateProject(gone.path, picked.data)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (located.status === "ok") onLocated(located.data);
    else setRefused(located.error);
  };

  const forget = async () => {
    const forgot = await commands
      .forgetProject(gone.path)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (forgot.status === "ok") onForgotten();
    else setRefused(forgot.error);
  };

  return (
    <Notice
      cause={cause}
      tone={refused === undefined ? "news" : "trouble"}
      fixes={[
        { label: "Locate…", onPress: () => void locate() },
        { label: "Forget", onPress: () => void forget() },
      ]}
      onDismiss={onDismiss}
    >
      {gone.said}
      {refused !== undefined && (
        <>
          {" "}
          <span>{refused}</span>
        </>
      )}
    </Notice>
  );
}
