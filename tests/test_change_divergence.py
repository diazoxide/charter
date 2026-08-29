"""Divergence, named — ADR 0013 rule 2 over a cross-repo change.

*"A divergence charter can see, charter names… WARN is not a surface. A divergence worth
naming under rule 2 is worth FAIL."*

Charter refuses to land a member whose blockers have not landed, and it cannot stop a human
merging in a browser. **What would be theatre is a guard that reports enforcement it does
not have**, so the same read that refuses also names the landings that happened anyway.
That is the half these tests are about, and every one of them asserts FAIL rather than an
exit code — `cmd_doctor` exits non-zero only on FAIL, and that exit code is the only thing
that makes the SessionStart wrapper print.

Five divergences, each with its own sentence. Three gates in sequence mask each other and
an exit-code assertion cannot tell them apart (#558), so every case below asserts **which**
one fired and, where a neighbour could have produced the same row, the absence of its words.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import change, commands_change, doctor
from tests._changerepo import ChangeRepoCase, git, sha

SLUG = "component-api-2"


class DivergenceCase(ChangeRepoCase):
    def findings(self, slug: str = SLUG) -> list[str]:
        return commands_change.divergences(self.WS, change.read(self.WS, slug))

    def one_line(self, slug: str = SLUG) -> str:
        got = self.findings(slug)
        self.assertEqual(len(got), 1, got)
        return got[0]


class TestALandingCharterDidNotMakeIsNamed(DivergenceCase):
    def test_a_member_merged_outside_charter_is_reported(self):
        """The forge says merged, the log has no line — so no `Charter-Change` trailer and
        no merge sha, and `charter change revert` has nothing to run against. Git stands in
        for the forge here and is strictly better at it: a local clone can see a revert,
        which the forge cannot."""
        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        self.land_a_merge(repo, SLUG, trailer=False)
        line = self.one_line()
        self.assertIn("charter did not land it", line)
        self.assertIn("Charter-Change", line)
        self.assertIn("revert", line)

    def test_a_member_charter_did_land_is_not_reported(self):
        """The control. Without it every assertion above is satisfied by a check that
        reports everything."""
        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        self.declare(SLUG, "charter", self.land_a_merge(repo, SLUG))
        self.assertEqual(self.findings(), [])

    def test_an_unmerged_member_is_not_reported_either(self):
        """A branch that exists and is not in `main` is work in progress, which is the
        ordinary state of a change and not a divergence. A check that called it one would
        be permanently red on every plane doing the thing this surface is for."""
        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        git(repo, "switch", "-q", "-c", change.default_branch(SLUG))
        (repo / "g").write_text("2\n")
        git(repo, "add", "-A")
        git(repo, "-c", "commit.gpgsign=false", "commit", "-qm", "wip")
        git(repo, "switch", "-q", "main")
        self.assertEqual(self.findings(), [])


class TestADeclarationGitDisagreesWithIsNamed(DivergenceCase):
    def test_a_landing_the_default_branch_no_longer_contains(self):
        """*"A member with a log line git no longer contains is not landed any more, and
        nothing had to notice or update a flag for that to become true."* Reset `main` past
        the merge — which is what somebody force-pushing a revert leaves behind — and the
        declaration is still there and no longer true."""
        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        merge = self.land_a_merge(repo, SLUG)
        self.declare(SLUG, "charter", merge)
        git(repo, "reset", "-q", "--hard", "HEAD~1")
        line = self.one_line()
        self.assertIn("no longer contains", line)
        self.assertIn("not landed any more", line)
        self.assertNotIn("charter did not land it", line)   # not the gate above it

    def test_a_landing_whose_commit_carries_no_trailer(self):
        """The log names a commit charter did not author for this change. Step 1's literal
        wording, and it is a different finding from the one above: there the commit is
        gone, here it is present and is somebody else's."""
        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        self.declare(SLUG, "charter", self.land_a_merge(repo, SLUG, trailer=False))
        line = self.one_line()
        self.assertIn("carries no", line)
        self.assertIn("Charter-Change", line)
        self.assertNotIn("no longer contains", line)

    def test_a_log_line_whose_merge_is_not_a_sha_is_named_as_that(self):
        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        self.declare(SLUG, "charter", "not-a-sha")
        self.assertIn("not a sha", self.one_line())


