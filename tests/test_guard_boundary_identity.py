"""A segment boundary is an operator the SHELL INTERPRETS — round three of one defect.

Round one matched a program by NAME. Round two matched a stream by PATH and a blank by
CHARACTER. This round is the same mistake at the tokenizer: `_segment_tokens` decided what
ended a command by comparing a token's TEXT against `_OPERATORS`, and posix `shlex` hands
back the identical one-character string `)` for a literal `\\)`, a quoted `')'` and a real
subshell close. So::

    cat \\) .charter/vaults/x.json

segmented as `cat` + `.charter/vaults/x.json`: the reader lost its operand, the operand
lost its reader, and the shipped hook ALLOWed a command that prints a vault. Verified
against a fabricated vault before the fix — `cat: ): No such file or directory` on stderr,
the JSON on stdout.

What actually makes a `)` a boundary is that the shell interprets it, and a quoted or
escaped character is by definition not interpreted. `_ShellLexer` reads that off the
tokenizer's own state machine (`_Tok.bare`) instead of re-deriving it from text that no
longer carries the information.

**These tests do not enumerate bad inputs.** Round two's blank-detection test listed four
whitespace strings and one new codepoint defeated it. Here the inputs are GENERATED from
`hooks._OPERATORS` and `hooks._PUNCTUATION_CHARS` — the module's own tables — crossed with
every quoting mechanism a POSIX shell has. Adding an operator to the table extends the
test; it cannot be added without being covered in both directions.

More spellings of the same class, found by asking "what gets through next?", are pinned
here too:

* a real **newline** is a command separator, and `shlex`'s default whitespace swallowed it,
  so a multi-line Bash call — most of them — collapsed into ONE segment and every command
  after the first line was invisible to every guard in this module. `_OPERATORS` has always
  listed `"\\n"`; the token never arrived. Live on `origin/main` as well as on the branch;
* `#` begins a comment only where a WORD begins. `shlex` honoured it mid-word and discarded
  the rest of the line, so `echo hi#; cat <vault>` — which runs the `cat` in bash —
  tokenized as a lone `echo hi`;
* the `&` in the REDIRECTION `>&` is not the control operator `&`. A glued punctuation run
  was cut into operator *characters*, so `cat 2>&1 <vault>` split at that `&` and stranded
  the vault path — the same "judged by its characters, not by what the shell makes of it"
  mistake one function over. `origin/main` denies that command; the branch allowed it, so
  it was a REGRESSION as well as a bypass;
* a **redirection is not the program and not an operand**. `< <vault> cat` prints the vault
  while token 0 is `<`, and `tee < <vault>` prints it while the program is in no reader
  list. The shell performs that open itself, before the program is execed — so the target
  of an input redirection is a read wherever it sits and whatever follows it. Live on
  `origin/main` too.
"""

from __future__ import annotations

import unittest

from charter import hooks

VAULT = ".charter/vaults/x.json"

#: Every way a POSIX shell can strip an operator character of its meaning. Each is a
#: FUNCTION of the operator text, so the set of operators and the set of quotings are
#: multiplied rather than listed out.
_QUOTINGS = (
    ("backslash", lambda t: "".join("\\" + c for c in t)),
    ("single-quoted", lambda t: "'" + t + "'"),
    ("double-quoted", lambda t: '"' + t + '"'),
    ("per-character single quotes", lambda t: "".join("'" + c + "'" for c in t)),
    ("quote-concatenation", lambda t: "''" + "".join("'" + c + "'" for c in t) + "''"),
    ("mixed quote styles", lambda t: "".join(
        ("'" + c + "'") if i % 2 else ('"' + c + '"') for i, c in enumerate(t))),
)


