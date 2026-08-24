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
import stat
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_secrets, config, hooks
from charter.secrets import registry
from tests._isolation import PersonaIso, run_hook

ROOT = Path(__file__).resolve().parents[1]


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


def _reason(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecisionReason", "")


class ReadGuardCase(PersonaIso):
    def read(self, path: str, tool: str = "Read", **extra):
        ti = {"file_path": path} if tool == "Read" else {"path": path, **extra}
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

    def test_the_browser_and_active_paths_are_covered_too(self):
        """`_VAULT_PATH_RE` already covers these for Bash; the two guards must not disagree
        about what counts as a vault."""
        for p in (".charter/browser", ".charter/active-persona"):
            self.assertEqual(_decision(self.read(p)), "deny", p)


class TestPathSpellingsAreDenied(ReadGuardCase):
    """`_VAULT_PATH_RE` was a literal, case-sensitive substring match (#431).

    `.charter//vaults/x.json`, `.charter/./vaults/x.json` and `.charter/vaults/x.json` are
    one file to `open()`. The Bash side allowed the first two — verified live, they printed
    a fabricated vault — and BOTH sides allowed `.Charter/vaults/x.json`, because the
    regex was case-sensitive and APFS is not. The harness normalises separators before this
    handler sees them, which is why only the case variant reached here; a guard that relies
    on its caller to normalise is a guard that stops existing the day the caller changes.
    """

    VARIANTS = (
        ".charter//vaults/db.json",
        ".charter/./vaults/db.json",
        ".charter/vaults/../vaults/db.json",
        ".CHARTER/vaults/db.json",
        ".Charter/Vaults/db.json",
        "/home/me/plane/.charter//vaults/db.json",
        ".charter//active-persona",
        ".charter/./browser/state.json",
    )

    def test_the_read_guard_denies_every_spelling(self):
        for p in self.VARIANTS:
            with self.subTest(path=p):
                self.assertEqual(_decision(self.read(p)), "deny")

    def test_the_bash_guard_denies_every_spelling(self):
        """The two must agree — this module's own argument. They did not."""
        for p in self.VARIANTS:
            with self.subTest(path=p):
                self.assertIsNotNone(hooks._leak_reason(f"cat {p}"), p)

    def test_the_registry_survives_the_normalisation(self):
        """The carve-out is load-bearing: `.charter/vaults.json` holds provider config and
        paths, never values, and folding case must not turn the registry into a vault."""
        for p in (".charter/vaults.json", ".charter//vaults.json", ".CHARTER/VAULTS.JSON"):
            with self.subTest(path=p):
                self.assertIsNone(_decision(self.read(p)))
                self.assertIsNone(hooks._leak_reason(f"cat {p}"))

    def test_a_traversal_INTO_the_vault_is_resolved(self):
        """`..` is resolved, not merely noticed. `.charter/x/../vaults/db.json` names no
        `.charter/vaults/` literally, and opens one."""
        self.assertEqual(_decision(self.read(".charter/x/../vaults/db.json")), "deny")
        self.assertIsNotNone(hooks._leak_reason("cat .charter/x/../vaults/db.json"))

    def test_a_traversal_back_OUT_of_the_vault_is_still_refused(self):
        """Deliberate, and the fail-CLOSED direction: `.charter/vaults/../notes.md` does
        resolve to an ordinary file, but the literal spelling names the vault directory and
        the guard refuses it rather than trusting a normalisation to say the path escaped.
        The cost is refusing a read nobody writes; the alternative is a subtraction rule
        inside a guard, which is where bypasses live."""
        self.assertEqual(_decision(self.read(".charter/vaults/../notes.md")), "deny")


class TestAMaterialisedSecretIsCovered(ReadGuardCase):
    """`secret cp` wrote plaintext outside `.charter/`, so no guard could see it (#423).

    Both guards matched `.charter/` and nothing else, and the `--reveal` denial told the
    agent to use `secret cp` — so the documented remedy was the bypass: `secret cp v k
    /tmp/x && cat /tmp/x` is a two-command, fully in-policy read of any vault value.
    """

    def setUp(self) -> None:
        super().setUp()
        registry.add_vault("tv", "plain-file", {"file": str(self.tmp / "tv.json")})
        # Straight to the provider: `cmd_secret_set` reads stdin, and the value here is a
        # fabricated string that must stay one — no real credential ever belongs in a
        # fixture, and an assertion that dumps one on failure is worse than no assertion.
        commands_secrets._provider("tv").set("API_TOKEN", "FABRICATED-NOT-A-REAL-VALUE")
        self.dest = self.tmp / "materialised" / "leak.txt"
        rc = commands_secrets.cmd_secret_cp(SimpleNamespace(
            vault="tv", key="API_TOKEN", dest=str(self.dest), persona=None))
        self.assertEqual(rc, 0)
        self.assertTrue(self.dest.exists())

    def test_reading_the_materialised_file_is_denied(self):
        self.assertEqual(_decision(self.read(str(self.dest))), "deny")

    def test_catting_the_materialised_file_is_denied(self):
        self.assertIsNotNone(hooks._leak_reason(f"cat {self.dest}"))

    def test_a_relative_spelling_from_the_sessions_cwd_is_denied(self):
        """Cover the class: the next input is `cd <dir> && cat leak.txt`, and an absolute
        match alone would be satisfied by a `cd`."""
        self.assertIsNotNone(
            hooks._leak_reason("cat materialised/leak.txt", cwd=str(self.tmp)))
        self.assertIsNotNone(
            hooks._leak_reason("cd materialised && cat leak.txt", cwd=str(self.tmp)))
        self.assertEqual(_decision(run_hook(hooks.pretooluse_read, {
            "tool_name": "Read", "tool_input": {"file_path": "materialised/leak.txt"},
            "cwd": str(self.tmp), "session_id": "s"})), "deny")

    def test_the_denial_does_not_name_the_value(self):
        """A denial is written into the trace and the transcript. The guard that exists to
        keep a credential out of both may not quote one."""
        r = self.read(str(self.dest))
        self.assertNotIn("FABRICATED-NOT-A-REAL-VALUE", _reason(r))

    def test_an_unrelated_file_beside_it_is_untouched(self):
        other = self.tmp / "materialised" / "notes.md"
        other.write_text("ordinary\n")
        self.assertIsNone(_decision(self.read(str(other))))
        self.assertIsNone(hooks._leak_reason(f"cat {other}"))

    def test_the_ledger_records_names_and_never_the_value(self):
        text = (config.STATE_DIR / "materialized.json").read_text()
        self.assertIn("API_TOKEN", text)
        self.assertNotIn("FABRICATED-NOT-A-REAL-VALUE", text)
        self.assertEqual(stat.S_IMODE((config.STATE_DIR / "materialized.json").stat().st_mode),
                         0o600)

    def test_the_cp_success_line_no_longer_recommends_reading_it_back(self):
        """The wording is part of the fix. The old `--reveal` denial said "use `secret
        cp`", and an agent's next move after a denial is whatever the denial names."""
        self.assertNotIn("secret exec`/`cp", hooks._REVEAL_REASON)
        self.assertIn("secret exec", hooks._REVEAL_REASON)
        self.assertNotIn("cp`", hooks._READ_REASON)


class TestTheDenialIsNotSwallowed(ReadGuardCase):
    """`except Exception: return 0` wrapped the `_deny` call itself (#438).

    A `BrokenPipeError` out of `_deny`'s own `print` is an `Exception`, so the handler
    answered 0 — an allow — on the one path where it had already decided to refuse. The
    Bash sibling has no such wrapper, so the two vault guards failed in opposite directions
    in a module that argues they must never disagree.
    """

    def test_a_broken_pipe_does_not_turn_a_deny_into_an_allow(self):
        deny = mock.Mock(side_effect=BrokenPipeError("closed"))
        with mock.patch.object(hooks, "_deny", deny):
            with self.assertRaises(BrokenPipeError):
                run_hook(hooks.pretooluse_read, {
                    "tool_name": "Read",
                    "tool_input": {"file_path": ".charter/vaults/db.json"},
                    "session_id": "s"})
        self.assertEqual(deny.call_count, 1)

    def test_a_failing_trace_still_leaves_the_deny_standing(self):
        """The trace is bookkeeping, not the verdict. It keeps its own handler so a full
        disk cannot un-deny a read — the direction the old blanket `except` had backwards.
        """
        with mock.patch.object(hooks, "_trace", side_effect=OSError("no space")):
            self.assertEqual(_decision(self.read(".charter/vaults/db.json")), "deny")

    def test_unparseable_input_still_fails_open_before_the_verdict(self):
        """The narrowing is not "remove the handler": a malformed payload must still not
        crash a turn. Only the DENY moved outside it."""
        r = run_hook(hooks.pretooluse_read,
                     {"tool_name": "Read", "tool_input": ["not", "a", "dict"],
                      "session_id": "s"})
        self.assertIsNone(_decision(r))


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
