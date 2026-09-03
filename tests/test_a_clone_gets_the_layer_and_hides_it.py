"""A clone inside a workspace gets charter's layer, hidden in its own `info/exclude` — #870.

`workspaces/<ws>/<repo>/` is a repo of its own. Claude Code's walk-up for `.claude/agents/`
and `.claude/skills/` stops at that git boundary and its project settings never walked up
at all, so a chat launched in a clone got none of the plane's layer — the widest gap of
the three, and the one #850 explicitly left open.

charter writes the layer there anyway and pays a cost the workspace directory does not
owe: every generated path is registered in that checkout's `.git/info/exclude`, which is
per-checkout, never committed and not itself tracked. The four things that makes true are
what this file is about — the write is idempotent, charter's files are marked, a file
charter did not generate is never touched, and removing the workspace removes what charter
added — plus the fifth that makes them work at all: a linked worktree's `.git` is a FILE,
and its exclude lives in a main repo somewhere else entirely.

Every test here writes only into a `PersonaIso` tmp plane (or a `mkdtemp` of its own for
the main repo behind a worktree, which by construction cannot be inside one). Nothing
touches the developer's real `workspaces/`. `git` identity and signing come from
`tests/_gitguard`, which redirects `$GIT_CONFIG_GLOBAL` for the whole suite — no test here
spells its own environment, and no test here writes git config outside the repo it made.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from charter import commands, config, doctor, workspace
from charter import commands_workspace as cw

from tests import _isolation
from tests.test_a_workspace_carries_charters_layer import _plane_settings


def _git(where, *args) -> subprocess.CompletedProcess:
    """`git` in *where*, inheriting this process's environment ON PURPOSE.

    `tests/_gitguard` has already pointed `$GIT_CONFIG_GLOBAL` at a file this package
    writes, so identity and `commit.gpgsign=false` are already answered for every child.
    Building a private env here would drop that redirect — which `_planeguard`'s
    `AmbientGitConfig` refuses at the `Popen`, and which is the shape that hangs a suite
    on somebody's fingerprint reader (#641).
    """
    return subprocess.run(["git", "-C", str(where), *args], check=True,
                          capture_output=True, text=True)


def _repo(d: Path) -> Path:
    """A real git repo at *d* with one commit — enough for `git status` to mean something."""
    d.mkdir(parents=True, exist_ok=True)
    _git(d.parent, "init", "-q", d.name)
    (d / "README.md").write_text("theirs\n")
    _git(d, "add", "-A")
    _git(d, "commit", "-qm", "init")
    return d


class CloneLayer(_isolation.PersonaIso):
    """A plane with settings and one workspace holding one REAL clone."""

    def setUp(self) -> None:
        super().setUp()
        # The tripwire the whole layer suite is written under: if `PersonaIso` ever stops
        # repointing derived paths at a throwaway tree, every write below lands in a real
        # plane — and half of them land in a repo charter does not own.
        self.assertIn("edm-test-", str(config.STATE_DIR))
        _plane_settings(config.ROOT)
        self.ws = "api"
        workspace.ensure(self.ws)
        self.clone = _repo(workspace.workspace_dir(self.ws) / "svc")

    def wire(self) -> dict:
        return dict(workspace.wire_guest(self.clone))

    def excludes(self, tree: Path | None = None) -> str:
        p = workspace.git_exclude_file(tree or self.clone)
        return p.read_text() if p and p.exists() else ""

    def status(self, tree: Path | None = None) -> str:
        """``-uall``, not the default. Porcelain collapses an untracked directory to one
        `?? .claude/` line, which reads as "charter's noise is here" whether the file
        inside is charter's or the operator's — the one distinction this whole design
        turns on. Listing every file is what makes a clean status mean clean."""
        return _git(tree or self.clone, "status", "--porcelain", "-uall").stdout


class TheLayerArrives(CloneLayer):
    def test_the_clone_gets_the_planes_settings(self):
        """The half that never walked up: Claude Code reads project settings from the
        session's cwd and nowhere else, so this file is the whole of whether a chat in the
        clone has a plugin, a status line and a `$CHARTER_HARNESS`."""
        self.wire()
        doc = json.loads((self.clone / ".claude" / "settings.json").read_text())
        self.assertEqual(sorted(doc), ["enabledPlugins", "env", "statusLine"])
        self.assertEqual(doc["enabledPlugins"], {"charter@charter": True})

    def test_the_clone_gets_the_planes_agents(self):
        """The half that walked up and was cut off. `enabledPlugins` alone does not close
        it: the plugin carries charter's own skills and cannot carry THIS plane's personas,
        which `persona sync-agents` generates from `personas/`. Without these a chat in a
        clone loads charter and still cannot delegate to a persona."""
        agent = config.ROOT / ".claude" / "agents" / "steward.md"
        agent.parent.mkdir(parents=True, exist_ok=True)
        agent.write_text("---\nname: steward\n---\nroute the work\n")
        self.wire()
        self.assertEqual((self.clone / ".claude" / "agents" / "steward.md").read_text(),
                         "---\nname: steward\n---\nroute the work\n")

    def test_the_workspace_directory_still_gets_neither(self):
        """The boundary did not dissolve, it moved one directory in. Agents and skills
        reach `workspaces/<ws>/` by walking up — a workspace directory is inside the
        plane's own repo — and a second copy there would shadow the first."""
        (config.ROOT / ".claude" / "agents").mkdir(parents=True, exist_ok=True)
        (config.ROOT / ".claude" / "agents" / "steward.md").write_text("x\n")
        workspace.wire_harnesses(self.ws)
        wd = workspace.workspace_dir(self.ws)
        self.assertFalse((wd / ".claude" / "agents").exists())
        self.assertTrue((self.clone / ".claude" / "agents" / "steward.md").is_file())

    def test_the_plane_charter_file_is_not_carried_in(self):
        """CLAUDE.md walks up on the same rule and is deliberately left behind: it is the
        one file a repo of its own is most likely to have opinions about, and dropping the
        plane's project instructions into somebody else's repo — to be read there as that
        repo's instructions — is a claim of a different size from mirroring a settings key."""
        (config.ROOT / "CLAUDE.md").write_text("the plane's instructions\n")
        (config.ROOT / ".claude" / "CLAUDE.md").write_text("also the plane's\n")
        self.wire()
        self.assertFalse((self.clone / "CLAUDE.md").exists())
        self.assertFalse((self.clone / ".claude" / "CLAUDE.md").exists())

    def test_wiring_a_workspace_wires_its_clones(self):
        """`charter workspace reinit` is the repair for a clone too — which it can only be
        if the clones hang off the call `reinit` already makes."""
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(rows["svc/.claude/settings.json"], "created")
        self.assertEqual(rows["svc/.git/info/exclude"], "created")


