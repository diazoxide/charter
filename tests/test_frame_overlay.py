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

import unittest

from charter import commands_frame, config, tui
from charter.frame import overlay


def _rows(n: int = 4) -> tuple[overlay.Row, ...]:
    """*n* rows whose ids, titles and notes are all distinguishable from each other."""
    return tuple(overlay.Row(id=f"act{i}", title=f"action {i}", note=f"note {i}")
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

    def test_an_escape_sequence_in_a_title_never_reaches_the_pane(self):
        rows = (overlay.Row(id="a", title="\x1b[31mred\x1b[0m\x1b]0;title\x07", note=""),)
        tty = _Tty([b"\r"], size=(60, 10))
        overlay.Surface(rows=rows).run(read=tty.read, write=tty.write,
                                       size=lambda: tty.size)
        body = tty.out()
        self.assertNotIn("\x1b]0;", body)
        self.assertNotIn("\x07", body)


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
        self.assertEqual(argv[argv.index("--") + 1:], ["python3", "-m", "x"])

    def test_a_target_that_is_not_a_pane_id_opens_nothing(self):
        self.assertIsNone(overlay.open_argv("charter", harness="0", command=["true"]))

    def test_closing_hands_the_pane_back_and_disarms_the_hatch(self):
        cmds = [" ".join(c) for c in
                overlay.close_argvs("charter", harness="%0", overlay_pane="%7")]
        self.assertTrue(any("select-pane" in c and "%0" in c for c in cmds), cmds)
        self.assertTrue(any("kill-pane" in c and "%7" in c for c in cmds), cmds)
        self.assertTrue(cmds[-1].endswith("select-pane -t %0"), cmds)


if __name__ == "__main__":
    unittest.main()
