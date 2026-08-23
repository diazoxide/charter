"""#390: `python -m charter` prepends the CURRENT WORKING DIRECTORY to `sys.path`
before it even looks for the `charter` package — undocumented nowhere, but `-m`'s own
well-documented behaviour. Charter re-launches itself as
`[sys.executable, "-m", "charter", ...]` from seven places (five reported, two more
found by grepping the tree for every `sys.executable` — see
`tests/test_self_relaunch_argv.py`'s own module docstring for the full list), each
setting the child's cwd to something a caller controls: a project directory, a workspace
root, wherever an operator's pane happened to start. On any of those that happens to
contain its own `charter/` package — a charter checkout dogfooding itself, the common
case for anyone developing charter — the child imports THAT tree instead of the
installed one.

This is the slow half, real subprocesses against a real decoy `charter/` package that
genuinely shadows (`TheDecoyGenuinelyShadows` proves it directly, not by assumption).
`tests/test_self_relaunch_argv.py` is the fast half: it pins that every production call
site actually builds an argv carrying `-P` (3.11+, `pyproject.toml` already requires
it), using mocks — a passing test here on ONE call site does not, by itself, prove the
other six were fixed.

`PYTHONPATH`, not a real installed distribution, stands in below for "the real package,
findable without depending on cwd": this dev checkout has no separate `uv tool`/`pipx`
install of its own to test against — confirmed by hand, `python3 -m charter` run from an
unrelated directory with no `PYTHONPATH` set raises `ModuleNotFoundError` (this
sandbox's own test suite only imports `charter` at all because IT, too, is
conventionally run with the repo root as cwd). `-P`/`PYTHONSAFEPATH` strip only the
cwd/script-dir entry `-m`/`-c` auto-prepend, never a `PYTHONPATH` entry or a real
site-packages one — indistinguishable from `-P`'s own point of view, so this is a
faithful stand-in for a real install rather than a shortcut around the mechanism being
tested (verified by hand: `PYTHONPATH=<repo> python3 -m charter --version` run from a
decoy cwd prints the decoy; the same command with `-P` added prints the repo's own
installed version).
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

import charter
from charter import util
from charter.frame import layout

REPO_ROOT = Path(__file__).resolve().parent.parent
INSTALLED_VERSION = charter.__version__
DECOY_VERSION = "0.0.0-decoy"

#: Just enough argparse to answer `--version` and to refuse `panel` — an old charter
#: build with no `panel` command is exactly what #390's field report was.
_DECOY_MAIN = '''\
import argparse
from . import __version__
p = argparse.ArgumentParser(prog="charter")
p.add_argument("--version", action="version", version=f"charter {__version__}")
sub = p.add_subparsers(dest="command")
sub.add_parser("doctor")
p.parse_args()
'''


class WithADecoyCwd(unittest.TestCase):
    """A temp dir whose own `charter/` package is a different, fake version — "a
    directory containing a charter/ package", #390's own words."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="charter-decoy-"))
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        pkg = self.tmp / "charter"
        pkg.mkdir()
        (pkg / "__init__.py").write_text(f'__version__ = "{DECOY_VERSION}"\n')
        (pkg / "__main__.py").write_text(_DECOY_MAIN)

        self.env = dict(os.environ)
        # Stands in for a real install — see the module docstring for why this is a
        # faithful substitute for `-P`'s own purposes, not a shortcut around them.
        self.env["PYTHONPATH"] = str(REPO_ROOT)
        # Never let a spawned child touch this machine's real ~/.charter.
        self.env["CHARTER_HOME"] = str(self.tmp / ".charter-home")


