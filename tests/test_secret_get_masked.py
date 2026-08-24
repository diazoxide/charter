"""The masked `charter secret get` line must not confirm a guessed value (#436).

What it printed was `present · 11 bytes · sha256:323725e8eff4` — an unsalted, un-keyed
SHA-256 prefix of the value plus its exact byte count. Both halves are pure functions of
the value, so the line is checkable **offline**: the length prefilters a wordlist and the
48-bit digest confirms the hit, with no further access to charter. The audit demonstrated
it end to end against a fabricated `Summer2024!`.

**The property under test is not "the digest is not sha256".** That is a spelling, and the
next spelling — `sha256(value)[12:24]`, blake2b, md5, a constant salt baked into the
source, base64 instead of hex — walks straight past a test written that way, while still
being exactly as checkable offline as before.

The property is: **the printed line is not a function of the value alone.** So the test
that carries the weight runs the same value through two control planes whose only
difference is the key file and requires the fingerprints to differ. Every keyless
construction there is fails that, including ones nobody has thought of, because a pure
function of the value cannot produce two answers for one value. Stability within a plane
is asserted alongside it, so "print random bytes" is not a passing answer either.

The second property is about the size: **the exact byte length must not be recoverable.**
Tested as indistinguishability — every length inside a band prints the same text — and
paired with a boundary assertion so a `size_band` that returns a constant fails too.
"""

from __future__ import annotations

import hashlib
import io
import os
import re
import stat
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_secrets, config
from charter.secrets import fingerprint, registry
from tests._isolation import PersonaIso

#: Fabricated. Chosen to look like the kind of value the offline check is decisive
#: against — a human password inside a wordlist — without being anyone's credential.
GUESSABLE = "Summer2024!"

_FP = re.compile(r"fp:([0-9a-f]+)")


