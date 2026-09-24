import type { ReactNode } from "react";
import {
  CircleCheck,
  CircleDashed,
  CircleSlash,
  CircleX,
  Clock,
  FolderGit2,
  GitBranch,
  Hand,
  LoaderCircle,
  SkipForward,
  TriangleAlert,
} from "lucide-react";
import type { Piece, RepoState } from "./bindings";
import { Menued } from "./Menus";
import type { Catalogued, Offer } from "./actions";
import type { WorkspaceState } from "./workspaceState";
import { useArrived } from "./lib/arrived";

/**
 * The bottom region: what the focused workspace's repos are doing (ADR 0038).
 *
 * Repo git state, worktrees and pipelines, which used to be two sections of the right-hand
 * side. They moved because the right-hand side is what is asking for you and this is not: it
 * is what is true, and nothing in it can be pressed.
 *
 * **Read-only, and that is asserted rather than described** — `regions.e2e.ts` presses on
 * every control in here and expects to find none. It is the one half of ADR 0038's reading
 * ("the bottom is where you read what is true and do not touch it") that a test can hold.
 *
 * **A repo row has a context menu, and a menu is not a control** (charter-app#174). It is the
 * explorer's clone menu — `New tab in <repo>` and `Start new chats in <repo>` — drawn from the
 * same catalogue rows, now that the core says where a clone is (`Panels.paths`). The row gains
 * no button and no Tab stop, the menu is drawn in a portal outside this region, and neither row
 * touches the repo the region is reading: both are about where the next chat starts. So the
 * spec that presses on everything in here still finds nothing to press. A pointer reaches it
 * here; the keyboard reaches the same two rows on the explorer's clone heading and in the
 * palette.
 *
 * **The worktree rows under each repo still have none, and that is a decision.** Their verbs
 * exist — `worktree.merge:<repo>/<piece>` and `worktree.remove:<repo>/<piece>`, drawn on the
 * explorer's rows — but putting `Remove worktree` under the pointer in the region ADR 0038
 * says is for reading is an amendment to ADR 0038, argued on its own, and not a defect fix.
 * A right-click there, and on a repo nobody cloned, answers with nothing rather than with the
 * browser's own menu, which `useNoBrowserMenu` covers for the whole window.
 *
 * **Nothing here waits on a network.** The CI cell is what a forge refresher last wrote into
 * `.charter/cache/glstate.json`; charter-app reads that file and never fetches. A cell with
 * nothing to show says why, because a blank one reads as "green" to a person in a hurry.
 *
 * **The worktrees are a count and a list of branches, not the explorer's rows again.** They
 * are in two regions — ADR 0038 names that as the visible crack in its own rule — and the
 * least dishonest way to have them in both is to make each answer its own question: on the
 * left a piece is a thing you pick, here it is a branch that exists and may be unwired.
 *
 * ## It is a table, and that is the answer to "add tabs columns"
 *
 * Every repo used to be one run-on sentence — `svc main · 3 changed, 1 untracked · origin/main
 * · 2 ahead · 2 worktrees · failed #41 · 2m ago` — and with four repos there was no way to read
 * *down* it. "Which of these is dirty" is a column question, and a column question asked of
 * prose is answered by reading every word of every row.
 *
 * So it is a real `<table>` with a real `<thead>`: the columns line up because a table lays
 * them out, and a screen reader says "Changes: 3 changed" rather than reading the row as one
 * sentence. A grid of `<li>`s would need `display: contents` to align across rows, which drops
 * the list semantics in WebKit — and this window runs in WebKit on both platforms.
 *
 * **It stays unpressable.** A `<table>` has no interactive element in it, `<thead>` is not a
 * tablist, and the column headings are `<th scope="col">`. Tabs in the sense of *controls* are
 * exactly what ADR 0038 forbids down here, and the spec that presses on everything in this
 * region would have said so.
 *
 * ## The worktrees are a tree here too
 *
 * Under each repo's row, spanning its full width, is that clone's pieces drawn with the same
 * guides the explorer uses — the operator asked for the tree in both places. It is not the
 * explorer's rows brought back: nothing in it is pickable, it carries the branch and the two
 * states that change what starting a chat there would mean, and the row above it keeps the
 * counts. It is the "list of branches" this component's own docstring has always promised,
 * finally drawn as the thing it is.
 *
 * ## A cell that names something is one line
 *
 * The same rule the explorer takes, asked of both regions by the operator: a row never folds,
 * and the region scrolls sideways instead. Here it replaces `overflow-wrap: anywhere`, which
 * broke `origin/main` in the middle of a word to make it fit — and a table cell is as tall as
 * its ROW, so one folded cell made all five of them two lines tall and the column question
 * this table exists to answer took two passes again.
 *
 * **The two cells that hold a SENTENCE still wrap**: a tree charter could not read, and a
 * pipeline nobody fetched, each say why in charter's own words. Held on one line, either would
 * push the region's horizontal scroll out past every column it has. `regions.e2e.ts` holds
 * both halves, in a real WebView, because jsdom lays nothing out.
 */
