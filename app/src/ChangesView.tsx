import type { KeyboardEvent, ReactNode } from "react";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import {
  CircleCheck,
  CircleDashed,
  CircleSlash,
  CircleX,
  Clock,
  FileDiff,
  FolderGit2,
  GitBranch,
  GitFork,
  Hand,
  LoaderCircle,
  Puzzle,
  SkipForward,
  TriangleAlert,
} from "lucide-react";
import type { FactCell, FactColumn, Piece, RepoState } from "./bindings";
import { EmptyState } from "./EmptyState";
import { treeKey, type Row } from "./Explorer";
import { Menued } from "./Menus";
import type { Catalogued, Offer } from "./actions";
import type { WorkspaceState } from "./workspaceState";
import type { Cloning } from "./NotCloned";
import type { CloneState } from "./repoClones";
import { useArrived } from "./lib/arrived";
import { useTabStop } from "./roving";

/**
 * The Changes view: what the focused workspace's repos are doing (ADR 0038 as amended
 * 2026-10-10, #1676).
 *
 * Repo git state, worktrees and pipelines, which used to be two sections of the right-hand
 * side. They moved because the right-hand side is what is asking for you and this is not: it
 * is what is true, and nothing in it can be pressed. **They were the bottom region until
 * #1676** (B-7): the bottom took height from the terminals, so it became a view on the left
 * side's activity bar, beside Chats, Explorer and Search, and the bottom of the window is the
 * terminals'. Its tab counts the files git has uncommitted ({@link uncommitted}).
 *
 * ## It is a tree, an editor's source-control view (#1701)
 *
 * In a side the bottom bar's table scrolled sideways past every column it had. So each repo is
 * a heading — the repo and the branch its checkout is on — and what is true of it are rows
 * under it, in the order a person asks: what is uncommitted, which branches are cut off it (and
 * under that row, each branch), and what its pipeline last said; then a row per column an
 * extension adds. Drawn with the window's one tree style (`.tree`, #1672): a level is a
 * `role="group"` whose guide is a straight line, so the worktrees under a clone lost the bottom
 * bar's elbows (#1682). Nothing folds: a fold is a press, and every row is always drawn.
 *
 * **A row that names something is one line**, the explorer's rule, asked of both sides by the
 * operator: the view scrolls sideways rather than fold a name. A row that holds a SENTENCE — a
 * tree purlis could not read, a pipeline nobody fetched — still wraps, because held on one line
 * it would push the scroll out past everything else. `regions.e2e.ts` holds both halves, in a
 * real WebView, because jsdom lays nothing out. The one-line rule is Tailwind's
 * `whitespace-nowrap` on the row, from the utilities layer, so the sentence's own rule in
 * `App.css` still wins on the sentence.
 *
 * ## The keyboard reads it; nothing in it is pressed
 *
 * **One Tab stop, and the tree's keys** (#1701): the WAI-ARIA "Tree View" pattern the explorer
 * is — Up, Down, Home and End from the roving focus (`roving.ts`), Right into a row's first
 * child, Left back to its parent, and type-ahead (`Explorer.treeKey`). The stop at rest is the
 * first row, which is where ⌃⇧G puts the keyboard (`giveViewTheKeyboard` lands on a tree's
 * stop). A row the keyboard is on is read, never run: Enter and Space do nothing here.
 *
 * **Read-only, and that is asserted rather than described** — `regions.e2e.ts` presses on
 * every control in here and expects to find none. A focusable row is not a control: it is what
 * lets the view be read and scrolled by keys. It is the one half of ADR 0038's reading ("the
 * bottom is where you read what is true and do not touch it", where "the bottom" is now this
 * view) that a test can hold.
 *
 * **A repo's rows have a context menu, and a menu is not a control** (charter-app#174). It is
 * the explorer's clone menu — `New tab in <repo>` and `Start new chats in <repo>` — drawn from
 * the same catalogue rows, now that the core says where a clone is (`Panels.paths`). It is on
 * the heading and on each row that is about the clone (its changes, its branches' count, its
 * pipeline, an extension's value); the menu is drawn in a portal outside this region, and none
 * of its rows touches the repo the region is reading: they are about where the next chat
 * starts. The keyboard opens it on the row it is on (Shift+F10, `Menus.openFromTheKeyboard`).
 *
 * **The worktree rows under each repo still have none, and that is a decision.** Their verbs
 * exist — `worktree.merge:<repo>/<piece>` and `worktree.remove:<repo>/<piece>`, drawn on the
 * explorer's rows — but putting `Remove worktree` under the pointer in the region ADR 0038
 * says is for reading is an amendment to ADR 0038, argued on its own, and not a defect fix.
 * A right-click there answers with nothing rather than with the browser's own menu, which
 * `useNoBrowserMenu` covers for the whole window.
 *
 * **A repo the workspace names and nobody cloned here has the explorer's Clone row as its menu**
 * (#1215), by the same argument as a clone's: a menu, drawn in a portal, and a clone adds a
 * folder beside the others without touching any repo this region reads. Its heading says what
 * became of the clone — under way, or failed in the core's words — as text, never a control.
 *
 * **Nothing here waits on a network.** The pipeline row is what a forge refresher last wrote
 * into `.charter/cache/glstate.json`; charter-app reads that file and never fetches. A row with
 * nothing to show says why, because a blank one reads as "green" to a person in a hurry.
 *
 * **The worktrees are a count and a list of branches, not the explorer's rows again.** They
 * are in two regions — ADR 0038 names that as the visible crack in its own rule — and the
 * least dishonest way to have them in both is to make each answer its own question: on the
 * left a piece is a thing you pick, here it is a branch that exists and may be unwired.
 */
