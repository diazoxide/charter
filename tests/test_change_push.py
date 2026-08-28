"""`charter change push` — branches, requests, and the cross-link block.

**Nothing here reaches a network.** The git half is real — a real clone of a real local bare
repository, so the push argv is genuinely executed and `origin/HEAD` is genuinely read — and
the forge half is a stand-in that records what it was asked. That split is deliberate: the
things worth pinning on this command are the argv git receives and the bytes charter puts in
a request body, and both are observable without a remote that exists on the internet.

The cross-link block is the reason PR creation is worth adding to the protocol at all: it is
the cross-repo link that survives charter being uninstalled, and it is the one artifact that
must be *maintained* rather than written once, because membership changes and five
hand-written bodies go stale the first time it does.
"""
from __future__ import annotations

import io
import os
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import change, commands_change, contain, workspace
from charter.forge import base
from tests._isolation import PersonaIso

#: The environment every `git` here runs under.
#:
#: **`GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM` are the load-bearing pair**, and they are
#: here because leaving them out HUNG this suite. The developer's own `~/.gitconfig` carries
#: ``commit.gpgsign = true`` with ``gpg.format = ssh`` and 1Password's ``op-ssh-sign`` as the
#: signer, so a bare ``git commit`` in a fixture repo inherits all three, shells out to
#: 1Password, and parks behind a biometric prompt that `subprocess` will wait out — measured
#: here as a run that stopped advancing at test 3,955 of 7,728 with an `op-ssh-sign` child
#: alive under it.
#:
#: It is #546's failure exactly, one tool over, and `util.run`'s own docstring predicted it:
#: its `GIT_TERMINAL_PROMPT=0` covers *"git's own prompts only — not a GUI credential
#: manager, and not an SSH signing agent, which is a separate way for a captured git call to
#: hang."* This is that separate way. It is also invisible in CI, where there is no global
#: config to inherit — which makes it the worst shape: green on the runner, hung on the
#: machine of whoever is signing their commits.
GIT_ENV = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@e",
           "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@e",
           "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_SYSTEM": os.devnull,
           "GIT_TERMINAL_PROMPT": "0"}


def git(*args, cwd=None):
    return subprocess.run(["git", *args], cwd=cwd, check=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                          env={**os.environ, **GIT_ENV})


class FakeForge:
    """A forge that answers from fields a test sets, and records every call.

    Not a `mock.Mock`: the interesting assertions are about *which* of these charter called
    and with what, and a Mock that auto-creates `checks_at` would let a gate that never ran
    look like one that passed.
    """

    kind = "github"
    host = "github.com"
    cli = "gh"
    change_sigil = "#"
    owner_noun = "org"

    def __init__(self, **kw):
        self.requests: dict = kw.get("requests", {})
        self.checks = kw.get("checks", base.Checks(1, base.CHECKS_PASSED))
        self.bodies: dict = kw.get("bodies", {})
        self.next_number = kw.get("next_number", 601)
        self.merge_sha = kw.get("merge_sha", "e0c9d13")
        self.merge_confirms = kw.get("merge_confirms", True)
        self.reflects_merges = kw.get("reflects_merges", True)
        self.raise_on: str | None = kw.get("raise_on")
        self.calls: list[tuple] = []

    def _maybe_raise(self, what):
        if self.raise_on == what:
            raise base.ForgeWriteError(f"{what} failed: the forge said no")

    def request_for(self, path, branch):
        self.calls.append(("request_for", path, branch))
        self._maybe_raise("request_for")
        return self.requests.get(branch)

    def checks_at(self, path, sha, number=None):
        self.calls.append(("checks_at", path, sha, number))
        return self.checks

    def change_body(self, path, number):
        self.calls.append(("change_body", path, number))
        self._maybe_raise("change_body")
        return self.bodies.get(number, "")

    def create_change(self, path, base_branch, head, title, body):
        self.calls.append(("create_change", path, base_branch, head, title, body))
        self._maybe_raise("create_change")
        n = self.next_number
        self.next_number += 1
        self.bodies[n] = body
        self.requests[head] = base.Request(n, base.REQUEST_OPEN, "4b1e77a")
        return n

    def update_change_body(self, path, number, body):
        self.calls.append(("update_change_body", path, number, body))
        self._maybe_raise("update_change_body")
        self.bodies[number] = body

    def merge_change(self, path, number, method, title, message):
        self.calls.append(("merge_change", path, number, method, title, message))
        self._maybe_raise("merge_change")
        if not self.merge_confirms:
            raise base.ForgeWriteError("the forge did not confirm a merge")
        # The forge's own state moves, so the caller's read-back sees a merged request
        # rather than the one it read before. A fake whose state did not move would let a
        # read-back that never happened look like one that confirmed.
        if self.reflects_merges:
            for branch, req in list(self.requests.items()):
                if req is not None and req.number == number:
                    self.requests[branch] = base.Request(
                        req.number, base.REQUEST_MERGED, req.head, self.merge_sha)
        return self.merge_sha


