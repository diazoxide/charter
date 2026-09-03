"""A chat launched in `workspaces/<ws>/` gets charter's layer there — #850.

Claude Code reads project settings from the session's working directory and does not walk
up, so a chat whose cwd is a workspace directory loaded charter's plugin from nowhere, ran
no status line and had no `$CHARTER_HARNESS`. The fix is one generated file per workspace
and a marker saying charter wrote it.

Every test here writes only into a `PersonaIso` tmp plane. Nothing in this file may touch
the developer's real `workspaces/` — see `_planeguard` for what that has cost before.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_workspace, config, doctor, workspace
from charter.harness import base, claude_code, registry

from tests import _isolation


def _plane_settings(root: Path, **extra) -> Path:
    """The plane's own `.claude/settings.json`, shaped like this repo's committed one."""
    d = root / ".claude"
    d.mkdir(parents=True, exist_ok=True)
    p = d / "settings.json"
    doc = {
        "env": {"CHARTER_HARNESS": "claude-code"},
        "statusLine": {"type": "command", "command": "charter statusline",
                       "padding": 0, "refreshInterval": 10},
        "enabledPlugins": {"charter@charter": True},
    }
    doc.update(extra)
    p.write_text(json.dumps(doc, indent=2) + "\n")
    return p


class WorkspaceLayer(_isolation.PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        # The tripwire this whole file is written under. `PersonaIso` repoints every
        # derived path at a throwaway `edm-test-…` tree; if that ever stops being true,
        # every write below lands in somebody's real plane.
        self.assertIn("edm-test-", str(config.STATE_DIR))
        _plane_settings(config.ROOT, permissions={"allow": ["Bash(ls:*)"]})
        self.ws = "api"
        workspace.ensure(self.ws)

    def settings(self, name: str | None = None) -> Path:
        return workspace.workspace_dir(name or self.ws) / ".claude" / "settings.json"


class WhatIsWritten(WorkspaceLayer):
    def test_a_workspace_gets_the_planes_three_keys_and_only_those(self):
        """`enabledPlugins`, `statusLine` and `env` — the plugin alone loses the last two."""
        p = self.settings()
        self.assertTrue(p.is_file(), "workspaces/api/.claude/settings.json was not written")
        doc = json.loads(p.read_text())
        self.assertEqual(sorted(doc), ["enabledPlugins", "env", "statusLine"])
        self.assertEqual(doc["enabledPlugins"], {"charter@charter": True})
        self.assertEqual(doc["env"], {"CHARTER_HARNESS": "claude-code"})
        self.assertEqual(doc["statusLine"]["command"], "charter statusline")

    def test_the_planes_other_keys_stay_in_the_plane(self):
        """A `permissions` block is the plane's decision about the plane's own root."""
        self.assertNotIn("permissions", json.loads(self.settings().read_text()))

    def test_nothing_else_is_materialised_into_the_workspace(self):
        """Skills arrive with the plugin; agents walk up from here because this directory
        is inside the plane's git repo. A second copy of either would shadow the plugin's
        non-deterministically — Claude Code's own words are "is already taken by X, which
        takes precedence"."""
        wd = workspace.workspace_dir(self.ws)
        for unwanted in ("skills", "agents", ".claude/skills", ".claude/agents",
                         "CLAUDE.md", ".claude/CLAUDE.md"):
            self.assertFalse((wd / unwanted).exists(), f"{unwanted} should not be written")

    def test_the_layer_names_exactly_one_file(self):
        rows = workspace.harness_layer(self.ws)
        self.assertEqual([rel for rel, _status in rows], [".claude/settings.json"])


class NothingReachesAClone(WorkspaceLayer):
    def test_a_clone_inside_the_workspace_is_left_alone(self):
        """`workspaces/<ws>/<repo>/` is a repo charter does not own — `git add -A` there
        would stage whatever charter left behind."""
        clone = workspace.workspace_dir(self.ws) / "api-service"
        (clone / ".git").mkdir(parents=True)
        workspace.wire_harnesses(self.ws)
        self.assertFalse((clone / ".claude").exists())
        self.assertFalse((clone / workspace.GENERATED_MARKER).exists())

    def test_a_clone_directory_is_not_a_workspace_directory(self):
        clone = workspace.workspace_dir(self.ws) / "api-service"
        clone.mkdir(parents=True)
        self.assertTrue(workspace.is_workspace_dir(workspace.workspace_dir(self.ws)))
        self.assertFalse(workspace.is_workspace_dir(clone))
        self.assertFalse(workspace.is_workspace_dir(config.ROOT))


