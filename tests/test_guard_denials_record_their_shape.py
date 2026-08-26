"""A guard that cannot say WHAT tripped it cannot be audited.

`_single_credential_reason`'s docstring justifies the rule by asserting the denials "only
catch a DELIBERATE bypass". Ten days of traces on one machine recorded 335 of them across
three control planes — nobody deliberately bypasses 335 times, so either the claim is wrong
or the matcher is over-broad. Both hypotheses were untestable, because the trace recorded
`cmd=head` (the literal string "git") and a hardcoded `reason="single-credential"`.

So the deny now carries a `shape`: the binary plus the token that actually matched. Two
properties are load-bearing and both are asserted here:

* it DISCRIMINATES — a different trigger yields a different shape, or the field buys nothing;
* it carries NO OPERAND — this is a guard whose whole purpose is keeping secrets out of the
  transcript, and a trace that logged raw commands would re-create the leak in a file.
"""

import unittest

from tests._isolation import run_hook
from tests.test_hooks import InAControlPlane
from charter import hooks, trace

SSH_URL = "git@github.com:acme/x.git"


class ShapeCase(InAControlPlane):
    def shape_of(self, cmd: str) -> str | None:
        """Run the guard and return the `shape` recorded on the deny it traced."""
        r = run_hook(hooks.pretooluse, {"tool_input": {"command": cmd}, "session_id": "s"})
        self.assertIsNotNone(r, f"expected a decision for: {cmd}")
        self.assertEqual("deny", r["hookSpecificOutput"]["permissionDecision"])
        denies = [e for e in trace.read("s") if e.get("event") == "deny"]
        self.assertEqual(1, len(denies), denies)
        return denies[0].get("shape")


class TestEachTriggerHasItsOwnShape(ShapeCase):
    def test_ssh_transport_env(self):
        self.assertEqual("git GIT_SSH_COMMAND=", self.shape_of("GIT_SSH_COMMAND=ssh git fetch"))

    def test_ssh_command_config_flag(self):
        self.assertEqual("git -c core.sshCommand=",
                         self.shape_of("git -c core.sshCommand=ssh fetch"))

    def test_persisted_ssh_command(self):
        self.assertEqual("git config core.sshCommand",
                         self.shape_of("git config core.sshCommand ssh"))

    def test_an_ssh_url_operand(self):
        self.assertEqual("git <ssh-url>", self.shape_of(f"git clone {SSH_URL}"))

    def test_signing(self):
        self.assertEqual("git commit --gpg-sign", self.shape_of("git commit --gpg-sign -m x"))

    def test_ssh_to_a_forge(self):
        self.assertEqual("ssh <forge>", self.shape_of("ssh -T git@github.com"))


class TestTheShapesAreDistinct(ShapeCase):
    def test_no_two_triggers_collapse_to_one_label(self):
        """If two triggers share a shape the field cannot separate them in the tally."""
        seen = [
            self.shape_of("GIT_SSH_COMMAND=ssh git fetch"),
            self.shape_of("git -c core.sshCommand=ssh fetch"),
            self.shape_of("git config core.sshCommand ssh"),
            self.shape_of(f"git clone {SSH_URL}"),
            self.shape_of("git commit --gpg-sign -m x"),
            self.shape_of("ssh -T git@github.com"),
        ]
        self.assertEqual(len(seen), len(set(seen)), seen)

    def setUp(self):
        super().setUp()
        # each `shape_of` asserts exactly one deny, so this case reads its own session per call
        self._n = 0

    def shape_of(self, cmd):
        self._n += 1
        r = run_hook(hooks.pretooluse,
                     {"tool_input": {"command": cmd}, "session_id": f"s{self._n}"})
        self.assertEqual("deny", r["hookSpecificOutput"]["permissionDecision"])
        return [e for e in trace.read(f"s{self._n}") if e.get("event") == "deny"][0].get("shape")


class TestTheShapeCarriesNoOperand(ShapeCase):
    """The trace must not become the leak the guard exists to prevent."""

    def test_a_secret_looking_value_never_reaches_the_trace(self):
        shape = self.shape_of("GIT_SSH_COMMAND='ssh -i /keys/id_rsa_SUPERSECRET' git fetch")
        self.assertNotIn("SUPERSECRET", shape)
        self.assertNotIn("id_rsa", shape)

    def test_the_host_and_path_of_an_ssh_url_are_not_recorded(self):
        shape = self.shape_of("git clone git@github.com:acme/private-repo.git")
        self.assertNotIn("private-repo", shape)
        self.assertNotIn("acme", shape)

    def test_the_whole_command_is_never_the_shape(self):
        cmd = "git commit --gpg-sign -m 'release 1.2.3'"
        self.assertNotIn("1.2.3", self.shape_of(cmd))


class TestTheHumanReasonIsUnchanged(InAControlPlane):
    """`_single_credential_reason` keeps its signature and its prose — the refactor adds a
    sibling, it does not rewrite what the operator is told."""

    def test_it_still_returns_the_remedy_string(self):
        reason = hooks._single_credential_reason(f"git clone {SSH_URL}")
        self.assertIsNotNone(reason)
        self.assertIn("token-only", reason)
        self.assertIn("https://github.com", reason)

    def test_it_still_returns_none_for_an_ordinary_command(self):
        self.assertIsNone(hooks._single_credential_reason("git status"))


class TestTheCmdFieldLeaksNoValueEither(ShapeCase):
    """`cmd` records the command's first token, which for `VAR=value cmd` is the whole
    assignment — value included. Live traces held `D=/private/tmp/.../demo-plane;` and
    `R=/Users/aharon/IdeaProjects/charter;`, so this is observed, not theoretical.

    It predates `shape` and was invisible until `shape` established the rule it breaks:
    nothing the operator typed as a VALUE may reach a file that outlives the conversation.
    """

    def head_of(self, cmd: str) -> str:
        r = run_hook(hooks.pretooluse,
                     {"tool_input": {"command": cmd}, "session_id": "h"})
        self.assertEqual("deny", r["hookSpecificOutput"]["permissionDecision"])
        return [e for e in trace.read("h") if e.get("event") == "deny"][0].get("cmd")

    def test_an_env_assignment_records_only_the_variable_name(self):
        head = self.head_of("GIT_SSH_COMMAND=/keys/id_rsa_SECRET git fetch")
        self.assertNotIn("SECRET", head)
        self.assertNotIn("/keys/", head)
        self.assertEqual("GIT_SSH_COMMAND=", head)

    def test_an_ordinary_binary_is_unchanged(self):
        self.assertEqual("git", self.head_of("git clone git@github.com:acme/x.git"))


if __name__ == "__main__":
    unittest.main()
