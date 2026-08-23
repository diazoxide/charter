"""A harness that charter can run has to say what to type and what to exec.

`registry.KINDS` exists so a harness added to it is covered everywhere the day it is
registered. That only holds if the launcher reads these two facts off the harness rather
than keeping its own table — the hardcoded-literal problem the registry was built to end.
"""

from __future__ import annotations

import unittest

from charter import harness


class HarnessLaunchIdentity(unittest.TestCase):
    def test_every_registered_harness_says_what_to_type(self):
        for h in harness.all():
            with self.subTest(harness=h.name):
                self.assertTrue(h.cli_name, f"{h.name} has no cli_name")

    def test_cli_names_are_distinct(self):
        names = [h.cli_name for h in harness.all()]
        self.assertEqual(len(names), len(set(names)), f"colliding cli_names: {names}")

    def test_launch_argv_passes_the_operators_arguments_through_verbatim(self):
        h = harness.get(harness.CLAUDE_CODE)
        self.assertEqual(h.launch_argv(["--resume", "a;b"]),
                         ["claude", "--resume", "a;b"])

    def test_launch_argv_returns_a_list_never_a_string(self):
        """Pinned against tmux 3.7c: separate argv is not shell-interpreted, a joined
        string is. A harness returning a string would put the injection back."""
        for h in harness.all():
            with self.subTest(harness=h.name):
                self.assertIsInstance(h.launch_argv([]), list)


if __name__ == "__main__":
    unittest.main()
