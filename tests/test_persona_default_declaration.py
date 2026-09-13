"""A plane declares its front door in `charter.toml`, beside `[workspace] default`.

The committed mechanism that already existed — `personas/.default` — is a dotfile inside
`personas/`, invisible to `ls`, documented in no page, and (issue #255) unused even in
charter's own plane. It keeps working; the declaration a consumer can actually find moves
to the file they already read to understand their control plane.

charter learns *which* persona is the front door and nothing about what that persona is
called: no name is hardcoded anywhere in the engine.
"""
from __future__ import annotations

import errno
import io
import os
import unittest
from contextlib import redirect_stderr
from types import SimpleNamespace
from unittest import mock

from tests._isolation import PersonaIso
from charter import config, instance, persona


class DeclaredDefaultIso(PersonaIso):
    def _no_env(self):
        return mock.patch.dict(
            os.environ,
            {k: v for k, v in os.environ.items() if k != "CHARTER_PERSONA"},
            clear=True)

    def declare(self, name: str) -> None:
        """Write a control plane whose `[persona] default` is *name*."""
        (config.ROOT / "charter.toml").write_text(
            f'schema = 1\n\n[[forge]]\nkind = "github"\nowner = "acme"\n\n'
            f'[persona]\ndefault = "{name}"\n')


class TestInstanceReadsTheDeclaration(DeclaredDefaultIso):
    def test_reads_the_declared_default_persona(self):
        self.declare("steward")
        cfg = instance.load(config.ROOT)
        self.assertEqual(instance.default_persona_of(cfg), "steward")

    def test_silence_is_none_not_a_guess(self):
        """A plane that declares no front door has none — charter never picks one."""
        (config.ROOT / "charter.toml").write_text('schema = 1\n')
        self.assertIsNone(instance.default_persona_of(instance.load(config.ROOT)))

    def test_a_blank_declaration_is_treated_as_absent(self):
        (config.ROOT / "charter.toml").write_text('schema = 1\n\n[persona]\ndefault = "  "\n')
        self.assertIsNone(instance.default_persona_of(instance.load(config.ROOT)))


class TestResolutionOrder(DeclaredDefaultIso):
    def test_the_declaration_is_adopted_when_nothing_else_is_set(self):
        self.make_persona("steward", role="Steward", vault="none")
        self.declare("steward")
        with self._no_env():
            self.assertEqual(persona.resolve_active(), "steward")

    def test_the_source_names_charter_toml(self):
        """`source()` is what the session briefing prints, so it must say where the
        identity came from — a consumer looking for the wrong file cannot change it."""
        self.make_persona("steward", role="Steward", vault="none")
        self.declare("steward")
        with self._no_env():
            self.assertEqual(persona.source(), "charter.toml")

    def test_charter_toml_outranks_the_legacy_dotfile(self):
        self.make_persona("steward", role="S", vault="none")
        self.make_persona("older", role="O", vault="none")
        (config.PERSONAS_DIR / ".default").write_text("older\n")
        self.declare("steward")
        with self._no_env():
            self.assertEqual(persona.resolve_active(), "steward")

    def test_the_dotfile_still_resolves_when_charter_toml_is_silent(self):
        """Back-compat: a plane that adopted `personas/.default` keeps working untouched."""
        self.make_persona("older", role="O", vault="none")
        (config.PERSONAS_DIR / ".default").write_text("older\n")
        (config.ROOT / "charter.toml").write_text('schema = 1\n')
        with self._no_env():
            self.assertEqual(persona.resolve_active(), "older")
            self.assertEqual(persona.source(), "committed-default")

    def test_the_local_choice_still_outranks_the_declaration(self):
        """`charter persona use` is a developer's own selection; a plane-wide declaration
        must never override the person sitting at the terminal."""
        self.make_persona("steward", role="S", vault="none")
        self.make_persona("forge", role="F", vault="none")
        self.declare("steward")
        persona.set_active("forge")
        with self._no_env():
            self.assertEqual(persona.resolve_active(), "forge")

    def test_the_environment_still_outranks_the_declaration(self):
        self.make_persona("steward", role="S", vault="none")
        self.declare("steward")
        with mock.patch.dict(os.environ, {"CHARTER_PERSONA": "forge"}):
            self.assertEqual(persona.resolve_active(), "forge")


