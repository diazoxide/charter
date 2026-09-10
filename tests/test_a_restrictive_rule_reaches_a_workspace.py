"""A `charter guard ask` rule is in force in a chat in a workspace — #942.

`charter guard ask <pattern>` writes the plane's own `.claude/settings.json` and tells the
operator it *"applies to everyone on this repo"*. A framed chat runs with its cwd at
`workspaces/<ws>/`, Claude Code reads project settings from the session's own directory and
does not walk up, and the file charter generates there carried `enabledPlugins` and `env`
and never `permissions`. So a force-prompt rule — a SAFETY rule — was silently not in force
in the chats where the guarded command actually runs, and every `doctor` row about that chat
was green.

**The fix is the restrictive half only.** `permissions.ask` and `permissions.deny` travel;
`permissions.allow` still does not. That keeps `claude_code.WORKSPACE_KEYS`' reasoning
exactly as it was — *"copying a grant sideways into a directory nobody granted it in puts a
permission in force where no one clicked for it"* — because a restrictive rule is the
opposite of a grant: it adds a prompt or a refusal, it never puts one in force.

**And from BOTH of the plane's files.** `doctor.LANDING_PROMPT` is
`charter guard ask --local 'charter change land *'`, and `--local` writes the plane's
gitignored `.claude/settings.local.json`. A machine-local rule has to land in a generated
file that is itself machine-local, which is why there are two generated files and not one.

Every test here writes only into a `PersonaIso` tmp plane — see `_planeguard` for what
touching a real `workspaces/` has cost before.
"""

from __future__ import annotations

import io
import json
import os
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, commands_workspace, config, doctor, workspace
from charter.harness import claude_code, registry

from tests import _isolation
from tests.test_a_workspace_carries_charters_layer import _plane_settings


def _plane_local(root: Path, **buckets) -> Path:
    """The plane's MACHINE-LOCAL settings — what `charter guard ask --local` writes.

    Spelled out here rather than driven through `add_permission_rule`, because half these
    cases are about a plane an operator wrote by hand: charter never writes a `deny` at all
    (`commands.py`'s own note), so a fixture that could only produce what charter writes
    could not state the case at all.
    """
    d = root / ".claude"
    d.mkdir(parents=True, exist_ok=True)
    p = d / "settings.local.json"
    p.write_text(json.dumps({"permissions": buckets}, indent=2) + "\n")
    return p


class PlaneWithRestrictions(_isolation.PersonaIso):
    """A real plane whose committed settings hold one of each bucket, and one workspace."""

    def setUp(self) -> None:
        super().setUp()
        # The tripwire the whole layer suite is written under: if `PersonaIso` ever stops
        # repointing derived paths at a throwaway tree, every write below lands in a real
        # plane.
        self.assertIn("edm-test-", str(config.STATE_DIR))
        _isolation.make_plane(self)
        _plane_settings(config.ROOT, permissions={
            "allow": ["Bash(ls:*)"],
            "ask": ["Bash(terraform apply *)"],
            "deny": ["Bash(rm -rf /)"],
        })
        self.ws = "north"
        workspace.ensure(self.ws)

    def generated(self, rel: str = "settings.json", name: str | None = None) -> Path:
        return workspace.workspace_dir(name or self.ws) / ".claude" / rel

    def doc(self, rel: str = "settings.json", name: str | None = None) -> dict:
        return json.loads(self.generated(rel, name).read_text())


