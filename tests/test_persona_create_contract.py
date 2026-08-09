"""What `charter persona create` is allowed to produce.

Two failures shipped together and reinforced each other:

* the scaffold carried author-facing slots — `(describe what this persona owns and
  does)` — and `create` generated `.claude/agents/<name>.md` from it *immediately*, so
  a persona dispatched before anyone edited it told its sub-agent that its
  responsibilities were a parenthetical instruction to a human;
* `create` had no way to set `delegate-when`, the field that decides whether the
  steward ever routes anything to the persona at all, so every new persona lint-warned
  from birth and two personas in a real roster were never dispatched once.

So: routing intent is required up front (it is knowable at creation), the charter body
is not (it is not), and the honest gap between them is `draft: true`.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr
from types import SimpleNamespace

from charter import commands_persona, config, persona
from tests._isolation import PersonaIso


def _args(name, **kw):
    base = dict(name=name, role=None, vault=None, extends=None, with_vault=False,
                use=False, force=False)
    base["delegate_when"] = kw.pop("delegate_when", "things this persona owns")
    base.update(kw)
    return SimpleNamespace(**base)


def _charter_text(name):
    return (config.PERSONAS_DIR / name / "persona.md").read_text()


def _agent_path(name):
    return config.ROOT / ".claude" / "agents" / f"{name}.md"


class DelegateWhenIsRequired(PersonaIso):
    def test_create_without_delegate_when_fails(self):
        rc = commands_persona.cmd_persona_create(_args("solo", delegate_when=None))
        self.assertEqual(rc, 1)
        self.assertFalse(persona.load("solo"), "no persona may be written on failure")

    def test_blank_delegate_when_is_treated_as_absent(self):
        rc = commands_persona.cmd_persona_create(_args("solo", delegate_when="   "))
        self.assertEqual(rc, 1)

    def test_create_with_delegate_when_succeeds_and_records_it(self):
        rc = commands_persona.cmd_persona_create(_args("solo", delegate_when="k8s work"))
        self.assertEqual(rc, 0)
        self.assertEqual(persona.load("solo")["meta"]["delegate-when"], "k8s work")

    def test_extends_may_omit_it_because_it_is_inherited(self):
        self.make_persona("parent", role="Parent", vault="parent",
                          **{"delegate-when": "parent work"})
        rc = commands_persona.cmd_persona_create(
            _args("child", delegate_when=None, extends="parent"))
        self.assertEqual(rc, 0)
        self.assertEqual(persona.resolve("child")["meta"]["delegate-when"], "parent work")

    def test_a_child_may_still_state_its_own(self):
        self.make_persona("parent", role="Parent", vault="parent",
                          **{"delegate-when": "parent work"})
        commands_persona.cmd_persona_create(
            _args("child", delegate_when="child work", extends="parent"))
        self.assertEqual(persona.resolve("child")["meta"]["delegate-when"], "child work")


class CreateStampsDraft(PersonaIso):
    def test_a_new_persona_is_a_draft(self):
        commands_persona.cmd_persona_create(_args("fresh"))
        self.assertIn("draft: true", _charter_text("fresh"))
        self.assertTrue(persona.is_draft("fresh"))

    def test_no_sub_agent_is_generated_while_draft(self):
        """The whole point: an unwritten charter must never become a system prompt."""
        commands_persona.cmd_persona_create(_args("fresh"))
        self.assertFalse(_agent_path("fresh").exists())

    def test_dropping_the_line_makes_it_dispatchable(self):
        commands_persona.cmd_persona_create(_args("fresh"))
        p = config.PERSONAS_DIR / "fresh" / "persona.md"
        p.write_text(p.read_text().replace("draft: true\n", ""))
        self.assertFalse(persona.is_draft("fresh"))
        self.assertEqual(commands_persona._write_agent("fresh"), "written")
        self.assertTrue(_agent_path("fresh").exists())


class TemplateShipsNoPlaceholders(PersonaIso):
    """A scaffold slot is an instruction to the author. It must never survive into a
    file whose only reader is an agent."""

    def test_the_created_charter_has_no_fill_in_slots(self):
        commands_persona.cmd_persona_create(_args("fresh", role="Widget Wrangler"))
        text = _charter_text("fresh")
        for slot in ("(describe", "(which repos", "(persona-specific"):
            self.assertNotIn(slot, text)

    def test_no_parenthetical_slot_of_any_shape_survives(self):
        import re
        commands_persona.cmd_persona_create(_args("fresh"))
        # a line that is nothing but a bulleted parenthetical is the scaffold shape
        bad = [ln for ln in _charter_text("fresh").splitlines()
               if re.match(r"^\s*[-*]\s*\(.*\)\s*$", ln)]
        self.assertEqual(bad, [])

    def test_the_charter_still_states_the_role_and_routing(self):
        commands_persona.cmd_persona_create(
            _args("fresh", role="Widget Wrangler", delegate_when="widget things"))
        text = _charter_text("fresh")
        self.assertIn("Widget Wrangler", text)
        self.assertIn("widget things", text)


class SyncAgentsRefusesDrafts(PersonaIso):
    def _finished(self, name):
        return self.make_persona(name, role=name.title(), vault=name,
                                 **{"delegate-when": f"{name} work"})

    def test_one_draft_does_not_stop_the_others(self):
        """Per-persona refusal. Aborting the whole sweep because one charter is
        unfinished would punish every other persona for it."""
        self._finished("alpha")
        self._finished("beta")
        self.make_persona("wip", role="WIP", vault="wip", draft="true",
                          **{"delegate-when": "later"})
        rc = commands_persona.cmd_persona_sync_agents(SimpleNamespace(persona=None))
        self.assertEqual(rc, 0)
        self.assertTrue(_agent_path("alpha").exists())
        self.assertTrue(_agent_path("beta").exists())
        self.assertFalse(_agent_path("wip").exists())

    def test_write_agent_reports_the_draft_outcome(self):
        self.make_persona("wip", role="WIP", vault="wip", draft="true")
        self.assertEqual(commands_persona._write_agent("wip"), "draft")

    def test_marking_a_live_persona_draft_removes_its_generated_agent(self):
        """Otherwise the persona stays dispatchable through a stale file — the exact
        thing `draft:` exists to prevent."""
        self._finished("alpha")
        self.assertEqual(commands_persona._write_agent("alpha"), "written")
        self.assertTrue(_agent_path("alpha").exists())
        p = config.PERSONAS_DIR / "alpha" / "persona.md"
        p.write_text(p.read_text().replace("name: alpha", "name: alpha\ndraft: true", 1))
        commands_persona.cmd_persona_sync_agents(SimpleNamespace(persona=None))
        self.assertFalse(_agent_path("alpha").exists(),
                         "a draft persona must not stay dispatchable")

    def test_a_draft_is_not_also_told_to_run_sync_agents(self):
        """The agent-in-sync check reported "no generated sub-agent — run `charter
        persona sync-agents`" beside the draft warning. For a draft that advice cannot
        work: sync-agents refuses by design. One honest message, not two, one wrong."""
        self.make_persona("wip", role="W", vault="wip", draft="true",
                          **{"delegate-when": "later"})
        msgs = [m for _lvl, m in commands_persona._agent_sync_issues("wip")]
        self.assertEqual(msgs, [])

    def test_a_finished_persona_is_still_told_when_its_agent_is_missing(self):
        self.make_persona("done", role="D", vault="done", **{"delegate-when": "x"})
        msgs = [m for _lvl, m in commands_persona._agent_sync_issues("done")]
        self.assertTrue(any("sync-agents" in m for m in msgs), msgs)

    def test_a_persona_may_declare_that_it_holds_no_credentials(self):
        """`lint` warned on any persona without a vault, but plenty legitimately have no
        credentials — a status-line or release persona touches nothing secret, or leans on
        a tool's own auth. The warning fired forever on personas that were entirely
        correct, and a lint with a permanent false positive is one people scroll past."""
        self.make_persona("tui", role="TUI", vault=persona.NO_VAULT,
                          **{"delegate-when": "layout"})
        msgs = [m for _lvl, m in persona.lint("tui")]
        self.assertFalse([m for m in msgs if "no vault" in m], msgs)

    def test_silence_about_a_vault_still_warns(self):
        """The case the warning was written for: an author who never considered it. Only
        an explicit declaration is believed — the same declared-not-inferred rule
        `[plane] shape` follows."""
        self.make_persona("quiet", role="Q", **{"delegate-when": "x"})
        msgs = [m for _lvl, m in persona.lint("quiet")]
        self.assertTrue([m for m in msgs if "no vault" in m], msgs)

    def test_the_sentinel_is_not_offered_as_a_vault_to_open(self):
        """Returning it would send every caller looking for a vault literally named
        `none`; they cannot tell the two apart and should not have to."""
        self.make_persona("tui", role="TUI", vault=persona.NO_VAULT,
                          **{"delegate-when": "layout"})
        self.assertIsNone(persona.vault_of("tui"))
        self.assertTrue(persona.declares_no_vault("tui"))

    def test_a_real_vault_name_is_unaffected(self):
        self.make_persona("ops", role="O", vault="ops", **{"delegate-when": "x"})
        self.assertEqual(persona.vault_of("ops"), "ops")
        self.assertFalse(persona.declares_no_vault("ops"))

    def test_asking_for_a_secret_says_which_of_the_two_situations_it_is(self):
        """"No vault" and "no credentials by design" send the user to fix different
        things — the second wants a different persona, not a vault."""
        self.make_persona("tui", role="TUI", vault=persona.NO_VAULT,
                          **{"delegate-when": "layout"})
        persona.set_active("tui")
        err = io.StringIO()
        with redirect_stderr(err):
            self.assertIsNone(commands_persona._resolve_vault(SimpleNamespace(persona="tui")))
        self.assertIn("no credentials by design", err.getvalue())

    def test_the_generated_agent_does_not_advertise_a_vault_it_has_none_of(self):
        """This string is what the router reads when choosing an agent, so a vault named
        here is a capability claim. Claiming one the sub-agent cannot open is worse than
        saying nothing."""
        self.make_persona("tui", role="TUI", vault=persona.NO_VAULT,
                          **{"delegate-when": "layout"})
        desc = commands_persona._agent_description("tui", persona.resolve("tui")["meta"])
        self.assertNotIn("vault", desc)
        self.assertIn("no credentials", desc)

    def test_the_generated_agent_names_the_actual_vault_not_the_persona(self):
        """These differ whenever `vault:` points elsewhere, and the description used the
        persona's own name unconditionally — so a persona on a shared vault was told to
        open one that does not exist."""
        self.make_persona("ops", role="Ops", vault="platform", **{"delegate-when": "x"})
        desc = commands_persona._agent_description("ops", persona.resolve("ops")["meta"])
        self.assertIn("'platform' vault", desc)

    def test_a_credential_free_sub_agent_still_gets_the_prohibition(self):
        """Having no vault is exactly when a sub-agent might improvise with a credential
        it found lying around — so the rule that must survive is the ban, not the how-to."""
        self.make_persona("tui", role="TUI", vault=persona.NO_VAULT,
                          **{"delegate-when": "layout"})
        r = persona.resolve("tui")
        body = commands_persona._render_agent("tui", r["meta"], r.get("charter") or "")
        self.assertIn("holds no credentials", body.lower())
        self.assertNotIn("--persona tui <list|exec|cp>", body)

    def test_the_generated_credential_command_actually_parses(self):
        """Issue #18. Every `charter …` command in a generated sub-agent is run through
        charter's REAL parser, because the broken form (`secret --persona X list`, which
        argparse rejects with "invalid choice: 'X'") is a perfectly plausible string and a
        string-match test passes it happily. It shipped twice: once originally, and once
        when the surrounding block was rewritten and the order carried forward unread.

        This is the only credential instruction those agents carry, so an agent following
        a broken one hits an argparse error and may reach for `op` directly — which is
        precisely what the vault abstraction exists to prevent.
        """
        import re as _re
        import shlex
        from charter import cli

        self.make_persona("ops", role="Ops", vault="ops", **{"delegate-when": "x"})
        self.make_persona("dev", role="Dev", vault="dev", uses="ops",
                          **{"delegate-when": "y"})
        parser = cli.build_parser()

        checked = 0
        for name in ("ops", "dev"):
            r = persona.resolve(name)
            body = commands_persona._render_agent(name, r["meta"], r.get("charter") or "")
            # Scoped to `persona secret`, the credential path this is about. A sweep over
            # every backticked command would also catch prose that CITES a command rather
            # than instructing one — the body says "Never `charter persona use` to switch
            # the active persona", a deliberate fragment — and telling citation from
            # instruction is guesswork this test should not be doing.
            for cmd in _re.findall(r"`(charter persona secret [^`]+)`", body):
                cmd = cmd.replace("…", "").strip()
                if "<" in cmd:                      # a placeholder, not a real invocation
                    cmd = _re.sub(r"<[^>]+>", "PLACEHOLDER", cmd)
                argv = shlex.split(cmd)[1:]         # drop the `charter` program name
                if not argv:
                    continue
                try:
                    parser.parse_args(argv)
                except SystemExit:
                    self.fail(f"generated agent for '{name}' carries an invocation charter "
                              f"itself rejects: {cmd}")
                checked += 1
        self.assertGreater(checked, 0, "no charter commands found to check — did the "
                                       "generated body stop carrying them?")

    def test_a_hand_written_agent_is_never_removed(self):
        """Generated files are charter's to manage; hand-written ones are not."""
        self.make_persona("wip", role="WIP", vault="wip", draft="true")
        _agent_path("wip").parent.mkdir(parents=True, exist_ok=True)
        _agent_path("wip").write_text("# hand-written, no marker\n")
        commands_persona.cmd_persona_sync_agents(SimpleNamespace(persona=None))
        self.assertEqual(_agent_path("wip").read_text(), "# hand-written, no marker\n")


if __name__ == "__main__":
    unittest.main()
