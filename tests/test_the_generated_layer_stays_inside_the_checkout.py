"""Charter's generated layer is written, replaced and removed only inside the checkout or
workspace it belongs to.

A guest repository charter clones into `workspaces/<ws>/<repo>/`, and a committed LIVE
workspace tree, are content charter did not write and must not trust with a path. By
committing a symlink, a directory symlink, or a `.charter-generated` marker naming an
outside path or holding the digest of an outside file, such a tree could once make charter
write, replace or unlink a file OUTSIDE the checkout it was wiring:

    * a `.charter-generated` that is a symlink — the marker publish followed it, creating a
      file (and its parent directories) where the link pointed, or replacing the existing
      file there with marker JSON;
    * a `.claude` or `.claude/agents` that is a directory symlink — every mirrored plane
      file was written through it into the outside directory;
    * a `.charter-generated` recording the digest of an outside file, beside a file symlink
      to it — charter read the digest back, called the file its own, and overwrote the
      outside target;
    * a `.charter-generated` whose key was absolute or climbed out with `..` — the withdraw
      pass unlinked the outside file it named and pruned its emptied directory;
    * a `workspaces/<ws>` that is itself a symlink out of the plane, or one whose
      `.charter-generated` is — the whole layer landed outside the plane.

Every case here builds a REAL git repo with a committed symlink in a `PersonaIso` tmp
plane, and the outside target lives in a `mkdtemp` of its own — by construction not under
the plane. The assertion is the same shape throughout: the outside target is untouched,
uncreated or undeleted, whatever the layer rows say. `git` identity and signing come from
`tests/_gitguard`, which redirects `$GIT_CONFIG_GLOBAL` for the whole suite.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from charter import config, workspace

from tests import _isolation
from tests.test_a_workspace_carries_charters_layer import _plane_settings


def _git(where, *args) -> subprocess.CompletedProcess:
    """`git` in *where*, inheriting this process's environment on purpose — `tests/_gitguard`
    has already pointed `$GIT_CONFIG_GLOBAL` at a file this package writes, so identity and
    `commit.gpgsign=false` are answered for every child (see `test_a_clone_gets_the_layer`)."""
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


def _commit(d: Path, msg: str = "hostile") -> None:
    _git(d, "add", "-A")
    _git(d, "commit", "-qm", msg)


class Containment(_isolation.PersonaIso):
    """A plane with settings and one workspace, plus an OUTSIDE directory a committed guest
    can try to reach."""

    def setUp(self) -> None:
        super().setUp()
        # The tripwire the layer suites are written under: if `PersonaIso` ever stops
        # repointing derived paths at a throwaway tree, every write below lands in a real
        # plane — and half of them in a repo, or a directory, charter does not own.
        self.assertIn("edm-test-", str(config.STATE_DIR))
        _plane_settings(config.ROOT, permissions={"ask": ["Bash(terraform apply *)"],
                                                  "deny": ["Bash(rm -rf /)"]})
        self.ws = "api"
        workspace.ensure(self.ws)
        agents = config.ROOT / ".claude" / "agents"
        agents.mkdir(parents=True, exist_ok=True)
        (agents / "steward.md").write_text("route the work\n")
        # By construction outside the plane: a mkdtemp is never under `config.ROOT`.
        self.outside = Path(tempfile.mkdtemp(prefix="edm-victim-")).resolve()
        self.addCleanup(shutil.rmtree, self.outside, ignore_errors=True)

    def wsdir(self, name: str | None = None) -> Path:
        return workspace.workspace_dir(name or self.ws)


class AMarkerThatIsASymlink(Containment):
    """gH / gH2: `.charter-generated` committed as a symlink. The marker publish resolved
    the link and wrote through it — creating the target and its parents where it pointed,
    or replacing the existing file there with marker JSON."""

    def test_a_dangling_marker_link_creates_nothing_outside(self):
        target = self.outside / "deep" / "dir" / "marker.json"  # nothing there yet
        clone = _repo(self.wsdir() / "svc")
        (clone / workspace.GENERATED_MARKER).symlink_to(target)
        _commit(clone)
        workspace.wire_guest(clone)
        self.assertFalse(target.exists(),
                         "charter created a file outside the checkout, through the link")
        self.assertFalse(target.parent.exists(),
                         "charter created directories outside the checkout")

    def test_an_existing_file_a_marker_link_points_at_is_not_replaced(self):
        victim = self.outside / "existing.rc"
        victim.write_text("the operator's own file\n")
        clone = _repo(self.wsdir() / "svc")
        (clone / workspace.GENERATED_MARKER).symlink_to(victim)
        _commit(clone)
        workspace.wire_guest(clone)
        self.assertEqual(victim.read_text(), "the operator's own file\n",
                         "charter replaced an outside file with its marker, through the link")


class ADirectoryThatIsASymlink(Containment):
    """gI / gJ: `.claude` or `.claude/agents` committed as a directory symlink. Every
    mirrored plane file was written through it into the outside directory."""

    def test_a_dot_claude_directory_link_receives_no_mirrored_files(self):
        dotclaude = self.outside / "dotclaude"
        dotclaude.mkdir()
        clone = _repo(self.wsdir() / "svc")
        (clone / ".claude").symlink_to(dotclaude)
        _commit(clone)
        workspace.wire_guest(clone)
        self.assertFalse((dotclaude / "settings.json").exists(),
                         "charter wrote settings through the .claude directory link")
        self.assertFalse((dotclaude / "agents" / "steward.md").exists(),
                         "charter mirrored an agent through the .claude directory link")

    def test_an_agents_directory_link_receives_no_mirrored_files(self):
        agents = self.outside / "agents"
        agents.mkdir()
        clone = _repo(self.wsdir() / "svc")
        (clone / ".claude").mkdir()
        (clone / ".claude" / "agents").symlink_to(agents)
        _commit(clone)
        workspace.wire_guest(clone)
        self.assertFalse((agents / "steward.md").exists(),
                         "charter mirrored an agent through the .claude/agents directory link")


class AMarkerVouchingForAnOutsideFile(Containment):
    """gK: a `.charter-generated` recording the sha256 of an outside file's content, beside a
    file symlink pointing at it. Charter read the digest back through the link, called the
    file its own, and overwrote the outside target with charter's copy."""

    def test_a_forged_digest_and_a_file_link_do_not_overwrite_the_outside_file(self):
        victim = self.outside / "existing.md"
        victim.write_text("the operator's own document\n")
        clone = _repo(self.wsdir() / "svc")
        (clone / ".claude" / "agents").mkdir(parents=True)
        (clone / ".claude" / "agents" / "steward.md").symlink_to(victim)
        (clone / workspace.GENERATED_MARKER).write_text(json.dumps(
            {".claude/agents/steward.md": workspace.content_digest(victim.read_text())}) + "\n")
        _commit(clone)
        workspace.wire_guest(clone)
        self.assertEqual(victim.read_text(), "the operator's own document\n",
                         "charter overwrote an outside file it was tricked into owning")