class TheFingerprintIsNotAFunctionOfTheValue(PersonaIso):
    """The core property. Nothing here mentions a hash algorithm by name."""

    def _in_plane(self, state_dir: Path, value: str) -> str:
        with mock.patch.object(config, "STATE_DIR", state_dir):
            return fingerprint.masked(value)

    def _fp(self, state_dir: Path, value: str) -> str:
        line = self._in_plane(state_dir, value)
        m = _FP.search(line)
        self.assertIsNotNone(m, f"no fingerprint in {line!r} — a plane with a writable "
                                f"state dir must produce one")
        return m.group(1)

    def test_two_planes_fingerprint_one_value_differently(self):
        """The whole point. A keyless digest — of any algorithm, at any offset, with any
        constant salt, in any encoding — is a pure function of the value and therefore
        cannot answer differently in two planes. This fails for all of them."""
        a = self._fp(self.tmp / "plane-a", GUESSABLE)
        b = self._fp(self.tmp / "plane-b", GUESSABLE)
        self.assertNotEqual(
            a, b,
            "the same value fingerprinted identically in two control planes, so the "
            "fingerprint is computable from the value alone — which is what makes the "
            "masked line an offline check against a guess")

    def test_one_plane_fingerprints_one_value_the_same_way_twice(self):
        """The other half: "print something unpredictable" is not a fix. The fingerprint
        has a job — is this the same value as before — and it still has to do it."""
        home = self.tmp / "plane-stable"
        self.assertEqual(self._fp(home, GUESSABLE), self._fp(home, GUESSABLE))

    def test_one_plane_fingerprints_two_values_differently(self):
        home = self.tmp / "plane-distinct"
        self.assertNotEqual(self._fp(home, GUESSABLE), self._fp(home, GUESSABLE + "!"))

    def test_no_keyless_digest_of_the_value_reproduces_the_printed_fingerprint(self):
        """The filed regression, widened past the one algorithm that was filed. Every
        hash stdlib guarantees, over the value and over the value with charter's own name
        as a constant salt — none of them may contain the printed fingerprint anywhere,
        at any offset."""
        printed = self._fp(self.tmp / "plane-c", GUESSABLE)
        raw = GUESSABLE.encode()
        candidates = [raw, b"charter" + raw, raw + b"charter"]
        checked = 0
        for algo in sorted(hashlib.algorithms_guaranteed):
            for material in candidates:
                try:
                    digest = hashlib.new(algo, material).hexdigest()
                except TypeError:
                    continue                      # shake_* needs an explicit length
                checked += 1
                self.assertNotIn(printed, digest,
                                 f"the printed fingerprint is recoverable from the value "
                                 f"alone via {algo}")
        self.assertGreater(checked, 10, "the digest sweep did not actually run")

    def test_a_plane_that_cannot_hold_a_key_prints_no_fingerprint_at_all(self):
        """The fallback must not restore the property the key removes. A read-only or
        unusable state directory means LESS information, never an unkeyed digest — that
        fallback would fire in exactly the locked-down environments nobody is watching."""
        blocker = self.tmp / "not-a-directory"
        blocker.write_text("")
        line = self._in_plane(blocker / "state", GUESSABLE)
        self.assertNotIn("fp:", line)
        self.assertNotIn(hashlib.sha256(GUESSABLE.encode()).hexdigest()[:12], line)
        self.assertFalse(re.search(r"[0-9a-f]{8}", line),
                         f"{line!r} still carries something digest-shaped")

    def test_the_key_file_is_0600(self):
        home = self.tmp / "plane-perm"
        with mock.patch.object(config, "STATE_DIR", home):
            fingerprint.fingerprint(GUESSABLE)
            path = fingerprint.key_path()
        self.assertTrue(path.exists())
        self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)

    def _loose_key_file(self, home: Path) -> Path:
        """A key file left over at 0644 and the wrong length, so the next call has to
        regenerate it — the case where the mode argument to `os.open` is ignored (#437)."""
        home.mkdir(parents=True, exist_ok=True)
        with mock.patch.object(config, "STATE_DIR", home):
            path = fingerprint.key_path()
        path.write_bytes(b"leftover")
        os.chmod(path, 0o644)
        return path

    def test_a_leftover_loose_key_file_is_tightened_before_it_is_rewritten(self):
        home = self.tmp / "plane-relax"
        path = self._loose_key_file(home)
        self.assertIsNotNone(self._fp(home, GUESSABLE))
        self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        self.assertEqual(len(path.read_bytes()), fingerprint.KEY_BYTES)

    def test_no_key_is_written_into_a_file_charter_could_not_make_private(self):
        """`os.fchmod` returning successfully is not evidence the bits moved — a mount
        with fixed permissions accepts it and reports the old mode. The mode is read back
        off the descriptor, and a key that would have landed world-readable is not
        generated at all: no fingerprint beats a fingerprint whose key anyone can read."""
        home = self.tmp / "plane-fixed-perms"
        path = self._loose_key_file(home)
        with mock.patch.object(os, "fchmod", lambda *a: None):
            line = self._in_plane(home, GUESSABLE)
        self.assertNotIn("fp:", line)
        self.assertEqual(path.read_bytes(), b"leftover",
                         "key material was written into a file charter had just found it "
                         "could not make private")


class TheSizeIsABandNotACount(unittest.TestCase):
    """No plane state involved — `size_band` is a pure function of the value."""

    def test_every_length_inside_a_band_prints_the_same_text(self):
        """Indistinguishability is the property: a reader of the line cannot tell 9 bytes
        from 15. Asserting the literal string `"8–15 bytes"` would instead be a tautology
        against the constant under test."""
        for lo, hi in ((1, 15), (16, 31), (32, 63), (64, 127)):
            bands = {fingerprint.size_band("x" * n) for n in range(lo, hi + 1)}
            self.assertEqual(len(bands), 1,
                             f"lengths {lo}..{hi} printed {len(bands)} different labels: "
                             f"{sorted(bands)}")

    def test_neighbouring_bands_still_differ(self):
        """Paired with the test above so `return "some bytes"` is not a passing answer:
        the label has to keep telling a password from a PEM file."""
        seen = [fingerprint.size_band("x" * n) for n in (0, 1, 16, 32, 64, 2048)]
        self.assertEqual(len(set(seen)), len(seen), f"bands collapsed: {seen}")

    def test_an_empty_value_is_still_called_out(self):
        self.assertEqual(fingerprint.size_band(""), "empty")
        self.assertNotEqual(fingerprint.size_band("x"), "empty")

    def test_the_band_counts_utf8_bytes_not_characters(self):
        """`len()` on a `str` counted characters while the label said "bytes"; three
        Cyrillic characters occupy six bytes in the file."""
        self.assertEqual(fingerprint.size_band("а" * 8), fingerprint.size_band("x" * 16))


