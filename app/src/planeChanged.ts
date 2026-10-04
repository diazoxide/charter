import { useEffect, useRef, useState } from "react";
import { listen } from "./here";

import type { PlaneAnswer, PlaneChanged, PlaneId } from "./bindings";

/**
 * The answer a reader of the plane holds, by its name (FD-10).
 *
 * **Which answer a change concerns is the core's question, never the window's**
 * (`charter_core::planechange::answers`): the core reads the plane format's paths, so it is
 * the one place that knows a todo is part of the sidebar and a memory is not. `plane-changed`
 * carries the answers a batch concerns, and a reader here only names the one it holds.
 */
export type Interest = PlaneAnswer;

/** The sidebar (`plane_sidebar`). */
export const SIDEBAR: Interest = { answer: "sidebar" };

/** The instructions a chat read at its start, which mark a chat running on old ones
 *  (`chats_plane_updated`). */
export const INSTRUCTIONS: Interest = { answer: "instructions" };

/** The curation actions offered on each subject (`curation_offers`). */
export const CURATIONS: Interest = { answer: "curations" };

/** What a project has on, and the theme it draws. */
export const SETTINGS: Interest = { answer: "settings" };

/** The git standings: the alerts and the Saving rows. What auto-save did reaches these and
 *  no other reader (#933). */
export const GIT: Interest = { answer: "git" };

/** The plane root's panels (`plane_root_panels`). */
export const ROOT_PANELS: Interest = { answer: "rootPanels" };

/** The view tabs, which read any of the plane's stores. */
export const VIEWS: Interest = { answer: "views" };

/** One workspace's panels (`workspace_panels`). */
export function panelsOf(workspace: string): Interest {
  return { answer: "panels", workspace };
}

/**
 * Whether the answers the core said moved include the one a reader holds.
 *
 * **Not knowing concerns everyone.** `null` — or an event from a core too old to say — is the
 * core saying it cannot name what moved (a batch it could not place), and every reader reads
 * again, as each did before there were kinds. The core's `panels` with no workspace is every
 * workspace's: a persona or the project moved.
 */
export function concerns(
  answers: readonly PlaneAnswer[] | null | undefined,
  mine: Interest,
): boolean {
  if (answers === null || answers === undefined) return true;
  return answers.some((told) => {
    if (told.answer !== mine.answer) return false;
    if (told.answer !== "panels" || mine.answer !== "panels") return true;
    return told.workspace === null || told.workspace === mine.workspace;
  });
}

/**
 * How many times the core has said one of `planes` changed on disk (charter-app#264).
 *
 * **A count, so it goes in a dependency array.** Every read of the plane that must follow the
 * disk — the workspace's panels, the sidebar, the alerts — lists it, and runs again exactly
 * when it moves. The core debounces (`app/src-tauri/src/planewatch.rs`), so one `git pull` is
 * one bump rather than ten.
 *
 * **Filtered on the plane, as `chat-moved` is.** The event is emitted on the app and a
 * process holds several projects, so a todo closed in one must not make another read itself
 * again.
 *
 * Listening starts at the mount, before any answer it would invalidate is asked for — the
 * reason `chatState.ts` gives for the same order. A window that cannot listen (a unit test,
 * a webview being torn down) is simply never bumped, and reads the plane when it is focused,
 * as it did before there was anything to listen to.
 *
 * **It moves only for a change that concerns the answer it names** ([`concerns`], FD-10), so
 * a panel reads again for what its answer is made of instead of for every write anywhere in
 * the plane — and which changes those are is the core's to say.
 */
export function usePlaneChanged(planes: readonly PlaneId[], interest: Interest): number {
  const [count, setCount] = useState(0);
  // The list by value: a fresh array from the caller each render must not re-register.
  const holding = planes.join("\n");
  // The interest is read when an event arrives, not listened under: focusing another
  // workspace changes it, and re-registering the listener then would be IPC per focus and a
  // gap in which a change could go unheard. Focusing reads the workspace anyway.
  const wanted = useRef(interest);
  useEffect(() => {
    wanted.current = interest;
  });

  useEffect(() => {
    const mine = new Set(holding.split("\n"));
    let gone = false;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unlisten = await listen<PlaneChanged>("plane-changed", (event) => {
          if (gone || !mine.has(event.payload.plane)) return;
          if (concerns(event.payload.answers, wanted.current)) setCount((was) => was + 1);
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in. See the docstring.
      }
    })();
    return () => {
      gone = true;
      stop?.();
    };
  }, [holding]);

  return count;
}
