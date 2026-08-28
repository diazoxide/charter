# The frame

`charter claude` runs the harness inside a frame charter composes: the harness in the
middle, charter's own panels on the edges. It works on every harness, which is the point —
a status line is Claude Code's own surface, and Codex and opencode have none at all.

    charter claude               # or codex, opencode
    charter frame -- <cmd>       # anything charter has never met
    charter claude --no-frame    # bare, no frame at all

`claude`, `codex` and `opencode` are top-level commands (`charter claude`), never nested
under `frame` (`charter frame claude`) — `charter frame` is its own, separate escape hatch
for a command charter has no launcher for at all. That is not just naming: once `charter
frame` is on the command line, everything after it is grafted onto the harness's own argv
verbatim, before charter's own argument parser ever sees it, so `frame claude -p hi` would
hand `claude -p hi` to the `frame --` mechanism rather than route anywhere. The same reason
keeps `frame-palette`, `frame-density`, `frame-toggle`, `frame-switch`, `frame-resize`
and `frame-probe` — the command tmux's hotkey calls back into (and the program the
palette's own pane runs), the one the density rows start, the one a component's own key
runs, the one the workspace and persona rows start, the one the `window-resized` hook
calls back into, and the read-only probe — as top-level names rather than nested under
`frame` too.

## What it needs

tmux composes the rectangles and does every part of terminal emulation; charter fills the
edges and never draws or parses the harness's own pane (ADR 0018). Only tmux being
*missing* stops a launch. tmux 3.2 is the version charter has checked its own
requirements against — the `F12` escape hatch's `run-shell -C`, and the pane-scoped hooks
that carry the harness's exit code back out. Below 3.2 `charter <harness>` still starts and
nothing is switched off: the escape hatch stays bound but may do nothing, and if the
exit-code hooks
fail to install charter says so and declines to attach rather than risk a session nothing
can end. The resize-recovery hook needs a further 3.3; below that a resize still works,
panels can just drift out of shape until the frame is relaunched.

`charter <harness> --probe` (or the standalone `charter frame-probe`) answers "can a frame
run here, and what will it not be able to do" without starting anything: exit 0 if a frame
can run, non-zero if tmux is missing entirely, plus a line for each standing limit — a
tmux below 3.2, a tmux below 3.3 (no resize-recovery hook), and any `[frame] slots` entry
charter sizes but has no renderer for. `charter doctor` carries the same facts as its own
`frame` row.

The 3.3 line is worth calling out because the two floors are easy to conflate: 3.3 sits
*above* the 3.2 floor, so a tmux 3.2 passes the floor cleanly and still has no
`window-resized` hook. Charter used to say so only in the milliseconds before the frame
came up, which is nowhere. Now both surfaces name it, and what it costs is exactly one
thing: resize your terminal and the panels stretch, and stay stretched until the frame is
relaunched.

Those limits are deliberately **not** printed when a frame launches. A warning written to
your terminal microseconds before tmux switches to the alternate screen is not readable —
measured at 86 bytes ahead of the switch — and it comes back into view only once the frame
exits. All of them are standing properties of this machine and this plane rather than news
about one launch, so they live on the two surfaces you can ask on demand.

## What changes inside the frame

**Scrollback is tmux's own copy-mode, not your terminal's — the difference people notice
first.** The frame raises tmux's `history-limit` (50 000 lines, configurable) and binds the
mouse wheel to enter copy-mode, but it is tmux's buffer you are scrolling, not the
terminal emulator's.

**Mouse is off by default, and turning it on costs you your terminal's own text-selection.**
That is not a trade charter can soften and not one a later release will remove — it is how
tmux works. tmux asks your terminal to report the mouse, and from that moment your terminal
stops doing its own drag-select over the whole window. Measured on tmux 3.1c, 3.2 and 3.7c:
there is no state in which charter's panels are clickable and native selection survives.
`[frame] mouse = true` opts in with your eyes open.

**With mouse off, whether a panel is clickable is decided by the harness, not by charter.**
tmux asks your terminal to report the mouse from the *active* pane's own request alone. With
`[frame] mouse` off and the harness pane active, nothing has asked, so a click on a panel
produces no bytes at all — there is nothing for charter to receive and nothing for tmux to
route. If the harness you ran (Claude Code, say) does request mouse reporting, panels become
clickable while it is active, and you lose drag-select for as long as it is. Charter does not
control that program and cannot promise you either behaviour. Keys always work. A surface
charter draws over a whole pane and drives itself is the exception — while it is open it
*is* the active pane, so its own request is the one that reaches your terminal.

**Focus events are on inside a frame charter launched, and off inside your own tmux.** tmux
ships `focus-events` off, and it is a server-wide setting; charter turns it on for its own
private server, which is what lets a panel know it stopped being the active pane. Inside a
tmux you already have, charter writes no config at all (see below), so panels there never
learn it. Your terminal emulator also has to report focus for any of it to work.

**Panels repaint when charter's own hooks say something changed**, not by reaching out for
anything themselves: every `posttooluse*` hook bumps a version marker charter already
writes, and each panel notices the bump with a cheap local check (a `stat`, a few times a
second) — never a poll of anything outside the plane, and never work while nothing has
changed.

**Panels measure their own pane**, never `$COLUMNS` — a tmux pane inherits the *launching*
shell's environment whole, so trusting `$COLUMNS` there would lay a panel out at the outer
terminal's width and wrap inside its own much narrower one.

The pane a panel measures is the descriptor its process was **given**, taken once at start.
A panel paints to `sys.stdout` and measures from that same descriptor, so a component's
library that replaces that global — a logging handler, a progress bar, a framework's output
capture — would otherwise have the panel painting into the library's log and laying the
frame out for a rectangle nobody has, silently. And when the pane genuinely cannot be
measured, the panel says so (`charter: pane size unknown`) rather than assuming 80x24: if
your output is a file or a pipe there is no rectangle to be wrong about and the panel draws
at that default as before, but a real terminal that will not report its size gets a
sentence until it does.

**The frame animates only while work is in flight.** Work that is still running puts a
spinner and a count on the bottom row — `⠙ 2 running` — and the panel repaints often
enough for it to turn. Only the bottom row moves; the other panels repaint when something
changes and not otherwise.

**A dispatch, a `charter clone`, or a `gl-refresh`** — all three, since #420. Eight
parallel clones read as eight, because each repo takes its own record. The row counts what
is running and never names it, so the three are one number there.

What they are is not lost, though: every record says which kind it is, and the surfaces
that read a name back to you — the dispatch-overlap nudge, the `⚡` badge on a persona's
chip, this session's own `⚡ N` — ask for dispatches and get only those. That was the whole
reason the spinner shipped narrower than promised: put a clone in the same tracker without
a kind on it, and the overlap nudge tells you *"`x` writes code and `clone` are already
running"*. Every reader now says which kinds it means, and the default is dispatches, so
the next kind of work charter learns to record cannot leak into a sentence by being
forgotten at one call site.

The moment the last of it finishes, the frame goes completely still again and the panel
goes back to waiting for a version bump. Nothing is polled to find this out: charter
records work when it *starts* (that is also what makes overlapping agents visible at all),
so the panel asks one question per tick — a single `stat` of that tracker's own directory
— and reads the records themselves only when that answer moves. Measured on macOS/APFS:
about 5µs per tick added to the ~26µs a panel already spends checking the version file,
five times a second, or roughly 0.003% of one core while idle.

