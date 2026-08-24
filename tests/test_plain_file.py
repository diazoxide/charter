"""A plain-file vault is never group- or other-readable while it holds new plaintext (#437).

`_save` used to open the vault `O_WRONLY|O_CREAT|O_TRUNC, 0o600` and chmod it afterwards,
with a docstring claiming the plaintext "is never briefly world-readable". The mode
argument to `open(2)` applies **only when the call creates the inode**. For a vault
someone had hand-authored at 0644 it was ignored outright, so the whole of `json.dump`
ran against a world-readable file and the chmod landed after the value was already on
disk. Measured before the fix: pre-existing 0644, mode while the plaintext was on disk
0644, mode after `set` 0600, same inode throughout.

**The plain-file vault storing plaintext at rest is a documented, accepted trade-off and
is not what this file tests.** The defect is the mode, and the sentence that was wrong
about why the mode was safe.

The property: *at the moment the plaintext is handed to the file that will hold it, that
file is not accessible to group or other.* Two things follow from stating it that way.

**The observation is on the descriptor, not on the path.** A `Path.stat()` of the vault
would report the wrong file entirely for a temp-file-plus-`os.replace` implementation —
which is the other reasonable fix — and would say nothing about what happens to a file
swapped in at the path mid-write. `os.fstat(fd)` asks about the object actually being
written, identified by `(st_dev, st_ino)`.

**The instrumentation sits under every spelling of "write a file", and says so when it
misses.** A spy on `json.dump` — the fix the issue suggested testing with — is blind to
`Path.write_text`, to a bare `os.write`, and to the deletion of the guard it is pinning.
This wraps `io.open` (which `os.fdopen`, `open()` and `Path.open` all reach) and
`os.write`, and then asserts the watch actually saw the plaintext go by. If a future
implementation writes through some path none of that covers, this test goes RED asking to
be extended rather than passing on having observed nothing.
"""

from __future__ import annotations

import builtins
import io
import json
import os
import shutil
import stat
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config
from charter.secrets import registry
from charter.secrets.base import VaultError
from charter.secrets.plain_file import PlainFileProvider
from tests._isolation import PersonaIso

#: Fabricated, and shaped so it cannot be mistaken for anyone's credential. Its only job
#: is to be findable in the bytes handed to a descriptor.
PLAINTEXT = "FABRICATED-437-not-a-credential"


class _Watch:
    """Every mode held by every file that plaintext was written into, while it was.

    Keyed by `(st_dev, st_ino)` from an `fstat` of the descriptor being written, so one
    reused file-descriptor NUMBER across two files does not merge two records, and two
    names for one inode do not split one.
    """

    def __init__(self, marker: str) -> None:
        self.marker = marker.encode()
        self.records: dict[tuple, dict] = {}
        self.saw_open = 0

    def note(self, fd: int, data) -> None:
        if isinstance(data, str):
            data = data.encode("utf-8", "replace")
        try:
            st = os.fstat(fd)
        except (OSError, ValueError):
            return
        rec = self.records.setdefault((st.st_dev, st.st_ino),
                                      {"bytes": bytearray(), "modes": []})
        rec["bytes"].extend(data)
        rec["modes"].append(stat.S_IMODE(st.st_mode))

    def loose_modes(self) -> list[int]:
        """Modes that were in force on a file at a moment the plaintext was going into
        it, that another account on the machine could have read through."""
        out = []
        for rec in self.records.values():
            if self.marker in bytes(rec["bytes"]):
                out += [m for m in rec["modes"] if m & 0o077]
        return out

    def saw_the_plaintext(self) -> bool:
        return any(self.marker in bytes(r["bytes"]) for r in self.records.values())


class _SpyFile:
    def __init__(self, f, watch: _Watch) -> None:
        self._f, self._w = f, watch

    def write(self, data):
        try:
            self._w.note(self._f.fileno(), data)
        except (OSError, ValueError, AttributeError):
            pass
        return self._f.write(data)

    def writelines(self, lines):
        for line in lines:
            self.write(line)

    def __getattr__(self, name):
        return getattr(self._f, name)

    def __enter__(self):
        self._f.__enter__()
        return self

    def __exit__(self, *exc):
        return self._f.__exit__(*exc)


class _Watching:
    """Mixin: `self.watching()` installs the watch for the rest of the test."""

    def watching(self, marker: str = PLAINTEXT) -> _Watch:
        """Install the watch for the duration of the test."""
        watch = _Watch(marker)
        real_io_open, real_os_write = io.open, os.write

        def io_open(*a, **kw):
            f = real_io_open(*a, **kw)
            if "r" not in (kw.get("mode") or (a[1] if len(a) > 1 else "r")):
                watch.saw_open += 1
                return _SpyFile(f, watch)
            return f

        def os_write(fd, data):
            watch.note(fd, data)
            return real_os_write(fd, data)

        self.enterContext(mock.patch.object(io, "open", io_open))
        self.enterContext(mock.patch.object(builtins, "open", io_open))
        self.enterContext(mock.patch.object(os, "write", os_write))
        return watch


class _WriteObservingCase(_Watching, unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix="edm-plainfile-"))
        self.addCleanup(lambda: shutil.rmtree(self.tmp, ignore_errors=True))
        self.vault = self.tmp / "v.json"

    def provider(self) -> PlainFileProvider:
        return PlainFileProvider("t", {"file": str(self.vault)})


