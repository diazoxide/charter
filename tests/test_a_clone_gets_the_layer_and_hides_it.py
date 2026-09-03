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

import contextlib
import io
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config, doctor, workspace
from charter import commands_workspace as cw
from charter.harness import claude_code, registry

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

    def test_the_clone_gets_the_planes_skills_too(self):
        """`.claude/skills/` walks up on the same rule and is cut off at the same boundary.
        A plane that keeps a skill of its own beside the plugin's has it in a workspace
        directory and would lose it one directory in."""
        s = config.ROOT / ".claude" / "skills" / "deploy" / "SKILL.md"
        s.parent.mkdir(parents=True, exist_ok=True)
        s.write_text("# deploy\n")
        self.wire()
        self.assertEqual((self.clone / ".claude" / "skills" / "deploy" / "SKILL.md")
                         .read_text(), "# deploy\n")

    def test_a_file_that_is_not_text_is_skipped_and_takes_nothing_with_it(self):
        """One unreadable file in `.claude/agents/` must not empty the layer — the
        restraint `One misbehaving harness does not empty the layer` already records one
        level up."""
        d = config.ROOT / ".claude" / "agents"
        d.mkdir(parents=True, exist_ok=True)
        (d / "logo.png").write_bytes(b"\x89PNG\r\n\x1a\n\xff\xfe")
        (d / "steward.md").write_text("route it\n")
        self.wire()
        self.assertFalse((self.clone / ".claude" / "agents" / "logo.png").exists())
        self.assertTrue((self.clone / ".claude" / "agents" / "steward.md").is_file())

    def test_wiring_a_workspace_wires_its_clones(self):
        """`charter workspace reinit` is the repair for a clone too — which it can only be
        if the clones hang off the call `reinit` already makes."""
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(rows["svc/.claude/settings.json"], "created")
        self.assertEqual(rows["svc/.git/info/exclude"], "created")


class APathThatWouldEscapeTheCheckout(CloneLayer):
    """`_harness_files`' containment refusal, which nothing reached.

    The sweep returned the whole `if … continue` as a survivor, and the guard is worth
    more here than it was one directory up: a `rel` carrying `..` escaping a WORKSPACE
    directory lands somewhere else in the plane, and a `rel` escaping a CLONE lands in a
    repo charter is a guest in — outside the tree it is allowed to write at all, with an
    `info/exclude` entry that could never hide it because the path is not under the
    checkout.

    The harness contract already says paths stay inside. A contract nothing enforces is a
    comment, and this is the one place every harness's answer passes through.
    """

    def files(self, rel: str) -> dict:
        rogue = SimpleNamespace(workspace_files=lambda: {rel: "not charter's to place\n"})
        with mock.patch.object(registry, "all",
                               return_value=[rogue, claude_code.ClaudeCodeHarness()]):
            return dict(workspace._guest_files(self.clone))

    def test_a_relative_path_climbing_out_is_dropped(self):
        self.assertEqual(sorted(self.files("../escaped.json")), [".claude/settings.json"])

    def test_a_path_that_climbs_out_and_lands_back_inside_is_kept(self):
        """The honest other half. Containment is decided by where the join LANDS, not by
        whether the spelling contains `..` — `../svc/x.json` from this checkout resolves
        back into it, so it is inside and it is written. Said out loud because a reader
        who assumes the test is "reject any `..`" would then be surprised by the guard's
        real shape, and would write the next harness against the wrong contract."""
        self.assertEqual(sorted(self.files("../svc/nested.json")),
                         ["../svc/nested.json", ".claude/settings.json"])

    def test_nothing_is_written_outside_the_checkout(self):
        rogue = SimpleNamespace(
            workspace_files=lambda: {"../escaped.json": "not charter's to place\n"})
        with mock.patch.object(registry, "all",
                               return_value=[rogue, claude_code.ClaudeCodeHarness()]):
            workspace.wire_guest(self.clone)
        self.assertFalse((workspace.workspace_dir(self.ws) / "escaped.json").exists())


