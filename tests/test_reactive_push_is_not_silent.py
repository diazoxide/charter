"""A reactive memory push that cannot land must say so somewhere (#373).

`charter persona remember` on a plane whose `share` is `push` writes the file, commits it
to the plane root's checked-out branch, prints *"→ pushing to the control plane in the
background"* and exits 0. The push itself happens in a DETACHED `charter workspace _pushbg`
with stdout and stderr on ``/dev/null``. When the plane's default branch is protected — as
charter's own is, by a ruleset with a required status check — that push is rejected and the
commit is left sitting on local `main`, unpushed, with nothing anywhere saying so.

Three subagents stranded eleven commits that way in one session and all three reported
"memory recorded". Such a commit then survives only until somebody runs the standard move
for a diverged branch, `git reset --hard origin/main` — which is exactly what an agent
reaches for on noticing `main` is ahead for reasons it did not intend.

**The policy already existed and this path never reached it.** #167 decided what charter
does when the plane's default branch requires a pull request: `planegit._land_via_branch`
pushes the commit that is already on HEAD to `charter/<sha>` and hands back a compare URL,
without ever moving the root's HEAD (ADR 0008, and #157's guard). `test_save_pr_gated.py`
holds that for `charter save`. The reactive path missed it because
`commands_workspace.cmd_workspace_pushbg` was a SECOND implementation of the push — no
protected-branch recognition and ``return 0`` on every failure — which is the same
structural fault `planegit`'s own module docstring says the module was extracted to end,
one layer down: there were two committers, and then there were two pushers.

    charter save            → rejected → _land_via_branch → PR URL printed
    persona remember (bg)   → rejected → return 0

So there are two halves to this, failing in opposite directions:

* the background push must reach the same policy (the first two classes below), and
* its outcome must survive a process that had nobody listening — recorded, and reported by
  `doctor`, which is the surface ADR 0008 already chose for the plane root (last class).

Deliberately NOT "skip the commit when the branch is protected". charter cannot know a
branch is protected without asking the remote; `doctor` runs from SessionStart and must not
reach the network; and ADR 0009 forbids naming a cause charter only inferred. The rejection
IS the evidence, and it arrives after the commit already exists.
"""

from __future__ import annotations

import io
import json
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

from charter import commands_workspace, config, doctor, planegit
from tests._isolation import PersonaIso

#: Pinned rather than inherited, for the reason `test_doctor_plane_root_behind` records:
#: `init.defaultBranch` decides what these fixtures even are, and a machine that sets it and
#: a CI runner that does not disagree about it. Signing is pinned off so nothing stops to
#: ask a passphrase of a suite with nobody there to answer.
_PINS = ["-c", "init.defaultBranch=main", "-c", "commit.gpgsign=false",
         "-c", "tag.gpgsign=false"]

#: GitHub's protected-branch refusal. Kept character-for-character in step with
#: `test_save_pr_gated.py`'s fixture, and deliberately WITHOUT the `! [remote rejected]`
#: line real git also prints: that phrase makes `push_head` take its non-fast-forward
#: rebase-retry first, and a fake that intercepts only `push` would then run a REAL
#: `git rebase FETCH_HEAD` against a FETCH_HEAD this fixture never creates. The retry is
#: harmless in life (the rebase is a no-op and the second push is refused identically) and
#: pure noise here, so the fixture stays on the shorter form the sibling test established.
_GH006 = ("remote: error: GH006: Protected branch update failed for refs/heads/main.\n"
          "remote: error: Required status check \"ci\" is expected.")


def git(where, *args):
    return subprocess.run(["git", *_PINS, "-C", str(where), *args], check=True,
                          capture_output=True, text=True)


