import { useCallback, useSyncExternalStore } from "react";
import { commands } from "./bindings";
import { familyOf } from "./Notice";
import { atCreation, sayAboutThisMachine, type Reading } from "./windowprefs";

/**
 * **A Dismiss lasts until the Notice's cause changes** (ruling V91j, NO-2 #1229).
 *
 * A Notice names what it is about by its **cause** (`Notice.tsx`): `pin-dormant:delta`,
 * `chat-fresh:3`. Dismissing it keeps the cause here, per project, and the Notice is not drawn
 * while it is kept.
 *
 * **Kept on this machine, in the layout file** (`charter/layout.json`, `regions.ts`), beside the
 * text sizes and your editor: what the operator has already seen is about how they use their
 * window on this machine, the same tier, so it adds no store. Never in the project: a dismissal
 * is one operator's, and a project carries what it holds to every clone. It is read before the
 * window exists, so a dismissed Notice is never drawn for a frame at a relaunch and taken away.
 *
 * **Written one project at a time, by the core** (`set_dismissed`, D-NO2-1 as amended): the
 * window sends its project's list and the core replaces only that key, under the file's lock.
 * The window's layout writes never carry them, and the core keeps the file's own over anything
 * one sends — two windows each hold only what they read at launch, and the whole map written
 * back by one would take away what the other dismissed since.
 *
 * **It clears itself once the cause is gone, so a return shows again.** "Gone" is the core's
 * answer and never the window's emptiness: a source that reads a family of causes from the core
 * calls {@link settleNotices} with every cause of that family the answer holds, and only then is
 * a kept cause the answer lacks let go. A list that is empty because it has not been read yet,
 * is being read again, or could not be read settles nothing, so a panel re-reading never clears
 * a dismissal (no flicker clear).
 *
 * **Only a cause the core answers for is kept** ({@link KEPT}). A Notice about an event this run
 * saw — a session saved, a chat that did not start — is put away by its own Dismiss, which ends
 * that occurrence; the next one is a new occurrence and shows. Keeping such a cause would hide
 * every later one for good, because nothing would ever answer that it had gone.
 */

/**
 * **The families of cause a dismissal is kept for**, each read from the core by a source that
 * settles it: a dormant pin (`plane_pins`' `missing`), what a relaunch did to a chat
 * (`opened_chats`), a doctor finding that stands as a Notice (`plane_doctor`, #1250), and a
 * sandbox change that left chats behind (`chats_on_older_sandbox`, #1428). A family is the cause
 * up to its first `:`.
 */
export const KEPT: ReadonlySet<string> = new Set([
  "pin-dormant",
  "chat-resumed",
  "chat-guessed",
  "chat-fresh",
  "doctor-finding",
  "sandbox-changed",
]);

/** The most causes kept per project. A bound on what a hand-edited file can make the window
 *  hold; settling keeps the real number to what is standing now. */
export const MOST_PER_PROJECT = 200;

/**
 * **What the person has seen once on this machine** (#1501): a Notice shown once per machine,
 * whatever the project, kept under this key beside the projects' paths
 * (`purlis_core::windowprefs::ON_THIS_MACHINE`). A project is keyed by its absolute path, so
 * no project is ever this key.
 *
 * Its causes never change and nothing settles them: a Notice seen once stays seen. Only the
 * window writes them, on the person's press (`set_dismissed_on_this_machine`), and no chat
 * reaches a window's commands, so no chat sets or clears what the person has seen.
 */
export const ON_THIS_MACHINE = "on this machine";

/** The causes kept {@link ON_THIS_MACHINE}: each a Notice shown once per machine. */
export const ONCE_ON_THIS_MACHINE: ReadonlySet<string> = new Set(["chip-explained"]);

const kept = (cause: unknown): cause is string =>
  typeof cause === "string" && cause.length <= 1024 && KEPT.has(familyOf(cause));

const keptOnce = (cause: unknown): cause is string =>
  typeof cause === "string" && ONCE_ON_THIS_MACHINE.has(cause);

/** Every project's kept causes, as the layout file holds them. */
export type Dismissed = Record<string, string[]>;

/** The dismissals as a layout document holds them, read field by field, and what was put
 *  right. A cause of a family this build does not keep is dropped. */
export function loadDismissed(raw: unknown): { dismissed: Dismissed; said: string[] } {
  const held =
    raw !== null && typeof raw === "object" && !Array.isArray(raw)
      ? (raw as { dismissed?: unknown }).dismissed
      : undefined;
  if (held === undefined) return { dismissed: {}, said: [] };
  if (held === null || typeof held !== "object" || Array.isArray(held))
    return { dismissed: {}, said: ['"dismissed" is not a map of projects, so nothing is'] };
  const said: string[] = [];
  const dismissed: Dismissed = {};
  for (const [project, causes] of Object.entries(held as Record<string, unknown>)) {
    if (!Array.isArray(causes)) {
      said.push(`"dismissed" for ${project} is not a list, so it was left out`);
      continue;
    }
    const usable = [...new Set(causes.filter(project === ON_THIS_MACHINE ? keptOnce : kept))].slice(
      0,
      MOST_PER_PROJECT,
    );
    if (usable.length < causes.length) said.push(`some of ${project}'s dismissals were left out`);
    if (usable.length > 0) dismissed[project] = usable;
  }
  return { dismissed, said };
}

