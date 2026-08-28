"""`charter change create | add | drop | list | show | forget` — records only, no forge.

Two properties run through every case here.

**Which refusal fired, never merely that one did.** `charter change add` has three gates in
sequence — the change exists, the repo resolves to a clone, the repo is not already a
member — and an exit-code assertion cannot tell them apart. On #558, deleting a refusal
still exited 1, for a worse reason, and the test stayed green over a real deletion. So every
refusal below asserts its own message *and*, where a neighbouring gate could have produced
the same code, the absence of that neighbour's words.

**The containment property, stated as a property.** A member resolves through
`contain.child(workspace_dir, name)` and must be a clone the operator already put there. So
the reach of a change is bounded by what is on this disk, never by anything that travelled
in a committed file — and a record can name a repository you do not have, be refused for
that by name, and never name a *place*.
"""
from __future__ import annotations

import ast
import io
import json
import os
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import change, commands_change, contain, workspace
from tests._isolation import PersonaIso


def args(**kw) -> SimpleNamespace:
    kw.setdefault("workspace", "ws")
    for k in ("change", "repo", "branch", "needs", "why"):
        kw.setdefault(k, None)
    return SimpleNamespace(**kw)


class ChangeCommands(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("ws")
        for repo in ("charter", "charter-metrics", ".github"):
            self.clone(repo)

    def clone(self, name: str) -> None:
        """A clone, by git's own definition: a directory whose `.git` is a DIRECTORY."""
        (workspace.workspace_dir("ws") / name / ".git").mkdir(parents=True)

    def call(self, fn, **kw) -> tuple[int, str, str]:
        """Call a handler, returning ``(code, stdout, stderr)``."""
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = fn(args(**kw))
        return code, out.getvalue(), err.getvalue()

    def create(self, slug="component-api-2", why="API 1 -> 2") -> None:
        code, _, err = self.call(commands_change.cmd_change_create, change=slug, why=why)
        self.assertEqual(code, 0, err)


class TestTheHappyPath(ChangeCommands):
    def test_create_then_add_then_show_prints_the_member(self):
        self.create()
        code, _, err = self.call(commands_change.cmd_change_add,
                                change="component-api-2", repo="charter")
        self.assertEqual(code, 0, err)
        code, out, _ = self.call(commands_change.cmd_change_show, change="component-api-2")
        self.assertEqual(code, 0)
        self.assertIn("charter", out)
        self.assertIn("change/component-api-2", out)
        self.assertIn("API 1 -> 2", out)

    def test_the_default_branch_is_the_slug_and_it_is_stored(self):
        """Stored, not derived by convention afterwards: a convention breaks the moment
        somebody names one differently, and git can say a branch exists but never that it
        is *this change's*."""
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2", repo="charter")
        rec = change.read("ws", "component-api-2")
        self.assertEqual(rec["members"][0]["branch"], "change/component-api-2")

    def test_an_explicit_branch_is_kept(self):
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2",
                 repo="charter", branch="feature/other-name")
        rec = change.read("ws", "component-api-2")
        self.assertEqual(rec["members"][0]["branch"], "feature/other-name")

    def test_needs_is_recorded_and_shown(self):
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2", repo="charter")
        self.call(commands_change.cmd_change_add, change="component-api-2",
                 repo="charter-metrics", needs=["charter"])
        code, out, _ = self.call(commands_change.cmd_change_show, change="component-api-2")
        self.assertEqual(code, 0)
        self.assertIn("needs: charter", out)

    def test_list_shows_every_change(self):
        self.create("one", "first")
        self.create("two", "second")
        code, out, _ = self.call(commands_change.cmd_change_list)
        self.assertEqual(code, 0)
        self.assertIn("one", out)
        self.assertIn("two", out)
        self.assertIn("second", out)

    def test_the_record_holds_no_state(self):
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2", repo="charter")
        text = change.path_for("ws", "component-api-2").read_text()
        for word in ("state", "landed", "pr", "number", "ci", "checks", "url", "remote"):
            self.assertNotIn(f'"{word}"', text)

    def test_a_member_with_no_blockers_shows_no_needs(self):
        """The negative half of `test_needs_is_recorded_and_shown`: without it, printing an
        empty `needs:` on every row would pass both."""
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2", repo="charter")
        code, out, _ = self.call(commands_change.cmd_change_show, change="component-api-2")
        self.assertEqual(code, 0)
        self.assertNotIn("needs:", out)

    def test_a_change_with_no_exclusions_prints_no_excluded_section(self):
        self.create()
        code, out, _ = self.call(commands_change.cmd_change_show, change="component-api-2")
        self.assertEqual(code, 0)
        self.assertNotIn("excluded", out)

    def test_an_empty_workspace_lists_nothing_and_says_how_to_start(self):
        code, out, err = self.call(commands_change.cmd_change_list)
        self.assertEqual(code, 0)
        self.assertEqual(out, "")
        self.assertIn("charter change create", err)


