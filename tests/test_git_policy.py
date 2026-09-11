"""Golden rule — ONE CREDENTIAL: every git op uses the glab token over HTTPS; no SSH keys,
no commit signing. Two halves: gitpolicy makes it mechanically true (local git config +
SSH→HTTPS rewrites), and the PreToolUse guard denies a deliberate bypass."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config, gitpolicy, hooks

_ENV = {"GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null"}
# built from parts so the literal never appears in a command this repo's own guard scans
SSH_SCP = "git" + "@gitlab.com:"
SSH_URL = "ssh://" + "git" + "@gitlab.com/"


class GitPolicyCase(unittest.TestCase):
    def setUp(self):
        self.repo = Path(tempfile.mkdtemp(prefix="edm-gitpol-"))
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True,
                       capture_output=True, env={**os.environ, **_ENV})
        self.addCleanup(lambda: shutil.rmtree(self.repo, ignore_errors=True))

    def _cfg(self, *args):
        return subprocess.run(["git", "-C", str(self.repo), "config", *args],
                              capture_output=True, text=True).stdout.strip()

    def test_fresh_repo_is_non_compliant(self):
        self.assertTrue(gitpolicy.check(self.repo))

    def test_apply_makes_it_compliant_and_is_idempotent(self):
        changed = gitpolicy.apply(self.repo)
        self.assertTrue(changed)
        self.assertEqual(gitpolicy.check(self.repo), [])
        self.assertEqual(gitpolicy.apply(self.repo), [])   # idempotent: nothing left to change

    def test_applies_token_helper_and_disables_signing(self):
        gitpolicy.apply(self.repo)
        self.assertEqual(self._cfg("--local", "credential.helper"), "!glab auth git-credential")
        self.assertEqual(self._cfg("--local", "commit.gpgsign"), "false")
        self.assertEqual(self._cfg("--local", "tag.gpgsign"), "false")

    def test_ssh_urls_rewrite_to_https(self):
        gitpolicy.apply(self.repo)
        got = subprocess.run(["git", "-C", str(self.repo), "config", "--get-all",
                              f"url.{gitpolicy.HTTPS_BASE}.insteadOf"],
                             capture_output=True, text=True).stdout.split()
        self.assertIn(SSH_SCP, got)
        self.assertIn(SSH_URL, got)

    def test_rewrite_actually_resolves_ssh_remote_over_https(self):
        gitpolicy.apply(self.repo)
        subprocess.run(["git", "-C", str(self.repo), "config",
                        "remote.probe.url", SSH_SCP + "grp/repo.git"], capture_output=True)
        resolved = subprocess.run(["git", "-C", str(self.repo), "ls-remote", "--get-url", "probe"],
                                  capture_output=True, text=True).stdout.strip()
        self.assertTrue(resolved.startswith("https://gitlab.com/"), resolved)

    def test_never_writes_global_config(self):
        """SCOPE INVARIANT — the policy touches the umbrella and its clones, NOTHING outside.
        Point git's global config at a sentinel file and prove apply() leaves it untouched."""
        sentinel = Path(tempfile.mkdtemp(prefix="edm-globalcfg-")) / "gitconfig"
        sentinel.write_text("")
        self.addCleanup(lambda: shutil.rmtree(sentinel.parent, ignore_errors=True))
        old = os.environ.get("GIT_CONFIG_GLOBAL")
        os.environ["GIT_CONFIG_GLOBAL"] = str(sentinel)
        try:
            gitpolicy.apply(self.repo)
        finally:
            os.environ.pop("GIT_CONFIG_GLOBAL", None) if old is None else \
                os.environ.__setitem__("GIT_CONFIG_GLOBAL", old)
        self.assertEqual(sentinel.read_text(), "", "policy leaked into GLOBAL git config")
        # …while the repo's own LOCAL config did get it
        local = subprocess.run(["git", "-C", str(self.repo), "config", "--local", "--list"],
                               capture_output=True, text=True).stdout
        self.assertIn("credential.helper", local)

    def test_source_never_references_global_or_system_scope(self):
        """Belt-and-braces: no code path may widen the scope beyond --local. Inspect real
        string LITERALS via AST — grepping the text would also match the module's own
        docstring warning about --global (prose is not an argument)."""
        import ast
        tree = ast.parse(Path(gitpolicy.__file__).read_text())
        widened = [n.value for n in ast.walk(tree)
                   if isinstance(n, ast.Constant) and isinstance(n.value, str)
                   and n.value in ("--global", "--system")]
        self.assertEqual(widened, [], "gitpolicy must only ever write --local config")

    def test_scope_is_umbrella_and_its_clones_only(self):
        """repos() must enumerate the umbrella + workspace clones — never anything else."""
        root = Path(tempfile.mkdtemp(prefix="edm-scope-"))
        self.addCleanup(lambda: shutil.rmtree(root, ignore_errors=True))
        subprocess.run(["git", "init", "-q", str(root)], check=True, capture_output=True)
        ws = root / "workspaces" / "w"
        (ws / "repoA").mkdir(parents=True)
        subprocess.run(["git", "init", "-q", str(ws / "repoA")], check=True, capture_output=True)
        outside = root.parent / (root.name + "-outside")   # a sibling repo NOT under the umbrella
        outside.mkdir()
        subprocess.run(["git", "init", "-q", str(outside)], check=True, capture_output=True)
        self.addCleanup(lambda: shutil.rmtree(outside, ignore_errors=True))
        found = gitpolicy.repos(root, root / "workspaces")
        self.assertIn(root, found)
        self.assertIn(ws / "repoA", found)
        self.assertNotIn(outside, found)

    def test_non_compliant_lists_only_drifted_repos_in_scope(self):
        """Clones inside the umbrella are in scope — preflight uses this to catch a repo
        that was cloned manually (not via `edm clone`) and so never got the policy."""
        root = Path(tempfile.mkdtemp(prefix="edm-scan-"))
        self.addCleanup(lambda: shutil.rmtree(root, ignore_errors=True))
        subprocess.run(["git", "init", "-q", str(root)], check=True, capture_output=True)
        ws = root / "workspaces" / "w"
        for name in ("compliant", "manual"):
            (ws / name).mkdir(parents=True)
            subprocess.run(["git", "init", "-q", str(ws / name)], check=True, capture_output=True)
        gitpolicy.apply(root)
        gitpolicy.apply(ws / "compliant")
        bad = gitpolicy.non_compliant(root, root / "workspaces")
        self.assertIn(ws / "manual", bad)          # the hand-cloned repo is flagged
        self.assertNotIn(ws / "compliant", bad)
        self.assertNotIn(root, bad)

    def test_local_config_reader_returns_multivalued_keys(self):
        gitpolicy.apply(self.repo)
        cfg = gitpolicy._local_config(self.repo)
        self.assertEqual(cfg["credential.helper"], ["!glab auth git-credential"])
        self.assertEqual(len(cfg[gitpolicy._URL_KEY.lower()]), 2)   # both SSH forms

    def test_non_git_dir_is_a_noop(self):
        plain = Path(tempfile.mkdtemp(prefix="edm-plain-"))
        self.addCleanup(lambda: shutil.rmtree(plain, ignore_errors=True))
        self.assertEqual(gitpolicy.apply(plain), [])


