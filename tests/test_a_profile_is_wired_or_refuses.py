"""A profile is wired or it refuses — detection per kind, the fix, and where it runs.

Charter's guard lives in the harness's own config folder: Claude Code's plugin, Codex's
plugin plus its hook trust, opencode's shim. A profile names another folder, so every one
of those can be absent for a profile while the operator's default folder is wired — and a
chat that looks guarded and is not is the failure this whole task exists to stop. Measured
2026-09-11 and again 2026-09-12: under an empty `CLAUDE_CONFIG_DIR` charter's plugin is not
merely disabled, it is unknown to that folder.

**Wiring is detected by ASKING the harness under the profile's own environment**, never by
reading the profile's variable names: one account can be reached through variables that do
or do not move the plugin (`XDG_DATA_HOME` moves opencode's login and leaves its plugins
alone). So the probe is `claude plugin list --json` / `opencode debug config` run with the
profile's `command` and `env`, and `$CODEX_HOME/config.toml` read at the profile's home.

No test here runs a real harness. The probe seams are `charter.util.run` — which
`plugincache._claude_json` and the opencode probe both go through — and the git call
`profiles.ignore_check` makes, which :func:`spawns` deliberately lets through to the real
`util.run` so a fixture's `charter.local.toml` is still checked against a real repository.
"""

from __future__ import annotations

import io
import json
import os
import shlex
import shutil
import time
import unittest
from contextlib import contextmanager, redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, commands_frame, commands_harness, config, contain, doctor
from charter import plugincache, profiles, profiletrust, util, wiring
from charter.frame import launcher, reopen as reopen_state
from charter.harness import claude_code, codex, opencode
from tests import _claudeguard
from tests._isolation import (APipe as _APipe, ATerminal as _ATerminal, PersonaIso,
                              Typed as _Typed, approve_every_profile, approve_profile,
                              declare_profiles, make_plane)
from tests.test_a_profile_launch_is_refused_before_tmux import _ALaunchNamesAProfile

REPO = Path(__file__).resolve().parents[1]

#: kind → registry NAME, asked of the registry rather than written down twice.
KINDS = {p.kind: p.harness for p in profiles.builtins().values()}


def make_profile(name, kind="claude", command=None, env=(), source=profiles.LOCAL_FILE):
    """A `Profile` built by hand, for the cases whose subject is detection alone.

    The fixtures that drive `init`, `harness install` and a launch declare a real
    `charter.local.toml` instead (`declare_profiles`): those paths read the file.
    """
    return profiles.Profile(name=name, kind=kind, harness=KINDS[kind],
                            command=tuple(command or [kind]),
                            env=tuple(sorted(env)), source=source)


class Probe(SimpleNamespace):
    """One recorded spawn: its argv, cwd and the environment it was given."""


@contextmanager
def spawns(calls, answer=None, *, rc=0, raises=None):
    """Answer every harness probe from *answer*, recording each into *calls*.

    A dispatcher rather than a blanket patch: `profiles.ignore_check` runs
    `git --no-optional-locks status` through this very function, and a fixture whose git
    call was swallowed would be testing the refusal instead of the profile.

    *answer* is either a string (stdout for every probe) or a callable taking the argv.
    """
    real = util.run

    def run(cmd, cwd=None, check=True, env=None, timeout=None, **kw):
        cmd = list(cmd)
        if cmd[:1] == ["git"]:
            return real(cmd, cwd=cwd, check=check, env=env, timeout=timeout, **kw)
        # Only the names a profile can set, never the whole environment: `util.run`'s
        # ``env`` is an overlay on this process's, and an assertion message that dumped it
        # would print the operator's own shell — tokens included — into a test report.
        seen = {k: v for k, v in (env or {}).items()
                if k in ("CLAUDE_CONFIG_DIR", "CODEX_HOME", "XDG_CONFIG_HOME",
                         "CHARTER_HARNESS", "CHARTER_HARNESS_PROFILE")}
        calls.append(Probe(argv=cmd, cwd=cwd, env=seen))
        if raises is not None:
            raise raises
        out = answer(cmd) if callable(answer) else (answer or "")
        return SimpleNamespace(returncode=rc, stdout=out, stderr="")

    with mock.patch.object(util, "run", run), \
            mock.patch.object(plugincache, "available", lambda *a, **k: True):
        yield


class _TempDir:
    """A directory outside every plane, removed afterwards — for `enterContext`."""

    def __enter__(self):
        import tempfile

        self.path = tempfile.mkdtemp(prefix="charter-outside-")
        return self.path

    def __exit__(self, *exc):
        shutil.rmtree(self.path, ignore_errors=True)
        return False


def listing(*entries) -> str:
    """`claude plugin list --json`'s answer for *entries*, each already a dict."""
    return json.dumps(list(entries))


def entry(scope="project", enabled=True, project=None, pid=plugincache.PLUGIN_ID) -> dict:
    """One `claude plugin list --json` row. `installedAt` because a covering entry is an
    INSTALL RECORD — a settings-file disable creates none (measured 2026-09-12)."""
    row = {"id": pid, "scope": scope, "enabled": enabled,
           "installedAt": "2026-09-12T00:00:00Z", "version": "0.60.0"}
    if project is not None:
        row["projectPath"] = str(project)
    return row


#: Two declared profiles whose names sort the other way round from the order they are
#: declared in, so the listing's order is shown to be its own and not the file's.
DECLARED_WITH_A_NAME_AFTER_ITS_SIBLING = """
[harness.claude-work]
kind = "claude"
command = ["claude"]

[harness.aaa-codex]
kind = "codex"
command = ["codex"]
"""

#: The harness programs a listing depends on. `wiring.listed` shows a built-in only when its
#: program is on `PATH`, and `tests/_claudeguard` fakes `claude` and `opencode` but not
#: `codex` — so a case that counts rows says which of the three this machine "has" rather
#: than inheriting whatever the machine running it has (B5; `CONTRIBUTING.md`, *your machine
#: is not the runner*).
HARNESS_PROGRAMS = ("claude", "codex", "opencode")


@contextmanager
def on_path(*present: str):
    """`shutil.which` answering for the three harness programs as *present* says, and for
    every other program — `git`, `tmux` — as the machine does."""
    real = shutil.which

    def which(cmd, *a, **k):
        if cmd in HARNESS_PROGRAMS:
            return f"/usr/bin/{cmd}" if cmd in present else None
        return real(cmd, *a, **k)

    with mock.patch.object(wiring.shutil, "which", which):
        yield


# --------------------------------------------------------------------------- #
# Claude Code                                                                  #
# --------------------------------------------------------------------------- #


class ClaudeCodeIsAskedUnderTheProfilesEnvironment(PersonaIso, unittest.TestCase):
    """`claude plugin list --json`, run as the profile would run `claude`.

    Measured 2026-09-12 on 2.1.269, and it is the fact the whole class rests on: the `enabled`
    field is the EFFECTIVE setting for the plugin id resolved at the probe's own cwd — local
    over project over user — and every listed entry of that id carries the same value. So the
    probe must be given the chat's directory, and a settings-file disable reaches charter
    without charter reading a settings file (ruling 36's "unless").
    """

    def setUp(self):
        super().setUp()
        self.here = self.tmp / "here"
        self.here.mkdir()
        # `$HOME` pinned, so where `~/.claude-alt` expands to is a LITERAL below rather than
        # the same `expanduser` the code runs agreeing with itself (B4).
        self.home = self.tmp / "home"
        self.enterContext(mock.patch.dict(os.environ, {"HOME": str(self.home)}))
        self.p = make_profile("claude-alt", env=[("CLAUDE_CONFIG_DIR", "~/.claude-alt")])
        # These cases are about what the probe answers, which is the question AFTER the
        # gate: a profile the operator has not approved is never probed, and
        # `NothingUnapprovedIsRun` is where that is the subject.
        approve_profile(self, self.p)

    def test_the_probe_runs_the_profiles_command_with_its_env(self):
        """Never `claude` and never charter's own environment: a profile exists to name
        another binary and another folder, and a probe that ignored either would answer for
        the session the operator is NOT about to start."""
        calls = []
        with spawns(calls, listing(entry(project=self.here))):
            wiring.detect(self.p, cwd=self.here)
        self.assertEqual(calls[0].argv, ["claude", "plugin", "list", "--json"])
        self.assertEqual(calls[0].env["CLAUDE_CONFIG_DIR"], str(self.home / ".claude-alt"))
        self.assertEqual(str(calls[0].cwd), str(self.here))

    def test_the_probe_looks_the_command_up_on_the_profiles_own_path(self):
        """A2: the launcher finds a profile's command on the `PATH` its `env` sets
        (`launcher.refusal`), so the probe has to look there too. Looked up on this
        process's `PATH` instead, a profile whose program lives only on its own would read
        "could not be asked" and refuse a launch that would have found the program."""
        bin_dir = self.tmp / "profile-bin"
        bin_dir.mkdir()
        program = bin_dir / "claude-only-here"
        program.write_text("#!/bin/sh\nexit 0\n")
        program.chmod(0o755)
        p = make_profile("claude-path", command=["claude-only-here"],
                         env=[("PATH", str(bin_dir))])
        approve_profile(self, p)
        self.assertIsNone(shutil.which("claude-only-here"))
        calls = []
        real = util.run

        def run(cmd, *a, **kw):
            if list(cmd)[:1] == ["git"]:
                return real(cmd, *a, **kw)
            calls.append(list(cmd))
            return SimpleNamespace(returncode=0, stdout=listing(entry(project=self.here)),
                                   stderr="")

        with mock.patch.object(plugincache, "available", _claudeguard.REAL_AVAILABLE), \
                mock.patch.object(util, "run", run):
            w = wiring.detect(p, cwd=self.here)
        self.assertEqual(calls, [["claude-only-here", "plugin", "list", "--json"]])
        self.assertEqual(w.state, wiring.WIRED)

    def test_an_enabled_install_covering_the_directory_is_wired(self):
        calls = []
        with spawns(calls, listing(entry(project=self.here))):
            w = wiring.detect(self.p, cwd=self.here)
        self.assertEqual(w.state, wiring.WIRED)
        self.assertEqual(w.fix, "")

    def test_an_install_covering_the_PLANE_covers_a_chat_in_its_workspace(self):
        """D3, measured: `charter init` installs at PROJECT scope for the plane root, and a
        chat's directory is `workspaces/<ws>/` — a different `projectPath`, so
        `plugincache.covers` alone answers False for it. Charter mirrors the plane's
        `enabledPlugins` into that directory (`claude_code.WORKSPACE_KEYS`), and the probe's
        own `enabled` is resolved there, so the install record that covers the PLANE is the
        one that answers for the chat."""
        make_plane(self)
        ws = config.ROOT / "workspaces" / "w"
        ws.mkdir(parents=True)
        record = entry(project=config.ROOT)
        self.assertFalse(plugincache.covers(record, ws))
        with spawns([], listing(record)):
            self.assertEqual(wiring.detect(self.p, cwd=ws).state, wiring.WIRED)

    def test_an_install_for_somebody_elses_checkout_is_not_this_planes(self):
        """The half of the rule above that has to keep working: `covers` is what stops
        "already installed" being printed over a plane with no plugin at all."""
        with spawns([], listing(entry(project=self.tmp / "elsewhere"))):
            w = wiring.detect(self.p, cwd=self.here)
        self.assertEqual(w.state, wiring.UNWIRED)

    def test_a_long_names_fix_is_whole_and_its_sentence_is_bounded(self):
        """A `Wiring` is escaped and never clipped (a `doctor` row counts what it hides, and
        could not count a cut made before it); the SENTENCE bounds the name it quotes."""
        long_name = "l" * 161
        p = approve_profile(self, make_profile(long_name))
        for answer, rc in ((listing(), 0), ("", 1)):
            with self.subTest(rc=rc), spawns([], answer, rc=rc):
                w = wiring.detect(p, cwd=self.here)
                said = wiring.refusal(p, cwd=self.here)
            self.assertEqual(w.fix, f"charter harness install {long_name}")
            self.assertIn(f"profile '{'l' * 160}...'", said)

    def test_no_entry_is_unwired(self):
        with spawns([], listing()):
            w = wiring.detect(self.p, cwd=self.here)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertEqual(w.fix, "charter harness install claude-alt")

    def test_a_disabled_install_is_unwired_and_the_fix_is_the_enable(self):
        """NOT `charter harness install`: `plugincache.install` answers `present` for an
        install that exists, so pointing back at it would change nothing and print a fix
        that loops — review 7's objection, one harness over."""
        with spawns([], listing(entry(project=self.here, enabled=False))):
            w = wiring.detect(self.p, cwd=self.here)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("disabled", w.detail)
        self.assertIn("plugin enable", w.fix)
        self.assertNotIn("charter harness install", w.fix)

    def test_the_enable_names_its_scope_and_the_directory_it_must_run_in(self):
        """A8, measured on claude 2.1.270 in throwaway folders (2026-09-13): `plugin enable
        --scope local` run FROM the chat's directory undid a disable in that directory's
        `settings.local.json`, in its `settings.json`, and in a git plane root's
        `settings.local.json`; `--scope project` and `--scope user` each exited 1 over a
        local disable and changed nothing — review 7's loop. So the fix says the scope, and
        says where to stand, as one line that can be pasted."""
        with spawns([], listing(entry(project=self.here, enabled=False))):
            w = wiring.detect(self.p, cwd=self.here)
        self.assertEqual(shlex.split(w.fix),
                         ["cd", str(self.here), "&&",
                          f"CLAUDE_CONFIG_DIR={self.home / '.claude-alt'}",
                          "claude", "plugin", "enable", plugincache.PLUGIN_ID,
                          "--scope", "local"])

    def test_a_fix_for_a_directory_with_a_space_can_still_be_pasted(self):
        """A11: every path and word is shell-quoted, so the line survives the paste."""
        here = self.tmp / "a dir; with parts"
        here.mkdir()
        p = make_profile("claude-sp", env=[("CLAUDE_CONFIG_DIR", str(self.tmp / "c c"))])
        approve_profile(self, p)
        with spawns([], listing(entry(project=here, enabled=False))):
            w = wiring.detect(p, cwd=here)
        words = shlex.split(w.fix)
        self.assertEqual(words[:3], ["cd", str(here), "&&"])
        self.assertEqual(words[3], f"CLAUDE_CONFIG_DIR={self.tmp / 'c c'}")

    def test_a_plane_install_the_chats_directory_disables_is_not_wired(self):
        """A7, and the reason the plane-root coverage rule is safe to keep: the record that
        covers the plane carries the `enabled` the binary resolved AT THE CHAT'S DIRECTORY,
        so a plane install that directory disables reads as disabled — counted as covering,
        and still refused."""
        make_plane(self)
        ws = config.ROOT / "workspaces" / "w"
        ws.mkdir(parents=True)
        record = entry(project=config.ROOT, enabled=False)
        self.assertFalse(plugincache.covers(record, ws))
        with spawns([], listing(record)):
            w = wiring.detect(self.p, cwd=ws)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("disabled", w.detail)

    def test_a_disabled_user_entry_listed_first_does_not_hide_an_enabled_project_entry(self):
        """Review 6: `installed_for` returns the FIRST covering entry, and this machine lists
        user, project and local entries side by side."""
        with spawns([], listing(entry(scope="user", enabled=False),
                                entry(scope="project", project=self.here, enabled=True))):
            self.assertEqual(wiring.detect(self.p, cwd=self.here).state, wiring.WIRED)

    def test_a_local_disable_over_a_user_enable_is_unwired(self):
        """Ruling 28. Today the binary resolves this itself and hands every entry the same
        `enabled` — so this pins the rule for the day it stops doing that, and the rule
        chosen is the one that fails closed."""
        with spawns([], listing(entry(scope="user", enabled=True),
                                entry(scope="local", project=self.here, enabled=False))):
            w = wiring.detect(self.p, cwd=self.here)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("local", w.detail)

    def test_a_scope_charter_does_not_recognise_sorts_last_rather_than_raising(self):
        """`_SCOPE_ORDER` is charter's reading of three scopes the CLI documents. A fourth
        one — a newer Claude Code, a row charter has never seen — must not take the row down,
        and must not outrank a scope charter does understand."""
        with spawns([], listing(entry(scope="cowork", project=self.here, enabled=False),
                                entry(scope="user", enabled=True))):
            self.assertEqual(wiring.detect(self.p, cwd=self.here).state, wiring.WIRED)

    def test_a_project_enable_over_a_user_disable_is_wired(self):
        """The same pair in the other order, so the order of the list decides nothing."""
        with spawns([], listing(entry(scope="project", project=self.here, enabled=True),
                                entry(scope="user", enabled=False))):
            self.assertEqual(wiring.detect(self.p, cwd=self.here).state, wiring.WIRED)

    def test_the_settings_disable_the_binary_already_reports_needs_no_second_reader(self):
        """Ruling 36, settled by measurement rather than by the fallback it provided for.

        2.1.269, an enabled USER-scope install and a disable written ONLY to
        `<cwd>/.claude/settings.local.json` — and only to `<cwd>/.claude/settings.json` —
        each flipped the listed entry's `enabled` to false. So charter reads no settings file
        of its own: a second reader would answer for a different merge than the binary's
        (measured: `<ancestor>/.claude/settings.json` does NOT reach a session below it,
        while `settings.local.json` does), and a wrong UNWIRED refuses a chat that would have
        been guarded.
        """
        (self.here / ".claude").mkdir()
        (self.here / ".claude" / "settings.local.json").write_text(
            json.dumps({"enabledPlugins": {plugincache.PLUGIN_ID: False}}))
        with spawns([], listing(entry(scope="user", enabled=False))):
            self.assertEqual(wiring.detect(self.p, cwd=self.here).state, wiring.UNWIRED)
        with spawns([], listing(entry(scope="user", enabled=True))):
            self.assertEqual(wiring.detect(self.p, cwd=self.here).state, wiring.WIRED)

    def probe_in(self, said: str) -> str:
        """What `CANNOT_TELL` printed after `check by hand:` — the probe, and not the detail.

        Asked apart from the sentence because the detail names the same command: an
        assertion against the whole refusal passes while `{probe}` says something else
        entirely, which is how a sentence that ends "check by hand:" and then names the wrong
        file gets shipped.
        """
        self.assertIn("check by hand: ", said)
        return said.split("check by hand: ", 1)[1]

    def test_an_unreadable_list_is_unknown_and_refuses(self):
        with spawns([], "", rc=1):
            w = wiring.detect(self.p, cwd=self.here)
            self.assertEqual(w.state, wiring.UNKNOWN_STATE)
            said = wiring.refusal(self.p, cwd=self.here)
        self.assertIn("could not ask", said)
        self.assertIn("claude plugin list --json", self.probe_in(said))

    def test_a_probe_that_times_out_refuses(self):
        """Ruling 12 and review 10: an unknown is not a pass, and nothing raises out of it."""
        with spawns([], raises=util.ProcTimeout(["claude"], 5.0)):
            w = wiring.detect(self.p, cwd=self.here)
            said = wiring.refusal(self.p, cwd=self.here)
        self.assertEqual(w.state, wiring.UNKNOWN_STATE)
        self.assertIn("could not ask", said)

    def test_a_wrapper_scripts_answer_is_the_one_used(self):
        p = approve_profile(self, make_profile("wrapped", command=["/opt/claude-wrap"]))
        calls = []
        with spawns(calls, listing()):
            wiring.detect(p, cwd=self.here)
        self.assertEqual(calls[0].argv[0], "/opt/claude-wrap")

    def test_the_detail_names_the_folder_the_probe_asked_about(self):
        """#970's rule, with #970's resolver: `doctor`'s `plane-root guard` row and this one
        name ONE folder for one environment, so two rows cannot disagree about which folder
        a session reads."""
        with spawns([], listing(entry(project=self.here))):
            w = wiring.detect(self.p, cwd=self.here)
        self.assertIn(str(self.home / ".claude-alt"), w.detail)

    def test_a_nul_byte_in_the_environment_is_unknown_and_never_a_traceback(self):
        """A1: `exec` refuses a NUL in an environment value with `ValueError`, before
        anything runs — and a profile's `env` is a file a chat can write. The real
        `util.run`, so it is the refusal and not a stand-in that is caught."""
        p = approve_profile(self, make_profile(
            "claude-nul", env=[("CLAUDE_CONFIG_DIR", "/tmp/a\x00b")]))
        with mock.patch.object(plugincache, "available", lambda *a, **k: True):
            w = wiring.detect(p, cwd=self.here)
            said = wiring.refusal(p, cwd=self.here)
        self.assertEqual(w.state, wiring.UNKNOWN_STATE)
        self.assertIn("could not ask", said)
        self.assertNotIn("\x00", said)


