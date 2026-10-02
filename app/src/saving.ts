import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type PlaneId, type PlaneSaving, type RepoSaving } from "./bindings";
import { PLANE_SHAPE, usePlaneChanged } from "./planeChanged";

/** The window event a finished save sends, so every reader of the save standing reads again —
 *  the title bar after the Saving tab's button, and the tab after the title bar's. */
export const PLANE_SAVED = "charter-plane-saved";

/** The window event the Saving tab sends to leave a blocked save (charter-app#295): a chat, or a
 *  terminal, opened in the plane by the project that holds it. */
export const WAY_OUT = "charter-saving-way-out";

/** What a way out asks for, and of which project. */
export type WayOut = { plane: string; way: "chat" | "terminal" };

/** Ask the project `plane` to open a chat or a terminal in its plane. */
export function askWayOut(plane: string, way: WayOut["way"]): void {
  window.dispatchEvent(new CustomEvent<WayOut>(WAY_OUT, { detail: { plane, way } }));
}

/** Say that a save of some project finished. */
export function tellSaved(): void {
  window.dispatchEvent(new Event(PLANE_SAVED));
}

/** How often the standing is read while nothing else asks. A file an agent writes is not an
 *  event this window hears (`planewatch.rs` watches no memory directory and no `.git`), so the
 *  title bar asks git — one `status` — this often. */
export const SAVING_REREAD_MS = 10_000;

/**
 * **The project's save standing, kept fresh** (charter-app#294): asked when the project
 * changes, when it changes on disk, when the window comes back into focus, when any save
 * finishes, and every {@link SAVING_REREAD_MS}. Written as `useAlerts` is: the command's own
 * promise, a `gone` flag, and state set only in its callback.
 *
 * The last answer is kept while the next is on its way, so the bar never blanks between reads.
 */
