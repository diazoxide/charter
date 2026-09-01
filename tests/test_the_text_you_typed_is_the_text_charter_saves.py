"""charter's own text-taking commands may not carry a live command substitution (#778).

#710 refused the shape on `gh`/`glab` bodies. **charter's own commands were not in that
table**, and it bit immediately: while writing up #710's findings, the coordinating agent
ran ``charter persona remember "… `pending` …"``. zsh ran the word, printed `command not
found`, spliced its empty output, and the saved memory read *"appending to  each pass"*
with the word silently gone. That file lives under `personas/_shared/memory/`, which this
plane commits and pushes — the same defect reaching a public repository by an indirect
route, which is exactly why nobody saw it.

**What decided that this becomes a guard rather than a third documented limit is a
measurement**, and it came out the opposite way from the one #778 predicted. #778 expected
charter's own prose to be "similar or worse" than the commit-message figure of 26 of 30.
Measured over the 284 committed memory bodies on `main`:

* **9 carry a backtick and 4 carry `$(`; 13 would meet a liveness-keyed guard — 5%.**
  Not 87%. 254 of the 284 predate the working rule that tells agents to avoid the shape,
  so this is charter's natural prose, not an effect of the rule.
* The reason is real rather than lucky: a commit message is rendered as markdown by a
  forge, so agents write code spans in them; a memory body is read back by
  `charter recall` in a terminal, so agents write *"the $( branch"* and *"a backtick"* as
  words.
* And the 13 are not false positives at all. **283 of the 284 bodies are single-line**, so
  none of them came through the `"$(cat <<'EOF' …)"` spelling that makes a backtick inert;
  a live backtick in a single-line double-quoted operand is a command that was going to
  corrupt its own text.

**The remedy had to be checked too, because it is not the one #710 names.** 221 of the 284
bodies contain an apostrophe — and *all nine* of the backtick-carrying ones do — so "use
single quotes" fails on the exact population this refuses. What works is one backslash per
backtick, verified against a real `bash` in `TheRemedyItSteersToward` rather than believed:
inside double quotes a backslashed backtick is a literal backtick and the apostrophes keep
working.

The commit-message surface stays out, and `CommitMessagesAreStillTheDocumentedLimit` pins
it. That is #711, and the measurement there says the opposite of this one.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import tempfile
import unittest

from charter import cli, config, hooks
from tests._isolation import PersonaIso, run_hook

#: The #778 case with the payload replaced. `id -un` is a substitution and nothing else,
#: which is the only property under test; a runnable reproduction of an exfiltration does
#: not belong in a committed file.
INCIDENT = 'charter persona remember "the marker is appended to `id -un` each pass"'


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


def _reason(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecisionReason", "")


class CharterProseCase(PersonaIso):
    """Helpers only — the hook driven end to end, with a plane present.

    Split out rather than subclassed for its tests, following the forge guard's own file:
    inheriting a TestCase re-runs every parent case once per child, which turns one real
    failure into four and makes the count say nothing.
    """

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")

    def deny_reason(self, cmd: str) -> str:
        r = run_hook(hooks.pretooluse, {"tool_name": "Bash",
                                        "tool_input": {"command": cmd},
                                        "cwd": str(config.ROOT)})
        self.assertEqual(_decision(r), "deny", f"expected a refusal for {cmd!r}")
        return _reason(r)

    def allowed(self, cmd: str) -> None:
        r = run_hook(hooks.pretooluse, {"tool_name": "Bash",
                                        "tool_input": {"command": cmd},
                                        "cwd": str(config.ROOT)})
        self.assertNotEqual(_decision(r), "deny", f"expected {cmd!r} to be allowed")


class TheTextThatWasLost(CharterProseCase):
    """The #778 case is refused, and the refusal explains itself."""

    def test_the_incident_is_refused(self) -> None:
        why = self.deny_reason(INCIDENT)
        # The four things the reader needs, none of which is "denied": which command, what
        # it does with the text, what the shell is about to do, and what to type instead.
        self.assertIn("charter persona remember", why)
        self.assertIn("personas/", why)
        self.assertIn("substitution", why)
        self.assertIn("\\`", why)

    def test_the_modern_spelling_is_refused_too(self) -> None:
        self.assertIn("$(…)", self.deny_reason('charter persona remember "at $(date) today"'))
        self.assertIn("`…`", self.deny_reason(INCIDENT))

    def test_the_shared_flag_does_not_change_the_answer(self) -> None:
        """`--shared` writes to `personas/_shared/memory/`, which is where the #778 memory
        landed. A guard that read the flag would have missed the actual incident."""
        self.deny_reason('charter persona remember --shared "a `id -un` span"')

    def test_every_row_in_the_table_is_refused(self) -> None:
        # The count is the precondition: a loop over a table that emptied proves nothing,
        # and half this table is generated.
        self.assertEqual(len(hooks._CHARTER_PROSE), 16)
        for (noun, verb), _facts in sorted(hooks._CHARTER_PROSE.items()):
            with self.subTest(f"{noun} {verb}"):
                self.deny_reason(f'charter {noun} {verb} x --why "`id -un`"')

    def test_the_alias_spellings_are_refused_end_to_end(self) -> None:
        """`ws` and `wt` written out, not walked. The table's alias half is *generated*, so
        every assertion that loops over it goes quiet rather than red when the generator
        produces nothing — which the deletion sweep demonstrated by corrupting the alias
        map's keys and surviving the whole suite. These five lines are what an agent
        actually types, and they cannot be satisfied by an empty table.
        """
        for cmd in ('charter ws note "a `id -un` span"',
                    'charter ws remember "a `id -un` span"',
                    'charter ws todo "a `id -un` span"',
                    'charter ws vision "a `id -un` span"',
                    'charter wt abandon "a `id -un` span"'):
            with self.subTest(cmd):
                self.deny_reason(cmd)

    def test_a_verb_that_is_not_in_the_table_is_allowed(self) -> None:
        """Reading, listing and searching are not writing. `recall` in particular takes a
        query that an agent has every reason to compute."""
        for cmd in ("charter recall", "charter persona recall", "charter workspace recall",
                    "charter status", "charter report list", "charter change list",
                    "charter persona list", "charter news"):
            with self.subTest(cmd):
                self.allowed(f'{cmd} --query "`id -un`"')

    def test_a_flags_value_cannot_supply_the_pair(self) -> None:
        """`charter recall --scope persona remember "…"` is a SEARCH, and `persona` and
        `remember` really are adjacent argv words on it. A guard that walked adjacent pairs
        the way the forge guard must would refuse it; reading the first two words after
        `charter` cannot, because charter's root parser has no option that takes a value.

        This is the test that makes that design choice load-bearing rather than incidental —
        switch the reader to pairs and it goes red.
        """
        self.allowed('charter recall --scope persona remember "`id -un`"')
        self.allowed('charter persona lint --only remember "`id -un`"')


