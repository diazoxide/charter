"""The palette: `F2` opens it, typing filters it, Enter runs what is left.

Spec §4h — **`F2` becomes the palette; the menu ceases to exist as a separate thing.**
Not a new key, and not a second surface beside `display-menu`: two answers to "how do I
do a thing" is how the single menu became weird in the first place.

Three properties get most of this file's length, because each is a rule the plan states
and a first implementation loses:

* **An unavailable action appears WITH ITS REASON, not hidden.** `$CHARTER_WORKSPACE`
  pins a frame and refuses a workspace switch today; a palette that silently omits the
  row is worse than one that explains, because the operator cannot ask about an option
  they cannot see (#512's shape, one surface along).
* **No row cap.** charter's nine-row limit was charter's, not tmux's — tmux 3.1c drew 20
  rows fine, and this surface scrolls.
* **Filtering is case-insensitive**, on what the operator can actually read.
"""

from __future__ import annotations

import os
import pty
import threading
import time
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, contain, tui
from charter.frame import action, actions, builtin_actions, overlay, palette, state, switch

from tests._isolation import PersonaIso


def _reg(*specs) -> actions.ActionRegistry:
    """A registry holding one action per ``(id, title, reason)`` — ``reason`` empty for
    an action that can run."""
    reg = actions.ActionRegistry()
    for aid, title, reason in specs:
        reg.register(action.Action(
            id=aid, title=title, run=lambda ctx: None,
            available=(lambda ctx, r=reason: not r),
            reason_unavailable=(lambda ctx, r=reason: r)))
    return reg


class _Tty:
    """A pane's tty, faked — `tests/test_frame_overlay.py::_Tty` in miniature."""

    def __init__(self, script, size=(60, 12)) -> None:
        self.script = list(script)
        self.written: list[str] = []
        self.size = size

    def read(self):
        return self.script.pop(0) if self.script else None

    def write(self, s: str) -> None:
        self.written.append(s)

    def drive(self, surface):
        return surface.run(read=self.read, write=self.write, size=lambda: self.size)

    @property
    def pane(self) -> str:
        return "".join(self.written)


class TheHotkeyOpensThePalette(unittest.TestCase):
    """`F2` and the palette are the same thing, and the menu is not beside it."""

    def test_the_hotkey_bind_opens_the_palette(self):
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=100,
                                        session="f-1")
        self.assertIn("frame-palette", text)

    def test_no_bind_anywhere_still_opens_a_menu(self):
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=100,
                                        session="f-1")
        self.assertNotIn("frame-menu", text)
        self.assertNotIn("display-menu", text)


class TypingFiltersAndEnterRuns(unittest.TestCase):
    """The headline: a palette listing actions, narrowed by typing, run by Enter."""

    def setUp(self) -> None:
        self.reg = _reg(("frame.detach", "detach the frame", ""),
                        ("density.rich", "density: rich", ""),
                        ("density.minimal", "density: minimal", ""))

    def _palette(self):
        return palette.Palette(
            catalogue=palette.rows(self.reg.offers(fid="f-1", snapshot={})))

    def test_the_palette_lists_every_action(self):
        p = self._palette()
        self.assertEqual([r.id for r in p.rows],
                         ["frame.detach", "density.rich", "density.minimal"])

    def test_typing_narrows_the_list_to_what_matches(self):
        tty = _Tty([b"detach"])
        p = self._palette()
        tty.drive(p)
        self.assertEqual([r.id for r in p.rows], ["frame.detach"])

    def test_enter_runs_the_action_the_filter_left(self):
        tty = _Tty([b"minimal\r"])
        chosen = tty.drive(self._palette())
        self.assertIsNotNone(chosen)
        self.assertEqual(chosen.id, "density.minimal")

    def test_what_the_operator_typed_is_on_the_screen(self):
        tty = _Tty([b"dens"])
        tty.drive(self._palette())
        self.assertIn("dens", tty.pane)


