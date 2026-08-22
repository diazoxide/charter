import os
import unittest
from unittest import mock

from tests._isolation import PersonaIso, run_hook
from charter import hooks, config, persona


def _decision(r):
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


def _context(r):
    return (r or {}).get("hookSpecificOutput", {}).get("additionalContext", "")


class TestLeakGuard(PersonaIso):  # A
    def test_reveal_flag_denied(self):
        r = run_hook(hooks.pretooluse, {"tool_input": {"command": "edm persona secret get k --reveal"}})
        self.assertEqual(_decision(r), "deny")

    def test_cat_vault_denied(self):
        r = run_hook(hooks.pretooluse, {"tool_input": {"command": "cat .charter/vaults/dev.json"}})
        self.assertEqual(_decision(r), "deny")

    def test_ls_vault_is_fine(self):
        self.assertIsNone(run_hook(hooks.pretooluse, {"tool_input": {"command": "ls .charter/vaults/"}}))


class InAControlPlane(PersonaIso):
    """A tmp plane that actually IS one.

    `pretooluse` gates the single-credential and plane-root guards on
    `config.HAS_CONTROL_PLANE`: the plugin installs per user or per project but the handler
    ran everywhere, so `git clone git@…`, `git commit -S` and `ssh -T git@github.com` were
    denied in every unrelated repo on the machine, explaining a control plane that did not
    exist there. These classes assert the guards FIRE, which is in-plane behaviour, so the
    fixture has to be in a plane. `PersonaIso` alone is a bare tmp dir — the out-of-plane
    case — which `TestGuardsAreScopedToAPlane` covers deliberately.

    `_leak_reason` is not gated and needs no plane: not printing a secret into the
    transcript is a safety invariant, not a policy a plane happens to hold.
    """

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True


class TestCommittingInACloneIsNotNudged(InAControlPlane):
    """Guard B is gone (#371). Committing in a clone is charter's OWN prescribed workflow.

    `charter clone` puts every repo under `workspaces/`, and `skills/working-in-a-clone`
    says *"Commit to the repo you are in"* — so the nudge's trigger condition was the
    intended state, not a deviation from it. Measured: 471 asks in one plane over two weeks,
    every one of them this rule, 97 of 98 approved on the first day approvals were countable.

    These cases are the ones that USED to ask. They are kept, inverted, because "the nudge
    is gone" has to be asserted at the shapes that produced it — a bare `assertIsNone` on
    some unrelated command would pass whether or not the code came out.
    """

    def test_cd_into_a_clone_then_commit_is_silent(self):
        self.assertIsNone(run_hook(hooks.pretooluse, {
            "tool_input": {"command": "cd workspaces/default/x && git commit -m y"},
            "cwd": str(self.tmp)}))

    def test_git_dash_C_into_a_clone_is_silent(self):
        self.assertIsNone(run_hook(hooks.pretooluse, {
            "tool_input": {"command": "git -C workspaces/default/x push"}, "cwd": str(self.tmp)}))

    def test_the_legacy_repos_path_is_silent(self):
        self.assertIsNone(run_hook(hooks.pretooluse, {
            "tool_input": {"command": "cd repos/default/x && git commit -m y"},
            "cwd": str(self.tmp)}))

    def test_a_commit_with_cwd_inside_a_clone_is_silent(self):
        cwd = config.WORKSPACES_DIR / "ws" / "repo"
        cwd.mkdir(parents=True)
        self.assertIsNone(run_hook(hooks.pretooluse,
                                   {"tool_input": {"command": "git commit -m x"}, "cwd": str(cwd)}))

    def test_a_command_that_merely_mentions_a_clone_path_is_silent(self):
        """The false-positive half. `_REPOS_REF_RE` scanned the raw command string, so a
        `grep`, an `echo`, a commit message or a `gh` comment body all reproduced as `ask`
        — the technique this file abandoned twice elsewhere (`_leak_reason`,
        `_single_credential_hit`) for causing exactly this."""
        for cmd in ("grep -rn 'git commit' workspaces/demo/repo",
                    "gh issue comment 5 --body 'we should git rebase workspaces/demo/repo'",
                    "echo 'next: git push from workspaces/demo/repo'"):
            with self.subTest(cmd=cmd):
                self.assertIsNone(run_hook(hooks.pretooluse,
                                           {"tool_input": {"command": cmd}, "cwd": "/tmp"}))

    def test_no_ask_row_is_traced_for_a_clone_commit(self):
        """The trace event went with it: `ask` had exactly one producer."""
        from charter import trace
        cwd = config.WORKSPACES_DIR / "ws" / "repo"
        cwd.mkdir(parents=True)
        run_hook(hooks.pretooluse, {"tool_input": {"command": "git commit -m x"},
                                    "cwd": str(cwd), "session_id": "s-371"})
        self.assertEqual([e for e in trace.read("s-371") if e.get("event") == "ask"], [])

    def test_no_pending_ask_marker_is_left_behind(self):
        cwd = config.WORKSPACES_DIR / "ws" / "repo"
        cwd.mkdir(parents=True)
        run_hook(hooks.pretooluse, {"tool_input": {"command": "git commit -m x"}, "cwd": str(cwd),
                                    "session_id": "s-371", "tool_use_id": "tu-371"})
        self.assertEqual(list(config.SESSIONS_DIR.glob("*.ask-pending")), [])

    def test_the_release_floor_still_stops_an_unattended_tag(self):
        """The one thing the nudge covered by ACCIDENT (#299) is covered on purpose by A4,
        which runs BEFORE it did — so removing B cannot reopen it."""
        cwd = config.WORKSPACES_DIR / "ws" / "repo"
        cwd.mkdir(parents=True)
        r = run_hook(hooks.pretooluse, {"tool_input": {"command": "git tag v9.9.9"},
                                        "cwd": str(cwd), "permission_mode": "bypassPermissions"})
        self.assertEqual(_decision(r), "deny")

    def test_secret_leak_still_hard_denied(self):  # A stays a hard block
        r = run_hook(hooks.pretooluse, {"tool_input": {"command": "cat .charter/vaults/dev.json"}})
        self.assertEqual(_decision(r), "deny")

    def test_add_personas_not_denied(self):
        self.assertIsNone(run_hook(hooks.pretooluse,
                                   {"tool_input": {"command": "git add personas/dev/memory/"}, "cwd": str(self.tmp)}))