class OwnershipIsASidecar(WorkspaceLayer):
    def test_the_marker_records_what_charter_wrote(self):
        """Not a key inside the vendor's JSON — the same reason symlinking `.claude/` was
        wrong. `.charter-structure` is the precedent for a charter-owned marker here."""
        marker = workspace.workspace_dir(self.ws) / workspace.GENERATED_MARKER
        self.assertTrue(marker.is_file())
        self.assertIn(".claude/settings.json", json.loads(marker.read_text()))

    def test_a_hand_edited_file_is_reported_and_never_rewritten(self):
        p = self.settings()
        p.write_text('{"statusLine": {"type": "command", "command": "mine"}}\n')
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(rows[".claude/settings.json"], "foreign")
        self.assertIn("mine", p.read_text())

    def test_a_file_with_no_marker_at_all_is_foreign(self):
        """Charter cannot tell one it wrote before the marker existed from one somebody
        else wrote, and guessing wrong in that direction destroys work."""
        (workspace.workspace_dir(self.ws) / workspace.GENERATED_MARKER).unlink()
        self.settings().write_text('{"env": {"MINE": "1"}}\n')
        rows = dict(workspace.harness_layer(self.ws))
        self.assertEqual(rows[".claude/settings.json"], "foreign")

    def test_charters_own_file_is_refreshed_rather_than_left_stale(self):
        _plane_settings(config.ROOT, enabledPlugins={"charter@charter": True,
                                                     "other@market": True})
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(rows[".claude/settings.json"], "refreshed")
        self.assertIn("other@market", self.settings().read_text())

    def test_an_unchanged_file_is_present_and_not_rewritten(self):
        before = self.settings().stat().st_mtime_ns
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(rows[".claude/settings.json"], "present")
        self.assertEqual(self.settings().stat().st_mtime_ns, before)


class StalenessIsRegenerateAndCompare(WorkspaceLayer):
    def test_a_workspace_reads_stale_when_the_plane_moves(self):
        """Regenerate and compare, the way `persona lint --only stale` does — the
        generator's own wording drifts, so a stored diff would answer the wrong question."""
        _plane_settings(config.ROOT, statusLine={"type": "command",
                                                 "command": "charter statusline",
                                                 "padding": 0, "refreshInterval": 5})
        self.assertEqual(dict(workspace.harness_layer(self.ws))[".claude/settings.json"],
                         "stale")

    def test_reading_staleness_writes_nothing(self):
        _plane_settings(config.ROOT, env={"CHARTER_HARNESS": "claude-code", "X": "1"})
        before = self.settings().read_text()
        workspace.harness_layer(self.ws)
        self.assertEqual(self.settings().read_text(), before)

    def test_a_missing_file_reads_missing(self):
        self.settings().unlink()
        self.assertEqual(dict(workspace.harness_layer(self.ws))[".claude/settings.json"],
                         "missing")


class TheOtherHarnessesGetADeficit(WorkspaceLayer):
    def test_opencode_and_codex_declare_the_workspace_scope_ceiling(self):
        """Their config is global — Codex has no project-level config at all — so
        per-workspace divergence is not buildable for them. Silence would read as three
        ticks."""
        keyed = {h.name: [d for d in h.deficits if d.key == base.WORKSPACE_SCOPE]
                 for h in registry.all()}
        self.assertEqual(keyed["claude-code"], [])
        for name in ("opencode", "codex"):
            self.assertTrue(keyed[name], f"{name} must name the ceiling, not stay silent")
            self.assertTrue(keyed[name][0].detail)

    def test_a_harness_that_cannot_isolate_writes_nothing_into_a_workspace(self):
        gaps = dict(workspace.harness_deficits())
        self.assertEqual(sorted(gaps), ["codex", "opencode"])
        # And nothing of theirs landed in the workspace.
        wd = workspace.workspace_dir(self.ws)
        self.assertEqual(sorted(p.name for p in wd.iterdir() if p.name.startswith(".")),
                         sorted([".charter-structure", workspace.GENERATED_MARKER,
                                 ".claude"]))

    def test_every_registered_harness_either_carries_files_or_names_the_ceiling(self):
        """The rot this pins: a harness added later that answers neither would report a
        clean workspace it has never been checked in."""
        for h in registry.all():
            files = h.workspace_files()
            gap = [d for d in h.deficits if d.key == base.WORKSPACE_SCOPE]
            self.assertTrue(bool(files) != bool(gap),
                            f"{h.name} must return workspace files OR declare "
                            f"{base.WORKSPACE_SCOPE}, exactly one")


class ThePlaneRootIsUntouched(WorkspaceLayer):
    def test_wire_at_the_plane_root_still_writes_only_the_env_key(self):
        """`init`'s contract is unchanged: that file is user-owned and git-tracked, and
        the never-repair restraint on it is the whole reason the workspace file needed a
        marker of its own."""
        h = claude_code.ClaudeCodeHarness()
        rows = h.wire(config.ROOT)
        self.assertEqual([label for _s, label in rows], [".claude/settings.json (env)"])
        self.assertFalse((config.ROOT / workspace.GENERATED_MARKER).exists())
        self.assertFalse((config.ROOT / ".claude" / workspace.GENERATED_MARKER).exists())

    def test_the_planes_own_settings_are_not_rewritten(self):
        before = (config.ROOT / ".claude" / "settings.json").read_text()
        workspace.wire_harnesses(self.ws)
        self.assertEqual((config.ROOT / ".claude" / "settings.json").read_text(), before)


