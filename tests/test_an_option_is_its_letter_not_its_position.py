"""A short option is its LETTER, wherever the letter sits in the token (#556).

`hooks._split_env_chdir` matched a wrapper's value-taking flags with `tok.startswith(flag)`,
which is a spelling and not the property. getopt BUNDLES short options — `-iC<dir>` is
`-i -C <dir>` — so the flag the guard already modelled in three other forms was sitting one
letter in, matched nothing, fell through the generic "starts with `-`, skip it" branch, and
its value was never recovered.

## The measurement

A plane built by `charter init`, a fabricated value in `.charter/vaults/x.json`, every
command fed to the real `charter hook pretooluse` (the decision is nested under
`hookSpecificOutput`; empty stdout is an allow):

    env -C .charter/vaults cat x.json        -> DENY   (control)
    env -C.charter/vaults cat x.json         -> DENY   (control)
    env --chdir=.charter/vaults cat x.json   -> DENY   (control)
    env -iC.charter/vaults cat x.json        -> ALLOW  <-- the bypass
    env -iC .charter/vaults cat x.json       -> ALLOW  <-- and its separated twin
    env -viC.charter/vaults cat x.json       -> ALLOW
    env -iS'cat .charter/vaults/x.json'      -> ALLOW  <-- found by the sweep below

and each of the four really runs: `env -iC.charter/vaults pwd` answers the vault directory,
`env -viC.charter/vaults cat x.json` and `env -iS'cat …'` both print the fabricated value.
Three spellings of one flag denied, which is exactly what a spelling-shaped guard looks like
from the inside.

## The property, and what it costs

**Walk the letters; consult the value table first for every one of them; end the walk at a
letter neither table places.** That is `hooks._wrapper_option`, and the two tables it reads
are `_WRAPPER_VALUE_FLAGS` (letters that take a value) and `_WRAPPER_NOVALUE_LETTERS`
(letters that do not). Both are per-wrapper, because `env -i` takes nothing while
`stdbuf -i` takes a value and one flat set would swallow the program on `env -i cat <vault>`
— the fail-open the value table was written per-wrapper to avoid.

**A letter in neither table ends the walk, and since this round the end of a walk is no
longer silent.** An option this grammar could not place leaves the program unnamed: a value
flag nobody listed is indistinguishable from one that takes nothing, so `_split_env_chdir`
reports the rest of the segment as files the command may open and the leak guard asks
`_names_a_vault_path` of them. That is what keeps a short table a false NEGATIVE rather than
a bypass — and the sweep found two live fail-opens of exactly that kind, both now in the
value table as well:

    env -P /bin cat .charter/vaults/x.json   -> ALLOW  (BSD `env -P utilpath`)
    sudo -T 5 cat .charter/vaults/x.json     -> ALLOW  (`sudo --command-timeout`)

`/bin` and `5` were named as the program, the `cat` was an argument of nothing, and the
vault was an operand of nothing. A longer list of spellings would have closed those two and
left the next one open; the fallback is what makes the list an optimisation instead of the
guard.

`tests/_planeguard._bundled_option` has walked a bundle since `bash -lc` was found to be a
way past the spawn guard, and its docstring gives the reason in one line — *"a list of
SPELLINGS is only ever as long as the last audit"*. This is that walk, on the other side of
the module.
"""

from __future__ import annotations

import unittest

from charter import hooks
from tests._isolation import PersonaIso, run_hook

VAULT = ".charter/vaults/x.json"
VAULTS = ".charter/vaults"

#: `(command, must be denied)` — the measurement above, verbatim. The controls are not
#: padding: they are the three spellings whose denial made a hand probe conclude the flag
#: was covered, and without them a later refactor can trade one spelling for another and
#: stay green.
BUNDLE_ROWS = (
    (f"cat {VAULT}", True),                                  # the control
    (f"env -C {VAULTS} cat x.json", True),                   # control: separated
    (f"env -C{VAULTS} cat x.json", True),                    # control: glued, first
    (f"env --chdir={VAULTS} cat x.json", True),              # control: long form
    (f"env -iC{VAULTS} cat x.json", True),                   # #556
    (f"env -iC {VAULTS} cat x.json", True),                  # #556, separated value
    (f"env -viC{VAULTS} cat x.json", True),                  # #556, two letters deep
    (f"env -iS'cat {VAULT}'", True),                         # the sweep's find
    (f"sudo -bD{VAULTS} cat x.json", True),
    (f"xargs -0a{VAULT} echo", True),
    (f"env -P /bin cat {VAULT}", True),                       # the unplaced-flag fallback
    (f"sudo -T 5 cat {VAULT}", True),
)


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