class AnUnavailableActionSaysWhy(unittest.TestCase):
    """Step 4: listed, never hidden, and the reason is what is drawn."""

    def test_it_is_listed_and_its_reason_is_the_row(self):
        reg = _reg(("workspace.w0", "workspace: ws01",
                    "cannot switch: $CHARTER_WORKSPACE pins this frame to 'ws00'"))
        rows = palette.rows(reg.offers(fid="f-1", snapshot={}))
        self.assertEqual([r.id for r in rows], ["workspace.w0"])
        self.assertIn("pins this frame", rows[0].note)


class ThereIsNoRowCap(unittest.TestCase):
    """Step 5: charter's nine-row limit was charter's, not tmux's."""

    def test_the_twentieth_action_is_offered_and_reachable(self):
        reg = _reg(*[(f"a.a{i:02d}", f"action {i:02d}", "") for i in range(20)])
        p = palette.Palette(catalogue=palette.rows(reg.offers(fid="f-1", snapshot={})))
        self.assertEqual(len(p.rows), 20)
        tty = _Tty([b"\x1b[F\r"])            # End, then Enter
        self.assertEqual(tty.drive(p).id, "a.a19")


class FilteringIsCaseInsensitive(unittest.TestCase):
    def test_lower_case_typing_finds_an_upper_case_title(self):
        reg = _reg(("frame.detach", "Detach The Frame", ""))
        p = palette.Palette(catalogue=palette.rows(reg.offers(fid="f-1", snapshot={})))
        tty = _Tty([b"detach"])
        tty.drive(p)
        self.assertEqual([r.id for r in p.rows], ["frame.detach"])


#: Every shape a committed value has ever used to break a charter surface, in one tuple.
#: A newline (`[frame] hotkey`'s own incident — a committed value reaching a tmux config
#: line achieved code execution at launch), U+2028 (which `str.splitlines` splits and
#: `str.split("\n")` does not — the difference that made #453), an escape sequence, both
#: quote styles, and `#`, which is what tmux's own format parser reads.
HOSTILE = (
    "two\nlines",
    "line\u2028separator",
    "esc\x1b[31mred",
    'quote" and \'quote',
    "#(touch /tmp/pwned)",
    "#{session_name}",
    "\r\ncarriage",
)


class HostileValuesRenderAsOneRow(unittest.TestCase):
    """Containment, run rather than reasoned about (#472).

    A row's title and note are committed values — a workspace name, a persona name, a
    provider's own string — and `overlay.Surface.render` contains each one BEFORE `tui
    .width` measures it. These assert the property on the palette's own path, with the
    query and the heading included, because those are the two strings this module adds.
    """

    def _lines(self, surface, width=60, height=12):
        return surface.render(width, height)

    def test_a_hostile_title_is_one_line_and_fits_the_pane(self):
        for raw in HOSTILE:
            with self.subTest(raw=raw):
                p = palette.Palette(catalogue=(overlay.Row(id="a.b", title=raw),))
                lines = self._lines(p)
                self.assertEqual(len(lines), 12, lines)
                for line in lines:
                    self.assertLessEqual(tui.width(tui.strip_ansi(line)), 60, line)

    def test_a_hostile_note_is_one_line_and_fits_the_pane(self):
        for raw in HOSTILE:
            with self.subTest(raw=raw):
                p = palette.Palette(
                    catalogue=(overlay.Row(id="a.b", title="t", note=raw),))
                lines = self._lines(p)
                self.assertEqual(len(lines), 12, lines)
                for line in lines:
                    self.assertLessEqual(tui.width(tui.strip_ansi(line)), 60, line)

    def test_no_hostile_byte_reaches_the_pane(self):
        """The bytes themselves, not merely the line count: an `\x1b[31m` drawn into a
        pane repaints the rest of the frame in someone else's colour."""
        for raw in HOSTILE:
            with self.subTest(raw=raw):
                p = palette.Palette(catalogue=(overlay.Row(id="a.b", title=raw, note=raw),))
                drawn = "\n".join(p.render(60, 12))
                for bad in ("\n\x1b[31m", "\u2028"):
                    self.assertNotIn(bad, drawn.replace("\n", " "), drawn)
                self.assertNotIn("\r", drawn, drawn)

    def test_a_hostile_query_is_contained_before_it_is_measured(self):
        """The query is built from `overlay.decode`'s printable single characters, so a
        newline cannot get into it by the front door — contained anyway, because the guard
        belongs at the join and not at whichever writer happens to exist today."""
        p = palette.Palette(catalogue=(overlay.Row(id="a.b", title="t"),),
                            query="two\nlines")
        p._refilter()
        lines = p.render(60, 12)
        self.assertEqual(len(lines), 12, lines)
        self.assertNotIn("\n", lines[0])

    def test_a_hostile_action_title_is_contained_by_the_contract_itself(self):
        """`Action.__post_init__` contains a title, so a provider's own string is already
        one line before `rows` ever sees it — asserted here so the palette's containment
        and the contract's cannot both be removed on the belief that the other has it."""
        for raw in HOSTILE:
            with self.subTest(raw=raw):
                a = action.Action(id="a.b", title=raw, run=lambda ctx: None)
                self.assertEqual(a.title, contain.one_line(raw))
                self.assertNotIn("\n", a.title)


