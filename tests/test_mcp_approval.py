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
"""

from __future__ import annotations

import io
import json
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_persona, config, mcpseen, persona
from tests._isolation import PersonaIso

VAULT = "reddit"

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
        mcpseen.approve(name, [fp for _s, _e, fp in persona.mcp_credentialed(name) if fp])

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


class AnHttpServerHasAConsentLine(ApprovalBase):
    """#427. `describe` fed the line the operator is told to read; for `http`/`sse` it fed
    an empty string."""

    def test_an_http_server_has_a_nonempty_consent_line(self):
        line = mcpseen.describe(HTTP)
        self.assertIn("https://api.acme.example/mcp", line)
        other = dict(HTTP, url="https://evil.example.net/mcp")
        self.assertNotEqual(mcpseen.fingerprint(VAULT, HTTP),
                            mcpseen.fingerprint(VAULT, other),
                            "two endpoints must not share one approval")

    def test_the_consent_line_names_the_env_keys(self):
        line = mcpseen.describe(dict(STDIO, env={"PATH": "/tmp/bin", "NODE_OPTIONS": "-r x"}))
        self.assertIn("PATH", line)
        self.assertIn("NODE_OPTIONS", line)

    def test_the_consent_line_shows_no_secret_values(self):
        """It prints vault KEY names, never values — and the entry has no values to print
        in the first place. Asserted so a future 'be more helpful' edit cannot add them."""
        line = mcpseen.describe(dict(STDIO, secrets={"REDDIT_CLIENT_ID": "client-id"}))
        self.assertIn("uvx", line)
        self.assertNotIn("client-id", line)

    def test_control_characters_cannot_repaint_the_line(self):
        """The next input through this door. The line IS the consent, so a committed
        `args` carrying \\r, an ANSI erase, or a bidi override must not be able to show
        the operator something other than what would run."""
        for token in ("\rharmless", "\x1b[2Kharmless", "a‮b", "x\ny"):
            with self.subTest(token=token):
                line = mcpseen.describe(dict(STDIO, args=[token]))
                self.assertNotIn("\r", line)
                self.assertNotIn("\n", line)
                self.assertNotIn("\x1b", line)
                self.assertNotIn("‮", line)
                self.assertIn("uvx", line, "the real command still shows")

    def test_a_flood_of_args_cannot_push_the_env_or_the_url_off_the_line(self):
        """The bypass round one shipped. `[type url]` and `(env: …)` are appended AFTER
        `args`, and the finished line was cut at 600 characters — so ~600 characters of
        plausible `args` in a committed file produced a consent line naming neither the
        `env` it set nor the `url` it pointed at, while the approved render carried both
        to `execvpe`. Each part now has its own budget, so padding one cannot cut another.
        """
        entry = dict(STDIO, args=["x" * 100000], url="https://evil.example/mcp",
                     env={"PATH": "/tmp/attacker-bin", "NODE_OPTIONS": "-r /tmp/x.js"})
        line = mcpseen.describe(entry)
        self.assertTrue(line.startswith("uvx "), "the command stays at the front")
        self.assertIn("more chars", line, "and the clipping is announced")
        for named in ("PATH", "NODE_OPTIONS", "evil.example"):
            self.assertIn(named, line, f"{named} chooses the destination and must show")

    def test_one_long_part_cannot_clip_a_later_part_of_its_own_kind(self):
        """The next spelling: hiding does not need a different field to hide behind, only
        an earlier one. A per-part budget answers both."""
        line = mcpseen.describe(dict(STDIO, args=["z" * 100000, "--allow-remote-code"]))
        self.assertIn("--allow-remote-code", line, "a later arg is still named")
        line = mcpseen.describe(dict(STDIO, env={"A" * 100000: "1", "PATH": "/tmp/bin"}))
        self.assertIn("PATH", line, "a later env key is still named")

    def test_a_line_too_long_to_read_is_refused_rather_than_cut(self):
        """Enough parts that even their clipped forms overflow. Cutting would put charter
        back to choosing which half of the destination the operator sees, so it fails
        closed instead: no line, no digest, no approval."""
        flood = dict(STDIO, args=["y" * 60] * 200)
        self.assertEqual(mcpseen.describe(flood), "")
        self.assertIsNone(mcpseen.fingerprint(VAULT, flood))

        name = self._persona(flood)
        mcpseen.approve(name, ["deadbeef" * 8])
        self.assertWithheld(self._render())

    def test_a_whitespace_only_destination_is_a_blank_line_and_is_refused(self):
        """#427's guard tested `""` and let `"   "` through: `describe` returned three
        spaces, which is truthy, so `fingerprint` produced a real digest and a visually
        blank consent line was approvable — the exact property the docstring denies."""
        for blank in ("   ", " " * 601, " ", "\u00a0"):
            with self.subTest(blank=repr(blank)):
                entry = {"type": "stdio", "command": blank, "args": ["  ", ""],
                         "url": "  ", "secrets": {"ACME_TOKEN": "acme-token"}}
                line = mcpseen.describe(entry)
                if blank == "\u00a0":
                    # A non-breaking space is Separator, not ASCII space: `_safe` escapes
                    # it, so it shows as a destination rather than reading as blank.
                    self.assertIn("00a0", line)
                    continue
                self.assertEqual(line, "", "a blank line is no line")
                self.assertIsNone(mcpseen.fingerprint(VAULT, entry))

        name = self._persona({"type": "stdio", "command": "   ",
                              "secrets": {"ACME_TOKEN": "acme-token"}})
        mcpseen.approve(name, ["deadbeef" * 8])
        self.assertWithheld(self._render())

    def test_padding_with_spaces_cannot_indent_the_destination_out_of_view(self):
        line = mcpseen.describe(dict(STDIO, args=[" " * 600 + "--evil"]))
        self.assertTrue(line.startswith("uvx --evil"), line[:40])

    def test_an_entry_with_no_destination_cannot_be_approved(self):
        """The general case behind #427: a line `describe` cannot render is not a line
        anyone can consent to, so no digest exists for it and nothing can wrap it."""
        blind = {"type": "http", "secrets": {"ACME_TOKEN": "acme-token"}}
        self.assertEqual(mcpseen.describe(blind), "")
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

    def _sync(self, stdin, answers=None, **flags):
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
        by_server = {s: fp for s, _e, fp in persona.mcp_credentialed(name)}
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
        above the prompt names everything the approval hands over."""
        name = self._persona({
            "type": "stdio", "command": "uvx",
            "args": ["--config", "{" + "a" * 640 + "}"],
            "env": {"PATH": "/tmp/attacker-bin", "NODE_OPTIONS": "--require /tmp/x.js"},
            "secrets": {"REDDIT_CLIENT_ID": "client-id"},
        })
        rc, err = self._sync(_Tty(), answers=["y"])
        self.assertEqual(rc, 0)
        shown = [ln for ln in err.splitlines() if "reddit/reddit →" in ln]
        self.assertEqual(len(shown), 1, err)
        for named in ("PATH", "NODE_OPTIONS"):
            self.assertIn(named, shown[0],
                          f"{named} reaches execvpe, so it must reach the operator")
        rendered = self._render(name)
        self.assertWrapped(rendered, "precondition: the operator did approve it")
        self.assertEqual(sorted(rendered.get("env") or {}), ["NODE_OPTIONS", "PATH"],
                         "what was approved is what the harness gets")

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
