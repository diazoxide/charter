"""Registering a vault over a name that already exists (issue #22).

`charter vault add <name>` used to overwrite the registration in place — different
provider, no prompt, exit 0. The registration is the ONLY pointer to a plain-file vault's
secrets, so replacing it migrates nothing: it strands the file on disk with nothing
referring to it, and `charter secret get` then reports the key as *missing* rather than as
unreachable. Observed during a real migration, where three vaults were re-registered onto
1Password and every one was accepted silently.

It also broke the rule the rest of charter states and follows — additive: never delete or
rename a user's thing to make room; name the blocker and refuse. `init`, `reinit` and
`_create_baseline_dirs` all work that way.

**Nothing here may reach a real credential store, and until #546 two cases did.**
`cmd_vault_add` finishes by calling `prov.health()`, and for a `1password` vault that
shells out — so `test_force_does_not_migrate_and_says_so` and
`test_the_account_pin_never_travels` ran the operator's OWN `op` binary against the vault
names their own fixtures spell (`Eng`, `Engineering`). Measured by putting a logging
stand-in for `op` first on `$PATH` and running the whole suite: four invocations, from
those two tests, and no others anywhere in 6636. They passed because an unauthenticated
`op` exits non-zero in well under a second and the provider then reports a vault it cannot
read — the right answer, by luck. `op` **blocks** instead when it has no usable session and
a human could be asked, and the suite then parks in `subprocess.communicate` behind a
biometric prompt: measured at 15 minutes inside a fresh tmux pane, with
`python3 -m unittest discover -s tests` never finishing.

So `_no_real_op` is applied to every case in this file rather than to the two that were
caught, on the same reasoning `_launch`'s tty pins carry: the next case here will be
written by copying its neighbour. It is the pattern the five sibling `op` modules already
use — a fake on `OnePasswordProvider.runner` plus a stubbed `shutil.which` — spelled the
way `test_op_schema_is_told_the_same_everywhere.py` spells it, because that module drives
`cmd_vault_add` too and therefore has to reach the provider the same indirect way.

The suite-wide half is `tests._planeguard.RealVaultReach`, which refuses the spawn itself,
so instance three fails on the pull request that adds it rather than on the machine most
likely to have `op` installed — the operator's.
"""
from __future__ import annotations

import io
import json
import os
import unittest
from contextlib import redirect_stderr
from types import SimpleNamespace
from unittest import mock

from charter import commands_secrets, config
from charter.secrets import base, registry
from charter.secrets.onepassword import OnePasswordProvider
from tests._isolation import PersonaIso

#: `git init` and `git check-ignore` are real subprocesses here (see
#: `PlaintextMustNotLandInGit`), and both read the machine's git configuration:
#: `check-ignore` honours `core.excludesFile`, so a developer whose global ignore names
#: `*.json` or `cfg/` would watch `test_a_tracked_path_is_refused` fail for a reason that
#: has nothing to do with charter, and `init` honours `init.templateDir`, which can run
#: hooks. The same two lines a dozen other modules here already carry (`test_git_policy`,
#: `test_freshness`, `test_reactive_memory`, …).
_HERMETIC_GIT = {"GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_SYSTEM": os.devnull}


class _FakeOp:
    """Stands in for the `op` CLI, for a vault that has just been registered.

    Deliberately the smallest fake that answers `health()` — the only path `cmd_vault_add`
    reaches — rather than a sixth full model of `op`. `health()` asks `keys()`, which is
    one `item get`; a failed `item get` is followed by an `item list` to decide whether the
    item is absent or merely unreadable (#322), so both are answered. Every value below is
    an inert fixture string; there is no 1Password vault on this machine to record one
    from, and nothing here asserts on one.
    """

    def __init__(self, title: str = "charter-devops") -> None:
        self.title = title
        self.calls: list[list[str]] = []

    def __call__(self, argv, input=None, **kw):
        self.calls.append(list(argv))
        bare = [a for a in argv if not a.startswith("--")]
        if bare[:3] == ["op", "item", "get"]:
            return SimpleNamespace(returncode=0, stderr="", stdout=json.dumps({
                "id": "itm1", "title": self.title, "category": "PASSWORD", "fields": []}))
        if bare[:3] == ["op", "item", "list"]:
            return SimpleNamespace(returncode=0, stderr="",
                                   stdout=json.dumps([{"title": self.title}]))
        return SimpleNamespace(returncode=0, stdout="", stderr="")


