import { useEffect, useSyncExternalStore } from "react";
import { commands, type PlaneId } from "./bindings";

/**
 * **The theme each project this window holds draws** (charter-app#273, ADR 0048) — **in each of
 * its workspaces** (charter-app#281) — kept as `extensionsOn.ts` keeps what each has on, and for
 * the same reason: the answer is the core's (`extension::project::theme::resolve`, asked with
 * `project_theme_drawn` — the record, the project's two files and the workspace's
 * `workspace.json`, no extension's directory), asked once per project and workspace and again
 * after a save. A workspace's `settings.theme.use` is a layer between the project's Shared and
 * Local files, so the question is the pair; `undefined` asks for the project outside every
 * workspace.
 *
 * `null` is an answer: nothing picked a theme, and the window keeps its own. A question that
 * failed is read as that too — a project whose theme cannot be asked must not hold the window's
 * theme back.
 *
 * **Only what something on screen asks about is kept.** The window asks for the workspace in
 * front and a settings tab for its own, so the pairs change with every workspace switch, and a
 * workspace can be deleted. Each hook says it is interested while it is mounted; a pair nothing
 * is interested in any more is forgotten, so a change on disk re-asks what is on screen and not
 * every workspace ever focused.
 */

type Where = { plane: PlaneId; workspace: string | undefined };

const whereOf = new Map<string, Where>();
const known = new Map<string, string | null>();
/** The newest question out per project and workspace: an answer to an older one is dropped. */
const latest = new Map<string, number>();
/** How many answers each pair has had: what a settings tab re-reads its own sentence on, since
 *  an approval in the Extensions dialog tells this store and not the tab. */
const answers = new Map<string, number>();
/** How many mounted hooks are asking about each pair. */
const interest = new Map<string, number>();
const listeners = new Set<() => void>();

function keyOf(plane: PlaneId, workspace: string | undefined): string {
  return `${plane}\u0000${workspace ?? ""}`;
}

function ask(plane: PlaneId, workspace: string | undefined) {
  const key = keyOf(plane, workspace);
  whereOf.set(key, { plane, workspace });
  const mine = (latest.get(key) ?? 0) + 1;
  latest.set(key, mine);
  void commands
    .projectThemeDrawn(plane, workspace ?? null)
    .then((said) => (said.status === "ok" ? (said.data ?? null) : null))
    .catch(() => null)
    .then((drawn) => {
      if (latest.get(key) !== mine) return;
      known.set(key, drawn);
      answers.set(key, (answers.get(key) ?? 0) + 1);
      for (const listener of listeners) listener();
    });
}

/** Ask `plane` again, in every workspace something on screen asks about — after its settings
 *  or a workspace's were saved, the plane changed on disk, or an extension was approved or
 *  removed. With no plane, every one. */
export function projectThemeChanged(plane?: PlaneId) {
  for (const where of [...whereOf.values()])
    if (plane === undefined || where.plane === plane) ask(where.plane, where.workspace);
}

/** Says a hook is asking about `plane` in `workspace` while it is mounted, and asks once; the
 *  answer is forgotten when nothing asks about it any more. */
function useInterest(plane: PlaneId | undefined, workspace: string | undefined) {
  useEffect(() => {
    if (plane === undefined) return;
    const key = keyOf(plane, workspace);
    interest.set(key, (interest.get(key) ?? 0) + 1);
    if (!latest.has(key)) ask(plane, workspace);
    return () => {
      const left = (interest.get(key) ?? 1) - 1;
      if (left > 0) {
        interest.set(key, left);
        return;
      }
      interest.delete(key);
      whereOf.delete(key);
      known.delete(key);
      latest.delete(key);
      answers.delete(key);
    };
  }, [plane, workspace]);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** What `plane` draws in `workspace`, as a file holds it; `null` when nothing picked one;
 *  `undefined` until it has said. Redraws when it changes, asking once. */
export function useProjectTheme(
  plane: PlaneId | undefined,
  workspace?: string,
): string | null | undefined {
  useInterest(plane, workspace);
  return useSyncExternalStore(subscribe, () =>
    plane === undefined ? undefined : known.get(keyOf(plane, workspace)),
  );
}

/** How many times `plane` has answered in `workspace`, asking once: a number that changes
 *  whenever what the window draws there was asked again — after a save, an approval or a
 *  removal, or a change on disk. */
export function useProjectThemeAnswers(plane: PlaneId, workspace?: string): number {
  useInterest(plane, workspace);
  return useSyncExternalStore(subscribe, () => answers.get(keyOf(plane, workspace)) ?? 0);
}

/** For tests: forget every answer. */
export function forgetProjectThemes() {
  whereOf.clear();
  known.clear();
  latest.clear();
  answers.clear();
  interest.clear();
}
