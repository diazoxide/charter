"""No test forks a charter child that nothing waits for.

#527 closed **which plane** a test-spawned charter lands on. This is the other half: **how
many charter children a test forks at all**, and the answer at b3dbd54 was 66 detached ones
per green run — 27 `charter _version-check`, each a GET to PyPI, 36 `charter gl-refresh`,
each running the forge client for every clone in the workspace, and 3 `charter
frame-gather`. Counted in-process, by wrapping `subprocess.Popen.__init__` before the
`tests` package is imported, because a ``ps | grep`` sample of a running suite matches its
own command line and reports a number that is not real.

**They fire in a test and almost never in real use for the same reason a temp plane is an
isolated one.** Both spawners are throttled by state in `config.STATE_DIR` — a cache TTL
and a cooldown lock — and `PersonaIso` hands every case a fresh one, so the cache is always
absent and the lock never exists. `test_statusline_brand` has said so in a comment for as
long as it has had one: *"a temp STATE_DIR always looks stale"*. Six modules stubbed the
spawner by hand because of it; the other twelve did not, and forked.

**The fix refuses the FORK, and that is what keeps it from being a stub.** The obvious move
— `PersonaIso` stubs `update.maybe_spawn` and `glstate.maybe_spawn` for every case — makes
a test that cannot fail: `test_glstate`, `test_glstate_respawn`, `test_dev_channel` and
`test_self_relaunch_argv` call those functions directly to assert on the argv and the
cooldown, and a base-class stub would swallow them, silently for the "must not raise"
cases. Refusing at `subprocess.Popen` leaves every one of those running its throttle logic
and asserting on it exactly as before; what it refuses is the `start_new_session=True`
child at the end, which is the only part nobody was asserting about.

Every case here is a control. `TheGuardIsNotBlind` makes the refusal happen for real and
`EitherWayOutWorks` exercises both documented escapes, because a tripwire with no usable
exit is a tripwire whoever hits it next deletes.
"""

from __future__ import annotations

import ast
import subprocess
import unittest
from pathlib import Path
from unittest import mock

from charter import glstate, update
from tests import _planeguard
from tests._isolation import PersonaIso, make_plane, no_background_refresh


class WhatIsRefused(unittest.TestCase):
    """The discriminator is a Popen KEYWORD, not a list of subcommand spellings.

    ``start_new_session=True`` is charter's own shape for "a child that outlives this
    process" — `util.detach_self`'s docstring calls it the load-bearing part — so a
    background refresher nobody has written yet is refused on the commit that writes it.
    """

    def test_a_detached_charter_child_is_refused(self):
        with self.assertRaises(_planeguard.BackgroundCharterChild):
            subprocess.Popen(["charter", "_version-check"], start_new_session=True,
                             cwd="/nonexistent-so-nothing-can-actually-start")

    def test_a_charter_child_the_test_will_wait_for_is_not(self):
        """`subprocess.run`, `check_output` and a plain `Popen` all construct the class
        without that keyword. Those children are the ones a test drives on purpose and
        waits for, and this guard is not about them — it is about the ones nobody waits
        for. Refused here only by the CWD, which proves the guard let it past."""
        with self.assertRaises(FileNotFoundError):
            subprocess.Popen(["charter", "_version-check"],
                             cwd="/nonexistent-so-nothing-can-actually-start")

    def test_a_detached_child_that_is_not_charter_is_not_refused(self):
        """The suite forks plenty of detached things that are nobody's control plane."""
        with self.assertRaises(FileNotFoundError):
            subprocess.Popen(["tmux", "-L", "nothing", "kill-server"],
                             start_new_session=True,
                             cwd="/nonexistent-so-nothing-can-actually-start")


class TheGuardIsNotBlind(unittest.TestCase):
    """The refusal, made to happen, and read."""

    def _refusal(self) -> str:
        with self.assertRaises(_planeguard.BackgroundCharterChild) as raised:
            subprocess.Popen(["charter", "gl-refresh"], start_new_session=True,
                             cwd="/nonexistent-so-nothing-can-actually-start")
        return str(raised.exception)

    def test_the_message_names_the_test_the_argv_and_both_ways_out(self):
        said = self._refusal()
        self.assertIn("test_the_message_names_the_test_the_argv_and_both_ways_out", said)
        self.assertIn("charter gl-refresh", said)
        self.assertIn("no_background_refresh", said)
        self.assertIn("allow_background_children", said)
        self.assertIn("#542", said)

    def test_the_message_says_why_a_test_sees_this_and_a_real_machine_does_not(self):
        """The single most useful sentence in it: the reader's next thought is "this never
        happens to me", and the answer is that their plane has a cache and a cooldown lock
        and a test's plane is one line old."""
        said = self._refusal()
        self.assertIn("STATE_DIR", said)
        self.assertIn("cooldown", said)

    def test_the_refusal_is_not_an_exception(self):
        """Both spawners wrap their `Popen` in ``try/except Exception`` and return — so an
        `Exception` here would be swallowed into "the spawn did not happen", which is
        exactly what the guard is trying to report. `_planeguard.RealPlaneWrite` and
        `_envguard.AmbientEnvRead` are `BaseException` for the same reason."""
        self.assertTrue(issubclass(_planeguard.BackgroundCharterChild, BaseException))
        self.assertFalse(issubclass(_planeguard.BackgroundCharterChild, Exception))