class APlaneWithNothingToMirror(_isolation.PersonaIso):
    def test_no_plane_settings_means_no_workspace_file_and_no_marker(self):
        self.assertIn("edm-test-", str(config.STATE_DIR))
        workspace.ensure("solo")
        wd = workspace.workspace_dir("solo")
        self.assertFalse((wd / ".claude").exists())
        self.assertEqual(workspace.harness_layer("solo"), [])

    def test_a_malformed_plane_settings_file_is_never_guessed_over(self):
        (config.ROOT / ".claude").mkdir(parents=True, exist_ok=True)
        (config.ROOT / ".claude" / "settings.json").write_text("{not json")
        workspace.ensure("solo")
        self.assertFalse((workspace.workspace_dir("solo") / ".claude").exists())


class ReinitRepairs(WorkspaceLayer):
    def test_reinit_puts_a_deleted_layer_back(self):
        self.settings().unlink()
        commands_workspace.cmd_workspace_reinit(SimpleNamespace(name=self.ws, all=False))
        self.assertTrue(self.settings().is_file())

    def test_reinit_refreshes_a_stale_layer(self):
        _plane_settings(config.ROOT, enabledPlugins={"charter@charter": True,
                                                     "later@market": True})
        commands_workspace.cmd_workspace_reinit(SimpleNamespace(name=self.ws, all=False))
        self.assertIn("later@market", self.settings().read_text())


class DoctorSaysSo(WorkspaceLayer):
    def setUp(self) -> None:
        super().setUp()
        # A real plane: the check refuses to say anything outside one, and the row this
        # class is about is the one an operator reads on a plane.
        _isolation.make_plane(self)
        _plane_settings(config.ROOT)
        self.ws = "api"
        workspace.ensure(self.ws)

    def test_a_current_workspace_reads_ok(self):
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.OK)
        self.assertEqual(r.name, "workspace layer")

    def test_a_stale_workspace_is_named_with_the_repair_command(self):
        _plane_settings(config.ROOT, env={"CHARTER_HARNESS": "claude-code", "Y": "2"})
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(self.ws, r.detail)
        self.assertIn("charter workspace reinit", r.hint)
        # `Result.render` writes the arrow. A hint carrying its own prints two, which is
        # what `check_workspace_clones` does and what this row must not learn from it.
        self.assertFalse(r.hint.startswith("→"), r.hint)

    def test_a_missing_layer_is_named(self):
        self.settings().unlink()
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(self.ws, r.detail)

    def test_a_hand_edited_layer_says_charter_will_not_touch_it(self):
        self.settings().write_text('{"env": {"MINE": "1"}}\n')
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("charter did not write", r.detail + r.hint)

    def test_the_row_names_the_harnesses_that_cannot_isolate(self):
        r = doctor.check_workspace_harness()
        self.assertIn("opencode", r.detail + r.hint)
        self.assertIn("codex", r.detail + r.hint)

    def test_the_check_is_in_the_preflight_and_in_its_pinned_names(self):
        self.assertIn("workspace layer", doctor.check_names())

    def test_the_check_writes_nothing(self):
        self.settings().unlink()
        doctor.check_workspace_harness()
        self.assertFalse(self.settings().exists())


class LaunchEnsuresTheBoundaryFirst(WorkspaceLayer):
    def test_launch_root_makes_the_workspace_rather_than_falling_back_to_the_plane(self):
        """Today a chat for a workspace with no directory records `workspace = <name>,
        cwd = <plane root>` — a disagreement created at launch."""
        from charter import commands_frame

        self.assertFalse(workspace.workspace_dir("fresh").exists())
        root = commands_frame._launch_root("fresh")
        self.assertEqual(Path(root), workspace.workspace_dir("fresh"))
        self.assertNotEqual(Path(root), Path(config.ROOT))

    def test_the_ensured_workspace_carries_the_layer(self):
        from charter import commands_frame

        commands_frame._launch_root("fresh")
        self.assertTrue((workspace.workspace_dir("fresh") / ".claude"
                         / "settings.json").is_file())

    def test_a_name_that_cannot_be_a_workspace_still_falls_back_to_the_plane(self):
        """`_launch_root` is on a launch path with no operator waiting — it degrades, it
        does not raise."""
        from charter import commands_frame

        self.assertEqual(Path(commands_frame._launch_root("..")), Path(config.ROOT))


class TheGeneratorIsOnePlace(WorkspaceLayer):
    def test_the_document_is_the_harnesss_own_and_is_written_verbatim(self):
        files = claude_code.ClaudeCodeHarness().workspace_files()
        self.assertEqual(sorted(files), [".claude/settings.json"])
        self.assertEqual(self.settings().read_text(), files[".claude/settings.json"])

    def test_the_marker_holds_a_digest_of_that_exact_text(self):
        files = claude_code.ClaudeCodeHarness().workspace_files()
        marker = json.loads(
            (workspace.workspace_dir(self.ws) / workspace.GENERATED_MARKER).read_text())
        self.assertEqual(marker[".claude/settings.json"],
                         workspace.content_digest(files[".claude/settings.json"]))


if __name__ == "__main__":  # pragma: no cover
    import unittest

    unittest.main()