class TestEveryBundledSpellingOfOneVaultRead(unittest.TestCase):
    """`_leak_reason`, the guard's own answer, over the whole table."""

    def test_the_table_still_holds_its_controls(self):
        """A table that quietly loses the spellings that already denied stops being able to
        show that a fix traded one spelling for another."""
        cmds = [c for c, _ in BUNDLE_ROWS]
        self.assertEqual(len(BUNDLE_ROWS), 12)
        for control in (f"env -C {VAULTS} cat x.json", f"env -C{VAULTS} cat x.json",
                        f"env --chdir={VAULTS} cat x.json"):
            self.assertIn(control, cmds)

    def test_every_row_is_denied(self):
        for cmd, denied in BUNDLE_ROWS:
            with self.subTest(cmd=cmd):
                reason = hooks._leak_reason(cmd)
                self.assertEqual(reason is not None, denied, cmd)
                self.assertIn("reads a vault/secret file directly", reason or "")


class TestTheSameTableThroughTheRealHook(PersonaIso):
    """The same rows through `hooks.pretooluse`, which is where they were measured — the
    JSON a harness acts on, nested under `hookSpecificOutput` (reading `permissionDecision`
    from the root answers `None` for every input and looks exactly like a guard that has
    stopped working)."""

    def test_every_row_is_denied_end_to_end(self):
        for cmd, denied in BUNDLE_ROWS:
            with self.subTest(cmd=cmd):
                r = run_hook(hooks.pretooluse,
                             {"tool_name": "Bash", "tool_input": {"command": cmd},
                              "session_id": "s"})
                self.assertEqual(_decision(r), "deny" if denied else None, cmd)


