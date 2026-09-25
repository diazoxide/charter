import { useSyncExternalStore } from "react";
import { commands, type PlaneId } from "./bindings";

/**
 * **The repos this window is cloning into a workspace, one at a time** (ADR 0055).
 *
 * A workspace is made at once and its repos land after it, so the operator can start a chat
 * while they clone. Each repo is one `clone_repo`, asked in turn: that is what lets each one's
 * own state be drawn as it lands and a failed one be asked again, and it means no two clones
 * race to write the workspace's manifest. The state is this window's, for as long as it is
 * open — what is actually cloned is read from disk (`workspace_repos`), never from here.
 */

export type CloneState =
  | { state: "waiting" }
  | { state: "cloning" }
  | { state: "cloned" }
  | { state: "failed"; said: string };

type Clones = ReadonlyMap<string, CloneState>;

const EMPTY: Clones = new Map();
const byWorkspace = new Map<string, Clones>();
const listeners = new Set<() => void>();

function keyOf(plane: PlaneId, workspace: string): string {
  return `${plane}\u0000${workspace}`;
}

function set(plane: PlaneId, workspace: string, repo: string, state: CloneState) {
  const key = keyOf(plane, workspace);
  const next = new Map(byWorkspace.get(key) ?? EMPTY);
  next.set(repo, state);
  byWorkspace.set(key, next);
  for (const listener of listeners) listener();
}

/** Each repo that could not be cloned, with charter's own sentence about it. */
export type Failed = { repo: string; said: string }[];

/**
 * Add `repos` to the plane's inventory, then clone them into `workspace` one after another.
 * Resolves with the ones that failed, once every one has been tried.
 */
export async function cloneRepos(
  plane: PlaneId,
  workspace: string,
  repos: string[],
): Promise<Failed> {
  for (const repo of repos) set(plane, workspace, repo, { state: "waiting" });
  const taken = await commands
    .takeRepos(plane, repos)
    .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
  // A pick the inventory could not take still reaches `clone`, which says in its own words
  // what it could not find — one sentence per repo, where the operator is looking.
  const failed: Failed = [];
  for (const repo of repos) {
    set(plane, workspace, repo, { state: "cloning" });
    const answer = await commands
      .cloneRepo(plane, workspace, repo)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (answer.status === "ok") {
      set(plane, workspace, repo, { state: "cloned" });
    } else {
      const said = taken.status === "error" ? `${answer.error}\n${taken.error}` : answer.error;
      set(plane, workspace, repo, { state: "failed", said });
      failed.push({ repo, said });
    }
  }
  return failed;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** What this window has asked to clone into `workspace`, by repo. */
export function useRepoClones(plane: PlaneId, workspace: string): Clones {
  return useSyncExternalStore(subscribe, () => byWorkspace.get(keyOf(plane, workspace)) ?? EMPTY);
}

/** For tests: forget every clone. */
export function forgetRepoClones() {
  byWorkspace.clear();
}
