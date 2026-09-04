"""Installing charter installs charter (#881).

The README used to print three commands and hope the reader ran all of them. It prints one
now, and `charter init` installs Claude Code's charter plugin for the plane it creates —
`charter doctor --fix` for a plane that already exists.

Three obligations are pinned here, and they fail in three different places:

* **The argv is right.** `install_argvs` carries the ordering fact `tests/test_docs.py`
  used to hold against README prose — a marketplace that has not been added cannot be
  installed from — plus the plugin id, built from the same two manifests the prose was
  checked against.
* **It happens where somebody asked and nowhere else.** `charter init` and `charter doctor
  --fix` install; `charter reinit`, which re-runs the *wiring*, does not, and neither does
  any other command. Installing software as a side effect is #857.
* **`doctor` reports the gap without crying wolf.** WARN, never FAIL: a CLI-only install is
  supported, and `cmd_doctor`'s exit code is what makes the SessionStart preflight shout.

No test here runs `claude`. `plugincache._claude_json` and `plugincache.util.run` are the
two seams every call goes through and both are stubbed; `tests/_claudeguard.py` is the belt
for anything that is not, in this process and in children.
"""

from __future__ import annotations

import json
import unittest
from contextlib import contextmanager
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, doctor, plugincache
from charter.harness import claude_code, registry
from tests._isolation import PersonaIso

REPO = Path(__file__).resolve().parents[1]


@contextmanager
def rows(entries):
    """Answer `claude plugin list --json` with *entries*, and every other read with ``[]``."""
    def fake(args, cwd=None, timeout=None):
        return entries if args[:1] == ["list"] else []

    with mock.patch.object(plugincache, "available", return_value=True), \
            mock.patch.object(plugincache, "_claude_json", side_effect=fake):
        yield


@contextmanager
def spawns(calls, code=0, err=""):
    """Record every `util.run` argv into *calls* and answer with *code*."""
    def run(cmd, cwd=None, check=True, **kw):
        calls.append((list(cmd), cwd))
        return mock.Mock(returncode=code, stdout="", stderr=err)

    with mock.patch.object(plugincache.util, "run", run):
        yield


class TheArgvIsBuiltFromTheManifestsAndNotFromMemory(unittest.TestCase):
    """What `docs/install.md` used to spell out in prose, now spelled in code."""

    def _manifest(self, filename: str) -> dict:
        return json.loads((REPO / ".claude-plugin" / filename).read_text())

    def test_the_plugin_id_matches_both_manifests(self):
        """`plugin@marketplace`. Rename either manifest and `charter@charter` stops
        resolving, while every command charter builds keeps looking right — the silent-rot
        shape `TestVersionsMoveInLockstep` guards for the version numbers."""
        plugin = self._manifest("plugin.json")["name"]
        market = self._manifest("marketplace.json")["name"]
        self.assertEqual(plugincache.PLUGIN_ID, f"{plugin}@{market}")

    def test_the_marketplace_source_is_the_repository_charter_publishes_from(self):
        """`claude plugin marketplace add <owner>/<repo>`. A literal, because the wheel
        does not ship `.claude-plugin/` — so this pins it to `pyproject.toml`'s own
        `Repository` URL and the day charter moves house the suite fails, rather than
        every stranger's first install."""
        import tomllib
        url = tomllib.loads((REPO / "pyproject.toml").read_text())["project"]["urls"]["Repository"]
        self.assertTrue(url.endswith("/" + plugincache.MARKETPLACE_SOURCE), url)

    def test_it_adds_the_marketplace_before_installing_from_it(self):
        """The assertion that used to read the README's paste-in block: *"installing from a
        marketplace that has not been added fails"*."""
        add, put = plugincache.install_argvs()
        self.assertEqual(add[:4], ["claude", "plugin", "marketplace", "add"])
        self.assertEqual(put[:3], ["claude", "plugin", "install"])

    def test_it_installs_at_project_scope_non_interactively(self):
        """`project`, because the plugin is what carries a plane's pinned version — a
        machine-wide install collapses two planes onto one charter. `-y`, because `claude`
        refuses rather than prompts when stdout is not a TTY."""
        _add, put = plugincache.install_argvs()
        self.assertIn("--scope", put)
        self.assertEqual(put[put.index("--scope") + 1], "project")
        self.assertEqual(plugincache.INSTALL_SCOPE, "project")
        self.assertIn("-y", put)

    def test_a_scope_charter_would_not_install_at_yields_no_argv_at_all(self):
        """`None` rather than a list with a bad element in it, so a caller cannot run half
        a sequence against a value charter would not have built an argv from — the
        discipline `refresh_argvs` already keeps."""
        self.assertIsNone(plugincache.install_argvs("../../etc"))
        self.assertIsNone(plugincache.install_argvs(scope="project", source="--flag"))


