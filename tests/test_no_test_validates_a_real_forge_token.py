"""The sixth tripwire's other half: the forge token is not a fixture, and neither is the
answer a forge gives about it.

`test_plane_spawn_guard.TheOperatorsForgeTokenIsNeverReached` covers the REFUSAL —
`gh api`, `gh pr merge`, and since #638 `gh auth status` too. This covers the ANSWER,
which is the only reason that last one could be refused at all: `doctor.check_forge_auth`
is reached by eighteen test modules through `doctor.run_all()`, and it ran
``gh auth status --hostname github.com`` — the operator's real token, validated against a
real forge — 28 times per green run, 20 of them ``glab`` against gitlab.com.

`tests/_forgeprobe.py` answers that probe with a recorded reply. The cases here are the
control: `TheProbeIsAnswered` pins what the answer is, `WhatIsNotAnswered` pins how narrow
the match is, `NothingIsForeclosed` pins that `check_forge_auth` still runs every line it
ran before, and `TheFixtureIsTheOnlyThingInTheWay` takes the fixture away and watches the
refusal happen for real — because a fixture nobody has watched be necessary is a fixture
nobody knows is doing anything.

**There is deliberately no case here that drives `doctor.run_all()` to prove the eighteen
modules are clean.** They prove it themselves: `RealForgeReach` is armed for the whole
suite, so if this fixture stopped answering, every one of them would fail on the spawn
rather than on an assertion written here. A guard that runs against 7,985 tests is a
stronger statement than a nineteenth caller of the same function — and it is the one that
does not depend on whether the machine running the suite happens to have `glab` installed.
"""

from __future__ import annotations

import pathlib
import shutil
import subprocess
import unittest
from unittest import mock

from charter import doctor, util
from charter.forge.github import GitHubForge
from charter.forge.gitlab import GitLabForge
from tests import _forgeprobe, _planeguard


def _the_cli_is_installed(case) -> None:
    """Say the forge CLI is on this machine, because whether it really is decides the
    branch these cases are about.

    `check_forge_auth` returns "Install gh first" without spawning anything when
    `shutil.which` answers None — and what it answers is the machine: GitHub's ubuntu
    runners ship `gh` and not `glab`, this laptop has both, a fresh container has neither.
    A case that only passes where the probe happens to be reachable is the same defect this
    whole file is about, one question along. Nothing here lies about more than existence:
    the argv `doctor` then builds is the CLI's own name, not this path, so the fixture and
    the tripwire both see exactly what production writes.
    """
    real = shutil.which

    def which(cmd, *args, **kw):
        return f"/usr/bin/{cmd}" if cmd in ("gh", "glab") else real(cmd, *args, **kw)

    patcher = mock.patch("shutil.which", side_effect=which)
    patcher.start()
    case.addCleanup(patcher.stop)


