# What tmux actually does with mouse, focus and popups — measured

**Date:** 2026-08-26 · **Status:** measurement, complete
**Answers:** Phase 1 Task 1 of `docs/superpowers/plans/2026-08-26-phase1-component-registry.md`
**Tests:** `docs/superpowers/specs/2026-08-25-agentic-ide-foundation.md` §4c (mouse routing)
and §4g (`display-popup`)

Both hypotheses were tested against real tmux servers driven by a real pty. Where a
hypothesis was wrong the wrong half is stated first.

---

## 0. The two verdicts, up front

**§4c — mouse routing. The mechanism works. The reason given for choosing it does not.**

The mechanism survives exactly as written: with tmux's own `mouse` OFF, a program in a pane
that writes `\x1b[?1006h\x1b[?1000h` receives SGR click and scroll reports for its own
rectangle, translated to pane-relative coordinates, **without becoming the active pane** and
**without tmux stealing the wheel for copy-mode**. Measured on tmux 3.1c, 3.2 and 3.7c, on
charter's private socket and on the operator's own default socket.

What is **wrong** is this clause of §4c:

> "…which also preserves the drag-select the comment worried about losing."

It does not. tmux enables mouse reporting on the outer terminal from the **active pane's**
mode alone. So there is no state in which charter's panels can receive pointer events *and*
the operator's terminal still does its own drag-select — the moment any mouse-requesting pane
is active, `\x1b[?1006h\x1b[?1000h` goes out to the real terminal and native selection is gone
for the whole window. Turning tmux's `mouse` off does not avoid the trade
`instance.FRAME_FIELDS` names; it only makes the trade *conditional on which pane is
focused*. §5 below has the measurement.

The same fact has a second, sharper consequence: **charter's panels get pointer events only
while the active pane is one that asked for them.** If the harness pane (Claude Code) does
not request mouse reporting, the outer terminal is never asked to report, and a click on a
charter panel produces no bytes at all — there is nothing for tmux to route. Charter does not
control what the harness requests. §5.3.

**§4g — `display-popup`. Three of four claims hold; "receives focus events" is wrong, and the
resize claim is version-dependent in a way that breaks the 3.2 band charter promises to
launch on.**

| §4g claim | Verdict |
|---|---|
| runs a command in a floating pane, 3.2+ | **CONFIRMED** — and 3.2 confirmed as the exact floor, by running a 3.1c binary |
| "with its own tty" | **CONFIRMED** — a distinct `/dev/ttys*`, `isatty()` true both ways |
| "and its own input, mouse included" | **CONFIRMED** — requests 1006/1000 itself, receives click and scroll, popup-relative |
| "confirm what `display-popup` does with … focus" | **WRONG.** A popup's own program never receives `\x1b[I`/`\x1b[O`, even with `focus-events on` and the client's focus genuinely toggling. The **pane underneath** receives them (tmux 3.6+) |
| behaves sanely on `window-resized` | **True on 3.7c. False on 3.2** — where any client resize **kills the popup with SIGHUP** |

And one thing nobody asked about that will bite Phase 2: **inside a popup, `$TMUX_PANE` names
a pane tmux will not resolve, and says so with exit code 0 and empty output.** §8.4.

---

## 1. How this was measured

Every measurement drives a real tmux server over a real pty, so the bytes recorded are the
bytes that crossed a terminal.

```
[driver]  pty master  <->  pty slave  <->  `tmux -L <socket> attach -t s`
                                              |
                                              +-- pane %0 -> probe program (its own tty)
                                              +-- pane %1 -> probe program (its own tty)
```

* The **probe** is a Python program run as a pane's (or popup's) command. It puts its tty in
  raw mode, writes the mode-enable string under test, logs its own `ttyname`, `isatty`,
  `TMUX_PANE`, `TERM` and `TIOCGWINSZ`, installs a `SIGWINCH` handler, and then appends
  `repr()` of every byte it reads.
* The **driver** creates the pty, sets `TIOCSWINSZ` to 120x30, spawns the tmux client on it,
  records everything tmux writes to the client, and **injects** SGR mouse reports into the
  master exactly as a reporting terminal would send them.
