"""Everything inside a frame answers about the FRAME's workspace (#526, #524).

#512 established the shape and #525 built the mechanism: a frame is launched FOR a
workspace, nothing inside it can re-derive which one, so the launcher writes it down
(`frame/state.record_workspace`) and every surface reads it back. `workspace.resolve`'s
rungs are all dead inside a frame — `$CHARTER_WORKSPACE` arrives empty by design (#411),
the cwd is the plane root, the per-session pointer is keyed on the FRAME id, and the
per-terminal pointer on the asking pane's own `$TMUX_PANE`, which charter created — so a
process that asks for itself lands on the declared default.

Two things were still asking for themselves:

* **The sidebar's todos (#526).** `slots.todo_section` called `gather.read(fid)` with no
  workspace, and on a cold cache `gather.read` falls through to `gather.scan`, which
  resolves one from the panel process. A frame's first paint listed **`default`'s** open
  todos under this plane's `todos N` heading. Worse than #512's blank repo table, because
  a populated list reads as an answer: three todos an operator has never seen read as
  three todos they have.
* **The harness pane (#524).** The agent's own shell reaches the same dead rungs, so the
  frame could correctly draw `harness-wrapper` while every command typed inside it acted
  on `default`.

**The reconciliation is one ladder, and its ORDER is the decision #524 says charter
cannot leave silent.** A choice made inside the frame outranks the launch (`charter
workspace use` writes the per-session pointer under the frame's id, and "it moves the
panels too" is a documented promise); the launch outranks the two rungs that answer for
the ASKING PROCESS rather than for the frame. So both directions of #524's question are
answered, and neither silently: the frame follows the session when the session chose, and
the session follows the frame when it did not.

**Not a hook**, which is where #524 expected this to land. Its own third constraint rules
that out: a non-Claude harness has no session-start hook at all, and neither does a bare
`charter ws current` typed into the frame's shell. A rung in the ladder needs no harness
cooperation.
"""

from __future__ import annotations

import os
import sys
import unittest
from unittest import mock

from charter import config, session, statusline, todos, tui, workspace
from charter.frame import gather, slots, state
from tests._isolation import PersonaIso

#: The workspace the FRAME was launched for, and the one nothing inside it can re-derive.
FRAMED = "harness-wrapper"
#: What a process inside the frame resolves when it asks for itself — the answer #512
#: measured on the reporting plane, and the wrong one.
OTHER = "user-reporting"


class FramedCase(PersonaIso):
    """A frame launched for :data:`FRAMED`, on a plane where every rung a panel or a
    harness can actually reach names :data:`OTHER` instead.

    That is not a contrived plane. It is the one #524 measured: three per-terminal
    pointers naming one workspace, one naming another, and a `default` with no clones in
    it. The fixture's whole job is that resolving-for-yourself and reading-the-record give
    DIFFERENT answers — a fixture where they agree tests nothing at all.
    """

    FID = "harness_wrapper-4242"

    def setUp(self) -> None:
        super().setUp()
        for name in (FRAMED, OTHER):
            workspace.ensure(name)
            workspace.scaffold(name)
        state.record_workspace(self.FID, FRAMED)
        # The rung a process inside the frame WOULD reach: a per-terminal pointer, keyed
        # on the pane it happens to be in. Written directly rather than through
        # `set_active`, which also writes the per-session pointer — and the per-session
        # pointer is the rung that is allowed to win.
        self.pane = "%17"
        config.private_mkdir(config.TERMINALS_DIR)
        # Keyed through `session.terminal`, never through the raw variable: the id is
        # sanitised on its way to becoming a filename (`%17` is stored as `-17`), and a
        # fixture that wrote the raw spelling would leave the pointer unreadable and the
        # rung it stands for silently absent — a fixture failing to build the very
        # disagreement the file is about.
        tid = session.terminal(self.pane)
        (config.TERMINALS_DIR / f"{tid}.workspace").write_text(OTHER)
        self.enterContext(mock.patch.dict(os.environ, {"TMUX_PANE": self.pane}))

    def inside_the_frame(self):
        """`$CHARTER_SESSION_ID` holds the FRAME's id — that is what `session.current`
        answers inside a frame (ADR 0019), and it is the whole channel through which the
        launcher's record is addressable from in there."""
        return mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID})

    def assert_the_fixture_still_disagrees(self) -> None:
        """The guard that keeps every case below meaningful.

        If the plane's own rungs happened to answer :data:`FRAMED` too, every assertion
        here would pass with the fix reverted — the shape #588 names, a fixture agreeing
        with the machine's default. So this asserts the DISAGREEMENT exists before the
        frame is asked anything.
        """
        self.assertEqual(workspace.resolve(), OTHER,
                         "fixture: outside the frame this plane must resolve something "
                         "OTHER than the frame's workspace, or nothing here can fail")


