"""#937: on the dev channel, `charter version` called an older release newer.

Measured on the 0.60.0 wheel, on a plane declaring ``[update] channel = "dev"``, with a
cache holding ``latest: 0.58.0`` and a ``head`` from inside v0.59.0. One screen said:

    • A newer charter is published (0.58.0).
    •   update, commit and push the lock:  charter version bump --push
      installed  0.60.0
      latest     — (cached 0.58.0 is stale: it predates the 0.60.0 you are running)

The condition asked the dev channel (`update.newer_than` hands off to `newer_head`, which
nudges any build that records no commit), and the message answered with the PyPI cache,
a number that condition had never compared with anything. Three things were wrong, and
each class below pins one of them:

* **The headline printed a value its condition did not test.** On dev the verdict now
  names what was compared: the cached head against this build's commit, or, where there is
  no commit, the fact that this build did not come from `main`. It never prints a PyPI
  number, and its remedy is `charter update`. That is the dev channel's command. `version
  bump --push` writes a pin that this plane's own session start calls contradictory.
* **`report send` claimed the same comparison.** ``9d18d55 is out — this may already be
  fixed`` was said to a build that `9d18d55` predates. The nudge is kept, because
  `newer_head`'s docstring explains why it is deliberate. Only what it claims changed.
* **`version bump` pinned a version it had not fetched.** It called `fetch_and_store` and
  then re-read the cache, which a failed GET leaves untouched. So the "offline?" refusal
  fired only on an empty cache, and a stale one was installed over the running build and
  pushed to the team.

ADR 0013: do not present as checked what was not checked.

Nothing here reaches the network or installs anything: `NoNetwork` refuses every route
out, the two GETs are stubbed where a test needs an answer, and `sync_to`,
`set_locked_version` and `commit_push` are recorders.
"""

from __future__ import annotations

import io
import json
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import __version__, commands, commands_report, update, util
from tests._isolation import PersonaIso, pin_update_channel
from tests.test_dev_channel import NoNetwork

#: Older than any charter that can be running this suite. The field report's cache held
#: 0.58.0 under a 0.60.0 wheel, and the exact numbers are not the point.
_STALE = "0.0.1"
#: The report's cached head, `9d18d55`, which is contained in v0.59.0. It is older than
#: the wheel it was offered to, and nothing in charter can know that from a wheel.
_HEAD = "9d18d55" + "0" * 33
_MINE = "a" * 40


def _cache(**record) -> None:
    p = update._cache_file()
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(record))


def _run(fn, args) -> tuple[int, str]:
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = fn(args)
    return rc, out.getvalue() + err.getvalue()


def _verdict(printed: str) -> str:
    """Everything `charter version` printed except its three table rows.

    The `latest` row legitimately carries the cached PyPI number, as "cached 0.0.1 is
    stale", and that row was right all along. Filtering it out is what lets "no PyPI number
    in the verdict" be asserted at all, rather than weakened to "no word 'published'".
    """
    rows = ("installed ", "locked ", "latest ")
    return "\n".join(l for l in printed.splitlines() if not l.lstrip().startswith(rows))


