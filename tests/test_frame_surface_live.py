"""The surface an operator can change without relaunching, and the floor it was measured
against — Phase 3 of `docs/superpowers/specs/2026-08-28-frame-visual-design.md`.

**Phase 3.5 is why this file exists at all.** §9 of that spec records that every
`window-style` measurement behind the surface was run on tmux 3.7c and on nothing else,
and that the floor (`tmuxctl.FLOOR`, 3.2) had to be run before the surface could ship.
It has been. tmux 3.2 was built from source on this machine and the whole of §1 and §4
was re-run against it, one style per server, with the SGR lifted out of the client's wire
rather than eyeballed out of forty bytes of context:

* ``set-option -p window-style`` is **pane-scoped** there. `show -p` on the sibling panel
  and on the harness pane both answer ``''`` after it, and so does `show -w` on the
  window — so the harness boundary ADR 0018 draws holds at the floor by the same
  construction it holds at 3.7c.
* It honours **colour only**. ``reverse``, ``dim`` and ``bold`` each put *no SGR at all*
  on an attached client's wire; ``bg=colour236,dim`` put ``ESC[48;5;236m`` and nothing
  else. Identical to 3.7c.
* The **sixteen ANSI names resolve**, all sixteen: ``bg=black`` → ``ESC[40m`` through
  ``bg=white`` → ``ESC[47m``, ``bg=brightblack`` → ``ESC[100m`` through
  ``bg=brightwhite`` → ``ESC[107m``. Identical to 3.7c.
* The active/inactive split, the format-expansion of a style value, the refusal of
  ``bg=notacolour`` (rc 1, ``invalid style:``, previous value intact), the refusal of
  ``#(...)`` by the style parser, and survival across `respawn-pane` and `resize-window`
  are all identical too. Sixty-six of sixty-eight answers matched byte for byte; the two
  that did not were the measurement harness's own batching — re-run one pane per server
  they matched as well.

**So there is no version gate, and the absence is asserted below rather than assumed.**
The spec said that if 3.2 differed, `chrome` would be gated at the version that works,
the way `display-popup` is gated at 3.3. It does not differ. A gate added anyway would be
a refusal with no measurement behind it, on the one path where a refusal is invisible —
`_surface_argvs`' failures are reported, not fatal.
"""

from __future__ import annotations

import contextlib
import io
import os
import pathlib
import subprocess
import unittest
from unittest import mock

from charter import commands_frame, instance
from charter.frame import builtin_actions, chrome as chrome_mod, state
from tests._isolation import PersonaIso
from tests.test_frame_tmux_integration import _HAS_TMUX, _TmuxServerFixture


class TheSurfaceIsNotVersionGated(unittest.TestCase):
    """Phase 3.5's answer, kept honest as code rather than only as prose.

    The measurement said the floor behaves identically, so `_surface_argvs` and
    `_resurface_argvs` ask tmux's version about nothing. A future edit that adds a floor
    has to delete this test and say which measurement it found.
    """

    def test_neither_surface_builder_needs_to_know_the_tmux_version(self):
        """Asked as behaviour rather than as a grep of the source: with
        `tmuxctl.version` raising, both builders still answer, because neither has any
        reason to call it. A gate added later fails here on the exception."""
        from charter.frame import tmuxctl
        with mock.patch.object(tmuxctl, "version",
                               side_effect=AssertionError("a version gate appeared — "
                                                          "tmux 3.2 was measured "
                                                          "identical to 3.7c, so a gate "
                                                          "needs its own measurement")):
            self.assertEqual(
                len(commands_frame._surface_argvs(socket="s", pane_id="%3",
                                                  chrome="dark")), 2)
            self.assertEqual(
                len(commands_frame._resurface_argvs(socket="s", pane_id="%3",
                                                    chrome="off")), 2)

    def test_the_version_control_can_actually_fire(self):
        """The control for the test above: a builder that DID ask would fail there, so
        this proves the patched `version` raises rather than being quietly unused."""
        from charter.frame import tmuxctl
        with mock.patch.object(tmuxctl, "version", side_effect=AssertionError("boom")):
            with self.assertRaises(AssertionError):
                tmuxctl.version()

    def test_the_floor_is_still_the_version_this_was_measured_against(self):
        """If the floor moves, the measurement above is about a version charter no
        longer supports and has to be re-run rather than inherited."""
        from charter.frame import tmuxctl
        self.assertEqual(tmuxctl.FLOOR, (3, 2))


