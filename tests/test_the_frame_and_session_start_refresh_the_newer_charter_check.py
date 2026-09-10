"""The frame and SessionStart refresh the newer-charter check, and a tick cannot flood it.

#938. `charter _version-check` is the detached child that answers "is a newer charter
published?" into ``.charter/cache/update.json``, and `update.maybe_spawn` is the only thing
that starts it. For its whole life that function had one caller, `statusline._brand`, and
by 0.60.0 no Claude Code chat reached it through anything charter sets up: inside a frame
the footer command returns before it renders (#412, 0.52.0), and outside one `charter init`
writes no footer at all (#895, 0.57.0). On the plane that reported it the cache was five
days old and two releases behind, and `charter version` called that install up to date.

The CI-state cache never went stale that way because it has three triggers: `render`, the
frame's gather, and SessionStart. The update cache had only the first. This module pins the
other two.

**Each trigger is asserted from both sides.** A recorder standing in for `update.maybe_spawn`
proves the trigger is wired and proves nothing about what it costs, so the other half keeps
the real `maybe_spawn` and records only the fork. Its two throttles, `REFRESH_TTL` and the
`SPAWN_COOLDOWN` lock, then run for real. That half is the one that matters for the frame: a
panel whose gather cache is missing calls `gather.read`, and so `gather.scan`, on every
repaint (`frame/panel.py`), so a trigger that forked per call would fork per tick. #938's
evidence counted `maybe_spawn` calls per `--watch` repaint; this counts forks per frame tick.

**The render path is pinned too**, because it is what `charter statusline --watch` and
opencode's `/charter` still reach, and the issue's table lists both as surfaces that refresh.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from charter import config, glstate, hooks, root, statusline, update
from charter.frame import gather
from tests._isolation import (PersonaIso, PlaneIso, make_plane, no_update_check_in,
                              pin_update_channel, point_config_at, run_hook)

#: A chat id in the shape `state.new_chat_id` mints. Nothing creates its frame directory, so
#: `gather.read` finds no cache under it and falls through to a live `scan` on every call,
#: which is the worst case a panel can put a trigger in.
FID = "demo.1"

#: Repaints per measurement. Well past two, so a throttle that held for one extra call
#: and then let go would still show up as more than one fork.
TICKS = 25


def _record_version_checks(case) -> list[list[str]]:
    """Record every `charter _version-check` that would be forked, and fork none of them.

    `update.maybe_spawn` stays real, so the lock it touches and the cache age it reads decide
    what happens, which is the point of the measurement. Only the fork is replaced. Every
    other `Popen` is handed to the one that was in place, so a `git` a scan runs still runs
    and `tests._planeguard` still sees it.
    """
    spawned: list[list[str]] = []
    real = subprocess.Popen

    def popen(args, *rest, **kw):
        argv = [str(a) for a in args] if isinstance(args, (list, tuple)) else [str(args)]
        if "_version-check" in argv:
            spawned.append(argv)
            return mock.MagicMock()
        return real(args, *rest, **kw)

    patcher = mock.patch.object(update.subprocess, "Popen", popen)
    patcher.start()
    case.addCleanup(patcher.stop)
    return spawned


def _no_forge_refresh(case) -> None:
    """Stub the OTHER spawner only.

    `tests._isolation.no_background_refresh` stubs both, and the update check is what these
    cases are about. An empty workspace gives `glstate.maybe_spawn` nothing stale to fork for
    anyway; the stub keeps that true if a fixture ever grows a clone.
    """
    patcher = mock.patch.object(glstate, "maybe_spawn", lambda *a, **k: None)
    patcher.start()
    case.addCleanup(patcher.stop)


class TheFrameGatherIsATrigger(PersonaIso):
    """`gather.scan` asks for the update check beside the CI-state refresh it already asked
    for, for the reason the comment above that call has given since the frame shipped: a
    frame that never runs beside a rendered status line would otherwise never keep it warm."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)          # both spawners refuse to fork without a plane (#527)
        _no_forge_refresh(self)

    def test_a_gather_asks_for_the_newer_charter_check(self):
        calls = []
        with mock.patch.object(update, "maybe_spawn", lambda: calls.append("asked")):
            gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.tmp))
        self.assertEqual(calls, ["asked"])

    def test_a_check_that_raises_costs_the_gather_nothing(self):
        """`scan` never raises, and every step in it is wrapped on its own so one failure
        degrades one field. The trigger is a step like the others: a panel that lost its
        whole table because a background refresh could not start would be a worse defect
        than the stale cache this fixes."""
        def boom():
            raise RuntimeError("the check could not start")

        with mock.patch.object(update, "maybe_spawn", boom):
            data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.tmp))
        self.assertEqual(data["workspace"], config.DEFAULT_WORKSPACE)
        self.assertEqual(data["repos"], [])