class SingleCredentialGuardCase(unittest.TestCase):
    """The PreToolUse deny — a golden rule every persona (and sub-agent) inherits."""

    def _deny(self, cmd):
        return hooks._single_credential_reason(cmd)

    def test_denies_ssh_gitlab_url_to_git(self):
        self.assertIsNotNone(self._deny(f"git clone {SSH_SCP}acme/x.git"))
        self.assertIsNotNone(self._deny(f"git remote set-url origin {SSH_URL}acme/x.git"))

    def test_denies_git_ssh_command_override(self):
        self.assertIsNotNone(self._deny("GIT_SSH_COMMAND='ssh -i ~/.ssh/id' git push origin main"))

    def test_denies_commit_signing(self):
        self.assertIsNotNone(self._deny("git commit -S -m 'x'"))
        self.assertIsNotNone(self._deny("git commit --gpg-sign -m 'x'"))
        self.assertIsNotNone(self._deny("git -c commit.gpgsign=true commit -m 'x'"))

    def test_denies_ssh_probe_to_gitlab(self):
        self.assertIsNotNone(self._deny("ssh -T " + "git" + "@gitlab.com"))

    # --- FINDING 2, shape A: `-c core.sshCommand=…` is GIT_SSH_COMMAND's exact config twin
    def test_denies_core_sshcommand_config_before_subcommand(self):
        self.assertIsNotNone(self._deny("git -c core.sshCommand='ssh -i ~/.ssh/id' fetch"))

    def test_denies_core_sshcommand_config_after_subcommand(self):
        self.assertIsNotNone(self._deny("git fetch -c core.sshCommand='ssh -i ~/.ssh/id'"))

    def test_denies_core_sshcommand_case_insensitive_key(self):
        # git config keys are case-insensitive — `CORE.SSHCOMMAND` is the same key.
        self.assertIsNotNone(self._deny("git -c CORE.SSHCOMMAND=ssh push origin main"))

    # --- FINDING 2, shape B: git treats hostnames case-insensitively — so must the guard
    def test_denies_uppercase_host_in_ssh_url(self):
        self.assertIsNotNone(self._deny("git clone git@GITHUB.COM:acme/api.git"))
        self.assertIsNotNone(self._deny("git clone git@GitLab.Com:acme/x.git"))

    def test_denies_uppercase_host_ssh_probe(self):
        self.assertIsNotNone(self._deny("ssh -T git@GITHUB.COM"))

    # --- FINDING 2, sibling C: `--config-env` is `-c`'s documented twin (value read
    # --- from an env var, so it never even appears on the command line)
    def test_denies_config_env_sshcommand_attached(self):
        self.assertIsNotNone(self._deny("git --config-env=core.sshCommand=VAR fetch"))

    def test_denies_config_env_sshcommand_split(self):
        self.assertIsNotNone(self._deny("git --config-env core.sshCommand=VAR fetch"))

    def test_allows_config_env_for_an_unrelated_key(self):
        self.assertIsNone(self._deny("git --config-env=user.name=VAR fetch"))

    # --- FINDING 2, sibling D: `git config core.sshCommand …` is a PERSISTENT write —
    # --- after it runs, a plain `git fetch` goes over SSH with nothing left to see
    def test_denies_git_config_core_sshcommand_persistent_write(self):
        self.assertIsNotNone(self._deny("git config core.sshCommand 'ssh -i ~/.ssh/id'"))

    def test_denies_git_config_local_core_sshcommand_write(self):
        self.assertIsNotNone(
            self._deny("git config --local core.sshCommand 'ssh -i ~/.ssh/id'"))

    def test_allows_git_config_get_core_sshcommand_read(self):
        self.assertIsNone(self._deny("git config --get core.sshCommand"))

    def test_allows_bare_git_config_core_sshcommand_read(self):
        # no value follows → git's own default GET behaviour, not a write
        self.assertIsNone(self._deny("git config core.sshCommand"))

    def test_allows_unrelated_git_config_writes(self):
        self.assertIsNone(self._deny("git config user.email foo@bar.com"))
        self.assertIsNone(self._deny("git config --add remote.origin.fetch '+refs/*:refs/*'"))

    def test_config_write_denial_does_not_trip_on_a_commit_message(self):
        # a commit message merely MENTIONING core.sshCommand must not be misread as a
        # `git config` invocation (matches the existing SSH-URL-in-a-message regression)
        self.assertIsNone(self._deny(
            "git commit -m 'docs: never set core.sshCommand, use HTTPS instead'"))

    # --- FINDING 2, sibling E: GIT_CONFIG_COUNT/KEY_n/VALUE_n — the override lives
    # --- entirely in the environment, no flag on the command line at all
    def test_denies_git_config_key_value_env_mechanism(self):
        self.assertIsNotNone(self._deny(
            "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.sshCommand "
            "GIT_CONFIG_VALUE_0='ssh -i ~/.ssh/id' git fetch"))

    def test_denies_git_config_key_env_case_insensitive(self):
        self.assertIsNotNone(self._deny(
            "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=CORE.SSHCOMMAND "
            "GIT_CONFIG_VALUE_0='ssh -i ~/.ssh/id' git fetch"))

    def test_allows_git_config_key_env_for_an_unrelated_key(self):
        self.assertIsNone(self._deny(
            "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=user.name GIT_CONFIG_VALUE_0=bob git fetch"))

    def test_allows_normal_git_over_https(self):
        for ok in ("git push origin main", "git pull --ff-only", "git status",
                   "git clone https://gitlab.com/acme/x.git", "git commit -m 'x'"):
            self.assertIsNone(self._deny(ok), ok)

    def test_allows_pickaxe_search_not_mistaken_for_signing(self):
        # `git log -S<string>` is a content search — must NOT be read as commit signing
        self.assertIsNone(self._deny("git log -S'needle' --oneline"))
        self.assertIsNone(self._deny("git log -S needle"))

    # --- FINDING I4: the old check was a positional MEMBERSHIP test
    # --- (`any(v in args for v in _SIGN_VERBS) and "-S" in args`) — it read the verb from
    # --- ANYWHERE in argv, not the actual subcommand, producing both a false negative
    # --- (`git tag -s`/`--sign` walked straight through, since `-s`/`--sign` were never
    # --- checked at all) and a false positive (`git log -S commit` denied, because the
    # --- word "commit" — the pickaxe's own search string — happens to appear in argv and
    # --- match one of `_SIGN_VERBS` by pure membership, with no regard for what it means).
    def test_denies_tag_dash_s_signing(self):
        # false negative: `git tag -s` (GPG-sign the tag) was never checked at all.
        self.assertIsNotNone(self._deny("git tag -s -m 'v1' v1.0.0"))

    def test_denies_tag_dash_dash_sign(self):
        self.assertIsNotNone(self._deny("git tag --sign -m 'v1' v1.0.0"))

    def test_log_pickaxe_with_the_word_commit_is_not_mistaken_for_signing(self):
        # false positive: "commit" appearing as the pickaxe's OWN search string used to
        # satisfy `any(v in args for v in _SIGN_VERBS)` by membership, denying a read-only
        # `git log`. The real subcommand is `log`, never a committing verb.
        self.assertIsNone(self._deny("git log -S commit --oneline"))
        self.assertIsNone(self._deny("git log -Scommit"))

    def test_commit_dash_s_signoff_is_unrelated_and_stays_allowed(self):
        # `git commit -s`/`--signoff` adds a Signed-off-by trailer — NOT GPG signing
        # (that's `-S`/`--gpg-sign`). Golden rule 0 is about the signing KEY hanging an
        # agent, not the signoff trailer, so this must stay allowed even on `commit`.
        self.assertIsNone(self._deny("git commit -s -m 'x'"))
        self.assertIsNone(self._deny("git commit --signoff -m 'x'"))

    def test_plain_annotated_tag_without_any_signing_flag_stays_allowed(self):
        self.assertIsNone(self._deny("git tag -a v1.0.0 -m 'annotated, not signed'"))

    def test_tag_dash_capital_s_still_denied_via_subcommand_not_membership(self):
        # regression guard: the subcommand-based rewrite must still catch `-S` on `tag`
        # (it was already in _SIGN_VERBS before this fix — must not be lost in the rewrite).
        self.assertIsNotNone(self._deny("git tag -S -m 'v1' v1.0.0"))

    def test_allows_ssh_to_other_hosts(self):
        # devops SSH to a server is unrelated to the git credential rule
        self.assertIsNone(self._deny("ssh deploy@prod-box-01 'uptime'"))
        self.assertIsNone(self._deny("ssh -i ~/.ssh/key ubuntu@10.0.0.5"))

    def test_reason_names_the_remedy(self):
        reason = self._deny("git clone " + SSH_SCP + "e/x.git")
        self.assertIn("token", reason.lower())

    # --- regression: prose must not trip the guard (it denied a real commit whose MESSAGE
    # --- merely mentioned the SSH form). Only actual git/ssh invocations are inspected.
    def test_allows_ssh_url_mentioned_inside_a_commit_message(self):
        self.assertIsNone(self._deny(
            f"git commit -m 'docs: {SSH_SCP}x now rewrites to https://gitlab.com/x'"))

    def test_allows_ssh_url_inside_a_non_git_program(self):
        self.assertIsNone(self._deny(
            f"python3 -c \"print('verified: {SSH_SCP}repo resolves over https')\""))

    def test_allows_writing_docs_that_mention_ssh(self):
        self.assertIsNone(self._deny(f"echo 'never use {SSH_SCP}repo' >> docs/access.md"))

    def test_still_denies_the_real_thing_in_a_chained_segment(self):
        self.assertIsNotNone(self._deny(f"cd /tmp && git remote add o {SSH_SCP}e/x.git"))

    def test_allows_git_log_dash_S_pickaxe_in_segments(self):
        self.assertIsNone(self._deny("cd repo && git log -S'token' --oneline"))


