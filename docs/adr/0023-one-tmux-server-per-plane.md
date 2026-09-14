# One tmux server per plane

An operator ran charter in two projects at once. Their frames mixed, and one project's
harness profiles showed in the other.

Read in `charter/commands_frame.py` as it stood at 0.61.1: every frame on the machine ran on
one tmux server, `SOCKET = "charter"`. A frame's session is named by its bare workspace
(`state.workspace_prefix`), and `_launch` decided whether to start a session or join one by
that name — `joining = session in live_sessions`, where `live_sessions` listed every session
on the machine. So a second plane opening `default`, the workspace every plane has
(`config.DEFAULT_WORKSPACE_FALLBACK`), became a window in the first plane's `default`
session: the first plane's panels, tab strips, `pane-died` hooks, key bindings and F2
palette, reading the first plane's state and profiles. Measured on real tmux before anything
changed: two throwaway planes, each launched with its own real `_launch` against one shared
socket, and `list-panes -a` answered `$0 %0 | $0 %1` — both planes' harness panes in one
session.

`SOCKET`'s own note only ever meant *never the operator's default socket*. It was safe while
sessions were unique by frame id; per-workspace sessions made the name collide. Two later
fixes (#793, #933) added an `@charter_plane` session marker and taught a workspace switch and
a quit to veto another plane's session by it, and `_plane_session`'s own docstring named the
collision it could not reach — `charter -w default` in one plane attaching to another plane's
frame. The launch path, and everything a server holds for all its sessions, kept it.

## The decision

**Each plane gets its own tmux server.** `tmuxctl.plane_socket()` names it:
`charter-plane-<12 hex>`, the hex from a sha256 of the plane's state directory, resolved.

- **The state directory, because that is what is per-plane.** Two roots pointed at one
  `$CHARTER_HOME` share `.charter/frame/`, where every chat record lives, so they are one plane
  by every question a frame asks — the same call `@charter_plane` already makes.
- **Short, because a socket is a path with a length limit.** `sun_path` is 104 bytes on
  macOS; the measured path is `/private/tmp/tmux-502/charter-plane-<12 hex>`, 48 bytes.
- **A name, not a path**, so `tmuxctl.server_argv` aims `-L` at it and
  `tmuxctl.is_socket_path` reads it as one.
- **Asked at call time**, never cached: `config.use` re-points the plane at runtime.
- **Cheap**: one `realpath` and one hash. It reads no file, so profiles stay off the import
  path (the profiles plan's ruling 43).

A server per plane makes the crossing impossible for everything a server holds at once:
sessions, the `-f` config, key tables, global hooks and the server environment.

**`is_operator_socket` still tells charter's server from the operator's**, and now also
answers "charter's" for the legacy socket and for any `charter-plane-<12 hex>` in tmux's
socket directory. `$TMUX` read in a pane of another plane's frame, or of a frame on the old
shared server, names a server charter started; reading it as the operator's would open this
plane's chat as a window on that server — the same defect through another door.

## The legacy rule

Frames started before this are on `charter` until they end, and must keep working.

- **A chat's record decides, as it already did.** `state.record_server` has been written by
  every launch since #381, and every close, quit, switch, resize, chrome, respawn and palette
  path asks `state.frame_server(fid)`. Those paths now fall back to `tmuxctl.LEGACY_SOCKET`
  where they fell back to `SOCKET` — for a chat with no record, which only a charter older
  than the record wrote, and only ever on the shared server.
- **`state.reap` gives a directory with no record to the legacy server alone.** It used to
  match every server, which was one answer while there was one private server. A reap of a
  plane's new server would otherwise delete the state of a chat still running on the old one.
  A launch reaps the legacy server too, while any of the plane's directories point there.
- **`_plane_servers` asks the servers a chat points at, the plane's own included**, and no
  other. A server nothing has started answers tmux 3.7c's `error connecting to …`, which
  `_plane_live` reads as "charter could not ask" for the whole plane; asked unconditionally,
  a reopen on upgrade day read the plane as not running and put a second copy of a live chat
  on screen.
- **A launch's "nothing live" gate counts the legacy server**, for the same doubling — and
  a restore it holds back for that reason stops the launch's recorder and says so, because
  the fresh chat it opens instead would otherwise be recorded over the quit, and the record
  is the only copy of each chat's resume id.
- **On the legacy server, a chat's own recorded pane decides whose window is whose.** Before
  this ruling, a second plane's `default` joined the first plane's session as a window, so a
  mixed session carries the first plane's marker for both planes' windows, and both planes'
  first chat is `default.1`. By id alone, plane A's quit killed plane B's `default.1`, which
  was listed after its own. Vetoed on the marker, plane B's quit stopped none of its own
  windows. And the legacy reap's keep list, which was by id, kept plane A's quit `default.1`
  alive while B's ran, so `charter` never restored plane A. So `_chat_seats` reads panes
  (`list-panes -a`): where this plane recorded the chat's harness pane on that server, the
  pane decides and the marker does not, and a chat recorded on another server is never a
  window here. `_legacy_keep` keeps a chat this plane recorded only by that pane, and applies
  no marker veto, so a keep list still leans towards keeping.
- **Nothing new starts on the legacy server.** A frame there cannot open a chat: `+` and a
  workspace tab for a workspace it has no session for refuse by name, because the new chat
  would land on the plane's own server, where that frame cannot show it and a switch cannot
  follow. The refusal names `charter frame-quit` typed in the project, then `charter`, which
  restores the plane onto its own server, and `charter -w <workspace>` from a terminal. It
  names the typed command and not F2 because on the shared server the palette's `run-shell`
  inherits `$CHARTER_ROOT` from whichever launch started that server, so F2 there can act for
  another plane. A handoff from such a frame opens in
  the background on the plane's own server, and its "is this session ours" question is asked
  of that server.
- **A pane id is matched only against the server its chat records.** `_plane_session` finds a
  workspace's session by the pane ids this plane wrote down; with chats on two servers, `%3`
  on the legacy server and `%3` on the new one are different panes.

## Where two planes still meet

Two places still hold several planes' chats in one server: the legacy socket, and a tmux the
operator runs, where charter opens each chat as a window in their session. Two planes
launched inside one operator tmux put two `default.1` windows side by side, and
`_stop_chats` aims `kill-window` by chat id. So `_launch_in_operator_tmux` now writes
`@charter_plane` on its own chat WINDOW — a window option on charter's window, the same move
as `@charter_chat`, and never a session option on the operator's session — and
`#{@charter_plane}` resolves pane, then window, then session. `_chat_seats` tells two such
windows apart by the pane each plane recorded (above), and the marker is what it reads for a
chat with no pane record; `_window_seats`, which finds a launch's own window by chat id,
gained the marker veto. `tmuxctl.live_pane_by_pid` needed nothing: it proves a chat by the pid of the
process asking, and a pid is one pane.

## Rejected

**Unique session names on one shared server** — the workspace name plus a plane tag. It ends
the join by name, and it leaves the rest of what a server holds shared: the `-f` config that
only the first frame's client applies, the key tables every frame's `source-file` overwrites
("last launched wins"), the global hooks and the server environment a new server copies from
whichever client started it. Every plane would still be drawn under whichever plane started
the server.

## Consequences, including what this costs

- **One tmux server process per plane with a frame open**, where there was one for the
  machine: a machine with ten planes open runs ten.
- **The socket name is not memorable.** Every place charter tells an operator how to get back
  in — a detach, `charter reopen`'s refusals, the "probably another plane's" refusals — prints
  the plane's own name, and `charter` in the project attaches without it.
- **A frame started before the upgrade cannot add a chat** until it is quit and restored, and
  `charter` does not reattach it. The alternative was to keep opening chats on the shared
  server, which is the defect.
- **Residual: a legacy server restarted after the upgrade.** Pane ids start again from `%0`,
  so a pane record of a legacy chat that ended before the restart could match another plane's
  new window with the same chat id. Nothing charter runs now starts that server.
- **The suite's guard follows.** `tests._planeguard.RealTmuxReach` refuses the real plane's
  own socket, computed before any test isolates the state directory, and every
  `charter-plane-<12 hex>` in the real socket directory; `tests._tmuxreap.owns` refuses the
  plane shape, because twelve hex digits are all decimal for about one plane in 280 and would
  otherwise read as a reapable `charter-<slug>-<pid>`.
