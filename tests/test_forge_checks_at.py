"""`checks_at` and `request_for` — the two reads #561 did not have.

**The claim under test, in one sentence:** a head sha with zero checks is `NOT RUN`, a head
charter could not ask about is `UNKNOWN`, and neither is `PASSED`. `gh pr checks` says "no
checks reported" and `mergeStateStatus` says `CLEAN` when no run was ever created,
identically to a clean pass, and the merge button is offered anyway — which is how an
unverified branch nearly landed on this repository.

Every reply here is a **recorded fixture**, and nothing in this file reaches a network:
per CONTRIBUTING, a test that hits the forge is a flaky test, and CI has no credentials.
That bounds what the suite can prove — it proves charter reads a fixture correctly, and
#561 is also a claim about what the forge does — which is why the branch reports a manual
observation against a real head sha as well.
"""
from __future__ import annotations

import json
import unittest
from types import SimpleNamespace
from unittest import mock

from charter.forge import base
from charter.forge.github import GitHubForge
from charter.forge.gitlab import GitLabForge


def _proc(stdout="", rc=0, stderr=""):
    return SimpleNamespace(stdout=stdout, stderr=stderr, returncode=rc)


#: A real `gh api repos/<o>/<r>/commits/<sha>/check-runs` reply for a commit with no run,
#: captured against github.com. `total_count: 0` — and `statusCheckRollup` is `null` for the
#: same commit, which is the value that parses to exactly the same `None` as a rate-limited
#: call in `ci_status`.
ZERO_CHECK_RUNS = {"total_count": 0, "check_runs": []}

#: The combined commit status for a commit nothing has POSTed to.
ZERO_STATUSES = {"state": "pending", "statuses": [], "total_count": 0, "sha": "9f3a1c2"}


def _run(**kw):
    """One check run, with the two fields `checks_at` reads."""
    return {"status": kw.get("status", "completed"), "conclusion": kw.get("conclusion")}


def api_path(cmd) -> str:
    """The API path out of a forge argv. `gh api --hostname H PATH` and
    `glab --hostname H api PATH` put it in different positions, so it is found rather than
    indexed: the first bare word after ``api`` that is not a flag or a flag's value."""
    cmd = list(cmd)
    if "api" not in cmd:
        return ""
    rest = cmd[cmd.index("api") + 1:]
    i = 0
    while i < len(rest):
        if rest[i].startswith("-"):
            i += 2                     # every flag charter passes here takes a value
            continue
        return rest[i]
    return ""


class _FakeGH:
    """A `gh api` that answers from a path → payload table.

    Keyed on a SUBSTRING of the API path, so a test says which endpoint it is answering
    without restating charter's query string — the thing a test that computes its
    expectation from the code under test would do.
    """

    def __init__(self, table: dict, rc: int = 0, stderr: str = ""):
        self.table, self.rc, self.stderr = table, rc, stderr
        self.calls: list[list[str]] = []

    def __call__(self, cmd, **kw):
        self.calls.append(list(cmd))
        path = api_path(cmd)
        if self.rc:
            return _proc(rc=self.rc, stderr=self.stderr)
        for key, payload in self.table.items():
            if key in path:
                return _proc(stdout=json.dumps(payload))
        return _proc(stdout="")


def _gh(table, rc=0, stderr=""):
    fake = _FakeGH(table, rc, stderr)
    return mock.patch("charter.forge.github.util.run", fake), fake


