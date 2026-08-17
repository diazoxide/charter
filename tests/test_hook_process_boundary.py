"""Drive the hooks the way Claude Code does: spawn the CLI, feed stdin, read stdout.

Every other guard test calls the Python function directly. That leaves the path
Claude Code actually uses — `hooks.json` → `charter hook <name>` → stdin JSON →
stdout JSON → exit code — covered only in the middle.

That gap is not hypothetical. `secret exec … -- <cmd>` shipped broken on Python
3.11 for every release up to 0.2.6: the guard logic was correct and tested, and
what failed was argv handling between the entrypoint and the logic. A hook has
the same shape, so it gets the same treatment here.

These spawn a subprocess per case and are therefore the slowest tests in the
suite. That is the point — anything cheaper does not cross the boundary.
"""

from __future__ import annotations

import json
import subprocess
import os
import sys
import tempfile
import unittest
from pathlib import Path

_REPO = Path(__file__).resolve().parent.parent


def _hook(name: str, payload: dict | None = None) -> subprocess.CompletedProcess:
    """Run `charter hook <name>` as a real process, exactly as the manifest does.

    ``$CHARTER_ROOT`` is pinned to a throwaway directory, and that is not tidiness. These
    are SUBPROCESSES, so `PersonaIso` cannot reach them — it redirects module globals in
    this interpreter, not in a child. The child resolves its own plane by walking up from
    `cwd`, and since `find_root` began hopping outward through `workspaces/` (0.37.0) a
    checkout that happens to sit inside somebody's plane resolves to THAT plane. A handler
    that writes anything — a trace row, a guard-seen mark — then writes it into the
    developer's real control plane, from a test run.
    """
    root = tempfile.mkdtemp(prefix="charter-hookproc-")
    (Path(root) / "charter.toml").write_text("schema = 1\n")
    return subprocess.run(
        [sys.executable, "-m", "charter", "hook", name],
        input=json.dumps(payload if payload is not None else {}),
        capture_output=True, text=True, cwd=_REPO,
        env={**os.environ, "CHARTER_ROOT": root},
    )


def _manifest_hooks() -> list[tuple[str, str]]:
    """(event, command) for every handler the shipped manifest declares."""
    doc = json.loads((_REPO / "hooks" / "hooks.json").read_text())
    return [(event, h["command"])
            for event, groups in doc.get("hooks", {}).items()
            for g in groups for h in g.get("hooks", [])]


class GuardAcrossTheProcessBoundary(unittest.TestCase):
    def test_a_real_bypass_denies_through_the_real_entrypoint(self):
        """Positive control: the shape Claude Code consumes, from a real process."""
        p = _hook("pretooluse", {"tool_name": "Bash", "tool_input": {
            "command": "git -c core.sshCommand=/x push"}})
        self.assertEqual(p.returncode, 0, p.stderr)
        out = json.loads(p.stdout)                      # must be well-formed JSON
        spec = out["hookSpecificOutput"]                # must be correctly nested
        self.assertEqual(spec["hookEventName"], "PreToolUse")
        self.assertEqual(spec["permissionDecision"], "deny")
        self.assertTrue(spec.get("permissionDecisionReason"), "a denial must say why")

    def test_an_ordinary_command_produces_no_decision(self):
        """Negative control. Without it the suite passes just as well if the guard
        denied everything — which would be a catastrophic pass."""
        p = _hook("pretooluse", {"tool_name": "Bash", "tool_input": {"command": "git status"}})
        self.assertEqual(p.returncode, 0, p.stderr)
        decision = (json.loads(p.stdout).get("hookSpecificOutput", {})
                    if p.stdout.strip() else {}).get("permissionDecision")
        self.assertIsNone(decision, f"git status must not be denied, got {decision!r}")

    def test_a_secret_reveal_denies_too(self):
        """Second positive control on a different rule, so the first isn't load-bearing."""
        p = _hook("pretooluse", {"tool_name": "Bash", "tool_input": {
            "command": "charter secret get qa token --reveal --force"}})
        self.assertEqual(p.returncode, 0, p.stderr)
        spec = json.loads(p.stdout).get("hookSpecificOutput", {})
        self.assertEqual(spec.get("permissionDecision"), "deny")

    def test_stdout_is_json_or_empty_never_prose(self):
        """Claude Code parses stdout. Anything else — a traceback, a stray print —
        is a broken hook even when the exit code is 0."""
        for payload in ({}, {"tool_name": "Bash"}, {"tool_input": {}},
                        {"tool_input": {"command": "ls"}}):
            with self.subTest(payload=payload):
                p = _hook("pretooluse", payload)
                if p.stdout.strip():
                    json.loads(p.stdout)  # raises if it isn't JSON

    def test_every_manifest_hook_fails_open_on_an_empty_payload(self):
        """A crashing hook breaks the tool call, not just the guard. Whatever the
        input, the handler must exit 0."""
        engine = {c.split("charter hook ")[1].split()[0]
                  for _e, c in _manifest_hooks() if "charter hook " in c}
        self.assertTrue(engine, "manifest declares no engine hooks — did it move?")
        for name in sorted(engine):
            with self.subTest(hook=name):
                p = _hook(name, {})
                self.assertEqual(p.returncode, 0,
                                 f"{name} exited {p.returncode}: {p.stderr[:300]}")

    def test_every_manifest_hook_survives_malformed_stdin(self):
        """Not JSON at all — a hook must still not take the tool call down."""
        engine = {c.split("charter hook ")[1].split()[0]
                  for _e, c in _manifest_hooks() if "charter hook " in c}
        for name in sorted(engine):
            with self.subTest(hook=name):
                root = tempfile.mkdtemp(prefix="charter-hookproc-")
                (Path(root) / "charter.toml").write_text("schema = 1\n")
                p = subprocess.run([sys.executable, "-m", "charter", "hook", name],
                                   input="not json at all", capture_output=True,
                                   text=True, cwd=_REPO,
                                   env={**os.environ, "CHARTER_ROOT": root})
                self.assertEqual(p.returncode, 0,
                                 f"{name} exited {p.returncode}: {p.stderr[:300]}")

    def test_every_engine_hook_named_in_the_manifest_actually_exists(self):
        """A typo'd subcommand is silent: the handler just never runs."""
        for name in sorted({c.split("charter hook ")[1].split()[0]
                            for _e, c in _manifest_hooks() if "charter hook " in c}):
            with self.subTest(hook=name):
                p = _hook(name, {})
                self.assertNotIn("invalid choice", p.stderr,
                                 f"hooks.json names `charter hook {name}`, which the CLI "
                                 f"does not implement")


if __name__ == "__main__":
    unittest.main()
