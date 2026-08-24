"""A `secret exec` child gets one vault's secrets — not every vault's identity.

`env = dict(os.environ)` handed the child the whole environment. Measured with
fabricated values, `charter secret exec <v> --env T=K -- /usr/bin/env` returned the one
secret the model named as `***` and every OTHER vault's service-account token in the
clear, into the caller's captured output. `base.redact` cannot help: it only knows the
values this call resolved.

`VaultProvider.env_overlay` sells the binding as least-privilege — "without this the
mapping lives in every caller's shell… which is the property the vault abstraction
otherwise removes". Inheriting the whole environment put it straight back.

**No real credential appears here.** The variable names are charter's own (that is the
point — the binding is stored by name), the values are fabricated, and every assertion
compares a FILTERED projection: the child prints the names it was given, never its
environment and never a value.
"""

from __future__ import annotations

import io
import json
import os
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_secrets as cs
from charter import config
from charter.secrets import registry
from tests._isolation import PersonaIso

#: Prints only the NAMES of the variables it was asked about, one per line, and only
#: those that are set. Never the value, never the whole environment.
PROBE = (
    "import os,sys;"
    "print('\\n'.join(n for n in sys.argv[1:] if n in os.environ))"
)


class ExecEnvCase(PersonaIso):
    """Three vaults, each declaring a different identity binding.

    Names chosen to be obviously fabricated AND obviously not this machine's: a test
    that used the real `OP_SERVICE_ACCOUNT_TOKEN` would pass or fail depending on
    whether the operator happens to be signed in — the ambient-env-var trap.
    """

    DEVOPS_SOURCE = "OP_FABRICATED_DEVOPS_TOKEN"
    MARKETING_SOURCE = "OP_FABRICATED_MARKETING_TOKEN"
    INFRA_SOURCE = "FABRICATED_VAULT_TOKEN"
    TARGET_OP = "OP_FABRICATED_TARGET_TOKEN"
    TARGET_VAULT = "FABRICATED_VAULT_ADDR_TOKEN"
    UNRELATED = "FABRICATED_UNRELATED_SETTING"

    def setUp(self) -> None:
        super().setUp()
        vf = config.ROOT / ".charter" / "vaults" / "shared.json"
        vf.parent.mkdir(parents=True, exist_ok=True)
        vf.write_text(json.dumps({"k": "FABRICATED-not-a-real-credential"}))
        for name, source in (("devops", self.DEVOPS_SOURCE),
                             ("marketing", self.MARKETING_SOURCE)):
            registry.add_vault(name, "plain-file",
                               {"file": str(vf), "env": {self.TARGET_OP: source}})
        registry.add_vault("infra", "plain-file",
                           {"file": str(vf),
                            "env": {self.TARGET_VAULT: self.INFRA_SOURCE}})

        # The environment a caller's shell would carry. Set explicitly rather than
        # assumed present: an assertion about a variable that was never set passes for
        # the wrong reason.
        self.enterContext(mock.patch.dict(os.environ, {
            self.DEVOPS_SOURCE: "FABRICATED-devops-identity",
            self.MARKETING_SOURCE: "FABRICATED-marketing-identity",
            self.INFRA_SOURCE: "FABRICATED-infra-identity",
            self.TARGET_OP: "FABRICATED-ambient-op-identity",
            self.TARGET_VAULT: "FABRICATED-ambient-vault-identity",
            self.UNRELATED: "not-a-credential",
        }))

    def names_the_child_sees(self, vault: str, *names: str) -> set[str]:
        """Run the probe under `secret exec <vault>` and return which of *names* it saw."""
        args = SimpleNamespace(vault=vault, env=None, file=None, dotenv=None,
                               exec_mode=False, stream_mode=False,
                               command=[sys.executable, "-c", PROBE, *names])
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = cs.cmd_secret_exec(args)
        self.assertEqual(rc, 0, err.getvalue())
        return {line for line in out.getvalue().split() if line}


