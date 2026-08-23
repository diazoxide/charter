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
import re
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
        # `$CHARTER_HOME` ESCAPES `PersonaIso`, and this file writes to `config.STATE_DIR`.
        # `config._migrate_state_dir` returns `$CHARTER_HOME` VERBATIM when it is set, so
        # `config.use(tmp)` leaves STATE_DIR pointing outside tmp and every `plane-push.json`
        # here — including the deliberately malformed `{ not json` below — lands in the
        # developer's real `.charter/`. Unset on this machine and on CI, which is exactly
        # why it must be pinned rather than trusted: the day somebody sets it to share a
        # state dir across clones, the suite starts writing into it silently. The same
        # defence `test_dispatch.py` and `test_workspace_enforcement.py` already carry.
        config.STATE_DIR = self.tmp / ".charter"
        self.assertTrue(config.STATE_DIR.is_relative_to(self.tmp))
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

    def test_the_hint_names_where_the_remotes_own_words_are(self):
        """The recorded `detail` is git's answer to "why", and the background pusher's
        stderr went to `/dev/null`, so the file is the only copy. Naming it is what gives
        that field a reader; `doctor`'s own text stays constants (ADR 0009), so nothing a
        remote said is interpolated into the row."""
        head = self.commit_locally()
        planegit.record_push(planegit.PushResult(planegit.FAILED, "main",
                                                 detail="remote: boom"), head=head)
        self.assertIn(str(planegit.push_record_path()), doctor.check_plane_root().hint)


class TestAnUnreachableOriginLeavesNoNoticeBehind(DoctorCase):
    """SUPERSEDES `test_an_unreachable_origin_is_not_reported_as_a_stranded_commit`.

    That test asserted `doctor` IGNORES an `unreachable` record. It passed, and the
    guarantee it was reaching for was false: the ignoring happened in `doctor` while the
    WRITING happened in `push_head`, so an `unreachable` record still landed on disk and
    overwrote a live `branched`/`stranded` one. `cmd_workspace_autosave` reaches the
    background pusher through `commit_push(no_push=True)`, which returns before
    `commit_push`'s `_origin_https` pre-check, so re-pointing origin at an unrecognised
    host was enough to turn

        warn | a memory commit was committed but never pushed, 1 ahead of origin/main

    into

        ok   | clean on main, 1 ahead of origin/main

    over a commit that existed nowhere but that laptop — measured against a real bare
    remote with a real `pre-receive` hook, not inferred.

    So the rule moved to where it can hold: `push_head` does not RECORD `unreachable` at
    all. Keeping the old test alongside would have been strictly worse than deleting it —
    a green assertion about a state production code can no longer produce, guarding the
    second of the two places whose disagreement was the defect.
    """

    def _unrecognised_origin(self) -> None:
        git(self.root, "remote", "set-url", "origin",
            "https://git.example.invalid/acme/plane.git")

    def test_it_records_nothing_at_all(self):
        head = self.commit_locally()
        self._unrecognised_origin()
        res = planegit.push_head(self.root, announce=False)
        self.assertEqual(res.outcome, planegit.UNREACHABLE)
        self.assertIsNone(planegit.push_record(), head)

    def test_it_does_not_overwrite_a_real_notice(self):
        """The finding itself. A "nothing to report" outcome must never be able to erase
        one that had something to report."""
        head = self.commit_locally()
        planegit.record_push(
            planegit.PushResult(planegit.BRANCHED, "main", landed="charter/abc1234",
                                url="https://x/compare/charter/abc1234"), head=head)
        self._unrecognised_origin()
        self.pushbg()
        rec = planegit.push_record() or {}
        self.assertEqual(rec.get("outcome"), planegit.BRANCHED)
        self.assertEqual(rec.get("landed"), "charter/abc1234")

    def test_doctor_still_says_the_commit_did_not_land(self):
        """The end the operator sees, asserted through the whole path rather than on the
        file: the row must stay WARN, not fall back to `ok | clean on main`."""
        head = self.commit_locally()
        planegit.record_push(planegit.PushResult(planegit.FAILED, "main"), head=head)
        self._unrecognised_origin()
        self.pushbg()
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("reset --hard", r.hint)


