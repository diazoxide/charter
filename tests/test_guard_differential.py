"""A security branch may never deny LESS than the branch it merges into.

Three rounds of this guard each closed the instance they were shown and each shipped a new
one, because each round's test enumerated the inputs the round already knew about. This
test does not enumerate anything. It pins a **relation between two versions of the code**:

    for every command `origin/main` denies, this branch denies it too.

That is the property every one of those rounds actually needed and none of them had. Round
three added `{` and `}` to `_OPERATORS` and shipped a REGRESSION with them: `{` and `}` are
reserved words, and a shell recognises a reserved word only where a command word is
expected. Bash passes `cat { .charter/vaults/x.json` as ONE command — `cat: {: No such
file or directory` on stderr, the vault on stdout — while the guard read the `{` as a
boundary, split it into a reader with no operand and a path with no reader, and ALLOWed
what `origin/main` denies. Nothing in the branch's own tests could see that, because they
all asked "does the branch deny X", never "does the branch still deny what main denied".

**The corpus** is `tests/fixtures/guard_denied_by_main.txt`: every input, out of ~5,100
generated shell spellings, that `origin/main`'s own `charter/hooks.py` denies — recorded by
running that file, not by hand. Regenerate it by loading main's module beside this one::

    git show origin/main:charter/hooks.py > /tmp/main_hooks.py
    # import it as charter._hooks_main, call _leak_reason / _single_credential_reason
    # on each corpus input, and write the ones that return a reason

and keep the inputs: a corpus that shrinks when the code changes is a corpus that stops
being a floor. Adding a line is always safe; removing one needs the argument for why that
command may now be allowed.

**The divergences are asserted, not waived.** Both come from the same place: `main` glued
tokens together that a shell keeps apart, so some of what it denied was never one command.

1. This branch splits on a **newline**, which `main` never did — `shlex`'s default
   whitespace swallowed it, so a multi-line Bash call collapsed into one segment and every
   command after line 1 was invisible to every guard in the module. Splitting is strictly
   stronger in the direction that matters
   (`test_a_second_line_is_a_command_here_and_was_not_on_main`), and where it denies less it
   is because main had glued a reader on one line to a path on the next, which bash never
   does. The exemption below is a PREDICATE, not a list of blessed strings: an allowed
   command must contain a newline AND every one of its lines, judged alone, must also be
   allowed. If a line on its own is a leak, this test fails like any other regression.
2. On a command `shlex` cannot parse at all, this branch's fallback splits on the shell
   operators where `main` returned one whitespace-split segment — so
   `git ; $ <ssh-url> | ' if env` is a `git` with no URL here and a `git <ssh-url>` on main.
   Found by fuzzing this comparison; deliberately NOT given an exemption, because the only
   predicate broad enough to excuse it ("the branch sees more segments than main") would
   also have excused the `{` regression above. It is not in the corpus: it needs an
   unbalanced quote, which bash rejects too, so nothing runs either way.

The exemption predicate is kept as narrow as the argument for it. Anything wider stops
being a floor.
"""

from __future__ import annotations

import ast
import unittest
from pathlib import Path

from charter import hooks
from tests._isolation import PersonaIso

CORPUS = Path(__file__).with_name("fixtures") / "guard_denied_by_main.txt"

VAULT = ".charter/vaults/x.json"

#: The guards the corpus records a verdict for, by the name used in its first column.
_GUARDS = {
    "leak": lambda cmd: hooks._leak_reason(cmd, cwd="/plane"),
    "cred": hooks._single_credential_reason,
}


def _corpus() -> list[tuple[frozenset, str]]:
    rows = []
    for line in CORPUS.read_text().splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        kinds, _, literal = line.partition("\t")
        rows.append((frozenset(kinds.split(",")), ast.literal_eval(literal)))
    return rows


class DeniedByMainStaysDenied(PersonaIso):
    """The floor: nothing `origin/main` refuses may be allowed here."""

    def _denies(self, guard: str, cmd: str) -> bool:
        return bool(_GUARDS[guard](cmd))

    def test_the_corpus_is_actually_a_corpus(self):
        """A file that quietly emptied itself would make every assertion below vacuous."""
        rows = _corpus()
        self.assertGreater(len(rows), 800, "corpus too small to be a floor")
        self.assertTrue(any("leak" in k for k, _ in rows))
        self.assertTrue(any("cred" in k for k, _ in rows))

    def test_no_command_denied_by_main_is_allowed_here(self):
        weaker = []
        for kinds, cmd in _corpus():
            for guard in kinds:
                if self._denies(guard, cmd):
                    continue
                # The newline divergence, and only it: main glued two LINES into one
                # command. Allowed here only when no single line is a leak on its own.
                lines = [ln for ln in cmd.split("\n") if ln.strip()]
                if len(lines) > 1 and not any(self._denies(guard, ln) for ln in lines):
                    continue
                weaker.append((guard, cmd))
        self.assertEqual(weaker, [], "weaker than origin/main on these commands")

    def test_the_regression_that_started_this_round(self):
        """`{` is a reserved word, so bash passes it as an argument here and runs one
        command that prints the vault. Denied on main; the branch had allowed it."""
        for cmd in (f"cat {{ {VAULT}", f"cat }} {VAULT}", f"head {{ {VAULT}",
                    f"cat {{ {VAULT} ", f"grep -n x }} {VAULT}"):
            with self.subTest(cmd=cmd):
                self.assertIsNotNone(hooks._leak_reason(cmd, cwd="/plane"))

    def test_a_grouping_token_in_command_position_still_names_the_program(self):
        """The reason `{`/`}` were added at all: a group puts the real program at token 1.
        Making them positional must not give that back."""
        for cmd in (f"{{ cat {VAULT}; }}", f"( cat {VAULT} )",
                    f"echo hi; {{ cat {VAULT}; }}", f"echo hi | {{ cat {VAULT}; }}"):
            with self.subTest(cmd=cmd):
                self.assertIsNotNone(hooks._leak_reason(cmd, cwd="/plane"))

    def test_every_operator_mid_command_keeps_the_operand(self):
        """Generated from the module's own table, not listed: for each operator, a reader
        with the operator standing between it and the vault path must still deny. Either
        the shell passes the token as a word (`{`, `}`) or it refuses to parse the line at
        all (`(`, `)`); in neither case does the reader lose its operand."""
        for op in hooks._GROUPING:
            with self.subTest(op=op):
                self.assertIsNotNone(
                    hooks._leak_reason(f"cat {op} {VAULT}", cwd="/plane"))

    def test_a_second_line_is_a_command_here_and_was_not_on_main(self):
        """The divergence pays for itself in the direction that matters."""
        self.assertIsNotNone(hooks._leak_reason(f"echo hi\ncat {VAULT}", cwd="/plane"))
        self.assertIsNotNone(
            hooks._leak_reason(f"echo hi\n\ncat {VAULT}\necho bye", cwd="/plane"))


if __name__ == "__main__":
    unittest.main()
