import type { ReactNode } from "react";
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
 * **A clone is a heading and not a leaf, and that is a gap rather than a decision.**
 * `workspace_panels` answers with clone NAMES; the only paths the window ever holds are ones
 * the core spelled, and a piece carries its own. Making the clone itself pickable needs the
 * core to say where it is, which is a change to `Panels` and to the generated bindings, and
 * it is not in this one.
 *
 * **A piece row has a context menu and a clone row does not, for that same gap**
 * (charter-app#174). A worktree is something charter can act on, and since #174 the rows that
 * act on one name the piece they mean, so a menu here is the catalogue filtered to this piece
 * — merge above the line, remove below it. A clone is not: nothing in `actions.ts` is about
 * one, because nothing this window can do is. A menu there would have to invent a verb, which
 * is the second list `actions.ts` opens by refusing to have.
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
 * **The DOM is unchanged; only the look is.** This is deliberately not `role="tree"`. A real
 * tree owes the keyboard arrow navigation, typeahead and `aria-expanded` on every node, and
 * half a tree widget is worse for a screen reader than the list and `<details>` that are here
 * — which already say "collapsible" and already say which row is current. That is its own
 * ticket, and it is a behaviour change rather than a visual one.
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
  if (workspace === undefined) {
    return (
      <nav className="explorer" aria-label="Explorer" data-testid="explorer">
        {/* The strip for chats outside every workspace is not a workspace on the plane, so
            there is no directory to explore and nothing honest to draw. */}
        <p className="empty">No workspace focused, so there is nothing to explore.</p>
      </nav>
    );
  }

  const { panels, pieces, piecesRefused } = state;
  const clones = panels?.repos ?? [];
  // The chats that are in no piece of this workspace: they work in the workspace itself or
  // in a clone, and they are listed under the workspace row rather than dropped.
  const inAPiece = new Set(
    clones.flatMap((repo) =>
      (pieces[repo] ?? []).flatMap((piece) =>
        chats.filter((chat) => under(chat.cwd, piece.path)).map((chat) => chat.session),
      ),
    ),
  );

  return (
    <nav className="explorer" aria-label="Explorer" data-testid="explorer">
      {state.trouble && <Trouble>{state.trouble}</Trouble>}

      <button
        type="button"
        className="spot spot-root"
        // Not `aria-selected`: that belongs to a tab, and the three tablists in this window
        // are the axis (ADR 0036). This is the current item of a list, which is what
        // `aria-current` is for, and it is what the old sidebar's workspace rows used.
        aria-current={spot === undefined ? "true" : undefined}
        onClick={() => onPick(undefined)}
      >
        <Folders className="node-icon" />
        <span className="spot-name">{workspace}</span>
        <span className="spot-what">the workspace itself</span>
      </button>
      <ChatList
        chats={chats.filter((chat) => !inAPiece.has(chat.session))}
        states={states}
        onShow={onShowChat}
      />

      {panels === undefined ? (
        <Pending>Reading the plane…</Pending>
      ) : clones.length === 0 ? (
        <p className="none">No repos in this workspace</p>
      ) : (
        // The clones are the workspace row's children, and the wrapper is what lets them be
        // drawn as such — the tree lines hang off it, one level in from the root row.
        <div className="clones">
          {clones.map((repo) => (
            // `<details>` and not a primitive: the browser has a collapsible and
            // `docs/ui-primitives.md` says native HTML that already does the job is not what
            // the Radix rule is about. Open by default — a closed explorer explores nothing.
            <details className="clone" key={repo} data-testid={`clone-${repo}`} open>
              <summary>
                {/* The twisty says which way the disclosure goes, which the default marker
                    said in the platform's own glyph at the platform's own size. It turns
                    with `[open]`, and the turn is the one motion here that is a direct
                    answer to a click — `prefers-reduced-motion` stops it all the same. */}
                <ChevronRight className="twisty" />
                <FolderGit2 className="node-icon" />
                <span className="repo">{repo}</span>
                <PieceCount pieces={pieces[repo]} refused={piecesRefused[repo]} />
              </summary>
              {piecesRefused[repo] ? (
                // Said, never swallowed: a clone with no rows otherwise reads as a clone
                // nobody has cut a worktree in.
                <Trouble>
                  charter could not list the worktrees of <code>{repo}</code>: {piecesRefused[repo]}
                </Trouble>
              ) : pieces[repo] === undefined ? (
                <Pending>Asking git…</Pending>
              ) : pieces[repo].length === 0 ? (
                <p className="none">No worktrees cut here</p>
              ) : (
                <ul className="pieces">
                  {pieces[repo].map((piece) => {
                    const here = chats.filter((chat) => under(chat.cwd, piece.path));
                    const picked = spot?.repo === repo && spot.piece === piece.piece;
                    return (
                      <li key={piece.piece} data-testid={`piece-${repo}-${piece.piece}`}>
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
                          <button
                            type="button"
                            className="spot"
                            aria-current={picked ? "true" : undefined}
                            // The whole path, because two clones in one workspace can hold a
                            // piece of the same name and the row has room for one word.
                            title={piece.path}
                            onClick={() => onPick({ repo, piece: piece.piece, path: piece.path })}
                          >
                            <GitBranch className="node-icon" />
                            <span className="spot-name">{piece.piece}</span>
                          </button>
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
                        <ChatList chats={here} states={states} onShow={onShowChat} />
                      </li>
                    );
                  })}
                </ul>
              )}
            </details>
          ))}
        </div>
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

/** Where the next chat starts, when it is not the workspace's own directory.
 *
 *  It carries the path the CORE spelled — `worktree_list` answers with it — so nothing here
 *  ever joins one together. */
export type Spot = { repo: string; piece: string; path: string };

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
}: {
  chats: readonly OpenChat[];
  states: ChatStates;
  onShow: (session: number) => void;
}) {
  if (chats.length === 0) return null;
  return (
    <ul className="here">
      {chats.map((chat) => (
        <li key={chat.session}>
          <button type="button" className="chat" onClick={() => onShow(chat.session)}>
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