class TestAnOutOfOrderLandingIsNamed(DivergenceCase):
    """§3.2's honest half. Charter refuses to land a blocked member; it cannot stop a
    browser, and what makes the guard honest rather than decorative is naming it when it
    happens anyway."""

    def test_a_member_that_landed_ahead_of_its_blocker(self):
        one = self.repo("charter")
        two = self.repo("charter-metrics")
        self.make_change(SLUG, [("charter", ()), ("charter-metrics", ("charter",))])
        # The dependent landed; its blocker did not.
        self.declare(SLUG, "charter-metrics", self.land_a_merge(two, SLUG))
        self.assertTrue(one.exists())
        line = self.one_line()
        self.assertIn("charter-metrics", line)
        self.assertIn("landed while", line)
        self.assertIn("'charter'", line)

    def test_the_right_way_round_reports_nothing(self):
        one = self.repo("charter")
        two = self.repo("charter-metrics")
        self.make_change(SLUG, [("charter", ()), ("charter-metrics", ("charter",))])
        self.declare(SLUG, "charter", self.land_a_merge(one, SLUG))
        self.declare(SLUG, "charter-metrics", self.land_a_merge(two, SLUG))
        self.assertEqual(self.findings(), [])

    def test_it_is_a_pure_function_of_the_record_and_the_landings(self):
        """`out_of_order` is the intersection of two things `change` already derives, so
        it cannot disagree with the gate that refuses. Asked directly, one layer below the
        git reads, where nothing else can answer."""
        rec = change.new_record(SLUG, "w", "t", "now")
        rec["members"] = [{"repo": "a", "branch": "b", "needs": []},
                          {"repo": "b", "branch": "b", "needs": ["a"]}]
        self.assertEqual(commands_change.out_of_order(rec, {"b"}), {"b": ["a"]})
        self.assertEqual(commands_change.out_of_order(rec, {"a", "b"}), {})
        self.assertEqual(commands_change.out_of_order(rec, {"a"}), {})


class TestAStrayBranchIsNamed(DivergenceCase):
    """The one check that looks outside the change: a branch named like a member's, in a
    repository nobody declared, is either a member somebody forgot to add or a collision
    with a name that means something else. Both are worth a sentence."""

    def test_a_members_branch_name_in_a_repo_that_is_a_member_of_nothing(self):
        self.repo("charter")
        stray = self.repo("charter-jira")
        self.make_change(SLUG, [("charter", ())])
        git(stray, "switch", "-q", "-c", change.default_branch(SLUG))
        got = commands_change.stray_branches(self.WS)
        self.assertEqual(len(got), 1, got)
        self.assertIn("charter-jira", got[0])
        self.assertIn("member of", got[0])

    def test_a_repo_that_is_a_member_is_not_reported_for_having_the_branch(self):
        """The whole point of a member is that it has this branch."""
        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        git(repo, "switch", "-q", "-c", change.default_branch(SLUG))
        self.assertEqual(commands_change.stray_branches(self.WS), [])

    def test_a_workspace_with_no_changes_reports_nothing(self):
        """No records means no branch names to look for, and a check that looked for
        `change/*` by convention would report every branch anybody ever named that way —
        which is the convention §3.4 refuses to derive membership from."""
        stray = self.repo("charter-jira")
        git(stray, "switch", "-q", "-c", "change/whatever")
        self.assertEqual(commands_change.stray_branches(self.WS), [])


