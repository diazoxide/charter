import { useEffect, useState } from "react";
import { commands, type PlaneId, type PlaneRootPanels, type SessionRecordRow } from "./bindings";
import type { State } from "./chatState";
import type { ViewRef } from "./tabs";

/**
 * **Session records in the window** (SI-8d, ADR 0064): the operator's ruling that *the user can
 * always get old sessions back*.
 *
 * A record is opened as a view tab — `{ from: null, view: "session", key: <its plane-relative
 * path> }` — which draws it as read-only Markdown (`SessionRecordTab.tsx`), and resumed as a NEW
 * chat (`resume_session`). The Sessions panel, the palette and a row's menu reach both through
 * the catalogue's `session.open:<path>` and `session.resume:<path>` rows.
 */

/** The view a session record is opened as. The key is its plane-relative path, which is what
 *  the core reads it by (`sessionrecord::locate`) and what makes one tab per record. */
export function sessionView(path: string): ViewRef {
  return { from: null, view: SESSION_VIEW, key: path };
}

/** The view's id among charter's own. */
export const SESSION_VIEW = "session";

/** What a session record's tab is called. */
export function sessionTitle(title: string): string {
  return `Session · ${title}`;
}

/**
 * A chat a Resume started, as the window watches it: the record it resumes, whether it was
 * already the fallback, and whether its harness has reported anything yet.
 */
export type Resuming = {
  /** The record's plane-relative path. */
  path: string;
  /** Whether this chat is the fresh one a failed resume fell back to. It falls back no further. */
  afterFailure: boolean;
  /** Whether a state other than `unknown` has been seen for it: its harness reported. */
  heard: boolean;
};

/**
 * Whether a resumed chat's harness could not bring the conversation back, so the same record is
 * started again as a fresh chat.
 *
 * **What says so is the program, not its output** (spec decision 3): the chat's program ended
 * with a failure before its harness reported anything at all. A harness that cannot find the
 * conversation it was given ends like that — measured on opencode 1.18 (`Session not found`,
 * exit 1); Claude Code and Codex refuse an unknown id the same way. A chat whose harness had
 * reported, and a chat that ended cleanly, is the operator's to have ended, and nothing is
 * started in its place.
 */
export function lostOnResume(resuming: Resuming | undefined, state: State): boolean {
  return resuming !== undefined && !resuming.afterFailure && !resuming.heard && state === "failed";
}

/** `resuming` once `state` has been seen for its chat. */
export function heardFrom(resuming: Resuming, state: State): Resuming {
  return resuming.heard || state === "unknown" || state === "failed"
    ? resuming
    : { ...resuming, heard: true };
}

/**
 * The plane root's panels — its session records (SI-1, SI-8d) — asked while the plane root is
 * focused, and again whenever the plane changes on disk. A refusal is no panels rather than a
 * region that fails: the plane root has nothing else to draw there.
 */
export function usePlaneRootPanels(
  plane: PlaneId,
  atRoot: boolean,
  changed = 0,
): PlaneRootPanels | undefined {
  const [panels, setPanels] = useState<PlaneRootPanels>();
  useEffect(() => {
    if (!atRoot) return;
    let gone = false;
    void commands
      .planeRootPanels(plane)
      .then((said) => {
        if (!gone && said.status !== "error") setPanels(said.data ?? undefined);
      })
      .catch(() => {
        // Nothing: the region says what the plane root is, and draws no Sessions panel.
      });
    return () => {
      gone = true;
    };
  }, [plane, atRoot, changed]);
  return atRoot ? panels : undefined;
}

/** The records the palette offers rows for: the focused workspace's, or the plane root's. */
export type SessionRows = readonly SessionRecordRow[];
