"""The installed plugin versus the marketplace clone — by CONTENT, not by version string.

The gap this covers is live, and was measured on a real machine while it was written: the
marketplace clone at ``main``, the installed cache at whatever ``main`` was when the plugin
was last installed, **both saying 0.51.0**, 45 files apart — ``skills/secrets/SKILL.md``
and ``skills/browser/SKILL.md`` among them — and ``claude plugin update charter@charter``
correctly answering *already at the latest version*. Version-keyed updates cannot see this,
because charter's plugin version moves exactly once per release and the clone moves
whenever Claude Code re-fetches it.

Two consequences, both tested here:

* `doctor.check_plugin_freshness` compares files and says so.
* `charter update` on the dev channel force-refreshes, because uninstall + reinstall is the
  only mechanism that repopulates a version-keyed cache directory.

**What is NOT broken, so that nothing here "fixes" it:** ``hooks/hooks.json`` invokes
``charter hook … --plugin-version 0.51.0`` — the *command*, resolved from ``PATH``, i.e.
the ``uv tool`` install. Hook behaviour follows the CLI, not the plugin's bundled copy of
``charter/*.py``. The staleness that bites is the skills, which are text the model loads.

No test here runs `claude`. `plugincache._claude_json` is the one seam every call goes
through and it is stubbed; a test that reached the real CLI would mutate the developer's
own plugin installation.
"""

from __future__ import annotations

import unittest
from pathlib import Path
from unittest import mock

from charter import config, doctor, plugincache
from tests._isolation import PersonaIso

REPO = Path(__file__).resolve().parents[1]


def tree(root: Path, files: dict[str, str]) -> Path:
    for rel, body in files.items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(body)
    return root


PLUGIN = {
    ".claude-plugin/plugin.json": '{"name": "charter", "version": "0.51.0"}',
    "hooks/hooks.json": '{"hooks": {}}',
    "skills/secrets/SKILL.md": "# secrets\n",
    "skills/browser/SKILL.md": "# browser\n",
}


class TheHashCoversWhatThePluginLoads(PersonaIso):
    def _pair(self, extra_left=None, extra_right=None):
        left = tree(self.tmp / "installed", {**PLUGIN, **(extra_left or {})})
        right = tree(self.tmp / "clone", {**PLUGIN, **(extra_right or {})})
        return left, right

    def test_two_identical_trees_hash_the_same(self):
        left, right = self._pair()
        self.assertEqual(plugincache.content_hash(left), plugincache.content_hash(right))
        self.assertIsNotNone(plugincache.content_hash(left))

    def test_an_edited_skill_changes_the_hash(self):
        """The case that actually happened: same version on both sides, different text."""
        left, right = self._pair(extra_right={"skills/secrets/SKILL.md": "# secrets v2\n"})
        self.assertNotEqual(plugincache.content_hash(left), plugincache.content_hash(right))

    def test_an_added_file_changes_the_hash(self):
        left, right = self._pair(extra_right={"skills/newthing/SKILL.md": "# new\n"})
        self.assertNotEqual(plugincache.content_hash(left), plugincache.content_hash(right))

    def test_a_renamed_file_changes_the_hash_even_with_identical_bytes(self):
        """The path goes into the digest, not only the bytes. Without that, moving a skill
        from one directory to another — which changes which skill the model loads — would
        hash identically to not having moved it, and this check would report a tree that
        loads different instructions as being in step."""
        left = tree(self.tmp / "a", PLUGIN)
        moved = {k: v for k, v in PLUGIN.items() if k != "skills/secrets/SKILL.md"}
        moved["skills/vault/SKILL.md"] = PLUGIN["skills/secrets/SKILL.md"]
        right = tree(self.tmp / "b", moved)
        self.assertNotEqual(plugincache.content_hash(left), plugincache.content_hash(right))

    def test_a_deleted_file_changes_the_hash(self):
        left, right = self._pair()
        (right / "skills/browser/SKILL.md").unlink()
        self.assertNotEqual(plugincache.content_hash(left), plugincache.content_hash(right))

    def test_churn_outside_the_plugin_surface_is_not_staleness(self):
        """charter's marketplace entry is ``"source": "./"`` — the whole repository is
        copied into the cache, ``tests/`` and ``docs/`` and all. Hashing the copy wholesale
        would report a test-file edit as a stale plugin, which is not a stricter check but
        a noisier one: a row that warns about something benign trains people to scroll past
        the row, which costs the case that matters."""
        left, right = self._pair(extra_right={"tests/test_x.py": "assert True\n",
                                              "docs/news/unreleased-x.md": "hi\n",
                                              "README.md": "different\n",
                                              "charter/cli.py": "print(1)\n"})
        self.assertEqual(plugincache.content_hash(left), plugincache.content_hash(right))

    def test_a_missing_tree_is_none_rather_than_an_empty_hash(self):
        """"I could not look" must never render as "they match" — the same distinction
        `hooks.dispatched_handlers` documents at length, and the whole reason `doctor`
        reports WARN rather than a green tick when it cannot compare."""
        self.assertIsNone(plugincache.content_hash(self.tmp / "nothing-here"))
        self.assertIsNone(plugincache.content_hash(self.tmp / "installed" / "no" / "such"))

    def test_the_hash_of_an_empty_surface_is_not_the_hash_of_a_populated_one(self):
        empty = self.tmp / "empty"
        empty.mkdir()
        left, _ = self._pair()
        self.assertNotEqual(plugincache.content_hash(empty), plugincache.content_hash(left))

    def test_the_surface_names_every_plugin_directory_this_repo_actually_ships(self):
        """The constant is a whitelist, so a directory missing from it is a category of
        drift this module reports as clean. Asserted against the repo rather than against a
        second hardcoded list — a plugin directory added to charter that nobody adds here
        fails this, at the moment it is added."""
        shipped = {name for name in (".claude-plugin", "hooks", "skills", "commands",
                                     "agents")
                   if (REPO / name).is_dir()}
        self.assertTrue(shipped, "the repo ships no plugin directories at all?")
        self.assertTrue(shipped <= set(plugincache.PLUGIN_SURFACE),
                        f"not covered: {sorted(shipped - set(plugincache.PLUGIN_SURFACE))}")
        self.assertIn("skills", shipped)

    def test_the_repos_own_plugin_surface_hashes(self):
        """End-to-end against the real tree: whatever else is true, this must produce a
        digest for the checkout the suite is running in."""
        self.assertIsNotNone(plugincache.content_hash(REPO))


