"""`charter change land` — one member, three gates, and the flag that does not exist.

Three properties carry this file.

**Which gate fired, never merely that one did.** Three gates run in sequence and they mask
each other; an exit-code assertion cannot tell them apart and will stay green over a real
deletion. On #558, deleting the `-z "$claimed"` refusal still exited 1, for a worse reason.
So every gate here asserts its own words **and** the absence of its neighbours'.

**`NOT RUN` is not `PASSED`, and the gate refuses it.** This is #561 at the only place it
can actually merge something. `gh pr checks` would say "no checks reported" here and
`mergeStateStatus` would say `CLEAN`, and the merge button would be offered — but that
belongs in the spec rather than in a refusal charter prints, because it is an assertion
about tools charter did not run.

**There is no `--all`.** Not a flag that defaults off: a flag that does not parse (ADR 0020).

The release floor that keeps an unattended run from landing at all is **not tested here**:
it lives in `hooks._PUBLISH_FORGE` and is pinned by `tests/test_release_floor.py`, which
drives the real `charter hook pretooluse` rather than the function in isolation.
"""
from __future__ import annotations

import ast
import json
import unittest
from pathlib import Path
from unittest import mock

from charter import change, cli, commands_change, trace, workspace
from charter.forge import base
from tests.test_change_push import ChangeOnAForge, FakeForge, git

OPEN = base.REQUEST_OPEN
MERGED = base.REQUEST_MERGED
CLOSED = base.REQUEST_CLOSED
PASSED = base.Checks(7, base.CHECKS_PASSED)


class Landing(ChangeOnAForge):
    """One change, one or two members, all gates green unless a case says otherwise."""

    def setup_change(self, members=(("api", ()),), heads=None):
        slug = self.make(members=members)
        heads = heads or {}
        for name, _needs in members:
            branch = f"change/{slug}"
            self.branch_in(name, branch)
        # One shared branch name across members, so one request per member.
        self.forge.requests[f"change/{slug}"] = base.Request(
            601, OPEN, heads.get("api", "4b1e77a"))
        self.forge.checks = PASSED
        return slug

    def two_members(self, slug="component-api-2"):
        """`api`, and `web` which needs it — each on its OWN branch, so each has its own
        request and the blocker gate is asked a real question."""
        for name in ("api", "web"):
            self.clone(name)
        self.assertEqual(self.call(commands_change.cmd_change_create, change=slug,
                                   why="API 1 -> 2")[0], 0)
        self.assertEqual(self.call(commands_change.cmd_change_add, change=slug,
                                   repo="api", branch="change/api")[0], 0)
        self.assertEqual(self.call(commands_change.cmd_change_add, change=slug, repo="web",
                                   branch="change/web", needs=["api"])[0], 0)
        self.branch_in("api", "change/api")
        self.branch_in("web", "change/web")
        self.forge.checks = PASSED
        return slug

    def land(self, slug, repo="api", **kw):
        return self.call(commands_change.cmd_change_land, change=slug, repo=repo, **kw)


class TestTheFlagThatDoesNotExist(unittest.TestCase):
    def test_there_is_no_all_flag(self):
        p = cli.build_parser()
        with self.assertRaises(SystemExit):
            p.parse_args(["change", "land", "component-api-2", "--repo", "api", "--all"])

    def test_no_spelling_of_it_parses(self):
        p = cli.build_parser()
        for flag in ("--all", "--all-repos", "--every", "--each", "-a"):
            with self.subTest(flag=flag):
                with self.assertRaises(SystemExit):
                    p.parse_args(["change", "land", "c", "--repo", "api", flag])

    def test_repo_is_required_so_a_bare_land_cannot_mean_everything(self):
        """The other half of the same decision: if `--repo` were optional, "land the change"
        would have to mean something, and the only available meanings are the three `--all`
        would have had to choose between."""
        p = cli.build_parser()
        with self.assertRaises(SystemExit):
            p.parse_args(["change", "land", "component-api-2"])

    def test_the_word_all_appears_in_no_land_option_string(self):
        p = cli.build_parser()
        land = (p._subparsers._group_actions[0].choices["change"]
                ._subparsers._group_actions[0].choices["land"])
        for action in land._actions:
            for opt in action.option_strings:
                self.assertNotIn("all", opt.lower(), opt)