class TheResolverGainsAnEnvironmentRatherThanATwin(unittest.TestCase):
    """#970's `config_home`/`global_config_file` answer for a mapping charter hands them.

    A second resolver is the thing not to build: two rules for "which folder does Claude Code
    read" drift, and the drift is invisible until a row is green about the wrong folder.
    """

    def test_config_home_reads_the_mapping_it_is_given(self):
        self.assertEqual(claude_code.config_home({"CLAUDE_CONFIG_DIR": "/tmp/alt"}),
                         Path("/tmp/alt"))

    def test_an_empty_value_still_names_the_working_directory(self):
        """`??`, not `||` — read off the 2.1.268 binary and pinned by #970. An empty value
        is KEPT, and a kinder rule would answer for a different folder than the one Claude
        Code reads."""
        self.assertEqual(claude_code.config_home({"CLAUDE_CONFIG_DIR": ""}), Path(""))

    def test_no_mapping_still_means_the_process_environment(self):
        with mock.patch.dict(os.environ, {"CLAUDE_CONFIG_DIR": "/tmp/proc"}, clear=False):
            self.assertEqual(claude_code.config_home(), Path("/tmp/proc"))

    def test_the_global_config_file_follows_the_same_mapping(self):
        self.assertEqual(claude_code.global_config_file({"CLAUDE_CONFIG_DIR": "/tmp/alt"}),
                         Path("/tmp/alt/.claude.json"))


# --------------------------------------------------------------------------- #
# Codex                                                                        #
# --------------------------------------------------------------------------- #


#: A trust entry for one of charter's GUARD hooks — the Bash guard, as Codex keys it.
TRUST_KEY = f"{plugincache.PLUGIN_ID}:hooks/hooks.json:pre_tool_use:0:0"
#: A trust entry for one of charter's hooks that is not a guard (SessionStart's reconcile).
NOT_A_GUARD_KEY = f"{plugincache.PLUGIN_ID}:hooks/hooks.json:session_start:0:0"


def codex_config(*, plugin=True, policy=True, trust=True) -> str:
    doc = []
    if plugin:
        doc.append(f'[plugins."{plugincache.PLUGIN_ID}"]\nenabled = true\n')
    if policy:
        doc.append('[shell_environment_policy]\nset = { CHARTER_HARNESS = "codex" }\n')
    if trust:
        doc.append(f'[hooks.state."{TRUST_KEY}"]\ntrusted_hash = "sha256:abc"\n')
    return "\n".join(doc)