class DifferingNamesThePaths(PersonaIso):
    """A doctor row that names `skills/secrets/SKILL.md` is the difference between a number
    a reader distrusts and a fact they can go and look at."""

    def test_it_names_an_edited_file(self):
        left = tree(self.tmp / "a", PLUGIN)
        right = tree(self.tmp / "b", {**PLUGIN, "skills/secrets/SKILL.md": "changed\n"})
        self.assertEqual(plugincache.differing(left, right), ["skills/secrets/SKILL.md"])

    def test_it_names_a_file_present_on_only_one_side(self):
        left = tree(self.tmp / "a", PLUGIN)
        right = tree(self.tmp / "b", {**PLUGIN, "skills/new/SKILL.md": "x\n"})
        self.assertIn("skills/new/SKILL.md", plugincache.differing(left, right))

    def test_identical_trees_name_nothing(self):
        left = tree(self.tmp / "a", PLUGIN)
        right = tree(self.tmp / "b", PLUGIN)
        self.assertEqual(plugincache.differing(left, right), [])

    def test_it_stops_at_the_limit_rather_than_listing_forty_five_files(self):
        left = tree(self.tmp / "a", PLUGIN)
        right = tree(self.tmp / "b", {**PLUGIN,
                                      **{f"skills/s{i}/SKILL.md": "x\n" for i in range(20)}})
        self.assertEqual(len(plugincache.differing(left, right)), 3)
        self.assertEqual(len(plugincache.differing(left, right, limit=5)), 5)