def _no_real_op(case) -> _FakeOp:
    """Point this case's 1Password provider at a fake, and say `op` is installed.

    Both halves are needed and they close different holes. The runner is what a real
    `op` would be spawned through, and it is set on the CLASS because the provider these
    tests exercise is built inside `cmd_vault_add` by `registry.provider_for` — there is no
    instance for a test to reach. `shutil.which` is what the provider checks BEFORE running
    anything, so without stubbing it the answer would still be the machine's: "op CLI not
    on PATH" on a bare runner and a real spawn on a laptop that has 1Password installed.
    """
    real_runner = OnePasswordProvider.__dict__["runner"]
    op = _FakeOp()
    OnePasswordProvider.runner = op
    case.addCleanup(lambda: setattr(OnePasswordProvider, "runner", real_runner))
    import charter.secrets.onepassword as mod
    real_which = mod.shutil.which
    mod.shutil.which = lambda n: "/usr/local/bin/op" if n == "op" else None
    case.addCleanup(lambda: setattr(mod.shutil, "which", real_which))
    return op


def _args(name: str, provider: str = "plain-file", **kw) -> SimpleNamespace:
    return SimpleNamespace(name=name, provider=provider, file=kw.pop("file", None),
                           op_vault=kw.pop("op_vault", None), account=None,
                           persona=kw.pop("persona", None), force=kw.pop("force", False))


class VaultRegistrationCase(PersonaIso):
    """`PersonaIso`, plus the two things this file must not read off the machine."""

    def setUp(self) -> None:
        super().setUp()
        self.op = _no_real_op(self)
        self.enterContext(mock.patch.dict(os.environ, _HERMETIC_GIT))


class RegisteringOverAnExistingName(VaultRegistrationCase):
    def setUp(self) -> None:
        super().setUp()
        registry.add_vault("devops", "plain-file", {"file": str(self.tmp / "devops.json")})

    def test_a_collision_is_refused(self):
        with self.assertRaises(base.VaultError):
            registry.add_vault("devops", "1password", {"op-vault": "Eng"})

    def test_the_original_registration_survives_the_refusal(self):
        """The point of refusing: what was there must still be there afterwards."""
        with self.assertRaises(base.VaultError):
            registry.add_vault("devops", "1password", {"op-vault": "Eng"})
        got = registry.vaults()["devops"]
        self.assertEqual(got["provider"], "plain-file")
        self.assertEqual(got["config"]["file"], str(self.tmp / "devops.json"))

    def test_the_refusal_names_what_is_in_the_way_and_how_to_proceed(self):
        """A blocker the user cannot act on is only half a refusal."""
        with self.assertRaises(base.VaultError) as e:
            registry.add_vault("devops", "1password", {"op-vault": "Eng"})
        msg = str(e.exception)
        self.assertIn("plain-file", msg)              # what is registered
        self.assertIn("devops.json", msg)             # and where its secrets are
        self.assertIn("--force", msg)                 # how to override

    def test_force_replaces(self):
        registry.add_vault("devops", "1password", {"op-vault": "Eng"}, force=True)
        self.assertEqual(registry.vaults()["devops"]["provider"], "1password")

    def test_force_does_not_migrate_and_says_so(self):
        """`--force` is an override, not a migration. Moving secrets between providers is
        its own operation and must be typed on purpose — burying it inside `add` is how
        the original bug orphaned things."""
        err = io.StringIO()
        with redirect_stderr(err):
            commands_secrets.cmd_vault_add(
                _args("devops", "1password", op_vault="Eng", force=True))
        out = err.getvalue()
        self.assertIn("NOT migrated", out)
        self.assertIn(str(self.tmp / "devops.json"), out)   # where they still are

    def test_a_different_name_is_unaffected(self):
        registry.add_vault("qa", "plain-file", {"file": str(self.tmp / "qa.json")})
        self.assertEqual(sorted(registry.vaults()), ["devops", "qa"])

    def test_the_command_exits_non_zero_on_a_refusal(self):
        """Scripted callers must be able to tell from the exit code alone that the vault
        they asked for is not the vault that is registered."""
        with redirect_stderr(io.StringIO()):
            rc = commands_secrets.cmd_vault_add(_args("devops", "1password", op_vault="Eng"))
        self.assertEqual(rc, 1)

    def test_a_first_registration_still_just_works(self):
        with redirect_stderr(io.StringIO()):
            rc = commands_secrets.cmd_vault_add(_args("fresh"))
        self.assertEqual(rc, 0)
        self.assertIn("fresh", registry.vaults())