class TestGateAAReRequestThatIsOpen(Landing):
    def test_no_request_at_all_is_refused_and_names_push(self):
        slug = self.setup_change()
        self.forge.requests.clear()
        code, out, err = self.land(slug)
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("no request", err)
        self.assertIn("charter change push", err)
        self.assertNotIn("blocker", err)
        self.assertNotIn("NOT RUN", err)

    def test_a_closed_unmerged_request_is_rejected_and_names_drop(self):
        slug = self.setup_change()
        self.forge.requests[f"change/{slug}"] = base.Request(7, CLOSED, "c02de55")
        code, out, err = self.land(slug)
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("REJECTED", err)
        self.assertIn("charter change drop", err)
        self.assertNotIn("blocker", err)

    def test_an_already_merged_request_is_refused_without_merging_again(self):
        slug = self.setup_change()
        self.forge.requests[f"change/{slug}"] = base.Request(601, MERGED, "s", "e0c9d13")
        code, out, err = self.land(slug)
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("already merged", err)
        self.assertEqual([c for c in self.forge.calls if c[0] == "merge_change"], [])

    def test_a_repo_that_is_not_a_member_is_refused_by_name(self):
        slug = self.setup_change()
        code, out, err = self.land(slug, repo="charter-slack")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("is not a member", err)

    def test_an_excluded_repo_is_told_it_was_excluded_and_why(self):
        slug = self.setup_change()
        self.call(commands_change.cmd_change_drop, change=slug, repo="web",
                  why="only an action provider")
        code, out, err = self.land(slug, repo="web")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("was excluded", err)
        self.assertIn("only an action provider", err)


class TestGateBTheBlockers(Landing):
    def test_a_blocker_that_has_not_landed_refuses_and_names_it(self):
        slug = self.setup_change(members=(("api", ()), ("web", ("api",))))
        code, out, err = self.land(slug, repo="web")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("blocker", err)
        self.assertIn("api", err)
        self.assertIn("has not landed", err)
        self.assertNotIn("NOT RUN", err)          # not the gate below it
        self.assertNotIn("no request", err)       # nor the one above

    def test_a_blocker_the_forge_calls_merged_lets_the_dependent_through(self):
        """The gate PASSING, with each member on its own branch and its own request, so
        this is not the previous case wearing a different assertion."""
        slug = self.two_members()
        self.forge.requests["change/api"] = base.Request(601, MERGED, "a1", "e0c9d13")
        self.forge.requests["change/web"] = base.Request(14, OPEN, "w1")
        code, out, err = self.land(slug, repo="web")
        self.assertEqual(code, 0, err)
        self.assertNotIn("has not landed", err)
        self.assertEqual(
            [c for c in self.forge.calls if c[0] == "merge_change"][0][2], 14)

    def test_the_blocker_gate_reads_the_blockers_own_branch(self):
        slug = self.two_members()
        self.forge.requests["change/api"] = base.Request(601, MERGED, "a1", "e0c9d13")
        self.forge.requests["change/web"] = base.Request(14, OPEN, "w1")
        self.land(slug, repo="web")
        branches = [c[2] for c in self.forge.calls if c[0] == "request_for"]
        self.assertIn("change/api", branches)

    def test_a_blocker_naming_a_non_member_is_refused_and_nothing_merges(self):
        """`write` refuses this ordering, so it is placed by hand — a record can arrive from
        an older charter or a hand edit, and the reader must not assume the writer ran. Two
        guards can catch it, so this asserts the property (nothing merged, `ghost` named)
        rather than which of them got there first."""
        slug = self.setup_change(members=(("api", ()),))
        rec = change.read("ws", slug)
        rec["members"][0]["needs"] = ["ghost"]
        change.path_for("ws", slug).write_text(json.dumps(rec, indent=2) + "\n")
        code, out, err = self.land(slug)
        self.assertNotEqual(code, 0)
        self.assertIn("ghost", err)
        self.assertEqual([c for c in self.forge.calls if c[0] == "merge_change"], [])


