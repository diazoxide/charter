"""The forge learns to write, **loudly**.

ADR 0002 recorded that the reporting module was *"the only place in charter that writes to
a forge"* and called that concentration deliberate, *"a single seam, which is what makes
the feature testable without touching the network"*. Phase 4 adds a second seam and the
concentration argument survives in spirit — both are narrow, both are stdlib subprocess
calls to the forge's own CLI, both are testable without a network — but the sentence as
written becomes false, so the ADR is amended rather than left quietly wrong.

The extension comes with one hard requirement, stated by that ADR itself: **a write needs a
loud failure path.** `_api`'s "return None on any failure" is right for the status line and
catastrophic here — it means a pull request that was never opened, a cross-link block that
was never written, or a merge that never happened, each reported as a success.
"""
from __future__ import annotations

import ast
import json
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter.forge import base
from charter.forge.github import GitHubForge
from charter.forge.gitlab import GitLabForge


def _proc(stdout="", rc=0, stderr=""):
    return SimpleNamespace(stdout=stdout, stderr=stderr, returncode=rc)


class _Recorder:
    def __init__(self, payload=None, rc=0, stderr=""):
        self.payload, self.rc, self.stderr = payload, rc, stderr
        self.calls: list[list[str]] = []

    def __call__(self, cmd, **kw):
        self.calls.append(list(cmd))
        if self.rc:
            return _proc(rc=self.rc, stderr=self.stderr)
        return _proc(stdout=json.dumps(self.payload) if self.payload is not None else "")

    @property
    def argv(self):
        return self.calls[-1]


def _gh(payload=None, rc=0, stderr=""):
    r = _Recorder(payload, rc, stderr)
    return mock.patch("charter.forge.github.util.run", r), r


def _gl(payload=None, rc=0, stderr=""):
    r = _Recorder(payload, rc, stderr)
    return mock.patch("charter.forge.gitlab.util.run", r), r


class TestCreateReturnsANumber(unittest.TestCase):
    def test_github_create_returns_the_pull_request_number(self):
        patch, rec = _gh({"number": 601})
        with patch:
            n = GitHubForge().create_change("acme/api", "main", "change/x", "t", "b")
        self.assertEqual(n, 601)
        self.assertIn("POST", rec.argv)

    def test_gitlab_create_returns_the_iid_not_the_id(self):
        """A GitLab merge request has both, and only the iid is the one a human types."""
        patch, rec = _gl({"iid": 14, "id": 900123})
        with patch:
            n = GitLabForge().create_change("g/p", "main", "change/x", "t", "b")
        self.assertEqual(n, 14)

    def test_update_body_uses_the_right_verb(self):
        patch, rec = _gh({"number": 601})
        with patch:
            GitHubForge().update_change_body("acme/api", 601, "new body")
        self.assertIn("PATCH", rec.argv)
        self.assertIn("-f", rec.argv)
        self.assertIn("body=new body", rec.argv)


class TestAFailedWriteRaisesAndNeverReturnsNone(unittest.TestCase):
    """The whole point of the third discipline. Each of these returns ``None`` through
    `_api`, and a caller that believed it would report a success that did not happen."""

    def test_github_create_raises(self):
        patch, _ = _gh(rc=1, stderr="gh: Validation Failed (HTTP 422)")
        with patch, self.assertRaises(base.ForgeWriteError) as cm:
            GitHubForge().create_change("acme/api", "main", "change/x", "t", "b")
        self.assertIn("422", str(cm.exception))     # the forge's own words, not a re-diagnosis

    def test_gitlab_create_raises(self):
        patch, _ = _gl(rc=1, stderr="glab: 403 Forbidden")
        with patch, self.assertRaises(base.ForgeWriteError):
            GitLabForge().create_change("g/p", "main", "change/x", "t", "b")

    def test_update_body_raises(self):
        patch, _ = _gh(rc=1, stderr="nope")
        with patch, self.assertRaises(base.ForgeWriteError):
            GitHubForge().update_change_body("acme/api", 601, "b")

    def test_a_reply_with_no_number_raises_rather_than_returning_none(self):
        """A 200 with an unexpected body is still a write charter cannot confirm."""
        patch, _ = _gh({"message": "ok"})
        with patch, self.assertRaises(base.ForgeWriteError):
            GitHubForge().create_change("acme/api", "main", "change/x", "t", "b")

    def test_malformed_json_from_a_write_raises(self):
        with mock.patch("charter.forge.github.util.run",
                        lambda cmd, **kw: _proc(stdout="{not json")):
            with self.assertRaises(base.ForgeWriteError):
                GitHubForge().create_change("acme/api", "main", "change/x", "t", "b")

    def test_a_write_timeout_raises(self):
        from charter import util

        def boom(cmd, **kw):
            raise util.ProcTimeout(list(cmd), 60.0)

        with mock.patch("charter.forge.github.util.run", boom):
            with self.assertRaises(base.ForgeWriteError):
                GitHubForge().create_change("acme/api", "main", "change/x", "t", "b")

    def test_the_write_error_is_a_forge_error_so_old_handlers_still_catch_it(self):
        self.assertTrue(issubclass(base.ForgeWriteError, base.ForgeError))