class TestTheWalkReadsLettersAndNotTokens(unittest.TestCase):
    """Stated as the PARSE rather than as the verdict, because a denial arriving from
    somewhere else — the raw-string arm, the unplaced-flag fallback — can stand in for the
    walk being right and hide it going wrong."""

    def test_a_bundled_chdir_comes_back_as_the_chdir(self):
        for tok in ("-iC" + VAULTS, "-viC" + VAULTS, "-0iC" + VAULTS):
            with self.subTest(tok=tok):
                _p, _e, _a, chdir, _r = hooks._split_env_chdir(
                    ["env", tok, "cat", "x.json"])
                self.assertEqual(chdir, VAULTS)

    def test_a_bundled_value_may_also_be_the_next_token(self):
        """`env -iC <dir>` really does chdir — verified against this machine's `env`, which
        answers the named directory for `env -iC /tmp pwd`."""
        _p, _e, _a, chdir, _r = hooks._split_env_chdir(
            ["env", "-iC", VAULTS, "cat", "x.json"])
        self.assertEqual(chdir, VAULTS)

    def test_a_bundled_file_flag_is_still_a_file_the_wrapper_opens(self):
        """`xargs -a <file>` is not wrapping a read, it IS the read — the only program on
        the line is `echo`. Bundling it behind `-0` must not lose that."""
        _p, _e, _a, _c, reads = hooks._split_env_chdir(["xargs", "-0a" + VAULT, "echo"])
        self.assertIn(VAULT, reads)

    def test_a_bundled_split_string_is_still_unpacked(self):
        """`env -iS'cat <vault>'` — the packed command is behind a bundled `-i`. Skipping it
        leaves an empty argv and the guard sees no program at all, which is the fail-open
        `_SPLIT_STRING_FLAGS` exists against."""
        prog, _e, argv, _c, _r = hooks._split_env_chdir(["env", "-iScat " + VAULT])
        self.assertEqual(prog, "cat")
        self.assertEqual(argv, ["cat", VAULT])

    def test_a_packed_command_that_will_not_tokenize_still_names_its_reader(self):
        """`env -S` takes a STRING, and a string can be unbalanced. `shlex.split` raises
        `ValueError` on `cat '<vault>`, and without the fallback to a plain whitespace split
        that exception leaves the leak guard — a guard that raises is a guard that is not
        there. Found by `tools/sweep.py`, which narrowed the `except` and watched nothing
        notice."""
        import shlex
        packed = "cat '" + VAULT
        with self.assertRaises(ValueError):
            shlex.split(packed)
        prog, _e, argv, _c, _r = hooks._split_env_chdir(["env", "-S" + packed])
        self.assertEqual(prog, "cat")
        self.assertIn(VAULT, " ".join(argv))
        self.assertIsNotNone(hooks._leak_reason(f'env -S"{packed}"'))

    def test_the_walk_stops_at_a_value_taking_letter(self):
        """`env -Ci /tmp pwd` answers *"cannot change directory to 'i'"* — the `i` is `-C`'s
        VALUE, not a bundled flag. A walk that kept going past a value-taking letter would
        read somebody's data as options."""
        name, value, wants_next, placed = hooks._wrapper_option("env", "-Ci")
        self.assertEqual((name, value, wants_next, placed), ("-C", "i", False, True))

    def test_a_letter_neither_table_places_ends_the_walk(self):
        """Not guessed at in either direction: guessing "takes a value" swallows the
        program, guessing "takes none" walks into somebody's data."""
        self.assertEqual(hooks._wrapper_option("env", "-iq"), ("", "", False, False))
        self.assertEqual(hooks._wrapper_option("env", "-q"), ("", "", False, False))

    def test_a_flag_that_takes_no_value_still_takes_none(self):
        """The fail-open the per-wrapper tables exist for: `env -i` must not swallow the
        `cat`, and this is the row that catches a walk that got greedy."""
        prog, _e, _a, _c, _r = hooks._split_env_chdir(["env", "-i", "cat", VAULT])
        self.assertEqual(prog, "cat")
        self.assertEqual(hooks._wrapper_option("env", "-i"), ("", "", False, True))

    def test_end_of_options_is_placed_rather_than_unknown(self):
        """`--` means "the options stop here" and nothing else. Leaving it to the walk would
        report the whole rest of the segment as reachable for a token that names no flag."""
        self.assertEqual(hooks._wrapper_option("env", "--"), ("", "", False, True))

    def test_a_long_option_is_still_read_as_a_long_form(self):
        """The letter walk may not start eating long options because they happen to contain
        a short flag's letters — `--chdir` holds `C`'s neighbours and is not a bundle."""
        name, value, _w, placed = hooks._wrapper_option("env", "--chdir=" + VAULTS)
        self.assertEqual((name, value, placed), ("--chdir", VAULTS, True))
        self.assertEqual(hooks._wrapper_option("env", "--not-a-flag"),
                         ("", "", False, False))

    def test_a_flag_whose_value_is_missing_does_not_take_one(self):
        """`env -C` at the end of a segment has no next token to take. Popping one anyway
        raises `IndexError` inside the leak guard, and a guard that raises is a guard that
        is not there. Found by the hand-check, which killed the `and toks` conjunct and
        watched nothing notice."""
        self.assertEqual(hooks._split_env_chdir(["env", "-C"]), ("", [], [], "", []))
        self.assertIsNone(hooks._leak_reason("env -C"))
        self.assertIsNone(hooks._leak_reason("sudo -u"))

    def test_a_wrapper_with_no_value_flags_at_all_is_still_walked(self):
        """`_WRAPPER_VALUE_FLAGS` has no entry for these, and the walk still runs for them:
        the `()` default is what makes `takes` a sequence rather than `None`. Without it
        `nohup -q cat <vault>` raises `TypeError` out of the leak guard — which the hand-
        check found and no test did, because every wrapper the suite exercised happened to
        be in the table."""
        for wrapper in ("nohup", "setsid", "command", "builtin", "unbuffer"):
            with self.subTest(wrapper=wrapper):
                self.assertNotIn(wrapper, hooks._WRAPPER_VALUE_FLAGS)
                self.assertIn(wrapper, hooks._WRAPPERS)
                self.assertIsNotNone(hooks._leak_reason(f"{wrapper} -q cat {VAULT}"))