class TestGateCTheChecksAtThisHeadSha(Landing):
    def _refuse_with(self, checks):
        slug = self.setup_change()
        self.forge.checks = checks
        return self.land(slug)

    def test_a_head_with_zero_check_runs_is_refused_by_the_check_gate(self):
        code, out, err = self._refuse_with(base.Checks(0, base.CHECKS_NOT_RUN))
        self.assertEqual(code, 2)
        self.assertIn("NOT RUN", err)             # WHICH gate, not just that one fired
        self.assertNotIn("blocker", err)          # and not the gate above it
        self.assertNotIn("no request", err)

    def test_unknown_is_refused_and_says_something_different_from_not_run(self):
        code, out, err = self._refuse_with(base.Checks(None, base.CHECKS_UNKNOWN))
        self.assertEqual(code, 2)
        self.assertIn("UNKNOWN", err)
        self.assertNotIn("NOT RUN", err)

    def test_failed_is_refused(self):
        code, out, err = self._refuse_with(base.Checks(3, base.CHECKS_FAILED))
        self.assertEqual(code, 2)
        self.assertIn("FAILED", err)
        self.assertNotIn("NOT RUN", err)

    def test_running_is_refused_and_is_not_a_verdict(self):
        code, out, err = self._refuse_with(base.Checks(2, base.CHECKS_RUNNING))
        self.assertEqual(code, 2)
        self.assertIn("RUNNING", err)

    def test_the_refusal_names_the_head_sha_it_read(self):
        code, out, err = self._refuse_with(base.Checks(0, base.CHECKS_NOT_RUN))
        self.assertIn("4b1e77a", err)

    def test_nothing_is_merged_when_the_check_gate_refuses(self):
        slug = self.setup_change()
        self.forge.checks = base.Checks(0, base.CHECKS_NOT_RUN)
        self.land(slug)
        self.assertEqual([c for c in self.forge.calls if c[0] == "merge_change"], [])

    def test_the_checks_are_read_at_the_requests_head_and_not_at_a_branch(self):
        slug = self.setup_change()
        self.forge.requests[f"change/{slug}"] = base.Request(601, OPEN, "9f3a1c2")
        self.forge.checks = base.Checks(0, base.CHECKS_NOT_RUN)
        self.land(slug)
        read = [c for c in self.forge.calls if c[0] == "checks_at"]
        self.assertEqual(read[0][2], "9f3a1c2")
        self.assertEqual(read[0][3], 601)         # the number, so GitLab can reach the MR


