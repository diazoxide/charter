"""The frame updates because the agent did something.

Every `posttooluse*` handler runs on some tool call, and `hooks.py` already treats that
family as hot, so the bump is debounced and swallows every error: a hook may cost a
session its briefing, never its turn.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import hooks, workspace
from charter.frame import gather, notify, state

from tests._isolation import PersonaIso, PlaneIso, run_hook


class Notify(PlaneIso, unittest.TestCase):
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
        raise. Spying on `state.bump`/`gather.refresh` catches that the guard-removed
        version wouldn't: it proves nothing was even attempted, not merely that nothing
        blew up — the property this task's brief calls "must cost nothing outside a
        frame," checked before any gather work, not just before the bump.

        `_last["at"]` is reset here too, same as the other cases — otherwise the
        debounce window left over from a test that ran moments earlier (`_last["at"]`
        being very recent) would return early and mask a missing/broken `fid` guard
        just as effectively as a correct one, and this test would pass either way."""
        with mock.patch.dict(os.environ, {}, clear=True), \
             mock.patch.object(state, "bump") as bump, \
             mock.patch.object(gather, "refresh") as refresh:
            notify._last["at"] = 0.0
            notify.plane_changed()   # must not raise
            bump.assert_not_called()
            refresh.assert_not_called()

    def test_a_broken_state_directory_never_reaches_the_hook(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}), \
             mock.patch.object(state, "bump", side_effect=OSError("read-only")):
            notify._last["at"] = 0.0
            notify.plane_changed()   # must not raise

    def test_a_bump_refreshes_the_gather_cache(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}), \
             mock.patch.object(gather, "refresh") as refresh:
            notify._last["at"] = 0.0
            notify.plane_changed()
            refresh.assert_called_once()
            self.assertEqual(refresh.call_args.args, ("f-1",))

    def test_the_refresh_is_keyed_to_the_frames_workspace_not_this_hooks(self):
        """#512. The cache belongs to the FRAME and a panel draws it whole, so a refresh
        keyed to another workspace does not degrade the table — it replaces it. This runs
        inside the harness, which resolves for the SESSION; the launcher resolved for the
        frame and wrote that answer down. Without this, a launch gathers the workspace you
        launched for and the very first tool call swaps in another one's repos."""
        state.record_workspace("f-1", "the-frames-own")
        # Cleared rather than merely overwritten: rung 0 of `workspace_for` reads
        # `$CHARTER_WORKSPACE`, so a developer's own ambient pin would otherwise survive
        # into this environ and answer instead of the recorded name (#519, #521).
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}, clear=True), \
             mock.patch.object(gather, "refresh") as refresh:
            notify._last["at"] = 0.0
            notify.plane_changed()
            refresh.assert_called_once_with("f-1", workspace="the-frames-own")

    def test_a_frame_with_no_recorded_workspace_refreshes_what_it_always_did(self):
        """The migration case. `state.workspace_for`'s last rung is a local `resolve()`,
        so a frame launched by a charter that predates the record gathers exactly the
        workspace this hook would have gathered on its own — never worse than before
        #512, and never a blank."""
        self.assertIsNone(state.frame_workspace("f-1"))
        # Cleared rather than merely overwritten (#519, #521): otherwise an ambient
        # `$CHARTER_WORKSPACE` would answer `workspace.resolve()` too, and both sides of
        # the assertion below would collapse to the pinned name whether or not the
        # local-resolve fallback this test exists to pin actually ran.
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}, clear=True), \
             mock.patch.object(gather, "refresh") as refresh:
            notify._last["at"] = 0.0
            notify.plane_changed()
            refresh.assert_called_once_with("f-1", workspace=workspace.resolve())

    def test_a_second_call_inside_the_debounce_window_skips_the_refresh_too(self):
        """The cache refresh rides the SAME debounce as the version bump (see the
        module docstring on why one gate rather than two) — a second call within the
        250ms window must not gather again, exactly like it must not bump again."""
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}), \
             mock.patch.object(gather, "refresh") as refresh:
            notify._last["at"] = 0.0
            notify.plane_changed()
            notify.plane_changed()
            refresh.assert_called_once()

    def test_a_failure_to_gather_does_not_raise_and_the_bump_still_happens(self):
        """The property this task's brief puts first: a `gather.refresh` that somehow
        raises must not escape `plane_changed()` — proven here with a real
        `RuntimeError`, not by trusting `gather`'s own advertised politeness. And
        because the version bump is this function's original, load-bearing promise
        (pinned separately by `test_a_change_bumps_the_running_frame`), a broken
        refresh must not take the bump down with it: the version still has to move."""
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}), \
             mock.patch.object(gather, "refresh", side_effect=RuntimeError("boom")):
            before = state.version("f-1")
            notify._last["at"] = 0.0
            notify.plane_changed()   # must not raise
            self.assertNotEqual(before, state.version("f-1"))

    def test_the_cache_refreshes_before_the_version_bumps(self):
        """So a panel that polls `state.version` and then reads the cache never finds
        the version already moved but the cache still stale — see the module
        docstring's "refresh happens BEFORE the bump" note."""
        order = []
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}), \
             mock.patch.object(gather, "refresh",
                                side_effect=lambda fid, **kw: order.append("refresh")), \
             mock.patch.object(state, "bump",
                                side_effect=lambda fid: order.append("bump")):
            notify._last["at"] = 0.0
            notify.plane_changed()
        self.assertEqual(order, ["refresh", "bump"])