class AMarkerKeyThatClimbsOut(Containment):
    """gL: a `.charter-generated` whose key is absolute, or climbs out with `..`, naming an
    outside file whose digest it also records. The withdraw pass unlinked that file — the
    plane no longer declares it — and pruned its emptied directory. Marker keys were never
    validated."""

    def _clone_naming(self, key: str, victim: Path) -> Path:
        clone = _repo(self.wsdir() / "svc")
        (clone / workspace.GENERATED_MARKER).write_text(json.dumps(
            {key: workspace.content_digest(victim.read_text())}) + "\n")
        _commit(clone)
        return clone

    def test_a_dotdot_key_does_not_unlink_the_outside_file(self):
        # A victim just OUTSIDE the plane (a sibling of the plane root), so a bounded
        # `../../../..` chain from the clone genuinely reaches it — the shape gL used.
        victim = config.ROOT.parent / "victim-dotdot.txt"
        victim.write_text("a credential\n")
        self.addCleanup(victim.unlink, True)
        clone = self.wsdir() / "svc"
        clone.mkdir(parents=True)
        _git(clone.parent, "init", "-q", clone.name)
        (clone / "README.md").write_text("theirs\n")
        rel = os.path.relpath(victim, clone)   # a real `../../…` chain reaching the victim
        self.assertTrue(rel.startswith(".."), "fixture: the key must climb out")
        self.assertTrue((clone / rel).exists(),
                        "fixture: the marker key must actually reach the victim")
        (clone / workspace.GENERATED_MARKER).write_text(json.dumps(
            {rel: workspace.content_digest(victim.read_text())}) + "\n")
        _commit(clone)
        workspace.wire_guest(clone)
        self.assertTrue(victim.exists(), "charter unlinked an outside file a marker key named")
        self.assertEqual(victim.read_text(), "a credential\n")

    def test_an_absolute_key_does_not_unlink_the_outside_file(self):
        victim = self.outside / "secret.txt"
        victim.write_text("a credential\n")
        clone = self._clone_naming(str(victim), victim)
        workspace.wire_guest(clone)
        self.assertTrue(victim.exists(),
                        "charter unlinked an outside file an absolute marker key named")