class WhatTravelsIntoAWorkspace(PlaneWithRestrictions):
    def test_the_planes_ask_rule_is_in_force_in_the_workspace(self):
        """The whole of #942: this is the rule the operator was told applies to everyone."""
        self.assertEqual(self.doc()["permissions"]["ask"], ["Bash(terraform apply *)"])

    def test_a_hand_written_deny_travels_too(self):
        """Charter never writes a `deny` itself, so every one of them is an operator's own
        deliberate choice — and it was being dropped by the same key filter."""
        self.assertEqual(self.doc()["permissions"]["deny"], ["Bash(rm -rf /)"])

    def test_a_grant_still_never_travels(self):
        """`WORKSPACE_KEYS`' reasoning, unchanged: nothing is put in force in a directory
        nobody clicked for it in."""
        self.assertNotIn("allow", self.doc()["permissions"])
        self.assertNotIn("Bash(ls:*)", self.generated().read_text())

    def test_the_two_mirrored_keys_are_still_there_beside_it(self):
        self.assertEqual(sorted(self.doc()), ["enabledPlugins", "env", "permissions"])

    def test_a_plane_whose_permissions_are_all_grants_declares_none_at_all(self):
        """An empty `permissions` block in the generated file would look like policy."""
        _plane_settings(config.ROOT, permissions={"allow": ["Bash(ls:*)"]})
        workspace.wire_harnesses(self.ws)
        self.assertNotIn("permissions", self.doc())

    def test_a_bucket_that_is_not_a_list_is_read_past_rather_than_mirrored(self):
        """A `permissions` block of the wrong shape is somebody's deliberate structure.
        `add_permission_rule` refuses to WRITE into one; a mirror has less standing still,
        and must not put a shape the host cannot read into a second file."""
        _plane_settings(config.ROOT, permissions={"ask": "Bash(terraform apply *)",
                                                  "deny": ["Bash(rm -rf /)"]})
        workspace.wire_harnesses(self.ws)
        self.assertEqual(self.doc()["permissions"], {"deny": ["Bash(rm -rf /)"]})

    def test_the_rules_arrive_at_the_path_the_host_actually_reads(self):
        """Not a charter-shaped sidecar: the file Claude Code resolves from the session's
        own directory, which is what makes the rule in force rather than merely recorded."""
        self.assertIn(claude_code.WORKSPACE_SETTINGS, claude_code._PROJECT_SETTINGS)
        self.assertIn(claude_code.WORKSPACE_LOCAL_SETTINGS, claude_code._PROJECT_SETTINGS)


class AMachineLocalRuleStaysMachineLocal(PlaneWithRestrictions):
    """`doctor.LANDING_PROMPT` is a `--local` rule, so this is the case charter itself
    recommends and the one the issue records as still not in force anywhere."""

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        workspace.wire_harnesses(self.ws)

    def test_a_local_rule_lands_in_the_generated_local_file(self):
        self.assertEqual(self.doc("settings.local.json")["permissions"]["ask"],
                         ["Bash(charter change land *)"])

    def test_it_does_not_leak_into_the_committed_sibling(self):
        """The two plane files differ only in blast radius, and folding one into the other
        would publish a personal decision to every reader of the generated file."""
        self.assertNotIn("charter change land", self.generated().read_text())

    def test_the_generated_local_file_carries_permissions_and_nothing_else(self):
        """`enabledPlugins` and `env` are the committed file's to mirror. A second copy
        here would be one document contradicting the other with no rule saying which wins."""
        self.assertEqual(list(self.doc("settings.local.json")), ["permissions"])

    def test_a_local_grant_does_not_travel_either(self):
        _plane_local(config.ROOT, allow=["Bash(gh pr merge *)"])
        workspace.wire_harnesses(self.ws)
        self.assertFalse(self.generated("settings.local.json").exists())

    def test_a_plane_with_no_local_file_gets_no_generated_one(self):
        (config.ROOT / ".claude" / "settings.local.json").unlink()
        workspace.wire_harnesses(self.ws)
        self.assertFalse(self.generated("settings.local.json").exists())

    def test_a_malformed_local_file_is_never_guessed_over(self):
        """`_load_json_settings`' restraint, carried through: the operator's content is in
        there, and a mirror is not the place to start interpreting it."""
        (config.ROOT / ".claude" / "settings.local.json").write_text("{ not json")
        workspace.wire_harnesses(self.ws)
        self.assertFalse(self.generated("settings.local.json").exists())

    def test_a_plane_with_only_a_local_rule_still_gets_that_file(self):
        """No committed settings at all is an ordinary plane, and the `--local` rule is
        exactly the one charter's own doctor recommends there."""
        (config.ROOT / ".claude" / "settings.json").unlink()
        workspace.ensure("solo")
        self.assertFalse(self.generated("settings.json", "solo").exists())
        self.assertIn("charter change land",
                      self.generated("settings.local.json", "solo").read_text())


