"""What an MCP approval actually covers, and what the operator saw before giving it.

`charter/mcpseen.py` is the one consent mechanism charter builds: a machine-local record
saying *this operator has read this command line and agreed it may receive the vault's
value*. Three findings from the 2026-08-24 audit each hollowed it out from a different
side, and this module is the boundary for all three:

* **#426 — the digest covered five fields.** ``env`` was not one of them, and
  `persona.mcp_render_entry` copies every unconsumed key of the committed entry into the
  generated agent file. So a commit could add ``NODE_OPTIONS`` or re-point ``PATH`` on an
  ALREADY-APPROVED server and the approval stayed valid. The fix is not "add ``env``" —
  that is the same bug one field further out. It is: digest the whole entry.
* **#427 — the consent line was empty for `http`/`sse`.** No ``command``, no ``args``,
  so `describe` returned ``""`` and printed a blank under *"Read the command above."*
* **#428 — `--approve-mcp` approved everything, unasked.** Every credentialed server of
  every persona, in one non-interactive call, reported after the fact.

Every test here asserts the withholding direction: the interesting outcome is always
"the vault did NOT reach that command", never "sync-agents crashed".

**Four rounds of review then found the same class of hole in the fixes themselves, and it
was the same mistake every time: the guard was put on the FIELD that had just been attacked
rather than on the SURFACE it is printed on.** Round one matched a NAME (``""`` was blank,
``"   "`` was not). Round two matched a CHARACTER and a LIST (``str.isprintable`` plus a
regex over the ASCII space; a tuple of four blank strings) and U+3164 walked past both.
Round three matched the right class — every codepoint outside printable ASCII — but matched
it inside `describe`, while the ``persona/server`` label sharing that row went to the
terminal untouched, and while `secrets` stayed in the digest and off the line. Round three
then shipped a LONGER LIST — seven fields on the line against a digest over the whole entry
— and the vault, ``env`` VALUES, ``cwd`` and a clipped tail each walked past it in turn.

Round four stops adding fields. `fingerprint` is the SHA-256 of `describe`'s output, so
*two entries that render the same consent line have the same fingerprint* is true by
construction, and `describe` loops over the entry's keys rather than over a list. See
`TheDigestIsTheLine`, which is where that property and its converse are asserted.

So the tests that hold the line here are of three shapes and none of them is a list of bad
inputs: a SWEEP over all 1,114,112 codepoints for what the escapes must catch (and, for
`_esc`, that they are reversible); a SURFACE assertion over the whole printed transcript of
a real `sync-agents` run for where committed text is allowed to appear; and a PROPERTY over
the relation between the line and the digest. A field added to the consent line later is
covered by all three without anybody remembering to add it to any of them.
"""

from __future__ import annotations

import hashlib
import io
import json
import re
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_persona, config, mcpseen, persona
from tests._isolation import PersonaIso

VAULT = "reddit"

#: Charter's own colour codes, which `util` adds and a terminal does not print. Stripped
#: before a line is measured, so the width asserted is the width the operator sees.
_ANSI = re.compile(r"\x1b\[[0-9;]*m")

STDIO = {
    "type": "stdio",
    "command": "uvx",
    "args": ["some-reddit-mcp", "--read-only"],
    "secrets": {"REDDIT_CLIENT_ID": "client-id"},
}

HTTP = {
    "type": "http",
    "url": "https://api.acme.example/mcp",
    "secrets": {"ACME_TOKEN": "acme-token"},
}


class _Tty(io.StringIO):
    """A stdin that claims to be a terminal. Explicit in every test that cares, because
    the REAL stdin is a tty when the suite is run by hand and a pipe in CI — a test that
    reads the ambient answer passes for the wrong reason in one of the two."""

    def isatty(self) -> bool:
        return True


class ApprovalBase(PersonaIso):
    def _persona(self, entry=None, name="reddit", server="reddit"):
        self.make_persona(name, role="R", vault=VAULT, **{"delegate-when": "things"})
        self._write(name, {server: entry if entry is not None else dict(STDIO)})
        return name

    def _write(self, name, servers):
        (persona.dir_of(name) / "mcp.json").write_text(
            json.dumps({"mcpServers": servers}))

    def _entry(self, name="reddit", server="reddit"):
        return persona.mcp_servers(name)[server]

    def _render(self, name="reddit", server="reddit"):
        return persona.mcp_render_entry(name, VAULT, self._entry(name, server))

    def _approve(self, name="reddit"):
        mcpseen.approve(name, [fp for _s, _e, fp, _l in persona.mcp_credentialed(name) if fp])

    def _sync(self, stdin, answers=None, **flags):
        """A real `sync-agents --approve-mcp` run: its transcript and what it recorded.

        On the base class rather than beside the tests that answer prompts, because the
        end-to-end run is the only place that shows the LINE THE OPERATOR READS — and
        three rounds of this fix were shipped on `describe` looking well-behaved in
        isolation while the printed row said something else."""
        from contextlib import redirect_stderr, redirect_stdout
        buf, out = io.StringIO(), io.StringIO()
        args = SimpleNamespace(persona=None, approve_mcp=True,
                               yes=flags.get("yes", False),
                               dry_run=flags.get("dry_run", False))
        replies = list(answers or [])
        self.asked = 0   # the question itself lands on stderr, with everything else

        def fake_input(prompt=""):
            self.asked += 1
            if not replies:
                raise EOFError
            return replies.pop(0)

        with mock.patch("sys.stdin", stdin), \
                mock.patch("builtins.input", fake_input), \
                redirect_stdout(out), redirect_stderr(buf):
            rc = commands_persona.cmd_persona_sync_agents(args)
        return rc, buf.getvalue()

    def assertWrapped(self, out, msg=""):
        self.assertEqual(out.get("command"), "charter", msg or "expected the vault wrapper")
        self.assertEqual((out.get("args") or [])[:3], ["secret", "exec", VAULT])

    def assertWithheld(self, out, msg=""):
        self.assertNotEqual(out.get("command"), "charter",
                            msg or "expected the vault to be withheld")
        self.assertNotIn("secret", out.get("args") or [])