class TestZeroIsNotAPass(unittest.TestCase):
    """The headline. `total == 0` is `NOT RUN`; a failed call is `UNKNOWN`; they differ."""

    def test_zero_check_runs_is_not_a_pass_and_is_not_unknown(self):
        patch, _ = _gh({"check-runs": ZERO_CHECK_RUNS, "/status": ZERO_STATUSES})
        with patch:
            r = GitHubForge().checks_at("o/r", "9f3a1c2")
        self.assertEqual(r.total, 0)
        self.assertEqual(r.state, "not_run")

    def test_a_failed_call_is_unknown_and_is_not_not_run(self):
        patch, _ = _gh({}, rc=1, stderr="gh: rate limit exceeded")
        with patch:
            r = GitHubForge().checks_at("o/r", "9f3a1c2")
        self.assertIsNone(r.total)          # the ONLY way to say "could not ask"
        self.assertEqual(r.state, "unknown")

    def test_not_run_and_unknown_are_different_values(self):
        """Stated as its own case because the whole design turns on it: a single string
        that collapsed the two would pass both cases above and still be the bug."""
        self.assertNotEqual(base.CHECKS_NOT_RUN, base.CHECKS_UNKNOWN)
        self.assertNotIn(base.CHECKS_NOT_RUN, (base.CHECKS_PASSED,))
        self.assertNotIn(base.CHECKS_UNKNOWN, (base.CHECKS_PASSED,))

    def test_a_timeout_is_unknown(self):
        from charter import util

        def boom(cmd, **kw):
            raise util.ProcTimeout(list(cmd), 10.0)

        with mock.patch("charter.forge.github.util.run", boom):
            r = GitHubForge().checks_at("o/r", "9f3a1c2")
        self.assertEqual((r.total, r.state), (None, "unknown"))

    def test_malformed_json_is_unknown(self):
        with mock.patch("charter.forge.github.util.run",
                        lambda cmd, **kw: _proc(stdout="{not json")):
            r = GitHubForge().checks_at("o/r", "9f3a1c2")
        self.assertEqual((r.total, r.state), (None, "unknown"))


class TestTheBlindSpotInTheEndpoint(unittest.TestCase):
    """`commits/<sha>/check-runs` returns Check Runs ONLY. A repo whose CI reports through
    the Commit Statuses API — Jenkins, Buildkite, CircleCI's status integration — has
    `total_count: 0` there at a fully green head, and reading that alone gives a permanent
    false `NOT RUN` against a gate that deliberately offers no `--force`."""

    def test_zero_check_runs_and_one_passing_commit_status_is_passed(self):
        patch, _ = _gh({
            "check-runs": ZERO_CHECK_RUNS,
            "/status": {"state": "success", "total_count": 1,
                        "statuses": [{"state": "success", "context": "jenkins/pr"}]},
        })
        with patch:
            r = GitHubForge().checks_at("o/r", "9f3a1c2")
        self.assertEqual(r.total, 1)
        self.assertEqual(r.state, "passed")

    def test_a_failing_commit_status_fails_a_head_whose_check_runs_all_passed(self):
        patch, _ = _gh({
            "check-runs": {"total_count": 1, "check_runs": [_run(conclusion="success")]},
            "/status": {"state": "failure", "total_count": 1,
                        "statuses": [{"state": "failure", "context": "buildkite"}]},
        })
        with patch:
            r = GitHubForge().checks_at("o/r", "9f3a1c2")
        self.assertEqual((r.total, r.state), (2, "failed"))

    def test_both_endpoints_are_actually_called(self):
        patch, fake = _gh({"check-runs": ZERO_CHECK_RUNS, "/status": ZERO_STATUSES})
        with patch:
            GitHubForge().checks_at("o/r", "9f3a1c2")
        paths = [api_path(c) for c in fake.calls]
        self.assertTrue(any("check-runs" in p for p in paths), paths)
        self.assertTrue(any(p.endswith("status?per_page=100") for p in paths), paths)

    def test_a_failed_check_runs_read_is_unknown_even_when_the_statuses_passed(self):
        """The half the mirror case below cannot reach. With only the other direction
        pinned, folding a failed check-runs read into an empty list stayed green — the
        statuses read failed in the same fixture and produced the UNKNOWN by itself, so the
        mutation was masked by its neighbour rather than caught."""
        class _Half(_FakeGH):
            def __call__(self, cmd, **kw):
                if "check-runs" in api_path(cmd):
                    return _proc(rc=1, stderr="gh: API rate limit exceeded")
                return _proc(stdout=json.dumps(
                    {"state": "success", "total_count": 1,
                     "statuses": [{"state": "success", "context": "jenkins/pr"}]}))

        with mock.patch("charter.forge.github.util.run", _Half({})):
            r = GitHubForge().checks_at("o/r", "9f3a1c2")
        self.assertEqual((r.total, r.state), (None, "unknown"))

    def test_a_failed_status_read_is_unknown_even_when_the_check_runs_passed(self):
        """Incomplete enumeration answers UNKNOWN, never PASSED. Half a look is not a look."""
        class _Half(_FakeGH):
            def __call__(self, cmd, **kw):
                path = api_path(cmd)
                if "/status" in path:
                    return _proc(rc=1, stderr="gh: Bad credentials")
                return _proc(stdout=json.dumps(
                    {"total_count": 1, "check_runs": [_run(conclusion="success")]}))

        with mock.patch("charter.forge.github.util.run", _Half({})):
            r = GitHubForge().checks_at("o/r", "9f3a1c2")
        self.assertEqual((r.total, r.state), (None, "unknown"))