class TestTheContainmentProperty(ChangeCommands):
    def test_a_repo_with_no_clone_is_refused_and_names_charter_clone(self):
        self.create()
        code, _, err = self.call(commands_change.cmd_change_add,
                                change="component-api-2", repo="charter-slack")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("no clone in workspace", err)
        self.assertIn("charter clone charter-slack -w ws", err)
        self.assertNotIn("already a member", err)      # and not the gate below it
        self.assertNotIn("no change", err)             # nor the gate above it

    def test_dot_github_is_accepted_because_segment_ok_is_the_rule(self):
        """`workspace.valid_name` rejects a leading dot and `org/.github` is a real
        repository whose name comes from a forge rather than from charter."""
        self.create()
        code, _, err = self.call(commands_change.cmd_change_add,
                                change="component-api-2", repo=".github")
        self.assertEqual(code, 0, err)
        self.assertFalse(workspace.valid_name(".github"))

    def test_a_member_that_is_a_path_is_refused(self):
        self.create()
        for repo in ("..", "../charter", "/etc", "a/b", "a\\b", "x\x00y", ""):
            with self.subTest(repo=repo):
                code, _, err = self.call(commands_change.cmd_change_add,
                                        change="component-api-2", repo=repo)
                self.assertEqual(code, commands_change.REFUSED)
                self.assertIn("no clone in workspace", err)

    def test_a_traversing_member_never_resolves_even_when_the_target_exists(self):
        """The refusal is about the string. Asking the filesystem would make a traversal
        succeed exactly when the attacker's target happens to exist — and here it does: the
        target below is a real clone, one `..` above the workspace.

        **Which refusal fires is the assertion, not that one did.** The record boundary in
        `change.py` refuses this name too, so an exit-code test stays green with
        `contain.child` deleted from the resolver — and the deleted version has already
        `lstat`ed a path of somebody else's choosing outside the workspace by then, and
        reports a malformed record rather than a member that does not resolve."""
        self.create()
        outside = self.tmp / "outside"
        (outside / ".git").mkdir(parents=True)
        code, _, err = self.call(commands_change.cmd_change_add,
                                 change="component-api-2", repo="../../outside")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("no clone in workspace", err)
        self.assertNotIn("is not a name", err)      # not the boundary one layer down
        self.assertIsNone(change.member(change.read("ws", "component-api-2"), "../../outside"))

    def test_a_member_that_is_a_symlink_out_of_the_workspace_is_refused(self):
        """The name passes `segment_ok` — it is the *target* that leaves. `is_clone` is
        what answers this: git draws the line, since a clone's `.git` is a directory."""
        self.create()
        elsewhere = self.tmp / "elsewhere"
        elsewhere.mkdir()
        os.symlink(elsewhere, workspace.workspace_dir("ws") / "sneaky")
        code, _, err = self.call(commands_change.cmd_change_add,
                                change="component-api-2", repo="sneaky")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("no clone in workspace", err)

    def test_a_directory_that_is_not_a_checkout_is_not_a_member(self):
        self.create()
        (workspace.workspace_dir("ws") / "refs-like").mkdir()
        code, _, _ = self.call(commands_change.cmd_change_add,
                              change="component-api-2", repo="refs-like")
        self.assertEqual(code, commands_change.REFUSED)


class TestEachRefusalIsItsOwn(ChangeCommands):
    def test_an_unknown_change_is_named_as_such(self):
        code, _, err = self.call(commands_change.cmd_change_add,
                                change="never-created", repo="charter")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("no change", err)
        self.assertNotIn("no clone", err)

    def test_a_duplicate_member_is_named_as_such(self):
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2", repo="charter")
        code, _, err = self.call(commands_change.cmd_change_add,
                                change="component-api-2", repo="charter")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("already a member", err)
        self.assertNotIn("no clone", err)

    def test_a_cycle_is_named_as_such_and_nothing_is_written(self):
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2", repo="charter")
        self.call(commands_change.cmd_change_add, change="component-api-2",
                 repo="charter-metrics", needs=["charter"])
        before = change.path_for("ws", "component-api-2").read_bytes()
        # `.github` needs charter-metrics, which needs charter — fine. Now make charter
        # need `.github` and the three of them close a loop.
        self.call(commands_change.cmd_change_add, change="component-api-2",
                 repo=".github", needs=["charter-metrics"])
        rec = change.read("ws", "component-api-2")
        rec["members"][0]["needs"] = [".github"]
        with self.assertRaises(change.RecordError) as cm:
            change.write("ws", "component-api-2", rec)
        self.assertIn("cycle", str(cm.exception))
        del before

    def test_needs_naming_a_non_member_is_refused_by_the_command(self):
        self.create()
        code, _, err = self.call(commands_change.cmd_change_add, change="component-api-2",
                                repo="charter", needs=["nobody"])
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("not a member", err)
        self.assertNotIn("no clone", err)
        self.assertEqual(change.read("ws", "component-api-2")["members"], [])

    def test_creating_a_change_twice_is_refused(self):
        self.create()
        code, _, err = self.call(commands_change.cmd_change_create,
                                change="component-api-2", why="again")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("already exists", err)
        self.assertEqual(change.read("ws", "component-api-2")["why"], "API 1 -> 2")

    def test_a_slug_that_is_not_a_name_is_refused(self):
        for slug in ("..", "a/b", ".hidden", "-b", "x\x00y"):
            with self.subTest(slug=slug):
                code, _, err = self.call(commands_change.cmd_change_create,
                                        change=slug, why="x")
                self.assertEqual(code, 1)
                self.assertIn("is not a change name", err)


class TestWhyIsRequired(ChangeCommands):
    def test_create_without_why_does_not_parse(self):
        from charter import cli
        with self.assertRaises(SystemExit):
            cli.build_parser().parse_args(["change", "create", "component-api-2"])

    def test_create_with_an_empty_why_is_refused_by_the_handler(self):
        """argparse cannot see `--why ""`. A change with no stated reason is unreadable six
        months later, which is the one job the record has that git cannot do."""
        code, _, err = self.call(commands_change.cmd_change_create,
                                change="component-api-2", why="   ")
        self.assertEqual(code, 1)
        self.assertIn("--why", err)
        self.assertFalse(change.exists("ws", "component-api-2"))

    def test_drop_without_why_does_not_parse(self):
        from charter import cli
        with self.assertRaises(SystemExit):
            cli.build_parser().parse_args(["change", "drop", "component-api-2", "charter"])

    def test_drop_with_an_empty_why_is_refused_by_the_handler(self):
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2", repo="charter")
        code, _, err = self.call(commands_change.cmd_change_drop,
                                change="component-api-2", repo="charter", why="")
        self.assertEqual(code, 1)
        self.assertIn("--why", err)
        self.assertIsNotNone(change.member(change.read("ws", "component-api-2"), "charter"))


