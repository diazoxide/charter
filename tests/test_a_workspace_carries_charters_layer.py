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

    def test_reinit_says_what_it_DID_rather_than_what_it_found(self):
        """A `.claude` that is a file cannot be made into a directory, and charter never
        deletes or renames existing content. Reading the pre-state would have printed
        "wrote it" over a write that never happened — and a tick is what stops you
        checking."""
        import shutil

        shutil.rmtree(workspace.workspace_dir(self.ws) / ".claude")
        (workspace.workspace_dir(self.ws) / ".claude").write_text("not a directory\n")
        rows = dict(workspace.reinit(self.ws)["layer"])
        self.assertEqual(rows[".claude/settings.json"], "blocked")
        self.assertEqual((workspace.workspace_dir(self.ws) / ".claude").read_text(),
                         "not a directory\n")

    def test_a_current_layer_is_not_announced(self):
        """Idempotent means quiet: a second `reinit` has nothing to say about the layer."""
        commands_workspace.cmd_workspace_reinit(SimpleNamespace(name=self.ws, all=False))
        self.assertEqual(workspace.reinit(self.ws)["layer"], [])


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


class TheCeilingReportedIsTheOneAboutWorkspaces(WorkspaceLayer):
    """`harness_deficits` filters on `WORKSPACE_SCOPE`, and Codex is why.

    Codex declares three deficits — `status-bar`, `session-lock` and `WORKSPACE_SCOPE` —
    so `next(...)` without the key test returns whichever is first. The existing case
    asserts *which harnesses* have a ceiling; nothing asserted *which ceiling*, which is
    why the sweep found the filter as a survivor.

    The consequence is a sentence rather than a crash, which is why it would have survived
    a reader too: `check_workspace_harness` renders the match as
    *"(codex: config is machine-global, so a workspace cannot diverge)"*. Drop the filter
    and that explanation is attached to Codex's **status bar** — a true fact about a
    different limitation, printed as the reason a workspace cannot hold its own config.
    """

    def test_the_deficit_returned_is_the_workspace_one_not_merely_the_first(self):
        got = dict(workspace.harness_deficits())
        self.assertIn("codex", got)
        self.assertEqual(got["codex"].key, base.WORKSPACE_SCOPE)
        self.assertNotEqual(got["codex"].key, "status-bar")

    def test_a_harness_declaring_other_ceilings_only_is_not_reported_here(self):
        """The filter's other half: a harness with deficits but none about workspaces has
        no ceiling to name, and must not be listed as unable to diverge."""
        only_other = SimpleNamespace(
            name="probe", deficits=(base.Deficit("status-bar", "d", "r"),))
        real = claude_code.ClaudeCodeHarness()
        with mock.patch.object(registry, "all", return_value=[only_other, real]):
            self.assertEqual(workspace.harness_deficits(), [])