Work charter has stopped believing in — no result after thirty minutes, so the process was
probably killed — is still reported, and deliberately *not* animated: `⋯ 1 stalled`.
Charter keeps such a record for a day so a stuck dispatch (or a clone that was killed
mid-fetch) stays visible, and a spinner turning next to it would be claiming progress that
stopped half an hour ago.

**A panel that fails says so in its own pane.** Anything charter's own code can see — an
unknown slot, a renderer that raises, a crash in the panel's poll — is painted into the
pane as one line, and the panel then stays alive holding it there rather than exiting. That
last part is the point: a panel process that exits hands its pane to tmux, which writes
`Pane is dead (status N)` over it after scrolling the pane up one line, and `top` and
`bottom` are one line tall, so exiting is what made the reason unreadable.

**A panel whose process is gone is respawned, three times.** Deaths charter's own code
cannot see — the interpreter failing to start, a kill, an out-of-memory — fire that panel's
own `pane-died` hook, and charter brings the pane back after a growing pause (1s, 2s, 4s).
After the third attempt it stops and leaves the pane dead with tmux's own message visible.
That message really is all there is for this half, and not because charter throws anything
away: an interpreter that cannot start never runs a line, so nothing is written into the
pane to preserve (measured — a pane whose command is a nonexistent python shows
`Pane is dead (status 1)` and an empty scrollback behind it). Restarting is the only thing
that can help a failure like that, which is why it is the half that gets a retry.
The count is per slot, per frame, and is not reset by a respawn that appears to work: three
deaths in one frame's life is a broken panel, not a streak to start over. A panel has never
been able to take the agent down with it; the hook is scoped to the panel's own pane, so it
cannot reach the harness pane's hooks, which are what carry the agent's exit code. It works
the same inside a tmux you already have — which it did not until recently: a panel that
died there used to stay dead for the life of the frame. Making it work there needs one more
thing than the hook, and it is the reason the first attempt at this changed nothing: tmux
only runs a `pane-died` hook for a pane that died and *stayed*, and at tmux's default a pane
whose program exits is destroyed along with its hook. Charter sets that one option on the
**window it opened** and nowhere else, so panes in your own windows still close the way they
always did.

Charter never touches `~/.tmux.conf` — the frame's settings go into a private server of
charter's own (`tmux -L charter`), one server shared by every frame on the machine, with
each frame a session on it.

## Inside a tmux you already have

**Run from inside an existing tmux session, the frame does not nest.** Charter reads
`$TMUX`, and instead of starting a server of its own it opens **one window in your
server**, in the session you were in, with the identical layout inside it. One tmux, one
prefix key, your own window list. `charter claude` switches you to that window and waits;
when the harness exits, charter closes the window, tmux puts you back where you were, and
`charter claude` exits with the harness's own code.

Charter is a guest there, and behaves like one — it writes **nothing** of yours. Not a
server option, not a session option, not a key binding. What it does write is scoped to the
window it opened and the panes it created inside that window: those keep their dead panes so
charter can read an exit code and bring a dead panel back, and that window draws its pane
borders charter's way rather than yours. Your other windows are not touched by it, and the
settings go when the window does. That boundary has costs, and they are the honest price of
the sentence above:

- **Your scrollback limit and your mouse setting apply, not charter's.** `history-limit`
  and `mouse` are session options in tmux; setting them for the frame would set them for
  every other window in that session too. `[frame] history-limit` and `[frame] mouse` are
  ignored inside your tmux.
- **Your `focus-events` setting applies, and panels get no focus events unless you have
  turned it on.** `focus-events` is a tmux *server* option — measured on 3.2 and 3.7c, it
  sits in `show-options -s` and setting it for one session sets it for every session on
  that server. Charter turns it on for its own private server and will not touch yours.
  `set -s focus-events on` in your own config is how you get it here.
- **No hotkey.** tmux key tables are server-wide with no per-window form, so any key
  charter bound would be taken from every window you have open. The spec allowed a
  prefix-scoped bind here; charter takes the stricter option, because what the palette
  would offer there is "Detach", which your own prefix key already does better, and the
  density rows, which `[frame] density` sets. The bottom panel drops its hotkey hint to
  match rather than advertising a key that does nothing.
- **No `F12` escape hatch either, and this is the one that is worth knowing.** In charter's
  own tmux, `F12` returns you to your agent session from anywhere in the frame, including
  from a pane that has stopped answering its keyboard. It is a root key-table entry, which
  is the same server-wide thing the hotkey is, so the same rule applies here and charter
  binds nothing. Inside your tmux the way out of a stuck pane is your own prefix key,
  which charter has not taken and cannot take.
- **Your status bar stays.** The frame gets the window, not the screen.
- **Your pane-border styling does not apply inside the frame's window.** Charter pins all
  five of the options tmux draws a border from — both border styles, plus
  `pane-border-lines`, `pane-border-indicators` and `pane-border-status` — so every rule in
  the frame is one colour and the frame looks the same in your tmux as in charter's own.
  With `[frame] chrome` left at its default this is the only place charter overrides a
  preference of yours rather than deferring to it; setting `chrome` adds a second, on
  charter's own panel panes and never on the pane your harness runs in — and it also puts
  that colour BEHIND the frame's rules, because the cell between two panes is in neither
  of them and would otherwise stay your terminal's own background: a one-cell seam running
  between panels that are all the same colour. The rules take the colour the frame's
  components agree on, and the frame-wide `chrome` colour when they name different ones.
  Still window-scoped, still both border styles set to one value. The reason for
  the borders is that two of those options make one rule differ from its neighbour:
  the active-pane colour and the arrow indicators mark some borders and not others, and
  `pane-border-status top` writes your hostname into every rule and takes a row the
  frame's height arithmetic never budgeted for. Window-scoped, so your own windows keep
  your own values.
- **The harness inherits your tmux SERVER's environment, not your current shell's.**
  Charter states its own five identity variables and `$PATH` on the pane it creates, and
  nothing else. Anything you exported in the shell you typed `charter claude` in — a key
  for this project, a `direnv` load, a `nvm use` — does not reach the harness; whatever
  your environment held when you first started `tmux` does. It is the same thing that
  already happens on charter's own server, and the reason is the same: a tmux `-e` is a
  command line, and a command line is world-readable to every local user on Linux and
  recorded permanently by process auditing. Charter will not put your environment there.
  Export it in the pane the frame runs in, or start the harness from a shell inside the
  frame, if the harness needs it.
- **If `charter` itself is killed while the harness is running**, the harness keeps
  running and its window stays in your window list — close it with your own `prefix-&`.
  On charter's private server that same case is handled by a teardown hook; here a hook
  that closed the window would destroy the pane before charter could read the exit code
  out of it, so charter watches instead and does the closing itself.
- **Charter asks tmux four times a second** whether the harness is still there, for as
  long as the frame is up. There is no `attach` to block on when you are already
  attached, and this is what replaces it.