class TestAQuotedOperatorIsAWord(unittest.TestCase):
    """Generated over `_OPERATORS` x `_QUOTINGS`. A literal operator inside a reader's argv
    is that reader's OPERAND, so the vault path stays where the guard can see it."""

    def test_no_quoted_operator_strands_a_readers_operand(self):
        for op in hooks._OPERATORS:
            for name, quote in _QUOTINGS:
                cmd = f"cat {quote(op)} {VAULT}"
                with self.subTest(operator=op, quoting=name):
                    segs = hooks._segment_argv(cmd)
                    # The property, not a spelling: SOME segment names `cat` and carries
                    # the vault path. Which token the quoted operator lands on is the
                    # tokenizer's business; that the reader keeps its operand is ours.
                    self.assertTrue(
                        any(seg[0] == "cat" and VAULT in seg for seg in segs),
                        f"{cmd!r} -> {segs!r}")

    def test_no_quoted_operator_turns_the_leak_guard_off(self):
        for op in hooks._OPERATORS:
            for name, quote in _QUOTINGS:
                cmd = f"cat {quote(op)} {VAULT}"
                with self.subTest(operator=op, quoting=name):
                    self.assertIsNotNone(hooks._leak_reason(cmd), cmd)

    def test_the_reveal_arm_survives_it_too(self):
        """`charter secret get v k \\) --reveal` was ALLOWed: the `)` ended the segment
        before `--reveal` was reached."""
        for op in hooks._OPERATORS:
            for name, quote in _QUOTINGS:
                cmd = f"charter secret get v k {quote(op)} --reveal"
                with self.subTest(operator=op, quoting=name):
                    self.assertIsNotNone(hooks._leak_reason(cmd), cmd)

    def test_a_followed_cd_is_not_shaken_off_either(self):
        """Round one taught this guard to follow `cd`. A phantom boundary between the `cd`
        and the reader put them in different segments and undid it."""
        for op in hooks._OPERATORS:
            for name, quote in _QUOTINGS:
                cmd = f"cd .charter/vaults && cat {quote(op)} x.json"
                with self.subTest(operator=op, quoting=name):
                    self.assertIsNotNone(hooks._leak_reason(cmd), cmd)


class TestABareOperatorIsStillABoundary(unittest.TestCase):
    """The other direction, generated from the same table. A fix that stopped splitting on
    real operators would hide every command after the first — which is the failure the
    quoting fix must not buy.

    These two tests used to run over the whole of `_OPERATORS`, grouping tokens included,
    and that is how this file shipped a REGRESSION while looking covered: `cat a { echo b`
    is one command to bash, not two, and asserting the split made the guard strand a
    reader's operand (see `tests/test_guard_differential.py`). They now generate from
    `_CONTROL_OPERATORS` — the tokens that really are a boundary wherever they stand — and
    the grouping tokens are asserted below in the position a shell actually recognises.
    """

    def test_every_control_operator_ends_a_segment(self):
        for op in hooks._CONTROL_OPERATORS:
            with self.subTest(operator=op):
                segs = hooks._segment_argv(f"cat a {op} echo b")
                self.assertGreater(len(segs), 1, f"{op!r} -> {segs!r}")
                self.assertNotIn(VAULT, [t for seg in segs for t in seg])

    def test_a_real_second_command_after_a_real_operator_is_still_seen(self):
        for op in hooks._CONTROL_OPERATORS:
            with self.subTest(operator=op):
                cmd = f"echo hi {op} cat {VAULT}"
                self.assertIsNotNone(hooks._leak_reason(cmd), cmd)

    def test_a_grouping_token_is_a_boundary_where_a_shell_recognises_one(self):
        """In COMMAND POSITION the group is stripped and the real program is named."""
        for cmd in (f"echo hi; {{ cat {VAULT}; }}", f"echo hi; ( cat {VAULT} )",
                    f"{{ cat {VAULT}; }}", f"( cat {VAULT} )",
                    f"echo hi | {{ cat {VAULT}; }}"):
            with self.subTest(cmd=cmd):
                self.assertIsNotNone(hooks._leak_reason(cmd), cmd)

    def test_a_grouping_token_mid_command_is_an_argument(self):
        """And nowhere else. `cat a { echo b` is one command to bash — `bash -n` accepts
        it — so the segment stays whole and `cat` keeps every operand it was given."""
        self.assertEqual(hooks._segment_argv("cat a { echo b"),
                         [["cat", "a", "{", "echo", "b"]])
        for op in hooks._GROUPING:
            with self.subTest(operator=op):
                self.assertIsNotNone(hooks._leak_reason(f"cat {op} {VAULT}"))


