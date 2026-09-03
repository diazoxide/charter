"""What the shim's payload makes charter's guards DECIDE — no fixture in the middle (#433).

Three rounds of this fix have been pinned by matching a STRING, and two of them were
bypassed by the same attack in a different spelling. Round one asserted the handler name.
Round two asserted the call site's text and each half of the tool-name table separately —
and `TOOL_NAMES["write"] = "write"` (a case change, satisfying `assertIn('"write"')`
twice over) passed the whole suite while `posttooluse` bailed at its
``tool_name not in ("Write", "Edit", "MultiEdit")`` test and the "you just wrote a secret
into committed memory" scan stopped firing on this harness. `tool_input: {}` on the
after-block passed the same way, for the same reason: nothing downstream of the shim was
ever asked a question.

So this module asks the only question that cannot be satisfied by agreement between two
fixtures: **run the generated plugin under a real JS runtime, take the payload it really
built, hand that exact dict to the real Python handler, and assert what the handler
DECIDED.** Every field the shim fills is then load-bearing by construction — `tool_name`
because the handler gates on it, `tool_input` because the handler reads the path out of
it, `cwd` because the containment rule resolves the plane root against it — and a field
blanked at the call site fails here whatever the tables say.

`tests/test_opencode_shim_dispatches_at_runtime.py` is the sibling that asks which handler
was spawned and what the payload contained. This one asks whether that payload is enough
to make the guard refuse, tally or warn. Skipped, loudly, when no runtime is installed;
the static assertions in `tests/test_opencode_shim.py` are the floor for that case.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from charter import config, hooks
from charter.harness import opencode
from tests._isolation import PersonaIso, PlaneIso, run_hook

_RUNTIME = shutil.which("bun") or shutil.which("node")
_DRIVER = Path(__file__).parent / "fixtures" / "opencode_driver.mjs"

#: What the shim reads back from `charter hook`. Irrelevant to every assertion here — the
#: subject is the payload going OUT — but the shim must be handed something parseable or
#: it takes its fail-open path and the test would prove nothing.
_ALLOW = '{"hookSpecificOutput":{"hookEventName":"PreToolUse"}}'


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


def _note(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("additionalContext", "")


@unittest.skipIf(_RUNTIME is None, "neither bun nor node is installed")
class ShimPayloadDrivesTheRealHandler(PlaneIso):
    """One helper: the payload the real shim builds for one real opencode tool call."""

    def setUp(self) -> None:
        super().setUp()
        self.js = Path(tempfile.mkdtemp(prefix="charter-oc-e2e-"))
        self.addCleanup(shutil.rmtree, self.js, True)
        (self.js / "charter.mjs").write_text(opencode.SHIM)
        (self.js / "drive.mjs").write_text(_DRIVER.read_text())

    def payload(self, event: str, tool: str, args: dict, *, output: str = "ran",
                directory: Path | None = None, session: str = "s1") -> dict:
        """Run the generated plugin for one tool call; return what it put on stdin.

        Nothing is constructed here. If the shim spawns no handler at all — which is what
        a `constructor`-shaped id used to do, and what a broken call site does — there is
        no payload to return and the test fails on that rather than on a stand-in.
        """
        scenario = {"event": event, "tool": tool, "args": args, "sessionID": session,
                    "directory": str(directory if directory is not None else self.tmp),
                    "reply": _ALLOW, "output": output}
        proc = subprocess.run([_RUNTIME, "drive.mjs"], input=json.dumps([scenario]),
                              capture_output=True, text=True, timeout=120, cwd=self.js)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        calls = json.loads(proc.stdout)[0]["calls"]
        self.assertEqual(len(calls), 1,
                         f"the shim spawned {len(calls)} handlers for `{tool}`, not one")
        return calls[0]["payload"]


class TheVaultGuardRefusesWhatTheShimReallySends(ShimPayloadDrivesTheRealHandler):
    """#433's own case, end to end: opencode's `read` on a vault must be denied."""

    def test_a_read_of_a_vault_file_is_denied(self):
        p = self.payload("before", "read", {"filePath": ".charter/vaults/devops.json"})
        self.assertEqual(_decision(run_hook(hooks.pretooluse_read, p)), "deny")

    def test_a_grep_into_the_vault_directory_is_denied(self):
        p = self.payload("before", "grep",
                         {"path": ".charter/vaults", "pattern": "token"})
        self.assertEqual(_decision(run_hook(hooks.pretooluse_read, p)), "deny")