class PlaneCase(PersonaIso):
    """A plane root that is a real git repo with an origin charter recognises."""

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        # Re-derive rather than hand-setting HAS_CONTROL_PLANE: `config.derive` reads the
        # marker file, which only exists as of the line above. `PersonaIso` restores the
        # real values from the snapshot it took first, so this needs no cleanup of its own.
        config.use(self.tmp)
        self.root = Path(config.ROOT)
        subprocess.run(["git", *_PINS, "init", "-q", "-b", "main", str(self.root)],
                       check=True, capture_output=True)
        git(self.root, "config", "user.email", "t@e")
        git(self.root, "config", "user.name", "t")
        (self.root / "seed").write_text("x\n")
        git(self.root, "add", "-A")
        git(self.root, "commit", "-qm", "seed")
        # Without an origin on a KNOWN forge, the pusher returns before ever pushing and
        # every assertion below would pass vacuously.
        git(self.root, "remote", "add", "origin", "https://github.com/acme/plane.git")

    def head_branch(self) -> str:
        return git(self.root, "symbolic-ref", "--short", "HEAD").stdout.strip()

    def fake_git(self, *, protected: bool, branch_push_ok: bool = True,
                 failure: str | None = None):
        """Swap git out for one that answers pushes the way a protected remote does.

        BOTH module-level `_git` bindings are replaced. `commands_workspace` does
        ``from .commands import _git`` at import time, so patching `planegit._git` alone
        would leave the pre-fix background pusher running real git against a remote that
        does not exist — and a test that cannot reach the code it is about is not a test.
        """
        real = planegit._git
        seen: list[list[str]] = []

        def fake(args, cwd=None, **kw):
            seen.append(list(args))
            # `push` is not args[0]: the credential flag is prefixed, so the invocation is
            # `-c credential.helper=… push <url> <refspec>`.
            if "push" in args:
                if failure is not None:
                    return subprocess.CompletedProcess(args, 1, "", failure)
                if protected and args[-1].endswith(":main"):
                    return subprocess.CompletedProcess(args, 1, "", _GH006)
                if not branch_push_ok:
                    return subprocess.CompletedProcess(args, 1, "", "remote: boom")
                return subprocess.CompletedProcess(args, 0, "", "")
            return real(args, cwd=cwd, **kw)

        self.enterContext(mock.patch.object(planegit, "_git", fake))
        self.enterContext(mock.patch.object(commands_workspace, "_git", fake))
        return seen

    def pushbg(self):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands_workspace.cmd_workspace_pushbg(None)
        return rc, out.getvalue() + err.getvalue()