* The driver answers the capability queries a real xterm answers — DA1 (`\x1b[c`), DA2
  (`\x1b[>c`), XTVERSION (`\x1b[>q`), OSC 10/11, and `\x1b[?996n`. **This mattered.** Before
  it answered them, tmux never sent `\x1b[?1004h` to the client even with `focus-events on`;
  after, it did. A mute terminal makes tmux withhold features, and a measurement against a
  mute terminal would have produced a false negative on focus. Stated because it is the one
  place this harness could have lied.

### Honest limits of the harness

1. Mouse reports were **injected**, not produced by a physical mouse. That deliberately
   separates two questions which are answered separately below: *(a)* does tmux ask the
   terminal to report at all, and *(b)* if the bytes arrive anyway, where does tmux route
   them. Conflating them is how one would conclude the §4c hypothesis works in full.
2. Three tmux versions were built and run: **3.1c, 3.2, 3.7c**. 3.3 through 3.6 were not
   built; statements about them come from tmux's own shipped `CHANGES` and are labelled as
   such.
3. The operator's machine has **no** `~/.tmux.conf` and **no** `~/.config/tmux/tmux.conf`
   (checked; both absent), and their default socket had no server running. The "operator with
   `mouse on`" case is therefore reproduced with a synthetic config, and both the real and
   synthetic cases are reported separately in §7.

### Versions

```
$ /opt/homebrew/bin/tmux -V
tmux 3.7c
$ ./build/tmux-3.2/tmux -V
tmux 3.2
$ ./build/tmux-3.1c/tmux -V
tmux 3.1c
```

3.2 and 3.1c were built from the release tarballs against homebrew's libevent 2.1.13 and
ncurses 6.6 (`./configure && make`, both rc 0).

---

## 2. Step 1 — mouse routing, the four combinations

One pane filling a 120x30 window, running the probe. `-f /dev/null`, so nothing but the
option under test differs.

```
tmux -L <sock> -f /dev/null new-session -d -s s -x 120 -y 30 'python3 probe.py <log> 12 <modes>'
tmux -L <sock> attach -t s          # on the pty
```

Injected into the pty master, in order:

```
\x1b[<0;10;5M   press button 1 at col 10 row 5
\x1b[<0;10;5m   release button 1
\x1b[<64;10;5M  wheel up
\x1b[<65;10;5M  wheel down
```

| | tmux `mouse` | pane requested | probe received | `pane_in_mode` after wheel |
|---|---|---|---|---|
| **A** | off | `\x1b[?1006h\x1b[?1000h` | all four, verbatim | `0` |
| **B** | on | `\x1b[?1006h\x1b[?1000h` | all four, verbatim | `0` |
| **C** | off | nothing | nothing | `0` |
| **D** | on | nothing | nothing | `1`, `copy-mode` |

Raw probe log, case **A** (tmux `mouse off`):

```
TTY_STDIN=/dev/ttys038
TTY_STDOUT=/dev/ttys038
ISATTY=(True, True)
TMUX=/private/tmp/tmux-502/wf1a,92956,0 TMUX_PANE=%0
TERM=tmux-256color
ENABLE_WRITTEN='\x1b[?1006h\x1b[?1000h\x1b[?1004h'
IN b'\x1b[<0;10;5M'
IN b'\x1b[<0;10;5m'
IN b'\x1b[<64;10;5M'
IN b'\x1b[<65;10;5M'
IN b'q'
DONE
```

Case **C** — tmux `mouse off`, pane asked for nothing — the same four injections produce:

```
ENABLE_WRITTEN=''
IN b'q'
DONE
```

i.e. tmux **parses and discards** them. There is no pass-through of unrequested mouse bytes.

### What tmux sends to the outer terminal

Captured from the pty master during attach, filtered to mouse modes:

| case | last mode changes tmux wrote to the client |
|---|---|
| **A** mouse off, pane wants mouse | `…1006l 1000l 1002l 1003l` then **`1006h 1000h`** |
| **B** mouse on, pane wants mouse | `…1000l 1002l 1003l` then **`1006h 1000h 1002h`** |
| **C** mouse off, pane wants nothing | `…1006l 1000l 1002l 1003l` — **never enabled** |
| **D** mouse on, pane wants nothing | **`1006h 1000h 1002h`** |