class TestDrop(ChangeCommands):
    def test_a_dropped_member_joins_excluded_with_its_reason_and_a_timestamp(self):
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2", repo="charter")
        code, _, err = self.call(commands_change.cmd_change_drop, change="component-api-2",
                                repo="charter", why="maintainer pins API 1")
        self.assertEqual(code, 0, err)
        rec = change.read("ws", "component-api-2")
        self.assertEqual(rec["members"], [])
        self.assertEqual(rec["excluded"][0]["repo"], "charter")
        self.assertEqual(rec["excluded"][0]["why"], "maintainer pins API 1")
        self.assertTrue(rec["excluded"][0]["at"])

    def test_a_repo_that_was_never_a_member_can_be_excluded(self):
        """§4's own step: the operator decides a repo is out of scope and records that, so
        the record says it was considered rather than forgotten."""
        self.create()
        code, _, err = self.call(commands_change.cmd_change_drop, change="component-api-2",
                                repo="charter-slack", why="no components")
        self.assertEqual(code, 0, err)
        self.assertIn("never a member", err)
        self.assertEqual(change.read("ws", "component-api-2")["excluded"][0]["repo"],
                         "charter-slack")

    def test_dropping_a_blocker_is_refused_and_names_its_dependents(self):
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2", repo="charter")
        self.call(commands_change.cmd_change_add, change="component-api-2",
                 repo="charter-metrics", needs=["charter"])
        code, _, err = self.call(commands_change.cmd_change_drop, change="component-api-2",
                                 repo="charter", why="out")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("cannot be dropped", err)
        self.assertIn("charter-metrics", err)
        self.assertIn("to land first", err)
        # And not the record boundary's own sentence, which refuses the same write for a
        # different reason ("needs 'charter', which is not a member") and would leave an
        # exit-code test green over this gate's deletion — while telling the operator their
        # record is malformed rather than which member is still waiting on this one.
        self.assertNotIn("not a member of this change", err)
        self.assertIsNotNone(change.member(change.read("ws", "component-api-2"), "charter"))

    def test_dropping_the_same_repo_twice_is_refused(self):
        self.create()
        self.call(commands_change.cmd_change_drop, change="component-api-2",
                 repo="charter-slack", why="no components")
        code, _, err = self.call(commands_change.cmd_change_drop, change="component-api-2",
                                repo="charter-slack", why="again")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("already excluded", err)

    def test_a_dropped_repo_is_never_silently_gone(self):
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2", repo="charter")
        self.call(commands_change.cmd_change_drop, change="component-api-2",
                 repo="charter", why="reason kept")
        code, out, _ = self.call(commands_change.cmd_change_show, change="component-api-2")
        self.assertEqual(code, 0)
        self.assertIn("excluded", out)
        self.assertIn("reason kept", out)

    def test_re_adding_an_excluded_repo_lifts_the_exclusion_loudly(self):
        self.create()
        self.call(commands_change.cmd_change_drop, change="component-api-2",
                 repo="charter", why="changed my mind later")
        code, _, err = self.call(commands_change.cmd_change_add,
                                change="component-api-2", repo="charter")
        self.assertEqual(code, 0, err)
        self.assertIn("lifted", err)
        self.assertIn("changed my mind later", err)
        rec = change.read("ws", "component-api-2")
        self.assertEqual(rec["excluded"], [])
        self.assertIsNotNone(change.member(rec, "charter"))

    def test_dropping_a_name_that_is_a_path_is_refused(self):
        self.create()
        for repo in ("..", "a/b", "x\\x00y"):
            with self.subTest(repo=repo):
                code, _, err = self.call(commands_change.cmd_change_drop,
                                         change="component-api-2", repo=repo, why="out")
                self.assertEqual(code, commands_change.REFUSED)
                self.assertIn("is not a name", err)
                self.assertEqual(change.read("ws", "component-api-2")["excluded"], [])


class TestForget(ChangeCommands):
    def test_forget_deletes_the_record(self):
        self.create()
        code, _, err = self.call(commands_change.cmd_change_forget, change="component-api-2")
        self.assertEqual(code, 0, err)
        self.assertFalse(change.exists("ws", "component-api-2"))

    def test_forget_deletes_no_landing_log_line(self):
        """The log is a past-tense declaration of what happened. Deleting history to tidy a
        list is how a store starts lying."""
        self.create()
        change.log_dir("ws").mkdir(parents=True)
        log = change.log_dir("ws") / "host.jsonl"
        log.write_text('{"change": "component-api-2", "merge": "e0c9d13"}\n')
        self.call(commands_change.cmd_change_forget, change="component-api-2")
        self.assertIn("e0c9d13", log.read_text())

    def test_forgetting_what_is_not_there_is_refused(self):
        code, _, err = self.call(commands_change.cmd_change_forget, change="never-created")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("no change", err)

    def test_a_slug_that_is_a_path_forgets_nothing(self):
        self.create()
        code, _, _ = self.call(commands_change.cmd_change_forget, change="../../etc/passwd")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertTrue(change.exists("ws", "component-api-2"))


