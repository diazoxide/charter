"""A vault read that can hang forever is a session that can hang forever.

`util.run`'s own docstring records the failure this prevents, by example: "every
un-timeouted path could hang indefinitely: a 1Password session needing re-auth stalled the
SessionStart preflight". Reference resolution shells out to exactly that CLI — `op read`,
`vault kv get` — and did so with no bound at all, on the one operation in the module that
leaves the machine.

The hang is worse here than the docstring's case, because of WHERE a resolve happens: a
reference is read inside `charter secret exec`, which an agent runs unattended. A prompt
that waits for an answer nobody is going to see does not fail; it stops, with no output to
explain why, for as long as the session lasts.
"""
from __future__ import annotations

import unittest
from unittest import mock

from charter import util
from charter.secrets import reference
from charter.secrets.base import VaultError


class _Vault(reference.ReferenceProvider):
    """A provider whose reference table is in memory — the resolver is what is under test,
    not the 0600 file it usually reads."""

    def __init__(self):
        super().__init__("t", {})
        self._refs = {"tok": "op://Eng/deploy/token"}

    def reference_for(self, key):          # noqa: D102 - see class docstring
        return self._refs[key]


class TestResolutionIsBounded(unittest.TestCase):
    def setUp(self) -> None:
        self.v = _Vault()
        self.enterContext(mock.patch.object(reference.shutil, "which", lambda c: f"/usr/bin/{c}"))

    def test_the_resolver_is_given_a_timeout(self):
        """Not "a timeout exists somewhere" — that this call passes one."""
        seen = {}

        def runner(argv, **kw):
            seen.update(kw)
            return mock.Mock(returncode=0, stdout="v")

        self.v.runner = staticmethod(runner)
        self.v.get("tok")
        self.assertIn("timeout", seen)
        self.assertTrue(0 < seen["timeout"] <= 300,
                        "bounded, and generous enough for a CLI that may re-auth")

    def test_a_timeout_is_classified_not_a_traceback(self):
        """ADR 0009 — charter names a cause it recognised. A `ProcTimeout` escaping as a
        traceback would be charter failing to classify its own failure, in the one place
        the operator most needs to be told what to do next."""
        def runner(argv, **kw):
            raise util.ProcTimeout("op", 60.0)

        self.v.runner = staticmethod(runner)
        with self.assertRaises(VaultError) as caught:
            self.v.get("tok")
        said = str(caught.exception)
        self.assertIn("op", said)
        self.assertIn("tok", said)

    def test_the_timeout_message_names_re_authentication(self):
        """The overwhelmingly likely cause, and the one the operator can act on. A bare
        "timed out" sends them looking at the network."""
        def runner(argv, **kw):
            raise util.ProcTimeout("op", 60.0)

        self.v.runner = staticmethod(runner)
        with self.assertRaises(VaultError) as caught:
            self.v.get("tok")
        self.assertIn("auth", str(caught.exception).lower())

    def test_a_timeout_never_carries_the_reference_output(self):
        """The module's existing rule — "Resolver output withheld — it can contain the
        secret" — has to hold on this path too, which is a second, later exit."""
        def runner(argv, **kw):
            raise util.ProcTimeout("op", 60.0)

        self.v.runner = staticmethod(runner)
        with self.assertRaises(VaultError) as caught:
            self.v.get("tok")
        self.assertNotIn("stdout", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