class CodexIsReadAtTheProfilesHome(PersonaIso, unittest.TestCase):
    """Codex needs no subprocess: the three marks are all in `$CODEX_HOME/config.toml`.

    All three, because each is separately absent on a fresh home and any one of them missing
    means charter is not running there: the plugin declares the hooks, the policy line is the
    only thing that tells a Codex shell which harness it is, and an untrusted hook is inert.
    """

    def setUp(self):
        super().setUp()
        self.home = self.tmp / "codex-home"
        self.home.mkdir()
        self.p = approve_profile(self, make_profile("codex-alt", kind="codex",
                                                    env=[("CODEX_HOME", str(self.home))]))

    def write(self, body: str):
        (self.home / "config.toml").write_text(body)

    def test_all_three_marks_are_wired(self):
        self.write(codex_config())
        w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.WIRED)
        self.assertIn(str(self.home), w.detail)

    def test_an_empty_home_is_unwired(self):
        self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state, wiring.UNWIRED)

    def test_the_plugin_without_the_policy_line_is_unwired(self):
        self.write(codex_config(policy=False))
        w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("shell_environment_policy", w.detail)
        # No table at all: charter writes the whole thing, so its own command IS the fix.
        self.assertEqual(w.fix, "charter harness install codex-alt")

    def test_a_policy_that_names_some_other_variable_is_not_charters(self):
        self.write(codex_config(policy=False)
                   + '\n[shell_environment_policy]\nset = { SOMETHING_ELSE = "codex" }\n')
        w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("CHARTER_HARNESS", w.detail)

    def test_untrusted_hooks_are_unwired(self):
        """Codex trusts hooks by hash, so a plugin nobody approved is installed and inert —
        which reads exactly like wired to anything that stops at the plugin table."""
        self.write(codex_config(trust=False))
        w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("approve", w.fix.lower() + w.detail.lower())

    def test_one_trust_entry_is_the_measured_rule(self):
        """Measured 2026-09-12 on the operator's own wired `~/.codex/config.toml`: Codex
        writes a trust entry per hook LAZILY, as each first fires — 12 of the plugin's 18
        keys were present on a machine that has been running charter under Codex for weeks.
        So "an entry for every hook key" — the plan's fallback — would call a wired machine
        unwired, and the rule is "charter's hooks were approved here at least once".
        `trusted_hash` itself cannot be recomputed: the command string, the hook object as
        JSON in three spellings, the matcher group and the joined commands were all tried
        against a real hash and none matched."""
        self.write(codex_config())
        self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state, wiring.WIRED)

    def test_a_disabled_plugin_is_unwired_whatever_else_is_there(self):
        """A4: the plan's first mark, `plugins."charter@charter".enabled is True`, on its
        own — the policy line and a trusted guard both present, and the plugin switched off."""
        self.write(codex_config().replace("enabled = true", "enabled = false"))
        w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("not an enabled plugin", w.detail)

    def test_a_trusted_hook_that_is_not_a_guard_does_not_make_it_wired(self):
        """A5, ruling 7: "wired" is charter's GUARD actually running. Codex trusts hooks one
        at a time, so an approved SessionStart reconcile beside an untrusted Bash guard is a
        home where nothing stops a shell command."""
        self.write(codex_config(trust=False)
                   + f'\n[hooks.state."{NOT_A_GUARD_KEY}"]\ntrusted_hash = "sha256:abc"\n'
                   + f'\n[hooks.state."{TRUST_KEY}"]\nenabled = false\n')
        w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("no guard hook of charter's is trusted", w.detail)

    def test_a_config_in_a_shape_codex_never_writes_is_unknown_and_never_a_traceback(self):
        """A1. Each of these parses as TOML, and each is a line a chat can write: read
        without checking the shape first, every one of them was an `AttributeError` out of a
        launch. They read as "could not tell", with the key named."""
        hostile = {
            f'plugins."{plugincache.PLUGIN_ID}" = true\n': f'plugins."{plugincache.PLUGIN_ID}"',
            "plugins = 3\n": "plugins",
            f'[hooks.state]\n"{TRUST_KEY}" = "trusted"\n': "hooks.state",
            'hooks = "all of them"\n': "hooks",
            "[hooks]\nstate = 1\n": "hooks.state",
            "shell_environment_policy = 1\n": "shell_environment_policy",
            '[shell_environment_policy]\nset = "CHARTER_HARNESS=codex"\n':
                "shell_environment_policy.set",
        }
        for body, key in hostile.items():
            with self.subTest(body=body):
                self.write(body)
                w = wiring.detect(self.p, cwd=self.tmp)
                self.assertEqual(w.state, wiring.UNKNOWN_STATE)
                self.assertIn(key, w.detail)
                self.assertIn("could not ask", wiring.refusal(self.p, cwd=self.tmp))

    def test_a_trust_entry_of_the_wrong_shape_is_named_rather_than_skipped(self):
        """The ledger entry itself: `trusted_hash` read off a string is the traceback, and a
        chat that wanted a home to read as wired would write exactly that."""
        self.write(codex_config(trust=False) + f'\n[hooks.state]\n"{TRUST_KEY}" = "yes"\n')
        w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNKNOWN_STATE)
        self.assertIn("pre_tool_use", w.detail)

    def test_a_trusted_hash_that_is_not_a_string_is_not_trust(self):
        self.write(codex_config(trust=False)
                   + f'\n[hooks.state."{TRUST_KEY}"]\ntrusted_hash = 1\n')
        self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state, wiring.UNWIRED)

    def test_a_nul_byte_in_the_home_is_unknown_and_never_a_traceback(self):
        """A1: `open` refuses a NUL in a path with `ValueError`."""
        p = approve_profile(self, make_profile("codex-nul", kind="codex",
                                               env=[("CODEX_HOME", "/tmp/a\x00b")]))
        self.assertEqual(wiring.detect(p, cwd=self.tmp).state, wiring.UNKNOWN_STATE)
        self.assertIn("could not ask", wiring.refusal(p, cwd=self.tmp))

    def test_the_codex_steps_are_a_list_each_pasteable_whatever_the_home_is_called(self):
        """A11: a home holding `; ` split a joined sentence in the wrong place, and a home
        with a space printed a `CODEX_HOME=` a shell would cut in two."""
        home = self.tmp / "my home; really"
        steps = wiring.codex_steps(home)
        self.assertEqual(len(steps), len(wiring.CODEX_COMMANDS) + 1)
        for step, argv in zip(steps, wiring.CODEX_COMMANDS):
            self.assertEqual(shlex.split(step), [f"CODEX_HOME={home}", *argv])
        self.assertEqual(steps[-1], wiring.CODEX_APPROVE)

    def test_another_plugins_trust_entry_is_not_charters(self):
        self.write(codex_config(trust=False)
                   + '\n[hooks.state."other@other:hooks/hooks.json:stop:0:0"]\n'
                     'trusted_hash = "sha256:def"\n')
        self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state, wiring.UNWIRED)

    def test_the_default_home_is_the_one_codex_reads_with_no_env(self):
        """No `CODEX_HOME` on the profile and none in the process: `~/.codex` and not a
        sandbox left over from another fixture."""
        home = self.tmp / "fakehome"
        (home / ".codex").mkdir(parents=True)
        (home / ".codex" / "config.toml").write_text(codex_config())
        p = make_profile("codex", kind="codex", source=profiles.BUILTIN)
        with mock.patch.dict(os.environ, {"HOME": str(home), "PATH": os.environ["PATH"]},
                             clear=True):
            self.assertEqual(wiring.detect(p, cwd=self.tmp).state, wiring.WIRED)

    def test_a_malformed_config_is_unknown(self):
        self.write("[x")
        w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNKNOWN_STATE)
        self.assertIn("could not ask", wiring.refusal(self.p, cwd=self.tmp))

    def test_the_fix_once_charter_has_written_its_line_is_codexs_own_commands(self):
        """Review 7's objection, one mark further on. Once `shell_environment_policy` names
        the harness, `charter harness install` has done everything charter can do — the
        plugin install and the hook approval are Codex's, and pointing back at charter's
        command would print a fix that changes nothing."""
        self.write(codex_config(plugin=False, trust=False))
        w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn(f"CODEX_HOME={shlex.quote(str(self.home))}", w.fix)
        self.assertIn("codex plugin add", w.fix)
        self.assertIn(wiring.CODEX_APPROVE, w.fix)
        self.assertNotIn("charter harness install", w.fix)

    def test_the_fix_for_a_foreign_policy_table_is_the_line_not_the_command(self):
        """Review 7: `codex.install()` answers `present` for ANY `[shell_environment_policy]`,
        so naming `charter harness install` here would print a fix that changes nothing."""
        self.write(f'[plugins."{plugincache.PLUGIN_ID}"]\nenabled = true\n\n'
                   '[shell_environment_policy]\ninherit = "core"\n\n'
                   f'[hooks.state."{TRUST_KEY}"]\ntrusted_hash = "sha256:abc"\n')
        w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn('CHARTER_HARNESS = "codex"', w.fix)
        self.assertNotIn("charter harness install", w.fix)

    def test_nothing_charter_reads_writes_into_the_home(self):
        """Detection is a READ. A probe that wrote would change the answer it is asking
        about, and would do it in somebody else's account folder."""
        self.write(codex_config())
        before = sorted(p.name for p in self.home.iterdir())
        wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(sorted(p.name for p in self.home.iterdir()), before)


# --------------------------------------------------------------------------- #
# opencode                                                                     #
# --------------------------------------------------------------------------- #


class OpencodeIsAskedUnderTheProfilesConfigHome(PersonaIso, unittest.TestCase):
    """All three marks (ruling 26), and the third is the one a byte check cannot reach."""

    def setUp(self):
        super().setUp()
        self.xdg = self.tmp / "xdg"
        self.home = self.xdg / "opencode"
        (self.home / "plugin").mkdir(parents=True)
        self.p = approve_profile(self, make_profile("oc-alt", kind="opencode",
                                                    env=[("XDG_CONFIG_HOME", str(self.xdg))]))

    def shim(self):
        opencode.refresh_shim(self.home)

    def answer(self, *entries) -> str:
        return json.dumps({"plugin": list(entries)})

    def named(self) -> str:
        return f"file://{self.home / opencode.SHIM_PATH}"

    def test_the_shim_in_the_answer_is_wired(self):
        self.shim()
        calls = []
        with spawns(calls, self.answer(self.named())):
            self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state, wiring.WIRED)
        self.assertEqual(calls[0].argv, ["opencode", "debug", "config"])
        self.assertEqual(calls[0].env["XDG_CONFIG_HOME"], str(self.xdg))

    def test_an_answer_that_names_no_plugin_is_unwired(self):
        """The shim is on disk and is charter's, and nothing else is in the realm — so the
        only mark that can speak is the one about what opencode actually LOADS. A shim
        opencode does not load is a guard that does not run."""
        self.shim()
        with spawns([], self.answer()):
            w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("loads no plugin", w.detail)

    def test_a_bare_string_is_a_plugin_list_of_one(self):
        """opencode takes a bare string here as well as a list (`_configured_plugins` says
        so about the other door into the same realm)."""
        self.shim()
        with spawns([], json.dumps({"plugin": self.named()})):
            self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state, wiring.WIRED)

    def test_an_entry_that_is_not_a_file_uri_is_read_as_a_path(self):
        self.shim()
        with spawns([], self.answer(str(self.home / opencode.SHIM_PATH))):
            self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state, wiring.WIRED)

    def test_an_entry_that_is_not_a_string_is_skipped_rather_than_raised_on(self):
        self.shim()
        with spawns([], self.answer(17, self.named())):
            self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state, wiring.WIRED)

    def test_an_answer_that_is_not_an_object_is_unwired_rather_than_a_crash(self):
        self.shim()
        with spawns([], json.dumps(["not", "a", "config"])):
            self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state, wiring.UNWIRED)

    def test_an_answer_with_no_plugin_key_is_unwired(self):
        self.shim()
        with spawns([], json.dumps({"plugin": None})):
            self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state, wiring.UNWIRED)

    def test_a_plugin_entry_naming_a_missing_shim_is_unwired(self):
        """Review 5: `unvouched()` answers `()` for a missing shim, so an entry naming a
        deleted file would read as wired to anything that asked it."""
        with spawns([], self.answer(self.named())):
            w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("cannot vouch", w.detail)

    def test_a_byte_perfect_shim_beside_a_foreign_plugin_is_not_wired(self):
        """The documented bypass, reproduced (ruling 26; `opencode.foreign_plugins`, ADR 0015).

        opencode imports the whole plugin directory into ONE realm: a byte-perfect
        `charter.ts` beside `plugin/aaa_boot.ts` holding `Object.hasOwn = () => false` turns
        every guard lookup into `undefined`, a vault read routes to the Bash guard and is
        allowed — while `shim_is_charters` says True throughout. Dropping this mark reopens it.
        """
        self.shim()
        (self.home / "plugin" / "aaa_boot.ts").write_text("Object.hasOwn = () => false;\n")
        with spawns([], self.answer(self.named())):
            w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("aaa_boot.ts", w.detail)

    def test_a_shim_charter_cannot_vouch_for_is_unwired(self):
        (self.home / opencode.SHIM_PATH).write_text("// not charter's\n")
        with spawns([], self.answer(self.named())):
            w = wiring.detect(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("cannot vouch", w.detail)

    def test_an_opencode_probe_that_times_out_refuses(self):
        self.shim()
        with spawns([], raises=util.ProcTimeout(["opencode"], 5.0)):
            w = wiring.detect(self.p, cwd=self.tmp)
            said = wiring.refusal(self.p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNKNOWN_STATE)
        self.assertIn("could not ask", said)
        self.assertIn("check by hand: ", said)
        self.assertIn("opencode debug config", said.split("check by hand: ", 1)[1])

    def test_a_non_zero_probe_is_unknown_even_when_it_printed_an_answer(self):
        """The exit code is the answer about whether there IS an answer. A `debug config`
        that printed a wired-looking document and then failed has told charter nothing, and
        reading the document anyway is how an unknown becomes a pass."""
        self.shim()
        with spawns([], self.answer(self.named()), rc=2):
            self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state,
                             wiring.UNKNOWN_STATE)

    def test_a_nul_byte_in_the_config_home_is_unknown_and_never_a_traceback(self):
        """A1: the real `util.run`, which refuses a NUL in an environment value before it
        spawns anything — so nothing here reaches a binary."""
        p = approve_profile(self, make_profile("oc-nul", kind="opencode",
                                               env=[("XDG_CONFIG_HOME", "/tmp/x\x00y")]))
        self.assertEqual(wiring.detect(p, cwd=self.tmp).state, wiring.UNKNOWN_STATE)
        self.assertIn("could not ask", wiring.refusal(p, cwd=self.tmp))

    def test_an_entry_holding_a_nul_byte_is_skipped_rather_than_raised_on(self):
        """`Path.resolve` refuses a NUL with `ValueError`; an answer is text the harness
        printed, and a plugin entry charter cannot resolve is not charter's shim."""
        self.shim()
        with spawns([], self.answer("file:///tmp/a\x00b.ts")):
            self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state, wiring.UNWIRED)

    def test_what_the_probe_is_named_as_is_contained_on_every_unknown(self):
        """`wiring.py`'s exit and not-JSON details name the probe, which is the profile's own
        command — text a chat wrote."""
        nasty = "oc\r\x1b[2K"
        p = approve_profile(self, make_profile("oc-nasty", kind="opencode", command=[nasty],
                                               env=[("XDG_CONFIG_HOME", str(self.xdg))]))
        for answer, rc in (("{}", 2), ("not json", 0)):
            with self.subTest(rc=rc), spawns([], answer, rc=rc):
                w = wiring.detect(p, cwd=self.tmp)
            self.assertEqual(w.state, wiring.UNKNOWN_STATE)
            self.assertIn("oc\\u000d", w.detail)
            self.assertNotIn("\r", w.detail)

    def test_bad_json_is_unknown(self):
        self.shim()
        with spawns([], "not json"):
            self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state,
                             wiring.UNKNOWN_STATE)

    def test_the_entry_is_compared_resolved(self):
        """Measured: the `file://` URI keeps the un-resolved spelling (`/tmp/...`), and on
        macOS the same directory is `/private/tmp/...` — two `Path`s that are the same
        directory and not the same string."""
        self.shim()
        with spawns([], self.answer(f"file://{os.path.join('/', 'tmp', '..', str(self.home / opencode.SHIM_PATH).lstrip('/'))}")):
            self.assertEqual(wiring.detect(self.p, cwd=self.tmp).state, wiring.WIRED)


# --------------------------------------------------------------------------- #
# The gate: nothing unapproved is run                                          #
# --------------------------------------------------------------------------- #


