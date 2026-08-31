"""The overlay: charter's own modal surface, and the one key that always leaves it.

Spec §4k — **full-pane, charter-drawn, modal**. Not `display-popup` (measured: on tmux
3.2, a version `tmuxctl.below_floor_message` explicitly still launches on, any client
resize kills a popup with SIGHUP), not `display-menu` (charter's own nine-row cap and no
way to page).

Three properties get most of this file's length, because the measurements in
`docs/superpowers/specs/2026-08-26-tmux-input-findings.md` say each of them is where a
first implementation goes wrong:

* **A `click` release arrives with no matching press.** Measured: a drag beginning on a
  pane border and ending inside a pane delivers exactly one release. So the surface keeps
  no press state at all, and the tests below feed a lone release and assert it lands.
* **Mouse only when this pane asked for reporting.** The overlay is the one surface
  charter can promise pointer events on, because it is the active pane while it is open
  and its own request is what reaches the terminal. When it did not ask, a pointer event
  must do nothing rather than something arbitrary.
* **The escape hatch is a tmux key table entry, not charter code.** The unit half is
  here; the half that presses the key on a real client against a deliberately WEDGED
  overlay is `tests/test_frame_overlay_escape_hatch.py`, which needs a real tmux.
"""

from __future__ import annotations

import re
import unittest

from charter import commands_frame, config, tui
from charter.frame import layout, overlay, tmuxctl

#: The two DEC private modes the overlay turns on and has to turn back off, spelled out
#: here rather than reached for through `overlay.ENTER`/`overlay.LEAVE`. A constant
#: compared against itself agrees with itself: an `ENTER` that lost its `\x1b[?25l` and a
#: `LEAVE` that lost its `\x1b[?25h` both still satisfy "starts with ENTER, ends with
#: LEAVE", and the operator is left on their own screen with no cursor either way.
_ALT_ON, _ALT_OFF = "\x1b[?1049h", "\x1b[?1049l"
_CURSOR_OFF, _CURSOR_ON = "\x1b[?25l", "\x1b[?25h"


def _rows(n: int = 4) -> tuple[overlay.Row, ...]:
    """*n* rows whose ids, titles and notes are all distinguishable from each other."""
    return tuple(overlay.Row(id=f"act{i}", title=f"action {i}", note=f"note {i}")
                 for i in range(n))


def _numbered(n: int = 20) -> tuple[overlay.Row, ...]:
    """*n* rows whose titles are zero-padded, so no title is a prefix of another.

    :func:`_rows`' `action 1` **is** a prefix of `action 10`, which is harmless for a
    test asking whether one title reached the pane and wrong for one asking WHICH rows
    the window is showing. Every scrolling test below asserts both halves — the row that
    must be drawn and the row that must not be — and only distinguishable titles can
    carry the second half.
    """
    return tuple(overlay.Row(id=f"r{i}", title=f"row-{i:02d}", note=f"why-{i:02d}")
                 for i in range(n))


class _Tty:
    """A pane's tty, faked: a script of reads and everything that was written to it.

    `read` answers the next scripted chunk and then ``None`` — end of input — so a test
    that forgets to end its script cancels rather than spinning. ``b""`` is a TICK: what
    a real `read` returns when a `SIGWINCH` interrupted it, which is how a resize reaches
    the loop.
    """

    def __init__(self, script: list[bytes | None], size=(60, 12)) -> None:
        self.script = list(script)
        self.written: list[str] = []
        self.size = size
        self.reads = 0

    def read(self) -> bytes | None:
        self.reads += 1
        return self.script.pop(0) if self.script else None

    def write(self, s: str) -> None:
        self.written.append(s)

    def out(self) -> str:
        return "".join(self.written)


def _lines(tty: "_Tty") -> list[str]:
    """The pane as the operator sees it: the LAST paint, one string per drawn row.

    Split on the paint's own `\r\n` before `tui.strip_ansi` touches anything, because
    `strip_ansi` turns a newline into a space (`tui.sanitize` — that is its whole job for
    a committed value) and would erase the line structure this file is asserting about.
    """
    paint = tty.out().rsplit("\x1b[H\x1b[2J", 1)[-1]
    for tail in (overlay.MOUSE_OFF, overlay.LEAVE):
        paint = paint.split(tail)[0]
    return [tui.strip_ansi(ln) for ln in paint.split("\r\n")]


def _paint(tty: "_Tty") -> str:
    """The last whole-pane write, as the bytes the pane's terminal received.

    :func:`_lines` is the same paint *as the operator sees it* and runs `tui.strip_ansi`
    on the way — which turns a control character into something printable, and so cannot
    be asked whether one reached the pane at all. This is the unstripped half, for the
    tests that are about exactly that.
    """
    paint = tty.out().rsplit("\x1b[H\x1b[2J", 1)[-1]
    for tail in (overlay.MOUSE_OFF, overlay.LEAVE):
        paint = paint.split(tail)[0]
    return paint


def _modes(s: str) -> list[tuple[int, str]]:
    """The DEC private modes in *s*, in order, as ``(number, "h" set / "l" reset)``."""
    return [(int(n), sl) for n, sl in re.findall(r"\x1b\[\?(\d+)([hl])", s)]


def _run(rows, script, size=(60, 12), **kw):
    """Drive a surface to completion. Returns ``(chosen row, the tty)``."""
    tty = _Tty(script, size)
    surface = overlay.Surface(rows=rows, **kw)
    chosen = surface.run(read=tty.read, write=tty.write, size=lambda: tty.size)
    return chosen, tty


class TheOverlayDraws(unittest.TestCase):
    """Step 1's property: rows go onto a pane, a selection comes back, the pane is
    restored."""

    def test_every_row_reaches_the_pane(self):
        _, tty = _run(_rows(3), [b"\r"])
        drawn = tui.strip_ansi(tty.out())
        for r in _rows(3):
            self.assertIn(r.title, drawn, f"{r.title!r} was never drawn")
            self.assertIn(r.note, drawn, f"{r.note!r} was never drawn")

    def test_the_row_the_operator_moved_to_is_the_one_returned(self):
        chosen, _ = _run(_rows(4), [b"\x1b[B", b"\x1b[B", b"\r"])
        self.assertEqual(chosen, _rows(4)[2])

    def test_escape_returns_nothing_at_all(self):
        chosen, _ = _run(_rows(4), [b"\x1b", b""])
        self.assertIsNone(chosen)

    def test_end_of_input_is_a_cancel_and_never_a_hang(self):
        """A closed stdin is the one answer that can never be a wedge — `picker.ask`
        makes the same call for the same reason."""
        chosen, tty = _run(_rows(4), [])
        self.assertIsNone(chosen)
        self.assertEqual(tty.reads, 1)

    def test_what_was_on_the_pane_comes_back(self):
        """The alternate screen is the restore, and it is asserted on the BYTES.

        Asserting that some `restore()` helper was called would be a test one layer below
        the code that prints; what the pane's terminal actually receives is the property.
        """
        _, tty = _run(_rows(3), [b"\r"])
        out = tty.out()
        self.assertTrue(out.startswith(overlay.ENTER), repr(out[:12]))
        self.assertTrue(out.endswith(overlay.LEAVE), repr(out[-12:]))

    def test_the_pane_is_restored_even_when_the_paint_raises(self):
        """A pane whose tty has gone away must not leave the operator on the alternate
        screen with no cursor — `frame/panel.py`'s `_hold` is the same argument one pane
        over. `os.get_terminal_size` on a vanished fd is exactly this `OSError`."""
        tty = _Tty([b"\r"])

        def size():
            raise OSError("the pane's tty went away")

        with self.assertRaises(OSError):
            overlay.Surface(rows=_rows(2)).run(read=tty.read, write=tty.write, size=size)
        self.assertTrue(tty.out().endswith(overlay.LEAVE), repr(tty.out()))

    def test_an_overlay_with_no_rows_says_so_rather_than_drawing_an_empty_box(self):
        """#512's shape: a convincing empty is worse than a refusal."""
        chosen, tty = _run((), [b"\r", b"\x1b", b""])
        self.assertIsNone(chosen)
        self.assertIn(overlay.EMPTY, tui.strip_ansi(tty.out()))

    def test_enter_on_an_empty_overlay_chooses_nothing(self):
        """And it does not end the overlay either — there is nothing to have chosen."""
        tty = _Tty([b"\r", b"\r", b"\x1b", b""])
        surface = overlay.Surface(rows=())
        self.assertIsNone(surface.run(read=tty.read, write=tty.write,
                                      size=lambda: tty.size))
        self.assertEqual(tty.reads, 4)