class TheHarnessAnswersForTheFrameItRunsIn(FramedCase):
    """#524 — the pane the agent types in."""

    def test_a_process_inside_the_frame_resolves_the_frames_workspace(self):
        self.assert_the_fixture_still_disagrees()
        with self.inside_the_frame():
            self.assertEqual(workspace.resolve(), FRAMED)

    def test_it_says_the_frame_is_what_decided(self):
        """`source` must mirror `chosen`'s rungs or the status line explains the active
        workspace by naming a rung that did not decide it — this module's own standing
        rule, and the reason the two walkers are kept in step."""
        with self.inside_the_frame():
            self.assertEqual(workspace.source(), "frame")

    def test_a_choice_typed_inside_the_frame_still_wins(self):
        """The direction #517 asks for, and the promise `charter ws use` already makes.
        The launcher's answer is a SEED, never a pin: pinning the session to the frame is
        the option #524 weighs and rejects, because it would take `ws use` away from every
        framed session."""
        with self.inside_the_frame():
            workspace.set_active(OTHER, terminal_id="")
            self.assertEqual(workspace.resolve(), OTHER)
            self.assertEqual(workspace.source(), "session")

    def test_the_pin_still_outranks_the_record(self):
        """`$CHARTER_WORKSPACE` means the same thing inside a frame as everywhere else —
        `state.workspace_for`'s rung 0 says so, and this rung sits far below it."""
        with self.inside_the_frame(), \
             mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": OTHER}):
            self.assertEqual(workspace.resolve(), OTHER)

    def test_the_tree_you_are_standing_in_still_outranks_the_record(self):
        """The cwd rung cannot be wrong — a workspace's trees live at paths that name it
        — so a harness launched from inside a clone keeps answering for that clone."""
        clone = workspace.workspace_dir(OTHER) / "svc"
        clone.mkdir(parents=True, exist_ok=True)
        with self.inside_the_frame():
            self.assertEqual(workspace.resolve(cwd=clone), OTHER)

    def test_outside_a_frame_nothing_changes(self):
        """The control, and the one that says this rung cannot leak. A session id that
        names no frame has no record to read, so the ladder is the ladder it was."""
        with mock.patch.dict(os.environ,
                             {"CHARTER_SESSION_ID": "a-claude-conversation-uuid"}):
            self.assertEqual(workspace.resolve(), OTHER)
            self.assertEqual(workspace.source(), "terminal")

    def test_a_frame_whose_record_predates_the_mechanism_falls_through(self):
        """A frame launched by a charter that never wrote the file, still running across
        the upgrade. It must degrade to today's answer rather than to an exception on the
        path every command takes to ask where it is."""
        state.frame_dir(self.FID, create=True).joinpath("workspace").unlink()
        with self.inside_the_frame():
            self.assertEqual(workspace.resolve(), OTHER)

    def test_a_corrupt_record_names_no_workspace(self):
        """The record is charter's own file, but it lands in `workspace_dir()`'s join and
        on a panel's screen, and #442 is what an unchecked `../../` in that position cost
        once. `state.frame_workspace` owns the name check; this is the assertion that this
        rung actually goes through it."""
        state.frame_dir(self.FID, create=True).joinpath("workspace").write_text("../../esc")
        with self.inside_the_frame():
            self.assertEqual(workspace.resolve(), OTHER)

    def test_the_frame_and_its_panels_now_give_the_same_answer(self):
        """The point of the whole thing, stated as the two halves meeting. `workspace_for`
        is what every PANEL asks and `resolve` is what the harness asks; #524 is that the
        two could disagree."""
        with self.inside_the_frame():
            self.assertEqual(state.workspace_for(self.FID), workspace.resolve())

    def test_asking_costs_no_exception_when_the_state_directory_is_unreadable(self):
        """This runs on `resolve`, which every command and every status-line render calls.
        It reports what it can read and never becomes the reason a command cannot run."""
        with self.inside_the_frame(), \
             mock.patch.object(state, "frame_workspace",
                               side_effect=OSError("no state dir")):
            self.assertEqual(workspace.resolve(), OTHER)