class TestTheLexerIsWhereQuotingIsKnown(unittest.TestCase):
    """`_Tok.bare` is read off `shlex`'s own state machine — the one layer that still knows
    what was quoted. Pinned per punctuation character so the flag cannot quietly become a
    constant, which would make every test above pass for the wrong reason."""

    def chars(self):
        return sorted(set(hooks._PUNCTUATION_CHARS) | set("{}"))

    def test_an_unquoted_punctuation_character_is_bare(self):
        for c in self.chars():
            with self.subTest(char=c):
                toks = hooks._lex(f"a {c} b")
                self.assertTrue(any(t.text == c and t.bare for t in toks),
                                f"{c!r} -> {toks!r}")

    def test_an_escaped_or_quoted_punctuation_character_is_not_bare(self):
        for c in self.chars():
            for name, quote in _QUOTINGS:
                with self.subTest(char=c, quoting=name):
                    toks = hooks._lex(f"a {quote(c)} b")
                    self.assertFalse(any(t.bare for t in toks[1:-1]),
                                     f"{quote(c)!r} -> {toks!r}")

    def test_an_empty_argument_is_a_token_and_not_end_of_input(self):
        """`grep -sa '' \\) <vault>` was one of the reported spellings; `''` must not be
        read as EOF or the rest of the command disappears."""
        self.assertEqual([t.text for t in hooks._lex("grep -sa '' x")],
                         ["grep", "-sa", "", "x"])


class TestANewlineSeparatesCommands(unittest.TestCase):
    """A newline does what `;` does. The equivalence is the assertion — not a list of
    multi-line commands — so it holds for shapes nobody thought to write down."""

    SHAPES = (
        "echo hi ; cat " + VAULT,
        "cd .charter/vaults ; cat x.json",
        "echo one ; echo two ; cat " + VAULT,
        "charter secret get v k ; echo done",
    )

    def test_a_newline_segments_exactly_as_a_semicolon_does(self):
        for shape in self.SHAPES:
            with self.subTest(shape=shape):
                self.assertEqual(hooks._segment_argv(shape.replace(" ; ", "\n")),
                                 hooks._segment_argv(shape))

    def test_a_newline_denies_exactly_as_a_semicolon_does(self):
        for shape in self.SHAPES:
            with self.subTest(shape=shape):
                self.assertEqual(bool(hooks._leak_reason(shape.replace(" ; ", "\n"))),
                                 bool(hooks._leak_reason(shape)))

    def test_the_reported_shape_of_a_multi_line_bash_call(self):
        self.assertIsNotNone(hooks._leak_reason("echo hi\ncat " + VAULT))

    def test_a_newline_inside_quotes_is_still_data(self):
        """The boundary rule is about INTERPRETED newlines, like every other operator: a
        newline inside a quoted argument is a character, and the reader keeps its operand."""
        for cmd in (f"cat '\n' {VAULT}", f'cat "a\nb" {VAULT}'):
            with self.subTest(cmd=cmd):
                segs = hooks._segment_argv(cmd)
                self.assertTrue(any(seg[0] == "cat" and VAULT in seg for seg in segs), segs)
                self.assertIsNotNone(hooks._leak_reason(cmd))