def args(**kw) -> SimpleNamespace:
    kw.setdefault("workspace", "ws")
    for k in ("change", "repo", "branch", "needs", "why", "squash", "rebase"):
        kw.setdefault(k, None)
    return SimpleNamespace(**kw)


class ChangeOnAForge(PersonaIso):
    """A workspace whose clones are real git clones of real local bare repositories."""

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("ws")
        self.remotes = Path(self.tmp) / "remotes"
        self.remotes.mkdir(parents=True, exist_ok=True)
        self.forge = FakeForge()
        self._resolve = mock.patch(
            "charter.commands_change.registry.resolve_host",
            lambda url, root: self.forge)
        self._resolve.start()
        self.addCleanup(self._resolve.stop)

    def clone(self, name: str) -> Path:
        """A real clone of a real bare repo, reachable over a `file://` URL so the origin
        parses as a namespace and the push actually runs."""
        bare = self.remotes / f"{name}.git"
        seed = self.remotes / f"{name}-seed"
        git("init", "-q", "--bare", "--initial-branch=main", str(bare))
        git("init", "-q", "--initial-branch=main", str(seed))
        (seed / "README").write_text("x\n")
        git("add", "README", cwd=seed)
        git("commit", "-qm", "seed", cwd=seed)
        git("remote", "add", "origin", f"file://{bare}", cwd=seed)
        git("push", "-q", "origin", "main", cwd=seed)
        dest = workspace.workspace_dir("ws") / name
        git("clone", "-q", f"file://{bare}", str(dest))
        return dest

    def call(self, fn, **kw):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = fn(args(**kw))
        return code, out.getvalue(), err.getvalue()

    def make(self, slug="component-api-2", members=(("api", ()),), why="API 1 -> 2"):
        for name, _needs in members:
            self.clone(name)
        code, _, err = self.call(commands_change.cmd_change_create, change=slug, why=why)
        self.assertEqual(code, 0, err)
        for name, needs in members:
            code, _, err = self.call(commands_change.cmd_change_add, change=slug,
                                     repo=name, needs=list(needs) or None)
            self.assertEqual(code, 0, err)
        return slug

    def branch_in(self, repo: str, branch: str) -> None:
        """Create the member's branch locally so there is something to push."""
        clone = workspace.workspace_dir("ws") / repo
        git("checkout", "-q", "-b", branch, cwd=clone)
        (clone / branch.replace("/", "_")).write_text("y\n")
        git("add", "-A", cwd=clone)
        git("commit", "-qm", "work", cwd=clone)


