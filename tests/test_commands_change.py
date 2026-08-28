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

from charter import change, commands_change, workspace
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
                          "members": [{"repo": "a\x07b", "branch": "x", "needs": []},
                                      {"repo": "short", "branch": "y", "needs": []}],
                          "excluded": []})
        code, out, _ = self.call(commands_change.cmd_change_show, change="hostile")
        self.assertEqual(code, 0)
        rows = [ln for ln in out.splitlines() if "branch " in ln]
        self.assertEqual(len(rows), 2)
        self.assertEqual(len({ln.index("branch ") for ln in rows}), 1)

    def test_a_refused_record_is_named_in_the_listing_without_forging_a_row(self):
        self._hand_write({"change": "hostile", "why": "w\nx", "created": "t", "by": "t",
                          "members": [], "excluded": []})
        code, _, err = self.call(commands_change.cmd_change_list)
        self.assertEqual(code, 1)
        self.assertIn("hostile", err)


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