class TestTheBackgroundPushObeysTheProtectedBranchPolicy(PlaneCase):
    def test_it_pushes_the_pull_request_branch_the_way_charter_save_does(self):
        """#167's policy, on the path SessionStart actually tells an agent to use."""
        seen = self.fake_git(protected=True)
        self.pushbg()
        pushed = [a for a in seen if "push" in a]
        self.assertTrue(any(p[-1].startswith("HEAD:refs/heads/charter/") for p in pushed),
                        pushed)

    def test_the_plane_roots_HEAD_never_moves(self):
        """#157's invariant. The background path has to coexist with the guard, not poke a
        hole in it — `push HEAD:refs/heads/<new>` needs no checkout and no worktree."""
        self.fake_git(protected=True)
        before = self.head_branch()
        self.pushbg()
        self.assertEqual(self.head_branch(), before)

    def test_no_checkout_switch_or_branch_is_ever_run_in_the_root(self):
        seen = self.fake_git(protected=True)
        self.pushbg()
        for args in seen:
            for verb in ("checkout", "switch", "worktree", "branch"):
                self.assertNotIn(verb, args, args)

    def test_it_records_where_the_commit_actually_landed(self):
        """A detached child with ``/dev/null`` for a voice has no caller to tell, so the
        outcome is written down instead of discarded. This is the whole of #373: the commit
        was fine, and nobody could find out where it went."""
        self.fake_git(protected=True)
        self.pushbg()
        rec = planegit.push_record()
        self.assertIsNotNone(rec)
        self.assertEqual(rec["outcome"], planegit.BRANCHED)
        self.assertTrue(str(rec.get("landed", "")).startswith("charter/"), rec)
        self.assertIn("github.com/acme/plane/compare/charter/", rec.get("url") or "")

    def test_a_push_that_lands_leaves_no_notice_behind(self):
        """Otherwise the record outlives the condition and `doctor` warns for ever about a
        commit that reached the remote an hour ago."""
        self.fake_git(protected=False)
        planegit.record_push(planegit.PushResult(planegit.FAILED, "main"), head="deadbeef")
        self.pushbg()
        self.assertIsNone(planegit.push_record())

    def test_an_ordinary_failure_is_recorded_and_never_rerouted(self):
        """ADR 0009: charter may name a cause it RECOGNISED. An auth failure is not a
        protected branch, and quietly pushing a side branch for one would be inventing a
        diagnosis — and acting surprisingly on the back of it."""
        seen = self.fake_git(protected=False, failure="fatal: Authentication failed")
        self.pushbg()
        self.assertFalse(any(a[-1].startswith("HEAD:refs/heads/charter/")
                             for a in seen if "push" in a))
        self.assertEqual((planegit.push_record() or {}).get("outcome"), planegit.FAILED)

    def test_a_pull_request_branch_that_also_fails_is_recorded_as_stranded(self):
        """The commit now exists nowhere but this laptop. Recording that as anything softer
        is the green tick over a push that never happened which `commit_push`'s own
        docstring already records twice."""
        self.fake_git(protected=True, branch_push_ok=False)
        self.pushbg()
        self.assertEqual((planegit.push_record() or {}).get("outcome"), planegit.STRANDED)

    def test_the_background_push_still_never_breaks_the_turn(self):
        """It is spawned from a Stop hook. rc 0 regardless is the contract; what changes is
        that rc 0 is no longer the only thing it leaves behind."""
        self.fake_git(protected=True, branch_push_ok=False)
        rc, _ = self.pushbg()
        self.assertEqual(rc, 0)

    def test_the_background_push_says_nothing_on_stdout_or_stderr(self):
        """Its streams are ``/dev/null``; anything printed is written to be discarded. The
        record is the channel, and keeping the announcement off this path is what makes
        that non-negotiable rather than merely preferred."""
        self.fake_git(protected=True)
        _, blob = self.pushbg()
        self.assertEqual(blob.strip(), "")


class TestThereIsOnlyOnePusher(PlaneCase):
    """The defect was a second implementation, so the fix is asserted as one implementation.

    Both callers reaching `planegit.push_head` is what makes the protected-branch policy
    true on both paths by construction, rather than by two lists of forge signatures that
    somebody has to keep in step.
    """

    def test_the_background_half_goes_through_the_shared_pusher(self):
        with mock.patch.object(planegit, "push_head") as m:
            m.return_value = planegit.PushResult(planegit.PUSHED, "main")
            commands_workspace.cmd_workspace_pushbg(None)
        self.assertTrue(m.called)
        self.assertEqual(Path(m.call_args[0][0]), Path(config.ROOT))

    def test_charter_save_goes_through_the_same_one(self):
        (self.root / "personas").mkdir(exist_ok=True)
        (self.root / "personas" / "n.md").write_text("x\n")
        with mock.patch.object(planegit, "push_head") as m:
            m.return_value = planegit.PushResult(planegit.PUSHED, "main")
            out, err = io.StringIO(), io.StringIO()
            with redirect_stdout(out), redirect_stderr(err):
                planegit.commit_push(self.root, ["add", "-A"], "m")
        self.assertTrue(m.called)


class DoctorCase(PlaneCase):
    """The plane root with a REAL upstream, so `@{upstream}` resolves as it does live."""

    def setUp(self) -> None:
        super().setUp()
        git(self.root, "remote", "remove", "origin")
        self.bare = self.tmp.parent / f"{self.tmp.name}-remote.git"
        subprocess.run(["git", *_PINS, "init", "-q", "-b", "main", "--bare", str(self.bare)],
                       check=True, capture_output=True)
        self.addCleanup(lambda: __import__("shutil").rmtree(self.bare, ignore_errors=True))
        git(self.root, "remote", "add", "origin", str(self.bare))
        git(self.root, "push", "-q", "-u", "origin", "main")

    def commit_locally(self, name="memory.md") -> str:
        (self.root / name).write_text("a fact\n")
        git(self.root, "add", "-A")
        git(self.root, "commit", "-qm", f"memory: {name}")
        return git(self.root, "rev-parse", "HEAD").stdout.strip()