class ARuleTheePlaneDropsIsWithdrawn(PlaneWithRestrictions):
    """The mirror runs both ways, and it has to.

    `charter guard` has no remove verb: a rule goes by editing the plane's settings. Before
    #942 a generated file stopped being wanted only when the plane's whole settings file
    did, so nothing ever needed withdrawing. Now a plane that drops its last `--local` rule
    stops wanting a whole generated file — and a mirror that only ever adds would put a
    restriction in force in every workspace and every checkout with no way to lift it.
    """

    def setUp(self) -> None:
        super().setUp()
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        workspace.wire_harnesses(self.ws)
        self.assertTrue(self.generated("settings.local.json").is_file(),
                        "fixture never mirrored the file this case is about")

    def test_deleting_the_planes_local_file_lifts_the_rule_from_the_workspace(self):
        (config.ROOT / ".claude" / "settings.local.json").unlink()
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(rows[".claude/settings.local.json"], "removed")
        self.assertFalse(self.generated("settings.local.json").exists())

    def test_dropping_the_last_restrictive_rule_lifts_it_too(self):
        """The bucket emptied rather than the file deleted — the ordinary edit."""
        _plane_local(config.ROOT, allow=["Bash(gh pr merge *)"])
        workspace.wire_harnesses(self.ws)
        self.assertFalse(self.generated("settings.local.json").exists())

    def test_a_malformed_plane_file_withdraws_rather_than_guessing(self):
        """Charter cannot read what the plane now says, so it cannot claim the old rule is
        still the plane's — and a stale restriction nobody can trace is worse than none."""
        (config.ROOT / ".claude" / "settings.local.json").write_text("{ not json")
        workspace.wire_harnesses(self.ws)
        self.assertFalse(self.generated("settings.local.json").exists())

    def test_a_generated_file_somebody_edited_is_never_deleted(self):
        """The destructive direction. `unwire_guest`'s rule verbatim: charter withdraws
        only a file whose content still matches the digest it recorded."""
        (config.ROOT / ".claude" / "settings.local.json").unlink()
        mine = '{"permissions": {"ask": ["Bash(mine *)"]}}\n'
        self.generated("settings.local.json").write_text(mine)
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(self.generated("settings.local.json").read_text(), mine)
        self.assertNotIn(".claude/settings.local.json", rows)

    def test_bytes_charter_cannot_read_are_left_exactly_where_they_are(self):
        """Unreadable and edited are one answer here — charter cannot compare it against
        what it wrote, so it is the operator's."""
        (config.ROOT / ".claude" / "settings.local.json").unlink()
        self.generated("settings.local.json").write_bytes(b"\xff\xfe not utf-8")
        workspace.wire_harnesses(self.ws)
        self.assertEqual(self.generated("settings.local.json").read_bytes(),
                         b"\xff\xfe not utf-8")

    def test_a_marker_entry_whose_file_is_already_gone_costs_nothing(self):
        """`wire_harnesses` runs on every launch, so a file somebody deleted by hand is an
        ordinary state and must not raise on the way past it."""
        (config.ROOT / ".claude" / "settings.local.json").unlink()
        self.generated("settings.local.json").unlink()
        rows = dict(workspace.wire_harnesses(self.ws))
        self.assertEqual(rows[".claude/settings.json"], "present")

    def test_two_withdrawals_come_back_in_path_order(self):
        """Path order, not whatever a set happens to iterate in. `cmd_workspace_reinit`
        prints one line per row, and a repair whose report reshuffles between runs cannot
        be diffed against the last one — `TheRowsComeBackInOneOrder`'s argument, on the
        rows this adds."""
        (config.ROOT / ".claude" / "settings.json").unlink()
        (config.ROOT / ".claude" / "settings.local.json").unlink()
        rows = workspace.wire_harnesses(self.ws)
        self.assertEqual(rows, [(".claude/settings.json", "removed"),
                                (".claude/settings.local.json", "removed")])

    def test_the_marker_stops_vouching_for_what_was_withdrawn(self):
        (config.ROOT / ".claude" / "settings.local.json").unlink()
        workspace.wire_harnesses(self.ws)
        marker = json.loads(
            (workspace.workspace_dir(self.ws) / workspace.GENERATED_MARKER).read_text())
        self.assertNotIn(".claude/settings.local.json", marker)
        self.assertIn(".claude/settings.json", marker)

    def test_an_emptied_claude_directory_does_not_stay_behind(self):
        """Charter still visible in a directory it has nothing in — and in a guest checkout
        that directory belongs to somebody else's repository."""
        (config.ROOT / ".claude" / "settings.json").unlink()
        (config.ROOT / ".claude" / "settings.local.json").unlink()
        workspace.wire_harnesses(self.ws)
        self.assertFalse((workspace.workspace_dir(self.ws) / ".claude").exists())

    def test_reinit_says_it_removed_rather_than_calling_it_refreshed(self):
        (config.ROOT / ".claude" / "settings.local.json").unlink()
        said: list[str] = []
        with mock.patch.object(commands_workspace.util, "ok",
                               side_effect=lambda m: said.append(m)), \
             mock.patch.object(commands_workspace.util, "info", side_effect=lambda m: None):
            commands_workspace.cmd_workspace_reinit(
                SimpleNamespace(name=self.ws, all=False))
        self.assertIn(f"Reinitialized '{self.ws}' → removed .claude/settings.local.json — "
                      f"the plane no longer declares it (charter's harness layer).", said)

    def test_a_withdrawal_is_hidden_no_longer_in_a_checkout(self):
        """The exclude block lists what the marker says charter owns, so a withdrawal has
        to take its line with it — otherwise charter is hiding a path it does not own."""
        clone = workspace.workspace_dir(self.ws) / "svc"
        (clone / ".git").mkdir(parents=True)
        workspace.wire_harnesses(self.ws)
        self.assertIn("/.claude/settings.local.json",
                      (clone / ".git" / "info" / "exclude").read_text())
        (config.ROOT / ".claude" / "settings.local.json").unlink()
        workspace.wire_harnesses(self.ws)
        self.assertNotIn("/.claude/settings.local.json",
                         (clone / ".git" / "info" / "exclude").read_text())
        self.assertFalse((clone / ".claude" / "settings.local.json").exists())


