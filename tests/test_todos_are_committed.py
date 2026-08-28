"""A LIVE workspace's todos — and now its changes — are actually **committed**, not merely
un-ignored.

Ticket 05 added `todos/` to the managed `.gitignore` block, which is only half the job.
Un-ignoring a path tells git it *may* be tracked; something still has to stage it. The
paths that `workspace save`, the Stop-hook autosave, `live --off` and `rename` act on come
from one list, and `todos/` was missing from it — so a LIVE workspace's todo list was
visible to git and committed by nothing.

That is the exact failure ADR 0004 predicted for this feature and the one the ticket called
"the quietest possible": every other part keeps working, and the list simply never travels.
"""
from __future__ import annotations

import re
import subprocess
import unittest

from charter import change, config
from charter import commands_workspace as cw
from charter import todos, workspace
from tests._isolation import PersonaIso

_A_CHANGE = {"change": "one", "why": "because", "created": "2026-08-28T00:00:00+00:00",
             "by": "t", "members": [], "excluded": []}


class TestTheSharedPathSet(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")
        workspace.scaffold("alpha")

    def test_todos_are_in_the_set_of_paths_that_get_committed(self):
        todos.add("alpha", "something worth sharing")
        self.assertIn("workspaces/alpha/todos", cw._ws_meta_paths("alpha"))

    def test_the_existing_shared_paths_are_still_there(self):
        """The manifest, charter and memory must keep travelling — this adds, never
        replaces."""
        todos.add("alpha", "something")
        paths = cw._ws_meta_paths("alpha")
        self.assertIn("workspaces/alpha/workspace.md", paths)
        self.assertIn("workspaces/alpha/memory", paths)

    def test_a_workspace_with_no_todos_does_not_list_the_directory(self):
        """The list is filtered by existence, so an absent `todos/` must not appear —
        `git rm --cached` on a path that was never tracked fails the whole call, taking
        the manifest and memory down with it."""
        self.assertNotIn("workspaces/alpha/todos", cw._ws_meta_paths("alpha"))

    def test_recording_the_first_todo_is_what_adds_it(self):
        before = cw._ws_meta_paths("alpha")
        todos.add("alpha", "the first one")
        after = cw._ws_meta_paths("alpha")
        self.assertNotIn("workspaces/alpha/todos", before)
        self.assertIn("workspaces/alpha/todos", after)

    def test_every_listed_path_actually_exists(self):
        """The whole list is fed to git as literal paths; one that is not there fails the
        command for all of them."""
        todos.add("alpha", "something")
        from charter import config
        for rel in cw._ws_meta_paths("alpha"):
            self.assertTrue((config.ROOT / rel).exists(), rel)


class TestUnIgnoringAndCommittingAgree(PersonaIso):
    """The two halves have to name the same paths. They are edited in different files —
    the gitignore block in `workspace.py`, the staged set in `commands_workspace.py` — so
    nothing but a test stops one from moving without the other. That is precisely how
    `todos/` came to be un-ignored and never committed."""

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")
        workspace.scaffold("alpha")
        todos.add("alpha", "something")
        workspace.set_live("alpha", True)

    def test_everything_committed_is_also_un_ignored(self):
        gitignore = (config.ROOT / ".gitignore").read_text()
        for rel in cw._ws_meta_paths("alpha"):
            self.assertIn(f"!/{rel}", gitignore,
                          f"{rel} is staged for commit but still git-ignored")


def _un_ignored(name: str) -> set[str]:
    """The repo-relative paths the managed block un-ignores for one workspace.

    Normalised, because the two lists cannot be compared as sets in the shape they are
    written: `_live_block` emits `!/`-prefixed lines and a `/**` half for each directory,
    while `_ws_meta_paths` emits bare repo-relative paths. Only the `!` lines count — the
    block also carries a plain *exclusion* line for `changes/log`, which is the one path it
    deliberately keeps ignored.
    """
    out = set()
    for line in workspace._live_block([name]).splitlines():
        m = re.fullmatch(rf"!/(workspaces/{re.escape(name)}/[^ ]+)", line.strip())
        if m:
            out.add(re.sub(r"/\*\*$", "", m.group(1)))
    return out


class TestChangesTravelWithTheWorkspace(PersonaIso):
    """The fourth store. `changes/` is the same two-sided question `todos/` was — a path
    that is un-ignored and staged by nothing never travels, which is the quietest way this
    can fail — plus one asymmetry `todos/` does not have: `changes/log/` must be committed
    **never**, because it holds merge shas per host, appended without a lock, exactly as
    `pieces/` does."""

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")
        workspace.scaffold("alpha")
        workspace.write_manifest("alpha", {"repos": []})
        todos.add("alpha", "something")
        change.write("alpha", "one", dict(_A_CHANGE))
        workspace.set_live("alpha", True)

    def test_changes_are_in_the_set_of_paths_that_get_committed(self):
        self.assertIn("workspaces/alpha/changes", cw._ws_meta_paths("alpha"))

    def test_changes_are_un_ignored(self):
        self.assertIn("workspaces/alpha/changes", _un_ignored("alpha"))

    def test_everything_un_ignored_is_also_committable(self):
        """The direction the shipped test never asserted. Without it, adding a path to the
        gitignore block and forgetting `_ws_meta_paths` stays green — which is precisely
        how `todos/` came to be un-ignored and committed by nothing."""
        self.assertEqual(_un_ignored("alpha"), set(cw._ws_meta_paths("alpha")))

    def test_the_landing_log_is_in_neither_list(self):
        log = "workspaces/alpha/changes/log"
        self.assertNotIn(log, _un_ignored("alpha"))
        self.assertNotIn(log, cw._ws_meta_paths("alpha"))

    def test_git_itself_still_ignores_the_landing_log_inside_a_shared_changes_dir(self):
        """Asked of git rather than of the block's text: re-including `changes/` and then
        re-excluding `changes/log/` is a rule about *order*, and a rule about order is one
        a reader can get right in prose and wrong in the file."""
        if not _have_git():
            self.skipTest("no git")
        _git_init(config.ROOT)
        change.log_dir("alpha").mkdir(parents=True)
        (change.log_dir("alpha") / "h.jsonl").write_text('{"ts": "x"}\n')
        self.assertEqual(_check_ignore("workspaces/alpha/changes/one.json"), 1)   # tracked
        self.assertEqual(_check_ignore("workspaces/alpha/changes/log/h.jsonl"), 0)  # ignored

    def test_an_emptied_changes_directory_is_not_handed_to_git(self):
        """`charter change forget` on the last record leaves the directory behind, and one
        holding nothing but the never-committed `log/` is the same case. Either way `git rm
        --cached` on a path with nothing tracked under it fails the WHOLE call — untracking
        nothing, and leaving the manifest and memory committed on a workspace the operator
        just made private."""
        change.log_dir("alpha").mkdir(parents=True)
        (change.log_dir("alpha") / "h.jsonl").write_text("{}\n")
        change.forget("alpha", "one")
        self.assertTrue(change.changes_dir("alpha").exists())
        self.assertNotIn("workspaces/alpha/changes", cw._ws_meta_paths("alpha"))

    def test_live_off_untracks_the_whole_set_with_an_emptied_changes_dir(self):
        """The failure above, measured through git rather than argued about. One pathspec
        matching nothing is enough to make `git rm --cached` exit non-zero and untrack
        none of the others."""
        if not _have_git():
            self.skipTest("no git")
        _git_init(config.ROOT)
        subprocess.run(["git", "add", "-f", "--", *cw._ws_meta_paths("alpha")],
                       cwd=config.ROOT, check=True, capture_output=True)
        change.forget("alpha", "one")            # leaves changes/ behind, empty
        proc = subprocess.run(["git", "rm", "-r", "--cached", "-q", "--",
                               *cw._ws_meta_paths("alpha")],
                              cwd=config.ROOT, capture_output=True, text=True)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        tracked = subprocess.run(["git", "ls-files"], cwd=config.ROOT,
                                 capture_output=True, text=True).stdout
        self.assertNotIn("workspaces/alpha/memory", tracked)


class TestTheStructureBumpFlagsEveryOlderWorkspace(PersonaIso):
    """v3 creates no directory — `changes/` is lazy on purpose. The bump exists so a plane
    that went LIVE before this version re-runs `refresh_live_block()` and picks up the new
    lines; without it the records simply never travel, and nothing re-runs `set_live` on
    its own."""

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")
        workspace.scaffold("alpha")
        workspace.set_live("alpha", True)

    def _pin_to_v2(self) -> None:
        (workspace.workspace_dir("alpha") / ".charter-structure").write_text("2\n")
        # The block as a v2 charter wrote it: no `changes` lines at all.
        gi = config.ROOT / ".gitignore"
        gi.write_text(re.sub(r"^!?/workspaces/alpha/changes.*\n", "", gi.read_text(),
                             flags=re.MULTILINE))

    def test_a_v2_workspace_reports_needs_reinit(self):
        self._pin_to_v2()
        self.assertTrue(workspace.needs_reinit("alpha"))

    def test_reinit_adds_the_new_un_ignore_lines(self):
        self._pin_to_v2()
        self.assertNotIn("!/workspaces/alpha/changes",
                         (config.ROOT / ".gitignore").read_text())
        workspace.reinit("alpha")
        text = (config.ROOT / ".gitignore").read_text()
        self.assertIn("!/workspaces/alpha/changes", text)
        self.assertIn("/workspaces/alpha/changes/log/", text)

    def test_reinit_creates_no_changes_directory(self):
        self._pin_to_v2()
        workspace.reinit("alpha")
        self.assertFalse(change.changes_dir("alpha").exists())

    def test_a_fresh_workspace_is_current(self):
        """Positive control: the bump must flag OLD workspaces, not every workspace."""
        self.assertFalse(workspace.needs_reinit("alpha"))


def _have_git() -> bool:
    import shutil
    return shutil.which("git") is not None


def _git_init(path) -> None:
    subprocess.run(["git", "init", "-q", "-b", "main", str(path)],
                   check=True, capture_output=True)
    subprocess.run(["git", "config", "maintenance.auto", "false"], cwd=path,
                   check=True, capture_output=True)


def _check_ignore(rel: str) -> int:
    """git's own verdict: 0 = ignored, 1 = not ignored."""
    return subprocess.run(["git", "check-ignore", "-q", "--", rel],
                          cwd=config.ROOT, capture_output=True).returncode


if __name__ == "__main__":
    unittest.main()