class TestForbiddenInputs(unittest.TestCase):
    """Task 5 Step 6 asks for this assertion **here** rather than in the forge task: at
    Task 5 there is no landing path, so the test would pass against a module that could not
    fail it — §4i's convincing empty."""

    MODULES = ("charter.commands_change", "charter.forge.github", "charter.forge.gitlab")

    def _land_source(self) -> str:
        return Path(commands_change.__file__).read_text(encoding="utf-8")

    def _strings(self, module_name: str) -> list[str]:
        """Every string LITERAL in a module.

        Literals rather than raw text, deliberately. Both forbidden inputs are named in
        comments in these very files — which is the point of naming them — and a substring
        search over a file that documents a property finds the documentation. What matters
        is whether either can reach a forge, and a value that reaches a forge is a string
        the module builds.
        """
        src = Path(__import__(module_name, fromlist=["x"]).__file__).read_text("utf-8")
        return [n.value for n in ast.walk(ast.parse(src))
                if isinstance(n, ast.Constant) and isinstance(n.value, str)]

    def test_the_landing_path_never_invokes_gh_pr_checks(self):
        for module in self.MODULES:
            for literal in self._strings(module):
                with self.subTest(module=module):
                    self.assertNotIn("pr checks", literal)
                    self.assertNotIn("pr status", literal)
                    self.assertNotIn("mr checks", literal)

    def test_the_landing_path_never_reads_merge_state_status(self):
        """Both report a run that never happened identically to a pass. Named as forbidden
        inputs in the spec AND in the test, because the failure they cause is a green light
        rather than an error."""
        for module in self.MODULES:
            for literal in self._strings(module):
                with self.subTest(module=module):
                    self.assertNotIn("mergeStateStatus", literal)
                    self.assertNotIn("mergeable_state", literal)

    def test_the_documentation_of_the_ban_is_still_there(self):
        """The other half, so the test above cannot be satisfied by deleting the reasoning:
        both names must still appear in the source as prose, because a reader who does not
        know why the obvious field is unused will add it back."""
        src = self._land_source() + Path(
            __import__("charter.forge.base", fromlist=["x"]).__file__).read_text("utf-8")
        self.assertIn("mergeStateStatus", src)
        self.assertIn("gh pr checks", src)

    def test_the_gate_reads_checks_at_and_not_ci_status(self):
        """`ci_status` collapses six worlds into None — a CLI failure, a timeout, a non-zero
        exit, malformed JSON, an auth failure, and 'no check ever ran'."""
        tree = ast.parse(self._land_source())
        called = {n.func.attr for n in ast.walk(tree)
                  if isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute)}
        self.assertIn("checks_at", called)
        self.assertNotIn("ci_status", called)
        self.assertNotIn("open_change", called)


class TestTheMergeMethod(Landing):
    def test_the_default_is_a_merge_commit(self):
        slug = self.setup_change()
        code, out, err = self.land(slug)
        self.assertEqual(code, 0, err)
        merged = [c for c in self.forge.calls if c[0] == "merge_change"][0]
        self.assertEqual(merged[3], "merge")

    def test_squash_is_permitted(self):
        slug = self.setup_change()
        code, out, err = self.land(slug, squash=True)
        self.assertEqual(code, 0, err)
        self.assertEqual([c for c in self.forge.calls if c[0] == "merge_change"][0][3],
                         "squash")

    def test_rebase_is_refused_and_says_why(self):
        slug = self.setup_change()
        code, out, err = self.land(slug, rebase=True)
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("rebase", err)
        self.assertIn("Charter-Change", err)     # the trailer it could not put anywhere
        self.assertIn("revert", err)
        self.assertEqual([c for c in self.forge.calls if c[0] == "merge_change"], [])

    def test_rebase_is_refused_before_any_gate_runs(self):
        """It is a refusal of the REQUEST. Running the gates first would report a check
        failure for a landing charter was never going to perform."""
        slug = self.setup_change()
        self.forge.calls.clear()
        self.land(slug, rebase=True)
        self.assertEqual(self.forge.calls, [])

    def test_rebase_is_not_in_the_protocols_permitted_set_either(self):
        self.assertNotIn("rebase", base.MERGE_METHODS)


