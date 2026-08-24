"""`charter secret exec --dotenv`: render several secrets into one dotenv file.

The consuming parser is the `dotenv` package (Playwright's
`dotenvFileLoader` calls `dotenv.parse`), verified at v17.4.2 by exhaustive
fuzz (30,940 values, 0 corrupted — see `docs/superpowers/plans/
2026-08-07-secret-exec-dotenv.md`, "Encoding rule"). Three tiers, in order:
single quotes (fully literal; unsafe when '#' meets a quote, since a failed
quote match falls back to unquoted parsing where '#' starts a comment — or
when a quote meets a real newline), backticks (also fully literal, and
unlike single quotes carry a real newline safely), double quotes (the only
tier that can carry a literal CR, via escaping — ambiguous if the value
already holds a literal '\\n'/'\\r' sequence). `DotenvGolden` below checks
every entry against a fixture generated from the real `dotenv` package —
the authoritative check; `DotenvLine` keeps hand-written cases for
readability and named regressions.
"""

from __future__ import annotations

import io
import json
import os
import tempfile
import sys
from unittest import mock
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from types import SimpleNamespace

from tests._isolation import PersonaIso
from charter import commands_secrets

_GOLDEN_FIXTURE = Path(__file__).parent / "fixtures" / "dotenv_golden.json"


