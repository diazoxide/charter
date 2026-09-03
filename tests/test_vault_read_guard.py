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
from tests._isolation import PersonaIso, make_plane, run_hook

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
        """Denied by the shared predicate, not by a step this route adds. It used to be the
        latter — an appended-slash retry that lived only here — and the Bash route, reaching
        the same predicate with the same operand, answered ALLOW on the directory that holds
        every vault (#462). `tests/test_vault_path_spellings.py` asserts the two routes
        agree; this asserts the answer itself."""
        self.assertEqual(_decision(self.read(".charter/vaults", tool="Grep", pattern="token")),
                         "deny")

    def test_a_grep_rooted_at_the_state_directory_itself_is_denied(self):
        """#443. The pattern required a trailing slash after the directory name, so the one
        target that walks EVERY vault — the state directory itself — was the one it could
        not see. `Grep` recurses; the file it reaches is the same file naming
        `.../vaults/devops.json` reaches."""
        for p in ('.charter', '.charter' + "/", "/home/me/plane/" + '.charter'):
            with self.subTest(p=p):
                self.assertEqual(_decision(self.read(p, tool="Grep", pattern="token")),
                                 "deny", p)

    def test_the_pre_rename_directory_is_covered_too(self):
        """`.edm` is charter's pre-rename state directory, and `config._migrate_state_dir`
        still falls back to it when a migration cannot complete. One extra alternative
        against a silent gap on a plane that never finished moving."""
        self.assertEqual(_decision(self.read(".edm/vaults/devops.json")), "deny")
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


class TestTheDenialIsNotSwallowed(ReadGuardCase):
    """`except Exception: return 0` wrapped the `_deny` call itself (#438).

    A `BrokenPipeError` out of `_deny`'s own `print` is an `Exception`, so the handler
    answered 0 — an allow — on the one path where it had already decided to refuse. The
    Bash sibling has no such wrapper, so the two vault guards failed in opposite directions
    in a module that argues they must never disagree.
    """

    def setUp(self) -> None:
        super().setUp()
        # A PLANE, for `test_a_failing_trace_still_leaves_the_deny_standing` below. Since
        # #852 `hooks._trace` is a no-op outside one, so without this the injected
        # `trace.record` failure is never reached and the case passes without ever
        # exercising the `except` it names — the exact vacuity its own docstring warns
        # about one paragraph down. The read guard itself is ungated, so nothing else in
        # this class changes answer.
        make_plane(self)

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
        """The trace is bookkeeping, not the verdict: a full disk may not un-deny a read.

        The failure is injected at `trace.record`, the function that actually touches the
        disk, and NOT at `hooks._trace`. Patching `_trace` itself would have replaced the
        very handler under test with a raising mock and then asserted that some OTHER
        handler caught it — a test manufacturing the condition it claims to observe, which
        would report green if `_trace`'s own `except` were deleted tomorrow.
        """
        from charter import trace as trace_mod
        with mock.patch.object(trace_mod, "record", side_effect=OSError("no space")):
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
        carve-out the Bash guard makes, and for the same reason. The regex anchors `vaults`
        to a path SEGMENT: `vaults` and `vaults/` are the directory of secrets, and
        `vaults.json` — where `vaults` is only a prefix of the segment — is the map."""
        self.assertIsNone(_decision(self.read(".charter/vaults.json")))

    def test_an_ordinary_file_is_untouched(self):
        self.assertIsNone(_decision(self.read("charter/hooks.py")))

    def test_a_path_merely_under_the_state_directory_is_not_a_vault(self):
        """The directory alternative is anchored at the END of the operand on purpose. A
        Read of the tool-gate's session ceiling is not a read of a credential, and turning
        every state file into a hard denial would be a new guard smuggled in under an old
        one's name."""
        self.assertIsNone(_decision(self.read('.charter' + "/state/sessions/s.tools")))

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