class NothingUnapprovedIsRun(PersonaIso, unittest.TestCase):
    """A probe runs the profile's OWN command, so it may only run what may be run.

    Ruling 1, and the Global Constraint it comes from: `charter.local.toml` is a file a chat
    can write with no diff to show for it, and a probe is a launch of that command by another
    name. So a probe passes the two checks a launch passes first — git would not carry the
    file, and Task 3's launch record says the operator approved this command — and a surface
    that cannot ask says so instead of asking the harness.
    """

    def setUp(self):
        super().setUp()
        make_plane(self)
        declare_profiles(self)
        self.p = profiles.current().profiles["claude-work"]

    def said(self, fn) -> tuple[object, str]:
        buf = io.StringIO()
        with redirect_stderr(buf):
            got = fn()
        return got, buf.getvalue()

    def test_the_launcher_and_the_gate_agree_about_every_profile(self):
        """One rule, asked in two places, pinned together so they cannot drift apart: the
        launcher's chain refuses a profile for approval or for the file exactly when
        `wiring.detect` declines to ask its harness anything. Branched on the KIND
        (ruling 27), never on the sentence."""
        approve_profile(self, "codex-pinned")
        for p in profiles.current().profiles.values():
            asked = []
            with mock.patch.object(wiring, "_asked", side_effect=lambda q, *, cwd: (
                    asked.append(q.name) or wiring.Wiring(wiring.WIRED, "", ""))), \
                    mock.patch.object(launcher.shutil, "which", return_value="/usr/bin/x"):
                refused = launcher.refusal(p, root=config.ROOT, attended=False,
                                           env=dict(os.environ), probe=False)
                wiring.detect(p, cwd=config.ROOT)
            gated = p.name not in asked
            self.assertEqual(gated, refused is not None and refused.kind in (
                launcher.KIND_UNATTENDED, launcher.KIND_IGNORED), p.name)

    def test_an_unapproved_profile_is_never_probed(self):
        calls = []
        with spawns(calls, raises=AssertionError("a declared command was run")):
            w = wiring.detect(self.p, cwd=config.ROOT)
            said = wiring.refusal(self.p, cwd=config.ROOT)
        self.assertEqual(calls, [])
        self.assertEqual(w.state, wiring.UNKNOWN_STATE)
        # Task 3's own sentence, and not a paraphrase of it: the state word is the record's.
        self.assertEqual(said, profiletrust.UNATTENDED.format(name="claude-work",
                                                              state=profiletrust.NEW))
        self.assertEqual(w.fix, "charter claude-work")

    def test_an_approved_profile_in_a_file_git_would_commit_is_never_probed(self):
        """D: the ignore check moved in with the approval. A record saying the operator once
        approved a command says nothing about the file that command now sits in, and a
        declaration in a committable file is one charter has refused."""
        approve_profile(self)
        (config.ROOT / ".gitignore").write_text("# nothing ignored\n")
        calls = []
        with spawns(calls, raises=AssertionError("a declared command was run")):
            w = wiring.detect(self.p, cwd=config.ROOT)
            said = wiring.refusal(self.p, cwd=config.ROOT)
            installed = wiring.install(self.p, config.ROOT)
            rows = {r.name: r for r in doctor.check_profile_wiring()}
        # The built-ins are probed in the same doctor run — the file decides nothing about
        # a command out of charter's own registry — so this is about the declared one.
        self.assertEqual([c for c in calls if c.env.get("CHARTER_HARNESS_PROFILE") ==
                          "claude-work"], [])
        self.assertEqual(w.state, wiring.UNKNOWN_STATE)
        self.assertEqual(w.fix, profiles.FIX_NOT_IGNORED)
        self.assertIn("is refused", said)
        self.assertEqual([s for s, _l in installed], ["refused"])
        self.assertEqual(rows["profile claude-work"].hint, profiles.FIX_NOT_IGNORED)
        self.assertIn("not probed", rows["profile claude-work"].detail)

    def test_doctor_does_not_probe_an_unapproved_profile(self):
        calls = []
        with spawns(calls, listing()):
            rows = {r.name: r for r in doctor.check_profile_wiring()}
        row = rows["profile claude-work"]
        self.assertEqual(row.status, doctor.WARN)
        self.assertEqual(row.hint, "charter claude-work")
        self.assertIn("not approved yet", row.detail)
        self.assertIn(profiletrust.NEW, row.detail)
        # The BUILT-IN `claude` is probed in the same run — it is charter's own command and
        # needs no approval — so the assertion is about this profile's folder, not about
        # whether anything was spawned at all.
        self.assertEqual([c for c in calls if "CLAUDE_CONFIG_DIR" in c.env], [])

    def test_install_asks_first(self):
        """Installing runs the profile's own command (`claude plugin install`), so it asks
        exactly as a launch does. A no is the workspace picker's cancel code, and nothing ran."""
        calls = []
        with spawns(calls, raises=AssertionError("a declared command was run")), \
                mock.patch.object(plugincache, "install") as inst, \
                mock.patch("sys.stdin", (typed := _Typed("n\n"))), \
                mock.patch("sys.stdout", (screen := _ATerminal())):
            rc = commands_harness.cmd_harness_install(SimpleNamespace(name="claude-work"))
        self.assertEqual(rc, profiletrust.DECLINED_EXIT)
        self.assertEqual(typed.reads, 1)
        self.assertIn("run this? [y/N]", screen.getvalue())
        inst.assert_not_called()
        self.assertEqual(calls, [])
        self.assertEqual(profiletrust.approval_needed(self.p), profiletrust.NEW)

    def test_install_after_a_yes_records_it_and_installs(self):
        seen = []
        with spawns([], listing(entry(project=config.ROOT))), \
                mock.patch.object(plugincache, "install",
                                  side_effect=lambda *a, **k: seen.append(k) or
                                  ("installed", "ok")), \
                mock.patch("sys.stdin", _Typed("y\n")), \
                mock.patch("sys.stdout", _ATerminal()):
            rc, _out = self.said(lambda: commands_harness.cmd_harness_install(
                SimpleNamespace(name="claude-work")))
        self.assertEqual(rc, 0)
        self.assertEqual(len(seen), 1)
        self.assertEqual(profiletrust.approval_needed(self.p), "")

    def test_install_with_no_terminal_refuses_as_an_open_nobody_is_at(self):
        """No terminal on both ends, so no question: `UNATTENDED`, and rc 1."""
        with spawns([], raises=AssertionError("a declared command was run")), \
                mock.patch.object(plugincache, "install") as inst, \
                mock.patch("sys.stdin", _APipe()):
            rc, out = self.said(lambda: commands_harness.cmd_harness_install(
                SimpleNamespace(name="claude-work")))
        self.assertEqual(rc, 1)
        self.assertIn(profiletrust.UNATTENDED.format(name="claude-work",
                                                     state=profiletrust.NEW), out)
        inst.assert_not_called()

    def test_install_refuses_a_yes_it_could_not_record(self):
        """N2b's nit, on this surface too (`KIND_RECORD`): a yes charter cannot write down
        would only be asked again, so it refuses — and installs nothing."""
        with spawns([], raises=AssertionError("a declared command was run")), \
                mock.patch.object(plugincache, "install") as inst, \
                mock.patch.object(profiletrust, "record_launched", return_value="read-only"), \
                mock.patch("sys.stdin", _Typed("y\n")), \
                mock.patch("sys.stdout", _ATerminal()):
            rc, out = self.said(lambda: commands_harness.cmd_harness_install(
                SimpleNamespace(name="claude-work")))
        self.assertEqual(rc, 1)
        self.assertIn("could not record", out)
        inst.assert_not_called()


class AYesNeverWalksPastAWiringRefusal(_ALaunchNamesAProfile, unittest.TestCase):
    """Ruling 27, with the wiring link in the chain: after a yes the WHOLE chain runs again
    before the `exec`, so a profile the operator approved a moment ago and whose folder does
    not carry charter's guard is still refused — on every path (re-review N2)."""

    def unwired(self):
        return mock.patch.object(wiring, "_asked", return_value=wiring.Wiring(
            wiring.UNWIRED, "charter@charter is not installed", "charter harness install x"))

    def test_a_yes_on_an_unwired_profile_still_refuses_with_no_frame(self):
        said = io.StringIO()
        with self.unwired(), redirect_stderr(said), \
                mock.patch("sys.stdin", _Typed("y\n")), \
                mock.patch("sys.stdout", _ATerminal()), \
                mock.patch.object(launcher.shutil, "which", return_value="/nowhere/claude"):
            rc = launcher.start(self._profile(), [], fid=None, attended=True)
        self.assertEqual(profiletrust.approval_needed(self._profile()), "", "no record")
        self.assertEqual(self.execs, [])
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn("not wired", said.getvalue())

    def test_a_yes_on_an_unwired_profile_still_refuses_in_the_pane(self):
        from charter.frame import state

        state.frame_dir("beta.1", create=True)
        said = io.StringIO()
        with self.unwired(), redirect_stderr(said), \
                mock.patch("sys.stdin", _Typed("y\n")), \
                mock.patch("sys.stdout", _ATerminal()), \
                mock.patch.object(launcher.shutil, "which", return_value="/nowhere/claude"), \
                mock.patch.object(launcher, "framed_chat", return_value="beta.1"), \
                mock.patch.object(launcher, "_wait_for_the_operator"):
            rc = launcher.cmd_frame_launch(SimpleNamespace(
                profile="claude-work", attended=True, rest=[]))
        self.assertEqual(self.execs, [])
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn("not wired", said.getvalue())

    def test_a_yes_before_tmux_still_refuses_an_unwired_profile_before_tmux(self):
        """The same, where `charter <profile>` asks: `_asked_here`'s re-run is the whole
        chain, so nothing is allocated for a profile that will not start."""
        with self.unwired():
            rc, said = self._said(profile="claude-work", stdin=_Typed("y\n"),
                                  stdout=_ATerminal())
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertIn("not wired", said)
        self.assertFalse(self._started())

    def test_callers_branch_on_the_kind_not_the_text(self):
        """Every sentence reworded — `NEEDS_ASKING` and both of the wiring refusal's — and
        the pane still asks, and still refuses the unwired profile after the yes: which
        refusal it is was never read off the words."""
        from charter.frame import state

        state.frame_dir("beta.1", create=True)
        stdin, said = _Typed("y\n"), io.StringIO()
        with self.unwired(), redirect_stderr(said), \
                mock.patch.object(profiletrust, "NEEDS_ASKING", "reworded {name} {state}"), \
                mock.patch.object(wiring, "NOT_WIRED", "reworded {name} {detail} {fix}"), \
                mock.patch.object(wiring, "CANNOT_TELL",
                                  "reworded {kind} {name} {detail} {probe}"), \
                mock.patch("sys.stdin", stdin), \
                mock.patch("sys.stdout", (screen := _ATerminal())), \
                mock.patch.object(launcher.shutil, "which", return_value="/nowhere/claude"), \
                mock.patch.object(launcher, "framed_chat", return_value="beta.1"), \
                mock.patch.object(launcher, "_wait_for_the_operator"):
            rc = launcher.cmd_frame_launch(SimpleNamespace(
                profile="claude-work", attended=True, rest=[]))
        self.assertEqual(stdin.reads, 1)
        self.assertIn("run this? [y/N]", screen.getvalue())
        self.assertEqual(rc, launcher.REFUSED_EXIT)
        self.assertEqual(self.execs, [])
        self.assertIn("reworded claude-work", said.getvalue())


# --------------------------------------------------------------------------- #
# The launch                                                                   #
# --------------------------------------------------------------------------- #


class TheLaunchRefusesAnUnwiredProfile(PersonaIso, unittest.TestCase):
    """The last guard before the `exec`, and it applies to built-ins too (ruling 10).

    `charter codex` on a plane where nobody wired Codex now refuses where it runs today. A
    chat that looks guarded and is not is the same failure whichever profile started it.
    """

    def setUp(self):
        super().setUp()
        make_plane(self)
        declare_profiles(self)

    def refuse(self, p):
        return launcher.refusal(p, root=config.ROOT, attended=False, env=dict(os.environ))

    def test_a_built_in_that_is_not_wired_is_refused_with_the_fix(self):
        home = self.tmp / "empty-codex"
        home.mkdir()
        p = make_profile("codex", kind="codex", source=profiles.BUILTIN)
        with mock.patch.dict(os.environ, {"CODEX_HOME": str(home)}), \
                mock.patch.object(launcher.shutil, "which", lambda *a, **k: "/bin/codex"):
            r = self.refuse(p)
        self.assertIsNotNone(r)
        self.assertEqual(r.kind, wiring.KIND_WIRING)
        self.assertIn("charter harness install codex", r.text)

    def test_a_built_in_claude_under_claudeguards_empty_answer_is_refused(self):
        """This is why `wired_as_today` exists. In-process the suite's guard makes
        `plugincache.available` answer False, so detection reads UNKNOWN and refuses; in a
        child process its fake `claude` answers `[]`, which reads as unwired. Either way a
        launch test that is not about wiring would refuse without the stand-in."""
        p = profiles.current().profiles["claude"]
        r = self.refuse(p)
        self.assertIsNotNone(r)
        self.assertEqual(r.kind, wiring.KIND_WIRING)

    def test_a_wired_built_in_reaches_the_end_of_the_chain(self):
        with spawns([], listing(entry(project=Path.cwd()))):
            self.assertIsNone(self.refuse(profiles.current().profiles["claude"]))

    def test_a_cached_wired_answer_never_starts_a_chat(self):
        """Review B2 and ruling 21: the cache is a file a chat can write, key and stamp."""
        p = profiles.current().profiles["claude"]
        here = Path.cwd()
        wiring.remember(p, cwd=here, w=wiring.Wiring(wiring.WIRED, "remembered", ""))
        self.assertIsNotNone(wiring.cached(p, cwd=here))
        with spawns([], listing()):
            r = self.refuse(p)
        self.assertIsNotNone(r)
        self.assertEqual(r.kind, wiring.KIND_WIRING)

    def test_every_launch_probes_fresh(self):
        p = profiles.current().profiles["claude"]
        calls = []
        with spawns(calls, listing(entry(project=Path.cwd()))):
            self.refuse(p)
            self.refuse(p)
        self.assertEqual(len(calls), 2)

    def test_a_reopen_skips_an_unwired_profile_by_name(self):
        """Ruling 10 at a reopen, which has nobody to ask and no pane anybody is reading: it
        says which chat, why, and the fix, rather than starting a chat whose launcher refuses
        where nobody sees it. The chat stays in the manifest for the next `charter reopen`."""
        (config.WORKSPACES_DIR / "alpha").mkdir(parents=True, exist_ok=True)
        chat = reopen_state.Chat(chat="alpha.1", workspace="alpha", persona="",
                                 harness="claude-code", cwd="", resume="", transcript="",
                                 active=True, profile="claude")
        warned, launched = [], []
        with spawns([], listing()), \
                mock.patch.object(commands_frame.util, "warn", side_effect=warned.append), \
                mock.patch.object(commands_frame, "cmd_launch",
                                  side_effect=lambda args: launched.append(args) or 0):
            got = commands_frame._reopen_one(chat)
        self.assertIsNone(got)
        self.assertEqual(launched, [])
        said = "\n".join(warned)
        self.assertIn("alpha.1", said)
        self.assertIn("not wired", said)
        self.assertIn("charter harness install claude", said)

    def test_a_reopen_asks_about_the_directory_the_chat_comes_back_in(self):
        (config.WORKSPACES_DIR / "alpha").mkdir(parents=True, exist_ok=True)
        chat = reopen_state.Chat(chat="alpha.1", workspace="alpha", persona="",
                                 harness="claude-code", cwd="", resume="", transcript="",
                                 active=True, profile="claude")
        asked = []
        with mock.patch.object(wiring, "refusal",
                               side_effect=lambda p, *, cwd: asked.append(cwd) or "no"), \
                mock.patch.object(commands_frame.util, "warn"), \
                mock.patch.object(commands_frame, "cmd_launch", return_value=0):
            commands_frame._reopen_one(chat)
        self.assertEqual([Path(a).resolve() for a in asked],
                         [(config.WORKSPACES_DIR / "alpha").resolve()])

    def test_a_handoff_asks_about_the_workspace_the_chat_would_stand_in(self):
        """Re-review N8: a handoff probes before it writes anything, and it probes the
        TARGET workspace's directory. Claude Code resolves `enabledPlugins` at the session's
        own directory and does not walk up (measured), and charter mirrors the plane's into
        `workspaces/<ws>/` — so asking from the caller's directory would answer about the
        wrong chat."""
        from charter import commands_frame

        ws = config.WORKSPACES_DIR / "beta"
        ws.mkdir(parents=True)
        seen = []
        with mock.patch.object(
                commands_frame, "_profile_refusal",
                side_effect=lambda p, *, attended, cwd=None: seen.append(cwd)):
            commands_frame._launch_refusal(profiles.current().profiles["claude"],
                                           attended=False,
                                           cwd=commands_frame._chat_dir_of("beta"))
        self.assertEqual(seen, [ws])

    def test_a_workspace_with_no_directory_yet_is_asked_about_at_the_plane(self):
        """Before `--create` makes it there is nothing to read, and `workspace.ensure` is
        not called here: a refusal that made a directory as a side effect of deciding not to
        open a chat is the half-done open the check exists to prevent."""
        from charter import commands_frame

        self.assertEqual(commands_frame._chat_dir_of("never-made"), Path(config.ROOT))

    def test_wiring_is_the_last_check_so_a_refusal_behind_it_is_never_probed(self):
        """The order is the order the checks cost in, and a probe is the expensive one: a
        command that is not on `PATH` is answered without asking the harness anything."""
        p = approve_profile(self, make_profile("gone", command=["/nowhere/claude"]))
        calls = []
        with spawns(calls, raises=AssertionError("probed anyway")):
            r = self.refuse(p)
        self.assertEqual(r.kind, launcher.KIND_PATH)
        self.assertEqual(calls, [])


