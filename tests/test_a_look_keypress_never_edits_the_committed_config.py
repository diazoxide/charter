"""#787: recording a chrome choice must not edit `charter.toml` — comments included.

`charter frame-chrome <word>` records the operator's choice, and #787 reports that it
records it by substituting the colour word through the whole file — rewriting comments
that merely *mention* the old word, and turning this repository's own explanation of focus
inversion into a false sentence:

    committed:  # inverts on focus (`brightblack` active is `black`), so the focused …
    reported:   # inverts on focus (`black` active is `black`), so the focused …

**Read against `main`, no such write exists.** `cmd_chrome` records through
`state.record_chrome`, into the frame's own state directory, and the only code in charter
that writes `charter.toml` at all is `instance._set_key` (the version lock and the default
persona, both confined to one section's own line span) and `commands.cmd_init`'s first
render. The docstrings of `cmd_chrome`, `cmd_density` and `cmd_toggle` each say
"charter.toml is not touched" and `cli` says it twice more. So the damage #787 measured on
the operator's plane came from something outside charter — the same substitution a
`s/brightblack/black/g` makes — and the committed diff it was read from changes six `bg`
VALUES as well, which no chrome word would touch.

That makes this file a guard and not a fix. The property was undefended: no test asserted
that the committed file survives a chrome keypress, so the design's loudest claim was
resting on three docstrings. What is asserted here is the whole of #787's requirement —
the choice IS recorded, the committed file is byte-identical afterwards, and the sentence
#787 watched being falsified is still true — so that a writer added to this path in future
has to delete an assertion to land.
"""

from __future__ import annotations

import os
import pathlib
import subprocess
import unittest
from unittest import mock

from charter import commands_frame, config, instance
from charter.frame import state
from tests._isolation import PersonaIso

#: The operator's own committed text, trimmed to the two tables and the one paragraph
#: #787 was measured on. `chrome = "light"` is deliberately NOT the word pressed below, so
#: "the file was not rewritten" and "the keypress did nothing" cannot be the same green.
_COMMITTED = '''\
[charter]
version = "0.54.0"

[frame]
mouse = true

# How charter's own chrome READS. Three words: `off`, `dark`, `light`.
chrome = "light"

# Every pane charter draws is `brightblack` and the harness pane is left alone, which is
# the whole look: charter's chrome is grey, the work area is the terminal's own. The pair
# inverts on focus (`brightblack` active is `black`), so the focused panel is the one that
# differs from its neighbours rather than the one that lights up.

[[frame.component]]
use = "identity"
bg  = "brightblack"
pad = 1

[[frame.component]]
use  = "workspaces"
edge = "top"
size = 1
bg   = "brightblack"
pad  = 1
'''

#: The sentence #787 watched become false, verbatim. A substitution of `brightblack` makes
#: this read "`black` active is `black`" — a comment asserting that focus changes nothing,
#: in a comment whose whole job is to explain why the surface is legible.
_TRUE_SENTENCE = "inverts on focus (`brightblack` active is `black`)"