class TestACommentStartsWhereAWordStarts(unittest.TestCase):
    def test_a_hash_inside_a_word_is_an_ordinary_character(self):
        """bash runs the `cat`; shlex discarded the rest of the line."""
        self.assertEqual(hooks._segment_argv("echo hi#; cat v"),
                         [["echo", "hi#"], ["cat", "v"]])
        self.assertIsNotNone(hooks._leak_reason("echo hi#; cat " + VAULT))

    def test_a_comment_ends_at_the_newline_and_not_after_it(self):
        """`shlex` ends a comment with `readline()`, which ate the separator too — so the
        comment bypass came back the moment newlines became boundaries."""
        self.assertIsNotNone(hooks._leak_reason("echo a # note\ncat " + VAULT))
        self.assertIsNotNone(hooks._leak_reason("# leading comment\ncat " + VAULT))

    def test_a_real_comment_is_still_a_comment(self):
        self.assertEqual(hooks._segment_argv("cat v # a trailing note"), [["cat", "v"]])
        self.assertIsNone(hooks._leak_reason("echo hi # mentions " + VAULT))


#: What a POSIX shell does with each redirection, written out here INDEPENDENTLY of the
#: module's own tables. The first version of this test asked `op in hooks._REDIRECT_READS`
#: and so asserted nothing at all: the mutation `_REDIRECT_READS = _REDIRECTIONS` widened
#: the module and the expectation together, and SURVIVED. A test may only be generated from
#: a table it does not also take its answer from.
#:
#: A finite closed grammar is the one place a written-out set is the right shape — unlike
#: the blanks of round two, no new POSIX redirection is going to appear. What keeps it
#: honest is `test_every_redirection_has_a_stated_meaning`, which fails if the module's
#: table and this one drift apart in either direction.
OPENS_A_PATH_FOR_READING = {
    "<": True,        # opens the named file as stdin
    "<>": True,       # opens it for reading and writing
    "<<": False,      # the following word is a heredoc DELIMITER, not a file
    "<<<": False,     # the following word is here-string DATA, not a file
    "<&": False,      # duplicates a descriptor that is already open
    ">": False,       # truncate-for-writing: not a read
    ">>": False,      # append-for-writing: not a read
    ">|": False,      # truncate even under `noclobber`: not a read
    ">&": False,      # duplicates a descriptor for writing
}


class TestARedirectionIsNotAControlOperator(unittest.TestCase):
    """`>&` and `>|` are single shell tokens. Splitting a punctuation run character by
    character turned their `&` and `|` into boundaries, so `cat 2>&1 <vault>` lost its
    operand — allowed here, DENIED on `origin/main`, i.e. a regression as well as a bypass.

    Generated from `hooks._REDIRECTIONS`, the table the split regex and both recognisers are
    all built from. A redirection cannot be added to the module without arriving here."""

    def test_no_redirection_is_cut_into_a_control_operator(self):
        for op in hooks._REDIRECTIONS:
            with self.subTest(redirection=op):
                pieces = [p for p in hooks._OPERATOR_SPLIT_RE.split(op) if p]
                self.assertEqual(pieces, [op], f"{op!r} -> {pieces!r}")

    def test_no_redirection_strands_a_readers_operand(self):
        for op in hooks._REDIRECTIONS:
            for cmd in (f"cat 2{op}1 {VAULT}", f"cat {op}1 {VAULT}"):
                with self.subTest(cmd=cmd):
                    segs = hooks._segment_argv(cmd)
                    self.assertTrue(
                        any(seg[0] == "cat" and VAULT in seg for seg in segs),
                        f"{cmd!r} -> {segs!r}")
                    self.assertIsNotNone(hooks._leak_reason(cmd), cmd)

    def test_the_control_operators_of_the_same_spelling_still_bound(self):
        """The fix must not buy its correctness by making `&` or `|` stop separating."""
        for cmd in (f"echo hi & cat {VAULT}", f"echo hi | cat {VAULT}",
                    f"echo hi |& cat {VAULT}", f"echo hi&cat {VAULT}"):
            with self.subTest(cmd=cmd):
                self.assertIsNotNone(hooks._leak_reason(cmd), cmd)