class TestAnotherVaultsIdentityDoesNotTravel(ExecEnvCase):
    def test_another_vaults_token_env_is_not_inherited(self):
        seen = self.names_the_child_sees(
            "devops", self.MARKETING_SOURCE, self.INFRA_SOURCE)
        self.assertEqual(seen, set(),
                         "a child of one vault holds another vault's identity")

    def test_the_vault_being_read_keeps_its_own_names(self):
        """So `charter secret exec devops -- charter secret get devops K` still works.
        Also the sibling that stops the assertion above passing because the child's
        environment is empty for some unrelated reason."""
        seen = self.names_the_child_sees(
            "devops", self.DEVOPS_SOURCE, self.TARGET_OP)
        self.assertEqual(seen, {self.DEVOPS_SOURCE, self.TARGET_OP})

    def test_an_unrelated_variable_still_reaches_the_child(self):
        """This filters IDENTITY, not the environment. A child that lost PATH or its
        locale would break every real use of `secret exec`."""
        seen = self.names_the_child_sees("devops", self.UNRELATED, "PATH")
        self.assertEqual(seen, {self.UNRELATED, "PATH"})

    def test_the_target_half_of_a_binding_is_stripped_too(self):
        """`OP_SERVICE_ACCOUNT_TOKEN` is where the CLI READS an identity from, so an
        ambient one is exactly as much a cross-identity credential as the source it is
        copied from. A filter written against `.values()` alone would let it through —
        and it is the variable `op` actually authenticates with."""
        seen = self.names_the_child_sees("infra", self.TARGET_OP)
        self.assertEqual(seen, set())

    def test_a_vault_declaring_no_identity_still_strips_the_others(self):
        """The single-account setup: the vault being read declares nothing, so it
        subtracts nothing, and every declared name belongs to someone else."""
        registry.add_vault("plain", "plain-file",
                           {"file": str(config.ROOT / ".charter" / "vaults" / "shared.json")})
        seen = self.names_the_child_sees(
            "plain", self.DEVOPS_SOURCE, self.MARKETING_SOURCE, self.UNRELATED)
        self.assertEqual(seen, {self.UNRELATED})


class TestTheFilterIsBuiltFromTheRegistry(ExecEnvCase):
    def test_both_halves_of_every_binding_are_collected(self):
        got = cs._identity_vars()
        self.assertEqual(got["devops"], {self.TARGET_OP, self.DEVOPS_SOURCE})
        self.assertEqual(got["infra"], {self.TARGET_VAULT, self.INFRA_SOURCE})

    def test_a_malformed_committed_env_block_does_not_crash(self):
        """`vaults.json` has a committed half, so it is untrusted input: a string where
        a mapping belongs must not turn `secret exec` into a traceback."""
        doc = registry.load_registry()
        doc["vaults"]["devops"]["config"]["env"] = "OP_TOKEN"
        registry.save_registry(doc)
        got = cs._identity_vars()
        self.assertNotIn("devops", got)
        self.assertEqual(got["infra"], {self.TARGET_VAULT, self.INFRA_SOURCE})


class TestEveryRunModeUsesTheSameEnvironment(ExecEnvCase):
    def test_stream_mode_filters_too(self):
        """--stream and --exec skip the capturing path entirely; a filter applied at the
        capture site would miss both, and those are the unredacted modes."""
        args = SimpleNamespace(
            vault="devops", env=None, file=None, dotenv=None,
            exec_mode=False, stream_mode=True,
            command=[sys.executable, "-c", PROBE,
                     self.MARKETING_SOURCE, self.DEVOPS_SOURCE])
        err = io.StringIO()
        # --stream inherits this process's stdio, so the child writes to fd 1 directly;
        # capture it at the fd level.
        r, w = os.pipe()
        saved = os.dup(1)
        try:
            os.dup2(w, 1)
            os.close(w)
            with redirect_stderr(err):
                rc = cs.cmd_secret_exec(args)
            sys.stdout.flush()
        finally:
            os.dup2(saved, 1)
            os.close(saved)
        with os.fdopen(r) as fh:
            printed = fh.read()
        self.assertEqual(rc, 0)
        self.assertEqual({n for n in printed.split() if n}, {self.DEVOPS_SOURCE})


if __name__ == "__main__":
    unittest.main()
