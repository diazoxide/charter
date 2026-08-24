"""`edm secret exec --exec`: replace the process so a streaming child works.

Capturing a child (stdout=PIPE) buffers until it exits, which deadlocks any
long-running process — an MCP stdio server never completes its handshake. The
``--exec`` path hands the process over via ``os.execvpe`` instead, so fds 0/1/2
are inherited untouched. These tests pin the contract without actually exec'ing
(which would replace the test runner).
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr
from types import SimpleNamespace

from charter import commands_secrets
from tests._isolation import PersonaIso


class _StubProvider:
    """Stands in for a vault; records which keys were asked for."""

    def __init__(self, values: dict[str, str]):
        self.values = values
        self.asked: list[str] = []

    def get(self, key: str) -> str:
        self.asked.append(key)
        return self.values[key]


class SecretExecMode(PersonaIso):
    """`PersonaIso`, not a bare `TestCase`: `cmd_secret_exec` records the hand-out in the
    session trace (#441), and a fixture that has not redirected `config` appends that row
    to the plane the suite resolved to — the developer's own (#372, #402)."""

    def setUp(self) -> None:
        super().setUp()
        self.provider = _StubProvider({"kibana-user": "svc_logs",
                                       "kibana-pass": "s3cr3t-value"})
        self._orig_provider = commands_secrets._provider
        self._orig_execvpe = commands_secrets.os.execvpe
        commands_secrets._provider = lambda _name: self.provider
        self.execs: list[tuple] = []

        def _fake_execvpe(file, argv, env):
            self.execs.append((file, list(argv), dict(env)))

        commands_secrets.os.execvpe = _fake_execvpe
        self.addCleanup(self._restore)

    def _restore(self) -> None:
        commands_secrets._provider = self._orig_provider
        commands_secrets.os.execvpe = self._orig_execvpe

    @staticmethod
    def _args(**kw):
        base = {"vault": "elastic-logs-master", "env": None, "file": None,
                "dotenv": None, "command": [], "exec_mode": False}
        base.update(kw)
        return SimpleNamespace(**base)

    def test_exec_replaces_process_with_secrets_in_env(self):
        """--exec hands off via execvpe, with the resolved secrets in the child env."""
        rc = commands_secrets.cmd_secret_exec(self._args(
            env=["ELASTIC_USERNAME=kibana-user", "ELASTIC_PASSWORD=kibana-pass"],
            command=["--", "node", "server.js"], exec_mode=True))

        self.assertEqual(len(self.execs), 1, "expected exactly one execvpe hand-off")
        file, argv, env = self.execs[0]
        self.assertEqual(file, "node")
        self.assertEqual(argv, ["node", "server.js"], "the leading `--` must be stripped")
        self.assertEqual(env["ELASTIC_USERNAME"], "svc_logs")
        self.assertEqual(env["ELASTIC_PASSWORD"], "s3cr3t-value")
        # execvpe never returns in reality. The stub does, so this also pins that
        # control must NOT fall through into the capturing path and re-run the command.
        self.assertEqual(rc, 0)

    def test_without_exec_the_process_is_not_replaced(self):
        """Default path still captures + redacts — no execvpe hand-off."""
        commands_secrets.cmd_secret_exec(self._args(
            env=["FOO=kibana-user"], command=["--", "true"]))
        self.assertEqual(self.execs, [])

    def test_exec_refuses_file_injection(self):
        """--file writes a temp file that exec would orphan, so the combination is refused."""
        err = io.StringIO()
        with redirect_stderr(err):
            rc = commands_secrets.cmd_secret_exec(self._args(
                file=["KUBECONFIG=kibana-pass"], command=["--", "kubectl"], exec_mode=True))

        self.assertEqual(rc, 2)
        self.assertEqual(self.execs, [], "must not exec when the guard trips")
        self.assertIn("--file cannot be combined with --exec", err.getvalue())
        self.assertEqual(self.provider.asked, [],
                         "must refuse before resolving any secret")

    def test_exec_missing_command_is_rejected(self):
        """An empty command is caught before any secret is resolved."""
        err = io.StringIO()
        with redirect_stderr(err):
            rc = commands_secrets.cmd_secret_exec(self._args(command=[], exec_mode=True))
        self.assertEqual(rc, 2)
        self.assertEqual(self.execs, [])
        self.assertEqual(self.provider.asked, [])


if __name__ == "__main__":
    unittest.main()
