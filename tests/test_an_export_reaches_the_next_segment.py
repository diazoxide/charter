"""The guards read the environment a command line ESTABLISHES, not the prefix attached to
one invocation (#496).

`_git_target` learned to read `GIT_DIR` / `GIT_WORK_TREE` from the env-assignment prefix of
a git invocation in #477 — the form `_split_env` was already holding. An `export` in an
earlier segment of the same command line sets the same variable for the same shell and
reaches the same git, and nothing read it.

## The measurement

Verified end to end against git 2.50.1: a plane root on `main`, a shell standing in a
workspace clone.

    GIT_DIR=<plane>/.git git checkout feature                -> DENY   (as of #477)
    export GIT_DIR=<plane>/.git && git checkout feature      -> ALLOW  <-- the bypass
    export GIT_WORK_TREE=<plane> && git checkout feature     -> ALLOW
    declare -x GIT_DIR=<plane>/.git && git checkout feature  -> ALLOW
    GIT_DIR=<plane>/.git; export GIT_DIR; git checkout feature -> ALLOW
    set -a; GIT_DIR=<plane>/.git; git checkout feature       -> ALLOW

and the plane root's HEAD really moved: `git -C <plane> symbolic-ref --short HEAD` answers
`feature` afterwards, with the hook having printed nothing.

**And the guard next door had it too**, found by sweeping the shape rather than from a
report — the golden rule's one-credential guard reads the same env prefix:

    GIT_SSH_COMMAND=/tmp/k git push                          -> DENY
    export GIT_SSH_COMMAND=/tmp/k && git push                -> ALLOW  <-- the sibling

## The property

`_plane_root_git` already carried one shell effect across segments — a `cd`, whose
destination it tracks in `here` (#183). The environment a command line has set for its later
segments is the same shape of carried state, and `hooks._exported_env` is it: computed once,
parallel to the segments, and read by BOTH guards, because a second hand-written copy grows
its own blind spots on its own schedule.

**The shapes are the ones a shell really exports with**, each checked against bash 5 and
zsh: `export NAME=VALUE`; `declare -x` / `typeset -x`, which bundle their `x`; `NAME=VALUE`
in one segment and `export NAME` in a later one; and `set -a`, after which a bare assignment
segment is exported too. A bare `NAME=VALUE` segment on its own is deliberately NOT
exported — `FOO=1; <child>` really does leave `FOO` unset in the child, verified — so
tracking it as an export would be a false denial invented out of nothing.

**This environment only ever GROWS.** `unset`, `export -n` and a subshell ending are not
modelled: forgetting a variable is the fail-OPEN direction, and a list that gets shorter as
the command line gets longer is a bypass by construction. It is the same invariant
`_git_target`'s subject list keeps, and it costs a denial on
`export GIT_DIR=… && unset GIT_DIR && git checkout feature`.

**The boundary is `cd`'s boundary.** A `$(…)`, a sourced file, a `~/.bashrc`, and a `GIT_DIR`
already in the session's environment before the hook ran are all outside it — for the last
one the `PreToolUse` payload carries the command and the cwd, not the environment the
command will inherit, so there is nothing to read. Stated limits, not gaps, and
`TestTheStatedLimits` holds them stated.
"""

from __future__ import annotations

import unittest

from charter import hooks


def _seg(cmd: str):
    return hooks._segment_argv(cmd)


def _carried(cmd: str) -> list[list[str]]:
    return hooks._exported_env(_seg(cmd))


