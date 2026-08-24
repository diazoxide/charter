"""`uses:`/`borrows:` gates tools. It does not gate vault reach — and now says so (#440).

Two halves of one frontmatter sentence, enforced differently: `persona.effective_tools`
consults `borrows:`/`uses:` and the tool-gate refuses anything outside it, while
`commands_secrets._provider` takes the vault name straight from the argument list. A
persona declaring neither can name any registered vault, four different ways, with no
refusal and no warning.

**That asymmetry is the defect, not the reach itself.** Every persona runs as the same uid
against the same `.charter/vaults/` files — `tests/test_vault_read_guard.py` says exactly
this about the sibling case — so a check inside `charter` would be a boundary charter
cannot hold, and the paths that would have to carry it are not all interactive: an MCP
server is launched as `charter secret exec <vault> --exec -- <server>`
(`persona.mcp_render_entry`), by the harness, with no tty and no guarantee about which
persona is active. Enforcing there would break credentialed MCP servers the first time a
session adopted a persona other than the owner.

So the fix is the disclosure, and these tests hold both ends of it: the behaviour as it
actually is, and the sentence in `docs/personas.md` that describes it. If someone later
decides to enforce, the first two tests are the ones to invert — deliberately, in one
place, rather than discovering the gap from a reader who assumed the other half.

No real credential appears here: the fabricated value never leaves this file, and nothing
below asserts on it.
"""
from __future__ import annotations

import io
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands_secrets, config, persona
from charter.secrets import registry
from tests._isolation import PersonaIso

#: Fabricated. Registered so `keys()` has something to list; never asserted on, never
#: printed — the reviewer's rule is that a failing assertion may not carry a secret, and
#: the cheapest way to keep it is to have no secret worth carrying.
FABRICATED = "fabricated-not-a-real-token"


class VaultReachCase(PersonaIso):
    def setUp(self):
        super().setUp()
        self.make_persona("devops", role="DevOps", vault="devops", tools="kubectl")
        # `writer` declares neither `uses:` nor `borrows:` — no claim on anything of
        # devops's, by either half of the sentence.
        self.make_persona("writer", role="Writer", vault="none")
        vf = Path(config.VAULTS_DIR) / "devops.json"
        vf.parent.mkdir(parents=True, exist_ok=True)
        registry.add_vault("devops", "plain-file", {"file": str(vf)}, persona="devops")
        registry.provider_for("devops").set("API_TOKEN", FABRICATED)
        persona.set_active("writer")

    @staticmethod
    def _args(**kw):
        return SimpleNamespace(**kw)


class TestTheTwoHalvesDisagree(VaultReachCase):
    def test_the_tool_half_is_enforced(self):
        """`kubectl` is devops's. `writer` borrowed nothing, so it is not writer's."""
        self.assertNotIn("kubectl", persona.effective_tools("writer"))

    def test_the_vault_half_is_not(self):
        """The behaviour as it is. `writer` names devops's vault and charter serves it —
        key NAMES only, which is what `secret list` was always for."""
        buf = io.StringIO()
        with redirect_stdout(buf):
            rc = commands_secrets.cmd_secret_list(self._args(vault="devops"))
        self.assertEqual(rc, 0)
        self.assertIn("API_TOKEN", buf.getvalue())

    def test_the_persona_scoped_wrapper_is_the_same_answer(self):
        """`--persona X` resolves to *being* X; it consults no grant on the way."""
        self.assertEqual(persona.resolve_active("devops"), "devops")
        from charter import commands_persona
        self.assertEqual(commands_persona._resolve_vault(
            self._args(persona="devops")), "devops")


class TestTheDocsSayWhichIsWhich(unittest.TestCase):
    """The half that is actually fixable. `docs/personas.md` presented `uses:` as a
    two-part grant three times over, and a reader who checked the tool half — found it
    enforced — would reasonably assume the vault half was too."""

    @property
    def text(self) -> str:
        return (Path(__file__).resolve().parents[1] / "docs" / "personas.md").read_text()

    def test_the_disclosure_is_present(self):
        t = self.text
        self.assertIn("Vault reach is declared, not gated", t)
        self.assertIn("any session can name any registered vault", t)

    def test_the_line_that_read_as_a_permission_is_gone(self):
        """`borrows: release  # …and whose tools/vault I may actually use` — the comment
        on the example a reader copies. It named the one thing `borrows:` does not do."""
        self.assertNotIn("whose tools/vault I may actually use", self.text)

    def test_the_generated_sub_agent_does_not_claim_a_wall(self):
        """It used to assert *"You do NOT hold their credentials"* as a fact about the
        world. It is a rule the agent keeps — which is worth saying, and worth saying
        honestly, because a promise the runtime does not keep is the thing that teaches a
        reader to trust the next one."""
        from charter import commands_persona
        src = Path(commands_persona.__file__).read_text()
        self.assertNotIn("You do NOT hold their credentials", src)


if __name__ == "__main__":
    unittest.main()