class TestAMalformedReplyIsUnknownNotGreen(unittest.TestCase):
    """The sweep found these reachable and unpinned. Every one is a reply charter did not
    write, on the path whose whole job is to refuse a head it cannot vouch for — so the
    question each answers is the same: does a surprise degrade to `unknown`, or to `passed`?"""

    def test_a_null_entry_in_the_statuses_list_does_not_crash_and_is_not_green(self):
        patch, _ = _gh({"check-runs": ZERO_CHECK_RUNS,
                        "/status": {"state": "success", "total_count": 2,
                                    "statuses": [None, {"state": "success"}]}})
        with patch:
            r = GitHubForge().checks_at("o/r", "9f3a1c2")
        self.assertEqual(r.total, 2)
        self.assertEqual(r.state, "unknown")     # the null contributes UNKNOWN, and it wins

    def test_a_status_with_no_state_key_is_unknown(self):
        patch, _ = _gh({"check-runs": ZERO_CHECK_RUNS,
                        "/status": {"state": "success", "total_count": 1,
                                    "statuses": [{"context": "jenkins"}]}})
        with patch:
            self.assertEqual(GitHubForge().checks_at("o/r", "9f3a1c2").state, "unknown")

    def test_a_check_run_that_is_not_an_object_is_unknown(self):
        patch, _ = _gh({"check-runs": {"total_count": 1, "check_runs": ["surprise"]},
                        "/status": ZERO_STATUSES})
        with patch:
            r = GitHubForge().checks_at("o/r", "9f3a1c2")
        self.assertEqual((r.total, r.state), (1, "unknown"))

    def test_a_gitlab_request_with_a_null_state_is_not_read_as_open(self):
        """`(mr.get("state") or "").lower()` — a merge request whose state is absent or null
        must not fall through to the `else` that means OPEN, because an open request is the
        one thing `land` will act on."""
        fake = _FakeGH({"merge_requests": [{"iid": 14, "sha": "s", "state": None}]})
        with mock.patch("charter.forge.gitlab.util.run", fake):
            r = GitLabForge().request_for("g/p", "change/x")
        self.assertEqual(r.state, "open")
        self.assertIsNone(r.merge)

    def test_a_gitlab_pipeline_with_a_null_status_is_unknown(self):
        fake = _FakeGH({"merge_requests/7": {"iid": 7, "sha": "9f3a1c2",
                                             "head_pipeline": {"status": None}}})
        with mock.patch("charter.forge.gitlab.util.run", fake):
            self.assertEqual(GitLabForge().checks_at("g/p", "9f3a1c2", 7).state, "unknown")