class VersionOnDevSaysWhatItCompared(NoNetwork, PersonaIso):
    def setUp(self):
        super().setUp()
        pin_update_channel(self, "dev")
        # Not vacuous: the fixture is only the report's shape if the cached release really
        # is older than the build running the test.
        self.assertLess(update._parse(_STALE), update._parse(__version__))

    def _version(self, *, commit):
        with mock.patch("charter.channel.installed_commit", return_value=commit):
            return _run(commands.cmd_version, SimpleNamespace())

    def test_an_older_cached_release_is_not_called_newer_on_a_build_with_no_commit(self):
        """The report, line for line: a wheel on a dev plane and a stale PyPI cache."""
        _cache(latest=_STALE, head=_HEAD, ts=1.0)
        rc, printed = self._version(commit=None)
        self.assertEqual(rc, 0)
        # The table read the same cache, so an absent number below is not a missing cache.
        self.assertIn(f"cached {_STALE} is stale", printed)
        verdict = _verdict(printed)
        self.assertNotIn("published", verdict)
        self.assertNotIn(_STALE, verdict, "the verdict printed a PyPI number it never compared")
        self.assertNotIn("version bump", verdict,
                         "the dev channel was told to pin a published release")
        # No commit means no comparison, so the head is not offered as newer either: here
        # it is OLDER than the wheel, and charter has no way to know that.
        self.assertNotIn(_HEAD[:7], verdict)
        self.assertIn(update.NOT_INSTALLED_FROM_MAIN, verdict)
        self.assertIn("charter update", verdict)

    def test_a_newer_cached_release_is_not_announced_on_dev_either(self):
        """The same defect when the number happens to be higher. The dev condition never
        compared it, so printing it is still printing something nobody checked."""
        _cache(latest="99.0.0", head=_HEAD, ts=1.0)
        _, printed = self._version(commit=None)
        verdict = _verdict(printed)
        self.assertNotIn("99.0.0", verdict)
        self.assertNotIn("published", verdict)

    def test_a_build_with_a_commit_names_both_commits_and_calls_neither_newer(self):
        """A commit makes a comparison possible, but only an unequal one. The cached head
        can be older than a build `charter update` installed after the cache was written,
        so the verdict says the two differ and not which one is ahead."""
        _cache(latest=_STALE, head=_HEAD, ts=1.0)
        rc, printed = self._version(commit=_MINE)
        self.assertEqual(rc, 0)
        verdict = _verdict(printed)
        self.assertIn(_HEAD[:7], verdict)
        self.assertIn(_MINE[:7], verdict)
        self.assertNotIn("newer", verdict.lower())
        self.assertNotIn(_STALE, verdict)
        self.assertNotIn("version bump", verdict)
        self.assertIn("charter update", verdict)

    def test_the_dev_verdict_is_not_gated_on_a_pypi_reading(self):
        """The old condition was `latest and newer_than(...)`, so a PyPI field decided
        whether a dev plane heard about `main` at all. With no release cached, a wheel on
        a dev plane was told it was up to date."""
        _cache(head=_HEAD, ts=1.0)
        _, printed = self._version(commit=None)
        verdict = _verdict(printed)
        self.assertNotIn("up to date", verdict)
        self.assertIn(update.NOT_INSTALLED_FROM_MAIN, verdict)

    def test_a_build_on_the_cached_head_is_up_to_date(self):
        _cache(latest=_STALE, head=_MINE, ts=1.0)
        rc, printed = self._version(commit=_MINE)
        self.assertEqual(rc, 0)
        verdict = _verdict(printed)
        self.assertIn("up to date", verdict)
        self.assertNotIn("charter update", verdict)


class VersionOnStableIsUnchanged(NoNetwork, PersonaIso):
    """The stable channel's verdict was correct, and it must survive the dev fix as it was."""

    def setUp(self):
        super().setUp()
        pin_update_channel(self, "stable")

    def test_a_newer_published_release_is_named_with_the_bump(self):
        _cache(latest="99.0.0", head=_HEAD, ts=1.0)
        rc, printed = _run(commands.cmd_version, SimpleNamespace())
        self.assertEqual(rc, 0)
        verdict = _verdict(printed)
        self.assertIn("A newer charter is published (99.0.0).", verdict)
        self.assertIn("charter version bump --push", verdict)
        self.assertNotIn(update.NOT_INSTALLED_FROM_MAIN, verdict)

    def test_an_older_cached_release_is_not_called_newer(self):
        _cache(latest=_STALE, ts=1.0)
        _, printed = _run(commands.cmd_version, SimpleNamespace())
        verdict = _verdict(printed)
        self.assertNotIn("published", verdict)
        self.assertIn("up to date", verdict)


class TheReportNudgeOnDevClaimsNoComparison(NoNetwork, PersonaIso):
    def setUp(self):
        super().setUp()
        pin_update_channel(self, "dev")

    def _nudge(self, *, commit, head=_HEAD) -> str:
        _cache(latest=_STALE, head=head, ts=1.0)
        buf = io.StringIO()
        with mock.patch("charter.channel.installed_commit", return_value=commit), \
             mock.patch.object(util, "_USE_COLOR", False), redirect_stderr(buf):
            commands_report._warn_if_stale()
        return buf.getvalue()

    def test_a_build_with_no_commit_is_not_told_a_fix_may_already_exist(self):
        """`9d18d55 is out — this may already be fixed`, said to a wheel `9d18d55` predates."""
        said = self._nudge(commit=None)
        # The nudge is deliberate (`newer_head`'s docstring), so it is still there...
        self.assertIn(f"charter {__version__} dev", said)
        # ...and it no longer claims a comparison nobody made.
        self.assertNotIn("is out", said)
        self.assertNotIn("may already be fixed", said)
        self.assertNotIn(_HEAD[:7], said)
        self.assertIn(update.NOT_INSTALLED_FROM_MAIN, said)
        self.assertIn("charter update", said)

    def test_a_build_with_a_commit_keeps_the_nudge_it_had(self):
        """#937 changes what the no-commit case claims and nothing else."""
        said = self._nudge(commit=_MINE)
        self.assertIn(f"{_HEAD[:7]} is out", said)
        self.assertNotIn(update.NOT_INSTALLED_FROM_MAIN, said)

    def test_a_build_on_the_cached_head_is_not_nudged(self):
        self.assertEqual(self._nudge(commit=_MINE, head=_MINE), "")