class TestGateFallthrough(PersonaIso):
    def test_declared_tool_allowed(self):
        self.make_persona("dev", role="Dev", vault="dev", tools="kubectl")
        with mock.patch.dict(os.environ, {"CHARTER_PERSONA": "dev"}):
            r = run_hook(hooks.pretooluse, {"tool_input": {"command": "kubectl get pods"}})
        self.assertEqual(_decision(r), "allow")

    def test_undeclared_tool_silent(self):
        self.make_persona("dev", role="Dev", vault="dev", tools="kubectl")
        with mock.patch.dict(os.environ, {"CHARTER_PERSONA": "dev"}):
            self.assertIsNone(run_hook(hooks.pretooluse, {"tool_input": {"command": "helm list"}}))


class TestSessionStart(PersonaIso):  # C
    def test_injects_active_persona_memory(self):
        self.make_persona("dev", role="Dev", vault="dev")
        persona.remember("dev", "fact one", title="one")
        with mock.patch.dict(os.environ, {"CHARTER_PERSONA": "dev"}):
            r = run_hook(hooks.sessionstart, {"session_id": "t"})
        ctx = _context(r)
        self.assertIn("dev", ctx)
        self.assertIn("one", ctx)

    def test_role_injected_even_without_memory(self):
        # A persona's ROLE (identity + remit) is injected even with no memory, so the
        # default persona reliably shapes the session. ($CHARTER_WORKSPACE suppresses the
        # separate workspace nudge so this isolates the persona branch.)
        self.make_persona("dev", role="Dev", vault="dev", **{"delegate-when": "dev tasks"})
        with mock.patch.dict(os.environ, {"CHARTER_PERSONA": "dev", "CHARTER_WORKSPACE": "default"}):
            ctx = _context(run_hook(hooks.sessionstart, {"session_id": "t"}))
        self.assertIn("dev", ctx)
        self.assertIn("`dev` persona for this session", ctx)
        self.assertIn("dev tasks", ctx)  # the remit (delegate-when) is surfaced

    def test_no_active_persona_is_silent(self):
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "default"}, clear=False):
            os.environ.pop("CHARTER_PERSONA", None)
            self.assertIsNone(run_hook(hooks.sessionstart, {"session_id": "t"}))