class TestTheWalkNeedsNoFilterOfItsOwn(unittest.TestCase):
    """Two lines that guarded the walk were **deleted** rather than left unpinned.

    The hand-check killed each of them and nothing noticed; a differential over 6.9M
    `(wrapper, token)` pairs then showed why — neither alone nor both together changes a
    single answer. So they are gone, and the two facts that make them unnecessary are
    asserted here instead, because "it happens to be equivalent today" is how a deleted
    guard becomes a bypass tomorrow.
    """

    def test_no_no_value_table_places_a_dash(self):
        """This is what makes the deleted `tok.startswith("--")` early return unnecessary: a
        long option's first walked character is the second `-`, and a `-` in one of these
        strings would make the walk step over it and start reading a long option's letters
        as a bundle."""
        for wrapper, letters in hooks._WRAPPER_NOVALUE_LETTERS.items():
            with self.subTest(wrapper=wrapper):
                self.assertNotIn("-", letters)

    def test_no_value_table_holds_a_bare_double_dash(self):
        """And this is what makes the deleted `len(f) == 2 and not f.startswith("--")`
        filter unnecessary: the walk asks `"-" + ch in takes`, which is two characters, so
        the only spelling the filter could ever have removed is `--` itself."""
        for wrapper, flags in hooks._WRAPPER_VALUE_FLAGS.items():
            with self.subTest(wrapper=wrapper):
                self.assertNotIn("--", flags)
        self.assertNotIn("--", hooks._SPLIT_STRING_FLAGS)

    def test_a_long_option_ends_the_walk_unplaced(self):
        """The behaviour the two deletions have to preserve, stated directly."""
        for wrapper in sorted(hooks._WRAPPER_NOVALUE_LETTERS):
            for tok in ("--nonesuch", "--chdir-ish", "---", "--0i"):
                with self.subTest(wrapper=wrapper, tok=tok):
                    self.assertEqual(hooks._wrapper_option(wrapper, tok),
                                     ("", "", False, False))


class TestTheTwoLetterTablesCannotDisagree(unittest.TestCase):
    """The value table is consulted FIRST for every letter in the walk, so a letter in both
    tables would behave as value-taking and the no-value entry would be dead. That is a
    silent way for an audit to be wrong, so the tables are held apart instead."""

    def test_no_letter_is_in_both_tables_for_the_same_wrapper(self):
        for wrapper, letters in hooks._WRAPPER_NOVALUE_LETTERS.items():
            valued = {f[1] for f in hooks._WRAPPER_VALUE_FLAGS.get(wrapper, ())
                      if len(f) == 2 and not f.startswith("--")}
            with self.subTest(wrapper=wrapper):
                self.assertEqual(valued & set(letters), set(),
                                 f"{wrapper}: a letter in both tables — the no-value entry "
                                 f"is dead code and the audit that added it is wrong")

    def test_every_no_value_table_names_a_wrapper_this_module_strips(self):
        """A table keyed on a name `_WRAPPERS` does not hold is never read, and a walk that
        is never read looks exactly like one that works."""
        for wrapper in hooks._WRAPPER_NOVALUE_LETTERS:
            self.assertIn(wrapper, hooks._WRAPPERS, wrapper)


class TestAnOptionThisGrammarCannotPlaceIsNotALicenceToStopLooking(unittest.TestCase):
    """The fallback, stated on its own — because the two rows it was found by are now in the
    value table, and a fix that only added them would leave the NEXT missing letter a bypass
    rather than a false negative.

    A flag charter has never heard of, taking a value it has never heard of, standing in
    front of a vault read."""

    def test_an_unknown_wrapper_flag_leaves_the_rest_of_the_segment_reachable(self):
        _p, _e, _a, _c, reads = hooks._split_env_chdir(
            ["env", "--not-a-real-flag", "/bin", "cat", VAULT])
        self.assertIn(VAULT, reads)
        self.assertIsNotNone(hooks._leak_reason(f"env --not-a-real-flag /bin cat {VAULT}"))

    def test_a_flag_the_sweep_added_is_PLACED_and_not_merely_caught(self):
        """The four value flags this round added were each a live fail-open — the flag's
        value was named as the program — and each is now covered TWICE: by the table, and by
        the fallback standing behind it.

        **That is exactly the shape where a test can pass for the wrong reason.** The
        fallback alone gets the vault row right, so a row asserting only the denial stays
        green with the table entry deleted — which is what the hand-check found for all four
        of them. What the entry actually buys is PRECISION: with the flag placed, the guard
        trusts the program after it, and `echo <vault>` stays allowed, because printing a
        path is not reading a file. Both halves are asserted, so deleting an entry fails
        here rather than quietly widening the guard.
        """
        for wrapper, flag, value in (("env", "-P", "/bin"), ("sudo", "-T", "5"),
                                     ("doas", "-a", "style"), ("xargs", "-J", "%"),
                                     ("xargs", "-R", "2"), ("xargs", "-S", "4096")):
            with self.subTest(wrapper=wrapper, flag=flag):
                self.assertIn(flag, hooks._WRAPPER_VALUE_FLAGS[wrapper])
                self.assertIsNotNone(
                    hooks._leak_reason(f"{wrapper} {flag} {value} cat {VAULT}"))
                self.assertIsNone(
                    hooks._leak_reason(f"{wrapper} {flag} {value} echo {VAULT}"))

    def test_the_fallback_does_not_deny_a_command_that_names_nothing_guarded(self):
        """It reports paths the command MAY open; the leak guard still asks the same
        `_names_a_vault_path` of them it asks of a reader's operands. An unknown flag is not
        by itself a reason to refuse anything."""
        self.assertIsNone(hooks._leak_reason("env --not-a-real-flag /bin cat notes.md"))

    def test_a_placed_flag_does_not_trip_the_fallback(self):
        """Otherwise every wrapped command would carry its whole argv into `reads` and the
        distinction this class rests on would not exist."""
        _p, _e, _a, _c, reads = hooks._split_env_chdir(["env", "-i", "cat", "notes.md"])
        self.assertEqual(reads, [])

    def test_only_a_BUNDLED_separated_value_reports_the_rest_of_the_segment(self):
        """The precision the `nxt != name` test buys, which the hand-check killed and
        nothing noticed.

        A flag written on its own — `env -C <dir>`, `env --chdir <dir>` — is placed exactly,
        so the guard trusts the program after it and `echo <vault>` stays allowed: printing
        a path is not reading a file. The same flag found INSIDE a bundle is a guess about
        one letter's arity, so the rest of the segment stays reachable and the same `echo`
        is refused. Without the test, every separated value would report its whole segment
        and the difference would be invisible.
        """
        self.assertIsNone(hooks._leak_reason(f"env -C /tmp echo {VAULT}"))
        self.assertIsNone(hooks._leak_reason(f"env --chdir /tmp echo {VAULT}"))
        self.assertIsNone(hooks._leak_reason(f"sudo -u root echo {VAULT}"))
        self.assertIsNotNone(hooks._leak_reason(f"env -iC /tmp echo {VAULT}"))


