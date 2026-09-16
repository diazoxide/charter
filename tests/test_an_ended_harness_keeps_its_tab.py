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

import unittest
from pathlib import Path
from unittest import mock

from charter import config
from charter.frame import leave, overlay, palette, selector, state, tabmenu

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
        caller would have to parse out of a title."""
        def chose(surface, *_a, **_kw):
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


if __name__ == "__main__":
    unittest.main()