class TestTheTrailer(Landing):
    def test_the_landing_commit_carries_the_charter_change_trailer(self):
        slug = self.setup_change()
        code, out, err = self.land(slug)
        self.assertEqual(code, 0, err)
        message = [c for c in self.forge.calls if c[0] == "merge_change"][0][5]
        self.assertIn(f"Charter-Change: {slug}", message)

    def test_the_trailer_is_on_a_squash_too(self):
        slug = self.setup_change()
        code, out, err = self.land(slug, squash=True)
        self.assertEqual(code, 0, err)
        message = [c for c in self.forge.calls if c[0] == "merge_change"][0][5]
        self.assertIn(f"Charter-Change: {slug}", message)

    def test_the_trailer_is_the_last_line_so_git_interpret_trailers_sees_it(self):
        slug = self.setup_change()
        self.land(slug)
        message = [c for c in self.forge.calls if c[0] == "merge_change"][0][5]
        self.assertTrue(message.splitlines()[-1].startswith("Charter-Change: "))

    def test_a_slug_cannot_forge_a_second_trailer(self):
        """The slug is validated at creation and CONTAINED again on read, because a record
        can arrive from an older charter or a hand edit."""
        slug = self.setup_change()
        rec = change.read("ws", slug)
        rec["change"] = "c\nCharter-Change: something-else"
        change.path_for("ws", slug).write_text(json.dumps(rec, indent=2) + "\n")
        code, out, err = self.land(slug)
        merges = [c for c in self.forge.calls if c[0] == "merge_change"]
        if merges:
            self.assertEqual(
                len([ln for ln in merges[0][5].splitlines()
                     if ln.startswith("Charter-Change: ")]), 1)
        else:
            self.assertNotEqual(code, 0)     # refused on read, which is also correct


class TestTheReadBackAndTheLog(Landing):
    def test_the_success_line_reports_what_the_forge_confirmed(self):
        """A success line reports what charter CONFIRMED, not what it asked for. The fake's
        own state moves on merge, so the sha printed is the one a fresh read answered."""
        slug = self.setup_change()
        self.forge.merge_sha = "CONFIRMEDSHA"
        code, out, err = self.land(slug)
        self.assertEqual(code, 0, err)
        self.assertIn("CONFIRMEDSHA", out)

    def test_a_read_back_that_does_not_confirm_records_nothing(self):
        slug = self.setup_change()
        self.forge.reflects_merges = False        # the merge "succeeded" and did not land
        code, out, err = self.land(slug)
        self.assertEqual(code, 1)
        self.assertIn("did not confirm", err)
        self.assertEqual(commands_change.landings("ws", slug), {})

    def test_the_log_line_is_written_after_a_confirmed_merge(self):
        slug = self.setup_change()
        code, out, err = self.land(slug)
        self.assertEqual(code, 0, err)
        log = commands_change.landings("ws", slug)
        self.assertEqual(log["api"]["merge"], "e0c9d13")
        self.assertEqual(log["api"]["head"], "4b1e77a")
        self.assertEqual(log["api"]["number"], 601)

    def test_a_link_at_the_log_directory_is_refused(self):
        """#336's first half. When the DIRECTORY is the link, every file inside it is an
        ordinary regular file with nothing for a per-file check to object to."""
        slug = self.setup_change()
        outside = Path(self.tmp) / "elsewhere"
        outside.mkdir(parents=True, exist_ok=True)
        (outside / "h.jsonl").write_text(
            json.dumps({"change": slug, "repo": "api", "merge": "dead"}) + "\n")
        d = change.log_dir("ws")
        d.parent.mkdir(parents=True, exist_ok=True)
        d.symlink_to(outside)
        self.assertEqual(commands_change.landings("ws", slug), {})

    def test_a_link_at_one_log_file_is_refused_and_the_others_are_still_read(self):
        """#336's second half, which the directory check structurally cannot see."""
        slug = self.setup_change()
        outside = Path(self.tmp) / "elsewhere2"
        outside.mkdir(parents=True, exist_ok=True)
        (outside / "x.jsonl").write_text(
            json.dumps({"change": slug, "repo": "api", "merge": "dead"}) + "\n")
        d = change.log_dir("ws")
        d.mkdir(parents=True, exist_ok=True)
        (d / "aaa.jsonl").symlink_to(outside / "x.jsonl")
        (d / "zzz.jsonl").write_text(
            json.dumps({"change": slug, "repo": "web", "merge": "beef"}) + "\n")
        got = commands_change.landings("ws", slug)
        self.assertNotIn("api", got)
        self.assertEqual(got["web"]["merge"], "beef")

    def test_a_malformed_log_line_is_skipped_rather_than_taking_the_read_down(self):
        slug = self.setup_change()
        d = change.log_dir("ws")
        d.mkdir(parents=True, exist_ok=True)
        (d / "h.jsonl").write_text(
            "{not json\n"
            + json.dumps({"change": slug, "repo": "api", "merge": "beef"}) + "\n")
        self.assertEqual(commands_change.landings("ws", slug)["api"]["merge"], "beef")

    def test_the_log_is_never_committed(self):
        """`changes/log/` joins neither `_live_block` nor `_ws_meta_paths` — it holds merge
        shas, per host, appended without a lock, exactly as `pieces/` does."""
        block = workspace._live_block({"ws"})
        self.assertIn("!/workspaces/ws/changes", block)
        self.assertIn("/workspaces/ws/changes/log/", block)
        self.assertNotIn("!/workspaces/ws/changes/log", block)


