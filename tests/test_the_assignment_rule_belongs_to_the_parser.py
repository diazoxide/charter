"""What counts as an assignment is decided by the parser that will do the assigning (#555).

`hooks._ENV_ASSIGN_RE` is `^[A-Za-z_][A-Za-z0-9_]*=` — a shell IDENTIFIER — and one constant
was answering two different parsers' questions with it.

For the front of a segment the identifier rule is exactly right, and that is not an
accident of taste: bash answers *"a-b=1: command not found"*, so `a-b=1 cat <vault>` runs a
program literally named `a-b=1` and reads nothing. Widening the constant everywhere would
have moved denials in both directions, because the same predicate stands in front of
ordinary segments where a token containing `=` is not an assignment at all (`git -c
a.b=c …`, an operand carrying a query string).

`env` is a different parser reading the same bytes. Its operand scan is `strchr(arg, '=')`
— GNU coreutils and BSD alike — so ANY argument containing an `=` is an assignment, and the
scan keeps going until it meets one without. The guard stopped at the first token it could
not read as a shell identifier and named that token the program.

## The measurement

A plane built by `charter init`, a fabricated value in `.charter/vaults/x.json`, every
command fed to the real `charter hook pretooluse` (the decision is nested under
`hookSpecificOutput`; empty stdout is an allow):

    cat .charter/vaults/x.json                       -> DENY   (control)
    env FOO=1 cat .charter/vaults/x.json             -> DENY   (control)
    env a-b=1 cat .charter/vaults/x.json             -> ALLOW
    env a.b=1 cat .charter/vaults/x.json             -> ALLOW
    env 1FOO=1 cat .charter/vaults/x.json            -> ALLOW
    env 'x y=1' cat .charter/vaults/x.json           -> ALLOW
    env a=1 b-c=2 cat .charter/vaults/x.json         -> ALLOW
    env -- a-b=1 cat .charter/vaults/x.json          -> ALLOW
    env -Sfoo=1 -Sbar=2 cat .charter/vaults/x.json   -> ALLOW

Every one of the seven printed `{"k":"FABRICATED-NOT-A-REAL-SECRET-9271"}` on this machine
(macOS 15, BSD `env`). The last row is the one wearing #547's clothes: after `-Sfoo=1`
unpacks, `env` has left its option scan, and `-Sbar=2` is an assignment to a variable
literally named `-Sbar` — confirmed, `env -Sfoo=1 -Sbar=2 sh -c 'echo $foo $bar'` prints
`1` and an empty `bar` while the `cat` runs.

## The property

**Two parsers, two predicates.** `_ENV_ASSIGN_RE` stays where the SHELL decides — the front
of a segment, `_invocation`, the trace's redaction — and `_WRAPPER_ASSIGN_OPERANDS` names
the wrappers whose own operand scan takes any `name=value`, applied only inside that
wrapper's argv. Neither question is answered with the other's constant, so widening one
cannot move the other.

Consuming a token there is the fail-CLOSED direction — it moves the program rightward and
can never swallow one — which is why the second `-S…` above is read as one more `-S` rather
than modelled as the assignment `env` makes of it: unpacking it reaches the same verdict on
a command that RUNS, and `tests/test_guard_differential.py` forbids denying less than
`origin/main` in any case.
"""

from __future__ import annotations

import unittest

from charter import hooks
from tests._isolation import PersonaIso, run_hook

VAULT = ".charter/vaults/x.json"

#: `(command, must be denied)` — the measurement above, verbatim. The two controls are the
#: neighbours whose denial made the guard look present.
ASSIGNMENT_ROWS = (
    (f"cat {VAULT}", True),                              # the control
    (f"env FOO=1 cat {VAULT}", True),                    # control: a shell identifier
    (f"env a-b=1 cat {VAULT}", True),                    # #555, as filed
    (f"env a.b=1 cat {VAULT}", True),
    (f"env 1FOO=1 cat {VAULT}", True),
    (f"env 'x y=1' cat {VAULT}", True),
    (f"env a=1 b-c=2 cat {VAULT}", True),                # a valid one first
    (f"env -- a-b=1 cat {VAULT}", True),                 # past end-of-options
    (f"env -Sfoo=1 -Sbar=2 cat {VAULT}", True),          # #555, second row
    (f"sudo a-b=1 cat {VAULT}", True),                   # `sudo [VAR=value]`, same scan
)


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


class TestEveryAssignmentSpellingOfOneVaultRead(unittest.TestCase):
    def test_the_table_still_holds_its_controls(self):
        cmds = [c for c, _ in ASSIGNMENT_ROWS]
        self.assertEqual(len(ASSIGNMENT_ROWS), 10)
        self.assertIn(f"env FOO=1 cat {VAULT}", cmds)
        self.assertIn(f"cat {VAULT}", cmds)

    def test_every_row_is_denied(self):
        for cmd, denied in ASSIGNMENT_ROWS:
            with self.subTest(cmd=cmd):
                reason = hooks._leak_reason(cmd)
                self.assertEqual(reason is not None, denied, cmd)
                self.assertIn("reads a vault/secret file directly", reason or "")


class TestTheSameTableThroughTheRealHook(PersonaIso):
    def test_every_row_is_denied_end_to_end(self):
        for cmd, denied in ASSIGNMENT_ROWS:
            with self.subTest(cmd=cmd):
                r = run_hook(hooks.pretooluse,
                             {"tool_name": "Bash", "tool_input": {"command": cmd},
                              "session_id": "s"})
                self.assertEqual(_decision(r), "deny" if denied else None, cmd)