class TestMergeIsConfirmedNotAssumed(unittest.TestCase):
    def test_github_merge_returns_the_sha(self):
        patch, rec = _gh({"merged": True, "sha": "e0c9d13"})
        with patch:
            sha = GitHubForge().merge_change("acme/api", 601, "merge", "t", "m")
        self.assertEqual(sha, "e0c9d13")
        self.assertIn("PUT", rec.argv)
        self.assertIn("merge_method=merge", rec.argv)

    def test_github_squash_asks_for_a_squash(self):
        patch, rec = _gh({"merged": True, "sha": "5qua5h0"})
        with patch:
            GitHubForge().merge_change("acme/api", 601, "squash", "t", "m")
        self.assertIn("merge_method=squash", rec.argv)

    def test_the_trailer_reaches_the_commit_message_field(self):
        patch, rec = _gh({"merged": True, "sha": "e0c9d13"})
        with patch:
            GitHubForge().merge_change("acme/api", 601, "merge", "title",
                                       "why\n\nCharter-Change: component-api-2")
        self.assertIn("commit_message=why\n\nCharter-Change: component-api-2", rec.argv)

    def test_a_200_that_did_not_merge_raises(self):
        """GitHub answers 405 with `merged: false` for a request that is not mergeable, and
        a caller that read only the exit code would record a landing that did not happen."""
        patch, _ = _gh({"merged": False, "message": "Pull Request is not mergeable"})
        with patch, self.assertRaises(base.ForgeWriteError) as cm:
            GitHubForge().merge_change("acme/api", 601, "merge", "t", "m")
        self.assertIn("not mergeable", str(cm.exception))

    def test_a_write_that_answers_with_an_empty_body_raises(self):
        """`_write` returns ``None`` for a 200 with nothing in it, and each caller turns
        that into a refusal rather than a `None` sha or a `None` number. The deletion sweep
        found these three unpinned: a guard that is real and reachable, with no test."""
        for forge, patcher, call, args in (
            (GitHubForge(), _gh, "merge_change", ("acme/api", 601, "merge", "t", "m")),
            (GitHubForge(), _gh, "create_change", ("acme/api", "main", "b", "t", "b")),
            (GitLabForge(), _gl, "merge_change", ("g/p", 14, "merge", "t", "m")),
            (GitLabForge(), _gl, "create_change", ("g/p", "main", "b", "t", "b")),
        ):
            patch, _ = patcher(None)          # a 200 with an empty body
            with self.subTest(forge=forge.kind, call=call):
                with patch, self.assertRaises(base.ForgeWriteError):
                    getattr(forge, call)(*args)

    def test_a_merge_reply_that_is_not_an_object_raises(self):
        """A 200 whose body is a JSON array, or a bare string. `data.get` would be an
        `AttributeError` out of a write path documented to raise `ForgeWriteError`."""
        for payload in ([], "surprise", 7):
            with self.subTest(payload=payload):
                patch, _ = _gh(payload)
                with patch, self.assertRaises(base.ForgeWriteError):
                    GitHubForge().merge_change("acme/api", 601, "merge", "t", "m")

    def test_gitlab_names_the_state_it_saw_even_when_there_is_none(self):
        """The refusal quotes the state so a reader knows what the forge said; a reply with
        no `state` at all must still produce a sentence rather than a `None` in it."""
        patch, _ = _gl({"merge_commit_sha": "e0c9d13"})
        with patch, self.assertRaises(base.ForgeWriteError) as cm:
            GitLabForge().merge_change("g/p", 14, "merge", "t", "m")
        self.assertIn("unknown", str(cm.exception))
        self.assertNotIn("None", str(cm.exception))

    def test_gitlab_merge_requires_the_state_to_say_merged(self):
        patch, _ = _gl({"state": "opened", "merge_commit_sha": None})
        with patch, self.assertRaises(base.ForgeWriteError):
            GitLabForge().merge_change("g/p", 14, "merge", "t", "m")

    def test_gitlab_merge_returns_the_merge_commit(self):
        patch, _ = _gl({"state": "merged", "merge_commit_sha": "e0c9d13"})
        with patch:
            self.assertEqual(
                GitLabForge().merge_change("g/p", 14, "merge", "t", "m"), "e0c9d13")

    def test_gitlab_squash_asks_for_a_squash(self):
        patch, rec = _gl({"state": "merged", "squash_commit_sha": "5qua5h0"})
        with patch:
            GitLabForge().merge_change("g/p", 14, "squash", "t", "m")
        self.assertIn("squash=true", rec.argv)

    def test_rebase_is_not_in_the_permitted_set(self):
        """Charter constraining its OWN act, not the repository's policy: a rebase landing
        replays the author's commits, so there is no commit to carry the trailer and no
        single sha to revert."""
        self.assertEqual(base.MERGE_METHODS, ("merge", "squash"))
        self.assertNotIn("rebase", base.MERGE_METHODS)