export function ChangesView({
  workspace,
  state,
  offers,
  onPress,
  columns = [],
  cloning = new Map(),
}: {
  workspace: string | undefined;
  state: WorkspaceState;
  /**
   * The columns the extensions on in this project and workspace add (charter-app#340), filled
   * per repo from each one's facts file by the core — `extension_facts`, which never starts a
   * program. A row under each repo the file names: a value, never a control.
   */
  columns?: readonly FactColumn[];
  /** The catalogue by id, which is what a repo row's menu is drawn out of. */
  offers: Catalogued;
  onPress: (offer: Offer) => void;
  /** What this window is cloning into the workspace (#1215): a repo not cloned here says
   *  what became of its clone, in words, never as a control. */
  cloning?: Cloning;
}) {
  const { panels, repos, pieces, piecesRefused, reading, trouble } = state;
  const names = panels?.repos ?? [];
  const absent = panels?.absent ?? [];
  const rows = workspace === undefined ? [] : treeRows(names, absent, pieces, columns);
  const stop = useTabStop(
    undefined,
    rows.map((one) => one.id),
  );

  if (workspace === undefined) {
    return (
      <section className="state-bar" aria-label="Repository state" data-testid="changes-view">
        <EmptyState
          headline="No workspace focused"
          body="Focus a workspace, and the branch, changes and pipeline of each of its repos are listed here."
          mark={GitFork}
          size="panel"
          testid="changes-empty"
        />
      </section>
    );
  }

  const byName = new Map((repos?.repos ?? []).map((repo) => [repo.name, repo]));
  const byId = new Map(rows.map((one) => [one.id, one]));
  const item = (id: string) => {
    const one = byId.get(id);
    return {
      role: "treeitem" as const,
      "aria-level": one?.level,
      "aria-posinset": one?.posinset,
      "aria-setsize": one?.setsize,
      "aria-expanded": one?.parents ? true : undefined,
      "data-row": id,
    };
  };
  const at: At = { item, offers, onPress };

  const onKey = (event: KeyboardEvent<HTMLElement>) => {
    if (menuKey(event)) {
      // The menu of the repo the row is in, opened where the row is: the same event a pointer
      // sends, from the row, so a worktree's row answers with nothing here too.
      event.preventDefault();
      const on = event.target as HTMLElement;
      const box = on.getBoundingClientRect();
      on.dispatchEvent(
        new MouseEvent("contextmenu", {
          bubbles: true,
          cancelable: true,
          clientX: box.left,
          clientY: box.bottom,
        }),
      );
      return;
    }
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const to = treeKey(rows, (event.target as HTMLElement).dataset.row, event.key);
    if (to === "not-mine") return;
    event.preventDefault();
    if (to === "stay" || !("focus" in to)) return;
    [...event.currentTarget.querySelectorAll<HTMLElement>("[data-row]")]
      .find((el) => el.dataset.row === to.focus)
      ?.focus();
  };

  return (
    <section className="state-bar" aria-label="Repository state" data-testid="changes-view">
      {trouble && (
        <Trouble offers={offers} onPress={onPress}>
          {trouble}
        </Trouble>
      )}
      {repos?.cache_refused && (
        <Trouble offers={offers} onPress={onPress}>
          {repos.cache_refused}
        </Trouble>
      )}

      {panels === undefined ? (
        <Pending>Reading the project…</Pending>
      ) : names.length === 0 && absent.length === 0 ? (
        <EmptyState
          headline="No repos in this workspace"
          body="Each repo the workspace names is listed here: the branch it is on, its changes, the branches cut off it and its pipeline."
          mark={FolderGit2}
          size="panel"
          testid="changes-empty"
        />
      ) : (
        <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
          <div
            className="tree"
            role="tree"
            aria-label={`Repos of ${workspace}`}
            data-testid="changes-tree"
            onKeyDown={onKey}
          >
            {names.map((name) => (
              <RepoRows
                key={name}
                name={name}
                state={byName.get(name)}
                pieces={pieces[name]}
                piecesRefused={piecesRefused[name]}
                reading={reading}
                columns={columns}
                at={at}
              />
            ))}
            {absent.map((name) => (
              // Its menu is the explorer's Clone row (#1215): a menu, not a control, so the
              // region stays unpressable. Membership without a clone is said, because a repo the
              // workspace means to hold and nobody has cloned is not the same as one that is not
              // listed — and, while one is under way, what became of the clone.
              <Menued
                key={`absent-${name}`}
                on={{ on: "absent", repo: name }}
                offers={offers}
                onPress={onPress}
              >
                <div data-testid={`repo-${name}`}>
                  <RovingFocusGroup.Item asChild tabStopId={absentRow(name)}>
                    <div className="repo-row absent whitespace-nowrap" {...item(absentRow(name))}>
                      <FolderGit2 className="node-icon" />
                      <span className="repo">{name}</span>{" "}
                      <span className="none">{absentSaid(cloning.get(name))}</span>
                    </div>
                  </RovingFocusGroup.Item>
                </div>
              </Menued>
            ))}
          </div>
        </RovingFocusGroup.Root>
      )}

      {/* A refusal is drawn, never swallowed: a row that is simply missing looks like a
          workspace with fewer repos than it has. */}
      {panels?.refused.map(([name, why]) => (
        <Trouble key={`refused-${name}`} offers={offers} onPress={onPress}>
          purlis will not read <code>{name}</code>: {why}
        </Trouble>
      ))}

      <p className="note">
        CI was last fetched by a refresher. charter-app reads this, never fetches it.
      </p>
    </section>
  );
}

