"""The suite-wide tripwire that refuses writes into the developer's real `.charter/`.

Every case below installs a THROWAWAY directory as "the real plane" and asserts against
that, never against the operator's actual one. The alternative — pointing a case at the
live `config.STATE_DIR` to prove `rmtree` is refused — would delete a machine's vaults and
running frames the first time the guard regressed, which is the exact accident this file
exists to prevent. The fixture tree is populated and asserted intact afterwards, so a
guard that raised *after* delegating would still be caught.
"""

from __future__ import annotations

import builtins
import io
import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config, root
from tests import _planeguard


class WhatIsGuarded(unittest.TestCase):
    """The two facts that decide whether the guard is pointed at anything at all."""

    def test_the_guarded_directory_is_this_machines_own_plane_state(self):
        """Installed against the plane the test PROCESS resolved, not some later one.

        Recomputed from `root.find_root` rather than read off `config.STATE_DIR`, which
        any `PersonaIso` case may have repointed by the time this runs.
        """
        state = str(config.derive(root.find_root())["STATE_DIR"])
        self.assertIn(os.path.abspath(state), _planeguard._REAL,
                      "the guard is watching a directory that is not this plane's state")

    def test_every_write_primitive_is_wrapped_at_package_import(self):
        """No test can opt out by forgetting a base class, because nothing opted IN.

        Named one by one: each of these is a way to create or destroy a file that some
        part of charter actually uses, and a list that drifts short is a guard with a
        door left open. `os.makedirs` is deliberately absent — it calls the module-level
        `mkdir` by name, so it goes through that wrapper.
        """
        guarded = {"tests._planeguard"}
        for owner, name in ((os, "mkdir"), (os, "rmdir"), (os, "remove"), (os, "unlink"),
                            (os, "rename"), (os, "replace"), (os, "symlink"), (os, "link"),
                            (os, "truncate"), (os, "chmod"), (os, "open"),
                            (shutil, "rmtree"), (builtins, "open"), (io, "open")):
            with self.subTest(call=f"{owner.__name__}.{name}"):
                fn = getattr(owner, name)
                self.assertIn(getattr(fn, "__module__", None), guarded,
                              f"{owner.__name__}.{name} is unwrapped — writes through it "
                              f"reach the real plane unseen")


class _FakePlane(unittest.TestCase):
    """A throwaway directory installed as "the real plane" for the duration of one test."""

    def setUp(self):
        self.plane = Path(tempfile.mkdtemp(prefix="guard-fake-plane-"))
        self.addCleanup(shutil.rmtree, self.plane, True)
        self.elsewhere = Path(tempfile.mkdtemp(prefix="guard-elsewhere-"))
        self.addCleanup(shutil.rmtree, self.elsewhere, True)

        # A populated tree, so "refused" and "deleted, then refused" are distinguishable.
        # The frame is NOT named `<workspace>-<pid>`: nothing here calls `reap`, so the
        # name means nothing, and a name that LOOKS like a frame id would invite a reader
        # to think liveness was being tested. (A `-1` suffix would be worse still — pid 1
        # is launchd, permanently alive, so anything keying off it can never fail.)
        (self.plane / "frame" / "a-frame-still-on-screen").mkdir(parents=True)
        (self.plane / "frame" / "a-frame-still-on-screen" / "exit").write_text("0\n")
        (self.plane / "vaults.json").write_text("{}")
        self.enterContext(mock.patch.object(
            _planeguard, "_REAL", (str(self.plane), str(self.plane.resolve()))))

    def assertPlaneIntact(self):
        exit_file = self.plane / "frame" / "a-frame-still-on-screen" / "exit"
        self.assertEqual(exit_file.read_text(), "0\n")
        self.assertEqual((self.plane / "vaults.json").read_text(), "{}")