class TestAProtectedRejectionIsNotMistakenForAConflict(PlaneCase):
    """F2. Real git prints its own ``! [remote rejected]`` line above a server-side hook
    decline, so the word "rejected" in `push_head`'s non-fast-forward test matches a
    protected branch too — and taking that branch first was not merely a wasted round trip.

    Measured against a real bare remote with a real `pre-receive` hook refusing
    refs/heads/main in GH006 wording, with an origin git cannot fetch from: the fetch
    failed, which REMOVED FETCH_HEAD, so `rebase FETCH_HEAD` failed, and charter recorded
    `conflict`, printed *"remote moved"* and *"rebase hit a conflict"*, created no
    `charter/<sha>` branch, and left the commit stranded. Every one of those sentences was
    about something that had not happened — which is #373's own defect, in the code #373
    added, wearing the retry's clothes.

    So the fixture here carries the ``! [remote rejected]`` line the unit fixture at the
    top of this file deliberately omits. That omission is what kept this invisible.
    """

    #: What real git prints. The `! [remote rejected]` line is the whole point.
    REJECTED = ("remote: error: GH006: Protected branch update failed for refs/heads/main.\n"
                "remote: error: Required status check \"ci\" is expected.\n"
                "To https://github.com/acme/plane.git\n"
                " ! [remote rejected] HEAD -> main (pre-receive hook declined)\n"
                "error: failed to push some refs to 'https://github.com/acme/plane.git'")

    def fake_git_rejecting_main(self, *, fetch_ok: bool):
        """git that refuses `HEAD:main` the way a real protected remote does, and whose
        `fetch` either works or does not. Everything else is real git."""
        real = planegit._git
        seen: list[list[str]] = []

        def fake(args, cwd=None, **kw):
            seen.append(list(args))
            if "push" in args:
                if args[-1].endswith(":main"):
                    return subprocess.CompletedProcess(args, 1, "", self.REJECTED)
                return subprocess.CompletedProcess(args, 0, "", "")
            if "fetch" in args and not fetch_ok:
                return subprocess.CompletedProcess(
                    args, 128, "", "fatal: unable to access 'https://…': Could not resolve host")
            return real(args, cwd=cwd, **kw)

        self.enterContext(mock.patch.object(planegit, "_git", fake))
        self.enterContext(mock.patch.object(commands_workspace, "_git", fake))
        return seen

    def test_the_pull_request_path_is_reached_even_when_the_fetch_fails(self):
        seen = self.fake_git_rejecting_main(fetch_ok=False)
        self.pushbg()
        self.assertEqual((planegit.push_record() or {}).get("outcome"), planegit.BRANCHED)
        self.assertTrue(any(a[-1].startswith("HEAD:refs/heads/charter/")
                            for a in seen if "push" in a), seen)

    def test_no_fetch_or_rebase_is_attempted_at_all(self):
        """The round trip the author flagged, now closed by the same ordering. A protected
        branch is recognised from the FIRST push's own words; nothing about the remote
        moved, so nothing should be fetched or rebased to find that out."""
        seen = self.fake_git_rejecting_main(fetch_ok=True)
        self.pushbg()
        self.assertFalse([a for a in seen if "fetch" in a or "rebase" in a], seen)

    def test_a_failed_fetch_is_never_recorded_as_a_conflict(self):
        """A fetch that failed leaves no FETCH_HEAD, so rebasing onto it fails for a reason
        that has nothing to do with a conflict. Reported as the push failure that actually
        happened — ADR 0009: charter names a cause it recognised, never one it inferred."""
        real = planegit._git

        def fake(args, cwd=None, **kw):
            if "push" in args:
                return subprocess.CompletedProcess(
                    args, 1, "", "! [rejected] HEAD -> main (fetch first)")
            if "fetch" in args:
                return subprocess.CompletedProcess(args, 128, "", "fatal: could not read")
            return real(args, cwd=cwd, **kw)

        self.enterContext(mock.patch.object(planegit, "_git", fake))
        self.enterContext(mock.patch.object(commands_workspace, "_git", fake))
        self.pushbg()
        self.assertEqual((planegit.push_record() or {}).get("outcome"), planegit.FAILED)

    def test_a_rebase_that_really_conflicts_is_still_a_conflict(self):
        """The other direction, so the fix above cannot be "stop saying conflict"."""
        real = planegit._git

        def fake(args, cwd=None, **kw):
            if "push" in args:
                return subprocess.CompletedProcess(
                    args, 1, "", "! [rejected] HEAD -> main (fetch first)")
            if "fetch" in args:
                return subprocess.CompletedProcess(args, 0, "", "")
            if args and args[0] == "rebase" and "--abort" not in args:
                return subprocess.CompletedProcess(args, 1, "", "CONFLICT (content)")
            return real(args, cwd=cwd, **kw)

        self.enterContext(mock.patch.object(planegit, "_git", fake))
        self.enterContext(mock.patch.object(commands_workspace, "_git", fake))
        self.pushbg()
        self.assertEqual((planegit.push_record() or {}).get("outcome"), planegit.CONFLICT)

    def test_a_protected_rejection_on_the_RETRY_still_reaches_the_policy(self):
        """The first push fails plain non-fast-forward; the rebase succeeds; the second
        push is the one the protected branch refuses. Pins that moving the check earlier
        did not remove the later one."""
        real = planegit._git
        pushes = []

        def fake(args, cwd=None, **kw):
            if "push" in args:
                pushes.append(args)
                if args[-1].endswith(":main"):
                    return subprocess.CompletedProcess(
                        args, 1, "",
                        "! [rejected] HEAD -> main (fetch first)" if len(pushes) == 1
                        else self.REJECTED)
                return subprocess.CompletedProcess(args, 0, "", "")
            if "fetch" in args or (args and args[0] == "rebase"):
                return subprocess.CompletedProcess(args, 0, "", "")
            return real(args, cwd=cwd, **kw)

        self.enterContext(mock.patch.object(planegit, "_git", fake))
        self.enterContext(mock.patch.object(commands_workspace, "_git", fake))
        self.pushbg()
        self.assertEqual((planegit.push_record() or {}).get("outcome"), planegit.BRANCHED)


