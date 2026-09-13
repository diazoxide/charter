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
    def test_names_the_two_step_codex_actually_has(self):
        """Pinned against codex-cli 0.147.0 the way every other fact in `codex.py` was.

        `codex plugin --help` offers add / list / marketplace / remove, and the snapshot a
        plugin installs from is refreshed at the MARKETPLACE level — so updating is two
        commands, the same shape as Claude Code's marketplace-then-plugin pair.
        """
        with tempfile.TemporaryDirectory() as tmp:
            status, detail = harness.get(harness.CODEX).upgrade(Path(tmp))
        self.assertEqual(status, "manual")
        self.assertIn("codex plugin marketplace upgrade", detail)
        self.assertIn("codex plugin add charter@charter", detail)

    def test_it_does_not_name_the_subcommand_codex_rejects(self):
        """`codex plugin update` is the command everyone reaches for and the one codex
        does not have: it exits with "unrecognized subcommand 'update'". A rejection is
        the only kind of evidence this file accepts, and this is the rejection."""
        with tempfile.TemporaryDirectory() as tmp:
            _status, detail = harness.get(harness.CODEX).upgrade(Path(tmp))
        self.assertNotIn("codex plugin update", detail)

    def test_the_dead_wiring_table_is_gone(self):
        """`_WIRING` declared hooks charter stopped writing when `_block()` narrowed to
        `shell_environment_policy` — it has been referenced nowhere since."""
        self.assertFalse(hasattr(codex, "_WIRING"))


def _tree(top: Path) -> dict[str, bytes | None]:
    """Every path under *top* with its bytes (``None`` for a directory) — what "wrote
    nothing" is measured against, so a created directory counts as a write too."""
    return {str(p.relative_to(top)): (None if p.is_dir() else p.read_bytes())
            for p in sorted(top.rglob("*"))}


def _config_dirs(tmp: str) -> dict[str, str]:
    """Every harness's config dir pointed at *tmp*, by the variable each one reads, so no
    call here can reach the operator's real `~/.config/opencode`, `~/.claude` or `~/.codex`."""
    return {"XDG_CONFIG_HOME": tmp, "CLAUDE_CONFIG_DIR": tmp, "CODEX_HOME": tmp}


def _older_stamp(g: Path) -> None:
    p = g / opencode.SHIM_PATH
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text("// charter-version: 0.0.1\nold body\n")


def _foreign_beside_older_stamp(g: Path) -> None:
    _older_stamp(g)
    (g / opencode.PLUGIN_DIR / "aaa_boot.ts").write_text("Object.hasOwn = () => false\n")


def _not_ours(g: Path) -> None:
    p = g / opencode.SHIM_PATH
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text("mine, hands off\n")


def _edited(g: Path) -> None:
    p = g / opencode.SHIM_PATH
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(f"// charter-version: {__version__}\nnot what charter generates\n")


#: Each state opencode's shim can be found in, named by what the real call answers there.
#: The older stamp is the one that matters most: the real call replaces it before asking
#: whether charter can vouch for the realm, so a dry run that asked of the file as found
#: would call the plane `manual` where the real call says `moved`.
SHIM_STATES = {
    "absent (moved)": lambda g: None,
    "an older stamp (moved)": _older_stamp,
    "charter's own (current)": opencode.ensure_shim,
    "no stamp (manual)": _not_ours,
    "this version's stamp over another body (manual)": _edited,
    "an older stamp beside a foreign plugin (manual)": _foreign_beside_older_stamp,
}


class ADryRunIsTheSameAnswerWithoutTheMove(unittest.TestCase):
    """`upgrade(root, dry_run=True)` is how `doctor` asks (#1039).

    `doctor.check_version_lock` called the writer to get a sentence, so under opencode a
    diagnosis rewrote the GLOBAL plugin every project on the machine loads. The dry run is
    `apply_ask_rule`'s shape: the same function and the same judgement, minus the write.
    Paired with the real call in every state, so the question `doctor` asks and the move
    `version sync` makes cannot come to disagree about what the plane needs.
    """

    def test_every_harness_answers_the_real_call_s_status_and_sentence_and_writes_nothing(self):
        for h in harness.all():
            for state, arrange in SHIM_STATES.items():
                with self.subTest(harness=h.name, shim=state), \
                     tempfile.TemporaryDirectory() as tmp, \
                     mock.patch.dict(os.environ, _config_dirs(tmp), clear=True):
                    arrange(opencode.global_dir())
                    before = _tree(Path(tmp))
                    would = h.upgrade(Path(tmp), dry_run=True)
                    self.assertEqual(_tree(Path(tmp)), before,
                                     f"{h.name}'s dry run wrote under its config dir")
                    self.assertEqual(would, h.upgrade(Path(tmp)))

    def test_the_real_call_still_moves_opencode_s_shim(self):
        """The other side of the flag: `version sync` and `update` move the plane, and a
        dry run that leaked into them would leave every opencode shim where it was."""
        with tempfile.TemporaryDirectory() as tmp, \
             mock.patch.dict(os.environ, _config_dirs(tmp), clear=True):
            g = opencode.global_dir()
            _older_stamp(g)
            self.assertEqual(harness.get(harness.OPENCODE).upgrade(Path(tmp))[0], "moved")
            self.assertTrue(opencode.shim_is_charters(g))


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

    def test_codex_gets_its_own_two_step_not_claude_code_s_command(self):
        out = self._sync({"CHARTER_HARNESS": "codex"})
        self.assertNotIn("claude plugin update", out)
        self.assertIn("codex plugin add charter@charter", out)


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

    def test_the_row_writes_nothing_under_any_harness_s_config_dir(self):
        """#1039: the row asked opencode how this plane moves by MOVING it — `upgrade`
        rewrote the global shim, from a check the SessionStart hook and agents run
        routinely. A check that writes changes the state it reports. Each shim state the
        real call would write in, under every harness, so a harness that grows a writer
        later is held to this the day it is registered."""
        for h in harness.all():
            for state in ("absent (moved)", "an older stamp (moved)"):
                with self.subTest(harness=h.name, shim=state), \
                     tempfile.TemporaryDirectory() as tmp:
                    env = {"CHARTER_HARNESS": h.name, **_config_dirs(tmp)}
                    with mock.patch.dict(os.environ, env, clear=True):
                        SHIM_STATES[state](opencode.global_dir())
                    before = _tree(Path(tmp))
                    self._hint(env)
                    self.assertEqual(_tree(Path(tmp)), before,
                                     f"doctor's version lock row wrote under {h.name}'s "
                                     f"config dir")


if __name__ == "__main__":
    unittest.main()