class TheSecretScanFiresOnWhatTheShimReallySends(ShimPayloadDrivesTheRealHandler):
    """The warning both round-two bypasses killed, asserted through the shim.

    `posttooluse` gates on ``tool_name in ("Write", "Edit", "MultiEdit")`` and then reads
    the path out of `tool_input`. A shim that translates `write` to `write` fails the
    first test; one that sends `tool_input: {}` fails the second. Both were green on the
    whole suite, and both are red here.
    """

    def _leak(self) -> Path:
        name = self.make_persona("scanned")
        p = config.PERSONAS_DIR / name / "memory" / "leak.md"
        p.parent.mkdir(parents=True, exist_ok=True)
        # Not a real credential: the `ghp_` prefix plus the right length is the whole of
        # what the scanner matches, and this string authenticates to nothing.
        p.write_text("token: ghp_" + "a" * 36)
        return p

    def test_a_write_of_a_secret_into_committed_memory_is_warned_about(self):
        leak = self._leak()
        p = self.payload("after", "write",
                         {"filePath": str(leak), "content": leak.read_text()})
        self.assertIn("SECURITY", _note(run_hook(hooks.posttooluse, p)))

    def test_an_edit_that_leaves_a_secret_in_committed_memory_is_warned_about(self):
        """The `edit` twin. `TOOL_NAMES["edit"] = "edit"` is a separate one-word mutation
        from the `write` one, and a test that covered only `write` would leave it."""
        leak = self._leak()
        p = self.payload("after", "edit",
                         {"filePath": str(leak), "oldString": "x",
                          "newString": leak.read_text()})
        self.assertIn("SECURITY", _note(run_hook(hooks.posttooluse, p)))


class TheTalliesRecordWhatTheShimReallySends(ShimPayloadDrivesTheRealHandler):
    """`posttooluse_skill` gates on ``tool_name != "Skill"`` and `posttooluse_dispatch` on
    ``tool_name not in ("Task", "Agent")``, so both are one table entry from silence — the
    silence they were in for four releases, for a different reason."""

    def test_a_skill_invocation_is_tallied(self):
        from charter import skilluse

        name = self.make_persona("tallied")
        config.ACTIVE_PERSONA_FILE.parent.mkdir(parents=True, exist_ok=True)
        config.ACTIVE_PERSONA_FILE.write_text(name)
        p = self.payload("after", "skill", {"name": "browser"})
        run_hook(hooks.posttooluse_skill, p)
        self.assertEqual(skilluse.by_persona(name).get("browser"), 1)

    def test_a_dispatch_is_tallied(self):
        from charter import dispatch

        name = self.make_persona("dispatched")
        p = self.payload("after", "task",
                         {"subagent_type": name, "prompt": "go", "description": "d"})
        run_hook(hooks.posttooluse_dispatch, p)
        self.assertEqual(dispatch.tally().get(name), 1)


class TheContainmentRuleResolvesTheShimsCwd(ShimPayloadDrivesTheRealHandler):
    """`cwd` is the field with no other witness, and blanking it was green on the suite.

    `_plane_root_branch_reason` compares the command's working directory against the plane
    root. A shim that sends ``cwd: ""`` produces a guard that runs, resolves nothing, and
    allows every branch move in the shared tree — which the news entry for this fix claims
    is refused on opencode exactly as it is under Claude Code. This is that claim.
    """

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        self.root = Path(config.ROOT)
        self._git("init", "-q", "-b", "main", str(self.root))
        self._git("-C", str(self.root), "config", "user.email", "t@e")
        self._git("-C", str(self.root), "config", "user.name", "t")
        self._git("-C", str(self.root), "config", "commit.gpgsign", "false")
        (self.root / "README").write_text("plane\n")
        self._git("-C", str(self.root), "add", "-A")
        self._git("-C", str(self.root), "commit", "-qm", "init")
        self.elsewhere = config.WORKSPACES_DIR / "ws" / "svc"
        self.elsewhere.mkdir(parents=True, exist_ok=True)
        self._git("init", "-q", "-b", "main", str(self.elsewhere))

    @staticmethod
    def _git(*args):
        return subprocess.run(["git", *args], check=True, capture_output=True, text=True)

    def test_a_branch_move_in_the_plane_root_is_refused(self):
        p = self.payload("before", "bash", {"command": "git checkout -b feature"},
                         directory=self.root)
        self.assertEqual(_decision(run_hook(hooks.pretooluse, p)), "deny")

    def test_the_same_command_in_a_workspace_clone_is_allowed(self):
        """The other direction, so the test above cannot be satisfied by a guard that
        refuses everything — which would be a cage, not containment."""
        p = self.payload("before", "bash", {"command": "git checkout -b feature"},
                         directory=self.elsewhere)
        self.assertIsNone(_decision(run_hook(hooks.pretooluse, p)))


if __name__ == "__main__":
    unittest.main()
