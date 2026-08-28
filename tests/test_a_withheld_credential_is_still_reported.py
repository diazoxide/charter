"""A persona running without the credential it declares is reported by something STANDING.

#489, split out of #452. The #330 consent gate is working as designed and the run that
withholds a vault is **not** silent: `sync-agents` names every server it withheld and the
command that would restore it. That warning is the only place it was ever said.

`persona.mcp_withheld` had exactly one caller — `cmd_persona_sync_agents`, on the run that
wrote the file — and nothing that reports standing state. So once the terminal scrolls, a
persona running without its credential is indistinguishable from one that never declared
one:

* ``.claude/agents/<name>.md`` renders ``{"command": "uvx", "args": [...]}``, byte-identical
  to what an entry with no ``secrets`` at all produces. `TheGeneratedFileCannotSayIt`
  asserts that equality rather than describing it, because it is the whole finding: **the
  damage lands in a GENERATED file people are told never to hand-edit, so it reads as
  intended output.**
* `charter persona lint` said nothing.
* `charter doctor` said nothing.

The failure it produces is the one `mcpseen`'s own module docstring warns about — an MCP
server failing to authenticate, three layers away — except that after the run it *is*
silent, because the only thing that was ever going to say it has already been printed once
and lost.

**Where it is reported, and why not somewhere else.**

`persona lint` is the home: it already reports per-persona defects and it is the command
you run *because* a persona is misbehaving. `doctor` gets it for free, because
`check_personas` runs `persona.lint` across the roster and names the personas with
findings — one line, which is that check's stated design, and one WORDING, which is this
repo's repeated lesson. A dedicated `doctor` row would be a second sentence about one fact,
and two sentences about one fact is how the pair comes to disagree (`base.loose_dir_note`
records the same argument for the surface next door).

**A warning, not an error, and that is a decision.** Withholding is the gate WORKING: the
operator may have read the command and declined it, and `charter persona lint` exiting 1
forever would make this the finding planes learn to turn off. What charter owes is that the
state stays visible, not that it overrules the answer. `TheLevelIsAChoice` pins both
directions, including the one case that *is* an error — an entry `describe` cannot render,
which can never be approved by anybody, so the only way out is an edit and "approve it" is
advice that cannot work (#371: a prompt is worth its interruption only if it names what to
do next).

**The generated file deliberately carries no trace of its own**, which the issue floats and
this rejects. `mcpseen` learned across four rounds that the way to close this class is to
stop keeping a second representation of consent — *"there is nothing left to fall out of
step"*. A comment in a wholesale-regenerated file is exactly a second representation: it
goes stale the moment the operator approves without re-syncing, and a reader who trusts it
then reads a false statement about a live credential. The artefact is where the question
gets asked; the RECORD is where it gets answered, and `lint` asks the record.

**The sibling, found by enumerating rather than by being filed.** `lint`'s "declares
secrets but names no vault" error read ``entry.get("secrets")`` and not ``secret_files`` —
while `mcpseen.needs_consent`, `persona.mcp_render_entry` and `charter secret exec`'s own
``--env``/``--file`` pair all treat the two as one mechanism. A server declaring only
``secret_files`` against a persona with no vault therefore rendered with no credential and
was reported by NOTHING: not by that error, because the key was not read, and not as
withheld, because with no vault there is no consent to withhold. `secret_files` is not the
exotic half — it is what Google ADC needs, and it is the declaration #489's own
reproduction carries. See `SecretFilesIsNotTheForgottenHalf`.
"""

from __future__ import annotations

import io
import json
import re
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_persona, config, doctor, mcpseen, persona
from tests._isolation import PersonaIso

VAULT = "wsvault"

#: Charter's own colour codes, which `util` adds and a terminal does not print.
_ANSI = re.compile(r"\x1b\[[0-9;]*m")

#: The declaration from #489's reproduction, field for field: `secret_files`, because
#: `GOOGLE_APPLICATION_CREDENTIALS` takes a PATH and not a value (#190).
GA4 = {
    "command": "uvx",
    "args": ["analytics-mcp==0.7.0"],
    "secret_files": {"GOOGLE_APPLICATION_CREDENTIALS": "GOOGLE_SERVICE_ACCOUNT"},
}

#: The other mechanism, same shape: a value straight into the environment.
ENVVAR = {
    "command": "uvx",
    "args": ["some-mcp"],
    "secrets": {"API_TOKEN": "api-token"},
}