class NothingShowsUpInTheirStatus(CloneLayer):
    """The constraint that makes the feature admissible: the operator's own `git status`."""

    def test_git_status_in_the_clone_stays_clean(self):
        self.assertEqual(self.status(), "")
        self.wire()
        self.assertEqual(self.status(), "",
                         "charter's files are untracked noise in a repo it does not own")

    def test_the_block_names_the_files_and_not_a_directory(self):
        """A `.claude/` glob would hide the operator's OWN untracked files under
        `.claude/` from their own status — charter making somebody's work invisible in
        their repo is a worse failure than the noise this exists to prevent."""
        self.wire()
        (self.clone / ".claude" / "mine.md").write_text("my note\n")
        self.assertIn(".claude/mine.md", self.status())

    def test_the_clones_gitignore_is_never_touched(self):
        """A tracked file in somebody else's repo. `info/exclude` is the one place a guest
        may write: per-checkout, never committed, not itself tracked."""
        gi = self.clone / ".gitignore"
        gi.write_text("build/\n")
        _git(self.clone, "add", "-A")
        _git(self.clone, "commit", "-qm", "ignore build")
        self.wire()
        self.assertEqual(gi.read_text(), "build/\n")

    def test_nothing_charter_wrote_can_be_staged(self):
        """`git add -A` in the operator's repo is the failure mode named in #850's own
        text. Excluded paths are not added, so the layer cannot reach their commit."""
        self.wire()
        _git(self.clone, "add", "-A")
        staged = _git(self.clone, "diff", "--cached", "--name-only").stdout
        self.assertEqual(staged.strip(), "")


