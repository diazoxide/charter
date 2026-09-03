"""`doctor` reports the directory it is RUNNING in, not the plane it resolved (#851).

Fourth variant of the family #168, #177 and #261 belong to. Each one is a checker reading
a proxy instead of the fact: *packaged* read as protection, *installed* read as wired,
*enabled* read as loaded. This one is **elsewhere read as here**.

`_settings_files` asked about `config.ROOT/.claude/settings.json`, and `config.ROOT` is
resolved by walking UP from the working directory — so it lands on the plane from anywhere
inside it. Claude Code does not walk up: a session's project settings, agents, skills and
commands come from the session's own directory and nowhere above it. Only `CLAUDE.md`
walks.

So a chat launched at `workspaces/<ws>/` — where the `+` button and every workspace tab put
it — has no plugin, no `PreToolUse` guard and no status line, and `charter doctor` read the
plane's file and called all of it fine. **A checker that reports on a different directory
than the one it is running in is wrong regardless of what it finds there**, and doctor is
the one command an operator runs *because something felt wrong in that chat*.

The fix is not to prefer either directory silently. The plane is still identity — personas,
the vault, memory and workspaces resolve to it from anywhere (`config.in_tree`,
`docs/control-plane.md`) — and the session's own root is what the host reads settings from.
Doctor now says which is which, and the rows that read settings answer for the session.
"""

from __future__ import annotations

import json
import os
import unittest
from pathlib import Path
from unittest import mock

from charter import config, doctor
from tests._isolation import PersonaIso

OK, WARN = doctor.OK, doctor.WARN

#: Hand-spelled, never imported from the module under test. A test that compares a report
#: against the constant that spells it asserts nothing — it passes just as happily over an
#: empty string. `_GUARD_HOOK` lives in `commands` and is deliberately not reused here.
_PRETOOLUSE = '{"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [' \
              '{"type": "command", "command": "charter hook pretooluse"}]}]}}'


class SessionRootCase(PersonaIso):
    """A plane wired the way `charter init` leaves one, and a workspace directory under it
    that nothing has scaffolded — the exact shape the report describes."""

    def setUp(self) -> None:
        super().setUp()
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        self.enterContext(mock.patch.object(config, "HAS_CONTROL_PLANE", True))
        # A HOME with no user-level settings, so nothing here can pass on the developer's
        # own machine config and prove nothing on CI.
        self.home = self.tmp / "home"
        (self.home / ".claude").mkdir(parents=True, exist_ok=True)
        self.enterContext(mock.patch.dict(os.environ, {"HOME": str(self.home)}))
        os.environ.pop("CLAUDE_PLUGIN_ROOT", None)
        self.workspace = config.ROOT / "workspaces" / "fleet"
        self.workspace.mkdir(parents=True, exist_ok=True)
        # Resolved, because `os.getcwd()` is: the kernel hands back the physical path, and
        # macOS's temp directory is reached through a symlink (`/var` → `/private/var`).
        # Comparing a report built from `getcwd` against an unresolved fixture path fails
        # on the developer's machine and passes on CI, which is worse than either.
        self.workspace = self.workspace.resolve()
        # Registered AFTER `PersonaIso`'s own cleanup, so it runs BEFORE it (LIFO): the
        # tmp tree cannot be removed out from under the process's own cwd.
        self.addCleanup(os.chdir, os.getcwd())

    def rooted_at(self, where: Path) -> None:
        """Launch this "session" at *where* — the directory the host resolves settings
        against. `os.getcwd()` is the only evidence a running `charter doctor` has of it:
        `$CLAUDE_PROJECT_DIR` is exported to hook commands, not to the session's shell."""
        os.chdir(where)

    def wire_the_plane(self) -> Path:
        p = config.ROOT / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(_PRETOOLUSE)
        return p

    def wire(self, where: Path) -> Path:
        p = where / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(_PRETOOLUSE)
        return p