class FilteringIsPinnedInBothDirections(unittest.TestCase):
    """Every mutation the plan names for this task, asserted from the other side."""

    def _p(self, *titles, query=""):
        cat = tuple(overlay.Row(id=f"a.a{i}", title=t) for i, t in enumerate(titles))
        return palette.Palette(catalogue=cat, query=query)

    def test_an_upper_case_query_finds_a_lower_case_title(self):
        """The other direction from `FilteringIsCaseInsensitive`: a fold applied to only
        one side of the comparison passes one of these two and fails the other."""
        p = self._p("detach the frame", query="DETACH")
        p._refilter()
        self.assertEqual(len(p.rows), 1, [r.title for r in p.rows])

    def test_a_query_matches_the_id_as_well_as_the_title(self):
        p = palette.Palette(catalogue=(overlay.Row(id="acme.deploy", title="ship it"),),
                            query="acme")
        p._refilter()
        self.assertEqual(len(p.rows), 1)

    def test_a_query_never_matches_the_reason(self):
        """The note is charter's own sentence about why a row cannot run. Matching it
        would make typing `lock` list every action that merely mentions one."""
        p = palette.Palette(
            catalogue=(overlay.Row(id="a.b", title="ship it",
                                   note="the session lock is held"),),
            query="lock")
        p._refilter()
        self.assertEqual(p.rows, ())

    def test_an_empty_query_shows_everything_rather_than_nothing(self):
        p = self._p("one", "two", "three")
        self.assertEqual(len(p.rows), 3)

    def test_the_order_is_never_disturbed_by_filtering(self):
        """No ranking: a palette that reordered itself as the operator typed would move the
        row under the cursor out from under it between keystrokes."""
        p = self._p("zzz match", "aaa match", "mmm match", query="match")
        p._refilter()
        self.assertEqual([r.title for r in p.rows],
                         ["zzz match", "aaa match", "mmm match"])

    def test_a_hundred_rows_are_a_hundred_rows(self):
        """The cap mutation, from the top: `rows` may not clip, and neither may `narrow`."""
        offers = [SimpleNamespace(id=f"a.a{i:03d}", title=f"row {i:03d}",
                                  available=True, reason="") for i in range(100)]
        self.assertEqual(len(palette.rows(offers)), 100)
        self.assertEqual(len(palette.narrow(palette.rows(offers), "row")), 100)