A `$TMUX` that names a server nothing answers on — one captured by `env` and re-exported,
or a `tmux kill-server` under a running script — is not a tmux you are inside. Charter
checks before it builds anything, and falls back to its own private server.

## Exit codes

The launcher does not `exec` tmux — an attached `tmux new-session` reports 0 regardless of
what actually ran inside it (measured against tmux 3.7c), so a frame's exit code would
otherwise always read as success. Instead charter waits for the session, then reads back
the harness's real status and exits with that. `--no-frame` and the automatic bypass when
stdout is not a terminal both skip the frame entirely and `exec` straight into the harness,
so a pipe (`charter claude -p … | jq`) carries the real exit code with no help needed.

## What `charter frame -- <cmd>` accepts

The escape hatch runs whatever **tmux** runs, and charter refuses nothing up front. That
rule is tmux's rather than charter's, and it has one edge worth knowing (measured against
tmux 3.7c):

    charter frame -- 'ulimit -n; exit 3'    # ONE argument  → handed to a shell
    charter frame -- ./build.sh --release   # TWO OR MORE   → exec'd directly, no shell

So a single argument can be a whole command line — builtins, `;`, pipelines, redirection —
while two or more are looked up as a program and its arguments, with nothing in between to
expand or resolve them.

Charter deliberately does **not** check the command against `$PATH` before starting it.
Such a check answers the wrong question for the first form (that text is not even one
word), and for the second it is a guess where a real answer arrives milliseconds later: a
command that resolves can still exit 127 on its own, or carry a broken shebang.

## When the command dies before the frame is drawn

This is the one case where you were never attached to anything, so without a report there
is nothing at all to see — and until 0.50 there wasn't. `charter frame -- nosuchthing`
returned 127 having printed **zero bytes**; so did anything else that died in the opening
milliseconds, a harness crashing on a bad config included. Charter now says what happened,
after the fact, out of what tmux actually did:

    ✗ charter frame: `nosuchthing` exited 127 before the frame was drawn — you were never
      attached, so nothing it printed was ever on screen.
      the pane still had this in it:
        zsh:1: command not found: nosuchthing

The dead pane's own last words are quoted back when there are any — that second line is
your shell's, whichever shell tmux starts on this machine, and it is more accurate than
anything charter could write in its place. When there are none — a failed direct exec
leaves the pane completely empty and a bare exit 1 — charter answers for itself instead,
telling apart the three states that need three different remedies: on `$PATH`, a file that
exists but is not executable, or neither.

The same report comes back inside a tmux you already have, where the wording is if
anything an understatement: you are attached the whole time, to your own server, but
charter never switches you to the frame's window — it is opened, filled with a corpse and
closed again without your screen changing at all.

A command that finishes *successfully* before the frame comes up (`charter frame -- true`)
is not reported, and its output is not reprinted: that was its stdout, and charter will not
invent it on stderr. If you want a short command's output, `--no-frame` — or a pipe, which
bypasses the frame on its own — is the right tool; the frame is for something you sit in
front of.

## When the terminal is too small

The size those floors are measured against is the terminal — or, inside a tmux you
already have, the tmux WINDOW the frame gets, which is what charter asks tmux for rather
than measuring the pane it was typed in.

Below `[frame]`'s `min-cols`/`min-rows`, the side panel (`right`) is the first to drop —
any shortage costs it, since it cannot spare its own divider. A further shortage in rows
drops `top` too. Below half of either floor, every panel drops and the harness simply gets
the whole terminal, the same choice `charter`'s own status line makes when it runs out of
width. So a narrow terminal degrades to the two strips on its own; nothing has to be
configured for it. A density change goes through the same floors, so choosing `full` in a
terminal with no room for a side panel gives you the edges that fit rather than a failed
split.

A resize goes through them too, so a *running* frame degrades and recovers the same way a
launch would — with one exception. Dragging below half the floors does not take the last
panel away: a frame with no panels also has no resize hook, and that hook is the only thing
that would notice you making the terminal big again, so charter would have no way back.
It keeps what it has at that size. `F2` still turns everything off, because a keypress can
be followed by another one.

`bottom` is never dropped by those floors, and it is the only slot that never is: it is
the attention strip — one alert and the command that fixes it — which is the whole reason
a cramped terminal is worth framing at all. It is one row at every size.

`repos` is the one whose height moves. It is its content's: one row per repo (and per
worktree, in a single-repo workspace), plus its own `▪ repos N` heading, capped so the
harness always keeps at least 12 rows. "Its content" means *what the panel will actually
draw*, not how many clones there are: the launcher and the resize hook ask the same
function the panel asks, at the same density and — the part that is easy to get wrong — at
the width of the **pane**, not of the window. Those are the same number for the shipped
`slots`, where the table spans the whole window. They are not the same if you have written
a `slots` list that puts `right` before `repos`: the sidebar is split off first, so the
table comes out 23 columns narrower, and it is that width the pane is sized for. The cap
is recomputed on every terminal resize, not remembered from the launch — tmux does not
refuse an over-large pane height, it takes the difference out of the neighbouring pane,
and the neighbour is your agent session. Recomputing means charter runs for a moment on
each resize (~35ms, in the background, so nothing waits on it). During a fast drag those
runs finish out of order — nothing serialises them — so each one re-reads the window
immediately before it applies anything and does nothing at all if the size has moved since
it measured. The newest measurement is the only one that still matches, which is the one
worth applying.

