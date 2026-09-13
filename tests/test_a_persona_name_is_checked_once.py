"""Every command that takes a persona name checks it once, the same way, before it looks
anything up (#1059).

#1057 (PR #1058) gave `vault add --persona` the three refusals and left the other readers
of a persona name as they were. Measured before, on a plane defining only `devops`:

* `persona secret list --persona devosp` never asked whether `devosp` was a persona. It went
  straight to `persona.vault_of`, whose fallback is the vault *tagged* with that name, so a
  vault tagged with the typo was read for a persona nobody defines.
* `persona use ../x` said `no persona '../x' (create it: charter persona create ../x)`, and
  `persona create ../x` refuses that name. The remedy led to a second refusal.
* The rest said the same absence in four different sentences, or said nothing: `persona
  stats devosp` and `persona optimize devosp` exited 0 over a persona that is not there.

**One check, three sentences, the same line from every command.** A name of only whitespace
gets #1055's sentence, a name outside the alphabet gets `persona create`'s with no create
hint, and a valid name no persona has gets `persona use`'s, hint included. The expected lines
below are written out rather than read off `persona`'s constants, so a change to a constant
has to change this file too.
"""
from __future__ import annotations

import io
import re
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import (commands, commands_handoff, commands_persona, commands_secrets, config,
                     persona)
from charter.frame import switch
from charter.secrets import registry
from tests._isolation import PersonaIso

_ANSI = re.compile(r"\x1b\[[0-9;]*m")

#: A bad name of each kind, and the line every command says to it.
WHITESPACE = (" ", "no persona ' ' (a persona name is never only whitespace)")
INVALID = ("../x", "invalid persona name '../x' (lowercase letters, digits, '.', '_', '-')")
ABSENT = ("devosp", "no persona 'devosp' (create it: charter persona create devosp)")


def _run(cmd, **kw) -> tuple[int, str]:
    err = io.StringIO()
    with redirect_stderr(err), redirect_stdout(io.StringIO()):
        rc = cmd(SimpleNamespace(**kw))
    return rc, _ANSI.sub("", err.getvalue())


def _create(**kw) -> dict:
    return {"name": "child", "extends": None, "force": False, "delegate_when": "ci work",
            "vault": None, "role": None, "with_vault": False, "use": False, **kw}


def _recall(bad: str) -> dict:
    return dict(persona=bad, query=None, scope=None, ephemeral=False, since=None, limit=8,
                workspace=None, all_workspaces=False, full=False)


#: ``(command, how to run it with the bad name)``, for every command that must also refuse a
#: valid name no persona has. Each runner answers ``(refused, the text it said)``.
MUST_EXIST = (
    ("persona use", lambda b: _run(commands_persona.cmd_persona_use, name=b)),
    ("persona show", lambda b: _run(commands_persona.cmd_persona_show, name=b)),
    ("persona default", lambda b: _run(commands_persona.cmd_persona_default, name=b,
                                       clear=False)),
    ("persona remove", lambda b: _run(commands_persona.cmd_persona_remove, name=b,
                                      force=False)),
    ("persona remember", lambda b: _run(commands_persona.cmd_persona_remember, name=b,
                                        text="a fact", title=None, shared=False,
                                        ephemeral=False, no_sync=True)),
    ("persona recall", lambda b: _run(commands_persona.cmd_persona_recall, name=b,
                                      query=None, log=8)),
    ("persona dedupe", lambda b: _run(commands_persona.cmd_persona_dedupe, name=b,
                                      threshold=0.5)),
    ("persona log", lambda b: _run(commands_persona.cmd_persona_log, name=b, message=None,
                                   n=20)),
    ("persona forget", lambda b: _run(commands_persona.cmd_persona_forget, name=b, slug="s",
                                      shared=False, ephemeral=False)),
    ("persona stats", lambda b: _run(commands_persona.cmd_persona_stats, name=b,
                                     recent_days=14)),
    ("persona optimize", lambda b: _run(commands_persona.cmd_persona_optimize, name=b,
                                        all=False, apply=False, stale_days=90)),
    ("persona migrate", lambda b: _run(commands_persona.cmd_persona_migrate, name=b)),
    ("persona sync-agents --persona", lambda b: _run(
        commands_persona.cmd_persona_sync_agents, persona=b, approve_mcp=False, yes=False,
        dry_run=False)),
    ("persona secret list --persona", lambda b: _run(
        commands_persona.cmd_persona_secret_list, persona=b, vault=None)),
    ("persona create --extends", lambda b: _run(commands_persona.cmd_persona_create,
                                                **_create(extends=b))),
    ("vault add --persona", lambda b: _run(
        commands_secrets.cmd_vault_add, name="v", provider="plain-file", file=None,
        op_vault=None, account=None, persona=b, force=False, share=False, env=[],
        token_env=None)),
    ("recall --persona", lambda b: _run(commands.cmd_recall, **_recall(b))),
)

