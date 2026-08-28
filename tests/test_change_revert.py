"""`charter change revert` — a revert is a NEW change, and that is the whole design.

Force-pushing three default branches back past the merges leaves a world where the change
happened, was undone, and no repository's history mentions either — the exact failure the
cross-repo surface exists to prevent. `component-api-2` and `revert-component-api-2`, both
named, both cross-referenced, in every repository they touched, reads as a decision six
months later; a force-push reads as corruption.

Three properties run through every case here.

**The refusals are an absence, and the absence is measured.** No force-push, no branch
deletion, no default-branch reset, no closing a request charter did not open. Those are not
guarded by a flag — the argv is never built — so the test that pins them RECORDS every git
invocation a real revert makes and asserts what is in it. A source-level grep is the other
half (`tests/test_commands_change.py`) and neither substitutes for the other: the grep
covers paths this file does not walk, and this covers argv the grep cannot see is dynamic.

**`-m 1` is asked of git, never remembered.** A squash landing is an ordinary one-parent
commit and `-m` on one fails; a merge landing has two and `git revert` refuses without it.
Both shapes are landed for real here and reverted for real, because the parent count is the
one fact that decides the flag and storing it in the record would be a derivable fact
cached for convenience — the sentence ADR 0011 is.

**A member landed outside charter is named, not guessed.** No line in `changes/log/` means
no merge sha, and charter will not pick the merge commit that looks about right (ADR 0009).
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import change, commands_change
from tests._changerepo import ChangeRepoCase, git, sha

SLUG = "component-api-2"
REVERTED = "revert-component-api-2"


def args(**kw) -> SimpleNamespace:
    kw.setdefault("workspace", "ws")
    for k in ("change", "repo", "branch", "needs", "why"):
        kw.setdefault(k, None)
    return SimpleNamespace(**kw)


class RevertCase(ChangeRepoCase):
    def call(self, fn, **kw) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = fn(args(**kw))
        return code, out.getvalue(), err.getvalue()

    def revert(self, slug: str = SLUG):
        return self.call(commands_change.cmd_change_revert, change=slug)

    def record_git(self):
        """Every git argv a revert issues, captured through the one funnel.

        `_git` is the single place the `["git", "-C", <clone>, …]` prefix is built, and
        that funnel is what makes "what did this command run" a question with an answer:
        a call site assembling its own argv would be invisible here.
        """
        seen: list[list[str]] = []
        real = commands_change.util.run

        def spy(cmd, *a, **kw):
            seen.append(list(cmd))
            return real(cmd, *a, **kw)

        return seen, mock.patch.object(commands_change.util, "run", spy)

    def one_landed_member(self, *, squash: bool = False, trailer: bool = True):
        """One member, landed for real, with charter's declaration beside it."""
        repo = self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        merge = self.land_a_merge(repo, SLUG, squash=squash, trailer=trailer)
        self.declare(SLUG, "charter", merge)
        return repo, merge