class TheReportIsInAStableOrder(CloneLayer):
    """A checkout holds MORE than one generated file now, so ordering stopped being
    invisible the day agents were mirrored in.

    `doctor` prints the first four findings and elides the rest; an order that comes out
    of a dict is a truncated report whose contents change with insertion, for a tree where
    nothing changed. The exclude block has the same property with an operator reading it,
    and `unwire_guest`'s answer is what a caller prints back.
    """

    def agents(self, *names: str) -> None:
        d = config.ROOT / ".claude" / "agents"
        d.mkdir(parents=True, exist_ok=True)
        for n in names:
            (d / n).write_text(f"# {n}\n")

    def test_the_layer_rows_are_sorted(self):
        self.agents("zulu.md", "alpha.md", "mike.md")
        rels = [rel for rel, _status in workspace.guest_layer(self.clone)]
        self.assertEqual(rels, sorted(rels))
        # Spelled out as well as compared: `sorted(rels) == rels` also holds for a list of
        # one, and this class exists because there is now more than one.
        self.assertEqual(rels[0], ".claude/agents/alpha.md")

    def test_the_checkouts_come_back_in_a_stable_order(self):
        """A workspace holds several repos, and every row `wire_harnesses` returns is
        prefixed with the checkout's directory name. `iterdir` hands back the
        FILESYSTEM's order, so a report that inherits it lists the same tree differently
        on two machines — and differently again after a `workspace restore` re-creates
        the directories in another sequence."""
        for name in ("zebra", "alpha"):
            _repo(workspace.workspace_dir(self.ws) / name)
        trees = [t.name for t in workspace.guest_trees(self.ws)]
        self.assertEqual(trees, sorted(trees))
        self.assertEqual(trees[0], "alpha", "the first made was 'svc', not the first named")

    def test_the_block_lists_the_paths_sorted(self):
        self.agents("zulu.md", "alpha.md", "mike.md")
        self.wire()
        listed = [ln for ln in self.excludes().splitlines() if ln.startswith("/")]
        self.assertEqual(listed[:3], ["/.claude/agents/alpha.md",
                                      "/.claude/agents/mike.md",
                                      "/.claude/agents/zulu.md"])

    def test_what_unwiring_reports_removing_is_sorted(self):
        self.agents("zulu.md", "alpha.md", "mike.md")
        self.wire()
        removed = workspace.unwire_guest(self.clone)
        self.assertEqual(removed[:-1], sorted(removed[:-1]))
        self.assertEqual(removed[0], ".claude/agents/alpha.md")
        self.assertEqual(removed[-1], workspace.GENERATED_MARKER,
                         "the marker goes last — it is what vouches for the rest")


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

    def test_an_absent_exclude_file_reads_as_missing_too(self):
        """Not merely an emptied one: `git init` makes the file, and a repo whose
        `.git/info/` somebody cleaned out has no file to read at all."""
        self.wire()
        workspace.git_exclude_file(self.clone).unlink()
        self.assertEqual(dict(workspace.guest_layer(self.clone))[".git/info/exclude"],
                         "missing")
        self.assertEqual(self.wire()[".git/info/exclude"], "created")

    def test_a_generated_file_deleted_from_disk_stays_named_in_the_block(self):
        """It is still charter's path — charter puts it back on the next wire, and a block
        that dropped it would report `stale` on a checkout where nothing is wrong."""
        self.wire()
        (self.clone / ".claude" / "settings.json").unlink()
        self.assertEqual(dict(workspace.guest_layer(self.clone))[".git/info/exclude"], "ok")

    def test_an_all_foreign_checkout_gets_no_row_either(self):
        p = self.clone / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{}\n")
        self.assertNotIn(".git/info/exclude", dict(workspace.guest_layer(self.clone)))

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

    def test_a_relative_gitdir_is_resolved_against_the_checkout(self):
        """The spelling a SUBMODULE uses — `gitdir: ../.git/modules/<name>`. git writes an
        absolute path for a worktree, so nothing else here exercises the join, and without
        it the path is resolved against the process's cwd instead."""
        sub = workspace.workspace_dir(self.ws) / "vendored"
        sub.mkdir()
        real = workspace.workspace_dir(self.ws) / "modules-vendored"
        real.mkdir()
        (sub / ".git").write_text("gitdir: ../modules-vendored\n")
        self.assertEqual(workspace.git_dir(sub), sub / ".." / "modules-vendored")
        self.assertTrue(workspace.git_dir(sub).is_dir())

    def test_a_gitdir_path_containing_a_colon_is_read_whole(self):
        """`split(":", 1)`, never `rsplit`. A directory name may contain a colon, and
        splitting from the right hands back the tail of the PATH instead of the value —
        a `.git` file that points somewhere real, read as pointing nowhere."""
        odd = workspace.workspace_dir(self.ws) / "mod:ules"
        odd.mkdir()
        sub = workspace.workspace_dir(self.ws) / "colon"
        sub.mkdir()
        (sub / ".git").write_text("gitdir: ../mod:ules\n")
        self.assertEqual(workspace.git_dir(sub), sub / ".." / "mod:ules")

    def test_a_git_file_written_with_crlf_still_points_somewhere(self):
        """`.strip()`, never `lstrip`. `splitlines` takes the `\n` and leaves the `\r`,
        so a pointer file written on Windows keeps a carriage return on the end of the
        path — and a `Path` with a trailing `\r` is a directory that does not exist."""
        real = workspace.workspace_dir(self.ws) / "modules-crlf"
        real.mkdir()
        sub = workspace.workspace_dir(self.ws) / "fromwindows"
        sub.mkdir()
        (sub / ".git").write_bytes(b"gitdir: ../modules-crlf\r\n")
        self.assertEqual(workspace.git_dir(sub), sub / ".." / "modules-crlf")
        self.assertTrue(workspace.git_dir(sub).is_dir())

    def test_a_git_file_that_says_nothing_is_not_a_checkout(self):
        junk = workspace.workspace_dir(self.ws) / "junk"
        junk.mkdir()
        (junk / ".git").write_text("not a gitdir line\n")
        self.assertIsNone(workspace.git_dir(junk))


