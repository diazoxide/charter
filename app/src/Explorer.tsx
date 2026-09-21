import type { OpenChat } from "./bindings";
import { ChatState } from "./NeedsYou";
import { WorktreeMark } from "./Worktree";
import { stateOf, type ChatStates } from "./chatState";
import type { WorkspaceState } from "./workspaceState";

/**
 * The left region: the repo and worktree **explorer** (charter ADR 0038).
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
 */
export function Explorer({
  workspace,
  state,
  chats,
  states,
  spot,
  onPick,
  onShowChat,
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
      {state.trouble && (
        <p className="trouble" role="alert">
          {state.trouble}
        </p>
      )}

      <button
        type="button"
        className="spot"
        // Not `aria-selected`: that belongs to a tab, and the three tablists in this window
        // are the axis (ADR 0036). This is the current item of a list, which is what
        // `aria-current` is for, and it is what the old sidebar's workspace rows used.
        aria-current={spot === undefined ? "true" : undefined}
        onClick={() => onPick(undefined)}
      >
        <span className="spot-name">{workspace}</span>
        <span className="spot-what">the workspace itself</span>
      </button>
      <ChatList
        chats={chats.filter((chat) => !inAPiece.has(chat.session))}
        states={states}
        onShow={onShowChat}
      />

      {panels === undefined ? (
        <p className="pending">Reading the plane…</p>
      ) : clones.length === 0 ? (
        <p className="none">No repos in this workspace</p>
      ) : (
        clones.map((repo) => (
          // `<details>` and not a primitive: the browser has a collapsible and
          // `docs/ui-primitives.md` says native HTML that already does the job is not what
          // the Radix rule is about. Open by default — a closed explorer explores nothing.
          <details className="clone" key={repo} data-testid={`clone-${repo}`} open>
            <summary>
              <span className="repo">{repo}</span>
              <PieceCount pieces={pieces[repo]} refused={piecesRefused[repo]} />
            </summary>
            {piecesRefused[repo] ? (
              // Said, never swallowed: a clone with no rows otherwise reads as a clone
              // nobody has cut a worktree in.
              <p className="trouble" role="alert">
                charter could not list the worktrees of <code>{repo}</code>: {piecesRefused[repo]}
              </p>
            ) : pieces[repo] === undefined ? (
              <p className="pending">Asking git…</p>
            ) : pieces[repo].length === 0 ? (
              <p className="none">No worktrees cut here</p>
            ) : (
              <ul className="pieces">
                {pieces[repo].map((piece) => {
                  const here = chats.filter((chat) => under(chat.cwd, piece.path));
                  const picked = spot?.repo === repo && spot.piece === piece.piece;
                  return (
                    <li key={piece.piece} data-testid={`piece-${repo}-${piece.piece}`}>
                      <button
                        type="button"
                        className="spot"
                        aria-current={picked ? "true" : undefined}
                        // The whole path, because two clones in one workspace can hold a
                        // piece of the same name and the row has room for one word.
                        title={piece.path}
                        onClick={() => onPick({ repo, piece: piece.piece, path: piece.path })}
                      >
                        <span className="spot-name">{piece.piece}</span>
                      </button>
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
        ))
      )}

      {(panels?.absent.length ?? 0) > 0 && (
        <section className="absent" data-testid="absent">
          <h2>Not cloned here</h2>
          <ul>
            {panels?.absent.map((name) => (
              // Membership without a clone. There is nothing to explore in it and nothing
              // to start a chat in, so it is named and not made a heading.
              <li key={name}>{name}</li>
            ))}
          </ul>
        </section>
      )}

      {panels?.refused.map(([name, why]) => (
        <p className="trouble" role="alert" key={`refused-${name}`}>
          charter will not read <code>{name}</code>: {why}
        </p>
      ))}
    </nav>
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
