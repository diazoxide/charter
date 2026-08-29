"""The overlay: charter's own modal surface, and the one key that always leaves it.

**Full-pane, charter-drawn, modal** (§4k). Not `display-popup`, and not `display-menu`.
Both were measured before this module was written, and both lost:

* On tmux **3.2** — a version `tmuxctl.below_floor_message` explicitly still launches on
  — **any client resize kills a popup**: `rc 129` (128 + SIGHUP), the popup's own log
  ending mid-stream, with whatever the operator was typing. 3.3 changed that ("Do not
  close popups on resize, instead adjust them to fit"), so a popup-first palette is one
  surface that works and one resize-shaped bug on exactly one version, plus two surfaces
  to keep in step. A full pane always works.
* `display-menu` draws a list and takes a keypress, and charter's own nine-row cap sat
  on top of it — charter's cap, not tmux's; tmux 3.1c drew 20 rows fine. Neither
  filters, neither scrolls, and neither is charter's to draw in. (`frame/menu.py` is
  gone: `frame/palette.py` is what `F2` opens now, on this surface.)

The popup's one real advantage survives and is why §4k keeps it as a later enhancement: a
popup **is** the active surface, so its own mouse request is what reaches the terminal.
This module gets the same property a different way — by making its pane the active one
and zooming it over the window (:func:`modal_argvs`).

---

**Input is bytes; output is text.** A pane's input is a byte protocol — SGR mouse reports
are bytes, and an escape sequence can arrive split across two reads — while everything
charter draws is text `tui` has already measured. :func:`decode` owns the first half and
holds back a partial sequence rather than guessing at it; :class:`Surface` owns the
second and never calls `len` on anything it is about to lay out.

**A `click` release may arrive with no matching press, and this module keeps no press
state.** Measured, with tmux's own `mouse` off: a drag that begins on a pane border and
ends inside a pane delivers exactly one release (`b'\\x1b[<0;70;4m'`) — the press was
dropped because a border is not a pane, and the motion was dropped because the pane asked
for 1000 rather than 1002/1003. §4i states the consequence: the first component that
keeps press state wedges on it. So a release is a click on its own terms here, and — for
the same reason — a click only ever **selects**. `Enter` is what chooses. The irreversible
half is never driven by an event that can arrive unpaired.

**Pointer events at all only because this pane asked for them.** §4i's correction to §4c
is that charter's panels receive pointer events only while the ACTIVE pane requests
reporting, and charter does not control what the harness requests. The overlay is the one
place charter can promise them, because while it is open it *is* the active surface. One
declaration — :attr:`Surface.mouse` — both writes :data:`MOUSE_ON` to the tty and decides
whether a report is acted on, so there is no state in which charter acts on a report it
never asked the terminal for.

---

**The escape hatch is a tmux key table entry and runs no charter code.**

§4e: focus has two levels. tmux owns pane focus; charter owns intra-pane focus; and *the
escape hatch operates at the tmux level*, so it works when charter's own loop is wedged
or a third-party component has captured input badly. A single-level hatch would live
inside the thing it must escape.

So :func:`hatch_bind` emits `bind -n <key> run-shell -C '#{@charter_hatch}'` — tmux
matches the key in its own root table before any byte reaches the pane, and `run-shell
-C` runs the expansion as **tmux commands**, with no shell, no `$PATH` and no second
charter process. Measured against tmux 3.7c: the key returns the harness to the operator
while the overlay's process is in `time.sleep` with its tty in raw mode, never having
read a byte. `run-shell -C` first exists in tmux **3.2** (CHANGES FROM 3.1c TO 3.2: "Add
a -C flag to run-shell to use a tmux command rather than a shell command"), which is
`tmuxctl.FLOOR` exactly.

**Why an option and not the pane ids themselves.** A key table is server-wide in tmux —
there is no per-session keymap — so every frame on charter's shared private server ends
up sharing this one bind. `commands_frame.conf_text`'s docstring records what a bind that
embedded one frame's own identity costs: the second frame launched opens the *first*
frame's menu. The ids therefore live in a **window** option, and the presser's own window
answers for them. Measured with two windows on one server, each with its own value: F12
pressed on window 1 selected window 1's harness and left window 0 untouched.

**Order inside the command is the unconditional half of the promise.** The harness is
selected *first* and the overlay killed second, so a kill that cannot happen — a stale
pane id, an overlay already gone — costs nothing that has not already been delivered.
Measured: a `kill-pane` naming a pane that no longer exists is silent (nothing on the
client, nothing in any pane, no copy-mode) and the `select-pane` before it has already
run.

**The hatch exists on charter's own server and nowhere else, and that is a real limit.**
`bind -n` writes a ROOT key table, and a key table is server-wide with no per-window form
— so on the path where charter builds the frame as a window inside the operator's OWN
tmux (`commands_frame._launch_in_operator_tmux`), binding this key would take F12 from
every window they have open. Charter binds nothing there, exactly as it binds no hotkey
there. The honest consequence: on that path the way out of a pane that has stopped
answering is the operator's own prefix key, and `docs/frame.md` says so beside the other
costs of being a guest. A promise made on both servers and kept on one would be worse
than the limit.

**And an empty target is not a no-op — it is the current pane.** Measured against tmux
3.7c: with `@charter_hatch` expanding to `kill-pane -t ""`, tmux killed the pane the
command was running against. That is why :func:`hatch_command` never emits a `kill-pane`
at all when there is no overlay, and why ``None`` is the only spelling of "there is no
overlay": an ``""`` slipping through as a falsy sentinel would be that measurement, live.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Callable, NamedTuple

from .. import contain, tui
from . import layout, tmuxctl

#: The key that returns to the harness, from any state, at the tmux level.
#:
#: **A constant rather than a `[frame]` setting, deliberately, and this is the version to
#: revisit if an operator's terminal eats it.** The hatch is the safety property of the
#: whole command surface: a value an operator can set is a value an operator can set
#: wrong, and the failure mode of a mis-set hatch is exactly the state the hatch exists
#: for. `[frame] hotkey` is configurable because opening a palette has an alternative;
#: this does not.
#:
#: F12 rather than a letter or a modifier combination, because `bind -n` is the ROOT key
#: table: whatever this is, tmux takes it away from the harness pane too, everywhere,
#: for as long as the frame is up. A function key is the one class of key an agent
#: harness has no use for. Held to `instance._HOTKEY_RE` by a test for the reason that
#: constant exists — this string reaches tmux CONFIG TEXT.
HATCH_KEY = "F12"

#: The window option carrying the tmux commands the hatch runs — see the module
#: docstring for why the ids are here rather than in the bind.
#:
#: `@`-prefixed because that is tmux's own namespace for a user option; charter-prefixed
#: because the operator's own tmux may be the server charter is a guest on.
HATCH_OPTION = "@charter_hatch"

#: What the overlay writes to its own tty to ask for SGR mouse reporting: 1006 (SGR
#: encoding, so coordinates past column 223 survive) then 1000 (press/release only —
#: deliberately NOT 1002/1003, which add motion, because §4f closed the event kinds
#: without `drag`). Measured: tmux propagates this pane's exact request to the outer
#: terminal when tmux's own `mouse` is off, and coordinates arrive pane-relative and
#: 1-based, with any `pane-border-status` row already subtracted.
MOUSE_ON = "\x1b[?1006h\x1b[?1000h"

#: And the withdrawal, in the reverse order it was asked for. Written before
#: :data:`LEAVE`, so the pane hands its terminal back in the state it found it.
MOUSE_OFF = "\x1b[?1000l\x1b[?1006l"

#: The alternate screen, plus a hidden cursor. This is what "restores what was there"
#: means: the pane's own scrollback and last paint are untouched underneath, and come
#: back whole when the overlay leaves. Per-pane — measured, each tmux pane keeps its own
#: screen — so an overlay in charter's pane cannot disturb the harness's.
ENTER = "\x1b[?1049h\x1b[?25l"

#: The exit, and it runs in a ``finally``: an overlay that raised must not leave the
#: operator on an alternate screen with no cursor, which is a terminal that looks broken
#: and takes a `reset` to fix. `frame/panel.py`'s `_hold` is the same argument for a
#: panel that cannot paint.
LEAVE = "\x1b[?25h\x1b[?1049l"

#: What an overlay with nothing to offer says. **#512's shape**: "no repos" drawn on a
#: plane that had them is worse than a refusal, and an overlay drawing an empty box is
#: the same defect — indistinguishable from one whose rows failed to load.
EMPTY = "(nothing to choose)"

#: How many of the pane's rows the overlay spends on itself rather than on rows: the
#: header and the key hint. Named once because both :meth:`Surface.render` and the click
#: coordinate arithmetic need it, and two answers to "where does row 0 start" is an
#: off-by-one nobody sees until a click selects the wrong thing.
_HEADER_ROWS = 1
_FOOTER_ROWS = 1
_CHROME_ROWS = _HEADER_ROWS + _FOOTER_ROWS

#: The two cells between the title column and the note column.
_GAP = 2

#: The narrowest the title column is ever squeezed to. Below this a title is not
#: shortened, it is erased, and a row identified by nothing is worse than a row with no
#: note — so this is the floor the cap below stops at rather than a value it trades away.
_MIN_TITLE = 8

#: How tall the overlay's pane is split before it is zoomed. It is zoomed over the whole
#: window on the very next command, so this is only the size the pane holds for the
#: instant in between — small enough that tmux never refuses the split for want of room
#: in a short frame, which is the one way this number could cost anything.
_SPLIT_ROWS = 5

#: The event kinds :func:`decode` produces. A subset of `component.EVENT_KINDS` — §4f
#: closed that list at six, and `key` is the only one of these this SURFACE acts on
#: besides the pointer pair (:meth:`Surface.handle`).
#:
#: `focus`/`blur` joined the decoder with #607, and the sentence they replace said they
#: could not exist because `focus-events` ships off in tmux and gates the whole path.
#: Task 7 of the Phase 2 plan (#559) turned it on: `commands_frame.conf_text` writes
#: `set -g focus-events on` for every frame charter launches, so the bytes are real on
#: charter's own server. They are still absent inside an operator's existing tmux, where
#: charter sources no config at all — which is "never fires", the direction
#: `component.EVENT_KINDS` asks to degrade in, and not the `#{client_flags}` guard §4i
#: warns about, which reads `attached,focused` with the feature dead.
#:
#: **This surface acts on neither**, and that is not an oversight: the overlay runs in a
#: pane `open_argv` splits for it moments earlier, nothing in that pane writes
#: `\x1b[?1004h`, and tmux sends focus reports only to a pane whose program asked
#: (measured — see `frame/events.py`). `frame/events.py` is what these are decoded for,
#: one pane over. `Surface.handle` returning `None` for them is pinned, so this addition
#: cannot move a palette's selection.
#:
#: **What it would cost if one did arrive is a repaint, and that is stated rather than
#: waved away.** `Surface.run` sets `repaint = True` for every event its `handle` does not
#: turn into a verdict, so a focus report would redraw the pane where before these bytes
#: were consumed and produced nothing. A palette redraws on every keystroke already, so
#: the cost is one the surface pays constantly by design — but "cannot reach it" and
#: "would be free if it did" are two claims, and only the first was true.
KEY, CLICK, SCROLL, RESIZE = "key", "click", "scroll", "resize"
FOCUS, BLUR = "focus", "blur"

#: What :meth:`Surface.handle` answers with. Strings rather than an enum for the reason
#: the rest of this package uses strings: they appear in a test's failure message as
#: themselves.
CHOOSE, CANCEL = "choose", "cancel"

_R, _DIM, _BOLD, _REV = "\033[0m", "\033[2m", "\033[1m", "\033[7m"

#: The selected/unselected marker. **ASCII, and `frame/picker.py` records why**: `●`,
#: `◆` and the pointing triangles are East-Asian *Ambiguous*, which `statusline
#: ._persona_chips` measured breaking this exact layout twice. Both entries are the same
#: width by construction, so a selection moving does not move the text beside it.
_MARK = ("> ", "  ")

#: One SGR mouse report: `ESC [ < button ; col ; row (M|m)`, `M` a press and `m` a
#: release. Anchored at the start because :func:`decode` matches it against the head of
#: what is left, and bounded on the digits so a byte stream that is not a mouse report
#: cannot make this scan without end.
_SGR = re.compile(rb"^\x1b\[<(\d{1,5});(\d{1,5});(\d{1,5})([Mm])")

#: The button numbers this contract has names for, and nothing else gets one.
#:
#: **xterm spreads one button NUMBER across three bit positions**, and reading only the
#: low two is how a decoder invents a button it was never sent: the number is
#: ``(b & 3)``, plus **4** if bit 6 is set (which is what makes 64–67 the wheel), plus
#: **8** if bit 7 is set (buttons 8–11, the thumb buttons on an ordinary mouse).
#: :func:`decode` puts the three back together before it looks anything up here, so every
#: number this does not name — ``3``, the horizontal wheel at 6/7, the extra buttons at
#: 8–11, and anything past them — is dropped rather than folded onto a button that means
#: something else.
#:
#: **Measured rather than assumed to be unreachable**, tmux 3.7c and 3.2, injected into a
#: real client over a real pty, against a non-active pane that asked for 1000+1006::
#:
#:     right-click  over panel  ->  panel READ b'\\x1b[<2;20;5M\\x1b[<2;20;5m'
#:     middle-click over panel  ->  panel READ b'\\x1b[<1;20;5M\\x1b[<1;20;5m'
#:     shift+click  over panel  ->  panel READ b'\\x1b[<4;20;5M\\x1b[<4;20;5m'
#:     button 128   over panel  ->  panel READ b'\\x1b[<128;20;5M\\x1b[<128;20;5m'
#:     button 131   over panel  ->  panel READ b'\\x1b[<131;20;5M\\x1b[<131;20;5m'
#:
#: tmux forwards all of them verbatim. The last two are why bit 7 is read: taken as
#: ``b & 3`` alone, a thumb-button press is `left` and a component that acts on a left
#: click acts on it — `EVENT_KINDS`'s "fires wrongly", from the one place that can still
#: tell the difference. They are dropped instead, because §4f named no kind for them and
#: a button charter cannot name is a button charter should not report.
#:
#: The modifier bits (4 shift, 8 meta, 16 ctrl) are deliberately NOT named, which is
#: :data:`_CSI`'s rule for a modified arrow said here: the low bits name the gesture, the
#: high ones name the keyboard, and this contract has no use for the keyboard. So
#: `shift+click` above is a `left` click, which is what the operator who pressed it meant.
#:
#: ``3`` is absent and its absence is the guard: in the SGR encoding a release names the
#: button that was released and the trailing `m` is what makes it a release, so `3` is the
#: X10 encoding's "no button" in a report that should not be carrying it. Measured, an
#: actual X10 report (`ESC [ M` and three bytes) is forwarded to a 1006-requesting pane
#: **still in X10 form** rather than translated — so dropping ``3`` here costs no release
#: on any terminal, and an X10-only terminal degrades to nothing reaching a component at
#: all (see :func:`decode`).
_SGR_BUTTONS = {0: "left", 1: "middle", 2: "right"}

#: The wheel, in the same numbering: 4 and 5 once bit 6 has been folded in. 6 and 7 are
#: the horizontal wheel a trackpad swipe reports, and are absent for the reason they were
#: always dropped — this contract has one axis.
_SGR_WHEEL = {4: "up", 5: "down"}

#: The prefix of an SGR report that has not all arrived yet. Kept whole rather than
#: decoded as `ESC`, `[`, `<`, `0`, `;` — five keypresses out of half a mouse click.
_SGR_PARTIAL = re.compile(rb"^\x1b\[<[\d;]*$")

#: CSI sequences that are keys, by their final byte and (for the `~` family) their
#: number. tmux's own key table sends these forms; `\x1bO` is the application-cursor
#: variant, which nothing here requests but a terminal may still send after a program in
#: this pane switched modes and died without switching back.
_CSI_KEYS = {b"A": "up", b"B": "down", b"C": "right", b"D": "left",
             b"H": "home", b"F": "end"}
_TILDE_KEYS = {b"1": "home", b"4": "end", b"5": "pgup", b"6": "pgdn", b"7": "home",
               b"8": "end"}

#: The two CSI sequences that are not a key at all but this pane's own focus changing.
#: tmux's spelling, measured against real tmux 3.7c: selecting a pane whose program had
#: written `\x1b[?1004h` delivered `b'\x1b[I'`, selecting away delivered `b'\x1b[O'`, and
#: the client's own terminal losing and regaining focus delivered the same pair to the
#: active pane.
#:
#: **Matched only with NO parameters, and that is a guard rather than tidiness.** `CSI I`
#: is ECMA-48's CHT and `CSI O` is a private form; both take a numeric parameter in every
#: use that is not this one, so `\x1b[3I` is a cursor movement some program echoed and not
#: three focus events. Requiring the parameter list to be empty is
#: `component.EVENT_KINDS`'s "degrade to never fires, never to fires wrongly" applied at
#: the only place that can tell the two apart.
#:
#: Before #607 both fell through to :data:`_CSI_KEYS`, which has no name for either, and
#: were consumed and dropped — so this changes nothing about what the palette sees.
_CSI_FOCUS = {b"I": FOCUS, b"O": BLUR}

#: One WHOLE CSI: `ESC [`, its numeric parameters, and the final byte that ends it —
#: ECMA-48's own shape, `0x40`–`0x7e`.
#:
#: **Matched before any name is looked up, because finding the end is a separate question
#: from knowing the key.** A terminal sends far more sequences than the six names above:
#: every modified arrow (`\x1b[1;5A` is Ctrl-Up), every function key, and the brackets
#: around a paste (`\x1b[200~` … `\x1b[201~`). A decoder that recognises only what it
#: wants and lets the rest fall through to its single-byte path does not drop those — it
#: **replays them as the keys they are spelled with**, so Ctrl-Up types `1;5A` and F1
#: types `P`. That is the very thing :func:`decode`'s docstring already refuses to do to
#: a sequence that arrived half-way, and a whole one deserves it no less; Task 4's
#: palette filters on exactly these keys, which is where it would have cost something.
#:
#: The parameters are captured but only consulted for the `~` family: for the others the
#: FINAL BYTE names the key and the parameters name modifiers, and this surface has no
#: use for a modifier. So Ctrl-Up moves the selection up, which is what an operator who
#: pressed it meant.
#:
#: The parameter class carries `<` as well, which no key sequence uses — it is how an SGR
#: mouse report opens. :data:`_SGR` is tried first and answers every well-formed one, so
#: the only reports reaching here are the ones it REFUSED (more digits than its bound
#: admits, say). Those are still whole sequences, and this is what makes them consumed
#: whole instead of typed as `<99999999;1;1M`.
_CSI = re.compile(rb"^\x1b\[([\d;<]*)([\x40-\x7e])")

#: An unfinished CSI (or SS3) — an escape sequence still arriving. Held back by
#: :func:`decode` until the rest lands or the caller says nothing more is coming. The
#: parameter class is :data:`_CSI`'s plus the `<` an SGR mouse report opens with, so one
#: expression covers both families' prefixes.
_CSI_PARTIAL = re.compile(rb"^\x1b(\[[\d;<]*|O|)$")


def _title_width(shown: list[tuple[str, str]], width: int) -> int:
    """How wide the title column is in a pane *width* columns across.

    The titles' own width, **capped at half the row**. Sizing a column from its contents
    alone is what makes a two-column layout eat its second column in a narrow pane:
    measured at 34 columns, the widest title took 28 of them and every note came out as
    one glyph and an ellipsis. That is not a cosmetic loss — Task 4's rule is that an
    unavailable action is listed *with its reason*, and the reason is the note. A reason
    truncated to nothing still LOOKS like an answer, which is #512's shape one column
    over.

    Half rather than a fixed number of columns because both sides have to scale: a wide
    pane should spend its extra width on whichever column needs it, and only a
    proportional cap does that without a second rule for wide panes. `tui.width` on the
    marker rather than `len`, because :data:`_MARK`'s two entries are only the same width
    by construction and the day one of them stops being ASCII this must still be right.
    """
    longest = max([tui.width(t) for t, _ in shown] or [0])
    room = width - tui.width(_MARK[0]) - _GAP
    return min(longest, max(_MIN_TITLE, room // 2))


class Row(NamedTuple):
    """One line the overlay offers.

    *id* is what a caller matches on and never what is drawn; *title* and *note* are
    display text and are contained — not refused — before they are measured, the way
    `component.Component` splits the same pair. Task 3's actions and Task 6's pickers
    both become sequences of these, which is the whole of "one mechanism, three faces".
    """

    id: str
    title: str
    note: str = ""


class Event(NamedTuple):
    """One decoded input event.

    *name* is the sub-kind: which arrow or named key a ``key`` is, which way a ``scroll``
    went, and which button a ``click`` used (:data:`_SGR_BUTTONS`). Empty for the kinds
    that have only one form.

    *row* and *col* are **0-based**, converted here from the 1-based pair tmux hands over,
    and they are the coordinates of **the rectangle the receiver draws in** — which is the
    pane for :class:`Surface`, because a surface owns its whole pane, and the component's
    own canvas for a panel, because a component does not.

    **Two subtractions stand between a terminal's column and a component's, and exactly
    one of them is charter's.** Keeping them apart is the whole of this docstring:

    * **tmux's.** `pane_left`, and any `pane-border-status` row, are already gone before
      the bytes reach this process. Measured on 3.7c and 3.2, two panes split `-h -l 40`
      in a 120-column window, injected into a real client over a real pty::

          click window col 100  ->  panel READ b'\\x1b[<0;20;5M'    (100 - 80)
          click window col 81   ->  panel READ b'\\x1b[<0;1;5M'
          with pane-border-status top, window row 5  ->  arrives as row 4

      So **this module never does that arithmetic and must not start.**
    * **charter's.** The pad an operator asks for with `[frame] pad` is drawn by charter
      and tmux knows nothing about it — `slots.inset_rows` puts it in front of every row
      after the component has composed, and `slots.content_width` is the narrower canvas
      the component was told it had. tmux therefore reports a column in the PANE, and a
      component reasons in cells of its own rectangle. `events.Dispatcher` subtracts that
      one, once, on delivery, and drops what lands in the margin — see its `_on_canvas`.

    A decoder that did tmux's subtraction would double it; a dispatcher that skipped
    charter's would hand a padded component a column it never drew in. Both are the same
    error and only one of them looks like one.
    """

    kind: str
    name: str = ""
    row: int = 0
    col: int = 0
    pressed: bool = False


def decode(buf: bytes, *, final: bool = False) -> tuple[list[Event], bytes]:
    """*buf* as events, plus whatever is left of a sequence that has not all arrived.

    **The tail is the point.** A pane's `read` returns whatever the kernel had, which
    splits an escape sequence as readily as not: `\\x1b[` in one read and `B` in the next
    is an arrow key, and decoding the first half as an Escape keypress would cancel the
    overlay on a slow terminal — the one input this surface treats as "leave now".

    *final* is the caller saying nothing more is coming *right now* — a `read` that timed
    out, or a `SIGWINCH` that interrupted one. That is what resolves a lone `\\x1b` into
    an Escape keypress; there is no other way to tell it from the start of a sequence, and
    a quiet period is exactly how every terminal program tells them apart. An incomplete
    sequence that is not a lone `\\x1b` is dropped at that point rather than replayed as
    the keys it is spelled with.

    **And a sequence that arrived WHOLE gets the same treatment.** A terminal sends far
    more sequences than the six names this surface has, and one it has no name for is
    consumed to its end (:data:`_CSI`) rather than falling through to the single-byte
    path — otherwise Ctrl-Up types `1;5A`, F1 types `P`, and a paste types the brackets
    tmux wrapped it in. The rule is one rule; only where it is enforced was ever in
    question.

    **Two of those whole sequences are not keys, and #607 gave them their names.**
    ``\\x1b[I`` and ``\\x1b[O`` are this pane gaining and losing focus (:data:`_CSI_FOCUS`);
    they used to be consumed and dropped, which was right while nothing could receive
    them and wrong once `frame/events.py` could. Decoding is one question for every caller
    — WHICH kinds a given surface acts on is the caller's, and this surface acts on
    neither.

    **An SGR button number is not one number, and taking it for one is a mistake with
    four faces.** Bit 5 says the pointer MOVED and is answered first, on its own, because
    §4f closed the kinds without `drag`. What is left is a button NUMBER that xterm keeps
    in three places — the low two bits, plus 4 for bit 6, plus 8 for bit 7 — and
    :data:`_SGR_BUTTONS` and :data:`_SGR_WHEEL` name the six of those this contract has
    names for. Everything else is dropped. The modifier bits (4 shift, 8 meta, 16 ctrl)
    are not part of the number and go with the rest, exactly as :data:`_CSI`'s modifiers do.

    The four faces, all of them real reports a terminal sends: a shifted wheel scrolling
    the way the operator did not (`68`); a drag arriving as a click at every cell it
    crosses (`32`); a right-click indistinguishable from a left (`2`); and a thumb button
    arriving as a left click (`128`). #621's predecessor had fixed only the first.

    **An X10 report is not decoded here and reaches nobody, which is a limit rather than
    a bug.** A terminal too old to speak SGR sends `ESC [ M` and three bytes, and tmux
    forwards that form verbatim to a pane that asked for 1006 rather than translating it
    (measured). :data:`_CSI` consumes the `ESC [ M` and the three payload bytes fall to
    the single-byte path as stray keys — which `frame/events.py` never delivers, because
    `key` is not a kind charter carries. So on such a terminal the pointer degrades to
    "never fires", which is the direction `component.EVENT_KINDS` asks for.
    """
    evs: list[Event] = []
    while buf:
        m = _SGR.match(buf)
        if m:
            button, col, row_, kind = int(m[1]), int(m[2]), int(m[3]), m[4]
            buf = buf[m.end():]
            if button & 32:
                # **Motion, and it is dropped before anything else is asked.** §4f closed
                # the event kinds without `drag`, and bit 5 is the only thing telling a
                # pointer that MOVED from one that was pressed — folded into the number
                # below it would announce a drag as a click at every cell it crossed, and
                # `96` (wheel + motion) as a scroll nobody performed.
                #
                # Measured, tmux 3.7c and 3.2, `\x1b[<32;100;5M` and `\x1b[<96;100;5M`
                # injected into a real client with tmux's own `mouse` both off and ON (so
                # the outer terminal carried `1002h`): the pane that asked for 1000
                # received NOTHING, either version, either flag. tmux filters motion to
                # what the pane's own mode admits, so this arm is charter refusing to
                # depend on that filtering for its contract — the bit is in the protocol,
                # and what a component is handed should be decided by what the bit says
                # rather than by another program's tidiness.
                continue
            # **One button NUMBER, reassembled from the three places xterm keeps it**:
            # the low two bits, plus 4 for bit 6, plus 8 for bit 7. That is what makes
            # 64–67 the wheel and 128–131 the thumb buttons, and doing it here rather
            # than testing each bit in turn is what leaves no order to get wrong. The
            # modifier bits (4, 8, 16) are not part of it and are dropped with it.
            number = (button & 3) + (4 if button & 64 else 0) + (8 if button & 128 else 0)
            if number in _SGR_WHEEL:
                # Reported as a press with no release — the second reason this module
                # keeps no press state.
                evs.append(Event(SCROLL, _SGR_WHEEL[number], row=row_ - 1, col=col - 1))
            elif number in _SGR_BUTTONS:
                # A release with no press is a click. See the module docstring: it is
                # measured, not hypothetical.
                evs.append(Event(CLICK, _SGR_BUTTONS[number], row=row_ - 1, col=col - 1,
                                 pressed=(kind == b"M")))
            # and every other number — 3, the horizontal wheel, the extra buttons, and
            # whatever a terminal invents past them — is dropped, which is the whole point
            # of naming them in one place instead of testing bits.
            continue
        if not final and (_SGR_PARTIAL.match(buf) or _CSI_PARTIAL.match(buf)):
            return evs, buf
        if buf.startswith(b"\x1b["):
            m = _CSI.match(buf)
            if m is None:
                # `ESC [` with no final byte in sight and not a prefix `_CSI_PARTIAL`
                # would have held: a stream this module cannot find the end of. Drop the
                # introducer and let the rest take its chances rather than announcing an
                # Escape keypress, which is the one input that means "leave now".
                buf = buf[2:]
                continue
            params, final_byte, buf = m[1], m[2], buf[m.end():]
            if not params and final_byte in _CSI_FOCUS:
                # This pane gained or lost focus. Not a key, and never named as one: a
                # `focus` reaching `_CSI_KEYS` would be a keypress spelled `I`.
                evs.append(Event(_CSI_FOCUS[final_byte]))
                continue
            # For the `~` family the FIRST parameter names the key and the rest are
            # modifiers, exactly as the final byte names it for the others — so
            # `\x1b[5;5~` (Ctrl-PgUp) is PgUp, and one rule covers both families rather
            # than the arrows honouring a modifier and the page keys refusing one.
            name = (_TILDE_KEYS.get(params.split(b";", 1)[0]) if final_byte == b"~"
                    else _CSI_KEYS.get(final_byte))
            if name:
                evs.append(Event(KEY, name))
            continue
        if buf.startswith(b"\x1bO"):
            # SS3, the application-cursor form. Exactly one byte follows, and the same
            # rule applies to it: `\x1bOA` is Up, `\x1bOP` is F1 and is consumed whole
            # rather than typing a `P`.
            name = _CSI_KEYS.get(buf[2:3])
            buf = buf[3:]
            if name:
                evs.append(Event(KEY, name))
            continue
        ch, buf = buf[:1], buf[1:]
        if ch == b"\x1b":
            if not final and not buf:
                return evs, ch
            evs.append(Event(KEY, "escape"))
        elif ch in (b"\r", b"\n"):
            evs.append(Event(KEY, "enter"))
        elif ch in (b"\x7f", b"\x08"):
            evs.append(Event(KEY, "backspace"))
        elif ch == b"\x03":
            # Ctrl-C reads as "leave", not as a signal: the surface has the tty in raw
            # mode, so nothing else is going to turn this into one.
            evs.append(Event(KEY, "escape"))
        else:
            try:
                text = ch.decode()
            except UnicodeDecodeError:
                continue                       # a partial UTF-8 byte: not a keypress
            if text.isprintable():
                evs.append(Event(KEY, text))
    return evs, b""


@dataclass
class Surface:
    """The rows, the selection, and the loop that owns the pane until one is chosen.

    **Modal**: :meth:`run` consumes every event it is given and returns only when the
    operator chose a row or cancelled. Nothing falls through it — an unrecognised key is
    swallowed rather than ending the overlay, because a surface that exits on a stray
    keypress is a surface that loses the operator's place for a typo.

    Rendering is whole every paint, for `frame/panel.py`'s reason: a pane is a few
    hundred cells, so diffing would be optimising something already free, and there is no
    partial-line state to reconcile between paints.
    """

    rows: tuple[Row, ...] = ()

    #: What the header says this overlay is for. Contained before it is drawn: a picker's
    #: title is a workspace or persona name in Task 6, which is a committed value.
    heading: str = "charter"

    #: Whether this pane asks its terminal for pointer reports — and, by the same
    #: attribute, whether it acts on one. See the module docstring: one declaration, not
    #: two, so the request and the handling cannot disagree.
    mouse: bool = False

    _sel: int = field(default=0, init=False)
    _top: int = field(default=0, init=False)

    @property
    def selected(self) -> Row | None:
        """The row under the cursor, or ``None`` when there are no rows at all."""
        return self.rows[self._sel] if self.rows else None

    def move(self, delta: int) -> None:
        """Move the selection by *delta*, clamped to the ends. Never wraps: a list that
        wraps loses the operator's sense of where they are in it, and this one has no
        row cap to make wrapping worth it."""
        if self.rows:
            self._sel = max(0, min(len(self.rows) - 1, self._sel + delta))

    def _window(self, height: int) -> tuple[int, int]:
        """``(first row drawn, how many)`` for a pane *height* tall.

        **This is what "keeps the selection" means across a resize.** The selection is an
        index into `rows` and a resize cannot move it; what a resize *can* do is leave it
        off the bottom of a shorter pane, drawn nowhere, so the next keypress appears to
        come from nothing. Scrolling the window to contain it is the half that has to be
        recomputed every paint rather than only when the selection moves.
        """
        n = max(1, height - _CHROME_ROWS)
        top = self._top
        if self._sel < top:
            top = self._sel
        if self._sel >= top + n:
            top = self._sel - n + 1
        self._top = max(0, min(top, max(0, len(self.rows) - n)))
        return self._top, n

    def render(self, width: int, height: int) -> list[str]:
        """The whole pane, as lines, each already inside *width*.

        Every title and note goes through `contain.one_line` **before** `tui.width` sees
        it (#472): a committed value is what a picker's rows are made of, and a name
        carrying a newline that reached width arithmetic first would size the column from
        a string that is about to become two rows.
        """
        top, n = self._window(height)
        shown = [(contain.one_line(r.title), contain.one_line(r.note))
                 for r in self.rows[top:top + n]]
        title_w = _title_width(shown, width)
        out = [tui.truncate(f"{_BOLD}{contain.one_line(self.heading)}{_R}"
                            f"{_DIM} · {len(self.rows)} to choose from{_R}", width)]
        if not self.rows:
            out.append(tui.truncate(f"  {_DIM}{EMPTY}{_R}", width))
        for i, (title, note) in enumerate(shown, start=top):
            on = i == self._sel
            mark = _MARK[0] if on else _MARK[1]
            body = (f"{mark}{tui.pad(title, title_w)}{' ' * _GAP}"
                    f"{_DIM}{note}{_R}")
            out.append(tui.truncate(f"{_REV}{body}{_R}" if on else body, width))
        while len(out) < height - _FOOTER_ROWS:
            out.append("")
        # ASCII, for `_MARK`'s reason: the arrows and the return symbol an overlay wants
        # here (`↑↓`, `⏎`) are all East-Asian *Ambiguous*, which `statusline
        # ._persona_chips` records breaking a charter layout twice on a terminal that
        # draws them two cells wide.
        out.append(tui.truncate(
            f"{_DIM}  up/down move   enter choose   esc cancel   "
            f"{HATCH_KEY} back to the harness{_R}", width))
        return out[:max(1, height)]

    def handle(self, ev: Event, height: int) -> str | None:
        """One event. :data:`CHOOSE`, :data:`CANCEL`, or ``None`` for "carry on".

        A pointer event is acted on **only** when this surface asked its terminal for
        one, and a click only ever selects — see the module docstring for both.
        """
        if ev.kind == KEY:
            if ev.name == "enter":
                return CHOOSE if self.rows else None
            if ev.name == "escape":
                return CANCEL
            page = max(1, height - _CHROME_ROWS)
            self.move({"up": -1, "down": 1, "pgup": -page, "pgdn": page,
                       "home": -len(self.rows), "end": len(self.rows)}.get(ev.name, 0))
            return None
        if not self.mouse:
            return None
        if ev.kind == SCROLL:
            self.move(-1 if ev.name == "up" else 1)
        elif ev.kind == CLICK:
            i = self._top + ev.row - _HEADER_ROWS
            if 0 <= i < len(self.rows):
                self._sel = i
        return None

    def run(self, *, read: Callable[[], bytes | None],
            write: Callable[[str], None],
            size: Callable[[], tuple[int, int]]) -> Row | None:
        """Own the pane until a row is chosen or the operator leaves. Never a hang.

        *read* answers the next bytes, ``b""`` for "nothing arrived" — a poll that timed
        out, or a `read` a `SIGWINCH` interrupted, which is how a resize reaches this loop
        — and ``None`` for end of input. End of input is a **cancel**: it is what a closed
        stdin means, and it is the one answer that can never become a wedge (`picker.ask`
        makes the same call for the same reason).

        *size* is asked every iteration rather than once, because `window-resized` does
        not bump the frame's version and nothing else would redraw for it —
        `frame/panel.py`'s SIGWINCH section is the same fact one pane over.

        Both halves of :data:`ENTER`/:data:`LEAVE` are this function's, and the exit is a
        ``finally``: an overlay that raised must still hand the pane back.
        """
        write(ENTER + (MOUSE_ON if self.mouse else ""))
        try:
            w, h = size()
            self._paint(write, w, h)
            tail = b""
            while True:
                chunk = read()
                if chunk is None:
                    return None
                evs, tail = decode(tail + chunk, final=(chunk == b""))
                nw, nh = size()
                repaint = (nw, nh) != (w, h)
                w, h = nw, nh
                for ev in evs:
                    verdict = self.handle(ev, h)
                    if verdict == CHOOSE:
                        return self.selected
                    if verdict == CANCEL:
                        return None
                    repaint = True
                if repaint:
                    self._paint(write, w, h)
        finally:
            write((MOUSE_OFF if self.mouse else "") + LEAVE)

    def _paint(self, write: Callable[[str], None], width: int, height: int) -> None:
        """One whole pane, in ONE write: home the cursor, clear, draw.

        One call rather than a line at a time so a paint cannot be seen half-done, and so
        a test can read the pane's whole state out of a single string.
        """
        write("\x1b[H\x1b[2J" + "\r\n".join(self.render(width, height)))


# --------------------------------------------------------------------------- #
# The tmux half: the pane the overlay owns, and the key that always leaves it.
# --------------------------------------------------------------------------- #


def hatch_bind(key: str = HATCH_KEY) -> str:
    """The root-table bind, as one line of `commands_frame.conf_text`.

    `-n` is the root table, so tmux matches the key before any byte reaches the pane —
    which is the whole mechanism: a wedged overlay never gets a chance to swallow it.
    `run-shell -C` runs the expansion as tmux commands, with no shell, no `$PATH` and no
    second charter process; see the module docstring for what is deliberately *not* in
    here.
    """
    return f"bind -n {key} run-shell -C '#{{{HATCH_OPTION}}}'"


def hatch_command(*, harness: str, overlay_pane: str | None = None) -> str | None:
    """What the hatch runs, as tmux command text. ``None`` when charter will not build it.

    ``None`` for *overlay_pane* is "there is no overlay open", and it is the **only**
    spelling of that: an `""` is refused rather than read as absence, because a falsy
    sentinel resolving one way here and another way at the next call site is how two
    paths come to disagree. The stakes are specific — see the module docstring's last
    paragraph for the measurement that makes an empty kill target a killed panel.

    Both ids are held to `tmuxctl.PANE_ID_RE` because this string is stored in a tmux
    option that tmux later **re-parses as a command line**. That is `_PANE_ID_RE`'s exact
    call site one option over, and a value that fails it arms nothing at all rather than
    arming something charter cannot predict the parse of.
    """
    if not tmuxctl.PANE_ID_RE.fullmatch(harness):
        return None
    back = f"select-pane -t {harness}"
    if overlay_pane is None:
        return back
    if not tmuxctl.PANE_ID_RE.fullmatch(overlay_pane):
        return None
    return f"{back} ; kill-pane -t {overlay_pane}"


def arm_hatch_argv(server: str, *, harness: str,
                   overlay_pane: str | None = None) -> list[str] | None:
    """`set-option -w`: put :func:`hatch_command`'s text where the presser's own window
    will answer for it. ``None`` when there is nothing safe to arm.

    **Window-scoped, targeting the harness pane** — the idiom
    `commands_frame._panel_remain_on_exit_argv` already uses, and for the same reason:
    `-w -t <a pane id>` resolves to that pane's window, which is charter's own on either
    server. Never `-g`: a global write would hand frame N's harness pane to frame N-1,
    the "last launched wins" trap `conf_text`'s docstring names for `mouse` and
    `history-limit`.

    ``None`` rather than a raise, following `commands_frame._resize_hook_argv`: this is
    called from inside a launch, and a launch is not a place to propagate an exception
    from a value tmux itself produced.
    """
    cmd = hatch_command(harness=harness, overlay_pane=overlay_pane)
    if cmd is None:
        return None
    return tmuxctl.server_argv(server, "set-option", "-w", "-t", harness,
                               HATCH_OPTION, cmd)


def open_argv(server: str, *, harness: str, command: list[str],
              env: dict[str, str] | None = None) -> list[str] | None:
    """`split-window`: carve the overlay's own pane off *harness*, and report its id.

    Targeted at the harness pane's **id**, never a `session:0.0`-style index, for
    `frame/layout.py`'s measured reason: tmux renumbers pane indices on every split and
    ids it never reuses. `-P -F '#{pane_id}'` before `--`, so those are `split-window`'s
    own options and never reach *command*.

    The pane is split small and zoomed on the very next command (:func:`modal_argvs`), so
    :data:`_SPLIT_ROWS` is the size it holds for an instant rather than the size the
    operator sees. `-v`, so that `-l` is a count of ROWS: under `-h` the same number is
    columns, and a five-column overlay is a pane nothing can be drawn in.

    *env* rides on `-e` through `layout._env_argv`, the single funnel every `-e` charter
    builds goes through — #411's measurement is why the overlay needs one at all: every
    frame shares one tmux server, so a pane inherits the SERVER's environment, captured
    from whichever launcher started it, possibly days ago. A name outside
    `layout.CARRIABLE` **raises** there rather than returning ``None`` here, and that
    asymmetry is deliberate: a pane id charter read back off tmux is a value a launch
    must survive, while a `-e` name is something charter's own code chose, so it is a
    defect at the first call that builds it rather than a launch to degrade.
    """
    if not tmuxctl.PANE_ID_RE.fullmatch(harness):
        return None
    return tmuxctl.server_argv(server, "split-window", "-t", harness,
                               "-v", "-l", str(_SPLIT_ROWS),
                               *layout._env_argv(env),
                               "-P", "-F", "#{pane_id}", "--", *command)


def modal_argvs(server: str, *, harness: str,
                overlay_pane: str) -> list[list[str]]:
    """Make the new pane THE surface: hatch first, then focus, then the whole window.

    **The order is the property.** A surface that is selected before its escape hatch
    exists is a surface that can wedge with no way out — the same argument
    `commands_frame` already makes for installing the `pane-died` write hook before the
    teardown hook, one mechanism over. Arming first costs nothing: with the overlay pane
    not yet active, the hatch simply returns to a harness that already has focus.

    `resize-pane -Z` is what makes it full-pane and modal at once — measured, a zoomed
    pane occupies the entire window and its siblings are not drawn — and it is also what
    makes the overlay's own `\\x1b[?1006h\\x1b[?1000h` the request that reaches the
    terminal, since the outer terminal's mouse mode follows the ACTIVE pane (§4i).

    An empty list when either id is not tmux's own word for a pane: charter would rather
    open no overlay than open one it cannot promise a way out of.
    """
    arm = arm_hatch_argv(server, harness=harness, overlay_pane=overlay_pane)
    if arm is None:
        return []
    return [arm,
            tmuxctl.server_argv(server, "select-pane", "-t", overlay_pane),
            tmuxctl.server_argv(server, "resize-pane", "-Z", "-t", overlay_pane)]


def close_argvs(server: str, *, harness: str, overlay_pane: str) -> list[list[str]]:
    """Hand the pane back: focus the harness, kill the overlay, disarm the hatch.

    The same order the hatch itself runs in, and then one more command the hatch cannot
    run because by then there is no charter process left to run it: the option is
    re-armed with **no** overlay, so the next press names no pane at all rather than a
    dead one. (A dead one is harmless — measured silent — but "names a pane that is gone"
    is a state worth not being in.)
    """
    arm = arm_hatch_argv(server, harness=harness)
    if arm is None or not tmuxctl.PANE_ID_RE.fullmatch(overlay_pane):
        return []
    return [tmuxctl.server_argv(server, "select-pane", "-t", harness),
            tmuxctl.server_argv(server, "kill-pane", "-t", overlay_pane),
            arm]