class TheRemedyItSteersToward(CharterProseCase):
    """A denial with no cheap remedy is a denial that gets switched off (#371).

    The remedy here is **not** the one #710 names. 221 of the 284 committed memory bodies
    contain an apostrophe and all nine backtick-carrying ones do, so single-quoting — the
    obvious answer — fails on precisely the text this refuses. One backslash per backtick
    is what works, and this checks that against a real `bash` rather than asserting it,
    because a fix for a class of bug is unusually likely to contain that bug.
    """

    #: A real body's shape: prose with an apostrophe AND a code span. All nine of the
    #: backtick-carrying bodies in the corpus look like this, which is why the escaped form
    #: rather than the single-quoted one is the remedy the denial has to name.
    APOSTROPHE_AND_SPAN = "the flag's default is `id -un` here"

    def test_a_backslash_escaped_backtick_is_allowed(self) -> None:
        escaped = self.APOSTROPHE_AND_SPAN.replace("`", "\\`")
        self.allowed(f'charter persona remember "{escaped}"')

    def test_bash_agrees_the_escaped_form_is_inert_and_keeps_the_backtick(self) -> None:
        """The remedy has to be *correct*, not merely permitted: the shell must not run the
        word, and the character the author meant must survive into the argument.

        A sentinel file is the oracle for "did it run" — asking whether output reached
        stdout is a different question, and the forge guard's own differential test records
        what happens when the two are confused.
        """
        with tempfile.TemporaryDirectory() as d:
            sentinel = os.path.join(d, "ran")
            body = f"the flag's default is \\`touch {sentinel}\\` here"
            r = subprocess.run(["bash", "-c", f'printf %s "{body}"'],
                               capture_output=True, text=True, timeout=20)
            self.assertFalse(os.path.exists(sentinel), "bash RAN the escaped substitution")
            self.assertIn("`", r.stdout, "the backtick did not survive into the argument")
            self.assertIn("flag's", r.stdout, "the apostrophe did not survive")

    def test_single_quoting_is_allowed_where_the_text_permits_it(self) -> None:
        self.allowed("charter persona remember 'a `id -un` span'")

    def test_a_parameter_expansion_is_not_a_substitution(self) -> None:
        """The denial's answer for a value that really must be interpolated: compute it in a
        separate Bash call and pass `"$VAR"`. That has to be allowed, or the advice is a
        dead end — and it is genuinely safe, because a shell does not re-expand a
        parameter's value."""
        self.allowed('charter persona remember "release $NOTES shipped"')
        self.allowed('charter persona remember "release ${NOTES} shipped"')

    def test_a_bug_report_about_this_guard_can_still_be_filed(self) -> None:
        """#183 is this repository's record of a guard that **blocked its own bug report**:
        the plane-root guard read prose inside `charter report bug '…'` as a command, and the
        reporter had to file through a file. `report bug|gap` is in this table, so the same
        trap is one apostrophe away.

        It is not the same failure — that refusal was false, and a live backtick here is a
        command the shell really would run, so the text would arrive corrupted either way.
        But the *route out* has to exist and has to be the one the denial names, or the first
        person to report a fault in this guard cannot report it.
        """
        text = "charter refuses a code span in report bug"
        self.allowed(f"charter report bug '{text}: `id -un`'")
        self.allowed(f'charter report bug "{text}: \\`id -un\\`"')
        self.allowed("charter report bug --from-file /tmp/report.md")
        self.allowed("charter report bug - < /tmp/report.md")

    def test_report_names_its_own_file_input_and_the_memory_commands_do_not(self) -> None:
        """`report bug|gap` really has `--from-file` and `--stdin`; `persona remember` has
        neither. A denial that offered a flag the command does not accept would send the
        reader to a usage error, so the remedy is per-row and not one sentence for all.

        **The absence is asserted on the whole clause, not on the flag's spelling.** Testing
        only for `--from-file` passes against a guard that emits ``or pass `None <path>``` —
        which is exactly what the deletion sweep built, by collapsing the conditional so the
        file clause is always taken. `None` is not `--from-file`, so the assertion held
        while the denial had become nonsense.
        """
        why_report = self.deny_reason('charter report bug "a `id -un` span"')
        self.assertIn("--from-file", why_report)
        self.assertIn("--stdin", why_report)
        why_memory = self.deny_reason(INCIDENT)
        for absent in ("--from-file", "--stdin", "or pass", "None"):
            with self.subTest(absent):
                self.assertNotIn(absent, why_memory)