#: Every hook family the spec names as a liveness trigger, and what each one buys.
#:
#: `posttooluse*` alone is what this class used to enumerate, and the gap was real:
#: removing `notify.plane_changed()` from `hooks.sessionstart` or
#: `hooks.userpromptsubmit` left the whole suite green. The cost of the
#: `userpromptsubmit` one in particular is the "agent is thinking, no tool calls yet"
#: window — exactly when an operator looks at the panel and finds it stale, because the
#: next repaint waits for a tool call that has not happened yet. `sessionstart` is the
#: first paint of a frame's life: without it a panel opens showing whatever the
#: previous session left behind until something else moves.
_TRIGGERS = ("sessionstart", "userpromptsubmit", "posttooluse")


class EveryLivenessTriggerBumps(PlaneIso, unittest.TestCase):
    """`hooks/hooks.json` scopes the bare-named `posttooluse` handler to
    `Write|Edit|MultiEdit` alone — Bash, Skill, Task/Agent and SendMessage each route to
    their OWN `posttooluse-*` handler (`posttooluse-bash`, `-skill`, `-dispatch`,
    `-message`). Wiring `plane_changed()` into `posttooluse` only would leave the frame
    blind to Bash specifically, which is where most of the plane-state changes a panel
    cares about actually happen — commits, branch moves, worktree edits, none of them a
    Write/Edit/MultiEdit call.

    Enumerates `hooks._HANDLERS` against :data:`_TRIGGERS` rather than hardcoding
    today's names, so a future handler in any of the three families that forgets the
    call fails HERE, the same way it would have caught the gap this class exists to
    close."""

    def test_every_liveness_trigger_calls_plane_changed(self):
        names = [n for n in hooks._HANDLERS if n.startswith(_TRIGGERS)]
        # A floor, not a fixed count — the whole point is that a name added later is
        # picked up automatically, not that exactly seven exist today.
        self.assertGreaterEqual(len(names), 7,
                                "the three trigger families the spec names: "
                                "sessionstart, userpromptsubmit, and hooks.json's own "
                                "five posttooluse* matchers (posttooluse, -bash, "
                                "-skill, -dispatch, -message)")
        for family in _TRIGGERS:
            self.assertTrue(any(n.startswith(family) for n in names),
                            f"no handler at all for the {family!r} trigger family — "
                            "an enumeration that silently covers two of three is how "
                            "the gap this class closes got in")
        for name in names:
            with self.subTest(handler=name), \
                 mock.patch.object(notify, "plane_changed") as bumped:
                run_hook(hooks._HANDLERS[name], {})
                bumped.assert_called_once()


if __name__ == "__main__":
    unittest.main()
