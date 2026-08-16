"""Discarding a drafted report (#155).

`charter report` had {bug,gap,list,show,consent,send,comment} and no way to throw a draft
away. A report drafted with a wrong version stamp, redrafted, and sent under the new id left
the first one in `report list` permanently:

    b66a79e9dac3964a  bug  `vault add --share --force` half-applies …
        not sent
    c8e8f6e2723e91b4  bug  `vault add --share --force` half-applies …
        https://github.com/diazoxide/charter/issues/128

The cost is not tidiness. `not sent` is a TODO, and its value is that it is actionable —
once the list accumulates drafts nobody will ever send, the reader stops trusting the state
and a real pending report gets missed among the dead ones. That is the failure mode of a
suite full of permanently-skipped tests.

Drafting is deliberately cheap and local, which is right; that only works if undoing it is
cheap too.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import report
from charter import commands_report as cr
from tests._isolation import PersonaIso


class DeleteCase(PersonaIso):
    def draft(self, text="charter cannot archive a workspace") -> str:
        return report.record_gap(text)

    def delete(self, rid, force=False):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = cr.cmd_report_delete(SimpleNamespace(id=rid, force=force))
        return rc, out.getvalue() + err.getvalue()

    def listing(self):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            cr.cmd_report_list(SimpleNamespace())
        return out.getvalue() + err.getvalue()


class TestADraftCanBeDiscarded(DeleteCase):
    def test_delete_removes_it(self):
        rid = self.draft()
        rc, _ = self.delete(rid)
        self.assertEqual(rc, 0)
        self.assertIsNone(report.load(rid))

    def test_it_leaves_the_listing(self):
        """`report list` is the surface for "what have I got pending", and the whole issue
        is that it grew a permanent false positive."""
        rid = self.draft("something I will redraft")
        self.assertIn(rid, self.listing())
        self.delete(rid)
        self.assertNotIn(rid, self.listing())

    def test_other_drafts_are_untouched(self):
        keep = self.draft("the one I still mean to send")
        drop = self.draft("the superseded one")
        self.delete(drop)
        self.assertIsNotNone(report.load(keep))

    def test_an_unknown_id_is_an_error_that_says_how_to_look(self):
        rc, out = self.delete("nosuchid")
        self.assertEqual(rc, 1)
        self.assertIn("report list", out)


class TestASentReportIsARecord(DeleteCase):
    """A sent report is kept forever on purpose — `prune`'s docstring says why: it is what
    lets a later identical crash point at the existing upstream issue instead of drafting a
    duplicate. Deleting one silently would remove that pointer AND the only local trace of
    something that exists publicly under the Reporter's identity."""

    def sent(self) -> str:
        rid = self.draft("already filed upstream")
        report.mark_sent(rid, "https://github.com/diazoxide/charter/issues/1")
        return rid

    def test_deleting_a_sent_report_is_refused(self):
        rid = self.sent()
        rc, _ = self.delete(rid)
        self.assertNotEqual(rc, 0)
        self.assertIsNotNone(report.load(rid))

    def test_the_refusal_explains_the_consequence(self):
        rid = self.sent()
        _, out = self.delete(rid)
        self.assertIn("issue", out.lower())
        self.assertIn("--force", out)

    def test_the_refusal_is_distinguishable_from_a_missing_id(self):
        """A worker or a script needs to tell "there is no such report" from "I am refusing
        this one", and the exit code is what carries that without parsing English."""
        sent, _ = self.delete(self.sent())
        missing, _ = self.delete("nosuchid")
        self.assertNotEqual(sent, missing)

    def test_force_deletes_it(self):
        rid = self.sent()
        rc, _ = self.delete(rid, force=True)
        self.assertEqual(rc, 0)
        self.assertIsNone(report.load(rid))


class TestTheStoreLayer(DeleteCase):
    def test_delete_reports_whether_anything_went(self):
        rid = self.draft()
        self.assertTrue(report.delete(rid))
        self.assertFalse(report.delete(rid))

    def test_deleting_is_local_and_touches_no_network(self):
        """Drafting reaches no network by design (ADR 0003 keeps recording and reporting
        separate); undoing a draft must not either."""
        import subprocess
        rid = self.draft()
        calls = []
        real = subprocess.run

        def spy(cmd, *a, **kw):
            calls.append(cmd)
            return real(cmd, *a, **kw)

        subprocess.run = spy
        self.addCleanup(setattr, subprocess, "run", real)
        self.delete(rid)
        self.assertEqual([c for c in calls if c and c[0] in ("gh", "glab", "git")], [])


if __name__ == "__main__":
    unittest.main()