class TestTheChecksDegradeRatherThanGuess(DivergenceCase):
    """Every place `divergences` and `stray_branches` decline to answer, driven.

    **A check that cannot see is not a check that found nothing**, and every one of these
    is that distinction: a member with no clone, a clone charter cannot resolve a default
    branch for, a git call that failed. Each contributes NOTHING rather than a guess,
    because a divergence invented out of a failed read is the confident wrong answer
    ADR 0009 forbids — and because this runs at SessionStart, where a false FAIL is a red
    row on every session of a plane whose only sin is calling its branch something else.
    """

    def test_a_member_with_no_clone_contributes_nothing(self):
        """`charter change add` refused this repo once; a record can still name it after
        the clone was removed. `doctor` says the record is unreadable-or-absent elsewhere;
        this check has nothing to say about a repository that is not here."""
        self.make_change(SLUG, [("gone", ())])
        self.assertEqual(self.findings(), [])

    def test_a_clone_with_no_resolvable_default_branch_contributes_nothing(self):
        """Every question below is *relative to the default branch*. A repository charter
        cannot resolve one for produces no findings rather than findings against a branch
        nobody uses — `_plane_default_branch` answers `None` and means it."""
        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        self.land_a_merge(repo, SLUG, trailer=False)
        self.assertEqual(len(self.findings()), 1)      # the control: it does report
        git(repo, "branch", "-m", "main", "trunk")     # now nothing names a default
        self.assertEqual(self.findings(), [])

    def test_a_git_call_that_fails_is_read_as_no_and_never_as_yes(self):
        """The direction that matters. `_contains` and `_branch_exists` answer False for
        anything charter could not resolve, so a failed read never manufactures a
        containment — it only ever loses one."""
        repo = self.repo("charter")
        self.assertFalse(commands_change._branch_exists(repo, "no-such-branch"))
        self.assertFalse(commands_change._contains(repo, "0" * 40, "main"))
        self.assertFalse(commands_change._trailer_names(repo, "0" * 40, SLUG))

    def test_the_trailer_check_reads_this_commit_and_not_a_range(self):
        """`git show --no-patch --format=%B` on the sha itself. A `--grep` over a range
        would answer about the RANGE, so a trailer anywhere in history would vouch for a
        commit that carries none."""
        repo = self.repo("charter")
        with_trailer = self.land_a_merge(repo, SLUG)
        self.assertTrue(commands_change._trailer_names(repo, with_trailer, SLUG))
        self.assertFalse(commands_change._trailer_names(repo, with_trailer, "other-slug"))
        # The commit BEFORE it carries no trailer, even though one exists further along.
        parent = git(repo, "rev-parse", f"{with_trailer}^1").stdout.strip()
        self.assertFalse(commands_change._trailer_names(repo, parent, SLUG))

    def test_the_trailer_must_be_its_own_line_and_not_a_mention(self):
        """A commit message that talks ABOUT a change is not a commit charter authored
        for it. The comparison is against a stripped LINE, so prose containing the words
        does not vouch for the commit."""
        repo = self.repo("charter")
        (repo / "x").write_text("1\n")
        git(repo, "add", "-A")
        git(repo, "-c", "commit.gpgsign=false", "commit", "-qm",
            f"mentions Charter-Change: {SLUG} in the middle of a sentence here")
        self.assertFalse(commands_change._trailer_names(repo, sha(repo), SLUG))

    def test_the_branch_listing_answers_empty_for_a_repo_git_will_not_read(self):
        """`_local_branches` is one call for N questions, so a failure there would
        otherwise turn into N wrong answers rather than one missing one."""
        self.assertEqual(commands_change._local_branches(self.tmp / "not-a-repo"), set())

    def test_the_branch_listing_finds_every_local_branch_and_no_remote_one(self):
        repo = self.repo("charter")
        git(repo, "branch", "feature/x")
        git(repo, "branch", "change/whatever")
        self.assertEqual(commands_change._local_branches(repo),
                         {"main", "feature/x", "change/whatever"})

    def test_a_record_that_does_not_parse_contributes_no_branch_names(self):
        """`stray_branches` builds its wanted set from records it could READ. Guessing
        branch names from a record that did not parse is the unearned diagnosis ADR 0009
        forbids — and `change/<slug>` by convention would report every branch anybody ever
        named that way, which is the convention §3.4 refuses to derive membership from."""
        stray = self.repo("charter-jira")
        git(stray, "switch", "-q", "-c", change.default_branch(SLUG))
        change.path_for(self.WS, SLUG).parent.mkdir(parents=True, exist_ok=True)
        change.path_for(self.WS, SLUG).write_text('{"change": "broken"}')
        self.assertEqual(commands_change.stray_branches(self.WS), [])