class TestEveryConclusion(unittest.TestCase):
    """Step 7: map every conclusion the forge can return. An unmapped one degrades to
    `unknown`, never to `passed`."""

    def _state(self, run):
        patch, _ = _gh({"check-runs": {"total_count": 1, "check_runs": [run]},
                        "/status": ZERO_STATUSES})
        with patch:
            return GitHubForge().checks_at("o/r", "9f3a1c2")

    def test_success_passes(self):
        self.assertEqual(self._state(_run(conclusion="success")).state, "passed")

    def test_neutral_and_skipped_pass(self):
        """A `paths:` filter or an `if:` condition produces these constantly; any other
        reading refuses the gate on most real repositories."""
        for c in ("neutral", "skipped"):
            with self.subTest(conclusion=c):
                self.assertEqual(self._state(_run(conclusion=c)).state, "passed")

    def test_the_four_bad_conclusions_fail(self):
        for c in ("failure", "cancelled", "timed_out", "startup_failure"):
            with self.subTest(conclusion=c):
                self.assertEqual(self._state(_run(conclusion=c)).state, "failed")

    def test_action_required_is_failed_because_a_check_waiting_on_a_person_did_not_pass(self):
        self.assertEqual(self._state(_run(conclusion="action_required")).state, "failed")

    def test_an_unconcluded_run_is_running(self):
        for s in ("queued", "in_progress", "waiting", "pending", "requested"):
            with self.subTest(status=s):
                r = self._state(_run(status=s, conclusion=None))
                self.assertEqual((r.total, r.state), (1, "running"))

    def test_an_unmapped_conclusion_is_unknown_never_passed(self):
        r = self._state(_run(conclusion="something_github_ships_in_2027"))
        self.assertEqual(r.state, "unknown")
        self.assertNotEqual(r.state, "passed")

    def test_stale_does_not_count_toward_total_at_all(self):
        """A run the forge itself disowned. If it is the only one at the head, `NOT RUN`
        is the honest answer — not `passed`, and not `total == 1`."""
        r = self._state(_run(conclusion="stale"))
        self.assertEqual((r.total, r.state), (0, "not_run"))

    def test_a_stale_run_beside_a_passing_one_leaves_total_at_one(self):
        patch, _ = _gh({"check-runs": {"total_count": 2, "check_runs": [
            _run(conclusion="stale"), _run(conclusion="success")]},
            "/status": ZERO_STATUSES})
        with patch:
            r = GitHubForge().checks_at("o/r", "9f3a1c2")
        self.assertEqual((r.total, r.state), (1, "passed"))


class TestPrecedence(unittest.TestCase):
    """Fixed and stated, so two people reading the table agree:
    UNKNOWN > FAILED > RUNNING > NOT RUN > PASSED."""

    def test_unknown_outranks_failed(self):
        self.assertEqual(base.worst(["unknown", "failed", "passed"]), "unknown")

    def test_failed_outranks_running(self):
        self.assertEqual(base.worst(["failed", "running", "passed"]), "failed")

    def test_running_outranks_passed(self):
        self.assertEqual(base.worst(["running", "passed"]), "running")

    def test_one_failure_among_six_passes_fails(self):
        self.assertEqual(base.worst(["passed"] * 6 + ["failed"]), "failed")

    def test_an_empty_enumeration_is_not_run(self):
        self.assertEqual(base.worst([]), "not_run")

    def test_a_state_this_module_does_not_know_is_unknown(self):
        self.assertEqual(base.worst(["passed", "sort-of-green"]), "unknown")

    def test_the_order_is_exactly_this(self):
        """Pinned as a literal. A test that read the order off `CHECKS_PRECEDENCE` and then
        asserted `worst` agreed with it would survive reordering the constant."""
        self.assertEqual(base.CHECKS_PRECEDENCE,
                         ("unknown", "failed", "running", "not_run", "passed"))


