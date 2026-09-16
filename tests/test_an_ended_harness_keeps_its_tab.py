"""**A harness that ends keeps its tab, and closing the tab is the one way to end a chat.**

Today every harness exit is final: `pane-died[1]` is `kill-window`, so `/exit`, Ctrl-D or a
crash takes the chat's window and `state.reap` takes its directory. A chat is gone the moment
its harness stops, and the operator's own words for it were that charter had closed.

Task 2 of the one-exit-gate plan amends that (decisions 4, 5 and 12, and the ADR 0018
amendment this PR carries). What this module pins:

* **End of input is not Esc, and neither is Ctrl+C.** `overlay.Surface.run` answers ``None``
  for all three today, and an ended tab has to tell them apart: only a real Esc keystroke
  closes it for good, while a dropped terminal leaves it ended and open. So the surface
  records which way it left (:data:`overlay.LEFT_KEY` / :data:`overlay.LEFT_EOF`).
* **Ctrl+C decodes as its own key.** `decode` turned ``\\x03`` into `escape`, so a stray third
  Ctrl+C after a double-Ctrl+C `/exit` would close an ended tab for good. It is `ctrl-c` now,
  and every surface that cancelled on it keeps doing so through `Surface.cancel_keys` — the
  `F2` palette, its pickers, its confirmation drawers, the tab menu, and the never-started
  selector as #1103 left it. The ended selector and the crash drawer set `("escape",)`, so
  Ctrl+C does nothing there.

The ruling behind the split: *Ctrl+C is not Esc. Ctrl+C decodes as its own key. On an ended
tab's selector it does nothing; every other surface keeps its cancel behaviour.*
"""

from __future__ import annotations

import json
import subprocess
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, tui
from charter.frame import (actions, builtin_actions, chats, choose, ended, launcher,
                           leave, overlay, palette, reopen, selector, slots, state,
                           tabmenu)

from tests._isolation import (PersonaIso, approve_every_profile, declare_profiles,
                              make_plane, wired_as_today)


#: A server these chats record, standing in for a plane's own. A NAME no test starts a
#: server on: nothing in this module reaches tmux, and `tests._planeguard` refuses the
#: shape if one ever did.
SERVER = "charter-plane-5e77e45e77e4"


def _rows(n: int = 3) -> tuple[overlay.Row, ...]:
    return tuple(overlay.Row(id=f"r{i}", title=f"title {i}", note=f"note {i}")
                 for i in range(n))


def _plant(fid: str, *, ws: str = "beta", profile: str = "claude") -> None:
    """One chat on the plane, recorded the way a launch records it."""
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_server(fid, SERVER)
    state.record_harness_pane(fid, "%1")
    state.record_profile(fid, profile)
    state.record_identity(fid, {"CHARTER_HARNESS": "claude-code"})


class _Tty:
    """A pane's terminal, scripted: each read answers the next chunk, then end of input.

    ``None`` after the script is what `palette._reader` answers for a pane whose writer is
    gone, which is the state this module is about — so a script that runs out IS end of
    input rather than a hang.
    """

    def __init__(self, script) -> None:
        self.script = list(script)
        self.written: list[str] = []
        self.reads = 0

    def read(self) -> bytes | None:
        self.reads += 1
        return self.script.pop(0) if self.script else None

    def write(self, s: str) -> None:
        self.written.append(s)

    @property
    def size(self) -> tuple[int, int]:
        return (80, 24)


def _drive(surface, script):
    """Run *surface* over a scripted terminal. Answers ``(chosen, the tty)``."""
    tty = _Tty(script)
    chosen = surface.run(read=tty.read, write=tty.write, size=lambda: tty.size)
    return chosen, tty


class TheSurfaceTellsItsWaysOut(PersonaIso, unittest.TestCase):
    """`run` answers ``None`` three ways, and an ended tab acts on only one of them."""

    def test_the_surface_tells_escape_from_end_of_input(self):
        """The whole seam. Both are ``None`` to the caller and always have been; what is new
        is that the surface says which happened, because closing a chat for good and leaving
        it ended and open are not the same answer to a launcher that has to choose."""
        surface = overlay.Surface(rows=_rows())
        chosen, _tty = _drive(surface, [b"\x1b", b""])
        self.assertIsNone(chosen)
        self.assertEqual(surface.left, overlay.LEFT_KEY)

        surface = overlay.Surface(rows=_rows())
        chosen, tty = _drive(surface, [])
        self.assertIsNone(chosen)
        self.assertEqual(surface.left, overlay.LEFT_EOF)
        self.assertEqual(tty.reads, 1, "the surface read on past end of input")

    def test_ctrl_c_is_decoded_as_its_own_key(self):
        """``\\x03`` was `escape`, which is what made a stray Ctrl+C indistinguishable from
        the one keystroke that closes an ended tab for good."""
        evs, tail = overlay.decode(b"\x03", final=True)
        self.assertEqual([(e.kind, e.name) for e in evs],
                         [(overlay.KEY, overlay.CTRL_C)])
        self.assertEqual(tail, b"")

    def test_every_surface_that_cancelled_on_ctrl_c_still_does(self):
        """The compatibility half, and it is the half worth a case: making Ctrl+C its own key
        would otherwise silently take the cancel away from four live surfaces.

        Each row below is built the way its own call site builds it. The four palette routes
        really are one class — `commands_frame._draw_palette`, `_picker`, `_as_a_drawer` and
        `tabmenu.draw` all construct `palette.Palette` — so this asserts the shared default
        four times over, once per route, rather than pretending they are four classes.
        """
        plan = leave.plan(live=None, focus="")
        for what, surface in (
                ("the F2 palette", palette.Palette(catalogue=_rows(), mouse=True)),
                ("a confirmation drawer",
                 palette.Palette(catalogue=leave.confirm_rows(plan, verb=leave.QUIT),
                                 label=leave.QUIT, mouse=True)),
                ("a picker", palette.Palette(catalogue=_rows(), label="workspace",
                                             mouse=True)),
                ("the tab menu",
                 palette.Palette(catalogue=tabmenu.catalogue("beta.1"),
                                 label=tabmenu.label("beta.1"), mouse=True)),
                ("the never-started selector",
                 selector.Selector(catalogue=_rows(), footer=selector.FOOTER)),
        ):
            with self.subTest(surface=what):
                chosen, tty = _drive(surface, [b"\x03"])
                self.assertIsNone(chosen, f"{what} stopped cancelling on Ctrl+C")
                self.assertEqual(surface.left, overlay.LEFT_KEY)
                self.assertEqual(tty.reads, 1, f"{what} read on past a Ctrl+C")

    def test_a_surface_that_cancels_only_on_escape_ignores_ctrl_c(self):
        """What the ended selector and the crash drawer set, and the reason the key had to
        become its own: on an ended tab Ctrl+C must do NOTHING — no close, no mark, no
        manifest change — while Esc still closes the tab for good.

        Fed a Ctrl+C and then end of input, the surface leaves by END OF INPUT, which is the
        answer that changes nothing.
        """
        surface = overlay.Surface(rows=_rows(), cancel_keys=("escape",))

        chosen, tty = _drive(surface, [b"\x03"])

        self.assertIsNone(chosen)
        self.assertEqual(surface.left, overlay.LEFT_EOF,
                         "Ctrl+C cancelled a surface that only cancels on Esc")
        self.assertEqual(tty.reads, 2, "the Ctrl+C was not swallowed")

    def test_escape_still_cancels_a_surface_that_only_cancels_on_escape(self):
        """The negative control for the case above: narrowing `cancel_keys` must not take
        Esc away too, or an ended tab would have no way to close at all."""
        surface = overlay.Surface(rows=_rows(), cancel_keys=("escape",))

        chosen, _tty = _drive(surface, [b"\x1b", b""])

        self.assertIsNone(chosen)
        self.assertEqual(surface.left, overlay.LEFT_KEY)


class TheGuardsThatHadNoTest(PersonaIso, unittest.TestCase):
    """Three guards a hand deletion check found unpinned — each one red without its line.

    The sweep deletes one guard at a time and reports what survives; these three survived
    when this module was written, each for its own reason, and a guard no test can kill is
    a line the next reader is entitled to delete.
    """

    def setUp(self) -> None:
        super().setUp()
        _plant("beta.1")

    def test_an_exec_that_raises_claims_the_ended_state_back(self):
        """**`reset` answers whether it CLEARED a claim, and the undo needs that answer.**

        `launcher.attempt` hands the pane over at the `exec`; an `execvpe` that raises
        leaves the pane running nothing at all, so a tab that was ended is ended still and
        the claim has to go back. The undo learns that from `reset`'s return and from
        nowhere else — there is no second record of what the state was before the start.

        Red without it: `reset` answers `False`, the undo claims nothing, and a tab whose
        harness never restarted is left unclaimed — so its strip mark is gone, a close
        stops asking, and the next exit is presented as if it were the first.
        """
        state.claim_ended("beta.1")

        self.assertTrue(ended.reset("beta.1"),
                        "reset did not report that it cleared an ended claim")
        self.assertFalse(state.is_ended("beta.1"))

    def test_a_start_that_was_never_ended_reports_nothing_to_claim_back(self):
        """The other half, and the one that keeps the undo honest: a chat that was NOT
        ended must not come back claimed, or a pane nothing has ever run in would close
        without asking."""
        self.assertFalse(ended.reset("beta.1"))

    def test_a_start_that_raised_puts_the_ended_claim_back(self):
        """**`_start_linked`'s undo, which no existing case reaches.**

        `test_an_exec_that_raises_claims_the_ended_state_back` above pins `ended.reset`'s
        ANSWER; this pins the line that USES it. `attempt` hands the pane over at the
        `exec`, and an `execvpe` that raises leaves the pane running nothing at all — so a
        tab that was ended is ended still. Red without the line: the claim stays cleared,
        and a tab whose harness never restarted loses its strip mark, stops asking before it
        closes, and has its next exit presented as if it were the first.
        """
        state.claim_ended("beta.1")

        undo = launcher._start_linked("beta.1", chosen="", resumed=False,
                                      then=lambda: (lambda: None))
        self.assertFalse(state.is_ended("beta.1"), "the start did not clear the claim")
        undo()

        self.assertTrue(state.is_ended("beta.1"))

    def test_a_start_that_was_never_ended_is_not_claimed_by_its_undo(self):
        """The control that keeps the undo honest, and the reason `reset` answers a bool at
        all: a tab that was NOT ended must not come back claimed, or a pane nothing has ever
        run in would close without asking."""
        undo = launcher._start_linked("beta.1", chosen="", resumed=False,
                                      then=lambda: (lambda: None))
        undo()

        self.assertFalse(state.is_ended("beta.1"))

    def test_a_drawer_that_is_not_a_pane_id_is_never_WRITTEN(self):
        """**The two `PANE_ID_RE` checks were masking each other.** `record_drawer` holds
        the value on the way IN and `drawer` holds it again on the way OUT, so a test that
        only reads the value back stayed green with the inbound check deleted — the
        outbound one caught it. What that costs is not nothing: the bad value is then
        sitting in the chat's directory, and the next reader of that file is a `-t` target.

        So this asserts the FILE, which is the only thing the inbound guard decides.
        """
        state.record_drawer("beta.1", "%1;kill-server")

        on_disk = state.frame_dir("beta.1") / state._DRAWER_FILE
        self.assertFalse(on_disk.exists(),
                         f"a value that cannot name a pane was written to {on_disk}")

    def test_a_chat_that_was_never_drawn_is_not_presented(self):
        """**#384's early-death guard, which had no test of its own.**

        A harness dead BEFORE its chat was drawn is the launch's own early death: it is
        reported and its window closed. The ended step must do nothing for such a chat, and
        `state.was_drawn` is the only thing that tells the two apart — an exit code cannot,
        because a command that fails instantly and one that fails an hour later exit the
        same way. Without the gate the hook would present a tab for a chat whose window
        `_launch` is in the middle of killing.
        """
        state.record_exit("beta.1", 0)          # died, but the frame was never laid out
        fake = _Tmux([_row("%1", "1", "beta.1", commands_frame._this_plane())])

        with mock.patch.object(ended.tmuxctl, "run", fake):
            answer = ended.present("beta.1", socket=SERVER)

        self.assertEqual(answer, "")
        self.assertEqual(fake.calls, [],
                         "a chat that was never drawn reached tmux at all")
        self.assertFalse(state.is_ended("beta.1"))

    def test_reading_the_ended_mark_never_raises(self):
        """**`chats.roster` asks this per chat, so it is on the repaint path.**

        A chat id is not length-bounded on the way in (`$CHARTER_SESSION_ID` is an
        environment value), and a stat on a too-long name answers `ENAMETOOLONG` — on
        Linux. macOS returns False instead, which is why only CI caught it and why the
        guard has to be pinned by making the failure happen rather than by finding a name
        long enough: a test that depended on the platform's own limit would pass here and
        measure nothing.
        """
        real = Path.exists

        def too_long(self):
            if self.name == state._ENDED_FILE:
                raise OSError(63, "File name too long")
            return real(self)

        with mock.patch.object(Path, "exists", too_long):
            self.assertFalse(state.is_ended("beta.1"))


