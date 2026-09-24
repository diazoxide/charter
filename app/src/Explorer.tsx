import { useState, type KeyboardEvent, type ReactNode } from "react";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import {
  ChevronRight,
  FolderGit2,
  Folders,
  FolderX,
  GitBranch,
  LoaderCircle,
  SquareTerminal,
  TriangleAlert,
} from "lucide-react";
import type { OpenChat } from "./bindings";
import { ChatState } from "./NeedsYou";
import { Menued } from "./Menus";
import { WorktreeMark } from "./Worktree";
import { stateOf, type ChatStates } from "./chatState";
import type { Catalogued, Offer } from "./actions";
import type { WorkspaceState } from "./workspaceState";
import { useTabStop } from "./roving";

/**
 * The left region: the repo and worktree **explorer** (ADR 0038).
 *
 * **It replaces the workspace listing, it does not extend it.** What used to be here was
 * every workspace with its vision text and its chats — the axis ADR 0036 had just given the
 * strip above, answered a second time in more words. That duplication is what made the
 * operator ask what the left sidebar was for.
 *
 * What the left gets instead is the level the three strips do not reach. A workspace holds
 * several clones and several worktrees, and until now nothing in the window selected one.
 *
 * **Picking something here decides where the next chat starts**, which is what makes this a
 * selector rather than a second listing. `New tab` starts in the focused workspace's own
 * directory until a piece is picked, and in that piece afterwards. Nothing is started by
 * picking: the picker still asks which profile and which persona, and the core still decides
 * whether that directory can be started in — it writes the harness layer there or refuses
 * with a sentence naming what stopped it.
 *
 * **A clone is a heading, and it is picked from its menu rather than by a click.** A click on
 * the heading opens and closes it, which is what a heading does; the pick is `Start new chats
 * in <repo>` on the clone's context menu, beside `New tab in <repo>` (charter-app#174). Both
 * carry the path the core spelled (`Panels.paths`), so nothing here joins one together, and a
 * picked clone is marked the way a picked piece is.
 *
 * **Every row that is a place has a context menu, and each is the catalogue filtered to that
 * place** (charter-app#174). A piece's is merge above the line and remove below it; a clone's
 * is the two rows above and nothing below, because nothing in this window writes to a clone.
 * Nothing here says what those rows mean — `Menus.tsx` draws whatever `actions.ts` has, and
 * Shift+F10 or the menu key opens the same menu on the row the arrows are on.
 *
 * **It is drawn as a tree, and the lines are drawn by the rows rather than by the lists.**
 * The nesting was always here — workspace, clone, piece, chat — and nothing said so: four
 * levels of `padding-inline-start` and no line to follow, which is what the operator meant by
 * *"trees are not looking like tree"*. Each row draws its own vertical segment and its own
 * elbow (`App.css`, `.explorer .pieces > li::before`), so the last row's segment simply stops
 * at the elbow. The usual trick — a line on the list, masked at the bottom by a rectangle in
 * the background colour — cannot be used here, because **a region moves** (ADR 0038): the
 * explorer put in the bottom slot sits on `surface.deep`, and a mask painted in `surface.base`
 * would be a visible block. A row that draws its own line has no background to know.
 *
 * **A row never folds, and the region scrolls sideways instead.** The operator's words:
 * *"all trees components texts should not be breakable to new line — it should be horizontal
 * scrollable."* A row names one thing, and a name broken across two lines takes the tree with
 * it — the guides, the indent and the eye all read down a column of first lines. The rule is
 * in `App.css` beside the guides, because it had to be written without touching the padding
 * and the font size those elbows are tuned to. Sentences charter says about a FAILURE still
 * wrap; they are not rows. Neither half of this can be asserted in jsdom, which lays nothing
 * out: `workspace-explorer.e2e.ts` measures the rows and the elbows in the real WebView.
 *
 * **It is a tree, the WAI-ARIA "Tree View" pattern, whole** (charter-app#238). #189 gave it
 * the half that needs no tree semantics; a screen reader was still told it was a list and a
 * `<details>`, and Left and Right did nothing. Now:
 *
 * - **`role="tree"`, and every row a `treeitem`** with its `aria-level` and its place among
 *   its siblings (`aria-posinset` / `aria-setsize`). Those are written down rather than left to
 *   the DOM, because the rows are the buttons and a button cannot hold its children: the
 *   nesting a screen reader would otherwise infer runs through lists, `<details>` and wrappers
 *   that are there for the guides. {@link treeOf} is the one place the shape is decided.
 * - **`aria-expanded` on every row with children**, as the pattern asks of a parent: a clone
 *   says whether it is open, and the workspace row and a worktree with chats working in it say
 *   `true`, because they are parents that are always open. A leaf says nothing.
 * - **Right** opens a closed clone, or moves to a row's first child. **Left** closes an open
 *   clone, or moves to the row's parent — which is also what it does on a parent that cannot
 *   close. The fold is the same state a click on the clone's heading changes.
 * - **Up, Down, Home and End** are the roving focus's, as since #189: ONE Tab stop, through
 *   `roving.ts`, and the stop at rest is the current row — the picked worktree, or the
 *   workspace itself.
 * - **Type-ahead**: a printable key moves to the next row whose name starts with it, wrapping.
 *   One key and not a typed prefix — the names are short and few, and cycling on a repeated
 *   key finds any of them.
 * - **Enter** does what a click does: it picks a worktree, brings a chat forward, and on a
 *   clone's heading opens or closes it, which `<summary>` does natively.
 *
 * Left and Right are taken from the region's sideways scroll while a row has the keyboard. A
 * focused row is scrolled into view by the engine, so nothing the keyboard can reach is lost.
 */