class TheCommandActuallyPrintsIt(PersonaIso):
    """End to end through `cmd_secret_get`, so the helper being correct while the command
    still prints the old line is a failure and not a green suite."""

    def setUp(self):
        super().setUp()
        vf = config.VAULTS_DIR / "audit.json"
        registry.add_vault("audit", "plain-file", {"file": str(vf)})
        self.prov = registry.provider_for("audit")

    def _get(self, key: str) -> str:
        buf = io.StringIO()
        args = SimpleNamespace(vault="audit", key=key, reveal=False, force=False)
        with redirect_stdout(buf):
            rc = commands_secrets.cmd_secret_get(args)
        self.assertEqual(rc, 0)
        return buf.getvalue()

    def test_the_masked_line_carries_no_hash_of_the_value(self):
        self.prov.set("WEAK", GUESSABLE)
        out = self._get("WEAK")
        self.assertIn("present", out)
        self.assertNotIn("sha256:", out)
        self.assertNotIn(hashlib.sha256(GUESSABLE.encode()).hexdigest()[:12], out)

    def test_the_masked_line_does_not_disclose_the_exact_length(self):
        """Two values a reader could tell apart by length before, in one band now: strip
        the fingerprints and the two lines must be the same string."""
        self.prov.set("A", "x" * 9)
        self.prov.set("B", "y" * 15)
        a = _FP.sub("fp:-", self._get("A")).replace("audit/A", "audit/K")
        b = _FP.sub("fp:-", self._get("B")).replace("audit/B", "audit/K")
        self.assertEqual(a, b, "the masked line still distinguishes a 9-byte value from a "
                              "15-byte one")

    def test_the_value_itself_is_never_printed(self):
        self.prov.set("WEAK", GUESSABLE)
        self.assertNotIn(GUESSABLE, self._get("WEAK"))


class TheKeyIsAsProtectedAsAVault(PersonaIso):
    """Keying the fingerprint moves the secret, it does not remove it.

    A reader who holds `.charter/fingerprint.key` can compute the fingerprint of a guess
    and the offline check is back, unchanged. So the key has to sit on the same side of
    the read guard as the vault files — and it did not: the guard denied
    `.charter/vaults/devops.json` and allowed `.charter/fingerprint.key`, which for a
    1Password-backed vault (no vault file on disk at all) was the only readable thing
    between the printed line and the value.
    """

    def _read(self, path: str, tool: str = "Read", **extra):
        from charter import hooks
        from tests._isolation import run_hook

        ti = {"file_path": path} if tool == "Read" else {"path": path, **extra}
        r = run_hook(hooks.pretooluse_read,
                     {"tool_name": tool, "tool_input": ti, "session_id": "s", "cwd": "/tmp"})
        return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")

    def _bash(self, command: str):
        from charter import hooks
        from tests._isolation import run_hook

        r = run_hook(hooks.pretooluse,
                     {"tool_name": "Bash", "tool_input": {"command": command},
                      "session_id": "s", "cwd": "/tmp"})
        return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")

    def test_reading_the_key_is_denied_like_reading_a_vault(self):
        self.assertEqual(self._read(f".charter/{fingerprint.KEY_FILE}"), "deny")
        self.assertEqual(self._read(f"/home/me/plane/.charter/{fingerprint.KEY_FILE}"),
                         "deny")

    def test_the_legacy_state_directory_name_is_covered_too(self):
        """`.edm/` is the pre-rename spelling the guard deliberately still answers for."""
        self.assertEqual(self._read(f".edm/{fingerprint.KEY_FILE}"), "deny")

    def test_catting_the_key_is_denied(self):
        self.assertEqual(self._bash(f"cat .charter/{fingerprint.KEY_FILE}"), "deny")

    def test_the_registry_is_still_readable(self):
        """The carve-out the guard already had must survive: `.charter/vaults.json` holds
        provider config and paths, never values, and hard-denying it broke ordinary work."""
        self.assertNotEqual(self._read(".charter/vaults.json"), "deny")


if __name__ == "__main__":
    unittest.main()