#: The two that decide existence themselves. `persona create` makes the persona a name does
#: not define yet, and `persona lint` is the report that says why a named persona does not
#: load, which a "no persona" line would take the place of.
NAME_ONLY = (
    ("persona create", lambda b: _run(commands_persona.cmd_persona_create,
                                      **_create(name=b))),
    ("persona lint", lambda b: _run(commands_persona.cmd_persona_lint, name=b, only=None)),
)

#: The two whose refusal is framed: `handoff` says every refusal as `charter handoff: …
#: nothing was opened`, and `frame-switch` draws one row on the frame's screen. Both name
#: the personas there are beside it. The sentence inside the frame is still the same one.
FRAMED = (
    ("handoff --persona", lambda b: _run(
        commands_handoff.cmd_handoff, workspace=config.DEFAULT_WORKSPACE, vision=None,
        create=False, persona=b)),
    ("frame-switch --persona", lambda b: (lambda o: (0 if o.ok else 1, o.message))(
        switch.to_persona("f1", b))),
)


class NameCase(PersonaIso):
    """A plane defining one persona, `devops`."""

    def setUp(self) -> None:
        super().setUp()
        self.make_persona("devops", role="Devops", vault="ops")

    def assert_says(self, said: str, line: str) -> None:
        """One line, and it is the sentence: the glyph `util.err` puts in front is all."""
        lines = [ln for ln in said.splitlines() if ln.strip()]
        self.assertEqual(len(lines), 1, said)
        self.assertEqual(lines[0].removeprefix("✗ "), line, said)


class EveryCommandSaysTheSameLine(NameCase):
    def test_every_command_that_takes_a_persona_name_says_the_same_line(self):
        for bad, line in (WHITESPACE, INVALID, ABSENT):
            for label, run in MUST_EXIST:
                with self.subTest(command=label, name=bad):
                    rc, said = run(bad)
                    self.assertEqual(rc, 1, said)
                    self.assert_says(said, line)
            for label, run in FRAMED:
                with self.subTest(command=label, name=bad):
                    rc, said = run(bad)
                    self.assertEqual(rc, 1, said)
                    self.assertEqual(len(said.strip().splitlines()), 1, said)
                    self.assertIn(line, said)
                    self.assertIn(f"{line} — have: devops", said)
        for bad, line in (WHITESPACE, INVALID):
            for label, run in NAME_ONLY:
                with self.subTest(command=label, name=bad):
                    rc, said = run(bad)
                    self.assertEqual(rc, 1, said)
                    self.assert_says(said, line)


class PersonaSecretAsksBeforeTheTagFallback(NameCase):
    def test_a_misspelled_persona_does_not_reach_the_vault_tagged_with_the_misspelling(self):
        """Measured before: exit 0 and `Vault 'typo' has no secrets.`, read through
        `vault_of`'s fallback for a persona this plane does not define."""
        registry.add_vault("typo", "plain-file",
                           {"file": str(config.VAULTS_DIR / "typo.json")}, persona="devosp")
        for sub in ("list", "audit"):
            with self.subTest(sub=sub):
                rc, said = _run(getattr(commands_persona, f"cmd_persona_secret_{sub}"),
                                persona="devosp", vault=None, days=90)
                self.assertEqual(rc, 1, said)
                self.assert_says(said, ABSENT[1])
                self.assertNotIn("typo", said)

    def test_a_defined_persona_still_reaches_its_vault(self):
        registry.add_vault("ops", "plain-file", {"file": str(config.VAULTS_DIR / "ops.json")},
                           persona="devops")
        rc, said = _run(commands_persona.cmd_persona_secret_list, persona="devops", vault=None)
        self.assertEqual(rc, 0, said)
        self.assertIn("'ops'", said)