class TheEndedStateIsClaimedOnce(PersonaIso, unittest.TestCase):
    """`ended` is the claim that keeps ONE exit from being presented twice.

    Two things answer a harness that died while its launch was still laying the chat out:
    the `pane-died` hook, and `_launch`'s own late `_query_pane_dead_status` after it writes
    the drawn mark. Each of them would otherwise respawn the pane — two selectors for one
    exit, or a selector racing a drawer. `config.create_for` is `O_EXCL`, so the race has
    exactly one winner and the loser does nothing at all.
    """

    def setUp(self) -> None:
        super().setUp()
        _plant("beta.1")

    def test_only_the_first_caller_claims_it(self):
        self.assertTrue(state.claim_ended("beta.1"))
        self.assertFalse(state.claim_ended("beta.1"),
                         "a second caller claimed an exit that was already presented")
        self.assertTrue(state.is_ended("beta.1"))

    def test_a_chat_that_has_not_ended_is_not_ended(self):
        """The negative control: the claim below removes something that was not there."""
        self.assertFalse(state.is_ended("beta.1"))

    def test_clearing_it_makes_it_claimable_again(self):
        """Every harness START clears it (`ended.reset`), so the NEXT exit is presented
        exactly as the first was. Without this a resumed chat would end silently."""
        self.assertTrue(state.claim_ended("beta.1"))

        state.clear_ended("beta.1")

        self.assertFalse(state.is_ended("beta.1"))
        self.assertTrue(state.claim_ended("beta.1"),
                        "the second exit of a resumed chat was never presented")

    def test_the_claim_is_one_chats_and_not_the_planes(self):
        _plant("beta.2")
        state.claim_ended("beta.1")

        self.assertFalse(state.is_ended("beta.2"))
        self.assertTrue(state.claim_ended("beta.2"))

    def test_the_claim_is_the_file_called_ended(self):
        """**The NAME is the contract.** This claim is an `O_EXCL` file read by other
        charter processes — the hook's own child, the roster, the strip's mark — and across
        an upgrade, so the literal is what they agree on. Spelled by hand rather than
        through `state._ENDED_FILE`, which would hold for a rename of both halves at once.
        """
        state.claim_ended("beta.1")

        self.assertTrue((state.frame_dir("beta.1") / "ended").exists())

    def test_a_claim_that_cannot_be_removed_leaves_the_tab_ended(self):
        """**Every harness start calls this**, from `ended.reset` — a selector pick, the
        drawer's resume, a fresh start, a reopen and every ordinary launch.

        Narrowed to a type the filesystem does not raise, the `OSError` escapes `clear_ended`
        and takes `reset` with it, which is `launcher._start_linked` raising in the middle of
        a start: the exec never happens and the operator's pick does nothing. Leaving the
        claim behind is the cheap failure — the tab stays marked and the next start clears
        it — and breaking the start is not.
        """
        state.claim_ended("beta.1")
        real = Path.unlink

        def refuse(self, **kw):
            if self.name == state._ENDED_FILE:
                raise OSError(16, "Device or resource busy")
            return real(self, **kw)

        with mock.patch.object(Path, "unlink", refuse):
            state.clear_ended("beta.1")         # must not raise

        self.assertTrue(state.is_ended("beta.1"))

    def test_a_chat_with_no_directory_answers_rather_than_raising(self):
        """The `pane-died` hook runs this, and a hook never breaks a turn — so an id that
        can name no directory is an answer, not an exception."""
        self.assertFalse(state.claim_ended("../nope"))
        self.assertFalse(state.is_ended("../nope"))
        state.clear_ended("../nope")


class TheDrawnMarkAndTheDrawer(PersonaIso, unittest.TestCase):
    """What the ended step reads before it acts, and what it records when it splits.

    **The drawn mark is #384's guard kept.** A harness dead BEFORE its chat was drawn is the
    launch's own early death: it is reported and its window closed. The ended step must do
    nothing for such a chat, and the only thing that tells the two apart is whether the
    launch got as far as laying the frame out.
    """

    def setUp(self) -> None:
        super().setUp()
        _plant("beta.1")

    def test_a_chat_is_not_drawn_until_its_launch_says_so(self):
        self.assertFalse(state.was_drawn("beta.1"))

    def test_the_drawn_mark_is_written_and_read_back(self):
        state.record_drawn("beta.1")

        self.assertTrue(state.was_drawn("beta.1"))

    def test_a_drawer_pane_is_recorded_and_read_back(self):
        state.record_drawer("beta.1", "%7")

        self.assertEqual(state.drawer("beta.1"), "%7")

    def test_a_chat_with_no_drawer_has_none(self):
        self.assertIsNone(state.drawer("beta.1"))

    def test_a_drawer_that_is_not_a_pane_id_is_never_recorded(self):
        """#475's boundary, at the point it matters: this value comes off disk and becomes a
        `kill-pane -t` target. `%1;kill-server` in that file is the shape that already cost
        this project a `kill-server` armed on every window resize, and an empty target is
        worse than none — tmux 3.7c answers `kill-pane -t ''` by killing the ACTIVE pane.
        """
        for bad in ("%1;kill-server", "not-a-pane", "%1 %2", "%", "../x"):
            with self.subTest(pane=bad):
                state.record_drawer("beta.1", bad)
                self.assertIsNone(state.drawer("beta.1"),
                                  f"{bad!r} was recorded as a pane to kill")

    def test_a_recorded_drawer_can_be_forgotten(self):
        """What `ended.reset` and `ended.drop_drawer` leave behind once the pane is gone: a
        record naming a pane that no longer exists is a record aimed at whatever tmux hands
        that id to next."""
        state.record_drawer("beta.1", "%7")

        state.record_drawer("beta.1", "")

        self.assertIsNone(state.drawer("beta.1"))

    def test_the_drawn_mark_is_the_file_called_drawn(self):
        """**The NAME is the contract, so it is spelled by hand here.**

        This directory is read across processes and across a charter upgrade — a newer
        charter's hook child stats a file an older charter's launch wrote — so the literal
        is what the two halves agree on. Asserted as the path rather than through
        `state._DRAWN_FILE`, which would hold for any renaming of both halves at once and so
        would pin nothing about the name at all.
        """
        state.record_drawn("beta.1")

        self.assertTrue((state.frame_dir("beta.1") / "drawn").exists())

    def test_a_recorded_drawer_is_the_file_called_drawer(self):
        """:meth:`test_the_drawn_mark_is_the_file_called_drawn`'s contract, one file over —
        and this one's content is a `kill-pane -t` target, so the name is read by
        `drop_drawer` in a process that is about to kill something."""
        state.record_drawer("beta.1", "%7")

        self.assertEqual((state.frame_dir("beta.1") / "drawer").read_text().strip(), "%7")

    def test_a_drawn_mark_that_cannot_be_written_is_not_an_exception(self):
        """The documented safe degrade, and the direction #384 already describes: a mark
        that could not be written reads as *not drawn*, so the ended step offers nothing and
        the chat's window closes at its exit exactly as it did before this feature.

        Narrowed to a type the filesystem does not raise, the `OSError` escapes `record_drawn`
        — which `_launch` calls in the middle of laying a frame out, and which every turn of
        `_wait_out_the_ended_tab` calls again.
        """
        with mock.patch.object(state.config, "write_for",
                               side_effect=OSError(28, "No space left on device")):
            state.record_drawn("beta.1")        # must not raise

        self.assertFalse(state.was_drawn("beta.1"))

    def test_a_drawer_file_written_by_anything_else_is_not_a_target(self):
        """**The outbound half of the pair the sweep split.**
        `test_a_drawer_that_is_not_a_pane_id_is_never_WRITTEN` pins the inbound check, which
        decides what reaches the file; this pins what happens to a value that got there
        anyway. The file sits on disk between the two, and charter is not the only thing
        that can write into a directory — so the reader checks again before handing anybody
        a `-t` target.

        `%1;kill-server` is the exact shape that already cost this project a `kill-server`
        armed on every window resize (#475).
        """
        (state.frame_dir("beta.1") / state._DRAWER_FILE).write_text("%1;kill-server\n")

        self.assertIsNone(state.drawer("beta.1"),
                          "a planted value came back out as a pane to kill")

    def test_the_drawer_offers_resume_only_when_there_is_a_conversation(self):
        """Decision 4's third case at the drawer: *no conversation yet → only start fresh
        and close*. `launcher.resume_row` is what asks, at the moment of offering (#1101),
        and the row is ABSENT rather than refused — there is nothing to say about a
        conversation nobody started.
        """
        with mock.patch.object(launcher, "resume_row", return_value=None):
            self.assertEqual([r.id for r in ended.drawer_rows("beta.1")],
                             [ended.FRESH_ID, ended.CLOSE_ID])

        offered = selector.Resume(title="resume t2 · beta.1",
                                  note="claude-code · session 4f3c9ab1")
        with mock.patch.object(launcher, "resume_row", return_value=offered):
            rows = ended.drawer_rows("beta.1")

        self.assertEqual(rows[0].id, ended.RESUME_ID)
        self.assertEqual((rows[0].title, rows[0].note), (offered.title, offered.note),
                         "the drawer's resume row does not say which conversation")

    def test_a_drawer_that_cannot_be_written_is_not_an_exception(self):
        """The same safe degrade `record_drawn` makes one file over, and it is reached from
        the same places: `_open_drawer` records the pane tmux just reported, and
        `drop_drawer` records the empty answer after it kills one. Narrowed to a type the
        filesystem does not raise, the `OSError` escapes into the `pane-died` hook's child —
        which is `present`, whose whole contract is that a hook never breaks a turn.

        What a failed write costs is stated rather than guarded against: no record means
        `drop_drawer` will not look for that pane, which is the direction that kills nothing.
        """
        with mock.patch.object(state.config, "write_for",
                               side_effect=OSError(28, "No space left on device")):
            state.record_drawer("beta.1", "%7")     # must not raise

        self.assertIsNone(state.drawer("beta.1"))

    def test_none_of_it_raises_for_a_chat_with_no_directory(self):
        state.record_drawn("../nope")
        state.record_drawer("../nope", "%7")

        self.assertFalse(state.was_drawn("../nope"))
        self.assertIsNone(state.drawer("../nope"))