class AStartPaysTheProbeTwice(_ALaunchNamesAProfile, unittest.TestCase):
    """A3: "a start pays the probe twice" — once before tmux, once in the pane — and an
    open whose caller asked the chain for itself does not pay a third time in between."""

    def setUp(self):
        super().setUp()
        self._no_approval_needed()
        self.probed: list = []
        self.enterContext(mock.patch.object(
            wiring, "refusal", side_effect=lambda p, *, cwd: self.probed.append(cwd) or ""))

    def test_a_terminal_launch_probes_before_tmux(self):
        self.assertEqual(self._launch(profile="claude-work", stdin=_APipe()), 0)
        self.assertEqual(len(self.probed), 1)

    def test_a_plus_or_a_tab_probes_once_before_the_pane_and_not_again_in_launch(self):
        """`cmd_new_chat` and `_open_workspace` ask `_launch_refusal` so the refusal has a
        surface, then call `cmd_launch` with `attach=False` — which must not probe again."""
        p = self._profile()
        self.assertEqual(commands_frame._launch_refusal(
            p, attended=True, cwd=commands_frame._chat_dir_of("beta")), "")
        self.assertEqual(self._launch(profile="claude-work", attach=False, stdin=_APipe()), 0)
        self.assertEqual(len(self.probed), 1, self.probed)
        # …and the pane's own chain, before its `exec`, is the second.
        self.assertEqual(launcher.start(p, [], fid=None, attended=False), 0)
        self.assertEqual(len(self.probed), 2, self.probed)

    def test_a_handoffs_opening_does_not_probe_in_launch_either(self):
        opening = commands_frame.Opening("hello there")
        self.assertEqual(self._launch(profile="claude-work", attach=False, opening=opening,
                                      rest=["hello there"], stdin=_APipe()), 0)
        self.assertEqual(self.probed, [])

    def test_skipping_it_skips_the_probe_and_nothing_else(self):
        """The skip is the WIRING link alone: a `PATH` refusal for the same open still
        happens before tmux."""
        rc, said = self._said(profile="claude-work", attach=False, which=None, stdin=_APipe())
        self.assertEqual(rc, launcher.MISSING_EXIT)
        self.assertIn("not on PATH", said)
        self.assertFalse(self._started())


class ARefusalThatQuotesACommandShowsItEscaped(PersonaIso, unittest.TestCase):
    """Ruling 35, for the three texts that quote one: `NOT_ON_PATH`, `EXEC_FAILED` and
    `CANNOT_TELL`.

    `charter.local.toml` is a file a chat can write. A `\\r` or an ESC in a `command` could
    redraw the line above it — the approval prompt, or a refusal an operator is reading — so
    every profile-derived piece goes through `contain.readable` and is shown, never run.
    """

    def setUp(self):
        super().setUp()
        make_plane(self)
        self.nasty = "cl\raude\x1b[2K"

    def test_cannot_tell_escapes_the_command_it_names(self):
        p = approve_profile(self, make_profile(self.nasty, command=[self.nasty],
                                               env=[("CLAUDE_CONFIG_DIR", self.nasty)]))
        with spawns([], "", rc=1):
            said = wiring.refusal(p, cwd=self.tmp)
        self.assertNotIn("\r", said)
        self.assertNotIn("\x1b", said)
        self.assertIn(contain.readable(self.nasty), said)

    def test_not_wired_escapes_the_name_and_the_fix(self):
        p = approve_profile(self, make_profile(self.nasty))
        with spawns([], listing()):
            said = wiring.refusal(p, cwd=self.tmp)
        self.assertNotIn("\r", said)
        self.assertNotIn("\x1b", said)

    def test_not_on_path_escapes_the_command(self):
        p = make_profile("bad", command=[f"/nowhere/{self.nasty}"], source=profiles.BUILTIN)
        r = launcher.refusal(p, root=config.ROOT, attended=False, env=dict(os.environ))
        self.assertEqual(r.kind, launcher.KIND_PATH)
        self.assertNotIn("\r", r.text)
        self.assertNotIn("\x1b", r.text)

    def test_exec_failed_escapes_the_command(self):
        p = make_profile("bad", command=["/bin/sh"], source=profiles.BUILTIN)
        with mock.patch.object(launcher, "refusal", return_value=None), \
                mock.patch.object(launcher.os, "execvpe",
                                  side_effect=OSError(13, self.nasty)):
            r = launcher.attempt(p, [], fid=None, attended=False)
        self.assertEqual(r.kind, launcher.KIND_EXEC)
        self.assertNotIn("\r", r.text)
        self.assertNotIn("\x1b", r.text)


# --------------------------------------------------------------------------- #
# The cache                                                                    #
# --------------------------------------------------------------------------- #


class TheCache(PersonaIso, unittest.TestCase):
    """Display only (Task 5), and stated as writable by a chat (ruling 13).

    It sits under `.charter/`, which no guard covers, so a launch never trusts it — these
    cases are about what the SELECTOR may draw, and the last one is why the age test needs
    two bounds rather than one.
    """

    def setUp(self):
        super().setUp()
        make_plane(self)
        self.here = self.tmp / "here"
        self.here.mkdir()
        self.folder = self.tmp / "cc"
        (self.folder / "plugins").mkdir(parents=True)
        (self.folder / "plugins" / "installed_plugins.json").write_text("{}")
        self.p = make_profile("claude-alt", env=[("CLAUDE_CONFIG_DIR", str(self.folder))])
        self.w = wiring.Wiring(wiring.WIRED, "asked and answered", "")

    def path(self) -> Path:
        return Path(config.STATE_DIR) / wiring.CACHE

    def test_a_stamp_that_still_matches_is_answered(self):
        wiring.remember(self.p, cwd=self.here, w=self.w)
        self.assertEqual(wiring.cached(self.p, cwd=self.here), self.w)

    def test_a_touched_stamp_file_is_a_miss(self):
        wiring.remember(self.p, cwd=self.here, w=self.w)
        f = self.folder / "plugins" / "installed_plugins.json"
        os.utime(f, (0, 0))
        self.assertIsNone(wiring.cached(self.p, cwd=self.here))

    def test_the_chat_directorys_settings_file_is_stamped_too(self):
        """The Task 4 nit carried out of the plan review: a stamp that covered only the
        plane root would keep saying `wired` after a chat wrote
        `<chat dir>/.claude/settings.local.json`, which is exactly the file the binary reads
        for that chat."""
        wiring.remember(self.p, cwd=self.here, w=self.w)
        (self.here / ".claude").mkdir()
        (self.here / ".claude" / "settings.local.json").write_text("{}")
        self.assertIsNone(wiring.cached(self.p, cwd=self.here))

    def test_a_changed_profile_is_a_miss(self):
        wiring.remember(self.p, cwd=self.here, w=self.w)
        other = make_profile("claude-alt", command=["claude", "--pinned"],
                             env=[("CLAUDE_CONFIG_DIR", str(self.folder))])
        self.assertIsNone(wiring.cached(other, cwd=self.here))

    def test_another_directory_is_a_miss(self):
        wiring.remember(self.p, cwd=self.here, w=self.w)
        self.assertIsNone(wiring.cached(self.p, cwd=self.tmp))

    def test_an_old_entry_is_a_miss(self):
        wiring.remember(self.p, cwd=self.here, w=self.w)
        self.age(-25 * 3600)
        self.assertIsNone(wiring.cached(self.p, cwd=self.here))

    def test_a_stamp_in_the_future_is_stale(self):
        """Review B2: without the lower bound an entry dated ahead passes the age test
        forever, and the file is one a chat can write."""
        wiring.remember(self.p, cwd=self.here, w=self.w)
        self.age(3600)
        self.assertIsNone(wiring.cached(self.p, cwd=self.here))

    def age(self, delta: float) -> None:
        doc = json.loads(self.path().read_text())
        for e in doc.values():
            e["checked_at"] = time.time() + delta
        self.path().write_text(json.dumps(doc))

    def test_a_cache_that_is_not_an_object_is_a_miss(self):
        """A file a chat can write is a file that can hold a list, and `.get` on one is an
        `AttributeError` in whatever was asking — a selector draw, or a `remember`."""
        self.path().parent.mkdir(parents=True, exist_ok=True)
        self.path().write_text("[1, 2, 3]")
        self.assertIsNone(wiring.cached(self.p, cwd=self.here))
        wiring.remember(self.p, cwd=self.here, w=self.w)
        self.assertEqual(wiring.cached(self.p, cwd=self.here), self.w)

    def test_an_unreadable_cache_is_a_miss(self):
        self.path().parent.mkdir(parents=True, exist_ok=True)
        self.path().write_text("{nope")
        self.assertIsNone(wiring.cached(self.p, cwd=self.here))

    def test_a_disable_written_at_the_plane_root_is_a_miss(self):
        """A6, measured on claude 2.1.270: in a git plane the ROOT's `settings.local.json`
        reaches a session in `workspaces/<ws>/` below it. A stamp of the chat's own
        directory alone would keep drawing `wired` after the root disabled charter."""
        ws = config.ROOT / "workspaces" / "w"
        ws.mkdir(parents=True)
        wiring.remember(self.p, cwd=ws, w=self.w)
        self.assertEqual(wiring.cached(self.p, cwd=ws), self.w)
        (config.ROOT / ".claude").mkdir(exist_ok=True)
        (config.ROOT / ".claude" / "settings.local.json").write_text(
            json.dumps({"enabledPlugins": {plugincache.PLUGIN_ID: False}}))
        self.assertIsNone(wiring.cached(self.p, cwd=ws))

    def test_the_directories_between_the_chat_and_the_root_are_stamped_too(self):
        ws = config.ROOT / "workspaces" / "w"
        ws.mkdir(parents=True)
        wiring.remember(self.p, cwd=ws, w=self.w)
        (config.ROOT / "workspaces" / ".claude").mkdir()
        (config.ROOT / "workspaces" / ".claude" / "settings.json").write_text("{}")
        self.assertIsNone(wiring.cached(self.p, cwd=ws))

    def test_the_walk_stops_at_the_plane_root(self):
        """Above the plane is somebody else's directory: a file there is no stamp of this
        plane's, and walking to `/` would make every entry a miss on a machine whose home
        directory has a `.claude/` at all."""
        stamped = wiring._stamp(self.p, config.ROOT / "workspaces")
        above = str(Path(os.path.realpath(config.ROOT)).parent / ".claude")
        self.assertFalse(any(k.startswith(above + os.sep) for k in stamped), stamped)
        self.assertTrue(any(k.startswith(str(Path(os.path.realpath(config.ROOT)) / ".claude"))
                            for k in stamped), stamped)

    def test_a_directory_outside_the_plane_stamps_only_itself(self):
        outside = Path(self.enterContext(_TempDir()))
        stamped = wiring._stamp(self.p, outside)
        dirs = {str(Path(k).parent.parent) for k in stamped if "/.claude/" in k}
        self.assertEqual(dirs, {os.path.realpath(outside)})

    def test_a_cache_entry_of_the_wrong_shape_is_a_miss_and_never_a_traceback(self):
        """A1: every field of an entry is a value a chat can write, and each of these was a
        `TypeError` or a `Wiring` of the wrong type out of a selector draw."""
        hostile = [{"checked_at": "x"}, {"checked_at": None}, {"checked_at": True},
                   {"checked_at": [1]}, {"state": "wired-ish"}, {"detail": 3},
                   {"fix": None}]
        for change in hostile:
            with self.subTest(change=change):
                wiring.remember(self.p, cwd=self.here, w=self.w)
                doc = json.loads(self.path().read_text())
                for e in doc.values():
                    e.update(change)
                self.path().write_text(json.dumps(doc))
                self.assertIsNone(wiring.cached(self.p, cwd=self.here))

    def test_both_age_bounds_are_where_the_rule_says(self):
        """B3: `0 <= age < MAX_AGE`, a stated security rule, pinned at each edge. An entry
        answered this instant is a hit; one exactly `MAX_AGE` old is a miss; one a second in
        the future is stale."""
        now = 1_800_000_000.0
        with mock.patch.object(wiring.time, "time", return_value=now):
            wiring.remember(self.p, cwd=self.here, w=self.w)
        for at, hit in ((now, True), (now + wiring.MAX_AGE - 1, True),
                        (now + wiring.MAX_AGE, False), (now - 1, False)):
            with self.subTest(at=at), mock.patch.object(wiring.time, "time", return_value=at):
                self.assertEqual(wiring.cached(self.p, cwd=self.here) is not None, hit)

    def test_remembering_never_raises_over_a_cache_it_cannot_write(self):
        """A display cache is not worth a row, a selector or a launch."""
        with mock.patch.object(config, "write_for", side_effect=OSError("read-only")):
            wiring.remember(self.p, cwd=self.here, w=self.w)