class TestLandedIsBothHalves(Landing):
    """*Landed* is defined once, because six sections consume the word: the forge reports
    the request **merged**, and git shows the member's default branch **containing the sha
    the log recorded**. Both halves, because the forge alone cannot see a revert and the log
    alone cannot see a browser merge."""

    def _member(self, slug, repo="api"):
        rec = change.read("ws", slug)
        return rec, change.member(rec, repo)

    def test_a_request_that_is_not_merged_is_not_landed(self):
        slug = self.setup_change()
        rec, m = self._member(slug)
        landed, why = commands_change.member_landed("ws", m, self.forge, "p", {})
        self.assertFalse(landed)
        self.assertIn("open", why)

    def test_no_request_at_all_is_not_landed(self):
        slug = self.setup_change()
        self.forge.requests.clear()
        rec, m = self._member(slug)
        landed, why = commands_change.member_landed("ws", m, self.forge, "p", {})
        self.assertFalse(landed)
        self.assertIn("no request", why)

    def test_merged_with_no_log_line_is_landed_AND_named_as_outside_charter(self):
        """A member merged in a browser. `open_change` cannot see it at all and the log has
        nothing; calling it unlanded would block every dependent forever."""
        slug = self.setup_change()
        self.forge.requests[f"change/{slug}"] = base.Request(601, MERGED, "s", "e0c9d13")
        rec, m = self._member(slug)
        landed, why = commands_change.member_landed("ws", m, self.forge, "p", {})
        self.assertTrue(landed)
        self.assertIn("outside charter", why)

    def test_a_logged_merge_git_no_longer_contains_is_not_landed_any_more(self):
        """The revert case, and nothing had to notice or update a flag for it to become
        true. The sha below is on no branch of the clone."""
        slug = self.setup_change()
        self.forge.requests[f"change/{slug}"] = base.Request(601, MERGED, "s", "e0c9d13")
        rec, m = self._member(slug)
        log = {"api": {"merge": "0" * 40, "repo": "api", "change": slug}}
        landed, why = commands_change.member_landed("ws", m, self.forge, "p", log)
        self.assertFalse(landed)

    def test_a_log_sha_that_is_not_a_sha_never_reaches_git(self):
        """The log is never committed, which makes it LOCAL rather than trustworthy: a hand
        edit, or a half-written line from a killed process, reaches here — and from here the
        value goes into a `git merge-base` argv, where `-X` is a flag.

        Two shapes, and the second is the one that needs `fullmatch` with anchors: Python's
        `$` matches at the end of the string *or just before a trailing newline*, so a
        `.match` would admit `"e0c9d13\n"` — a newline on its way into an argv."""
        slug = self.setup_change()
        self.forge.requests[f"change/{slug}"] = base.Request(601, MERGED, "s", "e0c9d13")
        rec, m = self._member(slug)
        calls = []
        real = commands_change._contains
        commands_change._contains = lambda *a, **k: calls.append(a) or True
        try:
            # `""` is deliberately absent: an empty `merge` is "no sha was recorded",
            # which the line above this check answers as landed-outside-charter.
            for bad in ("--upload-pack=evil", "e0c9d13\n", "not-a-sha", "E0C9D13", "abc"):
                with self.subTest(merge=bad):
                    landed, why = commands_change.member_landed(
                        "ws", m, self.forge, "p",
                        {"api": {"merge": bad, "repo": "api", "change": slug}})
                    self.assertFalse(landed)
                    self.assertIn("is not a sha", why)
        finally:
            commands_change._contains = real
        self.assertEqual(calls, [], "a value that is not a sha reached git anyway")

    def test_a_logged_merge_git_still_contains_is_landed(self):
        slug = self.setup_change()
        self.forge.requests[f"change/{slug}"] = base.Request(601, MERGED, "s", "e0c9d13")
        clone = workspace.workspace_dir("ws") / "api"
        on_main = git("rev-parse", "origin/main", cwd=clone).stdout.strip()
        rec, m = self._member(slug)
        log = {"api": {"merge": on_main, "repo": "api", "change": slug}}
        landed, why = commands_change.member_landed("ws", m, self.forge, "p", log)
        self.assertTrue(landed, why)
        self.assertEqual(why, "")