class TestDoctorSaysItAtFail(DivergenceCase):
    """FAIL, not WARN, and the reason is mechanical: `cmd_doctor` exits non-zero only on
    FAIL, and that exit code is the only thing that makes the SessionStart wrapper print."""

    def test_a_divergence_is_a_fail_row(self):
        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        self.land_a_merge(repo, SLUG, trailer=False)
        r = doctor.check_changes()
        self.assertEqual(r.status, doctor.FAIL)
        self.assertIn("charter", r.detail)
        self.assertNotEqual(r.status, doctor.WARN)

    def test_an_out_of_order_landing_is_a_fail_row_too(self):
        self.repo("charter")
        two = self.repo("charter-metrics")
        self.make_change(SLUG, [("charter", ()), ("charter-metrics", ("charter",))])
        self.declare(SLUG, "charter-metrics", self.land_a_merge(two, SLUG))
        r = doctor.check_changes()
        self.assertEqual(r.status, doctor.FAIL)
        self.assertIn("landed while", r.detail)

    def test_a_record_charter_cannot_read_is_its_own_fail_and_not_folded_in(self):
        """"Charter cannot read this file" and "git disagrees with this file" send the
        reader to two different places; one count covering both sends them to neither."""
        self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        change.path_for(self.WS, SLUG).write_text('{"change": "x"}')
        r = doctor.check_changes()
        self.assertEqual(r.status, doctor.FAIL)
        self.assertIn("unreadable", r.detail)

    def test_a_clean_plane_is_ok_and_names_the_optional_prompt(self):
        """Task 9 Step 5's other half: doctor NAMES `charter guard ask --local …` and does
        not run it. `--local` is part of the recommendation — without it `guard ask` writes
        the plane's COMMITTED settings, and consent that travels in a commit enrols a whole
        team on one person's click (ADR 0003)."""
        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        self.declare(SLUG, "charter", self.land_a_merge(repo, SLUG))
        r = doctor.check_changes()
        self.assertEqual(r.status, doctor.OK, r.detail)
        self.assertIn("--local", r.detail)
        self.assertIn("charter change land", r.detail)

    def plane_files(self) -> list[str]:
        """Every path under the plane root, so "wrote nothing" is a whole-tree claim.

        Not `.claude/settings.local.json` by name: a check that only looks where it
        expects the write is a check the next write walks past, and the property is that
        this row WRITES NOTHING — not that it writes nothing in one directory.
        """
        return sorted(str(p.relative_to(self.tmp))
                      for p in self.tmp.rglob("*") if p.is_file())

    def test_doctor_writes_nothing_at_all_on_every_branch_it_has(self):
        """It states the trade-off and does not settle a legitimate choice by writing a
        line while nobody is looking (ADR 0017).

        **All four branches, because a write on any one of them is the same defect.** The
        first cut of this asserted only over a plane with one un-landed change, and a
        mutation that wrote the rule from the `no changes at all` branch survived it —
        which is a test of one code path wearing the name of a property.
        """
        cases = ["no changes at all"]
        before = self.plane_files()
        doctor.check_changes()
        self.assertEqual(self.plane_files(), before, cases[-1])

        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        cases.append("a change with nothing landed")
        before = self.plane_files()
        doctor.check_changes()
        self.assertEqual(self.plane_files(), before, cases[-1])

        merge = self.land_a_merge(repo, SLUG)
        self.declare(SLUG, "charter", merge)
        cases.append("a clean landed change (the OK row that names the prompt)")
        self.assertEqual(doctor.check_changes().status, doctor.OK)
        before = self.plane_files()
        doctor.check_changes()
        self.assertEqual(self.plane_files(), before, cases[-1])

        git(repo, "reset", "-q", "--hard", "HEAD~1")
        cases.append("a divergence (the FAIL row)")
        self.assertEqual(doctor.check_changes().status, doctor.FAIL)
        before = self.plane_files()
        doctor.check_changes()
        self.assertEqual(self.plane_files(), before, cases[-1])
        self.assertEqual(len(cases), 4)

    def test_a_plane_with_no_changes_says_none_and_stays_ok(self):
        self.assertEqual(doctor.check_changes().status, doctor.OK)
        self.assertEqual(doctor.check_changes().detail, "none")