Two things follow. tmux propagates the pane's *exact* request when `mouse` is off (1000+1006,
no 1002). With `mouse` on, tmux adds `1002h` — button-event tracking, which it needs for its
own border drags.

---

## 3. Step 2 — what scroll does, in a pane with scrollback

The probe writes 200 lines before it starts reading, so there is a real scrollback
(`history_size=173` in every reading below).

* **mouse off, pane requested mouse** — wheel reaches the program (`b'\x1b[<64;10;5M'`),
  `pane_in_mode=0`. Copy-mode is never entered.
* **mouse off, pane requested nothing** — the wheel reaches nobody, `pane_in_mode=0`. tmux
  drops it.
* **mouse on, pane requested mouse** — wheel reaches the program, `pane_in_mode=0`. tmux's
  default `WheelUpPane` binding tests `#{mouse_any_flag}` and forwards with `send -M`;
  `mouse_any_flag` was `1` throughout.
* **mouse on, pane requested nothing** — wheel-up enters copy-mode. Raw:

```
  after b'\x1b[<0;10;5M'  in_mode=0 mode=          any=0 sb=173
  after b'\x1b[<0;10;5m'  in_mode=0 mode=          any=0 sb=173
  after b'\x1b[<64;10;5M' in_mode=1 mode=copy-mode any=0 sb=173
  after b'\x1b[<65;10;5M' in_mode=0 mode=          any=0 sb=173
```

**A component that declares `scroll` therefore owns the wheel over its own pane
unconditionally** — the "different operations that look identical to the wheel" problem §4c
names is decided by *which pane the pointer is over*, and tmux does that part correctly. The
harness pane keeps its own scrollback semantics because it is a different pane.

---

## 4. Coordinates, borders, and one event shape the contract must tolerate

Two panes, `split-window -h -l 40`: pane `%0` at `left=0 w=79`, pane `%1` at `left=80 w=40`.