class AChromeKeypressLeavesTheCommittedConfigAlone(PersonaIso, unittest.TestCase):
    """`cmd_chrome` against a plane whose `charter.toml` mentions a colour in prose."""

    FID = "fr-787"

    def setUp(self):
        super().setUp()
        # `PersonaIso` derives every setting from an empty tmp dir, so the marker has to
        # exist before the derivation for `config.FRAME` to be this fixture's arrangement.
        # Re-derived rather than hand-patched: `config.FRAME` and `config.STATE_DIR` are
        # two of twenty-five values that follow from the root, and patching one of them by
        # hand is how a test comes to run against a plane that is half fixture.
        self.toml = self.tmp / "charter.toml"
        self.toml.write_text(_COMMITTED)
        config.use(self.tmp)

        # The standing rule this session learned the hard way: a plane-mutating call is
        # made only once the path it will write has been asserted to be the fixture's.
        # `state.record_chrome` writes under `config.STATE_DIR`, and a test that trusted
        # `$CHARTER_ROOT` instead of asserting the path deleted two of the operator's own
        # chat directories.
        self.assertTrue(
            pathlib.Path(config.STATE_DIR).resolve().is_relative_to(self.tmp.resolve()),
            f"refusing to record anything: STATE_DIR is {config.STATE_DIR}")

        self.ran: list[list[str]] = []
        patcher = mock.patch.object(
            commands_frame.tmuxctl, "run",
            side_effect=lambda _what, argv, **kw: self.ran.append(argv) or
            subprocess.CompletedProcess(argv, 0, "", ""))
        patcher.start()
        self.addCleanup(patcher.stop)

        # Both halves of the keypress: the panels, and the frame's own rules — which is
        # the half that reads `config.FRAME` (`instance.border_bg`) and so the half a
        # writer would most plausibly be bolted onto.
        state.record_panes(self.FID, panels={"identity": "%1", "workspaces": "%2"})
        state.record_harness_pane(self.FID, "%0")

    def _press(self, word="dark"):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True):
            return commands_frame.cmd_chrome(type("A", (), {"level": word})())

    def test_the_keypress_really_ran(self):
        """Asserted first and separately, because every assertion below is satisfied by a
        command that refused. #787's subject is a keypress that DID repaint the frame and
        edited the file on its way — so the guard is worth nothing until the repaint is
        shown to have happened."""
        self.assertEqual(self._press(), 0)
        self.assertEqual({a[a.index("-t") + 1] for a in self.ran}, {"%0", "%1", "%2"})

    def test_the_choice_is_recorded(self):
        """The half #787 says must keep working: the palette's chrome rows persist the
        choice deliberately, and a fix that stopped recording would satisfy every other
        assertion in this class."""
        self._press()
        self.assertEqual(state.chrome(self.FID), "dark")
        self.assertEqual(commands_frame._current_chrome(self.FID), "dark")

    def test_the_committed_file_is_byte_identical_afterwards(self):
        """The whole of #787, as bytes. Not "the values are right" — the FILE, so a
        rewrite that happened to reproduce the arrangement still fails."""
        before = self.toml.read_bytes()
        self._press()
        self.assertEqual(self.toml.read_bytes(), before)

    def test_the_committed_file_was_not_even_rewritten(self):
        """Bytes cannot see a rewrite that reproduces the content, and #787's compounding
        argument is about prose rewritten again on every keypress — so the mtime is part of
        the property. Stated over the WHOLE plane rather than over one path, because "which
        file did it edit" is the question #726 could not answer about its own symptom, and a
        guard that names only `charter.toml` answers it for one file.

        Paired with the state directory deliberately: the same walk that proves nothing the
        operator maintains moved proves that the keypress moved SOMETHING, so this cannot
        go green by the command doing nothing.
        """
        def snapshot(where):
            return {p: (p.read_bytes(), p.stat().st_mtime_ns)
                    for p in sorted(pathlib.Path(where).rglob("*")) if p.is_file()}

        state_dir = pathlib.Path(config.STATE_DIR)
        committed = {p: v for p, v in snapshot(self.tmp).items()
                     if not p.is_relative_to(state_dir)}
        recorded = snapshot(state_dir)
        self.assertIn(self.toml, committed)

        self._press()

        after = {p: v for p, v in snapshot(self.tmp).items()
                 if not p.is_relative_to(state_dir)}
        self.assertEqual(after, committed)
        self.assertNotEqual(snapshot(state_dir), recorded,
                            "the keypress recorded nothing, so this proves nothing")

    def test_the_comment_that_explains_focus_inversion_is_still_true(self):
        """#787's own measurement, as an assertion. The rewritten sentence is not merely
        an ugly diff — it asserts that a pane painted `black` inverts to `black`, which
        says focus changes nothing, which is false and self-refuting."""
        self._press()
        text = self.toml.read_text()
        self.assertIn(_TRUE_SENTENCE, text)
        self.assertNotIn("(`black` active is `black`)", text)

    def test_the_colour_words_in_the_prose_and_in_the_values_both_survive(self):
        """Counted rather than searched, because what #787 measured is a COUNT — six value
        lines and two comment lines on the operator's own file — and an `assertIn` is
        satisfied by one surviving mention.

        The fixture keeps two of each, which is the distinction the reported write did not
        make: `brightblack` in a `bg` key is the setting, `brightblack` in prose is a
        coincidence, and both counts have to hold for the same reason.
        """
        self._press()
        lines = self.toml.read_text().splitlines()
        self.assertEqual(
            sum("brightblack" in ln for ln in lines if ln.lstrip().startswith("#")), 2)
        self.assertEqual(
            sum("brightblack" in ln for ln in lines
                if not ln.lstrip().startswith("#")), 2)

    def test_the_committed_word_still_reads_as_the_committed_word(self):
        """The recorded choice and the committed default are two answers to two different
        questions, and the whole design rests on their staying different: `[frame] chrome`
        says what a frame STARTS at, the record says what this frame IS. A write into
        charter.toml would collapse them, and the collapse is invisible until relaunch."""
        self._press()
        # Re-READ, not `config.FRAME`: that dict was derived in `setUp` and would answer
        # `light` however the file had been rewritten — the first version of this test did
        # exactly that and stayed green against the injected defect.
        fresh = instance.load(config.ROOT).get("frame") or {}
        self.assertEqual(instance.chrome_level(fresh.get("chrome")), "light")
        self.assertEqual(commands_frame._current_chrome(self.FID), "dark")

    def test_the_word_landed_in_the_frames_own_state_directory(self):
        """Where the design says it goes — and asserted as a PATH rather than through
        `state.chrome`, which would be satisfied by a record kept anywhere at all. This is
        also what makes the record temporary: `state.reap` deletes that directory entire
        when the frame ends, so a relaunch is back to the committed word."""
        self._press()
        found = [p for p in pathlib.Path(config.STATE_DIR).rglob("chrome")
                 if p.is_file() and p.read_text().strip() == "dark"]
        self.assertTrue(found, f"nothing under {config.STATE_DIR} holds the word")


if __name__ == "__main__":
    unittest.main()