class TheDecoyGenuinelyShadows(WithADecoyCwd):
    """Without this class, every test below would be trivially satisfiable by a decoy
    that fails to shadow. This project has already caught four distinct flavours of
    test that cannot fail — a decoy like that would be a fifth."""

    def test_bare_dash_m_imports_the_decoy_not_the_real_package(self):
        p = subprocess.run([sys.executable, "-m", "charter", "--version"],
                           cwd=self.tmp, env=self.env,
                           capture_output=True, text=True, timeout=15)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(p.stdout.strip(), f"charter {DECOY_VERSION}")

    def test_bare_dash_m_reproduces_the_reported_panel_failure(self):
        """The exact field report: both panels dead, status 2 —
        ``charter: error: argument command: invalid choice: 'panel'`` — because the
        checkout's own tree, with no ``panel`` command, shadowed the installed one that
        has it."""
        p = subprocess.run([sys.executable, "-m", "charter", "panel", "top",
                            "--session", "f-1"],
                           cwd=self.tmp, env=self.env,
                           capture_output=True, text=True, timeout=15)
        self.assertEqual(p.returncode, 2)
        self.assertIn("invalid choice: 'panel'", p.stderr)


class SelfRelaunchArgvIsImmuneToTheDecoy(WithADecoyCwd):
    """The mechanism `util.self_relaunch_argv` — every call site's shared helper —
    actually buys."""

    def test_reports_the_installed_version_not_the_decoys(self):
        argv = util.self_relaunch_argv("--version")
        p = subprocess.run(argv, cwd=self.tmp, env=self.env,
                           capture_output=True, text=True, timeout=15)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(p.stdout.strip(), f"charter {INSTALLED_VERSION}",
                         f"imported the decoy despite -P: stdout={p.stdout!r} "
                         f"stderr={p.stderr!r}")


class APanelSpawnedTheWayCmdLaunchSpawnsIt(WithADecoyCwd):
    """#390's own reported symptom, proved against the REAL production call site:
    `commands_frame.cmd_launch` builds the panel argv via `layout.panel_argvs`, whose
    `panel_command` calls `util.self_relaunch_argv()` itself. tmux itself is never
    started here — `panel_argvs` is pure (see its own docstring), so the argv it builds
    is extracted and run directly as the subprocess a real tmux pane would otherwise
    run.

    Nothing is passed in: the interpreter half used to be a `charter_argv` argument this
    test handed the helper's own output to, which meant this test proved the helper was
    immune and said nothing about what production passed. `panel_command` now owns that
    half for the launcher and for `cmd_respawn` alike, so the argv below is production's
    with nothing supplied by the test — which is also what makes `RespawnRunsTheSameArgv`
    below cover the respawn path for free."""

    def _panel_argv(self) -> list[str]:
        [cmd] = layout.panel_argvs(slots=["top"], session="f-1", socket="testsock",
                                   harness_pane="%0")
        dashdash = cmd.index("--")
        return cmd[dashdash + 1:]

    def test_runs_rather_than_exiting_2(self):
        with subprocess.Popen(self._panel_argv(), cwd=self.tmp, env=self.env,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              text=True) as proc:
            try:
                time.sleep(0.6)
                rc = proc.poll()
                if rc is not None:
                    out, err = proc.communicate()
                    self.fail(f"panel exited early (rc={rc}) instead of running — the "
                             f"#390 symptom itself:\nstdout={out!r}\nstderr={err!r}")
            finally:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=5)


class RespawnRunsTheSameArgvAgainstTheDecoy(WithADecoyCwd):
    """The respawn path, against the same decoy — the site that actually regressed.

    `commands_frame.cmd_respawn` runs `respawn-pane -t %N -- <layout.panel_command(...)>`,
    and `respawn-pane` starts that command in THE PANE'S OWN cwd. For anyone dogfooding
    charter that is a charter checkout, so a respawn argv without `-P` is #390 verbatim,
    on the one path where the panel has already died once and tmux's own
    `Pane is dead (status 2)` is all the operator gets. Same decoy, same measurement as
    the launcher's argv above; what differs is only which production function built it.
    """

    def test_the_respawn_command_runs_rather_than_exiting_2(self):
        argv = layout.panel_command(slot="top", session="f-1")
        with subprocess.Popen(argv, cwd=self.tmp, env=self.env,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              text=True) as proc:
            try:
                time.sleep(0.6)
                rc = proc.poll()
                if rc is not None:
                    out, err = proc.communicate()
                    self.fail(f"the respawn argv exited early (rc={rc}) instead of "
                              f"running — #390 on the respawn path:\n"
                              f"stdout={out!r}\nstderr={err!r}")
            finally:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=5)


if __name__ == "__main__":
    unittest.main()