class TheOverlayIsModal(unittest.TestCase):
    """It consumes what it is given; nothing falls through it to anyone else."""

    def test_an_unrecognised_key_is_swallowed_rather_than_ending_the_overlay(self):
        chosen, tty = _run(_rows(3), [b"zzz", b"\x1b[B", b"\r"])
        self.assertEqual(chosen, _rows(3)[1])

    def test_opening_makes_the_overlay_the_active_pane_and_covers_the_window(self):
        """Modality at the tmux level: the surface is zoomed over the whole window and
        selected, which is also what makes its own mouse request the one that reaches the
        terminal (§4i)."""
        cmds = overlay.modal_argvs("charter", harness="%0", overlay_pane="%7")
        flat = [" ".join(c) for c in cmds]
        self.assertTrue(any("select-pane" in c and "%7" in c for c in flat), flat)
        self.assertTrue(any("resize-pane" in c and "-Z" in c for c in flat), flat)

    def test_the_hatch_is_armed_before_the_surface_can_capture_anything(self):
        """Order, not merely presence. A surface selected before its escape hatch exists
        is a surface that can wedge with no way out — the same "install the write hook
        before the teardown hook" argument `commands_frame` already makes for `pane-died`.
        """
        cmds = overlay.modal_argvs("charter", harness="%0", overlay_pane="%7")
        # The tmux SUBCOMMAND, not a substring of the whole line: the armed value itself
        # contains the text `select-pane`, and matching on that would make this pass on
        # the arm command alone.
        names = [c[3] for c in cmds]
        self.assertLess(names.index("set-option"), names.index("select-pane"), names)


