"""The third tripwire: the shell the suite was launched from is not a fixture either.

`tests/_planeguard.py` refuses a write into the operator's real `.charter/` (#402) and a
read of the setting their own `charter.toml` declares (#459). This is the same move for
what their *shell* declares. Run the suite inside a live charter frame and sixteen tests
failed that fail nowhere else (#519, #521); on #525 the same leak went the other way and a
mutation that dies with a clean environment **survived** under an ambient
``$CHARTER_WORKSPACE`` (#528). One class of defect, both signs.

**Every case here is a control.** A guard nobody has watched fail is a guard nobody knows
works, so `TheGuardIsNotBlind` makes the refusal happen for real and
`EveryWayOutOfTheRefusalWorks` exercises each documented escape — because a tripwire with
no usable exit is deleted by whoever hits it next. `WhatIsScrubbed` pins the other half,
which is silent by design: the values are gone from this process and from anything it
spawns, so a bulk `dict(os.environ)` and a subprocess give the same answer on every
machine without anybody having to declare anything.
"""

from __future__ import annotations

import json
import os
import pathlib
import subprocess
import sys
import unittest
from unittest import mock

from charter import commands_frame, legacyenv, session, workspace
from tests import _envguard
from tests._isolation import PersonaIso, child_plane_env


class WhatIsGuarded(unittest.TestCase):
    """The loud set is DERIVED. These cases are what stops it drifting into a spelling."""

    def test_every_variable_a_frame_exports_is_guarded(self):
        """`_FRAME_IDENTITY` is production's own answer to "which variables must be THIS
        session's rather than the launcher's", and its docstring commits the next such
        variable to that list. Asking it is what makes a variable invented next month
        guarded on the commit that invents it, instead of the commit that debugs it."""
        for name in commands_frame._FRAME_IDENTITY:
            with self.subTest(variable=name):
                self.assertIn(name, _envguard._LOUD)

    def test_every_variable_a_pane_is_identified_by_is_guarded(self):
        """`session.terminal` walks `_PANE_ID_VARS`; `_WINDOW_ID_VARS` is the chain it
        deliberately does not walk, guarded anyway because that decision can change."""
        for name in session._PANE_ID_VARS + session._WINDOW_ID_VARS:
            with self.subTest(variable=name):
                self.assertIn(name, _envguard._LOUD)

    def test_the_two_session_id_rungs_below_the_frame_are_guarded(self):
        """`session.current` falls from `$CHARTER_SESSION_ID` to
        `$CLAUDE_CODE_SESSION_ID`. A guard that covered only the first would leave the
        second reachable, which is the rung `test_workspace_lock` actually reads."""
        self.assertIn("CHARTER_SESSION_ID", _envguard._LOUD)
        self.assertIn("CLAUDE_CODE_SESSION_ID", _envguard._LOUD)

    def test_tmux_itself_is_guarded_not_only_its_pane(self):
        """The pair that decides "am I inside a tmux". `$TMUX_PANE` arrives from
        `_PANE_ID_VARS`; `$TMUX` has no constant to derive it from and is spelled once."""
        self.assertIn("TMUX", _envguard._LOUD)
        self.assertIn("TMUX_PANE", _envguard._LOUD)

    def test_every_name_charter_was_renamed_from_is_scrubbed(self):
        """charter's OWN former namespace, which the first version of this guard missed.

        It missed them the way lists get missed: ``$EDM_HOME``, ``$EDM_WORKSPACE`` and
        ``$EDM_PERSONA`` are outside ``CHARTER_``/``CLAUDE`` by spelling, so nothing in the
        property covered them — while `legacyenv.warn` reads all three at import of
        `charter.config` and prints a 133-column line to stderr for each one still set, in
        this process and in every subprocess this suite spawns. ``$EDM_WORKSPACE`` alone
        made `test_cli_smoke` fail on the width of `charter init`'s output and
        `test_a_deny_survives_a_broken_channel` fail on an exit status, on the machine of
        the person most likely to still have it exported, and neither failed in CI (#540).

        Scrubbed but deliberately NOT loud. Tier two is for names where "unset" is a claim
        about the world the test runs in — whose session is this, is this a real tmux,
        which workspace outranks every pointer. charter never honors these VALUES at all;
        what leaked was the banner, not an answer, so removing the name is the whole fix
        and a refusal would be noise on a read that decides nothing.
        """
        for name in legacyenv.NAMES:
            with self.subTest(variable=name):
                self.assertTrue(
                    _envguard._in_namespace(name),
                    f"${name} is in `charter.legacyenv.RENAMES`, so `legacyenv.warn` "
                    f"prints a 133-column stderr banner for it at import of "
                    f"`charter.config` — here and in every subprocess this suite spawns — "
                    f"whenever the operator has it exported. Unscrubbed, this suite "
                    f"answers differently on their machine than in CI (#540). Two ways "
                    f"out:\n"
                    f"  - derive it: `tests._envguard._scrubbed_names` asks "
                    f"`legacyenv.NAMES` for exactly this set, so a name that reached "
                    f"`RENAMES` from somewhere else needs its source asked for there too; "
                    f"or\n"
                    f"  - retire it: if charter no longer warns about ${name}, drop its "
                    f"pair from `legacyenv.RENAMES` — the warning and the scrub are two "
                    f"halves of one fact and must not disagree.")

    def test_every_loud_name_is_also_scrubbed(self):
        """Refusing a read the operator's value could still reach is half a guard: the
        loud set has to be a SUBSET of what install removed, or a name could be refused
        in-process and inherited by a subprocess at the same time."""
        for name in _envguard._LOUD:
            with self.subTest(variable=name):
                self.assertTrue(
                    _envguard._in_namespace(name),
                    f"${name} is refused but not scrubbed, so a subprocess would still "
                    f"see the operator's value")

    def test_the_refusal_is_not_an_exception(self):
        """`root.find_root` swallows `OSError`, `config.derive` catches everything, and
        `statusline` wraps whole panels in `except Exception` so a broken plane cannot
        cost the status line. A tripwire any of those could catch would be reported as
        "no session" and the test would pass, wrongly."""
        self.assertTrue(issubclass(_envguard.AmbientEnvRead, BaseException))
        self.assertFalse(issubclass(_envguard.AmbientEnvRead, Exception))

    def test_the_guard_is_armed_by_wrapping_the_test_runner(self):
        """Structural, not per-test: `unittest.TestCase.run` is what arms it, so no test
        can opt out by forgetting a base class — the property `_planeguard` gets from
        wrapping `open` and this gets from wrapping the one call every test goes through."""
        self.assertEqual(getattr(unittest.TestCase.run, "__module__", None),
                         "tests._envguard")

    def test_os_environ_is_the_guarded_mapping(self):
        """`os.getenv`, `subprocess` and `posixpath.expanduser` all look this up on the
        `os` module at call time, so replacing the object is what covers every reader."""
        self.assertIsInstance(os.environ, _envguard._GuardedEnviron)


