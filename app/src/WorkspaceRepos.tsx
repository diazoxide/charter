import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type PlaneId } from "./bindings";
import { RepoPicker } from "./RepoPicker";
import { cloneRepos, useRepoClones } from "./repoClones";

/**
 * **A workspace's repos, in its settings** (ADR 0055): the same picker as a new workspace's,
 * ticked with what is cloned here now.
 *
 * Applying clones what was ticked and removes what was unticked. A removal goes through
 * `drop_repo`, whose guard is inside the delete: a clone holding uncommitted or unpushed work,
 * or any worktree, is refused, and the refusal is drawn here in the core's own words. There is
 * no way past it from this surface.
 *
 * What is cloned is read from disk (`workspace_repos`), never from what this window asked for.
 */
export function WorkspaceRepos({ plane, workspace }: { plane: PlaneId; workspace: string }) {
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

  return (
    <fieldset className="settings-group" aria-label="Repos">
      <legend>Repos</legend>
      <p className="settings-who">
        Ticked repos are cloned into this workspace; unticked ones are removed, unless they hold
        work that is not pushed.
      </p>
      <RepoPicker plane={plane} picked={picked} onPicked={setPicked} />
      {busy.map(([repo, c]) => (
        <p key={repo} className="pending" aria-busy="true">
          {c.state === "cloning" ? `Cloning ${repo}…` : `${repo} is waiting to be cloned.`}
        </p>
      ))}
      {failed.map(([repo, c]) => (
        <div key={repo} className="settings-actions">
          <p className="trouble" role="alert">
            {repo}: {c.state === "failed" ? c.said : ""}
          </p>
          <button
            type="button"
            className="panel-view"
            tabIndex={0}
            disabled={applying}
            onClick={() => void cloneRepos(plane, workspace, [repo]).then(read)}
          >
            Retry {repo}
          </button>
        </div>
      ))}
      {refusals.map((line) => (
        <p key={line} className="trouble" role="alert">
          {line}
        </p>
      ))}
      <div className="settings-actions">
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
    </fieldset>
  );
}

function applyWords(adding: number, removing: number): string {
  if (adding === 0 && removing === 0) return "No changes";
  const parts = [];
  if (adding > 0) parts.push(`clone ${adding}`);
  if (removing > 0) parts.push(`remove ${removing}`);
  const said = parts.join(", ");
  return said.charAt(0).toUpperCase() + said.slice(1);
}