class EitherWayOutWorks(unittest.TestCase):
    """Both documented escapes, exercised rather than described."""

    def test_declaring_the_case_wants_one_lets_it_through(self):
        """The A half of an A/B: the same call `WhatIsRefused` above is refused for, made
        after the declaration. `FileNotFoundError` is the REAL `Popen` reporting a cwd that
        does not exist, which is proof the guard is no longer what stopped it."""
        _planeguard.allow_background_children(self)
        with self.assertRaises(FileNotFoundError):
            subprocess.Popen(["charter", "_version-check"], start_new_session=True,
                             cwd="/nonexistent-so-nothing-can-actually-start")

    def test_the_declaration_does_not_outlive_its_case(self):
        """Scoped by `addCleanup`, so the case after this one is guarded again. Run as an
        inner case rather than asserted on the flag, because what matters is the state the
        NEXT test runs in."""
        class Declares(unittest.TestCase):
            def runTest(inner):        # noqa: N805
                _planeguard.allow_background_children(inner)

        Declares().run(unittest.TestResult())
        with self.assertRaises(_planeguard.BackgroundCharterChild):
            subprocess.Popen(["charter", "_version-check"], start_new_session=True,
                             cwd="/nonexistent-so-nothing-can-actually-start")


class StoppingTheSpawnerIsTheOtherWayOut(PersonaIso):
    """`no_background_refresh` — the one spelling of what six modules wrote by hand."""

    def test_without_it_a_render_path_call_forks(self):
        """The control. `make_plane` gives this case a REAL plane, which is what both
        spawners require, and a plane one line old has no cache and no cooldown lock — so
        this is the state twelve modules were in."""
        make_plane(self)
        with self.assertRaises(_planeguard.BackgroundCharterChild):
            update.maybe_spawn()

    def test_with_it_the_same_call_forks_nothing(self):
        make_plane(self)
        no_background_refresh(self)
        update.maybe_spawn()                  # must not raise
        glstate.maybe_spawn([self.tmp], "default")

    def test_it_covers_the_forge_refresh_as_well_as_the_version_check(self):
        """The half the hand-rolled stubs missed. `test_statusline_plane_root_warning`
        stubbed `update.maybe_spawn` and named the reason — "a suite that quietly reaches
        the network is not hermetic" — and then forked a real `charter gl-refresh` past it
        on every dirty-root render (#542)."""
        make_plane(self)
        with self.assertRaises(_planeguard.BackgroundCharterChild):
            glstate.maybe_spawn([self.tmp], "default")