class AMarkerCharterCannotReadIsNotAMarker(WorkspaceLayer):
    """`_read_marker` — three ways the sidecar fails, and they must all mean the same thing.

    The marker answers *did charter write this file?*, so a marker charter cannot read has
    to answer **no**, and `harness_layer` must then call the file `foreign` and leave it.
    Getting that backwards is the destructive direction: a wrong *yes* overwrites work.

    Three failures the sweep found unpinned. **Missing** is the ordinary case and is
    covered elsewhere. **Unparseable** and **not-a-dict** are different: a truncated write
    leaves bytes that are not JSON, and a file whose top level is a list or a string parses
    fine and then answers nothing to `marker.get(rel)`. Without the `isinstance` that is an
    `AttributeError` out of a reader whose whole contract is to degrade.

    The `except` is narrow on purpose — `check_memory_indexes`' recorded reason is that a
    broad catch here once swallowed a `NameError` and reported OK.
    """

    def _marker(self):
        return workspace.workspace_dir(self.ws) / workspace.GENERATED_MARKER

    def test_bytes_that_are_not_json_read_as_no_marker(self):
        self._marker().write_text("{ truncated")
        self.assertEqual(workspace._read_marker(self.ws), {})

    def test_bytes_that_are_not_text_read_as_no_marker(self):
        self._marker().write_bytes(b"\xff\xfe not utf-8")
        self.assertEqual(workspace._read_marker(self.ws), {})

    def test_valid_json_that_is_not_an_object_reads_as_no_marker(self):
        for doc in ("[]", '"a string"', "3", "null"):
            self._marker().write_text(doc)
            self.assertEqual(workspace._read_marker(self.ws), {}, doc)

    def test_an_edited_file_whose_marker_cannot_be_read_is_foreign_and_survives(self):
        """The consequence, not the return value. `ok` is decided by CONTENT equality, so
        an unreadable marker changes nothing while the file still matches — the marker only
        separates `stale` (charter's, the plane moved) from `foreign` (somebody else's).
        Edit the file AND break the marker and charter must choose `foreign`, because the
        wrong answer here overwrites work."""
        self.settings().write_text("{ \"env\": {} }")
        self._marker().write_text("[]")
        rows = dict(workspace.harness_layer(self.ws))
        self.assertEqual(rows[".claude/settings.json"], "foreign")
        before = self.settings().read_text()
        workspace.wire_harnesses(self.ws)
        self.assertEqual(self.settings().read_text(), before,
                         "charter overwrote a file it could not vouch for")

    def test_a_file_charter_cannot_read_says_so_rather_than_guessing(self):
        """The `unreadable` status — the FILE, not the marker. Bytes that are not text
        cannot be compared against what charter would write, and `write_layer` treats it
        exactly as `foreign`: left alone."""
        self.settings().write_bytes(b"\xff\xfe not utf-8")
        rows = dict(workspace.harness_layer(self.ws))
        self.assertEqual(rows[".claude/settings.json"], "unreadable")
        before = self.settings().read_bytes()
        workspace.wire_harnesses(self.ws)
        self.assertEqual(self.settings().read_bytes(), before,
                         "charter overwrote bytes it could not read")
class OneMisbehavingHarnessDoesNotEmptyTheLayer(WorkspaceLayer):
    """`h.workspace_files() or {}` — the fallback, and why it is tolerance rather than a
    guard against something that happens.

    No registered harness can reach it: `base.Harness.workspace_files` answers `{}` by
    default and neither opencode nor Codex overrides it, so `None` requires a harness that
    violates its own annotation. The sweep found the `or {}` as a survivor for that reason.

    **It is kept because `_layer_files` iterates the whole registry.** A harness answering
    `None` is a bug in that harness; without the fallback it is a `TypeError` that empties
    the layer for **every** workspace and every other harness, so one broken integration
    takes charter's own files down with it. `registry.deficits`' complaint is that an empty
    dict and a ceiling are opposite facts — this is the third: a *wrong* answer, which must
    cost only its own harness.

    Charter still fails loudly about it elsewhere:
    `test_every_registered_harness_either_carries_files_or_names_the_ceiling` is what goes
    red the day a registered harness answers neither.
    """

    def test_a_harness_answering_none_costs_only_itself(self):
        broken = SimpleNamespace(workspace_files=lambda: None)
        real = claude_code.ClaudeCodeHarness()
        with mock.patch.object(registry, "all", return_value=[broken, real]):
            files = workspace._layer_files(self.ws)
        self.assertEqual(sorted(files), [".claude/settings.json"],
                         "a harness returning None emptied the whole layer")