class TheRegistryIsPortable(VaultRegistrationCase):
    """Issue #21. The registry recorded one developer's home directory, so a team that
    commits its reference vaults — they hold `op://` URIs, never values — found the vault
    files present on a fresh clone and the index that locates them useless, and scripted
    `charter vault add` calls to rebuild state already in git."""

    def _add(self, name: str, provider: str = "plain-file", file=None):
        with redirect_stderr(io.StringIO()):
            return commands_secrets.cmd_vault_add(
                _args(name, provider, file=str(file) if file else None))

    def test_a_managed_path_is_stored_relative_to_the_plane(self):
        self._add("devops")
        self.assertEqual(registry.vaults()["devops"]["config"]["file"],
                         ".charter/vaults/devops.json")

    def test_a_path_given_inside_the_plane_is_relativised_too(self):
        self._add("team", file=self.tmp / "secrets" / "team.json")
        self.assertEqual(registry.vaults()["team"]["config"]["file"],
                         "secrets/team.json")

    def test_a_path_outside_the_plane_stays_absolute(self):
        """A vault deliberately kept outside has no portable form, and rewriting it would
        silently re-point it at somewhere inside."""
        outside = self.tmp.parent / "elsewhere.json"
        self._add("ext", file=outside)
        self.assertEqual(registry.vaults()["ext"]["config"]["file"], str(outside))

    def test_a_relative_entry_resolves_against_the_plane_root(self):
        """The half that makes the stored form usable — and the half that makes the same
        registry work on a machine whose checkout lives somewhere else."""
        registry.add_vault("devops", "plain-file", {"file": ".charter/vaults/devops.json"})
        prov = registry.provider_for("devops")
        self.assertEqual(prov.path, self.tmp / ".charter" / "vaults" / "devops.json")

    def test_an_absolute_entry_still_resolves_unchanged(self):
        """Registries written before this keep working — no migration, no rewrite."""
        abs_path = self.tmp / "legacy.json"
        registry.add_vault("legacy", "plain-file", {"file": str(abs_path)})
        self.assertEqual(registry.provider_for("legacy").path, abs_path)

    def test_status_does_not_leak_the_local_layout(self):
        """#21's aside: `vault list` printed the absolute path in its STATUS column, which
        is noise and puts one developer's directory layout into output others may read."""
        self._add("devops")
        ok, detail = registry.provider_for("devops").health()
        self.assertTrue(ok)
        self.assertIn("not created yet", detail)
        self.assertNotIn(str(self.tmp), detail)

    def test_both_providers_resolve_the_same_way(self):
        """`plain-file` and `reference` had byte-identical `path` properties; they now
        share one, so neither can quietly keep resolving the old way."""
        registry.add_vault("a", "plain-file", {"file": "x/a.json"})
        registry.add_vault("b", "reference", {"file": "x/b.json"})
        self.assertEqual(registry.provider_for("a").path, self.tmp / "x" / "a.json")
        self.assertEqual(registry.provider_for("b").path, self.tmp / "x" / "b.json")