class TheDigestCoversTheWholeEntry(ApprovalBase):
    """#426. Not "``env`` is in the digest now" — *every* key is, including the ones
    charter has not been taught about yet."""

    def test_an_env_edit_lapses_the_approval(self):
        name = self._persona()
        self._approve()
        before = mcpseen.fingerprint(VAULT, self._entry())
        self.assertWrapped(self._render(), "precondition: approved and wrapped")

        hostile = dict(STDIO, env={"NODE_OPTIONS": "--require /tmp/x.js",
                                   "PATH": "/tmp/attacker-bin"})
        self._write(name, {"reddit": hostile})
        self.assertNotEqual(mcpseen.fingerprint(VAULT, self._entry()), before,
                            "an added `env` must change the digest")
        self.assertWithheld(self._render())

    def test_the_env_that_reaches_the_agent_file_is_the_one_that_was_digested(self):
        """Why `env` counts: `mcp_render_entry` keeps it, so it reaches the harness, which
        sets it on the `charter` process that `execvpe`s the server."""
        name = self._persona(dict(STDIO, env={"HTTPS_PROXY": "http://proxy.example:8080"}))
        self._approve()
        out = self._render()
        self.assertWrapped(out, "precondition: this exact entry was approved")
        self.assertEqual(out.get("env"), {"HTTPS_PROXY": "http://proxy.example:8080"},
                         "the rendered entry carries `env` — so the digest must too")
        self._write(name, {"reddit": dict(STDIO, env={"HTTPS_PROXY": "http://evil.example:8080"})})
        self.assertWithheld(self._render(), "a CHANGED env value must lapse the approval")

    def test_any_key_at_all_lapses_the_approval(self):
        """The class, not the demo. A key nobody has thought of yet is still digested,
        which is the property an allowlist of five fields could not have."""
        name = self._persona()
        self._approve()
        for key, value in (("env", {"A": "b"}),
                           ("url", "https://evil.example/mcp"),
                           ("type", "sse"),
                           ("cwd", "/tmp/elsewhere"),
                           ("headers", {"Authorization": "Bearer whatever"}),
                           ("a-key-charter-has-never-heard-of", "x")):
            with self.subTest(key=key):
                self._write(name, {"reddit": dict(STDIO, **{key: value})})
                self.assertWithheld(self._render(), f"an added `{key}` must lapse consent")

    def test_a_nested_edit_lapses_the_approval(self):
        """Deep, not shallow: the digest is recursive, so a change three levels down in a
        value charter never reads still counts."""
        name = self._persona(dict(STDIO, meta={"transport": {"proxy": "http://ok.example"}}))
        self._approve()
        self._write(name, {"reddit": dict(
            STDIO, meta={"transport": {"proxy": "http://evil.example"}})})
        self.assertWithheld(self._render())

    def test_reordering_keys_does_not_lapse_the_approval(self):
        """The other direction, which is what makes the digest usable: JSON object order
        is not meaning, and a prompt that fires on a re-serialised file is a prompt the
        operator learns to answer without reading."""
        entry = dict(STDIO, env={"A": "1", "B": "2"})
        shuffled = {k: entry[k] for k in reversed(list(entry))}
        shuffled["env"] = {"B": "2", "A": "1"}
        self.assertEqual(mcpseen.fingerprint(VAULT, entry),
                         mcpseen.fingerprint(VAULT, shuffled))

    def test_arg_order_does_lapse_the_approval(self):
        """A list IS meaning: `--allow x --deny y` is not `--deny x --allow y`."""
        a = dict(STDIO, args=["mcp", "--allow", "x"])
        b = dict(STDIO, args=["--allow", "x", "mcp"])
        self.assertNotEqual(mcpseen.fingerprint(VAULT, a), mcpseen.fingerprint(VAULT, b))

    def test_nothing_to_consent_to_is_still_none(self):
        entry = {"type": "stdio", "command": "uvx", "args": ["some-reddit-mcp"]}
        self.assertIsNone(mcpseen.fingerprint(VAULT, entry))
        self.assertIsNone(mcpseen.fingerprint(None, dict(STDIO)))

    def test_the_digest_survives_a_value_json_cannot_carry(self):
        """`fingerprint` is called on whatever a committed file parsed to, and the module
        promises nothing here raises."""
        self.assertIsNotNone(mcpseen.fingerprint(VAULT, dict(STDIO, weird=object())))


def _unescape(shown: str) -> str:
    """`mcpseen._esc` read backwards. A left inverse is what "reversible" MEANS, so the
    property is checked by decoding rather than by asserting a shape."""
    out, i = [], 0
    while i < len(shown):
        c = shown[i]
        if c != "\\":
            out.append(c)
            i += 1
        elif shown[i + 1] == "\\":
            out.append("\\")
            i += 2
        elif shown[i + 1] == '"':
            out.append('"')
            i += 2
        elif shown[i + 1] == "u":
            out.append(chr(int(shown[i + 2:i + 6], 16)))
            i += 6
        elif shown[i + 1] == "U":
            out.append(chr(int(shown[i + 2:i + 10], 16)))
            i += 10
        else:
            raise AssertionError(f"undecodable escape at {shown[i:i + 8]!r}")
    return "".join(out)


