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

from charter import change, commands_change, doctor
from tests._changerepo import ChangeRepoCase, git

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
