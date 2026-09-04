"""`charter workspace reinit --all` counts repairs and workspaces as two different things.

Reported from a 17-workspace plane (#876):

```
• Healed 32 of 17 workspace(s); the rest were current.
```

**32 of 17.** One counter was incremented once per REPAIR and then printed against
``len(names)``, which counts WORKSPACES — so the numerator outran its own denominator and
"the rest were current" described a negative number. A workspace needs more than one
repair routinely, not exceptionally: the harness layer is a row per file it writes and the
structure bump is another, so every workspace on a plane that predates `STRUCTURE_VERSION`
5 emits two `✓ Reinitialized` lines and contributes 2 to a counter labelled "workspaces".

That line is the only summary that survives the run — the per-workspace rows scroll away —
so it is what an operator reads to confirm a bulk mutation landed. A number that cannot be
true there discredits the whole report.

**Every fixture in this file gives at least one workspace TWO repairs**, and that is the
point rather than an accident of setup. A plane where every workspace needs exactly one
repair reads identically under both spellings, which is precisely why the defect shipped
past the tests that existed.

The invariant is structural, not arithmetic: `cmd_workspace_reinit` accumulates a SET OF
NAMES drawn from the list it is iterating, so ``len(repaired) <= len(names)`` holds by
construction and there is no subtraction in a position to get it wrong.
`ACountNeverExceedsItsTotal` reads the printed sentence back and checks the numbers
against each other, so the promise is pinned on the output rather than on the variable.

Sentences are spelled out by hand rather than rebuilt from the code that prints them: a
test assembled from the same f-string agrees with whatever wording that f-string takes,
including the wording this file exists to have replaced.
"""

from __future__ import annotations

import io
import json
import re
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from types import SimpleNamespace

from charter import config, workspace
from charter import commands_workspace as cw
from tests._isolation import PersonaIso

#: The summary's shape, read back so a case can compare the numbers in it. Deliberately
#: loose about the wording and strict about the arithmetic — this is the guard against a
#: numerator above its denominator, whatever sentence the numbers end up inside.
_SUMMARY = re.compile(r"Applied (\d+) repair\(s\) across (\d+) of (\d+) workspace\(s\)")


class BulkRepairCase(PersonaIso):
    """A plane whose workspaces can be made to need one repair, two, or none."""

    def setUp(self) -> None:
        super().setUp()
        # The tripwire every fixture that writes a plane runs under: `PersonaIso` repoints
        # the derived paths at a throwaway tree, and if it ever stops doing so the writes
        # below land in somebody's real plane.
        self.assertIn("edm-test-", str(config.STATE_DIR))
        self._plane_settings()

    def _plane_settings(self) -> None:
        """The plane's own `.claude/settings.json`, shaped like this repo's committed one.

        Without it there is no harness layer to write into a workspace at all, and the
        two-repairs case — the only one that separates the two spellings — cannot exist.
        """
        d = config.ROOT / ".claude"
        d.mkdir(parents=True, exist_ok=True)
        (d / "settings.json").write_text(json.dumps({
            "env": {"CHARTER_HARNESS": "claude-code"},
            "statusLine": {"type": "command", "command": "charter statusline",
                           "padding": 0, "refreshInterval": 10},
            "enabledPlugins": {"charter@charter": True},
            "permissions": {"allow": ["Bash(ls:*)"]},
        }, indent=2) + "\n")

    def current(self, name: str) -> None:
        """A workspace with nothing to repair."""
        workspace.ensure(name)

    def one_repair(self, name: str) -> None:
        """Stale STRUCTURE only — the layer is current, so exactly one row is printed."""
        workspace.ensure(name)
        (workspace.workspace_dir(name) / ".charter-structure").write_text("3\n")

    def two_repairs(self, name: str) -> None:
        """Stale layer AND stale structure — the shape a plane older than #884 is in.

        Two `✓ Reinitialized` lines, one workspace. Removing the generated marker with the
        file is what makes the layer read `created` rather than `foreign`: charter vouches
        for a path by digest, and a settings.json it has no digest for is the operator's.
        """
        workspace.ensure(name)
        (workspace.workspace_dir(name) / ".claude" / "settings.json").unlink()
        (workspace.workspace_dir(name) / workspace.GENERATED_MARKER).unlink()
        (workspace.workspace_dir(name) / ".charter-structure").write_text("3\n")

    def reinit_all(self) -> str:
        buf = io.StringIO()
        with redirect_stderr(buf):
            cw.cmd_workspace_reinit(SimpleNamespace(name=None, all=True))
        return buf.getvalue()

    def rows(self, said: str) -> list[str]:
        return [ln for ln in said.splitlines() if "Reinitialized" in ln]

    def summary(self, said: str) -> tuple[int, int, int]:
        """``(repairs, workspaces repaired, workspaces looked at)`` off the printed line."""
        m = _SUMMARY.search(said)
        self.assertIsNotNone(m, f"no summary line in:\n{said}")
        return int(m.group(1)), int(m.group(2)), int(m.group(3))