class WhatThePlaneHasToOfferIsAskedThreeWays(WorkspaceLayer):
    """`ClaudeCodeHarness.workspace_files` — the three states that answer nothing.

    All three were sweep survivors, and they are not one question: *no settings file at
    all*, *a settings file charter cannot parse*, and *a settings file with none of the
    three mirrored keys* are different planes, and the middle one is the one charter must
    not guess over. Its own docstring is the specification — *"Empty here means the
    workspace gets no file and no marker at all, which is the honest rendering of 'there is
    nothing to mirror'; writing an empty `{}` would look like a layer."*

    The `k in settings` filter is the third: without it a plane declaring one of the three
    keys would carry the other two as absent entries, and a workspace would hold a document
    asserting something the plane never said.
    """

    def test_a_plane_with_no_settings_file_offers_nothing(self):
        (config.ROOT / ".claude" / "settings.json").unlink()
        self.assertEqual(claude_code.ClaudeCodeHarness().workspace_files(), {})

    def test_a_plane_whose_settings_cannot_be_parsed_offers_nothing(self):
        (config.ROOT / ".claude" / "settings.json").write_text("{ not json")
        self.assertEqual(claude_code.ClaudeCodeHarness().workspace_files(), {},
                         "charter guessed over a file somebody is holding")

    def test_a_plane_declaring_none_of_the_three_keys_offers_nothing(self):
        (config.ROOT / ".claude" / "settings.json").write_text(json.dumps(
            {"permissions": {"allow": ["Bash(ls:*)"]}}))
        self.assertEqual(claude_code.ClaudeCodeHarness().workspace_files(), {},
                         "an empty document would look like a layer")

    def test_only_the_keys_the_plane_actually_declares_travel(self):
        (config.ROOT / ".claude" / "settings.json").write_text(json.dumps(
            {"env": {"CHARTER_HARNESS": "claude-code"},
             "permissions": {"allow": ["Bash(ls:*)"]}}))
        files = claude_code.ClaudeCodeHarness().workspace_files()
        doc = json.loads(next(iter(files.values())))
        self.assertEqual(list(doc), ["env"])
        self.assertNotIn("statusLine", doc)
        self.assertNotIn("permissions", doc)
class TheBoundaryAsksTheNameFirst(WorkspaceLayer):
    """`is_workspace_dir` — the parent comparison alone is not the boundary.

    Its own docstring names the case and nothing tested it, which is why the sweep found
    the `valid_name` guard as a survivor: ``WORKSPACES_DIR / ".."`` **is the plane root**,
    and its parent is `WORKSPACES_DIR`, so a comparison alone answers *yes* for the one
    directory the whole mechanism exists to keep its hands off. The plane root's
    `.claude/settings.json` is user-owned and git-tracked; generating into it is the
    failure `commands._ensure_statusline`'s never-repair restraint is written against.
    """

    def test_the_plane_root_reached_as_dot_dot_is_not_a_workspace(self):
        self.assertFalse(workspace.is_workspace_dir(config.WORKSPACES_DIR / ".."))

    def test_a_name_that_cannot_be_a_workspace_is_refused_beside_real_ones(self):
        self.assertTrue(workspace.is_workspace_dir(config.WORKSPACES_DIR / self.ws))
        for bad in ("..", ".", ".hidden"):
            self.assertFalse(workspace.is_workspace_dir(config.WORKSPACES_DIR / bad), bad)