class TheGuardIsNotBlind(unittest.TestCase):
    """A plain `TestCase` that declares nothing — the state every case below is about."""

    def test_asking_who_this_session_is_gets_refused(self):
        with self.assertRaises(_envguard.AmbientEnvRead):
            session.current()

    def test_resolving_a_workspace_gets_refused(self):
        """`workspace.resolve`'s top rung, and the one that produced #528's false green:
        both sides of an assertion collapsed to the ambient pin and the test agreed with
        itself."""
        with self.assertRaises(_envguard.AmbientEnvRead):
            workspace.resolve()

    def test_the_message_names_the_test_the_variable_and_both_ways_out(self):
        """`unittest` puts the test's name in the failure header; the message repeats it
        so an excerpt quoted into an issue or a CI log still says which test it was — and
        carries both exits, because a refusal with no remedy gets deleted."""
        with self.assertRaises(_envguard.AmbientEnvRead) as caught:
            session.current()
        message = str(caught.exception)
        self.assertIn("test_the_message_names_the_test_the_variable_and_both_ways_out",
                      message)
        self.assertIn("CHARTER_SESSION_ID", message)
        self.assertIn("PersonaIso", message)
        self.assertIn("patch.dict", message)
        self.assertIn("unset", message)

    def test_every_spelling_of_a_targeted_read_is_closed(self):
        """`session.current` uses `.get`, but the next reader may not, and
        `MutableMapping` routes several of these through `__getitem__` only if nothing
        overrides them. Probed rather than reasoned about."""
        for label, read in (("[]", lambda: os.environ["CHARTER_SESSION_ID"]),
                            ("get", lambda: os.environ.get("CHARTER_SESSION_ID")),
                            ("get w/ default",
                             lambda: os.environ.get("CHARTER_SESSION_ID", "")),
                            ("in", lambda: "CHARTER_SESSION_ID" in os.environ),
                            ("getenv", lambda: os.getenv("CHARTER_SESSION_ID")),
                            ("setdefault",
                             lambda: os.environ.setdefault("CHARTER_SESSION_ID", "x"))):
            with self.subTest(read=label):
                with self.assertRaises(_envguard.AmbientEnvRead):
                    read()

    def test_a_refused_setdefault_did_not_set_anything(self):
        """Refused at the front door, the way `_planeguard` refuses `rmtree` before it
        scans: a tripwire that raised after the write would leave the variable set for
        every test that runs after this one."""
        with self.assertRaises(_envguard.AmbientEnvRead):
            os.environ.setdefault("CHARTER_SESSION_ID", "tampered")
        self.assertNotIn("CHARTER_SESSION_ID", os.environ.copy())

    def test_a_variable_outside_the_namespace_reads_normally(self):
        """The guard is not "the environment is unreadable". `$PATH` still answers, and
        it has to: `subprocess`, `tempfile` and every `git` call in the suite need it."""
        self.assertTrue(os.environ["PATH"])

    def test_bulk_reads_are_deliberately_not_refused(self):
        """`mock.patch.dict` snapshots the whole mapping on entry and
        `commands_frame._frame_env` builds a child environment out of it. A bulk read that
        exploded would refuse the very calls that isolate a test. Safe because the values
        are GONE, which the assertions here are the proof of."""
        for label, read in (("dict()", lambda: dict(os.environ)),
                            ("copy", lambda: os.environ.copy()),
                            ("list", lambda: list(os.environ)),
                            ("len", lambda: len(os.environ))):
            with self.subTest(read=label):
                read()
        self.assertNotIn("CHARTER_SESSION_ID", dict(os.environ))
        self.assertNotIn("TMUX", dict(os.environ))