class AGeneratedFileThatIsASymlink(Containment):
    """gA: a `.claude/settings.json` committed as a symlink out of the checkout. Charter
    wrote nothing outside — but it dropped the file SILENTLY, so the guest kept the plane's
    ask/deny rules out of a chat rooted there and no row said so."""

    def test_a_settings_symlink_is_named_not_silently_dropped(self):
        victim = self.outside / "elsewhere.json"
        victim.write_text('{"theirs": true}\n')
        clone = _repo(self.wsdir() / "svc")
        (clone / ".claude").mkdir()
        (clone / ".claude" / "settings.json").symlink_to(victim)
        _commit(clone)
        workspace.wire_guest(clone)
        self.assertEqual(victim.read_text(), '{"theirs": true}\n',
                         "charter wrote the plane's settings through the link")
        rels = [rel for rel, _status in workspace.guest_layer(clone)]
        self.assertIn(".claude/settings.json", rels,
                      "the escaping settings file was dropped with no row to name it")


class AMarkerThatGitTracks(Containment):
    """A committed `.charter-generated` naming a file INSIDE the checkout and recording its
    digest. Charter never writes a marker git tracks, so a tracked one is the guest's — and
    trusting its digests let it name the guest's own committed file as charter's and have the
    withdraw pass delete it, all inside the checkout where containment alone says nothing."""

    def test_a_tracked_marker_does_not_delete_an_in_checkout_file(self):
        # A harness-root path the plane does not declare, so the withdrawal's root filter
        # passes and only the tracked-marker guard keeps the file: a committed marker vouching
        # for the guest's own committed file would otherwise have it withdrawn.
        clone = _repo(self.wsdir() / "svc")
        keep = clone / ".claude" / "agents" / "oldpersona.md"
        keep.parent.mkdir(parents=True)
        keep.write_text("the guest's own file\n")
        (clone / workspace.GENERATED_MARKER).write_text(json.dumps(
            {".claude/agents/oldpersona.md": workspace.content_digest(keep.read_text())}) + "\n")
        _commit(clone)
        workspace.wire_guest(clone)
        self.assertTrue(keep.exists(),
                        "charter deleted an in-checkout file a tracked marker vouched for")
        self.assertEqual(keep.read_text(), "the guest's own file\n")


class ASiblingThatSharesTheCheckoutsNamePrefix(Containment):
    """A `.charter-generated` linked at a SIBLING whose name begins with the checkout's own
    (`svc` → `svc-evil`). Containment compares resolved paths with a trailing separator, so
    `.../svc-evil` is not read as being under `.../svc` — the write is refused, not aimed at
    the sibling."""

    def test_a_marker_link_to_a_name_prefixed_sibling_is_refused(self):
        clone = _repo(self.wsdir() / "svc")
        evil = self.wsdir() / "svc-evil"          # a sibling of the checkout, outside it
        evil.mkdir()
        (clone / workspace.GENERATED_MARKER).symlink_to(evil / "marker.json")
        _commit(clone)
        workspace.wire_guest(clone)
        self.assertFalse((evil / "marker.json").exists(),
                         "charter wrote into a sibling that shares the checkout's name prefix")