class TestPerForgePolicy(unittest.TestCase):
    """Golden rule 0 restated: ONE CREDENTIAL PER FORGE. The invariant was never the
    `glab` binary — it is *the forge's CLI holds the token, git speaks HTTPS, never SSH,
    never signing*. `gh auth git-credential` satisfies it identically."""

    def test_each_forge_contributes_its_own_helper(self):
        from charter import gitpolicy
        from charter.forge.github import GitHubForge
        from charter.forge.gitlab import GitLabForge
        self.assertIn("glab", gitpolicy.policy_for(GitLabForge())["credential.helper"])
        self.assertIn("gh", gitpolicy.policy_for(GitHubForge())["credential.helper"])

    def test_signing_stays_off_for_every_forge(self):
        from charter import gitpolicy
        from charter.forge.github import GitHubForge
        from charter.forge.gitlab import GitLabForge
        for f in (GitLabForge(), GitHubForge()):
            p = gitpolicy.policy_for(f)
            self.assertEqual(p["commit.gpgsign"], "false")
            self.assertEqual(p["tag.gpgsign"], "false")

    def test_no_policy_value_mentions_ssh(self):
        from charter import gitpolicy
        from charter.forge.github import GitHubForge
        from charter.forge.gitlab import GitLabForge
        for f in (GitLabForge(), GitHubForge()):
            for v in gitpolicy.policy_for(f).values():
                self.assertNotIn("ssh", v.lower())