class ACheckoutInsideAWorkspaceGetsThemToo(PlaneWithRestrictions):
    """`workspaces/<ws>/<repo>/` is a repository of its own — the widest gap of the three,
    and where the guarded command is most likely to be typed."""

    def setUp(self) -> None:
        super().setUp()
        self.clone = workspace.workspace_dir(self.ws) / "svc"
        (self.clone / ".git").mkdir(parents=True)
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        workspace.wire_harnesses(self.ws)

    def test_the_committed_restrictions_reach_the_checkout(self):
        doc = json.loads((self.clone / ".claude" / "settings.json").read_text())
        self.assertEqual(doc["permissions"], {"ask": ["Bash(terraform apply *)"],
                                              "deny": ["Bash(rm -rf /)"]})

    def test_the_machine_local_ones_reach_it_in_a_machine_local_file(self):
        doc = json.loads((self.clone / ".claude" / "settings.local.json").read_text())
        self.assertEqual(doc["permissions"]["ask"], ["Bash(charter change land *)"])

    def test_no_grant_reaches_a_repository_nobody_granted_it_in(self):
        self.assertNotIn("Bash(ls:*)",
                         (self.clone / ".claude" / "settings.json").read_text())

    def test_both_files_are_hidden_from_the_checkouts_own_git_status(self):
        """A generated file charter does not register is charter's noise in somebody
        else's `git status` — the cost #870 pays for writing here at all."""
        body = (self.clone / ".git" / "info" / "exclude").read_text()
        self.assertIn("/.claude/settings.json", body)
        self.assertIn("/.claude/settings.local.json", body)

    def test_removing_the_layer_takes_the_local_file_with_it(self):
        workspace.unwire_guest(self.clone)
        self.assertFalse((self.clone / ".claude" / "settings.local.json").exists())