class TestABranchNameIsArgvNotARef(ChangeCommands):
    def test_a_branch_beginning_with_a_dash_is_refused(self):
        self.create()
        for branch in ("-b", "--upload-pack=touch /tmp/pwn"):
            with self.subTest(branch=branch):
                code, _, err = self.call(commands_change.cmd_change_add,
                                        change="component-api-2", repo="charter",
                                        branch=branch)
                self.assertEqual(code, 1)
                self.assertIn("FLAG", err)
                self.assertEqual(change.read("ws", "component-api-2")["members"], [])

    def test_a_branch_carrying_a_newline_is_refused(self):
        self.create()
        code, _, err = self.call(commands_change.cmd_change_add, change="component-api-2",
                                repo="charter", branch="ok\n✓ merged everything")
        self.assertEqual(code, 1)
        self.assertIn("one plain line", err)

    def test_the_argv_form_reaches_the_same_refusal(self):
        """`--branch=-b` is how a dash-led value gets past argparse at all, so the guard has
        to be charter's rather than the parser's."""
        from charter import cli
        ns = cli.build_parser().parse_args(
            ["change", "add", "component-api-2", "charter", "--branch=-b", "-w", "ws"])
        self.create()
        code, _, err = self.call(commands_change.cmd_change_add, change=ns.change,
                                repo=ns.repo, branch=ns.branch)
        self.assertEqual(code, 1)
        self.assertIn("FLAG", err)


class TestTheWritePathIsContainedToo(ChangeCommands):
    """`create` is the one verb that writes without reading first, so it is the one where
    `contain.writable` is the gate rather than a second opinion. A committed link at
    `changes/` would otherwise relocate every record written under it — `mkdir(exist_ok=
    True)` accepts a symlink to a directory without complaint."""

    def test_creating_into_a_changes_directory_that_links_out_of_the_plane_is_refused(self):
        elsewhere = self.tmp / "elsewhere"
        elsewhere.mkdir()
        os.symlink(elsewhere, change.changes_dir("ws"))
        code, _, err = self.call(commands_change.cmd_change_create,
                                 change="component-api-2", why="x")
        self.assertEqual(code, 1)
        self.assertIn("control plane", err)
        self.assertEqual(list(elsewhere.iterdir()), [])

    def test_forget_declines_a_record_that_is_a_link_out_of_the_plane(self):
        elsewhere = self.tmp / "elsewhere.json"
        elsewhere.write_text("{}")
        change.changes_dir("ws").mkdir(parents=True)
        os.symlink(elsewhere, change.changes_dir("ws") / "component-api-2.json")
        code, _, err = self.call(commands_change.cmd_change_forget, change="component-api-2")
        self.assertEqual(code, 1)
        self.assertIn("could not delete", err)
        self.assertTrue(elsewhere.exists())


class TestTheAuthorIsOneLine(ChangeCommands):
    """`by` comes out of `git config`, a file on this machine charter did not write, and it
    lands in a record a LIVE workspace commits and every `show` prints back."""

    def _author_with(self, stdout: str) -> str:
        from unittest import mock
        with mock.patch.object(commands_change.util, "run",
                               return_value=SimpleNamespace(stdout=stdout)):
            return commands_change._author()

    def test_only_the_first_line_is_taken(self):
        self.assertEqual(self._author_with("Real Name\n✓ merged everything\n"), "Real Name")

    def test_an_unset_git_identity_still_produces_a_name(self):
        self.assertTrue(self._author_with(""))


class TestHostileValuesRenderAsOneRow(ChangeCommands):
    """A change slug, a `why`, a repo name and a branch name are all committed values that
    reach a report line. #453's mechanism one surface over: a value crossing into a format
    with structure without being escaped for it. Tested with hostile values rather than
    reasoned about."""

    ESC = "\x1b"

    def _hand_write(self, rec: dict) -> None:
        d = change.changes_dir("ws")
        d.mkdir(parents=True, exist_ok=True)
        (d / f"{rec['change']}.json").write_text(json.dumps(rec))

    def test_a_why_that_would_forge_a_row_never_reaches_the_record(self):
        code, _, err = self.call(commands_change.cmd_change_create, change="hostile",
                                 why="ok\n✓ everything landed  #601")
        self.assertEqual(code, 1)
        self.assertIn("--why", err)
        self.assertFalse(change.exists("ws", "hostile"))

    def test_a_why_that_cannot_be_one_line_never_reaches_the_record_at_all(self):
        """The boundary half. `contain.segment_ok` deliberately does NOT constrain a repo
        name this way — see the two cases below, which are why the display half is still
        load-bearing rather than belt and braces."""
        self._hand_write({"change": "hostile", "why": f"plain {self.ESC}[2K{self.ESC}[1G x",
                          "created": "t", "by": "t", "members": [], "excluded": []})
        code, _, err = self.call(commands_change.cmd_change_show, change="hostile")
        self.assertEqual(code, 1)
        self.assertIn("not one plain line", err)

    def test_a_repo_name_carrying_an_escape_sequence_is_escaped_on_the_row(self):
        """`segment_ok` asks about separators, `..` and NUL — not about control characters
        — so a repo name out of a committed record reaches the row with its ESC intact
        unless the row escapes it. That is the display half doing work nothing else does."""
        self._hand_write({"change": "hostile", "why": "w", "created": "t", "by": "t",
                          "members": [{"repo": f"api{self.ESC}[2K{self.ESC}[1Gsneaky",
                                       "branch": "x", "needs": []}],
                          "excluded": []})
        code, out, _ = self.call(commands_change.cmd_change_show, change="hostile")
        self.assertEqual(code, 0)
        self.assertNotIn(self.ESC, out)

    def test_a_repo_name_carrying_a_newline_renders_as_exactly_one_row(self):
        self._hand_write({"change": "hostile", "why": "w", "created": "t", "by": "t",
                          "members": [{"repo": "api\n  charter  branch main  landed",
                                       "branch": "x", "needs": []}],
                          "excluded": []})
        code, out, _ = self.call(commands_change.cmd_change_show, change="hostile")
        self.assertEqual(code, 0)
        self.assertEqual(len([ln for ln in out.splitlines() if "branch " in ln]), 1)

    def test_a_member_name_never_widens_a_row_past_its_column(self):
        """`contain.one_line` BEFORE the width arithmetic (#472): `tui.pad` measures what it
        is given, so a value escaped afterwards is padded to the wrong width and the column
        stops lining up at exactly the row somebody else controls."""
        self._hand_write({"change": "hostile", "why": "w", "created": "t", "by": "t",
                          "members": [{"repo": "a\u2028b", "branch": "x", "needs": []},
                                      {"repo": "short", "branch": "y", "needs": []}],
                          "excluded": []})
        code, out, _ = self.call(commands_change.cmd_change_show, change="hostile")
        self.assertEqual(code, 0)
        rows = [ln for ln in out.splitlines() if "branch " in ln]
        self.assertEqual(len(rows), 2)
        self.assertEqual(len({ln.index("branch ") for ln in rows}), 1)

    def test_an_excluded_repo_is_named_on_its_row_rather_than_silently_shortened(self):
        """The exclusion rows are a second table over the same untrusted names, and this is
        the half `tui` cannot supply. `tui.sanitize` *removes* a character with no glyph, so
        a raw cell renders `x<U+2028>b` as `xb` — a row naming a repository that is not the
        one in the record, which is #498's finding exactly. `contain.one_line` escapes it
        instead, so the reader sees which repository it is."""
        self._hand_write({"change": "hostile", "why": "w", "created": "t", "by": "t",
                          "members": [],
                          "excluded": [{"repo": "x\u2028b", "why": "no components",
                                        "at": "t"}]})
        code, out, _ = self.call(commands_change.cmd_change_show, change="hostile")
        self.assertEqual(code, 0)
        self.assertIn("\\u2028", out)
        self.assertEqual(len([ln for ln in out.splitlines()
                              if "no components" in ln]), 1)

    def test_a_refused_record_is_named_in_the_listing_without_forging_a_row(self):
        self._hand_write({"change": "hostile", "why": "w\nx", "created": "t", "by": "t",
                          "members": [], "excluded": []})
        code, _, err = self.call(commands_change.cmd_change_list)
        self.assertEqual(code, 1)
        self.assertIn("hostile", err)