class APreexistingLooseVault(_WriteObservingCase):
    def setUp(self) -> None:
        super().setUp()
        self.vault.write_text(json.dumps({"OLD": "already here"}) + "\n")
        os.chmod(self.vault, 0o644)
        self.inode = self.vault.stat().st_ino

    def test_a_preexisting_loose_vault_is_tightened_before_the_write(self):
        watch = self.watching()
        self.provider().set("NEW", PLAINTEXT)
        self.assertTrue(
            watch.saw_the_plaintext(),
            "the watch never saw the plaintext reach a descriptor — `_save` now writes "
            "through a path this test does not observe, so it is not bounding anything. "
            "Extend the watch; do not delete the test.")
        self.assertEqual(
            watch.loose_modes(), [],
            "the plaintext was written into a file that was still group- or "
            "other-accessible at that moment")

    def test_the_vault_is_0600_when_the_dust_settles(self):
        self.provider().set("NEW", PLAINTEXT)
        self.assertEqual(stat.S_IMODE(self.vault.stat().st_mode), 0o600)

    def test_delete_takes_the_same_path(self):
        prov = self.provider()
        prov.set("NEW", PLAINTEXT)
        os.chmod(self.vault, 0o644)
        watch = self.watching(marker="already here")
        prov.delete("NEW")
        self.assertTrue(watch.saw_the_plaintext(), "the watch observed no write at all")
        self.assertEqual(watch.loose_modes(), [],
                         "`delete` rewrites the REMAINING secrets, and did so into a "
                         "loose file")

    def test_the_existing_secrets_survive(self):
        prov = self.provider()
        prov.set("NEW", PLAINTEXT)
        self.assertEqual(prov.get("OLD"), "already here")
        self.assertEqual(prov.get("NEW"), PLAINTEXT)


class AVaultThatCannotBeMadePrivate(_WriteObservingCase):
    """A filesystem with fixed permissions — exFAT, many network mounts — accepts the
    chmod and reports the old mode. Reading the mode back off the descriptor is what
    tells the two cases apart; a chmod that returned 0 does not."""

    def setUp(self) -> None:
        super().setUp()
        self.before = json.dumps({"OLD": "already here"}) + "\n"
        self.vault.write_text(self.before)
        os.chmod(self.vault, 0o644)

    def test_nothing_is_written_when_the_mode_cannot_be_settled(self):
        watch = self.watching()
        with mock.patch.object(os, "fchmod", lambda *a: None):
            with self.assertRaises(VaultError) as caught:
                self.provider().set("NEW", PLAINTEXT)
        self.assertIn("could not make it 0600", str(caught.exception),
                      "refused for some other reason than the one under test")
        self.assertFalse(watch.saw_the_plaintext(),
                         "the value reached the file charter had just refused to write")
        self.assertEqual(self.vault.read_text(), self.before,
                         "the previous contents were destroyed by a write that then "
                         "refused to happen")

    def test_the_refusal_names_the_file_and_says_nothing_was_written(self):
        with mock.patch.object(os, "fchmod", lambda *a: None):
            with self.assertRaises(VaultError) as caught:
                self.provider().set("NEW", PLAINTEXT)
        msg = str(caught.exception)
        self.assertIn(self.vault.name, msg)
        self.assertIn("Nothing was written", msg)
        self.assertNotIn(PLAINTEXT, msg)


class AVaultCharterCreatesItself(_WriteObservingCase):
    def setUp(self) -> None:
        super().setUp()
        self.vault = self.tmp / "fresh" / "v.json"

    def test_a_new_vault_is_0600_and_its_directory_is_0700(self):
        watch = self.watching()
        self.provider().set("NEW", PLAINTEXT)
        self.assertTrue(watch.saw_the_plaintext())
        self.assertEqual(watch.loose_modes(), [])
        self.assertEqual(stat.S_IMODE(self.vault.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(self.vault.parent.stat().st_mode), 0o700,
                         "a directory charter creates to hold plaintext vaults lists "
                         "every vault name to every account on the machine")

    def test_the_rotation_sidecar_is_0600_too(self):
        prov = self.provider()
        prov.set("NEW", PLAINTEXT)
        self.assertEqual(stat.S_IMODE(prov._meta_path.stat().st_mode), 0o600)


class TheVaultRegistryTakesTheSamePath(_Watching, PersonaIso):
    """`registry._write` had the identical in-place bug — #437 names it, and calls it
    cosmetic by comparison because the registry carries provider config, paths and
    environment variable NAMES, never a value.

    So it refuses nothing (the SHARED half is deliberately 0644 and committed), but the
    mode still has to be in force before the content is. Tested through the local half,
    which is the one that is meant to be 0600.
    """

    def test_the_local_registry_is_0600_before_its_content_lands(self):
        path = Path(config.VAULTS_REGISTRY)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('{"vaults": {}}\n')
        os.chmod(path, 0o644)

        watch = self.watching(marker="MARKER-437-registry")
        registry.save_registry({"vaults": {"MARKER-437-registry": {"provider": "x"}}})
        self.assertTrue(watch.saw_the_plaintext(),
                        "the watch never saw the registry content reach a descriptor")
        self.assertEqual(watch.loose_modes(), [])
        self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)

    def test_the_shared_registry_stays_committable(self):
        """The other half is meant to be world-readable; tightening it here would break
        the thing it exists for."""
        registry.save_shared({"vaults": {"team": {"provider": "reference"}}})
        path = Path(config.SHARED_VAULTS)
        self.assertEqual(stat.S_IMODE(path.stat().st_mode) & 0o044, 0o044)


if __name__ == "__main__":
    unittest.main()