class DeclaredSelfHostedForgeCase(unittest.TestCase):
    """FINDING 1 — a repo whose origin lives on a DECLARED self-hosted forge (GitLab
    Enterprise / GHE, named in this control plane's own charter.toml) must get THAT
    forge's own policy, not a silent GitLab-default false green.

    Verified live by the reviewer, before this fix: `forge_for` only recognised
    gitlab.com/github.com (the registry's class DEFAULTS), so a self-hosted host fell
    through to the GitLab fallback — wrong CLI's helper entirely on a GHE host, and on
    self-hosted GitLab the credential helper LOOKED right (`glab`) while the insteadOf
    rewrite still targeted gitlab.com, so the real `git@git.internal:` SSH remote was
    NEVER rewritten — `check()` still reported `[]` (token-only) while a plain `git push`
    genuinely transported over SSH, the exact hang golden rule 0 exists to prevent."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="edm-selfhost-root-"))
        self.addCleanup(lambda: shutil.rmtree(self.root, ignore_errors=True))
        self.repo = Path(tempfile.mkdtemp(prefix="edm-selfhost-repo-"))
        self.addCleanup(lambda: shutil.rmtree(self.repo, ignore_errors=True))
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True,
                       capture_output=True, env={**os.environ, **_ENV})

    def _declare(self, body: str) -> None:
        (self.root / "charter.toml").write_text(body)

    def _origin(self, url: str) -> None:
        subprocess.run(["git", "-C", str(self.repo), "remote", "add", "origin", url],
                       check=True, capture_output=True)

    def test_self_hosted_gitlab_gets_its_own_helper_and_insteadof(self):
        self._declare('[[forge]]\nkind = "gitlab"\nhost = "git.internal"\ngroup = "acme"\n')
        self._origin("https://git.internal/acme/api.git")
        with mock.patch.object(config, "ROOT", self.root):
            forge = gitpolicy.forge_for(self.repo)
            self.assertIsNotNone(forge, "a declared self-hosted host must resolve")
            self.assertEqual(forge.host, "git.internal")
            self.assertEqual(forge.credential_helper(), "!glab auth git-credential")
            https_base, ssh_forms = gitpolicy.insteadof_for(forge)
            self.assertEqual(https_base, "https://git.internal/")
            self.assertIn("git@git.internal:", ssh_forms)

    def test_self_hosted_github_enterprise_gets_gh_not_glab(self):
        self._declare('[[forge]]\nkind = "github"\nhost = "ghe.acme.com"\nowner = "acme"\n')
        self._origin("https://ghe.acme.com/acme/api.git")
        with mock.patch.object(config, "ROOT", self.root):
            forge = gitpolicy.forge_for(self.repo)
            self.assertIsNotNone(forge)
            # The live bug: a GHE host fell back to GitLab's policy — the WRONG CLI
            # entirely, not just the wrong host rewrite.
            self.assertEqual(forge.credential_helper(), "!gh auth git-credential")

    def test_apply_writes_the_declared_hosts_insteadof_not_gitlab_coms(self):
        self._declare('[[forge]]\nkind = "gitlab"\nhost = "git.internal"\ngroup = "acme"\n')
        self._origin("git" + "@git.internal:acme/api.git")  # the live bug: an SSH remote
        with mock.patch.object(config, "ROOT", self.root):
            changed = gitpolicy.apply(self.repo)
            self.assertTrue(changed)
            self.assertEqual(gitpolicy.check(self.repo), [])   # honestly compliant now
        got = subprocess.run(["git", "-C", str(self.repo), "config", "--local", "--get-all",
                              "url.https://git.internal/.insteadOf"],
                             capture_output=True, text=True).stdout.split()
        self.assertIn("git@git.internal:", got)
        # …and gitlab.com's rewrite was NOT written (that would still leave the real SSH
        # remote unrewritten — the exact false-green shape the reviewer found live).
        gitlab_com = subprocess.run(["git", "-C", str(self.repo), "config", "--local",
                                     "--get-all", "url.https://gitlab.com/.insteadOf"],
                                    capture_output=True, text=True)
        self.assertNotIn("git.internal", gitlab_com.stdout)

    def test_origin_https_resolves_the_self_hosted_ssh_remote(self):
        self._declare('[[forge]]\nkind = "gitlab"\nhost = "git.internal"\ngroup = "acme"\n')
        self._origin("git" + "@git.internal:acme/api.git")
        with mock.patch.object(config, "ROOT", self.root):
            from charter.commands import _origin_https
            self.assertEqual(_origin_https(self.repo), "https://git.internal/acme/api.git")


class FalseGreenViaHostMisrecognitionCase(unittest.TestCase):
    """FINDING 1 — the exact live reproduction the adversarial re-review reported: a
    clone whose origin embeds a KNOWN host as a substring of its PATH (not its actual
    host) must resolve to its OWN real host's forge — never silently fall through to
    the forge whose name merely appears in the path. Before the fix, `charter
    git-policy` printed a false '✓ token-only' for this exact repo while its origin was
    never actually rewritten off SSH."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="edm-falsegreen-root-"))
        self.addCleanup(lambda: shutil.rmtree(self.root, ignore_errors=True))
        (self.root / "charter.toml").write_text(
            '[[forge]]\nkind = "gitlab"\nhost = "git.internal"\ngroup = "acme"\n')
        self.repo = Path(tempfile.mkdtemp(prefix="edm-falsegreen-repo-"))
        self.addCleanup(lambda: shutil.rmtree(self.repo, ignore_errors=True))
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True,
                       capture_output=True, env={**os.environ, **_ENV})
        # the live bug: 'gitlab.com' is a substring of the PATH, not the host
        origin = "git" + "@git.internal:gitlab.com-mirror/api.git"
        subprocess.run(["git", "-C", str(self.repo), "remote", "add", "origin", origin],
                       check=True, capture_output=True)

    def test_forge_for_resolves_the_real_host_not_the_path_substring(self):
        with mock.patch.object(config, "ROOT", self.root):
            forge = gitpolicy.forge_for(self.repo)
        self.assertIsNotNone(forge)
        self.assertEqual(forge.host, "git.internal")
        self.assertEqual(forge.credential_helper(), "!glab auth git-credential")

    def test_apply_then_check_is_honestly_compliant_and_actually_rewrites(self):
        with mock.patch.object(config, "ROOT", self.root):
            changed = gitpolicy.apply(self.repo)
            self.assertTrue(changed)
            self.assertEqual(gitpolicy.check(self.repo), [])   # honestly compliant
        resolved = subprocess.run(
            ["git", "-C", str(self.repo), "ls-remote", "--get-url", "origin"],
            capture_output=True, text=True).stdout.strip()
        self.assertTrue(resolved.startswith("https://git.internal/"), resolved)
        # …and gitlab.com's own insteadOf was NOT written for this repo — that would
        # leave the real git@git.internal: prefix unrewritten (the exact live bug: SSH
        # transport, while `check()` still reported "token-only").
        gitlab_com = subprocess.run(
            ["git", "-C", str(self.repo), "config", "--local", "--get-all",
             "url.https://gitlab.com/.insteadOf"], capture_output=True, text=True)
        self.assertNotIn("git.internal", gitlab_com.stdout)