class GuardAskKeepsTheMirrorInStep(PlaneWithRestrictions):
    """The command that writes the rule refreshes the generated files it has just made
    stale. Without it the rule reaches a workspace chat at the NEXT launch, and the operator
    is told it applies now."""

    def setUp(self) -> None:
        super().setUp()
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(config.ROOT)

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()

    def test_the_rule_is_in_the_workspace_the_moment_the_command_returns(self):
        self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *", local=False)
        self.assertIn("Bash(kubectl delete *)", self.doc()["permissions"]["ask"])

    def test_a_local_rule_reaches_the_generated_local_file(self):
        self.invoke(commands.cmd_guard_ask, pattern="charter change land *", local=True)
        self.assertIn("Bash(charter change land *)",
                      self.doc("settings.local.json")["permissions"]["ask"])

    def test_the_landing_prompt_charter_itself_recommends_reaches_a_workspace_chat(self):
        """`doctor.LANDING_PROMPT` verbatim — the rule charter names and does not run, and
        the one the issue traced to a generated file charter never wrote."""
        self.assertIn("--local", doctor.LANDING_PROMPT)
        pattern = doctor.LANDING_PROMPT.split("--local ", 1)[1].strip().strip("'")
        self.invoke(commands.cmd_guard_ask, pattern=pattern, local=True)
        self.assertIn(commands._as_rule(pattern),
                      self.doc("settings.local.json")["permissions"]["ask"])

    def test_it_says_the_rule_travelled_rather_than_writing_in_silence(self):
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                              local=False)
        self.assertIn("workspaces/", said)

    def test_a_plane_with_no_workspaces_says_nothing_about_mirroring(self):
        """Idempotent means quiet: there is nothing to mirror into and no line to print."""
        import shutil

        shutil.rmtree(config.WORKSPACES_DIR / self.ws)
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                              local=False)
        self.assertNotIn("brought into step", said)

    def test_a_generated_file_somebody_edited_is_named_rather_than_repaired(self):
        """Charter never repairs a file it did not write, so the rule is honestly not in
        force in that chat — and silence there would be the tick that stops you checking."""
        self.generated().write_text('{"env": {"MINE": "1"}}\n')
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                              local=False)
        self.assertIn("NOT in force", said)
        self.assertIn(f"{self.ws}/.claude/settings.json", said)
        self.assertEqual(self.generated().read_text(), '{"env": {"MINE": "1"}}\n')

    def test_nothing_is_named_as_out_of_reach_when_everything_took_the_rule(self):
        """The other half of that warning. A sentence naming no files is a warning about
        nothing, and warnings about nothing are what teach an operator to skim the ones
        that are real."""
        _, said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                              local=False)
        self.assertNotIn("NOT in force", said)

    def test_an_allow_rule_does_not_refresh_anything_sideways(self):
        """`cmd_guard_allow` deliberately does not mirror. A grant is never carried, so a
        refresh there could only push UNRELATED drift into every workspace as a side effect
        of a command that did not ask for it."""
        _plane_settings(config.ROOT, enabledPlugins={"charter@charter": True,
                                                     "later@market": True})
        self.invoke(commands.cmd_guard_allow, pattern="npm test *", local=False)
        self.assertNotIn("later@market", self.generated().read_text())
        self.assertNotIn("npm test", self.generated().read_text())

    def test_a_workspaces_directory_that_cannot_be_read_does_not_fail_the_command(self):
        """The rule IS written; the mirror is the follow-up. A plane whose `workspaces/`
        cannot be listed must still get the rule into its settings file."""
        with mock.patch.object(workspace, "list_workspaces",
                               side_effect=OSError("workspaces/ is unreadable")):
            rc, _said = self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *",
                                    local=False)
        self.assertEqual(rc, 0)
        self.assertIn("Bash(kubectl delete *)", json.loads(
            (config.ROOT / ".claude" / "settings.json").read_text())["permissions"]["ask"])

    def test_one_workspace_that_cannot_be_wired_does_not_cost_the_others(self):
        workspace.ensure("south")
        real = workspace.wire_harnesses

        def _one_fails(name):
            if name == "north":
                raise OSError("this one is unwireable")
            return real(name)

        with mock.patch.object(workspace, "wire_harnesses", side_effect=_one_fails):
            self.invoke(commands.cmd_guard_ask, pattern="kubectl delete *", local=False)
        self.assertIn("Bash(kubectl delete *)",
                      self.doc(name="south")["permissions"]["ask"])