class TestWhatAShellReallyExports(unittest.TestCase):
    """`_exported_env` on its own, one row per spelling — because the guards above it can
    deny for other reasons and a carried env that stopped carrying would not show there."""

    def test_export_with_a_value_reaches_the_next_segment(self):
        self.assertEqual(_carried("export FOO=1 && git status")[-1], ["FOO=1"])

    def test_a_semicolon_carries_it_as_far_as_an_and(self):
        self.assertEqual(_carried("export FOO=1; git status")[-1], ["FOO=1"])

    def test_declare_and_typeset_export_only_with_x(self):
        """`declare -x FOO=1` is `export` spelled another way; `declare FOO=1` is a shell
        variable and exports nothing. Both verified against bash 5."""
        self.assertEqual(_carried("declare -x FOO=1 && git status")[-1], ["FOO=1"])
        self.assertEqual(_carried("typeset -x FOO=1 && git status")[-1], ["FOO=1"])
        self.assertEqual(_carried("declare FOO=1 && git status")[-1], [])

    def test_the_x_is_a_letter_and_may_be_bundled(self):
        """`declare -gx FOO=1` — the same walk-the-letters property `_wrapper_option` is."""
        self.assertEqual(_carried("declare -gx FOO=1 && git status")[-1], ["FOO=1"])

    def test_a_bare_assignment_segment_exports_nothing(self):
        """The row that keeps this from inventing denials: `FOO=1; <child>` leaves `FOO`
        unset in the child — verified against bash 5 and zsh. It is a SHELL variable."""
        self.assertEqual(_carried("FOO=1; git status")[-1], [])

    def test_a_bare_assignment_then_an_export_by_name_does_export(self):
        self.assertEqual(_carried("FOO=1; export FOO; git status")[-1], ["FOO=1"])

    def test_set_a_exports_the_bare_assignments_after_it(self):
        """`set -a; FOO=1; <child>` really does export — verified. `set -o allexport` is the
        same switch spelled long, and `-ax` bundles it."""
        for prefix in ("set -a", "set -o allexport", "set -ax"):
            with self.subTest(prefix=prefix):
                self.assertEqual(
                    _carried(f"{prefix}; FOO=1; git status")[-1], ["FOO=1"])

    def test_the_environment_is_the_one_in_force_before_each_segment(self):
        """A parallel list, not one answer for the whole line: the export's own segment has
        not run yet when the export is read."""
        carried = _carried("export FOO=1 && export BAR=2 && git status")
        self.assertEqual(carried, [[], ["FOO=1"], ["FOO=1", "BAR=2"]])

    def test_a_name_without_a_value_is_not_a_shell_variable(self):
        """`declare GIT_DIR` declares a name and assigns nothing, so a later
        `export GIT_DIR` has nothing to export.

        Recording it anyway is not harmless bookkeeping: a valueless `GIT_DIR` reaches
        `_git_target`, which reads it as "the directory I am standing in" and quietly adds
        the cwd's PARENT to the subjects of every later git command in the line. Found by
        `tools/sweep.py`, which dropped the `_ENV_ASSIGN_RE` test and watched nothing
        notice.
        """
        self.assertEqual(_carried("declare GIT_DIR; export GIT_DIR; git status")[-1], [])
        self.assertEqual(_carried("declare FOO BAR; export BAR; git status")[-1], [])
        self.assertEqual(_carried("declare FOO=1 BAR; export BAR; git status")[-1], [])

    def test_a_flag_cannot_be_mistaken_for_a_variable_name(self):
        """The reason the export loop needs no "skip the flags" branch, pinned rather than
        assumed — the branch was there, had no observable consequence over 213,927 segment
        lists, and was deleted.

        Every key in the shell-variable table comes from `_ENV_ASSIGN_RE`, which is a shell
        IDENTIFIER: it cannot match a token starting with `-`, so a flag can neither be
        recorded as a variable nor found as one. If that regex ever admitted a leading `-`,
        `export -n` would start exporting a variable called `-n`, and the deleted branch
        would be load-bearing again.
        """
        self.assertIsNone(hooks._ENV_ASSIGN_RE.match("-n=1"))
        self.assertIsNone(hooks._ENV_ASSIGN_RE.match("-x"))
        self.assertEqual(_carried("export -n FOO && git status")[-1], [])
        self.assertEqual(_carried("declare -x -- GIT_DIR=/p/.git && git status")[-1],
                         ["GIT_DIR=/p/.git"])

    def test_it_only_ever_grows(self):
        """`unset` is not modelled, and that is the fail-CLOSED direction. Pinned so a later
        round cannot add it without meeting this note first."""
        self.assertEqual(_carried("export FOO=1 && unset FOO && git status")[-1], ["FOO=1"])


