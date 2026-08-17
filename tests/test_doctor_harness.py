"""`doctor` reports which harness it is in, and every ceiling that harness has.

This is the honesty half of guarantee parity (ADR 0015). Charter enforces the same
invariants on both harnesses; what differs is what it can *offer*, and an offer charter
silently cannot make reads as a broken install — the failure `check_guard_wired` was
written for, one level up.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import doctor, harness


class DoctorHarness(unittest.TestCase):
    def test_the_opencode_row_names_every_ceiling(self):
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "opencode"}, clear=True):
            r = doctor.check_harness()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("opencode", r.detail)
        for d in harness.deficits(harness.OPENCODE):
            with self.subTest(key=d.key):
                self.assertIn(d.key, r.detail)

    def test_claude_code_reports_no_ceilings(self):
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}, clear=True):
            r = doctor.check_harness()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("claude-code", r.detail)
        self.assertNotIn("ceiling", r.detail)

    def test_an_unregistered_harness_is_not_reported_as_complete(self):
        """`deficits()` returns nothing for a harness charter has never met — which is
        charter having no knowledge, not the harness having no gaps. Rendering the two
        the same way prints a clean row over an unverified integration, which is exactly
        `check_guard_wired`'s failure: the absence of information shown as health."""
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "codex"}, clear=True):
            r = doctor.check_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("codex", r.detail)
        self.assertTrue(r.hint, "an unknown harness must say what to do about it")

    def test_no_harness_is_reported_without_nagging(self):
        """`charter doctor` from a plain terminal is ordinary. A row that warns every time
        it is run correctly teaches people to ignore the column — the failure named in
        `check_guard_wired`'s docstring."""
        with mock.patch.dict(os.environ, {}, clear=True):
            r = doctor.check_harness()
        self.assertEqual(r.status, doctor.OK)


if __name__ == "__main__":
    unittest.main()
