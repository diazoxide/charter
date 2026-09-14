"""Every command that shows the operator the plane's workspaces names each one it could not look
at, in `workspace reinit --all`'s sentence, and never drops it without a word (#1043, #1028).

`workspace.list_workspaces` leaves out a workspace charter cannot `stat`. On 3.14 it always did;
on 3.11–3.13 it raised instead, so these commands ended in a traceback. #1043 made the listing
one answer on every interpreter, which would have made every one of these commands quiet about
it everywhere. `workspace list` printed a table without it, `sync --all` synced "all workspaces"
past it, `recall --all` searched "every workspace" but that one.

The fixture is a real refusal: the workspace is a link into a directory at mode 000, beside a
readable workspace, so `stat` of it is refused and `stat` of its neighbour is not. The sentence
is spelled out by hand, not rebuilt from the function that prints it.
"""

from __future__ import annotations

import io
import os
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands, commands_workspace, config, memstore, report, workspace
from tests._isolation import PersonaIso, make_plane


def locked_workspace(case, name: str = "gamma") -> Path:
    """`workspaces/<name>` as a link into a directory at mode 000: listed under `workspaces/`, and
    refused to `stat`. Mode restored in cleanup."""
    locked = case.tmp / "locked"
    (locked / name).mkdir(parents=True)
    wd = workspace.workspace_dir(name)
    wd.parent.mkdir(parents=True, exist_ok=True)
    wd.symlink_to(locked / name)
    locked.chmod(0o000)
    case.addCleanup(locked.chmod, 0o755)
    return wd


def sentence(name: str = "gamma") -> str:
    wd = workspace.workspace_dir(name)
    return (f"workspace '{name}' cannot be checked — charter changes nothing it cannot see; "
            f"restoring read access to {wd} clears this.")


@unittest.skipIf(os.geteuid() == 0, "root searches a directory whatever its mode")
class CommandCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        workspace.ensure("alpha")
        workspace.scaffold("alpha")
        self.locked = locked_workspace(self)

    def run_cmd(self, fn, **kw) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue(), err.getvalue()


class TheWorkspaceTable(CommandCase):
    def test_workspace_list_names_it_beside_the_row_it_could_read(self):
        _, out, err = self.run_cmd(commands_workspace.cmd_workspace_list)
        self.assertIn("alpha", out)
        self.assertIn(sentence(), err)

    def test_with_no_workspace_it_could_read_it_does_not_say_there_are_none_yet(self):
        """"No workspaces yet — create one" is false of a plane holding one charter could not
        read, and sends the operator to make a second."""
        workspace.workspace_dir("alpha").rename(self.tmp / "alpha-gone")
        _, _, err = self.run_cmd(commands_workspace.cmd_workspace_list)
        self.assertIn(sentence(), err)
        self.assertNotIn("No workspaces yet", err)


class TheStatusCount(CommandCase):
    def test_status_names_it(self):
        _, out, err = self.run_cmd(commands.cmd_status, workspace=None, all=False)
        self.assertIn("alpha", out)
        self.assertIn(sentence(), err)


class SyncAll(CommandCase):
    def test_sync_all_names_it(self):
        _, _, err = self.run_cmd(commands.cmd_sync, all=True)
        self.assertIn(sentence(), err)


class RecallEveryWorkspace(CommandCase):
    def recall(self, all_workspaces: bool) -> str:
        memstore.write(workspace.memory_dir("alpha"), "the keycloak token rotates yearly",
                       title="keycloak token policy", timestamped=True)
        _, out, err = self.run_cmd(commands.cmd_recall, query="keycloak", scope=None,
                                   ephemeral=False, persona=None, workspace="alpha",
                                   all_workspaces=all_workspaces, since=None, limit=8,
                                   full=False)
        self.assertIn("keycloak", out)
        return err

    def test_recall_all_workspaces_names_it(self):
        self.assertIn(sentence(), self.recall(all_workspaces=True))

    def test_recall_in_one_workspace_does_not(self):
        """It searched one workspace, and said so; the one it did not list is no news there."""
        self.assertNotIn("cannot be checked", self.recall(all_workspaces=False))


class OptimizeEveryWorkspace(CommandCase):
    def test_optimize_with_no_name_names_it(self):
        _, _, err = self.run_cmd(commands_workspace.cmd_workspace_optimize, name=None, all=True,
                                 apply=False, stale_days=90)
        self.assertIn(sentence(), err)

    def test_optimize_of_one_named_workspace_does_not(self):
        _, _, err = self.run_cmd(commands_workspace.cmd_workspace_optimize, name="alpha",
                                 all=False, apply=False, stale_days=90)
        self.assertNotIn("cannot be checked", err)


class TheReportScrub(CommandCase):
    """`report` does not show the operator a set of workspaces: it removes their names from a
    draft bound for a public tracker. Naming the one it could not read there would publish the
    name the scrub exists to remove, so it is scrubbed like every other — its name is an
    identifier whether or not charter can look inside it."""

    def test_a_workspace_it_cannot_read_is_still_scrubbed(self):
        out, _ = report.scrub("ran charter sync in gamma and alpha")
        self.assertNotIn("gamma", out)
        self.assertNotIn("alpha", out)


if __name__ == "__main__":
    unittest.main()