class TheRowsQuietStates(WorkspaceLayer):
    """The states of `check_workspace_harness` that say **nothing is wrong**.

    `DoctorSaysSo` covers the three that report a problem. The sweep returned these as
    survivors, which is the usual asymmetry: the failing paths get cases because somebody
    was fixing a failure, and the reassuring ones are believed. A row that answers OK for
    the wrong reason is the harder defect, because nobody looks at it again.

    **This class writes a `charter.toml` and re-derives, and that is the whole reason the
    survivors survived.** `tests/_isolation.py` says it outright — *"`PersonaIso` hands
    every case a root; it deliberately does not put a `charter.toml`"* — so
    `HAS_CONTROL_PLANE` is False for every case in this module and the row returns
    ``no control plane found`` before reaching a single line below it. Dropping that guard
    changes nothing a test can see, because no test was ever past it.
    """

    def setUp(self) -> None:
        super().setUp()
        (Path(config.ROOT) / "charter.toml").write_text("schema = 1\n")
        prev = config.use(Path(config.ROOT))
        self.addCleanup(config.restore, prev)
        self.assertTrue(config.HAS_CONTROL_PLANE,
                        "fixture did not give the row a plane — every case below would "
                        "pass through the short-circuit and assert nothing")

    def test_outside_a_plane_the_row_says_so_and_checks_nothing(self):
        with mock.patch.object(config, "HAS_CONTROL_PLANE", False):
            r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.OK)
        self.assertEqual(r.detail, "no control plane found")

    def test_a_current_plane_counts_what_it_mirrors(self):
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("generated file(s) across all workspaces, all current", r.detail)
        self.assertNotIn("nothing to mirror", r.detail)

    def test_the_deficit_aside_names_the_harnesses_that_cannot_diverge(self):
        r = doctor.check_workspace_harness()
        self.assertIn("config is machine-global, so a workspace cannot diverge — "
                      "charter harness list", r.detail)

    def test_a_plane_whose_harnesses_can_all_isolate_gets_no_aside(self):
        """The `else` half, which no registry this repository ships can reach: opencode and
        Codex both declare the ceiling, so `gaps` is never empty and the aside is always
        rendered. That makes the empty string look like dead code — it is not, it is the
        answer the day a harness gains per-workspace config, and it is the difference
        between a clean row and one carrying a trailing parenthetical about nothing."""
        with mock.patch.object(workspace, "harness_deficits", return_value=[]):
            r = doctor.check_workspace_harness()
        self.assertNotIn("machine-global", r.detail)
        self.assertNotIn("  (", r.detail, "an empty aside still printed its brackets")

    def test_a_plane_with_nothing_to_mirror_says_so_rather_than_counting_zero(self):
        """`if not total` — the branch whose deletion the suite could not see.

        Falling through renders the sentence below it with the number it has, and
        ``0 generated file(s) across all workspaces, all current`` is a true count wearing
        the wrong row: it reports charter's files as in place and current when there are
        no files, and no workspace holds one. Both states are green and they are not the
        same fact — the second says the plane declares nothing worth mirroring, which is
        the only one of the two an operator can do anything about, and `reinit` is not
        what does it.
        """
        (config.ROOT / ".claude" / "settings.json").write_text(json.dumps(
            {"permissions": {"allow": ["Bash(ls:*)"]}}))
        self.assertTrue(workspace.list_workspaces(),
                        "no workspace at all — the count below would be zero either way")
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("nothing to mirror — the plane declares no plugin, status line or "
                      "env of its own", r.detail)
        self.assertNotIn("generated file(s)", r.detail)


class TheDetailNamesFourFindingsAndThenSaysSo(WorkspaceLayer):
    """`", ".join(findings[:4]) + (", …" if len(findings) > 4 else "")` — the truncation,
    pinned at its boundary and not only in its direction.

    Two survivors sat on this one expression and the sweep read them together, because
    either alone can be written off: a case with five findings pins the ellipsis but not
    where the cut is — `len(findings) >= 4` renders it there too — and a case with four
    pins the cut but not that the ellipsis ever appears. So both counts are here. FOUR is
    the last that fits whole; FIVE is the first that is abbreviated.

    **The count is the row's budget, not a detail.** This is one doctor line an operator
    reads in a terminal, `[:4]` is what fits on it, and the two failures either side are
    both silent: a boundary one low prints `…` over findings that were all named, and the
    ellipsis lost means a fifth broken workspace is dropped from the row with nothing
    saying anything was left out.
    """

    def setUp(self) -> None:
        super().setUp()
        # A real plane, for `DoctorSaysSo`'s reason: without one the row short-circuits on
        # `HAS_CONTROL_PLANE` and every count below is measured against a row that never
        # looked at a workspace.
        _isolation.make_plane(self)
        _plane_settings(config.ROOT)
        workspace.ensure(self.ws)

    def _detail_for(self, n: int) -> str:
        """The row's detail with exactly *n* findings — *n* workspaces missing their file.

        `self.ws` stays current on purpose: it counts towards `total` and contributes no
        finding, so the number under test is the number of things WRONG rather than the
        number of workspaces.
        """
        for i in range(n):
            ws = f"w{i}"
            workspace.ensure(ws)
            (workspace.workspace_dir(ws) / ".claude" / "settings.json").unlink()
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertEqual(r.detail.count("(missing)"), min(n, 4),
                         f"asked for {n} findings and the row named a different number")
        return r.detail

    def test_four_findings_are_all_named_and_nothing_says_there_are_more(self):
        detail = self._detail_for(4)
        for i in range(4):
            self.assertIn(f"w{i}/.claude/settings.json (missing)", detail)
        self.assertNotIn("…", detail,
                         "four fit — the row promised a fifth finding it does not have")

    def test_a_fifth_finding_is_abbreviated_rather_than_dropped_in_silence(self):
        detail = self._detail_for(5)
        self.assertTrue(detail.endswith(", …"),
                        f"a finding was dropped with nothing saying so: {detail}")