class TestARevertIsANewChange(RevertCase):
    def test_it_creates_a_new_record_whose_members_are_the_landed_ones(self):
        self.one_landed_member()
        code, _, err = self.revert()
        self.assertEqual(code, 0, err)
        rec = change.read("ws", REVERTED)
        self.assertEqual([m["repo"] for m in rec["members"]], ["charter"])

    def test_its_why_names_the_original_slug(self):
        """The one thread back. Six months later this record is what says which change
        this undid, and a `why` that did not name it would leave the reader with two
        records and no relation between them."""
        self.one_landed_member()
        self.revert()
        self.assertIn(SLUG, change.read("ws", REVERTED)["why"])

    def test_the_original_record_is_untouched(self):
        """A revert adds a change; it does not edit the one it reverts. The original is
        the statement that this work happened, and editing it to say it was undone would
        destroy exactly the half a stranger needs."""
        self.one_landed_member()
        before = change.path_for("ws", SLUG).read_bytes()
        self.revert()
        self.assertEqual(change.path_for("ws", SLUG).read_bytes(), before)

    def test_from_there_it_is_an_ordinary_change_with_ordinary_gates(self):
        """`show` reads it, `drop` needs a `--why`, `land` will refuse it the same way —
        there is no second lifecycle and no privileged revert path."""
        self.one_landed_member()
        self.revert()
        code, out, _ = self.call(commands_change.cmd_change_show, change=REVERTED)
        self.assertEqual(code, 0)
        self.assertIn(REVERTED, out)

    def test_the_ordering_is_the_originals_reversed(self):
        """**Undoing goes the other way round.** `charter-metrics` needed `charter` to land
        first because its code depends on charter's new API — so reverting `charter` while
        `charter-metrics` still depends on the API it removes leaves the dependent broken,
        which is the world the revert exists to restore.

        This is the case a revert that simply copied `needs` across would get exactly
        backwards, and it would look right in every single-member fixture."""
        one = self.repo("charter")
        two = self.repo("charter-metrics")
        self.make_change(SLUG, [("charter", ()), ("charter-metrics", ("charter",))])
        self.declare(SLUG, "charter", self.land_a_merge(one, SLUG))
        self.declare(SLUG, "charter-metrics", self.land_a_merge(two, SLUG))
        code, _, err = self.revert()
        self.assertEqual(code, 0, err)
        rec = change.read("ws", REVERTED)
        needs = {m["repo"]: m["needs"] for m in rec["members"]}
        self.assertEqual(needs, {"charter": ["charter-metrics"], "charter-metrics": []})

    def test_a_dependent_charter_never_landed_is_not_a_blocker_of_the_revert(self):
        """A member whose dependent is not being reverted is not waiting for anybody —
        carrying the edge across would be a blocker nothing will ever land, which is the
        record `change.order_refusal` refuses outright."""
        one = self.repo("charter")
        self.repo("charter-metrics")
        self.make_change(SLUG, [("charter", ()), ("charter-metrics", ("charter",))])
        self.declare(SLUG, "charter", self.land_a_merge(one, SLUG))
        code, _, err = self.revert()
        self.assertEqual(code, 0, err)
        rec = change.read("ws", REVERTED)
        self.assertEqual([m["needs"] for m in rec["members"]], [[]])

    def test_a_second_revert_of_the_same_change_is_refused_by_name(self):
        """Not silently re-derived over the top: the first revert's branches exist and its
        record holds decisions somebody may have edited. `forget` is the way back."""
        self.one_landed_member()
        self.revert()
        code, _, err = self.revert()
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("already exists", err)
        self.assertIn(REVERTED, err)