class TheProbeIsAnswered(unittest.TestCase):
    """Move one: the preflight gets a verdict, and nothing left this process to get it."""

    def setUp(self):
        _the_cli_is_installed(self)

    def test_the_preflight_reports_authenticated_without_contacting_a_forge(self):
        before = len(_forgeprobe.CALLS)
        result = doctor.check_forge_auth(GitHubForge())
        self.assertEqual(result.status, doctor.OK)
        self.assertEqual(
            result.detail,
            "✓ Logged in to github.com account charter-test-fixture "
            "(tests/_forgeprobe.py — no forge was contacted)")
        self.assertEqual(_forgeprobe.CALLS[before:],
                         [["gh", "auth", "status", "--hostname", "github.com"]])

    def test_the_gitlab_half_is_answered_too_and_it_is_the_larger_half(self):
        """Twenty of the twenty-eight measured children were `glab`, not `gh`: a plane that
        declares no `[[forge]]` block falls back to `GitLabForge`, and that is every plane
        `PersonaIso` hands out. A fix aimed only at the issue's `gh` would have left them."""
        before = len(_forgeprobe.CALLS)
        result = doctor.check_forge_auth(GitLabForge())
        self.assertEqual(result.status, doctor.OK)
        self.assertIn("gitlab.com", result.detail)
        self.assertEqual(_forgeprobe.CALLS[before:],
                         [["glab", "auth", "status", "--hostname", "gitlab.com"]])

    def test_the_backends_own_probe_is_answered_by_the_same_wrapper(self):
        """`Forge.check_auth` builds the identical argv. One wrapper on `charter.util.run`
        answers both, because both reach it by attribute lookup on the same module."""
        GitHubForge().check_auth()          # must not raise, and must not spawn
        self.assertEqual(_forgeprobe.CALLS[-1],
                         ["gh", "auth", "status", "--hostname", "github.com"])

    def test_the_reply_lands_on_stderr_where_gh_really_writes_it(self):
        """Which keeps `check_forge_auth`'s ``(proc.stdout or "") + (proc.stderr or "")``
        load-bearing. Answered on stdout, that concatenation would be a line no test in
        this suite could kill."""
        proc = util.run(["gh", "auth", "status", "--hostname", "github.com"], check=False)
        self.assertEqual(proc.stdout, "")
        self.assertEqual(proc.returncode, 0)
        self.assertEqual(
            proc.stderr,
            "github.com\n"
            "  ✓ Logged in to github.com account charter-test-fixture "
            "(tests/_forgeprobe.py — no forge was contacted)\n")

    def test_the_reply_names_itself_so_a_doctor_row_is_not_mistaken_for_a_session(self):
        """These rows are printed by cases that capture doctor's output. One reading
        "Logged in to github.com account <the operator>" would be a fixture impersonating a
        real session."""
        proc = util.run(["gh", "auth", "status", "--hostname", "github.com"], check=False)
        self.assertIn("charter-test-fixture", proc.stderr)
        self.assertIn("_forgeprobe.py", proc.stderr)

    def test_a_probe_naming_no_host_still_gets_an_answer(self):
        """Both backends pass `--hostname` today, so this is the shape charter does not
        write — and a fixture that raised or returned nothing on it would be a fixture that
        breaks on the first caller that stops passing it."""
        proc = util.run(["gh", "auth", "status"], check=False)
        self.assertIn("Logged in to a forge", proc.stderr)

    def test_a_full_path_is_recognised_as_the_same_cli(self):
        """`shutil.which` has already resolved the program on some paths and not on
        others, so the match is on the basename."""
        self.assertTrue(_forgeprobe.asks_a_forge_who_it_is(
            ["/opt/homebrew/bin/glab", "auth", "status"]))


class WhatIsNotAnswered(unittest.TestCase):
    """Move two: the fixture is as narrow as it can be, so everything else is refused
    rather than quietly pretended at."""

    def test_only_auth_status_is_answered(self):
        for argv in (["gh", "auth", "login"], ["gh", "auth", "token"],
                     ["gh", "api", "user"], ["gh", "status", "auth"],
                     ["glab", "mr", "merge", "14"]):
            with self.subTest(argv=argv):
                self.assertFalse(_forgeprobe.asks_a_forge_who_it_is(argv))

    def test_a_bare_forge_cli_with_nothing_after_it_is_not_the_probe(self):
        self.assertFalse(_forgeprobe.asks_a_forge_who_it_is(["gh"]))
        self.assertFalse(_forgeprobe.asks_a_forge_who_it_is(["gh", "auth"]))

    def test_an_empty_command_is_answered_rather_than_raising(self):
        """`util.run` accepts an empty command — it asks `cmd and cmd[0] == "git"` before
        indexing, and this wrapper stands in front of it. A filter that raised where the
        function it wraps does not would be the fixture breaking a call it has no opinion
        about."""
        self.assertFalse(_forgeprobe.asks_a_forge_who_it_is([]))

    def test_a_binary_that_is_not_a_forge_cli_is_never_answered(self):
        """`git` is spawned by this suite constantly, and `op auth status` is a shape the
        vault tripwire owns."""
        self.assertFalse(_forgeprobe.asks_a_forge_who_it_is(["git", "auth", "status"]))
        self.assertFalse(_forgeprobe.asks_a_forge_who_it_is(["op", "auth", "status"]))

    def test_the_names_are_the_tripwires_names(self):
        """One reader of "what is a forge CLI", asked of `registry.KINDS` through
        `_planeguard._forge_clis`. Two copies would drift, and the drift would be silent:
        the fixture would stop answering a CLI the guard still refused, and eighteen
        modules would go red for a reason neither file explained."""
        self.assertEqual(_forgeprobe._CLIS, _planeguard._forge_clis())
        self.assertEqual(_forgeprobe._CLIS, frozenset({"gh", "glab"}))

    def test_a_real_run_still_goes_through_to_the_real_thing(self):
        """The half that proves the wrapper is a filter and not a replacement: everything
        that is not the probe reaches `subprocess` exactly as before."""
        proc = util.run(["git", "--version"], check=False)
        self.assertEqual(proc.returncode, 0)
        self.assertIn("git version", proc.stdout)


