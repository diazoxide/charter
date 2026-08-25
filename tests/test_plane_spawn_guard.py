"""The suite-wide tripwire that refuses to SPAWN a charter against the developer's plane.

The sibling of `test_plane_write_guard.py`, and it follows the same rule: every case here
installs a THROWAWAY directory as "the real plane" and asserts against that. Pointing a
case at the operator's actual plane to prove a spawn is refused would, the first time the
guard regressed, run `gl-refresh` and `persona _gc` against a live machine — which is the
accident this guard exists to prevent.

**How "it was refused" is told apart from "it silently did nothing".** The fake charter
below writes a marker file when it runs. A refused case asserts both that
:class:`RealPlaneSpawn` was raised AND that the marker is absent; the allowed case asserts
the marker is PRESENT. Without that second half, an absent marker would prove nothing —
"the child never ran" and "the child ran and could not write" look identical, which is a
failure mode this repo has shipped before.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config, root
from tests import _planeguard


class WhatIsGuarded(unittest.TestCase):
    def test_popen_is_wrapped_on_the_class_at_package_import(self):
        """The CLASS, not the ``subprocess.Popen`` module attribute.

        `subprocess.run`/`check_output`/`check_call` construct the class directly, and so
        does anything that did ``from subprocess import Popen``. A wrapper on the module
        attribute would watch none of them.
        """
        self.assertEqual(getattr(subprocess.Popen.__init__, "__module__", None),
                         "tests._planeguard",
                         "Popen.__init__ is unwrapped — charter children spawn unseen")

    def test_the_guarded_root_is_this_machines_own_plane(self):
        """Armed against the plane the test PROCESS resolved, not some later one.

        Recomputed from `root.find_root` rather than read off `config.ROOT`, which any
        `PersonaIso` case may have repointed by the time this runs.
        """
        self.assertIn(os.path.abspath(str(root.find_root())), _planeguard._REAL_ROOT)


class _FakePlane(unittest.TestCase):
    """A throwaway plane installed as "the real plane", plus a fake ``charter`` binary."""

    def setUp(self):
        self.real = Path(tempfile.mkdtemp(prefix="spawnguard-real-"))
        self.addCleanup(shutil.rmtree, self.real, True)
        (self.real / root.MARKER).write_text("schema = 1\n")

        self.elsewhere = Path(tempfile.mkdtemp(prefix="spawnguard-else-"))
        self.addCleanup(shutil.rmtree, self.elsewhere, True)
        (self.elsewhere / root.MARKER).write_text("schema = 1\n")

        # A `charter` that leaves evidence. Named `charter` because that is one of the two
        # spellings the guard recognises, and it makes the "allowed" case a real spawn of a
        # real process rather than an assertion about a decision function.
        self.marker = self.elsewhere / "the-child-ran"
        self.fake = self.elsewhere / "charter"
        self.fake.write_text(f"#!/bin/sh\necho ran > {self.marker}\n")
        self.fake.chmod(0o755)

        self.enterContext(mock.patch.object(
            _planeguard, "_REAL_ROOT",
            (str(self.real), str(self.real.resolve()))))

    def run_fake(self, **kw):
        p = subprocess.Popen([str(self.fake)], **kw)
        p.wait()
        return p


class ARefusedSpawn(_FakePlane):
    def test_a_charter_child_that_would_walk_onto_the_real_plane_is_refused(self):
        """No ``$CHARTER_ROOT``, cwd inside the real plane: exactly the 131 detached
        children #527 measured, which resolved the operator's plane by walking up from
        wherever the test happened to be running."""
        with self.assertRaises(_planeguard.RealPlaneSpawn) as caught:
            self.run_fake(cwd=self.real, env={k: v for k, v in os.environ.items()
                                              if k != root.ENV_VAR})
        self.assertFalse(self.marker.exists(),
                         "refused after delegating — the child ran anyway")
        self.assertIn("REFUSED", str(caught.exception))

    def test_a_charter_root_pointing_at_the_real_plane_is_refused(self):
        """Handing the plane across the boundary only helps if it is a DIFFERENT plane."""
        with self.assertRaises(_planeguard.RealPlaneSpawn):
            self.run_fake(cwd=self.elsewhere,
                          env={**os.environ, root.ENV_VAR: str(self.real)})
        self.assertFalse(self.marker.exists())

    def test_a_dash_m_charter_argv_is_recognised_too(self):
        """`util.self_relaunch_argv`'s spelling — the one every self-relaunch site uses.

        The interpreter is `/bin/false` so that a guard which failed to refuse would leave
        a non-zero exit rather than run anything.
        """
        with self.assertRaises(_planeguard.RealPlaneSpawn):
            subprocess.Popen(["/bin/false", "-P", "-m", "charter", "gl-refresh"],
                             cwd=self.real,
                             env={k: v for k, v in os.environ.items()
                                  if k != root.ENV_VAR})

    def test_the_message_names_the_test_and_both_ways_out(self):
        """A tripwire nobody can act on is a tripwire that gets deleted."""
        with self.assertRaises(_planeguard.RealPlaneSpawn) as caught:
            self.run_fake(cwd=self.real, env={k: v for k, v in os.environ.items()
                                              if k != root.ENV_VAR})
        msg = str(caught.exception)
        self.assertIn("ARefusedSpawn.test_the_message_names_the_test_and_both_ways_out",
                      msg)
        self.assertIn("CHARTER_ROOT", msg)      # way out (2): hand it a throwaway plane
        self.assertIn("maybe_spawn", msg)       # way out (1): do not spawn at all
        self.assertIn(str(self.real), msg)      # which plane it would have landed on

    def test_it_is_a_base_exception_so_charters_own_fallbacks_cannot_eat_it(self):
        """`glstate.maybe_spawn` and `update.maybe_spawn` both wrap their `Popen` in
        `except Exception: return`. A tripwire those can catch reports nothing at all."""
        self.assertTrue(issubclass(_planeguard.RealPlaneSpawn, BaseException))
        self.assertFalse(issubclass(_planeguard.RealPlaneSpawn, Exception))


class AnAllowedSpawn(_FakePlane):
    def test_a_child_handed_a_throwaway_plane_really_runs(self):
        """The other half of the evidence: this is what proves an absent marker above
        means "never ran" rather than "could not write"."""
        p = self.run_fake(cwd=self.elsewhere,
                          env={**os.environ, root.ENV_VAR: str(self.elsewhere)})
        self.assertEqual(p.returncode, 0)
        self.assertTrue(self.marker.exists())

    def test_a_child_whose_cwd_is_a_throwaway_plane_really_runs(self):
        """`$CHARTER_ROOT` is not the only isolation that works — a cwd inside another
        plane resolves that one, and `charter init` in a fresh directory resolves none."""
        p = self.run_fake(cwd=self.elsewhere,
                          env={k: v for k, v in os.environ.items()
                               if k != root.ENV_VAR})
        self.assertEqual(p.returncode, 0)
        self.assertTrue(self.marker.exists())

    def test_a_child_that_is_not_charter_is_never_examined(self):
        """The guard is about charter's own plane resolution. Refusing `git`, `tmux` or
        `sh` because they happen to run inside the checkout would make the suite
        unusable — and none of them reads a `charter.toml`."""
        out = subprocess.run([sys.executable, "-c", "print('hi')"], cwd=self.real,
                             capture_output=True, text=True,
                             env={k: v for k, v in os.environ.items()
                                  if k != root.ENV_VAR})
        self.assertEqual(out.stdout.strip(), "hi")


class HowTheChildsPlaneIsResolved(_FakePlane):
    def test_it_asks_find_root_with_the_childs_environment_and_cwd(self):
        """Not with this process's. The whole defect is that the child's answer differs
        from the parent's, so a guard that asked the parent's question would agree with
        every spawn it was meant to catch.
        """
        seen = {}

        def spy(start=None, env=None):
            seen["start"], seen["env"] = start, env
            return self.elsewhere

        with mock.patch.object(root, "find_root", side_effect=spy):
            self.run_fake(cwd=self.elsewhere,
                          env={**os.environ, root.ENV_VAR: str(self.elsewhere)})
        self.assertEqual(Path(seen["start"]), self.elsewhere)
        self.assertEqual(seen["env"].get(root.ENV_VAR), str(self.elsewhere))

    def test_a_child_with_no_env_of_its_own_is_asked_about_ours(self):
        """``env=None`` means the child inherits this process's environment, so that is
        the environment its answer depends on."""
        seen = {}

        def spy(start=None, env=None):
            seen["env"] = env
            return self.elsewhere

        with mock.patch.object(root, "find_root", side_effect=spy):
            self.run_fake(cwd=self.elsewhere)
        self.assertIs(seen["env"], os.environ)

    def test_an_unresolvable_child_falls_back_to_its_own_cwd(self):
        """`charter.config` calls `find_root_or_cwd`, which falls back to the working
        directory when no plane is found — so a child with a bad `$CHARTER_ROOT` and a cwd
        of the real plane still lands on it. Answering "no plane, allow" there would be
        the guard waving through the one case the fallback makes dangerous.
        """
        with self.assertRaises(_planeguard.RealPlaneSpawn):
            self.run_fake(cwd=self.real,
                          env={**os.environ, root.ENV_VAR: str(self.elsewhere / "gone")})
        self.assertFalse(self.marker.exists())


class WhatCharterHandsItsOwnChildren(unittest.TestCase):
    """`util.child_env` — the reason an isolated case satisfies the guard without knowing
    it exists."""

    def setUp(self):
        self.plane = Path(tempfile.mkdtemp(prefix="spawnguard-plane-"))
        self.addCleanup(shutil.rmtree, self.plane, True)

    def test_a_child_is_told_the_plane_this_process_resolved(self):
        from charter import util
        with mock.patch.object(config, "ROOT", self.plane), \
                mock.patch.object(config, "HAS_CONTROL_PLANE", True):
            self.assertEqual(util.child_env()[root.ENV_VAR], str(self.plane))

    def test_an_inherited_pointer_does_not_win_over_the_resolved_plane(self):
        """`config.ROOT` is the plane this process is actually reading and writing. When
        the two disagree, the child's job is to agree with its parent, not with a shell
        variable the parent already declined to follow."""
        from charter import util
        with mock.patch.object(config, "ROOT", self.plane), \
                mock.patch.object(config, "HAS_CONTROL_PLANE", True), \
                mock.patch.dict(os.environ, {root.ENV_VAR: "/somewhere/else"},
                                clear=False):
            self.assertEqual(util.child_env()[root.ENV_VAR], str(self.plane))

    def test_a_planeless_process_hands_nothing(self):
        """`$CHARTER_ROOT` wins outright in `find_root` and a value with no `charter.toml`
        at it RAISES rather than falling back — so a pointer to a non-plane is worse than
        no pointer. The spawners refuse to fork at all in that state; this pins that
        `child_env` would not have lied to the child either."""
        from charter import util
        with mock.patch.object(config, "ROOT", self.plane), \
                mock.patch.object(config, "HAS_CONTROL_PLANE", False), \
                mock.patch.dict(os.environ, {}, clear=True):
            self.assertNotIn(root.ENV_VAR, util.child_env())


class NoBackgroundRefreshWithoutAPlane(unittest.TestCase):
    """Both best-effort refreshers return before forking when there is no plane.

    Not a tidiness rule: outside a plane `config.STATE_DIR` is ``<cwd>/.charter``, so the
    child would scatter charter's caches into whatever directory the render ran in — and,
    having been handed no plane, would go looking for one of its own.
    """

    def test_glstate_does_not_fork(self):
        from charter import glstate
        with mock.patch.object(config, "HAS_CONTROL_PLANE", False), \
                mock.patch.object(glstate.subprocess, "Popen") as popen:
            glstate.maybe_spawn([Path("/tmp")])
        popen.assert_not_called()

    def test_update_does_not_fork(self):
        from charter import update
        with mock.patch.object(config, "HAS_CONTROL_PLANE", False), \
                mock.patch.object(update.subprocess, "Popen") as popen:
            update.maybe_spawn()
        popen.assert_not_called()


if __name__ == "__main__":
    unittest.main()