class WritesAreRefused(_FakePlane):
    def test_reap_cannot_delete_a_live_frames_state(self):
        """The #402 accident itself: `frame.state.reap` rmtree's a frame directory the
        tmux server did not report live, and a faked server reports none.

        `shutil.rmtree` is checked at its own front door because its recursion deletes via
        `os.unlink(name, dir_fd=fd)` — a bare filename against an open directory, which no
        path-based check can resolve. Asserting the tree is still there is therefore the
        whole test: a guard that only watched the primitives passes the raise assertion
        and fails this one.
        """
        with self.assertRaises(_planeguard.RealPlaneWrite):
            shutil.rmtree(self.plane / "frame" / "a-frame-still-on-screen")
        self.assertPlaneIntact()

    def test_a_frame_directory_cannot_be_minted(self):
        with self.assertRaises(_planeguard.RealPlaneWrite):
            (self.plane / "frame" / "demo-2").mkdir(parents=True)
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.makedirs(self.plane / "frame" / "demo-3")
        self.assertFalse((self.plane / "frame" / "demo-2").exists())
        self.assertFalse((self.plane / "frame" / "demo-3").exists())

    def test_a_record_cannot_be_deleted(self):
        """`planegit.record_push` unlinks `plane-push.json` on a successful push — the
        developer's own record of a stranded plane, erased by a test that only meant to
        inspect an argv."""
        rec = self.plane / "vaults.json"
        with self.assertRaises(_planeguard.RealPlaneWrite):
            rec.unlink(missing_ok=True)
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.remove(rec)
        self.assertPlaneIntact()

    def test_a_file_cannot_be_written_over(self):
        for mode in ("w", "a", "x", "r+"):
            with self.subTest(mode=mode), self.assertRaises(_planeguard.RealPlaneWrite):
                builtins.open(self.plane / "vaults.json", mode)
        with self.assertRaises(_planeguard.RealPlaneWrite):
            (self.plane / "vaults.json").write_text("clobbered")
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.open(self.plane / "vaults.json", os.O_WRONLY | os.O_TRUNC)
        self.assertPlaneIntact()

    def test_a_file_cannot_be_moved_in_or_out(self):
        """Both ends of a rename: the source vanishes and the destination is overwritten,
        so watching one argument leaves the other as a way in."""
        outside = self.elsewhere / "spare"
        outside.write_text("x")
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.rename(self.plane / "vaults.json", outside)
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.replace(outside, self.plane / "vaults.json")
        self.assertPlaneIntact()
        self.assertEqual(outside.read_text(), "x")

    def test_a_link_cannot_be_planted(self):
        """`os.symlink(target, link)` CREATES its second argument and only reads its
        first — the opposite of every other call here."""
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.symlink(self.elsewhere, self.plane / "planted")
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.link(self.plane / "vaults.json", self.plane / "hard")
        self.assertFalse((self.plane / "planted").is_symlink())
        self.assertFalse((self.plane / "hard").exists())

    def test_a_relative_path_reaches_the_plane_too(self):
        """A test that `chdir`s into the plane writes with no plane prefix in the string
        at all, and a check that only compared prefixes would wave it straight through."""
        here = os.getcwd()
        self.addCleanup(os.chdir, here)
        os.chdir(self.plane)
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.mkdir("frame/demo-4")
        with self.assertRaises(_planeguard.RealPlaneWrite):
            os.mkdir(os.path.join("frame", os.pardir, "demo-5"))
        os.chdir(here)
        self.assertFalse((self.plane / "frame" / "demo-4").exists())
        self.assertFalse((self.plane / "demo-5").exists())


class WhatIsStillAllowed(_FakePlane):
    """A guard that refused everything would pass every case above and stop the suite
    dead. These are the calls that must keep working."""

    def test_reading_the_real_plane_is_untouched(self):
        self.assertEqual((self.plane / "vaults.json").read_text(), "{}")
        self.assertIn("vaults.json", os.listdir(self.plane))
        with builtins.open(self.plane / "vaults.json") as fh:
            self.assertEqual(fh.read(), "{}")
        self.assertEqual(os.stat(self.plane / "vaults.json").st_size, 2)

    def test_writing_anywhere_else_is_untouched(self):
        (self.elsewhere / "d").mkdir()
        (self.elsewhere / "d" / "f").write_text("ok")
        os.replace(self.elsewhere / "d" / "f", self.elsewhere / "g")
        self.assertEqual((self.elsewhere / "g").read_text(), "ok")
        shutil.rmtree(self.elsewhere / "d")
        self.assertFalse((self.elsewhere / "d").exists())

    def test_a_path_that_merely_starts_with_the_planes_name_is_not_inside_it(self):
        """`<plane>-sibling` shares the plane's string prefix and is a different
        directory; a `startswith` with no separator would refuse it."""
        sibling = Path(str(self.plane) + "-sibling")
        self.addCleanup(shutil.rmtree, sibling, True)
        sibling.mkdir()
        (sibling / "f").write_text("ok")
        self.assertEqual((sibling / "f").read_text(), "ok")


class TheRefusalCannotBeSwallowed(_FakePlane):
    def test_it_is_not_an_exception(self):
        """charter's write paths are wrapped in `except OSError` / `except Exception`
        fallbacks that exist so a degraded environment cannot break a command —
        `record_push` ends in `except OSError: pass`. A tripwire those can catch reports
        nothing and the test goes green over a deleted plane."""
        self.assertFalse(issubclass(_planeguard.RealPlaneWrite, Exception))
        try:
            os.mkdir(self.plane / "swallowed")
        except Exception:                                  # noqa: BLE001 — the point
            self.fail("`except Exception` swallowed the tripwire")
        except _planeguard.RealPlaneWrite:
            pass
        self.assertFalse((self.plane / "swallowed").exists())

    def test_it_names_the_path_and_the_way_out(self):
        with self.assertRaises(_planeguard.RealPlaneWrite) as caught:
            os.mkdir(self.plane / "frame" / "demo-5")
        msg = str(caught.exception)
        self.assertIn(str(self.plane / "frame" / "demo-5"), msg)
        self.assertIn("PersonaIso", msg)
        self.assertIn("CHARTER_ROOT", msg)      # the half PersonaIso cannot fix


if __name__ == "__main__":
    unittest.main()