class EveryCharterCharterStartsForItselfIsDetached(PersonaIso):
    """Why one keyword is enough, asked of production rather than assumed of it.

    The guard recognises a background child by ``start_new_session=True``. That is only a
    complete answer while every charter that charter starts for itself passes it — so this
    is where that stops being an assumption. Two cases, behaviour first and source second:
    the behavioural one fails if a spawner drops the flag, the AST one fails if a NEW
    spawner is written without it.
    """

    def _kwargs_of_the_spawn(self, call) -> dict:
        seen = {}

        def spy(args=None, *rest, **kw):
            seen["args"], seen["kw"] = args, kw
            raise RuntimeError("no child, thank you")

        make_plane(self)
        with mock.patch("subprocess.Popen", spy):
            call()
        return seen

    def test_the_version_check_is_started_detached(self):
        seen = self._kwargs_of_the_spawn(update.maybe_spawn)
        self.assertIn("_version-check", seen["args"])
        self.assertTrue(seen["kw"].get("start_new_session"))

    def test_the_forge_refresh_is_started_detached(self):
        seen = self._kwargs_of_the_spawn(
            lambda: glstate.maybe_spawn([self.tmp], "default"))
        self.assertIn("gl-refresh", seen["args"])
        self.assertTrue(seen["kw"].get("start_new_session"))

    #: ``<module>:<function>`` → why this `subprocess.Popen` starts a child nothing
    #: waits for. **Frozen deliberately**, the way
    #: `NoCharterEscapesThroughTheExecFamily.KNOWN` in `test_plane_spawn_guard.py` is: the
    #: point is that a NEW spawner is noticed, and a set that grew by itself would notice
    #: nothing.
    #:
    #: It is also the CONTROL, and that is what earns it over a bare "nothing is
    #: undetached" assertion. Four separate mutations to this case's own reader survived
    #: the version that only checked for undetached spawns — including one that made the
    #: reader answer ``None`` for every `subprocess.Popen`, so it found nothing at all and
    #: reported success for it. A census that does not say what it expected to find cannot
    #: tell "nothing is wrong" from "I looked at nothing".
    DETACHED = {
        "charter/update.py:maybe_spawn":
            "`charter _version-check` — a GET to PyPI, kicked off the status line's own "
            "render path so a render never blocks on the network.",
        "charter/glstate.py:maybe_spawn":
            "`charter gl-refresh` — the forge client over every clone in the workspace, "
            "same render path, same reason.",
        "charter/util.py:detach_self":
            "The helper the other three go through: `charter <args>` in a process that "
            "outlives this one, which is what a hook's `\"async\": true` used to buy.",
        "charter/planegit.py:_spawn_bg_push":
            "A push of the plane's own repo, after the turn that dirtied it.",
        "charter/commands_workspace.py:_spawn_pushbg":
            "The workspace half of the same push, from the Stop hook.",
        "charter/frame/builtin_actions.py:_spawn":
            "A palette action re-entering charter, with the overlay pane's tty already "
            "closed under it — which is why its streams go to DEVNULL as well.",
    }

    @staticmethod
    def _popens_in(source: str, where: str = "<source>") -> dict[str, bool]:
        """``{where:function -> passes start_new_session}`` for every `Popen` in *source*.

        Parsed, not grepped, so a `mock.patch("subprocess.Popen")` written in a docstring
        or a string is what it is. A function of its own, taking TEXT, so
        :meth:`test_it_sees_a_popen_written_either_way` can hand it both spellings and
        watch it answer — the reader's ``ast.Name`` branch is unexercised by charter, which
        writes ``subprocess.Popen`` everywhere, and a mutation deleting it survived the
        version of this case that only ever ran the reader over the real tree. It is not
        dead code: ``from subprocess import Popen`` is a shape `_guard_spawns`'s own
        docstring names as one it has to cover, so the branch stays and gets a control.
        """
        parsed = ast.parse(source)
        for enclosing in ast.walk(parsed):
            for node in ast.iter_child_nodes(enclosing):
                node.parent = enclosing
        found = {}
        for node in ast.walk(parsed):
            if not (isinstance(node, ast.Call) and _called_name(node.func) == "Popen"):
                continue
            found[f"{where}:{_enclosing_name(node)}"] = any(
                kw.arg == "start_new_session" for kw in node.keywords)
        return found

    def _popens_in_charter(self) -> dict[str, bool]:
        """The same reader over every module in `charter/`.

        Deliberately NOT wrapped in a try/except: a file under `charter/` that will not
        parse is a defect in charter, and skipping it would leave whatever it contains
        unguarded while this case carried on looking healthy.
        """
        tree = Path(__file__).resolve().parent.parent
        found = {}
        for path in sorted((tree / "charter").glob("**/*.py")):
            found.update(self._popens_in(path.read_text(encoding="utf-8"),
                                         path.relative_to(tree).as_posix()))
        return found

    def test_it_sees_a_popen_written_either_way(self):
        """The control. Both spellings, both answers, and the enclosing function named —
        a reader that quietly stopped recognising one of them would report "nothing
        undetached" for the best of reasons."""
        seen = self._popens_in(
            "import subprocess\n"
            "from subprocess import Popen\n"
            "def attribute_form():\n"
            "    subprocess.Popen(['x'], start_new_session=True)\n"
            "def bare_name_form():\n"
            "    Popen(['x'])\n")
        self.assertEqual(seen, {"<source>:attribute_form": True,
                                "<source>:bare_name_form": False})

    def test_it_names_a_spawn_that_sits_at_module_scope(self):
        """`_enclosing_name`'s other answer. Nothing in charter does this today; a reader
        that raised on it would fail the day something did, which is the day this census
        matters most."""
        self.assertEqual(self._popens_in("import subprocess\nsubprocess.Popen(['x'])\n"),
                         {"<source>:<module>": False})

    def test_every_charter_child_charter_starts_is_started_detached(self):
        """One keyword is a complete answer only while every self-relaunch passes it."""
        found = self._popens_in_charter()
        undetached = sorted(k for k, detached in found.items() if not detached)
        self.assertEqual(
            undetached, [],
            f"{undetached} starts a child from `subprocess.Popen` without "
            f"`start_new_session=True`. `tests._planeguard.BackgroundCharterChild` "
            f"recognises a background charter child by exactly that keyword, so a spawner "
            f"without it is one no test can be refused for forking (#542). Either detach "
            f"it — every other one in charter does — or, if this child is one its caller "
            f"really waits for, say so in `DETACHED` and give the guard another way to "
            f"see it.")

    def test_the_census_still_finds_every_spawner_it_was_written_against(self):
        """The control. Without it, a reader that quietly stopped recognising
        `subprocess.Popen` would report "nothing undetached" for the best of reasons —
        which is what four mutations to the previous version of this case did."""
        self.assertEqual(sorted(self._popens_in_charter()), sorted(self.DETACHED))


def _enclosing_name(node) -> str:
    """The name of the function *node* sits in, or ``<module>``.

    `ast` carries no parent links, so `_popens_in_charter` adds them on the way past. A
    `Popen` at module scope has no enclosing function and is named for what it is.
    """
    while getattr(node, "parent", None) is not None:
        node = node.parent
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            return node.name
    return "<module>"


def _called_name(func) -> str | None:
    if isinstance(func, ast.Attribute):
        return func.attr
    if isinstance(func, ast.Name):
        return func.id
    return None


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