class TheSelectorAfterAnExit(PersonaIso, unittest.TestCase):
    """**A clean exit puts the profile selector back in the chat's own pane**, with one row
    the selector has never had: resume.

    Decision 4's words are *back to the profile selector, with resume &lt;session name&gt;
    preselected, then start fresh, then close tab*. The row is offered only when the
    conversation actually exists (`leave.conversation_exists`), because a resume row that
    starts an empty harness is an offer charter already knows it cannot honour.
    """

    RESUME = None                      # built in setUp, once the module is importable

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        declare_profiles(self)
        wired_as_today(self)
        approve_every_profile(self)
        self.have = selector.read(Path(config.ROOT))
        # **`read` is stubbed for the cases that drive `pick`, and it is not tidiness.**
        # `selector.read` runs `profiles.ignore_check`, which is a real `git status` child —
        # the one subprocess profile code makes. Called once per loop of `pick`, on a plane
        # this class rebuilds per case, it is both slow and the shape `CONTRIBUTING.md`
        # warns hangs rather than fails when it inherits a signing config. What these cases
        # are about is which row `pick` answers with, so the profiles are stated once.
        self.enterContext(mock.patch.object(selector, "read", lambda root: self.have))
        # **And `states` is stated, because detecting wiring RUNS each profile's command.**
        # `wired_as_today` deliberately leaves that seam alone (it patches
        # `wiring.wired_or_refusal`, one layer up), and the declared fixture profile is
        # `npx -y @openai/codex@0.140.0` — so a row built here would shell out to npm and
        # wait on the network. `None` per profile is `states`' own "nothing to say about
        # this row", which is the ordinary runnable row these cases are about.
        self.enterContext(mock.patch.object(
            selector, "states", lambda ps, *, cwd: {p.name: None for p in ps}))
        self.resume = selector.Resume(title="resume t2 · beta.1",
                                      note="claude-code · session 4f3c9ab1")

    def _listed(self, *, resume=None, start="claude-work"):
        return selector.rows(self.have, cwd=Path(config.ROOT), start=start, resume=resume)

    def _cancelled(self, left: str):
        """`own_the_tty` answering a cancel, having recorded which key did it."""
        def fake(surface, *_a, **_kw):
            surface.left = left
            return None
        return fake

    def test_resume_is_first_and_preselected(self):
        """First because it is what the operator almost always wants after `/exit`, and
        preselected because Enter must do the obvious thing on a surface that appeared by
        itself. `opens_on` is what the cursor is placed by, so both halves are asserted."""
        listed = self._listed(resume=self.resume)

        self.assertEqual(listed[0].id, selector.RESUME_ID)
        self.assertFalse(listed[0].refused)
        self.assertEqual(selector.opens_on(listed, "claude-work", resume=self.resume),
                         selector.RESUME_ID)

    def test_the_resume_row_names_the_session_it_would_bring_back(self):
        """A row that said only `resume` would be asking the operator to take charter's word
        for which conversation comes back. The title is the session name and the note is the
        harness and the first bytes of the link."""
        listed = self._listed(resume=self.resume)

        self.assertEqual(listed[0].title, "resume t2 · beta.1")
        self.assertEqual(listed[0].note, "claude-code · session 4f3c9ab1")

    def test_no_conversation_means_no_resume_row_at_all(self):
        """Decision 4's third case: *no conversation yet → only start fresh and close*. The
        row is absent rather than refused — there is nothing to say about a conversation
        that was never started, and a refused row would be charter explaining itself about
        a chat nobody has typed in."""
        listed = self._listed(resume=None)

        self.assertNotIn(selector.RESUME_ID, [r.id for r in listed])
        self.assertEqual(selector.opens_on(listed, "claude-work"), "claude-work")

    def test_the_footer_says_escape_closes_this_tab(self):
        """The ordinary selector's Esc closes a chat that never started; this one closes a
        chat that ran. The footer has to say which, because it is the same key."""
        self.assertIn("esc close this tab", selector.FOOTER_ENDED)

    def test_a_real_escape_and_end_of_input_come_back_as_different_answers(self):
        """**The seam this whole task turns on.** Both are `own_the_tty` answering `None`;
        one is the operator closing the tab for good and the other is their terminal going
        away. `pick` answers two sentinels rather than one `None`, so the launcher cannot
        treat a dropped pty as a decision."""
        for left, want in ((overlay.LEFT_KEY, selector.KEY_CANCEL),
                           (overlay.LEFT_EOF, selector.END_OF_INPUT)):
            with self.subTest(left=left):
                with mock.patch.object(selector.palette, "own_the_tty",
                                       side_effect=self._cancelled(left)):
                    answer = selector.pick(cwd=Path(config.ROOT), root=Path(config.ROOT),
                                           start="claude-work", ended=True)
                self.assertIs(answer, want)

    def test_choosing_resume_asks_for_the_conversation_back(self):
        """The row comes back as a `Choice` that says `resume`, never as a profile name the
        caller would have to parse out of a title.

        **The stub answers ONCE and refuses a second time, and that is a guard about CI
        rather than tidiness.** `pick` loops on a row it could not start — the note goes in
        the footer and the list comes back (ruling 6) — so a stub that answered the same row
        forever turns any deletion of the `RESUME_ID` branch into a live spin inside `pick`
        rather than a failed assertion: the resume row falls through to
        `name = chosen.id.removeprefix(ROW_PREFIX)`, which is `resume:` and is in no
        `have.profiles`, so `after` becomes a `Refused` and the loop goes round again on a
        surface that answers identically.

        Measured, not feared: that mutation ran a sweep shard for its full 240 s cap and was
        killed, which is why the sweep filed the line *unresolved* rather than pinned — a
        mutation that HANGS is not a red test, and it costs a whole shard's answer. It is
        the second hang of this shape on this branch; `TheEndedLoopEndsOnItsOwn` writes up
        the first and bounds it the same way, with a runaway guard far above the real bound.
        """
        asked = {"n": 0}

        def chose(surface, *_a, **_kw):
            asked["n"] += 1
            if asked["n"] > 1:
                raise AssertionError(
                    "pick drew the selector a second time instead of answering: the resume "
                    "row was read as a profile name and refused, and the loop came round")
            return next(r for r in surface.rows if r.id == selector.RESUME_ID)

        with mock.patch.object(selector.palette, "own_the_tty", side_effect=chose):
            answer = selector.pick(cwd=Path(config.ROOT), root=Path(config.ROOT),
                                   start="claude-work", resume=self.resume, ended=True)

        self.assertIsInstance(answer, selector.Choice)
        self.assertTrue(answer.resume)
        self.assertEqual(answer.profile, "claude-work")

    def test_choosing_a_profile_row_starts_fresh(self):
        """The control beside it: every other row is a fresh start, and `resume` is false —
        which is what makes the launcher mint a new link rather than reuse the old one."""
        def chose(surface, *_a, **_kw):
            return next(r for r in surface.rows if r.id != selector.RESUME_ID)

        with mock.patch.object(selector.palette, "own_the_tty", side_effect=chose):
            answer = selector.pick(cwd=Path(config.ROOT), root=Path(config.ROOT),
                                   start="claude-work", resume=self.resume, ended=True)

        self.assertIsInstance(answer, selector.Choice)
        self.assertFalse(answer.resume)

    def test_a_resume_with_no_start_row_answers_an_empty_profile(self):
        """`Choice(start or "")`, and the fallback is not decoration.

        `pick` is called with `start=None` wherever no row is preselected — ruling 18's
        `default` naming a profile this machine lacks — and what comes back goes straight to
        `resolve(choice.profile)` in `_select_in_pane`, which hands `contain.readable` a
        value it cannot contain. The empty string is a name no profile has; ``None`` is a
        type the next function does not take.
        """
        asked = {"n": 0}

        def chose(surface, *_a, **_kw):
            asked["n"] += 1
            if asked["n"] > 1:
                raise AssertionError("pick looped past the resume row")
            return next(r for r in surface.rows if r.id == selector.RESUME_ID)

        with mock.patch.object(selector.palette, "own_the_tty", side_effect=chose):
            answer = selector.pick(cwd=Path(config.ROOT), root=Path(config.ROOT),
                                   start=None, resume=self.resume, ended=True)

        self.assertTrue(answer.resume)
        self.assertEqual(answer.profile, "")

    def test_the_footer_an_ended_selector_draws_is_the_ended_one(self):
        """**Two conditionals, four halves, and the existing case pins none of them.**
        `test_the_footer_says_escape_closes_this_tab` asserts what is IN `FOOTER_ENDED`; it
        never asserts that an ended selector is given it.

        Both halves of both, because collapsing either way is a real and different lie: a
        footer promising *esc close this chat* on a tab that ran, or promising *esc close
        this tab* on a pane where nothing ever started. And the refusal-carrying form is the
        one an ended tab shows most, which is why it is asked separately.
        """
        listed = self._listed(resume=self.resume)
        refused = selector.Refused("claude-work", "it would not start")

        self.assertEqual(selector._footer(listed, None, ended=True), selector.FOOTER_ENDED)
        self.assertEqual(selector._footer(listed, None), selector.FOOTER)
        self.assertIn(selector.ESC_HINT_ENDED,
                      selector._footer(listed, refused, ended=True))
        self.assertIn(selector.ESC_HINT, selector._footer(listed, refused))
        self.assertNotIn(selector.ESC_HINT_ENDED, selector._footer(listed, refused))

    def test_the_resume_row_id_can_never_be_read_as_a_profile(self):
        """**The invariant, not the spelling.** The id never leaves the process, so a
        re-spelling really is free — what must hold is that it cannot land in the PROFILE
        namespace. `pick` strips `ROW_PREFIX` off whatever it is handed and looks the rest
        up in `have.profiles`, so an id inside that namespace would be read as a profile
        name and a chat would start the wrong harness instead of resuming its own.
        """
        self.assertFalse(selector.RESUME_ID.startswith(selector.ROW_PREFIX))
        self.assertNotIn(selector.RESUME_ID.removeprefix(selector.ROW_PREFIX),
                         self.have.profiles)

    def _cancel_keys(self, *, ended: bool):
        """The `cancel_keys` `pick` builds its surface with."""
        seen: dict = {}

        def record(surface, *_a, **_kw):
            seen["keys"] = surface.cancel_keys
            surface.left = overlay.LEFT_KEY
            return None

        with mock.patch.object(selector.palette, "own_the_tty", side_effect=record):
            selector.pick(cwd=Path(config.ROOT), root=Path(config.ROOT),
                          start="claude-work", ended=ended)
        return seen["keys"]

    def test_ctrl_c_is_taken_away_only_on_an_ended_selector(self):
        """**The ruling, asked of what `pick` PASSES rather than of a surface built by
        hand.** `test_ctrl_c_does_nothing_on_an_ended_selector` below constructs the
        `Selector` itself, so it pins the surface's behaviour given `("escape",)` and says
        nothing about which selector gets it — the expression that decides is here.

        Both halves. Collapsed to the ended answer, every ordinary selector silently loses
        the Ctrl+C it has always had; collapsed to the other, a stray third Ctrl+C after a
        double-Ctrl+C `/exit` closes the tab that second press created.
        """
        self.assertEqual(self._cancel_keys(ended=True), ("escape",))
        self.assertEqual(self._cancel_keys(ended=False), ("escape", overlay.CTRL_C))

    def _offer(self, fid: str = "beta.1"):
        """A chat with an identity, a link and a transcript — what `resume_row` reads."""
        _plant(fid)
        path = Path(config.ROOT) / f"{fid}.jsonl"
        path.write_text("{}\n")
        state.record_harness_session(fid, "conv1")
        state.record_conversation(fid, str(path))
        return path

    def test_the_resume_row_names_the_kind_off_the_identity_record(self):
        """`CHARTER_HARNESS` is written by `launcher.environment` and read back here by a
        SECOND hand-spelled literal, so a retune of either half breaks a round trip that
        nothing else checks — and the row then names no harness at all."""
        self._offer()

        row = launcher.resume_row("beta.1")

        self.assertIsNotNone(row)
        self.assertTrue(row.note.startswith("claude-code"), row.note)

    def test_a_chat_recorded_by_an_older_charter_still_gets_its_row(self):
        """The `.get(…, "")` fallback. An identity from before that field is `{}`, and this
        runs on the `pane-died` path, where an exception is an offer never made — on exactly
        the tab a restored chat came back for."""
        self._offer()
        state.record_identity("beta.1", {})

        self.assertIsNotNone(launcher.resume_row("beta.1"))

    def test_a_chat_with_no_profile_answers_none_rather_than_raising(self):
        """Same shape, other source: `state.profile` answers ``None`` for a chat with none
        recorded, and `resolve` cannot contain that value."""
        state.frame_dir("beta.9", create=True)
        state.record_identity("beta.9", {})

        self.assertIsNone(launcher.resume_row("beta.9"))

    def test_resume_is_offered_only_while_the_conversation_is_really_there(self):
        """Decision 4, asked at the moment of offering (#1101). `leave.conversation_exists`
        is one `stat` of a path the harness itself named, so a transcript deleted since the
        tab ended is simply not offered — where a row built from a remembered answer would
        start a harness on a conversation that is gone.

        `test_no_conversation_means_no_resume_row_at_all` pins the SELECTOR given
        `resume=None`; nothing pinned `resume_row` doing the asking.
        """
        path = self._offer()
        self.assertIsNotNone(launcher.resume_row("beta.1"))

        path.unlink()

        self.assertIsNone(launcher.resume_row("beta.1"))

    def test_a_chat_with_a_link_and_no_conversation_answers_none(self):
        """`state.conversation` answers ``None`` for a chat that has a link but no transcript
        recorded — a Claude Code chat whose SessionStart reported an id before the first
        prompt (C1). Without the fallback that ``None`` reaches `os.path.isfile`, which
        raises `TypeError` on the `pane-died` path, where an exception is an offer never
        made."""
        _plant("beta.5")
        state.record_harness_session("beta.5", "conv5")

        self.assertIsNone(launcher.resume_row("beta.5"))

    def test_resume_is_only_preselected_when_its_row_is_actually_listed(self):
        """`opens_on` may name `RESUME_ID` only when that row is in the list it was handed.
        Dropping the second half of the conjunct returns the id of a row the surface does not
        have, and `Selector._refilter` then finds nothing to put the cursor on — an opening
        paint with no selection at all, on the one surface whose Enter must always do
        something."""
        listed = self._listed(resume=None)

        self.assertEqual(selector.opens_on(listed, "claude-work", resume=self.resume),
                         "claude-work")

    def test_ctrl_c_does_nothing_on_an_ended_selector(self):
        """The ruling, at the surface that has to keep it: a double Ctrl+C is how Claude
        Code exits, so the third press lands HERE — on the selector that replaced it — and
        it must not close the tab the second press created."""
        surface = selector.Selector(catalogue=_rows(), footer=selector.FOOTER_ENDED,
                                    cancel_keys=("escape",))

        chosen, _tty = _drive(surface, [b"\x03"])

        self.assertIsNone(chosen)
        self.assertEqual(surface.left, overlay.LEFT_EOF,
                         "Ctrl+C cancelled the selector an ended tab draws")


class _Tmux:
    """`tmuxctl.run` stood in: answers one `list-panes`, records every other command.

    The listing is what a real server would hand back — the pane's two option VALUES and
    its `#{pane_dead}` — so a case states what the SERVER says rather than what charter
    concluded from it, which is the only way these guards can be tested at all.
    """

    def __init__(self, rows=(), *, rc: int = 0, split: str = "%7") -> None:
        self.rows = list(rows)
        self.rc = rc
        self.split = split
        self.calls: list[list[str]] = []

    def __call__(self, _action, argv, **_kw):
        self.calls.append(list(argv))
        if "list-panes" in argv:
            return subprocess.CompletedProcess(
                argv, self.rc, stdout="".join(r + "\n" for r in self.rows), stderr="")
        if "split-window" in argv:
            return subprocess.CompletedProcess(argv, 0, stdout=self.split + "\n",
                                               stderr="")
        return subprocess.CompletedProcess(argv, 0, stdout="", stderr="")

    def verbs(self) -> list[str]:
        """The tmux SUBCOMMAND of each call — never a substring of the whole line, because
        an option's value can hold the text of another command."""
        return [c[3] for c in self.calls if len(c) > 3]

    def wrote(self, verb: str) -> list[list[str]]:
        return [c for c in self.calls if verb in c]


def _row(pane: str, dead: str, chat: str, plane: str, drawer: str = "") -> str:
    return "\t".join((pane, dead, chat, plane, drawer))