/** Shift+F10, or the keyboard's own menu key: what opens a context menu from the keyboard
 *  (`Menus.openFromTheKeyboard`'s two keys). */
function menuKey(event: KeyboardEvent<HTMLElement>): boolean {
  return event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey);
}

/** What every row is drawn with: its tree attributes by id, and the clone menu's catalogue. */
type At = {
  item: (id: string) => {
    role: "treeitem";
    "aria-level"?: number;
    "aria-posinset"?: number;
    "aria-setsize"?: number;
    "aria-expanded"?: boolean;
    "data-row": string;
  };
  offers: Catalogued;
  onPress: (offer: Offer) => void;
};

const repoRow = (repo: string) => `repo:${repo}`;
const absentRow = (repo: string) => `absent:${repo}`;
const changesRow = (repo: string) => `changes:${repo}`;
const branchesRow = (repo: string) => `branches:${repo}`;
const pieceRow = (repo: string, piece: string) => `piece:${repo}/${piece}`;
const ciRow = (repo: string) => `ci:${repo}`;
const factRow = (repo: string, column: FactColumn) =>
  `fact:${repo}/${column.extension}/${column.id}`;

/**
 * **Every row of the tree, in the order it is drawn** — for the one Tab stop and for the keys
 * (`Explorer.treeKey`), which read a row's parent and the row after it. The drawing below
 * follows the same order; a test that walks the keys holds the two together.
 */