class WhatIsScrubbed(unittest.TestCase):
    """The silent half. Nothing here declares anything, because there is nothing to see."""

    def test_no_charter_variable_survived_into_this_process(self):
        """Install removes them, so the answer is the same in a frame and in CI — which is
        what makes the bulk reads above safe and the 108 `patch.dict` calls that omit
        `clear=True` harmless for the names that mattered (#528)."""
        leaked = [k for k in os.environ.copy() if _envguard._in_namespace(k)]
        self.assertEqual(leaked, [], f"{leaked} reached the suite from the shell")

    def test_a_subprocess_does_not_inherit_the_operators_session(self):
        """The half `_planeguard` says it cannot see. A spawned `charter` resolves its own
        plane from its own environment, so the scrub has to be a real `unsetenv` and not a
        Python-side illusion — otherwise a subprocess in this suite keys state by the
        operator's live frame id.

        The child is asked about the whole guarded set rather than a hand-copied corner of
        it: the probe used to spell four names, and the three it did not spell were
        precisely the ones that reached `charter init`'s stderr through a subprocess and
        cost two failures (#540)."""
        probe = (f"import os;print(' '.join(k for k in os.environ "
                 f"if k.startswith({_envguard._PREFIXES!r}) "
                 f"or k in {tuple(sorted(_envguard._SCRUB_NAMES))!r}))")
        out = subprocess.run([sys.executable, "-c", probe],
                             capture_output=True, text=True, check=True).stdout.split()
        self.assertEqual(out, [], f"{out} reached a child process")

    def test_an_ambient_session_reaching_a_fresh_suite_is_removed_before_charter_loads(self):
        """The two cases above cannot fail on a machine that had nothing to leak.

        That is the "probe whose evidence cannot distinguish 'clean' from 'never ran'"
        shape, and on a CI runner — which exports none of these — it describes both of
        them exactly. So this one MANUFACTURES the operator's shell in a child process and
        watches the guard take it apart: a session id, a pinned workspace and a live tmux
        go in, and what comes back is what the suite could still see. It fails on every
        machine, including the one where the other two are vacuously true.

        A child rather than an in-process check because the scrub runs once, at import of
        the `tests` package, and by the time any test executes it has long since happened.

        The child imports that package, which imports `charter.config`, which resolves a
        plane — so it runs from a THROWAWAY plane with ``$PYTHONPATH`` carrying the tree,
        rather than from a cwd of the checkout. This was the last child in the suite still
        resolving the operator's live plane (`_planeguard.RealPlaneSpawn`). ``$CHARTER_ROOT``
        would not have fixed it: the scrub this case is about removes that pointer *before*
        charter loads, so for a child that imports this package the cwd is the only lever.

        ``$EDM_WORKSPACE`` is planted alongside them because it is the one that got away:
        every other name here was already gone when this case was written, and that one
        walked straight through the property into `charter init`'s stderr (#540). Planting
        it is what makes this case fail on the tree that had the hole.
        """
        planted = {"CHARTER_SESSION_ID": "ambient-sess", "CHARTER_WORKSPACE": "ambient-ws",
                   "TMUX": "/tmp/tmux-501/default,1,0", "TMUX_PANE": "%9",
                   "EDM_WORKSPACE": "legacy-ws"}
        probe = ("import json, os, tests;"
                 "from tests import _envguard;"
                 "print(json.dumps({"
                 "'left': sorted(k for k in os.environ.copy() "
                 "if _envguard._in_namespace(k)),"
                 "'recovered': sorted(_envguard.scrubbed())}))")
        tree = pathlib.Path(__file__).resolve().parent.parent
        plane, _ = child_plane_env(self)
        out = subprocess.run(
            [sys.executable, "-c", probe],
            env={**os.environ.copy(), **planted, "PYTHONPATH": str(tree)},
            cwd=str(plane),
            capture_output=True, text=True, check=True)
        got = json.loads(out.stdout)
        self.assertEqual(got["left"], [],
                         f"{got['left']} survived the scrub into a suite launched from a "
                         f"live frame")
        for name in planted:
            with self.subTest(variable=name):
                self.assertIn(name, got["recovered"])

    def test_what_was_removed_is_still_available_to_anything_that_needs_it(self):
        """Scrubbed, not destroyed. A test that genuinely has to know what the operator's
        shell held has an honest place to get it, so nobody's answer is to disable the
        guard. Asserted as a shape rather than a value: on a CI runner there is nothing to
        recover and the dict is empty, which is correct there and must stay green."""
        self.assertIsInstance(_envguard.scrubbed(), dict)
        for name in _envguard.scrubbed():
            with self.subTest(variable=name):
                self.assertTrue(_envguard._in_namespace(name))


