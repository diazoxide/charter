# The frame's visual design — surfaces, not a colour scheme

**Date:** 2026-08-28 · **Status:** decided, unimplemented
**Spec:** extends `docs/superpowers/specs/2026-08-25-agentic-ide-foundation.md` §4b, §4c, §4k
**Measurements:** every terminal claim below was run on this machine; the command is quoted
with its output. tmux 3.7c, Darwin 25.2.0, Python 3.14.4.

The ask, in the operator's words:

> lets add some background to our top/bottom/side bars. making them like a UI APP

---

## 1. The finding that decides the shape of this

**charter does not have to paint a background. tmux paints it, per pane, for free.**

`window-style` and `window-active-style` are settable **pane-scoped** on tmux 3.7c, and tmux
fills the pane's whole rectangle from them — including the cells no renderer wrote, on
resize, on reattach, at zero cost on the repaint path. Measured, three panes, styles set on
the two panels and not on the harness:

```
$ tmux set -p -t %1 window-style bg=colour236
$ tmux set -p -t %1 window-active-style bg=colour24
$ tmux show -p -t %0 -v window-style      ->  ''            (harness: nothing set)
$ tmux show -p -t %1 -v window-style      ->  'bg=colour236'
```

and what tmux then sent to an attached `xterm-256color` client, with `%1` active:

```
HARNESS : b'...\x1b(B\x1b[m\x1b[1;1HHARNESS'        <- no colour at all
PANELA  : b'...\x1b[K\x1b[48;5;24m\x1b[2BPANELA'    <- the ACTIVE style
PANELB  : b'...\x1b[K\x1b[48;5;236m\x1b[2BPANELB'   <- the INACTIVE style
```

Four consequences, and they are why this spec is short where it could have been long:

1. **The repaint cost of a background is zero.** It is not on the repaint path. Constraint 3
   (repaint on version bump or resize, never a loop) is untouched because nothing new
   repaints.
2. **A background cannot wrap a pane.** Constraint 5 is untouched because the fill never
   passes through a renderer and therefore has no width to get wrong.
3. **The focused/unfocused state is already correct**, drawn by tmux from its own pane
   focus, which §4e already says tmux owns. It needs no `focus-events`, so it works inside
   an operator's own tmux where charter writes no config at all.
4. **The harness pane is provably untouched** — `show -p` reads back `''`, and the wire
   carries no colour before its content. ADR 0018's line ("never decides what a cursor or a
   colour means inside it") holds by construction rather than by care.

It also lands in a seam that already exists. `commands_frame._CHROME` and `_chrome_argvs`
already pin every window option tmux draws a border from, window-scoped, targeting the
harness pane, on both servers, through the one funnel every panel pane comes out of. This is
two more options in the same place, pane-scoped instead of window-scoped.

**So the design question is not "how does charter paint a background".** It is: *which
colour*, *whose theme*, and *what else does an app look like*.

---

## 2. What "a UI app" is, concretely — and why the background is the smallest part

The frame today is four regions of the same terminal, told apart by a thin dim rule. It is
plain text with some colour in it. Rendered now, at 100 columns:

```
top    ' ⬢ default  \x1b[35m◆\x1b[0m \x1b[1msteward\x1b[0m\x1b[2m · ◇ personas …'
bottom '5 todos · \x1b[33m⚠\x1b[0m \x1b[2mplane root\x1b[0m charter\x1b[2m · \x1b[0m…'
right  '\x1b[2m▪ personas\x1b[0m 5'
       '\x1b[35m▸ \x1b[1msteward\x1b[0m                              \x1b[32m✎47\x1b[0m'
```

Six elements make a terminal frame read as an application rather than as output. Ranked by
how much each contributes, which is not the order anyone reaches for them in:

| # | element | what it says | where it comes from |
|---|---|---|---|
| 1 | **heading** | this region has a name and a subject | the renderer, weight only |
| 2 | **inset** | content is inside something | the renderer, one constant |
| 3 | **active-item highlight** | this row is the one you are on | the renderer, full-width reverse |
| 4 | **focus** | this region is the live one | tmux, `window-active-style` |
| 5 | **status** | this is fine / this needs you | the renderer, colour **and** glyph |
| 6 | **surface** | this is chrome, not content | tmux, `window-style` |

The strongest counter-argument in the brief is the right one:

> *A background fill on a 1-row bar is a colour, not a UI.*

Correct, and it is answered by the table rather than argued away. A fill on its own is a
coloured status line. What makes the frame an application is 1–3 and 5 — a named region, a
consistent inset, a row you can see you are on, and a status you can read without colour
vision. The surface is what makes those legible *as a region*, and on a one-row bar the
surface **is** the region — which is exactly why it is worth having and exactly why it is
last.

That ranking is also the phasing. Elements 1, 2, 3 and 5 are theme-safe and ship on by
default. Elements 4 and 6 need a colour, colour cannot be made theme-safe, and so they are
the opt-in.

> **Counter-argument, answered: *why paint at all — why not let the terminal theme show
> through?*** For five of the six elements charter already does, and keeps doing: weight,
> inset, a glyph, an inverted row and a hue name are all statements *relative* to the
> operator's own palette, and none of them names a colour charter chose. That is
> `_CHROME_STYLE`'s rule and this spec does not weaken it. The surface is the one element
> that cannot be said relatively — tmux ignores every attribute in `window-style` and honours
> only a colour (§4) — and so it is the one element that is off by default. **The answer to
> "why paint at all" is that by default charter does not**, and the operator who wants it
> says one word. What the shipped default buys is the other five, which are the ones that
> make it an application rather than a coloured line.

---

## 3. Colour: how much, and from where

### 3.1 What was measured

**`tput colors` and terminfo answer a *database*, not the terminal.** On this machine, inside
tmux:

```
$ echo $TERM $COLORTERM ; tput colors
tmux-256color truecolor
256
```

`tput colors` says 256 while `$COLORTERM` says 16 million, and the two disagree because
tmux's own terminfo entry has no way to say more than 256. Per-`TERM`, fresh process each
time:

```
$ python3 -c "import curses,sys; curses.setupterm(sys.argv[1]); print(curses.tigetnum('colors'))" <TERM>
xterm            8      vt100          -1
xterm-256color 256      dumb           -1
tmux-256color  256      xterm-direct   curses.error: could not find terminal
```

**`curses.setupterm` is one-shot per process and lies quietly afterwards.** Called three
times in one process, the second and third do nothing and `tigetnum` keeps answering the
first terminal's number:

```
$ python3 -c "
import curses
curses.setupterm('xterm');          print(curses.tigetnum('colors'))   # 8
curses.setupterm('xterm-256color'); print(curses.tigetnum('colors'))   # 8   <- wrong
curses.setupterm('dumb');           print(curses.tigetnum('colors'))"  # 8   <- wrong
```

That is a test-shaped trap of exactly the class this project has hit five times: a suite that
compares two terminals in one process passes with the feature dead. Recorded here so nobody
writes it.