class TheWriteIsIdempotent(CloneLayer):
    def test_a_second_wire_changes_nothing(self):
        self.wire()
        before = self.excludes()
        rows = self.wire()
        self.assertEqual(self.excludes(), before)
        self.assertEqual(rows[".git/info/exclude"], "present")

    def test_ten_wires_do_not_grow_the_file(self):
        """An `ensure` runs on every launch. Appending would leave `git status` clean and
        `info/exclude` growing without bound — the failure nobody would ever look for."""
        for _ in range(10):
            self.wire()
        text = self.excludes()
        self.assertEqual(text.count(workspace._EXCLUDE_BEGIN), 1)
        self.assertEqual(text.count("/.claude/settings.json"), 1)

    def test_the_operators_own_lines_survive(self):
        p = workspace.git_exclude_file(self.clone)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("# theirs\n*.tmp\n")
        self.wire()
        self.wire()
        text = self.excludes()
        self.assertIn("*.tmp", text)
        self.assertEqual(text.count("*.tmp"), 1)
        self.assertIn(workspace._EXCLUDE_BEGIN, text)

    def test_an_unterminated_block_is_replaced_rather_than_doubled(self):
        """Somebody deleted the end marker by hand. Appending a second block below the
        first is the duplication this whole function exists to prevent, wearing a crash
        for a hat."""
        p = workspace.git_exclude_file(self.clone)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(f"*.tmp\n{workspace._EXCLUDE_BEGIN}\n/.stale-path\n")
        self.wire()
        text = self.excludes()
        self.assertEqual(text.count(workspace._EXCLUDE_BEGIN), 1)
        self.assertNotIn("/.stale-path", text)
        self.assertIn("*.tmp", text)


class OwnershipIsLegible(CloneLayer):
    def test_the_marker_is_a_sidecar_in_the_clone(self):
        self.wire()
        doc = json.loads((self.clone / workspace.GENERATED_MARKER).read_text())
        self.assertEqual(doc[".claude/settings.json"],
                         workspace.content_digest(
                             (self.clone / ".claude" / "settings.json").read_text()))

    def test_the_marker_hides_itself_too(self):
        """It is charter's file in their repo exactly as the settings are."""
        self.wire()
        self.assertIn(f"/{workspace.GENERATED_MARKER}", self.excludes())
        self.assertEqual(self.status(), "")

    def test_doctor_can_tell_stale_from_foreign(self):
        self.wire()
        p = self.clone / ".claude" / "settings.json"
        p.write_text(json.dumps({"env": {"CHARTER_HARNESS": "claude-code"}}) + "\n")
        marker = json.loads((self.clone / workspace.GENERATED_MARKER).read_text())
        marker[".claude/settings.json"] = workspace.content_digest(p.read_text())
        (self.clone / workspace.GENERATED_MARKER).write_text(json.dumps(marker) + "\n")
        self.assertEqual(dict(workspace.guest_layer(self.clone))[".claude/settings.json"],
                         "stale")
        p.write_text("{}\n")
        self.assertEqual(dict(workspace.guest_layer(self.clone))[".claude/settings.json"],
                         "foreign")


class AFileCharterDidNotWrite(CloneLayer):
    def test_is_never_overwritten(self):
        p = self.clone / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text('{"statusLine": {"type": "command", "command": "theirs"}}\n')
        rows = self.wire()
        self.assertEqual(rows[".claude/settings.json"], "foreign")
        self.assertIn("theirs", p.read_text())

    def test_is_never_hidden_either(self):
        """Charter neither rewrites the operator's file nor makes it vanish from their own
        status — the second is the quieter half of the same restraint."""
        p = self.clone / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text('{"statusLine": {"type": "command", "command": "theirs"}}\n')
        self.wire()
        self.assertNotIn("/.claude/settings.json", self.excludes())
        self.assertIn(".claude/settings.json", self.status())

    def test_an_all_foreign_checkout_gets_no_block_at_all(self):
        p = self.clone / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{}\n")
        rows = self.wire()
        self.assertNotIn(".git/info/exclude", rows)
        self.assertNotIn(workspace._EXCLUDE_BEGIN, self.excludes())

    def test_a_path_the_operator_takes_over_stops_being_hidden(self):
        """charter wrote it, then they rewrote it. The marker no longer vouches for the
        content, so the next wire drops it from the block and their edit becomes visible
        to them again."""
        self.wire()
        self.assertIn("/.claude/settings.json", self.excludes())
        (self.clone / ".claude" / "settings.json").write_text('{"env": {"X": "1"}}\n')
        self.wire()
        self.assertNotIn("/.claude/settings.json", self.excludes())