class TestMemorySecretScan(PersonaIso):  # D
    def _write_path(self, name):
        p = config.PERSONAS_DIR / "dev" / "memory" / name
        p.parent.mkdir(parents=True, exist_ok=True)
        return p

    def test_secret_in_memory_warns_without_echoing_value(self):
        p = self._write_path("leak.md")
        r = run_hook(hooks.posttooluse, {"tool_name": "Write",
                                         "tool_input": {"file_path": str(p), "content": "api_key = sk-abcdef123456"}})
        ctx = _context(r)
        self.assertIn("SECURITY", ctx)
        self.assertNotIn("sk-abcdef123456", ctx)  # the value must never be echoed

    def test_clean_memory_is_silent(self):
        p = self._write_path("ok.md")
        self.assertIsNone(run_hook(hooks.posttooluse, {"tool_name": "Write",
                                   "tool_input": {"file_path": str(p), "content": "KC runs as a StatefulSet of 3"}}))

    def test_non_memory_file_ignored(self):
        p = self.tmp / "edm" / "x.py"
        self.assertIsNone(run_hook(hooks.posttooluse, {"tool_name": "Write",
                                   "tool_input": {"file_path": str(p), "content": "password = hunter2xxx"}}))

    def test_non_write_tool_ignored(self):
        p = self._write_path("leak2.md")
        self.assertIsNone(run_hook(hooks.posttooluse, {"tool_name": "Bash",
                                   "tool_input": {"file_path": str(p), "content": "token = sk-zzzzzzzzzz"}}))


class TestCommitmentGate(PersonaIso):  # F — ask before you build
    """The steward's quizzing was measured at 1-per-10 prompts with a daily rate swinging
    0.00–0.31: discretionary, so it fired on whim and went quiet during long grinds. This
    gate supplies the missing TRIGGER.

    The false-positive tests carry the real weight: a nudge that fires on a lookup becomes
    wallpaper and gets tuned out, which would leave the discipline worse off than before.
    """

    def _fires(self, prompt: str) -> bool:
        return bool(hooks._commitment_signals(prompt))

    # --- must fire: an action verb PLUS a genuine fork ------------------------------
    def test_fuzzy_build_request_fires(self):
        self.assertTrue(self._fires("can we somehow make the statusline better?"))

    def test_broad_scope_fires(self):
        self.assertTrue(self._fires("add a health check across every repo"))

    def test_destructive_fires(self):
        self.assertTrue(self._fires("remove the legacy personas and rewrite their charters"))

    def test_long_multipart_ask_fires(self):
        self.assertTrue(self._fires(
            "implement the new routing layer: " + "it should handle retries and backoff, "
            "plus a metrics hook, and we need to keep the old path working " * 3))

    # --- must NOT fire: no fork, or not a request for work -------------------------
    def test_plain_question_is_silent(self):
        for q in ("what does edm recall do?",
                  "why is the dispatch tally committing so often?",
                  "is all committed pushed?",
                  "how many personas are there?",
                  "show me the worktrees"):
            self.assertFalse(self._fires(q), q)

    def test_precise_small_fix_is_silent(self):
        """Action, but no fork — there is nothing to quiz about."""
        self.assertFalse(self._fires("fix the typo in docs/workspaces.md line 4"))

    def test_status_check_is_silent(self):
        self.assertFalse(self._fires("check the CI status"))

    def test_empty_prompt_is_silent(self):
        self.assertFalse(self._fires(""))
        self.assertFalse(self._fires("   "))

    def test_question_wins_over_action_words(self):
        """Leading interrogatives suppress even when a build verb appears later."""
        self.assertFalse(self._fires("what would it take to migrate every repo?"))

    # --- the emitted directive + cadence ------------------------------------------
    def test_nudge_names_the_signal_and_the_human_only_skills(self):
        ctx = hooks._commitment_nudge("can we somehow improve this across all repos?", None)
        self.assertIn("open-ended wording", ctx)
        self.assertIn("broad scope", ctx)
        self.assertIn("AskUserQuestion", ctx)
        self.assertIn("grill-with-docs", ctx)      # human-only: nobody else can offer it
        self.assertIn("Scout first", ctx)

    def test_a_bug_report_is_long_only_because_of_the_PASTE(self):
        """Validated against 935 real prompts: raw length flagged bug reports whose bulk was a
        pasted curl/stack-trace. Quizzing someone about approach when they handed you an error
        is the false positive that teaches them to ignore the gate."""
        report = ("fix the login error\n\n" + '{"dd":{"trace_id":"abc"},"msg":"boom"}' * 20)
        self.assertNotIn("a long, multi-part ask", hooks._commitment_signals(report))

    def test_long_PROSE_still_counts_as_multi_part(self):
        ask = ("refactor the routing layer. " + "it needs retries, backoff, a metrics hook, "
               "and the old path has to keep working while we migrate. " * 4)
        self.assertIn("a long, multi-part ask", hooks._commitment_signals(ask))

    def test_symptom_report_gets_diagnosis_not_a_design_quiz(self):
        ctx = hooks._commitment_nudge(
            "fix the migration bug, somehow every org fails with a 422", None)
        self.assertIn("symptom to diagnose", ctx)
        self.assertIn("systematic-debugging", ctx)
        self.assertIn("Quiz only if", ctx)          # a symptom needs a fix, not a questionnaire
        self.assertNotIn("brainstorming", ctx)

    def test_build_request_gets_the_design_route(self):
        ctx = hooks._commitment_nudge("can we somehow improve the statusline?", None)
        self.assertIn("work to be built", ctx)
        self.assertIn("brainstorming", ctx)

    def test_cooldown_suppresses_an_immediate_repeat(self):
        p = "can we somehow improve the routing?"
        self.assertTrue(hooks._commitment_nudge(p, "sess-1"))
        self.assertFalse(hooks._commitment_nudge(p, "sess-1"))   # clarification exchange

    def test_hook_emits_nothing_for_a_lookup(self):
        self.assertIsNone(run_hook(hooks.userpromptsubmit,
                                   {"prompt": "what is the active workspace?",
                                    "session_id": "sess-2"}))