class SharedAndLocalHalves(VaultRegistrationCase):
    """#21's remaining half: the registry that *locates* committed vaults must travel too.

    `vaults.json` at the plane root is committed; `.charter/vaults.json` stays local and
    wins on conflict. The split is per FIELD and it is small — going through them, only
    `account` (which 1Password account this developer signed into) genuinely differs
    between machines.
    """

    def _add(self, name, provider="plain-file", share=False, **kw):
        with redirect_stderr(io.StringIO()):
            a = _args(name, provider, **kw)
            a.share = share
            return commands_secrets.cmd_vault_add(a)

    def test_registering_is_local_by_default(self):
        """Matching `[memory].share`, which defaults to local so a plane never publishes
        by accident. A registry names which personas hold credentials and where their
        files live — a map worth having even without the values."""
        self._add("mine")
        self.assertIn("mine", registry.load_local()["vaults"])
        self.assertNotIn("mine", registry.load_shared()["vaults"])

    def test_share_writes_the_committed_half(self):
        self._add("team", "reference", share=True)
        self.assertIn("team", registry.load_shared()["vaults"])
        self.assertNotIn("team", registry.load_local()["vaults"])

    def test_a_shared_vault_resolves_with_no_local_file_at_all(self):
        """The fresh-clone case: git carried `vaults.json` and the vault file, nothing
        else. Before this, the vault files arrived and the index that found them didn't."""
        self._add("team", "reference", share=True)
        config.VAULTS_REGISTRY.unlink(missing_ok=True)
        self.assertIn("team", registry.vaults())
        self.assertEqual(registry.provider_for("team").path,
                         self.tmp / ".charter" / "vaults" / "team.json")

    def test_the_account_pin_never_travels(self):
        """It is the one genuinely per-developer field, so it is split off even when the
        rest of the entry is published."""
        self._add("ops", "1password", share=True, op_vault="Engineering")
        registry.add_vault("ops", "1password",
                           {"op-vault": "Engineering", "account": "me@corp.com"},
                           force=True, share=True)
        self.assertNotIn("account", registry.load_shared()["vaults"]["ops"]["config"])
        self.assertEqual(
            registry.load_local()["vaults"]["ops"]["config"]["account"], "me@corp.com")

    def test_the_merged_view_carries_both(self):
        registry.add_vault("ops", "1password",
                           {"op-vault": "Engineering", "account": "me@corp.com"},
                           share=True)
        cfg = registry.vaults()["ops"]["config"]
        self.assertEqual(cfg["op-vault"], "Engineering")
        self.assertEqual(cfg["account"], "me@corp.com")

    def test_local_overrides_shared_field_by_field(self):
        """Per field, not per vault: pinning `account` must not require restating the
        provider and file, which is how a local copy silently drifts from the shared one."""
        registry.add_vault("t", "reference", {"file": "a/t.json"}, persona="qa", share=True)
        local = registry.load_local()
        local["vaults"]["t"] = {"config": {"account": "me"}}
        registry.save_registry(local)
        got = registry.vaults()["t"]
        self.assertEqual(got["persona"], "qa")               # from shared
        self.assertEqual(got["config"]["file"], "a/t.json")  # from shared
        self.assertEqual(got["config"]["account"], "me")     # from local

    def test_scope_is_reported(self):
        registry.add_vault("s", "reference", {"file": "s.json"}, share=True)
        registry.add_vault("l", "plain-file", {"file": "l.json"})
        self.assertEqual(registry.scope_of("s"), "shared")
        self.assertEqual(registry.scope_of("l"), "local")

    def test_removing_clears_both_halves(self):
        """Removing one and leaving the other is how a vault comes back from the dead
        after the user watched charter say it was gone."""
        registry.add_vault("t", "reference", {"file": "t.json"}, share=True)
        registry.add_vault("t", "reference", {"file": "t.json"}, force=True)
        self.assertEqual(registry.scope_of("t"), "both")
        registry.remove_vault("t")
        self.assertNotIn("t", registry.vaults())
        self.assertEqual(registry.scope_of("t"), "local")   # i.e. present in neither file

    def test_the_shared_file_is_world_readable_and_the_local_one_is_not(self):
        """The committed half carries no values by construction; the local half holds
        per-developer paths and account pins and keeps 0600."""
        import stat as _stat
        registry.add_vault("s", "reference", {"file": "s.json"}, share=True)
        registry.add_vault("l", "plain-file", {"file": "l.json"})
        self.assertEqual(_stat.S_IMODE(config.SHARED_VAULTS.stat().st_mode), 0o644)
        self.assertEqual(_stat.S_IMODE(config.VAULTS_REGISTRY.stat().st_mode), 0o600)