class TheRefreshCommandsAreConstants(unittest.TestCase):
    """`claude plugin list --json` is a machine's own output rather than a committed file,
    so this is a narrower hazard than `charter.toml`'s — but it is still input, and it still
    reaches an argv."""

    def test_the_three_steps_are_in_the_order_that_was_verified_to_work(self):
        """Measured, not reasoned about. `install` alone against an already-installed plugin
        answers *"already installed"* and leaves the cache untouched; without `marketplace
        update` first, the reinstall faithfully re-copies the same stale content."""
        argvs = plugincache.refresh_argvs("charter@charter", "project")
        self.assertEqual([a[2] for a in argvs], ["marketplace", "uninstall", "install"])
        self.assertEqual(argvs[0], ["claude", "plugin", "marketplace", "update", "charter"])
        self.assertEqual(argvs[1], ["claude", "plugin", "uninstall", "charter@charter",
                                    "--scope", "project", "-y"])
        self.assertEqual(argvs[2], ["claude", "plugin", "install", "charter@charter",
                                    "--scope", "project", "-y"])

    def test_a_scope_charter_does_not_know_builds_nothing(self):
        for scope in ("global", "", None, "project;rm -rf /", "--force", 1, ["project"]):
            with self.subTest(scope=scope):
                self.assertIsNone(plugincache.refresh_argvs("charter@charter", scope))

    def test_a_plugin_id_charter_does_not_recognise_builds_nothing(self):
        """`util.run` takes a list and never a shell string, so the shell is not the
        exposure; the exposure is an element beginning with `-` that `claude` reads as a
        FLAG rather than as the plugin to uninstall."""
        for pid in ("--scope", "-y", "charter", "charter@", "@charter", "",
                    "charter@charter extra", "charter@charter\nrm -rf /",
                    "charter@charter;id", "a/b@c", None, 7, "charter@charter@charter"):
            with self.subTest(pid=pid):
                self.assertIsNone(plugincache.refresh_argvs(pid, "project"))

    def test_every_element_of_every_step_is_free_of_shell_metacharacters(self):
        argvs = plugincache.refresh_argvs("charter@charter", "user")
        for argv in argvs:
            for element in argv:
                for ch in (";", "|", "&", "`", "$", "\n", " "):
                    self.assertNotIn(ch, element)

    def test_all_three_scopes_are_accepted(self):
        for scope in ("user", "project", "local"):
            self.assertIsNotNone(plugincache.refresh_argvs("charter@charter", scope))


