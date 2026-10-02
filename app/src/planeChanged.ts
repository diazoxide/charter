import { useEffect, useRef, useState } from "react";
import { listen } from "./here";

import type { ChangeKind, PlaneChange, PlaneChanged, PlaneId } from "./bindings";

/**
 * One kind of change a reader's answer is made of (FD-10). A field left out matches anything;
 * `workspace: null` matches only a change in no workspace — the plane root's session records,
 * a persona's memory.
 */
export type Wanted = {
  readonly kind?: ChangeKind;
  readonly workspace?: string | null;
};

/** What a reader reads again for: any one of these. */
export type Interest = readonly Wanted[];

/**
 * The sidebar: every workspace with its todos and colour, and the personas
 * (`plane_sidebar`). Not memory, session records or the harness settings, which it never
 * reads.
 */
export const SIDEBAR: Interest = [
  { kind: "todos" },
  { kind: "workspace" },
  { kind: "persona" },
  { kind: "project" },
];

/**
 * Everything but the memory stores and the session records: the shape of the plane — its
 * settings, workspaces, todos, personas and harness files. What the readers that are not
 * panels follow (the instructions a chat started on, the curations, the git standings).
 *
 * **Why the git readers leave memory and records out** (FD-10): an agent saving a memory is
 * the commonest write there is, and asking git for every standing on each one is what the
 * switch-speed work (FR-27) took out. The standings still follow it — they poll on a timer and
 * after every save — and `null` (auto-save committed, or a batch the core could not place)
 * still reaches them at once.
 */
export const PLANE_SHAPE: Interest = [
  { kind: "project" },
  { kind: "harness" },
  { kind: "workspace" },
  { kind: "todos" },
  { kind: "persona" },
];

/** What a project has on, and the theme it draws: `charter.toml`, `charter.local.toml` and
 *  each `workspace.json` (charter-app#253, #273, #281). */
export const SETTINGS: Interest = [{ kind: "project" }, { kind: "workspace" }];

/** The plane root's panels: its session records, and nothing else (`plane_root_panels`). */
export const ROOT_PANELS: Interest = [{ kind: "sessions", workspace: null }];

/**
 * One workspace's panels (`workspace_panels`): everything in that workspace, and the
 * personas with their memory counts and the default persona, which the Personas panel draws.
 * Never another workspace's todos.
 */
export function workspaceInterest(workspace: string): Interest {
  return [
    { workspace },
    { kind: "persona" },
    { kind: "memory", workspace: null },
    { kind: "project" },
  ];
}

/**
 * Whether `changes` concern a reader whose answer is made of `interest`.
 *
 * **Not knowing concerns everyone.** `null` — or an event from a core too old to say — is the
 * core saying it cannot name what moved (auto-save committed, a batch it could not place), and
 * every reader reads again, as each did before there were kinds. A reader with no interest
 * named reads again on every change.
 */
export function concerns(
  changes: readonly PlaneChange[] | null | undefined,
  interest: Interest | undefined,
): boolean {
  if (changes === null || changes === undefined) return true;
  if (interest === undefined) return changes.length > 0;
  return changes.some((change) =>
    interest.some(
      (wanted) =>
        (wanted.kind === undefined || wanted.kind === change.kind) &&
        (wanted.workspace === undefined || wanted.workspace === change.workspace),
    ),
  );
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
 * **Given an `interest`, it moves only for a change that concerns it** ([`concerns`], FD-10),
 * so a panel subscribes to the kinds its answer is made of instead of reading again for every
 * write anywhere in the plane.
 */
export function usePlaneChanged(planes: readonly PlaneId[], interest?: Interest): number {
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
          if (concerns(event.payload.changes, wanted.current)) setCount((was) => was + 1);
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