class AnEscapingMarkerKeyIsUntrusted(Containment):
    """A whole marker is dropped when any key climbs out. Left UNcommitted so the tracked
    guard does not mask this; the observable is the exclude block, which is independent of the
    per-removal `_inside` guards: charter adds no line for a key it will not act on."""

    def _wire_with_key(self, key: str) -> str:
        sibling = self.wsdir() / "victim.txt"   # inside the plane, outside the checkout
        sibling.write_text("x\n")
        self.addCleanup(sibling.unlink, True)
        clone = _repo(self.wsdir() / "svc")   # README committed; the marker stays untracked
        target = str(sibling) if os.path.isabs(key) else key
        digest = workspace.content_digest(sibling.read_text())
        (clone / workspace.GENERATED_MARKER).write_text(json.dumps({target: digest}) + "\n")
        workspace.wire_guest(clone)
        exclude = workspace.git_exclude_file(clone)
        return exclude.read_text() if exclude and exclude.exists() else ""

    def test_a_dotdot_key_adds_no_exclude_line(self):
        self.assertNotIn("victim.txt", self._wire_with_key("../victim.txt"),
                         "charter hid a `..` key a hostile marker named")

    def test_an_absolute_key_adds_no_exclude_line(self):
        self.assertNotIn("victim.txt", self._wire_with_key(str(self.wsdir() / "victim.txt")),
                         "charter hid an absolute key a hostile marker named")


class AMarkerNamingAFileUnderADirectoryLink(Containment):
    """The half a `..`/absolute key check misses: a CLEAN key whose parent is a committed
    directory symlink out of the checkout. `_read_marker_at` accepts the key, so only the
    per-removal `_inside` guard (`_withdraw`, `unwire_guest`) stands between the withdraw and
    the outside file it reaches through the link. The marker is left UNcommitted so the
    tracked-marker guard does not mask this one."""

    def _clone(self) -> tuple[Path, Path]:
        # `.claude` — a HARNESS ROOT — committed as a directory link out, with the victim under
        # it. The key's root is `.claude`, so the withdrawal's root filter passes and the
        # per-removal `_inside` guard is the only thing that keeps the unlink out of the link.
        outside_claude = self.outside / "dotclaude"
        (outside_claude / "agents").mkdir(parents=True)
        victim = outside_claude / "agents" / "keep.txt"
        victim.write_text("the operator's file\n")
        clone = _repo(self.wsdir() / "svc")
        (clone / ".claude").symlink_to(outside_claude)   # committed directory link, out
        _commit(clone)
        # Untracked marker: recognised as charter's record, its clean harness-root key accepted.
        (clone / workspace.GENERATED_MARKER).write_text(json.dumps(
            {".claude/agents/keep.txt": workspace.content_digest(victim.read_text())}) + "\n")
        return clone, victim

    def test_a_wire_does_not_withdraw_through_a_directory_link(self):
        clone, victim = self._clone()
        workspace.wire_guest(clone)
        self.assertTrue(victim.exists(),
                        "charter withdrew an outside file reached through a directory link")

    def test_unwire_does_not_unlink_through_a_directory_link(self):
        clone, victim = self._clone()
        workspace.unwire_guest(clone)
        self.assertTrue(victim.exists(),
                        "unwire unlinked an outside file reached through a directory link")


