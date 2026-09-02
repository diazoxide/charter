"""#714: one pane per placed component, decided by asking tmux rather than the record.

Adding two `[[frame.component]]` tables to a plane whose frame was already running left
**six panel panes where there should have been two**, and the frame's own state recorded
only the newest of each::

    %366 %367 %368  panel workspaces --session harness-wrapper.1
    %369 %370 %371  panel chats      --session harness-wrapper.1

    .charter/frame/harness-wrapper.1/panes:
      {"top":"%362", ..., "workspaces":"%368", "chats":"%371"}

The four orphans were still running, each holding a ~24 MB `charter panel` process and
each drawing correct content, and they had squeezed the operator's harness from 38 rows to
30. Nothing in charter would ever have reaped them.

**The cause is one sentence and it is about a file, not about a loop.**
`state.record_panes` rewrites the pane map WHOLE on every re-layout. Splitting a second
pane for a component therefore deletes the first one's id, and after that no reader that
goes through `state.panes` can see that pane at all — not `_relayout`'s kill loop, not
`_drop_panels`, not `cmd_resize`. A re-layout built on the record is built on the thing
that is wrong, which is why `_reconcile_panels` reads the WINDOW.

Six properties, one class each:

**A panel says which component it is** (`ThePaneSaysWhichComponentItIs`). `_PANEL_OPTION`
from #634 marks a pane as *a panel* and does not name *which*, so the mark grows a second
option beside it (`_PANEL_SLOT_OPTION`) rather than the reconciliation guessing from
position. A positional match on a window whose panes have moved is the guess that kills
the wrong pane, and `frame/layout.py`'s module docstring already measures what "indices
move" costs.

**The window is asked, and a reply charter cannot trust is no reply**
(`TheWindowIsAskedNotTheRecord`). `list-panes` is the authority; a non-zero exit, or an
answer that does not contain the harness pane charter targeted, degrades to exactly the
behaviour that shipped before this — the record alone — rather than to "this window has no
panes", which would be a licence both to split duplicates and to kill.

**Nothing charter did not split is ever killed** (`OnlyWhatCharterSplitIsEverKilled`).
The discriminator is the mark, not position and not proximity: the harness pane, the
palette overlay, and a pane the operator split themselves all survive a reconciliation
that kills everything else in the window. So does a panel from a charter old enough to
carry the mark but no component id — charter can see it is its own and cannot see which
component it is, and the safe answer to that is to leave it running.

**Both directions, on a real tmux** (`TheReproductionOnARealServer`). The issue's own
acceptance test: a running frame, a `charter.toml` edited to add a component and then to
remove it, with a re-layout after each, ending with exactly one pane per placed component.
Run against tmux 3.7c and against 3.2 — `tmuxctl.FLOOR` — by putting a 3.2 built from the
release tarball first on `$PATH`; identical on both, so nothing here carries a version
gate.

**And it survives the record being wrong** (`TheRecordIsNoLongerTheOnlyThingThatSees`),
which is the defect rather than a hardening exercise: the state file is planted in the
exact shape the operator's frame was measured in, and the frame comes back to one pane per
component from there.

**Both callers split only what is left** (`BothCallersSplitOnlyWhatTheWindowIsMissing`).
The duplicate pane is a `split-window`, so a reconciliation that returned the right map
while its caller went on splitting anyway would have fixed nothing — asserted through
`_relayout` and through `_draw_panels`, which is where "is `_relayout` the right home" is
answered: neither on its own, the way #697 made `_dress_window` a step both run.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import unittest
from pathlib import Path
from unittest import mock

from charter import commands_frame, config, instance
from charter.frame import layout, state, tmuxctl

from tests import _tmuxreap
from tests import _tmuxchain
from tests._isolation import PersonaIso, make_plane

_HAS_TMUX = shutil.which("tmux") is not None

#: One socket for this module, carrying this process's pid so an interrupted run's server
#: is reaped by the next one rather than collided with (`tests/_tmuxreap.py`).
SOCKET = _tmuxreap.name("panel-reconcile")

#: This checkout, for the `$PYTHONPATH` a real `charter panel` child needs: its argv comes
#: from `util.self_relaunch_argv()` and carries `-P`, which strips exactly the cwd entry
#: `-m` would otherwise have used to find this tree (#390).
_REPO_ROOT = Path(__file__).resolve().parent.parent

#: The plane this test PROCESS started in — the developer's real one, captured before any
#: `setUp` repoints `config`. Read only to refuse to run against it.
_REAL_STATE_DIR = Path(config.STATE_DIR)

#: The four charter ships, plus the two the operator added to the live frame. Written as
#: committed `[[frame.component]]` tables rather than as resolved placements, because what
#: is under test is what editing that FILE does to a running frame.
_BASE_TABLES = [{"use": "identity"}, {"use": "attention"},
                {"use": "repos"}, {"use": "sidebar"}]
_ADDED_TABLES = _BASE_TABLES + [{"use": "workspaces"}, {"use": "chats"}]


def _a_dead_pid() -> int:
    """A real pid that has exited and been reaped — `tests/test_component_toggle_keys.py`'s
    helper, repeated for the reason that copy states. `state.reap` keeps any frame
    directory whose trailing number is a live pid, and a hand-written `-1` reads as pid 1,
    which never exits."""
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    return p.pid


class _Tmux:
    """A recording stand-in for `tmuxctl.run` that can answer `list-panes`.

    `tests/test_component_toggle_keys.py`'s fake with the one query this defect is about
    added. *window* is the reply `list-panes` gives, as ``{pane id: (mark, slot)}`` in
    tmux's own order — which is a pane's two option VALUES, exactly as a real server hands
    them back, so a test states what the server says rather than what charter concluded.
    ``None`` makes the call fail, which is the "charter could not ask" case.
    """

    def __init__(self, *, window=None, size="200:50", new_panes=("%90", "%91", "%92")):
        self.window = window
        self.size = size
        self.new_panes = list(new_panes)
        self.calls: list[list[str]] = []
        #: One entry per tmux INVOCATION, where `calls` has one per
        #: tmux COMMAND — see :meth:`__call__`.
        self.invocations: list[list[str]] = []

    def __call__(self, action, argv, *, env=None, timeout=None, report=True):
        """One tmux INVOCATION, which since #780 may carry several commands.

        Split through `tests/_tmuxchain.answer_run` so `self.calls` keeps one entry per
        tmux COMMAND — the list every assertion here reads — where charter now spends
        one invocation on a whole group of them.
        """
        self.invocations.append(list(argv))
        return _tmuxchain.answer_run(self._one, action, argv, env=env, timeout=timeout,
                                     report=report)

    def _one(self, action, argv, *, env=None, timeout=None, report=True):
        self.calls.append(list(argv))
        out = ""
        if "list-panes" in argv:
            if self.window is None:
                return subprocess.CompletedProcess(argv, 1, stdout="",
                                                   stderr="can't find pane\n")
            out = "".join(f"{p} {mark} {slot}\n"
                          for p, (mark, slot) in self.window.items())
        elif "display-message" in argv:
            out = self.size
        elif "split-window" in argv:
            out = f"{self.new_panes.pop(0)}\n" if self.new_panes else ""
        return subprocess.CompletedProcess(argv, 0, stdout=out, stderr="")

    def killed(self) -> list[str]:
        return [c[c.index("kill-pane") + 2] for c in self.calls if "kill-pane" in c]

    def split(self) -> list[str]:
        return [c[c.index("panel") + 1] for c in self.calls
                if "split-window" in c and "panel" in c]


def _pane(slot: str | None = None, *, charters: bool = True) -> tuple[str, str]:
    """One `_Tmux` window entry: the two option values a pane of this kind really has.

    `charters=False` is a pane charter never marked — the harness's, the palette
    overlay's, or one the operator split; *slot* ``None`` is a panel from a charter that
    marked panes without naming them.
    """
    return ((commands_frame._PANEL_MARK if charters else ""), slot or "")


class ThePaneSaysWhichComponentItIs(PersonaIso, unittest.TestCase):
    """#634's mark says a pane is A panel. It does not say WHICH, and that gap is why
    `_relayout` could not tell an orphan from a stranger.

    The alternative was a positional match — "the third pane from the top is `chats`" —
    and it is refused rather than deferred: `frame/layout.py`'s module docstring measures
    that tmux renumbers pane INDICES on every split, so a window whose panes have moved
    (any window a re-layout has ever touched) would hand a positional matcher the wrong
    pane, and what it does with the wrong pane is `kill-pane`.
    """

    def setUp(self):
        super().setUp()
        # The split funnel sizes each pane through the resolved ARRANGEMENT
        # (`layout._size_of`), so a component this plane has not placed cannot be split at
        # all — `_drawable_slots`' own guarantee one level up. The two extra tables are
        # therefore needed here as well as in the classes that are about them.
        self.enterContext(mock.patch.dict(
            config.FRAME, instance.frame_of({"frame": {"component": _ADDED_TABLES}}),
            clear=True))

    def test_the_mark_alone_cannot_name_a_component(self):
        """The control for this whole file: if the mark ever grew a component id of its
        own, the second option would be dead weight and every test below would be
        asserting about a mechanism nothing needs."""
        argv = commands_frame._panel_mark_argv(socket="s", pane_id="%3")
        self.assertEqual(argv[-2:], [commands_frame._PANEL_OPTION,
                                     commands_frame._PANEL_MARK])
        self.assertNotIn("chats", argv)

    def test_a_panel_is_told_which_component_it_draws(self):
        argv = commands_frame._panel_slot_argv(socket="s", pane_id="%3", slot="chats")
        self.assertEqual(argv[-4:], ["-t", "%3",
                                     commands_frame._PANEL_SLOT_OPTION, "chats"])
        self.assertEqual(argv[:6], ["tmux", "-L", "s", "set-option", "-p", "-t"])

    def test_the_two_options_are_different_names(self):
        """Two options rather than one value doing both jobs. `conf_text`'s
        `MouseDown1Pane` bind format-expands `_PANEL_OPTION` and routes a click on
        whether it reads TRUE; a component id spelled `0` is a perfectly legal id
        (`component._ID_RE`) and a FALSE format value, so folding the two together would
        have made that one component's panel the only pane in the frame that steals the
        keyboard."""
        self.assertNotEqual(commands_frame._PANEL_OPTION,
                            commands_frame._PANEL_SLOT_OPTION)
        self.assertEqual(commands_frame._PANEL_MARK, "1")

    def test_the_option_name_is_spelled_out_because_a_rename_costs_a_release(self):
        """**The one case in this file allowed to compare the constant against text**, and
        the sweep is what asked for it: every other case here builds its expectation out of
        `_PANEL_SLOT_OPTION`, so all of them pass with the constant respelled to anything
        at all.

        The spelling is a promise across VERSIONS, which is what makes it worth an
        assertion rather than a convention. A pane carries this option for as long as it
        lives, and the whole of #714 is a charter reading back what an earlier charter
        wrote onto a pane that is still running. Rename it and every panel of every live
        frame answers `""` again — charter's pane, unnameable, never killed and never
        adopted — which is this defect returning for one release per rename, silently,
        on exactly the frames that were already up.

        It also has to stay inside charter's own `@charter_` namespace: this option is
        written on the operator's own server as well as charter's (`_split_panels` does
        not gate on which), and charter compares it and closes panes on it. A name outside
        that namespace is a name something else on their server may also be using.
        """
        self.assertEqual(commands_frame._PANEL_SLOT_OPTION, "@charter_panel_slot")
        self.assertEqual(
            commands_frame._panel_slot_argv(socket="s", pane_id="%1",
                                            slot="chats")[-2],
            "@charter_panel_slot")
        self.assertIn("#{@charter_panel_slot}", commands_frame._PANEL_LIST_FORMAT)

    def test_a_name_charter_would_not_put_on_a_bind_line_is_refused(self):
        """The value reaches a comparison against `want` and a decision to `kill-pane`, so
        it is held to the alphabet `frame/component.py` holds every id that reaches tmux
        config text to — asked of `component.usable_id` rather than re-spelled. Each of
        these is a name `instance.frame_of` would already have refused; this is the guard
        that stops one arriving by some other route from being written onto a pane and
        read back as though charter had put it there."""
        for name in ("chats\nkill-server", "chats kill-server", "#{pane_id}",
                     "CHATS", "", "chats;kill-server"):
            with self.subTest(name=name):
                self.assertIsNone(
                    commands_frame._panel_slot_argv(socket="s", pane_id="%3", slot=name))

    def test_a_panel_charter_cannot_name_is_still_split_marked_and_drawn(self):
        """What declining costs and what it does not, at the funnel rather than at the
        builder. The pane is still created, still carries `_PANEL_OPTION`, and still gets
        its surface — a component charter cannot name is not a component charter refuses
        to draw. What it loses is the ability to be reconciled later, which is exactly the
        position every panel of a pre-#714 frame is in, and `_reconcile_panels` treats
        both the same way: left running, never guessed at.

        Sized explicitly, because that is what lets a name the arrangement never placed
        reach this funnel at all — `_panel_died_hook_argv`'s rule, one function over: what
        decides is the value that arrives, never where it came from."""
        fake = _Tmux(new_panes=("%20",))
        with mock.patch.object(commands_frame.tmuxctl, "run", fake):
            panes = commands_frame._split_panels(
                "s", slots=["CHATS"], fid="f", harness_pane="%1", env=None,
                pane_env=None, sizes={"CHATS": 1}, v=(3, 7))
        self.assertEqual(panes, {"CHATS": "%20"}, fake.calls)
        self.assertEqual([c for c in fake.calls
                          if commands_frame._PANEL_SLOT_OPTION in c], [],
                         "a name charter would not put on a bind line was written onto "
                         "a pane and will be read back as though charter had meant it")
        self.assertIn(["tmux", "-L", "s", "set-option", "-p", "-t", "%20",
                       commands_frame._PANEL_OPTION, commands_frame._PANEL_MARK],
                      fake.calls, "the pane charter could not name lost its panel mark "
                                  "too, so a click on it now steals the keyboard")

    def test_every_pane_the_split_funnel_creates_is_told(self):
        """`_split_panels` is the one place a panel pane comes out of, on both launch
        paths and every re-layout (#634's own funnel argument, one option over). A pane
        that reached the screen without this is a pane `_reconcile_panels` cannot name,
        and therefore one it will never kill — so it can be orphaned exactly once more."""
        fake = _Tmux(new_panes=("%20", "%21"))
        with mock.patch.object(commands_frame.tmuxctl, "run", fake):
            panes = commands_frame._split_panels(
                "s", slots=["workspaces", "chats"], fid="f", harness_pane="%1",
                env=None, pane_env=None, v=(3, 7))
        self.assertEqual(panes, {"workspaces": "%20", "chats": "%21"})
        said = {c[c.index("-t") + 1]: c[-1] for c in fake.calls
                if "set-option" in c and c[-2] == commands_frame._PANEL_SLOT_OPTION}
        self.assertEqual(said, {"%20": "workspaces", "%21": "chats"})


class TheWindowIsAskedNotTheRecord(PersonaIso, unittest.TestCase):
    """`_window_panels`: what tmux says, and what charter does when it cannot be sure.

    Every case below is a REPLY — a `list-panes` exit code and its bytes — because that is
    the whole of this function's input, and because the reply shapes that matter are the
    ones a stub, a truncated read or an older tmux can produce.
    """

    def _asked(self, *, window=None, stdout=None, rc=0, harness="%1"):
        def fake(_action, argv, **_kw):
            if stdout is not None:
                return subprocess.CompletedProcess(argv, rc, stdout=stdout, stderr="")
            body = "".join(f"{p} {m} {s}\n" for p, (m, s) in (window or {}).items())
            return subprocess.CompletedProcess(argv, rc, stdout=body, stderr="")
        with mock.patch.object(commands_frame.tmuxctl, "run", fake):
            return commands_frame._window_panels("s", harness)

    def test_it_asks_the_window_through_the_harness_pane(self):
        """Scoped to the harness pane's own window, which is what keeps a frame with
        several chats from reconciling another chat's window — measured on tmux 3.7c and
        at `tmuxctl.FLOOR` that `list-panes -t %N` resolves a pane target to its window
        and lists that window alone."""
        seen: list[list[str]] = []

        def fake(_action, argv, **_kw):
            seen.append(list(argv))
            return subprocess.CompletedProcess(argv, 0, stdout="%1  \n", stderr="")

        with mock.patch.object(commands_frame.tmuxctl, "run", fake):
            commands_frame._window_panels("s", "%1")
        self.assertEqual(seen, [["tmux", "-L", "s", "list-panes", "-t", "%1",
                                 "-F", commands_frame._PANEL_LIST_FORMAT]])

    def test_a_marked_pane_answers_with_its_component(self):
        got = self._asked(window={"%1": _pane(charters=False),
                                  "%4": _pane("chats"), "%5": _pane("workspaces")})
        self.assertEqual(got, {"%1": None, "%4": "chats", "%5": "workspaces"})

    def test_an_unmarked_pane_is_not_charters(self):
        """`None`, distinctly — the operator's own pane and the palette overlay both land
        here, and it is the only answer that must never become a kill."""
        self.assertEqual(self._asked(window={"%1": _pane(charters=False),
                                             "%9": _pane(charters=False)}),
                         {"%1": None, "%9": None})

    def test_a_panel_from_an_older_charter_is_charters_but_unnamed(self):
        """The migration case, and `""` rather than `None` because the two are different
        facts: this pane IS charter's, and charter cannot say which component it is."""
        self.assertEqual(self._asked(window={"%1": _pane(charters=False),
                                             "%4": _pane(None)}),
                         {"%1": None, "%4": ""})

    def test_a_slot_value_charter_would_not_have_written_is_not_read_back(self):
        """The read-side half of `_panel_slot_argv`'s guard. A pane option is a pane
        option: anything with `set-option -p` rights can put a value there, and this one
        decides which component a pane is believed to be — which decides whether it is
        kept, adopted or killed. Downgraded to "charter's, unnamed", which is the answer
        that never kills."""
        self.assertEqual(self._asked(window={"%1": _pane(charters=False),
                                             "%4": ("1", "#{pane_id}")}),
                         {"%1": None, "%4": ""})

    def test_a_mark_that_is_not_charters_own_constant_is_not_charters_pane(self):
        """Compared against `_PANEL_MARK`, not read as a tmux truth-value — and the
        asymmetry with `conf_text`'s bind is deliberate. That bind is generous (`2`, `on`
        and even `off` all read TRUE) because being wrong there costs a click going to
        the wrong pane. Being wrong here costs a pane."""
        self.assertEqual(self._asked(window={"%1": _pane(charters=False),
                                             "%4": ("on", "chats")}),
                         {"%1": None, "%4": None})

    def test_a_reply_charter_could_not_get_is_no_reply(self):
        """A non-zero exit ends it, and the reply's BYTES are not read as a consolation —
        which is why this case carries a perfectly well-formed body. tmux prints what it
        can before it fails, and a partial pane list read as a whole one is a window
        charter believes has fewer panes than it does: a licence to split a duplicate for
        every pane the failure cut off."""
        self.assertIsNone(self._asked(rc=1, stdout="%1  \n%4 1 chats\n"))

    def test_a_reply_without_the_pane_charter_asked_about_is_no_reply(self):
        """tmux lists the window CONTAINING the target, so the target is always in its own
        answer. A reply that does not contain it is not this window's pane list, whatever
        its exit code said — and this is what makes the whole reconciliation degrade to
        "charter cannot tell" (the record alone, which is what shipped) rather than to
        "this window has no panes", which is a licence to split duplicates and to kill."""
        self.assertIsNone(self._asked(window={"%4": _pane("chats")}))

    def test_a_line_that_is_not_three_fields_is_dropped(self):
        """`str.split(" ")` and not bare `.split()`: an unmarked pane's line is a pane id
        and two EMPTY fields, which whitespace-splitting collapses to one."""
        self.assertEqual(self._asked(stdout="%1  \nnonsense\n%4 1 chats\n"),
                         {"%1": None, "%4": "chats"})

    def test_a_pane_id_that_is_not_tmuxs_own_shape_is_dropped(self):
        """#475's rule applied on the way IN from tmux's stdout, the same way
        `_split_panels` applies it to `split-window`'s: these ids become `kill-pane -t`
        arguments."""
        self.assertEqual(self._asked(stdout="%1  \n%2;kill-server 1 chats\n"),
                         {"%1": None})


class OnlyWhatCharterSplitIsEverKilled(PersonaIso, unittest.TestCase):
    """Constraint 1 of #714, asserted from the other end: everything in the window is
    unwanted, and the reconciliation still kills only what charter itself said was a
    panel of a component.

    Each case below is an input that reaches exactly one refusal, so no two of them can
    stand in for each other.
    """

    def _reconcile(self, *, window, want, panels=None, harness="%1"):
        fake = _Tmux(window=window)
        with mock.patch.object(commands_frame.tmuxctl, "run", fake):
            keep = commands_frame._reconcile_panels(
                "s", harness_pane=harness, want=want, panels=panels or {})
        return keep, fake

    def test_the_harness_pane_is_never_killed(self):
        keep, fake = self._reconcile(window={"%1": _pane(charters=False)}, want=[])
        self.assertEqual(keep, {})
        self.assertEqual(fake.killed(), [])

    def test_a_record_that_names_the_harness_pane_does_not_kill_it(self):
        """`state.record_panes` deliberately never holds the harness pane, so a record
        naming it is a record charter did not write — a hand edit, or a truncated write
        over a file that is JSON on disk. Without the guard the loop's other branch
        `kill-pane`s it, taking the agent's own rectangle down along with the panel the
        operator was dropping. Reachable by nothing else here: this pane is unmarked, so
        the window walk leaves it alone on its own account."""
        keep, fake = self._reconcile(window={"%1": _pane(charters=False)},
                                     want=[], panels={"top": "%1"})
        self.assertEqual(keep, {})
        self.assertEqual(fake.killed(), [])

    def test_a_pane_the_operator_split_themselves_is_never_killed(self):
        """Unmarked and unrecorded — which is every pane in an operator's own tmux, and
        the palette's overlay pane on charter's own server (`frame/overlay.py` splits one
        and never marks it). The window says nothing about it, so charter says nothing to
        it."""
        keep, fake = self._reconcile(
            window={"%1": _pane(charters=False), "%7": _pane(charters=False)}, want=[])
        self.assertEqual(fake.killed(), [])
        self.assertEqual(keep, {})

    def test_a_panel_from_an_older_charter_is_left_running_rather_than_guessed_at(self):
        """The migration answer, and it is the SAFE direction rather than the complete
        one. A frame launched before this change has panels carrying `_PANEL_OPTION` and
        no component id; charter can see they are its own and cannot see which component
        each is. Killing one would be a positional guess on a window whose panes have
        moved. It is left running — the frame is no worse off than it was — and one
        re-layout later every pane charter splits carries an id, so it heals forward."""
        keep, fake = self._reconcile(
            window={"%1": _pane(charters=False), "%4": _pane(None)}, want=[])
        self.assertEqual(fake.killed(), [])
        self.assertEqual(keep, {})

    def test_an_older_charters_panel_is_still_reachable_through_the_record(self):
        """…and the record is what makes the migration case merely *incomplete* rather
        than *broken*: a frame from before #714 whose record is still correct drops a
        component's pane exactly as it always did. That is the direction removal has
        always worked in, and it must not regress on the frames that are running today."""
        keep, fake = self._reconcile(
            window={"%1": _pane(charters=False), "%4": _pane(None)},
            want=[], panels={"chats": "%4"})
        self.assertEqual(fake.killed(), ["%4"])
        self.assertEqual(keep, {})

    def test_a_record_id_that_is_not_tmuxs_own_shape_reaches_no_argv(self):
        """#475, on the value that is about to become a `kill-pane -t` argument. A
        `%1;kill-server` in that file armed `kill-server` on every window resize for the
        life of a window once already.

        On a window charter could NOT read, which is the only state in which this guard is
        the one that decides: with a reply in hand the id is discarded a line later for not
        being in it, and a guard that passes only because a different guard caught it is
        not a guard. Here the record is all there is — exactly as it was before #714 — and
        an unchecked value goes straight to `kill-pane`."""
        keep, fake = self._reconcile(window=None, want=[],
                                     panels={"top": "%1;kill-server"})
        self.assertEqual(keep, {})
        self.assertEqual(fake.killed(), [])
        self.assertEqual([c for c in fake.calls if any("kill-server" in a for a in c)],
                         [], "a value off disk reached a tmux argv unchecked")

    def test_a_panel_of_a_component_that_is_gone_is_killed_and_disarmed_first(self):
        """The control for every refusal above: with the same window and the same call,
        a pane charter DID name is closed — so none of those tests is passing because the
        reconciliation kills nothing at all.

        Disarmed before it is killed, and in that order: `kill-pane` on an armed panel
        fires its own `pane-died` hook, and `cmd_respawn` brings back the panel the
        operator just dropped, one respawn life poorer."""
        keep, fake = self._reconcile(
            window={"%1": _pane(charters=False), "%4": _pane("chats")}, want=[])
        self.assertEqual(keep, {})
        self.assertEqual(fake.killed(), ["%4"])
        order = [c[3] for c in fake.calls if c[3] in ("set-hook", "kill-pane")]
        self.assertEqual(order, ["set-hook", "kill-pane"])


class TheRecordIsNoLongerTheOnlyThingThatSees(PersonaIso, unittest.TestCase):
    """The defect itself: `state.panes` is wrong, and the frame still comes back to one
    pane per placed component.

    The record is planted in the shape the operator's live frame was measured in — the
    newest pane per component and nothing else — because "the reconciliation must survive
    the record being wrong" is not a hardening exercise here, it is the failure mode.
    """

    def _reconcile(self, *, window, want, panels):
        fake = _Tmux(window=window)
        with mock.patch.object(commands_frame.tmuxctl, "run", fake):
            keep = commands_frame._reconcile_panels(
                "s", harness_pane="%1", want=want, panels=panels)
        return keep, fake

    def test_an_unrecorded_pane_for_a_wanted_component_is_adopted_not_duplicated(self):
        """The whole of the growth half. Without this the component is `missing`, a
        second pane is split for it, the record is rewritten around the new id, and the
        pane that was already there is invisible to every reader from then on."""
        keep, fake = self._reconcile(
            window={"%1": _pane(charters=False), "%4": _pane("chats")},
            want=["chats"], panels={})
        self.assertEqual(keep, {"chats": "%4"})
        self.assertEqual(fake.killed(), [])

    def test_the_measured_frame_comes_back_to_one_pane_per_component(self):
        """`%366`-`%371`, and the record from the issue verbatim. Six panes, two
        components, one recorded each; four orphans still drawing."""
        window = {"%1": _pane(charters=False), "%362": _pane("identity")}
        window.update({p: _pane("workspaces") for p in ("%366", "%367", "%368")})
        window.update({p: _pane("chats") for p in ("%369", "%370", "%371")})
        keep, fake = self._reconcile(
            window=window, want=["identity", "workspaces", "chats"],
            panels={"identity": "%362", "workspaces": "%368", "chats": "%371"})
        self.assertEqual(keep, {"identity": "%362",
                                "workspaces": "%368", "chats": "%371"},
                         "the recorded pane is the survivor, so the resize hook and the "
                         "respawn hooks still agree about which pane is which")
        self.assertEqual(sorted(fake.killed()), ["%366", "%367", "%369", "%370"])
        self.assertEqual(fake.split(), [], fake.calls)

    def test_a_component_no_longer_placed_loses_a_pane_the_record_lost_too(self):
        """The removal half, which is the same reconciliation in the other direction and
        was broken today for the same reason — and is how the first orphan pair was made.
        Neither pane is in the record; `want` no longer names the component; both go."""
        keep, fake = self._reconcile(
            window={"%1": _pane(charters=False),
                    "%4": _pane("chats"), "%5": _pane("chats")},
            want=["repos"], panels={"repos": "%9"})
        self.assertEqual(keep, {}, "a recorded pane the window does not have was kept")
        self.assertEqual(sorted(fake.killed()), ["%4", "%5"])

    def test_a_recorded_pane_the_window_no_longer_has_is_dropped_not_kept(self):
        """A record can be wrong in the other direction too — it can name a pane that has
        gone (the operator closed it, the panel's window was rebuilt). Kept, it is a hole
        nothing ever fills, because the component looks present to `_relayout`'s `missing`
        and to every size re-assertion. Dropped, the component is split again."""
        keep, fake = self._reconcile(
            window={"%1": _pane(charters=False)}, want=["chats"],
            panels={"chats": "%4"})
        self.assertEqual(keep, {})
        self.assertEqual(fake.killed(), [],
                         "charter aimed a kill at a pane that is not there")

    def test_nothing_is_dropped_on_a_window_charter_could_not_read(self):
        """`_window_panels` answering `None` is not "the window is empty". Every recorded
        pane is still honoured and nothing is adopted, which is exactly the behaviour that
        shipped before this — a re-layout that cannot ask tmux is no worse than one that
        never could."""
        keep, fake = self._reconcile(window=None, want=["chats"],
                                     panels={"chats": "%4", "repos": "%5"})
        self.assertEqual(keep, {"chats": "%4"})
        self.assertEqual(fake.killed(), ["%5"])


class BothCallersSplitOnlyWhatTheWindowIsMissing(PersonaIso, unittest.TestCase):
    """The same property one level up, through the two real functions that split panes —
    because the duplicate pane is their `split-window`, and a reconciliation that returned
    the right map while its caller went on splitting anyway would fix nothing.

    `_draw_panels` as well as `_relayout`, which is the answer to "is `_relayout` the right
    home": neither, on its own. The reconciliation is a shared step both callers run, the
    way #697 made `_dress_window` one — and the two are handed different things, because a
    launch's `state.panes` describes whatever held this frame id last while a re-layout's
    describes the frame that is running.
    """

    def setUp(self):
        super().setUp()
        self.fid = f"rl-{_a_dead_pid()}"
        self.enterContext(mock.patch.dict(
            config.FRAME, instance.frame_of({"frame": {"component": _ADDED_TABLES}})))

    def _relayout(self, *, window, want, panels):
        fake = _Tmux(window=window)
        with mock.patch.object(commands_frame.tmuxctl, "run", fake):
            keep = commands_frame._relayout(
                "s", fid=self.fid, harness_pane="%1", panels=panels, want=want,
                v=(3, 7), window_cols=200, window_rows=50)
        return keep, fake

    def test_a_component_whose_pane_is_already_there_is_not_split_again(self):
        keep, fake = self._relayout(
            window={"%1": _pane(charters=False), "%4": _pane("chats")},
            want=["chats"], panels={})
        self.assertEqual(fake.split(), [], fake.calls)
        self.assertEqual(keep, {"chats": "%4"})

    def test_a_component_with_no_pane_anywhere_is_still_split(self):
        """The control: adoption must not become a reason never to split. A frame that
        genuinely lacks a component's pane still gets one."""
        keep, fake = self._relayout(window={"%1": _pane(charters=False)},
                                    want=["chats"], panels={})
        self.assertEqual(fake.split(), ["chats"], fake.calls)
        self.assertEqual(keep, {"chats": "%90"})

    def test_a_launch_into_a_window_that_already_holds_a_panel_adopts_it(self):
        """The same property on the LAUNCH path, which is why the reconciliation lives in
        `_draw_panels` rather than only in `_relayout` (#697's precedent, one function
        over). Both launch paths open their own window two dozen lines earlier, so today
        this finds nothing — and "this path happens not to need it" is precisely the shape
        #686 cost a release. Driven here at the one input that can tell the difference: a
        window that already holds a marked panel for a slot the launch is about to draw."""
        fake = _Tmux(window={"%1": _pane(charters=False), "%4": _pane("chats")})
        with mock.patch.object(commands_frame.tmuxctl, "run", fake):
            panes = commands_frame._draw_panels(
                "s", slots=["chats", "identity"], fid=self.fid, harness_pane="%1",
                env=None, v=(3, 7))
        self.assertEqual(fake.split(), ["identity"], fake.calls)
        self.assertEqual(panes, {"chats": "%4", "identity": "%90"})
        self.assertEqual(state.panes(self.fid), panes,
                         "the adopted pane is missing from the record, so the next "
                         "re-layout cannot see it either")

    def test_a_launch_does_not_believe_a_record_left_by_whatever_held_this_id(self):
        """Frame ids are reused — a chat is `<session>.<n>`, and a workspace relaunched
        after its server died gets the same one — so the pane map in the frame's directory
        can name panes of a server that no longer exists. A launch that believed it would
        split no panel at all and record a map of dead ids."""
        state.record_panes(self.fid, panels={"chats": "%77", "identity": "%78"})
        fake = _Tmux(window={"%1": _pane(charters=False)})
        with mock.patch.object(commands_frame.tmuxctl, "run", fake):
            panes = commands_frame._draw_panels(
                "s", slots=["chats", "identity"], fid=self.fid, harness_pane="%1",
                env=None, v=(3, 7))
        self.assertEqual(fake.split(), ["chats", "identity"], fake.calls)
        self.assertEqual(panes, {"chats": "%90", "identity": "%91"})
        self.assertEqual(fake.killed(), [],
                         "a launch aimed a kill at a pane recorded by a frame that is "
                         "over")

    def test_the_surviving_order_is_the_recorded_one(self):
        """`_relayout` hands `list(keep) + missing` to `layout.repos_cols`, so the order
        `keep` comes out in is load-bearing (#500 round 3): it has to be the order the
        panes are really in, which for a healthy frame is the record's. The record is
        walked before the window for that reason as well as for choosing survivors."""
        keep, _ = self._relayout(
            window={"%1": _pane(charters=False), "%2": _pane("identity"),
                    "%3": _pane("attention"), "%4": _pane("chats")},
            want=["chats", "identity", "attention"],
            panels={"identity": "%2", "attention": "%3"})
        self.assertEqual(list(keep), ["identity", "attention", "chats"])


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class TheReproductionOnARealServer(PersonaIso, unittest.TestCase):
    """#714's own acceptance test, against a real tmux: a running frame, a `charter.toml`
    edited to add two components and then to remove them, with a re-layout after each.

    **Nothing here is a fake.** The panes are split by charter's own `_split_panels`,
    which means real `charter panel` children pointed at a throwaway plane; the marks are
    the ones production writes; the re-layout is the real `_relayout`; and what is
    asserted is `list-panes` on a real server afterwards. A mock cannot make this claim:
    the whole defect is about what a pane option survives and what a state file forgets,
    and both of those are facts about tmux and the filesystem rather than about charter's
    control flow.

    Re-run against tmux 3.2 (`tmuxctl.FLOOR`) as well as 3.7c by putting a 3.2 built from
    the release tarball first on `$PATH`. Identical on both, so nothing here is gated on a
    version.
    """

    WS = "recon"

    def setUp(self) -> None:
        super().setUp()
        self.assertNotEqual(
            Path(config.STATE_DIR), _REAL_STATE_DIR,
            "this test runs charter's real re-layout, whose state this would write into "
            "the developer's own control plane")
        self.addCleanup(self._teardown_socket)
        self.fid = f"{self.WS}.1"
        self.plane = make_plane(self)
        # Both halves, and they are two different things (the same pair
        # `test_frame_tmux_integration`'s own switch test carries): the `-e` payload is
        # built from `state.identity` and is what each PANEL's process gets, while
        # `$CHARTER_ROOT` in THIS process is what the tmux client inherits — a live
        # re-layout hands `tmuxctl.run` no client environment at all (`_relayout`'s
        # `env=None`), so the server's own environment is this process's.
        self.enterContext(mock.patch.dict(os.environ, {
            "CHARTER_ROOT": str(self.plane), "CHARTER_WORKSPACE": self.WS,
            "CHARTER_SESSION_ID": self.fid,
            # The panel argv carries `-P` (#390), which strips exactly the entry `-m`
            # would have used to find this checkout; `$PYTHONPATH` is the substitute the
            # suite already uses for a panel spawned out of a working tree.
            "PYTHONPATH": os.pathsep.join(
                [str(_REPO_ROOT), os.environ.get("PYTHONPATH", "")]).rstrip(os.pathsep),
        }, clear=False))
        state.record_workspace(self.fid, self.WS)
        state.record_identity(self.fid, {"CHARTER_SESSION_ID": self.fid,
                                         "CHARTER_ROOT": str(self.plane),
                                         "CHARTER_WORKSPACE": self.WS})
        made = self._srv("new-session", "-d", "-s", self.WS, "-x", "200", "-y", "50",
                         "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(made.returncode, 0, made.stderr)
        self.harness = made.stdout.strip()
        state.record_harness_pane(self.fid, self.harness)
        self.v = tmuxctl.version()
        self.assertIsNotNone(self.v, "charter could not read this tmux's version")

    def _teardown_socket(self) -> None:
        """`kill-server` FIRST and unlink second, never two `addCleanup` calls in that
        order — `addCleanup` runs LIFO, so the second spelling unlinks the socket and then
        reconnects to nothing, leaving a server behind with `remain-on-exit` armed on its
        window (`_dress_window` arms it, so every test here has it)."""
        self._srv("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET).unlink(missing_ok=True)

    def _srv(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(["tmux", "-L", SOCKET, *args],
                              capture_output=True, text=True, timeout=15)

    def _window(self) -> dict[str, str]:
        """`{pane id: component}` for every pane charter marked, read off the real server
        with charter's own format — so this measures what a later reconciliation would
        see rather than what this test believes."""
        out = self._srv("list-panes", "-t", self.harness, "-F",
                        commands_frame._PANEL_LIST_FORMAT)
        self.assertEqual(out.returncode, 0, out.stderr)
        found = {}
        for line in out.stdout.splitlines():
            pane_id, mark, slot = line.split(" ")
            if mark == commands_frame._PANEL_MARK:
                found[pane_id] = slot
        return found

    def _drawn(self) -> list[str]:
        return sorted(self._window().values())

    def _edit_charter_toml(self, tables) -> None:
        """What editing the plane's `[[frame.component]]` tables does to a process that
        has already resolved them — `config.FRAME` is the resolved arrangement every
        re-layout reads, so this is that edit at the boundary charter actually feels it
        at."""
        self.enterContext(mock.patch.dict(
            config.FRAME, instance.frame_of({"frame": {"component": tables}}),
            clear=True))

    def _relayout(self, want) -> dict[str, str]:
        panes = commands_frame._relayout(
            SOCKET, fid=self.fid, harness_pane=self.harness, panels=state.panes(self.fid),
            want=want, v=self.v, window_cols=200, window_rows=50)
        state.record_panes(self.fid, panels=panes)
        return panes

    def _draw(self, slots) -> dict[str, str]:
        panes = commands_frame._draw_panels(
            SOCKET, slots=slots, fid=self.fid, harness_pane=self.harness, env=None,
            v=self.v, pane_env=commands_frame._relayout_pane_env(self.fid, self.v))
        return panes

    def test_adding_two_components_and_removing_them_leaves_one_pane_each(self):
        """The reproduction, end to end. Three edits — added, reverted, added again — is
        what the operator's session actually did, and it is what turned two panels into
        six."""
        self._edit_charter_toml(_BASE_TABLES)
        self._draw(["identity", "attention"])
        self.assertEqual(self._drawn(), ["attention", "identity"])

        # Edit one: the two tables go in, and the frame is re-laid-out.
        self._edit_charter_toml(_ADDED_TABLES)
        self._relayout(["identity", "attention", "workspaces", "chats"])
        self.assertEqual(self._drawn(),
                         ["attention", "chats", "identity", "workspaces"])

        # Edit two: reverted. The panes of a component nothing places any more go with it
        # — the direction that was broken, and the direction that made the first orphans.
        self._edit_charter_toml(_BASE_TABLES)
        self._relayout(["identity", "attention"])
        self.assertEqual(self._drawn(), ["attention", "identity"])

        # Edit three: added again. One pane each, not three.
        self._edit_charter_toml(_ADDED_TABLES)
        self._relayout(["identity", "attention", "workspaces", "chats"])
        self.assertEqual(self._drawn(),
                         ["attention", "chats", "identity", "workspaces"])
        self.assertEqual(sorted(state.panes(self.fid)),
                         ["attention", "chats", "identity", "workspaces"])

    def test_a_frame_whose_record_lost_a_pane_does_not_grow_a_second_one(self):
        """The measured state, planted on a real server: the panes exist, the record does
        not name them. Without the reconciliation this is where the second set of panes
        appears — `_relayout` finds the components missing and splits."""
        self._edit_charter_toml(_ADDED_TABLES)
        want = ["identity", "attention", "workspaces", "chats"]
        before = self._draw(want)
        self.assertEqual(len(before), 4)
        # Exactly what `record_panes` leaves behind after a second split for a component:
        # the newest id per component, and nothing at all about the older panes.
        state.record_panes(self.fid, panels={"identity": before["identity"]})

        self._relayout(want)
        self.assertEqual(self._drawn(),
                         ["attention", "chats", "identity", "workspaces"],
                         "a component the record had lost was split a second time")
        self.assertEqual(set(self._window()), set(before.values()),
                         "the panes that came back are new ones, so the old ones were "
                         "orphaned rather than adopted")

    def test_a_component_removed_from_the_config_loses_a_pane_the_record_lost(self):
        """The removal direction with the record in the state the field measured — which
        is how the first orphan pair was made, and the reason the issue calls this "the
        same reconciliation in the other direction, broken today for the same reason".

        A clean add-then-remove works either way, because a correct record can see the
        pane it is about to kill. This is the case that cannot: the tables come out of
        `charter.toml` while the panes they made are invisible to `state.panes`."""
        self._edit_charter_toml(_ADDED_TABLES)
        want = ["identity", "attention", "workspaces", "chats"]
        before = self._draw(want)
        state.record_panes(self.fid, panels={"identity": before["identity"]})

        self._edit_charter_toml(_BASE_TABLES)
        self._relayout(["identity", "attention"])
        self.assertEqual(self._drawn(), ["attention", "identity"],
                         "a component nothing places any more kept its pane, which is "
                         "still running and still drawing")

    def test_a_pane_the_operator_split_survives_a_relayout_that_drops_everything(self):
        """Constraint 1, on a real server and at the sharpest moment: `want` is empty, so
        every panel in this window is going, and the pane the operator split for
        themselves is in the middle of them. It is unmarked — measured on tmux 3.7c and at
        `tmuxctl.FLOOR`, a pane option is NOT inherited by a pane split off the one
        carrying it, so even a pane split out of a PANEL comes out unmarked — and it is
        still there afterwards, with its process alive."""
        self._edit_charter_toml(_ADDED_TABLES)
        self._draw(["identity", "attention", "workspaces", "chats"])
        theirs = self._srv("split-window", "-d", "-t", self.harness, "-l", "3",
                           "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(theirs.returncode, 0, theirs.stderr)
        mine = theirs.stdout.strip()

        self._relayout([])
        alive = self._srv("list-panes", "-t", self.harness,
                          "-F", "#{pane_id} #{pane_dead}").stdout.split("\n")
        self.assertIn(f"{mine} 0", alive,
                      "charter killed a pane it did not split")
        self.assertIn(f"{self.harness} 0", alive, "charter killed the harness pane")
        self.assertEqual(self._drawn(), [], f"a panel outlived `want=[]`: {alive}")