class TypingIsTheOnlyThingThatEditsTheQuery(unittest.TestCase):
    def _p(self):
        return palette.Palette(catalogue=(overlay.Row(id="a.b", title="alpha"),
                                          overlay.Row(id="a.c", title="beta")))

    def test_a_named_key_is_never_typed_into_the_query(self):
        """`decode` names every key it recognises with a word — `up`, `enter`, `pgdn` —
        and every one of them is longer than one character. A test of the SHAPE rather
        than of a list of names, so a key added to `decode` tomorrow cannot start typing
        itself into the query."""
        p = self._p()
        for name in ("up", "down", "pgup", "pgdn", "home", "end", "left", "right"):
            p.handle(overlay.Event(overlay.KEY, name), 12)
        self.assertEqual(p.query, "")
        self.assertEqual(len(p.rows), 2)

    def test_backspace_removes_exactly_one_character(self):
        p = self._p()
        for ch in "alp":
            p.handle(overlay.Event(overlay.KEY, ch), 12)
        p.handle(overlay.Event(overlay.KEY, "backspace"), 12)
        self.assertEqual(p.query, "al")

    def test_backspace_on_an_empty_query_is_not_a_cancel(self):
        """Escape is the one input that means "leave now" and it must be the only one: an
        operator who typed four characters and pressed backspace five times has not asked
        to close the palette."""
        p = self._p()
        for _ in range(3):
            self.assertIsNone(p.handle(overlay.Event(overlay.KEY, "backspace"), 12))
        self.assertEqual(p.query, "")

    def test_backspace_on_an_empty_query_does_not_move_the_selection_either(self):
        """`""[:-1]` is `""`, so the refusal above looks equivalent to deleting a character
        that is not there — and it is not: what follows an edit is `_refilter`, which puts
        the cursor back on the first row. Without this guard, backspace on an untouched
        palette walks the operator's selection to the top."""
        p = self._p()
        p.move(1)
        self.assertEqual(p.selected.title, "beta")
        p.handle(overlay.Event(overlay.KEY, "backspace"), 12)
        self.assertEqual(p.selected.title, "beta")

    def test_a_single_control_character_is_not_text_either(self):
        """`decode` never emits one — it drops a byte that is not printable — and this is
        checked again here for `_ACTION_ID_RE`'s own stated reason: the guard belongs at
        the join, not at whichever producer happens to exist today. Task 6's pickers feed
        this same `handle`."""
        p = self._p()
        for ch in ("\x07", "\x00", "\x1f"):
            with self.subTest(ch=ch):
                self.assertIsNone(p.handle(overlay.Event(overlay.KEY, ch), 12))
        self.assertEqual(p.query, "")

    def test_the_selection_returns_to_the_top_on_every_edit(self):
        """After a keystroke the operator is looking at a different list, and an index into
        the old one would leave the cursor on whichever row happened to land there."""
        p = palette.Palette(catalogue=tuple(
            overlay.Row(id=f"a.a{i}", title=f"row {i}") for i in range(6)))
        p.move(4)
        self.assertEqual(p.selected.title, "row 4")
        p.handle(overlay.Event(overlay.KEY, "r"), 12)
        self.assertEqual(p.selected.title, "row 0")

    def test_the_window_scrolls_back_with_it(self):
        """`_top` is reset too, so a filtered list cannot be drawn from a scroll position
        the new list does not have."""
        p = palette.Palette(catalogue=tuple(
            overlay.Row(id=f"a.a{i:02d}", title=f"row-{i:02d}") for i in range(40)))
        p.handle(overlay.Event(overlay.KEY, "end"), 12)
        p.render(60, 12)
        self.assertGreater(p._top, 0)
        p.handle(overlay.Event(overlay.KEY, "r"), 12)
        self.assertEqual(p._top, 0)

    def test_what_was_typed_is_on_the_header_and_leaves_when_it_is_deleted(self):
        p = self._p()
        p.handle(overlay.Event(overlay.KEY, "z"), 12)
        self.assertIn("z", p.render(60, 12)[0])
        p.handle(overlay.Event(overlay.KEY, "backspace"), 12)
        self.assertNotIn(palette.PROMPT.strip(), tui.strip_ansi(p.render(60, 12)[0]))

    def test_enter_still_chooses_and_escape_still_cancels(self):
        """The base class's contract survives being wrapped: everything this class does not
        claim must fall through to `overlay.Surface`."""
        p = self._p()
        self.assertEqual(p.handle(overlay.Event(overlay.KEY, "enter"), 12), overlay.CHOOSE)
        self.assertEqual(p.handle(overlay.Event(overlay.KEY, "escape"), 12), overlay.CANCEL)