class TheRecordedSurfaceOverridesTheConfiguredOne(PersonaIso, unittest.TestCase):
    """`state.record_chrome` / `state.chrome` / `commands_frame._current_chrome`.

    `record_density`'s twin, and asserted separately rather than trusted to be: the two
    read different files and only one of them is on its way to a tmux style.
    """

    FID = "fr-surface-1"

    def test_a_recorded_word_round_trips(self):
        state.record_chrome(self.FID, "dark")
        self.assertEqual(state.chrome(self.FID), "dark")

    def test_nothing_recorded_reads_back_as_none(self):
        """`None` is "never set", which is every frame until a keypress — and it must be
        distinguishable from `off`, or the fallback to the configured value could never
        happen."""
        self.assertIsNone(state.chrome(self.FID))

    def test_the_record_wins_over_the_configured_value(self):
        state.record_chrome(self.FID, "light")
        with mock.patch.dict(commands_frame.config.FRAME, {"chrome": "dark"}):
            self.assertEqual(commands_frame._current_chrome(self.FID), "light")

    def test_with_nothing_recorded_the_configured_value_is_used(self):
        with mock.patch.dict(commands_frame.config.FRAME, {"chrome": "dark"}):
            self.assertEqual(commands_frame._current_chrome(self.FID), "dark")

    def test_with_neither_the_answer_is_off(self):
        with mock.patch.dict(commands_frame.config.FRAME, {}, clear=True):
            self.assertEqual(commands_frame._current_chrome(self.FID), "off")

    def test_a_word_charter_does_not_know_is_off_from_either_source(self):
        """The gate is at the point of USE and it gates BOTH sources. A recorded file is
        machine-written but hand-editable; a committed config arrives from someone else's
        machine. Neither is allowed to be the word that reaches `chrome_options`."""
        state.record_chrome(self.FID, "bg=#{?#{==:1,1},colour196,colour46}")
        with mock.patch.dict(commands_frame.config.FRAME, {"chrome": "dark"}):
            self.assertEqual(commands_frame._current_chrome(self.FID), "dark")
        with mock.patch.dict(commands_frame.config.FRAME, {"chrome": ["dark"]}):
            self.assertEqual(commands_frame._current_chrome(self.FID), "off")

    def test_a_record_that_cannot_be_READ_degrades_rather_than_raising(self):
        """`_current_chrome` is called from `_split_panels`, which is on the launch path:
        a file charter cannot read must cost the surface, never the frame.

        The READ is what fails here, not a stand-in for it. Patching `state.chrome` to
        raise and then asserting that it raises is an assertion about the mock — the
        version of this test that shipped first did exactly that and would have stayed
        green with the `except OSError` deleted.
        """
        state.record_chrome(self.FID, "dark")
        real = pathlib.Path.read_text

        def refuse(self_path, *a, **kw):
            if self_path.name == "chrome":
                raise OSError("unreadable")
            return real(self_path, *a, **kw)

        with mock.patch.object(pathlib.Path, "read_text", refuse):
            self.assertIsNone(state.chrome(self.FID))
            with mock.patch.dict(commands_frame.config.FRAME, {"chrome": "light"}):
                self.assertEqual(commands_frame._current_chrome(self.FID), "light")
        # And the control: with the read working, the record is what is used — so the
        # assertion above is about the refusal and not about a fixture that never wrote.
        with mock.patch.dict(commands_frame.config.FRAME, {"chrome": "light"}):
            self.assertEqual(commands_frame._current_chrome(self.FID), "dark")


