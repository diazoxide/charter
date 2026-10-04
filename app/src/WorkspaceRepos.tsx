import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { commands, type PlaneId } from "./bindings";
import { RepoPicker } from "./RepoPicker";
import { cloneRepos, useRepoClones } from "./repoClones";
import type { RowIds } from "./settings/components";

/**
 * **A workspace's repos, in Settings at its level** (ADR 0055; SE-20): the same picker as a new
 * workspace's, ticked with what is cloned here now, drawn as one row of the settings set — the
 * control is the picker, what is under way is said under it, and a refusal is the row's error.
 *
 * Applying clones what was ticked and removes what was unticked. **It is a button and not a
 * write on each tick**: a clone and a removal are things done, with no Undo, so a tick is held
 * until it is confirmed (the rule DS-3b set for a choice that does something). A removal goes
 * through `drop_repo`, whose guard is inside the delete: a clone holding uncommitted or unpushed
 * work, or any worktree, is refused, and the refusal is drawn here in the core's own words.
 * There is no way past it from this surface.
 *
 * What is cloned is read from disk (`workspace_repos`), never from what this window asked for.
 */
export function useWorkspaceRepos(
  plane: PlaneId,
  workspace: string,
): { control: (ids: RowIds) => ReactNode; error: readonly string[] } {
  const [have, setHave] = useState<ReadonlySet<string>>();
  const [picked, setPicked] = useState<ReadonlySet<string>>(new Set());
  const [applying, setApplying] = useState(false);
  const [refusals, setRefusals] = useState<string[]>([]);
  const clones = useRepoClones(plane, workspace);
  const reading = useRef(0);

  const read = useCallback(() => {
    const mine = ++reading.current;
    void commands
      .workspaceRepos(plane, workspace)
      .then((said) => (said.status === "ok" ? (said.data?.repos ?? []).map((r) => r.name) : []))
      .catch(() => [])
      .then((names) => {
        if (reading.current !== mine) return;
        const now = new Set(names);
        setHave(now);
        setPicked(now);
      });
  }, [plane, workspace]);
  useEffect(read, [read]);

  const adding = have === undefined ? [] : [...picked].filter((n) => !have.has(n)).sort();
  const removing = have === undefined ? [] : [...have].filter((n) => !picked.has(n)).sort();

  const apply = async () => {
    setApplying(true);
    const refused: string[] = [];
    for (const repo of removing) {
      const answer = await commands
        .dropRepo(plane, workspace, repo)
        .catch((err: unknown) => ({ status: "error" as const, error: { said: String(err) } }));
      if (answer.status === "error") refused.push(answer.error.said);
    }
    setRefusals(refused);
    if (adding.length > 0) await cloneRepos(plane, workspace, adding);
    setApplying(false);
    read();
  };

  const failed = [...clones].filter(([, c]) => c.state === "failed");
  const busy = [...clones].filter(([, c]) => c.state === "waiting" || c.state === "cloning");

  return {
    error: [
      ...failed.map(([repo, c]) => `${repo}: ${c.state === "failed" ? c.said : ""}`),
      ...refusals,
    ],
    control: (ids) => (
      <div role="group" aria-labelledby={ids.labelledBy} aria-describedby={ids.describedBy}>
        <RepoPicker plane={plane} picked={picked} onPicked={setPicked} />
        {busy.map(([repo, c]) => (
          <p key={repo} className="pending" aria-busy="true">
            {c.state === "cloning" ? `Cloning ${repo}…` : `${repo} is waiting to be cloned.`}
          </p>
        ))}
        <div className="settings-actions">
          {failed.map(([repo]) => (
            <button
              key={repo}
              type="button"
              className="panel-view"
              tabIndex={0}
              disabled={applying}
              onClick={() => void cloneRepos(plane, workspace, [repo]).then(read)}
            >
              Retry {repo}
            </button>
          ))}
          <button
            type="button"
            className="panel-view"
            tabIndex={0}
            disabled={applying || (adding.length === 0 && removing.length === 0)}
            onClick={() => void apply()}
          >
            {applyWords(adding.length, removing.length)}
          </button>
        </div>
      </div>
    ),
  };
}

function applyWords(adding: number, removing: number): string {
  if (adding === 0 && removing === 0) return "No changes";
  const parts = [];
  if (adding > 0) parts.push(`clone ${adding}`);
  if (removing > 0) parts.push(`remove ${removing}`);
  const said = parts.join(", ");
  return said.charAt(0).toUpperCase() + said.slice(1);
}