**`$COLORTERM` inside a tmux pane describes the terminal that started the *server*, not the
terminal currently looking at the pane.** `COLORTERM` is not in tmux's `update-environment`
(measured: the default list is thirteen names, all X11/SSH), so a pane inherits the server's
frozen copy. Measured — server started from a truecolor shell, then a client attached with
`COLORTERM` unset, then a new pane opened:

```
pane COLORTERM after a no-COLORTERM attach = truecolor
client termfeatures                        = bpaste,ccolour,clipboard,cstyle,focus,title
                                             (no RGB — tmux knows, the pane does not)
```

So detach from a truecolor terminal, reattach over ssh from a 16-colour one, and every panel
still reads `COLORTERM=truecolor`. **`$COLORTERM` is unusable inside a frame.** It is the
spelling; the property is what the attached client can render, and only tmux knows it.

**That is also the whole of the ssh answer**, and it is the case charter would have got
wrong. A frame is long-lived and its panels outlive any one client: the operator starts it at
their desk, detaches, reattaches from a phone or a jump host, and reattaches again the next
morning. Anything charter decided *once* — at panel start, from the environment — is wrong
for every attachment after the first. Anything tmux decides is right for the client that is
actually looking, every time, because tmux recomputes it per client per attach.

**tmux already is the colour ladder, live, per client.** The same pane painting
`\x1b[48;5;236m` and `\x1b[48;2;30;60;90m`, with four different clients attached in turn —
this is what tmux put on each client's wire:

| client | features | 256-index bg | 24-bit bg |
|---|---|---|---|
| `xterm-256color` + `COLORTERM=truecolor` | …,`RGB`,… | `ESC[48;5;236m` | `ESC[48;2;30;60;90m` |
| `xterm-256color`, no `COLORTERM` | no `RGB` | `ESC[48;5;236m` | **`ESC[48;5;237m`** |
| `xterm` (8 colours) | no `RGB` | **`ESC[40m`** | **`ESC[40m`** |
| `vt100` | `bpaste,focus,title` | **`ESC[7m`** | **`ESC[7m`** |

tmux downsamples 24-bit → 256 → 16, and on a terminal with no colour at all it converts
colour to **reverse video** by itself. Attributes go the same way — bold, dim, reverse and
underline all reach `xterm`; on `vt100` bold, underline and reverse survive and **dim is
dropped**.

And `TERM=dumb` is not a tier at all:

```
$ TERM=dumb tmux -L probe attach
open terminal failed: terminal does not support clear
```

tmux refuses to run there, so the frame cannot exist there, and charter has nothing to
degrade.

### 3.2 The decisions

**Charter does not implement a colour-capability ladder.** tmux is one, it is per-client, it
is live across reattach, and it degrades all the way to reverse video. A second ladder inside
charter would be a second answer to the same question, computed from `$COLORTERM`, which is
the one input measured to be stale. The property is *what the attached client renders*;
`$COLORTERM` and `tput colors` are spellings of it, and both were measured wrong here.

**Charter never emits an absolute colour in its own chrome.** No 256-cube index, no 24-bit
triple, ever. Only three things are allowed: `default`, the sixteen ANSI names, and the SGR
attributes. This is `commands_frame._CHROME_STYLE`'s rule (`fg=default,dim` — *"never a
colour charter picked out of the 256 and imposed on a theme it cannot see"*) extended one
step: a name like `brightblack` is a slot in the operator's own palette, and an index like
`colour236` is a fixed point in the xterm cube that no theme moves.

The sharpest form of that argument is the inverse of the obvious one: **an absolute colour is
unsafe precisely on the terminals that render it faithfully.** A 16-colour client gets
charter's `colour236` downsampled to the operator's own black and looks fine; a truecolor
client with a light theme gets the dark grey verbatim and looks broken.