class TheTwoNewStateAccessorsRefuseRatherThanRaise(PersonaIso, unittest.TestCase):
    """The four guards the deletion sweep found unpinned in `record_chrome` / `chrome`.

    All four are live and none of them had a test: the sweep deleted each in turn and the
    whole 7037-test suite stayed green. They are not decoration — `$CHARTER_SESSION_ID`
    reaches both functions without ever passing through `state.frame_id`'s minting
    (`cmd_chrome` reads the variable directly), so a hostile or oversized id arrives here
    as an ordinary argument, and `frame_dir` answers `None` for it. Without the guards
    that is a `TypeError` from `None / "chrome"` — raised out of a command that runs
    detached from a palette row, where nothing would ever print it.

    **Each test names WHICH refusal fired**, per the deletion sweep: two guards in
    sequence mask each other, and "it did not raise" is satisfied by both.
    """

    #: A frame id `contain.child` refuses outright, so `frame_dir` answers `None` — the
    #: exact input `$CHARTER_SESSION_ID` can carry.
    HOSTILE = "../../../etc"

    def test_recording_against_an_unusable_frame_id_writes_nothing(self):
        state.record_chrome(self.HOSTILE, "dark")
        # WHICH refusal: `frame_dir` answered None, so nothing was written anywhere —
        # asserted as the absence of a readable value rather than as "no exception".
        self.assertIsNone(state.chrome(self.HOSTILE))

    def test_reading_an_unusable_frame_id_answers_none_rather_than_raising(self):
        self.assertIsNone(state.chrome(self.HOSTILE))
        with mock.patch.dict(commands_frame.config.FRAME, {"chrome": "dark"}):
            self.assertEqual(commands_frame._current_chrome(self.HOSTILE), "dark",
                             "the read raised instead of degrading, so a frame launched "
                             "with a hostile $CHARTER_SESSION_ID takes the launch down "
                             "rather than losing its surface")

    def test_a_write_that_fails_is_a_surface_lost_and_not_an_exception(self):
        """`record_chrome` is called from `cmd_chrome`, which runs detached from a
        palette row. A full disk there must cost the surface, not the command.

        **Injected at `config.write_for`, not at `pathlib.Path.write_text`.** This case was
        written against the latter, which pinned it to the writer's SPELLING rather than to
        the must-not-raise property it is named for: routing `record_chrome` through the
        state-file dispatch (#582) left the mock aimed at a call nobody makes, the write
        succeeded, and the case went red having found nothing. `config.write_for` is what
        `frame/state.py` depends on, so that is where a failing filesystem goes in — the
        same repoint `test_frame_state` and `test_component_toggle_keys` needed.
        """
        real = state.config.write_for

        def refuse(path, data):
            if pathlib.Path(path).name == "chrome.tmp":
                raise OSError("no space left on device")
            return real(path, data)

        with mock.patch.object(state.config, "write_for", refuse):
            state.record_chrome("fr-write-fails", "dark")
        # WHICH refusal: the write was attempted and refused, so nothing was recorded.
        self.assertIsNone(state.chrome("fr-write-fails"))
        # And the control — with the write working, the same call DOES record, so the
        # assertion above is about the refusal and not about a fixture that never wrote.
        state.record_chrome("fr-write-fails", "dark")
        self.assertEqual(state.chrome("fr-write-fails"), "dark")

    def test_an_empty_recorded_file_is_never_set_rather_than_a_word(self):
        """`None` and `""` are different answers to "has this frame been set by hand",
        and this function's docstring promises the first. Nothing downstream can tell
        them apart today — `chrome_level("")` is `None` either way — so the promise is
        the reason the line is here, and a promise nothing checks is what the sweep
        exists to find."""
        state.record_chrome("fr-empty", "")
        self.assertIsNone(state.chrome("fr-empty"))
        state.record_chrome("fr-empty", "   ")
        self.assertIsNone(state.chrome("fr-empty"))
        # The control: a real word is not flattened by the same line.
        state.record_chrome("fr-empty", "light")
        self.assertEqual(state.chrome("fr-empty"), "light")


