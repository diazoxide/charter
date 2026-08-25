# The frame owns the surface

ADR 0018 settled what charter draws when it *runs* a harness: tmux composes the
rectangles, charter fills only the edges, and the harness's own pane is never read,
parsed or drawn by charter at all. It answers "what may charter draw". It never had to
answer the question that arrives the moment charter's edges are actually filled in:
**charter has two surfaces now, and inside a frame they are the same surface twice.**

`charter statusline` is Claude Code's footer — repos, branches, CI, persona chips,
alerts, todos. Since #385 the frame's panels are built out of the same renderers, on
purpose, so that a fix lands in both at once. The result, inside a frame, is the plane's
state drawn on the top strip, again on the bottom strip, again in the sidebars, and then
one more time in Claude Code's own footer three lines below them.

## The decision

**Inside a live frame, `charter statusline` prints an empty line. The frame's panels are
the surface, and there is exactly one of it.**

Outside a frame nothing changes at all: the status line is still Claude Code's footer,
still the only charter surface most sessions ever have, and every one of its renderers is
untouched.

## Suppression means "render nothing", never "stop running"

This is the part that will look like dead code, so it is the part written down hardest.

The suppressed command still reads its payload and still records this turn's token usage
(`statusline.record_usage`). Claude Code's per-turn JSON is the **only** place those
numbers exist: it carries `context_window.current_usage` to the `statusLine` command and
nowhere else, and `charter/hooks.py` has zero references to any usage field — measured,
not assumed. The session's cache-hit history, its prefix-rebuild count and their
cumulative token cost are all reconstructed from what `_record_turn` wrote.

So the tempting cleanup — "this command prints nothing inside a frame, take it out of
`.claude/settings.json`" — does not remove a duplicate. It deletes the record, silently,
and nothing notices until somebody goes looking for a history that stopped being written
months earlier. **This is a command kept running for its side effect.** That sentence is
the reason this ADR exists rather than a comment.

## The decision lives at the command edge, not in `render()`

`statusline.render` is called directly by eight-plus `test_statusline_*` modules. The
frame check reads ambient state — an environment variable, a directory, a pid — and this
project's own suite now runs inside a frame. A check inside `render` would therefore make
those modules pass or fail according to which terminal the developer typed `python3 -m
unittest` in. A property that changes with the room you are standing in is not a property
a test can pin.

`statusline.main` is the whole command, and putting it there means no existing test of
`render` changes meaning. (`is_live` also requires the frame's directory to exist under
`config.STATE_DIR`, which every isolated test repoints — so a test that isolates plane
state cannot accidentally suppress itself either.)

## What counts as "inside a live frame"

Three conditions, all cheap enough for a path that runs every time the footer repaints:

* **stdout is not a tty.** Claude Code pipes it. A tty means a human typed `charter
  statusline` and wants the thing they asked for; a frame elsewhere on the screen is no
  reason to answer them with a blank line. (Measured 2026-08-24, Claude Code 2.1.241: the
  command's stdout is a pipe, and its environment is Claude Code's own, passed intact.)
* **the launcher pid at the end of that id is still a running process.** A frame id is
  `<workspace>-<launcher pid>`, so `os.kill(pid, 0)` answers "is that frame alive" with
  one syscall and no tmux subprocess. Without it, a directory left behind by a crashed
  launcher would blank this plane's status line forever, with nothing on screen to say
  why. `state.reap` already asks the same question for the same reason.
* **`$CHARTER_SESSION_ID` names a frame directory here that a LAUNCHER wrote a server
  marker into** — one read, not two, because the marker cannot exist without the
  directory and a separate `is_dir()` would be a guard no mutation could turn red. Any
  harness that knows its own session sets that variable, not only a frame. And a directory
  alone is not proof that a frame exists:
  `state.bump` creates one on demand and `notify.plane_changed` calls it from seven hook
  sites for whatever id is in the environment — so an operator who exports
  `CHARTER_SESSION_ID` in a shell rc gets a directory minted by their first tool call,
  carrying their own shell's permanently-live pid. Only `cmd_launch` records a server.