class TheEndedLoopEndsOnItsOwn(PersonaIso, unittest.TestCase):
    """**The loop that answers endings inside an operator's tmux stops by itself.**

    `_wait_out_the_ended_tab` is awake for the life of the frame, and until this it had no
    bound of any kind: its only exit was `ended.present` answering ``""``, which happens
    because `state.claim_ended` is an `O_EXCL` create. That made a file another process
    creates the sole terminator of a loop whose wait returns INSTANTLY for an already-dead
    pane — the state a crash leaves, since the drawer does not touch it. A claim that never
    lands was therefore charter opening drawers forever with an operator watching.

    Measured rather than argued: the deletion sweep could not score that claim at all,
    because deleting it made the shard HANG for ten minutes until the runner killed it and
    reaped an orphaned tmux server. A mutation that hangs is not a red test, which is why
    the runaway fakes below raise far above either bound — so a missing bound fails fast
    and visibly instead of running out somebody's clock.

    The third case is the control: the claim is still what ordinarily ends this, and a tab
    whose choice was taken must not be complained about.
    """

    def setUp(self) -> None:
        super().setUp()
        _plant("beta.1")

    def _run(self, answer, *, cap: int = 400):
        """Drive the loop with scripted `present` answers. ``(code, sentences, presented)``.

        *cap* is the runaway guard and deliberately ~50x either bound: without a bound in
        the code under test this raises, so the case is red in milliseconds rather than
        hanging the suite the way the sweep shard hung.
        """
        said: list[str] = []
        seen = {"n": 0}

        def present(fid, *, socket):
            seen["n"] += 1
            if seen["n"] > cap:
                raise AssertionError(
                    f"presented {seen['n']} times for one chat — the loop has no bound")
            return answer(seen["n"])

        with mock.patch.object(commands_frame, "_wait_for_harness", return_value=0), \
             mock.patch.object(ended, "present", present), \
             mock.patch.object(commands_frame.util, "err", said.append):
            code, self.gave_up = commands_frame._wait_out_the_ended_tab(
                SERVER, fid="beta.1", harness_pane="%1")
        return code, said, seen["n"]

    def test_a_claim_that_never_lands_stops_on_the_attempt_bound(self):
        """Every ending presented, none ever taken: the count is what stops it."""
        code, said, presented = self._run(lambda _n: "selector")

        self.assertEqual(presented, commands_frame._ENDED_ATTEMPTS)
        self.assertEqual(len(said), 1, said)
        self.assertIn("beta.1", said[0], "the sentence does not name the chat")
        self.assertIn("stopped waiting", said[0])
        self.assertIn("still holding", said[0],
                      "the operator is not told the choice is still takeable")
        self.assertEqual(code, 0)
        # **And the caller is TOLD, which is what makes the sentence above true.** The tail
        # of `_launch_in_operator_tmux` reads a non-`None` code as an ending like any other
        # and closes the window and reaps the directory — so without this second answer
        # charter promised the tab was still there and killed it in the next breath.
        self.assertTrue(self.gave_up, "the give-up was not reported to the caller")

    def test_a_runaway_that_is_slow_stops_on_the_deadline(self):
        """A chat resumed all day must not be cut off by a count, so the clock bounds it too
        — and a jump past the deadline stops it before the count is anywhere near spent."""
        clock = iter([0.0] + [commands_frame._ENDED_SECONDS + 1.0] * 4000)
        with mock.patch.object(commands_frame.time, "monotonic", lambda: next(clock)):
            code, said, presented = self._run(lambda _n: "selector")

        self.assertEqual(presented, 1,
                         "the deadline should have tripped before the attempt bound")
        self.assertEqual(len(said), 1, said)
        self.assertIn("hours is the cap", said[0])
        self.assertIn("beta.1", said[0])

    def test_the_deadline_trips_on_the_cap_itself(self):
        """`>=`, and the boundary is the whole of what distinguishes it from `>`.

        `test_a_runaway_that_is_slow_stops_on_the_deadline` jumps the clock a second PAST the
        cap, so it holds for either comparison. A clock landing exactly ON the cap is the one
        input that tells them apart, and with `>` that turn is allowed through — which is not
        an abstraction here: every turn of this loop is another drawer opened in front of an
        operator who has already stopped answering.
        """
        clock = iter([0.0] + [float(commands_frame._ENDED_SECONDS)] * 4000)

        with mock.patch.object(commands_frame.time, "monotonic", lambda: next(clock)):
            _code, said, presented = self._run(lambda _n: "selector")

        self.assertEqual(presented, 1, "the deadline did not trip on the cap itself")
        self.assertEqual(len(said), 1, said)
        self.assertIn("hours is the cap", said[0])

    def test_a_taken_choice_ends_it_on_the_claim_with_nothing_said(self):
        """The control: the claim is still the ordinary terminator, and a tab that was
        answered draws no sentence at all."""
        code, said, presented = self._run(lambda n: "selector" if n == 1 else "")

        self.assertEqual(presented, 2)
        self.assertEqual(said, [], "charter complained about a tab whose choice was taken")
        self.assertEqual(code, 0)
        # The control for the flag as well as for the sentence: an ordinary ending must
        # still close its window and reap, exactly as it always did.
        self.assertFalse(self.gave_up, "an ordinary ending was reported as a give-up")