class TestTheBranchCarriesTheRevert(RevertCase):
    def test_a_merge_landing_is_reverted_with_m_1(self):
        """Two parents, so `-m` is required — without it `git revert` refuses outright."""
        repo, merge = self.one_landed_member()
        code, _, err = self.revert()
        self.assertEqual(code, 0, err)
        git(repo, "switch", "-q", change.default_branch(REVERTED))
        body = git(repo, "log", "-1", "--format=%B").stdout
        self.assertIn("Revert", body)
        self.assertIn(merge[:7], body)

    def test_a_squash_landing_is_reverted_without_m(self):
        """One parent, so `-m 1` would FAIL — `git revert -m 1` on an ordinary commit
        errors with "mainline was specified but commit is not a merge". This is the
        measured reason the parent count is asked of git rather than remembered."""
        repo, _ = self.one_landed_member(squash=True)
        code, _, err = self.revert()
        self.assertEqual(code, 0, err)
        git(repo, "switch", "-q", change.default_branch(REVERTED))
        self.assertIn("Revert", git(repo, "log", "-1", "--format=%B").stdout)

    def test_no_m_flag_is_passed_when_git_says_the_commit_has_one_parent(self):
        """**The ARGV, not the outcome, and that is a correction with a measurement behind
        it.** The spec's reason for this conditional is that *"a squash landing is an
        ordinary commit and `-m` on one fails"* — and on **git 2.50.1 (Apple Git-155)**
        that is simply not true: `git revert --no-edit -m 1 <one-parent-sha>` returns 0,
        and so does `-m 2`. Measured, twice, in a throwaway repo.

        So an outcome assertion cannot see this at all — hard-coding `-m 1` passes every
        test that only looks at whether the revert worked, which is exactly what happened:
        the mutation survived until this test existed. What is still true on every git is
        the claim charter makes about itself — it asks git how many parents there are and
        passes `-m` only for a merge — and older gits DO fail with *"mainline was specified
        but commit is not a merge"*, so the conditional earns its place on the versions
        this is not measured against as well.
        """
        repo, _ = self.one_landed_member(squash=True)
        seen, patched = self.record_git()
        with patched:
            code, _, err = self.revert()
        self.assertEqual(code, 0, err)
        reverts = [c for c in seen if "revert" in c and "--abort" not in c]
        self.assertEqual(len(reverts), 1, reverts)
        self.assertNotIn("-m", reverts[0])

    def test_the_m_flag_is_passed_when_git_says_there_are_two(self):
        """The other half. Without it the assertion above is satisfied by never passing
        `-m` at all, which fails a real merge revert on every git there is."""
        repo, _ = self.one_landed_member()
        seen, patched = self.record_git()
        with patched:
            code, _, err = self.revert()
        self.assertEqual(code, 0, err)
        reverts = [c for c in seen if "revert" in c and "--abort" not in c]
        self.assertEqual(len(reverts), 1, reverts)
        self.assertIn("-m", reverts[0])
        self.assertEqual(reverts[0][reverts[0].index("-m") + 1], "1")

    def test_the_parent_count_comes_from_git_and_not_from_the_record(self):
        """The layer below the argv: `_parents` is what the conditional reads."""
        repo, _ = self.one_landed_member(squash=True)
        self.assertEqual(commands_change._parents(repo, sha(repo)), 1)
        other = self.repo("other")
        self.assertEqual(commands_change._parents(other, self.land_a_merge(other, "x-1")),
                         2)
        # And a sha git does not have answers `None`, never 1 — a default of 1 would drop
        # `-m` on a merge charter could not resolve and the revert would simply fail.
        self.assertIsNone(commands_change._parents(repo, "0" * 40))

    def test_the_branch_is_created_off_the_default_branch(self):
        """Read from the CLONE, never from the record: a base branch in a committed file
        is a destination in a committed file, which §6.1 rule 4 forbids."""
        repo, _ = self.one_landed_member()
        code, _, err = self.revert()
        self.assertEqual(code, 0, err)
        branch = change.default_branch(REVERTED)
        base = git(repo, "merge-base", f"refs/heads/{branch}", "refs/heads/main")
        # The merge base IS main's tip, which is what "branched off main, and main has not
        # moved since" means. A weaker `assertTrue` would pass for a branch rooted at the
        # first commit of the repository.
        self.assertEqual(base.stdout.strip(), sha(repo, "refs/heads/main"))

    def test_the_revert_actually_undoes_the_file(self):
        """The property, not the commit message: the file the member added is gone again
        on the revert branch and still there on `main`."""
        repo, _ = self.one_landed_member()
        self.revert()
        marker = change.default_branch(SLUG).replace("/", "_")
        git(repo, "switch", "-q", "main")
        self.assertTrue((repo / marker).exists())
        git(repo, "switch", "-q", change.default_branch(REVERTED))
        self.assertFalse((repo / marker).exists())