class TestTheBlockIsWrittenAndTheProseIsNot(ChangeOnAForge):
    def test_push_writes_the_block_and_leaves_prose_byte_identical(self):
        slug = self.make(members=(("api", ()),))
        self.branch_in("api", f"change/{slug}")
        # A body somebody else wrote, with charter's own markers already in it.
        self.forge.requests[f"change/{slug}"] = base.Request(601, base.REQUEST_OPEN, "s")
        above = "## Why\n\nBecause of the thing.\n"
        below = "\n## Notes\n\nDo not lose this.\n"
        self.forge.bodies[601] = (above + commands_change.BLOCK_BEGIN + "\nSTALE-CELL\n"
                                  + commands_change.BLOCK_END + below)
        code, out, err = self.call(commands_change.cmd_change_push, change=slug)
        self.assertEqual(code, 0, err)
        body = self.forge.bodies[601]
        self.assertTrue(body.startswith(above), repr(body[:80]))
        self.assertTrue(body.endswith(below.rstrip("\n")), repr(body[-80:]))
        self.assertIn("| api |", body)
        self.assertNotIn("STALE-CELL", body)

    def test_the_block_names_every_member_and_its_request(self):
        slug = self.make(members=(("api", ()), ("web", ("api",))))
        for r in ("api", "web"):
            self.branch_in(r, f"change/{slug}")
        code, out, err = self.call(commands_change.cmd_change_push, change=slug)
        self.assertEqual(code, 0, err)
        body = self.forge.bodies[self.forge.next_number - 1]
        self.assertIn("| api | ", body)
        self.assertIn("| web | ", body)
        self.assertIn("| api |\n", body + "\n")   # web's `needs` cell names api

    def test_a_member_with_no_request_renders_as_a_dash_not_as_a_missing_row(self):
        """A table that omits a member says the change has fewer members than it has."""
        rec = {"change": "c", "why": "w", "created": "t", "by": "b",
               "members": [{"repo": "api", "branch": "b1", "needs": []},
                           {"repo": "web", "branch": "b2", "needs": []}],
               "excluded": []}
        block = commands_change.cross_link_block(rec, {"api": (601, "acme/api", "#")})
        self.assertIn("| web | — |", block)
        self.assertIn("acme/api#601", block)

    def test_the_sigil_is_the_members_own_forge_not_the_planes(self):
        """A workspace can hold clones from several forges side by side, so a GitLab member
        renders `!14` on the row above a GitHub member\'s `#601`."""
        rec = {"change": "c", "why": "w", "created": "t", "by": "b",
               "members": [{"repo": "api", "branch": "b1", "needs": []},
                           {"repo": "web", "branch": "b2", "needs": []}],
               "excluded": []}
        block = commands_change.cross_link_block(
            rec, {"api": (601, "acme/api", "#"), "web": (14, "g/web", "!")})
        self.assertIn("acme/api#601", block)
        self.assertIn("g/web!14", block)


class TestShowsDerivedColumns(ChangeOnAForge):
    """§3.4 calls `charter change show` the monorepo view, and being a VIEW is what makes
    it correct: nothing here is written back, so nothing on disk can disagree with git."""

    def test_a_member_charter_can_reach_gets_its_request_and_its_check_state(self):
        slug = self.make(members=(("api", ()),))
        self.forge.requests[f"change/{slug}"] = base.Request(601, base.REQUEST_OPEN, "9f3a1c2")
        self.forge.checks = base.Checks(0, base.CHECKS_NOT_RUN)
        rows = commands_change.show_observed("ws", change.read("ws", slug))
        self.assertTrue(any("NOT RUN" in r for r in rows), rows)
        self.assertTrue(any("#601" in r for r in rows), rows)

    def test_a_workspace_whose_clones_resolve_to_no_forge_prints_nothing_at_all(self):
        """An honest silence rather than a block of `unknown` rows that look like an
        answer. This is what keeps `show` printing exactly the record on a plane charter
        cannot ask about — and it is the reason the record half of the command is still
        true without asking anybody."""
        slug = self.make(members=(("api", ()),))
        self._resolve.stop()
        self.addCleanup(self._resolve.start)
        with mock.patch("charter.commands_change.registry.resolve_host",
                        lambda url, root: None):
            self.assertEqual(commands_change.show_observed("ws", change.read("ws", slug)), [])

    def test_a_forge_that_will_not_answer_is_unknown_and_is_not_silence(self):
        """A different sentence from the case above, and never green: charter reached a
        forge and it would not say."""
        slug = self.make(members=(("api", ()),))
        self.forge.raise_on = "request_for"
        rows = commands_change.show_observed("ws", change.read("ws", slug))
        self.assertTrue(any("UNKNOWN" in r for r in rows), rows)

    def test_nothing_derived_reaches_the_record(self):
        slug = self.make(members=(("api", ()),))
        self.forge.requests[f"change/{slug}"] = base.Request(601, base.REQUEST_OPEN, "9f3a1c2")
        before = change.path_for("ws", slug).read_bytes()
        commands_change.show_observed("ws", change.read("ws", slug))
        self.assertEqual(change.path_for("ws", slug).read_bytes(), before)


