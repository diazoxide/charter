"""Typing a command charter doesn't have is a **gap** signal.

The reporting capability is advertised only when something fails — that costs no prompt
budget and is perfectly targeted. But a gap prints nothing on its own, so it would have no
delivery mechanism at all. An unknown subcommand closes that: someone reaching for
`charter workspace archive` has mechanically expressed a missing capability.

This path exits via ``SystemExit`` from inside ``parse_args``, which runs *above* the
``try`` that catches crashes — so it needs its own handling, not a third ``except``.
"""
from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout

from charter import cli
from tests._isolation import PersonaIso


def _run(argv) -> str:
    err = io.StringIO()
    with redirect_stderr(err), redirect_stdout(io.StringIO()):
        try:
            cli.main(argv)
        except SystemExit:
            pass
    return err.getvalue()


class TestUnknownSubcommandOffersToReportAGap(PersonaIso, unittest.TestCase):
    """`PersonaIso` because an unknown first word is exactly what a PROFILE name is: the
    launch rewrite asks `profiles.current()` whether the plane declares one before argparse
    ever sees it, and `tests/_planeguard` refuses a read of the developer's own
    `charter.local.toml`. A throwaway plane declares none, so the word stays unknown and
    the gap signal is what it was."""

    def test_an_unknown_command_points_at_gap_reporting(self):
        self.assertIn("charter report gap", _run(["frobnicate"]))

    def test_it_names_the_command_that_was_missing(self):
        self.assertIn("frobnicate", _run(["frobnicate"]))


class TestItDoesNotFireOnOrdinaryUsageErrors(unittest.TestCase):
    """A gap is 'charter cannot do this', not 'you typed it wrong'. Offering to file an
    upstream issue every time someone fat-fingers a flag would make the prompt noise, and
    noise is what gets a feature like this switched off."""

    def test_a_bad_flag_on_a_real_command_is_not_a_gap(self):
        self.assertNotIn("charter report gap", _run(["doctor", "--nonexistent-flag"]))

    def test_a_missing_required_argument_is_not_a_gap(self):
        self.assertNotIn("charter report gap", _run(["report", "show"]))

    def test_help_is_not_a_gap(self):
        self.assertNotIn("charter report gap", _run(["--help"]))

    def test_an_unknown_flag_is_not_a_gap(self):
        """`--frobnicate` is a typo, not a missing capability."""
        self.assertNotIn("charter report gap", _run(["--frobnicate"]))


if __name__ == "__main__":
    unittest.main()