class TestSshGuardCoversEveryForge(InAControlPlane):
    """The guard denied SSH for gitlab.com only. Under two forges that is worse than no
    guard — it holds for one host and silently lapses for the other, while still LOOKING
    present. Every configured forge host must be covered."""

    def _deny(self, cmd):
        r = run_hook(hooks.pretooluse, {"tool_input": {"command": cmd}})
        return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")

    def test_ssh_remote_denied_for_both_hosts(self):
        for host in ("gitlab.com", "github.com"):
            self.assertEqual(self._deny(f"git clone git@{host}:acme/api.git"), "deny", host)
            self.assertEqual(self._deny(f"git remote add o ssh://git@{host}/acme/api.git"),
                             "deny", host)

    def test_ssh_probe_denied_for_both_hosts(self):
        for host in ("gitlab.com", "github.com"):
            self.assertEqual(self._deny(f"ssh -T git@{host}"), "deny", host)

    def test_git_ssh_command_bypass_denied(self):
        self.assertEqual(self._deny("GIT_SSH_COMMAND=ssh git fetch"), "deny")

    def test_signing_flags_denied(self):
        self.assertEqual(self._deny("git commit -S -m x"), "deny")
        self.assertEqual(self._deny("git commit --gpg-sign -m x"), "deny")

    def test_https_clone_is_allowed_for_both_hosts(self):
        for host in ("gitlab.com", "github.com"):
            self.assertIsNone(self._deny(f"git clone https://{host}/acme/api.git"), host)

    def test_declared_self_hosted_forge_is_covered_too(self):
        """FINDING 1's guard half: a self-hosted host DECLARED in charter.toml must be
        denied exactly like a default host — the guard already widened via
        `registry.known_forges`; this pins that the widening survived the refactor that
        shares it with `gitpolicy.forge_for`/`commands._origin_https`."""
        (config.ROOT / "charter.toml").write_text(
            '[[forge]]\nkind = "gitlab"\nhost = "git.internal"\ngroup = "acme"\n')
        self.assertEqual(self._deny("git clone git@git.internal:acme/api.git"), "deny")

    # --- FINDING 2, shape A: `-c core.sshCommand=…` — GIT_SSH_COMMAND's config twin
    def test_core_sshcommand_config_denied_before_and_after_subcommand(self):
        self.assertEqual(self._deny("git -c core.sshCommand=ssh fetch"), "deny")
        self.assertEqual(self._deny("git fetch -c core.sshCommand=ssh"), "deny")

    # --- FINDING 2, shape B: git treats hostnames case-insensitively
    def test_ssh_remote_denied_regardless_of_host_case(self):
        for host in ("GITHUB.COM", "GitLab.Com"):
            self.assertEqual(self._deny(f"git clone git@{host}:acme/api.git"), "deny", host)

    # --- FINDING 2, sibling C: `--config-env=core.sshCommand=VAR` — `-c`'s documented twin
    def test_config_env_sshcommand_attached_form_denied(self):
        self.assertEqual(self._deny("git --config-env=core.sshCommand=VAR fetch"), "deny")

    def test_config_env_sshcommand_split_form_denied(self):
        self.assertEqual(self._deny("git --config-env core.sshCommand=VAR fetch"), "deny")

    def test_config_env_sshcommand_case_insensitive_key(self):
        self.assertEqual(self._deny("git --config-env=CORE.SSHCOMMAND=VAR fetch"), "deny")

    def test_config_env_unrelated_key_is_fine(self):
        self.assertIsNone(self._deny("git --config-env=user.name=VAR fetch"))

    # --- FINDING 2, sibling D: `git config core.sshCommand …` — a PERSISTENT write, no
    # --- SSH-shaped token left on the command line for a plain `git fetch` afterwards
    def test_git_config_core_sshcommand_write_denied(self):
        self.assertEqual(self._deny("git config core.sshCommand 'ssh -i k'"), "deny")

    def test_git_config_core_sshcommand_write_denied_case_insensitive(self):
        self.assertEqual(self._deny("git config CORE.SSHCOMMAND 'ssh -i k'"), "deny")

    def test_git_config_core_sshcommand_read_stays_allowed(self):
        self.assertIsNone(self._deny("git config --get core.sshCommand"))

    def test_git_config_core_sshcommand_bare_read_stays_allowed(self):
        # `git config core.sshCommand` with no value is git's own default GET form.
        self.assertIsNone(self._deny("git config core.sshCommand"))

    def test_git_config_unrelated_key_write_stays_allowed(self):
        self.assertIsNone(self._deny("git config user.email foo@bar.com"))

    # --- FINDING 2, sibling E: GIT_CONFIG_COUNT/KEY_n/VALUE_n — entirely via env vars
    def test_git_config_count_key_value_env_mechanism_denied(self):
        self.assertEqual(self._deny(
            "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.sshCommand "
            "GIT_CONFIG_VALUE_0='ssh -i k' git fetch"), "deny")

    def test_git_config_key_env_case_insensitive(self):
        self.assertEqual(self._deny(
            "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=CORE.SSHCOMMAND "
            "GIT_CONFIG_VALUE_0='ssh -i k' git fetch"), "deny")

    def test_git_config_key_env_unrelated_key_is_fine(self):
        self.assertIsNone(self._deny(
            "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=user.name "
            "GIT_CONFIG_VALUE_0=bob git fetch"))

    # --- FINDING 3: a bad [[forge]] block next to a good one must not silently drop
    # --- the good one's coverage — the live consequence: an SSH clone to the still-good
    # --- declared host was becoming ALLOWED with no signal.
    def test_a_bad_forge_block_does_not_silently_drop_a_good_siblings_coverage(self):
        (config.ROOT / "charter.toml").write_text(
            '[[forge]]\nkind = "gitlab"\nhost = "git.internal"\ngroup = "acme"\n\n'
            '[[forge]]\nkind = "bitbucket-typo"\nhost = "bad.example.com"\nowner = "x"\n')
        self.assertEqual(self._deny("git clone git@git.internal:acme/api.git"), "deny")