class WithheldBase(PersonaIso):
    def _persona(self, servers=None, name="ws", vault=VAULT):
        meta = {"role": "R", "delegate-when": "things"}
        if vault is not None:
            meta["vault"] = vault
        self.make_persona(name, **meta)
        if servers is not None:
            self._write(name, servers)
        return name

    def _write(self, name, servers) -> None:
        (persona.dir_of(name) / persona.MCP_FILE).write_text(
            json.dumps({"mcpServers": servers}))

    def _approve(self, name="ws") -> None:
        mcpseen.approve(name, [fp for _s, _e, fp, _l in persona.mcp_credentialed(name) if fp])

    def _sync(self, name=None):
        """A real `sync-agents` run — no `--approve-mcp`, the shape #489 describes: the
        operator ran it for something else entirely."""
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(persona=name, approve_mcp=False, yes=False, dry_run=False)
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands_persona.cmd_persona_sync_agents(args)
        return rc, _ANSI.sub("", err.getvalue())

    def _lint(self, name=None):
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(name=name, only=None)
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands_persona.cmd_persona_lint(args)
        return rc, _ANSI.sub("", err.getvalue() + out.getvalue())

    def _rows(self, name="ws", level=None):
        return [m for lvl, m in persona.lint(name)
                if "declares a credential" in m and (level is None or lvl == level)]

    def _agent(self, name="ws") -> str:
        return (Path(config.ROOT) / ".claude" / "agents" / f"{name}.md").read_text()


class TheGeneratedFileCannotSayIt(WithheldBase):
    """#452's sharpest sentence, asserted rather than quoted. This is why a standing report
    is needed at all: the artefact the operator actually reads cannot tell them."""

    def test_a_withheld_server_renders_exactly_like_one_that_declares_no_credential(self):
        name = self._persona({"ga4": dict(GA4)})
        self._sync()
        withheld = self._agent()

        bare = {k: v for k, v in GA4.items() if k != "secret_files"}
        self._write(name, {"ga4": bare})
        self._sync()
        self.assertEqual(withheld, self._agent(),
                         "the generated agent is byte-identical whether the credential was "
                         "withheld or never declared — which is the finding")

    def test_the_run_that_wrote_it_does_say_so(self):
        """The half that was never broken, pinned so the fix cannot be mistaken for it."""
        self._persona({"ga4": dict(GA4)})
        _rc, err = self._sync()
        self.assertIn("Withheld the vault", err)
        self.assertIn("--approve-mcp", err)


class LintReportsTheStandingState(WithheldBase):
    def test_it_names_the_server(self):
        self._persona({"ga4": dict(GA4)})
        rows = self._rows()
        self.assertEqual(len(rows), 1, rows)
        self.assertIn("ga4", rows[0])

    def test_it_names_what_to_do_next(self):
        """#371/PR 379: a prompt is worth its interruption only if it names the next move.
        Both moves — this is a decision the operator may legitimately make either way."""
        self._persona({"ga4": dict(GA4)})
        row = self._rows()[0]
        self.assertIn("charter persona sync-agents --approve-mcp", row)
        self.assertIn("secret_files", row, "and the other way out: drop the declaration")

    def test_it_says_what_actually_happens(self):
        """Not "unapproved" — the consequence. The failure this exists to pre-empt arrives
        three layers away as a server that will not authenticate."""
        self._persona({"ga4": dict(GA4)})
        row = self._rows()[0]
        self.assertIn("WITHOUT the vault", row)
        self.assertIn("authenticate", row)

    def test_approving_it_silences_the_row(self):
        """The complaint has to be caused by the state. A finding that is always there is
        one people learn to scroll past, and it takes the rest of the report with it."""
        name = self._persona({"ga4": dict(GA4)})
        self.assertTrue(self._rows(), "precondition: withheld")
        self._approve(name)
        self.assertEqual(self._rows(), [])

    def test_a_committed_edit_brings_the_row_back(self):
        """The approval is of a LINE, so an edited entry is a new destination — and the
        report has to follow the record rather than remember its own answer."""
        name = self._persona({"ga4": dict(GA4)})
        self._approve(name)
        self.assertEqual(self._rows(), [], "precondition: approved")
        self._write(name, {"ga4": dict(GA4, args=["analytics-mcp==0.7.0", "--write"])})
        self.assertTrue(self._rows(), "a changed command is not the approved one")

    def test_a_credential_free_server_produces_nothing(self):
        self._persona({"docs": {"command": "uvx", "args": ["docs-mcp"]}})
        self.assertEqual(persona.lint("ws"), [], "nothing is at stake, so nothing is said")

    def test_a_persona_with_no_mcp_json_produces_nothing(self):
        self._persona()
        self.assertEqual(persona.lint("ws"), [])

    def test_both_mechanisms_are_reported(self):
        """`secrets` and `secret_files` are two mechanisms for one thing, and the report
        may not know only the one that happened to be tested.

        A persona per case rather than a re-run `setUp`: `PersonaIso` snapshots the real
        config once and restores it on cleanup, so calling `setUp` twice discards the first
        snapshot and leaves the suite pointing at a tmp root for every module after this
        one — measured, as 26 unrelated failures in `_planeguard`'s own tests."""
        for label, entry in (("secret_files", GA4), ("secrets", ENVVAR)):
            with self.subTest(mechanism=label):
                name = self._persona({"srv": dict(entry)}, name=f"ws-{label}")
                self.assertTrue(self._rows(name), f"{label} was not reported")