class WhatCannotBeReadOrWritten(CloneLayer):
    """The refusals. Each is an honest answer charter gives instead of forcing a write
    into a repo it does not own — and each is a branch nothing else in this file reaches."""

    def test_a_checkout_whose_git_dir_vanished_reads_as_unreadable(self):
        """Reachable, not defensive: an operator can `rm -rf .git` (or move the repo out
        from under a worktree) between one wire and the next. charter cannot then tell
        whether its files are hidden, and saying `unreadable` is the difference between
        that and saying they are."""
        self.wire()
        shutil.rmtree(self.clone / ".git")
        self.assertEqual(dict(workspace.guest_layer(self.clone))[".git/info/exclude"],
                         "unreadable")

    def test_unwiring_a_checkout_whose_git_dir_vanished_still_takes_the_files(self):
        """The files are charter's wherever git went. Removal must not stall on a block
        it can no longer find."""
        self.wire()
        shutil.rmtree(self.clone / ".git")
        removed = workspace.unwire_guest(self.clone)
        self.assertIn(".claude/settings.json", removed)
        self.assertFalse((self.clone / ".claude").exists())

    def test_an_exclude_that_is_not_a_file_is_reported_rather_than_forced(self):
        self.wire()
        p = workspace.git_exclude_file(self.clone)
        p.unlink()
        p.mkdir()
        self.assertEqual(dict(workspace.guest_layer(self.clone))[".git/info/exclude"],
                         "unreadable")
        self.assertEqual(self.wire()[".git/info/exclude"], "blocked")

    def test_an_exclude_charter_cannot_write_is_reported_and_never_forced(self):
        """Readable, unwritable — the half a `read_text` failure does not reach. `blocked`
        rather than a silent success is what keeps `doctor` from calling a checkout hidden
        that is not."""
        self.wire()
        p = workspace.git_exclude_file(self.clone)
        p.write_text("*.tmp\n")
        p.parent.chmod(0o500)
        p.chmod(0o444)
        self.addCleanup(p.parent.chmod, 0o700)
        self.assertEqual(self.wire()[".git/info/exclude"], "blocked")
        self.assertEqual(p.read_text(), "*.tmp\n")

    def test_a_generated_path_replaced_by_a_directory_does_not_raise(self):
        """`.claude/settings.json` as a DIRECTORY. Every read charter makes of it fails,
        and the layer still has to answer for the rest of the checkout."""
        self.wire()
        p = self.clone / ".claude" / "settings.json"
        p.unlink()
        p.mkdir()
        rows = dict(workspace.guest_layer(self.clone))
        self.assertEqual(rows[".claude/settings.json"], "unreadable")
        self.assertEqual(dict(self.wire())[".claude/settings.json"], "foreign")

    def test_a_workspace_that_does_not_exist_has_no_guests(self):
        self.assertEqual(workspace.guest_trees("never-made"), [])

    def test_a_plain_file_beside_the_clones_is_not_a_guest(self):
        f = workspace.workspace_dir(self.ws) / "notes.md"
        f.write_text("a note\n")
        self.assertIsNone(workspace.git_dir(f))
        self.assertNotIn(f, workspace.guest_trees(self.ws))


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

    def test_unwiring_a_checkout_charter_never_wired_writes_nothing(self):
        """Byte for byte, trailing newline included. `unwire_guests` runs over every
        checkout in the workspace, and most of them may have nothing of charter's in
        them — a rewrite there is a write into somebody's repo for no reason at all."""
        p = workspace.git_exclude_file(self.clone)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("*.tmp")            # no trailing newline, on purpose
        workspace.unwire_guest(self.clone)
        self.assertEqual(p.read_text(), "*.tmp")

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
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            rc = cw.cmd_workspace_remove(SimpleNamespace(name=self.ws, force=True))
        self.assertEqual(rc, 0, out.getvalue() + err.getvalue())
        self.assertFalse(workspace.workspace_dir(self.ws).exists())
        self.assertNotIn(workspace._EXCLUDE_BEGIN, main_exclude.read_text())