class UnrecognizedForgeIsUnmanagedCase(unittest.TestCase):
    """CRITICAL CONSTRAINT — an origin host that is neither a default forge nor declared
    in charter.toml must NEVER read as compliant. Before this fix, `forge_for` fell back
    to GitLab's policy for ANY unrecognised host, so `check()` returned `[]` (green) for
    a repo whose policy was never actually verified — worse than no check at all, because
    it still LOOKS like golden rule 0 is enforced. The fix must not simply widen the
    fallback: an unrecognised host has to read as honestly unmanaged."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="edm-unknown-root-"))
        self.addCleanup(lambda: shutil.rmtree(self.root, ignore_errors=True))
        self.repo = Path(tempfile.mkdtemp(prefix="edm-unknown-repo-"))
        self.addCleanup(lambda: shutil.rmtree(self.repo, ignore_errors=True))
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True,
                       capture_output=True, env={**os.environ, **_ENV})
        subprocess.run(["git", "-C", str(self.repo), "remote", "add", "origin",
                        "https://bitbucket.example.org/acme/api.git"],
                       check=True, capture_output=True)

    def test_forge_for_is_none_not_a_silent_gitlab_fallback(self):
        with mock.patch.object(config, "ROOT", self.root):
            self.assertIsNone(gitpolicy.forge_for(self.repo))

    def test_check_reports_unmanaged_never_a_false_green(self):
        with mock.patch.object(config, "ROOT", self.root):
            drift = gitpolicy.check(self.repo)
        self.assertNotEqual(drift, [], "an unrecognised host must never report compliant")
        self.assertEqual(drift, [gitpolicy.UNMANAGED_FORGE])

    def test_apply_never_guesses_a_policy_for_it(self):
        with mock.patch.object(config, "ROOT", self.root):
            changed = gitpolicy.apply(self.repo)
        self.assertEqual(changed, [])
        local = subprocess.run(["git", "-C", str(self.repo), "config", "--local", "--list"],
                               capture_output=True, text=True).stdout
        self.assertNotIn("credential.helper", local)

    def test_origin_https_is_none_for_an_unrecognised_host(self):
        with mock.patch.object(config, "ROOT", self.root):
            from charter.commands import _origin_https
            self.assertIsNone(_origin_https(self.repo))

    def test_a_repo_with_no_origin_at_all_still_defaults_to_gitlab(self):
        """Back-compat: this is a DIFFERENT case from an unrecognised host — a fresh
        `git init` has no host to be wrong about, so it keeps the pre-multi-forge
        default, unchanged."""
        fresh = Path(tempfile.mkdtemp(prefix="edm-fresh-"))
        self.addCleanup(lambda: shutil.rmtree(fresh, ignore_errors=True))
        subprocess.run(["git", "init", "-q", str(fresh)], check=True, capture_output=True,
                       env={**os.environ, **_ENV})
        with mock.patch.object(config, "ROOT", self.root):
            forge = gitpolicy.forge_for(fresh)
        self.assertIsNotNone(forge)
        self.assertEqual(forge.kind, "gitlab")


class TheScopeScanIsExact(unittest.TestCase):
    """What the deletion sweep asks of `gitpolicy.scan`, which `repos` and doctor's `git auth` row
    both read (#942 final review)."""

    def plane(self) -> Path:
        root = Path(tempfile.mkdtemp(prefix="edm-scan5-"))
        self.addCleanup(lambda: shutil.rmtree(root, ignore_errors=True))
        return root

    def git_init(self, p: Path) -> Path:
        p.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "init", "-q", str(p)], check=True, capture_output=True,
                       env={**os.environ, **_ENV})
        return p

    def test_a_root_that_is_no_repository_is_not_in_scope(self):
        root = self.plane()
        clone = self.git_init(root / "workspaces" / "w" / "repoA")
        self.assertEqual(gitpolicy.repos(root, root / "workspaces"), [clone])

    def test_a_directory_in_a_workspace_that_is_no_repository_is_not_in_scope(self):
        root = self.git_init(self.plane())
        (root / "workspaces" / "w" / "memory").mkdir(parents=True)
        self.assertEqual(gitpolicy.repos(root, root / "workspaces"), [root])

    def test_a_file_among_the_workspaces_is_passed_over(self):
        """Finder leaves a `.DS_Store` in any directory it has shown."""
        root = self.git_init(self.plane())
        (root / "workspaces").mkdir()
        (root / "workspaces" / ".DS_Store").write_bytes(b"\0")
        self.assertEqual(gitpolicy.repos(root, root / "workspaces"), [root])

    def test_repositories_come_back_in_path_order_whatever_order_the_directory_lists(self):
        root = self.git_init(self.plane())
        zeta = self.git_init(root / "workspaces" / "alpha" / "zeta")
        eta = self.git_init(root / "workspaces" / "alpha" / "eta")
        theta = self.git_init(root / "workspaces" / "beta" / "theta")
        real = Path.iterdir
        with mock.patch.object(Path, "iterdir",
                               lambda self: iter(sorted(real(self), reverse=True))):
            found = gitpolicy.repos(root, root / "workspaces")
        self.assertEqual(found, [root, eta, zeta, theta])


if __name__ == "__main__":
    unittest.main()