class TheDigestIsTheLine(ApprovalBase):
    """Round four, and the one property the three rounds before it each restated and each
    left unimplemented.

    Rounds one to three digested one representation of the entry and printed a different,
    shorter one — a total digest against a line enumerating seven fields. Every bypass
    since has been one instance of that single gap, and each round closed the instance it
    was handed: the vault name, digested from the first commit and never printed; `env`
    VALUES, digested while only their KEYS were shown; `cwd`, and whatever the next
    committed key turns out to be; and a clipped tail that made two different `args` read
    the same. In each, the operator was re-asked under a line byte-identical to the one
    they had already approved — which is not consent, it is a second chance to make the
    same mistake.

    So there is no second representation any more. `fingerprint` is the SHA-256 of
    `describe`'s output and nothing else is mixed in, which makes the first test below
    true by construction rather than by inspection:

        *two entries that render the same consent line have the same fingerprint.*

    That moves the whole weight onto `describe` being TOTAL — a key it fails to print is a
    key a commit may change with the approval intact — which is the second test, and it is
    written over `entry.keys()` rather than over a list, so the key nobody has thought of
    yet is covered without being found by hand.

    What is NOT claimed here, because `SECURITY.md` does not claim it: that reading the
    line makes the operator understand it, or that anything stops somebody who already
    runs code as this user. This is a guard against a commit, answered by a person.
    """

    #: A rich entry: charter's readable fields, plus keys it has never been taught, plus a
    #: nested one. Used by the totality test, so a field added to `describe` later does not
    #: need to be added here to be covered — the loop is over the keys of this dict.
    RICH = {
        "type": "stdio", "command": "uvx", "args": ["some-reddit-mcp", "--read-only"],
        "url": "https://api.acme.example/mcp",
        "env": {"PATH": "/usr/bin", "NODE_OPTIONS": "--max-old-space-size=4096"},
        "secrets": {"REDDIT_CLIENT_ID": "client-id"},
        "secret_files": {"GOOGLE_APPLICATION_CREDENTIALS": "prod-sa-json"},
        "cwd": "/home/me/proj",
        "headers": {"Authorization": "Bearer whatever"},
        "meta": {"transport": {"proxy": "http://ok.example"}},
        "keepAlive": True, "timeout": 30, "note": None,
    }

    #: Pairs that MUST read differently, one axis each. Every one of them printed a single
    #: identical line before this commit except where noted, and each was reproduced end to
    #: end through `sync-agents --approve-mcp` on the branch this replaces.
    DIFFERENT = [
        ("the vault", ("reddit-low", dict(STDIO)), ("prod-aws", dict(STDIO))),
        ("an env value", ("v", dict(STDIO, env={"PATH": "/usr/bin"})),
                         ("v", dict(STDIO, env={"PATH": "/tmp/attacker-bin:/usr/bin"}))),
        ("an env pair boundary", ("v", dict(STDIO, env={"A": "b, C=d"})),
                                 ("v", dict(STDIO, env={"A": "b", "C": "d"}))),
        ("an untaught key", ("v", dict(STDIO, cwd="/home/me/proj")),
                            ("v", dict(STDIO, cwd="/tmp/attacker"))),
        ("a clipped tail", ("v", dict(STDIO, args=["-c{" + "b" * 195 + '"allow": false}'])),
                           ("v", dict(STDIO, args=["-c{" + "b" * 195 + '"allow": true }']))),
        ("a run of spaces", ("v", dict(STDIO, args=["--x", "a b"])),
                            ("v", dict(STDIO, args=["--x", "a  b"]))),
        ("a leading space", ("v", dict(STDIO, command="/bin/sh")),
                            ("v", dict(STDIO, command="  /bin/sh"))),
        ("where the words split", ("v", dict(STDIO, command="a b", args=[])),
                                  ("v", dict(STDIO, command="a", args=["b"]))),
        ("an empty word", ("v", dict(STDIO, args=["x"])),
                          ("v", dict(STDIO, args=["x", ""]))),
        ("arg order", ("v", dict(STDIO, args=["mcp", "--allow", "x"])),
                      ("v", dict(STDIO, args=["--allow", "x", "mcp"]))),
        ("a homoglyph url", ("v", dict(HTTP, url="https://api.acme.example/mcp")),
                            ("v", dict(HTTP, url="https://api.асme.example/mcp"))),
        ("a forged escape", ("v", dict(STDIO, args=["\\u3164"])),
                            ("v", dict(STDIO, args=["ㅤ"]))),
        ("the vault key", ("v", dict(STDIO, secrets={"R": "client-id"})),
                          ("v", dict(STDIO, secrets={"R": "aws-root-key"}))),
        ("a nested value", ("v", dict(STDIO, meta={"t": {"proxy": "http://ok.example"}})),
                           ("v", dict(STDIO, meta={"t": {"proxy": "http://evil.example"}}))),
        ("a bool and its spelling", ("v", dict(STDIO, verify=True)),
                                    ("v", dict(STDIO, verify="true"))),
        ("an int and its spelling", ("v", dict(STDIO, port=1)),
                                    ("v", dict(STDIO, port="1"))),
        ("null and its spelling", ("v", dict(STDIO, note=None)),
                                  ("v", dict(STDIO, note="null"))),
        ("a committed key spelling charter's word",
         ("v", dict(STDIO)), ("v", dict(STDIO, vault="prod-aws"))),
        ("a committed value spelling a segment",
         ("v", dict(STDIO, args=['x  vault "v"'])), ("v", dict(STDIO, args=["x"]))),
    ]

    #: Pairs that MUST read the same, and therefore share one approval. Both are
    #: deliberate: neither can change the argv `execvpe` receives, and a prompt that fires
    #: on a re-serialised file is a prompt the operator learns to answer without reading.
    SAME = [
        ("key order", ("v", {"command": "uvx", "args": ["m"], "secrets": {"A": "k"},
                             "env": {"A": "1", "B": "2"}}),
                      ("v", {"env": {"B": "2", "A": "1"}, "secrets": {"A": "k"},
                             "args": ["m"], "command": "uvx"})),
        ("an empty argv and none at all",
         ("v", {"command": "uvx", "args": [], "secrets": {"A": "k"}}),
         ("v", {"command": "uvx", "secrets": {"A": "k"}})),
    ]

    def test_two_entries_that_render_the_same_consent_line_have_the_same_fingerprint(self):
        """The invariant, stated as the attacker stated it. It reddens on env values, on
        `cwd`, on a clipped tail, on the vault and on every future key at once, which is
        what makes it a property rather than a nineteenth instance.

        Both directions are asserted over the same table, because a rule that only ever
        fires one way is a rule nothing holds to the other: an entry that reads the same
        must not lapse the approval, and an entry that reads differently must."""
        cases = ([(f"{why} — same", a, b, True) for why, a, b in self.SAME]
                 + [(f"{why} — different", a, b, False) for why, a, b in self.DIFFERENT])
        for why, (v1, e1), (v2, e2), same in cases:
            with self.subTest(why=why):
                l1, l2 = mcpseen.describe(v1, e1), mcpseen.describe(v2, e2)
                f1, f2 = mcpseen.fingerprint(v1, e1), mcpseen.fingerprint(v2, e2)
                self.assertTrue(l1 and l2, "precondition: both are renderable")
                self.assertEqual(l1 == l2, same, f"{l1!r}\n{l2!r}")
                # The invariant itself, asserted on every pair rather than on the ones
                # expected to collide: equal lines may never carry different digests.
                self.assertEqual(f1 == f2, l1 == l2,
                                 "the digest and the line must agree, always")

    def test_every_key_of_the_entry_reaches_the_line(self):
        """Totality — the half the invariant moves all the weight onto.

        Hashing the line is only safe if the line holds everything the harness receives:
        a key `describe` skips is a key a commit may change with the approval intact,
        which is the bug with the arrow pointing the other way. `mcp_render_entry` passes
        every key it does not consume straight through, so the test is over the KEYS of
        the entry — including two nobody has ever named — and not over a list."""
        keys = list(self.RICH) + ["a-key-charter-has-never-heard-of", "x-next-round"]
        base = mcpseen.describe(VAULT, self.RICH)
        self.assertTrue(base, "precondition: the rich entry renders")
        for key in keys:
            with self.subTest(key=key):
                changed = dict(self.RICH)
                changed[key] = ["a value nothing in charter reads", key]
                self.assertNotEqual(mcpseen.describe(VAULT, changed), base,
                                    f"`{key}` can be re-pointed under an unchanged line")
                dropped = {k: v for k, v in self.RICH.items() if k != key}
                if key in self.RICH:
                    self.assertNotEqual(mcpseen.describe(VAULT, dropped), base,
                                        f"`{key}` can be removed under an unchanged line")

    def test_the_escape_is_reversible(self):
        """What `_esc` has to be, now that the digest is taken over its output: a left
        inverse exists, so the characters printed for a committed value determine that
        value. Round three's escape was not — it collapsed runs of ASCII spaces and
        stripped the ends, which is fine for deciding "does this render as nothing" and
        wrong for deciding "is this the command I approved"."""
        for cp in range(0x110000):
            raw = "x" + chr(cp) + "\\\"" + chr(cp) * 2
            shown = mcpseen._esc(raw)
            if not all(" " <= c <= "~" for c in shown):
                self.fail(f"U+{cp:04X} reached the consent line unescaped: {shown!r}")
            if _unescape(shown) != raw:
                self.fail(f"U+{cp:04X} does not survive the round trip: {shown!r}")
        for raw in ("", " ", "   ", "  x  ", "a  b", '"', "\\", "\\u3164", "\\\\u3164",
                    "\U0001f600", chr(0x1F60) + "0"):
            self.assertEqual(_unescape(mcpseen._esc(raw)), raw, repr(raw))
        # And the delimiter property the line's shape rests on: an UNESCAPED quote is
        # charter's, so a committed value cannot close its own quotes and impersonate a
        # whole segment. Escapes are removed first, since `\\"` is not a delimiter.
        escapes = re.compile(r'\\\\|\\"|\\u[0-9a-f]{4}|\\U[0-9a-f]{8}')
        for raw in ('a"b', "a\\", '"', '\\"', 'x"  vault "v', "\\" * 5 + '"'):
            bare = escapes.sub("", mcpseen._esc(raw))
            self.assertNotIn('"', bare, f"{raw!r} spells a delimiter")
            self.assertNotIn("\\", bare, f"{raw!r} leaves a backslash no escape claims")

    def test_the_line_printed_is_the_line_recorded(self):
        """End to end, and the reason `mcp_credentialed` hands back the line rather than
        the entry. Two renderings of the same entry — one for the prompt, one for the
        digest — is exactly the drift this whole fix removes, so there is one."""
        name = self._persona(dict(STDIO, env={"PATH": "/tmp/attacker-bin"}))
        rc, err = self._sync(_Tty(), answers=["y"])
        self.assertEqual(rc, 0)
        shown = [ln for ln in err.splitlines() if "reddit/reddit →" in ln]
        self.assertEqual(len(shown), 1, err)
        line = shown[0].split("→", 1)[1].strip()
        self.assertIn('"PATH"="/tmp/attacker-bin"', line, "the value, not only the key")
        self.assertIn(f'vault "{VAULT}"', line, "whose credential is being spent")
        self.assertEqual(mcpseen.approved(name),
                         {hashlib.sha256(line.encode()).hexdigest()},
                         "what was recorded is the sha256 of what was printed")

    def test_re_pointing_the_vault_is_a_line_the_operator_can_see(self):
        """Finding one, end to end. `vault:` is a key of the COMMITTED `persona.md`, so a
        one-line commit re-points which credential is spent. It was digested from the
        first commit and printed by none, so the operator was re-asked under a
        byte-identical line — and could not have told which vault was at stake the FIRST
        time either, while the line printed charter's own word `(vault: …)` in front of
        the variable name, where a reader has every reason to expect the vault to be."""
        name = self._persona()
        self._approve()
        self.assertWrapped(self._render(), "precondition: approved under vault `reddit`")
        before = mcpseen.describe(VAULT, self._entry())

        # `_render` is called with the vault the persona would resolve to; re-pointing
        # `vault:` in persona.md is the same substitution one frame up.
        after = mcpseen.describe("prod-aws", self._entry())
        self.assertNotEqual(after, before, "the re-point has to be readable")
        self.assertIn('vault "prod-aws"', after)
        self.assertNotEqual(mcpseen.fingerprint("prod-aws", self._entry()),
                            mcpseen.fingerprint(VAULT, self._entry()))
        self.assertWithheld(persona.mcp_render_entry(name, "prod-aws", self._entry()),
                            "and the approval does not travel with the entry")