class DotenvLine(unittest.TestCase):
    """Every value must survive a dotenv parse byte-for-byte."""

    @staticmethod
    def _parse(line: str) -> str:
        """A faithful reimplementation of dotenv's single-line parse.

        Mirrors dotenv 17.x: a value wrapped in matching quotes (`'`, `"`, or
        `` ` ``) is unwrapped greedily to the last quote; only inside double
        quotes are `\\n` and `\\r` expanded; no other escape is processed —
        backtick-quoted content, like single-quoted, is fully literal.

        NOTE: this model does not reproduce dotenv's comment-stripping
        fallback for a value whose quotes don't match — that was exactly the
        gap that let Finding 1's silent-corruption bug pass here. The
        `DotenvGolden` test class below, generated from the real `dotenv`
        package, is the authoritative check; keep this one for readable
        round-trip assertions on individual cases.
        """
        _, _, raw = line.partition("=")
        if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in "\"'`":
            body = raw[1:-1]
            if raw[0] == '"':
                body = body.replace("\\n", "\n").replace("\\r", "\r")
            return body
        return raw.strip()

    def test_plain_value_round_trips(self):
        line = commands_secrets._dotenv_line("PASS", "hunter2")
        self.assertEqual(self._parse(line), "hunter2")

    def test_value_with_double_quote_round_trips(self):
        """The case naive backslash-escaping corrupts."""
        line = commands_secrets._dotenv_line("PASS", 'a"b')
        self.assertEqual(self._parse(line), 'a"b')

    def test_value_with_backslash_round_trips(self):
        line = commands_secrets._dotenv_line("PASS", "a\\b")
        self.assertEqual(self._parse(line), "a\\b")

    def test_value_with_single_quote_round_trips(self):
        line = commands_secrets._dotenv_line("PASS", "it's")
        self.assertEqual(self._parse(line), "it's")

    def test_apostrophe_plus_literal_backslash_n_round_trips(self):
        """Regression: the case that silently corrupted.

        An earlier rule double-quoted any value containing an apostrophe.
        `it's\\nb` (7 chars, a LITERAL backslash-n and no real newline) then
        encoded to `K="it's\\nb"` and decoded back to 6 chars with a REAL
        newline — a silently wrong credential.
        """
        value = "it's" + "\\" + "n" + "b"
        self.assertEqual(len(value), 7)
        line = commands_secrets._dotenv_line("PASS", value)
        self.assertEqual(self._parse(line), value)

    def test_literal_backslash_sequences_survive_without_a_newline(self):
        for value in ("a\\nb", "c:\\new\\report", "re\\r\\n", "\\\\n"):
            with self.subTest(value=value):
                line = commands_secrets._dotenv_line("K", value)
                self.assertEqual(self._parse(line), value)

    def test_real_newline_with_backslash_but_no_escape_sequence(self):
        """A backslash not followed by n/r is unambiguous — must NOT raise."""
        for value in ("a\\b\nc", "path\\to\nfile"):
            with self.subTest(value=value):
                line = commands_secrets._dotenv_line("K", value)
                self.assertEqual(self._parse(line), value)

    def test_pem_style_multiline_value_round_trips(self):
        value = "-----BEGIN KEY-----\nMIIB\n-----END KEY-----\n"
        line = commands_secrets._dotenv_line("KEY", value)
        self.assertEqual(self._parse(line), value)

    def test_backslash_n_plus_real_newline_prefers_a_literal_tier_not_a_raise(self):
        """Superseded premise, kept as an explicit regression pin.

        An earlier two-tier rule (single-quote unless '\\'' or a real
        newline is present, else double-quote-with-escaping) had to reject
        these: double-quoting would make the value's own literal '\\n'/'\\r'
        indistinguishable from the escaped real newline. The three-tier rule
        avoids the problem instead of rejecting it — none of these values
        contain a '#'+quote or quote+newline collision, so tier 1 or 2
        (fully literal, no escaping at all) applies and there is no
        ambiguity to reject. Must NOT raise.
        """
        for value in ("x\\ny\nz", "a\\rb\nc", "it's\\nb\nreal"):
            with self.subTest(value=value):
                line = commands_secrets._dotenv_line("K", value)
                self.assertEqual(self._parse(line), value)

    def test_raises_when_a_real_cr_collides_with_a_double_quote(self):
        """Genuinely unrepresentable in dotenv — must fail loudly, not corrupt.

        A real CR forces tier 3 (the only tier that can carry one — every
        other quote style has it normalised away to LF by dotenv itself),
        but tier 3 needs the value to be double-quote-free. A value with
        both has no tier left.
        """
        for value in ('"\r', '#"\r', 'a"b\rc', '\r"', 'x"\r\ny'):
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    commands_secrets._dotenv_line("K", value)

    def test_raises_when_every_quote_style_is_present(self):
        """The second unrepresentable class: no CR, but no usable quote either.

        `#` with a single quote rules out tier 1, a backtick rules out tier 2,
        and a double quote rules out tier 3.
        """
        for value in ("#'`\"x", 'a#\'b`c"d'):
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    commands_secrets._dotenv_line("K", value)

    def test_unrepresentable_message_names_the_actual_cause(self):
        """An operator cannot act on a message describing the wrong collision.

        The message must also never echo the secret itself.
        """
        with self.assertRaises(ValueError) as cm:
            commands_secrets._dotenv_line("K", 'a"b\rc')
        self.assertIn("carriage return", str(cm.exception))
        self.assertNotIn('a"b', str(cm.exception))

        with self.assertRaises(ValueError) as cm:
            commands_secrets._dotenv_line("K", "#'`\"secretpart")
        msg = str(cm.exception)
        self.assertIn("backtick", msg)
        self.assertNotIn("carriage return", msg)
        self.assertNotIn("secretpart", msg)

    def test_value_with_newline_round_trips(self):
        line = commands_secrets._dotenv_line("KEY", "line1\nline2")
        self.assertEqual(self._parse(line), "line1\nline2")

    def test_value_mixing_both_quotes_and_newline_round_trips(self):
        value = "a'b\"c\nd"
        line = commands_secrets._dotenv_line("KEY", value)
        self.assertEqual(self._parse(line), value)

    def test_empty_and_whitespace_values_round_trip(self):
        for value in ("", " ", "  lead", "trail  "):
            with self.subTest(value=value):
                line = commands_secrets._dotenv_line("K", value)
                self.assertEqual(self._parse(line), value)

    def test_special_characters_are_not_expanded(self):
        """`$`, `#` and `=` must stay literal, not be treated as syntax."""
        for value in ("a$bc", "${VAR}", "a#b", "a=b"):
            with self.subTest(value=value):
                line = commands_secrets._dotenv_line("K", value)
                self.assertEqual(self._parse(line), value)

    def test_line_has_no_trailing_newline(self):
        self.assertEqual(commands_secrets._dotenv_line("K", "v").count("\n"), 0)

    def test_rejects_invalid_env_var_name(self):
        for name in ("has-dash", "1leading", "has space", "", "has=eq"):
            with self.subTest(name=name):
                with self.assertRaises(ValueError):
                    commands_secrets._dotenv_line(name, "v")

    def test_accepts_valid_env_var_names(self):
        for name in ("A", "_x", "PLAYWRIGHT_MCP_SECRETS_FILE", "K1_2"):
            with self.subTest(name=name):
                commands_secrets._dotenv_line(name, "v")

    def test_finding1_hash_plus_single_quote_does_not_corrupt(self):
        """Regression for Finding 1 (CRITICAL) — confirmed live silent corruption.

        The old (pre-fix) rule single-quoted any value without a real
        newline, verbatim: `_dotenv_line("K", "a#b'")` produced `K='a#b''`.
        dotenv treats '#' as a comment starter in an *unquoted* value, and
        falls back to unquoted parsing whenever a quote match fails — so
        that line decoded back to just `'a`, three characters, silently
        discarding the rest of the credential. The fix moves a value
        combining '#' and \"'\" to the backtick tier instead.
        """
        value = "a#b'"
        line = commands_secrets._dotenv_line("K", value)
        self.assertEqual(line, "K=`a#b'`")
        self.assertEqual(self._parse(line), value)