class DoctorNamesTheLagRatherThanTicking(PlaneWithRestrictions):
    """Three rows were green over a chat with none of the plane's rules. The one that can
    name it is `workspace layer`, which regenerates and compares — so it sees the gap the
    moment the generator emits the buckets, and what it has to ADD is the consequence."""

    def test_a_current_workspace_still_reads_ok(self):
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.OK)

    def test_a_workspace_behind_the_planes_restrictions_reads_as_a_problem(self):
        _plane_settings(config.ROOT, permissions={"ask": ["Bash(terraform apply *)",
                                                          "Bash(kubectl delete *)"]})
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(f"{self.ws}/.claude/settings.json (stale)", r.detail)
        self.assertIn("prompted or refused by them", r.hint)
        self.assertIn("charter workspace reinit", r.hint)

    def test_a_foreign_generated_file_never_receives_them_and_the_row_says_so(self):
        """The state charter will never repair, so `reinit` alone is not the whole
        remedy — the file has to be removed first, and until it is the rule is not in
        force there."""
        self.generated().write_text('{"env": {"MINE": "1"}}\n')
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("charter did not write", r.hint)
        self.assertIn("prompted or refused by them", r.hint)

    def test_a_plane_with_no_restrictions_at_all_gets_no_such_sentence(self):
        """The guard on the sentence. A row that talks about "the plane's ask/deny rules"
        where there are none is a fact charter invented, and an operator who checks finds
        nothing — the cry-wolf failure `check_harness` records."""
        _plane_settings(config.ROOT)
        self.generated().unlink()
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertNotIn("prompted or refused by them", r.hint)

    def test_a_harness_that_cannot_answer_costs_the_sentence_and_not_the_row(self):
        """The findings are what the operator needs. Which of the plane's rules ride in a
        generated file is an extra sentence beside them, so a harness that raises when
        asked loses the sentence and never the row."""
        class _Mute(claude_code.ClaudeCodeHarness):
            def restrictive_rules(self):
                raise ValueError("this harness cannot say")

        self.generated().unlink()
        with mock.patch.object(registry, "all", return_value=[_Mute()]):
            r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(f"{self.ws}/.claude/settings.json (missing)", r.detail)
        self.assertNotIn("prompted or refused by them", r.hint)

    def test_a_harness_answering_none_costs_the_sentence_and_not_the_row(self):
        """`or ()` — the third answer a third-party integration can give, beside a real
        tuple and a raise. `_layer_files` keeps the same fallback one module over for the
        same reason: a bug in one harness must not cost the report about every other."""
        class _Null(claude_code.ClaudeCodeHarness):
            def restrictive_rules(self):
                return None

        self.generated().unlink()
        with mock.patch.object(registry, "all", return_value=[_Null()]):
            r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(f"{self.ws}/.claude/settings.json (missing)", r.detail)


class TheAskRulesRowSeesThemInAWorkspaceChat(PlaneWithRestrictions):
    """`check_ask_rules` reads the settings THIS SESSION reads (#855). In a workspace chat
    that is the generated file, which is why the row said `none` while the plane held
    rules — and why it now says what it says."""

    def test_a_chat_in_a_workspace_reports_the_planes_rule_rather_than_none(self):
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(workspace.workspace_dir(self.ws))
        r = doctor.check_ask_rules()
        self.assertEqual(r.status, doctor.OK)
        self.assertNotEqual(r.detail, "none")
        self.assertIn("1 rule(s)", r.detail)

    def test_a_chat_in_a_workspace_can_now_see_a_rule_shadowing_a_persona_tool(self):
        """The row exists to say why pre-approved tools started prompting. It could not
        answer that in a workspace chat at all, because the rule was not in the file it
        reads — and now the rule IS in force there, so the shadowing is real."""
        self.make_persona("ops", role="Ops", vault="none", tools="kubectl")
        from charter import persona

        persona.set_active("ops")
        _plane_settings(config.ROOT, permissions={"ask": ["Bash(kubectl *)"]})
        workspace.wire_harnesses(self.ws)
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(workspace.workspace_dir(self.ws))
        r = doctor.check_ask_rules()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("kubectl", f"{r.detail} {r.hint}")


class TheBucketsAreNamedAndAllowIsNotAmongThem(PlaneWithRestrictions):
    def test_only_the_restrictive_buckets_are_mirrored(self):
        """A literal, spelled here by hand. `RESTRICTIVE_BUCKETS` is the whole of what
        separates this change from the one `WORKSPACE_KEYS` refuses, and a test that reads
        the constant agrees with any value it takes."""
        self.assertEqual(claude_code.RESTRICTIVE_BUCKETS, ("ask", "deny"))

    def test_the_harness_answers_which_of_the_planes_rules_ride_in_its_files(self):
        """Asked of the harness rather than read out of a settings file by `doctor`: a
        literal there is the hardcoded-literal-per-harness failure `registry.py` ends."""
        self.assertEqual(sorted(claude_code.ClaudeCodeHarness().restrictive_rules()),
                         ["Bash(rm -rf /)", "Bash(terraform apply *)"])

    def test_a_harness_that_carries_no_workspace_files_carries_no_rules(self):
        for h in registry.all():
            if not h.workspace_files():
                self.assertEqual(h.restrictive_rules(), (),
                                 f"{h.name} claims rules ride in files it does not write")


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