class TestTheMarkersAreARefusalNotAFallback(unittest.TestCase):
    """`render.splice_personas` answers None when the markers are absent *"so a hand-written
    README is never appended to by surprise"*. A request body is the same thing with a wider
    audience, so the posture is the same: charter owns what is between its delimiters, and
    every other reading is charter editing a human's words."""

    BLOCK = "BLOCKTEXT"

    def splice(self, body):
        return commands_change.splice_block(body, self.BLOCK)

    def test_no_markers_at_all_is_refused(self):
        self.assertIsNone(self.splice("Just some prose about the change.\n"))

    def test_only_a_begin_marker_is_refused(self):
        self.assertIsNone(self.splice("a\n" + commands_change.BLOCK_BEGIN + "\nb\n"))

    def test_only_an_end_marker_is_refused(self):
        self.assertIsNone(self.splice("a\n" + commands_change.BLOCK_END + "\nb\n"))

    def test_markers_in_the_wrong_order_are_refused(self):
        self.assertIsNone(self.splice(commands_change.BLOCK_END + "\nx\n"
                                      + commands_change.BLOCK_BEGIN + "\n"))

    def test_a_doubled_begin_marker_is_refused(self):
        self.assertIsNone(self.splice(commands_change.BLOCK_BEGIN + "\n"
                                      + commands_change.BLOCK_BEGIN + "\nx\n"
                                      + commands_change.BLOCK_END + "\n"))

    def test_markers_inside_a_fenced_code_block_are_refused(self):
        """Somebody documenting the block in their own body. Splicing there rewrites an
        example, and the marker counts look perfectly balanced."""
        body = ("Here is what charter writes:\n\n```\n"
                + commands_change.BLOCK_BEGIN + "\n…\n" + commands_change.BLOCK_END
                + "\n```\n")
        self.assertIsNone(self.splice(body))

    def test_a_tilde_fence_counts_too(self):
        body = ("~~~\n" + commands_change.BLOCK_BEGIN + "\n…\n"
                + commands_change.BLOCK_END + "\n~~~\n")
        self.assertIsNone(self.splice(body))

    def test_a_balanced_pair_outside_a_fence_splices(self):
        body = ("top\n" + commands_change.BLOCK_BEGIN + "\nstale\n"
                + commands_change.BLOCK_END + "\nbottom\n")
        self.assertEqual(self.splice(body), "top\nBLOCKTEXT\nbottom")


class TestPushRefusals(ChangeOnAForge):
    def test_a_body_with_no_markers_is_named_and_not_edited(self):
        slug = self.make(members=(("api", ()),))
        self.branch_in("api", f"change/{slug}")
        self.forge.requests[f"change/{slug}"] = base.Request(601, base.REQUEST_OPEN, "s")
        self.forge.bodies[601] = "somebody's prose\n"
        code, out, err = self.call(commands_change.cmd_change_push, change=slug)
        self.assertEqual(code, 1)
        self.assertIn("charter block", err)
        self.assertEqual(self.forge.bodies[601], "somebody's prose\n")
        self.assertNotIn(("update_change_body", "", 601, ""),
                         [c[:1] + ("", c[2] if len(c) > 2 else "", "")
                          for c in self.forge.calls])

    def test_a_member_with_no_clone_is_named_and_the_others_still_push(self):
        slug = self.make(members=(("api", ()),))
        self.branch_in("api", f"change/{slug}")
        rec = change.read("ws", slug)
        rec["members"].append({"repo": "gone", "branch": "b", "needs": []})
        change.write("ws", slug, rec)
        code, out, err = self.call(commands_change.cmd_change_push, change=slug)
        self.assertEqual(code, 1)
        self.assertIn("no clone", err)
        self.assertIn("charter clone gone", err)
        self.assertIn("#601", out)            # api still went

    def test_a_change_with_no_members_is_refused_before_anything_runs(self):
        slug = self.make(members=())
        code, out, err = self.call(commands_change.cmd_change_push, change=slug)
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("no members", err)
        self.assertEqual(self.forge.calls, [])

    def test_a_push_that_git_refuses_is_reported_in_gits_own_words(self):
        """`planegit` declines to predict a protected branch because *"guessing it from the
        branch name is precisely the unearned diagnosis ADR 0009 forbids"*, and the same
        applies to every other reason a push can fail."""
        slug = self.make(members=(("api", ()),))
        # no branch created, so the push has nothing to push
        code, out, err = self.call(commands_change.cmd_change_push, change=slug)
        self.assertEqual(code, 1)
        self.assertIn("push failed", err)