class TestTheFalseGreen(SessionRootCase):
    """The report, reproduced: the plane is wired, the session is not, doctor said fine."""

    def test_the_planes_wiring_does_not_count_for_a_session_rooted_below_it(self):
        self.wire_the_plane()
        self.rooted_at(self.workspace)
        self.assertEqual(doctor.check_guard_wired().status, WARN)

    def test_the_settings_it_reads_are_the_ones_the_host_would_read(self):
        self.rooted_at(self.workspace)
        read = [str(p) for p in doctor._settings_files()]
        self.assertEqual(read[:2],
                         [str(self.workspace / ".claude" / "settings.json"),
                          str(self.workspace / ".claude" / "settings.local.json")])

    def test_a_plugin_enabled_only_in_the_planes_settings_does_not_reach_the_session(self):
        """`enabledPlugins` is a settings key like any other, so it is scoped to the
        directory the host read it from. The report's chat had no plugin for exactly this
        reason, and doctor named one."""
        p = config.ROOT / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps({"enabledPlugins": {"charter@charter": True}}))
        self.rooted_at(self.workspace)
        self.assertEqual(doctor._enabled_plugin_ids(), set())


class TestItSaysWhichDirectoryAnsweredForIt(SessionRootCase):
    def test_it_names_the_session_root_and_says_it_is_not_the_plane(self):
        self.rooted_at(self.workspace)
        detail = doctor.check_session_root().detail
        self.assertIn(str(self.workspace), detail)
        self.assertIn("not the plane", detail)

    def test_it_says_the_host_does_not_walk_up(self):
        """The whole mechanism, in the row. Without it "not the plane" reads as trivia —
        every workspace directory is not the plane — instead of as the reason the plane's
        `.claude/` is not in force."""
        self.rooted_at(self.workspace)
        self.assertIn("does not walk up", doctor.check_session_root().detail)

    def test_it_keeps_the_plane_as_identity(self):
        """Plane is identity, tree is artifacts. Charter's own vocabulary, said here so an
        operator does not read this row as "you are on the wrong plane" and go looking for
        their personas."""
        self.rooted_at(self.workspace)
        self.assertIn("identity", doctor.check_session_root().detail)

    def test_a_session_rooted_at_the_plane_says_so(self):
        self.rooted_at(config.ROOT)
        r = doctor.check_session_root()
        self.assertEqual(r.status, OK)
        self.assertIn("the plane", r.detail)
        self.assertNotIn("not the plane", r.detail)

    def test_it_is_a_fact_and_never_a_verdict(self):
        """A chat rooted in a workspace clone is the DESIGNED workflow — the `+` button
        puts it there. A row that warns every session for the normal case is one operators
        learn to skip, which is the failure `check_memory_indexes` and `check_harness` both
        record. The unwired guard is what warns; this row supplies the reason."""
        self.rooted_at(self.workspace)
        self.assertEqual(doctor.check_session_root().status, OK)

    def test_doctor_runs_it(self):
        self.assertIn("session root", doctor.check_names())


class TestTheGuardRowDoesNotSendYouSomewhereItDoesNotReach(SessionRootCase):
    def test_it_does_not_offer_reinit_as_the_fix_for_a_session_rooted_elsewhere(self):
        """`charter reinit` wires the PLANE's `.claude/settings.json`. That file is not the
        one this session reads, so following the hint would leave the session exactly as
        unguarded and the operator convinced they had fixed it — this issue again, one
        level down, in the remedy rather than in the reading."""
        self.rooted_at(self.workspace)
        hint = doctor.check_guard_wired().hint
        self.assertIn(str(self.workspace), hint)
        self.assertIn("~/.claude/settings.json", hint)

    def test_the_hint_stays_as_it_was_when_the_session_is_rooted_at_the_plane(self):
        self.rooted_at(config.ROOT)
        self.assertIn("charter reinit", doctor.check_guard_wired().hint)

    def test_wiring_the_session_root_is_what_counts(self):
        self.wire(self.workspace)
        self.rooted_at(self.workspace)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK)
        self.assertIn(str(self.workspace / ".claude" / "settings.json"), r.detail)

    def test_user_settings_still_reach_every_session(self):
        """Trust is inherited up to the git root and `~/.claude/settings.json` is read by
        every session, so this is the one declaration that covers a chat rooted anywhere
        inside the plane's repo. The hint above names it for that reason."""
        (self.home / ".claude" / "settings.json").write_text(_PRETOOLUSE)
        self.rooted_at(self.workspace)
        self.assertEqual(doctor.check_guard_wired().status, OK)