class TheExcludeIsReported(CloneLayer):
    """The row exists because its absence is the whole failure: every file can be current
    and the operator still sees charter's noise in their own `git status`."""

    def setUp(self) -> None:
        super().setUp()
        # `check_workspace_harness` short-circuits on `HAS_CONTROL_PLANE`, and
        # `_isolation` deliberately hands a root with no `charter.toml` — so without these
        # two lines every case here passes through "no control plane found" and asserts
        # nothing, which is exactly how the row's quiet states reached the sweep as
        # survivors once already.
        (Path(config.ROOT) / "charter.toml").write_text("schema = 1\n")
        prev = config.use(Path(config.ROOT))
        self.addCleanup(config.restore, prev)
        self.assertTrue(config.HAS_CONTROL_PLANE)

    def test_an_emptied_exclude_reads_as_missing_and_reinit_repairs_it(self):
        self.wire()
        workspace.git_exclude_file(self.clone).write_text("")
        self.assertEqual(dict(workspace.guest_layer(self.clone))[".git/info/exclude"],
                         "missing")
        self.assertEqual(self.wire()[".git/info/exclude"], "created")
        self.assertEqual(dict(workspace.guest_layer(self.clone))[".git/info/exclude"], "ok")

    def test_a_block_naming_the_wrong_files_reads_as_stale(self):
        self.wire()
        p = workspace.git_exclude_file(self.clone)
        p.write_text(f"{workspace._EXCLUDE_BEGIN}\n/.gone\n{workspace._EXCLUDE_END}\n")
        self.assertEqual(dict(workspace.guest_layer(self.clone))[".git/info/exclude"],
                         "stale")
        self.assertEqual(self.wire()[".git/info/exclude"], "refreshed")

    def test_reading_the_layer_never_writes(self):
        """`doctor` calls this from the SessionStart hook. A check that healed what it
        found would report every clone hidden by having just hidden it."""
        workspace.guest_layer(self.clone)
        self.assertFalse((self.clone / ".claude").exists())
        self.assertNotIn(workspace._EXCLUDE_BEGIN, self.excludes())

    def test_doctor_names_the_clone(self):
        rows = dict(workspace.harness_layer(self.ws))
        self.assertEqual(rows["svc/.claude/settings.json"], "missing")
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("api/svc/.claude/settings.json", r.detail)


class AWorktreeIsAGuestToo(CloneLayer):
    """`.git` is a FILE, and `info/exclude` is the MAIN repo's — measured, not assumed.

    Git treats `info/` as shared between a repo and its linked worktrees, so a pattern
    written to `.git/worktrees/<name>/info/exclude` is read by nobody. Assuming
    `<root>/.git/` is a directory does not fail loudly here: `mkdir -p` cheerfully creates
    the path beside the `.git` FILE, the write succeeds, and the operator's status is full
    of charter.
    """

    def setUp(self) -> None:
        super().setUp()
        self.main = Path(tempfile.mkdtemp(prefix="edm-test-main-"))
        self.addCleanup(shutil.rmtree, self.main, ignore_errors=True)
        _repo(self.main / "svc-main")
        self.tree = workspace.workspace_dir(self.ws) / "svc-wt"
        _git(self.main / "svc-main", "worktree", "add", "-q", str(self.tree), "-b", "side")

    def test_the_git_dir_is_resolved_through_the_pointer_file(self):
        self.assertFalse((self.tree / ".git").is_dir())
        # `.resolve()` on both sides: git writes the RESOLVED path into the pointer file,
        # and a macOS temp dir reaches the test as `/var/...` for git's `/private/var/...`.
        self.assertEqual(workspace.git_dir(self.tree).resolve(),
                         (self.main / "svc-main" / ".git" / "worktrees" / "svc-wt").resolve())

    def test_the_exclude_file_is_the_main_repos(self):
        self.assertEqual(workspace.git_exclude_file(self.tree).resolve(),
                         (self.main / "svc-main" / ".git" / "info" / "exclude").resolve())

    def test_the_worktrees_status_really_is_clean(self):
        """The end-to-end claim, against real git rather than against charter's model of
        it: this is what fails if `commondir` is skipped."""
        workspace.wire_guest(self.tree)
        self.assertEqual(self.status(self.tree), "")

    def test_a_worktree_is_a_guest_tree_and_is_not_a_clone(self):
        """`clones()` must not grow a second repo — a worktree is not one. `guest_trees`
        asks a different question: where would a chat's cwd be cut off from the plane?"""
        self.assertNotIn(self.tree, workspace.clones(self.ws))
        self.assertIn(self.tree, workspace.guest_trees(self.ws))

    def test_a_directory_with_no_checkout_is_not_a_guest(self):
        plain = workspace.workspace_dir(self.ws) / "notes"
        plain.mkdir()
        self.assertIsNone(workspace.git_dir(plain))
        self.assertIsNone(workspace.git_exclude_file(plain))
        self.assertNotIn(plain, workspace.guest_trees(self.ws))

    def test_a_git_file_pointing_nowhere_is_not_a_checkout(self):
        broken = workspace.workspace_dir(self.ws) / "broken"
        broken.mkdir()
        (broken / ".git").write_text("gitdir: ../does-not-exist\n")
        self.assertIsNone(workspace.git_dir(broken))
        self.assertNotIn(broken, workspace.guest_trees(self.ws))

    def test_a_git_file_that_says_nothing_is_not_a_checkout(self):
        junk = workspace.workspace_dir(self.ws) / "junk"
        junk.mkdir()
        (junk / ".git").write_text("not a gitdir line\n")
        self.assertIsNone(workspace.git_dir(junk))