class TheTtyIsOwnedAndHandedBack(unittest.TestCase):
    """`own_the_tty` against a REAL pty, because raw mode is not observable without one."""

    def setUp(self) -> None:
        self.master, self.slave = pty.openpty()
        self.addCleanup(self._close)

    def _close(self) -> None:
        """Close both ends — waking anything still blocked on the master FIRST.

        **Closing an fd does not wake a thread already blocked reading it.** Measured
        during this branch's own deletion sweep: with `_reader`'s `select` deleted, the
        tick test went red as it should and then the interpreter never exited, because a
        daemon thread sat in `os.read` on a pty whose other end was still open. A mutation
        that HANGS a sweep is worse than one that fails it — the next person sweeping this
        file gets no verdict at all, which is how the first attempt at this sweep produced
        thirty-seven worthless ones. One byte through the slave ends the read.
        """
        try:
            os.write(self.slave, b"\x00")
        except OSError:
            pass                              # a test that closed it already; nothing to wake
        for fd in (self.master, self.slave):
            try:
                os.close(fd)
            except OSError:
                pass

    def test_a_read_with_nothing_available_is_a_tick_and_never_a_block(self):
        """`b""` is what resolves a lone Escape (`overlay.decode`'s *final*) and what makes
        a resize repaint with no keystroke. Run on a worker thread with a deadline, because
        the failure mode of deleting the `select` is a read that never returns — which a
        plain call would turn into a hung suite rather than a red test."""
        read = palette._reader(self.master)
        answers = []
        t = threading.Thread(target=lambda: answers.append(read()), daemon=True)
        t.start()
        t.join(palette.TICK * 20)
        self.assertFalse(t.is_alive(), "the reader blocked instead of answering a tick")
        self.assertEqual(answers, [b""])

    def test_end_of_input_is_none_rather_than_an_endless_tick(self):
        """The other end is gone: answering `b""` for that would spin `Surface.run`
        forever on a palette nobody can close.

        **Asserted as the property, because the spelling is the platform's.** Closing the
        other end of a pty makes the next read return `b""` on macOS and raise
        ``OSError: [Errno 5]`` on Linux — measured on this exact test, which was green
        twice over locally and turned CI red on 3.11 and 3.14. So this asks what `read`
        ANSWERS rather than which branch answered, and the two branches are each reachable
        on one platform: a sweep run on either sees the other one survive, and CI is where
        the Linux half is proved.
        """
        read = palette._reader(self.master)
        os.close(self.slave)
        answers = []
        t = threading.Thread(target=lambda: answers.append(read()), daemon=True)
        t.start()
        t.join(palette.TICK * 20)
        self.assertFalse(t.is_alive())
        self.assertEqual(answers, [None])

    def test_what_arrived_is_what_is_read(self):
        os.write(self.slave, b"hi")
        self.assertEqual(palette._reader(self.master)(), b"hi")

    def test_the_tty_is_left_the_way_it_was_found_even_when_the_surface_raises(self):
        """`Surface.run`'s own argument one layer down: a palette that raised must not
        leave the operator in a terminal that takes a `reset` to fix."""
        import termios

        before = termios.tcgetattr(self.slave)
        surface = palette.Palette(catalogue=(overlay.Row(id="a.b", title="t"),))
        with mock.patch.object(surface, "run", side_effect=RuntimeError("boom")):
            with self.assertRaises(RuntimeError):
                palette.own_the_tty(surface, fd=self.slave,
                                    out=open(os.devnull, "w"))
        # `PENDIN` masked out of both: it is the kernel's own "input is waiting to be
        # retyped" flag, set as a side effect of the mode change and not part of what was
        # restored. Measured on macOS: the restored `c_lflag` came back as
        # `before | 0x20000000` on every run, which is `before` plus exactly that bit.
        after = termios.tcgetattr(self.slave)
        pendin = getattr(termios, "PENDIN", 0)
        after[3] &= ~pendin
        before[3] &= ~pendin
        self.assertEqual(after, before)

    def test_raw_mode_is_actually_entered(self):
        """Without it the line discipline holds every keystroke until Enter, echoes it
        where the palette is drawing, and turns Ctrl-C into a signal the surface never
        sees — `decode` reads `\x03` as "leave", which is only true once nothing else
        turns it into one first."""
        import termios

        seen = {}
        surface = palette.Palette(catalogue=(overlay.Row(id="a.b", title="t"),))
        with mock.patch.object(surface, "run",
                               side_effect=lambda **kw: seen.update(
                                   mode=termios.tcgetattr(self.slave))):
            palette.own_the_tty(surface, fd=self.slave, out=open(os.devnull, "w"))
        self.assertFalse(seen["mode"][3] & termios.ECHO, "the tty still echoes")
        self.assertFalse(seen["mode"][3] & termios.ICANON, "the tty is still canonical")


