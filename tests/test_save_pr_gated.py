"""A control plane whose own repo requires pull requests (#167).

charter's guidance is that control-plane content is committed with `charter save`, straight
to the default branch. That works only where a direct push lands. On a protected `main` it
cannot — and 0.30.0's guard (#157) refuses the obvious workaround of branching the plane
root, since the root is one working tree every session shares.

The reporter was left making the same edit twice: once in the root where `version bump` put
it, once in a workspace clone they could actually branch, then discarding charter's own
copy by hand. `doctor` sat on `! plane root 1 uncommitted file(s)` throughout, pointing at
a file the tool had just written and they were not permitted to commit the normal way.

The fix keeps #157's invariant exactly: **the plane root's HEAD never moves.**
`git push HEAD:refs/heads/<new>` needs no checkout, no branch creation and no worktree —
only a different *remote* ref for a commit that already exists locally.
"""

from __future__ import annotations

import io
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

from charter import config, planegit
from tests._isolation import PersonaIso


def git(where, *args):
    return subprocess.run(["git", "-C", str(where), *args], check=True,
                          capture_output=True, text=True)


class SaveCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        self.root = Path(config.ROOT)
        subprocess.run(["git", "init", "-q", "-b", "main", str(self.root)],
                       check=True, capture_output=True)
        git(self.root, "config", "user.email", "t@e")
        git(self.root, "config", "user.name", "t")
        (self.root / "seed").write_text("x\n")
        git(self.root, "add", "-A")
        git(self.root, "-c", "commit.gpgsign=false", "commit", "-qm", "seed")
        # An origin on a forge charter recognises: without one `commit_push` warns and
        # skips the push entirely, so every assertion below would pass vacuously.
        git(self.root, "remote", "add", "origin", "https://github.com/acme/plane.git")

    def head(self) -> str:
        return git(self.root, "symbolic-ref", "--short", "HEAD").stdout.strip()

    def run_save(self, protected: bool, push_branch_ok: bool = True):
        """Drive `commit_push` with a fake git that refuses the default-branch push the way
        a protected remote does, and accepts a branch push."""
        real = planegit._git
        seen = []

        def fake(args, cwd=None, **kw):
            seen.append(list(args))
            # `push` is not args[0]: `commit_push` prefixes the credential flag, so the
            # invocation is `-c credential.helper=… push <url> <refspec>`.
            if "push" in args:
                target = args[-1]
                if protected and target.endswith(":main"):
                    return subprocess.CompletedProcess(
                        args, 1, "",
                        "remote: error: GH006: Protected branch update failed for "
                        "refs/heads/main.\nremote: error: Required status check")
                if not push_branch_ok:
                    return subprocess.CompletedProcess(args, 1, "", "remote: boom")
                return subprocess.CompletedProcess(args, 0, "", "")
            return real(args, cwd=cwd, **kw)

        planegit._git = fake
        self.addCleanup(setattr, planegit, "_git", real)
        (self.root / "personas").mkdir(exist_ok=True)
        (self.root / "personas" / "note.md").write_text("control plane content\n")

        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = planegit.commit_push(self.root, ["add", "-A"], "charter: a change")
        return rc, out.getvalue() + err.getvalue(), seen


class TestAProtectedDefaultBranchIsSupported(SaveCase):
    def test_it_pushes_a_branch_instead_of_failing(self):
        rc, out, seen = self.run_save(protected=True)
        self.assertEqual(rc, 0)
        pushed = [a for a in seen if "push" in a]
        self.assertTrue(any(p[-1].startswith("HEAD:refs/heads/charter/") for p in pushed),
                        pushed)

    def test_it_hands_back_a_url_to_open_the_pull_request(self):
        """The whole point of not needing a PR API: a compare URL is a plain HTTPS link."""
        _, out, _ = self.run_save(protected=True)
        self.assertIn("github.com/acme/plane/compare/charter/", out)

    def test_it_says_the_default_branch_required_a_pull_request(self):
        _, out, _ = self.run_save(protected=True)
        self.assertIn("requires a pull request", out)

    def test_it_says_local_main_is_ahead_and_how_to_reconcile(self):
        """The cost, stated rather than hidden — the commit stays on local main until the
        PR lands."""
        _, out, _ = self.run_save(protected=True)
        self.assertIn("pull --rebase", out)


class TestTheGuardsInvariantHolds(SaveCase):
    def test_the_plane_roots_HEAD_never_moves(self):
        """#157's invariant, asserted directly. If landing a change required checking out a
        branch in the root, this fix would be undoing the guard it has to coexist with."""
        before = self.head()
        self.run_save(protected=True)
        self.assertEqual(self.head(), before)

    def test_no_checkout_switch_or_branch_is_ever_run_in_the_root(self):
        _, _, seen = self.run_save(protected=True)
        for args in seen:
            for verb in ("checkout", "switch", "worktree", "branch"):
                self.assertNotIn(verb, args, args)


class TestItOnlyFiresOnARecognisedRefusal(SaveCase):
    def test_an_ordinary_push_failure_is_reported_not_rerouted(self):
        """ADR 0009: charter may name a cause it RECOGNISED. An auth failure is not a
        protected branch, and silently pushing a side branch for one would be inventing a
        diagnosis — and doing something surprising on the back of it."""
        real = planegit._git
        seen = []

        def fake(args, cwd=None, **kw):
            seen.append(list(args))
            if "push" in args:
                return subprocess.CompletedProcess(args, 1, "", "fatal: Authentication failed")
            return real(args, cwd=cwd, **kw)

        planegit._git = fake
        self.addCleanup(setattr, planegit, "_git", real)
        (self.root / "personas").mkdir(exist_ok=True)
        (self.root / "personas" / "n.md").write_text("x\n")
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            planegit.commit_push(self.root, ["add", "-A"], "m")
        blob = out.getvalue() + err.getvalue()
        self.assertNotIn("requires a pull request", blob)
        self.assertFalse(any(a[-1].startswith("HEAD:refs/heads/charter/")
                             for a in seen if "push" in a))

    def test_an_unprotected_plane_still_pushes_straight_to_main(self):
        rc, out, seen = self.run_save(protected=False)
        self.assertEqual(rc, 0)
        self.assertNotIn("requires a pull request", out)

    def test_a_branch_push_that_also_fails_is_an_error(self):
        """Reporting success here would be the failure mode `commit_push`'s own docstring
        records twice: a green tick over a push that never happened."""
        rc, out, _ = self.run_save(protected=True, push_branch_ok=False)
        self.assertEqual(rc, 1)


class TestCompareUrls(unittest.TestCase):
    def test_github(self):
        self.assertEqual(
            planegit._compare_url("https://github.com/acme/plane.git", "charter/abc"),
            "https://github.com/acme/plane/compare/charter/abc?expand=1")

    def test_gitlab(self):
        url = planegit._compare_url("https://gitlab.com/acme/plane.git", "charter/abc")
        self.assertIn("/-/merge_requests/new", url)
        self.assertIn("charter/abc", url)

    def test_an_unknown_scheme_yields_nothing_rather_than_a_guess(self):
        self.assertIsNone(planegit._compare_url("git@github.com:acme/plane.git", "b"))


if __name__ == "__main__":
    unittest.main()