class TestDanglingDeclaration(DeclaredDefaultIso):
    def test_a_declaration_naming_no_persona_resolves_to_none(self):
        """Fail toward no change: a renamed or deleted persona leaves the session with no
        identity rather than a broken one. Saying so is `doctor`'s job, not this one's."""
        self.declare("ghost")
        with self._no_env():
            self.assertIsNone(persona.resolve_active())
            self.assertEqual(persona.source(), "none")



class TestWritingTheDeclaration(DeclaredDefaultIso):
    """`charter persona default <name>` writes the declaration a consumer can find."""

    def _plane(self, extra: str = "") -> None:
        (config.ROOT / "charter.toml").write_text(
            '# a plane someone hand-edited\nschema = 1\n\n'
            '[[forge]]\nkind = "github"\nowner = "acme"\n\n'
            '[workspace]\ndefault = "scratch"\n' + extra)

    def _run(self, name=None, clear=False) -> int:
        from charter import commands_persona
        return commands_persona.cmd_persona_default(
            SimpleNamespace(name=name, clear=clear))

    def test_setting_writes_charter_toml_not_the_dotfile(self):
        self._plane()
        self.make_persona("steward", role="S", vault="none")
        self.assertEqual(self._run(name="steward"), 0)
        cfg = instance.load(config.ROOT)
        self.assertEqual(instance.default_persona_of(cfg), "steward")
        self.assertFalse((config.PERSONAS_DIR / ".default").exists())

    def test_the_rest_of_the_file_survives_verbatim(self):
        """charter.toml is hand-edited and carries comments; stdlib TOML cannot write, so
        the edit is textual and must not disturb a line it does not own."""
        self._plane()
        self.make_persona("steward", role="S", vault="none")
        self._run(name="steward")
        text = (config.ROOT / "charter.toml").read_text()
        self.assertIn("# a plane someone hand-edited", text)
        self.assertIn('owner = "acme"', text)
        self.assertIn('[workspace]', text)

    def test_the_workspace_default_is_not_the_one_rewritten(self):
        """`[workspace] default` and `[persona] default` are the same key name in two
        sections — the edit is confined to its own section's line span."""
        self._plane()
        self.make_persona("steward", role="S", vault="none")
        self._run(name="steward")
        cfg = instance.load(config.ROOT)
        self.assertEqual(instance.default_workspace_of(cfg, "fallback"), "scratch")
        self.assertEqual(instance.default_persona_of(cfg), "steward")

    def test_setting_it_twice_replaces_rather_than_appends(self):
        self._plane()
        self.make_persona("steward", role="S", vault="none")
        self.make_persona("forge", role="F", vault="none")
        self._run(name="steward")
        self._run(name="forge")
        text = (config.ROOT / "charter.toml").read_text()
        self.assertEqual(text.count("[persona]"), 1)
        self.assertEqual(instance.default_persona_of(instance.load(config.ROOT)), "forge")

    def test_an_unknown_persona_is_refused(self):
        self._plane()
        self.assertEqual(self._run(name="ghost"), 1)
        self.assertIsNone(instance.default_persona_of(instance.load(config.ROOT)))

    def test_clearing_removes_the_declaration_and_the_legacy_dotfile(self):
        """Both rungs, or `--clear` would leave a plane still declaring a front door."""
        self._plane()
        self.make_persona("steward", role="S", vault="none")
        self.make_persona("older", role="O", vault="none")
        self._run(name="steward")
        (config.PERSONAS_DIR / ".default").write_text("older\n")
        self.assertEqual(self._run(clear=True), 0)
        with self._no_env():
            self.assertIsNone(persona.resolve_active())

    def test_setting_it_while_the_dotfile_survives_names_the_migration(self):
        """A plane with both would resolve through charter.toml and leave the old file
        quietly disagreeing — say so at the moment it becomes true."""
        self._plane()
        self.make_persona("steward", role="S", vault="none")
        (config.PERSONAS_DIR / ".default").write_text("older\n")
        buf = io.StringIO()
        with redirect_stderr(buf):
            self._run(name="steward")
        out = buf.getvalue().lower()
        self.assertIn("personas/.default", out)
        # Naming the file is not enough — it must say the old one no longer decides.
        self.assertIn("ignored", out)