class TheCommandWordItAnswersTo(CharterProseCase):
    """`charter`, and the one other spelling this repository actually uses."""

    def test_the_module_invocation_is_covered(self) -> None:
        """`python3 -m charter …` is what CONTRIBUTING tells you to run against a checkout,
        so it is not an exotic spelling here — it is the one an agent working ON charter
        types all day. `os.path.basename(prog)` is `python3` on that line, so the forge
        guard's reader would have seen no `charter` at all."""
        self.deny_reason('python3 -m charter persona remember "a `id -un` span"')

    def test_interpreter_flags_before_the_module_do_not_hide_it(self) -> None:
        self.deny_reason('python3 -B -m charter persona remember "a `id -un` span"')
        self.deny_reason('env -u PYTHONSAFEPATH python3 -B -m charter persona remember "`id -un`"')

    def test_another_module_is_not_charter(self) -> None:
        """**The word `charter` has to appear on the line, or this test measures nothing.**

        The obvious spelling — `python3 -m pytest persona remember "…"` — never reaches the
        module check at all: `_charter_substitution_hit`'s hot-path filter answers first
        because the string holds no `charter`. So the assertion passed on a path that could
        not have failed, and the deletion sweep proved it by removing the `else None` and
        surviving. That is this repository's signature defect — *a case two arms can both
        satisfy, tests neither* — and it is the second time this exact module has produced
        it.

        The word sits in the *prose* here, which is where it would sit in real use.
        """
        cmd = 'python3 -m pytest persona remember "check the charter `id -un`"'
        self.assertIn("charter", cmd, "precondition: the hot-path filter must let this "
                                      "reach the module check")
        self.allowed(cmd)

    def test_a_path_to_the_program_still_matches(self) -> None:
        self.deny_reason('/usr/local/bin/charter persona remember "a `id -un` span"')

    def test_a_charter_call_with_too_few_words_does_not_raise(self) -> None:
        """`charter "`x`"` has ONE word after the program, so a reader that indexes
        `words[1]` raises **IndexError** — and a raise is the outcome this module may least
        have, because `dispatch` runs the handler as a bare `rc = fn()` and an exception
        takes the turn down instead of producing a verdict. #710's own sweep found two of
        these; this is the third.

        The command has to carry a substitution AND the word `charter`, or the hot-path
        filters answer before the reader is ever reached and the assertion measures nothing.
        """
        for cmd in ('charter "`id -un`"', "charter `id -un`", 'charter'):
            with self.subTest(cmd):
                self.assertIsNone(hooks._charter_substitution_hit(cmd))
                self.allowed(cmd)

    def test_exactly_two_words_is_inside_the_boundary_not_outside_it(self) -> None:
        """`charter ws note` is the whole command — a noun and a verb and nothing else — so
        the guard's `len(words) < 2` must admit it. Widen that to `<= 2` and this line is
        **allowed**, which is a fail-open on precisely the two-word shape the table is keyed
        on.

        The substitution sits in a different segment, which is what makes a two-word segment
        reachable at all and is the coarseness the guard documents: the scope is the whole
        Bash call.
        """
        why = self.deny_reason('cd "$(pwd)" && charter ws note')
        self.assertIn("charter ws note", why)