class NothingActsOnARecordAlone(PersonaIso, unittest.TestCase):
    """**No kill, respawn or split acts on a record.** One listing proves the target.

    This is the #933 and #1103 rule, and the reason it has a class of its own is that every
    case below is a real state a plane reaches: two planes in one tmux with the same chat
    ids, a session an older charter created and never marked, a record left naming a pane
    tmux has since handed to somebody else, and a server that will not answer. Each one,
    acted on, is a respawn or a kill aimed at a window that is not this chat's.
    """

    def setUp(self) -> None:
        super().setUp()
        _plant("beta.1")
        state.record_drawn("beta.1")
        state.record_exit("beta.1", 0)
        self.plane = commands_frame._this_plane()
        self.enterContext(mock.patch.object(ended.tmuxctl, "is_operator_socket",
                                            return_value=False))

    def _present(self, rows, *, rc: int = 0) -> tuple[str, _Tmux]:
        fake = _Tmux(rows, rc=rc)
        with mock.patch.object(ended.tmuxctl, "run", fake):
            return ended.present("beta.1", socket=SERVER), fake

    def test_a_proven_dead_pane_is_respawned_without_k(self):
        """The one case that acts, so the refusals below mean something."""
        answer, fake = self._present([_row("%1", "1", "beta.1", self.plane)])

        self.assertEqual(answer, "selector")
        respawns = fake.wrote("respawn-pane")
        self.assertEqual(len(respawns), 1, fake.calls)
        self.assertNotIn("-k", respawns[0], "a stale record would have killed a live agent")
        self.assertEqual(respawns[0][respawns[0].index("-t") + 1], "%1")

    def test_another_planes_pane_is_not_touched(self):
        """Two planes open inside one tmux have chats of the same id — #933's own shape."""
        answer, fake = self._present([_row("%1", "1", "beta.1", "/some/other/plane")])

        self.assertEqual(answer, "")
        self.assertEqual(fake.wrote("respawn-pane"), [])
        self.assertFalse(state.is_ended("beta.1"))

    def test_an_unmarked_pane_is_not_touched(self):
        """A session an older charter created carries no plane marker, and is left alone."""
        answer, fake = self._present([_row("%1", "1", "beta.1", "")])

        self.assertEqual(answer, "")
        self.assertEqual(fake.wrote("respawn-pane"), [])

    def test_a_recorded_pane_listed_under_another_chat_is_not_touched(self):
        """The record is stale: tmux has handed `%1` to a different chat since."""
        answer, fake = self._present([_row("%1", "1", "beta.2", self.plane)])

        self.assertEqual(answer, "")
        self.assertEqual(fake.wrote("respawn-pane"), [])

    def test_a_live_pane_is_not_touched(self):
        """Charter refuses before tmux would: the harness is still running in there."""
        answer, fake = self._present([_row("%1", "0", "beta.1", self.plane)])

        self.assertEqual(answer, "")
        self.assertEqual(fake.wrote("respawn-pane"), [])

    def test_a_server_that_does_not_answer_touches_nothing(self):
        """#1100: a server that would not answer is not a server with nothing on it."""
        answer, fake = self._present([], rc=1)

        self.assertEqual(answer, "")
        self.assertEqual(fake.wrote("respawn-pane"), [])
        self.assertFalse(state.is_ended("beta.1"))

    def test_the_hook_and_the_late_check_present_once(self):
        """The `pane-died` hook and `_launch`'s late check both answer one death."""
        rows = [_row("%1", "1", "beta.1", self.plane)]
        first, fake = self._present(rows)
        second, again = self._present(rows)

        self.assertEqual((first, second), ("selector", ""))
        self.assertEqual(len(fake.wrote("respawn-pane")), 1)
        self.assertEqual(again.wrote("respawn-pane"), [],
                         "one exit was presented twice")

    def test_a_crash_opens_a_drawer_and_marks_it(self):
        state.record_exit("beta.1", 3)

        answer, fake = self._present([_row("%1", "1", "beta.1", self.plane)])

        self.assertEqual(answer, "drawer")
        self.assertEqual(fake.wrote("respawn-pane"), [],
                         "a crash must leave the harness's last lines on screen")
        split = fake.wrote("split-window")
        self.assertEqual(len(split), 1, fake.calls)
        self.assertEqual(split[0][split[0].index("-t") + 1], "%1")
        marked = fake.wrote(ended.DRAWER_OPTION)
        self.assertEqual(len(marked), 1, "the drawer was never marked as this chat's")
        self.assertEqual(marked[0][-1], "beta.1")
        self.assertEqual(state.drawer("beta.1"), "%7")

    def test_drop_drawer_kills_only_a_proven_drawer(self):
        state.record_drawer("beta.1", "%7")
        fake = _Tmux([_row("%7", "0", "beta.1", self.plane, "beta.1"),
                      _row("%8", "0", "beta.1", self.plane)])

        with mock.patch.object(ended.tmuxctl, "run", fake):
            ended.drop_drawer("beta.1")

        killed = fake.wrote("kill-pane")
        self.assertEqual([c[c.index("-t") + 1] for c in killed], ["%7"])
        self.assertIsNone(state.drawer("beta.1"))

    def test_drop_drawer_never_kills_the_recorded_pane_when_unproven(self):
        """The record names `%7`; the listing says `%7` belongs to another chat now."""
        state.record_drawer("beta.1", "%7")
        fake = _Tmux([_row("%7", "0", "beta.2", self.plane, "beta.2")])

        with mock.patch.object(ended.tmuxctl, "run", fake):
            ended.drop_drawer("beta.1")

        self.assertEqual(fake.wrote("kill-pane"), [])

    def test_a_chat_with_no_recorded_server_keeps_its_drawer_record(self):
        """**"Charter could not ask" is not "there is no drawer".**

        A chat whose server record is missing is one charter cannot LOOK on, not one whose
        drawer has gone — so forgetting the record would throw away the only pointer back
        to a pane that may still be on somebody's screen. That is the third answer
        collapsed into the second, which `proof` refuses for a server that will not answer
        (#1100) and which the `pr is None` branch refuses again.

        Red without the guard: the record is cleared, and nothing can prove that pane
        afterwards — there is no second place the id is written down.
        """
        state.record_drawer("beta.1", "%7")
        state.record_server("beta.1", "")

        fake = _Tmux()
        with mock.patch.object(ended.tmuxctl, "run", fake):
            ended.drop_drawer("beta.1")

        self.assertEqual(state.drawer("beta.1"), "%7",
                         "the drawer record was forgotten because charter could not ask "
                         "which server to look on")
        self.assertEqual(fake.calls, [], "it asked a server it does not know")

    def test_choose_respawns_only_a_proven_dead_pane(self):
        """A drawer can sit on screen for hours; the pane may be live again by the Enter."""
        fake = _Tmux([_row("%1", "0", "beta.1", self.plane)])

        with mock.patch.object(ended.tmuxctl, "run", fake):
            ended.choose(overlay.Row(id=ended.RESUME_ID, title="resume"), "beta.1")

        self.assertEqual(fake.wrote("respawn-pane"), [])

    def test_it_reads_nothing_from_the_pane_and_sends_no_keys(self):
        """ADR 0018's two bounds, asserted over every command this module issues: the
        presentation is chosen from the exit code and the record, never from what the
        harness printed, and charter never types into a harness."""
        seen: list[list[str]] = []
        for code, rows in ((0, [_row("%1", "1", "beta.1", self.plane)]),
                           (3, [_row("%1", "1", "beta.1", self.plane)]),
                           (0, [_row("%1", "0", "beta.1", self.plane)])):
            state.clear_ended("beta.1")
            state.record_exit("beta.1", code)
            _answer, fake = self._present(rows)
            seen.extend(fake.calls)

        flat = [" ".join(c) for c in seen]
        self.assertTrue(seen, "nothing was recorded, so this asserts nothing")
        self.assertFalse([c for c in flat if "capture-pane" in c], flat)
        self.assertFalse([c for c in flat if "send-keys" in c], flat)

    def test_a_chat_with_no_recorded_server_is_never_presented(self):
        """**A guard on what a respawn and a split are AIMED at**, so it is never deleted
        as dead unless a guard below refuses the same input — and none does.

        With it gone, `present` runs with `socket=""`, and the first thing it does is
        `proof`, which issues `tmuxctl.server_argv("", "list-panes", …)`. That builds
        `tmux -u -L '' …` — `is_socket_path("")` is False — a command against a server
        charter cannot name, on the `pane-died` path whose next step respawns a pane.
        Nothing between here and there re-asks for a socket.
        """
        state.record_server("beta.1", "")
        fake = _Tmux([_row("%1", "1", "beta.1", self.plane)])

        with mock.patch.object(ended.tmuxctl, "run", fake):
            rc = ended.cmd_frame_ended(SimpleNamespace(chat="beta.1"))

        self.assertEqual(rc, 0)
        self.assertEqual(fake.calls, [],
                         "a server charter cannot name was asked anyway")

    def test_a_listing_row_charter_cannot_assign_is_dropped_and_not_unpacked(self):
        """A window NAME may contain a TAB and `list-panes` does not quote it, so a row can
        come back with more fields than :data:`ended.PROOF_FORMAT` asks for.

        Red without the guard: the five-way unpack raises `ValueError`, `present`'s own
        `except` swallows it and answers ``""`` — so the hook silently stops working for
        EVERY exit on this plane, leaving each tab dead and offering nothing. The extra row
        here is a live pane, so the chat's real row is still in the same listing and the
        only thing being measured is whether the wide one costs the answer.
        """
        wide = _row("%4", "0", "beta.1", self.plane) + "\ta name with\ta tab"
        answer, fake = self._present([wide, _row("%1", "1", "beta.1", self.plane)])

        self.assertEqual(answer, "selector")
        self.assertEqual(len(fake.wrote("respawn-pane")), 1, fake.calls)

    def test_a_split_that_failed_is_never_acted_on_as_a_drawer(self):
        """**The guard below refuses only the WRITE, and that is why this one stays.**

        With it gone, a failed `split-window` leaves `pane == ""` and `_open_drawer` goes on
        to `set-option -p -t ''` and `select-pane -t ''`. An empty target is not a no-op:
        measured on tmux 3.7c, an empty `-t` resolves to the ACTIVE pane — so charter would
        mark the operator's own pane as this chat's drawer and move the keyboard onto it.
        `state.record_drawer` refuses to write `""`, which keeps the RECORD clean and stops
        neither of those two commands.
        """
        state.record_exit("beta.1", 3)

        class _TheSplitFails(_Tmux):
            def __call__(self, action, argv, **kw):
                out = super().__call__(action, argv, **kw)
                if "split-window" in argv:
                    return subprocess.CompletedProcess(argv, 1, stdout="", stderr="")
                return out

        fake = _TheSplitFails([_row("%1", "1", "beta.1", self.plane)])
        with mock.patch.object(ended.tmuxctl, "run", fake):
            ended.present("beta.1", socket=SERVER)

        self.assertEqual(fake.wrote("set-option"), [],
                         "an empty target was marked as this chat's drawer")
        self.assertEqual(fake.wrote("select-pane"), [],
                         "the keyboard was moved onto a pane the split never made")
        self.assertIsNone(state.drawer("beta.1"))

    def test_drop_drawer_forgets_nothing_when_the_server_proved_nothing(self):
        """The existing case covers the no-SOCKET door; this is the no-ANSWER one.

        A server that will not answer is not a server with nothing on it (#1100), so
        nothing is killed AND nothing is forgotten — the record is the only way back to a
        pane that may still be on somebody's screen, and a later call that does get an
        answer can still prove it and kill it.
        """
        state.record_drawer("beta.1", "%7")
        fake = _Tmux(rc=1)

        with mock.patch.object(ended.tmuxctl, "run", fake):
            ended.drop_drawer("beta.1")

        self.assertEqual(fake.wrote("kill-pane"), [])
        self.assertEqual(state.drawer("beta.1"), "%7",
                         "the record was dropped on a reading charter never got")

    def test_choosing_nothing_in_the_drawer_touches_no_tmux(self):
        """Esc and end of input both choose nothing, and `choose(None, …)` must act on
        nothing at all. Red without the guard: `row.id` raises `AttributeError` inside
        `draw`'s own `except`, the pane is handed back silently, and what the operator sees
        is a respawn that never happened — with no sentence anywhere saying so.
        """
        fake = _Tmux([_row("%1", "1", "beta.1", self.plane)])

        with mock.patch.object(ended.tmuxctl, "run", fake):
            ended.choose(None, "beta.1")

        self.assertEqual(fake.calls, [], "a cancel reached tmux")

    def test_close_in_the_drawer_goes_through_frame_close(self):
        """*close this tab* is the same teardown by every route — the mark, the transcript,
        the manifest entry and the window — so the drawer spawns `frame-close` rather than
        killing anything itself. `tabmenu`'s twin has the identical case.

        Red without the branch: the row falls past `CLOSE_ID` to the respawn arm, so *close
        this tab* would RESTART the harness it was pressed to stop.
        """
        spawned: list = []
        fake = _Tmux([_row("%1", "1", "beta.1", self.plane)])

        with mock.patch("charter.frame.builtin_actions._spawn",
                        side_effect=lambda argv, *, fid: spawned.append(argv)), \
                mock.patch.object(ended.tmuxctl, "run", fake):
            ended.choose(overlay.Row(id=ended.CLOSE_ID, title="close this tab"), "beta.1")

        self.assertEqual(len(spawned), 1, spawned)
        self.assertIn("frame-close", spawned[0])
        self.assertIn("beta.1", spawned[0])
        self.assertEqual(fake.wrote("respawn-pane"), [],
                         "close restarted the harness it was pressed to stop")

    def test_start_fresh_respawns_the_selector_and_resume_the_profile(self):
        """**The drawer never starts a profile of its own** — decision 4's *back to the
        profile selector*. *start fresh* respawns into the selector with the resume row
        withheld, and only the RESUME row runs the chat's own command again.

        Red without the branch: `FRESH_ID` falls to the `else` and returns, so *start fresh*
        does nothing at all — a row the operator pressed, and a pane still dead.
        """
        fresh = self._respawned(ended.FRESH_ID)
        self.assertIn("--select", fresh)
        self.assertIn("--ended", fresh)
        self.assertIn("--fresh", fresh)

        resume = self._respawned(ended.RESUME_ID)
        self.assertIn("--resume", resume)
        self.assertIn("claude", resume)
        self.assertNotIn("--select", resume,
                         "resume opened the selector instead of the chat's own command")

    def _respawned(self, row_id: str) -> list[str]:
        """The respawn argv `choose` issues for *row_id*, on a proven dead pane."""
        state.clear_ended("beta.1")
        fake = _Tmux([_row("%1", "1", "beta.1", self.plane)])
        with mock.patch.object(ended.tmuxctl, "run", fake):
            ended.choose(overlay.Row(id=row_id, title=row_id), "beta.1")
        wrote = fake.wrote("respawn-pane")
        self.assertEqual(len(wrote), 1, fake.calls)
        return wrote[0]

    def test_a_respawn_carries_the_frames_identity_and_nothing_else(self):
        """**A tmux `-e` is argv, and argv is not private** (#446).

        `respawn-pane -e NAME=VALUE` puts every name on the tmux client's COMMAND LINE:
        world-readable in `/proc/<pid>/cmdline` on Linux for as long as the client runs,
        visible to `ps` for every local user, and recorded by exec-audit tooling. Measured
        on one real environment, passing it whole was 129 argv elements, four live
        1Password service-account tokens and an npm auth token. So a respawn carries the
        five identity names and nothing else — and it carries ALL five, the empty ones
        included, because an inherited value is as wrong as a stale one.

        **The same answer on both servers, which is why this is one assertion rather than
        two.** A guest/host conditional used to stand at this line, and it could not decide
        anything: the two builders differ only by a `PATH` added `if env.get("PATH")`, and
        what reaches them is `state.identity(fid)` — whose key set is exactly
        `_FRAME_IDENTITY`, because that is what both of its writers pass. `ended.present`
        carries the measurement, `layout.respawn_argv` carries the second one (tmux
        overwrites a pane's `$PATH` after applying `-e`, so it would not have survived).

        Red if anybody passes the environment whole again, on either socket, which is the
        #446 defect this asserts against rather than the conditional that was deleted.
        """
        for guest in (True, False):
            with self.subTest(guest=guest):
                self.assertEqual(self._respawn_env(guest=guest),
                                 set(commands_frame._FRAME_IDENTITY))

    def _respawn_env(self, *, guest: bool) -> set[str]:
        """The NAMES a respawn puts on `-e`, with the socket read as guest or as charter's.

        Names and never values: what is being measured is which env a respawn carries, and
        a `-e` is argv — so the assertion must not itself spell a value out.
        """
        state.clear_ended("beta.1")
        fake = _Tmux([_row("%1", "1", "beta.1", self.plane)])
        with mock.patch.object(ended.tmuxctl, "is_operator_socket", return_value=guest), \
                mock.patch.object(ended.tmuxctl, "run", fake):
            ended.choose(overlay.Row(id=ended.FRESH_ID, title="start fresh"), "beta.1")
        argv = fake.wrote("respawn-pane")[0]
        return {argv[i + 1].split("=", 1)[0] for i, a in enumerate(argv) if a == "-e"}

    def test_an_unspellable_chat_is_refused_before_anything_is_looked_up(self):
        """**The guard is what refuses, and this is how that becomes observable.**

        `present` is the `pane-died` hook's own child, and the value it is handed is expanded
        by tmux out of `#{@charter_chat}` into a shell-quoted `run-shell` string. A value
        charter cannot name a chat from is one it must not look a server up for, so the id is
        held to `chats.ID_RE` before any state is read at all.

        **Deleting that guard does not change the ANSWER**, which is why the deletion sweep
        could take it with every test still green: `state.profile` would be called with the
        hostile id, `state.frame_dir` would refuse it as not a safe child, and `present` would
        return ``""`` a few lines further down. What changes is WHO refuses — an entry point
        tmux invokes would be depending for its safety on a function three calls away going
        on refusing the same inputs, which is the coupling that has bitten this project twice.

        So the assertion is not about the answer but about the reach: nothing is looked up,
        and no tmux command is issued.
        """
        looked_up: list = []
        fake = _Tmux()

        with mock.patch.object(ended.state, "profile",
                               side_effect=lambda f: looked_up.append(f)), \
                mock.patch.object(ended.tmuxctl, "run", fake):
            answer = ended.present("../nope", socket=SERVER)

        self.assertEqual(answer, "")
        self.assertEqual(looked_up, [],
                         "a value charter cannot name a chat from was looked up anyway")
        self.assertEqual(fake.calls, [], "an unspellable chat reached tmux")

    def test_a_respawn_tmux_refused_offers_nothing(self):
        """The plan's Behaviour §6, and the answer has to be ``""``.

        The respawn carries no `-k`, so tmux itself refuses a pane whose harness is somehow
        still running (`layout.respawn_argv`) — and answering `"selector"` for a pane that
        still holds whatever was in it would report a surface the operator cannot see, on
        the one call whose whole job is to say whether anything was offered.

        **Measured, and it differs from what the plan assumed:** this does not merely count
        toward `_wait_out_the_ended_tab`'s attempt bound. ``""`` is falsey, so that loop
        RETURNS rather than going round — a permanently refusing respawn ends at once
        instead of spending eight attempts on it.
        """
        class _TheRespawnFails(_Tmux):
            def __call__(self, action, argv, **kw):
                out = super().__call__(action, argv, **kw)
                if "respawn-pane" in argv:
                    return subprocess.CompletedProcess(argv, 1, stdout="",
                                                       stderr="pane still active")
                return out

        fake = _TheRespawnFails([_row("%1", "1", "beta.1", self.plane)])
        with mock.patch.object(ended.tmuxctl, "run", fake):
            answer = ended.present("beta.1", socket=SERVER)

        self.assertEqual(answer, "")
        self.assertEqual(len(fake.wrote("respawn-pane")), 1, "it never even tried")

    def test_only_the_recorded_pane_is_taken_as_the_harness(self):
        """**A guard on what a respawn is AIMED at**, and no guard below refuses the same
        input: `present` respawns whatever `Proof.harness` names.

        `proof` walks every row the listing returns. Without `pane == recorded`, any pane of
        this chat on this plane that carries no drawer marker becomes the harness — so the
        respawn lands in whichever such pane the listing happened to return last, which may
        be one the operator is working in. `%9` here is exactly that: same chat, same plane,
        no marker, listed after the recorded `%1`.
        """
        answer, fake = self._present([_row("%1", "1", "beta.1", self.plane),
                                      _row("%9", "1", "beta.1", self.plane)])

        self.assertEqual(answer, "selector")
        respawns = fake.wrote("respawn-pane")
        self.assertEqual(len(respawns), 1, fake.calls)
        self.assertEqual(respawns[0][respawns[0].index("-t") + 1], "%1",
                         "the respawn was aimed at a pane no record names")

    def test_a_drawer_row_choose_does_not_recognise_does_nothing(self):
        """The `else: return` beneath the two rows that act. Forced true, every row that is
        neither resume nor close respawns the pane as *start fresh* — including an id this
        function has never heard of, which is the shape a fourth drawer row would arrive in.
        """
        fake = _Tmux([_row("%1", "1", "beta.1", self.plane)])

        with mock.patch.object(ended.tmuxctl, "run", fake):
            ended.choose(overlay.Row(id="ended:something-new", title="?"), "beta.1")

        self.assertEqual(fake.wrote("respawn-pane"), [],
                         "a row `choose` does not recognise respawned the pane")

    def test_the_drawer_hands_its_pane_back_even_when_it_raises(self):
        """`draw` runs in a pane it must give back whatever happens — the same promise
        `overlay.Surface.run` makes one layer down. Narrowed to a type nothing raises, an
        error building the rows escapes and the drawer is left on screen holding a traceback,
        with the keyboard still in it."""
        closed: list = []

        with mock.patch.object(ended, "drawer_rows", side_effect=RuntimeError("boom")), \
                mock.patch.object(ended, "_close_this_pane",
                                  side_effect=lambda *a, **kw: closed.append(True)):
            rc = ended.draw(SimpleNamespace(chat="beta.1"))

        self.assertEqual(rc, 0)
        self.assertEqual(len(closed), 1, "the drawer's pane was never handed back")

    def test_the_keyboard_goes_back_to_a_real_harness_pane_and_only_to_one(self):
        """**Both halves, because deleting an `if` takes the thing it guards with it.**

        An earlier version of this case asserted only that an empty *harness* issues no
        `select-pane` — which is just as true once the whole statement is gone, so it passed
        with the guard and without it and pinned nothing at all. That is the shape this
        branch has paid for twice; the fix is to assert the BEHAVIOUR as well as the refusal.

        The body is what puts the operator's keyboard back on the chat's own pane as the
        drawer closes. Without it the keyboard is left in a pane `kill-pane` is about to
        take, which tmux then resolves wherever it likes.

        The guard is what keeps that same command off an EMPTY target. Measured on tmux
        3.7c, `select-pane -t ''` selects the ACTIVE pane — so a chat whose harness pane was
        never recorded would drag the operator's keyboard onto whatever they were looking
        at. Nothing below refuses it: `tmuxctl.run` issues the argv it is handed.
        """
        real = _Tmux()
        with mock.patch.object(ended.tmuxctl, "live_pane_by_pid", return_value=None), \
                mock.patch.object(ended.tmuxctl, "run", real):
            ended._close_this_pane(SERVER, fid="beta.1", harness="%1", own_pane="%3")

        aimed = [c[c.index("-t") + 1] for c in real.wrote("select-pane")]
        self.assertEqual(aimed, ["%1"],
                         "the keyboard was never put back on the chat's own pane")

        empty = _Tmux()
        with mock.patch.object(ended.tmuxctl, "live_pane_by_pid", return_value=None), \
                mock.patch.object(ended.tmuxctl, "run", empty):
            ended._close_this_pane(SERVER, fid="beta.1", harness="", own_pane="%3")

        self.assertEqual(empty.wrote("select-pane"), [],
                         "an empty target was aimed at the operator's own active pane")

    def test_the_palette_command_draws_the_drawer_when_it_is_told_to(self):
        """**`--ended` is the most specific of the four surfaces `frame-palette` can be**, so
        `cmd_palette` asks it first — a drawer is not something a keypress produces, it is a
        pane charter split beneath a harness that has just crashed.

        The literal is the argparse dest. Retuned, the flag matches nothing and that pane
        opens the ordinary `F2` palette instead: rows about the frame, in a rectangle charter
        opened to ask about this one chat's ending.
        """
        drawn: list = []

        with mock.patch.object(ended, "draw", side_effect=lambda a: drawn.append(a) or 0), \
                mock.patch.object(commands_frame.pane, "claim", return_value=None), \
                mock.patch.object(commands_frame.pane, "release"):
            rc = commands_frame.cmd_palette(
                SimpleNamespace(client="", pane=True, ended=True, chat="beta.1"))

        self.assertEqual(rc, 0)
        self.assertEqual(len(drawn), 1,
                         "the ended drawer was not the surface that opened")

    def test_a_chat_argument_that_is_absent_is_answered_not_raised(self):
        """`cmd_frame_ended` is a `run-shell -b` child and promises never to raise. Without
        the fallback, an args namespace carrying `chat=None` makes `None.strip()` an
        `AttributeError` out of a tmux hook, where nothing reads the traceback."""
        fake = _Tmux()

        with mock.patch.object(ended.tmuxctl, "run", fake):
            self.assertEqual(ended.cmd_frame_ended(SimpleNamespace(chat=None)), 0)

        self.assertEqual(fake.calls, [])

    def test_a_chat_argument_is_stripped_at_both_ends(self):
        """tmux expands `#{@charter_chat}` into a shell-quoted `run-shell` string, so the
        value can arrive with whitespace around it. `lstrip` leaves the trailing space, and
        `chats.ID_RE.fullmatch` then refuses the chat outright — the ended step silently
        never runs for it."""
        fake = _Tmux([_row("%1", "1", "beta.1", self.plane)])

        with mock.patch.object(ended.tmuxctl, "run", fake):
            ended.cmd_frame_ended(SimpleNamespace(chat="  beta.1  "))

        self.assertEqual(len(fake.wrote("respawn-pane")), 1,
                         "a chat id with whitespace around it was refused")

    def test_the_command_always_returns_zero(self):
        """It runs as a `run-shell -b` child of the tmux server: a non-zero return is
        printed INTO the harness pane and drops it into copy-mode, which is charter drawing
        in the one rectangle ADR 0018 says it never draws."""
        fake = _Tmux()
        with mock.patch.object(ended.tmuxctl, "run", fake):
            self.assertEqual(ended.cmd_frame_ended(SimpleNamespace(chat="../x")), 0)
        self.assertEqual(fake.calls, [], "an unspellable chat reached tmux")

        with mock.patch.object(ended, "proof", side_effect=RuntimeError("boom")):
            self.assertEqual(ended.present("beta.1", socket=SERVER), "")
            self.assertEqual(ended.cmd_frame_ended(SimpleNamespace(chat="beta.1")), 0)