class ACountNeverExceedsItsTotal(BulkRepairCase):
    """The bug, stated as the property it violated."""

    def test_the_workspace_count_does_not_outrun_the_workspace_total(self):
        """`Healed 6 of 3`, before. Three workspaces, two repairs each."""
        for n in ("alpha", "beta", "gamma"):
            self.two_repairs(n)
        said = self.reinit_all()
        repairs, across, total = self.summary(said)
        self.assertEqual(total, 3)
        self.assertLessEqual(across, total,
                             f"more workspaces repaired than exist:\n{said}")
        self.assertEqual(across, 3)
        self.assertEqual(repairs, 6)

    def test_the_rest_is_never_a_negative_number_of_workspaces(self):
        """"the rest were current" is a claim about ``total - across``. It has to be a
        count of workspaces somebody could go and look at."""
        for n in ("alpha", "beta"):
            self.two_repairs(n)
        self.current("gamma")
        repairs, across, total = self.summary(self.reinit_all())
        self.assertGreaterEqual(total - across, 0)
        self.assertEqual(total - across, 1, "the one workspace that needed nothing")

    def test_a_workspace_repaired_twice_is_one_workspace_and_two_repairs(self):
        """The two numbers must DIFFER here, or a test proves nothing about which of them
        the sentence is naming. One workspace, two `✓ Reinitialized` rows."""
        self.two_repairs("alpha")
        self.current("beta")
        said = self.reinit_all()
        self.assertEqual(len(self.rows(said)), 2, said)
        repairs, across, total = self.summary(said)
        self.assertEqual((repairs, across, total), (2, 1, 2))

    def test_the_repair_count_is_the_number_of_rows_printed_above_it(self):
        """The summary adds up the report it closes: whatever the mix, the first number is
        exactly how many `✓ Reinitialized` lines an operator scrolled past."""
        self.two_repairs("alpha")
        self.two_repairs("beta")
        self.one_repair("gamma")
        self.current("delta")
        said = self.reinit_all()
        repairs, across, total = self.summary(said)
        self.assertEqual(repairs, len(self.rows(said)))
        self.assertEqual((repairs, across, total), (5, 3, 4))


class TheSentenceNamesWhatItCounts(BulkRepairCase):
    """Both numbers are kept and each is labelled — deduping to one would have thrown the
    per-repair number away, and it is the one that says how much work the run did."""

    def test_the_wording_says_repairs_and_workspaces_in_the_same_breath(self):
        self.two_repairs("alpha")
        self.two_repairs("beta")
        self.current("gamma")
        self.assertIn(
            "Applied 4 repair(s) across 2 of 3 workspace(s); the rest were current.",
            self.reinit_all())

    def test_it_does_not_still_say_healed_n_of_m(self):
        """The old sentence had one unit for two quantities. Named here so a revert to it
        fails rather than passing on a substring the new wording happens to share."""
        self.two_repairs("alpha")
        self.two_repairs("beta")
        self.assertNotIn("Healed", self.reinit_all())

    def test_a_plane_with_nothing_to_repair_says_so_instead(self):
        """No summary at all when there was no repair — "Applied 0 repair(s) across 0 of
        3" is a true sentence and a worse one than the sentence that already existed."""
        for n in ("alpha", "beta", "gamma"):
            self.current(n)
        said = self.reinit_all()
        self.assertIn(f"Up to date (structure v{workspace.STRUCTURE_VERSION}) — "
                      f"nothing to do.", said)
        self.assertNotIn("Applied", said)

    def test_one_workspace_gets_no_summary_line(self):
        """`--all` on a plane of one: the row above IS the summary, and a second sentence
        restating it as "1 of 1" is noise. The boundary is >1, not >=1."""
        self.two_repairs("alpha")
        said = self.reinit_all()
        self.assertEqual(len(self.rows(said)), 2, said)
        self.assertNotIn("Applied", said)

    def test_two_workspaces_do_get_one(self):
        """The other side of the same boundary, so a fixture of one is never the only
        witness to it."""
        self.two_repairs("alpha")
        self.current("beta")
        self.assertIn("Applied 2 repair(s) across 1 of 2 workspace(s)", self.reinit_all())


class AWorkspaceCharterCouldNotRepairIsNotCurrent(BulkRepairCase):
    """The other half of the same honesty. `blocked` exists so the closing line does not
    contradict an error two lines above it; "the rest were current" is a claim about the
    workspaces NOT repaired, and a blocked one is not current either."""

    def blocked(self, name: str) -> None:
        """A manifest path charter refuses to write, and a stale structure besides.

        A dangling symlink out of the plane: `contain.writable` refuses the write (#328)
        and `Path.exists` answers False through it, so the manifest is reported blocked
        while everything else in the workspace stays repairable.
        """
        workspace.ensure(name)
        workspace.manifest_path(name).unlink()
        workspace.manifest_path(name).symlink_to(self.tmp / "nowhere.json")

    def test_a_blocked_workspace_is_called_out_rather_than_counted_current(self):
        self.two_repairs("alpha")
        self.blocked("beta")
        said = self.reinit_all()
        self.assertIn("workspace.json could not be written", said)
        self.assertIn("1 could not be repaired; the rest were current.", said)

    def test_the_clause_is_absent_when_nothing_was_blocked(self):
        """It reports a fact, so it says nothing when there is no fact to report."""
        self.two_repairs("alpha")
        self.current("beta")
        self.assertNotIn("could not be repaired", self.reinit_all())

    def test_a_plane_where_only_a_blocked_workspace_exists_never_says_up_to_date(self):
        """The reason `blocked` was split off `healed` in the first place, kept working
        now that both are sets: nothing was repaired, and "nothing to do" over an error is
        the report nobody trusts again."""
        self.blocked("alpha")
        self.blocked("beta")
        said = self.reinit_all()
        self.assertNotIn("Up to date", said)
        self.assertIn("Applied 0 repair(s) across 0 of 2 workspace(s); 2 could not be "
                      "repaired; the rest were current.", said)


if __name__ == "__main__":
    unittest.main()