class TestTheRefusals(RevertCase):
    """§3.7's four, each by name. Recorded rather than reasoned about."""

    def test_no_force_push_no_branch_deletion_no_reset_and_no_request_is_touched(self):
        self.one_landed_member()
        seen, patched = self.record_git()
        with patched:
            code, _, err = self.revert()
        self.assertEqual(code, 0, err)
        self.assertTrue(seen, "no git ran at all — this test proves nothing")
        flat = [" ".join(c) for c in seen]
        for banned in ("--force", "--force-with-lease", "-f ", "push", "branch -D",
                       "branch -d", "reset", "clean", "gh ", "glab "):
            for line in flat:
                self.assertNotIn(banned, line, f"a revert issued {line!r}")

    def test_every_process_a_revert_starts_is_git(self):
        """The other direction of the same property: not "it did not force-push" but "it
        ran nothing that was not git"."""
        self.one_landed_member()
        seen, patched = self.record_git()
        with patched:
            self.revert()
        self.assertEqual({c[0] for c in seen}, {"git"})

    def test_it_refuses_over_uncommitted_work_and_says_why(self):
        """`git revert` commits into the checkout, so running it over somebody's work in
        progress would fold that work into a revert commit — and the recovery for that is
        worse than the wait."""
        repo, _ = self.one_landed_member()
        (repo / "f").write_text("dirty\n")
        code, _, err = self.revert()
        self.assertEqual(code, 1)
        self.assertIn("uncommitted", err)
        self.assertNotIn("no landing record", err)   # and not the gate above it

    def test_a_refused_member_still_leaves_the_original_branch_alone(self):
        repo, _ = self.one_landed_member()
        (repo / "f").write_text("dirty\n")
        before = sha(repo, "refs/heads/main")
        self.revert()
        self.assertEqual(sha(repo, "refs/heads/main"), before)


class TestALandingCharterDidNotRecordIsNamed(RevertCase):
    def test_a_member_with_no_log_line_is_handed_to_a_human(self):
        """No merge sha, so no revert. Charter says so rather than picking the merge
        commit that looks about right — ADR 0009: it degrades to silence, never to a
        confident wrong answer."""
        one = self.repo("charter")
        self.repo("charter-metrics")
        self.make_change(SLUG, [("charter", ()), ("charter-metrics", ())])
        self.declare(SLUG, "charter", self.land_a_merge(one, SLUG))
        code, _, err = self.revert()
        self.assertEqual(code, 0, err)
        self.assertIn("charter-metrics", err)
        self.assertIn("no landing record", err)
        self.assertEqual([m["repo"] for m in change.read("ws", REVERTED)["members"]],
                         ["charter"])

    def test_a_change_with_no_landings_at_all_is_refused_by_its_own_message(self):
        self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        code, _, err = self.revert()
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("nothing to revert", err)
        self.assertNotIn("already exists", err)     # and not a neighbouring gate

    def test_the_merge_sha_is_never_guessed_from_the_branch_name(self):
        """The mutation Step 6 names. A branch called `change/<slug>` exists in the clone
        and is not the landing — deriving the sha from it would produce a plausible commit
        and revert the wrong thing."""
        self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        code, _, _ = self.revert()
        self.assertEqual(code, commands_change.REFUSED)
        self.assertFalse(change.exists("ws", REVERTED))


class TestTheLogIsUntrustedToo(RevertCase):
    """It is never committed, which makes it LOCAL rather than trustworthy: a hand edit or
    a half-written append from a killed process reaches the same argv."""

    def test_a_merge_value_that_is_not_a_sha_is_refused_by_name(self):
        self.repo("charter")
        self.make_change(SLUG, [("charter", ())])
        self.declare(SLUG, "charter", "--upload-pack=touch /tmp/pwned")
        code, _, err = self.revert()
        self.assertEqual(code, 1)
        self.assertIn("not a sha", err)

    def test_a_sha_with_a_trailing_newline_is_refused(self):
        """Python's `$` matches before a trailing newline, so `.match` would have admitted
        this — a newline on its way into a git argv out of a file anybody can edit."""
        self.assertIsNone(commands_change._SHA_RE.fullmatch("e0c9d13\n"))
        self.assertIsNotNone(commands_change._SHA_RE.fullmatch("e0c9d13"))

    def test_a_malformed_line_does_not_take_the_good_ones_down(self):
        repo, merge = self.one_landed_member()
        with open(change.log_path("ws"), "a") as f:
            f.write("{not json\n")
        self.assertEqual([e["merge"] for e in change.landings("ws", SLUG)], [merge])


if __name__ == "__main__":
    unittest.main()