class TheReportNudgeOnStableIsUnchanged(NoNetwork, PersonaIso):
    """A stable plane on the PyPI wheel also records no commit. So the no-commit sentence
    has to be gated on the channel, or a real newer release stops being named. Every
    dev-pinned test above passes with that gate deleted."""

    def setUp(self):
        super().setUp()
        pin_update_channel(self, "stable")

    def test_a_wheel_on_stable_is_still_told_the_release_is_out(self):
        _cache(latest="99.0.0", ts=1.0)
        buf = io.StringIO()
        with mock.patch("charter.channel.installed_commit", return_value=None), \
             mock.patch.object(util, "_USE_COLOR", False), redirect_stderr(buf):
            commands_report._warn_if_stale()
        said = buf.getvalue()
        self.assertIn("99.0.0 is out — this may already be fixed", said)
        self.assertNotIn(update.NOT_INSTALLED_FROM_MAIN, said)


class VersionBumpPinsOnlyWhatItFetched(NoNetwork, PersonaIso):
    """`version bump` with no `--to` pins the version its own GET returned, or refuses."""

    def setUp(self):
        super().setUp()
        self.calls: list[tuple] = []

        def sync_to(v):
            self.calls.append(("sync_to", v))
            return True, v

        def set_locked_version(root, v):
            self.calls.append(("set_locked_version", v))
            return True

        def commit_push(root, add, message):
            self.calls.append(("commit_push", message))
            return 0

        self.enterContext(mock.patch("charter.commands.sync_to", side_effect=sync_to))
        self.enterContext(mock.patch("charter.instance.set_locked_version",
                                     side_effect=set_locked_version))
        self.enterContext(mock.patch("charter.commands.commit_push", side_effect=commit_push))

    def _bump(self):
        return _run(commands.cmd_version_bump, SimpleNamespace(to=None, push=True))

    def test_a_failed_pypi_fetch_over_a_stale_cache_refuses(self):
        """The downgrade. `fetch_and_store` leaves `latest` alone when its GET fails, and
        the re-read found 0.58.0 there, installed it over 0.60.0 and pushed the pin."""
        pin_update_channel(self, "stable")
        _cache(latest=_STALE, ts=1.0)
        with mock.patch.object(update, "_fetch_latest", return_value=None):
            rc, printed = self._bump()
        self.assertEqual(rc, 1)
        self.assertEqual(self.calls, [], "bump acted on a version it did not fetch")
        self.assertIn("offline", printed)

    def test_a_dev_plane_whose_head_fetch_succeeds_refuses_the_same_way(self):
        """The measured shape: on dev the branch GET can succeed while PyPI's fails, and a
        partial fetch still writes the cache. The head is not a version, and the stale
        `latest` beside it is not what this call fetched."""
        pin_update_channel(self, "dev")
        _cache(latest=_STALE, head=_HEAD, ts=1.0)
        with mock.patch.object(update, "_fetch_latest", return_value=None), \
             mock.patch.object(update, "_fetch_head", return_value="f" * 40):
            rc, _ = self._bump()
        self.assertEqual(rc, 1)
        self.assertEqual(self.calls, [])

    def test_a_successful_fetch_pins_what_it_fetched(self):
        pin_update_channel(self, "stable")
        _cache(latest=_STALE, ts=1.0)
        with mock.patch.object(update, "_fetch_latest", return_value="99.0.0"):
            rc, _ = self._bump()
        self.assertEqual(rc, 0)
        self.assertEqual(self.calls, [("sync_to", "99.0.0"),
                                      ("set_locked_version", "99.0.0"),
                                      ("commit_push", "charter: pin to 99.0.0")])

    def test_a_padded_answer_is_trimmed_before_it_becomes_a_pin(self):
        """The fetched value is trimmed the way `--to` is, and the cache read it replaced
        was. It ends up on the right-hand side of `charter-cp==` and in a committed
        `charter.toml`, and a trailing newline there is a pin nothing can install."""
        pin_update_channel(self, "stable")
        with mock.patch.object(update, "_fetch_latest", return_value=" 99.0.0\n"):
            rc, _ = self._bump()
        self.assertEqual(rc, 0)
        self.assertEqual([v for _, v in self.calls[:2]], ["99.0.0", "99.0.0"])


if __name__ == "__main__":
    import unittest
    unittest.main()