class DotenvGolden(unittest.TestCase):
    """Authoritative check: a fixture generated from the REAL dotenv 17.4.2.

    Unlike `DotenvLine._parse` (a hand-written reimplementation that can
    itself be wrong — it missed the comment-stripping fallback that Finding
    1 exploited), every entry here was verified to round-trip through the
    actual `dotenv.parse`. See `tests/fixtures/dotenv_golden.json`.
    """

    @classmethod
    def setUpClass(cls):
        with open(_GOLDEN_FIXTURE, encoding="utf-8") as f:
            cls.fixture = json.load(f)

    def test_fixture_is_for_the_verified_dotenv_version(self):
        self.assertEqual(self.fixture["dotenv_version"], "17.4.2")
        self.assertTrue(self.fixture["entries"])
        self.assertTrue(self.fixture["unencodable"])

    def test_every_entry_matches_the_real_dotenv_encoding(self):
        for entry in self.fixture["entries"]:
            with self.subTest(value=entry["value"]):
                line = commands_secrets._dotenv_line("K", entry["value"])
                self.assertEqual(line, "K=" + entry["body"])

    def test_every_unencodable_value_raises(self):
        for value in self.fixture["unencodable"]:
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    commands_secrets._dotenv_line("K", value)


class _StubProvider:
    """Stands in for a vault; records which keys were asked for."""

    def __init__(self, values: dict[str, str]):
        self.values = values
        self.asked: list[str] = []

    def get(self, key: str) -> str:
        self.asked.append(key)
        return self.values[key]


