"""What `version` and `doctor` may claim about a pin they cannot enforce.

A pin lives in `charter.toml`, per control plane. The binary is ONE machine-global install
— `uv tool install charter-cp` puts a single `charter` on PATH — so two planes pinning
different versions cannot both be satisfied, and `version sync` does not resolve that, it
picks a winner and puts the other plane into drift. Reported from the field after exactly
that: a plane pinning 0.27.0 went from "in sync" to "drift" without a command being run in
it, because work in another plane upgraded the shared install.

The decision (see the parked issue) is that a pin is **advice**, not a promise charter
enforces — building a version-resolving shim is a version manager, and charter is a control
plane. What has to change is what charter *says*: drift is not this plane's private problem,
and the tooling should stop implying it is.

Separately, `latest` is a ≤24h cached PyPI reading (`update.REFRESH_TTL`). When the
installed version is NEWER than that cache, the cache is provably stale, and printing the
lower number as `latest` invites the reader to distrust every other line — which is how the
field report ended up with `installed 0.27.2 / latest 0.26.0`.

ADR 0013: do not report as checked what was not checked.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from unittest import mock

from charter import doctor, update
from tests._isolation import PersonaIso


class TestLatestIsNotPresentedAsFactWhenStale(PersonaIso):
    def _cache(self, latest, ts=1.0):
        f = update._cache_file()
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(f'{{"latest": "{latest}", "ts": {ts}}}')

    def test_no_cache_says_so(self):
        self.assertIn("not checked", update.latest_display("0.27.2"))

    def test_a_cache_ahead_of_you_shows_the_version(self):
        self._cache("0.28.0")
        self.assertIn("0.28.0", update.latest_display("0.27.2"))

    def test_a_cache_equal_to_you_shows_the_version(self):
        self._cache("0.27.2")
        self.assertIn("0.27.2", update.latest_display("0.27.2"))

    def test_a_cache_behind_you_is_not_offered_as_the_latest(self):
        """`installed 0.27.2 / latest 0.26.0` is a self-contradiction on the same screen.
        The reading is not wrong, it is OLD, and it must say which."""
        self._cache("0.26.0")
        shown = update.latest_display("0.27.2")
        self.assertNotEqual(shown.strip(), "0.26.0")
        self.assertIn("stale", shown.lower())

    def test_it_never_implies_you_are_behind_when_you_are_ahead(self):
        self._cache("0.26.0")
        self.assertIsNone(update.newer_than("0.27.2"))


class TestThePinIsDescribedAsAdvice(PersonaIso):
    def test_the_drift_hint_says_the_install_is_machine_wide(self):
        """Otherwise drift reads as this plane's private problem, and the fix reads as
        `version sync` — which just moves the drift to whichever plane you left."""
        with mock.patch("charter.instance.locked_version", return_value="0.27.0"), \
             mock.patch("charter.__version__", "0.27.2"):
            r = doctor.check_version_lock()
        self.assertEqual(r.status, doctor.WARN)
        text = (r.detail or "") + " " + (r.hint or "")
        self.assertIn(update.SHARED_INSTALL_NOTE, text)

    def test_a_plane_in_sync_says_nothing_about_it(self):
        """The note is for the moment it matters. Everywhere else it is furniture."""
        with mock.patch("charter.instance.locked_version", return_value="0.27.2"), \
             mock.patch("charter.__version__", "0.27.2"):
            r = doctor.check_version_lock()
        self.assertEqual(r.status, doctor.OK)
        self.assertNotIn(update.SHARED_INSTALL_NOTE, (r.detail or "") + (r.hint or ""))

    def test_the_note_names_how_to_see_the_truth(self):
        self.assertIn("uv tool list", update.SHARED_INSTALL_NOTE)

    def test_the_note_no_longer_calls_the_pin_advisory(self):
        """It used to, and that was right while the machine-global binary was the only
        thing a pin could be measured against. A plugin is installed per project out of a
        cache holding every version at once, so a pin IS honourable now — `claude plugin
        update charter@charter` moves this plane and no other (#127). Calling it advisory
        would send the reader back to the shared binary, which is the trap."""
        low = update.SHARED_INSTALL_NOTE.lower()
        self.assertNotIn("advisory", low)

    def test_the_note_points_at_the_per_plane_mechanism(self):
        """What must survive: the note explains that this binary is not the plane's, and
        where the plane's version actually lives."""
        self.assertIn("plugin", update.SHARED_INSTALL_NOTE.lower())


class TestVersionSyncSaysWhatItMutates(PersonaIso):
    def test_sync_warns_before_moving_a_shared_install(self):
        """`version sync` conforms the ONE global binary, so it can put every other plane
        on this machine into drift. That has to be said at the moment it is run."""
        from charter import commands

        with mock.patch("charter.instance.locked_version", return_value="0.27.0"), \
             mock.patch("charter.commands._installed_version", return_value="0.27.2"), \
             mock.patch("charter.commands.sync_to", return_value=(False, "not really")):
            out, err = io.StringIO(), io.StringIO()
            # charter's util writes progress to stderr; capture both so the assertion
            # does not depend on which stream a given line uses.
            with redirect_stdout(out), redirect_stderr(err):
                commands.cmd_version_sync(mock.Mock(yes=True))
            printed = out.getvalue() + err.getvalue()
        self.assertIn("machine", printed.lower())
        self.assertIn(update.SHARED_INSTALL_NOTE, printed)


if __name__ == "__main__":
    unittest.main()