/** What this launch holds; `undefined` until it is first read from what the window was handed. */
let now: Map<string, ReadonlySet<string>> | undefined;
const listeners = new Set<() => void>();
const NONE: ReadonlySet<string> = new Set();

function held(layout: Reading = atCreation().layout): Map<string, ReadonlySet<string>> {
  if (now !== undefined) return now;
  // A layout file charter refused is said once, by `regions.ts`.
  const { dismissed, said } =
    layout.found && layout.trouble === null
      ? loadDismissed(layout.document)
      : { dismissed: {}, said: [] };
  if (said.length > 0) {
    const where = layout.path || "the layout file";
    sayAboutThisMachine("dismissed", {
      severity: "warn",
      detail: `${where}: ${said.join("; ")}`,
      remedy: "nothing to do: those Notices show again, and the next dismissal rewrites it",
    });
  }
  now = new Map(Object.entries(dismissed).map(([project, causes]) => [project, new Set(causes)]));
  return now;
}

/** Every write, in the order the window made it, so an older list never lands last. */
let writing: Promise<void> = Promise.resolve();

function change(project: string, next: ReadonlySet<string>) {
  const all = held();
  if (next.size === 0) all.delete(project);
  else all.set(project, next);
  for (const listener of listeners) listener();
  const causes = [...next];
  writing = writing.then(async () => {
    const kept = await (
      project === ON_THIS_MACHINE
        ? commands.setDismissedOnThisMachine(causes)
        : commands.setDismissed(project, causes)
    ).catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    sayAboutThisMachine(
      "dismissed",
      kept.status === "error"
        ? {
            severity: "warn",
            detail: `purlis could not keep what you dismissed: ${kept.error}`,
            remedy: "it stays hidden until you quit; after a relaunch those Notices show again",
          }
        : undefined,
    );
  });
}

/** The causes dismissed in `project`. The same object until they change. */
export function dismissedIn(project: string): ReadonlySet<string> {
  return held().get(project) ?? NONE;
}

/**
 * Dismisses `cause` in `project` until it changes. A cause of a family nobody settles is not
 * kept: its Notice's own Dismiss is what puts that occurrence away.
 */
export function dismissNotice(project: string, cause: string): void {
  const was = dismissedIn(project);
  if (!kept(cause) || was.has(cause) || was.size >= MOST_PER_PROJECT) return;
  change(project, new Set(was).add(cause));
}

/**
 * **The core answered for a family of causes**: `present` is every cause of `family` its answer
 * holds. A kept cause of that family the answer lacks has gone, so its dismissal is let go and
 * a return shows the Notice again. Called only with a core's answer, never with a list the
 * window has not read yet.
 */
export function settleNotices(project: string, family: string, present: readonly string[]): void {
  const was = dismissedIn(project);
  const standing = new Set(present);
  const next = [...was].filter((cause) => familyOf(cause) !== family || standing.has(cause));
  if (next.length !== was.size) change(project, new Set(next));
}

/**
 * **A new occurrence of `cause`**, answered by the core as it happened (a chat resumed from its
 * record this run): whatever was dismissed under it was about the old one.
 */
export function showAgain(project: string, cause: string): void {
  const was = dismissedIn(project);
  if (!was.has(cause)) return;
  const next = new Set(was);
  next.delete(cause);
  change(project, next);
}

/**
 * Keeps `cause` as seen {@link ON_THIS_MACHINE}: its Notice is not shown again in any project,
 * at any later launch. Only a cause of {@link ONCE_ON_THIS_MACHINE} is kept.
 */
export function seeOnThisMachine(cause: string): void {
  const was = dismissedIn(ON_THIS_MACHINE);
  if (!keptOnce(cause) || was.has(cause)) return;
  change(ON_THIS_MACHINE, new Set(was).add(cause));
}

/** Whether `cause` was seen on this machine, for a component that redraws when it is, with
 *  the way to say it now is. */
export function useSeenOnThisMachine(cause: string): { seen: boolean; see: () => void } {
  const seen = useSyncExternalStore(onDismissals, () => dismissedIn(ON_THIS_MACHINE).has(cause));
  const see = useCallback(() => seeOnThisMachine(cause), [cause]);
  return { seen, see };
}

/** Calls `listener` whenever a dismissal is kept or let go. Answers the way to stop. */
function onDismissals(listener: () => void): () => void {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

/** {@link dismissedIn}, for a component that redraws when it changes, with the ways to change
 *  it already bound to `project`. */
export function useDismissals(project: string): {
  dismissed: ReadonlySet<string>;
  dismiss: (cause: string) => void;
  settle: (family: string, present: readonly string[]) => void;
  showAgain: (cause: string) => void;
} {
  const dismissed = useSyncExternalStore(onDismissals, () => dismissedIn(project));
  const dismiss = useCallback((cause: string) => dismissNotice(project, cause), [project]);
  const settle = useCallback(
    (family: string, present: readonly string[]) => settleNotices(project, family, present),
    [project],
  );
  const again = useCallback((cause: string) => showAgain(project, cause), [project]);
  return { dismissed, dismiss, settle, showAgain: again };
}

/** Forgets what this launch read and changed, as a new launch would. For tests. */
export function forgetDismissals(): void {
  now = undefined;
  writing = Promise.resolve();
}