class TestGitLabChecks(unittest.TestCase):
    """GitLab's blind spot is the mirror image: with merged results on, the pipeline runs
    against `refs/merge-requests/:iid/merge`, whose sha is not the branch head — so a
    sha-filtered query is EMPTY on a green merge request."""

    def _glab(self, table, rc=0):
        fake = _FakeGH(table, rc)

        def run(cmd, **kw):
            # `_glab` builds [cli, --hostname, host, api, path, …]; the substring table
            # keys off the path exactly as the GitHub fake does.
            return fake(cmd, **kw)

        return mock.patch("charter.forge.gitlab.util.run", run), fake

    def test_no_request_number_and_an_empty_sha_filter_is_unknown_not_not_run(self):
        patch, _ = self._glab({"pipelines": []})
        with patch:
            r = GitLabForge().checks_at("g/p", "9f3a1c2")
        self.assertEqual((r.total, r.state), (None, "unknown"))

    def test_the_merge_requests_head_pipeline_is_read_even_though_its_sha_differs(self):
        """The merged-results case. The pipeline ran against the merge ref, so its own sha
        is not the head — and it is still the check GitLab shows a human."""
        patch, _ = self._glab({
            "merge_requests/7": {"iid": 7, "sha": "9f3a1c2",
                                 "head_pipeline": {"status": "success",
                                                   "sha": "deadbee"}},
        })
        with patch:
            r = GitLabForge().checks_at("g/p", "9f3a1c2", number=7)
        self.assertEqual((r.total, r.state), (1, "passed"))

    def test_a_merge_request_with_no_head_pipeline_is_not_run(self):
        patch, _ = self._glab({"merge_requests/7": {"iid": 7, "sha": "9f3a1c2",
                                                    "head_pipeline": None}})
        with patch:
            r = GitLabForge().checks_at("g/p", "9f3a1c2", number=7)
        self.assertEqual((r.total, r.state), (0, "not_run"))

    def test_a_merge_request_that_moved_off_the_sha_is_unknown(self):
        """A pushed fixup. Its head pipeline is a check on some other head, and saying
        anything about THIS one from it would be the staleness bug the sha keying exists
        to prevent."""
        patch, _ = self._glab({"merge_requests/7": {
            "iid": 7, "sha": "c0ffee0", "head_pipeline": {"status": "success"}}})
        with patch:
            r = GitLabForge().checks_at("g/p", "9f3a1c2", number=7)
        self.assertEqual((r.total, r.state), (None, "unknown"))

    def test_a_merge_request_reply_with_no_sha_is_unknown_and_does_not_crash(self):
        """`mr.get("sha") or ""` — the deletion sweep found this reachable and unpinned.
        A reply missing the key must degrade to UNKNOWN like every other thing charter
        could not establish, not raise out of a method documented never to raise."""
        patch, _ = self._glab({"merge_requests/7": {"iid": 7,
                                                    "head_pipeline": {"status": "success"}}})
        with patch:
            r = GitLabForge().checks_at("g/p", "9f3a1c2", number=7)
        self.assertEqual((r.total, r.state), (None, "unknown"))

    def test_a_merge_request_reply_with_a_null_sha_is_unknown(self):
        patch, _ = self._glab({"merge_requests/7": {"iid": 7, "sha": None,
                                                    "head_pipeline": {"status": "success"}}})
        with patch:
            self.assertEqual(GitLabForge().checks_at("g/p", "9f3a1c2", 7).state, "unknown")

    def test_a_blocking_manual_pipeline_is_not_a_pass(self):
        patch, _ = self._glab({"merge_requests/7": {
            "iid": 7, "sha": "9f3a1c2", "head_pipeline": {"status": "manual"}}})
        with patch:
            self.assertEqual(GitLabForge().checks_at("g/p", "9f3a1c2", 7).state, "failed")

    def test_an_unmapped_pipeline_status_is_unknown_never_passed(self):
        patch, _ = self._glab({"merge_requests/7": {
            "iid": 7, "sha": "9f3a1c2", "head_pipeline": {"status": "quantum"}}})
        with patch:
            self.assertEqual(GitLabForge().checks_at("g/p", "9f3a1c2", 7).state, "unknown")

    def test_a_failed_merge_request_read_is_unknown(self):
        patch, _ = self._glab({}, rc=1)
        with patch:
            r = GitLabForge().checks_at("g/p", "9f3a1c2", number=7)
        self.assertEqual((r.total, r.state), (None, "unknown"))