class TheSidebarsTodosAreTheFramesTodos(FramedCase):
    """#526 — the section that reads as an answer whether or not it is one."""

    #: SHORT titles, and that is the whole of why they are short. The pane is 44 columns
    #: and `_todo_rows` truncates a row to fit it — so a descriptive title like "a todo
    #: from a workspace nobody in this frame is looking at" renders as
    #: `- a todo from a workspace nobody in this fr…`, and the `assertNotIn` below then
    #: passes against the defect it exists to catch because the string it looks for was
    #: never going to be on the row whole. Measured: it did, on `origin/main`.
    MINE = "FRAMES-OWN-TODO"
    THEIRS = "OTHER-WORKSPACES-TODO"

    def setUp(self) -> None:
        super().setUp()
        todos.add(FRAMED, self.MINE)
        todos.add(OTHER, self.THEIRS)
        gather.discard(self.FID)          # what `cmd_launch` runs before it draws

    def assert_a_todo_row_is_not_truncated(self, lines: list[str]) -> None:
        """No todo row ended in the kit's ellipsis.

        Without this, an `assertNotIn` for a title the pane was going to cut anyway is an
        assertion that cannot fail — and `assertIn` for one is a case that fails for the
        wrong reason. Either way the pane's width, not the workspace, decides the result.
        """
        cut = [ln for ln in lines if ln.startswith("- ") and ln.endswith(tui.ELLIPSIS)]
        self.assertEqual(cut, [], f"a todo row was cut by the pane, so a membership "
                                  f"assertion about its title decides nothing:\n{cut}")

    def sidebar(self, *, cols=44, rows=26) -> list[str]:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, rows))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            out = slots.render("right", self.FID)
        return [tui.strip_ansi(ln) for ln in out.split("\n")]

    def test_a_cold_first_paint_lists_the_frames_todos(self):
        self.assert_the_fixture_still_disagrees()
        lines = self.sidebar()
        self.assert_a_todo_row_is_not_truncated(lines)
        self.assertIn(f"- {self.MINE}", lines)

    def test_it_never_lists_another_workspaces_todos(self):
        """The half that matters more. An empty column reads as "nothing to do"; a
        POPULATED one reads as an answer, and an operator would take three todos they
        have never seen as three todos they have."""
        lines = self.sidebar()
        self.assert_a_todo_row_is_not_truncated(lines)
        self.assertNotIn(self.THEIRS, "\n".join(lines), "\n".join(lines))

    def test_the_heading_counts_the_frames_workspace(self):
        self.assertIn(f"{statusline._HEAD_PAD}todos 1", self.sidebar())

    def test_the_scan_fallback_is_still_there(self):
        """**A decision, not an inheritance.** #525 took the cold-cache scan away from
        `bottom` and gave it a `⋯ gathering…` placeholder, because `bottom` is the one
        ANIMATED slot — five repaints a second, each a git sweep. The sidebar is not
        animated: one scan, one paint, and a column that draws its todos immediately is
        worth having. Pinned so the difference stays deliberate rather than drifting into
        "the two slots happen to differ"."""
        with mock.patch.object(gather, "scan", wraps=gather.scan) as scanned:
            self.sidebar()
        self.assertTrue(scanned.called,
                        "the cold-cache scan was dropped — a first paint now says this "
                        "workspace has no todos, which is #526's own worse reading")

    def test_the_scan_is_asked_for_the_frames_workspace_by_name(self):
        """Not "the answer happened to be right". The gather is handed the workspace, and
        that is the property — a `scan()` left to resolve one for itself is the defect
        whatever it lands on, because on the next plane it lands somewhere else."""
        with mock.patch.object(gather, "scan", wraps=gather.scan) as scanned:
            self.sidebar()
        self.assertEqual([c.kwargs.get("workspace") for c in scanned.call_args_list],
                         [FRAMED] * scanned.call_count, scanned.call_args_list)

    def test_a_warm_cache_still_never_touches_the_todo_directory(self):
        """The idle-cost rule this section already kept (#387): `todos.open_todos` is one
        file read per todo and a panel repaints on every version bump. Re-asked because
        this change moved the call, and the cheapest way to get the workspace right would
        have been to read the directory here.

        The cache is SEEDED rather than warmed by a first render: `gather.read` scans on a
        miss and does not save what it scanned, so a test that rendered twice would be
        making two cold scans and pinning nothing about the cache at all."""
        gather.save(self.FID, gather.scan(workspace=FRAMED))
        with mock.patch.object(todos, "open_todos",
                               side_effect=AssertionError("read the workspace directory")):
            lines = self.sidebar()
        self.assert_a_todo_row_is_not_truncated(lines)
        self.assertIn(f"- {self.MINE}", lines)