class TestThePlaneRootGuardSeesIt(unittest.TestCase):
    """`_git_target` is handed `carried + segment prefix`, in that order, so an assignment
    attached to THIS invocation still overrides one an earlier `export` set — which is what
    git itself does."""

    def test_an_exported_git_dir_names_the_repository(self):
        carried = _carried("export GIT_DIR=/p/.git && git checkout feature")[-1]
        targets = [str(t) for t in hooks._git_target("/clone", [], carried)]
        self.assertIn("/p/.git", targets)

    def test_an_attached_assignment_still_wins_over_an_earlier_export(self):
        carried = _carried("export GIT_DIR=/p/.git && GIT_DIR=/q/.git git checkout f")[-1]
        targets = [str(t) for t in
                   hooks._git_target("/clone", [], carried + ["GIT_DIR=/q/.git"])]
        self.assertIn("/q/.git", targets)
        self.assertNotIn("/p/.git", targets)


class TestTheGuardNextDoorSeesItToo(unittest.TestCase):
    """The sibling the sweep found. Both guards read the same carried env, from one place,
    so neither can be fixed while the other is left behind — which is how `GIT_DIR` came to
    be refused in one spelling and allowed in the next."""

    def test_an_exported_ssh_command_is_still_a_golden_rule_violation(self):
        for cmd in ("export GIT_SSH_COMMAND=/tmp/k && git push",
                    "export GIT_SSH_COMMAND=/tmp/k; git push",
                    "declare -x GIT_SSH_COMMAND=/tmp/k && git push",
                    "export GIT_SSH=/tmp/k && git fetch"):
            with self.subTest(cmd=cmd):
                reason = hooks._single_credential_reason(cmd)
                self.assertIsNotNone(reason, cmd)
                self.assertIn("SSH", reason)

    def test_the_attached_spelling_is_unchanged(self):
        self.assertIsNotNone(
            hooks._single_credential_reason("GIT_SSH_COMMAND=/tmp/k git push"))

    def test_an_unrelated_export_is_not_a_violation(self):
        """The precision half: carrying the environment may not turn every `export` into a
        credential finding."""
        self.assertIsNone(hooks._single_credential_reason("export PAGER=cat && git log"))

    def test_the_trigger_shape_still_names_no_value(self):
        """A guard that exists to keep secrets out of the transcript may not start writing
        them into the trace because the variable arrived by a different route."""
        shape, _detail = hooks._single_credential_hit(
            "export GIT_SSH_COMMAND=/keys/id_rsa && git push")
        self.assertEqual(shape, "git GIT_SSH_COMMAND=")
        self.assertNotIn("id_rsa", shape)


class TestTheStatedLimits(unittest.TestCase):
    """Pinned as limits, so a later round meets the reasoning rather than the silence."""

    def test_a_command_substitution_is_not_followed(self):
        """The same boundary `cd` has: charter models the shell effects it has evidence for
        in the argv it was handed. A `$(…)` is a segment boundary to `_segment_argv`, so
        what is carried is the literal text in front of it and never the substitution's
        RESULT — which is the fail-open direction, and stated rather than hidden."""
        carried = _carried("export GIT_DIR=$(cat /tmp/p) && git status")[-1]
        self.assertEqual(carried, ["GIT_DIR=$"])
        self.assertNotIn("GIT_DIR=/tmp/p", carried)

    def test_an_environment_set_before_the_hook_ran_is_not_visible(self):
        """The `PreToolUse` payload carries the command and the cwd, not the environment the
        command will inherit. Nothing to read, so nothing is claimed."""
        self.assertEqual(_carried("git checkout feature")[-1], [])