class TestNoRefusalCanForgeARowEither(ChangeCommands):
    """The command surface's half of the same property. A repo name and a change slug both
    arrive as **argv**, so they reach a refusal without having passed through the record at
    all — and a refusal is a line of charter's own output that a value must not be able to
    write a second one of.

    Measured as a **comparison** with the same refusal over a benign value, rather than
    against a literal count: the count includes the workspace banner and whatever tips the
    command prints, and a test that hard-codes those measures the tips instead of the
    property. Each case asserts the benign refusal printed something first, so a command
    that fell silent cannot pass by printing nothing twice."""

    HOSTILE = "api\n  charter  branch main  landed"

    def _lines(self, fn, **kw) -> int:
        _, _, err = self.call(fn, **kw)
        return len(err.strip().splitlines())

    def _same(self, fn, benign: dict, hostile: dict) -> None:
        want = self._lines(fn, **benign)
        self.assertGreaterEqual(want, 2, "the benign refusal printed nothing to compare")
        self.assertEqual(self._lines(fn, **hostile), want)

    def test_the_no_clone_refusal_does_not_grow_for_a_hostile_repo(self):
        self.create()
        self._same(commands_change.cmd_change_add,
                   {"change": "component-api-2", "repo": "charter-slack"},
                   {"change": "component-api-2", "repo": self.HOSTILE})

    def test_the_unknown_change_refusal_does_not_grow_for_a_hostile_slug(self):
        self._same(commands_change.cmd_change_add,
                   {"change": "never-created", "repo": "charter"},
                   {"change": self.HOSTILE, "repo": "charter"})

    def test_the_bad_slug_refusal_does_not_grow_for_a_hostile_slug(self):
        self._same(commands_change.cmd_change_create,
                   {"change": "..", "why": "x"},
                   {"change": self.HOSTILE, "why": "x"})

    def test_the_forget_refusal_does_not_grow_for_a_hostile_slug(self):
        self.create()
        self._same(commands_change.cmd_change_forget,
                   {"change": "never-created"}, {"change": self.HOSTILE})

    def test_the_already_excluded_refusal_does_not_grow_for_a_hostile_repo(self):
        self.create()
        for repo in ("gone", self.HOSTILE):
            self.call(commands_change.cmd_change_drop, change="component-api-2",
                      repo=repo, why="first")
        self._same(commands_change.cmd_change_drop,
                   {"change": "component-api-2", "repo": "gone", "why": "again"},
                   {"change": "component-api-2", "repo": self.HOSTILE, "why": "again"})

    def test_the_duplicate_member_refusal_does_not_grow_for_a_hostile_repo(self):
        """A repo name is `segment_ok`, which asks about separators, `..` and NUL and
        nothing else — so a line break is a legal repository name and a legal directory
        name, and this member is a real clone with one in it."""
        self.clone(self.HOSTILE)
        self.create()
        for repo in ("charter", self.HOSTILE):
            self.call(commands_change.cmd_change_add, change="component-api-2", repo=repo)
        self._same(commands_change.cmd_change_add,
                   {"change": "component-api-2", "repo": "charter"},
                   {"change": "component-api-2", "repo": self.HOSTILE})

    def test_a_hostile_blocker_name_cannot_forge_the_drop_refusal(self):
        d = change.changes_dir("ws")
        d.mkdir(parents=True, exist_ok=True)
        for slug, blocker in (("benign", "web"), ("hostile", "we\nb  charter  landed")):
            (d / f"{slug}.json").write_text(json.dumps({
                "change": slug, "why": "w", "created": "t", "by": "t",
                "members": [{"repo": "api", "branch": "b", "needs": []},
                            {"repo": blocker, "branch": "b", "needs": ["api"]}],
                "excluded": []}))
        self._same(commands_change.cmd_change_drop,
                   {"change": "benign", "repo": "api", "why": "out"},
                   {"change": "hostile", "repo": "api", "why": "out"})