class AnHttpServerHasAConsentLine(ApprovalBase):
    """#427. `describe` fed the line the operator is told to read; for `http`/`sse` it fed
    an empty string."""

    def test_an_http_server_has_a_nonempty_consent_line(self):
        line = mcpseen.describe(VAULT, HTTP)
        self.assertIn("https://api.acme.example/mcp", line)
        other = dict(HTTP, url="https://evil.example.net/mcp")
        self.assertNotEqual(mcpseen.fingerprint(VAULT, HTTP),
                            mcpseen.fingerprint(VAULT, other),
                            "two endpoints must not share one approval")

    def test_the_consent_line_names_the_env_keys(self):
        line = mcpseen.describe(VAULT, dict(STDIO, env={"PATH": "/tmp/bin", "NODE_OPTIONS": "-r x"}))
        self.assertIn("PATH", line)
        self.assertIn("NODE_OPTIONS", line)

    def test_the_consent_line_names_the_vault_key_it_would_hand_over(self):
        """The next spelling of the homoglyph finding, one field further in — and the one
        this test used to assert the wrong way round.

        `secrets` is `{ENV_VAR: vault-key}`, so `client-id` below is a KEY NAME, and it is
        the field that decides WHICH credential the command receives —
        `mcp_render_entry` turns it into `secret exec <vault> --env REDDIT_CLIENT_ID=client-id`.
        It is in the digest, so editing it lapses the approval and the operator is asked
        again; it was not on the line, so the line they were asked under was byte-identical
        to the one they had already approved. Being re-asked under an unchanged line is a
        second chance to make the same mistake, which is finding three's shape exactly.

        The credential's VALUE cannot reach the line, and not because anything strips it:
        `describe` is a pure function of the entry, the value is not in the entry, and this
        module never opens a vault. The last assertion pins that — every token on the line
        came either out of the entry or out of charter's own words."""
        entry = dict(STDIO, secrets={"REDDIT_CLIENT_ID": "client-id"})
        line = mcpseen.describe(VAULT, entry)
        self.assertIn("uvx", line)
        self.assertIn('"REDDIT_CLIENT_ID"="client-id"', line)

        repointed = dict(STDIO, secrets={"REDDIT_CLIENT_ID": "aws-root-key"})
        self.assertNotEqual(mcpseen.fingerprint(VAULT, entry),
                            mcpseen.fingerprint(VAULT, repointed),
                            "precondition: re-pointing the credential does re-ask")
        self.assertNotEqual(line, mcpseen.describe(VAULT, repointed),
                            "and the line it re-asks under has to have changed too")

        # `secret_files` is the same decision through a different mechanism (#190): a path
        # to a materialised file rather than a value. Same key, same need to show it.
        files = mcpseen.describe(VAULT, dict(STDIO, secret_files={"GOOGLE_APPLICATION_CREDENTIALS":
                                                          "prod-sa-json"}))
        self.assertIn('"GOOGLE_APPLICATION_CREDENTIALS"="prod-sa-json"', files)

        # Charter's own words, plus the vault NAME — which is on the line as of round four
        # and comes out of the committed `persona.md`, not out of any vault.
        charters_own = set(mcpseen._WORDS) | {VAULT, "null", "true", "false"}
        for token in re.findall(r"[A-Za-z0-9_./-]{3,}", line):
            self.assertTrue(token in json.dumps(entry) or token in charters_own,
                            f"{token!r} reached the consent line from outside the entry")

    def test_control_characters_cannot_repaint_the_line(self):
        """The next input through this door. The line IS the consent, so a committed
        `args` carrying \\r, an ANSI erase, or a bidi override must not be able to show
        the operator something other than what would run."""
        for token in ("\rharmless", "\x1b[2Kharmless", "a‮b", "x\ny"):
            with self.subTest(token=token):
                line = mcpseen.describe(VAULT, dict(STDIO, args=[token]))
                self.assertNotIn("\r", line)
                self.assertNotIn("\n", line)
                self.assertNotIn("\x1b", line)
                self.assertNotIn("‮", line)
                self.assertIn("uvx", line, "the real command still shows")

    def test_nothing_a_long_part_hides_is_hidden_by_a_shorter_line(self):
        """Round one cut the FINISHED line at 600 characters and both `[type url]` and
        `(env: …)` were appended after `args`, so ~600 characters of plausible `args` in a
        committed file produced a consent line naming neither the `env` it set nor the
        `url` it pointed at. Round two gave each part its own 200-character budget and
        announced the cut, which bounded the line but was not one-to-one: see
        `test_a_clipped_tail_is_not_a_tail_anybody_read` below.

        Round four's answer is neither. A line is complete or there is no line — so a
        padded part cannot shorten another part, because no part is ever shortened."""
        entry = dict(STDIO, args=["x" * 100000], url="https://evil.example/mcp",
                     env={"PATH": "/tmp/attacker-bin", "NODE_OPTIONS": "-r /tmp/x.js"})
        self.assertEqual(mcpseen.describe(VAULT, entry), "",
                         "an entry that cannot be shown in full is not shown at all")
        self.assertIsNone(mcpseen.fingerprint(VAULT, entry))

        # The same fields, at a size that fits: all of them are named, values included.
        entry = dict(STDIO, args=["x" * 100], url="https://evil.example/mcp",
                     env={"PATH": "/tmp/attacker-bin", "NODE_OPTIONS": "-r /tmp/x.js"})
        line = mcpseen.describe(VAULT, entry)
        self.assertTrue(line.startswith("run uvx "), "the command stays at the front")
        self.assertNotIn("more chars", line, "and nothing was cut out of it")
        for named in ("PATH", "/tmp/attacker-bin", "NODE_OPTIONS", "-r /tmp/x.js",
                      "evil.example", "x" * 100):
            self.assertIn(named, line, f"{named} chooses the destination and must show")

    def test_a_clipped_tail_is_not_a_tail_anybody_read(self):
        """Finding four of round four, and the reason clipping is gone rather than fixed.

        `_clip` cut each part at 200 characters and announced how many it had dropped —
        which is not one-to-one: two args agreeing on their first 200 escaped characters
        and equal in length printed the same text AND the same `(+N more chars)` count.
        Two different `--config` payloads, one identical line. Round three could survive
        that (the digest was taken over the entry); once the digest IS the line, it would
        be one approval covering an entry the operator never saw."""
        head = "--config {" + "b" * 195
        a, b = head + '"allow_remote_code": false}', head + '"allow_remote_code": true }'
        self.assertEqual(len(a), len(b), "precondition: same length, same clipped prefix")
        lines = [mcpseen.describe(VAULT, dict(STDIO, args=[x])) for x in (a, b)]
        self.assertNotEqual(lines[0], lines[1], "the tail that differs must be readable")
        self.assertIn("allow_remote_code\\\": false", lines[0])
        self.assertIn("allow_remote_code\\\": true", lines[1])

    def test_a_line_too_long_to_read_is_refused_rather_than_cut(self):
        """Cutting would put charter back to choosing which half of the destination the
        operator sees, so it fails closed instead: no line, no digest, no approval."""
        flood = dict(STDIO, args=["y" * 60] * 200)
        self.assertEqual(mcpseen.describe(VAULT, flood), "")
        self.assertIsNone(mcpseen.fingerprint(VAULT, flood))

        name = self._persona(flood)
        mcpseen.approve(name, ["deadbeef" * 8])
        self.assertWithheld(self._render())

    def test_a_consent_line_is_printable_ascii_and_nothing_else(self):
        """The class, swept — not a list, sampled. This guard has now been walked past
        twice by the same attack in a new spelling: round one matched a NAME (`""`),
        round two matched a CHARACTER (`str.isprintable` plus a regex over the ASCII
        space) and U+3164 HANGUL FILLER walked through, being printable, not whitespace,
        `strip`-proof, and blank on every terminal.

        So the assertion is over the COMPLEMENT rather than over examples: printable
        ASCII is what a consent line may contain, and `_safe` must escape every one of
        the 1,114,112 codepoints Python can hold that is not in it. The second assertion
        is the one that makes "renders as nothing" decidable: on the escaped form, blank
        means all-ASCII-spaces and nothing else can spell it.
        """
        for cp in range(0x110000):
            out = mcpseen._safe(chr(cp) * 3)
            if not all(" " <= c <= "~" for c in out):
                self.fail(f"U+{cp:04X} reached the consent line unescaped: {out!r}")
            if (out == "") != (cp == 0x20):
                self.fail(f"U+{cp:04X} renders as {out!r}: blank must mean ASCII space "
                          f"and only ASCII space, or the next codepoint is the next bypass")

    def test_two_different_commands_never_read_the_same(self):
        """The homoglyph finding, generalised and swept. A consent line is worth reading
        only if reading it DISTINGUISHES what would run, so the escaping has to be
        one-to-one. Two ways it was not, both closed here: `\\u1f600` is five hex digits,
        so U+1F600 and U+1F60 followed by `"0"` spelled the same escape; and a committed
        `command` holding the six LITERAL characters `\\u3164` spelled the same line as
        one holding U+3164, so the escape could be forged in plain ASCII."""
        # The structural property that makes one-to-one hold for inputs of ANY length:
        # every escape is a fixed-width form, so no escape is a prefix of another with
        # the next input character glued on. `\\u1f600` is what violating it looks like.
        form = re.compile(r"\\u[0-9a-f]{4}|\\U[0-9a-f]{8}")
        seen = {}
        for cp in range(0x110000):
            ch = chr(cp)
            if not (" " <= ch <= "~"):
                esc = mcpseen._escape(ch)
                if not form.fullmatch(esc):
                    self.fail(f"U+{cp:04X} escapes to {esc!r}, which is not a fixed-width "
                              f"form — a shorter escape plus the next character spells it")
            out = mcpseen._safe("x" + ch + "x")
            prior = seen.setdefault(out, cp)
            if prior != cp:
                self.fail(f"U+{prior:04X} and U+{cp:04X} both read as {out!r}")

        # And the two concrete pairs the structure rules out, spelled in full so the
        # reason survives a rewrite of the loop above.
        self.assertNotEqual(mcpseen._safe(chr(0x1F600)), mcpseen._safe(chr(0x1F60) + "0"))
        for literal in ("\\u3164", "\\U0001f600", "\\\\"):
            out = mcpseen._safe("x" + literal + "x")
            if out in seen:
                self.fail(f"the literal text {literal!r} reads as U+{seen[out]:04X} — a "
                          f"forged escape is a command the operator cannot identify")
            self.assertNotEqual(out, mcpseen._safe("x" + chr(0x3164) + "x"))

    def test_a_destination_that_renders_as_nothing_is_refused(self):
        """The end-to-end half of the sweep above, through the real approval path. The
        listed spellings are illustrations of the class, not the guard — the guard is
        the sweep. `\u00a0`, `\u3164`, `\u2800`, `\u115f` and `\u1160` are each here
        because a previous round of this fix shipped while one of them worked."""
        blanks = ("   ", " " * 601, " ")
        for blank in blanks:
            with self.subTest(blank=repr(blank)):
                entry = {"type": "stdio", "command": blank, "args": ["  ", ""],
                         "url": "  ", "secrets": {"ACME_TOKEN": "acme-token"}}
                self.assertEqual(mcpseen.describe(VAULT, entry), "", "a blank line is no line")
                self.assertIsNone(mcpseen.fingerprint(VAULT, entry))

        for shown in ("\u00a0", "\u3164", "\u2800", "\u115f", "\u1160", "\u0301"):
            with self.subTest(shown=repr(shown)):
                # Not blank: SHOWN. A codepoint outside printable ASCII is a destination
                # the operator gets to see spelled out, which is the whole point of
                # escaping rather than of stripping.
                line = mcpseen.describe(VAULT, {"type": "stdio", "command": shown * 3,
                                                "secrets": {"ACME_TOKEN": "acme-token"}})
                self.assertEqual(line.split("  ")[0],
                                 "run " + f"\\u{ord(shown):04x}" * 3)

        name = self._persona({"type": "stdio", "command": "   ",
                              "secrets": {"ACME_TOKEN": "acme-token"}})
        mcpseen.approve(name, ["deadbeef" * 8])
        self.assertWithheld(self._render())

    def test_a_homoglyph_repoint_reads_differently_from_what_it_replaced(self):
        """The re-prompt already fires — the digest covers the url, so re-pointing
        `api.acme.example` at Cyrillic `api.асme.example` lapses the approval and the
        operator IS asked again. What was missing is that the two lines were byte-for-byte
        different and pixel-for-pixel identical, so reading the line could not tell the
        operator what had changed. Escaping is what makes the re-prompt answerable."""
        ascii_url = "https://api.acme.example/mcp"
        cyrillic = "https://api.\u0430\u0441me.example/mcp"
        self.assertNotEqual(ascii_url, cyrillic, "precondition: different strings")

        lines = [mcpseen.describe(VAULT, dict(HTTP, url=u)) for u in (ascii_url, cyrillic)]
        self.assertNotEqual(lines[0], lines[1], "the two lines must READ differently")
        self.assertIn("acme.example", lines[0])
        self.assertIn("\\u0430\\u0441me.example", lines[1], "the lookalike is spelled out")
        self.assertNotIn("\u0430", lines[1], "and the glyph itself never reaches the tty")

    def test_padding_cannot_scroll_the_destination_off_the_screen(self):
        """Finding 2, and the next spelling of it. Nine args of 200 invisible columns fit
        under the old 2000-character ceiling as a 1837-character line: 22 blank rows on an
        80x24 tty, with `uvx evil-server` scrolled off the top by the time the prompt was
        answered. Escaping makes that padding visible — and visible padding scrolls a line
        exactly as far, so the ceiling has to be the SCREEN the question is asked on.
        Every filler below is refused for the same reason and not for four reasons."""
        # Deliberately NOT `mcpseen.MAX_LINE`: the budget this test defends is a screen,
        # so it is spelled here in rows and columns. Half of an 80x24 terminal, which
        # leaves the prompt, the answer and some of the sync output visible with it.
        screen = 80 * 12

        for filler in ("x", "\u3164", "\u2800", " ", "\u0301", "\U0001f600", "\t"):
            with self.subTest(filler=repr(filler)):
                entry = dict(STDIO, args=["evil-server"] + [filler * 200] * 9,
                             env={"PATH": "/tmp/attacker-bin"})
                line = mcpseen.describe(VAULT, entry)
                # Two acceptable outcomes and no third: the padding collapses to nothing
                # (only the ASCII space does that) and the line is short, or the line does
                # not fit the screen and there is no line and no digest. What is refused
                # in every case is the middle: a line long enough to scroll the command
                # it names off the top of the terminal the prompt is printed on.
                self.assertLessEqual(len(line), screen,
                                     f"{len(line) // 80} rows of destination is a page "
                                     f"the operator scrolls, not a line they read")
                if line:
                    self.assertTrue(line.startswith("uvx evil-server"), line[:40])
                    self.assertIn("PATH", line, "and the env still shows")
                else:
                    self.assertIsNone(mcpseen.fingerprint(VAULT, entry))
        self.assertLessEqual(mcpseen.MAX_LINE, mcpseen.MAX_COLS * mcpseen.MAX_ROWS,
                             "the ceiling and the screen it names must agree")

        name = self._persona(dict(STDIO, args=["evil-server"] + ["\u3164" * 200] * 9,
                                  env={"PATH": "/tmp/attacker-bin"}))
        mcpseen.approve(name, ["deadbeef" * 8])
        self.assertWithheld(self._render())

    def test_a_real_entry_still_renders_under_the_screen_ceiling(self):
        """The other direction of the ceiling: fail-closed is only acceptable if it does
        not close on the servers people actually run. A fat-but-honest docker entry with
        several env keys still fits."""
        line = mcpseen.describe(VAULT, {
            "type": "stdio", "command": "docker",
            "args": ["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN",
                     "-e", "GITHUB_TOOLSETS", "ghcr.io/github/github-mcp-server", "stdio"],
            "env": {"PATH": "/usr/local/bin:/usr/bin", "HTTPS_PROXY": "http://p.example:3128",
                    "NODE_OPTIONS": "--max-old-space-size=4096"},
            "secrets": {"GITHUB_PERSONAL_ACCESS_TOKEN": "gh-pat"}})
        self.assertTrue(line.startswith("run docker run -i --rm"), line[:40])
        for named in ("ghcr.io/github/github-mcp-server", "PATH", "/usr/local/bin:/usr/bin",
                      "NODE_OPTIONS", "--max-old-space-size=4096"):
            self.assertIn(named, line)
        self.assertLessEqual(len(line), mcpseen.MAX_LINE)

    def test_padding_with_spaces_is_shown_or_refused_but_never_tidied_away(self):
        """Round three stripped and collapsed ASCII spaces out of every part, so
        `"   ...   --evil"` printed as `--evil` — a line that no longer said what would
        run, and (once the digest is the line) one approval covering both spellings.

        Padding is committed content now, like any other: it is shown between the quotes
        that mark the word's boundaries, it costs the columns it occupies, and enough of
        it puts the entry over the screen ceiling and gets it refused."""
        padded = mcpseen.describe(VAULT, dict(STDIO, args=["  --evil"]))
        self.assertIn('"  --evil"', padded, "the padding is shown, not stripped")
        self.assertNotEqual(padded, mcpseen.describe(VAULT, dict(STDIO, args=["--evil"])),
                            "two different argv must not share one line")

        line = mcpseen.describe(VAULT, dict(STDIO, args=[" " * 600 + "--evil"]))
        self.assertIn(" " * 600, line, "600 columns of padding cost 600 columns")

        # And past the ceiling there is no line at all, so nothing to answer blind.
        entry = dict(STDIO, args=[" " * 6000 + "--evil"])
        self.assertEqual(mcpseen.describe(VAULT, entry), "")
        self.assertIsNone(mcpseen.fingerprint(VAULT, entry))

    def test_an_entry_with_no_destination_cannot_be_approved(self):
        """The general case behind #427: a line `describe` cannot render is not a line
        anyone can consent to, so no digest exists for it and nothing can wrap it."""
        blind = {"type": "http", "secrets": {"ACME_TOKEN": "acme-token"}}
        self.assertEqual(mcpseen.describe(VAULT, blind), "")
        self.assertIsNone(mcpseen.fingerprint(VAULT, blind))

        name = self._persona(blind)
        mcpseen.approve(name, ["deadbeef" * 8])   # some earlier approval, whatever it was
        self.assertWithheld(self._render())

    def test_a_blind_entry_is_still_reported_as_withheld(self):
        """Not approvable must not mean invisible: it declares a secret, so the operator
        has to hear that it was refused."""
        name = self._persona({"type": "http", "secrets": {"ACME_TOKEN": "acme-token"}})
        withheld = persona.mcp_withheld(name)
        self.assertEqual([s for s, _e in withheld], ["reddit"])


