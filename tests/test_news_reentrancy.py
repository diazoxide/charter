"""A probe never runs from inside a probe.

`check:` is dispatched in-process, so an entry naming a command that itself probes news
puts `doctor` inside `doctor`: `doctor` → `check_news_adoption` → `probe` → `_dispatch`
→ `doctor` … Each level runs a full sweep, so it does not finish (#311).

Two properties made it worse than an ordinary bug, and both are what these tests pin:

**It was dormant until release.** `version: unreleased` entries are never probed, so the
landmine survived review, CI and merge untouched and armed itself at `charter news stamp`.

**CI could not catch it before the tag burned.** `charter news --for <v>` answers in 0.1s
and the release guard passes on it; only the `test` job fails — after the tag push has
already fired the irreversible PyPI upload.

So the fix is a guard rather than a rule about entries: whatever a `check:` names, the
probe is bounded and its answer is honest. "Honest" is the harder half — a nested `doctor`
exits 0 whenever nothing is broken, and taking that as the probe's answer would report the
entry **adopted**, which hides it forever. A re-entered probe therefore has no answer at
all: `unknown`, the ADR 0013 shape, and the reason names the entry rather than the machine.

Every test here bounds itself. A real recursion must fail CI, not wedge it, so the sweep
count is capped inside the test and asserted on — never left to a wall clock.
"""

from __future__ import annotations

import io
import json
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config, doctor, news

#: How many doctor sweeps a test tolerates before it breaks the loop itself. Two is the
#: guarded answer (the outer sweep, plus the one nested dispatch that is then refused);
#: anything beyond that is the bug. The cap exists so an unguarded tree ends in a failed
#: assertion instead of a hung runner.
SWEEP_CAP = 6