class ARowThatCouldNotRunSaysSoRatherThanEndingTheDoctor(WorkspaceLayer):
    """`except (OSError, ValueError)` around the whole workspace walk — both members.

    Neither is theoretical, and what they cost is not this row. `check_workspace_harness`
    runs from the SessionStart hook alongside every other check: an exception out of it is
    not a missing row, it is every row after it never printed, on a path the operator did
    not ask for and cannot see.

    **OSError is the operating system's.** `list_workspaces` walks `workspaces/`, and a
    directory it cannot read raises there. It is asked with a stub rather than a `chmod`
    deliberately: `chmod` is a no-op for root, root is who a container may run this suite
    as, and a guard whose test passes for the wrong reason on somebody's machine is the
    exact failure this file's tripwire exists for.

    **ValueError is a registered harness's.** The walk reaches every harness's
    `workspace_files()` through `harness_layer`, and
    `OneMisbehavingHarnessDoesNotEmptyTheLayer` already settles what charter owes a
    third-party integration that misbehaves: one broken harness costs its own answer and
    not everybody else's. This is that rule applied one level up — it must not cost the
    doctor either.
    """

    def setUp(self) -> None:
        super().setUp()
        _isolation.make_plane(self)
        _plane_settings(config.ROOT)
        workspace.ensure(self.ws)

    def test_a_workspaces_directory_that_cannot_be_read_is_reported_not_checked(self):
        with mock.patch.object(workspace, "list_workspaces",
                               side_effect=OSError("workspaces/ is unreadable")):
            r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertEqual(r.detail, "not checked (workspaces/ is unreadable)")
        self.assertEqual(r.hint, doctor._NOT_CHECKED_HINT)

    def test_a_harness_that_raises_costs_its_own_answer_and_not_the_doctor(self):
        def _boom():
            raise ValueError("this harness cannot say")

        broken = SimpleNamespace(name="broken", deficits=[], workspace_files=_boom)
        with mock.patch.object(registry, "all", return_value=[broken]):
            r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertEqual(r.detail, "not checked (this harness cannot say)")


class APathThatWouldLeaveTheWorkspaceIsNeverWritten(WorkspaceLayer):
    """`if wd.resolve() not in target.parents: continue` — asked with a path that escapes.

    `_layer_files`' own docstring says why the clause is there — *"the harness contract
    says paths stay inside; a contract nothing enforces is a comment, and this is the one
    place every harness's answer passes through"* — and nothing put a path through it that
    the contract forbids. Every registered harness names `.claude/settings.json`, which
    is inside, so deleting the whole guard left the suite green and the sweep said so.

    `AWorkspaceRootReachedThroughASymlink` pins the `.resolve()` inside the clause from
    the other side: drop it and the guard fires for EVERY file. That is the case where the
    clause wrongly refuses. This is the case it exists for, and the two together are why
    it is one clause with two normalised sides rather than either alone.

    **What it costs is not a stray file.** `wire_harnesses` writes what `_layer_files`
    returns, so a `rel` climbing out of the workspace is charter generating into a
    directory it never claimed — the plane root's own `.claude/settings.json` is one `..`
    away, git-tracked and the operator's, and `commands._ensure_statusline`'s never-repair
    restraint is written against exactly that write.
    """

    def test_a_relative_path_climbing_out_of_the_workspace_is_dropped(self):
        rogue = SimpleNamespace(workspace_files=lambda: {"../escaped.json": "{}\n"})
        real = claude_code.ClaudeCodeHarness()
        with mock.patch.object(registry, "all", return_value=[rogue, real]):
            files = workspace._layer_files(self.ws)
            rows = workspace.wire_harnesses(self.ws)
        self.assertEqual(sorted(files), [".claude/settings.json"],
                         "a path outside the workspace was accepted into the layer")
        self.assertEqual([rel for rel, _ in rows], [".claude/settings.json"])
        escaped = workspace.workspace_dir(self.ws).parent / "escaped.json"
        self.assertFalse(escaped.exists(),
                         "charter generated a file outside the workspace it was wiring")


