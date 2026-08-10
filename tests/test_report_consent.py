"""Consent to publish — asked once, remembered per **human**.

Consenting to file issues under your own GitHub identity is a property of the person, not
of a directory (docs/adr/0003). So it cannot live in STATE_DIR, which is per control plane
— a Reporter running several planes would be asked repeatedly, which trains them to click
through it — and emphatically not in charter.toml, which is committed and would enrol a
whole team on one person's say-so.
"""
from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_report, config, report
from tests._isolation import PersonaIso


class ConsentIso(PersonaIso):
    """PersonaIso isolates the plane; this also isolates the per-human config home, so a
    test never reads or writes the developer's real consent."""

    def setUp(self) -> None:
        super().setUp()
        self.home = Path(tempfile.mkdtemp(prefix="charter-consent-"))
        # CHARTER_CONFIG_HOME, deliberately NOT XDG_CONFIG_HOME: `gh` keeps its own auth
        # under XDG_CONFIG_HOME, so redirecting that to isolate consent logs `gh` out and
        # silently pushes `send` down the no-`gh` fallback path. Found the hard way.
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_CONFIG_HOME": str(self.home)}))
        # No test may depend on whether the developer running it happens to be logged in
        # to `gh` — that is the flakiness tests/test_forge_github.py exists to avoid.
        self.enterContext(mock.patch("charter.report.gh_available", return_value=True))


class TestConsentIsPerHuman(ConsentIso):
    def test_consent_is_absent_by_default(self):
        """Installing charter must never imply agreeing to publish anything."""
        self.assertFalse(report.has_consent())

    def test_granting_consent_persists_it(self):
        report.grant_consent()
        self.assertTrue(report.has_consent())

    def test_consent_is_not_stored_under_the_plane(self):
        """The load-bearing one: stored per-plane, a Reporter with several planes gets
        asked again and again, and the safeguard degrades into a reflex."""
        report.grant_consent()
        self.assertFalse(report.consent_path().is_relative_to(config.STATE_DIR))
        self.assertTrue(report.consent_path().is_relative_to(self.home))

    def test_consent_survives_a_different_control_plane(self):
        """Same human, different plane — already consented, so not asked again."""
        report.grant_consent()
        other = Path(tempfile.mkdtemp(prefix="other-plane-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(other, ignore_errors=True))
        orig = config.use(other)
        try:
            self.assertTrue(report.has_consent())
        finally:
            config.restore(orig)


class TestSendRefusesWithoutConsent(ConsentIso):
    def _draft(self):
        commands_report.cmd_report_gap(SimpleNamespace(text="no archive command"))
        return report.pending()[0]["id"]

    def test_send_without_consent_does_not_publish(self):
        rid = self._draft()
        with mock.patch("charter.report.gh") as gh:
            rc = commands_report.cmd_report_send(SimpleNamespace(id=rid, new=False, dry_run=False))
        self.assertNotEqual(rc, 0)
        gh.assert_not_called()
        self.assertIsNone(report.load(rid)["issue_url"])

    def test_send_after_consent_reaches_the_forge(self):
        report.grant_consent()
        rid = self._draft()
        with mock.patch("charter.report.gh") as gh:
            gh.return_value = "https://github.com/diazoxide/charter/issues/7"
            with mock.patch("charter.report.search_duplicates", return_value=[]):
                rc = commands_report.cmd_report_send(
                    SimpleNamespace(id=rid, new=False, dry_run=False))
        self.assertEqual(rc, 0)
        self.assertTrue(gh.called)


class TestGrantingConsentIsItsOwnCommand(ConsentIso):
    def test_the_consent_command_records_it(self):
        """A separate command rather than a flag on `send`. ADR 0003 rejects flags that
        collapse approval into the sending call — a flag an agent can pass is a flag it
        will pass every time, where a one-off command has a single, auditable purpose."""
        rc = commands_report.cmd_report_consent(SimpleNamespace())
        self.assertEqual(rc, 0)
        self.assertTrue(report.has_consent())


if __name__ == "__main__":
    unittest.main()