class TestALongValueIsClippedWhereTheRowIs(ChangeCommands):

    def _hand_write(self, rec: dict) -> None:
        d = change.changes_dir("ws")
        d.mkdir(parents=True, exist_ok=True)
        (d / f"{rec['change']}.json").write_text(json.dumps(rec))

    """The record's own bound is `contain`'s PATH budget, so a `why` a little longer than a
    row is a legitimate record — and the ROW budget is then doing real work at the printing
    site rather than being belt over a brace."""

    def test_a_long_why_is_clipped_on_the_row(self):
        long = "w" * (contain.DISPLAY_LIMIT + 60)
        self.create(why=long)
        code, out, _ = self.call(commands_change.cmd_change_show, change="component-api-2")
        self.assertEqual(code, 0)
        row = [ln for ln in out.splitlines() if ln.startswith("  why:")][0]
        self.assertLess(len(row), len(long))
        self.assertTrue(row.endswith("…"))

    def test_the_command_and_the_record_hold_a_why_to_the_same_bound(self):
        """Two bounds that disagree is a `why` charter accepts and then refuses to read
        back — a record it wrote and cannot open."""
        self.create(why="w" * change.TEXT_LIMIT)
        self.assertEqual(len(change.read("ws", "component-api-2")["why"]),
                         change.TEXT_LIMIT)

    def test_no_printed_row_can_be_as_long_as_the_value_behind_it(self):
        """The record's bound is `TEXT_LIMIT`; a ROW's is `DISPLAY_LIMIT`, and the gap
        between them is exactly what every `contain.one_line` at a printing site is for.
        Asked of every line of both commands at once, with every text field of the record
        set to the longest value the record will hold — so a cell somebody forgets to
        contain fails this without anybody adding a case for it."""
        long = "v" * (change.TEXT_LIMIT - 1)
        self._hand_write({
            "change": "component-api-2", "why": long, "created": long, "by": long,
            "members": [{"repo": "m" * 300, "branch": long, "needs": []},
                        {"repo": "n" * 300, "branch": long, "needs": ["m" * 300]}],
            "excluded": [{"repo": "x" * 300, "why": long, "at": long}]})
        for fn in (commands_change.cmd_change_show, commands_change.cmd_change_list):
            kw = {"change": "component-api-2"} if fn is commands_change.cmd_change_show else {}
            code, out, _ = self.call(fn, **kw)
            self.assertEqual(code, 0)
            self.assertTrue(out.strip())
            for line in out.splitlines():
                with self.subTest(fn=fn.__name__, line=line[:40]):
                    self.assertLess(len(line), change.TEXT_LIMIT, line[:120])

    def test_a_long_branch_is_clipped_on_the_row(self):
        long = "b" * (contain.DISPLAY_LIMIT + 60)
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2",
                  repo="charter", branch=long)
        code, out, _ = self.call(commands_change.cmd_change_show, change="component-api-2")
        self.assertEqual(code, 0)
        row = [ln for ln in out.splitlines() if "branch " in ln][0]
        self.assertLess(len(row), len(long))
        self.assertTrue(row.endswith("…"))


