"""Two spellings of one path got two answers out of the vault guards.

`cat .charter/vaults/db.json` was denied. `cat .charter//vaults/db.json` was allowed — one
extra separator, the identical file to the kernel, no wrapper, no interpreter, no program
off the allowlist. `.charter/./vaults/db.json` likewise. This is not the documented
`_READERS` ceiling (a program the guard does not know) and not the documented path-spelling
limit (a path *you* chose, outside the plane): it is the guard failing on the one path it
claims to cover, which is what `SECURITY.md` and `docs/hooks.md` now promise it catches.

The property, and the reason this file does not hold a list of bad strings: **the decision
depends on the path an operand names, not on how it is spelled.** So the tests generate
equivalent spellings mechanically and assert every one of them lands where the canonical
form lands — denied for a vault file, allowed for the registry and for ordinary work. A new
redundant-separator trick is a new element of `_respellings`, not a new bypass.

Deliberately NOT `realpath`: resolving would follow symlinks and stat every operand of every
Bash tool call. A symlink planted at a path the caller chose is the limit `SECURITY.md`
states and `tests/test_documented_limits.py` pins, not a case this closes.
"""

from __future__ import annotations

import unittest

from charter import hooks
from tests._isolation import PersonaIso, run_hook

#: Spellings of one path that name the same file on POSIX. Each takes a path and returns an
#: equivalent one; `normpath` collapses all of them.
_RESPELLINGS = (
    ("as written", lambda p: p),
    ("doubled separator", lambda p: p.replace("/", "//", 1)),
    ("every separator doubled", lambda p: p.replace("/", "//")),
    ("a dot segment", lambda p: p.replace("/", "/./", 1)),
    ("a leading dot segment", lambda p: "./" + p),
    ("an up-and-back segment", lambda p: p.replace("/", "/x/../", 1)),
    ("tripled separator", lambda p: p.replace("/", "///", 1)),
)

#: A real vault file, the registry beside it (config and paths, never values — an ordinary
#: read, and the false positive that a wider pattern would reintroduce), and an unrelated
#: file. Values are fabricated names only; nothing here reads a real plane.
VAULT = ".charter/vaults/db.json"
REGISTRY = ".charter/vaults.json"
ORDINARY = "docs/secrets.md"


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


class TestTheBashGuardAnswersThePathNotTheSpelling(unittest.TestCase):
    def test_every_spelling_of_a_vault_file_is_denied(self):
        for name, respell in _RESPELLINGS:
            with self.subTest(spelling=name):
                path = respell(VAULT)
                reason = hooks._leak_reason(f"cat {path}")
                self.assertIsNotNone(reason, f"allowed as {name}")
                self.assertIn("reads a vault/secret file directly", reason)

    def test_every_spelling_of_the_registry_stays_allowed(self):
        """The registry holds provider config and file paths, never a value. Denying it is
        the false positive #443's predecessor fixed, and a wider pattern brings it back."""
        for name, respell in _RESPELLINGS:
            with self.subTest(spelling=name):
                self.assertIsNone(hooks._leak_reason(f"grep -rn vaults {respell(REGISTRY)}"),
                                  f"denied as {name}")

    def test_every_spelling_of_an_ordinary_file_stays_allowed(self):
        for name, respell in _RESPELLINGS:
            with self.subTest(spelling=name):
                self.assertIsNone(hooks._leak_reason(f"cat {respell(ORDINARY)}"),
                                  f"denied as {name}")

    def test_the_raw_operand_is_still_tested_so_no_denial_was_traded_away(self):
        """The union is raw OR normalised, and this pins the raw half.

        `cat .charter/vaults/../../elsewhere` matches the pattern as written and normalises
        to a path outside the plane. Semantically the normalised answer is the right one —
        that command reads no vault — so this denial is a false positive charter keeps on
        purpose: a fix for a *spelling* hole may not quietly hand back a denial that existed
        before it, and fail-closed is the correct direction for a guard under review.
        Without this test, deleting the raw arm passes the whole file."""
        self.assertIsNotNone(hooks._leak_reason("cat .charter/vaults/../../elsewhere"))

    def test_a_directory_operand_keeps_its_trailing_slash_denial(self):
        """`normpath` strips a trailing slash and `_VAULT_PATH_RE` requires it, so testing
        only the normalised form would have quietly re-opened the recursive grep that walks
        every vault file. Both forms are tested; this is the case that proves it."""
        self.assertIsNotNone(hooks._leak_reason("grep -r . .charter/vaults/"))
        self.assertIsNotNone(hooks._leak_reason("grep -r . .charter//vaults//"))


class TestTheReadGuardAnswersThePathNotTheSpelling(PersonaIso):
    """The `Read`/`Grep` guard shares the pattern, and shared a hole with it: the Bash
    denial names the path it refused, so the agent's next move is the same path through a
    tool — the exact sequence #90 was filed for."""

    def read(self, path: str, tool: str = "Read", **extra):
        ti = {"file_path": path} if tool == "Read" else {"path": path, **extra}
        return run_hook(hooks.pretooluse_read,
                        {"tool_name": tool, "tool_input": ti, "session_id": "s", "cwd": "/tmp"})

    def test_every_spelling_of_a_vault_file_is_denied(self):
        for name, respell in _RESPELLINGS:
            with self.subTest(spelling=name):
                self.assertEqual("deny", _decision(self.read(respell(VAULT))), name)

    def test_every_spelling_of_the_registry_stays_allowed(self):
        for name, respell in _RESPELLINGS:
            with self.subTest(spelling=name):
                self.assertIsNone(_decision(self.read(respell(REGISTRY))), name)

    def test_a_grep_rooted_at_the_state_directory_is_denied_in_every_spelling(self):
        for path in (".charter", ".charter/", ".charter//", "./.charter"):
            with self.subTest(path=path):
                self.assertEqual("deny",
                                 _decision(self.read(path, tool="Grep", pattern="token")))


if __name__ == "__main__":
    unittest.main()