class TestClearingSaysWhatHappened(DeclaredDefaultIso):
    """`--clear` reports what charter removed, not what it was asked to remove (#1010, ADR 0013).

    The declared default persona is the front door every session with nothing chosen walks
    through, so a clear that silently did not happen keeps routing sessions there after the
    operator was told it would stop. `workspace default --clear` had the same defect (#955).
    """

    _plane = TestWritingTheDeclaration._plane
    _run = TestWritingTheDeclaration._run

    def setUp(self) -> None:
        super().setUp()
        if os.geteuid() == 0:
            self.skipTest("root ignores the mode, so a refused write cannot be staged")

    def _clear(self) -> tuple[int, str]:
        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = self._run(clear=True)
        return rc, buf.getvalue()

    def _read_only(self, path) -> None:
        mode = path.stat().st_mode & 0o777
        path.chmod(0o500 if path.is_dir() else 0o400)
        self.addCleanup(path.chmod, mode)

    def test_a_charter_toml_it_could_not_write_is_named_and_exits_non_zero(self):
        """Measured in #1010 with `charter.toml` read-only: `✓ Cleared the declared default
        persona`, exit 0, and `declared_default()` still `steward`. `set_default_persona`
        returned False and nothing asked it."""
        self._plane()
        self.make_persona("steward", role="S", vault="none")
        self._run(name="steward")
        toml = config.ROOT / "charter.toml"
        self._read_only(toml)
        rc, out = self._clear()
        self.assertEqual(rc, 1, out)
        self.assertNotIn("Cleared", out)
        self.assertIn(f"could not update {toml}", out)
        # The OS's words, and no cause of charter's: a read-only file, an unreadable one and a
        # full disk all land here, and naming one of them is the ADR 0009 failure.
        self.assertIn(os.strerror(errno.EACCES), out)
        self.assertNotIn("control plane?", out)
        self.assertEqual(persona.declared_default(), "steward")

    def test_a_legacy_dotfile_it_could_not_remove_is_named_not_a_traceback(self):
        """Measured in #1010 with `personas/` read-only: `legacy.unlink()` raised
        `PermissionError` and the command ended in a traceback. The declaration in
        charter.toml is gone by then, so the dotfile is what every session with nothing
        chosen adopts now, and the error names the file that still stands."""
        self._plane()
        self.make_persona("steward", role="S", vault="none")
        self.make_persona("older", role="O", vault="none")
        self._run(name="steward")
        legacy = config.PERSONAS_DIR / ".default"
        legacy.write_text("older\n")
        self._read_only(config.PERSONAS_DIR)
        rc, out = self._clear()
        self.assertEqual(rc, 1, out)
        self.assertNotIn("Cleared", out)
        self.assertIn(f"could not remove {legacy} ({os.strerror(errno.EACCES)})", out)
        self.assertIn("which is left in place", out)
        self.assertEqual(persona.default_persona(), "older")

    def test_a_charter_toml_with_nothing_to_remove_is_not_written_at_all(self):
        """A clear leaves its emptied `[persona]` header behind, so a plane that clears again
        has a section and no key. Now that a failed write is reported, rewriting that file
        unchanged would fail a clear on a read-only charter.toml that had nothing to take out
        of it, while the one rung that did declare a default, the dotfile, was removable."""
        self._plane("\n[persona]\n")
        self.make_persona("older", role="O", vault="none")
        legacy = config.PERSONAS_DIR / ".default"
        legacy.write_text("older\n")
        self._read_only(config.ROOT / "charter.toml")
        rc, out = self._clear()
        self.assertEqual(rc, 0, out)
        self.assertIn("Cleared the declared default persona", out)
        self.assertFalse(legacy.exists())

    def test_a_charter_toml_with_no_persona_section_is_left_byte_for_byte(self):
        """No `[persona]` header at all is the other "nothing to undeclare". `_set_key` returns
        before its append for it; without that return a clear falls through to the branch that
        adds a section, and writes `[persona]` with `default = "None"` into a plane that
        declared nothing. The deletion sweep found that return unpinned on #1020."""
        self._plane()
        toml = config.ROOT / "charter.toml"
        before = toml.read_bytes()
        rc, out = self._clear()
        self.assertEqual(rc, 0, out)
        self.assertIn("No default persona was declared.", out)
        self.assertEqual(before, toml.read_bytes())

    def test_with_no_charter_toml_there_is_nothing_to_clear_and_it_says_so(self):
        """A root with no charter.toml declares nothing in it. Before #1010 that answer came
        from a `False` nobody read; asking the OS for the file's words must not turn a
        missing file into a failure the reader is sent to fix."""
        self.assertFalse((config.ROOT / "charter.toml").exists())
        rc, out = self._clear()
        self.assertEqual(rc, 0, out)
        self.assertIn("No default persona was declared.", out)
        self.assertNotIn("could not", out)

if __name__ == "__main__":
    unittest.main()