class TheRowsComeBackInOneOrder(WorkspaceLayer):
    """`sorted(_layer_files(name).items())` — path order, not registry order.

    `list` passes every test this suite had, because the one harness that carries files
    carries exactly one. The sweep found the `sorted` as a survivor for that reason, and
    it is not cosmetic: `_layer_files` merges the whole registry into one dict, so without
    the sort the rows arrive in whatever order harnesses happen to have been registered
    in — which `registry.all` is free to change and no caller of this function knows.

    Two readers depend on the order being the path's. `doctor.check_workspace_harness`
    prints the first four findings and abbreviates the rest, so registry order decides
    WHICH four an operator is shown; `cmd_workspace_reinit` prints one line per file, and
    a repair whose report reshuffles between runs cannot be diffed against the last one.
    """

    def test_files_from_two_harnesses_are_reported_in_path_order(self):
        late = SimpleNamespace(workspace_files=lambda: {"z-late.json": "{}\n"})
        early = SimpleNamespace(workspace_files=lambda: {"a-early.json": "{}\n"})
        with mock.patch.object(registry, "all", return_value=[late, early]):
            rows = workspace.harness_layer(self.ws)
        self.assertEqual([rel for rel, _ in rows], ["a-early.json", "z-late.json"],
                         "the rows came back in the order the harnesses were registered")


class AMarkerThatCannotBeWrittenCostsOnlyTheMarker(WorkspaceLayer):
    """`except OSError: pass` around the marker write — the one catch in `wire_harnesses`
    that is not about the operator's file.

    The write above it has its own catch and its own test
    (`test_a_path_charter_cannot_write_is_reported_and_never_forced`, which reports
    `blocked`); this one is the sidecar, and it is silent on purpose. By the time it runs
    the layer is already on disk and correct — the marker only records that charter wrote
    it — so raising here would turn a workspace that was successfully wired into a failed
    `charter workspace reinit`, and `scaffold` calls this on every launch.

    A DIRECTORY at the marker's path is the fixture because it is the one refusal no
    permission bit decides: `write_text` on a directory raises `IsADirectoryError` for
    every user including root, where a `chmod 500` fixture silently succeeds for root and
    measures nothing. The cost of losing the marker is real and is charged next run —
    charter no longer recognises its own file and reads it as the operator's — which is
    why this is `pass` and not repair.
    """

    def test_a_directory_where_the_marker_goes_does_not_cost_the_layer(self):
        wd = workspace.workspace_dir(self.ws)
        marker = wd / workspace.GENERATED_MARKER
        self.settings().unlink()
        marker.unlink()
        marker.mkdir()

        rows = workspace.wire_harnesses(self.ws)

        self.assertEqual(rows, [(".claude/settings.json", "created")])
        self.assertTrue(self.settings().is_file(),
                        "the layer was lost to a marker that could not be written")
        self.assertTrue(marker.is_dir(), "charter removed what was in its way")