class AGeneratedFileLinkWhoseTargetIsTrusted(Containment):
    """The row status `_layer_status` gives an escaping leaf link when the marker (untracked,
    so trusted) records the digest of what is on the far end: `foreign`, not `stale`. Without
    the containment check the digest would match and charter would call the outside file its
    own to rewrite."""

    def test_a_link_whose_recorded_digest_matches_is_foreign_not_stale(self):
        victim = self.outside / "existing.md"
        victim.write_text("the operator's document\n")
        clone = _repo(self.wsdir() / "svc")
        (clone / ".claude" / "agents").mkdir(parents=True)
        (clone / ".claude" / "agents" / "steward.md").symlink_to(victim)
        _commit(clone)
        (clone / workspace.GENERATED_MARKER).write_text(json.dumps(   # untracked, trusted
            {".claude/agents/steward.md": workspace.content_digest(victim.read_text())}) + "\n")
        row = dict(workspace.guest_layer(clone))
        self.assertEqual(row.get(".claude/agents/steward.md"), "foreign",
                         "an escaping link with a matching digest read as charter's to rewrite")
        self.assertEqual(victim.read_text(), "the operator's document\n")


class AStructureMarkerThatIsASymlink(Containment):
    """`.charter-structure` committed as a symlink out of an otherwise-legitimate workspace
    directory. `scaffold` stamps the version through `contain.write_refusal`, so the link is
    refused and the outside target is never written."""

    def test_scaffold_does_not_stamp_through_the_link(self):
        victim = self.outside / "structure-target"
        victim.write_text("the operator's file\n")
        marker = self.wsdir() / ".charter-structure"
        marker.unlink(missing_ok=True)
        marker.symlink_to(victim)
        workspace.scaffold(self.ws)
        self.assertEqual(victim.read_text(), "the operator's file\n",
                         "scaffold stamped its version through the .charter-structure link")


class AWorkspaceDirectoryMarkerThatIsALink(Containment):
    """w2: the workspace directory's own `.charter-generated` committed as a symlink. When a
    generated file goes stale and the marker is republished, the publish followed the link
    and wrote outside."""

    def test_the_republished_marker_is_not_written_through_the_link(self):
        settings = self.wsdir() / ".claude" / "settings.json"
        outside_marker = self.outside / "w2.json"
        outside_marker.write_text(json.dumps(
            {".claude/settings.json": workspace.content_digest(settings.read_text())}) + "\n")
        before = outside_marker.read_text()
        m = self.wsdir() / workspace.GENERATED_MARKER
        m.unlink(missing_ok=True)
        m.symlink_to(outside_marker)
        # The plane moves on, so `settings.json` is now stale and the marker must be
        # republished — the moment the publish followed the link.
        _plane_settings(config.ROOT, permissions={"ask": ["Bash(new rule *)"]})
        workspace.wire_harnesses(self.ws)
        self.assertEqual(outside_marker.read_text(), before,
                         "charter republished the workspace marker through the link")


class AForgedDigestForAnOrdinaryFile(Containment):
    """A marker charter trusts (untracked, so not the clone-delivered tracked kind, and a clean
    in-checkout key) recording an ordinary file's own digest under NO harness root. Charter
    withdraws only under its own generated roots, so the file is never deleted whatever digest
    the marker records — the boundary that does not depend on the marker being distrusted."""

    def test_a_forged_digest_does_not_withdraw_an_ordinary_file(self):
        clone = _repo(self.wsdir() / "svc")
        keep = clone / "keep.txt"
        keep.write_text("the operator's file\n")
        (clone / workspace.GENERATED_MARKER).write_text(json.dumps(
            {"keep.txt": workspace.content_digest(keep.read_text())}) + "\n")  # untracked, trusted
        workspace.wire_guest(clone)
        self.assertTrue(keep.exists(),
                        "charter withdrew an ordinary in-checkout file a forged marker named")

    def test_a_forged_digest_does_not_withdraw_the_git_exclude(self):
        clone = _repo(self.wsdir() / "svc")
        excl = clone / ".git" / "info" / "exclude"
        excl.parent.mkdir(parents=True, exist_ok=True)
        excl.write_text("# theirs\n")
        (clone / workspace.GENERATED_MARKER).write_text(json.dumps(
            {".git/info/exclude": workspace.content_digest(excl.read_text())}) + "\n")
        workspace.wire_guest(clone)
        self.assertTrue(excl.exists(),
                        "charter withdrew .git/info/exclude a forged marker named")