class TheReportsCannotDisagree(WithheldBase):
    """`lint` and `sync-agents` both ask `persona.mcp_withheld`. The property, not the
    prose: a report that recomputes its own answer is one that drifts from the run."""

    def test_the_two_surfaces_name_the_same_servers(self):
        name = self._persona({"ga4": dict(GA4), "envs": dict(ENVVAR),
                              "docs": {"command": "uvx", "args": ["docs-mcp"]}})
        _rc, sync_err = self._sync()
        rows = self._rows(name)
        withheld = {s for s, _line in persona.mcp_withheld(name)}
        self.assertEqual(withheld, {"ga4", "envs"}, "precondition: two of the three")
        for server in withheld:
            self.assertTrue(any(server in r for r in rows), f"lint missed {server}")
            self.assertIn(server, sync_err, f"sync-agents missed {server}")
        self.assertFalse(any("docs" in r for r in rows),
                         "a server with nothing at stake is in neither report")

    def test_approving_one_of_two_leaves_the_other_in_both(self):
        name = self._persona({"ga4": dict(GA4), "envs": dict(ENVVAR)})
        keep = [fp for s, _e, fp, _l in persona.mcp_credentialed(name) if s == "envs" and fp]
        mcpseen.approve(name, keep)
        _rc, sync_err = self._sync()
        rows = " | ".join(self._rows(name))
        self.assertIn("ga4", rows)
        self.assertNotIn("envs", rows)
        self.assertIn("ga4", sync_err.split("Withheld the vault")[1])