class TurningTheSurfaceOffIsARemovalAndNotAStyle(unittest.TestCase):
    """`_resurface_argvs` — the argv, before any tmux runs it.

    The whole difference from `_surface_argvs`. On a launch `off` is free because nothing
    is set; on a running frame it has to UNSET, or the palette row that says `off` reports
    success and leaves the surface exactly where it was.
    """

    def _argvs(self, chrome):
        return commands_frame._resurface_argvs(socket="s", pane_id="%3", chrome=chrome)

    def _tails(self, chrome):
        """Each argv from `set-option` onwards, so the assertions are about what tmux is
        told and not about `tmuxctl.server_argv`'s prefix."""
        return [a[a.index("set-option"):] for a in self._argvs(chrome)]

    def test_off_unsets_every_option_the_table_can_set_and_sets_none(self):
        tails = self._tails("off")
        self.assertEqual(tails, [["set-option", "-p", "-u", "-t", "%3", name]
                                 for name in instance.chrome_option_names()])

    def test_dark_sets_both_options_and_unsets_nothing(self):
        tails = self._tails("dark")
        self.assertEqual(tails, [["set-option", "-p", "-t", "%3", name, value]
                                 for name, value in instance.FRAME_CHROME["dark"]])

    def test_a_word_charter_does_not_know_is_off_here_too(self):
        """The refusal is named: the argv is the UNSET list, which is `off`'s answer —
        not an empty list, which would leave a surfaced pane surfaced."""
        for hostile in ("bg=black", "", None, ["dark"], {"a": 1}, 3):
            with self.subTest(value=hostile):
                self.assertEqual(self._tails(hostile), self._tails("off"))

    def test_no_operator_string_reaches_tmux(self):
        """The containment boundary, asserted on this path as well as the launch one: a
        style charter did not write itself cannot leave through here."""
        argvs = self._argvs("bg=#{?#{==:1,1},colour196,colour46}")
        for argv in argvs:
            for word in argv:
                self.assertNotIn("#", word)

    def test_no_colour_unsets_rather_than_answering_nothing(self):
        """`NO_COLOR` on the LIVE path is the half that is easy to get wrong. Answering
        `[]` — which is what the launch path correctly answers — would leave a frame that
        was surfaced before the variable was exported still surfaced."""
        with mock.patch.dict(os.environ, {"NO_COLOR": ""}, clear=True):
            self.assertEqual(self._tails("dark"), self._tails("off"))
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertNotEqual(self._tails("dark"), self._tails("off"))

    def test_the_unset_list_is_derived_from_the_table(self):
        """`instance.chrome_option_names` is the union of the table's option names, so a
        third option added to `FRAME_CHROME` is unset by `off` on the day it is added
        instead of surviving a keypress nobody re-read."""
        with mock.patch.dict(instance.FRAME_CHROME,
                             {"dark": (("window-style", "bg=black"),
                                       ("pane-border-style", "fg=black"))}):
            self.assertIn("pane-border-style", instance.chrome_option_names())
            self.assertIn(["set-option", "-p", "-u", "-t", "%3", "pane-border-style"],
                          self._tails("off"))