# --------------------------------------------------------------------------- #
# charter harness install <profile>                                            #
# --------------------------------------------------------------------------- #


class HarnessInstallTakesAProfile(PersonaIso, unittest.TestCase):
    """Ruling 8: a profile name first, a registry NAME second."""

    def setUp(self):
        super().setUp()
        make_plane(self)
        self.oc = self.tmp / "oc"
        self.codex_home = self.tmp / "cx"
        declare_profiles(self, f"""
[harness.claude-alt]
kind = "claude"
command = ["claude"]
env = {{ CLAUDE_CONFIG_DIR = "{self.tmp / 'cc'}" }}

[harness.oc-alt]
kind = "opencode"
command = ["opencode"]
env = {{ XDG_CONFIG_HOME = "{self.oc}" }}

[harness.codex-alt]
kind = "codex"
command = ["codex"]
env = {{ CODEX_HOME = "{self.codex_home}" }}
""")
        approve_every_profile(self)

    def run_install(self, name) -> int:
        return commands_harness.cmd_harness_install(SimpleNamespace(name=name))

    def test_a_claude_profile_installs_under_its_env(self):
        seen = {}

        def fake(project, scope=plugincache.INSTALL_SCOPE, *, env=None, command=None):
            seen.update(env=env, command=command)
            return "installed", "ok"

        with mock.patch.object(plugincache, "install", fake), \
                spawns([], listing(entry(project=config.ROOT))):
            rc = self.run_install("claude-alt")
        self.assertEqual(rc, 0)
        self.assertEqual(seen["env"]["CLAUDE_CONFIG_DIR"], str(self.tmp / "cc"))
        self.assertEqual(seen["command"], ["claude"])

    def test_an_install_that_leaves_it_unwired_exits_non_zero(self):
        with mock.patch.object(plugincache, "install", return_value=("installed", "ok")), \
                spawns([], listing()):
            self.assertEqual(self.run_install("claude-alt"), 1)

    def test_an_opencode_profile_wires_its_own_config_home(self):
        """Into the folder the PROFILE names, and not into the one this process reads.

        The suite's own `$XDG_CONFIG_HOME` is asserted by its `(mtime_ns, size)` rather than
        by absence: it is shared with every other case in the run, and one of them may have
        wired it for its own reasons. What this case is about is that THIS call did not.
        """
        sandbox = Path(os.environ["XDG_CONFIG_HOME"]) / "opencode" / opencode.SHIM_PATH
        before = sandbox.stat() if sandbox.exists() else None
        with spawns([], json.dumps(
                {"plugin": [f"file://{self.oc / 'opencode' / opencode.SHIM_PATH}"]})):
            rc = self.run_install("oc-alt")
        self.assertEqual(rc, 0)
        self.assertTrue(opencode.shim_is_charters(self.oc / "opencode"))
        after = sandbox.stat() if sandbox.exists() else None
        self.assertEqual(before is None, after is None)
        if before is not None:
            self.assertEqual((before.st_mtime_ns, before.st_size),
                             (after.st_mtime_ns, after.st_size))

    def test_a_codex_profile_writes_its_policy_line_under_its_own_home(self):
        default = Path(os.environ["CODEX_HOME"]) / "config.toml"
        before = default.read_text() if default.exists() else None
        self.run_install("codex-alt")
        doc = (self.codex_home / "config.toml").read_text()
        self.assertIn('CHARTER_HARNESS = "codex"', doc)
        self.assertEqual(default.read_text() if default.exists() else None, before)

    def test_a_codex_profile_names_the_codex_side_steps(self):
        """Charter writes the one line Codex's plugin cannot write, and PRINTS the rest:
        installing a plugin and approving hooks are Codex's own commands, and a hook nobody
        approved is inert whatever charter did (ruling 7)."""
        out = self.said(lambda: self.run_install("codex-alt"))
        self.assertIn(f"CODEX_HOME={shlex.quote(str(self.codex_home))}", out)
        # Once: the refusal the install ends on names the steps, and a second list printed
        # above it was the same three lines again (C).
        self.assertEqual(out.count("codex plugin add"), 1, out)

    def test_an_existing_policy_table_without_charters_line_is_refused(self):
        self.codex_home.mkdir(parents=True, exist_ok=True)
        f = self.codex_home / "config.toml"
        f.write_text('[shell_environment_policy]\ninherit = "core"\n')
        before = f.read_bytes()
        out = self.said(lambda: self.assertEqual(self.run_install("codex-alt"), 1))
        self.assertEqual(f.read_bytes(), before)
        self.assertIn('set = { CHARTER_HARNESS = "codex" }', out)
        self.assertNotIn("charter harness install", out)

    def test_a_shim_charter_cannot_vouch_for_is_a_warning_and_not_an_item(self):
        """`Harness.wire` answers `unvouched` with a SENTENCE, not a path — #433's shape,
        where a shim with every guard cut out of it was listed under "already present"."""
        with mock.patch.object(wiring, "install",
                               return_value=[("unvouched", "somebody else's plugin")]), \
                spawns([], json.dumps({"plugin": []})):
            out = self.said(lambda: self.run_install("oc-alt"))
        self.assertRegex(out, r"!\s+somebody else's plugin")

    def test_a_doubled_codex_config_refuses(self):
        """Pinned (C): without its branch a doubled install printed the detail as a note and
        exited 0 over a Codex that runs charter twice per turn."""
        with mock.patch.object(wiring, "install",
                               return_value=[("doubled", "cx declares hooks")]), \
                mock.patch.object(wiring, "detect", side_effect=AssertionError("asked")):
            out = self.said(lambda: self.assertEqual(self.run_install("codex-alt"), 1))
        self.assertIn("charter fires twice", out)

    def test_a_malformed_codex_config_refuses_and_is_left_alone(self):
        with mock.patch.object(wiring, "install", return_value=[("malformed", "cx/config")]), \
                mock.patch.object(wiring, "detect", side_effect=AssertionError("asked")):
            out = self.said(lambda: self.assertEqual(self.run_install("codex-alt"), 1))
        self.assertIn("is not valid TOML", out)

    def test_a_refused_name_says_its_own_reason(self):
        """`:150`: a name the file declares and charter refused is not "no such profile" —
        the operator wrote it, and the reason is what they need."""
        declare_profiles(self, '[harness.bad-one]\nkind = "nope"\ncommand = ["x"]\n')
        out = self.said(lambda: self.assertEqual(self.run_install("bad-one"), 2))
        self.assertIn("profile 'bad-one' is refused", out)
        self.assertNotIn("no harness or profile named", out)

    def test_a_long_name_is_bounded_where_install_repeats_it_back(self):
        """`commands_harness.py`'s `shown`: a refusal sentence bounds the name with contain's
        fixed marker (ruling 45), and a 161-character name is one past that bound."""
        long_name = "i" * 161
        declare_profiles(self, f'[harness.{long_name}]\nkind = "claude"\ncommand = ["claude"]\n')
        with mock.patch("sys.stdin", _APipe()):
            out = self.said(lambda: self.assertEqual(self.run_install(long_name), 1))
        self.assertIn("i" * 160 + "...", out)
        self.assertNotIn(long_name, out)

    def test_a_name_is_repeated_back_escaped_once(self):
        """A12: `readable` then `!r` escaped every backslash `readable` had written, so an ESC
        read as `\\\\u001b` — two escapes for one byte, and neither the one a reader
        could type back."""
        out = self.said(lambda: self.assertEqual(self.run_install("x\x1bx"), 2))
        self.assertIn("'x\\u001bx'", out)
        self.assertNotIn("\\\\u001b", out)

    def test_a_registry_name_still_works_as_today(self):
        """`charter harness install codex` is what `docs/harnesses.md` tells people to run,
        and a profile-first lookup must not have taken it away (ruling 8). It writes the same
        line into the same file and points at the plugin the same way; what it no longer does
        is exit 0 over a Codex that will still refuse to launch — charter can write the
        policy line and nothing else, and the plugin install and the hook approval are
        Codex's own commands."""
        out = self.said(lambda: self.assertEqual(self.run_install("codex"), 1))
        self.assertIn(str(codex.config_path()), out)
        self.assertIn("plugin", out.lower())

    def test_an_unknown_name_is_refused_with_the_profiles_named(self):
        out = self.said(lambda: self.assertEqual(self.run_install("nope"), 2))
        self.assertIn("claude-alt", out)

    def test_a_committable_local_file_refuses_install(self):
        (config.ROOT / ".gitignore").write_text("# nothing ignored\n")
        out = self.said(lambda: self.assertEqual(self.run_install("claude-alt"), 1))
        self.assertIn("charter reinit", out)

    def said(self, fn) -> str:
        import io
        from contextlib import redirect_stderr, redirect_stdout

        buf = io.StringIO()
        with redirect_stderr(buf), redirect_stdout(buf):
            fn()
        return buf.getvalue()


# --------------------------------------------------------------------------- #
# init / reinit                                                                #
# --------------------------------------------------------------------------- #


class InitAndReinitWireEachProfile(PersonaIso, unittest.TestCase):
    """Setup runs PER PROFILE, and `reinit` installs no software (ruling 9)."""

    def setUp(self):
        super().setUp()
        make_plane(self)
        self.oc = self.tmp / "oc"
        declare_profiles(self, f"""
[harness.claude-alt]
kind = "claude"
command = ["claude"]
env = {{ CLAUDE_CONFIG_DIR = "{self.tmp / 'cc'}" }}

[harness.oc-alt]
kind = "opencode"
command = ["opencode"]
env = {{ XDG_CONFIG_HOME = "{self.oc}" }}

[harness.codex-alt]
kind = "codex"
command = ["codex"]
env = {{ CODEX_HOME = "{self.tmp / 'cx'}" }}
""")

    def wired(self, *, install: bool):
        with spawns([], listing()):
            return commands._wire_profiles(config.ROOT, install=install)

    def test_init_installs_for_an_approved_claude_profile(self):
        approve_every_profile(self)
        with mock.patch.object(wiring, "install",
                                           return_value=[("installed", "x")]) as inst:
            with spawns([], listing()):
                commands._wire_profiles(config.ROOT, install=True)
        self.assertIn("claude-alt", {c.args[0].name for c in inst.call_args_list})

    def test_init_leaves_a_codex_profile_opt_in(self):
        approve_every_profile(self)
        with mock.patch.object(wiring, "install",
                                           return_value=[("installed", "x")]) as inst:
            with spawns([], listing()):
                out = commands._wire_profiles(config.ROOT, install=True)
        self.assertNotIn("codex-alt", {c.args[0].name for c in inst.call_args_list})
        self.assertIn("charter harness install codex-alt", " ".join(l for _s, l in out))

    def test_reinit_installs_nothing_and_names_what_is_missing(self):
        approve_every_profile(self)
        with mock.patch.object(plugincache, "install") as inst:
            out = self.wired(install=False)
        inst.assert_not_called()
        self.assertIn("not wired", " ".join(l for _s, l in out))

    def test_reinit_wires_an_opencode_profiles_shim(self):
        approve_every_profile(self)
        with spawns([], json.dumps({"plugin": []})):
            commands._wire_profiles(config.ROOT, install=False)
        self.assertTrue(opencode.shim_is_charters(self.oc / "opencode"))

    def test_neither_touches_an_unapproved_profile(self):
        with mock.patch.object(wiring, "install") as inst:
            out = self.wired(install=True)
        inst.assert_not_called()
        self.assertIn("not approved yet", " ".join(l for _s, l in out))
        self.assertIn("run charter claude-alt once", " ".join(l for _s, l in out))

    def test_a_long_name_is_bounded_in_the_opt_in_line(self):
        """`commands.py`'s opt-in line repeats the name back with contain's fixed bound
        (ruling 45) — a report line, not a row anybody picks from."""
        long_name = "c" * 161
        declare_profiles(self, f'[harness.{long_name}]\nkind = "codex"\ncommand = ["codex"]\n')
        approve_every_profile(self)
        out = " ".join(l for _s, l in self.wired(install=True))
        self.assertIn("c" * 160 + "...", out)
        self.assertNotIn(long_name, out)

    def test_an_install_label_reaches_the_report_contained_once(self):
        """`wiring.install` contains every label it hands back, so `_wire_profiles` prints it
        as it is — contained twice, a path's backslashes would double."""
        approve_every_profile(self)
        with mock.patch.object(plugincache, "install",
                               return_value=("installed", "at c:\\x\x1b")), \
                spawns([], listing()):
            out = commands._wire_profiles(config.ROOT, install=True)
        line = next(l for _s, l in out if "claude-alt" in l)
        self.assertIn("c:\\\\x\\u001b", line)
        self.assertNotIn("\\\\\\\\", line)