class TheLevelIsAChoice(WithheldBase):
    def test_a_withheld_credential_is_a_warning(self):
        """Withholding is the gate working, and the operator may have declined on purpose.
        An exit code here would overrule an answer charter asked for."""
        self._persona({"ga4": dict(GA4)})
        self.assertEqual([lvl for lvl, m in persona.lint("ws")
                          if "declares a credential" in m], ["warn"])
        rc, out = self._lint("ws")
        self.assertEqual(rc, 0, out)
        self.assertIn("ga4", out)

    def test_an_entry_nobody_can_be_shown_is_an_error(self):
        """#427: `describe` cannot render it, so `fingerprint` is None and no approval can
        ever exist — `--approve-mcp` refuses it by name. Telling the operator to run that
        command would be a nudge that cannot work, so it is a different sentence AND a
        different level: the committed entry has to change."""
        self._persona({"ga4": dict(GA4, args=["evil"] + ["ㅤ" * 200] * 9)})
        rows = self._rows(level="error")
        self.assertEqual(len(rows), 1, persona.lint("ws"))
        self.assertIn(mcpseen.UNRENDERABLE, rows[0])
        self.assertNotIn("--approve-mcp", rows[0],
                         "advice that cannot work is worse than none")
        rc, _out = self._lint("ws")
        self.assertEqual(rc, 1, "an entry no answer exists for is an error")

    def test_the_row_names_the_persona_and_the_server(self):
        """Every lint row ends in "go and fix this", so it has to say WHICH."""
        self._persona({"ga4": dict(GA4)})
        rc, out = self._lint()
        self.assertEqual(rc, 0)
        self.assertTrue(any("ws:" in ln and "ga4" in ln for ln in out.splitlines()), out)

    def test_a_name_that_could_forge_a_row_never_reaches_this_one(self):
        """The name is a key of a committed `mcp.json`, and the boundary is upstream:
        `mcp_servers` refuses anything outside `mcp_name_ok`, so a server named with
        HANGUL FILLERs is not declared at all and is reported by the refusal row instead
        (#453). This asserts the two findings do not overlap — the withheld row can only
        ever be about a server that survived the bound."""
        self._persona({"ㅤㅤ": dict(GA4)})
        issues = persona.lint("ws")
        self.assertEqual(self._rows(), [], "a refused name declares no server to withhold")
        self.assertTrue([m for lvl, m in issues if lvl == "error" and "is refused" in m],
                        issues)

    def test_a_legal_but_long_name_is_bounded_on_the_row(self):
        """Defence in depth, and the layer that holds if that boundary is ever widened.
        `mcp_name_ok` bounds the ALPHABET at 64 characters; `mcpseen.label` is what keeps
        the row a row, and it is the same escape the sibling findings in this report use."""
        long_name = "a" * 64
        self.assertTrue(persona.mcp_name_ok(long_name), "precondition: a legal name")
        self._persona({long_name: dict(GA4)})
        rows = self._rows()
        self.assertEqual(len(rows), 1, rows)
        self.assertNotIn(long_name, rows[0], "the row carries `label`'s clip, not the key")
        self.assertIn("a" * 32 + "...", rows[0])
        self.assertEqual(len(rows[0].splitlines()), 1, "one finding is one row")


class DoctorCarriesIt(WithheldBase):
    """`doctor` has the standing-health role and reaches this through `check_personas`,
    which already runs `persona.lint` across the roster. One wording, two commands."""

    def test_the_personas_line_is_not_green(self):
        self._persona({"ga4": dict(GA4)})
        res = doctor.check_personas()
        self.assertEqual(res.status, doctor.WARN)
        self.assertIn("ws", res.detail)
        self.assertIn("charter persona lint", res.hint,
                      "the summary names the command with room to explain")

    def test_it_goes_green_once_the_credential_is_approved(self):
        name = self._persona({"ga4": dict(GA4)})
        self.assertEqual(doctor.check_personas().status, doctor.WARN, "precondition")
        self._approve(name)
        self.assertEqual(doctor.check_personas().status, doctor.OK,
                         "nothing else about this persona is wrong")

    def test_doctor_does_not_reach_a_vault_to_say_it(self):
        """This runs from the SessionStart hook. The finding is about the RECORD, not about
        whether the credential resolves — no provider is opened and no CLI is spawned."""
        self._persona({"ga4": dict(GA4)})
        with mock.patch("charter.secrets.registry.provider_for",
                        side_effect=AssertionError("check_personas opened a vault")):
            self.assertEqual(doctor.check_personas().status, doctor.WARN)