class TestTraceIsUnconditional(Landing):
    def _events(self):
        return [r for r in trace.read() if r.get("event") == "change-land"]

    def test_a_success_is_traced(self):
        slug = self.setup_change()
        self.land(slug)
        rows = self._events()
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["repo"], "api")
        self.assertEqual(rows[0]["checks"], "passed")
        self.assertNotIn("refused", rows[0])

    def test_the_trace_row_carries_exactly_these_field_names(self):
        """**The names, as literals.** The deletion sweep retuned `"workspace"` to a random
        string and nothing went red: every assertion here read a field it had just named, so
        the key set itself was unpinned.

        It is not decoration. §6.3 exists so that after the fact somebody can answer *which
        command landed what* — the security assessment called closing that hole on
        `commands_secrets` the highest-value observability change in the repo. A reader of
        `charter trace` filters on these names, so renaming one silently empties their query
        rather than failing anything.
        """
        slug = self.setup_change()
        self.land(slug)
        row = self._events()[0]
        self.assertEqual(
            {k for k in row if k not in ("ts", "event")},
            {"workspace", "change", "method", "repo", "number", "head", "checks", "merge"})
        self.assertEqual(row["event"], "change-land")

    def test_a_refused_row_carries_the_fields_reached_before_the_refusal(self):
        """A refusal traces what charter had established by the time it refused — no more,
        so the row cannot imply a read that never happened, and no less."""
        slug = self.setup_change()
        self.forge.checks = base.Checks(0, base.CHECKS_NOT_RUN)
        self.land(slug)
        row = self._events()[0]
        self.assertEqual(
            {k for k in row if k not in ("ts", "event")},
            {"workspace", "change", "method", "repo", "number", "head", "checks", "refused"})
        self.assertNotIn("merge", row)      # nothing merged, so no merge sha is claimed

    def test_a_refusal_is_traced_too_and_says_it_was_refused(self):
        slug = self.setup_change()
        self.forge.checks = base.Checks(0, base.CHECKS_NOT_RUN)
        self.land(slug)
        rows = self._events()
        self.assertEqual(len(rows), 1)
        self.assertIn("NOT RUN", rows[0]["refused"])
        self.assertEqual(rows[0]["checks"], "not_run")

    def test_the_rebase_refusal_is_traced(self):
        slug = self.setup_change()
        self.land(slug, rebase=True)
        self.assertEqual(len(self._events()), 1)

    def test_an_exception_on_the_way_through_is_still_traced(self):
        slug = self.setup_change()
        self.forge.request_for = mock.Mock(side_effect=RuntimeError("boom"))
        with self.assertRaises(RuntimeError):
            self.land(slug)
        rows = self._events()
        self.assertEqual(len(rows), 1)
        self.assertIn("unhandled", rows[0]["refused"])


if __name__ == "__main__":
    unittest.main()