class CloningWiresWhatItMade(CloneLayer):
    """`cmd_clone` calls `workspace.ensure` BEFORE the clones exist, so without a second
    call the layer would arrive one command late — on whatever unrelated command happened
    to `ensure` next."""

    def test_cmd_clone_wires_the_checkout_it_just_made(self):
        r = {"name": "svc", "default_branch": "main", "forge": "github",
             "path_with_namespace": "g/svc"}
        with mock.patch.object(commands, "_clone_one",
                               lambda rr, wd: {"repo": rr, "dest": self.clone,
                                               "status": "exists"}), \
             mock.patch.object(commands.workspace, "resolve", lambda *a, **k: self.ws), \
             mock.patch.object(commands.workspace, "banner", lambda *a, **k: None), \
             mock.patch.object(commands.workspace, "ensure",
                               lambda *a: workspace.workspace_dir(self.ws)), \
             mock.patch.object(commands.inventory, "load", lambda: {}), \
             mock.patch.object(commands.inventory, "repos", lambda d=None: [r]), \
             mock.patch.object(commands, "_resolve_targets", lambda a, d: [r]), \
             contextlib.redirect_stderr(io.StringIO()):
            rc = commands.cmd_clone(SimpleNamespace(repos=["svc"], workspace=self.ws))
        self.assertEqual(rc, 0)
        self.assertTrue((self.clone / ".claude" / "settings.json").is_file())
        self.assertEqual(self.status(), "")