class PlaintextMustNotLandInGit(VaultRegistrationCase):
    """A plain-file vault holds PLAINTEXT. The default sits under `.charter/`, which
    `charter init` gitignores — but `--file` accepts any path, and a vault pointed at one
    git tracks commits the credentials on the next `charter save`. Nothing said so:
    `doctor` reported "all healthy", because from the vault's point of view it was.

    The PATH is refused, not `--share`: a team that provisions the file out of band has a
    legitimate use for a shared pointer, and the unignored path is the actual defect — it
    also catches the far more common case where `--share` was never passed at all.

    This is the class that runs REAL git — `git init` here, and `git check-ignore` inside
    `_unignored_plaintext` — so it is also the class whose answer the machine could decide.
    That is not charter being wrong: `check-ignore` honours global excludes on purpose, and
    `_unignored_plaintext` says so, because the question is "would git take this file" and
    git is the authority. It is the FIXTURE that must not be the operator's. See
    `test_the_machines_own_git_configuration_does_not_decide_this` for the measurement.
    """

    def setUp(self) -> None:
        super().setUp()
        import subprocess
        subprocess.run(["git", "init", "-q", str(self.tmp)], check=True, capture_output=True)
        (self.tmp / ".gitignore").write_text("/.charter/\n")

    def _add(self, name, provider="plain-file", file=None):
        with redirect_stderr(io.StringIO()) as err:
            rc = commands_secrets.cmd_vault_add(
                _args(name, provider, file=str(file) if file else None))
        return rc, err.getvalue()

    def test_a_tracked_path_is_refused(self):
        rc, err = self._add("leaky", file=self.tmp / "cfg" / "leaky.json")
        self.assertEqual(rc, 1)
        self.assertIn("NOT gitignored", err)
        self.assertNotIn("leaky", registry.vaults())

    def test_the_machines_own_git_configuration_does_not_decide_this(self):
        """Whether `cfg/leaky.json` is "tracked" is git's answer, and git's answer includes
        `core.excludesFile` — so a developer whose global ignore names `cfg/` or `*.json`
        would watch the case above go red for a reason that has nothing to do with charter.

        Measured, not supposed: with a global config whose `excludesFile` holds `cfg/`,
        `test_a_tracked_path_is_refused` fails (`0 != 1`) without the two variables below
        and passes with them. `init.templateDir` is the other half — `git init` runs hooks
        out of it — which is why the system config is neutralised too.
        """
        self.assertEqual(os.environ["GIT_CONFIG_GLOBAL"], os.devnull)
        self.assertEqual(os.environ["GIT_CONFIG_SYSTEM"], os.devnull)

    def test_the_gitignored_default_is_fine(self):
        rc, _ = self._add("ok")
        self.assertEqual(rc, 0)
        self.assertIn("ok", registry.vaults())

    def test_a_path_outside_the_plane_is_not_our_business(self):
        rc, _ = self._add("ext", file=self.tmp.parent / "outside.json")
        self.assertEqual(rc, 0)

    def test_a_reference_vault_there_is_allowed(self):
        """It stores op:// URIs rather than values, and committing those is the point."""
        rc, _ = self._add("refs", provider="reference", file=self.tmp / "cfg" / "refs.json")
        self.assertEqual(rc, 0)

    def test_a_plane_that_is_not_a_repo_has_nothing_to_leak_into(self):
        import shutil as _sh
        _sh.rmtree(self.tmp / ".git")
        rc, _ = self._add("anywhere", file=self.tmp / "cfg" / "v.json")
        self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