class TheTableIsVerifiedAgainstTheRealParser(unittest.TestCase):
    """#710's bar: every row checked against real `--help`, not recalled — and here the
    parser itself can be asked, which is stronger than its rendered help.

    These are the two facts the guard's shape rests on. If either stops being true, this
    goes red and the next reader is told which part of the design moved.
    """

    def test_every_row_is_a_real_charter_subcommand_taking_free_text(self) -> None:
        p = cli.build_parser()
        for (noun, verb), _facts in sorted(hooks._CHARTER_PROSE.items()):
            with self.subTest(f"{noun} {verb}"):
                sub = self._sub(p, noun)
                self.assertIsNotNone(sub, f"charter has no `{noun}` command")
                leaf = self._sub(sub, verb)
                self.assertIsNotNone(leaf, f"charter {noun} has no `{verb}` verb")
                self.assertTrue(self._takes_free_text(leaf),
                                f"charter {noun} {verb} takes no free-text operand")

    def test_charter_has_no_global_option_that_takes_a_value(self) -> None:
        """Which is what makes reading the FIRST two words exact, where the forge guard has
        to walk adjacent pairs: `gh --repo o/r issue create` puts a flag's value in front of
        the noun, and charter's root parser has no flag that can do that.

        Reading two words instead of every pair is strictly narrower — it is what keeps
        `charter recall --scope persona remember`, where `persona` and `remember` really are
        adjacent argv words, out of the table. If a value-taking global option is ever added
        to charter, this test fails and the guard has to move to adjacent pairs.
        """
        for a in cli.build_parser()._actions:
            if isinstance(a, (argparse._HelpAction, argparse._VersionAction,
                              argparse._SubParsersAction)):
                continue
            self.assertEqual(a.nargs, 0,
                             f"root option {a.option_strings} takes a value — the guard "
                             f"must switch to adjacent pairs")

    def test_the_alias_map_names_the_two_pairs_charter_actually_has(self) -> None:
        """**Named, not iterated.** The loop below walks `_CHARTER_NOUN_ALIASES`, so
        misspelling a KEY makes it iterate a map that matches no row and the whole thing
        passes having compared nothing — which the deletion sweep proved by retuning
        `"workspace"` to `'xpsltqbdf'` and surviving. A vacuous test over a table is this
        module's signature failure and it does not get to happen twice.

        The mapping is checked against the real parser: `ws` and `wt` have to be genuine
        aliases of `workspace` and `worktree`, not names somebody assumed.
        """
        self.assertEqual(hooks._CHARTER_NOUN_ALIASES,
                         {"workspace": "ws", "worktree": "wt"})
        p = cli.build_parser()
        for full, short in hooks._CHARTER_NOUN_ALIASES.items():
            with self.subTest(f"{full}/{short}"):
                self.assertIs(self._sub(p, full), self._sub(p, short),
                              f"`{short}` is not the same parser as `{full}`")

    def test_every_alias_row_exists_by_name(self) -> None:
        """The generated half of the table, asserted row by row rather than by walking the
        thing that generates it. A generator that produces nothing passes any test written
        as a loop over its own output."""
        for pair in (("ws", "remember"), ("ws", "note"), ("ws", "todo"), ("ws", "vision"),
                     ("wt", "abandon")):
            with self.subTest(pair):
                self.assertIn(pair, hooks._CHARTER_PROSE)
                self.assertEqual(hooks._CHARTER_PROSE[pair],
                                 hooks._CHARTER_PROSE[(dict(zip(
                                     hooks._CHARTER_NOUN_ALIASES.values(),
                                     hooks._CHARTER_NOUN_ALIASES.keys()))[pair[0]],
                                     pair[1])])

    def test_the_aliases_are_generated_from_the_rows_not_typed_out(self) -> None:
        """`workspace`/`ws` and `worktree`/`wt` are the same command. A hand-written second
        copy is a row that drifts the day somebody adds a verb to one of them."""
        checked = 0
        for full, short in hooks._CHARTER_NOUN_ALIASES.items():
            for (noun, verb), facts in sorted(hooks._CHARTER_PROSE.items()):
                if noun != full:
                    continue
                with self.subTest(f"{short} {verb}"):
                    self.assertEqual(hooks._CHARTER_PROSE.get((short, verb)), facts)
                checked += 1
        # the precondition, without which the loop above proves nothing
        self.assertEqual(checked, 5, "the alias rows were not generated at all")

    @staticmethod
    def _sub(parser, name):
        for a in parser._actions:
            if isinstance(a, argparse._SubParsersAction) and name in a.choices:
                return a.choices[name]
        return None

    @staticmethod
    def _takes_free_text(parser) -> bool:
        for a in parser._actions:
            if isinstance(a, (argparse._HelpAction, argparse._VersionAction,
                              argparse._StoreTrueAction, argparse._StoreFalseAction)):
                continue
            if a.choices or a.type not in (None, str) or a.nargs == 0:
                continue
            return True
        return False