class TheHookAndItsBranch(PersonaIso, unittest.TestCase):
    """`pane-died[1]` stops being `kill-window` — for a PROFILE's chat, and nothing else.

    The escape hatch keeps today's ending (the ruling on open question 5): `charter frame --
    <cmd>` closes its window when the command exits and hands the caller its code, because
    it is not a harness and there is no conversation to offer back.
    """

    def test_index_one_runs_the_ended_step(self):
        argv = commands_frame._pane_died_ended_hook_argv(socket="s", harness_pane="%1")

        self.assertEqual(argv[-2], "pane-died[1]")
        self.assertEqual(argv[argv.index("-t") + 1], "%1")
        action = argv[-1]
        self.assertNotIn("kill-window", action, "the window is the whole thing kept now")
        self.assertIn("frame-ended", action)
        self.assertIn("#{@charter_chat}", action, "tmux expands the chat at the keypress")
        self.assertTrue(action.startswith("run-shell -b "), action)

    def test_the_action_is_a_constant(self):
        """A hook action is TEXT tmux re-parses, so charter interpolates nothing into it —
        the module docstring's own rule, and the bug it fixed. Two different sockets and
        panes must produce byte-identical action text."""
        one = commands_frame._pane_died_ended_hook_argv(socket="s", harness_pane="%1")[-1]
        two = commands_frame._pane_died_ended_hook_argv(socket="/tmp/other/s",
                                                        harness_pane="%94")[-1]

        self.assertEqual(one, two)

    def test_a_harness_chat_gets_the_ended_step(self):
        """Which of the two hooks a chat gets is decided ONCE, by a function with a name
        rather than a condition buried in the middle of a launch."""
        argv = commands_frame._pane_died_second_hook_argv(socket="s", harness_pane="%1",
                                                          harness_chat=True)

        self.assertIn("frame-ended", argv[-1])
        self.assertNotIn("kill-window", argv[-1])

    def test_a_chat_still_choosing_its_profile_gets_it_too(self):
        """**The defect this case exists for.** The hook is installed ONCE, at launch, and a
        selector chat has no profile at that moment — bare `charter`, the `+`, a workspace
        tab and the palette's new chat all open a pane that asks first and resolves a profile
        minutes later, in the pane.

        Keyed on "is a profile resolved right now", every one of those would have been armed
        with `kill-window`, so the first harness started from the selector would still have
        had its window killed on exit: the whole feature missing on its commonest path, and
        invisible to any test that launches a named profile.
        """
        argv = commands_frame._pane_died_second_hook_argv(socket="s", harness_pane="%1",
                                                          harness_chat=True)

        self.assertIn("frame-ended", argv[-1])

    def test_the_escape_hatch_keeps_kill_window(self):
        """`charter frame -- <cmd>` is not a harness and never becomes one (the ruling on
        open question 5). Its window closes when the command exits and the caller gets the
        exit code back, which is what a script waiting on it has always been promised.

        Red without the branch: the ended step would keep that window open, `attach` would
        never return, and the code would never arrive.
        """
        argv = commands_frame._pane_died_second_hook_argv(socket="s", harness_pane="%1",
                                                          harness_chat=False)

        self.assertEqual(argv[-1], "kill-window")
        self.assertEqual(
            argv, commands_frame._pane_died_teardown_hook_argv(socket="s",
                                                               harness_pane="%1"),
            "the escape hatch stopped getting the teardown hook unchanged")

    def test_the_escape_hatch_is_never_presented(self):
        """`_profile_name(None)` is `""` for `charter frame -- <cmd>`, and `present` reads
        that as *this is not a harness* before it asks tmux anything."""
        _plant("beta.9", profile="")
        state.record_drawn("beta.9")
        state.record_exit("beta.9", 0)
        fake = _Tmux([_row("%1", "1", "beta.9", commands_frame._this_plane())])

        with mock.patch.object(ended.tmuxctl, "run", fake):
            self.assertEqual(ended.present("beta.9", socket=SERVER), "")

        self.assertEqual(fake.calls, [], "a chat with no profile reached tmux")


class ClosingAnEndedTabDoesNotAsk(PersonaIso, unittest.TestCase):
    """Decision 5: closing a tab is the one way to end a chat, and it ends it for good.

    The confirmation exists to warn about stopping a RUNNING harness. An ended tab has none
    left to stop, so the row is an action rather than a doorway — and a tab still running
    keeps its warning, which is the pin that makes the other half mean something.
    """

    def setUp(self) -> None:
        super().setUp()
        _plant("beta.1")

    def test_a_running_chat_is_confirmed(self):
        self.assertTrue(leave.needs_confirming("beta.1"))

    def test_an_ended_chat_is_not(self):
        state.claim_ended("beta.1")

        self.assertFalse(leave.needs_confirming("beta.1"))

    def test_a_chat_still_at_the_selector_is_not(self):
        """It never started, so there is nothing to warn about there either."""
        state.record_waiting("beta.1")

        self.assertFalse(leave.needs_confirming("beta.1"))

    def test_the_tab_menus_last_row_closes_now_on_an_ended_tab(self):
        state.claim_ended("beta.1")

        rows = tabmenu.catalogue("beta.1")

        self.assertEqual(rows[-1].id, tabmenu.CLOSE_NOW_ID)
        self.assertIn("ended", rows[-1].title)
        self.assertIsNone(tabmenu.opens(rows[-1], "beta.1", live=None),
                          "an ended tab's close row is an action, not a doorway")

    def test_the_tab_menus_last_row_is_still_the_doorway_while_it_runs(self):
        rows = tabmenu.catalogue("beta.1")

        self.assertEqual(rows[-1].id, tabmenu.CLOSE_ID)
        self.assertIsNotNone(tabmenu.opens(rows[-1], "beta.1", live=None))

    def test_choosing_close_now_spawns_the_close(self):
        state.claim_ended("beta.1")
        spawned: list = []

        with mock.patch("charter.frame.builtin_actions._spawn",
                        side_effect=lambda argv, *, fid: spawned.append(argv)):
            acted = tabmenu.chose(tabmenu.catalogue("beta.1")[-1], "beta.1", fid="beta.1")

        self.assertTrue(acted)
        self.assertEqual(len(spawned), 1, spawned)
        self.assertIn("frame-close", spawned[0])
        self.assertIn("beta.1", spawned[0])

    def test_neither_close_now_id_can_be_shipped_as_an_action(self):
        """**The invariant, not the spelling.** Both ids are minted and read inside their
        own module and never leave the process, so a re-spelling of either really is free —
        what is NOT free is the colon.

        `frame/action.py` holds every action id to `component.usable_id` (lower-case
        letters, digits, `_` and at most one dot), and that is the whole of why these cannot
        collide: an id without a colon is one a provider could ship an action under, and
        that action would take the keypress meant for *close this tab*. `frame/leave.py`
        and `frame/tabmenu.py` use the same trick, so both are asked here.
        """
        from charter.frame import component

        for what, row_id in (("the tab menu's", tabmenu.CLOSE_NOW_ID),
                             ("the palette's", leave.CLOSE_NOW_ID)):
            with self.subTest(row=what):
                self.assertIn(":", row_id)
                self.assertFalse(component.usable_id(row_id),
                                 f"a provider could ship an action as {row_id} and steal "
                                 "the keypress that closes an ended tab")

    def test_the_palettes_close_row_says_why_it_will_not_ask(self):
        """The title is the whole account of why this row acts on one keypress where the
        doorway asks first — an operator who has closed a chat before has been warned every
        time, so a row that suddenly acts has to say what changed.

        The tab menu's twin has this assertion (`test_the_tab_menus_last_row_closes_now_on_an
        _ended_tab` checks its title names the ending); the palette's row had its id and its
        placement asserted and its words asserted nowhere.
        """
        state.claim_ended("beta.1")

        row = leave.open_rows("beta.1")[-1]

        self.assertIn("ended", row.title)
        self.assertIn("close", row.title)

    def test_the_palettes_close_row_swaps_the_same_way_and_stays_last(self):
        """`leave.open_rows` puts the destructive row last so it is never one `F2 Enter`
        away. That placement does not move when the row stops being a doorway."""
        state.claim_ended("beta.1")

        rows = leave.open_rows("beta.1")

        self.assertEqual(rows[0].id, leave.OPEN_ID.format(leave.QUIT))
        self.assertEqual(rows[-1].id, leave.CLOSE_NOW_ID)

    def test_choosing_the_palettes_close_row_closes_the_ended_tab(self):
        """**The row was drawn and dispatched nowhere, and this is the case that says so.**

        `test_the_palettes_close_row_swaps_the_same_way_and_stays_last` above asserts the
        row's id and its placement, which is why a retune of the id survived the sweep and
        why nobody noticed the id reached no dispatch at all: `leave.verb_of`,
        `leave.goes_through`, `leave.is_row` and `choose.noun_of` all answered *not mine*,
        `_picker` opened nothing, and `_draw_palette` fell through to
        `reg.invoke("leave:close:now")` — an id no action has. The operator pressed *chat:
        close — its harness has ended* and was told an action had failed.

        Driven through the real hops rather than asserted about them, which is
        `TheKeypressReachesTheTeardown`'s shape one surface over: the real
        `_palette_catalogue`, the real `_picker` as `own_the_tty`'s *then*, the real
        `_draw_palette`, and `builtin_actions._spawn` as the one thing stood in for — so
        what this proves is that the keypress reaches `frame-close` with this chat on its
        argv. The tab menu's twin has had exactly this case since it was written
        (`test_choosing_close_now_spawns_the_close`); this is the one the palette lacked.
        """
        (config.WORKSPACES_DIR / "beta").mkdir(parents=True, exist_ok=True)
        state.claim_ended("beta.1")
        spawned: list = []

        def _own(surface, *, fd=None, out=None, then=None):
            row = next(r for r in surface.rows if r.id == leave.CLOSE_NOW_ID)
            # An ACTION, not a doorway: it opens no confirmation, exactly as the tab
            # menu's twin does not (`tabmenu.opens` answers None for the same row).
            self.assertIsNone(then(row) if then is not None else None,
                              "the ended tab's close row opened a surface to confirm with")
            return row

        with mock.patch.dict("os.environ", {"CHARTER_SESSION_ID": "beta.1"}), \
                mock.patch.object(palette, "own_the_tty", side_effect=_own), \
                mock.patch.object(commands_frame, "_close_palette"), \
                mock.patch.object(commands_frame, "_say_on_screen") as said, \
                mock.patch("charter.frame.builtin_actions._spawn",
                           side_effect=lambda argv, *, fid: spawned.append(argv)):
            rc = commands_frame._draw_palette(SimpleNamespace(chat="beta.1"))

        self.assertEqual(rc, 0)
        self.assertEqual(len(spawned), 1, spawned)
        self.assertIn("frame-close", spawned[0])
        self.assertIn("beta.1", spawned[0])
        said.assert_not_called()