export function Explorer({
  workspace,
  state,
  chats,
  states,
  spot,
  onPick,
  onShowChat,
  offers,
  onPress,
}: {
  /** The focused workspace, or nothing when the strip is on the chats that are in none. */
  workspace: string | undefined;
  state: WorkspaceState;
  /** The chats working in this workspace, so a piece can say what is already running in it. */
  chats: readonly OpenChat[];
  states: ChatStates;
  /** The piece picked, or nothing for the workspace's own directory. */
  spot: Spot | undefined;
  onPick: (spot: Spot | undefined) => void;
  onShowChat: (session: number) => void;
  /** The catalogue by id, which is what a piece row's menu is drawn out of. */
  offers: Catalogued;
  onPress: (offer: Offer) => void;
}) {
  /** The clones the operator folded, by workspace and name: a row inside one is not drawn, so
   *  it cannot be where the keyboard comes back in. */
  const [folded, setFolded] = useState<ReadonlySet<string>>(new Set());
  const tree = treeOf(workspace, state, chats, folded);
  const drawn = tree.filter((row) => row.drawn);
  const picked =
    spot === undefined
      ? ROOT
      : spot.piece === undefined
        ? cloneRow(spot.repo)
        : pieceRow(spot.repo, spot.piece);
  const stop = useTabStop(
    picked,
    drawn.map((row) => row.id),
  );

  /** Opens or closes a clone: the one fold state, whether a click or a key asked. */
  const fold = (key: string, open: boolean) =>
    setFolded((was) => {
      if (open === !was.has(key)) return was;
      const now = new Set(was);
      if (open) now.delete(key);
      else now.add(key);
      return now;
    });

  /** Whether a row is drawn. One inside a folded clone is still in the document, and the
   *  roving focus is told to pass it by: jsdom focuses it, and the arrows would stop on it. */
  const byId = new Map(tree.map((row) => [row.id, row]));
  const isDrawn = (id: string) => byId.get(id)?.drawn ?? false;

  /** What a row says to a screen reader about where it is in the tree. */
  const treeitem = (id: string): TreeItem => {
    const row = byId.get(id);
    return {
      role: "treeitem",
      "aria-level": row?.level,
      "aria-posinset": row?.posinset,
      "aria-setsize": row?.setsize,
      "aria-expanded": row?.fold?.open ?? (row?.parents ? true : undefined),
      "data-row": id,
    };
  };

  /** Left, Right and type-ahead (#238). Up, Down, Home and End are the roving focus's. */
  const onTreeKey = (event: KeyboardEvent<HTMLElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const target = event.target as HTMLElement;
    const to = treeKey(drawn, target.dataset.row, event.key);
    if (to === "not-mine") return;
    event.preventDefault();
    if (to === "stay") return;
    if ("fold" in to) fold(to.fold, to.open);
    else
      [...event.currentTarget.querySelectorAll<HTMLElement>("[data-row]")]
        .find((el) => el.dataset.row === to.focus)
        ?.focus();
  };

  if (workspace === undefined) {
    return (
      // Inside the same roving group as the full explorer below, so that the `nav` is the SAME
      // element when a workspace arrives: a different parent would have React remount it, and
      // everything holding the old one — a scenario, a screen reader's place — would lose it.
      <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
        <nav className="explorer" aria-label="Explorer" data-testid="explorer">
          {/* The strip for chats outside every workspace is not a workspace on the plane, so
            there is no directory to explore and nothing honest to draw. */}
          <p className="empty">No workspace focused, so there is nothing to explore.</p>
        </nav>
      </RovingFocusGroup.Root>
    );
  }

  const { panels, pieces, piecesRefused } = state;
  const clones = panels?.repos ?? [];
  // The chats that are in no piece of this workspace: they work in the workspace itself or
  // in a clone, and they are listed under the workspace row rather than dropped. `treeOf`
  // decided which they are, and the render reads it rather than deciding a second time.
  const atTheRoot = chats.filter((chat) => byId.get(chatRow(chat.session))?.parent === ROOT);

  return (
    <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
      <nav className="explorer" aria-label="Explorer" data-testid="explorer">
        {state.trouble && <Trouble>{state.trouble}</Trouble>}

        {/* The tree is the rows and what holds them. The sentences about the whole region — the
          trouble above, the pending and empty notes and what charter would not read below —
          are outside it. The ones about ONE clone (its worktrees could not be listed, are
          still coming, or are none) stay inside that clone's `<details>`, beside the row they
          explain, and so inside the tree: moving them out would take them away from it. */}
        <div role="tree" aria-label="Repos and worktrees" onKeyDown={onTreeKey}>
          <RovingFocusGroup.Item asChild tabStopId={ROOT} active={spot === undefined}>
            <button
              type="button"
              className="spot spot-root"
              {...treeitem(ROOT)}
              // Not `aria-selected`, even in a tree: the three tablists in this window are
              // where it selects (ADR 0036), and a picked spot is not a selection the keyboard
              // moves but the place the next chat starts — the current item, which is what
              // `aria-current` is for and what the old sidebar's workspace rows used.
              aria-current={spot === undefined ? "true" : undefined}
              onClick={() => onPick(undefined)}
            >
              <Folders className="node-icon" />
              <span className="spot-name">{workspace}</span>
              <span className="spot-what">the workspace itself</span>
            </button>
          </RovingFocusGroup.Item>
          <ChatList
            chats={atTheRoot}
            states={states}
            onShow={onShowChat}
            treeitem={treeitem}
            isDrawn={isDrawn}
          />

          {clones.length > 0 && (
            // The clones are the workspace row's children, and the wrapper is what lets them be
            // drawn as such — the tree lines hang off it, one level in from the root row.
            <div className="clones" role="group">
              {clones.map((repo) => (
                // `<details>` and not a primitive: the browser has a collapsible and
                // `docs/ui-primitives.md` says native HTML that already does the job is not what
                // the Radix rule is about. Open by default — a closed explorer explores nothing.
                <details
                  className="clone"
                  key={repo}
                  data-testid={`clone-${repo}`}
                  // Held by the fold state rather than by the element, so that Left and Right
                  // (#238) open and close it through the same state a click on the heading does.
                  open={!folded.has(foldKey(workspace, repo))}
                  onToggle={(event) => fold(foldKey(workspace, repo), event.currentTarget.open)}
                >
                  {/* The clone's menu: a new tab in it, and picking it as where new chats
                    start (charter-app#174). On the heading, for the piece rows' reason — the
                    `<details>` also holds every row inside the clone. */}
                  <Menued on={{ on: "clone", repo }} offers={offers} onPress={onPress}>
                    <RovingFocusGroup.Item
                      asChild
                      tabStopId={cloneRow(repo)}
                      active={picked === cloneRow(repo)}
                    >
                      <summary
                        {...treeitem(cloneRow(repo))}
                        aria-current={picked === cloneRow(repo) ? "true" : undefined}
                      >
                        {/* The twisty says which way the disclosure goes, which the default marker
                    said in the platform's own glyph at the platform's own size. It turns
                    with `[open]`, and the turn is the one motion here that is a direct
                    answer to a click — `prefers-reduced-motion` stops it all the same. */}
                        <ChevronRight className="twisty" />
                        <FolderGit2 className="node-icon" />
                        <span className="repo">{repo}</span>
                        <PieceCount pieces={pieces[repo]} refused={piecesRefused[repo]} />
                      </summary>
                    </RovingFocusGroup.Item>
                  </Menued>
                  {piecesRefused[repo] ? (
                    // Said, never swallowed: a clone with no rows otherwise reads as a clone
                    // nobody has cut a worktree in.
                    <Trouble>
                      charter could not list the worktrees of <code>{repo}</code>:{" "}
                      {piecesRefused[repo]}
                    </Trouble>
                  ) : pieces[repo] === undefined ? (
                    <Pending>Asking git…</Pending>
                  ) : pieces[repo].length === 0 ? (
                    <p className="none">No worktrees cut here</p>
                  ) : (
                    <ul className="pieces" role="group">
                      {pieces[repo].map((piece) => {
                        const working = chats.filter((chat) => under(chat.cwd, piece.path));
                        const isPicked = spot?.repo === repo && spot.piece === piece.piece;
                        return (
                          <li
                            key={piece.piece}
                            role="none"
                            data-testid={`piece-${repo}-${piece.piece}`}
                          >
                            {/* **Right-click is what these rows were missing** (charter-app#174).
                            The menu is the catalogue filtered to this piece — merge above the
                            line, remove below it, and the discard row that only exists while
                            the core has refused THIS removal. Nothing here says what those
                            rows mean; `Menus.tsx` draws whatever `actions.ts` has.

                            On the button and not on the `<li>`: the `<li>` also holds the
                            chats running in this piece, and each of those is its own row with
                            its own identity. `asChild`, so the row gains no element. */}
                            <Menued
                              on={{ on: "worktree", repo, piece: piece.piece }}
                              offers={offers}
                              onPress={onPress}
                            >
                              <RovingFocusGroup.Item
                                asChild
                                tabStopId={pieceRow(repo, piece.piece)}
                                active={isPicked}
                                focusable={isDrawn(pieceRow(repo, piece.piece))}
                              >
                                <button
                                  type="button"
                                  className="spot"
                                  aria-current={isPicked ? "true" : undefined}
                                  {...treeitem(pieceRow(repo, piece.piece))}
                                  // The whole path, because two clones in one workspace can hold a
                                  // piece of the same name and the row has room for one word.
                                  title={piece.path}
                                  onClick={() =>
                                    onPick({ repo, piece: piece.piece, path: piece.path })
                                  }
                                >
                                  <GitBranch className="node-icon" />
                                  <span className="spot-name">{piece.piece}</span>
                                </button>
                              </RovingFocusGroup.Item>
                            </Menued>
                            {/* The branch, and the two states the operator has to see BEFORE they
                          start a chat in a tree: `unwired` and `stale`. The same component
                          the palette's worktree rows are written against. */}
                            <WorktreeMark
                              worktree={{
                                workspace,
                                repo,
                                piece: piece.piece,
                                branch: piece.branch,
                                wired: piece.wired,
                                stale: piece.stale,
                              }}
                            />
                            <ChatList
                              chats={working}
                              states={states}
                              onShow={onShowChat}
                              treeitem={treeitem}
                              isDrawn={isDrawn}
                            />
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </details>
              ))}
            </div>
          )}
        </div>

        {panels === undefined ? (
          <Pending>Reading the plane…</Pending>
        ) : (
          clones.length === 0 && <p className="none">No repos in this workspace</p>
        )}

        {(panels?.absent.length ?? 0) > 0 && (
          <section className="absent" data-testid="absent">
            <h2>Not cloned here</h2>
            <ul>
              {panels?.absent.map((name) => (
                // Membership without a clone. There is nothing to explore in it and nothing
                // to start a chat in, so it is named and not made a heading.
                <li key={name}>
                  <FolderX className="node-icon" />
                  {name}
                </li>
              ))}
            </ul>
          </section>
        )}

        {panels?.refused.map(([name, why]) => (
          <Trouble key={`refused-${name}`}>
            charter will not read <code>{name}</code>: {why}
          </Trouble>
        ))}
      </nav>
    </RovingFocusGroup.Root>
  );
}

/** A refusal, with the mark that says it is one.
 *
 *  The icon is decorative and Lucide hides it from a screen reader by itself (it adds
 *  `aria-hidden` to any icon given no accessible name of its own), so what an assistive
 *  technology gets is the alert and its sentence, exactly as before. */
function Trouble({ children }: { children: ReactNode }) {
  return (
    <p className="trouble" role="alert">
      <TriangleAlert className="node-icon" />
      <span>{children}</span>
    </p>
  );
}

/** Something charter is still reading.
 *
 *  **The one place in this region an animation earns its place.** A spinner here means "this
 *  is still happening", which is a state no colour and no word can distinguish from "this
 *  stopped and nothing came back" — the reason the operator asked for motion on the pipelines.
 *  It is a state that ends, and it is drawn at most twice. `prefers-reduced-motion` stops the
 *  spin and leaves the mark. */
function Pending({ children }: { children: ReactNode }) {
  return (
    <p className="pending">
      <LoaderCircle className="node-icon spinning" />
      <span>{children}</span>
    </p>
  );
}

/** Where the next chat starts, when it is not the workspace's own directory: a piece, or — with
 *  no `piece` — the clone itself, picked from its menu (charter-app#174).
 *
 *  It carries the path the CORE spelled — `worktree_list` answers with a piece's, and
 *  `Panels.paths` with a clone's — so nothing here ever joins one together. */
export type Spot = { repo: string; piece?: string; path: string };

/** How many pieces a clone has, on its heading, so a closed one still says whether there is
 *  anything in it. */
function PieceCount({ pieces, refused }: { pieces?: readonly unknown[]; refused?: string }) {
  if (refused !== undefined) return <span className="piece-count none">unreadable</span>;
  if (pieces === undefined) return <span className="piece-count pending">…</span>;
  return (
    <span className="piece-count" aria-label={`${pieces.length} worktrees`}>
      {pieces.length}
    </span>
  );
}

/** The chats working at a spot, each a button that brings its tab to the front.
 *
 *  This is not the sidebar's old listing coming back: it is every chat in ONE place, under
 *  the place, which is what answers "is anything already running in this worktree" before
 *  the operator starts a second one in it. */
function ChatList({
  chats,
  states,
  onShow,
  treeitem,
  isDrawn,
}: {
  chats: readonly OpenChat[];
  states: ChatStates;
  onShow: (session: number) => void;
  /** What a row says about its place in the tree. */
  treeitem: (id: string) => TreeItem;
  /** Whether a row is drawn, and so whether the arrows stop on it. */
  isDrawn: (id: string) => boolean;
}) {
  if (chats.length === 0) return null;
  return (
    <ul className="here" role="group">
      {chats.map((chat) => (
        <li key={chat.session} role="none">
          <RovingFocusGroup.Item
            asChild
            tabStopId={chatRow(chat.session)}
            focusable={isDrawn(chatRow(chat.session))}
          >
            <button
              type="button"
              className="chat"
              {...treeitem(chatRow(chat.session))}
              onClick={() => onShow(chat.session)}
            >
              {/* What tells a chat leaf from a worktree leaf at a glance. The tree has two
                kinds of leaf under one kind of parent, and at fifty chats the indent alone
                stopped being enough to tell them apart. */}
              <SquareTerminal className="node-icon" />
              <span className="session">{chat.name}</span>
              <ChatState state={stateOf(states, chat.session)} />
              {/* The PROFILE where there is one, and the harness otherwise. A profile is what
                the operator picked and what a relaunch looks up again; the kind is what the
                plane calls the harness. Showing the profile alone would hide which harness
                it runs, and showing the kind alone would hide which account. */}
              {chat.profile ? (
                <span className="harness">
                  {chat.profile}
                  {chat.harness && <span className="kind"> ({chat.harness})</span>}
                </span>
              ) : (
                chat.harness && <span className="harness">{chat.harness}</span>
              )}
              {chat.persona && <span className="persona">{chat.persona}</span>}
            </button>
          </RovingFocusGroup.Item>
          {/* What its harness cannot tell charter, on the chat itself (#27). A Codex chat
              reads `unknown` until its first prompt and never says it is waiting on an
              approval; without this it looks like charter is broken. */}
          {chat.unreported && <p className="unreported">{chat.unreported}</p>}
        </li>
      ))}
    </ul>
  );
}

/** Whether a chat's directory is this piece's, or inside it.
 *
 *  By path components and never by string prefix: `…/piece-two` starts with `…/piece` and is
 *  a different tree. */
function under(cwd: string | null, path: string): boolean {
  if (cwd === null) return false;
  if (cwd === path) return true;
  return cwd.startsWith(`${path}/`) || cwd.startsWith(`${path}\\`);
}

/** The workspace's own row, as a stop in the explorer's roving focus. */
const ROOT = "root";
const chatRow = (session: number) => `chat:${session}`;
const cloneRow = (repo: string) => `clone:${repo}`;
const pieceRow = (repo: string, piece: string) => `piece:${repo}/${piece}`;
/** A folded clone, by workspace as well as name: two workspaces can each clone `svc`. */
const foldKey = (workspace: string, repo: string) => `${workspace}/${repo}`;

/** One row of the tree, as the keyboard and a screen reader know it (#238). */
type Row = {
  id: string;
  /** What type-ahead matches: the row's own name, which is its first word on screen. */
  name: string;
  level: number;
  parent: string | undefined;
  /** Its place among its siblings, from 1, and how many siblings there are. */
  posinset: number;
  setsize: number;
  /** A clone's fold, by {@link foldKey}, and whether it is open. Nothing on a row that cannot
   *  fold. */
  fold?: { key: string; open: boolean };
  /** Whether it is drawn: a row inside a folded clone is not, so it cannot be the stop and
   *  no key moves to it. */
  drawn: boolean;
  /** Whether it has children, drawn or not: a parent says `aria-expanded`, a leaf does not. */
  parents: boolean;
};

/** The attributes a row carries as a `treeitem`, and the `data-row` the keys find it by. */
type TreeItem = {
  role: "treeitem";
  "aria-level"?: number;
  "aria-posinset"?: number;
  "aria-setsize"?: number;
  "aria-expanded"?: boolean;
  "data-row": string;
};

/**
 * Every row of the explorer's tree, in the order it is drawn — for `useTabStop`, for the
 * attributes a screen reader reads and for the keys that move.
 *
 * **The same walk the render does, and it has to stay so**: a row missing here can never be
 * the stop and says nothing about where it is, and a row listed as drawn that is not could be
 * the only stop, which would leave the explorer with none at all.
 *
 * The shape: the workspace, then the chats in no worktree and the clones as its children, the
 * worktrees as a clone's, and the chats working in a worktree as its. A clone whose worktrees
 * could not be listed has none drawn.
 */
function treeOf(
  workspace: string | undefined,
  state: WorkspaceState,
  chats: readonly OpenChat[],
  folded: ReadonlySet<string>,
): Row[] {
  if (workspace === undefined) return [];
  /** `shows` is whether its children are drawn when it is: not in a folded clone, and not in
   *  one whose worktrees could not be listed — the render says why instead. */
  type Node = { id: string; name: string; fold?: Row["fold"]; kids: Node[]; shows: boolean };
  const { panels, pieces, piecesRefused } = state;
  const inAPiece = new Set<number>();
  const chatNode = (chat: OpenChat): Node => ({
    id: chatRow(chat.session),
    name: chat.name,
    kids: [],
    shows: true,
  });
  const clones = (panels?.repos ?? []).map((repo): Node => {
    const kids = (pieces[repo] ?? []).map((piece): Node => {
      const working = chats.filter((chat) => under(chat.cwd, piece.path));
      for (const chat of working) inAPiece.add(chat.session);
      return {
        id: pieceRow(repo, piece.piece),
        name: piece.piece,
        kids: working.map(chatNode),
        shows: true,
      };
    });
    const key = foldKey(workspace, repo);
    const open = !folded.has(key);
    return {
      id: cloneRow(repo),
      name: repo,
      fold: { key, open },
      kids,
      shows: open && !piecesRefused[repo],
    };
  });
  const root: Node = {
    id: ROOT,
    name: workspace,
    kids: [...chats.filter((chat) => !inAPiece.has(chat.session)).map(chatNode), ...clones],
    shows: true,
  };

  const rows: Row[] = [];
  const walk = (node: Node, parent: Row | undefined, drawn: boolean, at: number, of: number) => {
    const row: Row = {
      id: node.id,
      name: node.name,
      level: (parent?.level ?? 0) + 1,
      parent: parent?.id,
      posinset: at + 1,
      setsize: of,
      fold: node.fold,
      drawn,
      parents: node.kids.length > 0,
    };
    rows.push(row);
    node.kids.forEach((kid, i) => walk(kid, row, drawn && node.shows, i, node.kids.length));
  };
  walk(root, undefined, true, 0, 1);
  return rows;
}

/** What a key does to the tree: move to a row, open or close a clone, `stay` — a key the tree
 *  takes and has nothing to do with here, such as Right on a leaf — or `not-mine`, a key the
 *  tree leaves to whoever else wants it. */
type TreeMove = { focus: string } | { fold: string; open: boolean } | "stay" | "not-mine";

/**
 * What a key does on a row of the tree, by the WAI-ARIA "Tree View" pattern — or nothing, for
 * a key the tree leaves alone.
 *
 * @param drawn The rows drawn, in order.
 * @param from The row the key was pressed on.
 */
function treeKey(drawn: readonly Row[], from: string | undefined, key: string): TreeMove {
  const at = drawn.findIndex((row) => row.id === from);
  if (at < 0) return "not-mine";
  const row = drawn[at];
  if (key === "ArrowRight") {
    if (row.fold && !row.fold.open) return { fold: row.fold.key, open: true };
    // The next row drawn is the first child exactly when its parent is this one.
    const child = drawn[at + 1];
    return child?.parent === row.id ? { focus: child.id } : "stay";
  }
  if (key === "ArrowLeft") {
    if (row.fold?.open) return { fold: row.fold.key, open: false };
    return row.parent === undefined ? "stay" : { focus: row.parent };
  }
  // Type-ahead: one printable character, and never Space, which is a button's own.
  if (!/^\S$/u.test(key)) return "not-mine";
  const wanted = key.toLocaleLowerCase();
  for (let step = 1; step < drawn.length; step++) {
    const next = drawn[(at + step) % drawn.length];
    if (next.name.toLocaleLowerCase().startsWith(wanted)) return { focus: next.id };
  }
  return "stay";
}