If the frame ends up narrower than the repo table's own columns (95), the table is not
drawn rather than drawn with its right-hand columns cut off: a row trimmed past the branch
loses the CI glyph and the open-change count, and a dirty, failing repo then reads as a
clean one. Below that width the `repos` pane is not split at all — an empty bordered box
says "no repos" to the eye on a plane that has fourteen. That is an ordinary width, not an
exotic one, so an 80-column frame is the two strips and your session, with the table's
rows going back to the harness rather than being taken and left blank. A `slots` list
naming `right` before `repos` moves the threshold up by the sidebar's 23 columns: such a
frame needs 118 columns of terminal before the table appears. Narrow a frame that is
*already running* below 95 and the pane goes away too — a resize adds and removes panes,
not only sizes them, so a running frame ends up with the panes a launch at its current size
would have drawn, in either direction. Panes are only moved once the window has held one
size for 400ms, so dragging through 95 columns does not thrash them in and out at every
step; until that settles the pane you are dragging past says `⋯ too narrow for the repo
table — 95 columns needed` rather than sitting there blank. A panel you hid with its own
key, or a density you chose from the palette, is never undone by a resize — only the size
filter is re-run. The attention strip is unaffected either way: it drops whole fields
instead, in priority order. (The status line outside a frame still crops instead of
refusing; that is #506.)

If a future `[frame] slots` ever names an edge charter sizes but has no renderer for, that
slot is skipped rather than drawn as a dead pane — the harness keeps the space — and
`charter frame-probe`/`charter doctor` say so.

## The status line goes quiet inside a frame

**Inside a frame, `charter statusline` prints nothing.** The panels are built out of the
status line's own renderers, so leaving both on drew the plane's state on the edges and
then again in Claude Code's footer three lines below them. The frame owns the surface
(ADR 0019); outside a frame the status line is unchanged in every respect.

Two things it does anyway, and they are deliberate:

- **It keeps running, and keeps recording.** Claude Code passes this session's token usage
  to the `statusLine` command and nowhere else — no hook ever sees those numbers — so the
  command still reads its payload and still writes the cache-hit and prefix-rebuild
  history. Unwiring `statusLine` from `.claude/settings.json` because it "prints nothing
  now" would delete that record rather than remove a duplicate.
- **A human asking still gets an answer.** Run `charter statusline` yourself in any
  terminal, or `charter statusline --watch`, and it renders in full. Only the piped
  invocation — which is how Claude Code calls it — goes blank, and only while a frame with
  this session's id is actually running.

**Only Claude Code's footer goes quiet, because it is the only one being duplicated.**
opencode has no status bar, so charter wires the plane in as an on-demand `/charter`
command instead — and that renders in full inside a frame, as it does everywhere else.
It is not a duplicate of anything: `/charter` puts the plane into the **agent's own
context**, which no panel can do, because a panel draws to a pane the model never reads.
codex is unaffected in either direction — `charter statusline --watch` never consults any
of this.

`ctx NN%` and `cache NN%` lived on the status line, and for one release a framed Claude
Code session did not show them anywhere. It does now: the `top` row draws them, out of the
history the suppressed status line goes on recording. The suppressed command is also what
makes that possible at all — it is the one process that sees both this frame's id and
Claude Code's session id, so it writes the mapping down; a panel reads it back and finds
the numbers.

The panel's gauge is not a second implementation: both surfaces share the colours and the
labels, so a green 60% in a frame and a green 60% in a footer mean the same thing. What a
panel cannot do is invent a figure it was never given — a frame whose harness has recorded
no turns yet, or whose harness is not Claude Code at all, simply has no gauge on its top
row rather than a `ctx 0%`.

codex and opencode still show nothing, and for a harder reason that has not changed:
nothing feeds either one a per-turn usage payload, so there is no history to read.
(opencode's `/charter` is not a way around that: it renders the same status line, which
has no numbers to show without a payload.)

## One frame, one charter session

`charter <harness>` exports the frame's id as `$CHARTER_SESSION_ID`, the same variable
every charter command reads to answer "which session am I". That is on purpose: inside a
frame, the frame **is** the charter session. The agent's shell, each panel and any
`charter` command typed inside the frame all agree on one identity, which is why
`charter workspace use <name>` typed at the agent moves the panels too — the pointer is
written under the frame's id and the panels read it back under the same one.

Claude Code's own session id has not gone anywhere; it arrives in the status line's stdin
payload and keys what comes with it (the usage history, the session trace). Two ids, two
jobs. See ADR 0019.

**Which workspace the panels draw is decided once, at launch, by the launcher.** A panel
cannot work it out for itself: the launcher is an ordinary shell in your own terminal and
answers from your cwd or your terminal's pointer, while a panel's cwd is the pane's and
its terminal id is its own tmux pane, so it would fall all the way through to `default` —
and did (#512). The launcher writes the answer into the frame's own state directory
instead. Nothing is pinned into the environment to achieve it, deliberately: exporting
`$CHARTER_WORKSPACE` would rank above every pointer and take `charter workspace use` away
from every framed session.

So the order the panels read is: `$CHARTER_WORKSPACE` if you pinned one, then what you
chose **inside** this frame, then what the launch resolved, then whatever the panel can
resolve for itself (a frame launched by an older charter, still running across the
upgrade). `charter workspace use <name>` at the agent still moves the panels, and still
moves them for the same reason as before.

The pin comes first because that is what it means everywhere else in charter: it ranks
above every pointer in `charter`'s own resolution, and `charter workspace use` warns you
that it will not stick while the variable is set. A frame is not an exception to that —
if it were, a frame launched under a pin that then had `ws use` typed at it would draw a
workspace no command in that session acts on. The `*` beside the name on `top` marks that
name as the pinned one.

**The repo table is gathered at launch, in the background.** A launch deletes the cached
scan first — pids are recycled and a new frame must not adopt a dead one's rows — and a
detached `charter frame-gather` fills it alongside the frame coming up, then bumps the
frame so the panels repaint. Until it lands, `repos` says `⋯ gathering this workspace's
repos…` rather than drawing an empty table: a workspace with no clones and a workspace not
yet looked at are different facts, and drawing them the same way is what made a frame read
as "no repos" on a plane full of them. A workspace that really has none says so, with the
command that changes it: `no clones in <workspace> · charter clone <repo> -w
<workspace>`. A panel never gathers on its own — it reads the cache or says it has
none.

## Configuring it

```toml
[frame]
slots = ["top", "bottom", "repos", "right"]
density = "full"
mouse = false
chrome = "off"
hotkey = "F2"
history-limit = 50000
min-cols = 100
min-rows = 20
```

### How much frame

`density` is a **preset over `slots`**, not a second way of configuring the same thing.
There are three levels:

| level | edges | each panel says |
|---|---|---|
| `minimal` | `top` and `bottom` | the two one-row strips and nothing else — no repo table, no sidebar, so every row and column the frame is not using is your session's; `top` drops the charter version and `bottom` keeps one field |
| `normal` | `top`, `bottom` and `repos` | both strips saying everything they have, and the repo table between them, as tall as the window can spare |
| `full` | every edge | the sidebar as well |

`full` is the shipped frame, so writing nothing at all and writing `density = "full"`
give you the same thing — and that is enforced rather than trusted: charter's own test
suite refuses a `density` default that does not expand to exactly the shipped `slots`
list, in the same order.

Writing `density = "full"` is the same as writing that level's `slots` list by hand, so
nothing else in charter has to know presets exist — the probe, `doctor`, and the size
floors all read the one resolved list. **An explicit `slots` wins**: `slots` is the
primitive, and if you wrote a list you meant that list.

`minimal` and `normal` are how you ask for less, and what they give back is a whole
component rather than a shorter one. `normal` drops the sidebar; `minimal` drops the repo
table too and leaves the two one-row strips, so a fourteen-repo workspace hands its
session fifteen rows and a border back rather than four. `minimal` also makes each
remaining panel terser: `top` drops the charter version (the workspace and the persona are
what it exists to tell you), and `bottom` keeps only its highest-priority attention field
— an alert if there is one, the spinner if work is running, otherwise the todo count, so
the row is never blank.

If you want the table but at `minimal`'s verbosity, write the `slots` list by hand and
declare the level beside it: an explicit `slots` wins over a preset, and the table then
keeps its four highest-ranked rows — the repo you are standing in, and the ones with
something on them — and still says how many it hid. `right` shows four persona chips that
way too, with its todo list shrunk to four rows the same way.

**The hotkey changes the density of the running frame, and nothing else.** `F2` opens the
palette, which lists all three levels with a `*` on the one in effect; choosing one
re-lays the frame out live — panes are split or closed, sizes re-asserted, panels repaint
at the new verbosity. It does **not** edit `charter.toml`: that file is yours, hand
maintained and committed, and charter's rule is that machine-written state belongs
somewhere a machine may rewrite whole. The override lives in the frame's own state
directory and goes with the frame; relaunch and you are back to what the file says. The
same applies to `charter frame-density <level>` typed by hand from inside a frame, which
is what the palette row starts.

A level is a *name for one arrangement*, applied by hiding and showing the same components
a component's own key does — see "A key that shows and hides one panel" below. The two
compose, and re-picking a level always puts the arrangement back to exactly what that level
names.

Inside a tmux you already have, charter binds no key at all (see above), so there is no
palette and no keypress route to density there — `[frame] density` is what sets it, and the
command still works if you run it inside the frame's own window. The same is true of a
component's `key`: the binds live in the config charter sources onto its own server, which
it never does inside your tmux, so `charter frame-toggle <name>` typed in the frame's own
window is the route there.

`hotkey` is checked against the shape of a tmux key name — optional `C-`/`M-`/`S-`
modifiers and then a key (`F2`, `Up`, `PPage`, `a`, `/`). Anything else falls back to
`F2`, the same way every other key in `[frame]` falls back to its default when charter
cannot make sense of it. That check is not cosmetic: this value is interpolated into tmux
configuration that `source-file` *executes*, and `charter.toml` is a committed, shared
file that arrives from someone else's machine.

Every edge is on by default, and the frame reads top to bottom as **identity · your
session · the repo table · what wants attention**.

`top` is a one-line strip — the workspace and the persona on the left, the charter version
(and the `dev` chip, if you are on that channel) at the far right, so identity and build
are not read as one sentence. The persona there is the *active* one, always; the roster of
every other persona joins it only when `right` is not on screen, since the sidebar draws
that same list with more on each row. Drop the sidebar — a narrow terminal, or a density
that does not include it — and the roster comes back to this row. `repos` is the repo table in a bordered component of its
own, headed `▪ repos 6` and as many rows tall as the plane and the terminal allow (see
above). `bottom` is the attention strip on the terminal's last row — one alert and the
command that fixes it, the in-flight spinner, this session's news, the todo count and the
hotkey hint, each shown whole or dropped whole. `right` is a 22-column sidebar carrying
two headed sections — `personas`, one row each with the memory, health and in-flight
badges lined up in a column of their own, and `todos`, this workspace's open todos beneath
them. The todo list is what `charter ws todo` shows, oldest first, cut to what the pane has
room for with a `…(+N more)` line saying how many it hid; a workspace with nothing open
gets no todo section at all rather than a heading over an empty space. The sidebar drops
itself on a terminal too small for it.

**The attention strip is last, and the table floats above it.** The table's height is its
content's, so whichever of the two sits lower moves up and down the screen as repos are
cloned or go quiet. Anchoring the alert is worth more than anchoring the table: an alert
you have to go looking for is one you read late.

**The `slots` order is the order the panes are split in, and therefore the geometry — in
both directions.** Sideways: a slot listed after `right` gets only the width the sidebar
left it, 23 columns fewer, which means a 118-column terminal before the table is drawn at
all. Vertically: every split but `top`'s goes directly below the harness, so a slot listed
*later* sits *higher* on screen. That is why the shipped list names `bottom` before
`repos` and the table appears above the strip. If you write the list yourself, keep that
pair in that order unless you mean to swap them.

**There used to be a `left` sidebar, and it is gone.** It drew repo rows recomposed for 22
columns — narrower than the name and branch columns of the table it was standing in for,
so a real branch name was always elided and a dirty, CI-failing repo could render looking
clean. `repos` draws that table properly now, and the 22 columns go back to your agent
session. A `charter.toml` still naming `left` in `slots` is not an error: the name is
dropped the way any unknown slot is, and you get the rest of your list. The same rule cuts
the other way for `repos`, which is a *new* name: a committed `slots = ["top", "bottom",
"right"]` still launches and simply has no table, because an explicit list is the
primitive and charter does not add to a list you wrote by hand. Add `repos` to it, or
delete the line and take the default.

### What it looks like

Five things make the frame read as an application rather than as output, and four of them
are on whatever you write in `charter.toml`:

- **A heading.** Each section of the sidebar and the repo table carries its name in bold
  with its count still dim — `▪ personas 6`. No row is added anywhere; it is weight, not
  furniture.
- **One inset.** Every row's text starts in the same column, whether it is a persona name,
  a todo title or a heading. One constant, so a panel added later lines up with the ones
  already there.
- **The row you are on.** The active persona's row in the sidebar is inverted across the
  whole pane, to its last column — not a marker at the start of it. Reverse video is your
  own foreground and background exchanged, so it is right on every colour scheme,
  including the ones where every grey charter could have picked is somebody's background.
- **A status you can read without colour.** Every status in the frame carries a glyph or a
  word that says the same thing as its colour: `⚠` on an alert, `⚑`/`✗` on a persona whose
  charter is a draft or broken, a count next to a badge. Colour is the second channel,
  never the only one. charter's own test suite strips every escape from each panel and
  fails if a status stops being distinguishable.
- **The surface**, which is the one that is off unless you ask. See below.

**`NO_COLOR` is honoured.** Set it to anything at all — including the empty string, which
is what a shell that exports it with no value gives you — and the panels emit no escape
sequences at all. So does a panel whose output is not a terminal, which is what
`charter panel top --session x > /tmp/log` does. Both were previously written in full
colour whatever you had asked for.

**Charter never picks a colour out of the 256-colour cube.** Everything it draws is either
one of the sixteen names your terminal palette defines or a plain attribute (bold, dim,
reverse) — so what you see is your own scheme, not charter's idea of one. The reason is the
inverse of the obvious one: an absolute colour is unsafe precisely on the terminals that
render it faithfully. A 16-colour terminal would downsample charter's grey to your own
black and look fine; a truecolor terminal on a light theme would get the dark grey
verbatim.

### A background behind the panels

```toml
[frame]
chrome = "dark"     # or "light", or "off" — the default
```

`chrome` gives charter's own panes a background, so the frame reads as chrome around your
session rather than as more text beside it. The focused panel is one shade off the others,
which is also how you can see which pane is live.

tmux paints it, not charter: the value sets `window-style` and `window-active-style` on
charter's panel panes. That is why it costs nothing on a repaint, cannot wrap a line, fills
the cells no renderer wrote, survives a resize and a reattach, and comes back by itself if
a panel dies and is respawned into the same pane — all measured against a real tmux, not
reasoned about. It is set per pane, so **the pane your
harness runs in is never touched** — charter does not decide what a colour means inside
your agent's own rectangle.

**Three words, and no `auto`.** Charter cannot see your theme. A pane cannot ask the
terminal (a colour query through tmux gets no reply), and `$COLORTERM` inside a pane
describes the terminal that started the tmux *server* — detach at your desk and reattach
over ssh and every panel still reads the old answer. So an `auto` would be a guess wearing
the word for a measurement, and `off` is the default because a background charter chose is
wrong on somebody's terminal and a frame that was fine before an upgrade should not come
back worse.

It is a word and not a style string on purpose: tmux expands formats inside a style value
at draw time, and `charter.toml` is a committed file that arrives from someone else's
machine. `chrome` names one of three looks charter holds itself; nothing you write there
reaches tmux. Anything else — a fourth word, a style, a list — leaves the frame at `off`
and charter still runs.

`NO_COLOR` overrides it: no colour on your screen caused by charter means none, whichever
process puts the bytes there.

**You do not have to edit a file to change it.** `F2` lists `chrome: dark`,
`chrome: light` and `chrome: off`, with the one you are on marked. Choosing one repaints
the frame you are looking at immediately — tmux redraws from the option, so no pane moves
and nothing is re-laid-out — and it does not touch `charter.toml`. The change lives in the
frame's own state directory and is gone when the frame ends, so relaunch and whatever you
configured is back. A pane the frame adds later (a density change, say) comes up in the
surface the frame is *on*, not the one it launched with.

`charter frame-chrome <off|dark|light>` is the same thing typed by hand from inside a
frame. Choosing `off` **removes** the options rather than setting a third look: afterwards
`tmux show -p -t <pane> -v window-style` answers nothing at all, which is what it answered
before you ever asked.

**It works on every tmux charter runs on.** The floor is tmux 3.2, and all of this was
re-measured there against a build from source, not inherited from the 3.7c it was designed
on: `window-style` is pane-scoped, it honours a colour and silently ignores `reverse`,
`dim` and `bold`, and all sixteen palette names resolve — sixty-six of sixty-eight answers
byte-identical to 3.7c, and the two that were not turned out to be the measuring harness.
So there is no version gate on any of it.

### A colour and an inset for one pane

`chrome` gives every panel the same background, which is the right default and the wrong
only answer: on a terminal that is already black, `chrome = "dark"` paints four panes the
colour the terminal already was, and the frame gains a background and no structure. What
makes a frame read as an application is that its regions are **told apart**. So each
component can say its own:

```toml
[[frame.component]]
use = "identity"

[[frame.component]]
use = "attention"

[[frame.component]]
use = "repos"
bg = "black"
pad = 1

[[frame.component]]
use = "sidebar"
bg = "brightblack"
pad = 1
```

`bg` is one of **seventeen words**: `default`, the eight ANSI colours (`black`, `red`,
`green`, `yellow`, `blue`, `magenta`, `cyan`, `white`) and their eight `bright` forms. The
focused pane is drawn in the other member of the pair — `blue` focuses to `brightblue` — so
you can still see which pane is live. `default` is your terminal's own background, which is
how one pane steps out of a frame-wide `chrome`.

Names and never `colour236` or `#1c1c1c`, for the reason the rest of charter's colour uses
names: `blue` is a slot in **your** palette and a cube index is a fixed point that no theme
moves. And this file is committed, so the colour you write is read on a machine whose theme
you have never seen. A word charter does not know — like a tmux style string, which tmux
would expand at draw time — takes the whole arrangement out of play and the frame falls
back to `slots`, which is visible, rather than one pane quietly losing its colour.

`bg` does not need `chrome` to be on. `chrome`'s default is `off` because a background
charter chose is wrong on somebody's terminal; a `bg` is a line you wrote by hand about one
pane, so it applies either way.

**The rules between the panes are painted too, and you do not configure them.** A pane
background fills that pane's rectangle, and the one-cell rule tmux draws between two panes
is in neither rectangle — so a frame whose panels were all `brightblack` came out as grey
boxes with your terminal's own black running between them, which reads as seams rather than
as an application. Charter puts a background behind the rules as well.

Which colour, given a rule has a pane on each side and they may be different colours: the
one your components **agree** on when they all resolve to the same background, and the
frame-wide `chrome` colour when they do not. A gutter in the frame's own colour between two
differently-coloured panels is what an application looks like; a gutter in no colour between
two identically-coloured panels is a seam. A plane that sets no `bg` at all is unchanged —
every pane is the frame-wide colour, so the rules are too.

There is one rule colour and not two, and that is deliberate: tmux draws the border of the
active pane from a second option, and letting the two differ is exactly the defect that put
charter in charge of these options in the first place — a rule that changes colour halfway
along, where it passes the active pane's corner. Which pane is live is shown on the pane
itself (its background is one shade off the others), never on the border. `chrome = "off"`
with no `bg` anywhere puts the rules back to your terminal's own, and `NO_COLOR` takes the
background off them while leaving the frame's rules drawn.

`pad` is how many cells that pane leaves empty at its **left and right edges** — one number,
both sides. Charter draws this one: tmux paints backgrounds and insets nothing.

**The pad comes out of the pane's width, not out of your terminal's.** The repo table
already gives up columns in a fixed order when its pane is narrow, and a padded pane simply
starts that arithmetic two cells earlier — the row is composed for the narrower pane rather
than composed wide and then pushed off its own right edge. On a pane too narrow to afford
it, the pad is dropped **whole** rather than reduced, so a narrow frame looks exactly as it
did before you wrote one. If a `pad` pushes the repo table under the width it needs, the
pane says so and the number it quotes already includes your pad — widen by it and the table
comes back.

There is no vertical pad, and that is deliberate. `identity` and `attention` are one row
each, so a top pad would not inset them — it would delete them. The repo table is sized to
its content, so a vertical pad there removes a repo from the table rather than moving it,
and which repo goes is a ranking charter made on purpose. Horizontal padding gets narrower;
vertical padding disappears.

`0` to `5`. Five is not a round number picked by hand: it is the widest inset the
narrowest pane charter draws — the 22-column sidebar — can actually take and still
have room for a name. A bigger one would be a value that pane always drops, on one of
the two panes you asked for this on. Anything else — a negative, a bigger number,
`true`, `"2"` — takes the arrangement out of play the way an unknown `bg` does.
### Writing the arrangement out

`slots` is shorthand. Each name in it places one of charter's built-in components on the
edge that component declares, in the split order you wrote. The long form says the same
thing one table per panel, and file order is split order:

```toml
[[frame.component]]
use = "identity"

[[frame.component]]
use = "attention"

[[frame.component]]
use = "repos"

[[frame.component]]
use = "sidebar"
```

That is exactly the shipped frame — the same panels, the same order, the same geometry as
`slots = ["top", "bottom", "repos", "right"]`. Note the names: `use` takes a **component
id**, which is the vocabulary the frame reasons in, and not a `slots` name. The four are
`identity`, `attention`, `repos` and `sidebar`; the sidebar is one pane drawing two parts,
`personas` and `todos`, which is why it has one name here and shows two headings on screen.
Mixing the vocabularies is refused rather than half-understood, so a file says which of the
two it is written in.

A component can be kept in the arrangement and not drawn:

```toml
[[frame.component]]
use = "repos"
visible = false
```

That is the one thing `slots` cannot express. Deleting a name from `slots` loses its
*position* along with it, so turning the panel back on later means remembering where in the
order it went; `visible = false` keeps the order and turns off the panel.

### A key that shows and hides one panel

Give a component a `key` and that key turns it off and on while the frame is running:

```toml
[[frame.component]]
use = "identity"

[[frame.component]]
use = "attention"

[[frame.component]]
use = "repos"
key = "F7"

[[frame.component]]
use = "sidebar"
key = "F8"
```

`F7` now hides the repo table and gives its rows back to your agent session; press it
again and the table is back. **Hiding a panel does not delete it from your arrangement**,
which is the whole difference from taking a name out of `slots`: charter still knows the
panel's edge, its size and where it sits in the order, so you never have to remember any
of it. Charter does not move panes that did not change, so a panel toggled back on is
split beside the ones that stayed; relaunch and you get your file's order exactly.

**Charter binds no key of its own for this, and it never will.** A tmux `bind -n`
intercepts the key before the pane underneath ever sees it, so a default would silently
take that key away from Claude Code — or codex, or whatever you ran — on every plane with
a `charter.toml`. Keys are bound because you named them. Pick ones your harness does not
use; function keys and `M-`/`C-` combinations are the usual safe ground.

**The key is held to the same alphabet as `hotkey`**, and for the same reason: it is
written into the tmux config your frame loads. Optional modifiers (`C-`, `M-`, `S-`) and
then a key name or one punctuation character — `F7`, `M-r`, `C-M-t`, `\`. Anything else
takes the whole arrangement out of play, like every other value charter cannot honour.

**And you cannot take a key charter has already bound.** Two components asking for the
same key, your frame's own `hotkey` (`F2` unless you moved it), and `F12` — the escape
hatch, the key that always returns you to your session even from a wedged overlay — are
all refused the same way. tmux has no notion of a key conflict: the last `bind` simply
replaces the earlier one, so one of the two would silently stop working and nothing would
say which.

**Density is now a name for one of these arrangements.** The three levels have not
changed and the `F2` palette still offers them — `minimal` still means the two one-row
strips, `full` still means every edge charter draws — but a level is applied by hiding
and showing the same components a key does, so the two compose. Press `minimal`, then
press the sidebar's own key, and you have the strips and the sidebar; pick a level again
and you are back to exactly what that level names. What a level still does that a key
cannot is set the *verbosity* — how much each panel says — which is why the levels are
worth keeping.

Nothing about this touches `charter.toml`. A key changes the frame you are in, for as
long as it runs; relaunch and you have the arrangement you committed.

If your plane is spelled with `slots`, write the arrangement out first — the long form
above says exactly the same thing, and the `[[frame.component]]` tables for any `slots`
list are one per name, in the same order.

For one of charter's own four, `edge` and `size` may be written down and may only say what
the component already declares — `edge = "right"` and `size = 22` on the sidebar, `edge =
"top"` on identity. Charter derives the built-in geometry from those declarations, and
nothing between `charter.toml` and tmux carries a per-plane override for them, so a value
charter cannot honour is not quietly accepted and ignored: it takes the arrangement out of
play. Writing them is still worth it if you like your config explicit — and it is what
makes the two forms round-trip.

`bg` and `pad` are the two keys that have no `slots` equivalent at all: they say how a pane
*looks*, not where it sits, so they only exist in the long form.

### A component charter did not write

`use` is a component id, and not every component id is one of charter's four. A Python
distribution you install can supply one:

```toml
[project.entry-points."charter.components"]
"acme.metrics" = "acme_charter.metrics:Component"
```

With that installed, your arrangement can place it:

```toml
[[frame.component]]
use  = "identity"

[[frame.component]]
use  = "acme.metrics"
edge = "right"
size = 12

[[frame.component]]
use  = "attention"
```

File order is split order here as everywhere else, so `acme.metrics` is split off before
the attention strip and the strip is inset beside it.

**`edge` and `size` are required here, and they win.** Required, because the only way to
ask a package where it would like to sit is to import it — and your config is resolved by
`charter --version` as much as by `charter frame`, so charter will not run a stranger's
code to answer a geometry question on every command. And they win, because your
`charter.toml` is committed and shared: a package's own preference overruling it would mean
one file drawing two different frames on two machines depending on what happens to be
installed. Arrangement is committed; execution is local.

**Charter never runs code your config names.** `use` is a *name*, resolved against what is
installed on this machine. If nothing supplies it, nothing runs and your arrangement is
refused — which is the whole reason components bind by name rather than by a `command =
"…"` string.

If the package is installed and then goes wrong — it raises when imported, it speaks a
component API charter does not, two packages claim the same id, or its `render` throws —
that costs **its own pane and nothing else**. The pane names the distribution and says what
happened, and the rest of your frame draws around it.

**What a component is handed, and what charter does not promise it.** Charter owns the
surface and the border; your component owns every cell it writes. The surface is
*underneath* — tmux paints the pane before your `render` is called — so a component that
paints no background of its own gets charter's for free and matches. One that paints its
own overrides its own cells and nothing else.

So that a component can match the rest of the frame without charter drawing over it, `ctx`
carries a read-only mapping of the frame's own recipes:

```python
def render(ctx):
    c = ctx.chrome
    return [f"{c['inset']}{c['heading']}metrics{c['reset']}{c['muted']} 12{c['reset']}",
            f"{c['inset']}{c['ok']}✓{c['reset']} all green"]
```

`heading`, `muted`, `selected`, `ok`, `warn`, `bad`, `reset` and `inset`. Every one is
either a plain attribute or one of the sixteen names your palette defines — never a colour
charter picked — and `inset` is the literal left margin the rest of the frame starts at, so
prepending it lines your rows up with charter's. Under `NO_COLOR`, or when the pane's
output is not a terminal, every escape in the mapping is the empty string and `inset` is
unchanged: the keys never disappear, so a component built on them does not break there.

There is no `surface` or `focus` recipe, because there is nothing to hand over — those two
are tmux pane options and no renderer can write one.

**What charter does not promise is that your pane looks like its own.** A component that
ignores the recipes and paints something else will not match, and charter will not make it:
it does not overdraw your heading, and it does not take a row out of your rectangle to fit
one of its own. What it does guarantee is what it already guaranteed — your paint stops at
your rectangle, your failure costs your pane and not the session, and your output is
contained before it reaches the terminal. A pane that clashes is a pane whose component
chose to, and that is honest: it *is* different, and a frame where every pane looked the
same regardless of who wrote it would hide the one thing worth knowing when a pane is
wrong.

You can run one by hand, which is what charter itself runs in the pane:

```
charter panel acme.metrics --session <frame-id>
```

That takes a component id — or one of the four slot names, which are shorthand for the
built-in ids, so `charter panel identity` and `charter panel top` draw the same strip.

**An arrangement charter cannot draw is refused whole, and your frame falls back to
`slots`.** Not one table at a time — dropping just the line charter could not make sense of
would hand you a frame with a panel silently missing from it, and a missing repo table is a
plane that looks like it has no clones. So a component charter has never heard of and no
installed distribution supplies, an edge it cannot place, a duplicate, a key that is not
one of the four, a provider placed without an `edge` and a `size`, or a `visible` that is
not `true`/`false` all mean the same thing: the arrangement is ignored and you get the
frame your `slots` (or `density`, or the default) describes. You see your whole arrangement
not take effect, which is something you can act on, rather than one pane's worth of quiet
fiction. A `key` charter will not bind, a key two components both claim, and a key equal to
your frame's own `hotkey` are on that list too — and so are a `bg` that is not one of the
seventeen words and a `pad` outside `0`–`8`.

Precedence, most explicit first: `[[frame.component]]`, then an explicit `slots`, then
`density`, then the shipped default.

`slots`/`density`/`mouse`/`chrome`/`hotkey` are spelled the same on both sides. `history-limit`,
`min-cols` and `min-rows` are the three that are not: charter.toml spells them with a
hyphen: the
resolved settings charter's own code reads back use an underscore
(`config.FRAME["history_limit"]`) instead. A key typed the underscored way in charter.toml
is silently not recognized — the hyphenated spelling above is the one that is read.

The hotkey (`F2` by default) opens the **palette**: a full-pane list of everything this
frame can do, drawn by charter in a pane of its own. Type to narrow it, arrow keys to move,
Enter to run, Escape to leave. It exists only on charter's own server; inside a tmux you
already have, charter binds no key at all (see above). However you detach — the palette's
own row, or tmux's own prefix key — charter notices the session is still running and prints
how to get back in (`tmux -L charter attach -t <frame-id>`) rather than leaving you to
remember the flags.

```
charter · 9 to choose from
> workspace: alpha — pick another
    persona: steward — pick another
    detach — leave the harness running
    density: minimal
    density: normal
  * density: full
  * chrome: off
    chrome: dark
    chrome: light

  up/down move   enter choose   esc cancel   F12 back to the harness
```

**Everything is listed, including what cannot run right now — with the reason beside it.**
An option you cannot see is one you cannot ask about, so a row that is refused stays and
says what would make it available. The reason is the right-hand column.

**Two of those rows are doorways.** `workspace:` and `persona:` say which one this frame is
on, and Enter opens the list of the others **in the same pane** — a picker, which is this
same surface over a different set of rows. Type to narrow it exactly as you would the
palette, Enter to switch, Escape to leave having changed nothing:

```
workspace · 4 to choose from
  * alpha
>   beta
    default
    zebra

  up/down move   enter choose   esc cancel   F12 back to the harness
```

**If you already know the name, do not go through the doorway — just type it.** Once
anything is typed, the palette also matches workspace and persona names, each row labelled
with which it is, so `F2` `b` `e` `t` `a` Enter switches without opening anything:

```
charter /zeb · 3 to choose from
> zeb-api                     workspace
    zebra-ui                  workspace
    zeb                       persona

  up/down move   enter choose   esc cancel   F12 back to the harness
```

The doorways are for browsing — when you do not know the name — and typing is for
switching, when you do. Both reach the same rows, and the one you are on keeps its `*`
either way.

**With nothing typed, no names are listed and none are read.** Names come off directory
listings on your plane, so opening the palette to press `detach` does not enumerate forty
workspaces to answer a question you did not ask. They are gathered on the first keystroke,
once, for as long as that palette is open.

**Typing the name of a noun this frame is pinned to lists it with the reason.** The doorway
refuses to open a picker for a pinned noun, because every name in it would be a move that
could not happen — but a name you typed is a question you asked, so the row appears with
`cannot switch: $CHARTER_WORKSPACE pins this frame to '<name>'` beside it rather than an
empty pane you cannot tell from a typo.

**The filter reads titles and action ids, never the right-hand column.** So `detach` finds
the detach row and `acme.deploy` finds a provider's action by the name its documentation
uses, while `persona` finds the persona *doorway* rather than every persona on the plane —
the kind label is a label, not a search term.

**There is no row cap.** The old menu was a tmux `display-menu`, drawn inside your terminal
and unable to scroll, so it cut every list at twelve and lost the digit shortcut past nine.
The picker is a pane charter draws: it scrolls, it filters, and a plane with forty
workspaces lists forty.

**The menu is gone.** `charter frame-menu` and `charter frame-action` no longer exist, and
neither does the `display-menu` they opened. `F2` was always trying to be a palette;
keeping both would have left two answers to "how do I do a thing", which is how the single
menu became weird in the first place.

**Switching from the picker moves the frame, and says so.** Choosing a workspace writes
the choice under the frame's own id — the same pointer `charter workspace use` writes from
inside the frame, which is what makes the panels follow — records it as the frame's
workspace, re-gathers the repo table for it, and bumps the frame so every panel repaints
against the new plane. A persona switch is the same minus the gather. Either way a
one-line message lands on your own screen saying what happened.

**A pinned frame says so before you press anything.** A frame launched with
`$CHARTER_WORKSPACE` (or `$CHARTER_PERSONA`) set is *pinned*: that variable is in every
panel pane's environment for as long as the pane lives, and nothing charter can write
outranks it — so that noun's row carries `cannot switch: $CHARTER_WORKSPACE pins this
frame to '<name>'` and opens no picker, rather than offering a list of moves that would not
happen. The other noun is unaffected: one pin, one noun. Nothing here creates a workspace
either — an unknown name is a refusal with the existing names beside it, never an implicit
create.

**A name that is not a name is drawn and never run.** A workspace or persona is a directory
somebody can add in a commit, and a filesystem forbids only `/` and NUL — so a name can hold
a newline, a U+2028, an escape sequence, a quote or a `#`. Every name is made one line
before any column is measured (#472), so it is exactly one row on screen; and the switch
re-checks it against the same alphabet `charter workspace use` does, so a name charter would
not accept is refused with a message rather than acted on.

**The session lock moves with you.** `charter workspace use` locks the session to what it
selected so a workspace cannot be swapped out from under a running task — but a keypress on
the picker *is* you, and the switcher's own first write would otherwise take a lock that
its second write hit, leaving a switcher that worked exactly once. So the switch overrides
the lock and names what it overrode: `workspace → beta  (lock moved from 'alpha')`. Silence
would be the wrong answer: an agent inside the frame took that lock, and its next command
acts on a workspace it was never told had moved.

**Pressing `F2` again while the palette is open opens a second one.** `bind -n` is tmux's
root key table, so tmux matches the key before any byte reaches the palette's pane — the
same property that makes `F12` work against a pane that has stopped answering. The second
palette is the one taking your keys; Escape closes it, and the first is an ordinary pane
you can select and close the same way.

### Picking a workspace when the frame opens

`charter <harness>` used to resolve a workspace silently and go straight in. If **nothing
chose one** — no `--workspace`, no `$CHARTER_WORKSPACE`, not standing in a workspace tree,
no per-session or per-terminal pointer, no declared default — it now asks first:

```
  charter · which workspace?

     1  * default          —
     2    harness-wrapper  7 repos
     3    user-reporting   1 repo

     n    create a new workspace
     q    cancel — start nothing

  workspace [default]:
```

A number, a name, or Enter for the marked one. `n` prompts for a name, checks it against
the workspace alphabet, and asks `create <name> and switch to it? [y/N]` — anything but a
`y` goes back to the list, and a cancelled picker creates nothing at all. `q`, Ctrl-C or a
closed stdin end the launch having started nothing; the exit code is 130.

**Picking is the confirmation that locks.** That is what selecting a workspace has always
meant — `charter workspace use` locks the session to what it selected — and the launch
says so on the line after your answer. The frame has its own way out: `F2 → workspace`
overrides the lock and tells you it did, so a choice made at the prompt does not send you
back to a shell to change it.

**It never asks twice, and it never asks a script.** Your choice is written as the
terminal's own pointer, so the next launch from that terminal has an answer and goes
straight in. And the prompt is reached only on the interactive path: `--no-frame`, a
redirected stdout and a stdin that is not a terminal each return before it — `charter
claude` from a script or another agent cannot block on it. `--workspace <name>` names one
outright and skips the picker; `--pick` asks even when something already chose.