class EveryRowThePaletteDrawsGoesSomewhere(PersonaIso, unittest.TestCase):
    """**No row `F2` draws may dispatch to an action the registry does not have.**

    The general form of the defect above, stated over the catalogue rather than over the one
    row that had it, so this class of bug cannot come back under a different id. A palette
    row ends in exactly one of two places: `_draw_palette` recognises it and acts on it
    itself — a picker doorway (`choose.noun_of`), or one of `frame/leave.py`'s own rows
    (`leave.is_row`) — or it is an action id and goes to `ActionRegistry.invoke`. There is
    no third destination, and `invoke` answers an id it does not hold by refusing, which
    reaches the operator as a failure notice for a keypress that was drawn as a working row.

    **Asked through the same two seams `_draw_palette` asks**, never a list of ids this
    file keeps: a test that re-spelled which rows are dispatched locally would go on passing
    the day the code and the copy drift, which is the failure mode it exists to catch.
    `reg.get` is the question `invoke` asks first, so this is `invoke`'s own refusal
    measured without starting anything.

    Both states of the plane, because the row that broke this only exists in one of them:
    an ordinary chat draws `leave:close` (a doorway) and an ended one draws
    `leave:close:now` (an action id belonging to no action).
    """

    FID = "beta.1"

    def setUp(self) -> None:
        super().setUp()
        (config.WORKSPACES_DIR / "beta").mkdir(parents=True, exist_ok=True)
        _plant(self.FID)
        self.enterContext(mock.patch.dict("os.environ",
                                          {"CHARTER_SESSION_ID": self.FID}))

    def _unhandled(self) -> list[str]:
        """Every catalogue row that would reach `invoke` with an id it does not hold."""
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off")
        rows = commands_frame._palette_catalogue(self.FID, reg, snapshot={})
        self.assertTrue(rows, "the catalogue is empty, so this asserts nothing")
        out = []
        for row in rows:
            if choose.noun_of(row) is not None or leave.is_row(row):
                continue        # `_draw_palette` acts on these itself, before `invoke`
            try:
                reg.get(row.id)
            except actions.ActionError:
                out.append(row.id)
        return out

    def test_no_row_the_palette_draws_dispatches_to_a_missing_action(self):
        self.assertEqual(self._unhandled(), [])

    def test_the_same_holds_on_an_ended_tab(self):
        """The state the defect lived in: `leave.open_rows` swaps the close doorway for
        `leave:close:now`, which is not an action and must not be sent to `invoke` as one."""
        state.claim_ended(self.FID)

        self.assertEqual(self._unhandled(), [])

    def _does_something(self, row) -> bool:
        """Whether choosing *row* in a real `_draw_palette` produces anything observable."""
        opened: list = []
        acted: list = []

        def _own(surface, *, fd=None, out=None, then=None):
            nxt = then(row) if then is not None else None
            if nxt is not None:
                opened.append(nxt)
            return row

        with mock.patch.object(palette, "own_the_tty", side_effect=_own), \
                mock.patch.object(commands_frame, "_close_palette"), \
                mock.patch.object(commands_frame, "_plane_live",
                                  return_value=(frozenset({self.FID}),
                                                {SERVER: {self.FID: "@0"}},
                                                frozenset({self.FID}))), \
                mock.patch.object(commands_frame, "_start_leaving",
                                  side_effect=lambda *a, **kw: acted.append("leave")), \
                mock.patch.object(commands_frame, "_say_on_screen",
                                  side_effect=lambda *a, **kw: acted.append("said")), \
                mock.patch.object(commands_frame, "_start_chat_switch",
                                  side_effect=lambda *a, **kw: acted.append("switch")), \
                mock.patch("charter.frame.builtin_actions._spawn",
                           side_effect=lambda *a, **kw: acted.append("spawn")):
            commands_frame._draw_palette(SimpleNamespace(chat=self.FID))
        return bool(opened or acted)

    def test_a_row_the_palette_recognises_but_never_acts_on_is_a_failure(self):
        """**`is_row` is only half the question, and it is the half that hides the bug.**

        The two cases above skip every row `leave.is_row` claims — rightly, because such a
        row must not reach `invoke` — but *recognised* is not *dispatched*. `_draw_palette`'s
        next branch describes rather than does: it says a doorway's note and returns 0. So a
        row added to `is_row` with no dispatch behind it — which is exactly the defect this
        class was written for, and exactly what `leave:close:now` was — keeps those two green
        while doing nothing at all.

        So this asks the behavioural question of every non-refused row that is NOT an action:
        choosing it must produce something the operator can observe — a surface opened in the
        pane, work spawned, a teardown started, or a sentence on the attention row. Silence
        is the bug. Action rows are excluded because `invoke` is their dispatch and the cases
        above already prove the registry holds them.
        """
        state.claim_ended(self.FID)
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off")
        rows = commands_frame._palette_catalogue(self.FID, reg, snapshot={})

        silent = []
        for row in rows:
            if row.refused:
                continue
            try:
                reg.get(row.id)
                continue            # an action: `invoke` is its dispatch
            except actions.ActionError:
                pass
            if not self._does_something(row):
                silent.append(row.id)

        self.assertEqual(silent, [],
                         "these rows are drawn, can run, and dispatch to nothing at all")


class AnEndedTabIsMarked(PersonaIso, unittest.TestCase):
    """A background or handed-off chat ends whether or not anybody is looking, so the strip
    is where an operator finds out — decision 4's *marked ended in the strip*."""

    def setUp(self) -> None:
        super().setUp()
        _plant("beta.1")
        _plant("beta.2")

    def test_the_roster_says_which_chat_has_ended(self):
        state.claim_ended("beta.2")

        by_id = {c.id: c for c in chats.roster("beta.1")}

        self.assertFalse(by_id["beta.1"].ended)
        self.assertTrue(by_id["beta.2"].ended)

    def test_the_mark_is_one_ascii_cell(self):
        """`slots._BAR_RULE`'s rule rather than a preference: a click on this row is
        resolved by COLUMN, so a glyph a terminal may draw two cells wide would move every
        field after it."""
        self.assertEqual(len(slots.ENDED_MARK), 1)
        self.assertTrue(slots.ENDED_MARK.isascii(), slots.ENDED_MARK)
        # And the mark itself, spelled by hand: it is operator-visible, this PR's own table
        # promises `x` on the strip, and the docs describe it by that character. The two
        # assertions above hold for any one-cell ASCII glyph, so neither pins what an
        # operator was told to look for.
        self.assertEqual(slots.ENDED_MARK, "x")

    def test_the_strip_draws_the_mark_and_the_tab_still_answers(self):
        row = slots._bar(["beta.1", "beta.2"], "beta.1", 80,
                         ended=frozenset({"beta.2"}))[0]

        self.assertIn(slots.ENDED_MARK, tui.strip_ansi(row))
        self.assertIn("beta.2", tui.strip_ansi(row))