class TestTheDestinationIsLocal(ChangeOnAForge):
    """§6.1. Membership is committed; destination is local. A record may say WHICH
    repositories are members and never WHERE they are."""

    def test_the_base_is_the_clones_own_default_branch(self):
        slug = self.make(members=(("api", ()),))
        self.branch_in("api", f"change/{slug}")
        code, out, err = self.call(commands_change.cmd_change_push, change=slug)
        self.assertEqual(code, 0, err)
        created = [c for c in self.forge.calls if c[0] == "create_change"]
        self.assertEqual(len(created), 1)
        self.assertEqual(created[0][2], "main")     # from origin/HEAD, not from the record

    def test_no_record_key_can_influence_the_base(self):
        """Written as a property over the record's whole key set rather than as a guess at
        which key somebody might add: the base is read from git, so no value in the record
        can appear in it."""
        slug = self.make(members=(("api", ()),))
        self.branch_in("api", f"change/{slug}")
        rec = change.read("ws", slug)
        rec["why"] = "trunk"
        rec["members"][0]["branch"] = f"change/{slug}"
        change.write("ws", slug, rec)
        self.call(commands_change.cmd_change_push, change=slug)
        created = [c for c in self.forge.calls if c[0] == "create_change"][0]
        self.assertEqual(created[2], "main")

    def test_the_record_carries_no_destination_key_at_all(self):
        for forbidden in ("url", "remote", "host", "forge", "base", "origin"):
            self.assertNotIn(forbidden, change.KEYS)
            self.assertNotIn(forbidden, change.MEMBER_KEYS)