# --------------------------------------------------------------------------- #
# doctor                                                                       #
# --------------------------------------------------------------------------- #


class DoctorHasARowPerProfile(PersonaIso, unittest.TestCase):
    """One row per profile the selector would list, and none of them on a hook path."""

    def setUp(self):
        super().setUp()
        make_plane(self)
        declare_profiles(self)
        approve_every_profile(self)
        # Which harnesses this machine "has", stated (B5): the listing depends on it.
        self.enterContext(on_path("claude", "opencode"))

    def test_every_listed_profile_has_a_row_in_order(self):
        with spawns([], listing()):
            names = doctor.check_names()
            produced = [r.name for r in doctor.run_all()]
        self.assertEqual(names, produced)
        at = names.index("harness profiles")
        self.assertEqual(names[at + 1:at + 5], ["profile claude", "profile opencode",
                                                "profile claude-work",
                                                "profile codex-pinned"])

    def test_a_built_in_whose_program_is_not_installed_has_no_row(self):
        with on_path("claude", "codex", "opencode"):
            with_codex = doctor.profile_row_names()
        self.assertIn("profile codex", with_codex)
        self.assertNotIn("profile codex", doctor.profile_row_names())

    def test_a_declared_profile_whose_program_is_not_installed_still_has_a_row(self):
        """`wiring.py`'s listing (C): a declaration is somebody's, and doctor is where they
        find out it cannot run — so only a BUILT-IN is dropped for a missing program."""
        with on_path():
            names = doctor.profile_row_names()
        self.assertEqual(names, ["profile claude-work", "profile codex-pinned"])

    def test_a_wired_row_is_ok_with_no_hint(self):
        with spawns([], listing(entry(project=config.ROOT))):
            rows = {r.name: r for r in doctor.check_profile_wiring()}
        row = rows["profile claude-work"]
        self.assertEqual(row.status, doctor.OK)
        self.assertEqual(row.hint, "")

    def test_an_unwired_row_names_the_install(self):
        with spawns([], listing()):
            rows = {r.name: r for r in doctor.check_profile_wiring()}
        row = rows["profile claude-work"]
        self.assertEqual(row.status, doctor.WARN)
        self.assertEqual(row.hint, "charter harness install claude-work")

    def test_an_unknown_row_says_the_check_could_not_run(self):
        with spawns([], "", rc=1):
            rows = {r.name: r for r in doctor.check_profile_wiring()}
        self.assertEqual(rows["profile claude-work"].hint, doctor._NOT_CHECKED_HINT)

    def test_a_probe_that_raises_costs_one_row(self):
        real = wiring.detect

        def boom(p, *, cwd):
            if p.name == "claude-work":
                raise RuntimeError("this one row")
            return real(p, cwd=cwd)

        with spawns([], listing()), \
                mock.patch.object(wiring, "detect", boom):
            rows = {r.name: r for r in doctor.check_profile_wiring()}
        self.assertEqual(rows["profile claude-work"].status, doctor.WARN)
        self.assertIn("profile codex-pinned", rows)

    def test_a_probe_that_raises_is_said_contained(self):
        """B2, ruling 35: an exception's text is whatever raised it put there, and here that
        is a path or an argv out of the profile."""
        def boom(p, *, cwd):
            raise RuntimeError("bad \x1b[2K path")

        with mock.patch.object(wiring, "detect", boom):
            rows = doctor.check_profile_wiring()
        for r in rows:
            self.assertNotIn("\x1b", r.detail)
        self.assertIn("RuntimeError: bad \\u001b[2K path",
                      {r.name: r for r in rows}["profile claude-work"].detail)

    def test_a_row_says_how_much_of_a_long_detail_it_did_not_show(self):
        """B1, ruling 45: a row the operator acts on counts what it hid. A clipped fix that
        does not say so reads as the whole fix."""
        long = "d" * 400
        with mock.patch.object(wiring, "detect",
                               return_value=wiring.Wiring(wiring.UNWIRED, long, "f" * 170)):
            row = {r.name: r for r in doctor.check_profile_wiring()}["profile claude"]
        self.assertEqual(row.detail, "d" * contain.DISPLAY_LIMIT + "… +240 not shown")
        self.assertEqual(row.hint, "f" * contain.DISPLAY_LIMIT + "… +10 not shown")
        self.assertNotIn("...", row.detail)

    def test_a_long_profile_name_is_counted_in_its_row_and_its_hint(self):
        """A 161-character name — one past the display bound — on the row name, which sizes
        the column, and on the `charter <name>` hint an unapproved row gives."""
        long_name = "n" * 161
        declare_profiles(self, f'[harness.{long_name}]\nkind = "claude"\ncommand = ["claude"]\n')
        with on_path():
            names = doctor.profile_row_names()
            rows = doctor.check_profile_wiring()
        self.assertEqual(names, [f"profile {'n' * 160}… +1 not shown"])
        self.assertEqual([r.name for r in rows], names)
        hint = f"charter {long_name}"
        self.assertEqual(rows[0].hint, f"{hint[:contain.DISPLAY_LIMIT]}… +9 not shown")

    def test_rows_are_probed_concurrently(self):
        def slow(p, *, cwd):
            time.sleep(0.3)
            return wiring.Wiring(wiring.WIRED, "slept", "")

        with mock.patch.object(wiring, "detect", slow):
            began = time.monotonic()
            rows = doctor.check_profile_wiring()
        self.assertGreaterEqual(len(rows), 2)
        self.assertLess(time.monotonic() - began, 0.3 * len(rows))

    def test_the_preflight_never_probes(self):
        real = util.run

        def run(cmd, *a, **kw):
            if cmd[:1] in (["claude"], ["opencode"], ["codex"]):
                raise AssertionError(f"{cmd[0]} was spawned on a hook path")
            return real(cmd, *a, **kw)

        with mock.patch.object(wiring, "detect",
                               side_effect=AssertionError("probed on a hook path")), \
                mock.patch.object(util, "run", run):
            rows = doctor.run_all(preflight=True)
        self.assertEqual([r for r in rows if r.name.startswith("profile ")], [])
        self.assertIn("harness profiles", [r.name for r in rows])

    def test_the_preflight_runs_no_git_for_profiles(self):
        """Re-review N8: every git call on a hook path is paid at every session start."""
        real = util.run
        seen = []

        def run(cmd, *a, **kw):
            seen.append(list(cmd))
            return real(cmd, *a, **kw)

        with mock.patch.object(util, "run", run):
            doctor.run_all(preflight=True)
        self.assertEqual([c for c in seen if profiles.LOCAL_FILE in c], [])

    def test_the_names_agree_with_the_preflight(self):
        self.assertEqual(doctor.check_names(preflight=True),
                         [r.name for r in doctor.run_all(preflight=True)])

    def test_a_hand_run_doctor_probes_every_approved_profile(self):
        asked = []

        def seen(p, *, cwd):
            asked.append(p.name)
            return wiring.Wiring(wiring.UNWIRED, "no", "fix it")

        profiletrust.record_launched(profiles.current().profiles["codex-pinned"])
        (config.STATE_DIR / profiletrust.RECORD).write_text("{}")
        with mock.patch.object(wiring, "detect", seen):
            doctor.check_profile_wiring()
        self.assertEqual(asked, ["claude", "opencode"])
        approve_profile(self)
        asked.clear()
        with mock.patch.object(wiring, "detect", seen):
            doctor.check_profile_wiring()
        self.assertEqual(sorted(asked), ["claude", "claude-work", "opencode"])

    def test_one_git_call_however_many_profiles_are_declared(self):
        """The file's ignore check is asked once for the table, not once per row: it is the
        same answer for every profile in the file."""
        real = profiles.ignore_check
        checks = []
        with mock.patch.object(profiles, "ignore_check",
                               side_effect=lambda root: checks.append(root) or real(root)), \
                mock.patch.object(wiring, "_asked",
                                  return_value=wiring.Wiring(wiring.WIRED, "ok", "")):
            doctor.check_profile_wiring()
        # One for the rows; each probed DECLARED profile's own `detect` gate asks again.
        self.assertEqual(len(checks), 1 + 2, checks)

    def test_a_plane_that_declares_nothing_makes_no_git_call_for_its_rows(self):
        (config.ROOT / profiles.LOCAL_FILE).unlink()
        with mock.patch.object(profiles, "ignore_check",
                               side_effect=AssertionError("git for no declared profile")), \
                mock.patch.object(wiring, "_asked",
                                  return_value=wiring.Wiring(wiring.WIRED, "ok", "")):
            rows = doctor.check_profile_wiring()
        self.assertEqual([r.name for r in rows], ["profile claude", "profile opencode"])

    def test_the_session_start_hook_runs_the_preflight(self):
        """The hook runs the same words a person types, so nothing in the process could tell
        the two apart — and a tty test would misread `charter doctor --json` and
        `charter doctor | less` as the hook. Codex trusts hooks by hash, so its users approve
        this one once more; the news entry says so."""
        doc = json.loads((REPO / "hooks" / "hooks.json").read_text())
        commands_run = [h["command"] for group in doc["hooks"]["SessionStart"]
                        for h in group["hooks"]]
        preflight = [c for c in commands_run if "charter doctor" in c]
        self.assertTrue(preflight)
        for c in preflight:
            self.assertIn("charter doctor --preflight", c)

    def test_the_parser_takes_the_preflight_flag(self):
        from charter import cli

        self.assertIs(cli.build_parser().parse_args(["doctor", "--preflight"]).preflight,
                      True)
        self.assertIs(cli.build_parser().parse_args(["doctor"]).preflight, False)


