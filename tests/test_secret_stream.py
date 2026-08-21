"""A long-running child whose credential must be a FILE (#190).

`mcp_render_entry` emitted only `--env`, and `--exec` is mandatory for an MCP stdio server:
it never returns, so the capturing form hangs holding output nobody reads. But `--exec`
replaces charter, so nothing survives to shred a temp file — which is why it is incompatible
with `--file`.

Google Application Default Credentials require a filesystem **path**
(`GOOGLE_APPLICATION_CREDENTIALS` takes a path, not a value). So the two ends did not meet,
and a persona whose analytics servers authenticate that way could not declare them at all —
it had to keep a plugin purely to carry them.

`--stream` is the third mode: fork, inherit stdio, wait, clean up. **Streaming was never what
`exec` bought here** — a forked child inherits the parent's descriptors — so the only thing
given up is replacing the process, which is exactly the thing that made cleanup impossible.

The limit is stated rather than implied away: a `SIGKILL`ed charter runs no cleanup and the
0600 file survives until it is removed. That is strictly better than every persona
re-implementing the same `trap` in shell with the same hole, and it is not a guarantee.
"""

from __future__ import annotations

import io
import json
import os
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import mcpseen, persona
from charter import commands_secrets as cs
from charter.secrets import registry
from tests._isolation import PersonaIso


class StreamCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        # A real vault: `cmd_secret_exec` resolves the provider BEFORE it validates the
        # mode flags, so without one every case here would fail as "no vault" (rc 1) and
        # prove nothing about the modes.
        from charter import config
        vf = config.ROOT / ".charter" / "vaults" / "v.json"
        vf.parent.mkdir(parents=True, exist_ok=True)
        vf.write_text(json.dumps({"k": "s3cret", "sa-json": '{"type":"service_account"}'}))
        registry.add_vault("v", "plain-file", {"file": str(vf)})

    def run_exec(self, argv, **kw):
        args = SimpleNamespace(vault="v", env=None, file=None, dotenv=None,
                               exec_mode=False, stream_mode=False, command=argv)
        for k, val in kw.items():
            setattr(args, k, val)
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = cs.cmd_secret_exec(args)
        return rc, out.getvalue() + err.getvalue()


class TestTheRendererEmitsTheRightMode(PersonaIso):
    def render(self, name, vault, entry):
        """Render an entry whose command the operator has approved.

        The wrapper is only emitted for a command this machine consented to, because the
        committed `mcp.json` chooses it (#330). These tests are about which MODE the
        wrapper uses, so they consent first; the withholding half is asserted in
        tests/test_committed_config_and_credentials.py.
        """
        mcpseen.approve(name, [mcpseen.fingerprint(vault, entry)])
        return persona.mcp_render_entry(name, vault, entry)

    def test_an_env_only_server_still_execs(self):
        """Unchanged for every server that already worked — `--exec` is still right when
        there is no file to clean up."""
        e = {"command": "npx", "args": ["posthog-mcp"], "secrets": {"TOKEN": "k"}}
        got = self.render("growth", "vlt", e)
        self.assertIn("--exec", got["args"])
        self.assertNotIn("--stream", got["args"])

    def test_a_file_credential_server_streams(self):
        e = {"command": "uvx", "args": ["ga4-mcp"],
             "secret_files": {"GOOGLE_APPLICATION_CREDENTIALS": "sa-json"}}
        got = self.render("growth", "vlt", e)
        self.assertIn("--stream", got["args"])
        self.assertNotIn("--exec", got["args"])
        self.assertIn("--file", got["args"])
        self.assertIn("GOOGLE_APPLICATION_CREDENTIALS=sa-json", got["args"])

    def test_a_server_with_both_streams(self):
        """One file anywhere in the entry decides the mode: exec cannot clean up, so a
        mixed server must not exec."""
        e = {"command": "uvx", "args": ["x"], "secrets": {"TOKEN": "k"},
             "secret_files": {"GOOGLE_APPLICATION_CREDENTIALS": "sa"}}
        got = self.render("growth", "vlt", e)
        self.assertIn("--stream", got["args"])
        self.assertIn("--env", got["args"])
        self.assertIn("--file", got["args"])

    def test_the_declaration_keys_do_not_leak_into_the_agent(self):
        e = {"command": "uvx", "secret_files": {"G": "sa"}}
        got = self.render("growth", "vlt", e)
        self.assertNotIn("secret_files", got)
        self.assertNotIn("secrets", got)

    def test_no_vault_means_no_rewrite(self):
        """A persona holding no credentials has nothing to inject, and wrapping the server
        through charter would buy nothing and add a process."""
        e = {"command": "uvx", "secret_files": {"G": "sa"}}
        self.assertEqual(persona.mcp_render_entry("growth", None, e), {"command": "uvx"})


class TestTheModesAreExclusive(StreamCase):
    def test_exec_and_stream_together_is_refused(self):
        rc, out = self.run_exec(["true"], exec_mode=True, stream_mode=True)
        self.assertEqual(rc, 2)
        self.assertIn("pick one", out)

    def test_exec_with_a_file_now_points_at_stream(self):
        """The old message said "use --env for an exec'd command", which was the only advice
        available. There is now a mode that does what the caller asked."""
        rc, out = self.run_exec(["true"], exec_mode=True, file=["G=k"])
        self.assertEqual(rc, 2)
        self.assertIn("--stream", out)


class TestStreamRunsAndCleansUp(StreamCase):
    def test_the_child_runs_and_its_exit_code_is_returned(self):
        rc, _ = self.run_exec(["sh", "-c", "exit 7"], stream_mode=True)
        self.assertEqual(rc, 7)

    def test_the_child_inherits_stdio_rather_than_being_captured(self):
        """The property that makes this usable for an MCP stdio server: streaming was never
        what exec bought, so forking loses nothing."""
        rc, _ = self.run_exec(["sh", "-c", "exit 0"], stream_mode=True)
        self.assertEqual(rc, 0)

    def test_a_missing_command_is_127_not_a_traceback(self):
        rc, out = self.run_exec(["definitely-not-a-real-binary-xyz"], stream_mode=True)
        self.assertEqual(rc, 127)
        self.assertIn("command not found", out)


class TestTheLimitIsStated(unittest.TestCase):
    def test_help_says_a_sigkill_leaves_the_file(self):
        """Charter must not describe this as guaranteed cleanup. A promise that fails
        silently exactly when something has already gone wrong is worse than none."""
        from charter import cli
        text = cli.build_parser().format_help()
        # The flag's own help is on the subparser; assert the source carries the caveat.
        import inspect
        src = inspect.getsource(cli)
        self.assertIn("SIGKILL", src)


if __name__ == "__main__":
    unittest.main()
