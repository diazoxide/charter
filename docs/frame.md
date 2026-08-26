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
keeps `frame-menu`, `frame-action`, `frame-density`, `frame-switch`, `frame-resize` and
`frame-probe` — the command tmux's hotkey calls back into, the one every menu action calls
back into, the one the density entries run, the one the workspace and persona entries run,
the one the `window-resized` hook calls back into, and the read-only probe — as top-level
names rather than nested under `frame` too.

## What it needs

tmux composes the rectangles and does every part of terminal emulation; charter fills the
edges and never draws or parses the harness's own pane (ADR 0018). Only tmux being
*missing* stops a launch. tmux 3.2 is the version charter has checked its own
requirements against — the hotkey's `display-menu`, and the pane-scoped hooks that carry
the harness's exit code back out. Below 3.2 `charter <harness>` still starts and nothing
is switched off: the hotkey stays bound but may open nothing, and if the exit-code hooks
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
  prefix-scoped bind here; charter takes the stricter option, because the menu's one
  entry is "Detach" and your own prefix key already does that better. The bottom panel
  drops its hotkey hint to match rather than advertising a key that does nothing.
- **Your status bar stays.** The frame gets the window, not the screen.
- **Your pane-border styling does not apply inside the frame's window.** Charter pins all
  five of the options tmux draws a border from — both border styles, plus
  `pane-border-lines`, `pane-border-indicators` and `pane-border-status` — so every rule in
  the frame is one colour and the frame looks the same in your tmux as in charter's own.
  This is the one place charter overrides a preference of yours rather than deferring to
  it, and the reason is that two of those options make one rule differ from its neighbour:
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
each resize (median 20ms, in the background, so nothing waits on it). During a fast drag
those runs can finish out of order and leave the table sized for a window you have already
resized past; it corrects itself the next time you resize, and #501 tracks closing it
properly.

If the frame ends up narrower than the repo table's own columns (95), the table is not
drawn rather than drawn with its right-hand columns cut off: a row trimmed past the branch
loses the CI glyph and the open-change count, and a dirty, failing repo then reads as a
clean one. Below that width the `repos` pane is not split at all — an empty bordered box
says "no repos" to the eye on a plane that has fourteen. That is an ordinary width, not an
exotic one, so an 80-column frame is the two strips and your session, with the table's
rows going back to the harness rather than being taken and left blank. A `slots` list
naming `right` before `repos` moves the threshold up by the sidebar's 23 columns: such a
frame needs 118 columns of terminal before the table appears. Narrow a frame that is
*already running* below 95 and the pane cannot be un-split — a resize changes sizes, not
which panes exist — so it shrinks to one row and says `⋯ too narrow for the repo table —
95 columns needed` rather than sitting there blank. The attention strip is unaffected
either way: it drops whole fields instead, in priority order. (The status line outside a
frame still crops instead of refusing; that is #506.)

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
menu, which now lists all three levels with a `•` on the one in effect; choosing one
re-lays the frame out live — panes are split or closed, sizes re-asserted, panels repaint
at the new verbosity. It does **not** edit `charter.toml`: that file is yours, hand
maintained and committed, and charter's rule is that machine-written state belongs
somewhere a machine may rewrite whole. The override lives in the frame's own state
directory and goes with the frame; relaunch and you are back to what the file says. The
same applies to `charter frame-density <level>` typed by hand from inside a frame, which
is what the menu entry runs.

Inside a tmux you already have, charter binds no key at all (see above), so there is no
menu and no keypress route to density there — `[frame] density` is what sets it, and the
command still works if you run it inside the frame's own window.

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

For one of charter's own four, `edge` and `size` may be written down and may only say what
the component already declares — `edge = "right"` and `size = 22` on the sidebar, `edge =
"top"` on identity. Charter derives the built-in geometry from those declarations, and
nothing between `charter.toml` and tmux carries a per-plane override for them, so a value
charter cannot honour is not quietly accepted and ignored: it takes the arrangement out of
play. Writing them is still worth it if you like your config explicit — and it is what
makes the two forms round-trip.

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
fiction.

Precedence, most explicit first: `[[frame.component]]`, then an explicit `slots`, then
`density`, then the shipped default.

`slots`/`density`/`mouse`/`hotkey` are spelled the same on both sides. `history-limit`,
`min-cols` and `min-rows` are the three that are not: charter.toml spells them with a
hyphen: the
resolved settings charter's own code reads back use an underscore
(`config.FRAME["history_limit"]`) instead. A key typed the underscored way in charter.toml
is silently not recognized — the hyphenated spelling above is the one that is read.

The hotkey (`F2` by default) opens a small menu on whichever frame you are attached to —
"Detach", the three density levels, and two submenus: the plane's workspaces and its
personas, each marking the one the frame is drawing. It exists only on charter's own
server; inside a tmux you already have, charter binds no key at all (see above). However
you detach — that entry, or tmux's own prefix key — charter notices the session is still
running and prints how to get back in (`tmux -L charter attach -t <frame-id>`) rather than
leaving you to remember the flags.

```
┌─charter───────────────────────────┐        ┌─charter · workspace───┐
│ Detach                        (1) │        │   default         (1) │
│   density: minimal            (2) │        │ * harness-wrapper (2) │
│ * density: normal             (3) │   ▸    │   release-0-54    (3) │
│   density: full               (4) │        │   user-reporting  (4) │
│ workspace: harness-wrapper  ▸ (5) │        └───────────────────────┘
│ persona: forge  ▸             (6) │
└───────────────────────────────────┘
```

**Switching from the menu moves the frame, and says so.** Choosing a workspace writes the
choice under the frame's own id — the same pointer `charter workspace use` writes from
inside the frame, which is what makes the panels follow — records it as the frame's
workspace, re-gathers the repo table for it, and bumps the frame so every panel repaints
against the new plane. A persona switch is the same minus the gather. Either way a
one-line message lands on your own screen saying what happened.

**Two switches are refused, and both say why on that same line.** A frame launched with
`$CHARTER_WORKSPACE` (or `$CHARTER_PERSONA`) set is *pinned*: that variable is in every
panel pane's environment for as long as the pane lives, and nothing charter can write
outranks it — so the menu says `cannot switch: $CHARTER_WORKSPACE pins this frame to
'<name>'` rather than reporting a move that would not happen. A name that is not there is
refused with the names that are; the menu never creates a workspace.

**The session lock moves with you.** `charter workspace use` locks the session to what it
selected so a workspace cannot be swapped out from under a running task — but a keypress
on a menu *is* you, and the switcher's own first write would otherwise take a lock that
its second write hit, leaving a switcher that worked exactly once. So the menu overrides
the lock and names what it overrode: `workspace → beta  (lock moved from 'alpha')`.

Long lists are cut to twelve rows with a last row saying how many were left out — a tmux
menu is drawn inside your terminal and does not scroll. Rows past the ninth are drawn with
no key at all — the digits run out at nine — and the arrow keys still reach them.

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