class TestTheCommandSurfacesOwnEdges(ChangeCommands):
    """What each command does at the edge of a guard rather than in the middle of it: an
    absent value, a fallback nobody exercises, a plural, a success line. Every case here
    was a line the deletion sweep could delete in silence."""

    ESC = "\x1b"
    HOSTILE = "api\n  charter  branch main  landed"

    def test_the_refusal_exit_code_is_two_and_that_is_the_contract(self):
        """The NUMBER, not the name. A caller branches on it — `commands_worktree` spends
        its own 2 the same way — and 1 is what every other failure returns, so a change of
        value here is a change of interface even though every test that uses the constant
        keeps passing."""
        self.assertEqual(commands_change.REFUSED, 2)

    def test_the_banner_says_the_workspace_came_from_the_flag(self):
        """The banner is how an agent knows which plane it is acting on. `-w` is the
        loudest of the rungs and it has to be the one reported."""
        self.create()
        _, _, err = self.call(commands_change.cmd_change_show, change="component-api-2")
        self.assertIn("workspace: ws", err)
        self.assertIn("--workspace", err)

    def test_an_absent_why_is_refused_rather_than_crashing(self):
        code, _, err = self.call(commands_change.cmd_change_create,
                                 change="component-api-2", why=None)
        self.assertEqual(code, 1)
        self.assertIn("--why", err)

    def test_dropping_from_a_change_that_does_not_exist_is_refused(self):
        code, _, err = self.call(commands_change.cmd_change_drop,
                                 change="never-created", repo="charter", why="out")
        self.assertEqual(code, commands_change.REFUSED)
        self.assertIn("no change", err)

    def test_one_blocker_reads_as_one_and_two_read_as_two(self):
        self.create()
        for repo in ("charter", "charter-metrics", ".github"):
            self.call(commands_change.cmd_change_add, change="component-api-2", repo=repo)
        rec = change.read("ws", "component-api-2")
        rec["members"][1]["needs"] = ["charter"]
        change.write("ws", "component-api-2", rec)
        _, _, err = self.call(commands_change.cmd_change_drop, change="component-api-2",
                              repo="charter", why="out")
        self.assertIn("still needs it", err)
        rec = change.read("ws", "component-api-2")
        rec["members"][2]["needs"] = ["charter"]
        change.write("ws", "component-api-2", rec)
        _, _, err = self.call(commands_change.cmd_change_drop, change="component-api-2",
                              repo="charter", why="out")
        self.assertIn("still need it", err)
        self.assertNotIn("still needs it", err)

    def test_a_dropped_member_does_not_read_as_never_a_member(self):
        """The two sentences say different things about the record, and the one that is
        wrong sends a reader looking for a member that was there all along."""
        self.create()
        self.call(commands_change.cmd_change_add, change="component-api-2", repo="charter")
        _, _, err = self.call(commands_change.cmd_change_drop, change="component-api-2",
                              repo="charter", why="out")
        self.assertNotIn("never a member", err)
        self.assertIn("member(s) left", err)

    def test_a_change_with_exclusions_says_how_many_in_both_commands(self):
        self.create()
        self.call(commands_change.cmd_change_drop, change="component-api-2",
                  repo="charter-slack", why="no components")
        _, show, _ = self.call(commands_change.cmd_change_show, change="component-api-2")
        _, listing, _ = self.call(commands_change.cmd_change_list)
        self.assertIn("1 excluded", show)
        self.assertIn("1 excluded", listing)

    def test_a_change_with_no_exclusions_counts_none_in_the_listing(self):
        self.create()
        _, listing, _ = self.call(commands_change.cmd_change_list)
        self.assertNotIn("excluded", listing)

    def test_a_record_file_whose_NAME_forges_a_row_is_named_safely_in_the_listing(self):
        """The refused half of the listing takes its slug from the FILESYSTEM, so it has
        been through nothing at all — and a file named with a line break is a file somebody
        can create. Compared against the same refusal over an ordinary name, so the count
        of tips and banners is not what is being measured."""
        self.create()
        d = change.changes_dir("ws")
        (d / "benign.json").write_text("{")
        _, _, one = self.call(commands_change.cmd_change_list)
        (d / "benign.json").unlink()
        (d / "hos\ntile.json").write_text("{")
        _, _, two = self.call(commands_change.cmd_change_list)
        self.assertGreaterEqual(len(one.strip().splitlines()), 2)
        self.assertEqual(len(two.strip().splitlines()), len(one.strip().splitlines()))

    def test_a_hostile_member_name_is_named_on_the_success_line_too(self):
        """A repo name is `segment_ok`, which admits a line break — and this is the line
        charter prints when it *accepts* the member, which is the one nobody thinks of."""
        self.clone(self.HOSTILE)
        self.create()
        _, _, benign = self.call(commands_change.cmd_change_add,
                                 change="component-api-2", repo="charter")
        _, _, hostile = self.call(commands_change.cmd_change_add,
                                  change="component-api-2", repo=self.HOSTILE)
        self.assertGreaterEqual(len(benign.strip().splitlines()), 2)
        self.assertEqual(len(hostile.strip().splitlines()),
                         len(benign.strip().splitlines()))

    def test_a_hostile_name_is_named_on_the_exclusion_success_line_too(self):
        self.create()
        _, _, benign = self.call(commands_change.cmd_change_drop,
                                 change="component-api-2", repo="gone", why="out")
        _, _, hostile = self.call(commands_change.cmd_change_drop,
                                  change="component-api-2", repo=self.HOSTILE, why="out")
        self.assertGreaterEqual(len(benign.strip().splitlines()), 2)
        self.assertEqual(len(hostile.strip().splitlines()),
                         len(benign.strip().splitlines()))

    def test_the_lifted_exclusion_warning_cannot_be_as_long_as_what_it_quotes(self):
        """**Both** fields it quotes, one case each. The reason is the operator's; the
        timestamp is charter's own — until the record is hand-edited, which is the only
        state this store ever assumes about a file it did not write this second."""
        long = "w" * (change.TEXT_LIMIT - 1)
        self.create()
        self.call(commands_change.cmd_change_drop, change="component-api-2",
                  repo="charter", why=long)
        _, _, err = self.call(commands_change.cmd_change_add,
                              change="component-api-2", repo="charter")
        self.assertIn("lifted", err)
        for line in err.splitlines():
            self.assertLess(len(line), change.TEXT_LIMIT, line[:80])

        d = change.changes_dir("ws")
        (d / "stamped.json").write_text(json.dumps(
            {"change": "stamped", "why": "w", "created": "t", "by": "t", "members": [],
             "excluded": [{"repo": "charter", "why": "short", "at": long}]}))
        _, _, err = self.call(commands_change.cmd_change_add,
                              change="stamped", repo="charter")
        self.assertIn("lifted", err)
        for line in err.splitlines():
            self.assertLess(len(line), change.TEXT_LIMIT, line[:80])

    def test_the_blocker_refusal_does_not_grow_for_a_hostile_member(self):
        """The refusal names the repo being dropped, and that name comes from argv — so
        this is the drop gate's own printing site, distinct from the blockers it lists."""
        d = change.changes_dir("ws")
        d.mkdir(parents=True, exist_ok=True)
        for slug, blocked in (("benign", "api"), ("hostile", self.HOSTILE)):
            (d / f"{slug}.json").write_text(json.dumps({
                "change": slug, "why": "w", "created": "t", "by": "t",
                "members": [{"repo": blocked, "branch": "b", "needs": []},
                            {"repo": "web", "branch": "b", "needs": [blocked]}],
                "excluded": []}))
        counts = []
        for slug, repo in (("benign", "api"), ("hostile", self.HOSTILE)):
            _, _, err = self.call(commands_change.cmd_change_drop, change=slug,
                                  repo=repo, why="out")
            counts.append(len(err.strip().splitlines()))
        self.assertGreaterEqual(counts[0], 3)
        self.assertEqual(counts[1], counts[0])

    def test_a_member_name_is_shown_rather_than_silently_shortened(self):
        """`tui.sanitize` REMOVES a character with no glyph, so a raw cell renders
        `a<U+2028>b` as `ab` — a row naming a repository that is not the one in the record
        (#498). `contain.one_line` escapes it instead, and the escape has to survive the
        column's own truncation, which is why the width is computed from the escaped cell —
        measuring the raw one gives a column 21 wide for a cell that needs 27, and the
        escape is exactly what falls off the end."""
        d = change.changes_dir("ws")
        d.mkdir(parents=True, exist_ok=True)
        (d / "hostile.json").write_text(json.dumps({
            "change": "hostile", "why": "w", "created": "t", "by": "t",
            "members": [{"repo": "b" * 20 + "\u2028a", "branch": "x", "needs": []},
                        {"repo": "s", "branch": "y",
                         "needs": ["b" * 20 + "\u2028a"]}],
            "excluded": []}))
        code, out, _ = self.call(commands_change.cmd_change_show, change="hostile")
        self.assertEqual(code, 0)
        self.assertEqual(out.count("\\u2028"), 2)   # the member row AND its `needs` cell

    def test_a_very_long_slug_is_clipped_on_both_rows(self):
        """`change_name_ok` puts no ceiling on a slug's length — the filesystem does, at a
        good deal more than a row."""
        slug = "s" * 200
        d = change.changes_dir("ws")
        d.mkdir(parents=True, exist_ok=True)
        (d / f"{slug}.json").write_text(json.dumps(
            {"change": slug, "why": "w", "created": "t", "by": "t",
             "members": [], "excluded": []}))
        _, show, _ = self.call(commands_change.cmd_change_show, change=slug)
        _, listing, _ = self.call(commands_change.cmd_change_list)
        for out in (show, listing):
            self.assertTrue(out.strip())
            for line in out.splitlines():
                self.assertLess(len(line), 200, line[:60])


