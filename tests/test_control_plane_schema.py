"""Schema drift across the whole control plane.

Generalizes the stamp/detect/heal pattern `workspace reinit` already proves. The healing
rule is the load-bearing part: additive only, existing content never touched."""
from __future__ import annotations

import io
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path

from charter import doctor, instance
from tests import _envguard


class SchemaIso(unittest.TestCase):
    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

        self.root = Path(tempfile.mkdtemp(prefix="charter-schema-")).resolve()
        (self.root / "charter.toml").write_text(f"schema = {instance.SCHEMA}\n")

    def _reinit(self):
        """Run `cmd_reinit` against `self.root` as if it were the active control plane."""
        from types import SimpleNamespace
        from charter import commands, config
        from unittest import mock
        with mock.patch.object(config, "ROOT", self.root), \
             mock.patch.object(config, "HAS_CONTROL_PLANE", True):
            return commands.cmd_reinit(SimpleNamespace())


class TestDrift(SchemaIso):
    def test_a_complete_control_plane_has_no_drift(self):
        for d in ("personas", "inventory", "workspaces"):
            (self.root / d).mkdir()
        self.assertEqual(instance.drift(self.root), [])

    def test_missing_baseline_directories_are_reported(self):
        found = instance.drift(self.root)
        self.assertTrue(found)
        self.assertTrue(any("personas" in f for f in found), found)

    def test_drift_names_what_is_missing_not_just_that_something_is(self):
        """A user must be able to act on the message without reading the source."""
        for f in instance.drift(self.root):
            self.assertTrue(any(c.isalpha() for c in f))

    def test_a_file_at_a_baseline_path_is_distinguished_from_missing(self):
        """C1 follow-up: `is_dir()` alone can't tell "absent" from "occupied by a
        file" — a control plane where `personas` exists as a plain file must not be
        reported as plain "missing directory: personas/", which would mislead a
        reader into thinking `reinit` can just create it (it can't, and won't)."""
        (self.root / "personas").write_text("oops\n")
        found = instance.drift(self.root)
        personas_lines = [f for f in found if "personas" in f]
        self.assertEqual(len(personas_lines), 1, found)
        self.assertNotEqual(personas_lines[0], "missing directory: personas/")
        self.assertIn("file", personas_lines[0].lower())
        # the other two baseline dirs are genuinely absent — still reported as such.
        self.assertIn("missing directory: inventory/", found)
        self.assertIn("missing directory: workspaces/", found)


class TestReinitIsAdditive(SchemaIso):
    def test_creates_what_is_missing(self):
        self.assertEqual(self._reinit(), 0)
        self.assertEqual(instance.drift(self.root), [])

    def test_never_touches_existing_content(self):
        """The rule `workspace reinit` already follows, and the reason reinit is safe to
        run on a repo full of someone's work."""
        (self.root / "personas").mkdir()
        keep = self.root / "personas" / "mine.md"
        keep.write_text("MINE\n")
        self._reinit()
        self.assertEqual(keep.read_text(), "MINE\n")

    def test_is_idempotent(self):
        self._reinit()
        self.assertEqual(self._reinit(), 0)
        self.assertEqual(instance.drift(self.root), [])


class TestReinitFileBlocksBaseline(SchemaIso):
    """C1 (CRITICAL): a baseline path occupied by a *file* used to fall through
    `is_dir()` straight into `mkdir()`, raising an uncaught `FileExistsError` — a raw
    traceback, no message, no `return 1`. Reported live by a reviewer. `reinit` must
    fail clearly instead, and — per the additive rule — never delete or rename the
    user's file to make room."""

    def test_returns_1_and_names_the_blocked_path_without_raising(self):
        blocker = self.root / "personas"
        blocker.write_text("not a directory\n")
        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = self._reinit()  # must not raise
        self.assertEqual(rc, 1)
        self.assertIn("personas", buf.getvalue())

    def test_the_blocking_file_is_left_byte_identical(self):
        blocker = self.root / "personas"
        original = b"not a directory\n"
        blocker.write_bytes(original)
        self._reinit()
        self.assertTrue(blocker.is_file(), "reinit must not delete or replace the file")
        self.assertEqual(blocker.read_bytes(), original)

    def test_other_baseline_dirs_are_unaffected_by_one_blocked_path(self):
        """Additive: a file blocking `personas` must not stop `inventory/` and
        `workspaces/` — which have nothing in the way — from being created."""
        (self.root / "personas").write_text("x")
        self._reinit()
        self.assertTrue((self.root / "inventory").is_dir())
        self.assertTrue((self.root / "workspaces").is_dir())

    def test_is_idempotent_while_still_blocked(self):
        (self.root / "personas").write_text("x")
        self.assertEqual(self._reinit(), 1)
        self.assertEqual(self._reinit(), 1)  # re-running doesn't raise or worsen anything
        self.assertEqual((self.root / "personas").read_text(), "x")


class TestReinitNoControlPlane(unittest.TestCase):
    """I3: `cmd_reinit` outside any control plane must fail clearly and create
    nothing — not silently no-op, not scaffold into whatever the cwd happens to be."""

    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="charter-schema-nocp-")).resolve()

    def test_returns_1_with_a_clear_message_and_creates_nothing(self):
        from types import SimpleNamespace
        from unittest import mock
        from charter import commands, config

        buf = io.StringIO()
        with mock.patch.object(config, "ROOT", self.root), \
             mock.patch.object(config, "HAS_CONTROL_PLANE", False):
            with redirect_stderr(buf):
                rc = commands.cmd_reinit(SimpleNamespace())
        self.assertEqual(rc, 1)
        self.assertIn("no control plane", buf.getvalue().lower())
        self.assertEqual(list(self.root.iterdir()), [])


class TestDoctorControlPlaneSchemaCheck(SchemaIso):
    """I2: `doctor`'s "schema" check (`check_control_plane_schema`) had zero
    automated coverage — verified only by hand. Follows the same
    mock.patch.object(config, "ROOT"/"HAS_CONTROL_PLANE") pattern used by
    `TestDoctorSurfacesConfigError` in test_config.py for the sibling checks."""

    def _check(self):
        from unittest import mock
        from charter import config
        with mock.patch.object(config, "ROOT", self.root), \
             mock.patch.object(config, "HAS_CONTROL_PLANE", True):
            return doctor.check_control_plane_schema()

    def test_reports_the_check_when_drift_is_present(self):
        # no baseline dirs created — every one of them is missing.
        result = self._check()
        self.assertEqual(result.status, doctor.WARN)
        self.assertIn("personas", result.detail)
        self.assertIn("charter reinit", result.hint)

    def test_is_quiet_when_there_is_no_drift(self):
        for d in ("personas", "inventory", "workspaces"):
            (self.root / d).mkdir()
        result = self._check()
        self.assertEqual(result.status, doctor.OK)


if __name__ == "__main__":
    unittest.main()
