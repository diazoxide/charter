"""#947: on a dev plane, charter still routed you to a pin its own session start rejects.

A plane that declares ``[update] channel = "dev"`` AND pins ``[charter] version`` is refused
at session start: *"Those ask for two different charters, so nothing was installed"*
(`hooks.py`). #941 took `version bump --push` out of `charter version` on dev for that
reason. Two commands were left sending people there:

* **`version bump` wrote the pin without a word.** No channel test anywhere in it: it
  installed the target, wrote `[charter] version`, and with `--push` committed it, so every
  teammate on the shared dev plane met the refusal about a `charter.toml` they never edited.
  It now refuses before anything moves, in session start's own terms, and names both ways
  out. `--to` does not get past it: a pin typed by hand is still a pin beside the channel.
* **`version sync` on a pin-less dev plane said "Pin one with: charter version bump
  --push"**, which is the command above. It now says what a dev plane follows instead, and
  names the same next step `charter version` names on this channel.

Nothing here reaches the network or installs anything: `NoNetwork` refuses every route out,
and `fetch_and_store`, `sync_to`, `set_locked_version` and `commit_push` are recorders.
"""

from __future__ import annotations

import io
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands, update
from tests._isolation import PersonaIso, pin_update_channel
from tests.test_dev_channel import NoNetwork


def _run(fn, args) -> tuple[int, str]:
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = fn(args)
    return rc, out.getvalue() + err.getvalue()


class VersionBumpOnADevPlaneRefuses(NoNetwork, PersonaIso):
    def setUp(self):
        super().setUp()
        self.calls: list[tuple] = []

        # Asking PyPI counts as moving something: it writes this plane's update cache.
        def fetch_and_store():
            self.calls.append(("fetch_and_store",))
            return "99.0.0"

        def sync_to(v):
            self.calls.append(("sync_to", v))
            return True, v

        def set_locked_version(root, v):
            self.calls.append(("set_locked_version", v))
            return True

        def commit_push(root, add, message):
            self.calls.append(("commit_push", message))
            return 0

        self.enterContext(mock.patch("charter.update.fetch_and_store",
                                     side_effect=fetch_and_store))
        self.enterContext(mock.patch("charter.commands.sync_to", side_effect=sync_to))
        self.enterContext(mock.patch("charter.instance.set_locked_version",
                                     side_effect=set_locked_version))
        self.enterContext(mock.patch("charter.commands.commit_push", side_effect=commit_push))

    def _bump(self, **kw):
        return _run(commands.cmd_version_bump,
                    SimpleNamespace(**{"to": None, "push": True, **kw}))

    def assertRefusedBeforeAnythingMoved(self, rc: int, said: str) -> None:
        self.assertEqual(rc, 1, said)
        self.assertEqual(self.calls, [], "bump moved something on a plane it refuses to pin")
        # Session start's own words for the conflict, so the two cannot describe one plane
        # two ways.
        self.assertIn('`[update] channel = "dev"`', said)
        self.assertIn("two different charters", said)
        # Both ways out: drop the channel, or do not pin and follow `main` instead.
        self.assertIn("drop", said)
        self.assertIn(update.dev_remedy(), said)
        self.assertNotIn("pinned this control plane", said)

    def test_with_no_target_it_refuses_before_asking_pypi(self):
        pin_update_channel(self, "dev")
        rc, said = self._bump()
        self.assertRefusedBeforeAnythingMoved(rc, said)

    def test_an_explicit_target_is_refused_the_same_way(self):
        """`--to` chooses WHICH release to pin, not whether a pin can sit beside the dev
        channel. The way back to a release is dropping the channel, and the refusal names
        it."""
        pin_update_channel(self, "dev")
        rc, said = self._bump(to="99.0.0")
        self.assertRefusedBeforeAnythingMoved(rc, said)

    def test_a_charter_run_out_of_its_own_tree_is_sent_to_git_to_stay_on_main(self):
        """The same next step `charter version` names on dev, from the same function:
        `charter update` refuses to install over the tree it is running from."""
        pin_update_channel(self, "dev")
        with mock.patch("charter.channel.running_inside", return_value=True):
            rc, said = self._bump()
            self.assertRefusedBeforeAnythingMoved(rc, said)
        self.assertIn("pull", said)

    def test_a_stable_plane_still_pins(self):
        """The other side of the gate: nothing about bump moves where the channel is not
        dev, or every plane loses the only command that writes a pin."""
        pin_update_channel(self, "stable")
        rc, said = self._bump()
        self.assertEqual(rc, 0, said)
        self.assertEqual(self.calls, [("fetch_and_store",), ("sync_to", "99.0.0"),
                                      ("set_locked_version", "99.0.0"),
                                      ("commit_push", "charter: pin to 99.0.0")])


class VersionSyncOnAPinlessDevPlaneNamesWhatItFollows(NoNetwork, PersonaIso):
    """No pin is the only state a dev plane can be in without session start refusing it, so
    "nothing to sync" is right. Offering the command that ends that state was not."""

    def _sync(self) -> tuple[int, str]:
        with mock.patch("charter.commands.sync_to") as installed:
            rc, said = _run(commands.cmd_version_sync, SimpleNamespace(cli=False))
        installed.assert_not_called()
        return rc, said

    def test_it_does_not_recommend_a_pin(self):
        pin_update_channel(self, "dev")
        rc, said = self._sync()
        self.assertEqual(rc, 0, said)
        self.assertIn("pins no version", said)
        self.assertNotIn("version bump", said)
        # What a dev plane follows, and the next step `charter version` names for it.
        self.assertIn(f"`{update.DEV_BRANCH}`", said)
        self.assertIn(update.dev_remedy(), said)

    def test_a_stable_plane_is_still_offered_the_pin(self):
        """Where a pin is a state session start accepts, offering one is still the useful
        next step, and the dev wording must not reach it."""
        pin_update_channel(self, "stable")
        rc, said = self._sync()
        self.assertEqual(rc, 0, said)
        self.assertIn("charter version bump --push", said)
        self.assertNotIn(update.dev_remedy(), said)