class RemovingTheWorkspaceRemovesWhatCharterAdded(CloneLayer):
    def test_unwiring_a_clone_takes_its_files_and_its_block(self):
        self.wire()
        removed = workspace.unwire_guest(self.clone)
        self.assertIn(".claude/settings.json", removed)
        self.assertIn(workspace.GENERATED_MARKER, removed)
        self.assertFalse((self.clone / ".claude").exists(),
                         "an emptied directory is charter still visible in their repo")
        self.assertNotIn(workspace._EXCLUDE_BEGIN, self.excludes())
        self.assertEqual(self.status(), "")

    def test_unwiring_leaves_the_operators_own_files_alone(self):
        self.wire()
        (self.clone / ".claude" / "mine.md").write_text("my note\n")
        p = workspace.git_exclude_file(self.clone)
        p.write_text("*.tmp\n" + p.read_text())
        workspace.unwire_guest(self.clone)
        self.assertTrue((self.clone / ".claude" / "mine.md").is_file())
        self.assertIn("*.tmp", self.excludes())

    def test_unwiring_never_deletes_a_file_the_operator_took_over(self):
        """Deleting it would be the same overwrite this design refuses, one verb on."""
        self.wire()
        p = self.clone / ".claude" / "settings.json"
        p.write_text('{"env": {"theirs": "1"}}\n')
        workspace.unwire_guest(self.clone)
        self.assertEqual(p.read_text(), '{"env": {"theirs": "1"}}\n')


class RemovingAWorkspaceWithAWorktree(CloneLayer):
    """The case `shutil.rmtree` cannot answer: the exclude file is not in the directory.

    A linked worktree's `info/exclude` belongs to a main repo somewhere else on disk, so
    removing the workspace would leave charter's block sitting in a repo that is still
    there, naming paths that no longer exist, in a file the operator did not write.
    """

    def setUp(self) -> None:
        super().setUp()
        self.main = Path(tempfile.mkdtemp(prefix="edm-test-main-"))
        self.addCleanup(shutil.rmtree, self.main, ignore_errors=True)
        _repo(self.main / "svc-main")
        self.tree = workspace.workspace_dir(self.ws) / "svc-wt"
        _git(self.main / "svc-main", "worktree", "add", "-q", str(self.tree), "-b", "side")
        workspace.wire_guest(self.tree)

    def test_the_block_is_gone_from_the_main_repo(self):
        main_exclude = self.main / "svc-main" / ".git" / "info" / "exclude"
        self.assertIn(workspace._EXCLUDE_BEGIN, main_exclude.read_text())
        workspace.unwire_guests(self.ws)
        self.assertNotIn(workspace._EXCLUDE_BEGIN, main_exclude.read_text())

    def test_workspace_remove_unwires_before_it_deletes(self):
        main_exclude = self.main / "svc-main" / ".git" / "info" / "exclude"
        from types import SimpleNamespace
        import contextlib
        import io

        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            rc = cw.cmd_workspace_remove(SimpleNamespace(name=self.ws, force=True))
        self.assertEqual(rc, 0, out.getvalue() + err.getvalue())
        self.assertFalse(workspace.workspace_dir(self.ws).exists())
        self.assertNotIn(workspace._EXCLUDE_BEGIN, main_exclude.read_text())


class TheAnnouncement(CloneLayer):
    def test_cloning_says_what_was_written_and_where_it_is_hidden(self):
        """charter has just written files into a repo the operator owns. A mechanism whose
        entire visible signature is its own absence is one nobody can find when they want
        it gone."""
        import contextlib
        import io

        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            commands._wire_clones(self.ws)
        self.assertIn("svc", err.getvalue())
        self.assertIn("info/exclude", err.getvalue())

    def test_a_second_clone_into_the_same_workspace_says_nothing(self):
        import contextlib
        import io

        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            commands._wire_clones(self.ws)
            err.truncate(0), err.seek(0)
            commands._wire_clones(self.ws)
        self.assertEqual(err.getvalue(), "")


if __name__ == "__main__":
    unittest.main()