**Coordinates are pane-relative and 1-based.** Injecting a click at window column 100 made the
panel's probe read `b'\x1b[<0;20;5M'` — 100 − 80 = 20. Injecting window column 81 produced
`b'\x1b[<0;1;5M'`. Charter never has to subtract `pane_left` itself, and must not.

With the operator's `pane-border-status top` set, the same injection at **row 5** arrived as
**row 4** — tmux subtracts the border row too. A component must not assume its own origin
from anything but what tmux hands it.

**Borders, tmux `mouse` off** (harness pane active throughout):

```
  click border col 80    active=0  probe=(nothing)
  click panel  col 81    active=0  probe=["b'\x1b[<0;1;5M'", "b'\x1b[<0;1;5m'"]
  click harness col 40   active=0  probe=(nothing)
  border DRAG 80 -> 70:  panes 0:79 1:40   (unchanged)
```

**Borders, tmux `mouse` on:**

```
  click border col 80    active=0  probe=(nothing)
  click panel  col 81    active=1  probe=["b'\x1b[<0;1;5M'", "b'\x1b[<0;1;5m'"]
  border DRAG 80 -> 70:  panes 0:69 1:50   (resized)
```

So with `mouse` on, a click on a charter panel **steals keyboard focus from the harness**;
with `mouse` off it does not. That is the deciding fact for the escape-hatch design in §4c —
with `mouse` off, focus never moves by accident, so the escape hatch is only ever needed for
charter's own intra-pane focus.

**The event shape to tolerate.** With `mouse` off, a drag that begins on a border and ends
inside a pane delivers a **lone release with no matching press**:

```
  border drag (session mouse off)  active=0  harness=["b'\x1b[<0;70;4m'"]  panel=-
```

The press at the border was dropped (borders are not a pane), the motion was dropped (the
pane asked for 1000, not 1002/1003), and only the release was routed. §4f's five event kinds
must therefore say that a `click` release may arrive with no preceding press, or the first
third-party component that keeps press state will wedge.

---

## 5. Where §4c is wrong: the outer terminal's mouse mode follows the ACTIVE pane

This is the measurement that changes the design.

### 5.1 Non-active pane requests mouse — terminal is NOT asked to report

Pane `%0` = `cat` (asks for nothing) and **is active**; pane `%1` = probe (asks for
1006+1000). tmux `mouse off`. What tmux wrote to the client during attach:

```
OUTER modes while active=harness: 1006l 1000l 1002l 1003l  1006l 1000l 1002l 1003l
```

Mouse reporting **off**. Then `select-pane -t s.1`:

```
OUTER modes after select-pane panel: 1006l 1000l 1002l 1003l  1006h 1000h
```

Mouse reporting **on**. The outer terminal's mouse state is a single, window-wide value
derived from the active pane's screen mode.

### 5.2 Both panes request mouse — routing works exactly as §4c hoped

Pane `%0` = probe (harness stand-in, asks for mouse) **and active**; pane `%1` = probe
(charter panel). tmux `mouse off`:

```
OUTER modes: 1006l 1000l 1002l 1003l  1006h 1000h
  attached                     active=0 harness=-  panel=-
  click harness area col 40    active=0 harness=[b'\x1b[<0;40;5M', b'\x1b[<0;40;5m'] panel=-
  click panel  area col 100    active=0 harness=-  panel=[b'\x1b[<0;20;5M', b'\x1b[<0;20;5m']
  click border col 80          active=0 harness=-  panel=-
  wheel up over panel          active=0 harness=-  panel=[b'\x1b[<64;20;5M']
  wheel up over harness        active=0 harness=[b'\x1b[<64;40;5M'] panel=-
```

Each pane gets its own rectangle's events, pane-relative, and the active pane never changes.
Identical on tmux 3.1c and 3.2. **This is the configuration in which §4c's mechanism actually
delivers**, and it requires the *harness* to be requesting mouse reporting.

### 5.3 What that means

* **Drag-select is not preserved.** In §5.2 the outer terminal has `1006h 1000h` set the whole
  time. A real terminal in that state does not do native text selection (modifier-held
  selection aside, which is the terminal emulator's own affordance and not tmux's to give).
  §4c's parenthetical is false, and the trade `instance.FRAME_FIELDS` describes is real
  whichever way `mouse` is set.
* **Charter cannot guarantee its panels are clickable.** In §5.1 — harness pane active, harness
  not asking for mouse — the terminal is never asked to report, so nothing arrives for tmux to
  route. Whether charter's panels receive clicks is decided by a program charter does not own.
* **`mouse on` makes it unconditional, at a price.** With `mouse on`, tmux enables reporting
  from attach regardless of the active pane, routes each event to the pane under the pointer
  *and* makes that pane active, and gives back border drag-resize. §7 shows charter's existing
  session-scoped `set -t <session> mouse off` cleanly overrides an operator's global
  `mouse on`, so both settings remain genuinely available per frame.

**Recommendation for Phase 2, stated as a recommendation and not a decision:** keep
`[frame] mouse` as the switch it already is, and make a component's `click`/`scroll`
declaration honest about the condition — pointer events are *available when the frame's mouse
mode is on, or when the active pane happens to request reporting*. A palette drawn in a
`display-popup` sidesteps the whole question, because the popup is the active surface and its
own request is what reaches the terminal (§8).

---

## 6. Focus events: gated by an option that is off by default

Two panes as above; the probe additionally writes `\x1b[?1004h`.

| `focus-events` | client reports its own focus? | probe receives on `select-pane` |
|---|---|---|
| `off` (tmux default) | no | **nothing** |
| `off` | yes (`\x1b[I` injected) | **nothing** |
| `on` | no | **nothing** |
| `on` | yes | `\x1b[I` / `\x1b[O`, correctly |

The full run with `focus-events on` and the client reporting focus:

```
OUTER 1004: ['\x1b[?1004h', '\x1b[?1004h', '\x1b[?1004h']
  attach (pane 0 active)        -> (nothing)
  client CSI I (focus in)       -> (nothing)
  select-pane panel             -> ["IN b'\x1b[I'"]
  select-pane harness           -> ["IN b'\x1b[O'"]
  select-pane panel             -> ["IN b'\x1b[I'"]
  client CSI O (focus out)      -> ["IN b'\x1b[O'"]
  client CSI I (focus in)       -> ["IN b'\x1b[I'"]
```

With `focus-events off`, tmux writes `\x1b[?1004l` to the client and never delivers pane focus
transitions — **even though `list-clients -F '#{client_flags}'` reports `attached,focused`
the whole time.** That flag is not evidence that focus is being delivered; a guard written
against it would pass while the feature was dead.

**Consequence for §4f's `focus`/`blur` event kind:** it does not exist unless charter sets
`set -t <session> focus-events on` in `conf_text`, and it depends on the operator's terminal
emulator reporting focus at all. A component's `focus` declaration must degrade to "never
fires" rather than to "fires wrongly".

One unexplained observation, recorded rather than smoothed over: with `focus-events off`, the
final injected `\x1b[I` appeared in the pane's log once, while the injected `\x1b[O` did not.
It reproduced across runs. It does not affect any conclusion here, but a `focus` implementation
should not assume a clean in/out pairing.

---

## 7. Both servers

### 7.1 The operator's own server, their own config

Run against the **operator's actual default socket** (`/private/tmp/tmux-502/default`), with a
guard that refuses if a server is already running there, and with no `kill-server` at any
point. Their startup files were checked at run time:

```
default socket before: 1 '' 'error connecting to /private/tmp/tmux-502/default (No such file or directory)'
socket path: /private/tmp/tmux-502/default
startup files tmux would read: ~/.tmux.conf exists = False | ~/.config/tmux/tmux.conf exists = False
GLOBAL mouse: mouse off | GLOBAL focus-events: focus-events off
  click harness col 40   active=0 harness=[b'\x1b[<0;40;5M', b'\x1b[<0;40;5m'] panel=-
  click panel   col 100  active=0 harness=-  panel=[b'\x1b[<0;20;5M', b'\x1b[<0;20;5m']
  click border  col 80   active=0 harness=-  panel=-
  wheel up over panel    active=0 harness=-  panel=[b'\x1b[<64;20;5M']
default socket after: 1 '' 'no server running on /private/tmp/tmux-502/default'
```

Byte-identical to charter's private socket. **On this machine the operator's config sets
nothing relevant** — the `mouse on` the coordinator's note anticipated is not present here.
The socket was left with no server, exactly as found.

### 7.2 An operator whose config does set `mouse on`

Same socket, started with a synthetic config:

```
set -g mouse on
set -g focus-events on
set -g pane-border-format " #{host} #{pane_index} "
set -g pane-border-status top
```

```
GLOBAL mouse: mouse on | GLOBAL focus-events: focus-events on
OUTER modes: 1000l 1002l 1003l  1006h 1000h 1002h
  click harness col 40   active=0 harness=[b'\x1b[<0;40;4M', b'\x1b[<0;40;4m'] panel=-
  click panel   col 100  active=1 harness=-  panel=[b'\x1b[<0;20;4M', b'\x1b[<0;20;4m']
```

Note `row 4`, not 5: `pane-border-status top` shifted every pane down one row and tmux
adjusted the translation. This is the geometry half of the #514 shape — an operator's border
settings change charter's usable rectangle, and charter must read geometry from tmux rather
than compute it.

### 7.3 charter's session-scoped override works

`conf_text` writes `set -t <session> mouse …`, session-scoped and never `-g`. Applied on top
of the operator's global `mouse on`:

```
  after charter's `set -t <session> mouse off`: session mouse = mouse off | global mouse = mouse on
OUTER modes after session mouse off: 1006l 1000l 1002l 1003l  1006h 1000h
  click panel (session mouse off)      active=0  panel=[b'\x1b[<0;20;6M', b'\x1b[<0;20;6m']
  border drag with session mouse off:  widths ['79','40'] -> ['79','40']
```

`1002h` is withdrawn, the panel click no longer steals focus, and the border stops dragging.
**The operator's global `mouse on` does not leak into charter's frame.** Nothing needs
changing here.

---

## 8. Step 3 — `display-popup`

### 8.1 The version it appeared in, confirmed against the shipped CHANGES

`/opt/homebrew/Cellar/tmux/3.7c/CHANGES`, in the section headed `CHANGES FROM 3.1c TO 3.2`:

> Add support for per-client transient popups, similar to menus but which are connected to an
> external command (like a pane). These are created with new command display-popup.

Confirmed by running the binaries rather than trusting the reading:

```
$ ./build/tmux-3.1c/tmux -L v31 display-popup -E -w 40 -h 10 -t v 'true'
unknown command: display-popup
   rc=1
$ ./build/tmux-3.1c/tmux -L v31 list-commands | grep -E 'popup|menu'
display-menu (menu) [-c target-client] [-t target-pane] [-T title] [-x position] [-y position] name key command ...

$ ./build/tmux-3.2/tmux -L v32 list-commands | grep -E 'popup|menu'
display-menu (menu) [-O] [-c target-client] ...
display-popup (popup) [-CE] [-c target-client] [-d start-directory] [-h height] [-t target-pane] [-w width] [-x position] [-y position] [command]
```

**`display-popup` first exists in 3.2.** Exactly `tmuxctl.FLOOR`.

### 8.2 tmux 3.7c — own tty, own mouse, own SIGWINCH

```
tmux -L <sock> display-popup -E -w 60 -h 12 -t s 'python3 probe.py <log> 25 sgr,mouse,focus'
```

Popup log, verbatim:

```
TTY_STDIN=/dev/ttys049
TTY_STDOUT=/dev/ttys049
ISATTY=(True, True)
TMUX=/private/tmp/tmux-502/wf1r,19759,0 TMUX_PANE=%118
TERM=tmux-256color
ENABLE_WRITTEN='\x1b[?1006h\x1b[?1000h\x1b[?1004h'
WINSZ_AT_START=(58, 10)
IN b'\x1b[<0;10;3M'
IN b'\x1b[<0;10;3m'
SIGWINCH winsz=(48, 6)
SIGWINCH winsz=(58, 10)
IN b'q'
DONE
```

while the pane underneath was on `/dev/ttys038` — a genuinely separate tty.

* **Mouse.** The popup's own `\x1b[?1006h\x1b[?1000h` propagated to the outer terminal
  (`1006h 1000h` appeared on the master right after the popup opened), and injected reports
  arrived popup-relative: window `(col 40, row 12)` became popup `(col 10, row 3)`, a constant
  offset of `(30, 9)` for a 60x12 popup centred in a 120x30 window. Scroll too:
  `b'\x1b[<64;10;3M'` and `b'\x1b[<65;10;3M'`.
* **Modality.** A click *outside* the popup reached nobody — not the popup, not the pane
  underneath — and a wheel outside the popup did **not** put the pane into copy-mode
  (`pane_in_mode: 0/`). A "click outside to dismiss" affordance cannot be built from the pane
  side; it needs `display-popup -B`/`-C` or tmux's own button-three popup menu.
* **Keys.** `q` reached the popup, not the pane. The popup owns the keyboard while open.
* **Resize.** Shrinking the client from 120x30 to 50x7 gave the popup `SIGWINCH` with
  `(48, 6)`; growing to 100x23 gave `(58, 10)` back. It survived both. The pane underneath got
  its own `SIGWINCH (50, 7)` and `(100, 23)`. A `window-resized` hook fired once. This is
  tmux ≥ 3.3 behaviour: `CHANGES FROM 3.2a TO 3.3` — *"Do not close popups on resize, instead
  adjust them to fit."*

### 8.3 tmux 3.2 — the popup DIES on resize

Same driver, `TMUXBIN=./build/tmux-3.2/tmux`:

```
popup process alive: True
popup log head:
    TTY_STDIN=/dev/ttys049
    TMUX=... TMUX_PANE=%118
    TERM=screen
    WINSZ_AT_START=(58, 10)
  click inside popup      popup=[b'\x1b[<0;10;3M', b'\x1b[<0;10;3m']
  wheel up INSIDE popup   popup=[b'\x1b[<64;10;3M']
  wheel down INSIDE popup popup=[b'\x1b[<65;10;3M']
after client resize -> 50x7: popup alive: False
window-resized hook fired: 0
popup rc: 129
```

`rc 129` is 128 + SIGHUP. The popup log ends mid-stream with no `DONE`. Its tty, mouse and
scroll all work on 3.2 — but **any client resize destroys it**, silently, with whatever the
operator was typing.

Two more 3.2-band differences visible in that head: `TERM=screen` (3.2's `default-terminal`
default, so fewer colours than the `tmux-256color` a 3.7c popup gets), and the pane underneath
received **no focus events at all** when the popup opened — that behaviour arrives in 3.6
(`CHANGES FROM 3.5a TO 3.6`: *"Send focus events to pane when entering or leaving popup (issue
3991)."*).

### 8.4 Focus is wrong in both directions, and one silent-zero to watch

**The popup itself never receives focus events.** With `focus-events on` and the client's
focus genuinely toggling in and out while the popup was open:

```
  client CSI I while popup open   pane=-  popup=-
  client CSI O while popup open   pane=-  popup=-
  client CSI I while popup open   pane=-  popup=-
```

The popup had `\x1b[?1004h` set. It received clicks and keys in the same run. It received no
focus event, ever, on 3.7c or 3.2.

**The pane underneath does**, on 3.7c, and — measured — **even with `focus-events off`**:

```
$ cat out/N_popup_mouseoff.pane
...
ENABLE_WRITTEN='\x1b[?1006h\x1b[?1000h\x1b[?1004h'
IN b'\x1b[O'     <- popup opened
IN b'\x1b[I'     <- popup closed
IN b'\x1b[O'     <- teardown
```

So popup enter/leave focus and client focus travel different paths inside tmux, and only the
latter is gated on `focus-events`.

**And the silent zero.** A program inside a popup that asks tmux about itself gets an empty
answer with a success exit code:

```
TMUX_PANE=%118
TTY=/dev/ttys049
list-panes sees: %0                       <- the popup is not in list-panes -a
resolve:  x                               <- display-message -p -t "$TMUX_PANE" "#{pane_id} #{pane_width}x#{pane_height}"
resolve rc=0                              <- rc 0
no -t resolves to: %0                     <- omitting -t silently targets the pane UNDERNEATH
```

This is the exact failure shape this repo keeps paying for: a query that answers wrongly
instead of failing. Any charter code that runs inside a popup and resolves geometry or
identity from `$TMUX_PANE` will get `""` and `rc 0`, and any tmux command it issues without
`-t` will act on the harness's pane. Phase 2 must treat "am I in a popup" as a thing charter
tells the process explicitly (`display-popup -e`, 3.3+) rather than something the process can
ask tmux.

---

## 9. Step 4 — the sub-3.2 band

`tmuxctl.below_floor_message` promises charter still launches below 3.2. Measured on a real
3.1c binary, alongside 3.2 for the boundary:

| probe | 3.1c | 3.2 | 3.7c |
|---|---|---|---|
| `display-popup` | `unknown command: display-popup`, rc 1 | present | present |
| `display-menu` | present | present | present |
| `new-session -e FOO=bar` | `tmux: unknown option -- e`, rc 1 | rc 0, `show-environment` → `FOO=bar` | rc 0 |
| `split-window -e BAZ=qux` | rc 0 | rc 0 | rc 0 |
| `set-hook -p -t v.0 pane-died` | `tmux: unknown option -- p`, rc 1 | rc 0 | rc 0 |
| `set-hook -g window-resized` | `invalid option: window-resized`, rc 1 | `invalid option: window-resized`, rc 1 | rc 0 |
| `set-hook -g client-resized` | rc 0 | rc 0 | rc 0 |
| `show -g popup-border-lines` | `invalid option`, rc 1 | `invalid option`, rc 1 | rc 0 |
| mouse routing (§5.2) | identical to 3.7c | identical to 3.7c | — |

### Four constants in `charter/frame/tmuxctl.py` are now measured rather than read

Each of these was justified in the source by tmux's published `CHANGES` with an explicit note
that no old binary existed to check it against. There is one now, and all four survive:

* **`FLOOR = (3, 2)`** — its stated reason is `set-hook -p`, "where exactly `set-hook -p`
  first appeared could not be confirmed by running it". Confirmed: **`-p` is rejected by 3.1c
  and accepted by 3.2**, exactly at the floor. The conservative choice was the correct one for
  the right reason.
* **`SESSION_ENV_FLOOR = (3, 2)`** — confirmed. 3.1c rejects `new-session -e` with
  `tmux: unknown option -- e` and a usage line, rc 1, which would indeed take the whole launch
  down; 3.2 accepts it and `show-environment -t` reads the value back.
* **`PANE_ENV_FLOOR = (3, 0)`** — `split-window -e` is accepted by 3.1c. Consistent.
* **`RESIZE_HOOK_FLOOR = (3, 3)`** — confirmed as *above* 3.2: both 3.1c and 3.2 answer
  `invalid option: window-resized`, rc 1, which is the exact error text the docstring predicted
  from a fabricated hook name on 3.7c.

### What is available instead of `display-popup` below 3.2

`display-menu`, which arrived in 3.0. Measured on 3.1c with a real attached client and twenty
items:

```
client: /dev/ttys048
display-menu still blocking: True
menu drew 1167 bytes; contains 'item 01': True; 'Palette': True
all 20 items drawn
display-menu exited after ESC: 0
```

* It draws **all twenty rows** in a 30-row client. **The nine-row cap §2 of the spec
  complains about is charter's, not tmux's** — `frame/menu.py:434` is
  `return str(i + 1) if i < 9 else ""`, and that function's own docstring records that such
  rows are still drawn and still arrow-selectable; what they lose is a digit shortcut,
  because the digits run out. tmux is not the constraint there.
* It **blocks the client that issued it** until dismissed (the first attempt at this
  measurement hit a 20-second `subprocess` timeout because of it) — which is why charter fires
  it from a `run-shell` bind rather than inline.
* ESC dismisses it, rc 0.

Beyond `display-menu`, the sub-3.2 fallbacks are the ordinary ones: a real pane
(`split-window` / `new-window`, both with `-e` since 3.0) and `command-prompt`. Nothing else
floating exists.

**The floor for a popup-based palette is therefore 3.3, not 3.2** — because on 3.2 the popup
does not survive a resize. Charter's supported band splits three ways:

| band | palette surface |
|---|---|
| < 3.2 | no popup at all — `display-menu`, or a real pane |
| 3.2 | popup exists but dies on any client resize; `TERM=screen`; no pane focus events |
| 3.3 – 3.5a | popup survives and adjusts to fit; still no pane focus events |
| ≥ 3.6 | popup survives; pane underneath gets focus out/in |

If Phase 2 wants one code path, the honest choice is a full-pane palette everywhere with the
popup as an *enhancement* gated at 3.3 — not a popup with a fallback, which would be two
surfaces to keep in step and a resize-shaped bug that only appears on one version.

---

## 10. Summary of what the spec should be corrected on

1. **§4c** — delete "which also preserves the drag-select the comment worried about losing".
   It is false. Whenever any mouse-requesting pane is active, the outer terminal is reporting
   and native selection is gone.
2. **§4c** — add the condition: charter panels receive pointer events only while the active
   pane requests mouse reporting, which charter does not control unless `[frame] mouse` is on.
3. **§4f** — the `click` event kind must tolerate a release with no matching press (§4).
4. **§4f** — the `focus`/`blur` event kind requires `focus-events on`, which is off by default
   in tmux and absent from `conf_text`. Without it the events never fire.
5. **§4g** — "confirm what `display-popup` does with mouse reporting **and focus**": mouse yes,
   focus no. A popup's own program never receives focus events. Only the pane underneath does,
   and only from 3.6.
6. **§4g** — "`SESSION_ENV_FLOOR` is `(3,2)`, and `display-popup` landed in the same release"
   is correct, but the popup is not *usable* until 3.3. State 3.3 as the popup floor.
7. **§2** — "a nine-row cap with no way to page" is charter's own digit-key limit
   (`frame/menu.py:434`), not a tmux limit. tmux drew twenty rows fine on 3.1c. The rest of
   that sentence — no filtering, no live state, no way to say why an action is unavailable —
   stands.

---

## 11. Reproducing this

No production code changed and nothing here is under test. The harness that produced every
byte quoted above lives outside the repo, in this task's scratch directory, as five small
stdlib-only scripts: a `probe.py` run as a pane's or popup's command, and drivers that create
a pty, attach a real tmux client to it, inject SGR reports, and diff each probe's log after
each step. The tmux invocations are quoted inline in each section, and the three binaries were
`/opt/homebrew/bin/tmux` (3.7c) plus 3.2 and 3.1c built from their release tarballs.

The suite was run before and after, unchanged: `Ran 5712 tests … OK` with the ambient
variables cleared, and again with `CHARTER_WORKSPACE` set.

Everything else in §4c and §4g survived the measurement unchanged.