class TestOnePullRequestNotOnePerMemory(PlaneCase):
    """F3. Before #373 the reactive path pushed nothing, so #167's one-branch-per-rejection
    shape cost a handful of branches from a human typing `charter save`. Routing `persona
    remember`, `workspace remember`, dispatch backfill, curate and autosave through it makes
    it one abandoned remote branch PER MEMORY — measured at four branches for four memories
    against a real bare remote — each superseding the last, none referenced by anything
    charter will ever say again.

    Each new HEAD is a descendant of the one before, so advancing the recorded branch is a
    plain fast-forward. Nothing here uses `--force`: git's own refusal is the safety
    property, and a push that cannot fast-forward falls back to a fresh `charter/<sha>`.
    """

    def branch_pushes(self, seen) -> list[str]:
        return [a[-1].split("refs/heads/", 1)[1] for a in seen
                if "push" in a and a[-1].startswith("HEAD:refs/heads/charter/")]

    def test_a_second_memory_advances_the_same_branch(self):
        seen = self.fake_git(protected=True)
        self.pushbg()
        first = (planegit.push_record() or {}).get("landed")
        (self.root / "second").write_text("y\n")
        git(self.root, "add", "-A")
        git(self.root, "commit", "-qm", "memory: second")
        self.pushbg()
        self.assertEqual((planegit.push_record() or {}).get("landed"), first)
        self.assertEqual(sorted(set(self.branch_pushes(seen))), [first])

    def test_a_branch_that_cannot_fast_forward_falls_back_to_a_fresh_name(self):
        """git refuses a non-fast-forward push and charter mints a new name rather than
        forcing. Asserted as an OUTCOME, so nothing here depends on `--force` being absent
        by inspection."""
        real = planegit._git
        planegit.record_push(
            planegit.PushResult(planegit.BRANCHED, "main", landed="charter/stale00"),
            head=git(self.root, "rev-parse", "HEAD").stdout.strip())

        def fake(args, cwd=None, **kw):
            if "push" in args:
                if args[-1].endswith(":main"):
                    return subprocess.CompletedProcess(args, 1, "", _GH006)
                if args[-1].endswith("charter/stale00"):
                    return subprocess.CompletedProcess(
                        args, 1, "", "! [rejected] (non-fast-forward)")
                return subprocess.CompletedProcess(args, 0, "", "")
            return real(args, cwd=cwd, **kw)

        self.enterContext(mock.patch.object(planegit, "_git", fake))
        self.enterContext(mock.patch.object(commands_workspace, "_git", fake))
        self.pushbg()
        rec = planegit.push_record() or {}
        self.assertEqual(rec.get("outcome"), planegit.BRANCHED)
        self.assertNotEqual(rec.get("landed"), "charter/stale00")
        self.assertTrue(str(rec.get("landed")).startswith("charter/"), rec)

    def test_a_stranded_record_is_never_reused_as_a_branch(self):
        """Only `branched` names a branch that exists on the remote. `stranded` means the
        push of that branch FAILED, so reusing the name would advance nothing and would
        put a name charter never landed into the next record."""
        seen = self.fake_git(protected=True)
        planegit.record_push(
            planegit.PushResult(planegit.STRANDED, "main", landed="charter/neverwas"),
            head=git(self.root, "rev-parse", "HEAD").stdout.strip())
        self.pushbg()
        self.assertNotIn("charter/neverwas", self.branch_pushes(seen))