* **the harness pane the launcher recorded matches this process's `$TMUX_PANE`.** The
  four conditions above establish that a live frame exists; only this one establishes
  that the process asking is *inside* it. A process can hold an id it merely inherited —
  most sharply below `tmuxctl.SESSION_ENV_FLOOR`, where charter cannot put the frame id on
  `new-session` and a second frame's harness inherits the first frame's (#411). Blanking
  there would take away the one correct surface that operator still had. tmux sets
  `$TMUX_PANE` in every process it starts in a pane, and it survives the harness's own
  spawning of this command (measured: a real `statusLine` invocation reported `PANE=[%0]`).

* **the harness is Claude Code.** Suppression removes a duplicate, and only Claude Code
  has the surface being duplicated. **A harness with no status bar of its own is never
  suppressed**, which is this ADR's own premise applied one step further rather than an
  exception to it: opencode has no footer, so charter wires the plane in as an on-demand
  `/charter` slash command whose body pipes `charter statusline` through a shell
  substitution. That invocation is piped, carries the live frame's id, and runs in the
  recorded harness pane — every other condition here, satisfied perfectly — and blanking
  it removes nothing, because `/charter` puts plane state into the **agent's context**,
  which no panel can do: a panel draws to a pane the model never reads. codex is untouched
  from either direction; `charter statusline --watch` returns before any of this.

Every failure answers "not in a frame", which renders. A status line that vanished for a
reason nobody can see is the worst outcome available here — which is also why `charter
doctor`'s frame row says so when a session's line is being suppressed, rather than leaving
a blank footer to be explained by reading source.

## The frame must therefore SHOW what it suppresses

The shipped `[frame] slots` default moves from `["top", "bottom"]` to all four edges.
This is part of the same decision, not a separate improvement: once the status line is
blank inside a frame, an edge the frame does not fill is information that is not on the
screen at all. Two one-line strips were the status line again, in a worse shape — which
is exactly how the first release of the frame was reported: *"only top and bottom single
lines added, no left right sidebar, feeling — nothing delivered."*

`left` (repo rows) and `right` (persona chips) have had renderers since #385; they were
built, tested and switched off. `layout.visible_slots` drops both on any shortage against
`min_cols`/`min_rows`, so a narrow terminal degrades to exactly the frame this default
used to be.

The order of that list is the split order and therefore the geometry — measured against
tmux 3.7c in a 200×50 window: `["top", "bottom", "left", "right"]` gives a full-width
200-column bottom row with 46-row side panels between the strips, while `["top", "left",
"right", "bottom"]` gives 48-row side panels and a bottom row of 154 columns, inset
between them. The bottom row carries the one alert and the command that fixes it, and
`slots._bottom` drops whole fields when it runs out of width, so those 46 columns are
worth more there than two extra rows are to a sidebar that is already truncating.

## `$CHARTER_SESSION_ID` inside a frame: decided, not inherited

The frame launcher exports the frame id under `$CHARTER_SESSION_ID` — the variable
`charter.session.current()` already owns. That collision was load-bearing and
undocumented: panels follow `charter ws use` **only** because of it, since
`workspace.set_active` writes a per-session pointer under `session.current()` and a panel
reads it back under the same name. (The per-*terminal* pointer cannot do this job: it is
keyed by `$TMUX_PANE`, and the harness and each panel are different panes.)

**Kept, deliberately: one variable, one meaning — "the charter session this process
belongs to". Inside a frame, that is the frame.** Every process the frame contains — the
agent's shell, each panel, any `charter` command typed inside it — agrees on one identity,
which is what makes a switch made in the harness visible on the edges.

**They are not two variables with two jobs. They are two ids competing for one slot, and
the frame is chosen to win it.** An earlier draft of this ADR argued the comfortable
version — that Claude Code's id "reaches charter through stdin, never the environment" —
and it is false: measured 2026-08-24 against Claude Code 2.1.241, `$CLAUDE_CODE_SESSION_ID`
is right there in the environment, and `session.current()` reads it one rung below
`$CHARTER_SESSION_ID`. Inside a frame the frame id **shadows** it.

Choosing that deliberately is still the right answer, because the alternative is worse:
the harness and its panels are different panes and different processes, and the frame id
is the only name all of them can agree on. But it has to be argued as what it is, and it
has a consequence the comfortable version hides:

* **Everything keyed on `session.current()` becomes per-FRAME for the life of the frame** —
  the workspace pointer, and `workspace.set_active`'s **session lock** with it. The lock
  belongs to the frame rather than to the Claude Code conversation inside it: resume the
  same conversation in a new frame and it is a new key, with no lock and no pointer
  carried over.
* **What still keys on Claude Code's own id is what arrives by payload and never reads the
  environment**: the token-usage history this ADR keeps alive, and the session trace.

Pinned by `tests/test_frame_owns_the_surface.py`, which asserts that a panel follows a
`ws use` made under the frame's id and does *not* follow one made under any other.

That rule sends a bill, and #411 was it: charter's private tmux server is shared by every
frame on the machine, so only the first launch's `new-session` starts it, and tmux builds
a later session's pane environment from the SERVER's global one. Measured against tmux
3.7c, a second frame's harness read the *first* frame's `$CHARTER_SESSION_ID` — so
`charter ws use` wrote the first frame's pointer and every hook bumped the first frame's
version, while the second frame's panels sat waiting for a change recorded somewhere else.
`layout.session_argv` now carries the frame's identity on `-e`, the same way the
operator's-tmux path already carried it.

**Only the identity, and that is a security boundary rather than an economy.** A tmux `-e`
becomes the client's command line: world-readable in `/proc/<pid>/cmdline`, visible to
`ps`, recorded permanently by exec-audit tooling. `_frame_env` is the operator's entire
environment — measured at 138 argv elements and 7,696 bytes on a real machine, carrying
two live service-account tokens — so charter puts exactly four names there
(`commands_frame._FRAME_IDENTITY`: the frame id, the harness, the plane root, the
workspace pin), each one a value that must be *this* frame's rather than whichever
launcher started the shared server, and none of them ever a credential. Everything else
keeps reaching the harness the way it always did, through the tmux client's own
environment. Each of the four is emitted even when the launcher does not have it
(`NAME=`), because inheriting a value is as wrong as carrying a stale one — a pinned
`$CHARTER_WORKSPACE` outranks every pointer in `workspace.resolve`, so an inherited pin
would make `charter ws use` unable to move that frame at all.

The inside-a-tmux path still passes the whole environment, deliberately and now
documented: there the `-e` overlay lands on the operator's own tmux server environment,
which may predate this plane entirely, so charter states everything rather than trusting
it. That is a real exposure with a real reason; narrowing it needs its own measurement of
what a harness may safely inherit from a server charter does not own, and this ADR does
not pretend to have made it.

## Consequences, including the ones that cost something

* **A framed Claude Code session has no context/cache gauge on any surface.** The status
  line is where `ctx NN%` and `cache NN%` were drawn, and it is now blank inside a frame;
  `slots._top` deliberately does not call `_context_gauge`, because that renderer is gated
  at every branch on a per-turn payload no panel is ever handed (#385 established this and
  left the decision here). This ADR keeps the *recording* alive so a panel can be given
  one, and names what such a panel would still need: the usage file is keyed by Claude
  Code's session id, which a panel does not know — only the suppressing `statusline.main`
  sees both ids at once, so it is the one place that could write the mapping. That is a
  surface decision with a width budget attached, and it belongs to the release that draws
  it, not to this one.

  **Closed by #413, and closed the way this bullet said it would have to be.** The
  suppressing `statusline.main` writes the mapping (`frame.state.record_harness_session`),
  because it remains the one process that sees both ids; `record_usage` grew a fourth
  field for the context percentage, which is the one figure nothing can re-derive from
  `read`/`write`; and `slots._top` draws `statusline.recorded_context_gauge` — a sibling
  of `_context_gauge` composed from the recorded history rather than a live payload, so
  the two surfaces share their thresholds and their labels. `_context_gauge` itself is
  still not called from a panel, and every word above about why remains true. What did NOT
  change is the rule this bullet's own reasoning rested on: every way of not knowing
  answers empty, because a gauge silently reading zero is worse than no gauge.
* **codex and opencode get no context gauge either, and for a harder reason: nothing feeds
  them one.** Neither invokes `charter statusline` with a per-turn payload, so there is no
  usage recorded to draw from at all — which #413 does not change and cannot: it made a
  recorded history readable by a panel, and there is no history to read for a harness
  nobody hands the numbers to. Their frames show everything else, and their `top` row
  simply has no gauge on it.
* **A human can still ask.** `charter statusline` at a terminal, and `charter statusline
  --watch`, render exactly as before, frame or no frame.
* **Nothing outside a frame changes.** No renderer was modified, no test of `render`
  changed meaning, and a session with no frame draws exactly what it drew before.
* **One more surface has to stay in step.** ADR 0018 kept charter out of the harness's
  pane; this one puts charter's whole plane-state surface inside panels charter draws. A
  fact worth showing now has one place to be shown in a frame, and a `[frame] slots` list
  that excludes that edge is an operator choosing not to see it — which `charter
  frame-probe` and `charter doctor` already report.