class DotenvExec(PersonaIso):
    """`PersonaIso`, not a bare `TestCase`: `cmd_secret_exec` records the hand-out in the
    session trace (#441), and a fixture that has not redirected `config` appends that row
    to the plane the suite resolved to — the developer's own (#372, #402)."""

    def setUp(self) -> None:
        super().setUp()
        self.provider = _StubProvider({"pw-user": "svc_qa",
                                       "pw-pass": 'p"ass\\word'})
        # NOT `self._orig`: `PersonaIso` keeps the config snapshot it restores under that
        # name, and shadowing it made every test in this class error in teardown.
        self._orig_provider = commands_secrets._provider
        commands_secrets._provider = lambda _name: self.provider
        self.addCleanup(
            lambda: setattr(commands_secrets, "_provider", self._orig_provider))
        self._td = tempfile.TemporaryDirectory()
        self.tmpdir = self._td.name
        self.addCleanup(self._td.cleanup)

    @staticmethod
    def _args(**kw):
        base = {"vault": "qa", "env": None, "file": None, "dotenv": None,
                "command": [], "exec_mode": False}
        base.update(kw)
        return SimpleNamespace(**base)

    def test_writes_one_file_with_all_entries(self):
        """Two --dotenv flags sharing an ENVVAR produce a single merged file."""
        import contextlib

        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = commands_secrets.cmd_secret_exec(self._args(
                dotenv=["SECRETS=USER:pw-user", "SECRETS=PASS:pw-pass"],
                command=["python3", "-c",
                         "import os;print(open(os.environ['SECRETS']).read(), end='')"]))
        self.assertEqual(rc, 0)
        self.assertEqual(self.provider.asked, ["pw-user", "pw-pass"])
        # One file, both entries, and neither value survived redaction.
        self.assertEqual(len(buf.getvalue().strip().splitlines()), 2)
        self.assertNotIn("svc_qa", buf.getvalue())

    def test_file_contents_round_trip_and_are_0600(self):
        """The file parses back to the real values and is mode 0600.

        The child cannot print the values (they would be redacted, and
        printing a secret is exactly what this feature prevents), so it
        writes a SHA-256 of each parsed value to a scratch file the test
        owns. Comparing digests proves the round-trip without exposing
        anything.
        """
        import hashlib
        import json

        out = os.path.join(self.tmpdir, "probe.json")
        child = (
            "import os,json,stat,hashlib\n"
            "p=os.environ['SECRETS']\n"
            "vals={}\n"
            "for line in open(p):\n"
            "    line=line.rstrip('\\n')\n"
            "    if not line: continue\n"
            "    k,_,raw=line.partition('=')\n"
            "    if len(raw)>=2 and raw[0]==raw[-1] and raw[0] in '\\\"\\'':\n"
            "        body=raw[1:-1]\n"
            "        if raw[0]=='\\\"': body=body.replace('\\\\n','\\n').replace('\\\\r','\\r')\n"
            "    else:\n"
            "        body=raw.strip()\n"
            "    vals[k]=hashlib.sha256(body.encode()).hexdigest()\n"
            f"json.dump({{'vals':vals,'mode':stat.S_IMODE(os.stat(p).st_mode)}},open({out!r},'w'))\n"
        )
        rc = commands_secrets.cmd_secret_exec(self._args(
            dotenv=["SECRETS=USER:pw-user", "SECRETS=PASS:pw-pass"],
            command=["python3", "-c", child]))
        self.assertEqual(rc, 0)
        probe = json.load(open(out))
        self.assertEqual(probe["mode"], 0o600)
        self.assertEqual(probe["vals"]["USER"],
                         hashlib.sha256(b"svc_qa").hexdigest())
        self.assertEqual(probe["vals"]["PASS"],
                         hashlib.sha256('p"ass\\word'.encode()).hexdigest())

    def test_temp_file_is_deleted_after_the_command(self):
        """No secrets file may outlive the child process.

        The *path* is not a secret, so the child may print it; the value
        inside is what redaction protects.
        """
        import contextlib

        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = commands_secrets.cmd_secret_exec(self._args(
                dotenv=["SECRETS=USER:pw-user"],
                command=["python3", "-c",
                         "import os;print(os.environ['SECRETS'])"]))
        self.assertEqual(rc, 0)
        path = buf.getvalue().strip()
        self.assertTrue(path, "child did not report the secrets-file path")
        self.assertFalse(os.path.exists(path),
                         f"secrets file survived the command: {path}")

    def test_rejects_spec_without_colon(self):
        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = commands_secrets.cmd_secret_exec(self._args(
                dotenv=["SECRETS=nocolon"], command=["true"]))
        self.assertEqual(rc, 2)
        self.assertIn("ENVVAR=NAME:key", buf.getvalue())

    def test_rejects_duplicate_name_within_one_envvar(self):
        """Two entries with the same NAME would leave precedence to the reader.

        dotenv's behaviour on a repeated key is not something this file should
        depend on, and a duplicate is almost always a typo — so refuse it
        rather than write two lines and hope.
        """
        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = commands_secrets.cmd_secret_exec(self._args(
                dotenv=["S=USER:pw-user", "S=USER:pw-pass"], command=["true"]))
        self.assertEqual(rc, 2)
        self.assertIn("twice", buf.getvalue())
        self.assertNotIn("svc_qa", buf.getvalue())

    def test_same_name_under_different_envvars_is_allowed(self):
        """Distinct files are independent — only a collision *within* one is bad."""
        rc = commands_secrets.cmd_secret_exec(self._args(
            dotenv=["A=USER:pw-user", "B=USER:pw-user"],
            command=["python3", "-c",
                     "import os;assert os.environ['A']!=os.environ['B']"]))
        self.assertEqual(rc, 0)

    def test_exec_conflict_names_both_flags(self):
        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = commands_secrets.cmd_secret_exec(self._args(
                file=["F=pw-user"], dotenv=["S=USER:pw-user"],
                exec_mode=True, command=["true"]))
        self.assertEqual(rc, 2)
        self.assertIn("--file and --dotenv", buf.getvalue())

    def test_rejects_spec_without_equals(self):
        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = commands_secrets.cmd_secret_exec(self._args(
                dotenv=["NAME:key"], command=["true"]))
        self.assertEqual(rc, 2)
        self.assertIn("ENVVAR=NAME:key", buf.getvalue())

    def test_rejects_invalid_entry_name(self):
        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = commands_secrets.cmd_secret_exec(self._args(
                dotenv=["SECRETS=bad-name:pw-user"], command=["true"]))
        self.assertEqual(rc, 2)
        self.assertIn("bad-name", buf.getvalue())

    def test_refuses_to_combine_with_exec(self):
        """--exec would leak the temp file: the process is replaced."""
        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = commands_secrets.cmd_secret_exec(self._args(
                dotenv=["SECRETS=USER:pw-user"], exec_mode=True,
                command=["true"]))
        self.assertEqual(rc, 2)
        self.assertIn("--dotenv", buf.getvalue())

    def test_values_are_redacted_from_output(self):
        """A child echoing the secret must not leak it to stdout."""
        import contextlib
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = commands_secrets.cmd_secret_exec(self._args(
                dotenv=["SECRETS=PASS:pw-pass"],
                command=["python3", "-c",
                         "import os;print(open(os.environ['SECRETS']).read())"]))
        self.assertEqual(rc, 0)
        self.assertNotIn('p"ass\\word', buf.getvalue())

    def test_finding2_multiline_secret_is_redacted_from_output(self):
        """Regression for Finding 2 (CRITICAL) — confirmed live redaction gap.

        A value with a real CR forces the tier-3 (escaping) path: the file
        on disk holds `\\r\\n` text, not the raw CR/LF bytes. Only the raw
        value was ever registered for redaction, so a child that echoes the
        file printed the escaped PEM key in the clear. Assert the whole
        multi-line secret — CRLF form and content alike — never reaches
        captured stdout.
        """
        pem = "-----BEGIN KEY-----\r\nMIIB\r\n-----END KEY-----\r\n"
        self.provider.values["pem"] = pem
        import contextlib
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = commands_secrets.cmd_secret_exec(self._args(
                dotenv=["SECRETS=KEY:pem"],
                command=["python3", "-c",
                         "import os;print(open(os.environ['SECRETS']).read())"]))
        self.assertEqual(rc, 0)
        output = buf.getvalue()
        self.assertNotIn(pem, output)
        self.assertNotIn(pem.replace("\r", "\\r").replace("\n", "\\n"), output)
        self.assertNotIn("BEGIN KEY", output)
        self.assertNotIn("MIIB", output)

    def test_finding3_bad_file_spec_does_not_leak_the_first_tmpfile(self):
        """Regression for Finding 3 (IMPORTANT) — confirmed live leak.

        `--file A=k1 --file BADSPEC` used to exit 2 leaving the FIRST
        file's 0600 temp file on disk forever: the `--file` loop's early
        `return 2` for the second, malformed spec never unlinked what the
        loop had already written.
        """
        written: list[str] = []
        orig_mkstemp = commands_secrets.tempfile.mkstemp

        def _tracking_mkstemp(*a, **kw):
            fd, path = orig_mkstemp(*a, **kw)
            written.append(path)
            return fd, path

        commands_secrets.tempfile.mkstemp = _tracking_mkstemp
        self.addCleanup(lambda: setattr(commands_secrets.tempfile, "mkstemp", orig_mkstemp))

        buf = io.StringIO()
        with redirect_stderr(buf):
            rc = commands_secrets.cmd_secret_exec(self._args(
                file=["A=pw-user", "BADSPEC"], command=["true"]))
        self.assertEqual(rc, 2)
        self.assertEqual(len(written), 1, "expected exactly one tmpfile to have been created")
        self.assertFalse(os.path.exists(written[0]),
                         f"leaked the first --file's tmpfile: {written[0]}")

    def test_finding4_non_vaulterror_exception_still_cleans_up_tmpfiles(self):
        """Regression for Finding 4 (IMPORTANT) — confirmed live leak.

        Only `base.VaultError` was ever caught around resolution, so any
        other exception (`FileNotFoundError` from `mkstemp` when a vault key
        contains '/', `UnicodeEncodeError`, `ENOSPC`, `KeyboardInterrupt`,
        ...) propagated straight out and stranded every 0600 tmpfile already
        written. Simulate that with a provider whose second `.get()` raises
        a plain `RuntimeError` — deliberately NOT a `VaultError` — after the
        first `--file` has already been written to disk.
        """
        class _BoomProvider(_StubProvider):
            def get(self, key: str) -> str:
                if key == "boom":
                    raise RuntimeError("simulated ENOSPC")
                return super().get(key)

        boom = _BoomProvider({"pw-user": "svc_qa", "boom": "unused"})
        commands_secrets._provider = lambda _name: boom

        written: list[str] = []
        orig_mkstemp = commands_secrets.tempfile.mkstemp

        def _tracking_mkstemp(*a, **kw):
            fd, path = orig_mkstemp(*a, **kw)
            written.append(path)
            return fd, path

        commands_secrets.tempfile.mkstemp = _tracking_mkstemp
        self.addCleanup(lambda: setattr(commands_secrets.tempfile, "mkstemp", orig_mkstemp))

        with self.assertRaises(RuntimeError):
            commands_secrets.cmd_secret_exec(self._args(
                file=["A=pw-user", "B=boom"], command=["true"]))

        self.assertEqual(len(written), 1,
                         "expected exactly the first --file's tmpfile to have been created")
        self.assertFalse(os.path.exists(written[0]),
                         f"leaked a tmpfile after a non-VaultError exception: {written[0]}")