class TheInputContract(unittest.TestCase):
    """`decode` — bytes to the five event kinds §4f left open, and nothing else."""

    def test_the_arrow_keys(self):
        evs, tail = overlay.decode(b"\x1b[A\x1b[B")
        self.assertEqual([(e.kind, e.name) for e in evs],
                         [(overlay.KEY, "up"), (overlay.KEY, "down")])
        self.assertEqual(tail, b"")

    def test_a_partial_escape_sequence_is_kept_rather_than_misread(self):
        """Half of `\\x1b[B` arriving is not an Escape keypress. Reading it as one would
        cancel the overlay on a slow terminal."""
        evs, tail = overlay.decode(b"\x1b[")
        self.assertEqual(evs, [])
        self.assertEqual(tail, b"\x1b[")
        evs, tail = overlay.decode(tail + b"B")
        self.assertEqual([(e.kind, e.name) for e in evs], [(overlay.KEY, "down")])

    def test_a_lone_escape_resolves_only_when_nothing_more_is_coming(self):
        evs, tail = overlay.decode(b"\x1b")
        self.assertEqual(evs, [])
        evs, tail = overlay.decode(tail, final=True)
        self.assertEqual([(e.kind, e.name) for e in evs], [(overlay.KEY, "escape")])
        self.assertEqual(tail, b"")

    def test_a_sequence_is_never_replayed_as_the_keys_it_is_spelled_with(self):
        """The rule `decode`'s docstring already states for a sequence that arrived
        HALF-way, applied to one that arrived WHOLE.

        A terminal sends far more escape sequences than the six names this surface has —
        every modified arrow, every function key, and the brackets around a paste. Each
        one is a `\\x1b[`, some digits and semicolons, and a final byte, and a decoder
        that only knows the six it wants hands the other bytes to whoever is next: `Ctrl`
        with an arrow types `1;5A`, `F1` types `P`, and pasting a word into a filter
        types `200~` before it and `201~` after. **Task 4's palette filters on exactly
        the keys this produces**, which is where the cost lands.

        Every case here is a real terminal's real bytes, so the property is "the surface
        consumes the whole sequence and answers only for the ones it knows", not "these
        eight strings".
        """
        for raw, want in (
                # A modified arrow is still that arrow: the final byte names the key and
                # the parameters name modifiers this surface has no use for.
                (b"\x1b[1;5A", [(overlay.KEY, "up")]),      # Ctrl-Up
                (b"\x1b[1;2B", [(overlay.KEY, "down")]),    # Shift-Down
                (b"\x1b[1;5H", [(overlay.KEY, "home")]),    # Ctrl-Home
                (b"\x1b[5;5~", [(overlay.KEY, "pgup")]),    # Ctrl-PgUp — the `~` family
                (b"\x1b[6~", [(overlay.KEY, "pgdn")]),      #             and plain PgDn
                # And one this surface has no name for at all is consumed, not spelled.
                (b"\x1bOP", []),                            # F1
                (b"\x1b[Z", []),                            # Shift-Tab
                (b"\x1b[24~", []),                          # F12 — the HATCH key itself
                (b"\x1b[3~", []),                           # Delete
                # Bracketed paste: the pasted text arrives, its brackets do not.
                (b"\x1b[200~hi\x1b[201~", [(overlay.KEY, "h"), (overlay.KEY, "i")]),
        ):
            with self.subTest(raw=raw):
                evs, tail = overlay.decode(raw, final=True)
                self.assertEqual([(e.kind, e.name) for e in evs], want)
                self.assertEqual(tail, b"")

    def test_a_modified_arrow_still_arrives_split_across_two_reads(self):
        """The longer sequence must not lose the hold-it-back half: `\\x1b[1;5` is a
        prefix of Ctrl-Up and of nothing this surface would rather act on."""
        evs, tail = overlay.decode(b"\x1b[1;5")
        self.assertEqual(evs, [])
        self.assertEqual(tail, b"\x1b[1;5")
        evs, tail = overlay.decode(tail + b"A")
        self.assertEqual([(e.kind, e.name) for e in evs], [(overlay.KEY, "up")])
        self.assertEqual(tail, b"")

    def test_printable_bytes_arrive_as_themselves(self):
        evs, _ = overlay.decode(b"wq")
        self.assertEqual([(e.kind, e.name) for e in evs],
                         [(overlay.KEY, "w"), (overlay.KEY, "q")])

    def test_an_sgr_press_and_release_are_both_clicks(self):
        evs, tail = overlay.decode(b"\x1b[<0;5;3M\x1b[<0;5;3m")
        self.assertEqual([(e.kind, e.pressed, e.row) for e in evs],
                         [(overlay.CLICK, True, 2), (overlay.CLICK, False, 2)])
        self.assertEqual(tail, b"")

    def test_a_release_with_no_matching_press_is_still_a_click(self):
        """MEASURED, not supposed: with tmux's own `mouse` off, a drag that begins on a
        pane border and ends inside a pane delivers exactly one release
        (`b'\\x1b[<0;70;4m'`). §4i: the first component keeping press state wedges on it.
        """
        evs, tail = overlay.decode(b"\x1b[<0;70;4m")
        self.assertEqual([(e.kind, e.pressed) for e in evs], [(overlay.CLICK, False)])
        self.assertEqual(tail, b"")

    def test_the_wheel_is_a_scroll_and_not_a_click(self):
        evs, _ = overlay.decode(b"\x1b[<64;10;5M\x1b[<65;10;5M")
        self.assertEqual([(e.kind, e.name) for e in evs],
                         [(overlay.SCROLL, "up"), (overlay.SCROLL, "down")])

    def test_a_partial_mouse_report_is_kept_whole(self):
        evs, tail = overlay.decode(b"\x1b[<0;5;")
        self.assertEqual(evs, [])
        self.assertEqual(tail, b"\x1b[<0;5;")
        evs, _ = overlay.decode(tail + b"3M")
        self.assertEqual([e.kind for e in evs], [overlay.CLICK])

    def test_a_sequence_whose_end_never_arrives_is_not_an_escape_keypress(self):
        """`\\x1b[12` and then nothing is a CSI whose final byte will never come — the
        `final=True` path with a buffer `_CSI_PARTIAL` would have held a moment earlier.

        The one thing it must not become is `escape`, which is the single input this
        surface reads as "leave now": a terminal that stopped mid-sequence would then
        close the overlay the operator was typing into. So the introducer is dropped and
        what is left takes its chances as ordinary keys, which a modal surface swallows.
        """
        evs, tail = overlay.decode(b"\x1b[12", final=True)
        self.assertNotIn("escape", [e.name for e in evs], evs)
        self.assertEqual([(e.kind, e.name) for e in evs],
                         [(overlay.KEY, "1"), (overlay.KEY, "2")])
        self.assertEqual(tail, b"")

    def test_an_unfinished_sequence_does_not_cancel_the_overlay(self):
        """The same fact one layer up, which is the layer it costs something on: the
        half-sequence arrives, the read after it times out (`b""` — a tick), and the
        overlay is still there to take the next keypress."""
        chosen, _ = _run(_rows(3), [b"\x1b[12", b"", b"\x1b[B", b"\r"])
        self.assertEqual(chosen, _rows(3)[1])

    def test_ctrl_c_is_a_key_that_means_leave(self):
        """The surface has the tty in RAW mode, so nothing downstream is going to turn
        `\\x03` into a signal — there is no "and then the interrupt fires" to fall back
        on. Dropped by the printable filter instead, it is a modal surface that does not
        answer the first key an operator tries."""
        evs, _ = overlay.decode(b"\x03")
        self.assertEqual([(e.kind, e.name) for e in evs], [(overlay.KEY, "escape")])
        tty = _Tty([b"\x03"])
        surface = overlay.Surface(rows=_rows(3))
        self.assertIsNone(surface.run(read=tty.read, write=tty.write,
                                      size=lambda: tty.size))
        self.assertEqual(tty.reads, 1, "the overlay read on past a Ctrl-C")

    def test_a_control_byte_this_surface_has_no_name_for_is_not_a_keypress(self):
        """Everything `decode` answers for, it names by hand; the filter is what keeps
        the rest from arriving as keys. Without it a stray `\\x01`, or the `\\x00`s a
        closing pty can deliver, become keypresses that Task 4's palette types into its
        filter box — and every one of them repaints the pane."""
        evs, tail = overlay.decode(b"\x01\x02\x0b\x1f", final=True)
        self.assertEqual(evs, [])
        self.assertEqual(tail, b"")

    def test_half_a_utf_8_character_is_not_a_keypress(self):
        """`decode` walks BYTES and a multi-byte character arrives as several of them,
        so an undecodable byte is a *fragment*, not input. Replaced rather than dropped
        it becomes a U+FFFD keypress — a key the operator never pressed, out of half of
        one they did."""
        evs, _ = overlay.decode("é".encode()[:1], final=True)
        self.assertEqual(evs, [])

    def test_the_wheel_is_a_scroll_whatever_modifier_is_held(self):
        """An SGR button number is a BITFIELD, and both halves of that matter here.

        Bit 6 (64) is what makes a report a wheel; bits 2–4 are shift/meta/ctrl, so
        shift with the wheel is 68 and ctrl with it is 80. A test written against the
        bare `64`/`65` lets a modifier turn a scroll into a CLICK, and on this surface a
        click SELECTS the row under the pointer — so a shift-wheel would jump the
        selection to wherever the pointer happened to rest instead of moving the list.

        The low two bits are which wheel: 0 up, 1 down, and 2/3 the horizontal one a
        trackpad swipe reports. Direction therefore comes from the low bit and never
        from the whole number, and a horizontal swipe moves nothing at all — this list
        has one axis, and scrolling it sideways-means-down is worse than not answering.
        """
        for button, want in ((64, [(overlay.SCROLL, "up")]),
                             (65, [(overlay.SCROLL, "down")]),
                             (68, [(overlay.SCROLL, "up")]),     # shift + wheel up
                             (69, [(overlay.SCROLL, "down")]),   # shift + wheel down
                             (80, [(overlay.SCROLL, "up")]),     # ctrl + wheel up
                             (81, [(overlay.SCROLL, "down")]),   # ctrl + wheel down
                             (66, []),                           # wheel left
                             (67, [])):                          # wheel right
            with self.subTest(button=button):
                evs, tail = overlay.decode(f"\x1b[<{button};10;5M".encode())
                self.assertEqual([(e.kind, e.name) for e in evs], want)
                self.assertEqual(tail, b"")


class TheMouseIsConditional(unittest.TestCase):
    """§4c, as the measurement leaves it: pointer events belong to the overlay only
    because the overlay is the pane that asked for them."""

    def test_a_surface_that_asked_moves_its_selection_on_a_lone_release(self):
        chosen, tty = _run(_rows(6), [b"\x1b[<0;3;4m", b"\r"], mouse=True)
        self.assertIn(overlay.MOUSE_ON, tty.out())
        self.assertEqual(chosen, _rows(6)[2])

    def test_a_surface_that_did_not_ask_neither_requests_nor_acts(self):
        """Not two flags — one. The request written to the tty and the events acted on
        are the same declaration, so there is no state in which charter acts on a report
        it never asked the terminal for."""
        chosen, tty = _run(_rows(6), [b"\x1b[<0;3;4m", b"\r"], mouse=False)
        self.assertNotIn(overlay.MOUSE_ON, tty.out())
        self.assertEqual(chosen, _rows(6)[0])

    def test_the_request_is_withdrawn_before_the_pane_is_handed_back(self):
        _, tty = _run(_rows(3), [b"\r"], mouse=True)
        out = tty.out()
        self.assertIn(overlay.MOUSE_OFF, out)
        self.assertLess(out.index(overlay.MOUSE_ON), out.index(overlay.MOUSE_OFF))

    def test_a_click_never_activates_a_row(self):
        """A pointer event that can arrive unpaired must not drive the irreversible half.
        The release with no press is real, so a click SELECTS and `Enter` chooses."""
        tty = _Tty([b"\x1b[<0;3;4m", b"\x1b", b""])
        surface = overlay.Surface(rows=_rows(6), mouse=True)
        self.assertIsNone(surface.run(read=tty.read, write=tty.write,
                                      size=lambda: tty.size))

    def test_a_click_off_the_end_of_the_list_changes_nothing(self):
        chosen, _ = _run(_rows(3), [b"\x1b[<0;3;11m", b"\r"], mouse=True)
        self.assertEqual(chosen, _rows(3)[0])

    def test_the_wheel_moves_through_the_rows(self):
        chosen, _ = _run(_rows(6), [b"\x1b[<65;3;4M", b"\x1b[<65;3;4M", b"\r"],
                         mouse=True)
        self.assertEqual(chosen, _rows(6)[2])


