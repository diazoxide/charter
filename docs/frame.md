# The frame

`charter claude` runs the harness inside a frame charter composes: the harness in the
middle, charter's own panels on the edges. It works on every harness, which is the point —
a status line is Claude Code's own surface, and Codex and opencode have none at all.

    charter claude               # or codex, opencode
    charter                      # the same thing, on a plane with [harness] default
    charter frame -- <cmd>       # anything charter has never met
    charter claude --no-frame    # bare, no frame at all

`charter` on its own opens the frame once the plane says which harness it means —
`[harness] default = "claude"` in `charter.toml`, documented in
[control-plane.md](control-plane.md#harnessdefault--bare-charter). It is a rewrite of the
command rather than a route of its own: `charter` becomes `charter claude` and everything
below applies to it unchanged. A plane that names no default keeps the usage list, and so
does `charter` with its output piped or redirected — a script asking whether charter is
installed must not get a harness session instead of an answer.

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
can end. The resize-recovery hook needs a further 3.3; below that a resize still works, the
panels just do not come back on their own — `charter frame-resize`, typed in the frame's
own window, restores them.

`charter <harness> --probe` (or the standalone `charter frame-probe`) answers "can a frame
run here, and what will it not be able to do" without starting anything: exit 0 if a frame
can run, non-zero if tmux is missing entirely, plus a line for each standing limit — a
tmux below 3.2, a tmux below 3.3 (no resize-recovery hook), any `[frame] slots` entry
charter sizes but has no renderer for, and a `[[frame.component]]` arrangement charter
refused. It closes with the three keys below, because it is also the closest thing charter
has to "tell me about the frame". `charter doctor` carries the same facts — the machine's
in its `frame` row, the file's in its `charter.toml` row, beside the other settings a plane
declares and charter is not honouring.

The 3.3 line is worth calling out because the two floors are easy to conflate: 3.3 sits
*above* the 3.2 floor, so a tmux 3.2 passes the floor cleanly and still has no
`window-resized` hook. Charter used to say so only in the milliseconds before the frame
came up, which is nowhere. Now both surfaces name it, and they name the remedy with it.

What it costs is this. Resize your terminal on a tmux below 3.3 and nothing re-measures the
panels, so they keep the shape the drag left them in. Measured on 3.2, a frame launched at
120x40 and dragged to 80x24 and back:

```
%1 5x120   %0 22x97   %4 22x22   %3 5x120   %2 5x120     <- and staying that way
```

While you are at the small size it is more than cosmetic: the sidebar is squeezed to **two
columns** of truncated glyphs (and still costs three, with its border), and the repo pane
holds `⋯ too narrow for the repo table — 95 columns needed` — a line written to be
transient, which here never settles because nothing measures again.

**`charter frame-resize`, typed in the frame's own window, fixes it completely and at
once.** It is the same command the missing hook would have called, and nothing in it
depends on your tmux version — only the hook that fires it does. On the same frame:

```
$ charter frame-resize
%1 1x120   %0 34x97   %4 34x22   %3 1x120   %2 1x120     <- the launch geometry, exactly
```

So the standing limit below 3.3 is not "until you relaunch", it is "until you ask" — and
on this tmux that command is the one recovery worth knowing. There is no palette row and
no key for it: the frame binds keys through the config it sources onto its own server, and
this is a command you type.

Those limits are deliberately **not** printed when a frame launches. A warning written to
your terminal microseconds before tmux switches to the alternate screen is not readable —
measured at 86 bytes ahead of the switch — and it comes back into view only once the frame
exits. All of them are standing properties of this machine and this plane rather than news
about one launch, so they live on the two surfaces you can ask on demand.

### What opening one costs

There is a moment between typing `charter claude` and the frame appearing, and most of the
tmux half of it used to be charter waiting on a socket. A tmux command is a round trip —
~5 ms on the machine these were measured on, ~13.4 ms on the one that reported it as slow —
and a four-panel launch made 46 of them before it could attach. Two thirds read nothing
back: window options, pane options, hooks, the panel splits. tmux takes a `;`-separated
list and runs the whole thing server-side, in order, so each of those groups is now one
invocation (#780).

Measured on a four-panel frame at 200x50, from the first tmux command to the attach:

| | tmux invocations | wall clock |
|---|---|---|
| tmux 3.7c | 46 → **17** | 227 ms → **91 ms** |
| at the 3.2 floor | 42 → **16** | 174 ms → **69 ms** |

No client is attached during any of that, by construction — there is nothing to attach to
until `attach` — which is why the blank has no progress line and cannot have one (#728).

The rest of the wait is charter's own import, paid once by the launcher, once by the
detached gather child, and once per panel process. That is a different lever and is not
this one.

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

**When a click does reach a panel, it acts where you pointed and does not move your
keyboard.** Charter delivers a click and a scroll to whichever panel the pointer is over,
in that panel's own cells, and never selects the pane — the frame exists to keep the
harness the thing you type into, and a click that quietly moved the keyboard somewhere
else would be the opposite of that. Measured on tmux 3.7c and 3.2 alike.

**If you already set `mouse = true`, this release stops your clicks moving the keyboard —
you no longer need `F12` after clicking a panel.** Until now, tmux's own default binding
selected the pane under the pointer before handing the click on, so `mouse = true` bought
you clicks that always work and cost you a keyboard that followed them. Charter now
rebinds that one key inside its own private tmux server, and only for the panes it created
itself: **a click on a panel acts where you pointed and leaves your keyboard on the
harness, while a click on the harness — or on any pane you split yourself inside the frame
— still selects it, exactly as tmux documents.** The wheel never moved the keyboard and
still does not. Both measured on 3.7c and 3.2:

| | `mouse = false` (default) | `mouse = true` |
|---|---|---|
| does a click reach the panel? | only while the active pane is asking your terminal to report — your harness decides | always, from the moment you attach |
| does a click on a panel move your keyboard? | **no** | **no** |
| does a *right*-click on a panel move your keyboard? | **no** | **no** |
| does a click on the harness, or a pane you split, move your keyboard? | no — with its mouse off tmux runs no mouse binding at all | yes — tmux's own behaviour, untouched |
| does a right-click on the harness, or a pane you split, still open tmux's pane menu? | yes — with its mouse off tmux runs no mouse binding at all | yes — tmux's own menu, untouched |
| does the wheel move your keyboard? | no | no |
| do you keep native drag-select? | while no mouse-requesting pane is active | no |
| a click on a pane border | reaches nobody — a border is a cell in neither pane | reaches nobody |

The drag-select row is the trade `mouse = true` makes, and it has not changed and will not:
it is how tmux works, not a default charter chose. What went away was a second cost that was
never part of that trade — one key's default behaviour, in a server charter owns.

**The right button took a release longer, and the reason is worth one sentence** because it
is what the last row of that table is about. tmux's default for the left button is two
commands, so charter could write out the half it wanted to keep. Its default for the right
button ends in tmux's own pane menu — Copy Line, Paste, Horizontal Split, Kill, Zoom — which
is a page long and *different on different tmux versions*, so a charter that wrote its own
replacement would have deleted that menu from your harness and from every pane you split
yourself, inside charter's own window, to fix a focus steal one click puts right. Charter
reads your server's binding back instead and puts its own panel test in front of it. If you
rebound that key yourself, your binding is what every pane charter did not create still
runs.

One consequence of that first column worth knowing, because it is new: a panel whose
component declares `click` or `scroll` asks *its own* terminal to report, so if you select
that panel's pane (with your tmux prefix, say), your terminal starts reporting and native
selection goes for as long as it is the active pane. Selecting the harness back — or `F12`
— ends it. **All four panels charter ships declare `click` now, and the two bars do too**,
so that consequence applies to any pane of the frame you make active — where before this
release it was the repo table alone. A component that declares neither kind still asks for
nothing and changes nothing; charter simply no longer ships one.

**The repo table scrolls and it selects.** Roll the wheel over it and the
window moves down the list; click a row and that repo is selected, its row drawn in reverse
video and its state read back in words on the right of the attention row — `▪ charter ·
fix/x · dirty · 2 ahead · CI failed`. A click only ever *selects*; nothing charter does from
a pointer is irreversible, because a click can reach a pane without its matching press (a
drag begun on a pane border delivers exactly one release), and a gesture that can arrive
half-formed is not one to hang an action on.

Three things follow that are worth saying plainly rather than leaving you to find:

- **The wheel does nothing when the table already fits**, which is most of the time — the
  pane is sized to its own content. There is nothing below to scroll to, so nothing moves
  and nothing repaints. It starts moving on any plane with more table ROWS than the pane
  has, which is the same plane that shows `…(7 below)`.
- **The last line says which side the rows you cannot see are on** — `…(4 above, 4 below,
  all clean)`, and just `…(8 above, all clean)` once you have scrolled to the end. It is one
  window over one list, so the first number is how far down you are. A side with nothing on
  it is not drawn, and the line is about no repo: clicking it selects nothing.
- **Rows, not clones.** A worktree gets a row of its own on a plane with a single clone, and
  those rows scroll like any other: a monorepo plane with twenty worktrees is a
  twenty-one-row table however few repos are in it. Scrolling past the clone's own row
  leaves its worktrees hanging from a `│` whose root is above the window, which is what a
  scrolled tree looks like; a click on a worktree row selects the clone it is a piece of.
- **You do not need a mouse.** `F2` → `repo: select the next row` (and `previous`) moves the
  selection with the arrow keys and Enter you already drive the palette with. That is the
  only route on a plane with `mouse = false`, which is the default, and charter will not
  bind a bare arrow key to change it: a `bind -n Up` is server-wide and would take the arrow
  before your harness sees it.
- **Those two rows repeat, and that is what makes them usable.** Enter on either one moves
  the selection and **leaves the palette open**, with what you typed and the row you are on
  exactly where you left them — so three rows down the table is `F2`, `next`, Enter, Enter,
  Enter, and one palette rather than three. The header says what each press did
  (`charter /next · selected auth`), because the palette is drawn over the whole window and
  the table is not on screen behind it. Escape leaves; every other row still closes the
  palette when it runs.

**The tab bars are the exception to "a click only selects", and they say why.** Click a name
on the `chats` or `workspaces` bar and the frame *switches* to it — there is no in-between
state to confirm, because the tab you are on *is* the selection and the `*` beside it is how
you can see that. The three things that make a click safe to switch on here are the same
three that keep it a mere selection on the table: charter acts on the **press**, which is the
half that is never delivered unpaired; a switch is undone by the identical click on the tab
you came from; and nothing else could ever finish the gesture, because a keypress does not
reach a panel at all — tmux gives your typing to the active pane, which is the harness.

Clicking the tab you are already on does nothing at all, rather than tearing the panels down
and putting them back to arrive where you were. So does clicking the row's own left inset,
the gap between two names, or the empty space past the last tab: those are cells no tab was
drawn into, and charter will not pick the nearest name for you.

**Right-click a chat tab and you get a menu about that chat.** Two rows — `chat: previous
transcript` and, last, `chat: close` — both about the tab under the pointer rather than the
chat you happen to be in, so you can close a chat from its own tab without switching to it
first. Close is a doorway, not a button: pressing it draws the same warning `F2 → chat:
close` draws, naming the chat and what stopping it costs, and the keypress on *that* is what
stops the harness. Escape leaves, `F12` always leaves, and nothing is stopped by a pointer.

Right-click on the tab you *are* on works too — closing the chat you are in is the ordinary
case of closing one. Right-click on the left inset, on the `+`, on a `+N` count or on empty
space does nothing at all, and the `workspaces` bar has no menu: a workspace has neither a
transcript nor a chat to close.

**`F2` still has every one of those rows**, and that is deliberate: a right-click menu is
invisible until you try it, so it is a faster route to two things and never the only one.

**And whether right-click reaches charter at all is your terminal's decision.** tmux
forwards it — measured on 3.2 and 3.7c, with `[frame] mouse` off and on, with no binding
involved — but many terminal emulators serve their own context menu on button 2 instead of
sending it to the application. iTerm2's default profile is one. If yours does, nothing
breaks and nothing is refused: the menu simply never appears, and `F2` is unchanged.

A right-click that lands on a panel and opens nothing now costs you nothing either: it
leaves your keyboard on the harness, the same as a left click has since charter rebound
`MouseDown1Pane`. It did not always — tmux selects the pane under the pointer before
forwarding a right-click, so until charter wrapped that binding too a miss cost you one
click on the harness or an `F12`. Right-clicking a pane charter did *not* create still
opens tmux's own pane menu, exactly as it always did; see the mouse table above.

**The `+N` counts are the exception, and they open the palette.** A `+9` stands for names
that are not on the row, so there is nothing there to switch *to* — but it is the field
you are most likely to try, and charter now hands off to the surface that can answer:
pressing either count, or the `n/N` a very narrow bar draws instead of names, opens `F2`.
It is the top-level palette rather than the picker for that bar's noun, so it is one
keystroke more than it should be; the doorway row for your noun is already there with the
name you are on in it.

**The names on a narrow bar do not move when you switch between them.** Where the whole list
does not fit, the bar cuts it into pages and draws the page yours falls on — and that cut is
decided by the names and the width alone, never by which one you are on. So clicking a tab
redraws the same row with the `*` moved and every other tab still where you pressed it, which
is what makes a double-click harmless: the second press lands on the tab you just arrived at,
and that one does nothing. It also means the names a click can reach are the ones on your
page; `F2` is what reaches the other pages.

**A persona row's badges explain themselves.** Clicking the badge column puts the glyph
legend on the attention row (see *Every glyph on a persona row* above). It works on the
row you are already on, which is where the question usually comes from.

**A persona row's name switches to it**, for the tab bar's three reasons, unchanged:
charter acts on the press, the row you came from is still in the column one click away, and
nothing on the machine could finish a two-step gesture, because your typing goes to the
harness. Click the name `docs` and the frame adopts `docs` — the same thing `charter frame-switch
--persona docs` does, started the same way, with the same refusals reaching you on the same
status line. Clicking the *name* of the persona you already are does nothing, which is also
what stops a double-click switching twice — though clicking its badges still explains them.
Neither the `▪ personas 6` heading, the `…(+N more)` row (it stands for the personas that
are *not* drawn), the `no personas` line, nor anything below the column does anything at
all — the todos and the changes are readouts. On a sidebar too narrow to give the badges a
column of their own, every cell of a persona row is its name.

**The todos below the personas have a keyboard route, and that is all they have.** A short
sidebar draws `…(+5 more)` under the two or three todos it has room for, and `F2` → `todo:
read the next open todo` reads the whole list out from there, one press per todo, on the
palette's own header — it repeats like the two repo rows, so five hidden todos are five
Enters and one palette. It says where you are (`todo 3/7: Cut 0.55.0`), it wraps at the end
rather than pressing nothing, and on a plane whose cache holds only part of a very long list
it says that too (`todo 3/20 of 400: …`). Your `▪ todos N` heading is that same total, so
the header and the pane can never disagree. The `…(+5 more)` row itself still does nothing
when clicked: it stands for todos it does not name, so there is no todo for a click on it to
be about — the same case the persona column's own overflow row carries.

**The `F2 palette` hint is a button now, and so are the two nouns on the identity row.**
Click `F2 palette` on the attention strip and the palette opens; click `⬢ <workspace>` or
`◆ <persona>` on the identity strip and the same palette opens. All three go to the same
place on purpose: `⬢ alpha` names the workspace you are *on* and `◆ steward` the persona
you *are*, so a click on either can only mean *let me pick another*, and picking needs a
list to pick from. The palette is that list, and opening it finishes on the pointer alone —
it makes itself the active pane, so your keyboard reaches the rows it just drew.

Everything else on those two rows is inert, and each one is a readout rather than an
offer: the charter version and the context gauge on the identity row; the todo count, the
alert, the in-flight spinner, this session's news, and the selected repo's `▪ ledger · main
· clean` on the attention row. That last one is the sharpest of them — it is the readout of
a row you selected on *another* pane, so a click on it could only mean "select what is
already selected", which is the one gesture the repo table and the tab bars both already
refuse. On a row starved narrow enough to drop the hotkey field, or at `terse` density
where only one field survives, there is nothing clickable on the attention strip at all —
and inside your own tmux there is no hint drawn, because charter binds no key there.

**None of this needs a mouse, and none of it is a route that did not already exist.** Every
one of these clicks is a shortcut to something `F2` reaches: the palette itself, and the
`workspace:` and `persona:` rows in it. That is the rule charter holds itself to — a
pointer affordance always has a key or a palette row beside it — and it is why adding these
added no new obligation.

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

The moment the last of it finishes, the frame paints once more and then goes completely
still, and the panel goes back to waiting for a version bump. Nothing is polled to find
this out: charter records work when it *starts* (that is also what makes overlapping
agents visible at all), so the panel asks one question per tick — a single `stat` of that
tracker's own directory — and reads the records themselves only when that answer moves.
Measured on macOS/APFS: about 5µs per tick added to the ~26µs a panel already spends
checking the version file, five times a second, or roughly 0.003% of one core while idle.

**That one last paint is the whole of it, and until #727 it was missing.** The panel had
a reason to repaint for every tick work was running and no reason at all on the tick after
the last record cleared — nothing resized, the version had not moved, no event arrived —
so the pane kept the last thing drawn into it: a spinner stopped mid-turn over a count of
work that had already finished, which is exactly what a hung frame looks like. Measured
against a real client with the tracker directory empty, it held for 15 s and kept holding;
one sighting survived a detach and reattach and was still claiming `⠸ 1 running` fifteen
minutes later. It now clears **0.21 s** after the last record goes, on tmux 3.7c and at
the 3.2 floor alike.

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
charter's own (`tmux -L charter`), one server shared by every frame on the machine. On it,
a **workspace is a session** and a **chat is a window** in that session: `charter claude`
in a workspace that already has one open adds a second chat beside it rather than starting
a second session (unless somebody is attached to it — see below), and the two run side by
side with their own harnesses, their own personas and their own tool ceilings. A chat's id
is `<workspace>.<n>` — allocated, so two of them
can never land on the same state — and it is what `$CHARTER_SESSION_ID` holds inside that
chat.

One chat's harness dying ends **that chat's window** and nothing else; the other chats in
the workspace keep running. When the last chat's window goes, so does the session, which is
what returns `charter claude` to your shell.

**Opening a workspace you already have open puts you in it rather than beside it.** A tmux
session has one current window, so two terminals attached to one workspace look at the same
chat — and a second launch that added a chat and selected it therefore pulled whoever was
already there off what they were reading. So it does not. `charter -w foo` from a second
terminal, while somebody is attached to `foo`, **attaches you to what they are looking at**;
nothing is started and nothing moves. Where nobody is attached — you closed the terminal, or
never had one — the same command opens a chat exactly as before. It is `code <path>`'s
behaviour: the flag means one thing whether or not the workspace happens to be running.

To open a *second* chat in a workspace you are already in, press the `+` at the end of the
chat bar, or run `charter <harness>` from inside the frame. Both do the same thing: a new
window in the workspace's session, with your terminal on it and the chat you left running
behind you.

**A launch that names something to run still runs it.** Attaching answers "put me in
`foo`"; it cannot answer "run *this* in `foo`", so `charter frame -- <cmd>` and
`charter claude --resume <id>` open a chat and run what you typed, attached client or not.
Only a launch that asked for nothing but the workspace is answered by focusing one.

Which workspace is yours is decided on **this plane's own chat directories, never on a
session name**. One tmux server serves every plane on the machine and session names are bare
workspace names, so `default` — a name every plane has — names one session that any of them
might have opened. Charter matches on the pane id its own launcher wrote down for a chat in
`.charter/frame/`, which the server minted and only one plane holds. A workspace this plane
has never opened is never focused, whatever another plane happens to be calling its own.

### Switching workspaces — several open, one on screen

**Your terminal moves; nothing stops.** `F2` → `workspace`, a click on the `workspaces`
bar, or `charter frame-switch --workspace <name>` puts your client on another workspace's
tmux session. The chat you were in keeps its harness, its pid, its window and its
workspace, and so does every chat in the workspace you arrive at — so you can have several
harnesses working at once and look at one of them. Come back and they are where you left
them.

Measured on tmux 3.7c and at the 3.2 floor with a real client: `switch-client` moves the
client, every pane on the server is still there afterwards with the same pid, and the
`attach` process itself is untouched. The switch itself is about **5 ms** of tmux; what it
costs beyond that is one re-layout at each end — the chat you arrive at gets fresh panels
and the panels of the chat you left are torn down, in that order and for a chat switch's
reason (#844): you are already looking at the window you arrived at, so the tidying goes
behind it. Both happen for the same reason a chat switch does them at all — a background
window keeps stale geometry. You land on whichever chat that workspace was last showing,
which is tmux's own answer and not a record charter keeps.

**Switching is restricted to workspaces of this plane, and anything else is refused by
name.** One tmux server serves every plane on the machine: the operator's own socket had
eleven sessions from three different projects on it the day this was written, and `default`
is a name every plane has. Crossing to another plane's session would cross every isolation
boundary charter has — a different `CHARTER_ROOT`, different personas, different vaults,
different memory — so charter will not do it by accident and will not do it on purpose.

Which session is yours is decided by two things, and neither is a session name. Charter
matches the `%<pane>` id **its own launcher wrote down** for a chat in `.charter/frame/`,
and it refuses any session whose `@charter_plane` marker names another plane. The marker is
set once, by the launch that creates a workspace's session, and holds that plane's
`.charter` path. Sessions started by a charter older than this carry no marker; those are
still found by the pane record, which is what they were always found by.

A workspace with no session yet is **opened**, and then you are taken to it — see
*Switching a workspace moves your terminal* below for what that costs.

### Switching between them

`F2` → `chat` lists this workspace's chats with the one you are typing in marked and the
harness each is running beside it; choose one and the client moves to that chat's window.
Typing the chat's name at the palette finds the same row without the doorway, so a chat is
two keystrokes away at any terminal width. `charter frame-chat <chat id>` is the same
switch by hand.

**The panels move with you rather than existing per chat**, and that is correctness before
it is thrift. A tmux window that is not the current one keeps the size it had when it was —
measured on tmux 3.7c and at the 3.2 floor alike — so panels left running in a chat you
switched away from are not idle, they are drawing at a width that is not their window's.
The switch therefore selects the new window (tmux resizes it *at* that moment), splits
fresh panels into a window that is already the right size, and then tears the old chat's
panels down.

**In that order, and the order is the difference between a switch you wait through and one
you do not** (#844). Both re-layouts are independent, and the one you are looking at is the
one you have already arrived at: the window a switch lands you on holds nothing but the
harness pane at full window size, because its panels were killed when you last left it. So
every tmux call spent tidying the window you left is a call you spend staring at a bare
harness pane. Measured with a real client and four panels at 200x50:

| | your client moves at | panels appear at | bare harness pane for |
|---|---|---|---|
| tmux 3.7c | invocation 3 | invocation 15 → **8** | 78 ms → **38 ms** |
| at the 3.2 floor | invocation 3 | invocation 14 → **8** | 63 ms → **36 ms** |

Nothing is dropped and nothing is deferred to some later event — it is the same teardown,
in the same switch, done in the half where nobody is waiting on it.

**It costs about a seventh of a second in total.** Measured with a real client and four
panels:

| | tmux invocations | wall clock | times your terminal is repainted |
|---|---|---|---|
| tmux 3.7c | 58 → 23 → **22** | 314 ms → **142 ms** | 45 → **14** |
| at the 3.2 floor | 50 → 21 → **20** | 237 ms → **114 ms** | 41 → **12** |

That is the price of not keeping four panel processes per chat drawing at the wrong width,
and it is the whole cost — nothing is lost, no harness is restarted, and the chat you left
goes on running exactly as it was.

**It used to be twice that, and the difference is how many times charter spoke rather than
what it said** (#780). A tmux command is a round trip over a socket — ~5 ms on the machine
this was measured on, ~13.4 ms on the one that reported the switch as slow — and charter
sent every one of them on its own. The third column is what that looked like from the
outside: tmux redraws once per command *list* rather than once per command, so 58
invocations were **45 separate screen updates**, which is the *"jumping texts"* an operator
sees while the panels are torn down and split back one at a time.

Most of those commands read nothing back, so tmux takes them as one list. The four kills
and their four disarms are one invocation now; so is each end's window dressing, so are the
four splits, so is everything the four new panes are told about themselves, so are the
four respawn hooks, and — since #844 — so are a frame's panel rows and the harness pane's
own row. Nothing was dropped and nothing was reordered inside a list: the same commands, in
the same order, in 38% of the invocations and a third of the repaints. It is not silent: 14
updates is still four panel processes coming up and painting themselves, which is a
different cost and not this one.

**A write that changes nothing costs the same repaint as one that does**, which is why the
list is the unit worth counting rather than the command. Measured on 3.7c and at the 3.2
floor with a real client, 200x50: `resize-pane -x 22` on a pane that is *already* 22 wide
sends no resize to the pane at all and still repaints the whole client — 1672 bytes on
3.7c, 1811 at the floor, which is what a real resize costs. Three of them in one list cost
one of those. A pure `display-message -p` costs nothing at all, which is why the
measurement #510 puts between charter's two size passes is not something that had to be
collapsed away.

A switch is refused, with the reason on your own screen, for a chat this workspace does not
have, one whose window has gone, one charter has no pane record for, and the chat you are
already in.

### Where a switch says what it did

**On the attention row — the frame's own last row, not tmux's message line.** Whatever you
choose off `F2`, the outcome appears there for a few seconds and then gives the row back:
`charter: persona → forge`, or `charter: no persona 'forg' — have: forge, scribe`. A
refusal stays up longer than a success, because a refusal is the only one of the two with
nothing else confirming it — when a switch takes, the identity row above has already
changed to say so.

**It used to be a `display-message`, and that cost four seconds of frozen screen** (#729).
A tmux client does not redraw its *panes* while a message is up, and the freeze is exactly
as long as the message asks for. So every persona and chat switch put a sentence on screen
announcing a repaint that the same sentence was hiding — and so did a workspace switch,
while there was one: measured with a real client, the panes were correct at 0.5 s and the
operator went on looking at the previous workspace until 4.3 s — on tmux 3.7c and at the 3.2 floor alike, within 0.1 s of each
other. The same switch now reaches your eyes in **0.14 s**, with no stale window at all —
0.33 s until #780 batched the tmux commands it spends that time in.

The row is also the only surface that can be aimed at *your* frame. `display-message -t`
selects the target for format evaluation, not the client — measured on both versions, a
message aimed at a pane of one session was drawn on a terminal attached to a *different*
one — so on a machine running several frames a refusal about one could be drawn across
another. A panel reads its own frame's state, and every client attached to that frame sees
it.

### The two bars, and why they are off unless you ask

`chats` and `workspaces` are tab strips: the chat bar names every chat in this workspace
with yours marked, and the workspace bar does the same for the plane. Where the names do
not all fit the bar draws the page yours falls on and counts what is off each end
(`+5  *harness-wrapper   news-dispatch-guard  …  +7`); narrower still it says only where you
are (`2/3`). It never shows half a name.

**Neither strip is labelled.** They used to open with the word `chats` or `workspaces`, and
that cost 9 and 14 columns of the row the names are competing for. What tells them apart is
where they are and what is on them: the strips are adjacent and always in the same order, the
chat bar draws a `+` and a spinner and the workspace bar deliberately draws neither, and a
chat id ends in an ordinal. A label that never changes is chrome you stop seeing after a day.

Fifteen workspaces need 262 columns to fit on one row, so on a real terminal the bar is
usually drawing a page. At 120 columns it draws seven of them, at 160 nine, at 200 eleven.

**A strip that cannot fit its names on one row is given another one — up to three.** It is
one row whenever the names fit, so a plane whose strip is not overflowing never loses a row
to this; and the rows it does take come out of what your session has above its own 12-row
floor, so a short terminal grows nothing at all. Fifteen workspaces take two rows at 160
columns and three at 120, and every name on every row is clickable. Resize the terminal and
the strip follows: widen it past 262 columns and the strip gives its extra rows back.

Below tmux 3.3 that last part does not happen on its own. `window-resized` is a hook tmux
added in 3.3, so charter has nothing to trigger a recompute on and the strip keeps the
height it was given when the frame was launched — which is still the right height for the
width it launched at. `charter frame-resize`, run in the frame's own window, puts every
panel back at once, and it is the same command the frame already tells you about in that
band.

**The tab you are on is drawn as a block**, in reverse video — your own terminal's two
colours exchanged, so it is right on every theme and needs no `[frame] chrome`. The `*`
beside the name stays, which is what keeps the bar readable with `NO_COLOR` set, on a
console that has no highlight to give, and with the pane's output redirected to a file.

**`rules = "visible"` puts a separator between tabs.** The same key that decides whether
you see the seams between panes decides whether you see them between tabs; at the shipped
`hidden` the strip is exactly the row it always was, and at `visible` it spends one column
per seam out of the names it can draw. The separator is an ASCII `|` and not the box-drawing
`│` an IDE would use, deliberately: that glyph is East-Asian *Ambiguous*, so a terminal may
draw it two cells wide where charter measured one — and on a row whose clicks are resolved
by *column*, ten separators drawn a cell wide each would put your press ten columns off the
tab you aimed at.

**A chat whose harness is working spins where an idle one shows a blank.** The mark column
in front of each name carries it — `✢`, `✶`, `✻` and back — so the strip is exactly as wide
while three chats are thinking as while none is, and no tab moves under a press. The chat
you are typing in keeps its `*` instead: it is the one whose harness you can watch
directly, and the `*` is the only thing that survives `NO_COLOR`.

charter reads this off the harness's hooks, never off its screen. Claude Code reports the
start of a turn (`UserPromptSubmit`) and the end of one (`Stop`); a chat running a harness
that reports no stop — opencode and Codex both hook tool calls only — shows nothing at all
rather than a mark charter could raise and never lower.

Two costs, both real. A turn interrupted with Esc fires no `Stop`, so its tab keeps spinning
until the mark decays — **ten minutes**, refreshed by every tool call the turn makes, so
what has to elapse is ten minutes with no tool call at all. The same number is why a turn
that thinks for longer than that with no tool call blinks off and comes back on its next
one. There is no third answer available from a hook channel that reports prompts and tool
calls: charter can be late to stop claiming, or early, and it is set to be early.

**The chat bar ends in a `+`, and pressing it opens another chat.** Same workspace, same
harness you are already in, its id allocated for you — which is why it takes nothing and
asks nothing. It runs `charter frame-new-chat`, which is `charter <harness>` in this
workspace with one difference: it builds the frame without becoming your terminal, because
the process behind a click is not one.

It stops, and says why on the attention row, in four cases: your frame is a window in a
tmux you already had (charter makes no chats for you there — `charter <harness>` in the
workspace still does); charter cannot prove the workspace's tmux session is this plane's
rather than another project's; this chat records no harness charter can launch and your
plane declares no `[harness] default`; or charter cannot enter the workspace's directory.

**The workspace bar has no `+`, deliberately.** A new chat is nothing but a press. A new
workspace is a directory and a *name*, which is `charter workspace create` — and a picker
that creates on a typo leaves litter.

**You do not need a mouse for the strips either.** `F2` → `chat: the next tab` (and
`previous`, and the same pair for `workspace`) walks the strip one step, in the order it is
drawn, running the same command a click on that tab runs. It wraps, so every press moves
something; a plane with nowhere else to go says so rather than sending you where you
already are. Those four rows are the only route to *the next one* on a plane with `mouse =
false`, which is the default — and charter will not bind a bare key to it for the reason
the repo table's own rows give: a `bind -n` is server-wide and would take the key before
your harness sees it. Unlike the repo rows they do not leave the palette open, because a
switch moves your terminal to another window and the palette's pane is in the one you are
leaving.

**They are tabs: with `mouse = true`, clicking a name switches to it.** A chat tab does
exactly what `F2` → `chat` does — the same command, the same five refusals, the same
sentence on your screen when one fires. A **workspace** tab does exactly what `charter
frame-switch --workspace <name>` does: your terminal moves to that workspace, and the chat
you were in keeps running behind you. *Mouse is off by default* above has the rules a click
follows and why a tab switches where a repo row only selects; the short of it is that
clicking the tab you are on, or anything that is not a drawn name and not a count, does
nothing at all. The mark stays on the workspace **this chat** is in, because that is what it
names — switching does not move the chat.

**Neither is drawn unless a plane places it**, and that is deliberate rather than
unfinished: on the ordinary plane there is one chat, and a row saying so permanently costs
a row off your harness, a 24 MB panel process, and a share of every switch's tmux
commands — to draw a name `F2` already reaches in two keystrokes. Turn one on with a
`[[frame.component]]` table, which is also how it gets a key of its own.

**`[[frame.component]]` replaces your whole arrangement rather than adding to it**, so the
bar goes in a list of every panel you want — charter's four and then the bar. A file that
names only the bar gets a frame that is only the bar:

```toml
[[frame.component]]
use = "identity"

[[frame.component]]
use = "attention"

[[frame.component]]
use = "repos"

[[frame.component]]
use = "sidebar"

[[frame.component]]
use  = "chats"
edge = "top"
size = 1
key  = "F9"
```

Those first four are the shipped frame written out longhand, and
[Writing the arrangement out](#writing-the-arrangement-out) says what each of them is. File
order is split order, so the bar named last is split off last.

Both are given up by a short terminal well before the identity row, because the palette
reaches everything they show and nothing is lost but the reminder.

**You can edit the arrangement while the frame is running**, which is when you can see what
you are arranging. The file is re-read at the next re-layout — a density row or a
component's own key, `F2` back into the chat, or a terminal resize that changes the frame's
shape — and the running frame is made to match it: a component you added gets a pane, one
you removed loses its pane, and one that already has a pane keeps the pane it has. Charter
works that out by asking tmux which panes this window holds and which components they draw,
not by consulting its own notes, so a frame that has been rearranged several times in one
session still ends up with exactly one panel per component. Panes you split yourself are
not part of that reckoning and are never touched.

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
  Each is pinned only on a tmux that has it: `pane-border-indicators` arrived in tmux 3.3,
  and on 3.2 charter pins the other four and says nothing about a name that release does
  not know.
  With `[frame] chrome` left at its default this is the only place charter overrides a
  preference of yours rather than deferring to it; setting `chrome` adds a second — a
  background on charter's own panel panes, and never on the pane your harness runs in — and
  it also puts that colour BEHIND the frame's rules, because the cell between two panes is
  in neither of them and would otherwise stay your terminal's own background: a one-cell
  seam running between panels that are all the same colour. On tmux 3.7 and newer each
  panel's rules are its own colour and the three round your harness pane take the colour
  your components agree on; below that they are window-scoped and take one colour for the
  whole frame. Both border styles are always set to one value. The reason for
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

## Leaving: detach, close, quit — and reopen

**Closing the window detaches, and that is the exit that costs nothing.** tmux sessions
survive a client leaving, so the common way out loses nothing and needs no resume: the
harnesses keep running and `tmux -L charter attach -t <workspace>` puts you back. A terminal
that dies, a lid that closes and an ssh connection that drops all do this. `F2 → detach` is
the same thing without needing to know tmux's prefix key.

**`F2 → charter: quit` stops everything on this plane, and records it first.** It is the only
route — never a signal, never a closing terminal. Enter on that row opens a confirmation
rather than doing anything: the row that goes through, and under it one row per chat saying
what that chat will and will not get back.

```
quit · 5 to choose from
>   quit — stop 4 chats in 3 workspaces; 2 of 4 can resume the conversation
    alpha.1 · claude-code       conversation resumes
  * alpha.2 · claude-code       reopens empty — no session id recorded for this chat yet
    api.1 · opencode            reopens empty — opencode records no session id to resume …
    gone.3 · claude-code        conversation resumes · workspace 'gone' is gone — reopen…

  up/down move   enter choose   esc cancel   F12 back to the harness
```

The chat rows are listed refused — they are the warning, and pressing one does nothing. The
row that runs is where the cursor lands, because the palette puts the cursor on the first
row and a confirmation whose Enter does nothing reads as broken.

It **warns and proceeds** — it does not refuse. Refusing would leave you unable to quit
while any agent was working, which on a control plane is most of the time. The one thing that
stops it is being unable to write the record down: a quit that killed the plane after failing
to record it is exactly the invasive quit this exists to prevent, so that refuses and stops
nothing.

**`charter reopen` puts the recorded plane back.** Every workspace, every chat, each one's
persona and the directory it belongs in — and, for Claude Code, the conversation, by
resuming it. It attaches you to the workspace you pressed quit in, on the chat that was in
front of you. The record describes one quit and is consumed chat by chat, so running it
twice does not double your tabs — and if some chat could not be started, exactly that chat
stays recorded, so a second `charter reopen` retries just it.

**A chat comes back in its workspace, and is told when that is not where it had been.** The
record stores the directory the harness was actually standing in; the restore lands in
`workspaces/<ws>/`, which is the isolation boundary the chat belongs to. The two agree for
every chat opened through the frame. Where they disagree — a chat started by typing
`charter` in the plane root, before the tab machinery existed, and carrying that as its
directory ever since — a restore that reproduced it faithfully would be reproducing a state
already agreed wrong, on every launch rather than once. So it lands in the workspace and
says where it had been:

```
⚠ charter reopen: harness-wrapper.1 comes back in its workspace directory
  …/workspaces/harness-wrapper — it had been standing in /Users/aharon/IdeaProjects/charter
```

**A directory *inside* the workspace is kept exactly as it is**, because the test is
containment and not equality: a chat recorded in `workspaces/<ws>/<repo>` is standing in one
of that workspace's clones on purpose, and moving it up to the workspace root would be a new
silent move in place of the old one. Worktrees count as well, wherever `[plane] worktrees`
puts them. Nothing is said about a chat that was already where it belongs.

**The workspace directory is made before the chat is put in it.** A workspace with no
directory yet is ordinary — `default` is offered whether or not anybody has made one — and
falling back to the plane root there would be the same wrongness by a second route. A
workspace whose directory has been deleted is remade the same way, empty, and the chat's
line says so; its clones are still gone, and charter still never re-homes a chat into a
workspace it did not name.

Run it from an ordinary shell. **Inside a tmux you already have it refuses**, because charter
builds a frame there as a window on your own server and that launcher stays awake for the
life of each frame — so reopening several chats would stop at the first. It says so and
leaves the record alone.

**It refuses a plane that is already running, too.** The record describes the plane as it is
now (see below), so reopening a live one would put a second copy of every chat beside the
first, with a new id each so nothing on screen tells them apart. Attach to what is there
(`tmux -L charter attach`) or quit it first. The record is left alone either way.

### The plane is recorded as it changes, and `charter` puts it back

A quit is no longer the only moment the plane is written down. The process holding your
terminal — the launcher that attached, or `charter reopen` itself — watches the same version
bumps every panel already polls, and writes the record about two seconds after the plane
stops changing. So a machine that goes down, a terminal that is killed, or a laptop that
never comes back leaves a record of the chats you had, not a record of the last time you
quit.

It writes exactly what a quit writes: which chats existed, in which workspace, with which
harness, persona and directory, and which one was on screen. It does **not** capture
scrollback — that is one `capture-pane` per chat and it is what a quit is for — but it does
name a capture a quit already left behind, so a record taken while you are working never
withdraws an offer a quit made.

**Bare `charter` restores it, and only when there is nothing to join.** If any chat is
running anywhere on this plane, `charter` behaves exactly as it always did: it focuses the
workspace you are in, or opens a chat. The restore happens on the launch that finds nothing
running at all — the one after a restart — and it is the same `charter reopen` underneath,
so every per-chat rule above holds. What differs is what it says:

```
✓ charter: restored the 4 chat(s) this plane last recorded
```

one line, because you typed `charter` and wanted a terminal. If part of the plane could not
come back, the same line names the rest:

```
✓ charter: restored 3 of the 4 chats this plane last recorded — beta.1 did not come back
```

It does not offer `charter reopen` as a retry, and that is a limit worth knowing rather
than a wording choice: the chats that could not be started *are* left in the record — but
the process that just restored your plane is also recording it, so a couple of seconds
later the record is the plane that is running and those leftovers are gone. Open what is
missing yourself (`charter -w <workspace>`), or quit and use `charter reopen` while nothing
is running, which is the state that command is for.

**Two keys, both on**, because recording and restoring are separable: recording costs a
little I/O and changes nothing you see, restoring changes what `charter` does. A plane can
keep the record and still leave putting it back to you.

```toml
[frame]
record  = true   # write the plane down as it changes
restore = true   # bare `charter` puts it back when nothing is running
```

**`charter --fresh` opts one run out of both halves.** It does not restore, *and* it does
not record over the record it skipped — so an experiment, a one-off chat, or a plane you
want to walk away from costs you nothing you had. `charter claude --fresh` says the same
thing about a named harness. The record is still there for `charter reopen` when you want
it.

**Resume is Claude Code only, and the warning says so per chat.** Charter records a harness's
own session id from Claude Code's status-line hook, which is the only harness that supplies
one — so it is the only harness whose conversation charter can ask for back. A chat that
cannot be resumed still comes back: its directory, its workspace and its persona return, and
only the conversation is gone. A chat whose *workspace* has been deleted comes back too, into
a remade and empty one, and says the workspace was missing; charter never quietly re-homes a
chat, and a workspace it re-makes is the chat's own.

**`F2 → chat: previous transcript`** opens what a chat had on screen before it was last
stopped, in a pager, in a window of its own. tmux history dies with its session and
`claude --resume` re-renders the conversation rather than the screen, so a quit captures the
last 2,000 lines of each pane (at most 512 KB) into `.charter/frame/<chat>.transcript`. It is
**offered, never replayed** — the reopened harness's pane starts clean, because replaying a
previous run's output above a new run's prompt would present a session that is not running as
though it were. The row is refused, with its reason, until a quit has captured one.

**`F2 → chat: close` stops one chat and marks it so nothing brings it back.** That is the
whole difference from quit: quit records, close forgets. It exists because charter reads "no
recorded exit code" as *"we do not know it stopped — bring it back"*, which is what makes a
restart cheap and which would otherwise resurrect a chat you had deliberately finished with.
It also drops that chat's captured transcript, since a capture exists to be offered on the way
back. Closing does **not** refuse while that chat's harness is working: charter's in-flight
tracker records no chat on any of its entries, so there is no reading to refuse on, and the
confirmation says the chat will not come back instead of pretending to check.

**What quit stops is this plane, and nothing else on the machine.** One tmux server serves
every frame on a machine and session names carry no plane — `default` is a name every plane
has — so quit works from *this* plane's chat directories and kills one window at a time, never
`kill-server` and never a session by name. Two planes with a workspace of the same name can
share one tmux session, and quitting one leaves the other's chats running.

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
any shortage costs it, since it cannot spare its own divider. Then the repo table, on
either axis: a pane too narrow to draw a table, or too short to draw one repo row under its
`▪ repos N` heading. Then the two tab bars, if you have placed them. The identity row goes
**last**, with everything else, below half of either floor — where every panel drops and
the harness simply gets the whole terminal, the same choice `charter`'s own status line
makes when it runs out of width. So a narrow terminal degrades to the two strips on its
own; nothing has to be configured for it. A density change goes through the same floors, so
choosing `full` in a terminal with no room for a side panel gives you the edges that fit
rather than a failed split.

**A rung drops when the pane it would get cannot carry what the rung is for**, and among
rungs that still can, the ones whose facts something else reaches go first. That is why
`top` is last above `bottom`: it is one row saying which workspace you are in and which
persona you are being, and on a terminal with no sidebar it is also the plane's only
roster, while the bars are reminders `F2` replaces in two keystrokes and the table needs
two rows before it can name a single repo.

**This ordering changed, and the change costs the harness nothing.** A 120x10 terminal used
to spend three rows — two pane rules and `▪ repos 8` — on a repo count, having just decided
it could not afford the one row that names your workspace. Measured on tmux 3.7c and the
3.2 floor, at 120 columns on a plane with eight clones:

| rows | before | now |
|---|---|---|
| 20 | identity 1 · harness 12 · sidebar 12 · table 3 · attention 1 | *unchanged* |
| 19 | harness 12 · table 4 · attention 1 | identity 1 · harness 12 · table 2 · attention 1 |
| 18 | harness 12 · table 3 · attention 1 | identity 1 · **harness 14** · attention 1 |
| 17 | harness 12 · table 2 · attention 1 | identity 1 · **harness 13** · attention 1 |
| 16 | harness 12 · table 1 · attention 1 | identity 1 · harness 12 · attention 1 |
| 15 | harness 11 · table 1 · attention 1 | identity 1 · harness 11 · attention 1 |
| 10 | harness 6 · table 1 · attention 1 | identity 1 · harness 6 · attention 1 |

A table pane the rows cannot afford was floored at one row and still paid for its rule, so
from 16 rows down the exchange is exactly even — a count for a name — and at 17 and 18 the
harness gains the rows the table was spending on two repos. Above 19 nothing about your
frame moves.

A resize goes through them too, so a *running* frame degrades and recovers the same way a
launch would — with one exception. Dragging below half the floors does not take the last
panel away: a frame with no panels also has no resize hook, and that hook is the only thing
that would notice you making the terminal big again, so charter would have no way back.
It keeps what it has at that size. `F2` still turns everything off, because a keypress can
be followed by another one.

`bottom` is never dropped by those floors, and it is the only slot that never is: it is
the attention strip — one alert and the command that fixes it — which is the whole reason
a cramped terminal is worth framing at all. It is one row at every size.

`repos` is the one whose height moves — unless you pin it, which is a `size` on its
`[[frame.component]]` table and is described under [The arrangement, written
out](#the-arrangement-written-out). By default it is its content's: one row per repo (and
per worktree, in a single-repo workspace), plus its own `▪ repos N` heading, capped so the
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
`charter` command typed inside the frame all agree on one identity — so a workspace, a
persona or a lock chosen inside the frame is one thing chosen for the whole frame, not one
per pane.

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

So the order the panels read is: `$CHARTER_WORKSPACE` if you pinned one, then what the
launch recorded — the pin it was launched under, then the workspace it resolved — then
whatever the panel can resolve for itself (a frame launched by an older charter, still
running across the upgrade).

**`charter workspace use <name>` typed at the agent does not move the panels, and this
line used to say the opposite.** It writes the per-session pointer under the frame's id,
which is what makes every `charter` command in that shell act on the new name — `charter
clone`, `charter repos`, `charter ws current`, all of it. What it does *not* do is change
which workspace the chat is in, and for one release it did: that pointer was a rung of the
ladder the panels read, and the same ladder decides which chats a workspace has. So the
command quietly re-homed the chat — the chat beside it in the same tmux session could no
longer see it, and its own tab bar filled with the other workspace's chats, which the
palette then refused to open. **A chat belongs to its workspace for life** (spec §4j); a
conversation wanted elsewhere is a new chat, opened with `charter <harness>` in that
workspace. If you want the panels on another workspace, `F2 → workspace` moves your
terminal to it and leaves every chat where it is.

**Renaming a workspace does not orphan its chats, and that is not an exception to §4j.**
For one release it did: a chat's workspace is fixed at launch, so `charter workspace rename
alpha alpha2` left every chat in it still recording `alpha` — invisible to `alpha2`, and
with no route back, because the pointer that used to repair it had stopped being a rung
(#795). `rename` now repoints both of the records that ARE rungs, and bumps each frame so
its panels repaint. §4j forbids moving a chat *between* workspaces, because that makes the
harness's own cwd, files and history about a different plane; a rename moves no chat — the
clones and the memory travel with the directory, and only the name changed — so following
it is the chat keeping its identity. **Chat ids are not rewritten**, so after a rename you
may see `alpha.1` beside a later `alpha2.3`. That is cosmetic and deliberate: rewriting ids
would break every `$CHARTER_SESSION_ID` already exported into a live process.

**That pointer, and everything else keyed on a chat, dies with the chat.** A chat id is
`<workspace>.<n>` and the ordinal is handed back when the chat's state is reaped, so the
next chat in that workspace very often gets the same *name* — and a pointer, a lock, a
persona selection or a tool-gate marker left under it would be inherited by a conversation
that never chose any of them. What that cost, before it was fixed, was a chat launched with
`--workspace alpha` whose every command acted on `gamma` and which refused
`charter workspace use alpha` as **locked** (#731). So reaping a chat removes
`.charter/sessions/<chat id>.*` along with `.charter/frame/<chat id>/`. Sessions that are
not chats — a bare harness outside a frame, keyed by its own id — are untouched, and so is
every live chat.

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

**A cache that exists and cannot be read gets its own line**, because "it has not landed
yet" and "it will never land" are different facts and a panel that never gathers on its own
has nothing coming to correct the second one. If `gather.json` is truncated, hand-edited or
left behind by an older charter, `repos` says so and names the command that rebuilds it:

```
  unreadable repo cache · charter frame-gather --session <chat> --workspace <workspace>
```

Both flags are filled in from the frame you are looking at — they are required on the
command line precisely so a detached gather never guesses which frame it is for, which is
not a question you can answer from inside a pane. `F2 → refresh — gather this workspace's
repos, todos and changes again` runs the same thing without the typing. What charter does
not do is delete the file for you: a repaint reads, and the evidence of whatever wrote it
stays where it is.

The wait and the failure are told apart by a fact rather than by a clock — the file is
there and does not parse — so a gather that is merely slow is never called broken.

**A workspace that is not on disk at all gets its own line, ahead of all three of those**,
because all three are claims about a workspace that is there (#752). A frame outlives the
directory it draws — `charter workspace remove`, a `git clean`, a teammate's pull, a plain
`mv` — and none of those is something charter can hook, so the pane simply drew whichever
"empty" the cache happened to reach, `⋯ gathering…` forever being the reported one. It now
says what is true, with the command that changes it:

```
  no workspace <workspace> · charter workspace create <workspace>
```

Told apart by the same kind of fact as the line above it — `workspace.exists`, asked of the
filesystem at the moment the pane is drawn — and nothing is repaired by drawing: the pane
does not re-create the directory. A workspace that was *renamed* never reaches this line,
because the chat followed the rename.

The question is asked where the pane was about to say "nothing here" and **not** above the
cache, so a table gathered before the workspace went stays on screen until the next gather
empties it. That is deliberate: a panel reads its cache or says it has none, and a renderer
that overrode its own cache from a `stat` would be re-deriving, on every repaint, state the
gather owns.

## Configuring it

```toml
[frame]
slots = ["top", "bottom", "repos", "right"]
density = "full"
mouse = false
chrome = "off"
hotkey = "F2"
record = true
restore = true
history-limit = 50000
min-cols = 100
min-rows = 20
```

### How much frame

`density` is a **preset over `slots`**, not a second way of configuring the same thing.
There are three levels:

| level | edges | each panel says |
|---|---|---|
| `minimal` | `top` and `bottom` | the two one-row strips and nothing else — no repo table, no sidebar, so every row and column the frame is not using is your session's; `top` drops the charter version and `bottom` keeps one attention field, plus the hotkey hint |
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

**The hotkey hint survives `minimal`, and it is the one field that does.** The other three
are news about your plane and ranking them against each other is what the level is for; the
hint is not news, it is the one thing on screen that says how to drive the frame — and
`minimal` is the arrangement where the palette is the only route to the repo table, the
todos, the workspace, the persona and the way back to `full`. So `bottom` reads
`7 todos · F2 palette` rather than `7 todos`. A pane too narrow for both still drops it,
because that is width and not density.

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

**"The frame's own window" is load-bearing, and charter says so if you miss it.** Every
`charter frame-*` command acts on the frame it is run *inside*; typed in the window you
started from, or in any ordinary shell, there is no frame to act on. Each one now says that
on stderr and exits non-zero, rather than doing nothing and reporting success:

```
$ charter frame-toggle repos
charter: charter frame-toggle acts on the frame it is run inside, and this shell is not in
one — nothing was changed.
  Run it in the window `charter <harness>` opened. …
```

That covers `frame-chat`, `frame-density`, `frame-toggle`, `frame-chrome`, `frame-switch`
and `frame-resize` — the six you can type. The commands tmux fires for itself
(`frame-palette`, `frame-respawn`, `frame-gather`) stay quiet at 0, because a non-zero exit
inside a `run-shell` is what makes tmux print into your harness pane.

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

#### Every glyph on a persona row

A glyph only says something to someone who has been told what it says, and the sidebar's
persona column is the densest thing charter draws — five glyph classes on one 22-column
row. Here is all of them.

| | where | means |
|---|---|---|
| `▪` | heading | a section heading, and the count beside it is the whole section — `▪ personas 6` |
| `▸` | start of a row | the persona you *are*. Its row is also drawn in reverse video |
| `▫` | start of a row | a persona you are not. Dispatchable, adoptable, otherwise idle |
| `◦` | after the name | this persona **declares a vault charter cannot use here** — either this machine has no vault by that name, or it has one whose file does not exist yet. `charter persona create` writes the declaration; nothing writes the vault, so this is the ordinary state of a persona nobody has given credentials to, and it is not an error |
| `!` | after the name | the vault is registered and **unhealthy** — unreadable, or its provider is misconfigured |
| `✎N` | badge column | `N` **committed** memories. Drawn in `ok` |
| `◌N` | badge column | `N` **session-scratch** memories, not committed. Drawn in `warn` |
| `⚑` | badge column | the persona's **charter is a draft**, so charter generates no sub-agent for it and it cannot be dispatched. `charter persona show <name>` says what it still needs |
| `✗` | badge column | the persona's **config is broken** — a dangling `extends:`/`uses:`, or an inheritance cycle |
| `⚡N age` | badge column | `N` dispatches **in flight**, and how long the oldest has been running. A `?` after the age means that record is past the point charter presumes it dead |

Nothing after the name and nothing in the badge column is the healthy, quiet case, and
silence is deliberate: a column that said *ok* on every row would spend the sidebar's
width on the rows with nothing to report.

`ok` and `warn` above are the [accent words](#the-three-accents) — green and yellow unless
your plane says otherwise. Named rather than spelled here so this table stays true on a
plane that changed them.

**You do not have to come here to read it.** Click the badge column on any persona row —
the right-hand column the `⚑`s line up in — and the frame puts the legend on the attention
row for ten seconds. That is the same dwell a workspace switch uses to say what it did, so
it costs no extra row and nothing stays on screen. Clicking a persona's *name* still
switches to it; the badges and the name are two cells of one row and mean two different
things. It is one legend for every row rather than a reading of the row you clicked —
short enough for the attention row, with this table as the long version.

**`◦` and `⚑` together on most rows is normal, and it was worth writing down.** A plane
whose personas came from `charter persona create` and were never given a vault shows `◦`
on every one of them; a plane whose charters are still drafts shows `⚑` beside it. Six
personas, five flags, nothing wrong — which is exactly the reading that sent one operator
into `frame/slots.py` to find out what charter was warning them about.

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
  **The inverted row carries no colour of charter's own**, and that is what makes the
  sentence above true rather than nearly true: it used to keep the cells' greens and
  yellows, which put a colour chosen for your background on top of your *foreground* — a
  selected repo row drawn yellow-on-light-grey on any dark theme, on the one row charter
  uses to say "this is the one you picked". They are dropped inside the inversion and
  nowhere else, so the glyphs and the highlight say it and nothing is drawn on a pair
  charter cannot check. Bold and dim survive: neither can be wrong on a ground charter
  cannot see. A pane a *component* draws is its own — charter never inverts somebody else's
  row, and does not reach into one to delete a colour whose meaning it does not know.
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
which is one of the two ways you can see which pane is live — the other is on the border,
below, and it is the one that works when you have set no surface at all.

**Each word carries the text that goes on it.** `dark` is `bg=black` *and* `fg=white`;
`light` is `bg=white` *and* `fg=black`. It did not used to: `chrome` set a background and
left every cell no renderer coloured drawing in the foreground your terminal picked to sit
on *its own* background — so `light` on a dark terminal was white text on white, and `dark`
on a light terminal was black on black. Not "a background that clashes": panels whose text
is not there, on whichever of the two words was the wrong one for you.

Those two are the only pairing charter makes, and the line is where a measurement stops
being one. `black` and `white` are the **poles** of the sixteen — a theme is not free to
render its white darker than its black — so charter can say what goes on them. It cannot say
what goes on `bg = "blue"`, which is why a pane that names its own background gets no
foreground it did not ask for; `text`, below, is how that pane is told. And if you set
`text`, it wins: charter's pairing is the default for its own recipe, not a second opinion.

Two caveats, named rather than papered over.

`dark`'s focused shade is `brightblack`, and `brightblack` is the one word in charter's own
recipes whose shade a theme really moves — a dark grey on most, a light tan on at least one.
On such a theme `chrome = "dark"` draws a focused pane that is lighter than the rest and
still carries `fg=white`. That is the background *pair* straddling the ledger rather than
the foreground being wrong, it predates this pairing, there is no second dark shade in the
sixteen to move it to, and `text` overrides it in one line.

And the pairing reaches the panes and not the **rules** between them. With
`rules = "visible"` and `chrome = "light"` the rules come out `fg=default,dim,bg=white` —
your terminal's own foreground, dimmed, over charter's white. The shipped `rules = "hidden"`
gives the glyph the surface's own colour and never reaches that line, and `text` overrides
it; the reason it is not fixed here is that `bg=black` is both `chrome = "dark"`'s
background *and* a component's own `bg = "black"`, so pairing a foreground off the surface
would hand a word you wrote the foreground the paragraph above says charter will not choose
for you. That is a decision about the per-component half, not a line in the rule assembler.

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

### The seam between two panels

```toml
[frame]
rules = "hidden"    # or "visible" — `hidden` is the default
```

Once your panels have a background, the one-cell gap between two of them is the only part
of the frame that does not. tmux draws that cell from `pane-border-style`, and charter has
pinned it to a dim glyph in your own foreground since it started drawing its own chrome —
so four grey panels came out as four grey rectangles with a dark line between each pair.
Reported four times, off a screenshot each time.

**You cannot ask for fewer characters.** tmux always paints something on a border cell:
`pane-border-lines` takes `single`, `double`, `heavy`, `simple` or `number`, and there is no
`none`. So this is a contrast question. `hidden` asks for the glyph in the same colour as the
cell behind it, and the two panels read as one surface. Both halves of that name one word out
of your own palette, so how close to invisible it lands is your theme's answer and not
charter's — the same reason `chrome` has no `auto`:

```
visible  ESC[2m ESC[100m ─────   a dim foreground glyph, on the surface
hidden   ESC[90m ESC[100m ─────  the glyph IS the surface
```

**A plane with no surface is unchanged, byte for byte.** There is no colour for the glyph
to take, so `hidden` composes exactly the style charter always drew and your frame looks
the same as it did. That is why this default is safe where `chrome`'s is `off`: it only
does anything on a plane that has already written a `chrome` or a `bg` by hand, which is a
plane that has already said it wants panels rather than boxes.

One pane can opt out of a frame-wide `chrome` with `bg = "default"`, and a rule over *that*
stays visible: `default` means your terminal's own background, and there is no way to spell
"your terminal's background" as a foreground. `hidden` there would draw the rule at full
strength, which is the opposite of what you asked for, so it draws the dim one instead.

Say `rules = "visible"` if you want the seams back.

### The colour of the text on it

```toml
[frame]
text = "black"      # any of `bg`'s seventeen words — `default` is the default
dim  = false        # `true` is the default
```

`bg` has been configurable across seventeen words for a while. The text drawn *on* it was
not configurable at all — charter's greens, blues, dims and bolds were chosen against a
dark terminal, and a plane that paints its panes light is drawing them on a background none
of them was picked for. `brightblack` is a dark grey on most themes and a light **tan** on
at least one real operator's, which is how this was found.

**Charter cannot work this out for you and does not pretend to.** The sixteen names have no
fixed RGB — that is the whole point of naming them rather than indexing the cube — so
"is this readable" is a question about a terminal charter cannot see. It cannot see it for
the same reason `chrome` has no `auto`: a colour query through tmux gets no reply, and
`$COLORTERM` inside a pane describes the terminal that started the *server*. A charter that
computed a "legible" foreground from your background word would be guessing and calling it
a measurement.

`text` sets your panes' **default foreground**, and tmux resolves it the way it resolves the
background — from the same two options, on the same rectangle, at draw time. So it reaches
every cell no renderer coloured, charter's own `ESC[0m` returns to *your* colour rather than
the terminal's, and nothing in charter or in a component you wrote has to be told. Measured
on tmux 3.7c and at the 3.2 floor.

**Which case this actually fixes**, stated plainly so you can tell whether it is yours. It
fixes a pane whose background is *inverted* relative to your terminal's — a dark terminal
with `bg = "white"`, or a light terminal with `bg = "black"` — where the foreground your
terminal picked for its own background is now sitting on the opposite one. **If you have
written `bg = "black"` on a light terminal, this key is the one you want**, and it is easy
to miss that you are in that case: your panes are dark, your text is your terminal's own
dark foreground, and nothing but `text = "white"` moves it.

If your terminal is already light and your panes are light, your default foreground was
already dark and this key has nothing to do. What is hard to read there is the **accent**
colours, and `text` does not reach those — they are the three keys below.

`dim = false` stops charter reducing the contrast of its own chrome — the `ESC[2m` on muted
text, and the `dim` on the frame's rules. It is a key of its own rather than one more colour
because it is the one thing in the frame that is **wrong by construction** on a surface it
was not chosen for: bold adds weight and reverse swaps your own two colours, so neither can
be wrong on a theme charter cannot see, but dim moves text toward the background, and that
is only safe when there is contrast to spare.

It stays *on* by default, and the reason is information rather than caution: dim is the only
thing separating muted text from ordinary text in the frame — a tree glyph from a repo name,
a count from a heading — so a frame with it off everywhere is a frame with a flat hierarchy.
Turn it off when your surface makes it unreadable.

### The three accents

```toml
[frame]
ok   = "green"      # any of `bg`'s seventeen words — these three are the defaults
warn = "yellow"
bad  = "red"
```

charter draws through eight named roles — `heading`, `muted`, `selected`, `ok`, `warn`,
`bad`, `reset`, `inset`. Four are theme-safe by construction: bold, reverse, a reset and two
spaces. `dim` and the default foreground are the two keys above. These three are the rest.

They shipped un-configurable on the argument that they are slots in **your** palette — your
green, not a green charter picked out of the 256 — so renaming them is a preference rather
than a legibility fix. That argument holds exactly while your palette's green, yellow and
red are readable on the ground they are drawn on, and on a light terminal they are not:
**yellow on tan is the pair no palette was designed for**, and no amount of it being your
own yellow makes it legible on your own tan.

Charter still cannot work out which of the sixteen would be — the same three measurements
as above — so it asks, in the vocabulary you already answer `bg` and `text` in.

`default` is a real answer here and often the best one: it is the pane's **own** foreground,
which is your `text` where you set one and your terminal's where you did not. `warn =
"default"` means "stop colouring the warnings", and the frame still says everything it said
— every status in it carries a glyph or a word, and charter's own suite fails if one stops
being distinguishable with the escapes stripped.

The three reach charter's own panels *and* the status line, because both are drawn on the
same terminal, and they reach a component you wrote through `ctx.chrome["warn"]` — one
answer, so the frame's own attention strip and a provider's pane cannot come out two
colours. A component that hard-coded its own green is left exactly alone and still shows it:
charter cannot know what somebody else's green means, so it neither recolours it nor escapes
it.

**What these three name is charter's own green, yellow and red** — every place the frame
draws one of those to mean "fine", "look at this" or "broken". They are not a general
recolouring: a repo's identity colour is its own (charter cycles eight of the sixteen to
keep neighbouring rows apart, and two of those eight happen to be green and yellow), and the
marks charter draws in cyan, blue or magenta stay where they are. So a pipeline that is
`running` keeps its cyan `●` while `⚡3 running` in the attention strip moves with `warn` —
they are the same word drawn two ways because they were always two colours, and these keys
rename colours rather than concepts.

If you want a badge uncoloured rather than recoloured, `default` is the word: the glyph
carries it either way, which charter's own suite asserts by stripping every escape from each
panel and failing if a status stops being distinguishable.

`charter doctor` is **not** covered, and that is deliberate rather than an oversight: it is a
one-shot report printed into your shell, not a surface the frame paints, and its `✓`/`!`/`✗`
are the same three colours by coincidence of meaning rather than by sharing this table. A
`[frame]` key reaching a command that runs with no frame would be the wrong scope.

`NO_COLOR` still wins over all of these, and over everything else here.

### Why the frame ships with no surface at all

`chrome = "off"` is the default and stays there, and the reason is the one above: charter
cannot see your theme, so any colour it shipped would be wrong for whoever's theme differs.
That is not a hedge — it is what produced the tan.

The obvious way out is an attribute rather than a colour. **`reverse` is theme-independent**
by definition: it swaps whatever your own foreground and background are, so a reversed panel
is distinct from your work area on every theme without charter knowing which, and tmux's own
status line has used it forever. It was measured for exactly this, on tmux 3.7c and at the
3.2 floor, and it does not work. tmux **accepts** `window-style reverse`, stores it, and
reads it back verbatim — and puts nothing at all on an attached client's wire for it. The
same is true of `bold` and `dim`. A pane style honours colour and silently ignores every
attribute.

A panel could reverse its own rows instead, and that is worse rather than merely harder: a
pane style fills the whole rectangle — the cells no renderer wrote, on resize, on reattach —
while a fill a panel painted itself lasts until the pane changes shape. You would get three
reversed rows in a fifteen-row pane and twelve of your terminal's own until the next tick.

So there is no theme-independent surface to ship, and what you get instead is three lines:

```toml
[frame]
chrome = "dark"     # or "light" — whichever side your terminal is on
text   = "white"    # or "black" — the other side
dim    = false      # if your surface is light
```

`rules = "hidden"` is already the default, so the seams are gone without you writing
anything. Add a `bg` per component if you want your regions told apart (below).

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

**Each panel's edges are its own**, so the rule between two panels is their shared colour.
That needs tmux **3.7 or newer**, where `pane-border-style` is a per-pane option.

**The three rules that run round your harness pane are painted too**, and they take the
surface your components **agree** on — nothing at all where they name different backgrounds,
because those three rules have a different panel on the far side of each of them and no one
colour would match all three. tmux draws each border cell from exactly one pane's options
and your harness's pane is the one those three are drawn from, so leaving them out does not
give your session dark edges: it gives the horizontal rule that runs under the top bar two
colours, dark as far as your pane's corner and surfaced the rest of the way. Charter draws
the foreground, weight and indicators of those same three rules anyway; the background is
the same cell.

**Inside** your harness pane is untouched at every level — a pane's interior is
`window-style`, which charter sets on its own panels and on no other pane, so your session
keeps your terminal's own background whatever `chrome` says.

On tmux 3.2 to 3.6 `pane-border-style` is a window option only, so there is one rule colour
for the whole frame: the one your components agree on, and the frame-wide `chrome` colour
when they do not. A plane that sets no `bg` at all is unchanged either way: every pane is
the frame-wide colour, so the rules are too.

A pane's two edge colours are always identical, and that is deliberate: tmux draws the
border of the active pane from a second option, and letting the two differ is exactly the
defect that put charter in charge of these options in the first place — a rule that changes
colour halfway along, where it passes the active pane's corner. So which pane is live is
never said with a border **colour**.

**It is said with a glyph.** tmux puts a small arrow on the borders of the active pane —
`pane-border-indicators arrows` — pointing into it, and charter pins that on. Read off a
real client, charter's own shape, the same rule with the focus in three places:

```
harness active   ESC[2m ─↑─────────────────────────────────┴──────────────
sidebar active   ESC[2m ───────────────────────────────────┴─↑────────────
footer active    ESC[2m ─↓─────────────────────────────────┴─↓────────────
```

One escape, at the start of the row, and none anywhere else in it: the rule is the same
one-colour rule charter has always drawn and the only thing that moved is which cell holds
an arrow. That is why this is not the two-coloured-rule defect wearing a new hat — a cue
made of a second *style* does put a seam mid-line, which is measured and is the arrangement
charter did not take.

It exists because without it there was **no answer at all**. With the shipped `chrome =
"off"` and no `bg`, every pane has your terminal's background, both rule styles are one
value, and nothing on screen said where the keyboard was — which matters more than it
sounds, because `F12` exists for "you are in a pane that has stopped answering".

Over a surface with the shipped `rules = "hidden"` the arrow is drawn in the colour of the
cell behind it, like the rest of that rule, so it disappears exactly where the pane's own
one-shade-off background is already telling you. The cue shows up where there is no other
one, and nowhere else. Below tmux **3.3** the option does not exist, charter does not send
it, and that plane gets the frame it had.

`chrome = "off"` with no `bg` anywhere puts the rules back to your terminal's own, and
`NO_COLOR` takes the background off them while leaving the frame's rules drawn.

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

**An arrangement is the whole list, every time.** These tables do not add to the shipped
frame, they replace it — so every example below writes all four out, and so must your file.
Naming one panel gets you a frame with one panel in it.

A component can be kept in the arrangement and not drawn:

```toml
[[frame.component]]
use = "identity"

[[frame.component]]
use = "attention"

[[frame.component]]
use     = "repos"
visible = false

[[frame.component]]
use = "sidebar"
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

For one of charter's own four, `edge` may be written down and may only say what the
component already declares — `edge = "right"` on the sidebar, `edge = "top"` on identity.
Charter derives the built-in geometry from those declarations, and nothing between
`charter.toml` and tmux carries a per-plane override for them, so a value charter cannot
honour is not quietly accepted and ignored: it takes the arrangement out of play. Writing
it is still worth it if you like your config explicit — and it is what makes the two forms
round-trip.

`size` is the same for three of the four: `size = 1` on identity, `size = 1` on the
attention strip, `size = 22` on the sidebar, and any other number takes the arrangement
out of play for the reason above. **That reason is about those three and not about
built-ins in general** — the two bars are a built-in charter derives nothing for, and they
take a real height like any component a package supplies (see below).

**`size` on the repo table pins the strip's height.**

```toml
[[frame.component]]
use = "identity"

[[frame.component]]
use = "attention"

[[frame.component]]
use  = "repos"
size = 15

[[frame.component]]
use = "sidebar"
```

The table is the one panel charter sizes to its content — one row per repo, so a plane
with two clones gets a two-row strip and a plane with fourteen gets a fourteen-row one.
That is the default and it does not change. What `size` says is *stop doing that*: the
strip is 15 rows whether you have one clone or thirty, which is what you want if your
clone count moves and you would rather your session did not shuffle under it every time.
Charter can honour it here because this height is not derived once at import like the
other three — it is recomputed from your arrangement at every launch and on every terminal
resize, so there is somewhere for your number to be read.

It is a number of cells: a whole number, at least 1. Anything else — `0`, a negative, a
`true`, a string, a float — takes the arrangement out of play like every other value
charter cannot honour.

**The chat and workspace bars take a real height too, by the same rule.** Charter places
neither of them by default, so neither has a slot whose geometry is derived at import;
their height is read off your arrangement at every launch, exactly like the repo table's
pin. So `size = 3` on `chats` gives you a three-row bar. It is a FLOOR rather than a
ceiling: a strip whose names need more rows than you pinned still grows into what the
window can spare, and a strip whose names fit on one row still draws one and leaves the
rest as empty surface — a thing you may want under a `bg`. The same whole-number rule
applies: anything charter cannot turn into cells takes the arrangement out of play.

**Your pin is still capped so your session keeps its 12 rows.** tmux does not refuse an
over-large pane height, it grants it out of the neighbour, and the neighbour is your agent
session — `size = 40` in a 20-row window would leave it one row tall. So a pin is what the
strip *wants*, and what the window can spare is decided afterwards, exactly as it already
is for a fourteen-repo plane on a short terminal. On a terminal with room, you get your
number; on one without, you get what is left, and the strip is dropped entirely below the
width its table needs — the one
[When the terminal is too small](#when-the-terminal-is-too-small) names.

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
use = "identity"

[[frame.component]]
use  = "acme.metrics"
edge = "right"
size = 12

[[frame.component]]
use = "attention"

[[frame.component]]
use = "repos"

[[frame.component]]
use = "sidebar"
```

File order is split order here as everywhere else, so `acme.metrics` is split off before
the attention strip and the strip is inset beside it. Charter's own four are written out
again because this list is the whole arrangement — leaving one out is how you turn it off,
not how you leave it alone.

**`edge` and `size` are required here, and they win.** Required, because the only way to
ask a package where it would like to sit is to import it — and your config is resolved by
`charter --version` as much as by `charter frame`, so charter will not run a stranger's
code to answer a geometry question on every command. And they win, because your
`charter.toml` is committed and shared: a package's own preference overruling it would mean
one file drawing two different frames on two machines depending on what happens to be
installed. Arrangement is committed; execution is local.

**So a component charter did not write declares `Fixed(n)`, and charter refuses the other
two rather than quietly substituting one.** `Content()` is "as tall as my own content" and
`Fill()` is "whatever is left", and charter can honour neither for a package: measuring
your content means importing your module and calling your `render` on every command that
reads a config, and the frame has exactly one pane that takes what is left — the repo
table, which is what gives `resize-pane` one boundary to move. A component that declares
either gets a pane saying so, named, with the rest of the frame drawn around it. Your
`Fixed(n)` is then the default for a rectangle nobody configured, and the committed table
is what picks the number.

`Fill()` is still the right — and required — policy for exactly one part of a composite: a
part is drawn inside its parent's pane rather than split for, so it has a parent with a
remainder to give it. It is *placing* that needs a number.

**Your distribution depends on `charter-cp`, at build time and at run time.** Discovery
does not: charter reads the entry point group out of your distribution's metadata and
imports nothing, so a provider you have installed but not placed costs a frame nothing at
all. Construction does: what charter accepts is a `charter.frame.component.Component`, and
the only way to hand it one is to import the class and build one. That is a trade taken on
purpose rather than an oversight — `Component.__post_init__` is where a mistake in your
component is refused with a message naming it, before charter has split a pane, and a
duck-typed shape would move every one of those refusals to the moment your pane draws.
`API_VERSION` is the other half of it: one integer declared on both sides, so a provider
built against a shape charter has since moved does not load, rather than half-working
inside somebody's frame.

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

**Those eight are also the whole of what survives the trip.** Charter contains every row a
component outside its own tree returns, before it reaches the terminal — and containment
means an escape comes out as the visible characters of itself, so `\x1b[2J` prints as
`\x1b[2J` instead of erasing somebody's pane. **What is exempt is the vocabulary above, and
it is exempt as a property rather than as a spelling**: an SGR whose every parameter is one
of these roles' comes through, however you spelled it. `\x1b[1m`, `\x1b[01m` and `\x1b[m`
are all charter's own vocabulary; so is `\x1b[1;32m`, which is two of the roles in one
escape and something charter never writes itself.

Everything else does not. A colour outside your palette's sixteen — `\x1b[38;5;236m`,
`\x1b[38;2;30;60;90m`, a background like `\x1b[41m` — a cursor move, an erase, an OSC title
string, a newline: each arrives as its own text. And one parameter charter does not serve
takes the whole escape with it, so `\x1b[1;41m` is contained entire rather than half-kept.
Under `NO_COLOR`, or a pane that is not a terminal, nothing is exempt: the recipes are
already empty there, so a component built on them is unaffected, and one that hard-coded an
escape has it contained like any other — charter emits no SGR from the frame, including on
somebody else's behalf.

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
fiction — and **`charter frame-probe` and `charter doctor` name the one key that did it**,
so you are not re-reading the file line by line. The launch itself stays silent, for the
reason every other standing limit does: a warning printed microseconds before tmux switches
to the alternate screen is not readable. A `key` charter will not bind, a key two components both claim, and a key equal to
your frame's own `hotkey` are on that list too — and so are a `bg` that is not one of the
seventeen words, a `pad` outside `0` to `5`, and a `size` charter cannot give the component
(any number but its own on the three whose height is fixed, and anything that is not a
whole number of cells on the repo table).

Precedence, most explicit first: `[[frame.component]]`, then an explicit `slots`, then
`density`, then the shipped default.

`slots`/`density`/`mouse`/`chrome`/`hotkey`/`record`/`restore` are spelled the same on both sides. `history-limit`,
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
how to get back in (`tmux -L charter attach -t <workspace>`) rather than leaving you to
remember the flags. The workspace is the session; tmux puts you back on whichever of its
chats was last in front of you.

```
charter · 16 to choose from
>   workspace: alpha — pick another    cannot switch: a chat belongs to its workspa…
    persona: steward — pick another
    detach — leave the harness running
    repo: select the next row
    repo: select the previous row
    chat: the next tab
    chat: the previous tab
    workspace: the next tab
    workspace: the previous tab
    todo: read the next open todo
    density: minimal
    density: normal
  * density: full
  * chrome: off
    chrome: dark
    chrome: light
    chat: previous transcript          no previous transcript for this chat — one …
    refresh — gather this workspace's repos, todos and changes again
    charter: quit — stop every harness on this plane
    chat: close — stop this chat and do not bring it back
```

**The two rows that stop something are last, and that is a guard rather than an ordering
taste.** The cursor starts on the first row that can run, so a destructive row at the top of
the list would be one `F2` `Enter` away. Every harmless row charter has keeps the top of the
list; see *Leaving* below for what those two do.

```
  up/down move   enter choose   esc cancel   F12 back to the harness
```

**Everything is listed, including what cannot run right now — with the reason beside it.**
An option you cannot see is one you cannot ask about, so a row that is refused stays and
says what would make it available. The reason is the right-hand column. A refused row is
listed lower than a row you typed the whole name of, and never dropped.

**Every row reserves the `*` column, whether or not it has a mark to put in it** — the
frame's one-inset rule, applied here. Four cells stand in front of every row's text: two
for the cursor and two for the "you are on this one" mark, in that order, whether or not
the row has either. So `detach` and `density: full` start in the same column, and the
distance from the cursor to the text is the same on every row of the list.

**Four of those rows are doorways.** `workspace:` and `persona:` say which one this frame is
on, and `charter: quit` and `chat: close` open a confirmation; Enter on any of them replaces
the surface **in the same pane** with a picker, which is this same surface over a different
set of rows. Type to narrow it exactly as you would the
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
>   zeb                       persona
    zeb-api                   workspace
    zebra-ui                  workspace

  up/down move   enter choose   esc cancel   F12 back to the harness
```

**The name you typed in full is the row Enter runs.** That is the whole of the ordering
rule and it has two clauses: a row whose name is exactly what you typed comes first, and
between two of those the one that can actually run comes before the one that cannot.
Everything else keeps the position the list already gave it — the doorways in their order,
then the actions in theirs, then the names — so nothing shuffles under you as you type, and
with nothing typed the list is the one pictured above, unmoved.

It matters because names overlap. `zeb` above is a persona and a prefix of two workspaces.
A chat id is `<workspace>.<n>`, so on a plane with a workspace `alpha` the `chat:` doorway
holds `alpha` in its own title — and it used to win, which meant `F2` `a` `l` `p` `h` `a`
Enter reached a doorway about chats instead of the workspace whose name you had just typed
in full.

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

**Switching a workspace moves your terminal, not your chat.** `F2` → `workspace` lists
the workspaces of this plane with the one this chat is in marked; choose another and your
terminal moves to that workspace's session. **A chat belongs to the workspace it was
opened in for life** — its id is `<workspace>.<n>`, and the harness inside it has that
workspace's directory as its cwd, that workspace's files open and that workspace's work in
its history — so nothing about the chat you left changes. It keeps running, keeps burning
whatever it was burning, and is still there when you switch back. That is the point:
several workspaces open at once, one on screen.

**A workspace that is not open yet is opened, and you are taken to it.** Most of the tabs
in the bar are like this — the bar lists every workspace the plane has, and only the ones
you have been in today are running. Clicking one starts it: a chat, a harness, the panels,
and then your terminal moves there. It is the same thing `charter <harness> --workspace
<name>` does, which is what this used to print for you to go and type.

The harness it opens with is **the one this chat is running**, because that is the only
answer a tab can carry — a tab names a workspace and nothing else. If charter has no record
of it, the plane's `[harness] default` is used, and with neither the click is refused by
name rather than starting something you did not choose.

What a click costs: one harness process at its own prompt with nothing sent to it, one chat
directory, and a set of panels. Nothing is asked first, deliberately — the frame delivers no
key events to a panel, so there is no confirmer available to one, and a tab that needed two
clicks would be the "select, then confirm" that clicking a tab is supposed to replace. The
undo is `F2` → `chat` → close.

A switch is still refused, with the reason on your own screen, for a name that cannot name a
workspace, a workspace this plane does not have, the workspace you are already in, and a
terminal that has moved nowhere. Two refusals are new, and both exist because opening is not
free:

* **A workspace whose name is already running on this machine under another plane.** One
  tmux server serves every plane on this machine, so `default` usually exists several times
  over. Charter will not add your chat to another project's session, and tells you to attach
  to it by hand if it turns out to be yours.
* **Nothing attached to the frame you are switching from.** If you have detached, or an
  agent with no terminal is driving the frame, the click is refused before anything starts
  rather than launching a harness into a workspace with no client to move into it.

**Switching a persona from the picker moves the frame, and says so.** Choosing one writes
the choice under the frame's own id and bumps the frame so every panel repaints, and a
one-line message lands on your own screen saying what happened.

**A pinned frame says so before you press anything.** A frame launched with
`$CHARTER_PERSONA` set is *pinned*: that variable is in every panel pane's environment for
as long as the pane lives, and nothing charter can write outranks it — so the persona row
carries `cannot switch: $CHARTER_PERSONA pins this frame to '<name>'` and opens no picker,
rather than offering a list of moves that would not happen. Nothing here creates a persona
either — an unknown name is a refusal with the existing names beside it, never an implicit
create.

**A name that is not a name is drawn and never run.** A workspace or persona is a directory
somebody can add in a commit, and a filesystem forbids only `/` and NUL — so a name can hold
a newline, a U+2028, an escape sequence, a quote or a `#`. Every name is made one line
before any column is measured (#472), so it is exactly one row on screen; and the switch
re-checks it against the same alphabet `charter workspace use` does, so a name charter would
not accept is refused with a message rather than acted on.

**The session lock is released by `charter workspace unlock`, and by nothing on the
palette.** `charter workspace use` locks the session to what it selected so a workspace
cannot be swapped out from under a running task, and a launch that asked you to pick one
takes the same lock. `F2 → workspace` used to override it — that was the frame's way out
while a workspace switch was a thing a frame could do — and now there is nothing to
override, so the way out is the one the launch names on the line that takes the lock:
`charter workspace unlock`, typed in the frame's own shell.

**Pressing `F2` again while the palette is open REOPENS it.** `bind -n` is tmux's root key
table, so tmux matches the key before any byte reaches the palette's pane — the same
property that makes `F12` work against a pane that has stopped answering — so the second
press really does run charter again. What it does is close the palette that is open and
draw a fresh one, which is also the more correct of the two: the list is resolved against
your plane at the moment it opens, so a reopen re-reads a workspace, persona, density or
surface that has moved since.

There is never more than one palette pane on a frame's window. charter finds an open one by
asking tmux — the overlay's pane carries a tmux pane option, so the answer is tmux's and
disappears with the pane, and there is nothing charter can believe that is out of date.
A second press therefore never refuses and never does nothing, which matters most on the
press you make because the first one seemed not to register.

### What a component can be told about

A component declares which events it handles, and supplies one function to receive them:

```python
component.Component(
    id="acme.metrics", title="Metrics", edge="right", size=component.Fixed(12),
    events=("focus", "blur"),
    on_event=lambda ev: remember(ev.kind),   # answer truthy to repaint
    render=lambda ctx: [line_for(remembered())],
)
```

The two go together or not at all. A component that declares `events` and supplies no
`on_event` is refused when it loads, and so is one that supplies `on_event` and declares
nothing — a declaration with nothing behind it looks exactly like a kind charter never
fires, and that ambiguity is the thing this refusal exists to remove.

**Five of the six kinds fire today.**

| kind | fires |
|---|---|
| `resize` | Always. Your pane's rectangle changed. |
| `focus`, `blur` | Inside a frame charter launched, on a terminal that reports focus. |
| `click`, `scroll` | Whenever a pointer report reaches the panel — see *Mouse is off by default*. |
| `key` | Never. Declarable, and delivered nowhere. |

`focus` and `blur` need tmux's `focus-events`, which charter turns on for its own server
and cannot turn on inside a tmux you already have (see *Inside a tmux you already have*) —
and your terminal emulator has to report focus in the first place. `key` is not delivered
because your harness owns the keyboard: tmux routes typing to the active pane, which is the
harness's, so the only keystrokes a panel could see are the ones you typed into the wrong
pane. `click` and `scroll` are the subject of *Mouse is off by default* above: charter
delivers them wherever they arrive, and whether they arrive at all is decided by your
`[frame] mouse` setting and by the harness.

**What a pointer event tells a component.** `ev.row` and `ev.col` are cells of the
component's own rectangle — the one `ctx.width` and `ctx.height` describe — so any `pad`
you set is already accounted for, and a pointer event landing in that margin is not
delivered at all. Neither is one on a pane charter cannot currently measure, which is the
moment the pane is showing `charter: pane size unknown` rather than the component.

`ev.name` is the button (`left`, `middle`, `right`) or the direction (`up`, `down`).
Modifier keys are not reported — a shift-click is a `left` click. The extra buttons a mouse
may have (the thumb buttons, and the horizontal wheel a trackpad swipe reports) are not
delivered: charter has no name for them, and reporting one as a `left` click would be
worse than reporting nothing.

A click arrives as two events, a press then a release, told apart by `ev.pressed` — and
either can arrive without the other, because tmux routes each one by where the pointer was
at the time. Act on one of them.

The wheel arrives as fast as it is turned, and a handler that answers truthy repaints for
every event it answers truthy to — so a component that redraws on `scroll` redraws as often
as somebody scrolls it. That is one pane's work and it stops when they stop, but a handler
that only needs to move a cursor should say so by returning falsy when nothing on screen
would differ.

Declaring a kind that does not fire is not an error and never becomes one. A declaration
says what a component *handles*; it is not a promise from charter that the event happens.
Give anything a pointer could do a key or a palette action as well — on a plane where the
harness never asks the terminal to report, the pointer is not a route to anything.

**A handler is told what happened; `render` is what draws.** Nothing `on_event` returns
reaches your pane — the return value only says whether to repaint, and the repaint runs
your ordinary `render`. A handler is handed no `ctx` either: the plane is read at the
repaint that follows, from the one snapshot every component in that repaint shares.

**A handler that raises costs the component its events, and the pane says so.** It is
retired — no further events are delivered to it — and its pane draws the reason instead of
its rows, the same answer charter gives a provider that fails to import.

A handler that never returns freezes that one pane. The other panels keep painting, your
harness is untouched, and `F12` still returns you to it — but that pane also keeps the
terminal mode charter set for it, so typing into it echoes nothing until you kill it.

**A part of a composite cannot declare events.** A composite draws its parts inside its own
pane, so a part is never placed on the frame and charter dispatches to the component that
owns the pane. A part that declared events would receive nothing, which is the thing this
release exists to stop happening — so it is refused when it registers, and the message says
to declare them on the composite instead.

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
says so on the line after your answer, naming the way out in the same sentence: `charter
workspace unlock`, in the frame's own shell. (It used to name `F2 → workspace` first; that
route no longer releases a lock — a workspace switch moves your terminal to another session
and leaves this one locked to the workspace its commands act on.)

**It never asks twice, and it never asks a script.** Your choice is written as the
terminal's own pointer, so the next launch from that terminal has an answer and goes
straight in. And the prompt is reached only on the interactive path: `--no-frame`, a
redirected stdout and a stdin that is not a terminal each return before it — `charter
claude` from a script or another agent cannot block on it. `--workspace <name>` names one
outright and skips the picker; `--pick` asks even when something already chose.