class TestTheHostileCases(SessionRootCase):
    """Every catch pinned by a test that makes the call under it throw. `nested_plane_in`
    states the rule these follow: a promise asserted only by paths that never fail is a
    promise nothing measured — and a preflight row must render something whatever it finds,
    because a check that raises is a check the reader never sees."""

    def test_a_deleted_working_directory_falls_back_to_the_plane(self):
        """`os.getcwd()` raises when the directory the process is standing in has been
        removed under it. Doctor still has to print a row — and it must not print the
        divergence one, which would blame a plane for a failed syscall."""
        with mock.patch("os.getcwd", side_effect=OSError("cwd is gone")):
            self.assertTrue(doctor.session_root().samefile(config.ROOT))
            self.assertTrue(doctor.session_is_the_plane(),
                            "a getcwd that failed is not a session rooted elsewhere")

    def test_an_unresolvable_path_compares_unresolved_rather_than_raising(self):
        """A symlink loop makes `Path.resolve` raise `RuntimeError`, and an inaccessible
        ancestor makes it raise `OSError`. Neither is a reason for `charter doctor` to
        traceback."""
        self.rooted_at(config.ROOT)
        with mock.patch.object(Path, "resolve", side_effect=RuntimeError("symlink loop")):
            self.assertIsInstance(doctor.session_is_the_plane(), bool)

    def test_with_no_plane_anywhere_the_row_says_so_instead_of_reporting_a_divergence(self):
        """`charter doctor` outside a control plane is supported — `check_guard_wired` says
        so in its own first branch, and `find_root_or_cwd` exists so import-time path
        building survives there. `config.ROOT` is then the starting directory rather than a
        plane, so without this branch the row compares a directory against a non-plane and
        announces a divergence between two things neither of which is a plane.

        **Asserted on what only this branch can say.** The first version of this test
        checked that the detail names the directory — which the fall-through detail also
        does, so deleting the branch left it green and the deletion sweep charged the line
        as unpinned. `no control plane` is reachable from here and from nowhere else."""
        self.rooted_at(self.workspace)
        with mock.patch.object(config, "HAS_CONTROL_PLANE", False):
            r = doctor.check_session_root()
        self.assertEqual(r.status, OK)
        self.assertIn(str(self.workspace), r.detail)
        self.assertIn("no control plane", r.detail)
        self.assertNotIn("not the plane", r.detail)


class TestTwoSpellingsOfOneDirectoryAreOneDirectory(SessionRootCase):
    """The comparison is the whole fix, and a path comparison is only right if the two
    sides are spelled the same way.

    A fixture whose paths need no normalising leaves that untested exactly where it breaks.
    #837 hit the mirror image of this — a masked pair of `resolve()` calls CI caught and
    macOS did not, *hidden locally because macOS `/tmp` is a symlink and every fixture path
    needed normalising for free*. So the symlink here is built on purpose rather than
    inherited from the platform, and the test means the same thing on both.

    The failure it prevents is worse than the one #851 is about: doctor announcing "this
    session is rooted elsewhere" for a session rooted exactly where it should be, and every
    settings row below it reporting a divergence that does not exist.
    """

    def _linked_plane(self) -> tuple[Path, Path]:
        real = self.tmp / "real-plane"
        real.mkdir()
        (real / "charter.toml").write_text("schema = 1\n")
        link = self.tmp / "linked-plane"
        link.symlink_to(real)
        return real, link

    def test_a_plane_spelled_through_a_symlink_is_still_this_sessions_plane(self):
        """`config.derive` stores ROOT exactly as it was handed in — `config.use`, a
        symlinked checkout, and macOS's own `/var` → `/private/var` all produce one. The
        session side needs nothing: `os.getcwd` hands back the physical path already."""
        real, link = self._linked_plane()
        with mock.patch.object(config, "ROOT", link):
            self.rooted_at(real)
            self.assertTrue(doctor.session_is_the_plane())

    def test_the_row_does_not_announce_a_divergence_that_is_not_there(self):
        real, link = self._linked_plane()
        with mock.patch.object(config, "ROOT", link):
            self.rooted_at(real)
            detail = doctor.check_session_root().detail
        self.assertIn("the plane", detail)
        self.assertNotIn("not the plane", detail)

    def test_the_planes_own_wiring_still_counts_when_it_is_reached_that_way(self):
        """The consequence, not the predicate. An unnormalised comparison would report the
        guard unwired at the plane's own root — the false alarm this fix must not create
        while removing a false green."""
        real, link = self._linked_plane()
        self.wire(real)
        with mock.patch.object(config, "ROOT", link):
            self.rooted_at(real)
            self.assertEqual(doctor.check_guard_wired().status, OK)