class SecretFilesIsNotTheForgottenHalf(WithheldBase):
    """The sibling. `lint`'s "declares secrets but names no vault" error read one of the two
    keys, so the OTHER one fell through every surface at once."""

    def _no_vault_rows(self, name="ws"):
        return [m for _lvl, m in persona.lint(name) if "names no vault" in m]

    def test_a_secret_files_server_with_no_vault_is_reported(self):
        self._persona({"ga4": dict(GA4)}, vault=None)
        rows = self._no_vault_rows()
        self.assertEqual(len(rows), 1, persona.lint("ws"))
        self.assertIn("ga4", rows[0])

    def test_a_secrets_server_with_no_vault_is_still_reported(self):
        """The half that always worked, pinned beside the half that did not."""
        self._persona({"envs": dict(ENVVAR)}, vault=None)
        self.assertTrue(self._no_vault_rows())

    def test_the_servers_are_named_in_a_stable_order(self):
        """One row, several servers, and a report people diff. The sort is what makes the
        row the same row on two machines; without it the order is `mcp.json`'s and a
        re-serialised file rewrites a finding that has not changed."""
        self._persona({"zeta": dict(GA4), "alpha": dict(ENVVAR)}, vault=None)
        rows = self._no_vault_rows()
        self.assertEqual(len(rows), 1, rows)
        self.assertIn("alpha, zeta", rows[0])

    def test_the_reserved_no_vault_value_counts_as_no_vault(self):
        self._persona({"ga4": dict(GA4)}, vault=persona.NO_VAULT)
        self.assertTrue(self._no_vault_rows())

    def test_a_credential_free_server_with_no_vault_is_not_reported(self):
        self._persona({"docs": {"command": "uvx", "args": ["docs-mcp"]}}, vault=None)
        self.assertEqual(self._no_vault_rows(), [])

    def test_no_persona_falls_between_the_two_reports(self):
        """The gap was structural: with no vault there is nothing to withhold, so a server
        the "no vault" error did not read was reported by neither surface. Asserted as the
        disjunction over both mechanisms and both vault states, which is the property —
        every declared credential is named by exactly one of the two."""
        for mech, entry in (("secret_files", GA4), ("secrets", ENVVAR)):
            for vault in (VAULT, None, persona.NO_VAULT):
                with self.subTest(mechanism=mech, vault=vault):
                    name = self._persona({"srv": dict(entry)}, vault=vault,
                                         name=f"ws-{mech}-{vault or 'unset'}")
                    said = [m for _l, m in persona.lint(name)
                            if "names no vault" in m or "declares a credential" in m]
                    self.assertEqual(len(said), 1,
                                     f"a declared credential must be named exactly once: "
                                     f"{persona.lint(name)}")
                    self.assertIn("srv", said[0])


class TheNoVaultSentinelIsNotAVaultName(WithheldBase):
    """The third instance, found by enumerating the surfaces rather than by being filed.

    ``vault: none`` is charter's reserved way of saying *this persona deliberately holds no
    credentials*, and `persona.vault_of` has always returned ``None`` for it. The MCP path
    read ``meta["vault"]`` raw. Measured against 0.53.0 on such a persona, before the fix::

        consent line   run uvx analytics-mcp  secrets "T"="k"  vault "none"
        rendered       charter secret exec none --env T=k --exec -- uvx analytics-mcp

    So `--approve-mcp` asked the operator to approve spending a vault named ``none``,
    recorded the consent, and wrote into the generated agent a launcher that cannot run —
    while `lint` was separately and correctly calling the same persona one that names no
    vault. One sentinel, two readings, and the consent record was on the wrong side of it.
    """

    def test_it_is_not_offered_for_approval(self):
        self._persona({"srv": dict(ENVVAR)}, vault=persona.NO_VAULT)
        self.assertEqual(persona.mcp_credentialed("ws"), [],
                         "there is no credential here to consent to")
        self.assertEqual(persona.mcp_withheld("ws"), [])

    def test_the_generated_agent_does_not_launch_a_vault_named_none(self):
        name = self._persona({"srv": dict(ENVVAR)}, vault=persona.NO_VAULT)
        mcpseen.approve(name, [mcpseen.fingerprint(persona.NO_VAULT,
                                                   persona.mcp_servers(name)["srv"]) or "x"])
        out = persona.mcp_render_entry(name, persona.NO_VAULT,
                                       persona.mcp_servers(name)["srv"])
        self.assertEqual(out.get("command"), "uvx", "passed through, not wrapped")
        self.assertNotIn("none", out.get("args") or [])
        self._sync()
        self.assertNotIn("secret exec none", self._agent(name).replace('", "', " "))

    def test_lint_says_it_once_and_says_the_right_thing(self):
        """The double report this fix also removes: "names no vault" is the finding, and
        "your credential is withheld" is not, because nothing was withheld."""
        self._persona({"srv": dict(ENVVAR)}, vault=persona.NO_VAULT)
        said = [m for _l, m in persona.lint("ws")
                if "names no vault" in m or "declares a credential" in m]
        self.assertEqual(len(said), 1, persona.lint("ws"))
        self.assertIn("names no vault", said[0])

    def test_the_sentinel_resolves_to_none_and_not_to_a_falsy_string(self):
        """``-> str | None`` is the contract, and every caller reads the result for
        truthiness — so ``""`` would pass every behavioural test here while breaking the
        one thing the annotation promises. Asserted directly, because nothing downstream
        can tell the two apart."""
        for absent in (None, "", "  ", persona.NO_VAULT, f"  {persona.NO_VAULT}  ", 7):
            with self.subTest(vault=repr(absent)):
                self.assertIsNone(persona.mcp_vault(absent))
        self.assertEqual(persona.mcp_vault(f"  {VAULT} "), VAULT)

    def test_a_persona_that_does_not_load_is_asked_about_rather_than_crashed_on(self):
        """`resolve` answers `None` for a persona whose `persona.md` does not load, and
        `_approve_mcp` reaches `mcp_credentialed` for every name `list_personas` globbed
        without asking first — so the fallback in `vault_for_mcp` is on a live path, not a
        decoration. `lint` reports the unloadable persona; nothing here may raise."""
        # A directory `list_personas` globs with no `persona.md` in it — the shape #336 is
        # about, and what `_approve_mcp` walks straight into: it takes every name the glob
        # returned and asks `mcp_credentialed` about each, loadable or not.
        d = config.PERSONAS_DIR / "broken"
        d.mkdir(parents=True, exist_ok=True)
        (d / persona.MCP_FILE).write_text(json.dumps({"mcpServers": {"srv": dict(ENVVAR)}}))
        self.assertIsNone(persona.resolve("broken"), "precondition: it does not load")
        self.assertIsNone(persona.vault_for_mcp("broken"))
        self.assertEqual(persona.mcp_credentialed("broken"), [])
        self.assertEqual(persona.mcp_withheld("broken"), [])
        self.assertTrue([m for _l, m in persona.lint("broken") if "does not load" in m],
                        persona.lint("broken"))

    def test_the_render_and_the_consent_list_agree(self):
        """Why one normalisation and not two: these are the two readers, and a persona the
        consent list thinks is credentialed while the render passes it through is an
        operator asked about a server the file does not wrap."""
        for i, vault in enumerate((VAULT, None, persona.NO_VAULT, "  ",
                                   f"  {persona.NO_VAULT}  ")):
            with self.subTest(vault=repr(vault)):
                name = self._persona({"srv": dict(ENVVAR)}, vault=vault, name=f"ws-{i}")
                self._approve(name)
                wrapped = persona.mcp_render_entry(
                    name, vault, persona.mcp_servers(name)["srv"]).get("command") == "charter"
                self.assertEqual(bool(persona.mcp_credentialed(name)), wrapped,
                                 "consent was asked for exactly when the vault is spent")