class PersonaUseSuggestsOnlyACreateThatWorks(NameCase):
    def test_an_invalid_name_gets_no_create_hint(self):
        """Measured before: `no persona '../x' (create it: charter persona create ../x)`."""
        for bad in ("../x", "Devops"):
            with self.subTest(name=bad):
                rc, said = _run(commands_persona.cmd_persona_use, name=bad)
                self.assertEqual(rc, 1, said)
                self.assertNotIn("persona create", said)
                self.assert_says(said, f"invalid persona name '{bad}' (lowercase letters, "
                                       "digits, '.', '_', '-')")

    def test_the_hint_it_does_give_is_one_create_accepts(self):
        rc, said = _run(commands_persona.cmd_persona_use, name="devosp")
        self.assertEqual(rc, 1, said)
        rc, said = _run(commands_persona.cmd_persona_create, **_create(name="devosp"))
        self.assertEqual(rc, 0, said)
        self.assertEqual(_run(commands_persona.cmd_persona_use, name="devosp")[0], 0)


class ARefusedNameIsShownOnOneLine(NameCase):
    def test_a_line_separator_in_a_refused_name_is_escaped(self):
        """`persona create` quoted the name raw, so a newline in it wrote a second line of
        output charter did not."""
        for label, run in MUST_EXIST + NAME_ONLY:
            with self.subTest(command=label):
                rc, said = run("ops\nforged")
                self.assertEqual(rc, 1, said)
                self.assert_says(said, "invalid persona name 'ops\\x0aforged' (lowercase "
                                       "letters, digits, '.', '_', '-')")


class APersonaThatDoesNotLoad(NameCase):
    """`personas/broken/persona.md` resolves out of the plane, so the roster lists `broken`
    and `load` refuses it."""

    def setUp(self) -> None:
        super().setUp()
        d = config.PERSONAS_DIR / "broken"
        d.mkdir()
        (d / "persona.md").symlink_to(config.ROOT / "elsewhere.md")
        (config.ROOT / "elsewhere.md").write_text("---\nname: broken\n---\n")

    def test_the_roster_surfaces_refuse_what_persona_use_refuses(self):
        """`handoff` and `frame-switch` asked the roster, which lists it, so a chat could be
        pinned to, or a frame switched onto, a persona `persona use` will not adopt."""
        self.assertIn("broken", persona.list_personas(), "precondition: it is listed")
        line = "no persona 'broken' (create it: charter persona create broken)"
        rc, said = _run(commands_persona.cmd_persona_use, name="broken")
        self.assertEqual(rc, 1, said)
        self.assert_says(said, line)
        for label, run in FRAMED:
            with self.subTest(command=label):
                rc, said = run("broken")
                self.assertEqual(rc, 1, said)
                self.assertIn(f"{line} — have: ", said)

    def test_lint_still_says_why_a_named_persona_does_not_load(self):
        """Lint's own finding, not `no persona` in its place."""
        rc, said = _run(commands_persona.cmd_persona_lint, name="broken", only=None)
        self.assertEqual(rc, 1, said)
        self.assertIn("broken: persona.md:", said)
        self.assertNotIn("create it", said)


class WhatTheCheckLeavesAlone(NameCase):
    def test_create_still_makes_a_persona_no_name_defines_yet(self):
        rc, said = _run(commands_persona.cmd_persona_create, **_create(name="scribe"))
        self.assertEqual(rc, 0, said)
        self.assertIsNotNone(persona.load("scribe"))

    def test_stats_and_optimize_still_take_the_shared_namespace_by_name(self):
        """`_shared` is outside the alphabet on purpose, so no persona can take it, and it is
        the one name these two read besides a persona's."""
        persona.ensure_shared()
        for label, cmd, kw in (
                ("stats", commands_persona.cmd_persona_stats, {"recent_days": 14}),
                ("optimize", commands_persona.cmd_persona_optimize,
                 {"all": False, "apply": False, "stale_days": 90})):
            with self.subTest(command=label):
                rc, said = _run(cmd, name=config.SHARED_PERSONA, **kw)
                self.assertEqual(rc, 0, said)
                self.assertNotIn("invalid persona name", said)

    def test_no_flag_and_an_empty_flag_still_mean_the_active_persona(self):
        """Empty is no flag, as every reader of `--persona` has always taken it."""
        registry.add_vault("ops", "plain-file", {"file": str(config.VAULTS_DIR / "ops.json")},
                           persona="devops")
        persona.set_active("devops")
        for flag in (None, ""):
            with self.subTest(flag=flag):
                rc, said = _run(commands_persona.cmd_persona_secret_list, persona=flag,
                                vault=None)
                self.assertEqual(rc, 0, said)
                self.assertIn("'ops'", said)


if __name__ == "__main__":
    unittest.main()