class ApproveMcpAsksBeforeItRecords(ApprovalBase):
    """#428. The flag used to be its own answer."""

    def _two_servers(self, name="reddit"):
        self.make_persona(name, role="R", vault=VAULT, **{"delegate-when": "things"})
        self._write(name, {"reddit": dict(STDIO), "acme": dict(HTTP)})
        return name

    def test_approve_mcp_prompts_per_server_at_a_tty(self):
        name = self._two_servers()
        rc, err = self._sync(_Tty(), answers=["y", "n"])
        self.assertEqual(rc, 0)
        self.assertEqual(self.asked, 2, "one question per server, not one per run")
        self.assertIn("approve reddit/acme?", err)
        self.assertIn("approve reddit/reddit?", err)
        self.assertLess(err.index("approve reddit/acme?"),
                        err.index("approve reddit/reddit?"), "asked in sorted order")
        # `acme` was answered yes, `reddit` no.
        approved = mcpseen.approved(name)
        by_server = {s: fp for s, _e, fp, _l in persona.mcp_credentialed(name)}
        self.assertIn(by_server["acme"], approved)
        self.assertNotIn(by_server["reddit"], approved)
        self.assertWithheld(self._render(name, "reddit"), "the declined server stays dry")
        self.assertWrapped(self._render(name, "acme"))

    def test_the_line_is_printed_before_the_question(self):
        self._two_servers()
        _rc, err = self._sync(_Tty(), answers=["n", "n"])
        self.assertIn("https://api.acme.example/mcp", err,
                      "the http server's destination is shown, not a blank")
        self.assertIn("uvx some-reddit-mcp", err)

    def test_padded_args_cannot_hide_the_env_from_the_operator_who_answers(self):
        """The reviewer's input, end to end. A committed entry pads `args` so the line
        the operator reads names neither the `PATH` it re-points nor the endpoint it
        connects to — and the render they approve carries both. The question is not
        whether `describe` is well-behaved in isolation but whether the line PRINTED
        above the prompt names everything the approval hands over.

        Two outcomes and no third, asserted in that order. Padded past the screen the
        question is asked on: no line, no question, nothing approved. Inside it: every
        one of `env`'s KEYS **and VALUES** on the line the operator answers under, because
        the value is the half that decides — `PATH` picks which binary `execvpe` finds."""
        padded = {
            "type": "stdio", "command": "uvx",
            "args": ["--config", "{" + "a" * 640 + "}"],
            "env": {"PATH": "/tmp/attacker-bin", "NODE_OPTIONS": "--require /tmp/x.js"},
            "secrets": {"REDDIT_CLIENT_ID": "client-id"},
        }
        name = self._persona(padded)
        rc, err = self._sync(_Tty(), answers=["y"])
        self.assertEqual(rc, 0)
        self.assertEqual(self.asked, 0, "nothing showable, nothing to ask")
        self.assertIn(mcpseen.UNRENDERABLE, err)
        self.assertWithheld(self._render(name))

        self._write(name, {"reddit": dict(padded, args=["--config", "{}"])})
        rc, err = self._sync(_Tty(), answers=["y"])
        self.assertEqual(rc, 0)
        shown = [ln for ln in err.splitlines() if "reddit/reddit →" in ln]
        self.assertEqual(len(shown), 1, err)
        for named in ("PATH", "/tmp/attacker-bin",
                      "NODE_OPTIONS", "--require /tmp/x.js"):
            self.assertIn(named, shown[0],
                          f"{named} reaches execvpe, so it must reach the operator")
        rendered = self._render(name)
        self.assertWrapped(rendered, "precondition: the operator did approve it")
        self.assertEqual(sorted(rendered.get("env") or {}), ["NODE_OPTIONS", "PATH"],
                         "what was approved is what the harness gets")

    def test_the_operator_is_never_asked_under_a_line_that_prints_as_nothing(self):
        """The reviewer's round-two input, end to end and in its own words. A committed
        entry of `"\u3164" * 3` — HANGUL FILLER, printable, not whitespace, `strip`-proof,
        blank on every terminal — produced `  reddit/reddit → ` and nothing else, a real
        digest, and one prompt. Answering yes wrapped the entry in `charter secret exec`.

        The fix is not "ask zero questions about it". It is that the line printed above
        the question SAYS something, so that yes and no are both informed answers: the
        codepoint is spelled out, the operator sees a command made of nothing but
        escapes, and declining leaves the vault withheld."""
        name = self._persona({"type": "stdio", "command": "\u3164" * 3,
                              "args": ["\u3164" * 8],
                              "secrets": {"ACME_TOKEN": "acme-token"}})
        rc, err = self._sync(_Tty(), answers=["n"])
        self.assertEqual(rc, 0)

        # Two occurrences: the line above the prompt, and the same line in the withheld
        # report afterwards. Both are read by a person, so both are asserted.
        shown = [ln for ln in err.splitlines() if "reddit/reddit \u2192" in ln]
        self.assertEqual(len(shown), 2, err)
        for ln in shown:
            dest = ln.split("\u2192", 1)[1].strip()
            self.assertNotEqual(dest, "", "the operator was asked under a blank line")
            self.assertTrue(all(" " <= c <= "~" for c in dest),
                            f"a glyph the terminal gets to interpret reached it: {dest!r}")
            self.assertIn("\\u3164", dest, "the codepoint is spelled out, not shown")
        self.assertEqual(self.asked, 1, "and it was a real question, asked once")
        self.assertEqual(mcpseen.approved(name), set(), "no is still no")
        self.assertWithheld(self._render(name))

    def test_no_committed_text_reaches_the_transcript_as_a_glyph_or_as_a_page(self):
        """The SURFACE, not the field — which is the one lesson three rounds have each
        paid for. `describe` was hardened three times, and the `persona/server` label
        printed in front of it on the same row went to the terminal untouched all three
        times, because each round guarded the field that had just been attacked.

        Reproduced end to end before this existed, through this same fixture: a server
        named `"\u3164" * 3` printed `reddit/ \u2192 uvx some-reddit-mcp` with nothing
        between the slash and the arrow; one carrying `ESC[2K\r` erased charter's own
        words beside it and repainted the row from column zero; `"\u202e"` reversed it;
        and one of a hundred thousand characters put twelve hundred rows in front of the
        destination while `MAX_LINE` was satisfied, because `describe` never saw the name.

        So the assertion is over the whole printed transcript rather than over any field
        in it: run the real command with each field of the entry hostile in turn, and the
        transcript may hold no glyph a benign run does not hold, and no line wider than
        the screen the question is asked on. A field added to this line later is covered
        without this test being edited.

        Charter's own decoration — the bullet, the tick, the arrow, the em dash — is
        DERIVED from a benign run of the same command rather than listed here. A list
        would drift the first time a message gained a glyph, and this file has already
        watched two lists of characters be walked past.

        **Colour is pinned off for the whole test, and that is a fix, not a tidy-up.**
        `util._USE_COLOR` is `sys.stderr.isatty()` evaluated at IMPORT time — before this
        test redirects stderr — so running the suite from a terminal put charter's own
        ANSI codes into the benign transcript, ESC joined `ours`, and `stray` could never
        contain it again. The derivation absorbed whatever the environment added, and it
        absorbed precisely the class this test exists to catch: with `label` mutated to
        raw interpolation and a hostile of `"ESC[2Kharmless"`, this failed under a pipe
        and reported OK under a pty. Pinned, every ESC in the transcript is committed
        text, and the assertion below has the same verdict in both.
        """
        self.enterContext(mock.patch("charter.util._USE_COLOR", False))
        ours: set[str] = set()
        for benign in (dict(STDIO), {"type": "http", "secrets": dict(STDIO["secrets"])}):
            self._persona(benign)
            _rc, out = self._sync(_Tty(), answers=["n"])
            ours |= {c for c in out if not (" " <= c <= "~")}
        self.assertIn("→", ours, "precondition: the arrow is charter's own")
        self.assertNotIn("\x1b", ours,
                         "charter's own colour is in the derived set, so a committed "
                         "escape sequence could never be stray again")

        hostiles = {
            "server name": ("\u3164" * 3, "a\x1b[2K\rharmless", "z" * 100000,
                            "\u202eevil", "   ", "a\nb"),
            "command": ("\u3164" * 3, "npx\r\x1b[2K", "\U0001f600" * 400),
            "arg": ("\u2800" * 300, "--x\u0301" * 90),
            "env key": ("PATH\u200b", "\u115f" * 40),
            "vault key": ("client-id\u202e", "\u1160" * 40),
        }
        for where, inputs in hostiles.items():
            for hostile in inputs:
                with self.subTest(where=where, input=repr(hostile[:14])):
                    name = "reddit"
                    self.make_persona(name, role="R", vault=VAULT,
                                      **{"delegate-when": "things"})
                    entry = {"type": "stdio", "command": "uvx",
                             "args": ["some-reddit-mcp"], "env": {"PATH": "/usr/bin"},
                             "secrets": {"REDDIT_CLIENT_ID": "client-id"}}
                    server = "reddit"
                    if where == "server name":
                        server = hostile
                    elif where == "command":
                        entry["command"] = hostile
                    elif where == "arg":
                        entry["args"] = [hostile, "--read-only"]
                    elif where == "env key":
                        entry["env"] = {hostile: "1"}
                    else:
                        entry["secrets"] = {"REDDIT_CLIENT_ID": hostile}
                    self._write(name, {server: entry})

                    _rc, err = self._sync(_Tty(), answers=["n"])
                    stray = {c for c in err if not (" " <= c <= "~")} - ours
                    self.assertEqual(stray, set(),
                                     "committed text put a glyph on the terminal that a "
                                     "benign run never puts there")
                    # The screen, measured on the line as PRINTED — bullet, indent, label,
                    # arrow and destination — not on the half `describe` happens to bound.
                    screen = mcpseen.MAX_COLS * mcpseen.MAX_ROWS
                    for ln in err.splitlines():
                        self.assertLessEqual(
                            len(_ANSI.sub("", ln)), screen,
                            f"{len(ln) // mcpseen.MAX_COLS} rows is a page the operator "
                            f"scrolls, not a line they read: {ln[:60]!r}")

    def test_the_destination_ceiling_leaves_room_for_the_label_beside_it(self):
        """The behaviour behind the arithmetic. `describe`'s ceiling is not the screen —
        it is the screen MINUS the label and the decoration printed on the same row, since
        those columns are spent whatever the destination does. A ceiling on the part you
        were looking at rather than on the line you print is a ceiling the other part is
        free to walk past, and that is precisely how a hundred-thousand-character server
        name printed twelve hundred rows with `MAX_LINE` satisfied throughout.

        The entry below sits in the gap between the two ceilings: refused today, and
        rendered — beside a name that fills the label — by anything that spends the whole
        screen on the destination."""
        fat = {"type": "stdio", "command": "uvx", "args": ["a" * 90] * 8,
               "secrets": {"REDDIT_CLIENT_ID": "client-id"}}
        self.assertEqual(mcpseen.describe(VAULT, fat), "",
                         "a destination this size does not leave room for the label")

        name = "reddit"
        self.make_persona(name, role="R", vault=VAULT, **{"delegate-when": "things"})
        self._write(name, {"z" * 100000: fat})
        _rc, err = self._sync(_Tty(), answers=["y"])
        self.assertEqual(self.asked, 0, "nothing showable, nothing to ask")
        for ln in err.splitlines():
            self.assertLessEqual(len(_ANSI.sub("", ln)),
                                 mcpseen.MAX_COLS * mcpseen.MAX_ROWS, ln[:60])

    def test_an_interrupt_names_the_persona_through_the_same_escape(self):
        """Ctrl-C at the prompt. Every other half of every other line here goes through
        `mcpseen.label`; so does this one, because "that half is safe today" is exactly
        the reasoning the label was shipped on for three rounds. `valid_name` bounds a
        persona's alphabet and not its length, so a 200-character directory is the case
        that is not hypothetical."""
        from contextlib import redirect_stderr, redirect_stdout
        name = self._persona(name="z" * 200)
        buf = io.StringIO()
        args = SimpleNamespace(persona=None, approve_mcp=True, yes=False, dry_run=False)

        def boom(prompt=""):
            raise KeyboardInterrupt

        with mock.patch("sys.stdin", _Tty()), mock.patch("builtins.input", boom), \
                redirect_stdout(io.StringIO()), redirect_stderr(buf):
            rc = commands_persona.cmd_persona_sync_agents(args)
        self.assertEqual(rc, 130)
        self.assertEqual(mcpseen.approved(name), set(), "and nothing was recorded")
        line = [ln for ln in buf.getvalue().splitlines() if "interrupted" in ln]
        self.assertEqual(len(line), 1, buf.getvalue())
        # From the word itself: the prompt this interrupted printed no newline, so the
        # message shares a physical row with it.
        tail = line[0][line[0].index("interrupted"):]
        self.assertLessEqual(len(_ANSI.sub("", tail)), mcpseen.MAX_COLS,
                             "one row, like every other line the operator reads here")
        self.assertIn(mcpseen.label(name), tail)
        self.assertNotIn(name, tail, "the whole 200-character directory name reached it")

    def test_every_field_that_reaches_the_line_goes_through_the_escape(self):
        """The routing half of the guarantee, split from the class half deliberately.

        `test_a_consent_line_is_printable_ascii_and_nothing_else` sweeps all 1,114,112
        codepoints through `_safe`, so WHICH codepoints are caught is settled there. What
        is left to check is whether each field reaches `_safe` at all — and one escaped
        codepoint per field settles that, because `_safe` is total: a field that skips it
        fails on the first non-ASCII character, whichever one is used. Listing fields is
        safe in a way that listing codepoints never was; the list is closed, it is the
        signature of `describe` plus the signature of `label`, and a field left off it
        fails the transcript test above instead.
        """
        blank, esc = "\u3164", "\\u3164"
        renders = {
            "command": lambda s: mcpseen.describe(VAULT, {"command": s}),
            "arg": lambda s: mcpseen.describe(VAULT, dict(STDIO, args=[s])),
            "type": lambda s: mcpseen.describe(VAULT, {"type": s, "url": "https://x.example"}),
            "url": lambda s: mcpseen.describe(VAULT, {"type": "http", "url": s}),
            "env key": lambda s: mcpseen.describe(VAULT, dict(STDIO, env={s: "1"})),
            "secrets var": lambda s: mcpseen.describe(VAULT, dict(STDIO, secrets={s: "k"})),
            "secrets vault key": lambda s: mcpseen.describe(VAULT, dict(STDIO, secrets={"V": s})),
            "secret_files var": lambda s: mcpseen.describe(VAULT, dict(STDIO, secret_files={s: "k"})),
            "secret_files vault key": lambda s: mcpseen.describe(VAULT, 
                dict(STDIO, secret_files={"V": s})),
            "persona name": lambda s: mcpseen.label(s, "acme"),
            "server name": lambda s: mcpseen.label("reddit", s),
        }
        for where, render in renders.items():
            with self.subTest(where=where):
                out = render(blank * 3)
                self.assertIn(esc, out, f"{where} does not go through the escape")
                self.assertTrue(all(" " <= c <= "~" for c in out),
                                f"{where} put a glyph on the line: {out!r}")

    def test_a_name_is_bounded_by_a_number_and_not_by_the_input(self):
        """`_clip` announces how much it cut, and that marker's own width grows with the
        input it describes — fine for a destination, where the count is what the operator
        needs, and wrong for a label, where the point is a hard bound. A budget a longer
        input makes longer is not a budget."""
        for n in (10 ** 3, 10 ** 5):
            with self.subTest(n=n):
                half = mcpseen.label("z" * n).split("/")[0]
                self.assertLessEqual(len(half), mcpseen.MAX_NAME)
                self.assertTrue(half.endswith("..."), "and the cut is still announced")
        self.assertEqual(mcpseen.label("   ", "reddit"), '""/reddit',
                         "a half that renders as nothing is named, not left as a gap")
        self.assertLessEqual(
            mcpseen.MAX_LABEL + mcpseen.MAX_LINE + mcpseen._DECORATION,
            mcpseen.MAX_COLS * mcpseen.MAX_ROWS,
            "the destination budget must leave room for the label printed beside it")

    def test_invisible_padding_is_refused_before_anyone_is_asked(self):
        """The other half of the same input: nine args of 200 blank columns fit under the
        old 2000-character ceiling and pushed `uvx evil-server` off the top of an 80x24
        screen. Over the screen ceiling there is no line, so there is no digest and no
        question — the operator hears it was refused instead of answering blind."""
        name = self._persona(dict(STDIO, command="uvx",
                                  args=["evil-server"] + ["\u3164" * 200] * 9,
                                  env={"PATH": "/tmp/attacker-bin"}))
        rc, err = self._sync(_Tty(), answers=["y"])
        self.assertEqual(rc, 0)
        self.assertEqual(self.asked, 0, "nothing showable, nothing to ask")
        self.assertIn(mcpseen.UNRENDERABLE, err, "and the refusal is said out loud")
        self.assertEqual(mcpseen.approved(name), set())
        self.assertWithheld(self._render(name))

    def test_declining_revokes_an_approval_it_already_had(self):
        name = self._two_servers()
        self._approve()
        self.assertWrapped(self._render(name, "reddit"), "precondition: approved")
        self._sync(_Tty(), answers=["n", "n"])
        self.assertWithheld(self._render(name, "reddit"))

    def test_silence_at_the_prompt_is_a_no(self):
        """EOF mid-run, an empty line, a stray word — anything but yes withholds."""
        for answers in ([], [""], ["maybe"], ["\t"]):
            with self.subTest(answers=answers):
                name = self._persona(name="reddit")
                rc, _err = self._sync(_Tty(), answers=answers)
                self.assertEqual(rc, 0)
                self.assertEqual(mcpseen.approved(name), set())

    def test_off_a_terminal_it_refuses_rather_than_approving(self):
        """The finding itself: `--approve-mcp` in a script approved everything silently."""
        name = self._persona()
        rc, err = self._sync(io.StringIO())
        self.assertEqual(rc, 1)
        self.assertEqual(mcpseen.approved(name), set(), "nothing was recorded")
        self.assertNotIn("--yes", err,
                         "a refusal must not print the flag that defeats it (#421)")
        self.assertEqual(self.asked, 0, "it did not try to ask a pipe")

    def test_yes_keeps_the_scripted_shape(self):
        name = self._persona()
        rc, err = self._sync(io.StringIO(), yes=True)
        self.assertEqual(rc, 0)
        self.assertEqual(self.asked, 0, "--yes does not ask")
        self.assertWrapped(self._render(name))
        self.assertIn("uvx some-reddit-mcp", err, "it still says what it approved")

    def test_dry_run_records_nothing(self):
        name = self._persona()
        rc, err = self._sync(io.StringIO(), dry_run=True)
        self.assertEqual(rc, 0)
        self.assertEqual(mcpseen.approved(name), set())
        self.assertIn("uvx some-reddit-mcp", err)

    def test_it_refuses_to_approve_an_entry_it_cannot_show(self):
        name = self._persona({"type": "http", "secrets": {"ACME_TOKEN": "acme-token"}})
        rc, err = self._sync(_Tty(), answers=["y"])
        self.assertEqual(rc, 0)
        self.assertEqual(self.asked, 0, "a blank line is not a question")
        self.assertEqual(mcpseen.approved(name), set())
        self.assertIn("cannot approve", err)

    def test_the_withheld_report_never_prints_a_blank_destination(self):
        self._persona({"type": "http", "secrets": {"ACME_TOKEN": "acme-token"}})
        from contextlib import redirect_stderr, redirect_stdout
        buf = io.StringIO()
        args = SimpleNamespace(persona=None, approve_mcp=False, yes=False, dry_run=False)
        with redirect_stdout(io.StringIO()), redirect_stderr(buf):
            commands_persona.cmd_persona_sync_agents(args)
        err = buf.getvalue()
        self.assertIn("reddit/reddit", err)
        self.assertNotIn("reddit/reddit → \n", err, "the consent line was blank")
        self.assertIn(mcpseen.UNRENDERABLE, err)
        agent = (Path(config.ROOT) / ".claude" / "agents" / "reddit.md").read_text()
        self.assertIn("mcpServers:", agent, "precondition: the server was still declared")
        self.assertNotIn("secret", agent.split("mcpServers:")[1].split("\n---")[0])


if __name__ == "__main__":
    unittest.main()