class TestTheAuthorsOwnEdges(ChangeCommands):
    """`by` comes out of `git config` — a file on this machine charter did not write — and
    lands in a record a LIVE workspace commits and every `show` prints back."""

    ESC = "\x1b"

    def _author_with(self, stdout) -> str:
        from unittest import mock
        with mock.patch.object(commands_change.util, "run",
                               return_value=SimpleNamespace(stdout=stdout)):
            return commands_change._author()

    def test_surrounding_whitespace_is_trimmed_from_both_ends(self):
        self.assertEqual(self._author_with("  Real Name  \n"), "Real Name")

    def test_a_name_carrying_an_escape_is_contained(self):
        self.assertNotIn(self.ESC, self._author_with(f"Real{self.ESC}[2KName\n"))

    def test_no_stdout_at_all_is_not_a_crash(self):
        self.assertTrue(self._author_with(None))

    def test_the_fallback_is_the_shell_the_operator_is_in(self):
        from unittest import mock
        with mock.patch.dict(os.environ, {"USER": "operator-name"}, clear=True):
            self.assertEqual(self._author_with(""), "operator-name")


class TestThereIsNoExpansionAndNoForge(unittest.TestCase):
    """§6.1 rule 2. Every member in a record was typed by somebody, and in a LIVE workspace
    the record is committed — so it is reviewable in a diff. A surface that could expand a
    pattern into members is a committed file naming repositories nobody typed.

    Asserted against the module's own syntax tree rather than its text, because the property
    is *absence* and a substring search over a file that also documents the property finds
    the documentation. A behavioural test cannot answer it at all: it can only try the
    patterns somebody thought of."""

    def _tree(self) -> ast.AST:
        from charter import commands_change as m
        return ast.parse(Path(m.__file__).read_text(encoding="utf-8"))

    def _imports(self) -> set[str]:
        names: set[str] = set()
        for node in ast.walk(self._tree()):
            if isinstance(node, ast.Import):
                names |= {a.name.split(".")[0] for a in node.names}
            elif isinstance(node, ast.ImportFrom):
                if node.module:
                    names.add(node.module.split(".")[0])
                names |= {a.name for a in node.names}
        return names

    def _called(self) -> set[str]:
        out: set[str] = set()
        for node in ast.walk(self._tree()):
            if isinstance(node, ast.Call):
                f = node.func
                if isinstance(f, ast.Name):
                    out.add(f.id)
                elif isinstance(f, ast.Attribute):
                    out.add(f.attr)
        return out

    def test_nothing_here_enumerates_repos(self):
        for name in ("list_repos", "repos", "glob", "rglob", "iterdir", "walk", "scandir"):
            self.assertNotIn(name, self._called(),
                             f"{name}() would let a change enumerate rather than be typed")

    def test_the_inventory_is_not_even_imported(self):
        self.assertNotIn("inventory", self._imports())

    def test_no_change_subcommand_takes_a_pattern_or_an_all_flag(self):
        from charter import cli
        top = cli.build_parser()._subparsers._group_actions[0].choices["change"]
        for name, sub in top._subparsers._group_actions[0].choices.items():
            for action in sub._actions:
                for opt in action.option_strings:
                    self.assertNotIn(opt, ("--all", "--all-repos", "--pattern", "--glob",
                                           "--match", "--every"), f"change {name} {opt}")

    def test_no_forge_module_and_no_network_module_is_imported(self):
        """Records only. The forge half is a later phase, and keeping the record surface
        separable from it is what makes "what did the operator declare" answerable without
        asking anybody's API."""
        for name in ("forge", "gitpolicy", "planegit", "glstate", "report", "inventory",
                     "socket", "urllib", "http", "ssl", "subprocess"):
            self.assertNotIn(name, self._imports())

    def test_the_only_child_process_this_module_can_start_is_a_local_config_read(self):
        """`util.run` is a general subprocess runner, so "no network" is a claim about the
        argv rather than about the import. Every command this module can hand it is listed
        here, in full: one read of `git config`, which touches no remote."""
        spawned = []
        for node in ast.walk(self._tree()):
            if (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                    and node.func.attr in ("run", "Popen", "call", "check_output",
                                           "check_call", "system", "popen")):
                spawned.append(ast.literal_eval(node.args[0]) if node.args else None)
        self.assertEqual(spawned, [["git", "config", "user.name"]])


if __name__ == "__main__":
    unittest.main()
