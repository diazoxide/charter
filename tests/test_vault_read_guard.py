"""The vault guard covers file-reading TOOLS, not just Bash (#90).

PreToolUse registered two matchers — `Bash` and `Task|Agent` — and `pretooluse` reads
`tool_input["command"]`. A `Read(file_path=".charter/vaults/devops.json")` carries no
`command`, matches no matcher, and reached none of it.

What made that worse than a plain gap: the Bash denial *names the path it refused*. So the
agent ran `cat .charter/vaults/devops.json`, read a well-worded denial containing the path,
and its obvious next move — `Read` on that same path — succeeded and dumped every plaintext
credential into the transcript. The guard handed over the target.

Not a privilege escalation: anyone who can run the agent can already read the file. It was a
hole in the one mitigation charter documents — keeping the value out of the model's context
and out of the transcript — and `docs/secrets.md` claimed the mitigation existed.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from charter import hooks
from tests._isolation import PersonaIso, run_hook

ROOT = Path(__file__).resolve().parents[1]


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


def _reason(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecisionReason", "")


class ReadGuardCase(PersonaIso):
    def read(self, path: str, tool: str = "Read", key: str = "file_path", **extra):
        ti = {key: path} if tool == "Read" else {"path": path, **extra}
        return run_hook(hooks.pretooluse_read,
                        {"tool_name": tool, "tool_input": ti, "session_id": "s", "cwd": "/tmp"})


class TestReadingAVaultIsDenied(ReadGuardCase):
    def test_read_of_a_vault_file_is_denied(self):
        self.assertEqual(_decision(self.read(".charter/vaults/devops.json")), "deny")

    def test_an_absolute_vault_path_is_denied(self):
        self.assertEqual(_decision(self.read("/home/me/plane/.charter/vaults/devops.json")),
                         "deny")

    def test_the_denial_names_the_supported_alternative(self):
        """Same wording as the Bash guard. A denial that only says no teaches nothing, and
        the agent's next move is to find another way to the same bytes."""
        r = self.read(".charter/vaults/devops.json")
        self.assertIn("secret exec", _reason(r))

    def test_grep_into_the_vault_directory_is_denied(self):
        self.assertEqual(_decision(self.read(".charter/vaults", tool="Grep", pattern="token")),
                         "deny")

    def test_the_harnesss_own_spelling_of_the_path_key_is_denied_too(self):
        """`file_path` is Claude Code's name for it. opencode 1.18.21's `read` takes
        `filePath` — read off the running server's own `/experimental/tool` schema, not
        guessed — and a guard keyed on one spelling is a guard that is ABSENT on the other
        harness. That is half of #433: routing opencode's `read` to this handler would
        still have allowed the read, because the payload's key had a different name.

        The key belongs to the harness, so the guard reads every spelling a harness charter
        supports uses. `hooks._PATH_KEYS` is the one place that says so."""
        for key in ("filePath", "notebookPath"):
            with self.subTest(key=key):
                self.assertEqual(
                    _decision(self.read(".charter/vaults/devops.json", key=key)), "deny")

    def test_the_browser_and_active_paths_are_covered_too(self):
        """`_VAULT_PATH_RE` already covers these for Bash; the two guards must not disagree
        about what counts as a vault."""
        for p in (".charter/browser", ".charter/active-persona"):
            self.assertEqual(_decision(self.read(p)), "deny", p)


class TestItDoesNotOverreach(ReadGuardCase):
    def test_the_registry_is_not_a_vault_file(self):
        """`.charter/vaults.json` holds provider config and paths, never values — the same
        carve-out the Bash guard makes, and for the same reason. Note the regex's trailing
        slash: `vaults/` is the directory of secrets, `vaults.json` is the map."""
        self.assertIsNone(_decision(self.read(".charter/vaults.json")))

    def test_an_ordinary_file_is_untouched(self):
        self.assertIsNone(_decision(self.read("charter/hooks.py")))

    def test_glob_is_not_denied(self):
        """Glob returns NAMES, not contents — the same reason `ls` is absent from the Bash
        guard's `_READERS`. Denying it would block discovering that a vault exists, which
        is not the secret."""
        self.assertIsNone(_decision(self.read(".charter/vaults", tool="Glob",
                                              pattern=".charter/vaults/*")))

    def test_a_payload_with_no_path_is_not_an_error(self):
        r = run_hook(hooks.pretooluse_read,
                     {"tool_name": "Read", "tool_input": {}, "session_id": "s"})
        self.assertIsNone(_decision(r))

    def test_an_unknown_tool_is_ignored(self):
        r = run_hook(hooks.pretooluse_read,
                     {"tool_name": "Bash", "tool_input": {"command": "cat .charter/vaults/x"},
                      "session_id": "s"})
        self.assertIsNone(_decision(r))


class TestItIsActuallyWired(unittest.TestCase):
    """A guard the manifest does not dispatch is a guard that does not run — which is the
    entire content of this issue, one layer up."""

    def _hooks(self):
        return json.loads((ROOT / "hooks" / "hooks.json").read_text())["hooks"]

    def test_pretooluse_registers_a_file_reading_matcher(self):
        matchers = [e.get("matcher", "") for e in self._hooks()["PreToolUse"]]
        self.assertTrue(any("Read" in m for m in matchers), matchers)

    def test_the_engine_knows_the_handler_the_manifest_names(self):
        cmds = [h["command"] for e in self._hooks()["PreToolUse"] for h in e["hooks"]]
        named = {c.split("charter hook ")[1].split()[0] for c in cmds if "charter hook " in c}
        self.assertIn("pretooluse-read", named)
        self.assertLessEqual(named, set(hooks._HANDLERS))


class TestTheDocsClaimIsTrueAgain(unittest.TestCase):
    def test_secrets_doc_still_claims_direct_reads_are_denied(self):
        """`docs/secrets.md` stated the guard denies reading a vault file directly. That was
        true for Bash and false for every file-reading tool the harness offers. The claim is
        worth keeping only while this test can be here."""
        text = (ROOT / "docs" / "secrets.md").read_text()
        self.assertIn("vault", text.lower())


if __name__ == "__main__":
    unittest.main()