class EveryWayOutOfTheRefusalWorks(unittest.TestCase):
    """The other direction: each documented escape, exercised rather than described."""

    def test_naming_the_variable_unset_makes_the_read_answer(self):
        _envguard.unset("CHARTER_SESSION_ID", "CLAUDE_CODE_SESSION_ID")
        self.assertIsNone(session.current())

    def test_declaring_the_whole_environment_unset_makes_the_read_answer(self):
        _envguard.unset_all()
        self.assertIsNone(session.current())

    def test_patching_a_value_in_makes_the_read_return_it(self):
        """Isolation is not "there is never a session" — it is "the session is what the
        FIXTURE says". A case about being inside a frame writes it and gets it back."""
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "sess-abc"}):
            self.assertEqual(session.current(), "sess-abc")

    def test_clearing_the_environment_is_itself_a_declaration(self):
        """`clear=True` is the fix #528 asks 108 call sites to add. If it did not count as
        a statement, the recommended remedy would trip the guard it was written for."""
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertIsNone(session.current())

    def test_popping_a_variable_says_it_is_unset_without_reading_it(self):
        """`os.environ.pop("TMUX", None)` is the most direct way there is of saying "not
        inside a tmux", and `MutableMapping.pop` is written in terms of `self[key]` — so
        without an override the guard would refuse the thing it was being told."""
        os.environ.pop("TMUX", None)
        self.assertNotIn("TMUX", os.environ)

    def test_a_patched_value_survives_an_unrelated_patch_dict_in_the_same_test(self):
        """`mock.patch.dict` restores by CLEARING the whole mapping and refilling it, which
        is why a declaration is not stored as a value. If it were, the block below would
        destroy the one above it and the rest of the test would refuse."""
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "sess-abc"}):
            with mock.patch.dict(os.environ, {"SOMETHING_ELSE": "1"}):
                pass
            self.assertEqual(session.current(), "sess-abc")