export function treeRows(
  names: readonly string[],
  absent: readonly string[],
  pieces: Record<string, Piece[]>,
  columns: readonly FactColumn[],
): Row[] {
  const rows: Row[] = [];
  const tops = names.length + absent.length;
  const add = (row: Omit<Row, "drawn">) => rows.push({ ...row, drawn: true });
  names.forEach((repo, n) => {
    const facts = columns.filter((column) => column.cells.some((cell) => cell.repo === repo));
    const cut = pieces[repo] ?? [];
    const size = 3 + facts.length;
    add({
      id: repoRow(repo),
      name: repo,
      level: 1,
      parent: undefined,
      posinset: n + 1,
      setsize: tops,
      parents: true,
    });
    const under = (id: string, name: string, place: number, parents = false) =>
      add({ id, name, level: 2, parent: repoRow(repo), posinset: place, setsize: size, parents });
    under(changesRow(repo), "changes", 1);
    under(branchesRow(repo), "branches", 2, cut.length > 0);
    cut.forEach((one, m) =>
      add({
        id: pieceRow(repo, one.piece),
        name: one.piece,
        level: 3,
        parent: branchesRow(repo),
        posinset: m + 1,
        setsize: cut.length,
        parents: false,
      }),
    );
    under(ciRow(repo), "pipeline", 3);
    facts.forEach((column, m) => under(factRow(repo, column), column.title, 4 + m));
  });
  absent.forEach((repo, n) =>
    add({
      id: absentRow(repo),
      name: repo,
      level: 1,
      parent: undefined,
      posinset: names.length + n + 1,
      setsize: tops,
      parents: false,
    }),
  );
  return rows;
}

/** What a repo that is not cloned here says in its row: that, or what its clone is doing. */
function absentSaid(clone: CloneState | undefined): string {
  switch (clone?.state) {
    case "waiting":
      return "waiting to be cloned";
    case "cloning":
      return "cloning…";
    case "cloned":
      return "cloned";
    case "failed":
      return `not cloned here — the clone failed: ${clone.said}`;
    default:
      return "not cloned here";
  }
}

/** A refusal, with the mark that says it is one. Lucide hides a nameless icon from a screen
 *  reader itself, so the alert reads exactly as it did before.
 *
 *  **Its menu is Read again** (#1244): the explorer's own row, so a refusal down here has its
 *  way out with the explorer hidden. A menu, not a control (charter-app#174), so the region
 *  stays unpressable. */