class TheEndedSelectorClosesOnlyOnEsc(PersonaIso, unittest.TestCase):
    """**Esc and end of input are the same `None` to every other surface, and here they are
    two different answers** — which is the whole reason `pick` grew two sentinels.

    A real Esc forgets the chat for good, through `cmd_close`: the mark, the transcript, the
    manifest entry and the window. End of input is a closed pty, a killed tmux server or a
    machine that went down, and none of them is the operator asking for anything — so the
    tab stays ended and open, and the pane's own death reaches `frame-ended`, which finds
    `ended` already claimed and does nothing.
    """

    def setUp(self) -> None:
        super().setUp()
        _plant("beta.1")
        state.claim_ended("beta.1")
        self.enterContext(mock.patch.object(launcher, "framed_chat",
                                            return_value="beta.1"))
        self.enterContext(mock.patch.object(launcher.pane, "claim"))
        self.enterContext(mock.patch.object(launcher.pane, "release"))

    def _select(self, answer, *, fresh: bool = False):
        """`_select_in_pane` on an ENDED tab, with the pick answered and both closers
        recorded. Answers ``(rc, chats cmd_close forgot, chats the cancel path closed)``."""
        closed: list = []
        cancelled: list = []
        with mock.patch.object(selector, "pick", return_value=answer), \
                mock.patch.object(commands_frame, "cmd_close",
                                  side_effect=lambda a: closed.append(a.chat_id)), \
                mock.patch.object(launcher, "_close_the_cancelled_chat",
                                  side_effect=lambda f: cancelled.append(f)):
            rc = launcher._select_in_pane(
                SimpleNamespace(start="claude", ended=True, fresh=fresh, profile="",
                                rest=[], attended=True), "beta.1")
        return rc, closed, cancelled

    def test_a_real_escape_closes_the_tab_for_good(self):
        rc, closed, cancelled = self._select(selector.KEY_CANCEL)

        self.assertEqual(rc, selector.CANCELLED_EXIT)
        self.assertEqual(closed, ["beta.1"], "Esc did not forget the chat")
        self.assertEqual(cancelled, [],
                         "an ended tab took the never-started pane's close, which writes "
                         "no closed mark")

    def test_end_of_input_leaves_the_tab_ended_and_open(self):
        rc, closed, cancelled = self._select(selector.END_OF_INPUT)

        self.assertEqual(rc, selector.CANCELLED_EXIT)
        self.assertEqual((closed, cancelled), ([], []),
                         "a terminal going away was read as a decision")
        self.assertTrue(state.is_ended("beta.1"), "the tab stopped being ended")
        self.assertFalse(state.was_closed("beta.1"), "end of input marked the chat closed")

    def _picked(self, choice):
        """`_select_in_pane` on an ENDED tab answering *choice*, with the launch stubbed.

        `attempt` answers ``None`` — what a stand-in for `os.execvpe` hands back — so the
        loop ends rather than going round, and `pick` raises on a second call for
        `test_choosing_resume_asks_for_the_conversation_back`'s measured reason: a stub that
        answers the same thing forever turns a deleted guard into a HANG instead of a
        failure, which is a mutation with no verdict and a shard with no answer.
        """
        closed: list = []
        tried: list = []
        asked = {"n": 0}

        def once(**_kw):
            asked["n"] += 1
            if asked["n"] > 1:
                raise AssertionError("the selector came round instead of starting")
            return choice

        def attempt(p, _rest, **kw):
            tried.append((p.name, kw.get("resume")))
            return None

        with mock.patch.object(selector, "pick", side_effect=once), \
                mock.patch.object(launcher, "resolve",
                                  return_value=(SimpleNamespace(name="claude"), "")), \
                mock.patch.object(launcher, "attempt", side_effect=attempt), \
                mock.patch.object(launcher, "_picked"), \
                mock.patch.object(commands_frame, "cmd_close",
                                  side_effect=lambda a: closed.append(a.chat_id)), \
                mock.patch.object(launcher, "_close_the_cancelled_chat",
                                  side_effect=closed.append):
            rc = launcher._select_in_pane(
                SimpleNamespace(start="claude", ended=True, fresh=False, profile="",
                                rest=[], attended=True), "beta.1")
        return rc, closed, tried

    def test_a_profile_pick_on_an_ended_tab_is_not_read_as_escape(self):
        """**The conjunct asks two things, and dropping either half is a chat forgotten.**

        `ended and choice is selector.KEY_CANCEL` is what tells *the operator pressed Esc*
        from *this tab was put here by an exit*. With the `choice is …` half gone, EVERY
        answer on an ended tab is read as Esc — a real profile pick included — so the chat
        the operator was trying to start a harness in is handed to `cmd_close` and forgotten
        for good: the mark, the transcript, the manifest entry and the window.

        Red without it: `closed == ["beta.1"]`, and `attempt` is never reached at all.
        """
        rc, closed, tried = self._picked(selector.Choice("claude"))

        self.assertEqual(rc, 0)
        self.assertEqual(closed, [], "a profile pick on an ended tab closed the tab")
        self.assertEqual(tried, [("claude", False)],
                         "the launch was never reached for the profile that was picked")

    def _a_conversation(self, fid: str):
        """Give *fid* a link and a transcript, so `resume_row` has something to offer."""
        path = Path(config.ROOT) / f"{fid}.jsonl"
        path.write_text("{}\n")
        state.record_harness_session(fid, "conv1")
        state.record_conversation(fid, str(path))
        return path

    def _resume_passed(self, *, ended: bool, fresh: bool):
        """The *resume* row `_select_in_pane` hands `selector.pick`."""
        seen: dict = {}

        def once(**kw):
            if seen:
                raise AssertionError("the selector came round instead of leaving")
            seen.update(kw)
            return selector.END_OF_INPUT

        with mock.patch.object(selector, "pick", side_effect=once), \
                mock.patch.object(commands_frame, "cmd_close"), \
                mock.patch.object(launcher, "_close_the_cancelled_chat"):
            launcher._select_in_pane(
                SimpleNamespace(start="claude", ended=ended, fresh=fresh, profile="",
                                rest=[], attended=True), "beta.1")
        return seen["resume"]

    def test_start_fresh_is_the_same_selector_with_the_resume_row_withheld(self):
        """`--fresh` is what the crash drawer's *start fresh* respawns into. The row is
        SUPPRESSED, not the conversation forgotten — the link stays recorded and the next
        exit offers it again — so collapsing the conditional either way is wrong in a
        different direction: always offer, and *start fresh* silently resumes.
        """
        self._a_conversation("beta.1")

        self.assertIsNone(self._resume_passed(ended=True, fresh=True))
        self.assertIsNotNone(self._resume_passed(ended=True, fresh=False))

    def test_a_selector_no_harness_put_here_carries_no_resume_row(self):
        """The other half of the same expression, and the one the ordinary path depends on:
        resume is the ENDED tab's row. A pane that never started a harness has a chat id and
        may well have a conversation recorded against it, so a selector that asked for one
        regardless would offer *resume* on a tab where nothing has ever run."""
        self._a_conversation("beta.1")

        self.assertIsNone(self._resume_passed(ended=False, fresh=False))

    def test_a_pane_that_never_started_still_closes_on_both(self):
        """The pin for the other side of the branch, as #1103 left it: nothing ran in that
        pane, so there is no chat to keep and both answers close its window."""
        state.clear_ended("beta.1")
        for answer in (selector.KEY_CANCEL, selector.END_OF_INPUT):
            with self.subTest(answer=answer):
                closed: list = []
                cancelled: list = []
                with mock.patch.object(selector, "pick", return_value=answer), \
                        mock.patch.object(commands_frame, "cmd_close",
                                          side_effect=lambda a: closed.append(a.chat_id)), \
                        mock.patch.object(launcher, "_close_the_cancelled_chat",
                                          side_effect=lambda f: cancelled.append(f)):
                    rc = launcher._select_in_pane(
                        SimpleNamespace(start="claude", ended=False, fresh=False,
                                        profile="", rest=[], attended=True), "beta.1")
                self.assertEqual(rc, selector.CANCELLED_EXIT)
                self.assertEqual(cancelled, ["beta.1"])
                self.assertEqual(closed, [], "a pane that never started wrote a closed mark")


class QuitAndReopenKeepEndedTabs(PersonaIso, unittest.TestCase):
    """Decision 12: a quit records ended-but-open tabs, with their link, so `charter reopen`
    brings them back ended and resumable.

    A quit is the one record that cannot be redone — the chats it describes are dead and
    their directories reaped — so an ended tab left out of it is a chat the operator loses
    by quitting, which is the outcome this whole design exists to prevent.
    """

    def setUp(self) -> None:
        super().setUp()
        _plant("beta.1")
        _plant("beta.2")
        state.claim_ended("beta.2")

    def _plan(self):
        return leave.plan(live={"beta.1", "beta.2"}, focus="beta")

    def _recorded(self, **kw) -> dict:
        commands_frame._record_the_plane(self._plan().chats, focus="beta",
                                         active=set(), **kw)
        return json.loads(reopen.path().read_text())

    def test_a_quit_records_an_ended_tab_as_ended(self):
        raw = self._recorded(windows={})

        by_id = {c["chat"]: c for f in raw["frames"] for c in f["chats"]}
        self.assertFalse(by_id["beta.1"]["ended"])
        self.assertTrue(by_id["beta.2"]["ended"])

    def test_a_quit_captures_no_ended_tab(self):
        """That pane is charter's selector or its drawer, never the harness's screen — so a
        capture would replace the harness's last words with a picture of charter asking what
        to do about them, on exactly the tab whose transcript is most wanted."""
        windows = {SERVER: {"beta.1": "@1", "beta.2": "@2"}}

        with mock.patch.object(commands_frame, "_capture_transcript",
                               return_value=True) as captured:
            self._recorded(windows=windows)

        asked = [call.args[1] for call in captured.call_args_list]
        self.assertEqual(asked, ["%1"], "the ended tab's pane was captured")

    def test_an_ended_tab_still_names_the_transcript_it_already_had(self):
        """Not capturing is not forgetting: the tab keeps the transcript its harness left,
        which is the one an operator would open."""
        dest = reopen.transcript_path("beta.2")
        dest.write_text("what the harness last said\n")

        raw = self._recorded(windows={})

        by_id = {c["chat"]: c for f in raw["frames"] for c in f["chats"]}
        self.assertEqual(by_id["beta.2"]["transcript"], dest.name)

    def test_the_confirmation_says_when_the_conversation_is_still_on_offer(self):
        """The other half of the ended tab's quit note. `test_the_confirmation_says_it_comes
        _back_ended` below pins the *nothing to resume* answer; nothing pinned the one an
        operator sees far more often — a tab whose conversation is still there.

        Collapsed to either half, the sentence stops describing the chat it is about: an
        operator reads *nothing to resume* over a conversation that is on offer, or is
        promised a resume for a transcript that is gone.
        """
        kept = Path(config.ROOT) / "beta.2.jsonl"
        kept.write_text("{}\n")
        state.record_harness_session("beta.2", "conv2")
        state.record_conversation("beta.2", str(kept))

        doomed = {c.chat: c for c in self._plan().chats}
        note = leave.note(doomed["beta.2"])

        self.assertIn("comes back ended", note)
        self.assertIn("resume offered", note)

    def test_the_confirmation_says_it_comes_back_ended(self):
        """`already ended on its own (N)` was about an exit code. What an operator needs
        before pressing quit is what they get BACK, which is a different sentence."""
        doomed = {c.chat: c for c in self._plan().chats}

        note = leave.note(doomed["beta.2"])

        self.assertIn("comes back ended", note)
        self.assertIn("nothing to resume", note)

    def test_the_manifests_ended_field_survives_the_round_trip(self):
        """**The wire key, spelled twice and by two different halves.** The writer emits
        `Chat._asdict()`, so the field name comes off the NamedTuple; the reader spells it
        by hand (`raw.get("ended")`). A retune of the reader's literal is silent — every
        restored tab comes back not-ended, which starts a harness on a tab whose whole
        purpose was to wait — and `test_a_record_from_before_this_field_reads_as_not_ended`
        below cannot see it, because that case asserts the absent-key direction, which a
        retuned key gives too.
        """
        kept = reopen._chat({"chat": "beta.1", "workspace": "beta", "persona": "",
                             "harness": "claude-code", "cwd": "", "resume": "",
                             "transcript": "", "profile": "claude", "brief": "",
                             "conversation": "", "ended": True})

        self.assertTrue(kept.ended)
        self.assertIn("ended", reopen.Chat._fields,
                      "the writer's field and the reader's literal have come apart")

    def test_the_manifests_active_field_survives_the_round_trip_too(self):
        """`active` is the same hand-spelled wire key beside `ended`, and it decides where
        the operator LANDS: `_attach_after_reopen` picks the chat whose record says it had
        the client when the quit ran. Retuned, every restored chat reads as inactive and the
        reopen falls through to the first chat of the focus workspace — somewhere the
        operator was not.
        """
        raw = {"chat": "beta.1", "workspace": "beta", "persona": "",
               "harness": "claude-code", "cwd": "", "resume": "", "transcript": "",
               "profile": "claude", "brief": "", "conversation": ""}

        self.assertTrue(reopen._chat({**raw, "active": True}).active)
        self.assertFalse(reopen._chat(raw).active)
        self.assertIn("active", reopen.Chat._fields,
                      "the writer's field and the reader's literal have come apart")

    def _record(self, *, ended: bool):
        return reopen.Chat(chat="beta.7", workspace="beta", persona="",
                           harness="claude-code", cwd="", resume="u", transcript="",
                           active=False, profile="claude", brief="", conversation="",
                           ended=ended)

    def test_a_reopen_brings_an_ended_chat_back_at_the_ended_selector(self):
        """It reopens at its OWN selector and starts no harness: nothing runs until somebody
        switches to that tab and presses Enter. That is what keeps `argv_select`'s rule —
        a selector is a question, so only an open somebody is in front of may reach one —
        true of the one selector an open nobody is at may create."""
        args = commands_frame._reopen_args(self._record(ended=True), harness_name="claude",
                                           profile="claude", reopening=None, resume=False)

        self.assertTrue(args.select)
        self.assertTrue(args.ended)
        self.assertEqual(args.start, "claude")
        self.assertFalse(args.fresh, "a restored tab withheld its own resume row")

    def test_a_reopen_brings_a_running_chat_back_on_its_profile(self):
        """The control: a chat that was RUNNING when the quit recorded it reopens on its
        profile exactly as it always has, with no selector in front of it."""
        args = commands_frame._reopen_args(self._record(ended=False), harness_name="claude",
                                           profile="claude", reopening=None, resume=True)

        self.assertFalse(args.select)
        self.assertFalse(args.ended)
        self.assertEqual(args.profile, "claude")
        self.assertTrue(args.resume)

    def test_a_record_from_before_this_field_reads_as_not_ended(self):
        """The migration case this reader is built for: a manifest one field older. Every
        chat in it was running, which is the reading that brings a conversation back."""
        older = reopen._chat({"chat": "beta.1", "workspace": "beta", "persona": "",
                              "harness": "claude-code", "cwd": "", "resume": "",
                              "transcript": "", "profile": "claude", "brief": "",
                              "conversation": ""})

        self.assertFalse(older.ended)


class TheRecordIsWrittenDown(PersonaIso, unittest.TestCase):
    """**Ruling 44: the decision is written down where the next reader will look.**

    This branch honours it in the diff and nothing guards it, which is how a dated note
    quietly stops being true — the words are edited, the code is not, and the two drift
    without a single test going red.

    The files are read off disk **from the repository root**, never through `config`:
    `tests/_isolation.py` points that at a throwaway plane, so a case asserting against this
    repository's own committed text has to reach for the checkout itself. That is
    CONTRIBUTING's rule and `test_frame_config._COMMITTED`'s shape.

    Substrings rather than whole paragraphs, because what is pinned is the CLAIM — that the
    amendment exists, that its two bounds are stated, that the vocabulary has the word, and
    that the spec section says when it was built — and not the prose around it, which should
    stay free to be rewritten.
    """

    REPO = Path(__file__).resolve().parent.parent

    def _read(self, rel: str) -> str:
        p = self.REPO / rel
        self.assertTrue(p.is_file(), f"{rel} is not in this checkout")
        return p.read_text()

    def test_adr_0018_carries_the_amendment_and_its_two_bounds(self):
        """The ADR is the boundary this whole module keeps: a harness exit stops being
        final, charter still starts nothing by itself, and it still touches no pane a
        listing has not proved."""
        text = self._read(
            "docs/adr/0018-charter-may-run-the-harness-but-never-draws-it.md")

        self.assertIn("a harness exit is no longer final", text)
        self.assertIn("Charter restarts nothing by itself", text)
        self.assertIn("Charter acts on a pane only as a listing proves it", text)

    def test_the_vocabulary_names_an_ended_tab(self):
        """`CONTEXT.md` holds the words charter uses. A feature whose central noun is not in
        it is one the next reader has to infer from code."""
        self.assertIn("**Ended tab**:", self._read("CONTEXT.md"))

    def test_the_ide_spec_says_when_its_section_was_built(self):
        """§4j's dated note. Undated, a spec section reads as a plan rather than as
        something that shipped, and nobody can tell which."""
        self.assertIn("Built 2026-09-15", self._read(
            "docs/superpowers/specs/2026-08-30-charter-opens-like-an-ide.md"))


if __name__ == "__main__":
    unittest.main()