class ReadingTheInstalledPlugin(unittest.TestCase):
    """Everything talks to `claude plugin … --json`, never to Claude Code's own files —
    `~/.claude/plugins/*` is an internal charter does not own, and `bin/edm` bet on an
    internal path and broke silently (#197)."""

    def _rows(self, rows):
        return mock.patch.object(plugincache, "_claude_json", return_value=rows)

    def test_the_charter_entry_is_found_among_others(self):
        rows = [{"id": "figma@official", "scope": "user", "installPath": "/x"},
                {"id": "charter@charter", "scope": "project", "installPath": "/y",
                 "projectPath": "/p"}]
        with self._rows(rows), mock.patch.object(plugincache, "available",
                                                 return_value=True):
            got = plugincache.installed_charter_plugin()
        self.assertEqual(got["id"], "charter@charter")

    def test_an_entry_charter_would_not_act_on_is_not_returned_at_all(self):
        """Validated where it is read, so no caller has to remember to check — the same
        placement argument `instance._HOTKEY_RE` makes for the config boundary."""
        for row in ({"id": "charter@charter", "scope": "root", "installPath": "/y"},
                    {"id": "-charter@charter", "scope": "user", "installPath": "/y"},
                    {"id": "charter@charter x", "scope": "user", "installPath": "/y"},
                    {"id": 7, "scope": "user"},
                    "not-a-dict"):
            with self.subTest(row=row):
                with self._rows([row]), mock.patch.object(plugincache, "available",
                                                          return_value=True):
                    self.assertIsNone(plugincache.installed_charter_plugin())

    def test_a_plugin_that_is_not_charter_is_not_charters_to_reinstall(self):
        rows = [{"id": "charterly@charter", "scope": "user", "installPath": "/y"}]
        with self._rows(rows), mock.patch.object(plugincache, "available",
                                                 return_value=True):
            self.assertIsNone(plugincache.installed_charter_plugin())

    def test_the_entry_for_the_plane_you_are_standing_in_wins(self):
        """Every project-scoped install points at the SAME versioned cache directory, so
        which one is chosen does not change the outcome — it changes which project's
        `claude plugin` invocation performs it, and the plane you are standing in is the
        least surprising choice."""
        rows = [{"id": "charter@charter", "scope": "project", "installPath": "/y",
                 "projectPath": "/elsewhere"},
                {"id": "charter@charter", "scope": "project", "installPath": "/y",
                 "projectPath": "/here"}]
        with self._rows(rows), mock.patch.object(plugincache, "available",
                                                 return_value=True):
            got = plugincache.installed_charter_plugin("/here")
        self.assertEqual(got["projectPath"], "/here")

    def test_a_failed_list_is_none_rather_than_nothing_installed(self):
        with self._rows(None), mock.patch.object(plugincache, "available",
                                                 return_value=True):
            self.assertIsNone(plugincache.installed_charter_plugin())

    def test_a_marketplace_name_charter_would_not_build_a_path_from_is_refused(self):
        for name in ("../../etc", "char ter", "-x", "", None, 7, "a/b"):
            with self.subTest(name=name):
                with mock.patch.object(plugincache, "_claude_json") as looked:
                    self.assertIsNone(plugincache.marketplace_clone(name))
                looked.assert_not_called()

    def test_force_refresh_declines_cleanly_with_no_claude_on_path(self):
        """charter supports opencode and Codex; a plane on either has no plugin cache. This
        must be a plain "nothing to do", not an error and not a claim of success."""
        with mock.patch.object(plugincache, "available", return_value=False):
            ok, detail = plugincache.force_refresh()
        self.assertFalse(ok)
        self.assertIn("claude", detail)

    def test_force_refresh_declines_when_no_charter_plugin_is_installed(self):
        with mock.patch.object(plugincache, "available", return_value=True), \
                self._rows([]):
            ok, detail = plugincache.force_refresh()
        self.assertFalse(ok)
        self.assertIn("no charter plugin is installed", detail)

    def test_force_refresh_stops_at_the_first_failing_step_and_names_it(self):
        rows = [{"id": "charter@charter", "scope": "user", "installPath": "/y"}]
        calls = []

        def run(cmd, cwd=None, check=True, **kw):
            calls.append(list(cmd))
            code = 1 if "uninstall" in cmd else 0
            return mock.Mock(returncode=code, stdout="", stderr="boom")

        with mock.patch.object(plugincache, "available", return_value=True), \
                self._rows(rows), mock.patch.object(plugincache.util, "run", run):
            ok, detail = plugincache.force_refresh()
        self.assertFalse(ok)
        self.assertIn("uninstall", detail)
        self.assertEqual(len(calls), 2, "it must not go on to install after a failure")

    def test_force_refresh_runs_all_three_steps_on_success(self):
        rows = [{"id": "charter@charter", "scope": "user", "installPath": "/y"}]
        calls = []

        def run(cmd, cwd=None, check=True, **kw):
            calls.append(list(cmd))
            return mock.Mock(returncode=0, stdout="", stderr="")

        with mock.patch.object(plugincache, "available", return_value=True), \
                self._rows(rows), mock.patch.object(plugincache.util, "run", run):
            ok, _ = plugincache.force_refresh()
        self.assertTrue(ok)
        self.assertEqual([c[2] for c in calls], ["marketplace", "uninstall", "install"])