class NothingIsForeclosed(unittest.TestCase):
    """Move three: the check still runs. `mock.patch("charter.doctor.check_forge_auth")`
    would have been one line and would have left the function's own branches dark for the
    whole suite — the trap #542 names. Answering the CHILD instead leaves every line of it
    executing on a recorded reply, and these cases are what say so."""

    def setUp(self):
        _the_cli_is_installed(self)

    def test_a_forge_that_answers_without_logged_in_is_still_a_failure(self):
        """F3's shape: `auth status` can exit 0 while saying something else entirely."""
        with mock.patch.object(util, "run", return_value=subprocess.CompletedProcess(
                [], 0, "", "not logged in to any hosts\n")):
            result = doctor.check_forge_auth(GitHubForge())
        self.assertEqual(result.status, doctor.FAIL)
        self.assertIn("gh auth login", result.hint)

    def test_a_probe_that_never_answers_is_a_warning_naming_the_timeout(self):
        with mock.patch.object(util, "run", side_effect=util.ProcTimeout(["gh"], 5.0)):
            result = doctor.check_forge_auth(GitHubForge())
        self.assertEqual(result.status, doctor.WARN)
        self.assertIn("timed out", result.detail)

    def test_a_test_that_wants_its_own_answer_still_wins(self):
        """`mock.patch` replaces this fixture and puts it back afterwards, which is what
        keeps the twenty-odd cases that already patch `charter.util.run` working."""
        with mock.patch.object(util, "run", return_value=subprocess.CompletedProcess(
                [], 0, "✓ Logged in to github.com as somebody-else\n", "")):
            result = doctor.check_forge_auth(GitHubForge())
        self.assertEqual(result.detail, "✓ Logged in to github.com as somebody-else")
        self.assertEqual(doctor.check_forge_auth(GitHubForge()).detail,
                         "✓ Logged in to github.com account charter-test-fixture "
                         "(tests/_forgeprobe.py — no forge was contacted)")


class TheFixtureIsTheOnlyThingInTheWay(unittest.TestCase):
    """Move four, and the one that makes the other three mean something: take the answer
    away and the preflight really does reach for the operator's forge CLI."""

    def setUp(self):
        _the_cli_is_installed(self)

    def test_without_the_fixture_the_preflight_is_refused_by_name(self):
        with mock.patch.object(util, "run", _forgeprobe._ORIGINAL), \
                self.assertRaises(_planeguard.RealForgeReach) as caught:
            doctor.check_forge_auth(GitHubForge())
        self.assertIn("gh auth status --hostname github.com", str(caught.exception))

    def test_the_wrapper_is_installed_rather_than_inherited(self):
        """Under an operator who happens to be logged out, the fixture's OK and a real
        `gh`'s FAIL are both just a doctor row, and every assertion above would pass with
        `install()` deleted — on that machine. This asks the question with a different
        answer either way: whose `run` is `charter.util.run`?"""
        self.assertEqual(util.run.__module__, "tests._forgeprobe")
        self.assertIsNot(util.run, _forgeprobe._ORIGINAL)

    def test_installing_twice_does_not_wrap_a_pin_a_running_test_has_put_in_place(self):
        """`install()` is reachable twice — `tests` can be imported by a child process that
        also imports a test module — and a second wrap would put the fixture on top of
        whatever a case had patched onto `util.run`, silently discarding it."""
        before = util.run
        _forgeprobe.install()
        self.assertIs(util.run, before)

    def test_the_answer_is_installed_below_the_refusal(self):
        """Order, read off the source because that is where it lives. The tripwire is armed
        first, so a forge spawn this fixture does not cover fails by name instead of
        reaching github.com. A comment saying so is not a test."""
        src = (pathlib.Path(__file__).parent / "__init__.py").read_text()
        self.assertLess(src.index("_planeguard.install()"),
                        src.index("_forgeprobe.install()"))


if __name__ == "__main__":
    unittest.main()
