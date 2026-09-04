"""A named ceiling that carries what to do about it.

`doctor` earned its deficit list by refusing to render a lower ceiling as health. The next
failure is subtler: a limit stated and left there reads as "nothing can be done", and the
operator stops looking. Where charter HAS an answer, the deficit carries it.

Where it has none, it says nothing rather than inventing one — an empty remedy is a claim
too, and a wrong workaround costs more than an honest gap.
"""

from __future__ import annotations

import tempfile
import unittest

from charter import doctor
from charter.harness import registry


class RemediesAreOffered(unittest.TestCase):
    def test_a_missing_status_bar_names_the_ambient_render(self):
        for name in ("opencode", "codex"):
            with self.subTest(harness=name):
                d = next(d for d in registry.get(name).deficits if d.key == "status-bar")
                self.assertIn("charter statusline --watch", d.remedy)

    def test_a_deficit_with_no_answer_offers_none(self):
        """opencode has no per-turn prompt hook and charter cannot conjure one — the
        nudges already ride tool output, which the `detail` explains. Inventing a remedy
        here would send someone off to configure something that does not exist."""
        d = next(d for d in registry.get("opencode").deficits if d.key == "prompt-hook")
        self.assertEqual(d.remedy, "")

    def test_every_remedy_is_a_command_the_operator_can_run(self):
        for h in registry.all():
            for d in h.deficits:
                if d.remedy:
                    with self.subTest(harness=h.name, key=d.key):
                        self.assertIn("charter ", d.remedy)

    def test_every_remedy_names_a_subcommand_the_cli_actually_registers(self):
        """A substring test is not enough, and #895 is why it is written down.

        `assertIn("charter ", …)` above passes for `charter definitely-not-a-command`, so
        the remedy could name a subcommand that had been deleted and every test here would
        stay green. #895 proposed removing `charter statusline` on the grounds that Claude
        Code's footer was its only consumer — and both remedies on this page are
        `charter statusline --watch`, which nothing in the suite would have noticed going
        away. It is measured against the parser rather than a hand-kept list, so a
        subcommand renamed is caught the same way one deleted is.
        """
        from charter import cli

        registered = set(cli.build_parser()._subparsers._group_actions[0].choices)
        self.assertIn("statusline", registered,
                      "the parser this test reads has no subcommands in it")
        for h in registry.all():
            for d in h.deficits:
                if not d.remedy:
                    continue
                words = d.remedy.split()
                self.assertEqual(words[0], "charter", d.remedy)
                with self.subTest(harness=h.name, key=d.key):
                    self.assertIn(words[1], registered,
                                  f"{h.name}/{d.key} sends the operator to "
                                  f"`{' '.join(words[:2])}`, which is not a command")


class DoctorShowsThem(unittest.TestCase):
    def test_the_row_carries_the_remedy_beside_the_ceiling(self):
        import os
        from unittest import mock
        # `clear=True` wipes the suite-wide sandbox redirect too, so `global_dir()` would
        # fall back to the developer's REAL ~/.config/opencode — which is both a leak and
        # a flake, since a stale plugin there would fail this test.
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "opencode",
                                          "XDG_CONFIG_HOME": tempfile.mkdtemp()},
                             clear=True):
            r = doctor.check_harness()
        self.assertIn("charter statusline --watch", r.detail)


if __name__ == "__main__":
    unittest.main()