class TheOverlaySurvivesAResize(unittest.TestCase):
    """`window-resized` does not bump the frame's version and nothing else redraws for
    it — `frame/panel.py`'s SIGWINCH section is the same fact one pane over."""

    def test_the_selected_row_is_still_selected_and_still_drawn(self):
        tty = _Tty([b"\x1b[B"] * 9 + [b"", b"\r"], size=(60, 14))
        surface = overlay.Surface(rows=_rows(20))

        def size():
            # The resize happens on the tick — a `read` interrupted by SIGWINCH.
            if tty.reads >= 10:
                return (40, 6)
            return (60, 14)

        chosen = surface.run(read=tty.read, write=tty.write, size=size)
        self.assertEqual(chosen, _rows(20)[9])
        # The pane is six rows now and the selection is the tenth: it is only still on
        # screen because the window scrolled to it.
        shown = _lines(tty)
        self.assertLessEqual(len(shown), 6, shown)
        self.assertTrue(any(_rows(20)[9].title in ln for ln in shown), shown)

    def test_a_pane_that_grew_fills_with_rows_and_not_with_blank_lines(self):
        """The resize the other way, and the clamp only this direction can reach.

        `_top` is wherever the last paint left the window, and a pane that GROWS can make
        it a top the list has too few rows to sit at: ten rows scrolled to `_top` 8 for a
        window two rows tall, and then a window ten rows tall. `_window`'s last line is
        what pulls the top back to 0 so the whole list fills the pane; without it the
        pane draws the last two rows and eight blank lines, and eight of the operator's
        ten choices are simply not on screen.

        **The test above only ever SHRINKS**, which is the one direction where that clamp
        cannot bind — a smaller window always has a top its list can sit at. So the
        clamp went unpinned, and deleting it left the whole suite green.
        """
        rows = _numbered(10)
        # Precondition, and not decoration: four rows really is too short for this list,
        # so the window really is scrolled away from row 0 before the pane grows. Without
        # this the assertion below would hold for a surface that never scrolled at all.
        _, small = _run(rows, [b"\x1b[F", b"\r"], size=(60, 4))
        self.assertNotIn(rows[0].title, "\n".join(_lines(small)))

        tty = _Tty([b"\x1b[F", b"", b"\r"], size=(60, 4))
        surface = overlay.Surface(rows=rows)

        def size():
            # The pane grows on the tick — a `read` a SIGWINCH interrupted — which is
            # how the test above delivers its shrink, in the other direction.
            return (60, 12) if tty.reads >= 2 else (60, 4)

        self.assertEqual(surface.run(read=tty.read, write=tty.write, size=size), rows[9])
        drawn = _lines(tty)
        for row in rows:
            self.assertIn(row.title, "\n".join(drawn), drawn)

    def test_a_narrow_pane_still_has_a_note_column(self):
        """Fitting inside the width is not the same as being readable at it.

        The title column is sized from the titles, so in a narrow pane a two-column
        layout eats the second column whole: measured at 34 columns, the widest title
        took 28 of them and every note came out as one glyph and an ellipsis. **Task 4's
        "an unavailable action appears WITH ITS REASON" is a note**, and a reason no pane
        narrower than a full-width laptop can show is that requirement unmet rather than
        met — #512's shape one column over, since a truncated reason still LOOKS like an
        answer.
        """
        rows = (overlay.Row(id="a", title="a fairly long action title here",
                            note="held"),
                overlay.Row(id="b", title="short", note="ready"))
        drawn = [tui.strip_ansi(ln) for ln in overlay.Surface(rows=rows).render(34, 8)]
        for note in ("held", "ready"):
            with self.subTest(note=note):
                self.assertTrue(any(note in ln for ln in drawn),
                                f"{note!r} never survived the title column: {drawn}")

    def test_a_narrower_pane_wraps_nothing(self):
        rows = (overlay.Row(id="a", title="a title far wider than this pane is",
                            note="and a note as well"),)
        tty = _Tty([b"\r"], size=(20, 8))
        overlay.Surface(rows=rows).run(read=tty.read, write=tty.write,
                                       size=lambda: tty.size)
        for line in _lines(tty):
            self.assertLessEqual(tui.width(line), 20, repr(line))


class TheWindowFollowsTheSelectionBothWays(unittest.TestCase):
    """`_window`'s two scroll branches are a mirror pair, and only one of them had a test.

    One moves the window DOWN when the selection goes off the bottom; the other moves it
    back UP when the selection goes off the top. The downward one was pinned by
    `TheOverlaySurvivesAResize`, which was also the only test in this file that scrolled
    at all — and it moves the selection down and shrinks the pane, so the upward branch
    was never asked for anything. Deleting it left the full suite green while `Home` drew
    the operator's selection **nowhere at all**, which is the outcome `_window`'s own
    docstring exists to rule out: "drawn nowhere, so the next keypress appears to come
    from nothing".
    """

    def test_home_scrolls_the_window_back_up_to_the_row_it_selected(self):
        rows = _numbered(20)
        # Precondition: `End` really does scroll the window off the top of the list. If
        # it did not, `Home` would have nothing to scroll back and everything below would
        # hold for a surface that never scrolls in either direction.
        _, down = _run(rows, [b"\x1b[F", b"\r"], size=(60, 8))
        self.assertIn(rows[19].title, "\n".join(_lines(down)))
        self.assertNotIn(rows[0].title, "\n".join(_lines(down)))

        chosen, up = _run(rows, [b"\x1b[F", b"\x1b[H", b"\r"], size=(60, 8))
        # The selection is an index and `move` clamps it, so it is on row 0 whether or
        # not the window followed — which is exactly why this cannot be the assertion.
        self.assertEqual(chosen, rows[0])
        drawn = _lines(up)
        self.assertTrue(any(rows[0].title in ln for ln in drawn),
                        f"the selected row is drawn nowhere: {drawn}")
        self.assertFalse(any(rows[19].title in ln for ln in drawn),
                         f"the window never left the bottom of the list: {drawn}")

    def test_arrowing_back_up_past_the_top_of_the_window_scrolls_too(self):
        """The same branch reached one row at a time rather than in one jump, because
        `home` is a `move` of the whole list length and a single `up` is not — a window
        that only recovered from the extreme would still lose the operator one row above
        the fold."""
        rows = _numbered(20)
        chosen, tty = _run(rows, [b"\x1b[F"] + [b"\x1b[A"] * 7 + [b"\r"], size=(60, 8))
        self.assertEqual(chosen, rows[12])
        drawn = _lines(tty)
        self.assertTrue(any(rows[12].title in ln for ln in drawn),
                        f"the selected row is drawn nowhere: {drawn}")