if __name__ == "__main__":
    unittest.main()


class AnEmptySecretIsRefused(PersonaIso):
    """`charter secret set devops API_TOKEN` typed without a value read EOF, stored `""`
    and exited 0. Afterwards `get` says the key is present, `vault list` counts it and
    `doctor` calls the vault healthy — the failure surfaces hours later as a 401 from
    something unrelated.

    A non-tty stdin is not on its own a request to read a secret from it: an agent's Bash
    tool, a CI step and `< /dev/null` all present one.
    """

    def setUp(self) -> None:
        super().setUp()
        from charter.secrets import registry
        registry.add_vault("dev", "plain-file", {"file": "dev.json"})

    def _set(self, **kw):
        args = SimpleNamespace(vault="dev", key="API_TOKEN", stdin=kw.pop("stdin", False),
                               from_file=kw.pop("from_file", None),
                               value=kw.pop("value", None),
                               allow_empty=kw.pop("allow_empty", False))
        with redirect_stderr(io.StringIO()) as err:
            rc = commands_secrets.cmd_secret_set(args)
        return rc, err.getvalue()

    def test_an_empty_non_tty_stdin_is_refused(self):
        with mock.patch.object(sys, "stdin", io.StringIO("")):
            rc, err = self._set()
        self.assertEqual(rc, 1)
        self.assertIn("empty value", err)

    def test_the_refusal_explains_where_the_nothing_came_from(self):
        with mock.patch.object(sys, "stdin", io.StringIO("")):
            _rc, err = self._set()
        self.assertIn("Nothing arrived on stdin", err)

    def test_an_ordinary_pipe_still_works_without_a_flag(self):
        """`… | charter secret set <vault> <key>` predates `--stdin` and is how people
        actually do this. Requiring the flag was the first fix here, and it broke every
        working pipeline to stop a mistake the empty check already catches."""
        with mock.patch.object(sys, "stdin", io.StringIO("s3cret")):
            rc, _ = self._set()
        self.assertEqual(rc, 0)
        from charter.secrets import registry
        self.assertEqual(registry.provider_for("dev").get("API_TOKEN"), "s3cret")

    def test_an_explicit_empty_stdin_is_still_refused(self):
        """`--stdin` says where the value comes from, not that empty is intended."""
        with mock.patch.object(sys, "stdin", io.StringIO("")):
            rc, err = self._set(stdin=True)
        self.assertEqual(rc, 1)
        self.assertIn("empty", err)

    def test_allow_empty_is_the_deliberate_override(self):
        with mock.patch.object(sys, "stdin", io.StringIO("")):
            rc, _ = self._set(stdin=True, allow_empty=True)
        self.assertEqual(rc, 0)

    def test_a_real_value_still_stores(self):
        with mock.patch.object(sys, "stdin", io.StringIO("s3cret\n")):
            rc, _ = self._set(stdin=True)
        self.assertEqual(rc, 0)

    def test_an_existing_secret_survives_a_refused_overwrite(self):
        """The damage was never the error — it was the silent replacement."""
        from charter.secrets import registry
        with mock.patch.object(sys, "stdin", io.StringIO("real-token\n")):
            self._set(stdin=True)
        with mock.patch.object(sys, "stdin", io.StringIO("")):
            self._set()
        self.assertEqual(registry.provider_for("dev").get("API_TOKEN"), "real-token")
