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
* **`$CHARTER_SESSION_ID` names a directory under this plane's frame state.** Any harness
  that knows its own session sets that variable, not only a frame; an id that never named
  a frame directory here is not this plane's frame whatever it parses as.
* **the launcher pid at the end of that id is still a running process.** A frame id is
  `<workspace>-<launcher pid>`, so `os.kill(pid, 0)` answers "is that frame alive" with
  one syscall and no tmux subprocess. Without it, a directory left behind by a crashed
  launcher would blank this plane's status line forever, with nothing on screen to say
  why. `state.reap` already asks the same question for the same reason.

Every failure answers "not in a frame", which renders. A status line that vanished for a
reason nobody can see is the worst outcome available here.

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

There are then two session identities in a framed session, and they key different things:

* **Claude Code's own session id** keys everything that arrives by payload — the usage
  history above, the session trace. It reaches charter through stdin, never the
  environment.
* **the frame id** keys everything a process can only read off its environment — the
  workspace pointer, the frame's version counter, the gather cache.

Two identities with two jobs, not one identity with a bug. Pinned by
`tests/test_frame_owns_the_surface.py`, which asserts that a panel follows a `ws use` made
under the frame's id and does *not* follow one made under any other.

That rule sends a bill, and #411 was it: charter's private tmux server is shared by every
frame on the machine, so only the first launch's `new-session` starts it, and tmux builds
a later session's pane environment from the SERVER's global one. Measured against tmux
3.7c, a second frame's harness read the *first* frame's `$CHARTER_SESSION_ID` — so
`charter ws use` wrote the first frame's pointer and every hook bumped the first frame's
version, while the second frame's panels sat waiting for a change recorded somewhere else.
`layout.session_argv` now carries the frame's environment on `-e`, the same way the
operator's-tmux path already did.

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
* **codex and opencode get no context gauge either, and for a harder reason: nothing feeds
  them one.** Neither invokes `charter statusline` with a per-turn payload, so there is no
  usage recorded to draw from at all. Their frames show everything else.
* **A human can still ask.** `charter statusline` at a terminal, and `charter statusline
  --watch`, render exactly as before, frame or no frame.
* **Nothing outside a frame changes.** No renderer was modified, no test of `render`
  changed meaning, and a session with no frame draws exactly what it drew before.
* **One more surface has to stay in step.** ADR 0018 kept charter out of the harness's
  pane; this one puts charter's whole plane-state surface inside panels charter draws. A
  fact worth showing now has one place to be shown in a frame, and a `[frame] slots` list
  that excludes that edge is an operator choosing not to see it — which `charter
  frame-probe` and `charter doctor` already report.