class IsolationIsTheOtherWayOut(PersonaIso):
    def test_persona_iso_answers_as_a_shell_that_never_saw_charter(self):
        self.assertIsNone(session.current())

    def test_a_persona_iso_fixture_can_still_declare_a_session(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "sess-abc"}):
            self.assertEqual(session.current(), "sess-abc")


class TheDeclarationDoesNotOutliveItsTest(unittest.TestCase):
    """The failure mode a suite-wide flag has, and this one must not.

    `PersonaIso` already carried one of these: a subclass named its cleanup `_restore`, the
    base's `addCleanup(self._restore)` bound the SUBCLASS's method, and 1186 tests ran
    afterwards against a leaked plane that looked exactly like a passing run. A declaration
    that leaked the same way would leave every later test reading the operator's shell
    while reporting green — the guard silently gone rather than loudly wrong.
    """

    def test_a_completed_isolated_case_leaves_the_guard_armed(self):
        case = IsolationIsTheOtherWayOut(
            "test_persona_iso_answers_as_a_shell_that_never_saw_charter")
        result = unittest.TestResult()
        case.run(result)
        self.assertTrue(result.wasSuccessful(), result.errors or result.failures)
        with self.assertRaises(_envguard.AmbientEnvRead):
            session.current()

    def test_a_completed_declaring_case_leaves_the_guard_armed(self):
        case = EveryWayOutOfTheRefusalWorks(
            "test_declaring_the_whole_environment_unset_makes_the_read_answer")
        result = unittest.TestResult()
        case.run(result)
        self.assertTrue(result.wasSuccessful(), result.errors or result.failures)
        with self.assertRaises(_envguard.AmbientEnvRead):
            session.current()

    def test_this_tests_own_declaration_survives_running_an_inner_case(self):
        """The save/restore the inner run needs, from the outer side. A plain reset would
        disarm this test's own statement halfway through, which is the more confusing
        direction: the failure would land on a line that has nothing to do with the cause.
        """
        _envguard.unset_all()
        case = IsolationIsTheOtherWayOut(
            "test_persona_iso_answers_as_a_shell_that_never_saw_charter")
        case.run(unittest.TestResult())
        self.assertIsNone(session.current())


if __name__ == "__main__":
    unittest.main()