class CommitMessagesAreStillTheDocumentedLimit(CharterProseCase):
    """#711 stays out of scope, and it is pinned here so the boundary is a decision rather
    than an omission.

    The measurement runs the other way from this guard's. On `main`, **29 of the last 30
    commit messages carry a backtick** (160 of 200), and **all 30 of the last 30 are
    multi-line** — which a single `-m "…"` cannot produce. The spelling that writes them,
    ``-m "$(cat <<'EOF' … EOF)"``, is itself a live `$(`, so a liveness-keyed guard on
    `git commit` would refuse the very form that makes the backticks inside it harmless.
    Its trigger would be the prescribed workflow, which is the inversion #371 deleted a
    guard for. `charter save` and `workspace save -m` write commit messages too, so they
    are out for the same reason and not by oversight.
    """

    def test_git_commit_is_not_covered(self) -> None:
        self.allowed('git commit -m "fix the `id -un` path"')

    def test_the_sibling_message_verbs_are_not_covered_either(self) -> None:
        for cmd in ('git tag -a v1 -m "`id -un`"',
                    'git merge --no-ff -m "`id -un`" topic',
                    'git notes add -m "`id -un`"',
                    'git stash push -m "`id -un`"'):
            with self.subTest(cmd):
                self.allowed(cmd)

    def test_charters_own_commit_message_commands_are_out_for_the_same_reason(self) -> None:
        for cmd in ('charter save "fix the `id -un` path"',
                    'charter workspace save w --message "`id -un`"',
                    'charter workspace rename a b -m "`id -un`"'):
            with self.subTest(cmd):
                self.allowed(cmd)

    def test_the_prescribed_commit_spelling_carries_a_live_substitution(self) -> None:
        """The fact that settles #711, stated as a test rather than as prose in a PR body.
        A guard on `git commit` would have to allow this line or refuse every commit this
        repository makes."""
        idiom = 'git commit -m "$(cat <<\'EOF\'\nsubject with a `code` span\nEOF\n)"'
        self.assertEqual(hooks._live_substitution(idiom), "$(")