class RowsAreContainedBeforeTheyAreMeasured(unittest.TestCase):
    """#472's rule, and #498's reason for stating it as a property rather than a list."""

    def test_a_title_carrying_a_line_break_still_renders_as_one_row(self):
        rows = (overlay.Row(id="a", title="one\nline two", note="n"),
                overlay.Row(id="b", title="plain", note="n"))
        tty = _Tty([b"\r"], size=(60, 10))
        overlay.Surface(rows=rows).run(read=tty.read, write=tty.write,
                                       size=lambda: tty.size)
        body = "\n".join(_lines(tty))
        # Both of them: `\n` is the one everybody handles, and U+2028 is the one #498 was
        # filed about after `\n` already was. Each survives as its own escape — visible,
        # and unable to end a line.
        self.assertIn("\\x0a", body)
        self.assertIn("\\u2028", body)
        rendered = [ln for ln in _lines(tty) if "line" in ln or "plain" in ln]
        self.assertEqual(len(rendered), 2, rendered)

    def test_a_heading_is_contained_the_way_a_row_is(self):
        """The heading is a committed value too — a workspace or persona name in Task 6 —
        and the pane's entire layout is ONE write split on `\\r\\n`.

        U+2028 is the character #498 was filed about, and it is what tells the heading's
        own `contain.one_line` apart from the `tui.sanitize` every `truncate` already
        runs: sanitize folds a `\\n` into a space and leaves a LINE SEPARATOR alone, so
        without the containing step this one reaches the pane, is counted as one cell,
        and breaks the line on any terminal that honours it.
        """
        tty = _Tty([b"\r"], size=(60, 10))
        overlay.Surface(rows=_rows(2), heading="repo\u2028two").run(
            read=tty.read, write=tty.write, size=lambda: tty.size)
        self.assertNotIn("\u2028", _paint(tty), repr(_paint(tty)))
        self.assertIn("\\u2028", _lines(tty)[0], _lines(tty))

    def test_an_escape_sequence_in_a_title_never_reaches_the_pane(self):
        rows = (overlay.Row(id="a", title="\x1b[31mred\x1b[0m\x1b]0;title\x07", note=""),)
        tty = _Tty([b"\r"], size=(60, 10))
        overlay.Surface(rows=rows).run(read=tty.read, write=tty.write,
                                       size=lambda: tty.size)
        body = tty.out()
        self.assertNotIn("\x1b]0;", body)
        self.assertNotIn("\x07", body)


class TheSelectionNeverWraps(unittest.TestCase):
    """`move` clamps to the ends, and the clamp carries more than the docstring's own
    reason for it.

    `home` and `end` are *spelled* as a move the whole length of the list
    (`move(-len(rows))`, `move(+len(rows))`), so a `move` that wrapped instead of
    clamping would not merely lose the operator's sense of place — it would turn both
    keys into exact no-ops, because a move of `len(rows)` modulo `len(rows)` is zero.
    Home and End would stop working with nothing to see.
    """

    def test_home_and_end_reach_the_ends_of_the_list(self):
        chosen, _ = _run(_rows(10), [b"\x1b[B"] * 5 + [b"\x1b[H", b"\r"])
        self.assertEqual(chosen, _rows(10)[0])
        chosen, _ = _run(_rows(10), [b"\x1b[B"] * 5 + [b"\x1b[F", b"\r"])
        self.assertEqual(chosen, _rows(10)[9])

    def test_up_at_the_top_stays_at_the_top(self):
        chosen, _ = _run(_rows(10), [b"\x1b[A", b"\x1b[A", b"\r"])
        self.assertEqual(chosen, _rows(10)[0])

    def test_down_past_the_bottom_stays_at_the_bottom(self):
        chosen, _ = _run(_rows(10), [b"\x1b[B"] * 25 + [b"\r"])
        self.assertEqual(chosen, _rows(10)[9])


class ThePaneIsHandedBackTheWayItWasFound(unittest.TestCase):
    """Every mode the overlay turns on is turned back off before it lets go.

    On the LITERAL bytes: `test_what_was_on_the_pane_comes_back` above asserts against
    `overlay.ENTER`/`overlay.LEAVE`, which is the right assertion for "the alternate
    screen is the restore" and no assertion at all about what those constants contain —
    an `ENTER` that lost its `\\x1b[?25l` and a `LEAVE` that lost its `\\x1b[?25h` both
    still start and end the output. What `LEAVE`'s docstring is about is the operator's
    terminal afterwards: a hidden cursor on their own screen looks broken and takes a
    `reset` to fix.
    """

    def test_the_cursor_and_the_screen_both_come_back(self):
        _, tty = _run(_rows(3), [b"\r"])
        out = tty.out()
        for on, off in ((_ALT_ON, _ALT_OFF), (_CURSOR_OFF, _CURSOR_ON)):
            with self.subTest(mode=repr(on)):
                self.assertEqual(out.count(on), 1, repr(out))
                self.assertEqual(out.count(off), 1, repr(out))
                self.assertLess(out.index(on), out.index(off), repr(out))

    def test_they_come_back_even_when_the_overlay_raises(self):
        """`frame/panel.py`'s `_hold` argument, one pane over: the pane that could not
        finish is exactly the pane nobody is left to restore."""
        tty = _Tty([b"\r"])

        def size():
            raise OSError("the pane's tty went away")

        with self.assertRaises(OSError):
            overlay.Surface(rows=_rows(2)).run(read=tty.read, write=tty.write, size=size)
        self.assertIn(_CURSOR_ON, tty.out(), repr(tty.out()))
        self.assertIn(_ALT_OFF, tty.out(), repr(tty.out()))


class TheMouseRequestIsTheOneThisSurfaceCanAnswer(unittest.TestCase):
    """What the overlay asks its terminal for is bounded by what `decode` and `handle`
    can do with the answers."""

    def test_it_never_asks_for_motion(self):
        """1002 and 1003 add MOTION reports — one per cell the pointer crosses, on a
        pane that is zoomed over the whole window.

        This module keeps no press state by design (§4i: the first component that keeps
        it wedges on it) and §4f closed the event kinds without a `drag`, so a motion
        report is an event with nowhere to go: `decode` would emit a click for every
        one, and `handle` would move the selection under a pointer that is merely
        passing over. The request and the handling are one declaration here, and this is
        the half that says what the declaration may contain.
        """
        asked = _modes(overlay.MOUSE_ON)
        self.assertTrue(all(sl == "h" for _, sl in asked), asked)
        numbers = [n for n, _ in asked]
        self.assertNotIn(1002, numbers, "motion on button-press")
        self.assertNotIn(1003, numbers, "motion always")
        self.assertIn(1000, numbers, "press/release, the two kinds `decode` names")
        self.assertIn(1006, numbers, "SGR, so a column past 223 still arrives")

    def test_every_mode_asked_for_is_withdrawn_in_the_reverse_order(self):
        """The pane hands its terminal back in the state it found it, which is an
        ordering claim as well as a membership one: modes are a stack, and 1006 is the
        ENCODING the 1000 reports arrive in — dropped first, a report already in flight
        arrives in the other encoding."""
        asked = _modes(overlay.MOUSE_ON)
        dropped = _modes(overlay.MOUSE_OFF)
        self.assertTrue(all(sl == "l" for _, sl in dropped), dropped)
        self.assertEqual([n for n, _ in dropped],
                         list(reversed([n for n, _ in asked])))