class EveryProfileDerivedTextIsShownEscaped(PersonaIso, unittest.TestCase):
    """Ruling 35, surface by surface: nothing a chat can write into `charter.local.toml`
    reaches a terminal as a control byte.

    The values a profile carries into these sentences are its NAME, its KIND, its COMMAND,
    its `env` values and — through those — the folder each probe asks about. Each is
    interpolated in its own place, so each is asked its own question here: a `contain` that
    went missing anywhere lets a `\r` redraw the line above it, which is how a refusal
    becomes a sentence the operator never sees.
    """

    NASTY = "x\ry\x1b[2K"

    def setUp(self):
        super().setUp()
        make_plane(self)

    def clean(self, *texts):
        for t in texts:
            self.assertNotIn("\r", t)
            self.assertNotIn("\x1b", t)

    def dirty_dir(self, name: str) -> Path:
        d = self.tmp / f"d{self.NASTY}{name}"
        d.mkdir(parents=True, exist_ok=True)
        return d

    def test_a_kind_charter_has_no_check_for_is_named_escaped(self):
        """Unreachable through `profiles.current()`, which refuses a kind the registry does
        not know — and reached the day a kind joins the registry without a wiring check,
        which is exactly when nobody is looking."""
        p = profiles.Profile(name=self.NASTY, kind=self.NASTY, harness=self.NASTY,
                             command=(self.NASTY,), env=(), source=profiles.BUILTIN)
        w = wiring.detect(p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNKNOWN_STATE)
        self.clean(w.detail, w.fix, wiring.refusal(p, cwd=self.tmp))
        self.clean(*(text for _status, text in wiring.install(p, self.tmp)))

    def test_a_reason_charter_may_not_run_it_yet_is_shown_escaped(self):
        """Task 3's sentence about a profile the operator has not approved quotes its NAME,
        which is text the file chose."""
        p = make_profile(self.NASTY)
        answer = wiring.detect(p, cwd=self.tmp)
        self.assertEqual(answer.state, wiring.UNKNOWN_STATE)
        self.clean(answer.detail, answer.fix, wiring.refusal(p, cwd=self.tmp),
                   *(label for _s, label in wiring.install(p, self.tmp)))

    def test_the_claude_detail_names_its_folder_and_scope_escaped(self):
        here = self.dirty_dir("cwd")
        p = approve_profile(self, make_profile(
            self.NASTY, env=[("CLAUDE_CONFIG_DIR", str(self.dirty_dir("cc")))]))
        with spawns([], listing(entry(scope=self.NASTY, project=here, enabled=False))):
            w = wiring.detect(p, cwd=here)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.clean(w.detail, w.fix, wiring.refusal(p, cwd=here))

    def test_the_codex_detail_names_its_home_escaped(self):
        home = self.dirty_dir("cx")
        (home / "config.toml").write_text("[x")
        p = approve_profile(self, make_profile(self.NASTY, kind="codex",
                                               env=[("CODEX_HOME", str(home))]))
        w = wiring.detect(p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNKNOWN_STATE)
        self.clean(w.detail, w.fix, wiring.refusal(p, cwd=self.tmp), wiring.by_hand(p))

    def test_the_opencode_detail_names_its_probe_home_and_neighbours_escaped(self):
        xdg = self.dirty_dir("xdg")
        home = xdg / "opencode"
        (home / "plugin").mkdir(parents=True)
        opencode.refresh_shim(home)
        (home / "plugin" / f"a{self.NASTY}.ts").write_text("Object.hasOwn = () => false;\n")
        p = approve_profile(self, make_profile(self.NASTY, kind="opencode",
                                               command=[f"oc{self.NASTY}"],
                                               env=[("XDG_CONFIG_HOME", str(xdg))]))
        answer = json.dumps({"plugin": [f"file://{home / opencode.SHIM_PATH}"]})
        with spawns([], answer):
            w = wiring.detect(p, cwd=self.tmp)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.clean(w.detail, w.fix)
        with spawns([], raises=util.ProcTimeout(["oc"], 5.0)):
            self.clean(wiring.refusal(p, cwd=self.tmp))

    def test_the_install_and_the_wiring_report_name_it_escaped(self):
        import io
        from contextlib import redirect_stderr, redirect_stdout

        p = approve_profile(self, make_profile(
            self.NASTY, kind="codex", env=[("CODEX_HOME", str(self.dirty_dir("cx2")))]))
        buf = io.StringIO()
        with redirect_stderr(buf), redirect_stdout(buf), \
                mock.patch.object(commands_harness, "_resolve", return_value=(p, "")), \
                mock.patch.object(profiles, "ignored_refusal", return_value=""):
            commands_harness.cmd_harness_install(SimpleNamespace(name=self.NASTY))
        self.clean(buf.getvalue())

    def test_the_init_report_names_a_skipped_profile_escaped(self):
        import io
        from contextlib import redirect_stderr

        p = make_profile(self.NASTY)
        read = profiles.ProfileSet(profiles={self.NASTY: p}, refused=(), default=None,
                                  default_from=None, default_refused=None)
        buf = io.StringIO()
        with mock.patch.object(profiles, "current", return_value=read), \
                redirect_stderr(buf):
            commands._say_profile_wiring(commands._wire_profiles(config.ROOT, install=True))
        self.assertIn("not approved yet", buf.getvalue())
        self.clean(buf.getvalue())

    def test_a_hostile_install_label_reaches_init_escaped(self):
        """`commands.py`'s install line, pinned with a label carrying the bytes a path out of
        the profile's `env` can carry."""
        p = approve_profile(self, make_profile(self.NASTY))
        read = profiles.ProfileSet(profiles={self.NASTY: p}, refused=(), default=None,
                                  default_from=None, default_refused=None)
        buf = io.StringIO()
        with mock.patch.object(profiles, "current", return_value=read), \
                mock.patch.object(plugincache, "install",
                                  return_value=("failed", f"at {self.NASTY}")), \
                redirect_stderr(buf):
            commands._say_profile_wiring(commands._wire_profiles(config.ROOT, install=True))
        self.assertIn("at x", buf.getvalue())
        self.clean(buf.getvalue())

    def test_a_hostile_scope_and_command_reach_the_enable_fix_escaped(self):
        """`wiring.py`'s disabled answer puts the listed `scope` in its detail and the
        profile's own `env` and command in its fix — three values neither charter nor the
        operator wrote."""
        here = self.dirty_dir("cwd2")
        p = approve_profile(self, make_profile(self.NASTY, command=[f"cl{self.NASTY}"],
                                               env=[("CLAUDE_CONFIG_DIR", self.NASTY)]))
        with spawns([], listing(entry(scope="local", project=here, enabled=False))):
            w = wiring.detect(p, cwd=here)
        self.assertEqual(w.state, wiring.UNWIRED)
        self.assertIn("clx\\u000dy", w.fix)
        self.clean(w.detail, w.fix)


class TheSeamsTheSweepAsksAbout(PersonaIso, unittest.TestCase):
    """One class for the lines a deletion sweep could not tell from their own absence.

    Each of these is a guard, a fallback or a threaded argument that the behaviour tests
    above reach only through a sibling that would have caught the same case. A sweep names
    them; this is where each gets a test that goes red on its own.
    """

    def setUp(self):
        super().setUp()
        make_plane(self)
        declare_profiles(self)
        self.here = self.tmp / "here"
        self.here.mkdir()

    def test_a_codex_profiles_cache_is_stamped_with_its_own_config_file(self):
        """`_stamp_paths` answers per kind, and a kind that fell through to `[]` would have
        an empty stamp — which matches forever, so the selector would draw a row about a
        Codex home somebody has since wired or unwired."""
        home = self.tmp / "cx"
        home.mkdir()
        (home / "config.toml").write_text(codex_config())
        p = make_profile("codex-alt", kind="codex", env=[("CODEX_HOME", str(home))])
        w = wiring.Wiring(wiring.WIRED, "asked", "")
        wiring.remember(p, cwd=self.here, w=w)
        self.assertEqual(wiring.cached(p, cwd=self.here), w)
        (home / "config.toml").write_text(codex_config(trust=False))
        self.assertIsNone(wiring.cached(p, cwd=self.here))

    def test_the_cache_file_is_the_one_the_docs_and_the_selector_name(self):
        """A path a chat can write is a path the docs state at full volume (ruling 13), so
        the spelling is a fact about this release rather than an internal detail."""
        self.assertEqual(wiring.CACHE, "cache/harness-wiring.json")
        p = profiles.current().profiles["claude"]
        wiring.remember(p, cwd=self.here, w=wiring.Wiring(wiring.WIRED, "x", ""))
        self.assertTrue((Path(config.STATE_DIR) / "cache" / "harness-wiring.json").is_file())

    def test_a_cache_entry_missing_a_field_is_a_miss_rather_than_a_crash(self):
        p = profiles.current().profiles["claude"]
        wiring.remember(p, cwd=self.here, w=wiring.Wiring(wiring.WIRED, "x", ""))
        path = Path(config.STATE_DIR) / wiring.CACHE
        doc = json.loads(path.read_text())
        for entry in doc.values():
            entry.pop("fix", None)
        path.write_text(json.dumps(doc))
        self.assertIsNone(wiring.cached(p, cwd=self.here))

    def test_cmd_doctor_passes_the_preflight_flag_through(self):
        """The flag is the only thing that tells the SessionStart hook apart from a person
        typing the same words, so a `cmd_doctor` that dropped it would put the probes back on
        the hook path with nothing saying so."""
        import io
        from contextlib import redirect_stdout

        buf = io.StringIO()
        with mock.patch.object(wiring, "detect",
                               side_effect=AssertionError("probed on a hook path")), \
                redirect_stdout(buf):
            commands.cmd_doctor(SimpleNamespace(json=True, preflight=True))
        names = [r["name"] for r in json.loads(buf.getvalue())]
        self.assertNotIn("profile claude", names)
        self.assertIn("harness profiles", names)

    def test_a_doctor_row_shows_a_profile_name_escaped(self):
        """Ruling 35 reaches the row NAMES too — and the name column is sized from them, so
        a control byte there would move every other row as well as redraw the line."""
        nasty = make_profile("cl\raude\x1b[2K", source=profiles.BUILTIN)
        with mock.patch.object(wiring, "listed", return_value=[nasty]), \
                mock.patch.object(wiring, "detect",
                                  return_value=wiring.Wiring(wiring.WIRED, "ok", "")):
            names = doctor.profile_row_names()
            rows = doctor.check_profile_wiring()
        for text in [*names, *(r.name for r in rows)]:
            self.assertNotIn("\r", text)
            self.assertNotIn("\x1b", text)

    def test_the_launcher_asks_about_the_directory_it_was_given(self):
        """`_launch` and the pane are already standing in the chat's directory; a `+`, a tab
        and a handoff are not, and the answer is about THEIR directory."""
        calls = []
        with spawns(calls, listing()):
            launcher.refusal(profiles.current().profiles["claude"], root=config.ROOT,
                             attended=False, env=dict(os.environ), cwd=self.here)
        self.assertEqual(str(calls[0].cwd), str(self.here))

    def test_a_codex_probe_charter_cannot_read_names_the_file_to_open(self):
        """Codex has no probe argv — its three marks are a file — so `{probe}` names the
        file. A fall-through to an empty command would print `check by hand:` and stop."""
        home = self.tmp / "cx2"
        home.mkdir()
        (home / "config.toml").write_text("[x")
        p = approve_profile(self, make_profile("codex-alt", kind="codex",
                                               env=[("CODEX_HOME", str(home))]))
        said = wiring.refusal(p, cwd=self.tmp)
        self.assertIn("check by hand: ", said)
        self.assertEqual(shlex.split(said.split("check by hand: ", 1)[1]),
                         ["cat", str(home / "config.toml")])

    def test_the_listing_is_built_ins_in_registry_order_then_the_rest_by_name(self):
        """`charter harness list`'s order, so a doctor row lands where the operator already
        reads that profile."""
        declare_profiles(self, DECLARED_WITH_A_NAME_AFTER_ITS_SIBLING)
        with on_path(*HARNESS_PROGRAMS):
            names = [p.name for p in wiring.listed()]
        self.assertEqual(names, ["claude", "opencode", "codex", "aaa-codex", "claude-work"])
        built_in = [n for n in profiles.builtins() if n in names]
        self.assertEqual(names[:len(built_in)], built_in)
        self.assertEqual(names[len(built_in):], sorted(names[len(built_in):]))

    def test_no_probe_is_spawned_for_a_command_that_is_not_on_path(self):
        """`shutil.which` and the exec are two moments, and the read charter would make in
        between is a spawn of something that is not there."""
        asked = []

        def available(command=("claude",), path=None):
            asked.append(tuple(command))
            return False

        with mock.patch.object(plugincache, "available", available), \
                mock.patch.object(util, "run",
                                  side_effect=AssertionError("spawned anyway")):
            self.assertIsNone(plugincache._claude_json(["list"],
                                                       command=["/nowhere/claude"]))
        self.assertEqual(asked, [("/nowhere/claude",)])

    def test_a_plugin_list_under_an_environment_exec_refuses_is_an_unknown_answer(self):
        """`plugincache._claude_json`'s own `ValueError`: `wiring` catches it one level up
        as well, but `plugincache.install` reaches this read with no such net, and "could not
        read the list" is the answer that installs nothing over it."""
        with mock.patch.object(plugincache, "available", lambda *a, **k: True):
            self.assertIsNone(plugincache._claude_json(["list"], env={"X": "a\x00b"}))
            self.assertEqual(plugincache.install(self.here, env={"X": "a\x00b"})[0],
                             "unknown")

    def test_a_registry_name_resolves_to_the_built_in_of_that_kind(self):
        """`charter harness install claude-code` — the NAME `$CHARTER_HARNESS` carries, which
        is not the word typed after `charter`."""
        p, why = commands_harness._resolve("claude-code")
        self.assertIsNotNone(p, why)
        self.assertEqual(p.name, "claude")

    def ordered(self, *names):
        """`profiles.current()` holding the plane's own profiles in the order *names* says —
        the order a `dict` of them happens to iterate in is exactly what `_resolve` must not
        depend on."""
        read = profiles.current()
        by_name = dict(read.profiles)
        by_name.update({"codex": make_profile("codex", kind="codex", source=profiles.BUILTIN)})
        return mock.patch.object(profiles, "current", return_value=read._replace(
            profiles={n: by_name[n] for n in names}))

    def test_a_declared_sibling_listed_first_is_not_the_built_in(self):
        """Both halves of `_resolve`'s test (ruling 8), each with the profile that would
        answer instead standing FIRST: `claude-work` is Claude Code and not named after its
        kind; `codex` is named after its kind and is not Claude Code."""
        with self.ordered("claude-work", "codex", "claude"):
            p, _why = commands_harness._resolve("claude-code")
        self.assertEqual(p.name, "claude")

    def test_a_refused_names_reason_is_its_own_and_not_the_first_refused(self):
        read = profiles.current()._replace(refused=(
            profiles.Refused("other", profiles.LOCAL_FILE, "other's reason"),
            profiles.Refused("mine", profiles.LOCAL_FILE, "my reason")))
        with mock.patch.object(profiles, "current", return_value=read):
            self.assertEqual(commands_harness._resolve("mine"), (None, "my reason"))

    def test_a_gap_is_warned_about_and_a_write_is_only_noted(self):
        """`init` and `reinit` do not fail over a profile's own account folder, so the only
        thing separating "charter wrote this" from "you have something to do" is which
        stream-level this line goes out at."""
        import io
        from contextlib import redirect_stderr

        buf = io.StringIO()
        statuses = ["skipped", "created", "missing", "opt-in", "unavailable", "unknown",
                    "unvouched", "failed", "refused", "installed", "present", "refreshed",
                    "current", "a-status-nobody-has-written-yet"]
        with redirect_stderr(buf):
            commands._say_profile_wiring([(s, s) for s in statuses])
        lines = [l for l in buf.getvalue().splitlines() if l.strip()]
        # A9: `unavailable` is a gap, not a note — and so is any status a harness adds later.
        self.assertEqual(dict(zip(statuses, (l[0] for l in lines))), {
            "skipped": "!", "created": "•", "missing": "!", "opt-in": "!",
            "unavailable": "!", "unknown": "!", "unvouched": "!", "failed": "!",
            "refused": "!", "installed": "•", "present": "•", "refreshed": "•",
            "current": "•", "a-status-nobody-has-written-yet": "!"})

    def test_nothing_is_wired_while_git_would_carry_the_local_file(self):
        """Every profile in it is refused, and `charter harness list` and doctor's
        `harness profiles` row already say so with the fix. Wiring one anyway would act on a
        declaration charter has just refused."""
        approve_every_profile(self)
        (config.ROOT / ".gitignore").write_text("# nothing ignored\n")
        with mock.patch.object(wiring, "install") as inst:
            self.assertEqual(commands._wire_profiles(config.ROOT, install=True), [])
        inst.assert_not_called()


if __name__ == "__main__":
    unittest.main()