class TestGuardsAreScopedToAPlane(PersonaIso):
    """Outside a control plane, charter has no opinion about your git.

    The plugin is installed per user or per project, but `pretooluse` ran everywhere. So
    installing charter to try it made `git clone git@github.com:…`, `git commit -S` and
    `ssh -T git@github.com` fail in every unrelated repo on the machine, with a message
    about a control plane that does not exist there — and README.md pre-empted the
    confusion with "that is the rule working, not a bug", which is true inside a plane and
    indefensible outside one.

    `PersonaIso`'s tmp dir has no `charter.toml`, so these run in the out-of-plane case by
    construction.
    """

    def _decide(self, cmd, cwd=None):
        r = run_hook(hooks.pretooluse,
                     {"tool_input": {"command": cmd}, "cwd": cwd or str(self.tmp)})
        return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")

    def test_ssh_git_is_not_denied_without_a_plane(self):
        self.assertIsNone(self._decide("git clone git@github.com:acme/app.git"))

    def test_signing_is_not_denied_without_a_plane(self):
        self.assertIsNone(self._decide("git commit -S -m 'signed'"))

    # The clone-commit nudge used to be asserted silent here too. Removed rather than kept
    # when #371 deleted the guard: it is now silent in EVERY plane, so the case could no
    # longer distinguish "scoped to a plane" from "gone", and a test that passes for a
    # reason other than the one it names is worse than no test.

    def test_the_same_command_is_denied_once_a_plane_exists(self):
        """The other half of the claim: this is scoping, not removal."""
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        self.assertEqual(self._decide("git clone git@github.com:acme/app.git"), "deny")

    def test_a_secret_leak_is_still_denied_without_a_plane(self):
        """Not gated, and must never be: keeping a credential out of the transcript is a
        safety invariant, not a policy a plane happens to hold."""
        self.assertEqual(self._decide("charter secret get devops API_TOKEN --reveal"), "deny")