class TestTheDoctorRowsOwnBranches(DivergenceCase):
    """Every arm of `check_changes`, including the ones a healthy plane never takes."""

    def test_a_plane_that_is_not_a_control_plane_says_so_and_reads_nothing(self):
        from charter import config
        config.HAS_CONTROL_PLANE = False
        try:
            r = doctor.check_changes()
        finally:
            config.HAS_CONTROL_PLANE = True
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("no control plane", r.detail)

    def test_a_check_that_could_not_run_is_WARN_and_says_its_silence_means_nothing(self):
        """`doctor`'s own discipline: "not checked" is WARN, never OK — a check that
        silently did nothing is worse than no check (`test_doctor_absent_is_not_health`)."""
        from charter import workspace as _ws
        with mock.patch.object(_ws, "list_workspaces", side_effect=OSError("nope")):
            r = doctor.check_changes()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("not checked", r.detail)
        self.assertIn("silence means nothing", r.hint)

    def test_the_catch_is_narrow_and_a_charter_bug_is_not_swallowed(self):
        """`check_memory_indexes`' recorded failure: a broad catch once swallowed a
        NameError and reported OK. A programming error must reach the caller."""
        from charter import workspace as _ws
        with mock.patch.object(_ws, "list_workspaces", side_effect=NameError("typo")):
            with self.assertRaises(NameError):
                doctor.check_changes()

    def test_many_findings_are_bounded_and_the_row_says_how_many_it_hid(self):
        """A row that listed forty divergences is a row nobody reads, and one that listed
        three and stopped silently is a row that lies about the size of the problem."""
        for i in range(5):
            repo = self.repo(f"r{i}")
            self.make_change(f"c-{i}", [(f"r{i}", ())])
            self.land_a_merge(repo, f"c-{i}", trailer=False)
        r = doctor.check_changes()
        self.assertEqual(r.status, doctor.FAIL)
        self.assertIn("more)", r.detail)
        self.assertLess(len(r.detail), 2000)

    def test_many_unreadable_records_are_bounded_the_same_way(self):
        for i in range(5):
            self.repo(f"r{i}")
            self.make_change(f"c-{i}", [(f"r{i}", ())])
            change.path_for(self.WS, f"c-{i}").write_text('{"change": "x"}')
        r = doctor.check_changes()
        self.assertEqual(r.status, doctor.FAIL)
        self.assertIn("more)", r.detail)

    def test_an_unreadable_record_outranks_a_divergence_and_says_a_different_thing(self):
        """"Charter cannot read this file" and "git disagrees with this file" send the
        reader to two different places. The unreadable one goes first because a change
        nobody can read is a change nobody can land."""
        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        self.land_a_merge(repo, SLUG, trailer=False)
        self.make_change("other", [("charter", ())])
        change.path_for(self.WS, "other").write_text('{"change": "x"}')
        r = doctor.check_changes()
        self.assertIn("unreadable", r.detail)
        self.assertNotIn("charter did not land it", r.detail)

    def test_every_workspace_is_looked_at_and_not_only_the_active_one(self):
        """`check_workspace_clones`' own finding (#156): the half-landed change is rarely
        the one you are standing in."""
        from charter import workspace as _ws
        _ws.ensure("other")
        d = _ws.workspace_dir("other") / "charter"
        d.mkdir(parents=True)
        import subprocess
        subprocess.run(["git", "init", "-q", "-b", "main", str(d)],
                       check=True, capture_output=True)
        for k, v in (("commit.gpgsign", "false"), ("user.email", "t@e"),
                     ("user.name", "t")):
            git(d, "config", "--local", k, v)
        (d / "f").write_text("1\n")
        git(d, "add", "-A")
        git(d, "commit", "-qm", "one")
        rec = change.new_record("elsewhere", "w", "t", "2026-08-29T00:00:00+00:00")
        rec["members"] = [{"repo": "charter", "branch": "change/elsewhere", "needs": []}]
        change.write("other", "elsewhere", rec)
        git(d, "switch", "-q", "-c", "change/elsewhere")
        (d / "g").write_text("2\n")
        git(d, "add", "-A")
        git(d, "commit", "-qm", "work")
        git(d, "switch", "-q", "main")
        git(d, "merge", "-q", "--no-ff", "-m", "landed elsewhere", "change/elsewhere")
        r = doctor.check_changes()
        self.assertEqual(r.status, doctor.FAIL)
        self.assertIn("other/elsewhere", r.detail)


class TestItReadsThisDiskAndNeverTheNetwork(DivergenceCase):
    def test_no_process_but_git_is_started(self):
        """This runs from SessionStart. A check that fetched would put the network on
        every session start, and the honest consequence — it can UNDER-report and can
        never invent — is in the row's own hint rather than only here."""
        seen: list[list[str]] = []
        real = commands_change.util.run

        def spy(cmd, *a, **kw):
            seen.append(list(cmd))
            return real(cmd, *a, **kw)

        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        self.declare(SLUG, "charter", self.land_a_merge(repo, SLUG))
        commands_change.util.run = spy
        try:
            self.findings()
        finally:
            commands_change.util.run = real
        self.assertTrue(seen, "no git ran — this test proves nothing")
        self.assertEqual({c[0] for c in seen}, {"git"})
        for cmd in seen:
            for word in ("fetch", "pull", "push", "ls-remote", "remote"):
                self.assertNotIn(word, cmd, cmd)


if __name__ == "__main__":
    unittest.main()
