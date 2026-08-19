"""Reading a logged-in session's token without printing it (#277).

The gap the report named was not "no way to read the token" — `@playwright/cli` has three,
and the vendor's own reference documents the idiom::

    TOKEN=$(playwright-cli --raw cookie-get session_id)

That IS the leak. The value lands in a shell variable through command substitution, in a
transcript, with nothing redacting it — which is the outcome the whole browser lane exists
to prevent. The gap is reading it *without* printing it.

Charter already owns that discipline end to end, for values that come from a vault: resolve
by name, inject into a child process, scrub from captured output, never put it in argv. A
browser session is just another place a value comes from — so this is one row in the
existing resolver table, not a new command. Everything downstream (`--env`, `--file`,
`--dotenv`, redaction, `persona secret exec`) then works unchanged and untouched.
"""
from __future__ import annotations

import unittest
from unittest import mock

from charter import browser
from charter.secrets import reference
from charter.secrets.base import VaultError


class _Vault(reference.ReferenceProvider):
    def __init__(self, refs, config=None):
        super().__init__("t", config or {})
        self._refs = dict(refs)

    def reference_for(self, key):
        return self._refs[key]


class TestTheSchemeIsRecognised(unittest.TestCase):
    def test_browser_is_a_reference_scheme(self):
        self.assertEqual(reference.scheme_of("browser://owner/localstorage/tok"), "browser")

    def test_an_unknown_scheme_is_still_refused(self):
        self.assertIsNone(reference.scheme_of("ftp://owner/x"))


class TestTheInvocation(unittest.TestCase):
    """Asserted, never run: the suite must not reach the network, and the failure worth
    catching is a malformed argv that only shows up when somebody needs a browser."""

    def _argv(self, uri, config=None):
        return reference._RESOLVERS["browser"](uri, config or {})[0]

    def test_it_pins_the_version(self):
        """The `browser` skill's loudest warning: a session belongs to the VERSION that
        opened it, and an unpinned `npx` resolves to a different daemon that reports
        `not open` while the first browser is alive and still logged in. Charter knows the
        pin, which is most of why this belongs to charter rather than to a hand-rolled
        `$(...)` in a shell script."""
        argv = self._argv("browser://owner/localstorage/tok")
        self.assertIn(f"@playwright/cli@{browser.PINNED}", argv)
        self.assertNotIn("@playwright/cli", argv, "the bare, unpinned spec must not appear")

    def test_the_vault_may_override_the_version(self):
        """A session opened at another version is unreadable at the pin — the same
        `not open` trap. `account` already sets the precedent for provider-specific vault
        config."""
        argv = self._argv("browser://owner/localstorage/tok", {"version": "0.1.19"})
        self.assertIn("@playwright/cli@0.1.19", argv)

    def test_it_names_the_session(self):
        self.assertIn("-s=owner", self._argv("browser://owner/cookie/session_id"))

    def test_it_asks_for_raw_output(self):
        """Without `--raw` the reply carries page status and generated-code sections, so
        the "secret" charter injected would be a decorated blob — and the redactor would
        then be scrubbing a string that never appears in the API call."""
        self.assertIn("--raw", self._argv("browser://owner/localstorage/tok"))

    def test_localstorage_and_cookie_reach_different_subcommands(self):
        self.assertIn("localstorage-get", self._argv("browser://o/localstorage/tok"))
        self.assertIn("cookie-get", self._argv("browser://o/cookie/session_id"))

    def test_the_key_is_passed_as_argv_not_interpolated(self):
        """The module's standing property: "a reference can never be command injection,
        whatever it contains"."""
        argv = self._argv("browser://o/localstorage/a b;rm -rf /")
        self.assertIn("a b;rm -rf /", argv)

    def test_it_resolves_through_npx(self):
        self.assertEqual(reference._RESOLVERS["browser"]("browser://o/cookie/c", {})[1], "npx")


class TestMalformedReferencesAreRefused(unittest.TestCase):
    """Validated when written, not at 3am when something tries to read it — the property
    `docs/secrets.md` already claims for the other schemes."""

    def _bad(self, uri):
        with self.assertRaises(VaultError) as caught:
            reference._RESOLVERS["browser"](uri, {})
        return str(caught.exception)

    def test_a_missing_session_is_named(self):
        self.assertIn("browser://", self._bad("browser:///localstorage/tok"))

    def test_an_unknown_source_is_named_with_the_ones_that_exist(self):
        said = self._bad("browser://owner/sessionstorage/tok")
        self.assertIn("localstorage", said)
        self.assertIn("cookie", said)

    def test_a_missing_key_is_named(self):
        self.assertIn("browser://", self._bad("browser://owner/localstorage"))


class TestTheValueThatComesBack(unittest.TestCase):
    def setUp(self):
        self.enterContext(mock.patch.object(reference.shutil, "which", lambda c: f"/bin/{c}"))

    def test_it_is_the_raw_value_with_the_trailing_newline_stripped(self):
        v = _Vault({"tok": "browser://owner/localstorage/access_token"})
        v.runner = staticmethod(lambda argv, **kw: mock.Mock(returncode=0, stdout="ey.J.Z\n"))
        self.assertEqual(v.get("tok"), "ey.J.Z")

    def test_a_closed_session_fails_without_echoing_anything_it_read(self):
        """`not open` is the common failure, and its stdout can carry whatever the page
        held. The module's withholding rule has to cover this scheme too."""
        v = _Vault({"tok": "browser://owner/localstorage/access_token"})
        v.runner = staticmethod(
            lambda argv, **kw: mock.Mock(returncode=1, stdout="ey.J.Z", stderr="not open"))
        with self.assertRaises(VaultError) as caught:
            v.get("tok")
        self.assertNotIn("ey.J.Z", str(caught.exception))

    def test_resolution_is_bounded_like_every_other_scheme(self):
        """npx can reach the network. The generic bound covers it — no per-scheme rule."""
        seen = {}

        def runner(argv, **kw):
            seen.update(kw)
            return mock.Mock(returncode=0, stdout="v")

        v = _Vault({"tok": "browser://owner/cookie/sid"})
        v.runner = staticmethod(runner)
        v.get("tok")
        self.assertIn("timeout", seen)


if __name__ == "__main__":
    unittest.main()
