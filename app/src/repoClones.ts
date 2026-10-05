import { useSyncExternalStore } from "react";
import { commands, type PlaneId } from "./bindings";

/**
 * **The repos this window is cloning into a workspace, one at a time** (ADR 0055).
 *
 * A workspace is made at once and its repos land after it, so the operator can start a chat
 * while they clone. Each repo is one `clone_repo`, asked in turn: that is what lets each one's
 * own state be drawn as it lands and a failed one be asked again.
 *
 * **One clone at a time per workspace, whoever asked** (#1215). Every call joins ONE chain per
 * project and workspace, so two rows pressed a second apart wait for each other rather than
 * run side by side: no two clones race to write the workspace's manifest, the inventory's
 * read-modify-write, or the layer `clone` wires into a sibling still being cloned. A repo
 * already waiting or cloning is not asked for again. The state is this window's, for as long
 * as it is open — what is actually cloned is read from disk (`workspace_repos`), never from
 * here.
 */

export type CloneState =
  | { state: "waiting" }
  | { state: "cloning" }
  | { state: "cloned" }
  | { state: "failed"; said: string };

type Clones = ReadonlyMap<string, CloneState>;

const EMPTY: Clones = new Map();
const byWorkspace = new Map<string, Clones>();
/** The tail of each workspace's chain: the next call starts when this settles. */
const chains = new Map<string, Promise<unknown>>();
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

/** Whether a repo's clone is asked for and not finished: waiting its turn, or running. */
export function underWay(state: CloneState | undefined): boolean {
  return state?.state === "waiting" || state?.state === "cloning";
}

/**
 * Add `repos` to the plane's inventory, then clone them into `workspace` one after another,
 * after any clone this window already asked for there. A repo already under way is left to
 * the call that asked first. `onEach` hears each repo as it finishes, so a list can be read
 * again as it lands rather than once the batch ends. Resolves with the ones that failed, once
 * every one has been tried.
 */
export function cloneRepos(
  plane: PlaneId,
  workspace: string,
  repos: string[],
  onEach?: (repo: string, state: CloneState) => void,
): Promise<Failed> {
  const key = keyOf(plane, workspace);
  const mine = repos.filter((repo) => !underWay(byWorkspace.get(key)?.get(repo)));
  for (const repo of mine) set(plane, workspace, repo, { state: "waiting" });
  const run = (chains.get(key) ?? Promise.resolve()).then(() =>
    cloneInTurn(plane, workspace, mine, onEach),
  );
  chains.set(
    key,
    run.catch(() => undefined),
  );
  return run;
}

async function cloneInTurn(
  plane: PlaneId,
  workspace: string,
  repos: string[],
  onEach?: (repo: string, state: CloneState) => void,
): Promise<Failed> {
  if (repos.length === 0) return [];
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
    const state: CloneState =
      answer.status === "ok"
        ? { state: "cloned" }
        : {
            state: "failed",
            said: taken.status === "error" ? `${answer.error}\n${taken.error}` : answer.error,
          };
    set(plane, workspace, repo, state);
    if (state.state === "failed") failed.push({ repo, said: state.said });
    onEach?.(repo, state);
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

/**
 * Forget what this window knows about repos that are no longer missing from `workspace` — the
 * ones `absent` no longer names and that finished. A `cloned` repo stays busy until the list
 * read from disk drops it; forgetting it then means a folder deleted later can be cloned again.
 */
export function settleRepoClones(plane: PlaneId, workspace: string, absent: readonly string[]) {
  const key = keyOf(plane, workspace);
  const held = byWorkspace.get(key);
  if (held === undefined) return;
  const next = new Map(
    [...held].filter(([repo, one]) => one.state !== "cloned" || absent.includes(repo)),
  );
  if (next.size === held.size) return;
  byWorkspace.set(key, next);
  for (const listener of listeners) listener();
}

/** For tests: forget every clone. */
export function forgetRepoClones() {
  byWorkspace.clear();
  chains.clear();
}
