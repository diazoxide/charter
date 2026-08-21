"""#324 — the spawn cooldown was measured from the wrong end.

`maybe_spawn` touched the lock immediately after `Popen`, so `SPAWN_COOLDOWN` ran from
SPAWN time. If the child never finished, the cache entries were still stale when the
cooldown lapsed and the next render spawned another — with no bound on how many detached
forge processes could accumulate, each holding the forge credential.

Fixed independently of the timeout, and these tests are why: with a bound in place a hung
child eventually dies, but "eventually" is not "not stacking". A refresh that is still
RUNNING must suppress the next one however long it has been running, and the cooldown
must start when the previous one FINISHED.

Nothing here spawns a real refresh — `Popen` is faked, and the one real process is a
`python -c pass` used to obtain a pid that is definitely dead.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
import unittest
from unittest import mock

from charter import glstate

from tests._isolation import PersonaIso


def _fake_popen(pid):
    """A `Popen` stand-in reporting *pid*, and a record of every call made."""
    calls = []

    def popen(cmd, **kw):
        calls.append(cmd)
        return mock.MagicMock(pid=pid)

    return popen, calls


def _a_definitely_dead_pid() -> int:
    """A real pid that has exited and been reaped. Preferred over a large made-up
    number, which is a guess about the machine rather than a fact about it."""
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    return p.pid


class RespawnTests(PersonaIso):
    def _dirs(self):
        d = self.tmp / "somerepo"
        d.mkdir(parents=True, exist_ok=True)
        return [d]

    def _age_lock(self, seconds: float) -> None:
        """Backdate the lock as if it had been written *seconds* ago."""
        lock = glstate._lock_file()
        t = time.time() - seconds
        os.utime(lock, (t, t))

    def _spawn_once(self, pid):
        """Drive one successful spawn and assert it happened — the precondition every
        test below depends on."""
        popen, calls = _fake_popen(pid)
        with mock.patch.object(glstate.subprocess, "Popen", side_effect=popen):
            glstate.maybe_spawn(self._dirs())
        self.assertEqual(len(calls), 1, "the first spawn did not happen — proves nothing")
        self.assertTrue(glstate._lock_file().exists(), "the lock was not armed")
        return calls

    def _try_spawn_again(self):
        popen, calls = _fake_popen(os.getpid())
        with mock.patch.object(glstate.subprocess, "Popen", side_effect=popen):
            glstate.maybe_spawn(self._dirs())
        return calls

    # --- the defect ----------------------------------------------------------------

    def test_a_running_refresh_suppresses_the_next_one_past_the_cooldown(self):
        """The core of #324. `os.getpid()` is this process — unambiguously alive — so
        the recorded refresh is genuinely still running when the cooldown lapses."""
        self._spawn_once(os.getpid())
        self._age_lock(glstate.SPAWN_COOLDOWN + 60)
        self.assertEqual(self._try_spawn_again(), [],
                         "a second refresh was spawned while the first was still running")

    def test_a_finished_refresh_stops_suppressing_once_the_cooldown_lapses(self):
        """The other side: suppression must not become permanent. A refresh that
        finished and marked itself done frees the next one after `SPAWN_COOLDOWN`."""
        self._spawn_once(os.getpid())
        glstate._mark_done()
        self._age_lock(glstate.SPAWN_COOLDOWN + 1)
        self.assertEqual(len(self._try_spawn_again()), 1)

    def test_a_crashed_refresh_does_not_suppress_forever(self):
        """A child that died without marking itself done leaves its pid behind. Once it
        is gone, the cooldown is all that stands in the way."""
        self._spawn_once(_a_definitely_dead_pid())
        self._age_lock(glstate.SPAWN_COOLDOWN + 1)
        self.assertEqual(len(self._try_spawn_again()), 1)

    def test_a_wedged_refresh_is_eventually_replaced(self):
        """The ceiling. A refresh still running after `STUCK_AFTER` would produce data
        already stale several times over, so waiting on it is worse than replacing it —
        and this is also what keeps a recycled pid from suppressing refreshes forever."""
        self._spawn_once(os.getpid())
        self._age_lock(glstate.STUCK_AFTER + 1)
        self.assertEqual(len(self._try_spawn_again()), 1)

    # --- the cooldown now runs from completion --------------------------------------

    def test_the_refresh_marks_the_lock_when_it_finishes(self):
        """`refresh` is what the detached child runs, so it is what knows the work is
        over. Called with no trees, it touches nothing on the network."""
        self._spawn_once(os.getpid())
        self._age_lock(glstate.SPAWN_COOLDOWN + 60)
        glstate.refresh([])
        lock = glstate._lock_file()
        self.assertLess(time.time() - lock.stat().st_mtime, 5,
                        "the lock's mtime is still the SPAWN time, not the completion")
        self.assertEqual(lock.read_text().strip(), "",
                         "a finished refresh must not still claim to be in flight")

    def test_completion_restarts_the_cooldown(self):
        """The behaviour that mtime carries: after a refresh finishes, the next spawn
        waits a full `SPAWN_COOLDOWN` from THAT moment."""
        self._spawn_once(os.getpid())
        self._age_lock(glstate.SPAWN_COOLDOWN + 60)
        glstate.refresh([])                      # finishes now
        self.assertEqual(self._try_spawn_again(), [],
                         "the cooldown did not restart when the refresh completed")

    # --- the render path may never raise --------------------------------------------

    def test_a_corrupt_lock_never_raises(self):
        """`maybe_spawn` runs on the status line. Whatever is in this file — a truncated
        write, a hand edit, a pid from another machine — it renders."""
        lock = glstate._lock_file()
        lock.parent.mkdir(parents=True, exist_ok=True)
        for junk in ("", "   ", "not-a-pid", "-1", "0", "99999999999999999999", "\x00"):
            lock.write_text(junk)
            self._age_lock(glstate.SPAWN_COOLDOWN + 1)
            with mock.patch.object(glstate.subprocess, "Popen",
                                   return_value=mock.MagicMock(pid=os.getpid())):
                glstate.maybe_spawn(self._dirs())   # must not raise

    def test_a_popen_that_reports_no_usable_pid_still_arms_the_cooldown(self):
        """A `Popen` stand-in — or a platform quirk — may not give an int. The cooldown
        must still arm, or a spawn that DID happen invites another in 120s."""
        with mock.patch.object(glstate.subprocess, "Popen",
                               return_value=mock.MagicMock(pid=object())):
            glstate.maybe_spawn(self._dirs())
        self.assertTrue(glstate._lock_file().exists())
        self.assertEqual(self._try_spawn_again(), [], "the cooldown was not armed")


if __name__ == "__main__":
    unittest.main()