class TestAMergedPullRequestStartsANewBranch(DoctorCase):
    """The other half of F3's safety: `planegit.is_spent` is what stops a merged pull
    request's branch being pushed to again, and it is the SAME predicate `doctor` uses to
    stop warning. Two answers to "is this record still live" is how a branch gets
    resurrected on one surface while the other has moved on."""

    def test_a_spent_record_stops_naming_a_branch_to_reuse(self):
        head = self.commit_locally()
        planegit.record_push(
            planegit.PushResult(planegit.BRANCHED, "main", landed="charter/merged1"),
            head=head)
        self.assertEqual(planegit._open_pull_request_branch(self.root), "charter/merged1")
        git(self.root, "push", "-q", "origin", "main")      # the pull request merges
        self.assertIsNone(planegit._open_pull_request_branch(self.root))

    def test_doctor_and_the_pusher_agree_about_spent(self):
        head = self.commit_locally()
        planegit.record_push(
            planegit.PushResult(planegit.BRANCHED, "main", landed="charter/merged2"),
            head=head)
        self.assertIn("charter/merged2", doctor.check_plane_root().hint)
        git(self.root, "push", "-q", "origin", "main")
        self.assertNotIn("charter/merged2", doctor.check_plane_root().hint)
        self.assertIsNone(planegit._open_pull_request_branch(self.root))


class TestTheStatusLineSaysItOnTheNearSideOfTheHazard(DoctorCase):
    """F4. `doctor` runs at SessionStart; the hazard it names happens mid-session, in the
    SAME session that stranded the commit — an agent notices `main` is ahead for reasons it
    did not intend and reaches for `git reset --hard origin/main`. That agent never saw the
    SessionStart row. The status line renders every turn."""

    def alert(self) -> str:
        from charter import statusline
        return _plain(statusline._plane_root_alert() or "")

    def test_a_clean_plane_says_nothing(self):
        """A count of zero on every turn is furniture, which is the one thing this element
        must not become."""
        self.assertNotIn("memory", self.alert())

    def test_a_commit_that_reached_nowhere_is_named_every_turn(self):
        head = self.commit_locally()
        planegit.record_push(planegit.PushResult(planegit.FAILED, "main"), head=head)
        self.assertIn("memory commit not pushed", self.alert())

    def test_a_commit_waiting_on_a_pull_request_reads_differently(self):
        """Opposite remedies: nothing is at risk, something is unfinished. A reader told
        the wrong one acts on it."""
        head = self.commit_locally()
        planegit.record_push(
            planegit.PushResult(planegit.BRANCHED, "main", landed="charter/abc1234"),
            head=head)
        said = self.alert()
        self.assertIn("memory awaiting a pull request", said)
        self.assertNotIn("not pushed", said)

    def test_it_clears_itself_on_the_same_evidence_doctor_uses(self):
        head = self.commit_locally()
        planegit.record_push(planegit.PushResult(planegit.FAILED, "main"), head=head)
        self.assertIn("memory", self.alert())
        git(self.root, "push", "-q", "origin", "main")
        self.assertNotIn("memory", self.alert())


def _plain(s: str) -> str:
    return re.sub(r"\033\[[0-9;]*m", "", s)


if __name__ == "__main__":
    unittest.main()