class TheActionsCharterOffersItself(PersonaIso, unittest.TestCase):
    """`builtin_actions.build` — everything the menu held, expressed as the public seam."""

    FID = "f-builtin"

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter")
        state.record_harness_pane(self.FID, "%3")
        state.record_identity(self.FID, {"CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})

    def _offers(self):
        reg = builtin_actions.build(self.FID, current_density="normal")
        return {o.id: o for o in reg.offers(fid=self.FID, snapshot={})}

    def test_charters_own_rows_are_offered_before_any_providers(self):
        ids = list(self._offers())
        self.assertEqual(ids[0], "frame.detach")
        self.assertTrue(any(i.startswith("density.") for i in ids), ids)

    def test_a_frame_with_no_recorded_harness_pane_says_why_density_cannot_move(self):
        """The exact condition `cmd_density` refuses on, surfaced as a reason instead of a
        keypress that does nothing."""
        state.record_harness_pane(self.FID, "not-a-pane-id")
        offer = self._offers()["density.normal"]
        self.assertFalse(offer.available)
        self.assertIn("no record of this frame's harness pane", offer.reason)

    def test_a_frame_with_a_real_harness_pane_can_move_its_density(self):
        """The other direction, so the reason above cannot pass by never being available."""
        self.assertTrue(self._offers()["density.normal"].available)

    def test_a_frame_whose_server_is_unrecorded_is_charters_own_and_detachable(self):
        """`state.frame_server` answers `None` for a frame launched by a charter that
        predates it. Reading that as "an operator's tmux" would tell somebody to press a
        prefix key charter never took."""
        state.record_server(self.FID, "")
        self.assertTrue(self._offers()["frame.detach"].available)

    def test_the_detach_row_starts_a_detached_client_command_on_the_frames_own_server(self):
        started = []
        reg = builtin_actions.build(self.FID, current_density="normal")
        with mock.patch.object(builtin_actions, "_spawn",
                               side_effect=lambda argv, *, fid: started.append(argv)):
            reg.get("frame.detach").run(SimpleNamespace(fid=self.FID))
        self.assertEqual(started,
                         [["tmux", "-L", "charter", "detach-client", "-s", self.FID]])

    def test_a_frame_with_no_recorded_server_still_detaches_on_charters_own_socket(self):
        """`_server`'s `or SOCKET` fallback, pinned where it has a consequence.

        **The row cannot pin it, and this branch's own deletion sweep is how that was
        found.** Delete the fallback and `_detachable` answers exactly the same, because
        `tmuxctl.is_operator_socket("")` is already False — one guard masking another, so
        the availability test stayed green over a real deletion. What actually differs is
        the argv the row STARTS: `tmux -L "" detach-client` names an empty server, which is
        not this frame's and may not be any running one.
        """
        state.record_server(self.FID, "")
        started = []
        reg = builtin_actions.build(self.FID, current_density="normal")
        with mock.patch.object(builtin_actions, "_spawn",
                               side_effect=lambda argv, *, fid: started.append(argv)):
            reg.get("frame.detach").run(SimpleNamespace(fid=self.FID))
        self.assertEqual(started,
                         [["tmux", "-L", "charter", "detach-client", "-s", self.FID]])

    def test_every_action_starts_its_work_in_a_session_of_its_own(self):
        """§4g plus the fact that the palette's pane is killed the instant it has invoked:
        `kill-pane` hands SIGHUP to that pane's process group, so a child inside it dies
        with it. Asserted on every built-in at once, because a new one added without this
        would fail silently and only on a real frame."""
        opened = []

        class _Popen:
            def __init__(self, argv, **kw):
                opened.append(kw)

        reg = builtin_actions.build(self.FID, current_density="normal")
        with mock.patch.object(builtin_actions.subprocess, "Popen", _Popen):
            for a in reg.all():
                a.run(SimpleNamespace(fid=self.FID))
        self.assertTrue(opened)
        for kw in opened:
            self.assertTrue(kw.get("start_new_session"),
                            "an action's work would die with the palette's own pane")
            self.assertEqual(kw.get("env", {}).get("CHARTER_SESSION_ID"), self.FID)


class ThePaletteCommand(PersonaIso, unittest.TestCase):
    """`_draw_palette` — the half that IS the pane, with the tty faked out."""

    FID = "f-draw"

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter")
        state.record_harness_pane(self.FID, "%3")
        state.record_identity(self.FID, {"CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        self.said = self.enterContext(mock.patch.object(commands_frame, "_say_on_screen"))
        self.closed = self.enterContext(
            mock.patch.object(commands_frame, "_close_palette"))

    def _draw(self, chosen):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True), \
             mock.patch.object(palette, "own_the_tty", return_value=chosen):
            return commands_frame.cmd_palette(
                SimpleNamespace(client="/dev/ttys7", pane=True))

    def test_cancelling_starts_nothing_and_says_nothing(self):
        with mock.patch.object(builtin_actions, "_spawn") as spawn:
            self.assertEqual(self._draw(None), 0)
        spawn.assert_not_called()
        self.said.assert_not_called()
        self.closed.assert_called_once()

    def test_choosing_a_row_starts_its_action(self):
        with mock.patch.object(builtin_actions, "_spawn") as spawn:
            self.assertEqual(self._draw(overlay.Row(id="frame.detach", title="d")), 0)
        spawn.assert_called_once()

    def test_a_refusal_at_the_moment_of_the_keypress_is_said_on_the_operators_screen(self):
        """`invoke` re-asks availability, so a row drawn while a plane was one way and
        pressed while it is another is refused — and that sentence has nowhere else to go,
        because the pane it would have been drawn in is the one about to be killed."""
        state.record_identity(self.FID, {"CHARTER_WORKSPACE": "pinned-ws"})
        with mock.patch.object(switch, "workspaces", return_value=["pinned-ws", "other"]):
            self.assertEqual(self._draw(overlay.Row(id="workspace.w1", title="w")), 0)
        self.said.assert_called_once()
        self.assertIn("$CHARTER_WORKSPACE pins this frame", self.said.call_args[0][1])
        self.assertEqual(self.said.call_args[0][2], "/dev/ttys7")

    def test_the_pane_is_not_killed_before_the_action_has_started(self):
        """§4g says `run` starts work and returns; `kill-pane` hands SIGHUP to this pane's
        process group, which takes the worker thread with it. So the close waits for the
        START — `Invocation.join`'s own "a shutdown that wants to know whether anything is
        still going" — and not for the work.

        Measured as an ORDER, not as a sleep: the action records when it was entered and
        `_close_palette` records when it ran, and the first must come first."""
        order = []

        def _slow(argv, *, fid):
            time.sleep(0.05)
            order.append("started")

        self.closed.side_effect = lambda *a, **kw: order.append("closed")
        with mock.patch.object(builtin_actions, "_spawn", side_effect=_slow):
            self._draw(overlay.Row(id="frame.detach", title="d"))
        self.assertEqual(order, ["started", "closed"])

    def test_the_pane_is_handed_back_even_when_the_surface_raises(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True), \
             mock.patch.object(palette, "own_the_tty", side_effect=RuntimeError("boom")):
            with self.assertRaises(RuntimeError):
                commands_frame.cmd_palette(
                    SimpleNamespace(client="", pane=True))
        self.closed.assert_called_once()

    def test_without_the_pane_flag_it_opens_one_instead_of_drawing(self):
        """One subcommand, two halves — and the flag is what tells them apart."""
        with mock.patch.object(commands_frame, "_draw_palette") as draw, \
             mock.patch.object(commands_frame, "_open_palette", return_value=0) as open_:
            commands_frame.cmd_palette(SimpleNamespace(client="", pane=False))
        draw.assert_not_called()
        open_.assert_called_once()


class WhatReachesTheOperatorsScreen(PersonaIso, unittest.TestCase):
    """`_say_on_screen` — the one line charter puts on a status area, and the parser it
    has to survive on the way."""

    FID = "f-say"

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter")

    def test_what_is_put_on_screen_is_inert_before_tmux_ever_parses_it(self):
        """`display-message`'s argument is a tmux FORMAT (its own docs say so), and what
        goes into it here is a workspace name or a provider's own refusal. The escaping is
        `tmuxctl.inert_format`'s and is asked for rather than re-spelled — a second copy is
        a second answer to "what may reach tmux's parser"."""
        with mock.patch.object(commands_frame.tmuxctl, "run") as run:
            commands_frame._say_on_screen(self.FID, "x #(touch /tmp/pwned) y")
        argv = run.call_args[0][1]
        self.assertNotIn("x #(touch /tmp/pwned) y", argv)
        self.assertIn("charter: x ##(touch /tmp/pwned) y", argv)

    def test_a_named_client_is_told_and_an_unnamed_one_is_the_session(self):
        """Measured against tmux 3.7c with two real ptys on one session: `-t <session>`
        drew on the most recently attached client regardless of who pressed, and `-c` drew
        on exactly the named one. The palette carries the presser's own client from the
        hotkey bind, so a refusal reaches the terminal that asked for it."""
        with mock.patch.object(commands_frame.tmuxctl, "run") as run:
            commands_frame._say_on_screen(self.FID, "hello", "/dev/ttys7")
        self.assertIn("-c", run.call_args[0][1])
        self.assertIn("/dev/ttys7", run.call_args[0][1])
        with mock.patch.object(commands_frame.tmuxctl, "run") as run:
            commands_frame._say_on_screen(self.FID, "hello")
        self.assertNotIn("-c", run.call_args[0][1])
        self.assertIn("-t", run.call_args[0][1])
        self.assertIn(self.FID, run.call_args[0][1])


if __name__ == "__main__":                          # pragma: no cover
    unittest.main()
