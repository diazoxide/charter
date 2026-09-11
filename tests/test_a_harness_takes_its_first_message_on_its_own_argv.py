"""A harness is told its first message on its own command line, in its own spelling.

`charter handoff` (chat-handoff plan, Task 2) opens a chat whose first message is a brief the
operator approved. Charter never types it into the harness pane — typing there is drawing in
it (ADR 0018), and it races the harness's own start — so the message rides the argv the
harness is started with. The spelling differs by harness: `claude "<text>"` and
`codex "<text>"` take it as their positional prompt, and opencode takes it as
`--prompt "<text>"`. That difference is why this is a member of the harness rather than the
launcher's `extra` pass-through. A harness charter has not measured answers `None`, and the
seam that asks refuses rather than guessing at an argument.
"""

from __future__ import annotations

import unittest

from charter.harness import base, registry


class AHarnessTakesItsFirstMessageOnItsOwnArgv(unittest.TestCase):
    def test_claude_code_takes_it_as_its_positional_prompt(self):
        self.assertEqual(registry.get("claude-code").first_message_argv("fix the widget"),
                         ["fix the widget"])

    def test_codex_takes_it_as_its_positional_prompt(self):
        self.assertEqual(registry.get("codex").first_message_argv("fix the widget"),
                         ["fix the widget"])

    def test_opencode_takes_it_as_its_prompt_flag(self):
        self.assertEqual(registry.get("opencode").first_message_argv("fix the widget"),
                         ["--prompt", "fix the widget"])

    def test_a_harness_charter_has_not_measured_says_none(self):
        self.assertIsNone(base.Harness().first_message_argv("x y"))

    def test_a_multi_line_text_stays_one_argument(self):
        """The whole message is ONE element, never split on whitespace or lines: tmux execs
        a multi-element argv directly, so each element reaches the harness as one argument,
        and #959's `tmuxctl.verbatim` is what carries a trailing `;` through whole."""
        text = "a\n\nb c"
        for h in registry.all():
            with self.subTest(harness=h.name):
                got = h.first_message_argv(text)
                self.assertEqual(got[-1], text)
                self.assertEqual(got.count(text), 1)


if __name__ == "__main__":
    unittest.main()