class TestAWrappersOwnOperandIsNotTheProgram(unittest.TestCase):
    """The third thing this sweep found, and it is the same shape one branch over.

    `timeout 5 cat <vault>` was the only wrapper whose leading POSITIONAL was modelled, and
    the branch that models it reads as if `timeout` were the only wrapper with one. Two more
    put an argument of their own in front of the program, and the parser named that argument
    as the program:

        chrt 5 cat .charter/vaults/x.json        -> ALLOW, `5` read as the program
        su-exec root cat .charter/vaults/x.json  -> ALLOW, `root` read as the program

    Both are Linux tools rather than macOS ones, so these are by construction from their
    documented grammars (`chrt [options] <priority> <command>`,
    `su-exec <user-spec> <command>`) rather than reproduced on this machine — the guard runs
    on planes where they exist.
    """

    def test_the_operand_is_stepped_over_and_the_program_is_found(self):
        for cmd in (f"chrt 5 cat {VAULT}", f"su-exec root cat {VAULT}",
                    f"chrt -r 5 cat {VAULT}", f"timeout 5 cat {VAULT}"):
            with self.subTest(cmd=cmd):
                self.assertIsNotNone(hooks._leak_reason(cmd), cmd)

    def test_only_the_documented_number_of_operands_is_taken(self):
        """One each. Taking two would step over the program itself, which is the fail-open
        direction and the reason this is a count per wrapper and not a `while`."""
        self.assertEqual(hooks._split_env(["chrt", "5", "cat", VAULT])[0], "cat")
        self.assertEqual(hooks._split_env(["su-exec", "root", "cat", VAULT])[0], "cat")

    def test_a_wrapper_with_no_leading_operand_is_untouched(self):
        """`nohup cat <vault>` has no argument of its own, and stepping over one would name
        the vault path as the program and lose the reader."""
        for wrapper in ("nohup", "setsid", "unbuffer", "command"):
            with self.subTest(wrapper=wrapper):
                self.assertEqual(hooks._split_env([wrapper, "cat", VAULT])[0], "cat")
                self.assertNotIn(wrapper, hooks._WRAPPER_LEADING_OPERANDS)

    def test_timeout_keeps_its_own_rule(self):
        """`timeout` is deliberately NOT in the table. Its operand is matched rather than
        counted, and `tests/test_the_end_of_a_name_is_the_end_of_the_string.py` holds
        `_DURATION_RE` to what a tightened version of it would answer (#577) — a bare count
        would make that measurement unreachable, since a count never looks at the token."""
        self.assertNotIn("timeout", hooks._WRAPPER_LEADING_OPERANDS)
        self.assertEqual(hooks._split_env_chdir(["timeout", "notaduration", "cat"])[0],
                         "notaduration")