class NewsDir(unittest.TestCase):
    """Entries read from a throwaway directory, so no test depends on what shipped."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        patch = mock.patch.object(news, "_PACKAGED", self.dir)
        patch.start()
        self.addCleanup(patch.stop)

    def write(self, check: str, *, slug: str = "x", version: str = "0.44.0") -> news.Entry:
        (self.dir / f"{version}-{slug}.md").write_text(
            f"---\nversion: {version}\nheadline: h\ncheck: {check}\nadopt: version\n---\nbody\n")
        return [e for e in news.all() if e.slug == slug][0]


class CheckDoctor(NewsDir):
    """The reproduction from #311, with the network-bound checks removed.

    `doctor._checks` is replaced by one that runs the real `check_news_adoption` — the real
    probe, the real `_dispatch`, the real parser, the real `cmd_doctor` — and nothing else.
    What is removed is only what makes a full sweep slow and machine-dependent; the loop
    itself is the shipped one.
    """

    def setUp(self):
        super().setUp()
        self.sweeps = 0

        def sweep():
            self.sweeps += 1
            if self.sweeps > SWEEP_CAP:
                # Break the loop rather than recursing on. Without this an unguarded tree
                # runs until the interpreter's recursion limit, through a full sweep at
                # every level — which is a hung CI job, not a red one.
                return []
            return [doctor.check_news_adoption()]

        patch = mock.patch.object(doctor, "_checks", sweep)
        patch.start()
        self.addCleanup(patch.stop)

    def _doctor(self) -> list[dict]:
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(io.StringIO()):
            commands.cmd_doctor(SimpleNamespace(json=True))
        return json.loads(out.getvalue())

    def test_doctor_finishes_when_an_entry_checks_doctor(self):
        self.write("doctor")
        self._doctor()
        self.assertLessEqual(
            self.sweeps, 2,
            f"`check: doctor` re-entered doctor {self.sweeps} times — a probe must not "
            f"run from inside a probe")

    def test_the_entry_is_reported_unchecked_never_adopted(self):
        """The half a depth guard alone does not fix.

        Refusing only the *nested* dispatch leaves the outer one reading the nested
        `doctor`'s exit code, which is 0 on a healthy plane — so the entry would report
        **adopted** and never be offered again. A re-entered probe has no answer.
        """
        self.write("doctor")
        row, = [r for r in self._doctor() if r["name"] == "news"]
        self.assertEqual(row["status"], doctor.WARN)
        self.assertIn("unchecked", row["detail"])


class ARefusedProbe(NewsDir):
    def test_a_check_that_probes_news_is_unknown(self):
        """`news --pending` is the other surface that probes, so the guard is about the
        shape and not about `doctor` — any command that probes is one a probe cannot use."""
        e = self.write("news --pending")
        with mock.patch.object(config, "HAS_CONTROL_PLANE", True):
            status, why = news.probe(e)
        self.assertEqual(status, news.UNKNOWN)
        self.assertTrue(why.strip())

    def test_the_reason_blames_the_entry_and_not_this_machine(self):
        """A probe that could not run *here* and a probe that can never run are both
        `unknown`, and folding them into one sentence hides the second behind the first.
        "did not run here" invites the reader to check their machine; nothing about their
        machine is wrong."""
        e = self.write("news --pending")
        with mock.patch.object(config, "HAS_CONTROL_PLANE", True):
            _, why = news.probe(e)
        self.assertNotIn("did not run here", why)
        self.assertIn("news --pending", why)

    def _pending(self) -> str:
        err = io.StringIO()
        args = SimpleNamespace(pending=True, since=None, until=None, for_version=None)
        with mock.patch.object(config, "HAS_CONTROL_PLANE", True), \
             redirect_stdout(io.StringIO()), redirect_stderr(err):
            commands.cmd_news(args)
        return err.getvalue()

    def test_charter_news_pending_prints_the_reason(self):
        """The one line that makes the next instance self-evident. `doctor` counts it as
        "1 unchecked" and points here; here is where the entry is named as the defect."""
        self.write("news --pending", slug="broken")
        self.assertIn("broken", self._pending())

    def test_pending_does_not_then_tick_every_probe_as_answered(self):
        """The closing ✓ claims *every entry with a probe reports adopted* — which is the
        one thing an unchecked entry did not say. Printed under the warning it contradicts,
        it is how the warning stops being read: the eye takes the tick as the summary."""
        self.write("news --pending", slug="broken")
        err = self._pending()
        self.assertNotIn("every entry with a probe reports adopted", err)
        self.assertIn("could not be checked", err)

    def test_pending_still_ticks_when_every_probe_did_answer(self):
        """And the tick survives where it is true — a guard that removed the good news
        along with the false claim would be a worse trade than the bug."""
        e = self.write("version", slug="fine")
        with mock.patch.object(commands, "cmd_version", lambda args: 0):
            self.assertEqual(news.probe(e)[0], news.ADOPTED)
            self.assertIn("every entry with a probe reports adopted", self._pending())


class TheGuardDoesNotBreakProbing(NewsDir):
    """The feature still works. A guard that quietly turned every probe into `unknown`
    would pass every test above and report nothing to anybody."""

    def _probe(self, code: int):
        e = self.write("version")
        ran = []

        def cmd(args):
            ran.append(True)
            return code

        with mock.patch.object(commands, "cmd_version", cmd):
            status, why = news.probe(e)
        self.assertEqual(len(ran), 1, "the probe did not actually run the subcommand")
        return status, why

    def test_a_check_that_exits_zero_still_reports_adopted(self):
        self.assertEqual(self._probe(0)[0], news.ADOPTED)

    def test_a_check_that_exits_non_zero_still_reports_pending(self):
        self.assertEqual(self._probe(1)[0], news.PENDING)

    def test_two_probes_in_a_row_both_run(self):
        """A guard set on the way in and never released would refuse everything after the
        first probe — the whole feature, broken by its own fix, and only on the second
        entry so a single-entry test would not see it."""
        self.assertEqual(self._probe(0)[0], news.ADOPTED)
        self.assertEqual(self._probe(1)[0], news.PENDING)


class TheGuardIsReleasedOnFailure(NewsDir):
    def test_a_probe_that_raises_leaves_nothing_armed(self):
        """`_dispatch` swallows every exception, so a command that blows up is a normal
        outcome here — and the one path where a guard set outside a `finally` stays armed
        for the life of the process, turning every later probe into `unknown`."""
        e = self.write("version")

        def boom(args):
            raise RuntimeError("probe exploded")

        with mock.patch.object(commands, "cmd_version", boom):
            self.assertEqual(news.probe(e)[0], news.UNKNOWN)

        with mock.patch.object(commands, "cmd_version", lambda args: 0):
            self.assertEqual(news.probe(e)[0], news.ADOPTED)

    def test_a_probe_that_exits_leaves_nothing_armed(self):
        """`SystemExit` is not an `Exception`, so it takes a different path out of
        `_dispatch` — and argparse raises it for every malformed `check:`."""
        e = self.write("version")

        def bail(args):
            raise SystemExit(2)

        with mock.patch.object(commands, "cmd_version", bail):
            self.assertEqual(news.probe(e)[0], news.UNKNOWN)

        with mock.patch.object(commands, "cmd_version", lambda args: 0):
            self.assertEqual(news.probe(e)[0], news.ADOPTED)


if __name__ == "__main__":
    unittest.main()
