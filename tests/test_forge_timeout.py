"""#324 — no forge CLI call may be unbounded.

`util.run` has taken a ``timeout`` since the day its docstring named ``gh api`` and
``glab api`` as paths that "could hang indefinitely". Not one forge call site passed
one. On the status-line path that is a detached process holding the forge credential
with no bound on its life, on a surface that asks for another every two minutes.

Two numbers, not one, and the split is the one that already exists in both backends.
The permissive path (`_api`, `ci_status`) feeds the status line: failing is nearly free
— a blank column, retried on the next refresh — and it is the path that stacks. The
strict path (`_paged_strict`, `_api_strict`, `repo_tree_strict`) is human-invoked, costs
a whole `discover` run when it gives up, and pulls a hundred records at a time.
"""

from __future__ import annotations

import unittest
from types import SimpleNamespace
from unittest import mock

from charter import glstate, util
from charter.forge import base
from charter.forge.github import GitHubForge
from charter.forge.gitlab import GitLabForge


#: `{}` rather than `[]`: it parses as a legal, empty response for every shape these
#: backends read — a page of records, a tree, and GitHub's GraphQL rollup object.
def _proc(stdout="{}", rc=0, stderr=""):
    return SimpleNamespace(stdout=stdout, stderr=stderr, returncode=rc)


def _drive_everything(forge, response):
    """Call every method on *forge* that shells out, and return the recorded
    `util.run` calls. Nothing here asserts — the tests do."""
    repo = {"path_with_namespace": "acme/api", "default_branch": "main", "id": 7}
    with mock.patch("charter.util.run", return_value=response) as m:
        forge.check_auth()
        forge.list_repos("acme")
        forge.repo_tree(repo)
        forge.repo_tree_strict(repo)
        forge.open_change("acme/api", "main")
        forge.ci_status("acme/api", "main")
    return m.call_args_list


class TheBoundsAreSane(unittest.TestCase):
    def test_both_bounds_exist_and_are_positive(self):
        for name in ("STATUS_TIMEOUT", "LIST_TIMEOUT"):
            v = getattr(base, name)
            self.assertIsInstance(v, (int, float), name)
            self.assertGreater(v, 0, name)

    def test_the_status_bound_is_the_tighter_of_the_two(self):
        self.assertLess(base.STATUS_TIMEOUT, base.LIST_TIMEOUT)

    def test_a_status_call_cannot_outlive_the_respawn_cooldown(self):
        """The number has to mean something against the surface it serves. A single
        status call that could run longer than `SPAWN_COOLDOWN` would be asking the
        cooldown to be the only thing standing between a hung CLI and a second one."""
        self.assertLess(base.STATUS_TIMEOUT, glstate.SPAWN_COOLDOWN)


class NoForgeCallIsUnbounded(unittest.TestCase):
    """The sweep. Written as "every recorded call" rather than a list of the four
    known sites, because the defect in #324 was not that one site forgot — it was that
    the parameter existed and nobody reached for it."""

    def _assert_all_bounded(self, calls):
        # Precondition: something was actually invoked. Without this the loop below is
        # vacuously true over an empty list, which is the shape of a test that passes
        # while proving nothing.
        self.assertGreater(len(calls), 0, "no CLI call was recorded at all")
        for c in calls:
            argv = " ".join(c.args[0])
            self.assertIn("timeout", c.kwargs, f"unbounded call: {argv}")
            self.assertIsNotNone(c.kwargs["timeout"], f"unbounded call: {argv}")
            self.assertGreater(c.kwargs["timeout"], 0, argv)

    def test_github(self):
        self._assert_all_bounded(_drive_everything(GitHubForge(), _proc()))

    def test_gitlab(self):
        self._assert_all_bounded(
            _drive_everything(GitLabForge(), _proc(stderr="Logged in")))


class EachCallGetsTheBudgetForItsJob(unittest.TestCase):
    def _timeout_of(self, forge, call, response=_proc()):
        with mock.patch("charter.util.run", return_value=response) as m:
            call(forge)
        self.assertTrue(m.call_args_list, "nothing was invoked")
        return {c.kwargs.get("timeout") for c in m.call_args_list}

    def test_github_status_calls_use_the_status_bound(self):
        for call in (lambda f: f.open_change("acme/api", "main"),
                     lambda f: f.ci_status("acme/api", "main")):
            self.assertEqual(self._timeout_of(GitHubForge(), call),
                             {base.STATUS_TIMEOUT})

    def test_gitlab_status_calls_use_the_status_bound(self):
        for call in (lambda f: f.open_change("acme/api", "main"),
                     lambda f: f.ci_status("acme/api", "main")):
            self.assertEqual(self._timeout_of(GitLabForge(), call),
                             {base.STATUS_TIMEOUT})

    def test_github_listing_uses_the_listing_bound(self):
        self.assertEqual(self._timeout_of(GitHubForge(), lambda f: f.list_repos("acme")),
                         {base.LIST_TIMEOUT})

    def test_gitlab_listing_uses_the_listing_bound(self):
        self.assertEqual(self._timeout_of(GitLabForge(), lambda f: f.list_repos("acme")),
                         {base.LIST_TIMEOUT})


class ATimeoutIsReportedInEachPathSVocabulary(unittest.TestCase):
    """`ProcTimeout` is a `RuntimeError`. Left to escape, it reaches `cli.main` — which
    catches only `KeyboardInterrupt` — as a traceback. Each path already says how it
    reports a failure, and a timeout is a failure."""

    def _timing_out(self):
        return mock.patch("charter.util.run",
                          side_effect=util.ProcTimeout(["gh", "api"], 10.0))

    def test_a_status_call_degrades_to_none_rather_than_raising(self):
        """`_api`'s own docstring: "callers feed the status line, which renders every
        turn and must never crash"."""
        for forge in (GitHubForge(), GitLabForge()):
            with self.subTest(forge.kind), self._timing_out():
                self.assertIsNone(forge.open_change("acme/api", "main"))
                self.assertIsNone(forge.ci_status("acme/api", "main"))

    def test_a_listing_call_raises_forge_error_not_a_bare_runtime_error(self):
        for forge in (GitHubForge(), GitLabForge()):
            with self.subTest(forge.kind), self._timing_out():
                with self.assertRaises(base.ForgeError) as caught:
                    forge.list_repos("acme")
                self.assertIn("10", str(caught.exception),
                              "the report must name the bound that was hit")

    def test_check_auth_raises_forge_error_on_a_timeout(self):
        for forge in (GitHubForge(), GitLabForge()):
            with self.subTest(forge.kind), self._timing_out():
                with self.assertRaises(base.ForgeError):
                    forge.check_auth()

    def test_repo_tree_still_degrades_to_empty_on_a_timeout(self):
        """The permissive tree read is documented to "degrade to [] on any failure,
        never raise"."""
        for forge in (GitHubForge(), GitLabForge()):
            with self.subTest(forge.kind), self._timing_out():
                self.assertEqual(forge.repo_tree({"path_with_namespace": "a/b", "id": 1}),
                                 [])


if __name__ == "__main__":
    unittest.main()