class TestDashFNeverDashCapitalF(unittest.TestCase):
    """#323. `-F/--field` gives an `@`-prefixed value file-read semantics — from `gh api
    --help`, *"if the value starts with @, the rest of the value is interpreted as a
    filename to read the value from"* — which turned a status refresh into an arbitrary
    local file read by a process holding the forge token.

    It applies HARDER on a write than on the read it was found in: a change's title and body
    carry the `why` and the member names, all committed values from someone else's machine.
    """

    def _argv_for_every_write(self):
        out = []
        for forge, patcher, calls in (
            (GitHubForge(), _gh, [
                ("create_change", ("acme/api", "main", "change/x", "t", "b"), {"number": 1}),
                ("update_change_body", ("acme/api", 1, "b"), {"number": 1}),
                ("merge_change", ("acme/api", 1, "merge", "t", "m"),
                 {"merged": True, "sha": "s"}),
            ]),
            (GitLabForge(), _gl, [
                ("create_change", ("g/p", "main", "change/x", "t", "b"), {"iid": 1}),
                ("update_change_body", ("g/p", 1, "b"), {"iid": 1}),
                ("merge_change", ("g/p", 1, "merge", "t", "m"),
                 {"state": "merged", "merge_commit_sha": "s"}),
            ]),
        ):
            for name, argtuple, payload in calls:
                patch, rec = patcher(payload)
                with patch:
                    getattr(forge, name)(*argtuple)
                out.append((forge.kind, name, rec.argv))
        return out

    def test_no_write_ever_passes_dash_capital_f(self):
        for kind, name, argv in self._argv_for_every_write():
            with self.subTest(forge=kind, call=name):
                self.assertNotIn("-F", argv)
                self.assertNotIn("--field", argv)

    def test_every_field_a_write_sends_goes_through_dash_f(self):
        for kind, name, argv in self._argv_for_every_write():
            with self.subTest(forge=kind, call=name):
                # every `key=value` token is preceded by `-f`
                for i, tok in enumerate(argv):
                    if "=" in tok and not tok.startswith("-") and i:
                        self.assertEqual(argv[i - 1], "-f", argv)

    def test_a_body_starting_with_an_at_sign_is_sent_as_a_literal(self):
        """The exact shape #323 is about. `@/etc/passwd` as a body must reach the forge as
        those characters, not as a file read by the process holding the token."""
        patch, rec = _gh({"number": 1})
        with patch:
            GitHubForge().create_change("acme/api", "main", "b", "t", "@/etc/passwd")
        self.assertIn("body=@/etc/passwd", rec.argv)
        self.assertEqual(rec.argv[rec.argv.index("body=@/etc/passwd") - 1], "-f")


class TestAWriteNeverRoutesThroughApi(unittest.TestCase):
    """Asserted against the syntax tree, because the property is *absence*: a write that
    called `_api` would swallow its failure and every behavioural test above would still
    pass on the happy path."""

    def _writes(self, module):
        src = Path(module.__file__).read_text(encoding="utf-8")
        tree = ast.parse(src)
        names = ("create_change", "update_change_body", "merge_change")
        return [n for n in ast.walk(tree)
                if isinstance(n, ast.FunctionDef) and n.name in names]

    def test_no_write_body_calls_api(self):
        from charter.forge import github, gitlab
        for module in (github, gitlab):
            for fn in self._writes(module):
                called = {c.func.attr for c in ast.walk(fn)
                          if isinstance(c, ast.Call) and isinstance(c.func, ast.Attribute)}
                with self.subTest(module=module.__name__, fn=fn.name):
                    self.assertNotIn("_api", called)
                    self.assertIn("_write", called)

    def test_there_are_three_writes_and_no_more(self):
        """A fourth write added without this file noticing is a fourth chance to swallow a
        failure. The count is a literal, not a length taken off the thing under test."""
        from charter.forge import github, gitlab
        for module in (github, gitlab):
            with self.subTest(module=module.__name__):
                self.assertEqual(len(self._writes(module)), 3)


if __name__ == "__main__":
    unittest.main()
