"""Codex, registered from what the binary says rather than what the docs say.

Every fact asserted here was pinned against codex-cli 0.147.0 by feeding candidate config
through `codex mcp list -c '<toml>'` and reading which shapes the parser REJECTS. Only
rejections carry information: unknown keys are ignored everywhere (`zzz_nonsense_key=1`
loads clean), so a shape that parses proves nothing on its own — but a shape that fails
proves the key is known and the type is wrong.
"""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from charter.harness import codex, registry


class CodexIsRegistered(unittest.TestCase):
    def test_it_is_known_by_the_name_it_puts_in_the_environment(self):
        self.assertEqual(registry.get("codex").name, "codex")

    def test_its_hook_entry_shape_is_the_one_the_parser_accepts(self):
        """`{command=...}` alone and `{handler_type=..., command=...}` are both rejected;
        `{type="command", command=...}` is accepted, and so is a `timeout`. That the
        parser rejects a MISSING `command` and a bogus `type` is what makes the accepted
        shape meaningful rather than an artefact of unknown-key tolerance."""
        self.assertEqual(codex.HOOK_ENTRY_KEYS, ("type", "command"))
        self.assertEqual(codex.HOOK_TYPE, "command")


class CodexCeilings(unittest.TestCase):
    def test_its_ceilings_are_named(self):
        keys = {d.key for d in registry.get("codex").deficits}
        self.assertEqual(keys, {"status-bar", "session-lock", "workspace-scope"})

    def test_the_prompt_hook_is_not_a_ceiling(self):
        """Unlike opencode, Codex implements `UserPromptSubmit` — its hook contract is
        Claude Code's near-verbatim, so charter's mid-session context arrives beside the
        turn rather than riding a tool result."""
        keys = {d.key for d in registry.get("codex").deficits}
        self.assertNotIn("prompt-hook", keys)


class CodexIsNotWiredYet(unittest.TestCase):
    def test_wiring_writes_nothing_until_it_is_consented_to(self):
        """Codex reads no project-level config FILE — `.codex/config.toml` and
        `codex.toml` in a project directory are both ignored, verified by planting a type
        error in each and watching the config load succeed anyway. (A project
        `.codex/skills/` IS read; that is a skills surface, not config.) Its hooks live
        only in
        `~/.codex/config.toml`, so wiring it is a MACHINE-WIDE act that would fire
        charter's hooks in every repo on the machine. `init` must not do that silently:
        it is the failure ADR 0014 already records, where a guard "fired in unrelated
        repos and explained a control plane that did not exist there"."""
        root = Path(tempfile.mkdtemp(prefix="charter-codex-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(root, True))
        self.assertEqual(registry.get("codex").wire(root), [])
        self.assertEqual(list(root.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