class ThePaneChromeIsCountedOnce(unittest.TestCase):
    """`_CHROME_ROWS` is both what `render` spends on itself and what the click
    arithmetic subtracts back off — two answers to "where does row 0 start" is the
    off-by-one nobody sees until a click selects the wrong thing."""

    def test_the_key_hint_survives_at_every_height(self):
        """The footer is a row `render` has to LEAVE FOR, not one it appends and hopes
        fits: `out[:height]` is the last thing that happens, so a chrome count that
        forgot the footer fills the pane with rows and then drops the hint off the
        bottom — at every height where there are more rows than fit. The hint is where
        `esc` and the hatch key are written down, which makes it the one line an
        operator who is stuck needs.
        """
        for height in range(3, 13):
            with self.subTest(height=height):
                drawn = [tui.strip_ansi(ln) for ln in
                         overlay.Surface(rows=_rows(20)).render(60, height)]
                self.assertLessEqual(len(drawn), height, drawn)
                self.assertIn("esc cancel", drawn[-1], drawn)

    def test_a_pane_with_room_for_only_one_line_of_list_spends_it_on_a_ROW(self):
        """The floor under the window's height, at the only height where it binds.

        Chrome is two rows and a two-row pane has nothing left over, so the window's
        height is a **floor** rather than the subtraction: one row, and the key hint is
        what `out[:height]` drops. Take the floor away and the subtraction answers zero,
        the slice of rows comes back empty, and a pane with room for exactly one row
        draws the hint instead — an overlay offering nothing at all, which is the one
        state the operator cannot act from. (One row shorter still, the subtraction
        answers **-1**, and `_window` starts walking `_top` forward on every paint.)

        `test_the_key_hint_survives_at_every_height` starts at three, which is why this
        was never asked: two is the height where the two rows of chrome and one row of
        list stop both fitting, and something has to give. What gives is the hint, and
        that is the trade this pins — the hint is where `esc` is written down, but a
        list with no rows in it has nothing for `esc` to cancel out of.
        """
        rows = _numbered(20)
        drawn = [tui.strip_ansi(ln) for ln in overlay.Surface(rows=rows).render(60, 2)]
        self.assertEqual(len(drawn), 2, drawn)
        self.assertIn(rows[0].title, drawn[1],
                      f"a two-row pane drew no rows at all: {drawn}")

    def test_a_click_selects_the_row_that_was_drawn_where_it_landed(self):
        """Agreement between the two halves, asserted against `render`'s own output
        rather than against `_HEADER_ROWS` restated in the test: whatever row the
        operator can see on pane row N is the row a click on pane row N selects. Once
        with the list at the top and once with it scrolled, because the window offset is
        the other half of the same arithmetic.
        """
        rows = tuple(overlay.Row(id=f"r{i}", title=f"row-{i:02d}", note=f"why-{i:02d}")
                     for i in range(20))
        for start in (0, 15):
            with self.subTest(selection=start):
                surface = overlay.Surface(rows=rows, mouse=True)
                surface.move(start)
                drawn = [tui.strip_ansi(ln) for ln in surface.render(60, 10)]
                landed = 0
                for pane_row, line in enumerate(drawn):
                    here = [r for r in rows if r.title in line]
                    if not here:
                        continue
                    landed += 1
                    surface.handle(overlay.Event(overlay.CLICK, row=pane_row), 10)
                    self.assertEqual(surface.selected, here[0], (pane_row, line))
                self.assertGreaterEqual(landed, 4, drawn)

    def test_a_note_never_abuts_the_title_it_belongs_to(self):
        """Two columns are two columns because there is a gap between them. With none, a
        title and its note read as one string — `frame/statusline.py`'s chips are where
        charter measured what that costs a reader — and the row's own identity is the
        half that stops being findable."""
        line = tui.strip_ansi(overlay.Surface(
            rows=(overlay.Row(id="a", title="abc", note="xyz"),)).render(40, 5)[1])
        self.assertRegex(line, r"abc {2,}xyz")


class TheColumnsAreSizedInCells(unittest.TestCase):
    """`_title_width`, at the two widths where its two rules bind."""

    def test_a_narrow_pane_shortens_a_title_rather_than_erasing_it(self):
        """A floor can only be seen from below it. At 34 columns — the width
        `test_a_narrow_pane_still_has_a_note_column` renders at — the cap answers 15 and
        `_MIN_TITLE` is never consulted, so that test passes with the floor deleted. This
        renders at twelve, where the cap alone would leave four columns for the title and
        identify the row by nothing: a row that cannot be told from its neighbour is
        worse than a row with no note, which is the trade the floor refuses to make.
        """
        rows = (overlay.Row(id="a", title="workspace-alpha", note="ready"),)
        line = tui.strip_ansi(overlay.Surface(rows=rows).render(12, 6)[1])
        # One cell of the column goes to the ellipsis marking the cut.
        self.assertIn(rows[0].title[:overlay._MIN_TITLE - 1], line, line)

    def test_a_title_is_measured_in_cells_and_not_in_characters(self):
        """`_MARK`'s own comment makes this argument for the two-character marker; the
        titles are the values that actually vary. A CJK title is half as many characters
        as it is cells, so sizing the column with `len` gives it half the room it needs
        and truncates every such title — a workspace or persona name in Task 6, which is
        the name the operator is picking BY.
        """
        rows = (overlay.Row(id="a", title="日本語のタイトル", note="reason"),
                overlay.Row(id="b", title="short", note="ready"))
        drawn = [tui.strip_ansi(ln) for ln in overlay.Surface(rows=rows).render(60, 8)]
        self.assertTrue(any(rows[0].title in ln for ln in drawn), drawn)
        self.assertTrue(any("reason" in ln for ln in drawn), drawn)

    def test_moving_the_selection_never_moves_the_text_beside_it(self):
        """`_MARK`'s two entries are the same width by construction, and this is the
        property that construction is for: an operator holding `down` watches the text
        stand still and the highlight move, not every row shift a column each time."""
        surface = overlay.Surface(rows=_rows(4))
        before = [tui.strip_ansi(ln) for ln in surface.render(60, 8)]
        surface.move(1)
        after = [tui.strip_ansi(ln) for ln in surface.render(60, 8)]
        for a, b in zip(before[1:5], after[1:5]):
            with self.subTest(row=a):
                self.assertEqual(a.index("action"), b.index("action"), (a, b))


