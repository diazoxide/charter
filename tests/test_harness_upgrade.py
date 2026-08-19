"""Every harness answers "how does MY installed charter artifact move?" — itself.

Before this, two code paths answered that question without knowing about each other:
`update.plugin_version_here()` (Claude Code's `$CLAUDE_PLUGIN_ROOT`) and
`Harness.stale_wiring()` (opencode's stamped shim). `cmd_version_sync` consulted only the
first and then printed a Claude Code command unconditionally, so an opencode user was told
to run `claude plugin update charter@charter` — a command with nothing to do with their
install.

The contract is four statuses and no fifth: charter MOVED it, it is already CURRENT, a
host owns it so charter can only NAME the command (`manual`), or charter does not know how
this harness updates (`absent`). `absent` is not a hole to fill with a plausible-looking
command: `base.Deficit` already says an invented remedy "sends somebody off to configure
something that does not exist", and that applies with more force here, where the command
would be run rather than read.
"""

from __future__ import annotations

import io
import os
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import __version__, doctor, harness, update
from charter.harness import codex, opencode
from tests._isolation import PersonaIso

STATUSES = {"moved", "current", "manual", "absent"}


class EveryHarnessAnswers(unittest.TestCase):
    def test_every_registered_harness_returns_a_known_status(self):
        """A harness added to KINDS is covered by `update` the day it is registered —
        the stated reason the registry exists, rather than a literal in `update` that
        somebody has to remember."""
        with tempfile.TemporaryDirectory() as tmp:
            for h in harness.all():
                with self.subTest(harness=h.name), \
                     mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": tmp}, clear=True):
                    status, detail = h.upgrade(Path(tmp))
                    self.assertIn(status, STATUSES)
                    self.assertTrue(detail.strip(), "a status with no detail explains nothing")


class ClaudeCode(unittest.TestCase):
    def test_names_the_plugin_command_and_does_not_run_it(self):
        """The host owns the plugin: `claude` may be absent, may prompt for a scope, and
        the command mutates the reader's editor install. charter says what to run."""
        h = harness.get(harness.CLAUDE_CODE)
        with tempfile.TemporaryDirectory() as tmp:
            status, detail = h.upgrade(Path(tmp))
        self.assertEqual(status, "manual")
        self.assertEqual(detail, update.PLUGIN_SYNC_CMD)


class OpenCode(unittest.TestCase):
    """opencode's shim is charter's OWN file, stamped by charter and already rewritten by
    `init`/`reinit` — so moving it is not a new liberty, and `refresh_shim` is the writer
    that already knows how."""

    def _global(self, tmp: str) -> Path:
        with mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": tmp}, clear=True):
            return opencode.global_dir()

    def test_a_stale_shim_is_moved_and_restamped(self):
        with tempfile.TemporaryDirectory() as tmp:
            g = self._global(tmp)
            p = g / opencode.SHIM_PATH
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text("// charter-version: 0.0.1\nold body\n")
            with mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": tmp}, clear=True):
                status, _ = harness.get(harness.OPENCODE).upgrade(Path(tmp))
                self.assertEqual(status, "moved")
                self.assertEqual(opencode.shim_version(g), __version__)

    def test_a_current_shim_is_left_alone(self):
        with tempfile.TemporaryDirectory() as tmp:
            g = self._global(tmp)
            opencode.ensure_shim(g)
            with mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": tmp}, clear=True):
                status, _ = harness.get(harness.OPENCODE).upgrade(Path(tmp))
        self.assertEqual(status, "current")

    def test_a_shim_charter_did_not_write_is_never_overwritten(self):
        """Additive-only: an operator who edited the shim keeps their edit and is told,
        which is the trade `refresh_shim` already makes."""
        with tempfile.TemporaryDirectory() as tmp:
            g = self._global(tmp)
            p = g / opencode.SHIM_PATH
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text("mine, hands off\n")
            with mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": tmp}, clear=True):
                status, _ = harness.get(harness.OPENCODE).upgrade(Path(tmp))
            self.assertEqual(status, "manual")
            self.assertEqual(p.read_text(), "mine, hands off\n")