class TheAnnouncement(CloneLayer):
    def test_cloning_says_what_was_written_and_where_it_is_hidden(self):
        """charter has just written files into a repo the operator owns. A mechanism whose
        entire visible signature is its own absence is one nobody can find when they want
        it gone."""
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            commands._wire_clones(self.ws)
        self.assertIn("svc", err.getvalue())
        self.assertIn("info/exclude", err.getvalue())

    def test_a_refreshed_layer_is_announced_too(self):
        """`created` is not the only thing worth saying. A plane whose status line moved
        makes the next clone REFRESH what is already there, and charter has written into
        the operator's repo again — the announcement is about the write, not about it
        being the first one."""
        commands._wire_clones(self.ws)
        _plane_settings(config.ROOT,
                        statusLine={"type": "command", "command": "charter frame"})
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            commands._wire_clones(self.ws)
        self.assertIn("svc", err.getvalue())
        self.assertIn("info/exclude", err.getvalue())

    def test_a_second_clone_into_the_same_workspace_says_nothing(self):
        """Every `workspace restore` re-clones what is already there. A line per checkout
        per run would be the announcement nobody reads by the time it matters."""
        commands._wire_clones(self.ws)
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            commands._wire_clones(self.ws)
        self.assertEqual(err.getvalue(), "")


class TheBlockDelimitersAreAnOnDiskContract(unittest.TestCase):
    """Spelled out here once, by hand.

    Every other reference in this file reads `workspace._EXCLUDE_BEGIN`, which agrees with
    any value that constant takes — the `retune-string` shape the deletion sweep keeps
    returning, and the same reason `TheMarkersNameIsPartOfTheOnDiskContract` spells out
    `.charter-generated` one file over.

    They are a real contract rather than decoration, and the file they are written in is
    the reason: the delimiters are how a LATER charter finds what an EARLIER one wrote in
    a repo charter does not own. Change the spelling and every checkout already wired gets
    a SECOND block appended beside the first, growing on every launch — the exact failure
    the block exists to prevent, happening in the operator's repository rather than in
    charter's, in a file they never look at.
    """

    def test_the_delimiters_are_spelled_out(self):
        self.assertEqual(
            workspace._EXCLUDE_BEGIN,
            "# >>> charter (generated layer — `charter workspace reinit`) >>>")
        self.assertEqual(workspace._EXCLUDE_END, "# <<< charter <<<")

    def test_the_note_makes_its_three_claims_to_the_person_who_finds_it(self):
        """The sentences are spelled out by hand, for `test_the_uninitialised_submodule_is
        _named`'s reason: a test built from the same constant agrees with any wording it
        takes, and the wording IS the deliverable here. This note is the only thing an
        operator who opens their own `info/exclude` has to go on — a block of paths with
        no explanation, in a file they did not write, is indistinguishable from something
        that should be deleted.

        Each claim is one the code keeps and this suite proves elsewhere: charter wrote
        them (`OwnershipIsLegible`), only what charter wrote is hidden
        (`AFileCharterDidNotWrite`), and nothing here can reach a teammate
        (`NothingShowsUpInTheirStatus`)."""
        note = workspace._EXCLUDE_NOTE
        self.assertIn("Files charter generated in this checkout so a chat here gets the "
                      "plane's layer.", note)
        self.assertIn("charter hides only what it wrote, never a directory of yours.",
                      note)
        self.assertIn("This file is per-checkout and never committed; nothing your "
                      "teammates clone is affected.", note)

    def test_every_line_charter_adds_that_is_not_a_path_is_a_comment(self):
        """`info/exclude` is a pattern file. A delimiter that is not a comment is a
        PATTERN, and `# >>> charter …` without its `#` would have git matching paths
        against charter's bookkeeping."""
        lines = [workspace._EXCLUDE_BEGIN, workspace._EXCLUDE_END,
                 *workspace._EXCLUDE_NOTE.splitlines()]
        for line in lines:
            self.assertTrue(line.startswith("#"), line)


if __name__ == "__main__":
    unittest.main()