class TestTheTwoPredicatesStayApart(unittest.TestCase):
    """The half of the fix that is about NOT widening: one constant answering two parsers is
    the defect, so replacing it with one WIDER constant would be the same defect."""

    def test_the_shell_rule_is_still_a_shell_identifier(self):
        """At the front of a segment `a-b=1` is a program name, because that is what bash
        makes of it — *"a-b=1: command not found"*, verified. A guard that read it as an
        assignment there would name `cat` as the program of a command that never runs."""
        self.assertIsNone(hooks._ENV_ASSIGN_RE.match("a-b=1"))
        self.assertIsNone(hooks._ENV_ASSIGN_RE.match("1FOO=1"))
        self.assertIsNotNone(hooks._ENV_ASSIGN_RE.match("FOO=1"))
        self.assertEqual(hooks._split_env(["a-b=1", "cat", VAULT])[0], "a-b=1")

    def test_a_wrapper_with_no_assignment_operands_keeps_the_shell_rule(self):
        """`nice a-b=1 cat <vault>` execs a program named `a-b=1` and reads nothing — `nice`
        has no `[VAR=value]` in its grammar. The wide rule is scoped to the wrappers that
        really do the assigning, not applied to every wrapper because it is convenient.

        **`doas` is the row that made this a test rather than a comment.** It was in the
        table for a round on the assumption that it is `sudo` with a shorter name; its usage
        is `doas [-Lns] [-a style] [-C config] [-u user] command [args]`, with no assignment
        operand in it. A hand-check deleted the entry and no test noticed, which is what a
        table row with no grammar behind it looks like — and padding a fail-closed table
        because it is fail-closed is the same reflex as padding a list of spellings.
        """
        for wrapper in ("nice", "doas", "timeout", "xargs", "nohup"):
            with self.subTest(wrapper=wrapper):
                self.assertNotIn(wrapper, hooks._WRAPPER_ASSIGN_OPERANDS)
                self.assertEqual(
                    hooks._split_env([wrapper, "a-b=1", "cat", VAULT])[0], "a-b=1")
        self.assertEqual(hooks._WRAPPER_ASSIGN_OPERANDS, frozenset(("env", "sudo")))

    def test_every_assignment_wrapper_is_one_this_module_strips(self):
        for wrapper in hooks._WRAPPER_ASSIGN_OPERANDS:
            self.assertIn(wrapper, hooks._WRAPPERS, wrapper)

    def test_a_git_config_flag_is_still_gits_option_and_not_an_assignment(self):
        """The false-positive direction the issue warned about. `git -c a.b=c <sub>` carries
        an `=` in a token that is an option VALUE; nothing here may start reading it as an
        environment assignment."""
        prog, env, argv = hooks._split_env(["git", "-c", "a.b=c", "status"])
        self.assertEqual((prog, env), ("git", []))
        self.assertEqual(argv, ["git", "-c", "a.b=c", "status"])


class TestTheParseAndNotJustTheVerdict(unittest.TestCase):
    """A denial can arrive from somewhere else — the raw-string arm, the unplaced-flag
    fallback — so the rows above are restated as the parse they depend on."""

    def test_a_name_that_is_not_an_identifier_is_still_an_assignment_to_env(self):
        prog, env, argv, _c, _r = hooks._split_env_chdir(
            ["env", "a-b=1", "cat", VAULT])
        self.assertEqual(prog, "cat")
        self.assertEqual(env, ["a-b=1"])
        self.assertEqual(argv, ["cat", VAULT])

    def test_the_scan_keeps_going_until_a_token_without_an_equals(self):
        """`env`'s operand scan is a loop, not a single test: `env a=1 b-c=2 cat <vault>`
        sets both and the utility is the `cat`."""
        prog, env, _a, _c, _r = hooks._split_env_chdir(
            ["env", "a=1", "b-c=2", "cat", VAULT])
        self.assertEqual((prog, env), ("cat", ["a=1", "b-c=2"]))

    def test_end_of_options_does_not_end_the_assignment_scan(self):
        """Verified: `env -- a-b=1 cat <vault>` prints the vault, so `--` stops the OPTIONS
        and nothing else."""
        prog, env, _a, _c, _r = hooks._split_env_chdir(
            ["env", "--", "a-b=1", "cat", VAULT])
        self.assertEqual((prog, env), ("cat", ["a-b=1"]))

    def test_a_second_packed_split_string_still_reaches_the_reader(self):
        """#555's second row. `env` reads `-Sbar=2` as an assignment because its option scan
        has ended; this parser reads it as one more `-S` and unpacks it. Different model,
        same program — and the guard's reading is the one that cannot lose a reader."""
        prog, _e, argv, _c, _r = hooks._split_env_chdir(
            ["env", "-Sfoo=1", "-Sbar=2", "cat", VAULT])
        self.assertEqual(prog, "cat")
        self.assertIn(VAULT, argv)

    def test_an_assignment_prefix_still_reaches_the_guards_that_read_it(self):
        """The env list is not a bin. `env a-b=1 GIT_SSH_COMMAND=/tmp/k git push` was
        allowed on `main` for this same reason — the golden-rule guard reads the prefix, and
        the prefix stopped at the token this parser could not place."""
        prog, env, _a, _c, _r = hooks._split_env_chdir(
            ["env", "a-b=1", "GIT_SSH_COMMAND=/tmp/k", "git", "push"])
        self.assertEqual(prog, "git")
        self.assertIn("GIT_SSH_COMMAND=/tmp/k", env)
        self.assertIsNotNone(
            hooks._single_credential_reason("env a-b=1 GIT_SSH_COMMAND=/tmp/k git push"))