if __name__ == "__main__":
    unittest.main()


class TestLeakGuardInspectsInvocationsNotProse(PersonaIso):
    """Both patterns were substring scans over the whole command line, so a command that
    merely MENTIONED the words was hard-denied with a reason misdescribing what it did.
    The sibling SSH guard already solved this and its docstring says why — "a commit
    message may legitimately *mention* an SSH URL"."""

    def test_a_commit_message_mentioning_the_flag_is_allowed(self):
        self.assertIsNone(hooks._leak_reason(
            'git commit -m "docs: document the --reveal flag"'))

    def test_searching_the_source_for_the_flag_is_allowed(self):
        self.assertIsNone(hooks._leak_reason("rg -n -- --reveal charter/"))

    def test_reading_the_registry_is_allowed(self):
        """`.charter/vaults.json` is the registry — provider config and paths, never
        values. Only `.charter/vaults/` holds secrets."""
        self.assertIsNone(hooks._leak_reason('grep -rn "vaults" .charter/vaults.json'))

    def test_the_real_thing_is_still_denied(self):
        for cmd in ("charter secret get devops API --reveal",
                    "python3 -m charter secret get d k --reveal",
                    "cat .charter/vaults/devops.json",
                    "edm persona secret get k --reveal"):
            with self.subTest(cmd=cmd):
                self.assertIsNotNone(hooks._leak_reason(cmd), cmd)

    def test_a_second_segment_is_inspected_too(self):
        self.assertIsNotNone(hooks._leak_reason("echo hi && charter secret get d k --reveal"))

    def test_the_flag_must_belong_to_charter(self):
        """A non-charter program's `--reveal` reveals nothing of ours."""
        self.assertIsNone(hooks._leak_reason("some-other-tool --reveal"))


class TestAbbreviationsCannotWalkPastTheGuard(unittest.TestCase):
    """argparse expands any unambiguous prefix, so `--rev` ran as `--reveal` while the
    guard — which looks for the flag a user would have to type — saw nothing to deny."""

    def test_reveal_cannot_be_abbreviated(self):
        from charter import cli
        p = cli.build_parser()
        with self.assertRaises(SystemExit):
            p.parse_args(["secret", "get", "v", "k", "--rev"])

    def test_the_full_flag_still_parses(self):
        from charter import cli
        ns = cli.build_parser().parse_args(["secret", "get", "v", "k", "--reveal"])
        self.assertTrue(ns.reveal)
