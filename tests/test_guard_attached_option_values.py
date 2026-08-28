"""A value ATTACHED to a short flag, read by a rule written for the long one (#547).

`env -Sfoo=1 cat .charter/vaults/x.json` printed a vault on a real plane while the same
command unwrapped was denied. `hooks._split_env_chdir` read the `--split-string=` spelling
BEFORE the glued `-S…` one, so a packed command carrying its own `=` was split at the wrong
one: the program came back `1`, the `cat` was never named, and nothing downstream had a
reader to object to.

**The six-row table below is the point of this file, not the one row that failed.** Measured
on a plane built by `charter init`, a fabricated value in `.charter/vaults/x.json`, every
command fed to the real `charter hook pretooluse`:

    cat .charter/vaults/x.json                           -> DENY   (control)
    env -Sfoo=1 cat .charter/vaults/x.json               -> ALLOW  <-- the bypass
    env -S "cat .charter/vaults/x.json"                  -> DENY
    env --split-string=foo=1 cat .charter/vaults/x.json  -> DENY
    env -S "foo=1 cat .charter/vaults/x.json"            -> DENY
    env FOO=1 cat .charter/vaults/x.json                 -> DENY

Four of the five neighbours denied, which is why a hand probe concluded the guard held: it
took the one form combining a GLUED SHORT FLAG with a value that itself contains `=` — and
that is the form a real `env -S` setting a variable naturally takes. So all six rows are
pinned in one table, driven by one loop. The five that already denied are what make the
sixth's regression visible; without them a later refactor can trade one spelling for another
and stay green, which is the shape this guard keeps getting caught by.

**And its siblings, because the bug is a SHAPE and `-S` is not the only flag with it.**
Every flag in `hooks._WRAPPER_VALUE_FLAGS` whose value may be attached had the same two
rules in the same wrong order, and `env -C` — the chdir whose value is what makes a later
relative operand resolve — was a live bypass of exactly the same kind:

    env -Cx=y/../.charter/vaults cat x.json  -> ALLOW, and it printed the vault

verified on the same plane, after the `mkdir x=y` a shell does without being asked twice.
`sudo -D` and `xargs -a` are the same parse and are here for the same reason.
`TestEveryFlagWhoseValueCanBeAttached` walks all three wrappers across all four spellings,
so the answer cannot depend on which one a reviewer happened to try.

**What this file does NOT claim.** `SECURITY.md` says guard rails, not guarantees, and
deciding what a shell will execute without executing it is not winnable in a Python
tokeniser. This is one mis-ordering with a correct answer — getopt gives a short option
everything glued after it, `=` included — not an attempt to close the class.

**The two neighbours found while measuring this one are closed now, and they have their own
files** rather than rows here, because each is its own property and this table is #547's
measurement verbatim:

* #556 — a value attached to a BUNDLED short option (`env -iC<dir> cat x.json`).
  `tests/test_an_option_is_its_letter_not_its_position.py`. The `bundled` spelling in
  `TestEveryFlagWhoseValueCanBeAttached` below is that fix crossed with this file's axis,
  which is where the two meet.
* #555 — `env` accepting assignments to names that are not shell identifiers while this
  parser stopped at the first token it could not read as one (`env a-b=1 cat <vault>`).
  `tests/test_the_assignment_rule_belongs_to_the_parser.py`.
"""

from __future__ import annotations

import unittest

from charter import hooks
from tests._isolation import PersonaIso, run_hook

VAULT = ".charter/vaults/x.json"

#: The measurement in #547, verbatim, as ``(command, must be denied)``. The `False` rows
#: are not padding: they are the control and the four neighbours whose denial made the
#: fifth look like a working guard. Written as one table so that no row can be dropped
#: without the count below noticing.
SIX_ROWS = (
    (f"cat {VAULT}", True),                          # the control
    (f"env -Sfoo=1 cat {VAULT}", True),              # #547 — allowed before the fix
    (f'env -S "cat {VAULT}"', True),
    (f"env --split-string=foo=1 cat {VAULT}", True),
    (f'env -S "foo=1 cat {VAULT}"', True),
    (f"env FOO=1 cat {VAULT}", True),
)


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