class TheSessionIdIsTheFramesId(FramedCase):
    """Why the record is addressable from inside the frame at all.

    Recorded because it is the load-bearing and least obvious fact in this whole file: the
    rung reads `state.frame_workspace(session.current())`, and that only finds anything
    because `$CHARTER_SESSION_ID` inside a frame holds the FRAME's id rather than the
    harness's own. If that ever stops being true, this rung answers `None` everywhere and
    every case above goes green while the defect comes back — so the fact is pinned here
    rather than assumed there.
    """

    def test_the_session_id_inside_a_frame_is_the_frame_id(self):
        with self.inside_the_frame():
            self.assertEqual(session.current(), self.FID)

    def test_the_record_is_keyed_on_exactly_that_id(self):
        self.assertEqual(state.frame_workspace(self.FID), FRAMED)

    def test_a_session_with_no_id_names_no_frame(self):
        """`for_frame`'s own precondition, asked of `for_frame`.

        A shell with no `$CHARTER_SESSION_ID` and no `$CLAUDE_CODE_SESSION_ID` reaches
        this rung with `None`, which is the ordinary case for anyone typing `charter ws
        current` in a plain terminal — the rung has to answer "no frame", not go looking
        for a directory named after nothing.

        Written because a deletion sweep deleted the refusal and reported that fifty-nine
        test modules execute this file and **not one of them names this function**. It
        happens to degrade the same way through `contain.child` today, which is exactly
        the kind of accident that stops being true when the other module changes.
        """
        for sid in (None, ""):
            with self.subTest(sid=sid):
                self.assertIsNone(workspace.for_frame(sid))

    def test_an_id_that_names_no_frame_answers_none(self):
        """The other half: a real session id that is simply not a frame's."""
        self.assertIsNone(workspace.for_frame("a-claude-conversation-uuid"))

    def test_a_second_frame_does_not_inherit_the_first_ones_record(self):
        """#411's protection, re-asked one rung down. The record is per-frame, so a frame
        that never recorded anything reads nothing — the failure mode a `$CHARTER_WORKSPACE`
        handed to the harness would have brought back."""
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "other_frame-99"}):
            self.assertEqual(workspace.resolve(), OTHER)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