class OneQuestionAboutTheTwoKeys(PersonaIso):
    """`mcpseen.declares_credential` is the shared answer. Asserted over the relation
    between the two functions rather than over one call of each, because the failure was
    two readers disagreeing about the same entry."""

    def test_the_two_mechanisms_answer_identically(self):
        for entry in ({"secrets": {"A": "k"}}, {"secret_files": {"A": "k"}},
                      {"secrets": {"A": "k"}, "secret_files": {"B": "k"}}):
            with self.subTest(entry=entry):
                self.assertTrue(mcpseen.declares_credential(entry))
                self.assertTrue(mcpseen.needs_consent("v", entry))

    def test_nothing_declared_is_nothing_to_consent_to(self):
        """Both keys, and both ways of being wrong. The non-dict cases are the ones the
        `isinstance` halves are for, and they have to be TRUTHY non-dicts to test anything:
        `mcp.json` is committed, so ``"secret_files": "GOOGLE_SERVICE_ACCOUNT"`` — the map
        written as the bare key somebody meant — is a plausible hand-edit, and without the
        type check `mcp_render_entry` would reach `.items()` on a string and take
        `sync-agents` down with it."""
        for entry in ({}, {"command": "uvx"}, {"secrets": {}}, {"secret_files": {}},
                      {"secrets": "not-a-map"}, {"secret_files": "not-a-map"},
                      {"secrets": ["A"]}, {"secret_files": ["A"]}, "not-an-entry"):
            with self.subTest(entry=entry):
                self.assertFalse(mcpseen.declares_credential(entry))
                self.assertFalse(mcpseen.needs_consent("v", entry))

    def test_consent_needs_a_vault_and_the_declaration_does_not(self):
        """The reason they are two functions: `lint` asks about a persona with NO vault,
        where `needs_consent` is False by construction and cannot be its answer."""
        entry = {"secret_files": {"A": "k"}}
        self.assertTrue(mcpseen.declares_credential(entry))
        self.assertFalse(mcpseen.needs_consent(None, entry))
        self.assertFalse(mcpseen.needs_consent("", entry))


if __name__ == "__main__":
    unittest.main()
