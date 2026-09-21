import type { Piece, RepoState } from "./bindings";
import type { WorkspaceState } from "./workspaceState";

/**
 * The bottom region: what the focused workspace's repos are doing (charter ADR 0038).
 *
 * Repo git state, worktrees and pipelines, which used to be two sections of the right-hand
 * side. They moved because the right-hand side is what is asking for you and this is not: it
 * is what is true, and nothing in it can be pressed.
 *
 * **Read-only, and that is asserted rather than described** — `regions.e2e.ts` presses on
 * every control in here and expects to find none. It is the one half of ADR 0038's reading
 * ("the bottom is where you read what is true and do not touch it") that a test can hold.
 *
 * **Nothing here waits on a network.** The CI cell is what a forge refresher last wrote into
 * `.charter/cache/glstate.json`; charter-app reads that file and never fetches. A cell with
 * nothing to show says why, because a blank one reads as "green" to a person in a hurry.
 *
 * **The worktrees are a count and a list of branches, not the explorer's rows again.** They
 * are in two regions — ADR 0038 names that as the visible crack in its own rule — and the
 * least dishonest way to have them in both is to make each answer its own question: on the
 * left a piece is a thing you pick, here it is a branch that exists and may be unwired.
 */
export function BottomBar({
  workspace,
  state,
}: {
  workspace: string | undefined;
  state: WorkspaceState;
}) {
  if (workspace === undefined) {
    return (
      <footer className="state-bar" aria-label="Repository state" data-testid="bottom-bar">
        <p className="empty">No workspace focused.</p>
      </footer>
    );
  }

  const { panels, repos, pieces, piecesRefused, reading, trouble } = state;
  const byName = new Map((repos?.repos ?? []).map((repo) => [repo.name, repo]));
  const names = panels?.repos ?? [];

  return (
    <footer className="state-bar" aria-label="Repository state" data-testid="bottom-bar">
      {trouble && (
        <p className="trouble" role="alert">
          {trouble}
        </p>
      )}
      {repos?.cache_refused && (
        <p className="trouble" role="alert">
          {repos.cache_refused}
        </p>
      )}

      {panels === undefined ? (
        <p className="pending">Reading the plane…</p>
      ) : names.length === 0 && panels.absent.length === 0 ? (
        <p className="none">No repos in this workspace</p>
      ) : (
        <ul className="repo-states">
          {names.map((name) => (
            <RepoRow
              key={name}
              name={name}
              state={byName.get(name)}
              pieces={pieces[name]}
              piecesRefused={piecesRefused[name]}
              reading={reading}
            />
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

      <p className="note">
        CI was last fetched by a refresher. charter-app reads this, never fetches it.
      </p>
    </footer>
  );
}

/** One repo, in one row: where HEAD is, what is uncommitted, how many worktrees are cut off
 *  it, and what the forge cache last recorded. */
function RepoRow({
  name,
  state,
  pieces,
  piecesRefused,
  reading,
}: {
  name: string;
  state: RepoState | undefined;
  pieces: Piece[] | undefined;
  piecesRefused: string | undefined;
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
      <span className="worktrees" data-testid={`worktrees-${name}`}>
        {" · "}
        <Worktrees pieces={pieces} refused={piecesRefused} />
      </span>{" "}
      <CiCell name={name} state={state} reading={reading} />
    </li>
  );
}

/** What git says about this clone's pieces, in one phrase. */
function Worktrees({ pieces, refused }: { pieces: Piece[] | undefined; refused?: string }) {
  // Never "no worktrees". A listing charter could not run says so, for the same reason an
  // unreadable tree is never drawn as clean.
  if (refused !== undefined) return <span className="none">worktrees unreadable</span>;
  if (pieces === undefined) return <span className="pending">worktrees: asking git…</span>;
  if (pieces.length === 0) return <span className="none">no worktrees</span>;
  const stale = pieces.filter((piece) => piece.stale).length;
  const unwired = pieces.filter((piece) => !piece.wired && !piece.stale).length;
  return (
    <>
      <span className="count">
        {pieces.length} {pieces.length === 1 ? "worktree" : "worktrees"}
      </span>
      {/* The two states that change what starting a chat in one would mean. Counted here
          rather than listed: the row a person acts on is the explorer's. */}
      {unwired > 0 && <span className="label unwired"> {unwired} unwired</span>}
      {stale > 0 && <span className="label stale"> {stale} stale</span>}
    </>
  );
}

/** One repo's CI cell, which always says something. */
function CiCell({
  name,
  state,
  reading,
}: {
  name: string;
  state: RepoState | undefined;
  reading: boolean;
}) {
  return (
    <span className="ci" data-testid={`ci-${name}`}>
      {state === undefined ? (
        <span className="pending">{reading ? "reading…" : "not read"}</span>
      ) : (
        <CiWords state={state} />
      )}
    </span>
  );
}

function CiWords({ state }: { state: RepoState }) {
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

/** An age a person reads. The bar says how old an answer is, because a two-hour-old
 *  "success" is not the same claim as one from a minute ago. */
export function ago(seconds: number | null): string {
  if (seconds === null) return "at an unknown time";
  if (seconds < 90) return `${seconds}s ago`;
  if (seconds < 5400) return `${Math.round(seconds / 60)}m ago`;
  return `${Math.round(seconds / 3600)}h ago`;
}