class TestARedirectionIsTheShellsOwnOpen(unittest.TestCase):
    """A redirection is neither the program nor an operand, and it may sit in FRONT of the
    command. `< <vault> cat` printed the vault with token 0 `<`; `tee < <vault>` printed it
    with a program in no reader list. The shell opens the file before the program exists, so
    the rule cannot be "which program follows" — that is guard-by-name again."""

    def test_every_redirection_has_a_stated_meaning(self):
        """A redirection cannot be added to the module without someone writing down, here,
        whether it opens a path — which is the question the guard turns on."""
        self.assertEqual(set(hooks._REDIRECTIONS), set(OPENS_A_PATH_FOR_READING))

    def test_only_an_input_redirection_records_a_read(self):
        """The read set is a real SUBSET: `<<` names a heredoc delimiter, `<<<` here-string
        data, `<&` a descriptor that is already open — none of the three is a path."""
        for op, opens in OPENS_A_PATH_FOR_READING.items():
            for fd in ("", "2"):
                with self.subTest(redirection=fd + op):
                    self.assertEqual(hooks._redirect_reads([fd + op, VAULT, "cat"]),
                                     [VAULT] if opens else [])

    def test_an_input_redirection_is_a_read_from_either_side(self):
        for op in hooks._REDIRECT_READS:
            for cmd in (f"{op} {VAULT} cat",          # in front: token 0 is not the program
                        f"cat {op} {VAULT}",
                        f"tee {op} {VAULT}",          # a program in no reader list
                        f"2{op} {VAULT} cat",         # with a descriptor number
                        f"{op} {VAULT} true"):        # no reader named anywhere at all
                with self.subTest(cmd=cmd):
                    self.assertIsNotNone(hooks._leak_reason(cmd), cmd)

    def test_a_leading_redirection_does_not_hide_the_program(self):
        """The `--reveal` arm reads the program too, and a leading redirection displaced it."""
        self.assertIsNotNone(
            hooks._leak_reason("< /dev/null charter secret get v k --reveal"))
        self.assertEqual(
            hooks._split_env(["<", "/dev/null", "cat", "x"])[0], "cat")
        self.assertEqual(
            hooks._split_env(["2>", "/dev/null", "env", "cat", "x"])[0], "cat")

    def test_a_redirection_relative_to_a_followed_cd(self):
        self.assertIsNotNone(hooks._leak_reason("cd .charter/vaults && cat < x.json"))
        self.assertIsNotNone(hooks._leak_reason("cd .charter/vaults && < x.json cat"))

    def test_an_ordinary_redirection_is_still_allowed(self):
        for cmd in ("cat < notes.txt", "< notes.txt cat", "sort < notes.txt > out.txt",
                    "cat notes.txt 2>/dev/null"):
            with self.subTest(cmd=cmd):
                self.assertIsNone(hooks._leak_reason(cmd), cmd)


class TestAWrapperThatOpensAFileIsAReader(unittest.TestCase):
    """`docs/hooks.md` said a wrapper "does not change what the program is" and named
    `xargs` in the list. That was not true of `xargs -a <vault> echo`, which prints the
    vault while the only program on the line is `echo` — the doc claimed more than the code
    delivered, which is the defect this audit exists to remove.

    Generated from `_WRAPPER_READ_FLAGS` so a wrapper added to that table is covered by
    being added, not by someone remembering to write a case for it."""

    def test_every_wrapper_read_flag_is_treated_as_a_read(self):
        for wrapper, flags in hooks._WRAPPER_READ_FLAGS.items():
            for flag in flags:
                for cmd in (f"{wrapper} {flag} {VAULT} echo",     # separate value
                            f"{wrapper} {flag}={VAULT} echo",     # attached value
                            f"{wrapper} {flag} {VAULT}"):         # no program at all
                    with self.subTest(cmd=cmd):
                        self.assertIsNotNone(hooks._leak_reason(cmd), cmd)

    def test_a_wrapper_read_is_relative_to_a_followed_cd(self):
        self.assertIsNotNone(
            hooks._leak_reason("cd .charter/vaults && xargs -a x.json echo"))

    def test_an_ordinary_file_is_still_allowed(self):
        self.assertIsNone(hooks._leak_reason("xargs -a notes.txt echo"))

    def test_an_optional_attached_value_flag_does_not_eat_the_program(self):
        """`xargs -e cat <vault>` was ALLOWed because `-e` was listed as taking a SEPARATE
        value: `cat` became the flag's value and the vault path became the program."""
        self.assertIsNotNone(hooks._leak_reason(f"xargs -e cat {VAULT}"))
        self.assertEqual(hooks._split_env(f"xargs -e cat {VAULT}".split())[0], "cat")
        # `-E` really does take a separate value, and must keep doing so.
        self.assertEqual(hooks._split_env(f"xargs -E EOF cat {VAULT}".split())[0], "cat")


