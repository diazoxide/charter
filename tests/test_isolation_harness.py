"""The test-isolation harness itself: `PersonaIso` must redirect EVERY `charter.config`
attribute that path-sensitive code might read, so a test never leaks ambient process
state (the real repo's ROOT/HAS_CONTROL_PLANE/etc.) through an un-patched attribute.

Not exercised via PersonaIso subclassing — this drives `PersonaIso.setUp`/cleanup
directly, so it can control the "ambient" value that must NOT leak through, and (for the
second case) the exact temp directory the harness installs.
"""
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config, root
from tests._isolation import PersonaIso, _PATCH


class TestIsolationHarnessHasControlPlane(unittest.TestCase):
    def test_patch_tuple_includes_has_control_plane(self):
        """Pins the landmine directly: a future attribute added to config.py without a
        matching harness entry is exactly this bug shape, recurring."""
        self.assertIn("HAS_CONTROL_PLANE", _PATCH)

    def test_setup_derives_has_control_plane_from_the_temp_root_not_ambient_state(self):
        original = config.HAS_CONTROL_PLANE
        # Simulate the ambient/real process believing it's inside a control plane — the
        # opposite of what a fresh temp ROOT (no charter.toml yet) should say. If
        # PersonaIso.setUp forgot to re-derive the flag, this ambient True would leak
        # straight through and the assertion below would catch it.
        config.HAS_CONTROL_PLANE = True
        case = PersonaIso()
        case.setUp()
        try:
            self.assertIs(config.HAS_CONTROL_PLANE, False)
            # Not just False by luck — actually derived from the harness's own ROOT.
            self.assertEqual(config.HAS_CONTROL_PLANE, (config.ROOT / root.MARKER).is_file())
        finally:
            case._restore()
            config.HAS_CONTROL_PLANE = original

    def test_a_control_plane_marker_in_the_temp_root_is_reflected_as_true(self):
        """A test that drops charter.toml into its own temp ROOT (e.g. to exercise
        instance-config code) must see HAS_CONTROL_PLANE come back True — proving the
        flag is derived from the harness's temp config.ROOT, not a boolean copied from
        ambient process state at import time (which is False in this checkout)."""
        original = config.HAS_CONTROL_PLANE
        fixed_tmp = Path(tempfile.mkdtemp(prefix="charter-iso-fixture-"))
        (fixed_tmp / root.MARKER).write_text("schema = 1\n")
        case = PersonaIso()
        try:
            with mock.patch("tests._isolation.tempfile.mkdtemp", return_value=str(fixed_tmp)):
                case.setUp()
            self.assertEqual(config.ROOT, fixed_tmp)
            self.assertTrue(config.HAS_CONTROL_PLANE)
        finally:
            case._restore()  # also removes fixed_tmp, since case.tmp == fixed_tmp
            config.HAS_CONTROL_PLANE = original


class TestIsolationHarnessInventory(unittest.TestCase):
    """During Task 1's fix round, a test wrote a fake `inventory/repos.json` straight
    into the real checkout because `_PATCH` omitted `INVENTORY` — every other
    well-known config path was redirected into the harness's tmp tree, but
    `inventory.load()`/`save()` kept resolving `config.INVENTORY` against whatever the
    real, un-isolated repo happened to have (ambient state), exactly the same landmine
    shape as `HAS_CONTROL_PLANE` above. This pins the fix."""

    def test_patch_tuple_includes_inventory(self):
        self.assertIn("INVENTORY", _PATCH)

    def test_setup_derives_inventory_from_the_temp_root_not_the_real_repo(self):
        original = config.INVENTORY
        case = PersonaIso()
        case.setUp()
        try:
            # Not the real repo's inventory/repos.json …
            self.assertNotEqual(config.INVENTORY, original)
            # … but derived from the harness's own tmp ROOT, the same way config.py
            # itself derives it at import time (`ROOT / "inventory" / "repos.json"`).
            self.assertEqual(config.INVENTORY, config.ROOT / "inventory" / "repos.json")
            self.assertTrue(str(config.INVENTORY).startswith(str(config.ROOT)))
        finally:
            case._restore()
            self.assertEqual(config.INVENTORY, original)  # restored on cleanup


class EveryRootDerivedPathIsIsolated(PersonaIso):
    """The class of bug, closed — rather than one more test per attribute.

    Every case above pins ONE attribute, which is exactly why four went missing at once:
    `VAULTS_REGISTRY`, `VAULTS_DIR`, `ACTIVE_WORKSPACE_FILE` and `DOCS_DIR` were all
    derived from `STATE_DIR`/`ROOT` at import and none was patched, so the suite wrote
    into the developer's real checkout. The registry was the sharp one — a run replaced a
    real `vaults.json` with fixture data, orphaning every vault registered on that
    machine, which is issue #22's harm arriving by a different road.

    Enumerating `config` instead of listing names means a constant added later is covered
    the day it appears, without anyone remembering this file exists.
    """

    @staticmethod
    def _under(p: Path, base: Path) -> bool:
        try:
            p.relative_to(base)
            return True
        except ValueError:
            return False

    def _path_constants(self) -> list[tuple[str, Path]]:
        out = []
        for name in sorted(dir(config)):
            if name.startswith("_") or name != name.upper():
                continue
            val = getattr(config, name)
            if isinstance(val, Path):
                out.append((name, val))
        return out

    def test_no_config_path_escapes_the_sandbox(self):
        """`relative_to(tmp)`, not "differs from the real value" — an embedded plane's
        worktree root is a SIBLING of ROOT, so a leaked one is outside the real plane too
        and a not-equal check would wave it through."""
        sandbox = [self.tmp.resolve(),
                   (self.tmp.parent / f"{self.tmp.name}.worktrees").resolve()]
        escaped = [f"{n} → {v}" for n, v in self._path_constants()
                   if not any(self._under(Path(v).resolve(), s) for s in sandbox)]
        self.assertEqual(escaped, [], "config paths still pointing outside the test "
                                      "sandbox — a test writing to one of these edits the "
                                      "developer's real checkout: " + "; ".join(escaped))

    def test_every_path_constant_is_named_in_the_patch_tuple(self):
        """Belt and braces: a constant could coincidentally resolve inside the sandbox
        while still being ambient. `_PATCH` is also what `_restore` reads, so a name
        missing here is a value never put back after the test."""
        missing = [n for n, _ in self._path_constants() if n not in _PATCH]
        self.assertEqual(missing, [], f"config path constants absent from _PATCH (add "
                                      f"them there AND derive them in setUp): {missing}")


if __name__ == "__main__":
    unittest.main()