class TestTheSixSpellingsOfOneVaultRead(unittest.TestCase):
    """`_leak_reason`, the guard's own answer, over the whole table."""

    def test_the_table_is_the_size_it_claims(self):
        """Six rows, one of them #547's. A table that quietly loses its neighbours stops
        being able to show that a fix traded one spelling for another."""
        self.assertEqual(len(SIX_ROWS), 6)
        self.assertIn(f"env -Sfoo=1 cat {VAULT}", [c for c, _ in SIX_ROWS])

    def test_every_row_is_denied(self):
        for cmd, denied in SIX_ROWS:
            with self.subTest(cmd=cmd):
                reason = hooks._leak_reason(cmd)
                self.assertEqual(reason is not None, denied, cmd)
                self.assertIn("reads a vault/secret file directly", reason or "")

    def test_the_glued_short_form_names_the_reader(self):
        """The row that failed, stated as the parse rather than as the verdict — so a
        denial arriving from somewhere else (the raw-string arm, say) cannot stand in for
        the ordering being right.

        `-Sfoo=1` packs `foo=1`; the packed word is an env ASSIGNMENT and the program after
        it is `cat`. Before the fix this came back `("1", …)`.
        """
        prog, env, argv, _chdir, _reads = hooks._split_env_chdir(
            ["env", "-Sfoo=1", "cat", VAULT])
        self.assertEqual(prog, "cat")
        self.assertEqual(env, ["foo=1"])
        self.assertEqual(argv, ["cat", VAULT])

    def test_the_long_form_still_splits_at_its_own_equals(self):
        """The rule that was being reached first is still right about its own spelling:
        `--split-string=foo=1` splits at the FIRST `=`, packed value `foo=1`. Fixing an
        ordering may not be a way of losing the other branch."""
        prog, env, _argv, _c, _r = hooks._split_env_chdir(
            ["env", "--split-string=foo=1", "cat", VAULT])
        self.assertEqual((prog, env), ("cat", ["foo=1"]))

    def test_a_separate_value_is_still_taken_from_the_next_token(self):
        """`-S` and `--split-string` with nothing attached: the value is the NEXT token,
        and skipping it instead of unpacking it leaves an empty argv — the fail-open
        `_SPLIT_STRING_FLAGS` was written against."""
        for flag in ("-S", "--split-string"):
            with self.subTest(flag=flag):
                prog, _e, _a, _c, _r = hooks._split_env_chdir(
                    ["env", flag, f"foo=1 cat {VAULT}"])
                self.assertEqual(prog, "cat")

    def test_a_glued_value_with_no_equals_in_it_still_works(self):
        """The neighbour that always denied. `-Scat <vault>` packs a whole command with no
        `=` anywhere, so it never depended on which rule ran first."""
        prog, _e, _a, _c, _r = hooks._split_env_chdir(["env", f"-Scat {VAULT}"])
        self.assertEqual(prog, "cat")


class TestTheSixSpellingsThroughTheRealHook(PersonaIso):
    """The same six rows through `hooks.pretooluse`, which is where they were measured.

    `_leak_reason` above is the guard; this is the DECISION — the JSON a harness acts on,
    nested under `hookSpecificOutput` (reading `permissionDecision` from the root answers
    `None` for every input and looks exactly like a guard that has stopped working, which
    cost the reporter of #547 a round).
    """

    def test_every_row_is_denied_end_to_end(self):
        for cmd, denied in SIX_ROWS:
            with self.subTest(cmd=cmd):
                r = run_hook(hooks.pretooluse,
                             {"tool_name": "Bash", "tool_input": {"command": cmd},
                              "session_id": "s"})
                self.assertEqual(_decision(r), "deny" if denied else None, cmd)


#: `(wrapper, flag whose value may be attached, what the value has to reach)`. One row per
#: flag in `_WRAPPER_CHDIR_FLAGS`/`_WRAPPER_READ_FLAGS` — the two tables where a LOST value
#: is a lost denial rather than a cosmetic misparse.
#: The fifth field is a letter of the SAME wrapper that takes no value, so the flag can be
#: bundled behind it — #556's spelling, crossed with this file's axis.
ATTACHED_VALUE_FLAGS = (
    ("env", "-C", "--chdir", "cat x.json", "i"),
    ("sudo", "-D", "--chdir", "cat x.json", "b"),
    ("xargs", "-a", "--arg-file", "echo", "0"),
)