class TestNothingThatWorkedStoppedWorking(unittest.TestCase):
    """The guards this branch already fixed. Each was a live bypass; a boundary rule that
    took quoting into account had every chance to undo one of them."""

    def test_a_group_still_puts_the_reader_at_token_zero(self):
        self.assertEqual(hooks._segment_argv("{ cat v; }"), [["cat", "v"]])
        self.assertEqual(hooks._segment_argv("( cat v )"), [["cat", "v"]])
        self.assertIsNotNone(hooks._leak_reason("{ cat " + VAULT + "; }"))

    def test_a_substitution_still_yields_both_readings(self):
        self.assertEqual(hooks._segment_argv("cat $(echo v)"),
                         [["echo", "v"], ["cat", "$", "echo", "v"]])
        self.assertEqual(hooks._segment_argv("cat `echo v`")[0], ["echo", "v"])

    def test_a_glued_punctuation_run_is_still_operators(self):
        self.assertEqual(hooks._segment_argv("( true );cat v"), [["true"], ["cat", "v"]])

    def test_an_escaped_dollar_does_not_make_a_substitution(self):
        """The `$` that turns `(` into a substitution has to be interpreted too — there is
        no inner `echo` segment here.

        The whole line stays ONE segment, which is what bash's own answer deserves: `bash
        -n -c 'cat \\$(echo v)'` is a syntax error, so nothing about it runs, and holding
        `cat`'s operands together is the conservative reading of a line no shell accepts.
        This used to assert a two-segment split — a boundary the shell never draws, and the
        same fiction that let `cat { <vault>` through."""
        segs = hooks._segment_argv(r"cat \$(echo v)")
        self.assertNotIn(["echo", "v"], segs)
        self.assertEqual(segs, [["cat", "$", "(", "echo", "v", ")"]])

    def test_prose_in_a_quoted_argument_is_still_prose(self):
        self.assertEqual(hooks._segment_argv("echo " + repr("a ; b")), [["echo", "a ; b"]])
        self.assertIsNone(hooks._leak_reason("git commit -m " + repr("about --reveal")))

    def test_the_unparseable_path_still_reports_itself(self):
        self.assertTrue(hooks._segment_argv_parsed("cat v")[1])
        self.assertFalse(hooks._segment_argv_parsed("cat 'v")[1])
        self.assertEqual(hooks._segment_argv("echo 'x ; cat v"),
                         [["echo", "'x"], ["cat", "v"]])

    def test_the_one_credential_guard_keeps_its_coverage(self):
        """`git push \\) <ssh-url>` lost its hit to the same phantom boundary."""
        for op in hooks._OPERATORS:
            for name, quote in _QUOTINGS:
                with self.subTest(operator=op, quoting=name):
                    self.assertIsNotNone(hooks._single_credential_hit(
                        f"git push {quote(op)} git@github.com:o/r.git"))

    def test_empty_is_empty(self):
        self.assertEqual(hooks._segment_argv(""), [])


if __name__ == "__main__":
    unittest.main()