class WhatItDoesNotClaim(CharterProseCase):
    """Per #370: the claim is small and the denial says so.

    It reads the SHAPE of a command line. It never sees the value — at `PreToolUse` the
    substitution has not run — so it must not be described as keeping a credential out of a
    file, and the sentence is held to that here.
    """

    def test_the_denial_says_it_reads_the_shape(self) -> None:
        why = self.deny_reason(INCIDENT)
        self.assertIn("SHAPE", why)
        self.assertIn("does not claim", why)

    def test_an_inert_substitution_is_not_refused(self) -> None:
        """One character's difference, and the whole calibration: prose that merely mentions
        a shell is prose."""
        self.allowed("charter persona remember 'run `env` before the build'")
        self.allowed('charter persona remember "set $HOME first"')
        self.allowed('charter persona remember "charter 0.54.0 shipped"')

    def test_it_is_not_gated_on_a_control_plane(self) -> None:
        """With `_leak_reason` and the forge guard, and for their reason: what it refuses is
        a fact about the SHELL. A plane-gated version would corrupt the same memory from one
        directory and refuse it from another."""
        old = config.HAS_CONTROL_PLANE
        config.HAS_CONTROL_PLANE = False
        try:
            self.assertIsNotNone(hooks._charter_substitution_hit(INCIDENT))
        finally:
            config.HAS_CONTROL_PLANE = old

    def test_it_does_not_raise_on_a_none_command(self) -> None:
        """`dispatch` runs the handler as a bare `rc = fn()`, so an exception takes the turn
        down instead of producing a verdict. The forge guard's sweep found exactly this.

        Only `_charter_substitution_hit` normalises — its `cmd or ""` is load-bearing,
        because the filter below it would be the thing that raises, and the sweep confirms
        it by killing the mutant that removes it. `_charter_prose_command` carries no
        fallback of its own, matching `_forge_prose_command`: it tolerates `None` because
        `_segment_argv` does, and that is asserted here rather than assumed.
        """
        self.assertIsNone(hooks._charter_substitution_hit(None))
        self.assertIsNone(hooks._charter_prose_command(None))
        self.assertEqual(list(hooks._segment_argv(None)), [])

    def test_the_spelling_is_reported_for_the_trace(self) -> None:
        """#289: the field that says WHICH shape tripped a guard is what makes "what fired
        this 335 times" answerable from the records."""
        self.assertEqual(hooks._charter_substitution_hit(INCIDENT)[0], "`")
        self.assertEqual(
            hooks._charter_substitution_hit('charter persona remember "$(date)"')[0], "$(")


if __name__ == "__main__":
    unittest.main()