class TestABranchNameIsArgvNotARef(unittest.TestCase):
    """`git check-ref-format` **accepts** `refs/heads/-b` — a leading dash is legal inside a
    ref, measured against git 2.50.1. So the record boundary refusing one is not enough on
    its own, and neither is the argv position: this phase ships both, because either alone
    has already been enough to ship a bug in this repository."""

    def test_the_branch_is_placed_after_a_double_dash(self):
        """The argv is spelled at its call site so the git verb stays a literal for the
        static checks in `test_commands_change.py`, which pin it whole. What this adds is
        the behavioural half: the branch really does arrive after the separator, measured
        by pushing a branch actually named `-b`."""
        import ast as _ast
        src = Path(commands_change.__file__).read_text(encoding="utf-8")
        pushes = [[a.value if isinstance(a, _ast.Constant) else "<var>" for a in n.args[1:]]
                  for n in _ast.walk(_ast.parse(src))
                  if isinstance(n, _ast.Call) and isinstance(n.func, _ast.Name)
                  and n.func.id == "_git" and len(n.args) >= 2
                  and isinstance(n.args[1], _ast.Constant) and n.args[1].value == "push"]
        self.assertEqual(pushes, [["push", "--set-upstream", "origin", "--", "<var>"]])

    def test_git_really_treats_it_as_an_operand(self):
        """Measured, not reasoned about. `git push origin -- -b` must fail complaining about
        a REFSPEC, not about an unknown option."""
        p = subprocess.run(["git", "push", "origin", "--", "-b"], cwd=os.getcwd(),
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                           env={**os.environ, "GIT_TERMINAL_PROMPT": "0",
                                "GIT_CONFIG_GLOBAL": "/dev/null"})
        self.assertNotIn("unknown option", (p.stderr or "").lower())

    def test_check_ref_format_would_have_passed_this(self):
        """The measurement behind the rule. If ref grammar were the guard, `-b` walks
        through it."""
        p = subprocess.run(["git", "check-ref-format", "refs/heads/-b"],
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.assertEqual(p.returncode, 0,
                         "git no longer accepts refs/heads/-b — re-measure the rule")

    def test_the_record_boundary_refuses_it_too(self):
        self.assertIsNotNone(change.branch_refusal("-b"))


class TestContainmentOnTheWayOut(unittest.TestCase):
    """§4e: *a change's name and description are untrusted committed values*. They reach a
    markdown table here, where a newline forges a row and a pipe forges a column."""

    def _block(self, **over):
        rec = {"change": over.get("slug", "c"), "why": over.get("why", "w"),
               "created": "t", "by": "b",
               "members": [{"repo": over.get("repo", "api"),
                            "branch": over.get("branch", "b1"), "needs": []}],
               "excluded": []}
        return commands_change.cross_link_block(rec, {"api": (1, "acme/api", "#")})

    def test_a_why_with_a_newline_renders_as_one_row(self):
        block = self._block(why="line one\n| forged | row | here |")
        rows = [ln for ln in block.splitlines() if ln.startswith("|")]
        self.assertEqual(len(rows), 3)     # header, separator, one member

    def test_a_why_with_a_pipe_closes_no_column(self):
        block = self._block(why="a | b")
        self.assertIn("a \\| b", block)

    def test_u2028_in_a_repo_name_does_not_forge_a_row(self):
        block = self._block(repo="api | forged |")
        rows = [ln for ln in block.splitlines() if ln.startswith("|")]
        self.assertEqual(len(rows), 3)
        self.assertIn("\\u2028", block)

    def test_a_branch_name_is_contained_too(self):
        """The branch field is the one that ALSO crosses into argv, so it needs both
        treatments and neither substitutes for the other."""
        rec = {"change": "c", "why": "w", "created": "t", "by": "b",
               "members": [{"repo": "api", "branch": "b | x |", "needs": ["n\nx"]}],
               "excluded": []}
        block = commands_change.cross_link_block(rec, {})
        rows = [ln for ln in block.splitlines() if ln.startswith("|")]
        self.assertEqual(len(rows), 3)

    def test_a_backtick_in_the_slug_does_not_break_out_of_the_code_span(self):
        block = self._block(slug="c` **loud** `")
        self.assertEqual(len([ln for ln in block.splitlines() if ln.startswith("|")]), 3)

    def test_the_markers_are_exactly_these_bytes(self):
        """**Pinned as literals**, and the deletion sweep is why: every other case in this
        file spells the markers as `commands_change.BLOCK_BEGIN`, so retuning the constant
        moved the test's own expectation with it and the mutation survived four modules and
        the full 7,722.

        It is not a formality. These strings are written into other people\'s request
        bodies and charter finds its own block by matching them back. Change either and
        every block already published is orphaned: `splice_block` stops finding it, refuses
        the body as unmarked, and the cross-link nobody can now maintain sits there going
        stale. That is a compatibility contract, and a contract needs a literal.
        """
        self.assertEqual(
            commands_change.BLOCK_BEGIN,
            "<!-- BEGIN charter change \u2014 GENERATED by `charter change push`; "
            "do not edit by hand. -->")
        self.assertEqual(commands_change.BLOCK_END, "<!-- END charter change -->")

    def test_the_markers_are_html_comments_so_a_rendered_body_does_not_show_them(self):
        """`render.PERSONAS_BEGIN`\'s shape rather than `workspace._LIVE_BEGIN`\'s `# >>>`:
        a request body is markdown and has a renderer, a .gitignore does not."""
        for marker in (commands_change.BLOCK_BEGIN, commands_change.BLOCK_END):
            with self.subTest(marker=marker[:20]):
                self.assertTrue(marker.startswith("<!--"))
                self.assertTrue(marker.endswith("-->"))
        self.assertNotEqual(commands_change.BLOCK_BEGIN, commands_change.BLOCK_END)


if __name__ == "__main__":
    unittest.main()