class DoctorReportsFreshnessOnBothChannels(PersonaIso):
    """The severity differs by channel, and that is not hedging.

    The marketplace clone tracks ``main``, which Claude Code re-fetches on its own, so SOME
    drift is the steady state for every stable plane from the day after a release onward.
    A warning there would be a permanent yellow row whose only honest fix is "wait for the
    next release" — the cry-wolf failure `doctor`'s own comments keep returning to. On dev
    the plane asked to track ``main`` and its plugin is not, which is a real gap with a real
    command that closes it.
    """

    def _check(self, installed, clone, channel="stable", version="0.51.0"):
        entry = {"id": "charter@charter", "scope": "project", "version": version,
                 "installPath": str(installed), "projectPath": str(self.tmp)}
        with mock.patch.object(config, "UPDATE", {"channel": channel}), \
                mock.patch.object(plugincache, "available", return_value=True), \
                mock.patch.object(plugincache, "installed_charter_plugin",
                                  return_value=entry), \
                mock.patch.object(plugincache, "marketplace_clone",
                                  return_value=Path(clone) if clone else None):
            return doctor.check_plugin_freshness()

    def test_matching_trees_are_green_on_both_channels(self):
        left = tree(self.tmp / "a", PLUGIN)
        right = tree(self.tmp / "b", PLUGIN)
        for ch in ("stable", "dev"):
            with self.subTest(channel=ch):
                res = self._check(left, right, channel=ch)
                self.assertEqual(res.status, doctor.OK)
                self.assertIn("matches", res.detail)

    def test_drift_on_the_dev_channel_is_a_warning_that_names_the_command(self):
        left = tree(self.tmp / "a", PLUGIN)
        right = tree(self.tmp / "b", {**PLUGIN, "skills/secrets/SKILL.md": "moved on\n"})
        res = self._check(left, right, channel="dev")
        self.assertEqual(res.status, doctor.WARN)
        self.assertIn("skills/secrets/SKILL.md", res.detail)
        self.assertIn("charter update", res.hint)

    def test_drift_on_the_stable_channel_is_reported_in_detail_not_in_a_hint(self):
        """`Result.render` drops the hint entirely at OK, so guidance written there would
        be invisible while looking shipped — which is the failure ADR 0013 is about, and an
        unusually poor place to commit it."""
        left = tree(self.tmp / "a", PLUGIN)
        right = tree(self.tmp / "b", {**PLUGIN, "skills/secrets/SKILL.md": "moved on\n"})
        res = self._check(left, right, channel="stable")
        self.assertEqual(res.status, doctor.OK)
        self.assertIn("skills/secrets/SKILL.md", res.detail)
        self.assertIn("claude plugin update", res.detail)
        self.assertIn(res.detail, res.render())

    def test_the_row_never_fails_and_so_never_blocks_a_session(self):
        """`cmd_doctor` exits non-zero only on FAIL, and that exit code is what makes the
        SessionStart preflight print. A plugin whose skills are a week old is not a reason
        to shout at every session start on every plane."""
        left = tree(self.tmp / "a", PLUGIN)
        right = tree(self.tmp / "b", {**PLUGIN, "skills/secrets/SKILL.md": "x\n"})
        for ch in ("stable", "dev"):
            self.assertNotEqual(self._check(left, right, channel=ch).status, doctor.FAIL)

    def test_a_marketplace_it_cannot_locate_is_not_checked_rather_than_green(self):
        left = tree(self.tmp / "a", PLUGIN)
        res = self._check(left, None, channel="dev")
        self.assertEqual(res.status, doctor.WARN)
        self.assertIn("not checked", res.detail)

    def test_an_unreadable_install_path_is_not_checked_rather_than_green(self):
        right = tree(self.tmp / "b", PLUGIN)
        res = self._check(self.tmp / "gone", right, channel="stable")
        self.assertEqual(res.status, doctor.WARN)
        self.assertIn("not checked", res.detail)

    def test_no_claude_on_path_is_a_plain_green_nothing_to_compare(self):
        with mock.patch.object(plugincache, "available", return_value=False):
            res = doctor.check_plugin_freshness()
        self.assertEqual(res.status, doctor.OK)
        self.assertIn("no `claude`", res.detail)

    def test_no_charter_plugin_installed_is_a_plain_green(self):
        """CLI-only is a supported install (`docs/install.md`), and a plane that never
        installed the plugin must not be warned about the plugin's freshness forever."""
        with mock.patch.object(plugincache, "available", return_value=True), \
                mock.patch.object(plugincache, "installed_charter_plugin",
                                  return_value=None):
            res = doctor.check_plugin_freshness()
        self.assertEqual(res.status, doctor.OK)
        self.assertIn("not installed", res.detail)

    def test_the_check_is_registered_so_doctor_actually_runs_it(self):
        """A check nobody calls is a check that does not exist. `_checks` is private, so
        this asks the public surface the SessionStart preflight uses."""
        with mock.patch.object(plugincache, "available", return_value=False):
            names = [r.name for r in doctor.run_all()]
        self.assertIn("plugin files", names)

    def test_it_does_not_replace_the_version_skew_row(self):
        """Two different questions — 'is the plugin newer than the CLI' and 'is the plugin
        the same files as the clone' — and losing either one loses a real guard."""
        with mock.patch.object(plugincache, "available", return_value=False):
            names = [r.name for r in doctor.run_all()]
        self.assertIn("plugin", names)
        self.assertIn("plugin files", names)


if __name__ == "__main__":
    unittest.main()