class TestEveryFlagWhoseValueCanBeAttached(unittest.TestCase):
    """The sibling search, run as a test rather than reported in a commit message.

    Four spellings per flag: separated, glued, long-with-`=`, and glued-with-an-`=`-in-the
    -value — the last being #547's shape, and the one that was allowed for `env -C`,
    `sudo -D` and `xargs -a` alike. The `x=y/../` prefix resolves away lexically, so all
    four name the same file; the `=` in it is the whole point.
    """

    def _cmds(self, wrapper: str, short: str, long: str, tail: str, lead: str):
        target = ".charter/vaults" if wrapper != "xargs" else VAULT
        return {
            "separated": f"{wrapper} {short} {target} {tail}",
            "glued": f"{wrapper} {short}{target} {tail}",
            "long with =": f"{wrapper} {long}={target} {tail}",
            "glued, = in the value": f"{wrapper} {short}x=y/../{target} {tail}",
            # #556: the same flag, one letter in. `-iC<dir>` is `-i -C <dir>` to getopt, and
            # a reader that matches by `tok.startswith(flag)` sees a short option only when
            # it is written first.
            "bundled, glued": f"{wrapper} -{lead}{short[1:]}{target} {tail}",
            "bundled, separated": f"{wrapper} -{lead}{short[1:]} {target} {tail}",
        }

    def test_every_spelling_is_denied(self):
        for wrapper, short, long, tail, lead in ATTACHED_VALUE_FLAGS:
            for spelling, cmd in self._cmds(wrapper, short, long, tail, lead).items():
                with self.subTest(wrapper=wrapper, spelling=spelling):
                    self.assertIsNotNone(hooks._leak_reason(cmd), cmd)

    def test_the_bundling_letter_really_takes_no_value(self):
        """Non-vacuity for the two rows above: if `lead` took a value of its own, the
        `bundled` spellings would be testing a different command than they claim and would
        pass for the wrong reason."""
        for wrapper, _short, _long, _tail, lead in ATTACHED_VALUE_FLAGS:
            with self.subTest(wrapper=wrapper):
                self.assertIn(lead, hooks._WRAPPER_NOVALUE_LETTERS[wrapper])
                self.assertNotIn(
                    "-" + lead, hooks._WRAPPER_VALUE_FLAGS.get(wrapper, ()))

    def test_the_value_itself_comes_back_whole(self):
        """Stated as the parse, because "denied" can be true for the wrong reason. A short
        option takes everything glued after it — getopt included the `=` — so the chdir is
        the whole `x=y/../.charter/vaults`, not the `y/../…` after its first `=`."""
        _p, _e, _a, chdir, _r = hooks._split_env_chdir(
            ["env", "-Cx=y/../.charter/vaults", "cat", "x.json"])
        self.assertEqual(chdir, "x=y/../.charter/vaults")

        _p, _e, _a, _c, reads = hooks._split_env_chdir(
            ["xargs", f"-ax=y/../{VAULT}", "echo"])
        self.assertEqual(reads, [f"x=y/../{VAULT}"])

    def test_the_long_form_is_still_read_as_a_long_form(self):
        """`--chdir=<dir>` splits at its `=` exactly as before — the glued rule must not
        start eating long options because they happen to contain a short flag's letters."""
        _p, _e, _a, chdir, _r = hooks._split_env_chdir(
            ["env", "--chdir=.charter/vaults", "cat", "x.json"])
        self.assertEqual(chdir, ".charter/vaults")

    def test_a_flag_that_takes_no_value_still_takes_none(self):
        """`env -i` ignores the environment and takes nothing. A glued rule that matched it
        would swallow the program — the fail-open `_WRAPPER_VALUE_FLAGS` is per-wrapper to
        avoid, since `xargs -i` and `stdbuf -i` DO take one."""
        prog, _e, _a, _c, _r = hooks._split_env_chdir(["env", "-i", "cat", VAULT])
        self.assertEqual(prog, "cat")


class TestTheParserIsShared(unittest.TestCase):
    """`tests/_planeguard.py` reads this parser rather than keeping its own (#543), so a
    change here moves the production guard AND the harness's spawn tripwire together.

    It used to repair this very ordering on the way in (`_unpack_split_strings`), with a
    comment saying to delete the repair when #547 landed. It is deleted; this asserts the
    reason it could be — the ordering is right in the one parser both callers read.
    """

    def test_the_harness_reads_production_and_gets_the_same_answer(self):
        """`env -Sfoo=1 charter docs` reaches `charter docs` — the harness's answer with
        no repair of its own, which is what the deleted one was standing in for.

        *consumed* is "in the input and not in the argv", so it holds the `-Sfoo=1` TOKEN
        rather than the `foo=1` word production unpacked out of it; the packed word is
        checked in the argv, where it is checked properly.
        """
        from tests import _planeguard
        argv, consumed = _planeguard._launcher_argv(["env", "-Sfoo=1", "charter", "docs"])
        self.assertEqual(argv, ["charter", "docs"])
        self.assertEqual(consumed, ["env", "-Sfoo=1"])

    def test_no_ordering_repair_is_left_in_the_harness(self):
        """Named directly, so re-adding a private repair is a test failure and not a
        judgement call. Two implementations of one question hide each other's defects —
        which is why #543 collapsed them, and why this one may not come back."""
        from tests import _planeguard
        self.assertFalse(hasattr(_planeguard, "_unpack_split_strings"))


if __name__ == "__main__":
    unittest.main()