class AnInstallBelongingToSomebodyElsesCheckoutIsNotAnAnswerHere(unittest.TestCase):
    """`installed_for` asks a different question from `installed_charter_plugin`.

    The older one answers *which install may charter refresh*, where every project-scoped
    install points at the same versioned cache directory and any one of them will do. This
    one answers *is THIS plane covered*, where they are not interchangeable at all — and
    reading a stranger's install as an answer would print "installed" over a plane with no
    plugin, which is `check_guard_wired`'s #168 defect wearing a new row.
    """

    def test_a_user_scope_install_covers_every_directory(self):
        with rows([{"id": "charter@charter", "scope": "user"}]):
            self.assertIsNotNone(plugincache.installed_for("/somewhere"))

    def test_a_project_scope_install_covers_only_its_own_project(self):
        entry = {"id": "charter@charter", "scope": "project", "projectPath": "/planes/a"}
        with rows([entry]):
            self.assertIsNotNone(plugincache.installed_for("/planes/a"))
            self.assertIsNone(plugincache.installed_for("/planes/b"))

    def test_an_unreadable_list_is_UNKNOWN_and_not_absence(self):
        """"I could not look" must never render as "there is nothing installed" — the
        distinction `plugincache.UNKNOWN` exists for, and the population most likely to be
        affected is an older `claude` that does not understand `--json`."""
        with mock.patch.object(plugincache, "available", return_value=True), \
                mock.patch.object(plugincache, "_claude_json", return_value=None):
            self.assertIs(plugincache.installed_for("/planes/a"), plugincache.UNKNOWN)

    def test_a_plugin_that_is_not_charters_is_not_charters(self):
        """Only `charter@charter`. Matching `charter@<anything>` would let charter act on a
        plugin called `charter` published by somebody else's marketplace."""
        with rows([{"id": "charter@someone-else", "scope": "user"}]):
            self.assertIsNone(plugincache.installed_for("/planes/a"))

    def test_the_same_directory_spelled_two_ways_is_the_same_directory(self):
        """Not a test-only nicety. macOS resolves `/tmp` to `/private/tmp` and `/var` to
        `/private/var`, so a plane under either is spelled one way by `claude plugin list`
        and another by the shell — and an unresolved compare would report "not installed"
        for an install that is right there."""
        import os
        import tempfile

        real = Path(tempfile.mkdtemp(prefix="charter-samedir-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(real, ignore_errors=True))
        link = real.parent / (real.name + "-link")
        os.symlink(real, link)
        self.addCleanup(lambda: link.unlink(missing_ok=True))
        entry = {"id": "charter@charter", "scope": "project", "projectPath": str(real)}
        with rows([entry]):
            self.assertIsNotNone(plugincache.installed_for(link))

    def test_a_projectPath_that_is_not_a_string_is_not_a_match(self):
        """`claude plugin list --json` is a machine's own output, not a committed file, but
        it is still input: a row whose `projectPath` is null or a number must answer "no",
        not raise out of a `doctor` row."""
        for junk in (None, 17, ["/planes/a"]):
            with rows([{"id": "charter@charter", "scope": "project", "projectPath": junk}]):
                self.assertIsNone(plugincache.installed_for("/planes/a"), junk)


class TheInstallItself(unittest.TestCase):
    def test_it_installs_when_nothing_covers_this_plane(self):
        calls: list = []
        with rows([]), spawns(calls):
            status, detail = plugincache.install("/planes/a")
        self.assertEqual(status, "installed")
        self.assertEqual([c[0] for c in calls], plugincache.install_argvs())
        self.assertIn("charter@charter", detail)

    def test_it_runs_from_the_plane_so_project_scope_lands_on_the_plane(self):
        """`claude plugin install --scope project` installs for the CWD. Run it from
        anywhere else and the plugin is installed for that other directory instead —
        silently, since the command succeeds either way."""
        import tempfile

        plane = Path(tempfile.mkdtemp(prefix="charter-installcwd-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(plane, ignore_errors=True))
        calls: list = []
        with rows([]), spawns(calls):
            plugincache.install(plane)
        self.assertEqual([c[1] for c in calls], [str(plane), str(plane)])

    def test_a_plane_path_that_is_not_a_directory_spawns_with_no_cwd(self):
        """Rather than handing `subprocess` a directory that is not there, which raises
        where a returned failure is wanted."""
        calls: list = []
        with rows([]), spawns(calls):
            plugincache.install("/no/such/plane")
        self.assertEqual([c[1] for c in calls], [None, None])

    def test_it_runs_nothing_when_an_install_already_covers_this_plane(self):
        calls: list = []
        with rows([{"id": "charter@charter", "scope": "user"}]), spawns(calls):
            status, _ = plugincache.install("/planes/a")
        self.assertEqual(status, "present")
        self.assertEqual(calls, [], "an install that is already there is not reinstalled")

    def test_it_installs_NOTHING_over_a_state_it_could_not_read(self):
        """The branch that matters most. An older `claude` answers here, and installing on
        a guess is how a second copy appears."""
        calls: list = []
        with mock.patch.object(plugincache, "available", return_value=True), \
                mock.patch.object(plugincache, "_claude_json", return_value=None), \
                spawns(calls):
            status, detail = plugincache.install("/planes/a")
        self.assertEqual(status, "unknown")
        self.assertEqual(calls, [])
        self.assertIn("claude plugin list", detail)

    def test_no_claude_on_PATH_is_an_ordinary_answer(self):
        """An opencode or Codex plane, or a terminal with no Claude Code. Never an error."""
        calls: list = []
        with mock.patch.object(plugincache, "available", return_value=False), spawns(calls):
            status, _ = plugincache.install("/planes/a")
        self.assertEqual(status, "unavailable")
        self.assertEqual(calls, [])

    def test_a_failing_install_is_reported_as_failed_and_never_as_done(self):
        calls: list = []
        with rows([]), spawns(calls, code=1, err="offline"):
            status, detail = plugincache.install("/planes/a")
        self.assertEqual(status, "failed")
        self.assertIn("offline", detail)

    def test_an_already_registered_marketplace_does_not_stop_the_install(self):
        """`claude plugin marketplace add` exits non-zero when the marketplace is already
        there, which is the ordinary state on a machine's second plane. Refusing to
        continue would make every install after the first impossible."""
        calls: list = []

        def run(cmd, cwd=None, check=True, **kw):
            calls.append(list(cmd))
            failed = "marketplace" in cmd
            return mock.Mock(returncode=1 if failed else 0, stdout="",
                             stderr="already exists" if failed else "")

        with rows([]), mock.patch.object(plugincache.util, "run", run):
            status, _ = plugincache.install("/planes/a")
        self.assertEqual(status, "installed")
        self.assertEqual(len(calls), 2, "the install step must still run")

    def test_both_steps_failing_names_the_install_error_first(self):
        """The two failures are not equally informative: "already registered" and "offline"
        fail the marketplace step identically, so the install's own error is the verdict —
        and the reader is told a second command was unhappy rather than left to guess."""
        calls: list = []
        with rows([]), spawns(calls, code=1, err="offline"):
            _status, detail = plugincache.install("/planes/a")
        self.assertLess(detail.index("plugin install"), detail.index("marketplace add"),
                        detail)


class InstallationHappensWhereSomebodyAskedForIt(PersonaIso):
    """#857's rule, one level up: `charter workspace list` must not install software.

    `init` and `doctor --fix` are the two doors, and `reinit` — which re-runs the wiring
    into the same plane — is deliberately not one of them. Asserted through
    `Harness.provision`, which is the only thing that can reach `plugincache.install`, so a
    new caller has to pass this file rather than merely avoid the two commands it names.
    """

    def _provisions(self, fn) -> int:
        seen: list = []
        with mock.patch.object(claude_code.ClaudeCodeHarness, "provision",
                               side_effect=lambda root: seen.append(root) or []):
            fn()
        return len(seen)

    def test_init_provisions(self):
        n = self._provisions(lambda: commands.cmd_init(
            SimpleNamespace(forge="github", owner="acme", host=None)))
        self.assertEqual(n, 1)

    def test_reinit_does_not(self):
        commands.cmd_init(SimpleNamespace(forge="github", owner="acme", host=None))
        n = self._provisions(lambda: commands.cmd_reinit(SimpleNamespace()))
        self.assertEqual(n, 0, "`reinit` re-runs the WIRING; it does not install software")

    def test_doctor_provisions_only_with_the_flag(self):
        commands.cmd_init(SimpleNamespace(forge="github", owner="acme", host=None))
        self.assertEqual(
            self._provisions(lambda: commands.cmd_doctor(SimpleNamespace(json=True))), 0)
        self.assertEqual(
            self._provisions(
                lambda: commands.cmd_doctor(SimpleNamespace(json=True, fix=True))), 1)

    def test_fix_outside_a_plane_installs_nothing_and_says_where_to_go(self):
        """The plugin is installed per PROJECT, so with no plane there is no directory to
        install it for. `doctor` outside a plane is a supported way to preflight a machine
        — it must not become a way to install into whatever directory you stood in."""
        import io
        from contextlib import redirect_stderr

        from charter import config

        config.use(self.tmp / "not-a-plane")
        buf = io.StringIO()
        n = 0
        with redirect_stderr(buf):
            n = self._provisions(lambda: commands._run_doctor_fix())
        self.assertEqual(n, 0)
        self.assertIn("charter init", buf.getvalue())

    def test_fix_with_nothing_to_do_says_so_rather_than_nothing(self):
        """A command that produced no output would read as one that did not run."""
        import io
        from contextlib import redirect_stderr

        commands.cmd_init(SimpleNamespace(forge="github", owner="acme", host=None))
        buf = io.StringIO()
        with redirect_stderr(buf), \
                mock.patch.object(commands, "_provision_harnesses", return_value=[]):
            commands._run_doctor_fix()
        self.assertIn("nothing to install", buf.getvalue())

    def test_fix_warns_about_an_install_it_could_not_vouch_for(self):
        """`unvouched` carries a sentence, and a `--fix` that failed must not read as a
        `--fix` that worked — the row it did not turn green is the verdict, but the reason
        is only here."""
        import io
        from contextlib import redirect_stderr

        commands.cmd_init(SimpleNamespace(forge="github", owner="acme", host=None))
        buf = io.StringIO()
        with redirect_stderr(buf), \
                mock.patch.object(commands, "_provision_harnesses",
                                  return_value=[("unvouched", "it did not work because X")]):
            commands._run_doctor_fix()
        self.assertIn("it did not work because X", buf.getvalue())

    def test_the_flag_is_reachable_from_the_command_line(self):
        """A flag argparse does not define is a flag nobody can type."""
        from charter import cli

        args = cli.build_parser().parse_args(["doctor", "--fix"])
        self.assertIs(args.func, commands.cmd_doctor)
        self.assertTrue(args.fix)
        self.assertFalse(cli.build_parser().parse_args(["doctor"]).fix)


class OnlyClaudeCodeHasAnArtifactCharterInstalls(unittest.TestCase):
    """The promise is repo-scoped (#881).

    opencode's plugin already arrives through `wire`; Codex's wiring is
    `~/.codex/config.toml`, machine-global, written only by `charter harness install codex`
    where running the command is the consent. `provision` is empty on the base class so
    that adding a harness cannot accidentally start writing a machine-global file.
    """

    def test_every_other_harness_provisions_nothing(self):
        for h in registry.all():
            if h.name == claude_code.NAME:
                continue
            self.assertEqual(h.provision(Path("/planes/a")), [], h.name)

    def test_a_machine_with_no_claude_gets_no_row_rather_than_a_warning(self):
        """An opencode or Codex plane has no Claude Code plugin to be missing, and a
        warning about one would be the cry-wolf failure `doctor.py` keeps returning to."""
        with mock.patch.object(plugincache, "available", return_value=False):
            self.assertEqual(
                claude_code.ClaudeCodeHarness().provision(Path("/planes/a")), [])

    def test_a_failed_install_is_a_sentence_init_warns_about_not_an_item_it_lists(self):
        """`unvouched` is the bucket #433 built: a path listed under "already present" is
        true about the filename and false about everything a reader takes from it."""
        calls: list = []
        with rows([]), spawns(calls, code=1, err="offline"):
            out = claude_code.ClaudeCodeHarness().provision(Path("/planes/a"))
        self.assertEqual([s for s, _ in out], ["unvouched"])
        self.assertIn("charter doctor --fix", out[0][1])


class TheDoctorRowReportsTheGapWithoutCryingWolf(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        from charter import config
        config.use(self.tmp)

    def test_an_absent_plugin_is_a_WARN_naming_the_exact_command(self):
        with rows([]):
            res = doctor.check_plugin_install()
        self.assertEqual(res.status, doctor.WARN)
        self.assertIn(doctor.PLUGIN_FIX_CMD, res.hint)

    def test_it_is_never_a_FAIL(self):
        """`cmd_doctor` exits non-zero only on FAIL, and that exit code is what makes the
        SessionStart preflight print — so a FAIL here reddens every CLI-only install, which
        `docs/install.md` supports and CI uses. A plane declaring `charter hook pretooluse`
        in its own settings is guarded with no plugin at all; `plane-root guard` is the row
        that answers whether the guard fires."""
        for entries in ([], [{"id": "charter@charter", "scope": "user"}],
                        [{"id": "other@thing", "scope": "user"}]):
            with rows(entries):
                self.assertNotEqual(doctor.check_plugin_install().status, doctor.FAIL,
                                    entries)

    def test_an_install_that_covers_this_plane_is_green_and_says_its_scope(self):
        entry = {"id": "charter@charter", "scope": "project",
                 "projectPath": str(self.tmp)}
        with rows([entry]):
            res = doctor.check_plugin_install()
        self.assertEqual(res.status, doctor.OK)
        self.assertIn("project", res.detail)

    def test_an_installed_but_DISABLED_plugin_is_not_a_tick(self):
        """#177: installed, enabled and wired are three states and only the third protects
        anything. 0.31.1 printed a tick over an installed-and-disabled plugin — the absence
        of a protection rendered as health — and this row must not do it again."""
        entry = {"id": "charter@charter", "scope": "project", "enabled": False,
                 "projectPath": str(self.tmp)}
        with rows([entry]):
            res = doctor.check_plugin_install()
        self.assertEqual(res.status, doctor.WARN)
        self.assertIn("DISABLED", res.detail)
        self.assertIn("claude plugin enable charter@charter", res.hint)

    def test_a_claude_that_does_not_report_enabled_is_not_reported_as_disabled(self):
        """Absent is not False. An older `claude` omitting the field would otherwise have
        every plane told its plugin is off — a problem invented out of a missing key, which
        is the same error pointing the other way."""
        entry = {"id": "charter@charter", "scope": "project", "projectPath": str(self.tmp)}
        with rows([entry]):
            self.assertEqual(doctor.check_plugin_install().status, doctor.OK)

    def test_fix_does_not_enable_a_plugin_somebody_turned_off(self):
        """charter reports; it does not revert a deliberate edit. Disabling a plugin is a
        choice, and a `--fix` that silently undid it is the thing `_ensure_statusline`
        exists not to do."""
        entry = {"id": "charter@charter", "scope": "project", "enabled": False,
                 "projectPath": str(self.tmp)}
        calls: list = []
        with rows([entry]), spawns(calls):
            status, _ = plugincache.install(self.tmp)
        self.assertEqual(status, "present")
        self.assertEqual(calls, [])

    def test_a_plane_with_no_claude_at_all_is_green_and_says_why(self):
        """opencode and Codex planes are supported installs with no Claude Code plugin to
        be missing. A yellow row on every one of them is the cry-wolf failure that costs
        the rows that matter."""
        with mock.patch.object(plugincache, "available", return_value=False):
            res = doctor.check_plugin_install()
        self.assertEqual(res.status, doctor.OK)
        self.assertIn("no `claude` on PATH", res.detail)

    def test_a_green_row_keeps_no_remedy_back_in_a_hint(self):
        """`Result.render` drops the hint entirely at OK, so guidance written there is
        invisible while looking shipped — #856, and `TestAGreenRowKeepsNothingBack` holds
        every check to it. Stated here too because this row has three green branches."""
        cases = [mock.patch.object(plugincache, "available", return_value=False),
                 rows([{"id": "charter@charter", "scope": "user"}])]
        for ctx in cases:
            with ctx:
                res = doctor.check_plugin_install()
            if res.status == doctor.OK:
                self.assertEqual(res.hint, "", res.detail)

    def test_an_unreadable_list_is_not_checked_rather_than_a_tick(self):
        """#171: a check that silently does nothing is worse than no check, and the
        population answering here is the one most likely to be running a stale plugin."""
        with mock.patch.object(plugincache, "available", return_value=True), \
                mock.patch.object(plugincache, "_claude_json", return_value=None):
            res = doctor.check_plugin_install()
        self.assertEqual(res.status, doctor.WARN)
        self.assertIn("not checked", res.detail)

    def test_a_raising_check_costs_one_row_and_not_the_whole_preflight(self):
        """`doctor._checks()` is an eager list literal with no per-check guard, so one
        raising check returns NO rows — and `hooks/hooks.json` renders a non-zero `charter
        doctor` as "charter preflight failed" at every SessionStart. This row found that
        out the hard way on its first full run."""
        with mock.patch.object(plugincache, "available",
                               side_effect=RuntimeError("something unforeseen")):
            res = doctor.check_plugin_install()
            rows_ = doctor.run_all()
        self.assertEqual(res.status, doctor.WARN)
        self.assertIn("not checked", res.detail)
        self.assertGreater(len(rows_), 20)

    def test_the_row_is_registered_so_doctor_actually_runs_it(self):
        """A check nobody calls is a check that does not exist, and `name_width` sizes the
        report from a second spelling of the list — so the two must agree."""
        with rows([]):
            self.assertIn("plugin install", [r.name for r in doctor.run_all()])
        self.assertIn("plugin install", doctor.check_names())


class SkewRefusesInDoctorAndOnlyWarnsInAHook(unittest.TestCase):
    """The asymmetry #881 asked to keep, pinned so it cannot quietly become symmetric.

    The two artifacts update through different channels, so skew is normal and will happen
    mid-session. A hook that hard-failed on it would refuse every tool call in that session
    — a cosmetic mismatch turned into an outage — so the hook speaks once and returns, and
    `doctor` is the surface that blocks.
    """

    def _newer(self) -> str:
        from charter import hooks
        major, minor, patch = (int(p) for p in hooks.MIN_PLUGIN_VERSION.split(".")[:3])
        return f"{major + 1}.{minor}.{patch}"

    def test_doctor_fails_on_a_newer_plugin(self):
        import os
        from charter import hooks

        with mock.patch.dict(os.environ, {"CLAUDE_PLUGIN_ROOT": "/plugin"}), \
                mock.patch.object(Path, "read_text",
                                  return_value=json.dumps({"version": self._newer()})):
            res = doctor.check_plugin_skew()
        self.assertEqual(res.status, doctor.FAIL)
        self.assertIn(hooks.MIN_PLUGIN_VERSION, res.detail)

    def test_the_hook_says_it_and_carries_on(self):
        """`skew_message` returns a STRING for the hook to speak. It raises nothing and
        denies nothing — the guard keeps guarding, which is the whole point."""
        from charter import hooks

        self.assertIsNotNone(hooks.skew_message(self._newer()))
        self.assertIsNone(hooks.skew_message(hooks.MIN_PLUGIN_VERSION))

    def test_no_hook_handler_refuses_on_skew(self):
        """The negative half, and the one that would actually brick a session. `dispatch`
        is where the skew check sits in front of every handler; nothing in that path may
        turn a version mismatch into a denial."""
        import inspect
        from charter import hooks

        src = inspect.getsource(hooks._queue_plugin_notices)
        for banned in ("sys.exit", "return 2", "deny"):
            self.assertNotIn(banned, src,
                             "a hook that blocks on skew bricks every tool call")


if __name__ == "__main__":
    unittest.main()
