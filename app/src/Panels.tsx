import { useEffect, useState } from "react";
import { commands, type Panels as PanelsModel, type RepoState, type RepoStates } from "./bindings";

/**
 * The right-hand side: the focused workspace's repos, their branches, its CI, its todos and
 * the plane's personas (spec decision 1).
 *
 * **Read-only.** Nothing here writes to the plane — M1 says panels are read-only, and what
 * changes a workspace is a command, not a click in a panel.
 *
 * **Nothing here waits on a network.** The CI cell is what a forge refresher last wrote into
 * `.charter/cache/glstate.json`; charter-app reads that file and never fetches. A cell with
 * nothing to show says why, because a blank one reads as "green" to a person in a hurry.
 */
export function Panels({ workspace }: { workspace: string | undefined }) {
  const { panels, repos, trouble, reading } = useWorkspace(workspace);

  if (workspace === undefined) {
    return (
      <aside className="panels" aria-label="Workspace" data-testid="panels">
        <p className="empty">No workspace focused.</p>
      </aside>
    );
  }

  const byName = new Map((repos?.repos ?? []).map((repo) => [repo.name, repo]));
  const names = panels?.repos ?? [];

  return (
    <aside className="panels" aria-label={`Workspace ${workspace}`} data-testid="panels">
      {trouble && (
        <p className="trouble" role="alert">
          {trouble}
        </p>
      )}

      <section data-testid="panel-repos">
        <h2>Repos</h2>
        {panels === undefined ? (
          <p className="pending">Reading the plane…</p>
        ) : names.length === 0 && panels.absent.length === 0 ? (
          <p className="none">No repos in this workspace</p>
        ) : (
          <ul className="repos">
            {names.map((name) => (
              <RepoRow key={name} name={name} state={byName.get(name)} reading={reading} />
            ))}
            {panels.absent.map((name) => (
              <li key={`absent-${name}`} className="repo-row absent" data-testid={`repo-${name}`}>
                <span className="repo">{name}</span>
                {/* Membership without a clone. Said, because a repo the workspace means to
                    hold and nobody has cloned is not the same as one that is not listed. */}
                <span className="branch none">not cloned here</span>
              </li>
            ))}
          </ul>
        )}
        {/* A refusal is drawn, never swallowed: a row that is simply missing looks like a
            workspace with fewer repos than it has. */}
        {panels?.refused.map(([name, why]) => (
          <p className="trouble" role="alert" key={`refused-${name}`}>
            charter will not read <code>{name}</code>: {why}
          </p>
        ))}
      </section>

      <section data-testid="panel-ci">
        <h2>CI</h2>
        <p className="note">
          Last fetched by a refresher. charter-app reads this, never fetches it.
        </p>
        {repos?.cache_refused && (
          <p className="trouble" role="alert">
            {repos.cache_refused}
          </p>
        )}
        {names.length === 0 ? (
          <p className="none">Nothing to check</p>
        ) : (
          <ul className="ci">
            {names.map((name) => (
              <li key={name} data-testid={`ci-${name}`}>
                <span className="repo">{name}</span>{" "}
                <CiCell state={byName.get(name)} reading={reading} />
              </li>
            ))}
          </ul>
        )}
      </section>

      <section data-testid="panel-todos">
        <h2>Todos</h2>
        {panels?.todos_refused && (
          <p className="trouble" role="alert">
            {panels.todos_refused}
          </p>
        )}
        {panels === undefined ? (
          <p className="pending">Reading the plane…</p>
        ) : panels.todos.length === 0 ? (
          <p className="none">Nothing to do</p>
        ) : (
          <ul className="todos">
            {panels.todos.map((todo) => (
              <li key={todo.slug}>
                <span className="todo-title">{todo.title}</span>
                {todo.stamp && <span className="stamp"> · {todo.stamp}</span>}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section data-testid="panel-personas">
        <h2>Personas</h2>
        {panels === undefined ? (
          <p className="pending">Reading the plane…</p>
        ) : panels.personas.length === 0 ? (
          <p className="none">No personas on this plane</p>
        ) : (
          <ul className="personas">
            {panels.personas.map((persona) => (
              <li key={persona}>
                {persona}
                {persona === panels.persona && <span className="default"> · default</span>}
              </li>
            ))}
          </ul>
        )}
      </section>
    </aside>
  );
}

/** One repo: what it is called, where its HEAD is, and how far that is from its upstream. */
function RepoRow({
  name,
  state,
  reading,
}: {
  name: string;
  state: RepoState | undefined;
  reading: boolean;
}) {
  return (
    <li className="repo-row" data-testid={`repo-${name}`}>
      <span className="repo">{name}</span>{" "}
      {state === undefined ? (
        <span className="branch pending">{reading ? "reading…" : "not read"}</span>
      ) : state.unreadable ? (
        // Never "clean". A tree charter could not read is the one thing a panel must not
        // round down, because the round-down says everything is fine.
        <span className="branch unreadable" role="alert">
          {state.unreadable}
        </span>
      ) : (
        <>
          <span className="branch">{headOf(state)}</span>
          <span className="dirt"> · {dirtOf(state)}</span>
          {state.upstream && <span className="upstream"> · {state.upstream}</span>}
          {gapOf(state) && <span className="gap"> · {gapOf(state)}</span>}
        </>
      )}
    </li>
  );
}

/** One repo's CI cell, which always says something. */
function CiCell({ state, reading }: { state: RepoState | undefined; reading: boolean }) {
  if (state === undefined)
    return <span className="pending">{reading ? "reading…" : "not read"}</span>;
  if (state.ci) {
    return (
      <span className={`ci-state ci-${state.ci}`}>
        {state.ci}
        {state.change !== null && (
          <span className="change">
            {" "}
            {state.sigil ?? "#"}
            {state.change}
          </span>
        )}
        <span className="stamp"> · {ago(state.fetched_seconds_ago)}</span>
      </span>
    );
  }
  if (state.not_fetched) return <span className="none">not fetched — {state.not_fetched}</span>;
  // An entry inside the window that names no pipeline. The cache cannot tell "there is none"
  // from "the call failed", so neither can this — but it is still a fetch, with an age.
  return <span className="none">no pipeline recorded · {ago(state.fetched_seconds_ago)}</span>;
}

/** Where HEAD is, in words. */
function headOf(state: RepoState): string {
  if (state.detached !== null) return `detached at ${state.detached || "an unnamed commit"}`;
  if (state.branch === null) return "no branch";
  return state.unborn ? `${state.branch} (no commits yet)` : state.branch;
}

/** Whether there is anything uncommitted, counted the way git counts it. */
function dirtOf(state: RepoState): string {
  const parts: string[] = [];
  if (state.tracked > 0) parts.push(`${state.tracked} changed`);
  if (state.untracked > 0) parts.push(`${state.untracked} untracked`);
  return parts.length === 0 ? "clean" : parts.join(", ");
}

/** How far the branch is from its upstream, or nothing when it is level. */
function gapOf(state: RepoState): string {
  const parts: string[] = [];
  if (state.ahead > 0) parts.push(`${state.ahead} ahead`);
  if (state.behind > 0) parts.push(`${state.behind} behind`);
  return parts.join(", ");
}

/** An age a person reads. The panel says how old an answer is, because a two-hour-old
 *  "success" is not the same claim as one from a minute ago. */
export function ago(seconds: number | null): string {
  if (seconds === null) return "at an unknown time";
  if (seconds < 90) return `${seconds}s ago`;
  if (seconds < 5400) return `${Math.round(seconds / 60)}m ago`;
  return `${Math.round(seconds / 3600)}h ago`;
}

/** Everything the core has said about ONE workspace. The name is part of the state, not
 *  beside it: that is what makes an answer for another workspace recognisable. */
type Answer = {
  workspace: string;
  panels?: PanelsModel;
  repos?: RepoStates;
  trouble?: string;
  /** Whether the repos ask has come back, however it came back. */
  read?: boolean;
};

/** What the core says about one workspace, in the two answers it gives.
 *
 *  Two asks, not one: the plane's own files come back at once and paint, and the part that
 *  runs git arrives after.
 *
 *  A late answer for a workspace that is no longer focused is dropped three times over — by
 *  the cleanup, by the name the answer itself carries, and by the state being keyed on the
 *  workspace, so nothing left over from the last one is ever drawn under this one's heading.
 *  Nothing is reset when the focus changes, because clearing state from inside an effect is
 *  a render the window does not need; the stale answer is simply not this workspace's. */
function useWorkspace(workspace: string | undefined) {
  const [answer, setAnswer] = useState<Answer>();

  useEffect(() => {
    if (workspace === undefined) return;
    let gone = false;
    // Merged onto what is already known about THIS workspace, and onto nothing when what is
    // held belongs to another one.
    const told = (what: Partial<Answer>) => {
      setAnswer((was) => ({
        ...(was?.workspace === workspace ? was : {}),
        workspace,
        ...what,
      }));
    };

    void commands
      .workspacePanels(workspace)
      .then((said) => {
        if (gone) return;
        if (said.status === "error") {
          told({ trouble: said.error });
        } else if (said.data?.workspace === workspace) {
          // An `ok` answer with no body reads as `ok` all the same, and the window must not
          // throw inside a promise nothing is holding. It simply has nothing to draw.
          told({ panels: said.data });
        }
      })
      .catch((err: unknown) => {
        if (!gone) told({ trouble: String(err) });
      });

    void commands
      .workspaceRepos(workspace)
      .then((said) => {
        if (gone) return;
        if (said.status === "error") {
          told({ read: true, trouble: said.error });
        } else if (said.data?.workspace === workspace) {
          told({ read: true, repos: said.data });
        }
      })
      .catch((err: unknown) => {
        if (!gone) told({ read: true, trouble: String(err) });
      });

    return () => {
      gone = true;
    };
  }, [workspace]);

  const mine = answer?.workspace === workspace ? answer : undefined;
  return {
    panels: mine?.panels,
    repos: mine?.repos,
    trouble: mine?.trouble,
    // Reading until the answer that runs git has come back for THIS workspace.
    reading: workspace !== undefined && !mine?.read,
  };
}