class TestTheAskRulesRowAnswersForTheSessionToo(SessionRootCase):
    """#855, folded in: the same defect one check over, and the same one-line shape of fix.

    `permissions.ask` is a host settings key, so it is scoped to the directory the host read
    it from. The row exists to name *why* a persona's declared tools started prompting
    (ADR 0014) — an answer read out of a file the session never opens sends the reader to
    the wrong file, in both directions.
    """

    def _persona_declaring(self, *tools: str) -> None:
        from charter import persona
        self.make_persona("ops", role="Ops", vault="none", tools=", ".join(tools))
        persona.set_active("ops")

    def _ask(self, where: Path, *rules: str) -> None:
        p = where / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps({"permissions": {"ask": list(rules)}}))

    def test_a_rule_in_the_planes_settings_does_not_shadow_a_session_rooted_below_it(self):
        self._persona_declaring("kubectl")
        self._ask(Path(config.ROOT), "Bash(kubectl *)")
        self.rooted_at(self.workspace)
        r = doctor.check_ask_rules()
        self.assertEqual(r.status, OK)
        self.assertNotIn("kubectl", f"{r.detail} {r.hint}")

    def test_a_rule_in_the_sessions_own_settings_does(self):
        self._persona_declaring("kubectl")
        self._ask(self.workspace, "Bash(kubectl *)")
        self.rooted_at(self.workspace)
        r = doctor.check_ask_rules()
        self.assertEqual(r.status, WARN)
        self.assertIn("kubectl", f"{r.detail} {r.hint}")


class TestTheWriterStillAsksAboutTheFileItWrites(SessionRootCase):
    def test_the_plugin_lookup_can_be_asked_about_a_named_root(self):
        """`commands._ensure_guard_hook` writes the PLANE's settings file and skips it when
        an enabled plugin already dispatches the guard. That question is about the plane's
        own `enabledPlugins`, wherever the operator happened to be standing when they typed
        `charter reinit` — so the writer names its root rather than inheriting doctor's."""
        plug = self.tmp / "plug"
        (plug / "hooks").mkdir(parents=True)
        (plug / "hooks" / "hooks.json").write_text(json.dumps({"hooks": {"PreToolUse": [
            {"hooks": [{"type": "command", "command": "charter hook pretooluse"}]}]}}))
        man = self.home / ".claude" / "plugins" / "installed_plugins.json"
        man.parent.mkdir(parents=True, exist_ok=True)
        man.write_text(json.dumps({"version": 2, "plugins": {
            "charter@charter": [{"scope": "user", "installPath": str(plug)}]}}))
        p = config.ROOT / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps({"enabledPlugins": {"charter@charter": True}}))
        self.rooted_at(self.workspace)
        self.assertIsNone(doctor._plugin_declaring_guard(),
                          "the session's own root sees no plugin")
        self.assertEqual(doctor._plugin_declaring_guard(Path(config.ROOT)),
                         "charter@charter")


if __name__ == "__main__":
    unittest.main()