export function BottomBar({
  workspace,
  state,
  offers,
  onPress,
}: {
  workspace: string | undefined;
  state: WorkspaceState;
  /** The catalogue by id, which is what a repo row's menu is drawn out of. */
  offers: Catalogued;
  onPress: (offer: Offer) => void;
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
      {trouble && <Trouble>{trouble}</Trouble>}
      {repos?.cache_refused && <Trouble>{repos.cache_refused}</Trouble>}

      {panels === undefined ? (
        <Pending>Reading the plane…</Pending>
      ) : names.length === 0 && panels.absent.length === 0 ? (
        <p className="none">No repos in this workspace</p>
      ) : (
        <table className="repo-states">
          <thead>
            <tr>
              <th scope="col">Repo</th>
              <th scope="col">Branch</th>
              <th scope="col">Changes</th>
              <th scope="col">Worktrees</th>
              <th scope="col">Pipeline</th>
            </tr>
          </thead>
          {names.map((name) => (
            <RepoRows
              key={name}
              name={name}
              state={byName.get(name)}
              pieces={pieces[name]}
              piecesRefused={piecesRefused[name]}
              reading={reading}
              offers={offers}
              onPress={onPress}
            />
          ))}
          {panels.absent.map((name) => (
            <tbody key={`absent-${name}`}>
              <tr className="repo-row absent" data-testid={`repo-${name}`}>
                <th scope="row" className="repo">
                  <FolderGit2 className="node-icon" />
                  <span>{name}</span>
                </th>
                {/* Membership without a clone. Said, because a repo the workspace means to
                    hold and nobody has cloned is not the same as one that is not listed. */}
                <td className="branch none" colSpan={4}>
                  not cloned here
                </td>
              </tr>
            </tbody>
          ))}
        </table>
      )}

      {/* A refusal is drawn, never swallowed: a row that is simply missing looks like a
          workspace with fewer repos than it has. */}
      {panels?.refused.map(([name, why]) => (
        <Trouble key={`refused-${name}`}>
          charter will not read <code>{name}</code>: {why}
        </Trouble>
      ))}

      <p className="note">
        CI was last fetched by a refresher. charter-app reads this, never fetches it.
      </p>
    </footer>
  );
}

/** A refusal, with the mark that says it is one. Lucide hides a nameless icon from a screen
 *  reader itself, so the alert reads exactly as it did before. */
function Trouble({ children }: { children: ReactNode }) {
  return (
    <p className="trouble" role="alert">
      <TriangleAlert className="node-icon" />
      <span>{children}</span>
    </p>
  );
}

/** Something charter is still reading, which is a state no colour tells from a stopped one. */
function Pending({ children }: { children: ReactNode }) {
  return (
    <p className="pending">
      <LoaderCircle className="node-icon spinning" />
      <span>{children}</span>
    </p>
  );
}

/** One repo: its row of columns, and — when git has listed any — its worktrees as a tree
 *  under it.
 *
 *  Its own `<tbody>`, so the two rows are one thing to the browser and to a screen reader,
 *  and so the tree is unmistakably *this* clone's rather than a row that happens to follow. */
function RepoRows({
  name,
  state,
  pieces,
  piecesRefused,
  reading,
  offers,
  onPress,
}: {
  name: string;
  state: RepoState | undefined;
  pieces: Piece[] | undefined;
  piecesRefused: string | undefined;
  reading: boolean;
  offers: Catalogued;
  onPress: (offer: Offer) => void;
}) {
  const tree = pieces !== undefined && pieces.length > 0;
  return (
    <tbody>
      {/* The clone's menu, on its row of columns and not on the `<tbody>`: the tree under it
          is the pieces', which have no menu down here (see the docstring). `asChild`, so the
          table gains no element. */}
      <Menued on={{ on: "clone", repo: name }} offers={offers} onPress={onPress}>
        <tr className="repo-row" data-testid={`repo-${name}`}>
          <th scope="row" className="repo">
            <FolderGit2 className="node-icon" />
            <span>{name}</span>
          </th>
          {state === undefined ? (
            <td className="branch pending" colSpan={2}>
              {reading ? "reading…" : "not read"}
            </td>
          ) : state.unreadable ? (
            // Never "clean". A tree charter could not read is the one thing a panel must not
            // round down, because the round-down says everything is fine. It takes both columns
            // rather than leaving an empty "Changes" cell beside it — an empty cell in a table
            // reads as "nothing", which is the round-down in another shape.
            <td colSpan={2}>
              <span className="branch unreadable" role="alert">
                <TriangleAlert className="node-icon" />
                {state.unreadable}
              </span>
            </td>
          ) : (
            <>
              <td className="branch">
                <GitBranch className="node-icon" />
                <span>{headOf(state)}</span>
                {state.upstream && <span className="upstream">{state.upstream}</span>}
                {gapOf(state) && <span className="gap">{gapOf(state)}</span>}
              </td>
              <td className="dirt">{dirtOf(state)}</td>
            </>
          )}
          <td className="worktrees" data-testid={`worktrees-${name}`}>
            <Worktrees pieces={pieces} refused={piecesRefused} />
          </td>
          <CiCell name={name} state={state} reading={reading} />
        </tr>
      </Menued>
      {tree && (
        <tr className="worktree-tree-row">
          {/* The whole width, because a tree indented inside one column of five would be
              three characters wide at the window sizes this region is given. */}
          <td colSpan={5}>
            <ul className="worktree-tree" data-testid={`worktree-tree-${name}`}>
              {pieces.map((piece) => (
                <li key={piece.piece}>
                  <GitBranch className="node-icon" />
                  <span className="piece">{piece.piece}</span>
                  {/* The branch only when it says something the name does not. charter cuts a
                      piece on a branch of its own name by default, and `perf perf` down a
                      whole column is the same word twice on every row. */}
                  {piece.branch && piece.branch !== piece.piece && (
                    <code className="branch">{piece.branch}</code>
                  )}
                  {/* The same two states the count above totals, said here of the one tree
                      they are true of. A total answers "is anything wrong in this clone";
                      a row answers "which one". */}
                  {piece.stale ? (
                    <span className="label stale">stale</span>
                  ) : (
                    !piece.wired && <span className="label unwired">unwired</span>
                  )}
                </li>
              ))}
            </ul>
          </td>
        </tr>
      )}
    </tbody>
  );
}