class TestDoctorSaysTheCommitDidNotLand(DoctorCase):
    def test_a_branched_push_warns_and_hands_over_the_pull_request_url(self):
        head = self.commit_locally()
        planegit.record_push(
            planegit.PushResult(planegit.BRANCHED, "main", landed="charter/abc1234",
                                url="https://github.com/acme/plane/compare/charter/abc1234?expand=1"),
            head=head)
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("charter/abc1234", r.detail + r.hint)
        self.assertIn("compare/charter/abc1234", r.hint)

    def test_a_failed_push_names_the_hazard_that_destroys_the_commit(self):
        """`git reset --hard origin/main` is the standard move after a divergence and it
        deletes the memory without a trace. A reader who is not told that will run it."""
        head = self.commit_locally()
        planegit.record_push(planegit.PushResult(planegit.FAILED, "main"), head=head)
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("reset --hard", r.hint)

    def test_a_record_whose_commit_reached_the_remote_is_spent(self):
        """Self-clearing on EVIDENCE rather than on a deletion somebody has to remember:
        once the recorded commit is an ancestor of the tracked remote ref, the condition is
        over whatever the file still says."""
        head = self.commit_locally()
        git(self.root, "push", "-q", "origin", "main")
        planegit.record_push(planegit.PushResult(planegit.FAILED, "main"), head=head)
        r = doctor.check_plane_root()
        self.assertNotIn("reset --hard", r.hint)

    def test_an_unreachable_origin_is_not_reported_as_a_stranded_commit(self):
        """A plane with no origin on a forge charter knows has a CONFIGURATION to fix, not
        a commit to rescue — and `commit_push` already says so at the time. Warning here
        too would put the row permanently yellow, the exact cost this check refuses to pay
        for untracked memory files."""
        head = self.commit_locally()
        planegit.record_push(planegit.PushResult(planegit.UNREACHABLE, "main"), head=head)
        self.assertNotIn("reset --hard", doctor.check_plane_root().hint)

    def test_unpushed_commits_are_counted_in_the_detail(self):
        """`clean on main` is a statement about the working TREE that reads as one about
        the plane — the complaint `test_doctor_plane_root_behind` records, in the other
        direction. Three commits sat between a tag and `main` while `git log <tag>..main`
        came back empty and the honest-looking reading was 'nothing to release'."""
        self.commit_locally()
        self.assertIn("1 ahead of", doctor.check_plane_root().detail)

    def test_a_root_in_step_with_its_upstream_says_nothing_extra(self):
        """A count of zero on every clean preflight would be furniture."""
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.OK)
        self.assertNotIn("ahead of", r.detail)

    def test_a_malformed_record_is_not_a_crash(self):
        """`check_plane_root` runs from SessionStart, where a hook may cost a session its
        briefing and must never cost it the turn."""
        self.commit_locally()
        p = config.STATE_DIR / "plane-push.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{ not json")
        self.assertIsInstance(doctor.check_plane_root(), doctor.Result)

    def test_the_record_is_written_under_the_ACTIVE_state_dir(self):
        """Read from `config` at CALL time. A path bound at import writes into the
        developer's real `.charter/` and the isolation harness never sees it."""
        planegit.record_push(planegit.PushResult(planegit.FAILED, "main"), head="abc")
        written = config.STATE_DIR / "plane-push.json"
        self.assertTrue(written.exists())
        self.assertEqual(json.loads(written.read_text())["outcome"], planegit.FAILED)


if __name__ == "__main__":
    unittest.main()