export function usePlaneSaving(plane: PlaneId | undefined): {
  saving: PlaneSaving | undefined;
  reread: () => void;
} {
  const [saving, setSaving] = useState<{ plane: PlaneId; standing: PlaneSaving }>();
  const [asked, setAsked] = useState(0);
  const changed = usePlaneChanged(plane === undefined ? [] : [plane], PLANE_SHAPE);

  useEffect(() => {
    if (plane === undefined) return;
    let gone = false;
    void commands
      .planeSaving(plane)
      .then((answer) => {
        // Only a standing is drawn: an answer that is not one says nothing about the plane.
        if (!gone && answer.status === "ok" && typeof answer.data?.stage === "string")
          setSaving({ plane, standing: answer.data });
      })
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [plane, asked, changed]);

  // Coming back to the window is when the operator wants to know what came in: fetch, which the
  // core does at most once a minute, and whose answer reaches here as a plane change.
  useEffect(() => {
    if (plane === undefined) return;
    const fetch = () => void commands.planeFetch(plane).catch(() => undefined);
    window.addEventListener("focus", fetch);
    return () => window.removeEventListener("focus", fetch);
  }, [plane]);

  useRereads(setAsked);

  const reread = useCallback(() => setAsked((n) => n + 1), []);
  // Another project's answer is not this one's: drawn only for the project it was read for.
  return { saving: saving?.plane === plane ? saving?.standing : undefined, reread };
}

/** Ask again on focus, after any save, and every {@link SAVING_REREAD_MS}. */
function useRereads(setAsked: (next: (n: number) => number) => void): void {
  useEffect(() => {
    const again = () => setAsked((n) => n + 1);
    window.addEventListener("focus", again);
    window.addEventListener(PLANE_SAVED, again);
    const timer = setInterval(again, SAVING_REREAD_MS);
    return () => {
      window.removeEventListener("focus", again);
      window.removeEventListener(PLANE_SAVED, again);
      clearInterval(timer);
    };
  }, [setAsked]);
}

/** How many mounted hooks hold each project and workspace's repo rows (`useRepoSavingKept`). */
const kept = new Map<string, number>();

function keyOf(plane: PlaneId | undefined, workspace: string | undefined): string | undefined {
  return plane === undefined || workspace === undefined ? undefined : `${plane}\u0000${workspace}`;
}

/**
 * Keeps a project's repo rows while this is mounted, for a project behind the one in front: a
 * switch back to it draws them at once (FR-27). It asks for nothing and draws nothing; the rows
 * are {@link useRepoSaving}'s, and are forgotten once nothing holds them.
 */
export function useRepoSavingKept(plane: PlaneId, workspace: string | undefined): void {
  const key = keyOf(plane, workspace);
  useEffect(() => {
    if (key === undefined) return;
    kept.set(key, (kept.get(key) ?? 0) + 1);
    return () => {
      const left = (kept.get(key) ?? 1) - 1;
      if (left > 0) kept.set(key, left);
      else kept.delete(key);
    };
  }, [key]);
}

/**
 * **The workspace's repos' save standing, kept fresh** (charter-app#299): one row per clone,
 * read as {@link usePlaneSaving} reads the plane and at the same moments. `undefined` until
 * the first answer, and for no workspace at all — the strip of chats outside every workspace
 * has no repos.
 *
 * **The last answer for each project something holds, not only the one in front's** (FR-27):
 * a project switched back to draws the rows it had at once and asks again behind them, rather
 * than drawing none, waiting on `git status` per clone, and drawing the window again when it
 * answers. An answer that says what is already drawn draws nothing.
 */
export function useRepoSaving(
  plane: PlaneId | undefined,
  workspace: string | undefined,
): RepoSaving[] | undefined {
  const [repos, setRepos] = useState<Readonly<Record<string, RepoSaving[]>>>({});
  /** What is drawn, for an answer to be compared with before anything is set: an update that
   *  changes nothing still draws the hook's owner once before React lets it go. */
  const drawn = useRef(repos);
  const [asked, setAsked] = useState(0);
  const changed = usePlaneChanged(plane === undefined ? [] : [plane], PLANE_SHAPE);
  const key = keyOf(plane, workspace);

  useEffect(() => {
    if (plane === undefined || workspace === undefined || key === undefined) return;
    let gone = false;
    void commands
      .workspaceSaving(plane, workspace)
      .then((answer) => {
        if (gone || answer.status !== "ok" || !Array.isArray(answer.data)) return;
        const rows = answer.data;
        if (same(drawn.current[key], rows)) return;
        // Only what something still holds is kept, and the pair in front.
        const still = Object.entries(drawn.current).filter(([one]) => one !== key && kept.has(one));
        drawn.current = { ...Object.fromEntries(still), [key]: rows };
        setRepos(drawn.current);
      })
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [plane, workspace, key, asked, changed]);

  useRereads(setAsked);
  return key === undefined ? undefined : repos[key];
}

/** Whether two answers say the same: the core sends plain data, so its JSON is the answer. */
function same(drawn: RepoSaving[] | undefined, answer: RepoSaving[]): boolean {
  return drawn !== undefined && JSON.stringify(drawn) === JSON.stringify(answer);
}

/** The stages, furthest back first. */
const STAGES = ["blocked", "changed", "committed", "pr-open", "saved"];

/** How far back a stage is: smaller is further back. A stage charter does not know is saved. */
export function behindness(stage: string): number {
  const at = STAGES.indexOf(stage);
  return at === -1 ? STAGES.length : at;
}

/** Whether pressing a repo's Save could do anything: it is saved at all, and it has files to
 *  commit, a blocked save to try again, or commits a push would carry. */
export function repoSavable(repo: RepoSaving): boolean {
  return (
    repo.stage !== "off" &&
    (repo.changed > 0 || repo.stage === "blocked" || (repo.stage === "committed" && repo.pushes))
  );
}

/** A repo's stage, as its row and the title bar say it. */
export function repoStageText(repo: RepoSaving): string {
  switch (repo.stage) {
    case "off":
      return "Off — charter does not save this repo";
    case "blocked":
      return `Blocked: ${repo.blocked ?? "the last save could not finish"}`;
    case "changed":
      return `${repo.changed} changed`;
    case "committed":
      return repo.ahead === null ? "Committed, not pushed" : `${repo.ahead} committed, not pushed`;
    case "pr-open":
      return "Pushed — waiting on its pull request";
    default:
      return "Saved";
  }
}

/**
 * **Where a repo's Save goes**, said before anyone presses it (ADR 0051, amended 2026-09-25):
 * the steps `reposave::save_as` takes, by mode, including where it stops short. A save always
 * commits every changed file on the branch the clone is on; in a PR mode on the pull
 * request's base or the default branch, that commit is then pushed as a branch of charter's
 * own, never to the branch itself.
 */
export function repoSaveGoesTo(repo: RepoSaving, workspace: string): string {
  if (repo.mode === "off") return "Nowhere — charter does not save it";
  const on = repo.branch;
  if (on === null) return "Nowhere — the clone is not on a branch; check one out first";
  const noForge = "its origin is not on a forge charter knows";
  switch (repo.mode) {
    case "commit":
      return `Commits on ${on} — nothing is pushed`;
    case "push":
      return repo.pushes
        ? `Commits on ${on} and pushes ${on}`
        : `Commits on ${on} — nothing is pushed: ${noForge}`;
    case "pr":
    case "pr-merge": {
      if (!repo.pushes) return `Commits on ${on}, then stops: ${noForge}`;
      if (repo.target === null)
        return `Commits on ${on}, then stops: charter does not know where a pull request goes — set [repos.${repo.name}] branch`;
      const pushed = repo.ownBranch
        ? `pushes that commit as charter/${workspace}/… (never to ${on})`
        : `pushes ${on}`;
      const merge =
        repo.mode === "pr-merge" ? ", and asks it to merge itself once its checks pass" : "";
      return `Commits on ${on}, ${pushed}, and opens a pull request into ${repo.target}${merge}`;
    }
    default:
      return `Saved by mode ${repo.mode}`;
  }
}

/** What saving everything said: every line, and every refusal in the core's words. */
export type SavedAll = { said: string[]; refused: string[] };

/**
 * **Save all** (charter-app#299): the plane, when `plane` is true, then each repo in `repos`,
 * one after another — a repo's save may push and open a pull request, and the core runs one
 * save per tree at a time anyway. A refusal does not stop the rest: each is its own tree.
 */
export async function saveAll(
  planeId: PlaneId,
  plane: boolean,
  message: string | null,
  workspace: string | undefined,
  repos: readonly RepoSaving[],
): Promise<SavedAll> {
  const out: SavedAll = { said: [], refused: [] };
  const take = (got: { status: "ok"; data: string[] } | { status: "error"; error: string }) => {
    if (got.status === "ok") out.said.push(...got.data);
    else out.refused.push(got.error);
  };
  const guarded = async (run: () => Promise<Parameters<typeof take>[0]>) => {
    try {
      take(await run());
    } catch (err: unknown) {
      out.refused.push(String(err));
    }
  };
  if (plane) await guarded(() => commands.savePlane(planeId, message));
  if (workspace !== undefined)
    for (const repo of repos.filter(repoSavable))
      await guarded(() => commands.saveRepo(planeId, workspace, repo.name, null));
  return out;
}