function Trouble({
  offers,
  onPress,
  children,
}: {
  offers: Catalogued;
  onPress: (offer: Offer) => void;
  children: ReactNode;
}) {
  return (
    <Menued on={{ on: "refusal" }} offers={offers} onPress={onPress}>
      <p className="trouble" role="alert">
        <TriangleAlert className="node-icon" />
        <span>{children}</span>
      </p>
    </Menued>
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

/** One row about a clone: a tree row, inside the clone's menu (see {@link RepoRows}). */
function CloneRow({
  id,
  at,
  className,
  testId,
  children,
}: {
  id: string;
  at: At;
  className: string;
  testId?: string;
  children: ReactNode;
}) {
  return (
    <RovingFocusGroup.Item asChild tabStopId={id}>
      <div className={`${className} whitespace-nowrap`} data-testid={testId} {...at.item(id)}>
        {children}
      </div>
    </RovingFocusGroup.Item>
  );
}

/** One repo: its heading, and under it what is true of it — in {@link treeRows}' order. */
function RepoRows({
  name,
  state,
  pieces,
  piecesRefused,
  reading,
  columns,
  at,
}: {
  name: string;
  state: RepoState | undefined;
  pieces: Piece[] | undefined;
  piecesRefused: string | undefined;
  reading: boolean;
  columns: readonly FactColumn[];
  at: At;
}) {
  const unread = reading ? "reading…" : "not read";
  const cut = pieces ?? [];
  return (
    // **The clone's menu is on the whole repo, heading and rows** (`asChild`, so the tree gains
    // no element): a right-click on any row about the clone opens it, and so does the keyboard
    // on one (`menuKey`). Its worktrees' rows stop the event, so they answer with nothing.
    <Menued on={{ on: "clone", repo: name }} offers={at.offers} onPress={at.onPress}>
      <div data-testid={`repo-${name}`}>
        <CloneRow id={repoRow(name)} at={at} className="repo-row">
          <FolderGit2 className="node-icon" />
          <span className="repo">{name}</span>
          {state !== undefined && state.unreadable === null && (
            <span className="branch">
              {" "}
              <GitBranch className="node-icon" />
              <span>{headOf(state)}</span>
              {state.upstream && <span className="upstream">{state.upstream}</span>}
              {gapOf(state) && <span className="gap">{gapOf(state)}</span>}
            </span>
          )}
        </CloneRow>
        <div role="group">
          <CloneRow id={changesRow(name)} at={at} className="changes">
            <FileDiff className="node-icon" />
            {state === undefined ? (
              <span className="pending">{unread}</span>
            ) : state.unreadable ? (
              // Never "clean". A tree charter could not read is the one thing this view must not
              // round down, because the round-down says everything is fine. A sentence, so it
              // wraps (`.branch.unreadable`).
              <span className="branch unreadable" role="alert">
                <TriangleAlert className="node-icon" />
                {state.unreadable}
              </span>
            ) : (
              <span className="dirt">{dirtOf(state)}</span>
            )}
          </CloneRow>
          <CloneRow
            id={branchesRow(name)}
            at={at}
            className="worktrees"
            testId={`worktrees-${name}`}
          >
            <GitBranch className="node-icon" />
            <Worktrees pieces={pieces} refused={piecesRefused} />
          </CloneRow>
          {cut.length > 0 && (
            // The branches' own level: the shared tree's group and its straight guide (#1682).
            // No menu on these rows (see the docstring): the event is stopped here, before the
            // clone's menu round the repo hears it.
            <div
              role="group"
              data-testid={`worktree-tree-${name}`}
              onContextMenu={(event) => event.preventDefault()}
            >
              {cut.map((piece) => (
                <RovingFocusGroup.Item
                  asChild
                  tabStopId={pieceRow(name, piece.piece)}
                  key={piece.piece}
                >
                  <div
                    className="piece whitespace-nowrap"
                    {...at.item(pieceRow(name, piece.piece))}
                  >
                    <GitBranch className="node-icon" />
                    <span className="piece-name">{piece.piece}</span>
                    {/* The branch only when it says something the name does not. charter cuts a
                      piece on a branch of its own name by default, and `perf perf` down a
                      whole column is the same word twice on every row. */}
                    {piece.branch && piece.branch !== piece.piece && (
                      <>
                        {" "}
                        <code className="branch">{piece.branch}</code>
                      </>
                    )}
                    {/* The same two states the count above totals, said here of the one tree
                      they are true of. A total answers "is anything wrong in this clone";
                      a row answers "which one". */}
                    {piece.stale ? (
                      <>
                        {" "}
                        <span className="label stale">stale</span>
                      </>
                    ) : (
                      !piece.wired && (
                        <>
                          {" "}
                          <span className="label unwired">unwired</span>
                        </>
                      )
                    )}
                  </div>
                </RovingFocusGroup.Item>
              ))}
            </div>
          )}
          <CloneRow id={ciRow(name)} at={at} className="ci" testId={`ci-${name}`}>
            {state === undefined ? (
              <span className="pending">{unread}</span>
            ) : (
              <CiWords state={state} />
            )}
          </CloneRow>
          {columns.map((column) => {
            const cell = column.cells.find((one) => one.repo === name);
            // A repo the facts file named no value for has no row — never another repo's value.
            if (cell === undefined) return null;
            return (
              <CloneRow
                key={`${column.extension}/${column.id}`}
                id={factRow(name, column)}
                at={at}
                className={cell.stale ? "fact stale" : "fact"}
                testId={`fact-${column.extension}-${column.id}-${name}`}
              >
                <FactOf column={column} cell={cell} />
              </CloneRow>
            );
          })}
        </div>
      </div>
    </Menued>
  );
}

/** One extension's value for one repo, named by its column. A stale value is dimmed and says
 *  how old it is. */
function FactOf({ column, cell }: { column: FactColumn; cell: FactCell }) {
  return (
    <>
      <Puzzle className="node-icon" />
      <span className="stamp" title={`From the extension ${column.extension}`}>
        {column.title}
      </span>{" "}
      {cell.value}
      {cell.stale && <span className="stamp"> · {ago(cell.age_seconds)}</span>}
    </>
  );
}

/** What git says about this clone's pieces, in one phrase. */
function Worktrees({ pieces, refused }: { pieces: Piece[] | undefined; refused?: string }) {
  // Never "no branches". A listing charter could not run says so, for the same reason an
  // unreadable tree is never drawn as clean.
  if (refused !== undefined) return <span className="none">branches unreadable</span>;
  if (pieces === undefined)
    return (
      <span className="pending">
        <LoaderCircle className="node-icon spinning" />
        branches: asking git…
      </span>
    );
  if (pieces.length === 0) return <span className="none">no branches</span>;
  const stale = pieces.filter((piece) => piece.stale).length;
  const unwired = pieces.filter((piece) => !piece.wired && !piece.stale).length;
  return (
    <>
      <span className="count">
        {pieces.length} {pieces.length === 1 ? "branch" : "branches"}
      </span>
      {/* The two states that change what starting a chat in one would mean. Counted here
          rather than listed: the row a person acts on is the explorer's. */}
      {unwired > 0 && <span className="label unwired"> {unwired} unwired</span>}
      {stale > 0 && <span className="label stale"> {stale} stale</span>}
    </>
  );
}

/**
 * The mark for each of the seven words a pipeline may be in.
 *
 * `CI_STATES` in `crates/purlis-core/src/cistate.rs` is the closed list — both forges map
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

/**
 * **The files git has uncommitted in the focused workspace's clones**: what the Changes tab
 * counts (B-8, #1676), changed and untracked alike, as {@link dirtOf} says them per repo. A
 * tree purlis could not read adds nothing: its counts are zero and mean "not known", and the
 * view says so on its row.
 */
export function uncommitted(state: WorkspaceState): number {
  return (state.repos?.repos ?? [])
    .filter((repo) => repo.unreadable === null)
    .reduce((sum, repo) => sum + repo.tracked + repo.untracked, 0);
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