class TheEscapeHatch(unittest.TestCase):
    """§4e: one key, at the TMUX level, that returns to the harness from any state.

    The half that presses it on a real client against a deliberately wedged overlay is
    `tests/test_frame_overlay_escape_hatch.py`. What is here is the shape of the thing
    tmux is asked to install.
    """

    def test_the_key_is_bound_in_tmux_own_root_table(self):
        bind = overlay.hatch_bind()
        self.assertTrue(bind.startswith(f"bind -n {overlay.HATCH_KEY} "), bind)

    def test_the_hatch_runs_no_charter_code_at_all(self):
        """The whole point: it has to work while charter's own loop is stuck. A
        `run-shell` spawning charter would put a second charter process in the path out
        of the first one, and `-C` is what makes tmux run the commands itself."""
        bind = overlay.hatch_bind()
        self.assertIn("run-shell -C", bind)
        self.assertNotIn("CHARTER_PY", bind)
        self.assertNotIn("-m charter", bind)

    def test_the_bind_carries_no_frame_identity_of_its_own(self):
        """A key table is server-wide in tmux. `conf_text`'s docstring records what a
        bind holding one frame's own id costs the second frame launched: it resolves the
        WRONG frame. The pane ids live in a WINDOW option the presser's own window
        answers."""
        self.assertNotIn("%", overlay.hatch_bind())
        self.assertIn(overlay.HATCH_OPTION, overlay.hatch_bind())

    def test_returning_to_the_harness_comes_first(self):
        """Order is the unconditional half of the promise. If the kill fails — a stale
        pane id, an overlay already gone — focus has already moved."""
        cmd = overlay.hatch_command(harness="%0", overlay_pane="%7")
        self.assertLess(cmd.index("select-pane"), cmd.index("kill-pane"), cmd)

    def test_with_no_overlay_open_the_hatch_still_returns_to_the_harness(self):
        cmd = overlay.hatch_command(harness="%0")
        self.assertEqual(cmd, "select-pane -t %0")

    def test_no_overlay_never_becomes_an_empty_kill_target(self):
        """MEASURED against tmux 3.7c: `kill-pane -t ""` does not fail — it kills the
        CURRENT pane. A hatch that expanded an unset option into that target would take
        a panel out on every stray press."""
        self.assertNotIn("kill-pane", overlay.hatch_command(harness="%0"))

    def test_the_empty_string_is_not_a_spelling_of_no_overlay(self):
        """`None` is the only spelling of "the caller did not say". A falsy sentinel
        resolving one way here and another way elsewhere is Phase 1's own lesson."""
        self.assertIsNone(overlay.hatch_command(harness="%0", overlay_pane=""))

    def test_only_tmux_own_word_for_a_pane_reaches_the_command(self):
        """The value becomes tmux command text that tmux re-parses — `_PANE_ID_RE`'s
        exact call site, one option over."""
        for bad in ("%0; kill-server", "%0\nkill-server", "harness", "", "%١٢", "%１１",
                    "#{q:x}", "-t"):
            with self.subTest(bad=bad):
                self.assertIsNone(overlay.hatch_command(harness=bad))
                self.assertIsNone(overlay.hatch_command(harness="%0",
                                                        overlay_pane=bad))

    def test_a_refused_pane_id_arms_nothing_rather_than_arming_something_wrong(self):
        self.assertIsNone(overlay.arm_hatch_argv("charter", harness="nope"))
        argv = overlay.arm_hatch_argv("charter", harness="%0")
        self.assertEqual(argv[:2], ["tmux", "-L"])
        self.assertIn("set-option", argv)
        self.assertIn("-w", argv)
        self.assertIn(overlay.HATCH_OPTION, argv)

    def test_the_option_is_window_scoped_and_never_global(self):
        """`-g` here would hand frame N's harness pane to frame N-1 — the "last launched
        wins" trap `conf_text` names for `mouse` and `history-limit`."""
        argv = overlay.arm_hatch_argv("charter", harness="%0")
        self.assertNotIn("-g", argv)
        self.assertEqual(argv[argv.index("-t") + 1], "%0")

    def test_the_frame_config_installs_it(self):
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=100,
                                        session="s1")
        self.assertIn(overlay.hatch_bind(), text.split("\n"))

    def test_the_hatch_bind_is_the_last_line_of_the_config(self):
        """POSITION, and it is this branch's whole compatibility story below
        `tmuxctl.FLOOR`.

        `run-shell -C` first exists in tmux 3.2, which is the floor exactly, and
        `below_floor_message` still launches beneath it — so on those versions this one
        line does not parse where the rest of the file does. Last is what makes that
        cost only the hatch: `status`, `mouse`, `history-limit`, the hotkey bind and the
        wheel bind have all been applied by the time tmux reaches it, whatever it then
        does with the remainder. First, the same failure takes the frame's own settings
        with it.
        """
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=100,
                                        session="s1")
        lines = [ln for ln in text.split("\n") if ln]
        self.assertEqual(lines[-1], overlay.hatch_bind(), lines)

    def test_the_hatch_key_is_not_the_key_that_opens_things(self):
        """Two answers to one keypress is one answer that never fires."""
        self.assertNotEqual(overlay.HATCH_KEY, config.FRAME["hotkey"])

    def test_the_hatch_key_is_a_key_tmux_config_can_hold(self):
        """The same bound `[frame] hotkey` is held to — a newline in a key name once
        achieved code execution at launch (`instance._HOTKEY_RE`)."""
        from charter import instance
        self.assertTrue(instance._HOTKEY_RE.fullmatch(overlay.HATCH_KEY))


