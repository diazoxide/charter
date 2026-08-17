"""The repair hint has to produce a launcher that actually starts.

0.35.0 told the operator to "set this server's command to `charter`, leaving its args
unchanged". Applied to the registration that prompted #197, that advice **half-works**, and
the half that fails is silent in the same way the original breakage was.

The registration is::

    command: <umbrella>/bin/edm
    args:    secret exec elastic-logs-master --env … --exec -- node <server>

`charter secret exec <vault>` has to find a plane holding that vault. The old command was an
absolute path into the umbrella; a bare `charter` resolves its plane from **the launching
directory**, and an `mcpServers` entry at the top level of `~/.claude.json` is user-scope —
it launches for every project, most of which are not that plane. So the server starts, fails
to find the vault, and the tools are missing again.

`$CHARTER_ROOT` is exactly the anchor for this, and it belongs in the hint whenever the args
show charter is being asked to open a vault. Verified against the real registration: with
the env var the credentials inject, without it the vault is not found.

Also: `0 launcher(s) resolve` was reported for a plane whose only MCP server had just been
repaired to a bare command. True — zero *absolute* launchers were checked — but read as
"nothing is registered", which is a different claim.
"""

from __future__ import annotations

import json
import os
import unittest
from pathlib import Path

from charter import doctor
from tests._isolation import PersonaIso


class HintCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        self.home = self.tmp / "home"
        self.home.mkdir(exist_ok=True)
        self._real_home = os.environ.get("HOME")
        os.environ["HOME"] = str(self.home)
        self.addCleanup(self._restore_home)

    def _restore_home(self) -> None:
        if self._real_home is None:
            os.environ.pop("HOME", None)
        else:
            os.environ["HOME"] = self._real_home

    def write(self, servers: dict) -> None:
        (self.home / ".claude.json").write_text(json.dumps({"mcpServers": servers}))

    def shim(self, args: list[str] | None = None) -> str:
        self.write({"es": {"command": str(self.tmp / "umbrella" / "bin" / "edm"),
                           "args": args or []}})
        return doctor.check_mcp_launchers().hint or ""


class TestAVaultOpeningLauncherIsAnchored(HintCase):
    ARGS = ["secret", "exec", "elastic-logs-master", "--env", "A=b", "--exec", "--", "node", "x"]

    def test_the_hint_names_charter_root(self):
        """Without it the repaired server resolves its plane from whatever project happens
        to be open — and a user-scope entry launches for all of them."""
        self.assertIn("CHARTER_ROOT", self.shim(self.ARGS))

    def test_the_hint_still_names_the_replacement_command(self):
        self.assertIn("`charter`", self.shim(self.ARGS))

    def test_the_hint_names_the_vault_it_must_reach(self):
        """"Set CHARTER_ROOT" is not actionable without saying which plane — and the vault
        name is the only clue charter has to which one that is."""
        self.assertIn("elastic-logs-master", self.shim(self.ARGS))

    def test_a_launcher_that_opens_no_vault_is_not_told_to_anchor(self):
        """Advice that applies to everything is read as boilerplate and then not read. A
        server charter merely launches has no plane to resolve."""
        hint = self.shim(["--port", "3000"])
        self.assertNotIn("CHARTER_ROOT", hint)
        self.assertIn("`charter`", hint)

    def test_an_unrelated_missing_launcher_is_untouched(self):
        self.write({"x": {"command": str(self.tmp / "opt" / "node"), "args": []}})
        hint = doctor.check_mcp_launchers().hint or ""
        self.assertNotIn("CHARTER_ROOT", hint)
        self.assertNotIn("rename", hint.lower())


class TestTheHealthyDetailIsHonest(HintCase):
    def test_nothing_absolute_does_not_read_as_nothing_registered(self):
        """`0 launcher(s) resolve` was shown for a plane with a working server on a bare
        command. Zero were *checked*; none were broken. Saying "0 resolve" states the second
        as if it were the first."""
        self.write({"es": {"command": "charter", "args": ["secret", "exec", "v"]}})
        r = doctor.check_mcp_launchers()
        self.assertEqual(r.status, doctor.OK)
        self.assertNotIn("0 launcher", r.detail)

    def test_it_still_counts_what_it_did_check(self):
        p = self.tmp / "real"
        p.write_text("#!/bin/sh\n")
        p.chmod(0o755)
        self.write({"a": {"command": str(p)}, "b": {"command": "npx"}})
        self.assertIn("1", doctor.check_mcp_launchers().detail)

    def test_no_servers_at_all_still_says_so(self):
        self.write({})
        self.assertIn("no MCP servers", doctor.check_mcp_launchers().detail)


if __name__ == "__main__":
    unittest.main()