class AFrameTickForksAtMostOneCheck(PersonaIso):
    """The measurement. The real `update.maybe_spawn`, the real throttles, and a frame that
    repaints :data:`TICKS` times with no gather cache to read."""

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        _no_forge_refresh(self)
        self.spawned = _record_version_checks(self)

    def _tick(self, times: int = TICKS) -> None:
        for _ in range(times):
            gather.read(FID)      # what `panel.py` calls on every repaint

    def test_a_stale_cache_forks_one_check_across_every_tick(self):
        self._tick()
        self.assertEqual(
            len(self.spawned), 1,
            f"{TICKS} repaints forked {len(self.spawned)} `_version-check` children. One "
            f"is the trigger working; more is the cooldown lock no longer holding on the "
            f"frame's path, and zero is the frame not asking at all (#938).")
        self.assertTrue(update._lock_file().exists(),
                        "the lock is what holds the other ticks back")

    def test_a_frame_that_outlives_the_cooldown_asks_again_once(self):
        """What makes this a refresh and not a one-shot: a framed chat left open past the
        cooldown, on a cache that is still stale, gets exactly one more check."""
        self._tick()
        self.assertEqual(len(self.spawned), 1, "the first stretch of ticks asked nothing")
        past = time.time() - update.SPAWN_COOLDOWN - 60
        os.utime(update._lock_file(), (past, past))
        self._tick()
        self.assertEqual(len(self.spawned), 2)

    def test_a_fresh_answer_forks_nothing(self):
        """The control for the one above: `REFRESH_TTL` alone, with no lock anywhere, is
        enough to keep every tick quiet."""
        cache = update._cache_file()
        cache.parent.mkdir(parents=True, exist_ok=True)
        cache.write_text('{"latest": "0.0.1", "ts": %f}' % time.time())
        self._tick()
        self.assertEqual(self.spawned, [])
        self.assertFalse(update._lock_file().exists())


class SessionStartIsATrigger(PlaneIso):
    """`charter hook sessionstart` asks for the update check in-process, so both throttles
    apply and `hooks/hooks.json` gains no command. This is what reaches a Claude Code chat
    outside a frame, on a plane `charter init` wired no footer into."""

    def test_session_start_asks_for_the_newer_charter_check(self):
        calls = []
        with mock.patch.object(update, "maybe_spawn", lambda: calls.append("asked")):
            run_hook(hooks.sessionstart, {"session_id": "t"})
        self.assertEqual(calls, ["asked"])

    def test_a_check_that_raises_still_briefs_the_session(self):
        """A hook may cost a session a background refresh, never its briefing. The persona
        is there so the briefing has something in it to lose."""
        self.make_persona("dev", role="Dev", vault="dev")

        def boom():
            raise RuntimeError("the check could not start")

        with mock.patch.dict(os.environ, {"CHARTER_PERSONA": "dev"}), \
             mock.patch.object(update, "maybe_spawn", boom):
            out = run_hook(hooks.sessionstart, {"session_id": "t"})
        self.assertIsNotNone(out, "the session got no briefing at all")
        self.assertIn("dev", out["hookSpecificOutput"]["additionalContext"])

    def test_sessions_starting_together_fork_one_check(self):
        """The real throttles again. Several chats opened at once, which `charter claude`
        with a restored layout does, cost one check between them."""
        spawned = _record_version_checks(self)
        for n in range(4):
            run_hook(hooks.sessionstart, {"session_id": f"t{n}"})
        self.assertEqual(len(spawned), 1)


class AChildHandedAHeldPlaneForksNothing(PersonaIso):
    """`tests._isolation.no_update_check_in`, which the suite's real-child fixtures use.

    A panel, a `frame-gather` or a `charter hook sessionstart` run as a subprocess is out of
    `tests._planeguard`'s sight, so the only thing keeping it off PyPI is the lock that helper
    plants in the plane the child is handed. These ask the same `update.maybe_spawn` a child
    runs, against another plane the way a child resolves one, so a helper that planted the
    lock anywhere else would show up here rather than as a GET nobody sees.
    """

    def setUp(self) -> None:
        super().setUp()
        self.spawned = _record_version_checks(self)
        self.child_plane = Path(tempfile.mkdtemp(prefix="charter-child-held-"))
        self.addCleanup(shutil.rmtree, self.child_plane, True)
        (self.child_plane / root.MARKER).write_text("schema = 1\n")

    def test_a_plane_it_held_forks_nothing(self):
        no_update_check_in(self.child_plane)
        point_config_at(self, self.child_plane)
        update.maybe_spawn()
        self.assertEqual(self.spawned, [])

    def test_the_same_plane_unheld_forks(self):
        """The control: without the helper, that plane is one a child would fork from."""
        point_config_at(self, self.child_plane)
        update.maybe_spawn()
        self.assertEqual(len(self.spawned), 1)


class TheRenderPathStillAsks(PersonaIso):
    """`charter statusline --watch` and opencode's `/charter` reach the check through
    `render` -> `_with_brand` -> `_brand`, and #938's table counts both as surfaces that
    still refresh. Adding two triggers is not a reason to lose the first."""

    def test_the_brand_asks_for_the_newer_charter_check(self):
        pin_update_channel(self)
        calls = []
        with mock.patch.object(update, "maybe_spawn", lambda: calls.append("asked")):
            statusline._brand()
        self.assertEqual(calls, ["asked"])


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