class AWorkspaceChildThatLinksToAnOutsideRepo(Containment):
    """A `workspaces/<ws>/<name>` committed (or planted) as a symlink to a git repository
    outside the plane. `guest_trees` follows it, so a launch / clone / reinit would wire it —
    writing charter's layer and an exclude block into a repo that is not under the plane at
    all. Every guest tree charter wires, unwires or reports must itself resolve inside
    `workspaces/`; one that does not is named and never wired."""

    def test_wire_harnesses_writes_nothing_into_the_outside_repo(self):
        evil = _repo(self.outside / "evilrepo")
        (evil / "keep.txt").write_text("mine\n")
        _commit(evil)
        (self.wsdir() / "svc").symlink_to(evil)   # a workspace child pointing out
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertFalse((evil / ".claude").exists(),
                         "charter wrote its layer into a git repo outside the plane")
        excl = evil / ".git" / "info" / "exclude"
        self.assertNotIn(workspace._EXCLUDE_BEGIN, excl.read_text() if excl.exists() else "",
                         "charter wrote its exclude block into an outside repo")
        self.assertTrue(any(rel.startswith("svc/") for rel in rows),
                        "the escaping tree was neither wired nor named")

    def test_the_escaping_tree_is_named_in_the_layer(self):
        evil = _repo(self.outside / "evilrepo")
        (evil / ".claude" / "agents").mkdir(parents=True)
        (evil / ".claude" / "agents" / "steward.md").write_text("theirs\n")
        _commit(evil)
        (self.wsdir() / "svc").symlink_to(evil)
        rows = dict(workspace.harness_layer(self.ws))
        # Named by the guard's single row — not by reading the outside tree's own files, which
        # is what happens if the guard is gone.
        self.assertEqual(rows.get("svc/.charter-generated"), "foreign",
                         "the escaping child was not named as foreign by the layer")
        self.assertNotIn("svc/.claude/agents/steward.md", rows,
                         "doctor read the layer through a tree that leaves the plane")

    def test_unwire_removes_nothing_from_the_outside_repo(self):
        evil = _repo(self.outside / "evilrepo")
        theirs = evil / ".claude" / "agents" / "steward.md"
        theirs.parent.mkdir(parents=True)
        theirs.write_text("their own agent\n")
        _commit(evil)
        # An UNtracked marker recording that file's digest, so a wire-then-unwire would treat
        # it as charter's to remove; only the per-tree guard keeps unwire out of this repo.
        (evil / workspace.GENERATED_MARKER).write_text(json.dumps(
            {".claude/agents/steward.md": workspace.content_digest(theirs.read_text())}) + "\n")
        (self.wsdir() / "svc").symlink_to(evil)
        workspace.unwire_guests(self.ws)
        self.assertTrue(theirs.exists(),
                        "unwire deleted a file from a git repo outside the plane")


class AWorkspaceDirectoryThatIsALink(Containment):
    """w9: `workspaces/<ws>` is itself a symlink out of the plane. The whole layer — the
    settings, the marker — landed in the directory the link pointed at."""

    def test_no_layer_is_written_outside_the_plane(self):
        target = self.outside / "w9real"
        target.mkdir()
        link = config.WORKSPACES_DIR / "w9"
        link.symlink_to(target)
        workspace.wire_harnesses("w9")
        self.assertFalse((target / ".claude").exists(),
                         "charter wrote the layer into a workspace directory outside the plane")
        self.assertFalse((target / workspace.GENERATED_MARKER).exists(),
                         "charter wrote its marker into a workspace directory outside the plane")

    def test_scaffold_writes_no_structure_marker_outside_the_plane(self):
        target = self.outside / "w9real"
        target.mkdir()
        link = config.WORKSPACES_DIR / "w9"
        link.symlink_to(target)
        workspace.scaffold("w9")
        self.assertFalse((target / ".charter-structure").exists(),
                         "scaffold stamped its structure marker outside the plane")
        self.assertFalse((target / "workspace.md").exists(),
                         "scaffold wrote a workspace charter outside the plane")


if __name__ == "__main__":
    unittest.main()
