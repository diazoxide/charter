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
keeps `frame-menu`, `frame-action`, `frame-density`, `frame-resize` and `frame-probe` —
the command tmux's hotkey calls back into, the one every menu action calls back into, the
one the density entries run, the one the `window-resized` hook calls back into, and the
read-only probe — as top-level names rather than nested under `frame` too.

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

**Mouse is off by default.** `set -g mouse on` takes over drag-select, so turning it on
trades your terminal's own text-selection for clickable panels — a trade this release does
not make for you. `[frame] mouse = true` opts in.

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
charter can read an exit code and bring a dead panel back. Your other windows are not
touched by it, and the setting goes when the window does. That boundary has costs, and they
are the honest price of the sentence above:

- **Your scrollback limit and your mouse setting apply, not charter's.** `history-limit`
  and `mouse` are session options in tmux; setting them for the frame would set them for
  every other window in that session too. `[frame] history-limit` and `[frame] mouse` are
  ignored inside your tmux.
- **No hotkey.** tmux key tables are server-wide with no per-window form, so any key
  charter bound would be taken from every window you have open. The spec allowed a
  prefix-scoped bind here; charter takes the stricter option, because the menu's one
  entry is "Detach" and your own prefix key already does that better. The bottom panel
  drops its hotkey hint to match rather than advertising a key that does nothing.
- **Your status bar stays.** The frame gets the window, not the screen.
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

`bottom` is never dropped by those floors — it **shrinks** instead. Its height is its
content's: one row per repo (and per worktree, in a single-repo workspace), plus the
attention row, capped so the harness always keeps at least 12 rows and floored at the one
row `bottom` has always been. A workspace with no clones gets exactly that one row. The
cap is recomputed on every terminal resize, not remembered from the launch — tmux does not
refuse an over-large pane height, it takes the difference out of the neighbouring pane,
and the neighbour is your agent session. Recomputing means charter runs for a moment on
each resize (median 20ms, in the background, so nothing waits on it). During a fast drag
those runs can finish out of order and leave `bottom` sized for a window you have already
resized past; it corrects itself the next time you resize, and #501 tracks closing it
properly.

If the frame ends up narrower than the repo table's own columns (95), the table is not
drawn rather than drawn with its right-hand columns cut off: a row trimmed past the branch
loses the CI glyph and the open-change count, and a dirty, failing repo then reads as a
clean one. The attention row above it is unaffected — it drops whole fields instead, in
priority order.

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

## Configuring it

```toml
[frame]
slots = ["top", "bottom", "right"]
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
| `minimal` | `top` and `bottom` | one field on the attention row, four rows of repo table |
| `normal` | `top` and `bottom` | everything they have, table as tall as the window can spare |
| `full` | every edge | everything they have |

`full` is the shipped frame, so writing nothing at all and writing `density = "full"`
give you the same thing — and that is enforced rather than trusted: charter's own test
suite refuses a `density` default that does not expand to exactly the shipped `slots`
list, in the same order.

Writing `density = "full"` is the same as writing that level's `slots` list by hand, so
nothing else in charter has to know presets exist — the probe, `doctor`, and the size
floors all read the one resolved list. **An explicit `slots` wins**: `slots` is the
primitive, and if you wrote a list you meant that list.

`minimal` and `normal` are how you ask for less. Both drop to the two strips; `minimal`
also makes each panel terser — `top` drops the charter version (the workspace and the
persona are what it exists to tell you), and `bottom` keeps only its highest-priority
attention field (an alert if there is one, the spinner if work is running, otherwise the
todo count, so the row is never blank) and at most four rows of repo table under it. The
four that survive are the ones worth keeping — the repo you are standing in, and the ones
with something on them — and the table still says how many it hid. If you have kept
`right` by writing `slots` yourself, `minimal` shows four chips in it and says the same.

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

Every edge is on by default. `top` is a one-line strip; `bottom` is the attention row
plus the repo table under it, as many rows tall as the plane and the terminal allow (see
above); `right` (persona chips) is a 22-column sidebar that drops itself on a terminal too
small for it. The **order** is the order the panes are split in, and therefore the
geometry: with `bottom` before the sidebar its rows span the whole frame, while listing it
last leaves it only the width the sidebar did not take. `bottom` is where an alert, the
command that fixes it, and the repo table all appear, so the shipped order gives it the
full width.

**There used to be a `left` sidebar, and it is gone.** It drew repo rows recomposed for 22
columns — narrower than the name and branch columns of the table it was standing in for,
so a real branch name was always elided and a dirty, CI-failing repo could render looking
clean. `bottom` draws that table properly now, and the 22 columns go back to your agent
session. A `charter.toml` still naming `left` in `slots` is not an error: the name is
dropped the way any unknown slot is, and you get the rest of your list.

`slots`/`density`/`mouse`/`hotkey` are spelled the same on both sides. `history-limit`,
`min-cols` and `min-rows` are the three that are not: charter.toml spells them with a
hyphen: the
resolved settings charter's own code reads back use an underscore
(`config.FRAME["history_limit"]`) instead. A key typed the underscored way in charter.toml
is silently not recognized — the hyphenated spelling above is the one that is read.

The hotkey (`F2` by default) opens a small menu on whichever frame you are attached to —
"Detach", and the three density levels. It exists only on charter's own server; inside a
tmux you already have, charter binds no key at all (see above). However you detach — that
entry, or tmux's own prefix key — charter notices the session is still running and prints
how to get back in (`tmux -L charter attach -t <frame-id>`) rather than leaving you to
remember the flags.