class TheChromeCommandChangesOneRunningFrame(PersonaIso, unittest.TestCase):
    """`charter frame-chrome <level>` — `cmd_chrome`.

    Every refusal is a quiet no-op returning 0, and each is asserted by WHAT DID NOT
    HAPPEN — nothing recorded, no argv issued — rather than by the exit code, which every
    refusal shares with success.
    """

    FID = "fr-surface-2"

    def setUp(self):
        super().setUp()
        self.ran: list[list[str]] = []
        patcher = mock.patch.object(
            commands_frame.tmuxctl, "run",
            side_effect=lambda _what, argv, **kw: self.ran.append(argv) or
            subprocess.CompletedProcess(argv, 0, "", ""))
        patcher.start()
        self.addCleanup(patcher.stop)

    def _args(self, level):
        return type("A", (), {"level": level})()

    def _with_panes(self, panes=None):
        state.record_panes(self.FID, panels=panes if panes is not None
                           else {"top": "%1", "right": "%2"})

    def test_a_level_is_recorded_and_every_panel_pane_is_told(self):
        self._with_panes()
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True):
            self.assertEqual(commands_frame.cmd_chrome(self._args("dark")), 0)
        self.assertEqual(state.chrome(self.FID), "dark")
        targets = {a[a.index("-t") + 1] for a in self.ran}
        self.assertEqual(targets, {"%1", "%2"})
        self.assertTrue(all("window-style" in a or "window-active-style" in a
                            for a in self.ran))

    def test_off_issues_the_unsets(self):
        self._with_panes({"top": "%1"})
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True):
            commands_frame.cmd_chrome(self._args("off"))
        self.assertEqual(state.chrome(self.FID), "off")
        self.assertTrue(all("-u" in a for a in self.ran), self.ran)

    def test_no_session_id_says_so_and_still_runs_nothing(self):
        """Records nothing, runs nothing — and, since #734, says so and exits non-zero.

        `charter frame-chrome dark` typed in an ordinary shell used to print zero bytes and
        report success. Separated from the level check below because they are two different
        refusals: this one is an operator who is not where they think they are, and the
        surface that is certainly theirs is their own stderr; that one is a word outside a
        closed set, answered inside a frame where a `run-shell` child's non-zero status
        would print into the harness pane."""
        self._with_panes()
        err = io.StringIO()
        with mock.patch.dict(os.environ, {}, clear=True), \
             contextlib.redirect_stderr(err):
            self.assertNotEqual(commands_frame.cmd_chrome(self._args("dark")), 0)
        self.assertIn("charter frame-chrome", err.getvalue())
        self.assertIsNone(state.chrome(self.FID))
        self.assertEqual(self.ran, [])

    def test_a_level_outside_the_enum_records_nothing_and_runs_nothing(self):
        self._with_panes()
        for hostile in ("bg=black", "solarized", "", None, ["dark"]):
            with self.subTest(level=hostile):
                with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID},
                                     clear=True):
                    self.assertEqual(commands_frame.cmd_chrome(self._args(hostile)), 0)
                self.assertIsNone(state.chrome(self.FID))
                self.assertEqual(self.ran, [])

    def test_a_frame_with_no_recorded_panes_records_nothing_and_runs_nothing(self):
        """Named separately from the two above because it is a DIFFERENT refusal: there
        is a session and the word is good, and there is simply nothing to resurface. Two
        guards in sequence mask each other, so each is asserted where it fires."""
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True):
            self.assertEqual(commands_frame.cmd_chrome(self._args("dark")), 0)
        self.assertIsNone(state.chrome(self.FID))
        self.assertEqual(self.ran, [])

    def test_a_pane_id_that_is_not_tmuxs_own_is_skipped_and_the_others_are_not(self):
        """The value arrived off DISK and is about to be a tmux argv — #475's rule. And
        the assertion is that the GOOD pane still got its options, or a single bad entry
        would silently cost the whole frame its surface."""
        self._with_panes({"top": "%1", "bad": "$(reboot)", "right": "%2"})
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True):
            commands_frame.cmd_chrome(self._args("dark"))
        targets = {a[a.index("-t") + 1] for a in self.ran}
        self.assertEqual(targets, {"%1", "%2"})

    def test_the_word_is_recorded_before_the_panes_are_touched(self):
        """A frame whose options fail halfway still splits its NEXT pane into the surface
        the operator asked for, because `_split_panels` reads `_current_chrome`."""
        self._with_panes({"top": "%1"})
        seen = []
        with mock.patch.object(commands_frame.tmuxctl, "run",
                               side_effect=lambda *a, **k: seen.append(
                                   state.chrome(self.FID)) or
                               subprocess.CompletedProcess([], 0, "", "")):
            with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID},
                                 clear=True):
                commands_frame.cmd_chrome(self._args("light"))
        self.assertTrue(seen and all(s == "light" for s in seen), seen)