/** What git says about this clone's pieces, in one phrase. */
function Worktrees({ pieces, refused }: { pieces: Piece[] | undefined; refused?: string }) {
  // Never "no worktrees". A listing charter could not run says so, for the same reason an
  // unreadable tree is never drawn as clean.
  if (refused !== undefined) return <span className="none">worktrees unreadable</span>;
  if (pieces === undefined)
    return (
      <span className="pending">
        <LoaderCircle className="node-icon spinning" />
        worktrees: asking git…
      </span>
    );
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
    <td className="ci" data-testid={`ci-${name}`}>
      {state === undefined ? (
        <span className="pending">{reading ? "reading…" : "not read"}</span>
      ) : (
        <CiWords state={state} />
      )}
    </td>
  );
}

/**
 * The mark for each of the seven words a pipeline may be in.
 *
 * `CI_STATES` in `crates/charter-core/src/cistate.rs` is the closed list — both forges map
 * their own vocabulary onto it — so this is exhaustive rather than a guess, and anything the
 * cache holds that is not one of the seven gets the dashed circle, which is what charter
 * already draws for "there is a fetch here and it names nothing".
 *
 * **`running` and `pending` are the ones that move.** That is the whole of the operator's
 * "show pipelines with animation": those two are the states where nothing else on the row
 * distinguishes *this is happening now* from *this stopped and nobody said so* — amber and
 * the word "running" are equally true of a job that died an hour ago. Every other mark is a
 * settled answer and sits still, because motion beside a settled answer is only something to
 * look at.
 *
 * **And the two move differently, because they are different claims** (M7.2). `running` spins:
 * work is being done. `pending` is a clock that breathes: the run is alive and queued, and
 * nothing is being done yet. Both used to spin, which drew a queued pipeline as a working one —
 * the same lie the paragraph above is about, told the other way round.
 *
 * **A mark that arrives at an answer settles into it, once.** A pipeline that finishes while
 * the operator is looking has its new mark drawn in over `duration.settle`; one that was already
 * finished when the row was drawn is simply there (`useArrived`). Every one of these motions is
 * a theme token, and every one of them stops under `prefers-reduced-motion` in the one place
 * the motion layer handles it (`src/theme/motion.ts`); the word and the shape do not.
 */
const CI_MARK: Record<string, { Mark: typeof CircleCheck; moving?: "spinning" | "breathing" }> = {
  success: { Mark: CircleCheck },
  failed: { Mark: CircleX },
  running: { Mark: LoaderCircle, moving: "spinning" },
  pending: { Mark: Clock, moving: "breathing" },
  manual: { Mark: Hand },
  canceled: { Mark: CircleSlash },
  skipped: { Mark: SkipForward },
};

function CiWords({ state }: { state: RepoState }) {
  const arrived = useArrived(state.ci);
  if (state.ci) {
    const { Mark, moving } = CI_MARK[state.ci] ?? { Mark: CircleDashed };
    const motion = moving ?? (arrived ? "settling" : undefined);
    return (
      <span className={`ci-state ci-${state.ci}`}>
        <Mark className={motion ? `node-icon ${motion}` : "node-icon"} />
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
  return (
    <span className="none">
      <CircleDashed className="node-icon" />
      no pipeline recorded · {ago(state.fetched_seconds_ago)}
    </span>
  );
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