class Codex(unittest.TestCase):
    def test_admits_it_does_not_know_rather_than_guessing(self):
        """`codex.py` pins every fact against the binary, never its documentation. A
        `codex plugin update` nobody has run would be the first line in it that is a
        guess — and this one gets EXECUTED by a reader, not merely read."""
        with tempfile.TemporaryDirectory() as tmp:
            status, detail = harness.get(harness.CODEX).upgrade(Path(tmp))
        self.assertEqual(status, "absent")
        self.assertNotIn("codex plugin update", detail)
        self.assertIn("charter harness list", detail)

    def test_the_dead_wiring_table_is_gone(self):
        """`_WIRING` declared hooks charter stopped writing when `_block()` narrowed to
        `shell_environment_policy` — it has been referenced nowhere since."""
        self.assertFalse(hasattr(codex, "_WIRING"))


class VersionSyncRoutesThroughTheHarness(unittest.TestCase):
    """The defect this member was extracted to remove.

    `cmd_version_sync` read `$CLAUDE_PLUGIN_ROOT` and then printed
    `claude plugin update charter@charter` unconditionally. Under opencode that variable
    is absent, so the branch fell through to advice about a harness the reader is not in.
    """

    def _sync(self, env: dict) -> str:
        from charter import commands

        err = io.StringIO()
        with mock.patch.dict(os.environ, env, clear=True), \
             mock.patch("charter.instance.load", return_value={}), \
             mock.patch("charter.instance.locked_version", return_value="9.9.9"), \
             redirect_stderr(err), redirect_stdout(io.StringIO()):
            commands.cmd_version_sync(SimpleNamespace(cli=False))
        return err.getvalue()

    def test_opencode_is_not_told_to_run_a_claude_code_command(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = self._sync({"CHARTER_HARNESS": "opencode", "XDG_CONFIG_HOME": tmp})
        self.assertNotIn("claude plugin update", out)

    def test_claude_code_still_gets_its_command(self):
        out = self._sync({"CHARTER_HARNESS": "claude-code"})
        self.assertIn(update.PLUGIN_SYNC_CMD, out)

    def test_codex_is_told_what_charter_does_not_know(self):
        out = self._sync({"CHARTER_HARNESS": "codex"})
        self.assertNotIn("claude plugin update", out)
        self.assertIn("has not pinned", out)


class DoctorNamesTheRightHarnessToo(PersonaIso):
    """The third site of the same defect.

    `check_version_lock`'s LAST branch — "not running under the plugin", which is every
    opencode session, every Codex session and every bare terminal — ended with "To move
    THIS plane only: claude plugin update charter@charter". Correct for exactly one of
    those readers.
    """

    def _hint(self, env: dict) -> str:
        (self.tmp / "charter.toml").write_text('schema = 1\n\n[charter]\nversion = "9.9.9"\n')
        with mock.patch.dict(os.environ, env, clear=True):
            return doctor.check_version_lock().hint

    def test_opencode_is_not_told_to_run_a_claude_code_command(self):
        with tempfile.TemporaryDirectory() as tmp:
            hint = self._hint({"CHARTER_HARNESS": "opencode", "XDG_CONFIG_HOME": tmp})
        self.assertNotIn("claude plugin update", hint)

    def test_the_shared_install_note_survives(self):
        """It is the reason the row exists: the binary is machine-global, so conforming
        it here can put another plane into drift."""
        with tempfile.TemporaryDirectory() as tmp:
            hint = self._hint({"CHARTER_HARNESS": "opencode", "XDG_CONFIG_HOME": tmp})
        self.assertIn("machine-global", hint)


if __name__ == "__main__":
    unittest.main()
