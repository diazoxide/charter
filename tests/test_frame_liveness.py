"""The frame updates because the agent did something.

Every `posttooluse*` handler runs on some tool call, and `hooks.py` already treats that
family as hot, so the bump is debounced and swallows every error: a hook may cost a
session its briefing, never its turn.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import hooks
from charter.frame import notify, state

from tests._isolation import PersonaIso, run_hook


class Notify(PersonaIso, unittest.TestCase):
    def test_a_change_bumps_the_running_frame(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}):
            before = state.version("f-1")
            notify._last["at"] = 0.0
            notify.plane_changed()
            self.assertNotEqual(before, state.version("f-1"))

    def test_a_second_bump_inside_the_debounce_window_is_skipped(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}):
            notify._last["at"] = 0.0
            notify.plane_changed()
            first = state.version("f-1")
            notify.plane_changed()
            self.assertEqual(first, state.version("f-1"))

    def test_outside_a_frame_it_does_nothing_at_all(self):
        """Not just "does not raise" — `state.bump` is itself hardened against a bad id
        (`contain.segment_ok` rejects a non-str/`None` name harmlessly), so a version of
        this guard that forwarded a missing id to `state.bump` anyway would still not
        raise. Spying on `state.bump` catches that the guard-removed version wouldn't:
        it proves nothing was even attempted, not merely that nothing blew up.

        `_last["at"]` is reset here too, same as the other cases — otherwise the
        debounce window left over from a test that ran moments earlier (`_last["at"]`
        being very recent) would return early and mask a missing/broken `fid` guard
        just as effectively as a correct one, and this test would pass either way."""
        with mock.patch.dict(os.environ, {}, clear=True), \
             mock.patch.object(state, "bump") as bump:
            notify._last["at"] = 0.0
            notify.plane_changed()   # must not raise
            bump.assert_not_called()

    def test_a_broken_state_directory_never_reaches_the_hook(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}), \
             mock.patch.object(state, "bump", side_effect=OSError("read-only")):
            notify._last["at"] = 0.0
            notify.plane_changed()   # must not raise


class EveryPostToolUseHandlerBumps(PersonaIso, unittest.TestCase):
    """`hooks/hooks.json` scopes the bare-named `posttooluse` handler to
    `Write|Edit|MultiEdit` alone — Bash, Skill, Task/Agent and SendMessage each route to
    their OWN `posttooluse-*` handler (`posttooluse-bash`, `-skill`, `-dispatch`,
    `-message`). Wiring `plane_changed()` into `posttooluse` only would leave the frame
    blind to Bash specifically, which is where most of the plane-state changes a panel
    cares about actually happen — commits, branch moves, worktree edits, none of them a
    Write/Edit/MultiEdit call.

    Enumerates `hooks._HANDLERS` rather than hardcoding today's five names, so a future
    SIXTH `posttooluse-*` handler that forgets the call fails HERE, the same way it
    would have caught the gap this class exists to close."""

    def test_every_posttooluse_handler_calls_plane_changed(self):
        names = [n for n in hooks._HANDLERS if n.startswith("posttooluse")]
        # A floor, not a fixed count — the whole point is that a sixth name added later
        # is picked up automatically, not that exactly five exist today.
        self.assertGreaterEqual(len(names), 5,
                                "hooks.json's own five posttooluse* matchers "
                                "(posttooluse, -bash, -skill, -dispatch, -message) "
                                "should all be registered in _HANDLERS")
        for name in names:
            with self.subTest(handler=name), \
                 mock.patch.object(notify, "plane_changed") as bumped:
                run_hook(hooks._HANDLERS[name], {})
                bumped.assert_called_once()


if __name__ == "__main__":
    unittest.main()