class WhatReinitSaysAboutTheLayer(WorkspaceLayer):
    """`cmd_workspace_reinit`'s per-file report — every outcome that reaches it.

    Nothing exercised this loop, which is why the deletion sweep returned eight survivors
    across it: both `did ==` branches, both their literals, the `"layer"` key, the `or ()`
    and the wrote/refreshed ternary.

    **`"present"` never arrives here, and that is worth writing down because it looks like
    it does.** `write_layer` answers `"present"` for a file already exactly right, and the
    `else` below increments `healed` and prints *refreshed* — so the two functions read
    side by side say a current layer is reported as healed, which would make
    `charter workspace reinit` claim work it had not done. It does not: `reinit` filters
    those rows first (`workspace.py:1592`, ``if st != "present"``), so a third branch here
    would be unreachable. `test_a_current_layer_reports_nothing_at_all` pins the filter
    from this end so the next reader need not re-derive it.

    The sentences are spelled out rather than compared against the code that prints them,
    because a test built from the same f-string agrees with any wording it takes — and
    *"was not written by charter — left completely untouched"* is the ownership promise
    made visible. If it stops being true, the operator finds out by having their file
    overwritten.
    """

    def _reinit(self):
        said: list[tuple[str, str]] = []
        with mock.patch.object(commands_workspace.util, "ok",
                               side_effect=lambda m: said.append(("ok", m))), \
             mock.patch.object(commands_workspace.util, "warn",
                               side_effect=lambda m: said.append(("warn", m))), \
             mock.patch.object(commands_workspace.util, "err",
                               side_effect=lambda m: said.append(("err", m))), \
             mock.patch.object(commands_workspace.util, "info", side_effect=lambda m: None):
            commands_workspace.cmd_workspace_reinit(
                SimpleNamespace(name=self.ws, all=False))
        return said

    def test_a_hand_edited_file_is_named_as_the_operators_and_left(self):
        self.settings().write_text("{}")
        said = self._reinit()
        self.assertIn(
            f"'{self.ws}': .claude/settings.json was not written by charter — left "
            f"completely untouched. Remove it if you want charter's own again.",
            [m for _, m in said])
        self.assertEqual(self.settings().read_text(), "{}")

    def test_a_current_layer_reports_nothing_at_all(self):
        said = self._reinit()
        self.assertNotIn("harness layer", " ".join(m for _, m in said))
        self.assertIn("Up to date", " ".join(m for _, m in said))

    def test_a_file_charter_owns_and_the_plane_moved_reads_refreshed(self):
        # One of the THREE mirrored keys, not `permissions` — that one stays in the plane,
        # so moving it leaves the layer correctly unchanged and this would assert nothing.
        _plane_settings(config.ROOT, statusLine={"type": "command",
                                                 "command": "charter statusline --wide"})
        said = self._reinit()
        self.assertIn(
            f"Reinitialized '{self.ws}' → refreshed .claude/settings.json "
            f"(charter's harness layer).", [m for _, m in said])

    def test_a_missing_file_reads_wrote_rather_than_refreshed(self):
        self.settings().unlink()
        (workspace.workspace_dir(self.ws) / workspace.GENERATED_MARKER).unlink()
        said = self._reinit()
        self.assertIn(
            f"Reinitialized '{self.ws}' → wrote .claude/settings.json "
            f"(charter's harness layer).", [m for _, m in said])

    def test_a_path_charter_cannot_write_is_reported_and_never_forced(self):
        """`blocked`. A DIRECTORY at that path reads `foreign` instead — charter cannot
        match it against the marker so it treats it as the operator's, which is the safe
        direction. `blocked` is the other half: the path IS charter's to write and the
        write itself fails."""
        f = self.settings()
        f.unlink()
        d = f.parent
        d.chmod(0o500)
        self.addCleanup(d.chmod, 0o700)
        said = self._reinit()
        self.assertIn(
            f"'{self.ws}': .claude/settings.json could not be written — something is in "
            f"the way at that path. charter never deletes or renames existing content.",
            [m for _, m in said])
        self.assertFalse(f.exists(), "charter forced the write past a refusal")
        self.assertNotIn("harness layer", " ".join(m for _, m in said),
                         "a blocked file was also counted as healed")