class APaneSplitLaterIsBornIntoTheSurfaceTheFrameIsOn(PersonaIso, unittest.TestCase):
    """`_split_panels` reads `_current_chrome`, not `config.FRAME`.

    The defect this closes is specific and was reachable: change the surface from the
    palette, then change the density, and the pane the re-layout adds comes up bare
    beside three surfaced ones — a frame that does not match itself, which is the exact
    thing §5.1 refused to ship.
    """

    FID = "fr-surface-3"

    def test_the_split_reads_the_live_word_and_not_the_configured_one(self):
        state.record_chrome(self.FID, "dark")
        seen = []
        with mock.patch.dict(commands_frame.config.FRAME, {"chrome": "off"}), \
             mock.patch.object(commands_frame, "_surface_argvs",
                               side_effect=lambda **kw: seen.append(kw["chrome"]) or []), \
             mock.patch.object(commands_frame.layout, "panel_argvs",
                               return_value=[["tmux", "split"]]), \
             mock.patch.object(commands_frame.tmuxctl, "run",
                               return_value=subprocess.CompletedProcess([], 0, "%7", "")):
            commands_frame._split_panels("sock", slots=["top"], fid=self.FID,
                                         harness_pane="%0", env=None, pane_env=None)
        self.assertEqual(seen, ["dark"])