class TheOverlayPaneIsCharterOwn(unittest.TestCase):
    def test_it_is_split_off_the_harness_pane_and_reports_its_own_id(self):
        argv = overlay.open_argv("charter", harness="%0", command=["python3", "-m", "x"])
        self.assertIsNotNone(argv)
        self.assertIn("split-window", argv)
        self.assertEqual(argv[argv.index("-t") + 1], "%0")
        self.assertEqual(argv[argv.index("-F") + 1], "#{pane_id}")
        # Up to the command SEPARATOR, not to the end: the mark rides on this same
        # invocation as a second command (below), and everything after `--` and before it
        # is the pane's program.
        cmd = argv[argv.index("--") + 1:]
        self.assertEqual(cmd[:cmd.index(tmuxctl.SEPARATOR)], ["python3", "-m", "x"])

    def test_the_pane_is_marked_in_the_SAME_invocation_that_makes_it(self):
        """#739. A pane id is not knowable until `split-window` has answered with one, so
        a targeted mark is necessarily a second round trip — and the gap between the two
        is exactly the instant a second `F2` arrives in, finding an overlay pane that is
        open and not yet findable. One command list closes it: tmux runs the whole list
        server-side and the split makes the new pane current for the rest of it, so an
        untargeted `set-option -p` lands on the pane just made (measured on 3.7c and on
        the 3.2 floor).
        """
        argv = overlay.open_argv("charter", harness="%0", command=["true"])
        after = argv[argv.index(tmuxctl.SEPARATOR) + 1:]
        self.assertEqual(after, overlay.mark_argv("charter")[3:])
        self.assertNotIn("-t", after,
                         "a targeted mark cannot be chained onto the split that makes "
                         "the pane it would target")

    def test_only_a_marked_pane_whose_id_is_tmuxs_own_is_swept(self):
        """`live_panes` reads a listing back off tmux, and what the caller builds out of
        one is a `kill-pane`. Three rows, three reasons to drop or keep (#739/#442):

        * the harness formats as its id and nothing else — no mark, not swept;
        * an overlay formats as its id and the mark — swept;
        * a first field that is not tmux's own word for a pane never becomes a kill
          target, however marked it looks. That is not hypothetical tidiness: the value
          arrives off another process's stdout, and `close_argvs`' own docstring records
          what an unpredictable `-t` costs.
        """
        listing = "%0 \n%1 1\n%2 1\nnot-a-pane 1\n"
        self.assertEqual(overlay.live_panes(listing), ("%1", "%2"))

    def test_a_sweep_of_nothing_issues_nothing_at_all(self):
        """An empty target list reaching `kill-pane` is a `kill-pane` with no `-t`, which
        kills the CURRENT pane — the operator's harness. `None` is the whole answer, and
        it is `tmuxctl.chain`'s rather than a second guard in front of it."""
        self.assertIsNone(overlay.sweep_argv("charter", ()))
        self.assertIsNone(overlay.sweep_argv("charter", ("not-a-pane",)))
        self.assertEqual(overlay.sweep_argv("charter", ("%4",))[3:],
                         ["kill-pane", "-t", "%4"])

    def test_a_target_that_is_not_a_pane_id_opens_nothing(self):
        self.assertIsNone(overlay.open_argv("charter", harness="0", command=["true"]))

    def test_the_pane_is_split_off_in_rows_and_not_in_columns(self):
        """`-l` is a count of ROWS under `-v` and of COLUMNS under `-h`, and
        `_SPLIT_ROWS` is a row count. The same number under the other flag is a
        five-column pane — one nothing can be drawn in, and one the zoom that follows
        would inherit its aspect from."""
        argv = overlay.open_argv("charter", harness="%0", command=["true"])
        self.assertIn("-v", argv)
        self.assertNotIn("-h", argv)
        self.assertEqual(argv[argv.index("-l") + 1], str(overlay._SPLIT_ROWS))

    def test_the_split_fits_the_shortest_harness_the_frame_will_lay_out(self):
        """The one way this number can cost anything, in its own docstring's words: tmux
        refuses a split it has no room for, and a refused split is a launch with no
        overlay in it. `layout.HARNESS_MIN_ROWS` is the floor the frame's own layout
        leaves the harness pane, the overlay is split off THAT pane, and the split takes
        its own rows plus the border between them."""
        self.assertLess(overlay._SPLIT_ROWS + 1, layout.HARNESS_MIN_ROWS)

    def test_the_overlay_pane_carries_the_frame_identity_the_panels_carry(self):
        """#411's measurement is why a pane charter opens needs `-e` at all: every frame
        shares one tmux server, so a new pane's environment comes from the SERVER's —
        captured from whichever launcher happened to start it, possibly days ago. The
        overlay runs charter's own code and resolves the frame from
        `$CHARTER_SESSION_ID`, so a pane opened without this reads another frame's."""
        argv = overlay.open_argv("charter", harness="%0", command=["true"],
                                 env={"CHARTER_SESSION_ID": "s-1", "CHARTER_ROOT": "/r"})
        carried = [argv[i + 1] for i, x in enumerate(argv) if x == "-e"]
        self.assertEqual(sorted(carried), ["CHARTER_ROOT=/r", "CHARTER_SESSION_ID=s-1"])
        # tmux's own options, so before the `--`: after it they would be the program's
        # argv, which is `_env_argv`'s own rule and this call site's to keep.
        self.assertLess(max(i for i, x in enumerate(argv) if x == "-e"),
                        argv.index("--"))

    def test_a_name_the_frame_may_not_carry_is_refused_loudly_here_too(self):
        """`layout._env_argv` is the single funnel every `-e` charter builds goes
        through, and it RAISES — #446's lesson being that a rule enforced at each call
        site is a rule the next call site skips. Loud is the point: a launch that
        believed it handed the overlay a variable which never arrived is the failure a
        silent drop produces. Only the NAME is in the message; a value that does not
        belong on a command line does not belong in a traceback either."""
        with self.assertRaises(ValueError) as caught:
            overlay.open_argv("charter", harness="%0", command=["true"],
                              env={"AWS_SESSION_TOKEN": "s3cr3t-value"})
        self.assertIn("AWS_SESSION_TOKEN", str(caught.exception))
        self.assertNotIn("s3cr3t-value", str(caught.exception))

    def test_closing_hands_the_pane_back_and_disarms_the_hatch(self):
        cmds = [" ".join(c) for c in
                overlay.close_argvs("charter", harness="%0", overlay_pane="%7")]
        self.assertTrue(any("select-pane" in c and "%0" in c for c in cmds), cmds)
        self.assertTrue(any("kill-pane" in c and "%7" in c for c in cmds), cmds)
        self.assertTrue(cmds[-1].endswith("select-pane -t %0"), cmds)

    def test_closing_returns_to_the_harness_before_it_kills_anything(self):
        """The same order the hatch itself runs in, and the same reason: a kill that
        cannot happen — a stale id, an overlay already gone — costs nothing that has not
        already been delivered.

        Measured on tmux 3.7c with harness `%0`, a panel `%1` and a zoomed overlay `%2`:
        shipped, focus lands on the harness; with the kill first, tmux moves focus off
        the dying pane by itself and it lands on the PANEL, and the `select-pane` that
        follows is then correcting a jump the operator already saw.

        On the tmux SUBCOMMAND rather than a substring of the line, for the reason
        `test_the_hatch_is_armed_before_the_surface_can_capture_anything` gives: the
        armed value itself contains the text `select-pane`.
        """
        cmds = overlay.close_argvs("charter", harness="%0", overlay_pane="%7")
        names = [c[3] for c in cmds]
        self.assertLess(names.index("select-pane"), names.index("kill-pane"), names)

    def test_a_refused_overlay_id_kills_nothing_rather_than_naming_the_current_pane(self):
        """MEASURED on tmux 3.7c, and it is the measurement this module's docstring
        leads with: `kill-pane -t ""` does not fail and is not a no-op — it kills the
        pane the command is running against. Re-measured for THIS call site, which is a
        CLI invocation rather than the hatch's `run-shell`: `tmux -L … kill-pane -t ""`
        returned 0 and killed the session's active pane.

        `close_argvs` is the one call site that can produce one. `hatch_command` takes
        `None` for "there is no overlay" and every test above feeds it a well-formed
        `%7`; this function takes the id as a plain string, so an unset variable, a
        `#{pane_id}` that came back empty from a pane that had already died, or a
        capture that failed all arrive here spelled `""`.
        """
        for bad in ("", " ", "%", "%0; kill-server", "%0\nkill-server", "harness",
                    "%１１", "#{q:x}", "-t"):
            with self.subTest(overlay_pane=bad):
                self.assertEqual(
                    overlay.close_argvs("charter", harness="%0", overlay_pane=bad), [],
                    "a close that cannot name the overlay must emit no command at all")
        self.assertEqual(overlay.close_argvs("charter", harness="0", overlay_pane="%7"),
                         [], "and a harness it cannot name is no better")

    def test_a_refused_id_opens_no_modal_overlay_at_all(self):
        """"Charter would rather open no overlay than open one it cannot promise a way
        out of" — the ids that fail `PANE_ID_RE` are exactly the ids the hatch cannot be
        armed with, so selecting and zooming on them is the wedge the hatch exists for,
        entered deliberately."""
        for bad in ("", "%0; kill-server", "harness", "#{q:x}"):
            with self.subTest(bad=bad):
                self.assertEqual(
                    overlay.modal_argvs("charter", harness="%0", overlay_pane=bad), [])
                self.assertEqual(
                    overlay.modal_argvs("charter", harness=bad, overlay_pane="%7"), [])


if __name__ == "__main__":
    unittest.main()
