"""End-to-end smoke tests: invoke the installed CLI as a subprocess, the way a user
actually runs it (``python3 -m charter ...``), and assert it doesn't blow up.

Unlike the rest of the suite (which calls handler functions directly), this module
exercises the real ``__main__`` entrypoint + argument parsing + import graph. That's
the only way to catch a bug like a handler doing a module-level ``from .browser import
runtime`` for a package that was deliberately never ported — every other test can pass
while the command itself is unusable.

Only read-only, side-effect-free, network-free subcommands belong here: ``doctor`` and
``--help`` on the CLI and its command groups. Anything that clones, pushes, or mutates
persistent state (workspace/persona create, secret set, sync, discover, ...) does NOT
belong in this module.
"""

from __future__ import annotations

import os
import subprocess
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Traceback-shaped substrings that must never appear in a successful (or cleanly
# failing) command's output.
_CRASH_MARKERS = ("Traceback (most recent call last):", "ModuleNotFoundError", "ImportError")


class CliSmokeTest(unittest.TestCase):
    """Runs `python3 -m charter ...` as a real subprocess, isolated from the
    developer's own ~/.charter and the umbrella's own inventory/vaults via a throwaway
    CHARTER_HOME, so a doctor run here can't read or write real secrets/state."""

    def setUp(self) -> None:
        import os
        import tempfile

        self.tmp = tempfile.mkdtemp(prefix="charter-smoke-")
        self.env = dict(os.environ)
        self.env["CHARTER_HOME"] = str(Path(self.tmp) / ".charter")
        # Force non-tty color codes off / deterministic output.
        self.env.pop("FORCE_COLOR", None)

    def tearDown(self) -> None:
        import shutil

        shutil.rmtree(self.tmp, ignore_errors=True)

    def _run(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, "-m", "charter", *args],
            cwd=REPO_ROOT,
            env=self.env,
            text=True,
            capture_output=True,
            timeout=60,
        )

    def _assert_clean(self, proc: subprocess.CompletedProcess, *, allow_nonzero: bool = False) -> None:
        combined = (proc.stdout or "") + (proc.stderr or "")
        for marker in _CRASH_MARKERS:
            self.assertNotIn(
                marker, combined,
                f"command crashed ({marker} in output):\n{combined}",
            )
        if not allow_nonzero:
            self.assertEqual(
                proc.returncode, 0,
                f"expected exit 0, got {proc.returncode}:\n{combined}",
            )

    # -- doctor: the first command a user runs -------------------------------- #

    def test_doctor_runs_without_crashing(self) -> None:
        proc = self._run("doctor")
        # doctor's own exit code reflects preflight FAIL/OK, not a crash — a real repo
        # may legitimately report failures (e.g. no glab auth). We only assert it
        # didn't blow up importing a nonexistent module.
        self._assert_clean(proc, allow_nonzero=True)
        self.assertIn("charter preflight", proc.stdout)

    def test_doctor_json_runs_without_crashing(self) -> None:
        proc = self._run("doctor", "--json")
        self._assert_clean(proc, allow_nonzero=True)
        # Valid JSON array of check results.
        import json

        payload = json.loads(proc.stdout)
        self.assertIsInstance(payload, list)
        self.assertTrue(payload)
        names = {item["name"] for item in payload}
        self.assertIn("python3", names)
        # The browser check must be gone, not merely skipped/renamed.
        self.assertNotIn("browser", names)

    # -- --help across every command group: cheap coverage that argparse wiring
    #    and every module it imports at parse time are intact. ------------------ #

    def test_top_level_help(self) -> None:
        self._assert_clean(self._run("--help"))

    def test_doctor_help(self) -> None:
        self._assert_clean(self._run("doctor", "--help"))

    def test_workspace_help(self) -> None:
        self._assert_clean(self._run("workspace", "--help"))

    def test_worktree_help(self) -> None:
        self._assert_clean(self._run("worktree", "--help"))

    def test_vault_help(self) -> None:
        self._assert_clean(self._run("vault", "--help"))

    def test_secret_help(self) -> None:
        self._assert_clean(self._run("secret", "--help"))

    def test_persona_help(self) -> None:
        self._assert_clean(self._run("persona", "--help"))

    def test_persona_secret_help(self) -> None:
        self._assert_clean(self._run("persona", "secret", "--help"))


class TestInitDoesNotPrintOneEnormousLine(unittest.TestCase):
    """The headline is a count; the paths go underneath it.

    That line named every path it had written on one line that does not wrap, and it grew
    every time charter learned to write something new: 184 characters at 0.38, 254 once
    harness wiring landed, 194 after 0.41.0 folded the settings entries together. Folding
    was right and was not enough — the shape was the rest of it.

    It is not only a terminal problem. The README's demo is a real capture, so this line
    set the width of that image: 1518px for one sentence nobody can read at 80 columns.
    The regression is silent — nothing fails, the asset just gets worse each release — so
    it is pinned here.
    """

    MAX = 100

    def _init(self):
        import tempfile
        from pathlib import Path
        d = tempfile.mkdtemp(prefix="charter-initwidth-")
        p = subprocess.run([sys.executable, "-m", "charter", "init",
                            "--forge", "github", "--owner", "acme"],
                           cwd=d, capture_output=True, text=True,
                           env={**os.environ, "CHARTER_ROOT": d,
                                "PYTHONPATH": str(Path(__file__).resolve().parents[1])})
        return p, (p.stdout or "") + (p.stderr or "")

    def test_no_line_of_init_output_is_absurdly_wide(self):
        p, out = self._init()
        # Exit code first. Without it this passes against "No module named charter" — a
        # one-line error is narrow, so the width assertion holds while init never ran.
        self.assertEqual(p.returncode, 0, out)
        widest = max((len(ln) for ln in out.splitlines()), default=0)
        self.assertLessEqual(widest, self.MAX,
                             f"widest init line is {widest} cols:\n" +
                             "\n".join(ln for ln in out.splitlines() if len(ln) > self.MAX))

    def test_it_still_says_what_it_wrote(self):
        """Narrower must not mean vaguer — every path is still named, just underneath."""
        p, out = self._init()
        self.assertEqual(p.returncode, 0, out)
        for expected in ("charter.toml", "personas/", "inventory/", "workspaces/"):
            self.assertIn(expected, out)


if __name__ == "__main__":
    unittest.main()