class ThePaletteCarriesTheThreeSurfaces(PersonaIso, unittest.TestCase):
    """Task 3.6. The operator who upgrades into a look they dislike is one keystroke from
    changing it, rather than one documentation search."""

    FID = "fr-surface-4"

    def _rows(self, *, current="off", panes=True):
        if panes:
            state.record_panes(self.FID, panels={"top": "%1"})
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome=current)
        return {o.id: o for o in reg.offers(fid=self.FID, snapshot={})}

    def test_there_is_one_row_per_surface_and_no_more(self):
        rows = self._rows()
        self.assertEqual({r for r in rows if r.startswith("chrome.")},
                         {f"chrome.{lv}" for lv in instance.FRAME_CHROME})

    def test_the_row_in_effect_is_marked_and_the_others_are_not(self):
        rows = self._rows(current="dark")
        self.assertTrue(rows["chrome.dark"].title.startswith(builtin_actions.MARK[0]))
        self.assertTrue(rows["chrome.off"].title.startswith(builtin_actions.MARK[1]))

    def test_the_mark_moves_with_the_frame(self):
        """Marked, never filtered: a list whose rows move around depending on state is a
        list nobody learns."""
        for level in instance.FRAME_CHROME:
            with self.subTest(level=level):
                rows = self._rows(current=level)
                marked = [r for r, o in rows.items()
                          if r.startswith("chrome.")
                          and o.title.startswith(builtin_actions.MARK[0])]
                self.assertEqual(marked, [f"chrome.{level}"])

    def test_a_frame_with_no_panes_lists_the_rows_with_a_reason(self):
        """Listed WITH ITS REASON, never dropped: an operator cannot ask about an option
        they cannot see, which is #512's shape one surface along."""
        rows = self._rows(panes=False)
        row = rows["chrome.dark"]
        self.assertFalse(row.available)
        self.assertIn("resurface", row.reason)

    def test_the_row_refuses_on_the_pane_map_and_not_on_the_harness_pane(self):
        """A surface sets a pane option on panes that already exist; it splits nothing.
        A row that refused on `_laid_out` would be refusing on somebody else's
        precondition."""
        state.record_panes(self.FID, panels={"top": "%1"})
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off")
        rows = {o.id: o for o in reg.offers(fid=self.FID, snapshot={})}
        self.assertTrue(rows["chrome.dark"].available)
        self.assertFalse(rows["density.minimal"].available,
                         "this fixture records no harness pane, so the CONTROL that "
                         "proves the two rows ask different questions has gone stale")

    def test_choosing_a_row_starts_the_command_and_nothing_else(self):
        state.record_panes(self.FID, panels={"top": "%1"})
        reg = builtin_actions.build(self.FID, current_density="normal",
                                    current_chrome="off")
        started = []
        with mock.patch.object(builtin_actions, "_spawn",
                               side_effect=lambda argv, **kw: started.append(argv)):
            reg.invoke("chrome.light", fid=self.FID, snapshot={})
        self.assertEqual(len(started), 1)
        self.assertEqual(started[0][-2:], ["frame-chrome", "light"])


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class TheLiveChangeReachesRealTmux(_TmuxServerFixture, PersonaIso):
    """`cmd_chrome` against a real server, read back with `show -p`.

    The exit criterion is stated as a reading rather than an intent: with `dark` the panel
    panes carry a background and the harness pane provably does not, and with `off` every
    pane reads back `''`.
    """

    SOCKET_NAME = f"charter-chrome-live-{os.getpid()}"
    FID = "fr-surface-live"

    def _panes(self) -> tuple[str, str]:
        r = self._srv("new-session", "-d", "-s", "h", "-x", "80", "-y", "24",
                      "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness = r.stdout.strip()
        p = self._srv("split-window", "-t", harness, "-P", "-F", "#{pane_id}",
                      "--", "sleep", "600")
        self.assertEqual(p.returncode, 0, p.stderr)
        return harness, p.stdout.strip()

    def _style(self, pane: str, option: str = "window-style") -> str:
        return self._srv("show", "-p", "-t", pane, "-v", option).stdout.strip()

    def _run(self, level: str) -> None:
        # `clear=True`, and `PATH` put back by hand rather than inherited by accident.
        # `tmuxctl.run` passes `env=None`, so the child inherits THIS mapping — and a
        # cleared environment is one with no `PATH`, where `tmux` is not found, the
        # command fails the way a refusal does (never raises) and the assertion below
        # would be measuring a fixture rather than charter. `NO_COLOR` is the variable
        # that must not leak in, and it does not.
        with mock.patch.dict(os.environ,
                             {"CHARTER_SESSION_ID": self.FID,
                              "PATH": os.defpath + os.pathsep + os.environ.get("PATH", "")},
                             clear=True):
            self.assertEqual(
                commands_frame.cmd_chrome(type("A", (), {"level": level})()), 0)

    def test_dark_then_off_leaves_every_pane_bare_again(self):
        harness, panel = self._panes()
        state.record_panes(self.FID, panels={"top": panel})
        state.record_server(self.FID, self.SOCKET_NAME)

        self._run("dark")
        self.assertEqual(self._style(panel), "bg=black")
        self.assertEqual(self._style(panel, "window-active-style"), "bg=brightblack")
        self.assertEqual(self._style(harness), "",
                         "charter styled the pane the operator's harness runs in")

        self._run("off")
        for option in instance.chrome_option_names():
            with self.subTest(option=option):
                self.assertEqual(self._style(panel, option), "",
                                 "`off` left a style behind — it is a REMOVAL on a "
                                 "running frame, not the absence of a set")
        self.assertEqual(self._style(harness), "")

    def test_switching_between_two_surfaces_leaves_nothing_of_the_first(self):
        _harness, panel = self._panes()
        state.record_panes(self.FID, panels={"top": panel})
        state.record_server(self.FID, self.SOCKET_NAME)
        self._run("dark")
        self._run("light")
        self.assertEqual(self._style(panel), "bg=white")
        self.assertEqual(self._style(panel, "window-active-style"), "bg=brightwhite")


if __name__ == "__main__":
    unittest.main()