class TestRequestForGitHub(unittest.TestCase):
    """`open_change` answers `int | None` for OPEN requests only, so today a
    closed-unmerged member and a member with no request at all are the same value — and
    `PARTIALLY LANDED` and `REJECTED` are both underivable."""

    def _req(self, payload):
        patch, fake = _gh({"pulls": payload})
        with patch:
            return GitHubForge().request_for("o/r", "change/x"), fake

    def test_an_open_request_carries_its_head_sha_and_no_merge_commit(self):
        r, _ = self._req([{"number": 601, "state": "open", "merged_at": None,
                           "merge_commit_sha": "a-test-merge-on-no-branch",
                           "head": {"sha": "4b1e77a"}}])
        self.assertEqual((r.number, r.state, r.head), (601, "open", "4b1e77a"))
        self.assertIsNone(r.merge)

    def test_a_merged_request_carries_the_merge_commit(self):
        r, _ = self._req([{"number": 601, "state": "closed",
                           "merged_at": "2026-08-30T10:00:00Z",
                           "merge_commit_sha": "e0c9d13", "head": {"sha": "4b1e77a"}}])
        self.assertEqual((r.state, r.merge), ("merged", "e0c9d13"))

    def test_a_closed_unmerged_request_is_closed_and_is_not_no_request(self):
        """This is REJECTED. `open_change` returns None here, identically to a branch
        nobody ever pushed."""
        r, _ = self._req([{"number": 7, "state": "closed", "merged_at": None,
                           "merge_commit_sha": None, "head": {"sha": "c02de55"}}])
        self.assertIsNotNone(r)
        self.assertEqual(r.state, "closed")

    def test_no_request_at_all_is_none(self):
        r, _ = self._req([])
        self.assertIsNone(r)

    def test_the_query_asks_for_every_state_not_just_open(self):
        _, fake = self._req([])
        path = [api_path(c) for c in fake.calls][0]
        self.assertIn("state=all", path)

    def test_a_failed_lookup_raises_rather_than_answering_no_request(self):
        """The return type has no value meaning "I could not ask", so it must raise.
        Answering None for a rate-limited lookup is how an open member reads as one that
        was never pushed — and then `push` opens a second request."""
        patch, _ = _gh({}, rc=1, stderr="gh: Bad credentials")
        with patch, self.assertRaises(base.ForgeError):
            GitHubForge().request_for("o/r", "change/x")


class TestRequestForGitLab(unittest.TestCase):
    def _req(self, payload, rc=0):
        fake = _FakeGH({"merge_requests": payload}, rc)
        with mock.patch("charter.forge.gitlab.util.run", fake):
            return GitLabForge().request_for("g/p", "change/x"), fake

    def test_the_iid_is_the_number(self):
        r, _ = self._req([{"iid": 14, "state": "opened", "sha": "771ab90"}])
        self.assertEqual((r.number, r.state, r.head), (14, "open", "771ab90"))

    def test_merged_carries_the_merge_commit(self):
        r, _ = self._req([{"iid": 14, "state": "merged", "sha": "771ab90",
                           "merge_commit_sha": "e0c9d13"}])
        self.assertEqual((r.state, r.merge), ("merged", "e0c9d13"))

    def test_a_squashed_merge_falls_back_to_the_squash_commit(self):
        r, _ = self._req([{"iid": 14, "state": "merged", "sha": "771ab90",
                           "merge_commit_sha": None, "squash_commit_sha": "5qua5h0"}])
        self.assertEqual(r.merge, "5qua5h0")

    def test_locked_reads_as_closed_not_open(self):
        r, _ = self._req([{"iid": 14, "state": "locked", "sha": "771ab90"}])
        self.assertEqual(r.state, "closed")

    def test_a_failed_lookup_raises(self):
        with self.assertRaises(base.ForgeError):
            self._req([], rc=1)


class TestTheProtocolCarriesTwoFieldsNotOneString(unittest.TestCase):
    def test_checks_is_a_frozen_record(self):
        c = base.Checks(total=0, state="not_run")
        with self.assertRaises(AttributeError):
            c.total = 1

    def test_both_backends_satisfy_the_widened_protocol(self):
        for f in (GitHubForge(), GitLabForge()):
            with self.subTest(kind=f.kind):
                self.assertIsInstance(f, base.Forge)

    def test_ci_status_was_left_exactly_as_it_is(self):
        """The permissive discipline is correct for the status line, and the change
        surface simply does not use it. Improving it here is a separate change with its
        own blast radius, so this pins that it was NOT touched."""
        self.assertEqual(base.CI_STATES,
                         frozenset({"success", "failed", "running", "pending",
                                    "manual", "canceled", "skipped"}))
        self.assertNotIn("not_run", base.CI_STATES)
        self.assertNotIn("unknown", base.CI_STATES)


if __name__ == "__main__":
    unittest.main()