class TheMarkersNameIsPartOfTheOnDiskContract(_isolation.PersonaIso):
    """`.charter-generated` is spelled out here once, by hand.

    Every other reference in this suite reads `workspace.GENERATED_MARKER`, which agrees
    with any value that constant takes — the deletion sweep found the literal as a
    `retune-string` survivor for exactly that reason, and it is the eleventh time this
    repository has shipped that shape.

    **The name is a contract, not an implementation detail.** It is the file that answers
    *is this layer charter's or the operator's* — the rule `_AGENT_MARKER` already sets for
    generated sub-agents: a file charter wrote is refreshed, a file without the marker is
    never clobbered. Rename it and charter stops recognising work it did itself: it would
    either overwrite an operator's edited `settings.json` believing it had never written
    one, or refuse to refresh its own believing somebody had taken ownership. Neither
    failure announces itself.

    It also sits beside `.charter-structure` in a workspace, which is the precedent for a
    charter-owned dotfile there and the reason the name is shaped the way it is.
    """

    def test_the_marker_is_named_charter_generated(self):
        self.assertEqual(workspace.GENERATED_MARKER, ".charter-generated")

    def test_it_is_a_dotfile_so_a_workspace_listing_does_not_show_it(self):
        self.assertTrue(workspace.GENERATED_MARKER.startswith("."))

class AWorkspaceRootReachedThroughASymlink(_isolation.PersonaIso):
    """`_layer_files`' containment check normalises **both** sides, and only one of them
    is normalised for it.

    `target` is `(wd / rel).resolve()`, so a `rel` carrying `..` cannot escape. `wd` looks
    like it needs no resolving — `config.ROOT` is resolved by `root.find_root` (`.resolve()`
    at `root.py:60`), so every path derived from it is already canonical and
    `wd == wd.resolve()`. On that reading the two `wd.resolve()` calls are dead and the
    deletion sweep, which found both as survivors, would be pointing at redundancy.

    **It is not redundancy, and the difference is `config.use`.** `use(root)` hands *root*
    straight to `derive()` without resolving it (`config.py:698`) — deliberately, so a
    caller pins the directory it named rather than one charter picked. `PersonaIso` calls
    exactly that. So a root reached through a symlink stays unresolved, and then::

        wd            = /private/tmp/…/link/workspaces/alpha
        wd.resolve()  = /private/tmp/…/real/workspaces/alpha

    With the resolve, `wd.resolve() in target.parents` holds and the file is written. Drop
    either call and the unresolved `wd` is not among the resolved `target`'s parents, the
    `continue` fires for **every** file, and a workspace silently receives **no layer at
    all** — the failure mode this whole feature exists to prevent, produced by the guard
    against a different one.

    macOS makes this easy to miss rather than hard to hit: `tempfile.mkdtemp()` answers
    under `/var/folders/…` and `/var` is itself a symlink, so a fixture that resolves its
    temp path before handing it over normalises for free and never reaches this. #837 hit
    the mirror image — a masked `.resolve()` pair "hidden locally because macOS `/tmp` is a
    symlink and every fixture path needed normalising for free".
    """

    def setUp(self):
        super().setUp()
        # A plane with settings of its own, because `workspace_files` mirrors the plane's
        # COMMITTED file rather than charter's constants — a plane with none answers `{}`
        # deliberately, and a fixture without one would assert nothing.
        real = Path(config.ROOT) / "real-plane"
        (real / "workspaces" / "alpha").mkdir(parents=True, exist_ok=True)
        (real / ".claude").mkdir(parents=True, exist_ok=True)
        (real / ".claude" / "settings.json").write_text(json.dumps({
            "env": {"CHARTER_HARNESS": "claude-code"},
            "statusLine": {"type": "command", "command": "charter statusline"},
            "enabledPlugins": {"charter@charter": True},
        }))
        (real / "charter.toml").write_text("schema = 1\n")
        link = Path(config.ROOT) / "link-plane"
        if not link.exists():
            link.symlink_to("real-plane")
        self.link, self.real = link, real

    def test_the_layer_reaches_a_workspace_under_an_unresolved_root(self):
        prev = config.use(self.link)
        self.addCleanup(config.restore, prev)
        wd = workspace.workspace_dir("alpha")
        self.assertNotEqual(wd, wd.resolve(),
                            "fixture normalised for free — this case would assert nothing")
        self.assertTrue(workspace._layer_files("alpha"),
                        "a workspace under a symlinked root received no layer at all")


if __name__ == "__main__":  # pragma: no cover
    import unittest
