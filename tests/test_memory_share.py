"""How far a written memory travels, declared per control plane.

`local` (the default) keeps it on disk; `commit` records it in git but does not publish;
`push` sends it immediately — today's umbrella behaviour, and the one that is dangerous as
a default for a stranger whose control plane may be public."""
from __future__ import annotations

import unittest
from unittest import mock

from charter import instance


class TestShareOf(unittest.TestCase):
    def test_default_is_local_when_nothing_is_declared(self):
        self.assertEqual(instance.share_of({}), "local")

    def test_each_declared_mode_is_read(self):
        for mode in instance.SHARE_MODES:
            self.assertEqual(instance.share_of({"memory": {"share": mode}}), mode)

    def test_an_unknown_mode_falls_back_to_local_not_to_push(self):
        """A typo must fail SAFE. Falling back to `push` would publish on a misspelling."""
        self.assertEqual(instance.share_of({"memory": {"share": "puhs"}}), "local")

    def test_modes_are_exactly_these_three(self):
        self.assertEqual(instance.SHARE_MODES, ("local", "commit", "push"))


class TestReactiveCommitHonoursPosture(unittest.TestCase):
    """The posture gates the reactive path — the one that commits and pushes the moment a
    memory is written."""

    def _record(self, share):
        from charter import commands
        # Patched on `planegit`, which is where both the reactive recorder and the one
        # committer now live. `commands` re-exports them, but a re-export is a NAME —
        # patching it cannot intercept a call made inside the defining module.
        from charter import planegit
        with mock.patch.object(planegit.config, "MEMORY_SHARE", share), \
             mock.patch.object(planegit, "commit_push") as cp:
            commands.commit_memory_reactive(["personas/p/memory/m.md"], "t")
        return cp

    def test_local_does_not_commit_at_all(self):
        self._record("local").assert_not_called()

    def test_commit_records_but_does_not_push(self):
        cp = self._record("commit")
        cp.assert_called_once()
        self.assertTrue(cp.call_args.kwargs.get("no_push"),
                        "`commit` must not publish")

    def test_push_publishes_immediately(self):
        cp = self._record("push")
        cp.assert_called_once()
        self.assertFalse(cp.call_args.kwargs.get("no_push"))

    def test_unrecognised_value_behaves_like_local(self):
        """`config.MEMORY_SHARE` is always pre-clamped through `instance.share_of` — but
        this reactive path must not itself depend on that. A value outside the three known
        modes fell through the local/commit branches into the final `return` (today's
        `push` path, committed with `background=True`) — the exact fail-unsafe shape this
        pins shut: no commit attempted at all, same as `local`."""
        self._record("puhs").assert_not_called()


if __name__ == "__main__":
    unittest.main()