**`NO_COLOR` is honoured, and today nothing in charter does.** `NO_COLOR` appears nowhere in
`charter/`, `tests/` or `docs/`. `util._USE_COLOR = sys.stderr.isatty()` gates
`util.info/ok/warn/err` and nothing else; the frame renderers colour unconditionally.
Measured — `panel.run` with stdout a `StringIO`, which is what
`charter panel top --session x > /tmp/log` (a case `panel.py`'s own docstring documents) does:

```
'\x1b[H\x1b[2J ⬢ default  \x1b[35m◆\x1b[0m \x1b[1msteward\x1b[0m…'
```

A clear-screen and full SGR, into a file. The rule this spec adds:
**`NO_COLOR` set to any value, or stdout not a tty, means charter emits no SGR from the frame
at all** — and the property is `os.environ.get("NO_COLOR") is not None`, per the
`no-color.org` convention, not `== "1"`, because matching a value rather than presence is the
spelling-not-property mistake again.

> **Correction, 2026-09-02.** The rule above stands; the citation does not. `no-color.org`
> has said *"when present **and not an empty string** (regardless of its value)"* since
> `jcs/no_color` commit `99f90e27` (2022-06-27) — so this spec quoted the sentence that
> commit replaced, and charter's presence-only reading is charter's own choice rather than
> the convention's. It differs on exactly one input, `NO_COLOR=""`. The same page also says
> the standard is only about colour and not about bold, underline or italic; charter empties
> every SGR role regardless, which is stricter than it asks and is argued for in
> `chrome.no_colour`. Left in place rather than rewritten: this file is dated, and what it
> recorded on 2026-08-28 is part of the record.

**And `NO_COLOR` forces `[frame] chrome` to `off`, which is the half that is easy to miss.**
The surface is painted by tmux, not by charter, so gating only charter's own SGR would leave
an operator who asked for no colour looking at a coloured frame — charter having asked
somebody else to paint it. That is honouring the letter of the promise and not the property,
which is this spec's own subject matter. `NO_COLOR` means no colour on the operator's screen
caused by charter, whichever process puts the bytes there.

**`isatty` does almost nothing in a real frame, and that is worth saying plainly** so nobody
reads it as the main mechanism: a panel's stdout *is* the pane, so it is always a tty in
production. The only case it catches is the redirect `panel.py`'s own docstring documents
(`charter panel top --session x > /tmp/log`). `NO_COLOR` is what does the work.

---

## 4. Whose theme wins, and who is worse off

Charter cannot detect the operator's background colour. Two mechanisms were tried and both
failed on this machine:

**OSC 11 through tmux gets no answer.** A pane process wrote `\x1b]11;?\x07` and read for one
second. tmux *forwarded* the query to the outer terminal (the outer pty saw `\x1b]11;?`) and
the reply never came back to the pane:

```
OSC11 reply: b''
```

So the query costs a full second of the paint path and answers nothing — and it also emits an
escape which a terminal that does not implement it may echo as text.

**tmux 3.7c's own theme hooks did not fire.** `client-light-theme` and `client-dark-theme`
exist in the hook table and are documented. Against a pty client answering
`\x1b]11;rgb:ffff/ffff/ffff` — solicited, unsolicited, ST-terminated and BEL-terminated, over
eight seconds — neither fired. The harness is not at fault: `client-attached`, set the same
way in the same run, did fire. There is no `#{client_theme}` format on 3.7c either; the
format list has `client_termfeatures` and `client_termtype` and nothing about theme.

So charter is choosing in the dark, and the two candidate slabs both fail somewhere:

| style | dark theme | light theme |
|---|---|---|
| `bg=black` | at or near the background — invisible | a dark slab on a light page |
| `bg=white` | a light slab on a dark page | at or near the background — invisible |
| `bg=brightblack` | a mid-grey, lighter than the bg — correct | a mid-grey, darker than the bg — correct |
| `bg=brightblack`, **Solarized Dark** | brightblack *is* base03, the background — invisible | — |

`brightblack` is the near-answer and Solarized is the counter-example that kills it as a
default: Solarized deliberately maps the bright range onto its base tones, so on Solarized
Dark a `brightblack` surface is the background.

And the attribute route is closed. `window-style` accepts attributes and **silently ignores
them** — the option parser takes them, the drawing code does not. Measured, one value per
run, reading what reached the wire before the pane's content:

```
bg=colour236               -> \x1b[48;5;236m
fg=colour100               -> \x1b[38;5;100m
bg=black                   -> \x1b[40m
bg=brightblack             -> \x1b[100m
bg=white,fg=black          -> \x1b[30m\x1b[47m
reverse                    -> (nothing)
dim                        -> (nothing)
bold                       -> (nothing)
bg=default,fg=default,dim  -> (nothing)
bg=colour236,dim           -> \x1b[48;5;236m     <- the colour only; dim dropped
```

So `window-style reverse` — the one style that would have been theme-relative by
construction — does nothing at all. A tmux-drawn surface **must** name a colour, and no
colour is right on both themes.

### The decision

**The pane surface is opt-in, and it ships off.**

`[frame] chrome`, a closed enum: `"off"` (default), `"dark"`, `"light"`.

**Who is worse off:** an operator on a dark terminal who would have liked the fill, and who
now has to write one line. That is the cost, and it is paid in the direction this config
section already chose once. `instance.FRAME_FIELDS`' `mouse` is off by default with the
recorded reason *"Off is the default because an operator who has not asked for it keeps their
selection"* — a default that can make an existing working frame *worse* on upgrade must be
opt-in. A light-terminal operator upgrading into a default `dark` gets a frame that is worse
than the one they had, on a surface they never touched, having done nothing. A dark-terminal
operator upgrading into a default `off` gets a frame that is **better** than the one they had
— because §2's elements 1, 2, 3 and 5 ship on — and one line short of the one they wanted.

Those are not symmetric, and the asymmetry is the whole argument.

**There is no `"auto"`, and that is deliberate.** An `auto` that resolved to `off` would be a
config value that changes nothing while claiming to decide something — the convincing empty
this project keeps paying for (#512's "no repos" over a plane that had them). An `auto` that
guessed would be a guess wearing the word for a measurement. The three words say what they
do.

**The value is an enum, never a style string, and that is a containment boundary.** A tmux
style value **is format-expanded at draw time** — measured, stored verbatim and evaluated:

```
$ tmux set -p -t %1 window-style 'bg=#{?#{==:1,1},colour196,colour46}'
$ tmux show -p -t %1 -v window-style
bg=#{?#{==:1,1},colour196,colour46}          <- stored as written
   wire: b'...\x1b[48;5;196m\x1b[2BPANEL'    <- tmux evaluated the conditional
```

A committed `charter.toml` carrying a free style string would therefore be a committed value
reaching a tmux evaluator, which is the `[frame] hotkey` class exactly (`instance._HOTKEY_RE`:
a newline there ran a second tmux command at launch with no keypress). I could **not** turn
that into execution on 3.7c — `#(...)` is refused by the style parser directly
(`invalid style: bg=#(echo colour196)`), and nested inside a `#{?…}` it was stored, the false
branch was taken, and no command ran. **I could not achieve execution, and that is not the
same as it being safe**: the category is confirmed and only one version was tested. The
asymmetry `_HOTKEY_RE` already argues applies unchanged — an enum charter refuses that an
operator wanted costs them a rename; a style string charter accepted that tmux evaluates
costs an unknown amount on a version nobody ran. So the word is the config surface, charter
holds the style constants itself, and no operator string reaches tmux.

**The enum is whole-frame, not per component.** §8 of the foundation spec: *"It does not add
a config key per component. The registry is code; `[frame]` gains initial visibility, not
thirty knobs."* One frame has one look. A per-component colour key would be thirty knobs and
the first thing it would produce is a frame that does not match itself.

### Open question for the operator — answerable in one word

> **Should `[frame] chrome` ship as `off` or as `dark`?**
>
> **Recommendation: `off`**, for the asymmetry above — a default must not make an upgraded
> frame worse for anyone, and `dark` does that to every light-terminal operator. The frame
> still changes visibly on upgrade, because §2's elements 1, 2, 3 and 5 are theme-safe and
> ship on. The palette carries `chrome: dark` / `chrome: light` / `chrome: off` as three
> rows, so the operator who wants the fill is one keystroke from it and never has to find a
> config key.
>
> Answer `dark` and the default flips; nothing else in this spec changes.
>
> **And the operator does not have to wait for the answer to get what they asked for.**
> `charter.toml` on their own plane is a committed file they own: `[frame] chrome = "dark"`
> in charter's own `charter.toml` gives this plane the fill on the day Phase 3 lands,
> without making it the shipped default for a light-terminal operator who has never asked
> for anything. That is the division `[frame]` already runs on — the plane's own file says
> what this plane looks like, and `FRAME_DEFAULTS` says what a stranger's plane looks like
> before they have said anything at all.

---

## 5. The elements, named

Six elements, each with the mechanism, the theme argument and the blast radius. The names in
the left column are the vocabulary the implementation uses.

### 5.1 `surface` — the pane background

tmux, `window-style` / `window-active-style`, **pane-scoped** (`set -p -t <pane_id>`), set on
charter's panel panes and **never on the harness pane**. Gated by `[frame] chrome`:

| `chrome` | `window-style` | `window-active-style` |
|---|---|---|
| `off` | *unset* | *unset* |
| `dark` | `bg=black` | `bg=brightblack` |
| `light` | `bg=white` | `bg=brightwhite` |

Two named slots one step apart, so the focused pane is a shade off the others on both. Never
an index. Applied in `_chrome_argvs`' neighbourhood, from `_split_panels` — the one funnel
every panel pane charter creates comes out of, on both servers, which is where
`remain-on-exit` and `_CHROME` already are. Nothing here runs on a repaint.

**All four of charter's panes, not just the two bars.** An earlier draft of this scoped the
surface to `identity` and `attention` on the grounds that a 50-row sidebar fill is the most
expensive and the least necessary. That reasoning came from a design where charter painted
it. It does not, so the cost argument is gone — and what is left is that two chrome-coloured
panes beside two uncoloured ones is a frame that does not match itself, which is the opposite
of the ask.

**It survives a panel respawn**, which the alternative would not have. `commands_frame`'s
`pane-died` hook respawns a dead panel *into the same pane*, and these are pane options, so
the surface is a property of the rectangle rather than of the process in it. A renderer-side
fill would have to be re-established by whatever came back.

**And it leaves `Registry.draw` alone**, which matters for a test that would otherwise have
had to change: `tests/test_component_providers.py:654` pins that a component drawn at
`width=0, height=0` answers `()` — nothing at all. A fill applied unconditionally in the
draw path would have made that a row of zero spaces and broken it. tmux painting the
rectangle means the zero-size case still answers nothing, correctly.

### 5.2 `focus` — which region is live

`window-active-style`, and nothing else. It follows tmux's own pane focus, which §4e already
assigns to tmux, so it needs no `focus-events` and works inside an operator's own tmux where
charter sources no config.

**It must not be the border, and #514 is why.** The frame's rules came out in two colours
because tmux's `pane-active-border-style` defaults to `fg=green` while `pane-border-style`
defaults to `default`, so a rule running past the active pane's corner changed colour
mid-line. `_CHROME` fixed that by pinning both to the same value. A focus indicator drawn on
the border would reopen exactly that defect. `window-active-style` cannot: it is per pane and
has no corner to run past.

**It must not be a box either**, and that one is already enforced.
`tests/test_frame_slots.py:140` `NoPanelDrawsItsOwnChrome` asserts, for every slot in
`slots.SLOTS`, that no rendered line has a box glyph at both ends — with two live controls
proving the check can fail. A focused panel drawn as a box would be red there, correctly:
charter's panes are bordered by tmux and a panel drawing its own edges is the defect that
test exists for.

**With `chrome = "off"` there is no focus indicator.** Stated plainly rather than worked
around: a renderer cannot know whether its pane is active without asking tmux on every
repaint (forbidden — it is a subprocess on the paint path) or receiving focus events (which
exist only on charter's own server, `commands_frame.conf_text`'s `set -g focus-events on`).
Focus is the compositor's to say, and the compositor says it in colour.

### 5.3 `heading` — a region with a name

Weight, not a new row. `slots._sidebar_head` already draws `▪ personas 5` in dim; it becomes
the label in **bold** with the count still dim, at the existing pad. **No row is added
anywhere.**

That constraint is not aesthetic. Fifteen-plus tests assert the exact line count of a panel —
`tests/test_frame_density.py:383` (`assertEqual(len(normal.split("\n")), 1 + 9)`),
`tests/test_frame_slots.py:1104` (`assertEqual(len(self._render(rows=6).split("\n")), 6)`),
and thirteen more. **A heading row is the single change with the widest blast radius in this
whole spec**, and it buys nothing that weight does not. The heading tests that exist
(`tests/test_frame_slots.py:1539`, `:814`) compare through `tui.strip_ansi`, so a weight
change is invisible to them and they stay green for the right reason.

### 5.4 `inset` — content is inside something

One constant for the column content starts at, read by every renderer rather than spelled per
call site. `statusline._HEAD_PAD` is the value that already exists; this makes it the answer
rather than one of several. No row change, no width change.

### 5.5 `selected` — the row you are on

**Full-width reverse video, and this is the element that most makes the frame read as an
app.** The active persona is `▸ steward` today — a glyph in a list. It becomes the whole row,
inverted, to the pane's edge.

Reverse is theme-safe **by construction**: it is defined as the operator's own foreground and
background exchanged, so it is correct on every theme including Solarized, and it needs no
colour and therefore no `chrome` gate. It survives every tier measured — `tui.sanitize` keeps
SGR 7 (measured: `'\x1b[7m x \x1b[27m'` in, unchanged out), tmux passes `ESC[7m` through to
`xterm`, `xterm-256color` and `vt100` alike, and on a colour-less client tmux *converts* its
own colour to reverse.

It is the one element that needs new painting machinery, because it must reach the pane's
last column — and it is the one element with a defect that only appears when you build it.
**A reverse row cancels itself at the first `\x1b[0m` inside it**, and charter's rows are full
of them. §6.5 measures it and specifies the fix, which has a trap of its own.

### 5.6 `status` — ok / warn / error

The colours already exist (`\x1b[32m✎ 4`, `\x1b[33m⚠`) and already pair with glyphs. What
this spec adds is the rule, so the pairing stops being a habit:

> **A status conveyed by colour alone is a status some operators cannot read.** Every status
> in the frame carries a glyph or a word that says the same thing. Colour is the second
> channel, never the only one.

Enforced rather than asked for: a test strips every SGR from each renderer's output and
asserts the remaining text still distinguishes ok from warn from error. That is the same
shape as `tests/test_frame_slots.py`'s `NoPanelDrawsItsOwnChrome`, which already asserts a
structural property over every slot in `slots.SLOTS`.

The three roles map to the sixteen names, never to indices: `ok` → `green`, `warn` →
`yellow`, `bad` → `red`. Which is what the code already does; it is written down so the next
field added does not reach for `colour208`.

---

## 6. The painting seam, and the five measured hazards

### 6.1 `tui` destroys a background fill, and `tui` is right

Every `tui` node strips trailing whitespace, including whitespace hiding behind trailing SGR
escapes (`tui._finish`, `tui._HIDDEN_TRAIL`). Measured — a 20-cell fill in, an 8-cell line
out, both ways round:

```
fill inside the span  in ='\x1b[48;5;236m charter            \x1b[0m'   width 20
                      out='\x1b[48;5;236m charter\x1b[0m'               width  8
pad outside the span  in ='\x1b[48;5;236m charter\x1b[0m            '   width 20
                      out='\x1b[48;5;236m charter\x1b[0m'               width  8
Row(Cell(bg+' charter', 20))                                            width  8
tui.truncate(same, 40)  -> unchanged, width 20    <- _finish is the destroyer
```

**`_finish` does not change.** `tui` is also the status line, which writes into a line it does
not own and must never leave painted cells trailing across somebody else's prompt. And a
line with no trailing whitespace is what keeps a copy out of the frame clean. The property is
right; it is simply not the frame's whole story.

**So the fill is applied by the frame, after `tui` has rendered, in one place that knows the
pane's real rectangle** — a new `charter/frame/chrome.py`. Not inside `tui`, not inside each
renderer.

*After* is load-bearing and names a specific line. `slots._persona_rows:892` composes each
persona row as `tui.Row(...).render(width)[0]`, and `slots._top:351` does the same for the
top bar — both go through `_finish`. A fill applied before that call is stripped by it.
Measured, the finished highlighted row handed back into a `Row`:

```
in : '\x1b[7m…steward\x1b[0m\x1b[7m   \x1b[32m✎47\x1b[0m\x1b[7m<25 spaces>\x1b[27m'   width 40
out: '\x1b[7m…steward\x1b[0m\x1b[7m   \x1b[32m✎47\x1b[0m\x1b[7m\x1b[27m'              width 15
```

Whereas `tui.truncate` at the pane width returns it unchanged. So the ordering rule is:
compose the row through `tui` as today, **then** hand the finished string to `chrome`.

One measured consequence worth recording: `_finish`'s behaviour is **pinned by nothing**
today, in either direction. `tests/test_tui.py` covers only `term_width`;
`tests/test_tui_control_chars.py` covers `width`/`sanitize`/`truncate`/`pad`. The only file
that names `_HIDDEN_TRAIL` is
`tests/test_the_end_of_a_name_is_the_end_of_the_string.py:249`, which classifies it in a
substitution inventory. A test that a rendered line carries no trailing whitespace is part of
Phase 1, because this spec is about to build on a promise nothing checks.

### 6.2 A full-width fill removes every cell of slack

Measured in a 20-column pane, one row each at W−1, W and W+1 cells:

```
row 0: '\x1b[48;5;196mAAAAAAAAAAAAAAAAAAA\x1b[49m '   <- 19 cells, bg stops at col 19
row 1: '\x1b[48;5;46mBBBBBBBBBBBBBBBBBBBB'            <- 20 cells, exactly W: SAFE
row 2: '\x1b[48;5;21mCCCCCCCCCCCCCCCCCCCC'            <- 21 cells: wraps…
row 3: 'C\x1b[49m    '                                <- …one cell onto the next row
row 4: '\x1b[48;5;226mDDD\x1b[49m  '                  <- and every row below shifted down
```

Exactly W is safe — the deferred-wrap state is resolved by the following `\n` and produces no
blank row. W+1 shears the pane.

**This is the real cost of element 5.5.** Today's lines are ragged and short, so an
off-by-one is a cosmetic gap. A row painted to the edge is one cell from shearing the pane,
which is #553 arriving through a new door. Two rules follow, and both are Phase 1 exit
criteria:

- **One measurement, not two.** The fill uses the same width the renderer used. `slots._width()`
  asks the pane's own tty because a panel process inherits the *launching* shell's `$COLUMNS`
  (`slots.py`'s docstring measures a 22-column pane whose launcher had exported
  `COLUMNS=200`). A fill computed at 200 in a 22-column pane wraps every row four times over.
  The Phase 2 plan already records the mutation that proves this class:
  `panel._component_text`'s `width=slots._width()` replaced by a constant `80` made a
  provider's output wrap and destroy the frame in a 40-column pane.
- **The guard is a test that goes red when the clamp is deleted**, per the deletion sweep.

### 6.3 A missing reset outlives the paint that made it

`panel._write` writes `\x1b[H\x1b[2J` and then the content, with no reset first. A renderer
that leaves a background set — a provider's, or charter's own after a future edit — makes the
**next** repaint's clear-screen fill the whole pane with it. Measured, two paints in one pane:

```
during the leaking paint (a component that omitted \x1b[0m):
  row 0: '\x1b[48;5;196mLEAK\x1b[49m '
  row 1..4: ''

after the next '\x1b[H\x1b[2J' + 'second paint':
  row 0: '\x1b[48;5;196msecond paint        '   <- the leak survived the clear
  row 1: '                    '                 <- and filled every other row
  row 2: '                    '
```

Constraint 4 holds — it costs that pane and no other — but the pane stays wrong until
something resets it, which nothing does. **Fix: `_write` prefixes `\x1b[m`.** One escape, on a
path that already writes two, and it makes "a broken component costs its own pane" true for
one paint rather than for the rest of the session. Worth having before any component paints a
background, which is what this spec is about to make normal.

### 6.4 `ESC[K` is not available, and does not need to be

The cheap way to fill a row is to set a background and erase to end of line. It is unavailable
twice over, and both are already-shipped facts rather than new decisions:

- **`tui.sanitize` deletes it.** `\x1b[K` is a CSI that is not SGR, so `_MARKUP_OR_CONTROL`
  removes it before anything is measured. Measured: `'\x1b[48;5;236m x\x1b[K'` in,
  `'\x1b[48;5;236m x'` out. A component cannot emit it through the contained path at all.
- **`tmux-256color` declares no back-colour erase** (`tigetflag('bce') == 0`, while
  `xterm-256color` answers 1) — even though tmux 3.7c *does* honour it in practice, measured
  with a red `bg + "A" + ESC[K` covering all 30 columns of a pane while a plain `bg + "B"`
  covered exactly one. The database and the behaviour disagree; the property charter can rely
  on is neither, because the fill it needs is now tmux's job anyway.

### 6.5 A reverse row cancels itself at the first inner reset — and the fix has its own trap

This one killed the naive version of §5.5. Charter's rows carry `statusline._R`
(`"\033[0m"`) after every coloured span, and a full reset cancels **reverse** along with
everything else. Measured, the actual sidebar row wrapped in `\x1b[7m…\x1b[27m`:

```
row : '\x1b[35m▸ \x1b[1msteward\x1b[0m   \x1b[32m✎47\x1b[0m'
out : '\x1b[7m\x1b[35m▸ \x1b[1msteward\x1b[0m   \x1b[32m✎47\x1b[0m<pad>\x1b[27m'
                                        ^^^^^^^^ reverse ends here, 22 chars in
```

The row is highlighted for two words and plain for the rest. **A highlight cannot be a
wrapper around an already-composed row.**

The fix: `chrome.reverse` re-asserts `\x1b[7m` after every SGR that resets all attributes.
Measured on the same row — three re-assertions, width exactly 40, and `tui.truncate` at the
pane width returns it unchanged.

**And the fix's own first version was the mistake this spec keeps naming.** "An SGR that
resets everything" is not the string `\x1b[0m`. It is a parameter list containing a parameter
whose *numeric value* is zero — and an empty parameter is zero, and leading zeros are legal.
Testing the parameter as a string misses two real spellings:

```
spelling      params    string-match ("" or "0")   numeric value == 0
'\x1b[0m'     '0'       True                       True
'\x1b[m'      ''        True                       True
'\x1b[00m'    '00'      False   <- MISSED          True
'\x1b[1;00m'  '1;00'    False   <- MISSED          True
'\x1b[2m'     '2'       False                      False
'\x1b[22;39m' '22;39'   False                      False
```

A row carrying `\x1b[00m` would highlight for half its width and every test written against
`\x1b[0m` would pass. That is #547, #558, #537, #498 and #577's shape, found inside the fix
for it, before it shipped. The property is the number; the spelling is `\x1b[0m` because that
is what charter happens to write today, and what a provider writes is not charter's to
choose.

### 6.6 What the fill costs

Explicit spaces cost `W` bytes a row. Measured: filling a 200-cell top bar costs **41 µs** on
a render that already costs 727 µs (+5.6%), and filling all 50 rows of a 22-column sidebar
costs **202 µs** on a render that already costs 6 826 µs (+3.0%). Paid per pane per repaint,
which is per version bump or resize — and only `bottom` repaints on its own clock, at 5 Hz,
while work is in flight (`slots.ANIMATED`). For element 5.5 that cost applies to **one row**,
the selected one, not to the pane.

---

## 7. Providers, and how this does not become a ransom note

§4b says a provider's component draws its own rectangle. That is unchanged, and the coherent
look does not come from taking it back.

**Charter owns the surface and the border; the provider owns every cell it writes.** The
surface is *underneath* — tmux paints the pane, and a provider's cells sit on it. A provider
that paints nothing gets charter's surface for free and matches. A provider that paints its
own background overrides its own cells and nothing else: it cannot reach past its rectangle,
because `Registry._fit` already contains and clips every foreign row (`escape=cid in
self._foreign`, `contain.one_line` before the width arithmetic).

**Charter does not overdraw a provider's heading**, which was the obvious alternative and is
worse. It would cost a row — §5.3's blast radius — and it would break `Registry.draw`'s
height budget for the one class of component charter did not write. §4b's rectangle is the
provider's; charter does not take a row out of it to make things match.

**What charter hands over instead is the recipes.** `ctx` gains one read-only mapping of the
roles in §5 to their SGR strings, resolved for this frame's `chrome` setting and this pane's
state. A provider that wants to match writes `ctx.chrome.heading` and matches; one that does
not, does not, and looks different — which is honest, because it *is* different, and a frame
where every pane looks identical regardless of who wrote it would be hiding the one thing an
operator needs to know when a pane is wrong.

This is a widening of `ctx`, which `ctx.Ctx`'s docstring says should cost a test change and
the conversation that goes with it: *"a future field is a widening of what a stranger's code
may reach"*. The conversation is this paragraph. The field is a `MappingProxyType` of strings,
carries no callable, and reads nothing — it is the same shape `SERVES["gather"]` already
returns.

> **Counter-argument, answered:** *how does this not become unreadable the moment a provider
> paints its own colours next to charter's?* It can, and charter cannot prevent it — a
> provider's module is ordinary Python and `ctx`'s own docstring says outright that it is not
> a sandbox. What charter can guarantee is the three things it already guarantees: the
> provider's paint stops at its rectangle, its failure costs its pane and not the session, and
> its output is contained before it reaches the terminal. Adding "and it looks like ours" to
> that list would be a promise enforced by nothing. The honest version is the recipes plus the
> containment, and a pane that clashes is a pane whose provider chose to.

---

## 8. What changes for existing operators

| | before | after, `chrome` unset |
|---|---|---|
| pane backgrounds | none | none |
| focus indicator | none | none |
| headings | dim label + count | **bold** label, dim count — same row, same width |
| content inset | per call site | one constant, same value |
| active persona row | `▸ steward` | the whole row, reverse, to the pane edge |
| status | colour + glyph, by habit | colour + glyph, by test |
| `NO_COLOR` / not a tty | escapes emitted anyway | **no SGR at all** |
| `[frame]` keys | 7 | 8 (`chrome`) |

Nothing is removed and no row count changes anywhere. The one behaviour that *changes* for an
existing operator rather than being added is `NO_COLOR`/not-a-tty, and it changes in the
direction of a promise charter did not previously keep.

**Opt-in:** `chrome = "dark"` or `chrome = "light"` adds the pane surface and the focus
indicator. The TOML key is `chrome` — one word, so the hyphen question
(`tests/test_frame_config.py:136` requires `history-limit`, not `history_limit`, for
multi-word keys) does not arise.

**`docs/frame.md:200-209` needs its sentence updated.** It currently says the pane-border
pinning is *"the one place charter overrides a preference of yours rather than deferring to
it"*. With `chrome` set that is no longer true — `window-style` is the second. It stays true
for the default, because the default sets nothing.

---

## 9. What I could not measure

Stated rather than reasoned about, per this project's rule.

- ~~**tmux 3.1c and 3.2.**~~ **ANSWERED in Phase 3.5 — the floor behaves identically, and
  nothing is gated.** tmux 3.2 was built from source on this machine (`./configure &&
  make`) and the whole of §1 and §4 was re-run against it, one style per server, with the
  SGR lifted out of an attached client's wire rather than read out of forty bytes of
  context. Sixty-six of sixty-eight answers were byte-identical to 3.7c; the two that were
  not (`window-style bold`, and one pane's context window) were the measuring harness's own
  batching — tmux emits an SGR only when the style CHANGES, so four panes that downsample
  to the same colour produce one escape and three silences, which reads as "three styles
  tmux ignored" and is nothing of the sort. Re-run one pane per server they matched too.

  | measured at `tmuxctl.FLOOR` (3.2) | answer | 3.7c |
  |---|---|---|
  | `set -p window-style` pane-scoped? | yes — sibling panel, harness pane and `show -w` all `''` | same |
  | honours colour only? | yes — `reverse`/`dim`/`bold` each put **no SGR at all** on the wire; `bg=colour236,dim` put `ESC[48;5;236m` and nothing else | same |
  | do the 16 ANSI names resolve? | yes, all 16 — `bg=black`→`ESC[40m` … `bg=white`→`ESC[47m`, `bg=brightblack`→`ESC[100m` … `bg=brightwhite`→`ESC[107m` | same |
  | active/inactive split | active pane `ESC[100m`, inactive `ESC[40m`, harness `ESC(B ESC[m` | same |
  | style value format-expanded | stored verbatim: `bg=#{?#{==:1,1},colour196,colour46}` | same |
  | `#(...)` in a style | refused by the parser: `invalid style:` | same |
  | a refused `set-option` | rc 1, `invalid style: bg=notacolour`, previous value intact — reported, not fatal | same |
  | survives `respawn-pane`, `resize-window` | yes | same |
  | downsample per client | 256 → `ESC[48;5;236m`; 8-colour `xterm` → `ESC[40m`; `vt100` → `ESC[7m` | same |
  | `set -p -u` removes it | rc 0, reads back `''`; unsetting one never set is also rc 0 | same |
  | live `set -p` on an attached pane | repaints by itself — `ESC[40m` on the wire with no `refresh-client` | same |

  **So `chrome` is not gated at a version**, and the absence of a gate is asserted rather
  than left implicit (`tests/test_frame_surface_live.py`). The spec's contingency — "if 3.2
  differs, `chrome` is gated at the version that works, the way `display-popup` is gated at
  3.3" — did not come due.
- **What the sixteen ANSI names actually resolve to in the operator's terminal.** The claim
  that 0–15 track the operator's chosen palette while 16–255 are a fixed cube is the basis
  for §3.2's "named slots, never indices", and it is a documented property of terminal
  emulators, not something I ran here. What I *did* measure is tmux's arithmetic over them
  (24-bit → `colour237`, → `ESC[40m`, → `ESC[7m`), which is consistent with it and does not
  prove it.
- **Whether the design looks right.** No screenshot was taken and nobody looked at a screen.
  §5's elements are specified as mechanisms; whether a bold heading and a reverse selected row
  read as an application is a judgement that needs an eye on a real terminal, in both a light
  and a dark theme. That is Phase 2's exit criterion and it is not satisfiable by a test.
- **Execution through a tmux style value.** §4 records the attempts. The category is
  confirmed (styles are format-expanded); execution was not achieved and was not proven
  impossible.
- **`tmux -CC` (control mode), the case iTerm2's native tmux integration uses.** Measured
  only this far: a `-CC` attach emits a DCS-wrapped control protocol and no screen output at
  all —

  ```
  $ tmux -CC -L probe attach
  \x1bP1000p%begin 1787864930 292 0\r\n%end …\r\n%session-changed $0 0\r\n
  ```

  — so the *client* draws the panes, not tmux. Whether `window-style` reaches a `-CC`
  client's native rendering was not measured, because it needs iTerm2, which is not
  scriptable here. **The failure direction is known and is the safe one**: §5's renderer
  elements are SGR the panel writes into its own pane and tmux records in the pane's screen,
  so they survive control mode unchanged; if the surface does not reach a `-CC` client, it is
  simply absent, which is `chrome = "off"`. Degrading to nothing rather than to something
  wrong is why this needs no version gate — but it should be confirmed on a real iTerm2
  before `docs/frame.md` promises the surface to anyone.

---

## 10. Phased implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:subagent-driven-development`.

**Spec:** this file. **Foundation:** `docs/superpowers/specs/2026-08-25-agentic-ide-foundation.md`
§4b, §4c, §4e, §4k. **Measurements:** §1, §3.1, §4, §6 above — raw bytes, with the commands.

### What the measurements already decided

- **tmux paints the background, charter does not** (§1). `window-style`/`window-active-style`,
  pane-scoped. Nothing new is on the repaint path.
- **`$COLORTERM` inside a pane is stale** (§3.1) — it describes the terminal that started the
  server. It is not in `update-environment`. Never read it in a panel.
- **`curses.setupterm` is one-shot per process** (§3.1) and silently answers the first
  terminal's numbers afterwards. A test comparing two terminals in one process passes with
  the feature dead.
- **`window-style` honours colour only** (§4) — `reverse`, `dim` and `bold` are accepted and
  ignored.
- **A tmux style value is format-expanded** (§4). The config surface is an enum; no operator
  string reaches tmux.
- **`tui._finish` deletes a fill and is right to** (§6.1). The fill goes after the node, in
  the frame, never in `tui` — and *after* names `slots._persona_rows:892` and `slots._top:351`,
  which are `tui.Row(...).render()` calls.
- **Exactly the pane width is safe; W+1 shears the pane** (§6.2).
- **A missing reset survives the next clear-screen** (§6.3).
- **A reverse row cancels itself at the first full reset inside it** (§6.5), and "full reset"
  is a numeric parameter value, not the string `\x1b[0m` — `\x1b[00m` and `\x1b[1;00m` reset
  everything too.

### Global constraints

- `dependencies = []`, stdlib only (`tests/test_packaging.py:26`). No colour library, ever.
  stdlib `unittest`, never pytest.
- Run the suite in **two environments** and say which; they must agree.
- `PersonaIso`; new `patch.dict(os.environ, …)` MUST pass `clear=True`.
- `tui.width()` never `len()`. `contain.one_line` **before** width arithmetic.
- **No renderer gains or loses a row.** Fifteen-plus tests assert exact line counts
  (`tests/test_frame_density.py:383,443`; `tests/test_frame_slots.py:832,1088,1104,1139,1196`
  and more). A change that needs a row is a change that needs this list revisited first.
- Mutation-test every guard: apply, RED, restore, GREEN, `__pycache__` cleared. **Report the
  mutation actually run and its actual result.**
- **The deletion sweep is required before any PR in this phase**
  (`docs/superpowers/specs/2026-08-27-deletion-sweep-harness.md`). For every `if` that
  refuses, clamps, contains or falls back, write the test that goes RED when that line is
  deleted — and **assert which refusal fired**, not just the exit code. Two guards in
  sequence mask each other.
- No version bump, no stamping, no tag.

---

### Phase 1 — the fill primitive, and the promise nothing checks

*Nothing visible ships. This phase exists so the next one can be believed.*

- [ ] **1.1** Pin `tui._finish` in both directions — a rendered line carries no trailing
      whitespace, including whitespace behind a trailing SGR. It is unpinned today (§6.1) and
      this spec is about to depend on it. Failing test first.
- [ ] **1.2** `charter/frame/chrome.py`: `fill(row, width)` → the row padded to exactly
      *width* cells with the pad **inside** the style span, measured with `tui.width`. Takes a
      **finished** string — one that has already been through whatever `tui` node composed it
      — and says so in its docstring, because a caller who reverses the order gets a row that
      silently comes back 15 cells wide (§6.1).
- [ ] **1.3** `chrome.reverse(row, width)` over `fill`, re-asserting `\x1b[7m` after every SGR
      that resets all attributes (§6.5). **The test is the leading-zero case, not the happy
      one:** a row carrying `\x1b[00m` and a row carrying `\x1b[1;00m` must both stay
      highlighted to the last column. A `p in ("", "0")` implementation passes every test
      written against `\x1b[0m` and fails both of these — write those two first and watch them
      fail. Mutation: swap the numeric test for the string test; confirm RED.
- [ ] **1.4** The wrap guard: `fill` refuses to answer more than *width* cells, and the test
      that goes RED when the clamp is deleted asserts **which** refusal fired. Mutation:
      replace the measured width with a constant `80`; confirm a 22-column pane wraps and the
      test is RED.
- [ ] **1.5** `panel._write` prefixes `\x1b[m` (§6.3). Failing test first: a paint that leaves
      a background set must not colour the next paint's whole pane. Note the four call sites
      that structurally depend on `_write` emitting `\x1b[H\x1b[2J` and splitting on it —
      `tests/test_frame_panel.py:129,172,193` and
      `tests/test_component_id_is_the_currency.py:110` — and put the reset **before** the
      cursor-home so `split("\x1b[2J", 1)[1]` still answers the content. Mutation: remove the
      prefix; confirm RED.
- [ ] **1.6** `NO_COLOR` and not-a-tty (§3.2). The property is
      `os.environ.get("NO_COLOR") is not None` — **presence, not a value** — and
      `sys.stdout.isatty()`. One function, asked by the frame, never re-spelled: two
      implementations of one question hide each other's defects (#547). Mutation: change the
      check to `== "1"`; confirm RED with `NO_COLOR=""` and with `NO_COLOR=0`.
- [ ] **1.7** Full suite, two environments, **no existing test modified**. Deletion sweep run
      and reported.

**Exit criteria.** A row filled to exactly the pane's measured width paints one row in a real
tmux pane and shears nothing; the same row computed against a wrong width is refused, and the
refusal is named. A reversed row carrying `\x1b[00m` is highlighted to its last column.
`NO_COLOR=` (empty) suppresses colour. A component that omits its reset costs one paint, not
the session. `charter/tui.py` is byte-identical to `main`.

---

### Phase 2 — the elements, on by default

*The visible payoff, and the half that is theme-safe.*

- [ ] **2.1** `heading` — `slots._sidebar_head`'s label in bold, count still dim, **same row,
      same width**. The existing heading tests compare through `tui.strip_ansi`
      (`tests/test_frame_slots.py:814,1539`) and must stay green **unmodified**; if one needs
      changing, the change is wrong.
- [ ] **2.2** `inset` — one constant, read by every renderer. `statusline._HEAD_PAD` is the
      value; make it the answer rather than one of several. No width or row change.
- [ ] **2.3** `selected` — the active persona row, full-width reverse, through Phase 1's
      `chrome.reverse`, applied **to the finished row `_persona_rows` returns**, not to the
      cells it composes (§6.1: line 892 is a `tui.Row(...).render()` and would strip the pad).
      Applied inside `persona_section`, so `tests/test_builtin_components.py:272` — raw byte
      equality of `slots.render("right")` against `"\n".join([*personas, "", *todos])`, the
      most brittle assertion in the suite for this spec — stays green **unmodified**, because
      both sides reach it through the same helper. If that test needs touching, the highlight
      has been put at the wrong level.
- [ ] **2.4** `status` — the rule made a test: strip every SGR from each slot's output and
      assert ok/warn/error are still distinguishable. Same shape as
      `tests/test_frame_slots.py:140` `NoPanelDrawsItsOwnChrome`, over every slot in
      `slots.SLOTS`, with live controls proving the check can fail.
- [ ] **2.5** Mutations: drop the reverse on the selected row; drop a status glyph and keep
      its colour; widen the inset in one renderer only. Each RED.
- [ ] **2.6** `docs/frame.md` gains an appearance section — there is none today. It says what
      the frame draws, that status is never colour alone, and that `NO_COLOR` is honoured.
- [ ] **2.7** News entry (this phase is user-visible), full suite, two environments, sweep.

**Exit criteria.** Every exact-line-count test in §Global constraints passes **unmodified**.
`tests/test_builtin_components.py:272` and `tests/test_frame_slots.py:140` pass
**unmodified**. A frame with no `[frame] chrome` at all shows a named region, a consistent
inset, a visible selected row, and a status readable with every SGR stripped. **And a human
has looked at it on a light terminal and on a dark one** — §9 says this is not satisfiable by
a test, and the phase does not exit without it.

---

### Phase 3 — the surface, opt-in

*The one thing that cannot be made theme-safe, behind one word.*

- [x] **3.1** `[frame] chrome` in `instance.FRAME_FIELDS`, TOML key `chrome`, a closed enum
      `off` / `dark` / `light`, default `off`. Validated at the config boundary the way
      `density_level` and `toggle_key` are, `isinstance` first — `tomllib` can hand it a list
      or a table and `config.FRAME` resolves on `charter --version`.
- [x] **3.2** The style pair applied pane-scoped from `_split_panels`, beside `_chrome_argvs`
      — the one funnel, both servers, at launch and at a density change, **never on a
      repaint**.
- [x] **3.3** The harness pane is never styled. Test it by reading it back:
      `show -p -t <harness> -v window-style` must answer `''`, and the client wire must carry
      no colour before the harness's content. This is ADR 0018's boundary; assert it rather
      than intend it.
- [x] **3.4** **No operator string reaches tmux.** The enum maps to style constants charter
      holds. Mutation: let the config value through as a style; confirm RED, and confirm the
      test names *which* refusal fired.
- [x] **3.5** Run §1/§4's measurements again on **tmux 3.2** (`tmuxctl.FLOOR`) — pane-scoped
      `set -p window-style`, the active/inactive split, and the per-client downsample. §9 says
      they were run on 3.7c only. If 3.2 differs, `chrome` is gated at the version that works
      and says so, the way `display-popup` is gated at 3.3 (§4k).
- [x] **3.6** The palette carries `chrome: dark` / `chrome: light` / `chrome: off` as three
      rows, so the operator who upgrades into a look they dislike is one keystroke from
      fixing it rather than one documentation search.
- [x] **3.7** `docs/frame.md:200-209`'s "the one place charter overrides a preference of
      yours" becomes true again — it is no longer the only one when `chrome` is set.
- [x] **3.8** News entry, full suite, two environments, sweep.

**Exit criteria.** With `chrome = "dark"`, charter's panel panes carry a background and the
harness pane provably does not — read back from tmux, not from charter's own intent. With
`chrome` unset, `show -p` answers `''` for every pane and the frame is byte-identical to
Phase 2's. A `chrome` value that is not one of the three words leaves the frame at `off` and
`charter --version` still runs. The measurements are re-run on 3.2 and the answer is
recorded, whichever way it goes.

---

### Phase 4 — the provider seam

*So a provider can match without charter overdrawing it.*

- [x] **4.1** `ctx` gains the roles of §5 as a read-only mapping of SGR strings, resolved for
      this frame's `chrome` and this pane's state. A `MappingProxyType`, no callable, reads
      nothing — the shape `SERVES["gather"]` already returns.
- [x] **4.2** The exact-attribute-set test on `Ctx` is updated, deliberately and visibly —
      `ctx.Ctx`'s docstring says a widening should cost a test change and the conversation
      that goes with it. §7 is the conversation; the test change is the cost.
- [x] **4.3** A provider that ignores the recipes still cannot reach past its rectangle.
      Assert it: a provider whose rows carry a background and no reset colours its own pane
      and no other, and its next repaint is clean (Phase 1.5).
- [x] **4.4** `docs/frame.md`'s provider section says what a provider gets and what charter
      does not promise: the surface is charter's, the cells are the provider's, and a pane
      that clashes is a pane whose provider chose to.
- [x] **4.5** Mutations: hand a provider a mutable recipes dict; let a foreign row skip
      `_fit`. Each RED.
- [x] **4.6** Full suite, two environments, sweep.

**Exit criteria.** A provider's component drawn with the recipes is indistinguishable from a
built-in at the same size. One drawn without them is visibly different and harms nothing
outside its pane. `dir(ctx)` and `vars(ctx)` still carry no callable of charter's.

---

### Across all four phases

- The suite gives the same answer in two environments **and CI is green at the head sha under
  review**. Two local environments cannot see a CI-only failure (#554 passed 12/12 locally
  while CI was red at that exact head). Read
  `gh api repos/diazoxide/charter/commits/<HEAD_SHA>/check-runs`, which cannot confuse "green"
  with "no run was ever created" (#561).
- **The deletion sweep is run by the repository, not promised by whoever wrote the branch.**
  Rounds one through three found thirty-six unpinned guards by hand, and round three's own fix
  commit — the one whose message says every added guard now has a test — added six more.
