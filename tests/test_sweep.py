"""The deletion sweep, swept.

`tools/sweep.py` is a tool for finding guards no test goes red without, so the one thing
it may not have is a guard no test goes red without. Every case here is written to die
when a specific line of `sweep.py` is deleted, and the fixtures are the real shapes from
the three review rounds rather than invented ones — `_placed_here`'s two-conjunct guard,
`_window`'s clamp, `_component_text`'s `except Exception` — so that a refactor which
still passes has kept the property those rounds actually measured.
"""
from __future__ import annotations

import ast
import contextlib
import dataclasses
import hashlib
import io
import json
import os
import re
import resource
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
from pathlib import Path

from tools import sweep


def _mutations(source: str, lines: set[int] | None = None, path: str = "charter/x.py"):
    blob = textwrap.dedent(source).lstrip("\n").encode("utf-8")
    n = len(blob.splitlines())
    return sweep.mutations_for(path, blob, lines if lines is not None else set(range(1, n + 2)))


def _by(muts, operator: str):
    return [m for m in muts if m.operator == operator]


def _afters(muts, operator: str):
    return sorted(m.after for m in _by(muts, operator))


# ======================================================================================
# The operator table — one class per shape, each built from the guard that found it
# ======================================================================================

class TheRefusalShape(unittest.TestCase):
    """`if C: return` / `raise` / `continue`, dropped."""

    def test_a_refusal_that_returns_is_offered_for_deletion(self):
        muts = _mutations("""
            def close_argvs(arm, overlay_pane):
                if arm is None:
                    return []
                return [arm]
        """)
        drops = _by(muts, "drop-if")
        self.assertEqual([m.line for m in drops], [2])
        self.assertEqual(drops[0].after, "")
        self.assertIn("arm is None", drops[0].before)

    def test_the_body_may_be_an_assignment_and_not_only_a_return(self):
        """Round three's two sharpest findings were assignments under an `if`.

        `_window`'s `if self._sel < top: top = self._sel` and `panel_argvs`'
        `if size is None: size = SLOT_SIZE[slot]` are both a refusal spelled as a
        correction. A shape table that only knew `return`/`raise`/`continue` would have
        written both off before asking.
        """
        muts = _mutations("""
            def _window(self, height):
                top = self._top
                if self._sel < top:
                    top = self._sel
                return top
        """)
        self.assertEqual([m.line for m in _by(muts, "drop-if")], [3])

    def test_a_multi_statement_body_is_not_dropped_wholesale(self):
        """Deleting two statements at once cannot say which one was the guard."""
        muts = _mutations("""
            def f(buf):
                if buf is None:
                    buf = b""
                    log(buf)
                return buf
        """)
        self.assertEqual(_by(muts, "drop-if"), [])

    def test_deleting_the_only_statement_of_a_block_becomes_pass_and_still_parses(self):
        """The spec says to skip a mutation that does not parse. `pass` is the same
        deletion spelled so that it does, and it is tried before the mutation is given up
        on — otherwise every one-line function body would be unsweepable."""
        muts = _mutations("""
            def f(x):
                while x:
                    if x > 1:
                        break
        """)
        drops = _by(muts, "drop-if")
        self.assertEqual([m.after for m in drops], ["pass"])
        ast.parse(drops[0].source)


class TheConjunctShape(unittest.TestCase):
    """`if A and B:` — one half at a time, because each half is its own finding."""

    def test_each_half_of_a_two_part_guard_is_its_own_mutation(self):
        """`_placed_here`'s `isinstance(name, str) and name not in SLOT_SIZE` was
        reported as TWO unpinned guards on the same line (#553 round three). A mutation
        that could only take the whole condition would have collapsed them into one."""
        muts = _mutations("""
            def _placed_here():
                for placed in items:
                    name = placed.get("slot")
                    if isinstance(name, str) and name not in SLOT_SIZE:
                        out[name] = placed
        """)
        self.assertEqual(_afters(muts, "drop-conjunct"),
                         ["(name not in SLOT_SIZE)", "isinstance(name, str)"])

    def test_a_three_part_guard_drops_one_part_at_a_time_and_keeps_the_rest(self):
        muts = _mutations("""
            def f(a, b, c):
                if a and b and c:
                    return 1
        """)
        self.assertEqual(_afters(muts, "drop-conjunct"),
                         ["(a and b)", "(a and c)", "(b and c)"])

    def test_a_single_condition_offers_no_conjunct_mutation(self):
        muts = _mutations("""
            def f(a):
                if a:
                    return 1
        """)
        self.assertEqual(_by(muts, "drop-conjunct"), [])


class TheClampShape(unittest.TestCase):
    """`max(a, b)` / `min(…)`, dropped to each operand."""

    def test_a_floor_is_dropped_to_the_value_it_floors(self):
        """`_window`'s `n = max(1, height - _CHROME_ROWS)` (#554 round three)."""
        muts = _mutations("""
            def _window(self, height):
                n = max(1, height - _CHROME_ROWS)
                return n
        """)
        self.assertEqual(_afters(muts, "unclamp"), ["(height - _CHROME_ROWS)", "1"])

    def test_a_nested_clamp_offers_the_inner_variable_on_its_own(self):
        """`self._top = max(0, min(top, max(0, len(self.rows) - n)))` is unpinned, and
        the mutation that showed it was replacing the whole thing with `top`. A nested
        clamp has to be walked, not just matched at the outermost call."""
        muts = _mutations("""
            def _window(self, top, n):
                self._top = max(0, min(top, max(0, len(self.rows) - n)))
        """)
        self.assertIn("top", _afters(muts, "unclamp"))

    def test_a_three_argument_max_is_left_alone(self):
        """`max(a, b, c)` has no single inner operand a clamp is 'dropping to'."""
        muts = _mutations("""
            def f(a, b, c):
                x = max(a, b, c)
        """)
        self.assertEqual(_by(muts, "unclamp"), [])


class TheCatchShape(unittest.TestCase):
    """`except E:` narrowed to something nothing raises."""

    def test_a_broad_catch_is_narrowed_and_the_binding_survives(self):
        """`_component_text`'s `except Exception as e` was round two's finding 4. The
        `as e` has to be kept or the mutation is a `NameError` in the handler body — a
        red for a reason that has nothing to do with the guard, which is a false pin."""
        muts = _mutations("""
            def _component_text(cid):
                try:
                    return draw(cid)
                except Exception as e:
                    return str(e)
        """)
        narrowed = _by(muts, "narrow-except")
        self.assertEqual([m.after for m in narrowed], ["ZeroDivisionError"])
        self.assertIn("except ZeroDivisionError as e:", narrowed[0].source.decode())

    def test_a_bare_except_is_not_mutated(self):
        """There is nothing to narrow to that is narrower than `except:`."""
        muts = _mutations("""
            def f():
                try:
                    g()
                except:
                    pass
        """)
        self.assertEqual(_by(muts, "narrow-except"), [])

    def test_a_catch_already_at_the_sentinel_is_not_mutated_into_itself(self):
        muts = _mutations("""
            def f():
                try:
                    g()
                except ZeroDivisionError:
                    pass
        """)
        self.assertEqual(_by(muts, "narrow-except"), [])


class TheContainmentShape(unittest.TestCase):
    """`f(contain.one_line(x))` -> `f(x)`."""

    def test_a_containment_call_is_replaced_by_the_value_it_contained(self):
        """`run`'s `contain.one_line(repr(slot))` (#553 round three, panel.py:458)."""
        muts = _mutations("""
            def run(slot):
                reason = f"unknown slot {contain.one_line(repr(slot))} "
                return reason
        """)
        self.assertEqual(_afters(muts, "uncontain"), ["repr(slot)"])

    def test_containment_inside_an_fstring_is_found(self):
        """Every one of these lives inside an f-string in the real source, so a walk that
        did not descend into `FormattedValue` would find none of them."""
        muts = _mutations("""
            def render(self, width):
                return f"{contain.one_line(self.heading)} · {len(self.rows)}"
        """)
        self.assertEqual(_afters(muts, "uncontain"), ["self.heading"])

    def test_an_unrelated_module_call_is_not_treated_as_containment(self):
        muts = _mutations("""
            def f(x):
                return tui.truncate(x, 10)
        """)
        self.assertEqual(_by(muts, "uncontain"), [])


class TheFallbackShape(unittest.TestCase):
    """`d.get(k) or ()` and `d.get(k, v)` -> `d[k]`."""

    def test_a_get_with_an_or_fallback_becomes_a_subscript(self):
        """`_placed_here`'s `config.FRAME.get("components") or ()` (#553 round three)."""
        muts = _mutations("""
            def _placed_here():
                for placed in config.FRAME.get("components") or ():
                    yield placed
        """)
        self.assertEqual(_afters(muts, "no-fallback"), ['config.FRAME["components"]'])

    def test_a_get_with_a_default_becomes_a_subscript(self):
        """`_derive`'s `SLOT_OF.get(c.id, c.id)` (#553 round two, finding 8)."""
        muts = _mutations("""
            def _derive(c):
                return _builtins.SLOT_OF.get(c.id, c.id)
        """)
        self.assertIn("_builtins.SLOT_OF[c.id]", _afters(muts, "no-fallback"))

    def test_an_or_over_an_empty_literal_drops_to_the_left(self):
        muts = _mutations("""
            def f(placed):
                return placed or []
        """)
        self.assertEqual(_afters(muts, "no-fallback"), ["placed"])


class TheTypeFilterShape(unittest.TestCase):
    """`if isinstance(x, str)`, dropped."""

    def test_a_negated_type_filter_becomes_false(self):
        """`slots.drawable`'s `if not isinstance(name, str): return False`."""
        muts = _mutations("""
            def drawable(name):
                if not isinstance(name, str):
                    return False
                return True
        """)
        self.assertEqual(_afters(muts, "drop-isinstance"), ["False"])

    def test_a_plain_type_filter_becomes_true(self):
        muts = _mutations("""
            def f(name):
                if isinstance(name, str):
                    use(name)
        """)
        self.assertEqual(_afters(muts, "drop-isinstance"), ["True"])


class TheConditionalExpressionShape(unittest.TestCase):
    """`a if C else b`, collapsed each way."""

    def test_both_arms_are_offered(self):
        """`_policy_cells`' `return size.n if isinstance(size, Fixed) else 1` — the
        `else 1` was round three's fifth finding on #553."""
        muts = _mutations("""
            def _policy_cells(size):
                return size.n if isinstance(size, Fixed) else 1
        """)
        self.assertEqual(_afters(muts, "collapse-ifexp"), ["1", "size.n"])

    def test_a_gate_written_as_a_conditional_expression_is_found(self):
        """`snapshot = gather.read(fid) if c.needs else {}` — round two's finding 5 is an
        `if` that never uses the `if` keyword, and it cost a real idle-cost property."""
        muts = _mutations("""
            def _component_text(c, fid):
                snapshot = gather.read(fid) if c.needs else {}
        """)
        self.assertIn("gather.read(fid)", _afters(muts, "collapse-ifexp"))


class TheComprehensionFilterShape(unittest.TestCase):
    """A refusal in expression clothes."""

    def test_a_generator_filter_is_dropped(self):
        """`harness_rows`' `if _edge_of(slot) not in _COLUMN_EDGES` lives inside a
        `sum(...)`. It was round two's finding 1 on #553 and a table that only knew the
        statement spelling of `if` could not see it at all."""
        muts = _mutations("""
            def harness_rows(sizes, window_rows):
                used = sum(n + _BORDER_ROWS for slot, n in sizes.items()
                           if _edge_of(slot) not in _COLUMN_EDGES)
                return window_rows - used
        """)
        self.assertEqual(_afters(muts, "drop-comprehension-if"), ["True"])

    def test_a_comprehension_with_no_filter_offers_nothing(self):
        muts = _mutations("""
            def f(xs):
                return [x for x in xs]
        """)
        self.assertEqual(_by(muts, "drop-comprehension-if"), [])


class TheBranchShape(unittest.TestCase):
    """A branch in a chain is disabled, because it cannot be excised."""

    def test_an_elif_branch_is_disabled_rather_than_deleted(self):
        """`decode`'s `elif ch == b"\\x03"` Ctrl-C branch was round two's finding 14.
        Deleting it would take the `else` with it, so the branch is turned off instead."""
        muts = _mutations("""
            def decode(ch):
                if ch == b"a":
                    return 1
                elif ch == b"\\x03":
                    return 2
                else:
                    return 3
        """)
        self.assertIn("False", _afters(muts, "disable-branch"))

    def test_a_bare_if_is_never_disabled(self):
        """A bare `if` whose body binds a name would fall through to a `NameError` — a
        red for a reason that has nothing to do with the property, which is a FALSE PIN
        and the one outcome worse than no signal at all. So the operator is restricted
        to chains that have somewhere else for control to go.
        """
        muts = _mutations("""
            def f(x):
                if x:
                    y = 1
                    z = 2
                return y
        """)
        self.assertEqual(_by(muts, "disable-branch"), [])


class TheConstantShape(unittest.TestCase):
    """A module-level constant is a guard too."""

    def test_an_integer_constant_is_moved_by_one(self):
        """`_SPLIT_ROWS = 5` and `_MIN_TITLE = 8` were both unpinned on #554 round two,
        each with a docstring making a specific claim for the number."""
        muts = _mutations("_SPLIT_ROWS = 5\n")
        self.assertEqual(_afters(muts, "retune-constant"), ["6"])

    def test_a_constant_summed_from_named_parts_drops_each_part(self):
        """`_CHROME_ROWS = _HEADER_ROWS + _FOOTER_ROWS`, and the finding was that
        dropping the footer left the suite green."""
        muts = _mutations("_CHROME_ROWS = _HEADER_ROWS + _FOOTER_ROWS\n")
        self.assertEqual(_afters(muts, "drop-term"), ["_FOOTER_ROWS", "_HEADER_ROWS"])

    def test_a_local_variable_is_not_a_constant(self):
        """Only module scope, and only a constant-looking name. Perturbing every integer
        in every function is a different tool, and a much noisier one."""
        muts = _mutations("""
            def f():
                LIMIT = 5
                return LIMIT
        """)
        self.assertEqual(_by(muts, "retune-constant"), [])

    def test_a_lowercase_module_level_name_is_not_a_constant(self):
        muts = _mutations("timeout = 5\n")
        self.assertEqual(_by(muts, "retune-constant"), [])

    def test_a_boolean_is_not_retuned_into_two(self):
        """`True + 1` is `2` and Python will not complain, which is exactly why this
        needs saying: a flag flipped to `2` is not a mutation anybody can read."""
        muts = _mutations("ENABLED = True\n")
        self.assertEqual(_by(muts, "retune-constant"), [])


# ======================================================================================
# Splicing — line numbers are the whole output of this tool
# ======================================================================================

class TheStringShape(unittest.TestCase):
    """`retune-string` (#569): the value moved, every structural property held.

    The spec declared this family a gap on the grounds that a string has no honest general
    perturbation — "picking `1003` over `1000` for `MOUSE_ON` is fitting the answer key".
    :func:`sweep.retune` is the answer: same length, same character classes, different
    value, derived from the constant and from nothing else. Every case below asserts one
    half of that — either that the value really moved, or that some structure really did
    not.
    """

    def test_a_string_the_program_compares_is_retuned(self):
        muts = _mutations("""
            def f(answer):
                if answer == "yes":
                    return 1
        """)
        self.assertEqual(_afters(muts, "retune-string"), ["'zft'"])

    def test_the_mutant_is_the_same_length_and_the_same_character_classes(self):
        """The whole justification for the operator. A perturbation that changed the
        length, or turned a digit into a letter, would break f-string specs, regexes and
        escape sequences — and a mutation that reddens the suite because the mutant no
        longer *parses as its own kind of string* is a false pin."""
        for shipped in ("\x1b[?1000h", "components", "{:<28}", "%s says", "a-b_c.d"):
            moved = sweep.retune(shipped)
            self.assertNotEqual(moved, shipped)
            self.assertEqual(len(moved), len(shipped))
            for a, b in zip(shipped, moved):
                self.assertEqual((a.isdigit(), a.isupper(), a.islower(), a.isalnum()),
                                 (b.isdigit(), b.isupper(), b.islower(), b.isalnum()),
                                 f"{shipped!r} -> {moved!r} changed a character's class")

    def test_the_terminal_escape_stays_a_terminal_escape(self):
        """`MOUSE_ON` is the spec's own example of the string it could not mutate."""
        self.assertEqual(sweep.retune("\x1b[?1000h"), "\x1b[?2111i")

    def test_a_character_behind_a_backslash_is_left_alone(self):
        """A raw `\\d` is one escape, not a backslash next to a letter. Shifting the `d`
        makes `re.error: bad escape \\e`, which reddens the suite for a reason that has
        nothing to do with the property — the definition of a false pin."""
        self.assertEqual(sweep.retune(r"^Ran (\d+) tests?"), r"^Sbo (\d+) uftut?")

    def test_a_retuned_pattern_is_still_a_pattern(self):
        """#698. `retune` shifts a digit `0 -> 1` and `9 -> 0`, and inside a character
        class that INVERTS the range: `[0-9] -> [1-0]` is `re.error: bad character
        range`. The module then raises on import, no test runs at all, and the sweep
        reports `no verdict` on a question it could have asked properly.

        Measured over `charter/` and `tools/` before this rule: **56 of the 108**
        patterns in the tree retuned into something `re.compile` refuses.
        """
        for shipped in (r"\$[0-9]+", r"@[0-9]+", "[A-Za-z0-9._-]+", "(?i)^core=",
                        "^[a-z0-9][a-z0-9._-]*$", r"\bagentId:\s*([0-9a-f]{6,})"):
            with self.subTest(shipped=shipped):
                moved = sweep.retune(shipped, regex=True)
                self.assertNotEqual(moved, shipped,
                                    "the question was withdrawn, not kept")
                re.compile(moved)          # must not raise

    def test_the_low_end_of_a_range_moves_and_the_high_end_holds(self):
        """WHICH end moves is the whole reason the rule names one. Holding both would
        keep the pattern valid by making the mutation a no-op — and a character class is
        the whole of many patterns here, so that would withdraw the question rather than
        fix it. `[0-9] -> [1-9]` is a valid pattern that stops matching `0`."""
        self.assertEqual(sweep.retune(r"@[0-9]+", regex=True), r"@[1-9]+")
        self.assertEqual(sweep.retune("[A-Za-z]", regex=True), "[B-Zb-z]")

    def test_the_letter_after_an_inline_group_is_left_alone(self):
        """`(?i) -> (?j)` is `unknown extension ?j`. The same rule as the backslash, one
        step over: a character that says what KIND of thing comes next is syntax, and
        the syntax is not the value this operator asks about."""
        self.assertEqual(sweep.retune("(?i)^core", regex=True), "(?i)^dpsf")
        re.compile(sweep.retune("(?P<n>x)", regex=True))

    def test_the_regex_rules_are_off_for_a_string_that_is_not_one(self):
        """A separator that happens to spell `[a-b]` has no syntax to protect, and holding
        a character in it would weaken the mutation for nothing. The caller says which
        strings are patterns — `regex_positions` knows and `retune` does not guess.

        And a `-` OUTSIDE a class is not a range in either mode: `a-b` is two literals and
        a dash, so both ends move and the two answers agree. The rule is about a character
        class, not about every dash in a string."""
        self.assertEqual(sweep.retune("[a-b]"), "[b-c]")
        self.assertEqual(sweep.retune("[a-b]", regex=True), "[b-b]")
        self.assertEqual(sweep.retune("a-b"), sweep.retune("a-b", regex=True))

    def test_a_pattern_the_rules_cannot_keep_valid_is_withheld_not_dropped(self):
        """The backstop, and the reason the shift rules may be approximate. A `\\x1f`
        retunes to `\\x2g`, which is not a hex escape — the one case left in `charter/`
        today. It is planned, declined, and REPORTED: a question not asked in silence is
        the same failure as a shard that never reported."""
        muts = _mutations(
            'import re\n'
            'P = re.compile("[\\\\x00-\\\\x1f]")\n')
        offered = [m for m in muts if m.operator == "retune-string"]
        self.assertEqual(len(offered), 1)
        self.assertTrue(offered[0].withheld.startswith("the retuned pattern"),
                        offered[0].withheld)

    def test_a_pattern_that_survives_the_rules_carries_no_refusal(self):
        """The control: the backstop must not decline what the rules already fixed."""
        muts = _mutations(
            'import re\n'
            'P = re.compile(r"[0-9]+")\n')
        offered = [m for m in muts if m.operator == "retune-string"]
        self.assertEqual([m.withheld for m in offered], [""])

    def test_only_the_pattern_argument_of_an_re_call_is_a_pattern(self):
        """`re.split` takes a regex and `str.split` takes a separator; they share a name,
        which is why this is keyed on the module and not on `READERS`."""
        tree = ast.parse('import re\nre.split("[a-b]", s)\n"[a-b]".split(x)\n')
        found = sweep.regex_positions(tree)
        consts = [n for n in ast.walk(tree)
                  if isinstance(n, ast.Constant) and n.value == "[a-b]"]
        self.assertEqual(len(consts), 2)
        self.assertEqual([id(c) in found for c in consts], [True, False])

    def test_a_class_that_ended_really_ended(self):
        """The `not in_class` guard on `[`, which decides where the class STOPS. Inside a
        class a `[` is a literal, so treating it as an opening one leaves the scan still
        inside a class after the real `]` has closed it — and the next `x-y` in ordinary
        pattern text, where a dash is a literal dash, gets read as a range and held."""
        self.assertEqual(sweep.retune("[^[]x-y", regex=True), "[^[]y-z")

    def test_a_string_that_is_not_a_pattern_is_never_withheld_for_being_a_bad_one(self):
        """The backstop asks `re.compile` only where the program does. A comparison
        operand that happens to spell a character class has no regex syntax to be wrong
        about, and withholding its mutation would subtract a question for a reason that
        does not apply to it."""
        muts = _mutations(
            'def f(x):\n'
            '    return x == "[0-9] items"\n')
        offered = [m for m in muts if m.operator == "retune-string"]
        self.assertEqual([m.withheld for m in offered], [""])
        self.assertEqual([m.after for m in offered], ["'[1-0] jufnt'"])

    def test_a_withheld_mutation_never_reaches_a_sandbox(self):
        """It has its verdict before anything is applied, and that is the point: running
        it would measure an import error. The box raises if it is touched at all."""
        class _Untouchable:
            def clean_failures(self, modules):
                raise AssertionError("a withheld mutation was measured")

            def apply(self, mutation):
                raise AssertionError("a withheld mutation reached the tree")

        m = sweep.Mutation(path="p.py", line=1, end_line=1, operator="retune-string",
                           question="q", before="a", after="b", symbol="f",
                           withheld="the retuned pattern is not one re.compile accepts")
        verdict, outcome, full = sweep.decide(_Untouchable(), m, ["tests.test_x"])
        self.assertEqual(verdict, "withheld")
        self.assertIn("re.compile", outcome.detail)
        self.assertFalse(outcome.conclusive)
        self.assertIsNone(full)

    def test_a_withheld_verdict_gets_its_own_bucket_and_fails_nothing(self):
        """Its own bucket and not `unresolved`: the two say opposite things about what
        the tool knows, and a deliberate subtraction that rendered as a timeout is how a
        branch comes to sit behind a `no verdict` no re-run can clear (#693).

        **Beside the verdict and never instead of it** (#698) — so the case needs a
        measured mutation to sit beside, and it used to have none. A gate holding one
        withheld mutation and nothing else measured *nothing*, and #782 gives that its own
        sentence: the aside is the same either way, and the verdict in front of it is the
        honest one in both.
        """
        m = sweep.Mutation(path="p.py", line=1, end_line=1, operator="retune-string",
                           question="q", before="a", after="b", symbol="f",
                           withheld="not a pattern")
        held = sweep.Result(m, "withheld", None, None, [], None)
        gate = sweep.classify([held, _result("pinned")])
        self.assertEqual(len(gate.withheld), 1)
        self.assertEqual((gate.unresolved, gate.unapplied, gate.actionable), ([], [], []))
        self.assertEqual(sweep.headline(gate), "no survivors, 1 withheld")
        # And on its own it is not a clean sweep of anything — nothing was measured.
        alone = sweep.classify([held])
        self.assertEqual(sweep.gate_conclusion(alone), sweep.NOTHING)
        self.assertEqual(sweep.headline(alone), "nothing to sweep, 1 withheld")

    def test_a_string_with_nothing_to_move_offers_no_mutation(self):
        muts = _mutations("""
            def f(x):
                return x.split("/")
        """)
        self.assertEqual(_by(muts, "retune-string"), [])

    def test_a_docstring_is_prose_and_is_never_mutated(self):
        muts = _mutations('''
            MARK = "the mark"

            def f():
                """A docstring naming the mark, which is not a value anybody tests."""
                return MARK
        ''')
        self.assertEqual(_afters(muts, "retune-string"), ["'uif nbsl'"])

    def test_a_message_nobody_reads_back_is_not_a_read_position(self):
        """The scoping half, and the honest one. A key, a pattern and a separator decide
        what the program *does*; a log line decides what it *says*, and nothing in a suite
        is obliged to assert it. Measured on `charter/`: mutating every string took the
        tree from 7,006 mutations to 14,801 and spent the difference on prose."""
        muts = _mutations("""
            def f(log, d):
                log("could not reach the vault")
                return d["session"]
        """)
        self.assertEqual(_afters(muts, "retune-string"), ["'tfttjpo'"])

    def test_a_member_of_a_module_table_is_read_through_the_lookup_and_not_at_the_table(self):
        """The value of a module-level `NAME = "…"` is a claim — `MOUSE_ON` and `_MARK` are
        two of round two's five findings and both are scalars. Every string inside a
        container assigned to an upper-case name is not: measured on `charter/`, walking
        into containers takes the read positions from 2,826 to 4,065, and what those 1,239
        extra mutations ask for is a test per member of every membership table in the tree.
        Where a member genuinely decides something, the lookup that reads it is a read
        position on its own."""
        muts = _mutations("""
            MARK = "seen"
            NAMES = ("alpha", "beta")

            def f(x):
                return x in NAMES
        """)
        self.assertEqual(_afters(muts, "retune-string"), ["'tffo'"])

    def test_every_read_position_is_reached(self):
        muts = _mutations("""
            MARK = "alpha"

            def f(d, s, name):
                a = d["key"]
                b = d.get("other")
                c = {"third": 1}
                e = s.startswith("pre")
                g = " and ".join(name)
                h = "%s!" % name
                return a, b, c, e, g, h, MARK
        """)
        self.assertEqual(
            sorted(_afters(muts, "retune-string")),
            sorted(["'bmqib'", "'lfz'", "'puifs'", "'uijse'", "'qsf'", "' boe '",
                    "'%t!'"]))

    def test_the_width_literal_inside_an_fstring_is_reached_where_positions_are_exact(self):
        """#508's defect exactly: `f"{name:<28}"`, a layout claim with nothing measuring
        it, which had to be hand-checked because this operator did not exist. The segment's
        span carries no quotes, so the splice is raw text and `span_is_sound` proves the
        round-trip a different way — the bytes at the span ARE the characters of the value.

        BOTH arms are asserted, because the interesting one is the older interpreter. PEP
        701 landed in 3.12; before it `ast` gives an f-string's internal nodes approximate
        positions, the bytes at the span are not the value, and the check refuses. That is
        the tool asking one fewer question rather than making an edit it cannot describe —
        and :func:`sweep.reach` is what stops it being a silent difference.
        """
        muts = _mutations("""
            def f(name):
                return f"{name:<28}"
        """)
        if sys.version_info >= (3, 12):
            self.assertEqual(_afters(muts, "retune-string"), ["<39"])
        else:
            self.assertEqual(_by(muts, "retune-string"), [])

    def test_an_fstring_segment_whose_source_is_not_its_value_is_refused(self):
        """`{{` is two source characters and one value character, so a raw splice at that
        span would replace bytes the mutation cannot describe. Refused rather than guessed
        at — the same rule as every other unsound span."""
        muts = _mutations("""
            MARK = f"a{{b}}c{0:<28}"
        """)
        self.assertEqual(_afters(muts, "retune-string"),
                         ["<39"] if sys.version_info >= (3, 12) else [])

    def test_the_report_says_when_the_interpreter_puts_a_shape_out_of_reach(self):
        """A sweep that asks fewer questions than it could, and does not say so, is the
        quietest way this tool can mislead: the report reads exactly like a clean one. So
        the header names the shapes this interpreter cannot reach, next to the platform it
        measured on and for the same reason."""
        text = sweep.report([], Path("."), "a" * 12, "b" * 12, None, 1.0)
        self.assertEqual("f-string" in text, sys.version_info < (3, 12))
        self.assertIn(".".join(str(n) for n in sys.version_info[:3]), text)
        # And on the pull request too, which is the page anybody will actually read.
        summary = sweep.gate_summary(sweep.classify([]), "a" * 40, "b" * 40, 1.0, False)
        self.assertEqual("f-string" in summary, sys.version_info < (3, 12))

    def test_a_bytes_constant_is_retuned_as_bytes(self):
        muts = _mutations("""
            def f(ch):
                return ch == b"\\x03q"
        """)
        self.assertEqual(_afters(muts, "retune-string"), ["b'\\x03r'"])


class TheUnevaluatedPosition(unittest.TestCase):
    """#632: a position the interpreter never evaluates is not a read position.

    `sweep(...) -> tuple[list[Result], list["Pair"]]` came back a SURVIVOR on #630's own
    gate, and it is a survivor **no test can ever kill**: every module in this tree begins
    `from __future__ import annotations`, so that annotation is the string
    `"tuple[list[Result], list['Pair']]"` and nothing reads it. There is deliberately no
    suppression list to put such a thing on, so the operator must not offer it — an
    unkillable finding is the false positive the spec names as what gets a gate switched
    off.

    The rule is the PROPERTY and not the line: `"Pair"` is a `Subscript` slice, which is
    a read position for `d["components"]` and is not one for `list["Pair"]`. Every case
    below asserts one half — either that a deferred position really is skipped, or that
    something next door to it really is not.
    """

    def test_a_forward_reference_in_a_return_annotation_is_not_offered(self):
        """#632's own line, and the whole point: PEP 563 stores the annotation as source
        text, so retuning `"Pair"` changes the contents of a string nothing evaluates."""
        muts = _mutations("""
            from __future__ import annotations

            def sweep(root) -> tuple[list[Result], list["Pair"]]:
                return ([], [])
        """)
        self.assertEqual(_by(muts, "retune-string"), [])

    def test_an_argument_annotation_is_deferred_too_and_so_is_a_variable_one(self):
        """`returns` is one of three annotation positions and skipping only that one
        would leave `def _second_order(boxes: list["Sandbox"])` — a real line of this
        file — reporting a survivor nothing can kill."""
        muts = _mutations("""
            from __future__ import annotations

            def f(box: "Sandbox", xs: list["Pair"]) -> None:
                seen: dict[str, "Pair"] = {}
                return None
        """)
        self.assertEqual(_by(muts, "retune-string"), [])

    def test_the_value_beside_a_deferred_annotation_is_still_read(self):
        """An `AnnAssign`'s VALUE is evaluated exactly as any other assignment's is. Only
        the annotation is deferred, so `"Pair"` goes and `"left"` stays."""
        muts = _mutations("""
            from __future__ import annotations

            SLOT: dict[str, "Pair"] = {"left": 1}
        """)
        self.assertEqual(_afters(muts, "retune-string"), ["'mfgu'"])

    def test_a_subscript_that_really_is_a_lookup_is_untouched(self):
        """The narrowness that keeps this a scoping fix rather than a deletion. Dropping
        the `Subscript` arm of `read_positions` would silence the phantom AND `d["k"]`
        with it, which is the operator's single most common real position."""
        muts = _mutations("""
            from __future__ import annotations

            def f(d):
                return d["components"]
        """)
        self.assertEqual(_afters(muts, "retune-string"), ["'dpnqpofout'"])

    def test_without_the_future_import_the_annotation_is_an_evaluated_expression(self):
        """The narrow rule, and the reason it is narrow. Without PEP 563 the annotation
        IS evaluated where it is written, and what happens to a string inside it depends
        on what evaluates it — `typing.List["Pair"]` compiles the text into a `ForwardRef`
        right there at import. So here the string is live: a test that exercises whatever
        resolves the hint goes red without it, and the author of a branch the gate stopped
        has the ordinary move available. That is the whole line the predicate is drawn on
        — under `--enforce`, "unpinnable" means "blocked with nothing to do about it"."""
        muts = _mutations("""
            def sweep(root) -> tuple[list[Result], list["Pair"]]:
                return ([], [])
        """)
        self.assertEqual(_afters(muts, "retune-string"), ["'Qbjs'"])

    def test_only_the_annotations_future_defers_anything(self):
        """`from __future__ import division` is a `__future__` import and is not this
        one. A predicate that matched the module and not the name would silence a real
        read position in any file that imports a different future."""
        self.assertIs(sweep.defers_annotations(
            ast.parse("from __future__ import annotations\n")), True)
        self.assertIs(sweep.defers_annotations(
            ast.parse("from __future__ import division\n")), False)
        self.assertIs(sweep.defers_annotations(ast.parse("import __future__\n")), False)
        self.assertIs(sweep.defers_annotations(ast.parse("x = 1\n")), False)
        # One of several names on the one line still defers.
        self.assertIs(sweep.defers_annotations(
            ast.parse("from __future__ import division, annotations\n")), True)

    def test_the_neighbours_a_forward_reference_hides_among_are_checked_one_by_one(self):
        """The cases next door, each answered on whether the interpreter evaluates it.

        * `typing.cast("Pair", v)` — evaluated as an ordinary argument and then *thrown
          away*; `cast` returns its second argument and never reads the first.
        * `TypeVar(bound="Pair")` — the same, one keyword along.
        * a `NamedTuple` / `TypedDict` field written as a class annotation — deferred.
        * a `dataclasses.field` line — the annotation is deferred, the `field(...)` call
          beside it is not, and `"factory"` below is there to prove the difference.
        * `Literal["a", "b"]` — deferred, while the `== "a"` that reads the value is not.

        Asserted as one exact set rather than five absences, so that a rule which stopped
        deferring annotations fails here by producing MORE than this list.
        """
        muts = _mutations('''
            from __future__ import annotations

            import dataclasses
            import typing

            T = typing.TypeVar("T", bound="Pair")

            class Row(typing.NamedTuple):
                kids: list["Row"]

            class Cfg(typing.TypedDict):
                slot: "Slot"

            @dataclasses.dataclass
            class Held:
                pairs: list["Pair"] = dataclasses.field(default_factory=dict)
                mode: typing.Literal["a", "b"] = "a"

            def f(v, mode):
                return typing.cast("Pair", v), mode == "a", {"factory": 1}
        ''')
        self.assertEqual(_afters(muts, "retune-string"), ["'b'", "'gbdupsz'"])

    def test_a_typed_dict_spelled_as_a_call_keeps_its_field_names(self):
        """The boundary the rule is drawn on is EVALUATION and not "looks like typing".
        `TypedDict("Cfg", {"slot": str})` is an ordinary call in an ordinary expression:
        `"slot"` becomes a field name at runtime, so it is read and stays mutable."""
        muts = _mutations("""
            from __future__ import annotations

            import typing

            Cfg = typing.TypedDict("Cfg", {"slot": str})
        """)
        self.assertEqual(_afters(muts, "retune-string"), ["'tmpu'"])

    def test_the_deferred_ids_are_the_annotation_and_nothing_around_it(self):
        """`unevaluated` walks the annotation subtree only. Widened by one step it would
        swallow the `AnnAssign`'s value, the argument's default and the function body."""
        tree = ast.parse(textwrap.dedent("""
            from __future__ import annotations

            def f(x: list["A"] = ["B"]) -> "C":
                y: "D" = "E"
                return y
        """))
        deferred = sweep.unevaluated(tree)
        inside = sorted(n.value for n in ast.walk(tree)
                        if isinstance(n, ast.Constant) and isinstance(n.value, str)
                        and id(n) in deferred)
        self.assertEqual(inside, ["A", "C", "D"])

    def test_a_module_that_defers_nothing_defers_nothing(self):
        self.assertEqual(sweep.unevaluated(ast.parse('def f() -> "C": return 1\n')), set())


class TheBoundaryShape(unittest.TestCase):
    """`shift-boundary` (#569): `<` against `<=`, and nothing else moved.

    The general half of the near-synonym family. Two comparisons that accept the same
    values one number apart, so the mutant always runs and asks exactly one question — is
    the EDGE pinned, or only the direction? `drop-if` cannot ask it: a test that never
    approaches the edge answers "yes, the refusal exists" perfectly well.
    """

    def test_a_strict_comparison_is_offered_one_notch_wider(self):
        muts = _mutations("""
            def _window(sel, top):
                if sel < top:
                    return sel
                return top
        """)
        self.assertEqual(_afters(muts, "shift-boundary"), ["((sel) <= (top))"])

    def test_an_inclusive_comparison_is_offered_one_notch_narrower(self):
        muts = _mutations("""
            def f(n, cap):
                return n <= cap
        """)
        self.assertEqual(_afters(muts, "shift-boundary"), ["((n) < (cap))"])

    def test_a_chain_moves_one_link_and_respells_the_rest(self):
        """`0 <= i < n` is ONE node. Moving one link means writing the whole chain back
        out, and a link this tool could not spell would vanish from the mutant — an edit
        that is not the edit the report describes."""
        muts = _mutations("""
            def f(i, n):
                return 0 <= i < n
        """)
        self.assertEqual(sorted(_afters(muts, "shift-boundary")),
                         ["((0) < (i) < (n))", "((0) <= (i) <= (n))"])

    def test_a_link_that_is_not_a_boundary_is_carried_through_untouched(self):
        muts = _mutations("""
            def f(a, b, c):
                return a < b == c
        """)
        self.assertEqual(_afters(muts, "shift-boundary"), ["((a) <= (b) == (c))"])

    def test_every_comparison_operator_python_has_is_spelled(self):
        """`CMP_TEXT` is subscripted directly, with no fallback, so an operator missing
        from it would be a `KeyError` in the middle of a sweep. There is no runtime guard
        for that — a guard nothing can reach is a line this tool would delete — so the
        completeness lives here, where it fails on the day Python grows an eleventh
        comparison rather than the day somebody's sweep crashes."""
        self.assertEqual(set(ast.cmpop.__subclasses__()), set(sweep.CMP_TEXT))

    def test_equality_is_not_a_boundary(self):
        """`!=` is the negation of `==`, not a near-synonym of it, and inverting a whole
        condition is a coarser question this table does not ask."""
        muts = _mutations("""
            def f(a):
                return a == 3
        """)
        self.assertEqual(_by(muts, "shift-boundary"), [])


class TheSynonymShape(unittest.TestCase):
    """`swap-synonym` and `drop-normalise` (#569): one documented axis, moved.

    Each pair in :data:`sweep.SYNONYMS` is two standard-library names that do the same job
    and differ along exactly one axis, so the mutant is type-correct by construction and a
    red means a test noticed the AXIS rather than a test noticing a crash. Nothing
    charter-specific is in the table — the same discipline as `NARROW_TO`.
    """

    def test_a_case_fold_is_swapped_for_its_opposite(self):
        muts = _mutations("""
            def f(name):
                return name.lower()
        """)
        self.assertEqual(_afters(muts, "swap-synonym"), ["name.upper"])

    def test_an_anchor_is_swapped_end_for_end(self):
        muts = _mutations("""
            def f(name):
                return name.startswith("x")
        """)
        self.assertEqual(_afters(muts, "swap-synonym"), ["name.endswith"])

    def test_an_ordering_is_asked_about_by_dropping_it(self):
        muts = _mutations("""
            def f(xs):
                return sorted(xs)
        """)
        self.assertEqual(_afters(muts, "swap-synonym"), ["list"])

    def test_a_call_whose_swap_would_be_a_type_error_is_not_offered(self):
        """`sorted(xs, key=f)` -> `list(xs, key=f)` raises `TypeError`, which reddens the
        suite for a reason that has nothing to do with the ordering. That is a FALSE PIN,
        and a false pin is the failure this whole file exists to prevent — so the operator
        declines rather than scoring a point it did not earn."""
        muts = _mutations("""
            def f(xs, key):
                return sorted(xs, key=key)
        """)
        self.assertEqual(_by(muts, "swap-synonym"), [])

    def test_a_normalisation_is_dropped_to_its_receiver(self):
        """#572's own shape: the map keys were `resolve()`d on both sides and the prefix
        that chose them was not. A test whose paths carry no symlink cannot tell the
        mutant from the shipped line, which is exactly what the sweep should say."""
        muts = _mutations("""
            def f(p):
                return p.resolve()
        """)
        self.assertEqual(_afters(muts, "drop-normalise"), ["p"])

    def test_a_normalisation_with_arguments_is_left_alone(self):
        muts = _mutations("""
            def f(p):
                return p.resolve(strict=True)
        """)
        self.assertEqual(_by(muts, "drop-normalise"), [])

    def test_a_swap_that_is_not_type_correct_everywhere_is_not_in_the_table(self):
        """`index` is on `list`, `tuple` and `str`; `rindex` is on `str` alone. Swapping
        it would turn `args.index("-m")` into an `AttributeError` — a red for a reason that
        has nothing to do with which end was searched, which is a false pin. Measured
        before the pair was dropped: all five `.index(` calls in `charter/` are on lists."""
        self.assertNotIn("index", sweep.SYNONYMS)
        muts = _mutations("""
            def f(args):
                return args.index("-m")
        """)
        self.assertEqual(_by(muts, "swap-synonym"), [])

    def test_a_function_in_a_module_is_not_a_method_on_a_type(self):
        """`shlex.split` and `re.split` live in a namespace with no `rsplit` in it at all,
        so the swap is an `AttributeError` rather than a question about which end was
        searched. The pair is justified by two methods on one TYPE, and a module is not an
        instance of anything. Measured on `charter/`: eight such call sites.

        The `maxsplit` on the second call is what keeps it a question at all rather than
        scenery — see :func:`sweep.indistinguishable`."""
        muts = _mutations("""
            import shlex

            def f(command, name):
                return shlex.split(command), name.split(",", 1)
        """)
        self.assertEqual(_afters(muts, "swap-synonym"), ["name.rsplit"])

    def test_a_module_reached_through_a_package_is_recognised_too(self):
        muts = _mutations("""
            import os

            def f(p):
                return os.path.split(p)
        """)
        self.assertEqual(_by(muts, "swap-synonym"), [])

    def test_every_pair_in_the_table_names_the_one_axis_it_moves(self):
        """The table is the justification. A pair with no axis written down is a swap
        somebody liked the look of, and that is how this becomes a general-purpose mutation
        engine that reddens code for reasons nobody can read."""
        for name, (other, axis) in sweep.SYNONYMS.items():
            self.assertTrue(axis and isinstance(axis, str), name)
            self.assertNotEqual(name, other)


class TheMutantIsTheEditDescribed(unittest.TestCase):
    """`parenthesised` (#655): the mutation applied is the mutation the report prints.

    Measured over `charter/` and `tools/` before this existed: **144 of 8,903 expression
    mutations spliced something else**, because an `ast` node's span excludes the
    parentheses a programmer put around it. `(x or "").strip()` became
    ``x or "".lstrip()`` — not a swap of which end is stripped but a deletion of the
    strip on every path but one — and `(vc or {}).get("config", {})` became
    ``vc or {}["config"]``, which is a `KeyError` and therefore a FALSE PIN, the failure
    this whole file exists to prevent.

    The shapes below are the real ones from that measurement, and each is written to die
    if the mutant stops standing exactly where it was put.
    """

    def _every_splice_stands_where_it_was_put(self, source: str, least: int = 1) -> None:
        """The property, not an example of it: bracketing the mutant changes nothing.

        A splice that re-associates is exactly a splice whose result would parse
        DIFFERENTLY with brackets around it, so this is the whole claim and it needs no
        table of precedences to state. Two splices are not expressions at all and are
        skipped rather than fudged: a statement deletion, and an f-string's literal text.
        """
        blob = textwrap.dedent(source).lstrip("\n").encode("utf-8")
        tree = ast.parse(blob)
        sp = sweep._Spans(blob)
        raw = {sp.span(part) for node in ast.walk(tree)
               if isinstance(node, ast.JoinedStr)
               for part in node.values if isinstance(part, ast.Constant)}
        muts = sweep.mutations_for(
            "charter/x.py", blob, set(range(1, len(blob.splitlines()) + 2)))
        self.assertGreaterEqual(len(muts), least)
        checked = 0
        for m in muts:
            if m.span in raw:
                continue
            try:
                ast.parse(m.after, mode="eval")
            except SyntaxError:
                continue
            a, b = m.span
            bracketed = blob[:a] + b"(" + m.after.encode("utf-8") + b")" + blob[b:]
            with self.subTest(mutation=str(m)):
                self.assertEqual(
                    ast.dump(ast.parse(m.source)), ast.dump(ast.parse(bracketed)),
                    f"{m.tag}: the mutant re-associates — the program it makes is not "
                    f"the edit the report describes ({m.before!r} -> {m.after!r})")
            checked += 1
        self.assertGreater(checked, 0)

    def test_a_grouped_receiver_keeps_its_grouping(self):
        """137 of the 144 are this line, in one spelling or another: the receiver is an
        `or` chain, its parentheses are grouping rather than syntax, and `ast` does not
        put them in the span. The mutant the report called "which side is stripped"
        stripped neither side."""
        muts = _mutations("""
            def f(p):
                return (p.stderr or p.stdout or "").strip()
        """)
        self.assertEqual(_afters(muts, "swap-synonym"),
                         ['(p.stderr or p.stdout or "").lstrip'])
        self._every_splice_stands_where_it_was_put("""
            def f(p):
                return (p.stderr or p.stdout or "").strip()
        """)

    def test_a_fallback_on_a_grouped_receiver_does_not_become_a_key_error(self):
        """The false pin, which is the half that matters. `vc or {}["config"]` evaluates
        the subscript only when `vc` is falsy, and `{}["config"]` raises — so the suite
        goes red for a crash and the fallback is certified as tested. Four sites in
        `charter/` are exactly this."""
        muts = _mutations("""
            def f(vc):
                return (vc or {}).get("config", {})
        """)
        self.assertIn('(vc or {})["config"]', _afters(muts, "no-fallback"))

    def test_a_fallback_written_as_an_or_keeps_its_receiver_grouped_too(self):
        """The other `no-fallback` spelling, `d.get(k) or ()`, builds the same subscript
        from the same receiver — and a test that only covered `d.get(k, v)` left this one
        unpinned. Found by deleting the line."""
        muts = _mutations("""
            def f(vc):
                return (vc or {}).get("config") or {}
        """)
        self.assertIn('(vc or {})["config"]', _afters(muts, "no-fallback"))

    def test_a_conjunct_that_is_itself_an_or_keeps_its_grouping(self):
        """`and` binds tighter than `or`, so a half spelled `a or b` joined back with
        ` and ` reads as `a or (b and c)` — a different condition from the one dropped.
        Also found by deleting the line: the two-conjunct fixtures could not reach it,
        because a lone survivor gets its brackets from the outer spelling instead."""
        muts = _mutations("""
            def f(a, b, c, d):
                if (a or b) and c and d:
                    return 1
        """)
        self.assertIn("((a or b) and c)", _afters(muts, "drop-conjunct"))

    def test_a_clamp_dropped_inside_a_tighter_expression_keeps_its_grouping(self):
        """`2 * max(0, inner - w)` -> `2 * inner - w` is not the clamp dropped, it is
        arithmetic rewritten."""
        muts = _mutations("""
            def f(inner, w):
                return 2 * max(0, inner - w)
        """)
        self.assertIn("(inner - w)", _afters(muts, "unclamp"))

    def test_a_containment_dropped_beside_a_concatenation_keeps_its_grouping(self):
        muts = _mutations("""
            def f(detail):
                return contain.one_line(detail[-1] if detail else "no output") + "!"
        """)
        self.assertEqual(_afters(muts, "uncontain"),
                         ['(detail[-1] if detail else "no output")'])

    def test_an_fstring_format_spec_is_text_and_never_grows_brackets(self):
        """A format spec's span holds raw bytes and not an expression, so `retune-string`
        splices text there. `x-y` happens to parse as a `BinOp`, and bracketing it would
        put `(y-z)` inside the string — a corruption, not a mutation.

        BOTH arms, for the reason the width-literal case above gives at length: PEP 701
        landed in 3.12, and before it an f-string's internal positions are approximate, so
        `span_is_sound` refuses the segment and there is nothing here to protect. That is
        `sweep.reach`'s declared gap rather than a different answer to this question."""
        muts = _mutations("""
            def f(v):
                return f"{v:x-y}"
        """)
        if sys.version_info >= (3, 12):
            self.assertEqual(_afters(muts, "retune-string"), ["y-z"])
            self.assertIn('f"{v:y-z}"', _by(muts, "retune-string")[0].source.decode())
        else:
            self.assertEqual(_by(muts, "retune-string"), [])

    def test_text_that_is_not_an_expression_is_returned_untouched(self):
        """A statement deletion splices nothing, or `pass`. Neither is an expression and
        neither may grow brackets."""
        self.assertEqual(sweep.parenthesised(""), "")
        self.assertEqual(sweep.parenthesised("pass"), "pass")
        self.assertEqual(sweep.parenthesised("<39"), "<39")

    def test_a_bracket_already_around_the_whole_expression_is_not_doubled(self):
        """`drop-conjunct` spells each half and then the join is spelled again, so this
        runs twice over the same text. `((prog != "export"))` is a mutant nobody reads."""
        self.assertEqual(sweep.parenthesised("(a and b)"), "(a and b)")
        self.assertEqual(sweep.parenthesised("(a) and b"), "((a) and b)")

    def test_a_tight_expression_is_left_alone(self):
        for text in ("name", "d[k]", "f(x)", "p.stem", '"lit"', "[1, 2]", "{}"):
            self.assertEqual(sweep.parenthesised(text), text)

    def test_every_expression_python_has_is_either_loose_or_tight(self):
        """The same protection `CMP_TEXT` gets, for the same reason: an expression node
        in neither tuple would be treated as tight by default, and a splice that
        re-associates is a mutation that is not the mutation the report describes. This
        fails on the day Python grows a new expression, rather than on the day somebody's
        survivor turns out to be a different program."""
        self.assertEqual(set(ast.expr.__subclasses__()),
                         set(sweep.LOOSE) | set(sweep.TIGHT))
        self.assertEqual(set(sweep.LOOSE) & set(sweep.TIGHT), set())

    def test_every_shape_in_the_table_stands_where_it_was_put(self):
        """The property over one module carrying every operator that splices source text
        back into the tree, each in a position tight enough to rebuild it."""
        self._every_splice_stands_where_it_was_put("""
            BUDGET = 3 + 4

            def f(p, vc, detail, xs, n, cap, w):
                a = 2 * max(0, n - w)
                b = (p.stderr or "").strip()
                c = (vc or {}).get("config", {})
                d = contain.one_line(detail[-1] if detail else "x") + "!"
                e = 2 * (n if cap else n - 1)
                g = 2 * ((vc or {}).get("k") or ())
                h = -(n if cap else n - 1)
                if isinstance(n, int) and n - 1 < cap:
                    return a, b, c, d, e, g, h
                return (p.name or "").lower()
        """, least=10)


class TheMutantThatAsksNothing(unittest.TestCase):
    """`indistinguishable` (#655): a mutation that cannot be killed is not offered.

    #655 proposed a third verdict beside `pinned`/`survived` for the mutant no honest
    test can redden. The measurement it asked for first refuses it: across every sweep
    result this repository still holds — 461 distinct survivors on ten branches — the
    number a rule could decide is one. `path.partition("/")` is equivalent only if a
    GitHub `path_with_namespace` holds one slash, which is a fact about a remote API and
    not about the code; `GIT_TIMEOUT = 20 -> 21` would have to be decided from "no
    covering test names the symbol", which is the report's loudest FINDING and is lifted
    by renaming the constant.

    What is decidable is answered where #632 answered its sibling — the operator does not
    offer the mutation. A survivor no test can kill is a false positive, and the place to
    fix a false positive is the question, not the answer.
    """

    def test_a_split_with_no_maxsplit_is_not_offered(self):
        muts = _mutations("""
            def f(path):
                return path.split("/")
        """)
        self.assertEqual(_by(muts, "swap-synonym"), [])

    def test_an_rsplit_with_no_maxsplit_is_not_offered_either(self):
        muts = _mutations("""
            def f(path):
                return path.rsplit("/")
        """)
        self.assertEqual(_by(muts, "swap-synonym"), [])

    def test_the_whitespace_form_is_the_same_function_too(self):
        muts = _mutations("""
            def f(text):
                return text.split()
        """)
        self.assertEqual(_by(muts, "swap-synonym"), [])

    def test_a_maxsplit_makes_it_a_question_again(self):
        muts = _mutations("""
            def f(path):
                return path.split("/", 1)
        """)
        self.assertEqual(_afters(muts, "swap-synonym"), ["path.rsplit"])

    def test_a_maxsplit_passed_by_keyword_counts_too(self):
        muts = _mutations("""
            def f(path):
                return path.split("/", maxsplit=1)
        """)
        self.assertEqual(_afters(muts, "swap-synonym"), ["path.rsplit"])

    def test_the_pair_really_is_one_function_without_a_maxsplit(self):
        """The evidence for the rule rather than a restatement of it. Exhaustive over
        every string of up to five characters drawn from an alphabet that CONTAINS the
        separator, for `str` and for `bytes`, plus the no-argument whitespace form."""
        import itertools
        for n in range(6):
            for letters in itertools.product("ab.", repeat=n):
                s = "".join(letters)
                self.assertEqual(s.split(), s.rsplit(), s)
                for sep in (".", "a", "ab"):
                    self.assertEqual(s.split(sep), s.rsplit(sep), (s, sep))
                    self.assertEqual(s.encode().split(sep.encode()),
                                     s.encode().rsplit(sep.encode()), (s, sep))
        self.assertNotEqual("a.b.c".split(".", 1), "a.b.c".rsplit(".", 1))

    def test_a_pair_that_does_move_its_axis_is_untouched(self):
        muts = _mutations("""
            def f(name):
                return name.lower()
        """)
        self.assertEqual(_afters(muts, "swap-synonym"), ["name.upper"])

    def test_the_refusal_names_the_reason_rather_than_dropping_it_silently(self):
        call = ast.parse('x.split(",")', mode="eval").body
        self.assertIn("maxsplit", sweep.indistinguishable("split", "rsplit", call))
        self.assertEqual(sweep.indistinguishable("lower", "upper", call), "")
        with_max = ast.parse('x.split(",", 1)', mode="eval").body
        self.assertEqual(sweep.indistinguishable("split", "rsplit", with_max), "")


class TheNonDecimalConstantShape(unittest.TestCase):
    """An integer written in base 8 or 16 was written that way because its digits matter."""

    def test_a_permission_is_moved_by_one_and_stays_octal(self):
        muts = _mutations("""
            def f(p):
                p.chmod(0o600)
        """)
        self.assertEqual(_afters(muts, "retune-constant"), ["0o601"])

    def test_a_decimal_literal_in_the_same_position_is_not_retuned(self):
        """Every `0`, `1` and `2` index in the tree would otherwise become a mutation. The
        thresholds that matter are reached by `shift-boundary` instead, which asks the same
        question of `x > 28` without asking it of `xs[0]`."""
        muts = _mutations("""
            def f(p):
                p.chmod(384)
        """)
        self.assertEqual(_by(muts, "retune-constant"), [])


class TheSplicePreservesEveryLineNumber(unittest.TestCase):
    def test_a_deleted_multi_line_statement_leaves_the_rest_where_it_was(self):
        """A survivor reported at the wrong line is a survivor nobody acts on."""
        muts = _mutations("""
            def f(arm, pane):
                if arm is None or not RE.fullmatch(pane):
                    return []
                marker = 1
                return marker
        """)
        drop = _by(muts, "drop-if")[0]
        after = drop.source.decode().splitlines()
        self.assertEqual(len(after), 5)
        self.assertEqual(after[3].strip(), "marker = 1")

    def test_a_replacement_never_shortens_the_file(self):
        muts = _mutations("""
            def f(x):
                y = max(0, min(x,
                               10))
                return y
        """)
        for m in _by(muts, "unclamp"):
            self.assertEqual(len(m.source.decode().splitlines()), 4, m.after)

    def test_offsets_are_counted_in_bytes_and_not_characters(self):
        """`ast`'s `col_offset` is a UTF-8 byte offset. charter's docstrings carry `↑↓⏎`
        and `·`, so a splice that indexed by character would land three bytes early on
        every line after one and quietly corrupt the mutant."""
        muts = _mutations("""
            def f(x):
                note = "· ↑↓ ⏎"
                return max(0, x)
        """)
        m = _by(muts, "unclamp")[0]
        text = m.source.decode()
        self.assertIn('note = "· ↑↓ ⏎"', text)
        self.assertRegex(text, r"return (0|x)\n")


class EverySpliceChecksItsOwnSpan(unittest.TestCase):
    """A mutation reported at a line it did not change is worse than one not offered."""

    def _first_expr(self, source: str):
        blob = textwrap.dedent(source).lstrip("\n").encode()
        tree = ast.parse(blob)
        sp = sweep._Spans(blob)
        node = next(n for n in ast.walk(tree) if isinstance(n, ast.IfExp))
        return sp, node

    def test_a_span_that_reparses_into_the_same_tree_is_sound(self):
        sp, node = self._first_expr("x = a if c else b\n")
        self.assertTrue(sweep.span_is_sound(sp, node))

    def test_an_expression_broken_over_two_lines_is_still_sound(self):
        """An expression's span stops INSIDE the brackets that made the line break legal,
        so `a if c\\n else b` is a SyntaxError on its own and a perfectly good mutation in
        place. Without the parentheses this check rejects it, and two real `decode`
        mutations on #554 went missing."""
        sp, node = self._first_expr("""
            key = (_TILDE_KEYS.get(params) if final == b"~"
                   else _CSI_KEYS.get(final))
        """)
        self.assertTrue(sweep.span_is_sound(sp, node))

    def test_a_span_pointing_at_the_wrong_bytes_is_refused(self):
        """Before 3.12 and PEP 701, `ast` gave only approximate positions for expressions
        inside an f-string — and `contain.one_line(…)` is inside an f-string nearly
        everywhere it appears here, which is what `uncontain` exists to mutate. A splice
        a few bytes off can still PARSE, so the parse check alone would not catch it."""
        blob = b"x = a if c else b\n"
        tree = ast.parse(blob)
        node = next(n for n in ast.walk(tree) if isinstance(n, ast.IfExp))
        shifted = sweep._Spans(b"# a comment first\n" + blob)
        self.assertFalse(sweep.span_is_sound(shifted, node))

    def test_a_statement_span_round_trips_with_its_body(self):
        blob = b"def f(a):\n    if a is None:\n        return []\n"
        tree = ast.parse(blob)
        sp = sweep._Spans(blob)
        node = next(n for n in ast.walk(tree) if isinstance(n, ast.If))
        self.assertTrue(sweep.span_is_sound(sp, node))


class TheScopeIsTheDiff(unittest.TestCase):
    def test_a_mutation_outside_the_charged_lines_is_not_offered(self):
        """A PR is answerable for the guards IT adds."""
        source = """
            def old(a):
                if a is None:
                    return []

            def new(b):
                if b is None:
                    return []
        """
        self.assertEqual([m.line for m in _mutations(source, lines={6, 7})], [6])

    def test_a_node_spanning_into_the_charged_lines_counts(self):
        """A branch that edits the second line of a two-line condition has changed the
        guard, and charging only nodes that START on a touched line would miss it."""
        source = """
            def f(arm, pane):
                if arm is None or \\
                        not RE.fullmatch(pane):
                    return []
        """
        self.assertTrue(_mutations(source, lines={3}))


class TheDiffIsReadWithNoContext(unittest.TestCase):
    """`--unified=0`, so three lines of untouched context are not charged to the branch."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="charter-sweep-diff-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp, ignore_errors=True))
        # Sealed off from the machine's git: a global `core.hooksPath`, a signing key or
        # a template dir would run this fixture through whatever the developer has
        # installed, and one of those costs a full minute per commit here.
        env = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull,
                   GIT_CONFIG_SYSTEM=os.devnull, GIT_CONFIG_NOSYSTEM="1",
                   GIT_TERMINAL_PROMPT="0")

        def run(*a):
            subprocess.run(("git", "-c", "core.hooksPath=", "-c", "commit.gpgsign=false")
                           + a, cwd=self.tmp, check=True, env=env, timeout=60,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        run("init", "-q", "-b", "main")
        run("config", "user.email", "sweep@example.invalid")
        run("config", "user.name", "sweep")
        (self.tmp / "charter").mkdir()
        self.f = self.tmp / "charter" / "m.py"
        self.f.write_text("a = 1\nb = 2\nc = 3\nd = 4\ne = 5\nf = 6\ng = 7\n")
        run("add", "-A")
        run("commit", "-qm", "base")
        self.base = sweep.git("rev-parse", "HEAD", cwd=self.tmp).strip()
        self.f.write_text("a = 1\nb = 2\nc = 3\nd = 44\ne = 5\nf = 6\ng = 7\n")
        run("add", "-A")
        run("commit", "-qm", "change")
        self.head = sweep.git("rev-parse", "HEAD", cwd=self.tmp).strip()

    def test_only_the_changed_line_is_charged(self):
        found = sweep.added_lines(self.tmp, self.base, self.head, ("charter",))
        self.assertEqual(found, {"charter/m.py": {4}})

    def test_a_file_the_branch_did_not_touch_is_absent(self):
        (self.tmp / "charter" / "other.py").write_text("x = 1\n")
        found = sweep.added_lines(self.tmp, self.base, self.head, ("charter",))
        self.assertNotIn("charter/other.py", found)


# ======================================================================================
# The verdict — the asymmetry that decides the whole design
# ======================================================================================

class _FakeBox:
    """A sandbox that answers whatever the case needs it to.

    `subset` may be a list, which is then consumed one answer per call — that is how a
    run that goes red once and green on confirmation is spelled.
    """

    def __init__(self, subset=None, full: sweep.Outcome | None = None,
                 clean: frozenset = frozenset()):
        self._subset = subset if isinstance(subset, list) else [subset] * 4
        self._full = full
        self._clean = clean
        self.applied: list[sweep.Mutation] = []
        self.subset_calls = 0
        self.full_calls = 0

    def clean_failures(self, modules):
        return self._clean

    def apply(self, m):
        self.applied.append(m)

    def restore(self):
        pass

    def subset(self, modules):
        self.subset_calls += 1
        return self._subset[min(self.subset_calls - 1, len(self._subset) - 1)]

    def full(self):
        self.full_calls += 1
        return self._full


class _FullFlake(_FakeBox):
    """Green subset, then a full suite that answers differently each time it is asked."""

    def __init__(self, first, second):
        super().__init__(subset=sweep.Outcome(True, 40, "OK"))
        self._answers = [first, second]

    def full(self):
        self.full_calls += 1
        return self._answers[min(self.full_calls - 1, len(self._answers) - 1)]


_M = sweep.Mutation(path="charter/x.py", line=1, end_line=1, operator="drop-if",
                    question="?", before="if a: return", after="", symbol="f")


class TheFullSuiteHasTheLastWord(unittest.TestCase):
    def test_a_survivor_of_its_subset_is_re_run_against_everything(self):
        """Selection is an optimisation and must never be the final word. This is the
        line that makes a false 'pinned' impossible, and it costs one full run."""
        box = _FakeBox(subset=sweep.Outcome(True, 40, "OK"),
                       full=sweep.Outcome(False, 6000, "FAILED", failing=frozenset({"m.C.t"})))
        verdict, _, full = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "pinned")
        self.assertGreaterEqual(box.full_calls, 1)
        self.assertEqual(full.ran, 6000)

    def test_a_survivor_of_everything_is_reported(self):
        box = _FakeBox(subset=sweep.Outcome(True, 40, "OK"),
                       full=sweep.Outcome(True, 6000, "OK"))
        verdict, _, _ = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "survived")

    def test_a_red_full_suite_is_confirmed_before_it_pins_anything(self):
        """This is where the asymmetry bites hardest. A red FULL suite is the verdict that
        says 'tested', and it is never revisited — so a flake here does not mislabel a
        mutation, it certifies a guard. Six thousand tests, real tmux servers, and a
        machine that may be running other sweeps: one confirming run is expensive and
        still cheaper than one guard wrongly declared safe."""
        box = _FullFlake(sweep.Outcome(False, 6000, "FAILED", failing=frozenset({"m.C.t"})),
                         sweep.Outcome(True, 6000, "OK"))
        verdict, _, full = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "survived")
        self.assertEqual(box.full_calls, 2)
        self.assertIn("green on confirmation", full.detail)

    def test_a_full_suite_red_twice_pins_the_guard(self):
        box = _FullFlake(sweep.Outcome(False, 6000, "FAILED", failing=frozenset({"m.C.t"})),
                         sweep.Outcome(False, 6000, "FAILED", failing=frozenset({"m.C.t"})))
        verdict, _, _ = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "pinned")
        self.assertEqual(box.full_calls, 2)

    def test_a_red_subset_is_confirmed_once_and_then_believed(self):
        """A red twice IS a red, and re-running the whole suite for it would spend four
        minutes to learn nothing. The asymmetry runs one way only."""
        box = _FakeBox(subset=sweep.Outcome(False, 40, "FAILED", failing=frozenset({"m.C.t"})))
        verdict, _, full = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "pinned")
        self.assertEqual(box.subset_calls, 2)
        self.assertEqual(box.full_calls, 0)
        self.assertIsNone(full)

    def test_a_red_that_does_not_reproduce_is_not_allowed_to_pin_anything(self):
        """The suite starts real tmux servers and several sweeps share a machine. A flaky
        red does not merely mislabel one mutation — it certifies a guard as tested by a
        failure that had nothing to do with it, which is the exact defect this tool was
        written to catch. So a red that does not reproduce goes to the full suite."""
        box = _FakeBox(subset=[sweep.Outcome(False, 40, "FAILED", failing=frozenset({"m.C.t"})),
                               sweep.Outcome(True, 40, "OK")],
                       full=sweep.Outcome(True, 6000, "OK"))
        verdict, subset, _ = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "survived")
        self.assertEqual(box.full_calls, 1)
        self.assertIn("green on confirmation", subset.detail)

    def test_a_file_no_traced_module_reaches_goes_to_the_full_suite(self):
        """Absence of evidence is not a pin. A file the trace never saw — because its
        only exercise is through a subprocess, say — must not be certified as tested by
        that silence."""
        box = _FakeBox(full=sweep.Outcome(True, 6000, "OK"))
        verdict, subset, _ = sweep.decide(box, _M, [])
        self.assertEqual(verdict, "survived")
        self.assertEqual(box.subset_calls, 0)
        self.assertEqual(box.full_calls, 1)
        self.assertIn("no covering module", subset.detail)


_Completed = sweep._Finished


class AHangIsNotAPass(unittest.TestCase):
    """Round three had a mutation that wedged the suite for 1800s rather than failing.

    Reading that as green would have pinned a guard on a timeout — the loudest possible
    version of the false pin this whole file exists to prevent.
    """

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="charter-sweep-hang-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp, ignore_errors=True))
        (self.tmp / "tests").mkdir()
        (self.tmp / "tests" / "__init__.py").write_text("")
        (self.tmp / "tests" / "test_hang.py").write_text(textwrap.dedent(f"""
            import subprocess, unittest
            class T(unittest.TestCase):
                def test_wedges(self):
                    kid = subprocess.Popen(["sleep", "120"])
                    open({str(self.tmp / "kid.pid")!r}, "w").write(str(kid.pid))
                    kid.wait()
        """))
        self.box = object.__new__(sweep.Sandbox)
        self.box.path = self.tmp
        self.box._pristine = {}

    def test_a_run_that_wedges_is_red_and_never_a_survivor(self):
        outcome = self.box.run(["tests.test_hang"], timeout=5)
        self.assertFalse(outcome.green)
        self.assertIn("timed out", outcome.detail)

    def test_a_wedged_runs_grandchildren_die_with_it(self):
        """`subprocess.run(timeout=…)` kills only the direct child. This suite starts
        real tmux servers, and one left behind by a timed-out mutation would be inherited
        by the next mutation and reported as ITS failure.

        Polled rather than asserted once: SIGKILL is delivered synchronously but the
        orphan is reaped by init a moment later, and until it is reaped its pid still
        answers `kill(pid, 0)`. Asserting immediately made this case flaky on CI — it
        failed on 3.11 in one run and passed in the next at the same commit, which is
        exactly the kind of red this whole tool exists to stop anybody trusting.
        """
        self.box.run(["tests.test_hang"], timeout=5)
        pid = int((self.tmp / "kid.pid").read_text())
        for _ in range(100):
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return
            time.sleep(0.1)
        self.fail(f"pid {pid} outlived the run it was started from")

    def test_killing_a_group_that_has_already_gone_is_not_an_error(self):
        proc = subprocess.Popen([sys.executable, "-c", "pass"], start_new_session=True)
        proc.wait()
        sweep._kill_group(proc)


class AMutantThatEatsTheMachineIsARedAndNotAnOutage(unittest.TestCase):
    """#773, the sixth way a sweep lies and the one the timeouts cannot reach.

    `SUBSET_TIMEOUT` and `FULL_TIMEOUT` answer "a mutation that hangs". They do not answer
    this, and the reason is exact: **the timer dies with the process it is timing.** A
    mutant that allocates without bound exhausts the runner in two to four minutes against
    a 900 s cap, the host kills the machine, and the shard reports

        ##[error]The runner has received a shutdown signal…
        ##[error]Process completed with exit code 143.

    which is byte-for-byte a spot reclaim. Measured on #710: four of its 118 mutations
    never terminate, the fastest growing at 4,274 MB/min, and on run 33331151759 the one
    shard holding a memory-eater is the one shard that died — while the shard holding the
    mutation that spins WITHOUT allocating survived on the timeout, which is the control.

    Capped, that becomes a `MemoryError` in the child: a red, on tests that were green
    unmutated, which `decide` reads as a **pin**. That is the right verdict for a mutant
    that destroys the apparatus.
    """

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="charter-sweep-cap-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp, ignore_errors=True))
        (self.tmp / "tests").mkdir()
        (self.tmp / "tests" / "__init__.py").write_text("")
        (self.tmp / "tests" / "test_limit.py").write_text(textwrap.dedent("""
            import os, resource, unittest
            class T(unittest.TestCase):
                def test_reports_the_limit_it_was_given(self):
                    soft, _ = resource.getrlimit(resource.RLIMIT_AS)
                    self.assertEqual(soft, int(os.environ["WANT_SOFT_LIMIT"]))
        """))
        (self.tmp / "tests" / "test_runaway.py").write_text(textwrap.dedent("""
            import unittest
            class T(unittest.TestCase):
                def test_allocates_without_end(self):
                    held = []
                    while True:                     # the shape of every #710 runaway
                        held.append(bytearray(4 * 1024 * 1024))
        """))
        self.box = object.__new__(sweep.Sandbox)
        self.box.path = self.tmp
        self.box._pristine = {}
        self.box.memory_cap = 0

    # -- the arithmetic ---------------------------------------------------------------

    def test_one_share_per_sandbox_and_one_left_over_for_everything_else(self):
        """The sweep is not the only thing on the box: the parent, the trace cache, the
        runner's own agent and the operating system live in the share nobody is given. A
        cap of `total // jobs` would let four concurrent runaways add up to the whole
        machine, which is the failure this exists for."""
        self.assertEqual(sweep.address_space_cap(4, total=16 * 1024 ** 3),
                         16 * 1024 ** 3 // 5)
        self.assertEqual(sweep.address_space_cap(1, total=16 * 1024 ** 3),
                         8 * 1024 ** 3)
        self.assertLess(sweep.address_space_cap(4, total=16 * 1024 ** 3) * 4,
                        16 * 1024 ** 3)

    def test_a_machine_that_will_not_say_how_big_it_is_gets_the_floor(self):
        """Found unpinned by this branch's own sweep, at `tools/sweep.py:1803`.

        `os.sysconf` is not on every platform and does not answer on every platform, and
        the one thing this must not do is propagate: an unbounded mutant is the failure
        being fixed, and a sweep that refuses to start because it could not size a cap is a
        worse one. So the catch is real, and the sweep was right that nothing held it.
        """
        real = os.sysconf

        def refuses(_name):
            raise ValueError("unrecognised configuration name")

        os.sysconf = refuses
        self.addCleanup(setattr, os, "sysconf", real)
        self.assertEqual(sweep.total_memory(), 0)
        self.assertEqual(sweep.address_space_cap(4), sweep.SUITE_ADDRESS_SPACE)

    def test_a_job_count_nobody_validated_does_not_divide_by_zero(self):
        """Found unpinned by this branch's own sweep, at `tools/sweep.py:1820`.

        `--jobs` is an integer the operator types and nothing checks its sign, and
        `jobs + 1` is a **divisor** — so `--jobs -1` is a `ZeroDivisionError` raised while
        sizing a memory cap, which is a sweep that will not start at all, over a typo. The
        floor is not decoration and it is not dead: it is the only thing between a
        mistyped flag and a stack trace.
        """
        for jobs in (-5, -1, 0):
            self.assertEqual(sweep.address_space_cap(jobs, total=64 * 1024 ** 3),
                             64 * 1024 ** 3, f"--jobs {jobs}")

    def test_the_floor_is_what_the_suite_measured_and_it_wins_on_a_small_machine(self):
        """The two failures are not symmetrical. Too loose is the status quo — the runner
        dies. Too tight reddens the **unmutated baseline**, and this tool refuses to sweep
        a red tree, so the gate stops answering for everybody. Erring loose costs one
        branch a re-run; erring tight costs every branch the gate."""
        self.assertEqual(sweep.address_space_cap(4, total=2 * 1024 ** 3),
                         sweep.SUITE_ADDRESS_SPACE)
        # And a machine that will not say how big it is gets the measured figure, not none.
        self.assertEqual(sweep.address_space_cap(4, total=0), sweep.SUITE_ADDRESS_SPACE)
        # 1,580 MB is the whole process tree's peak on a ten-core Linux box, so the floor
        # has to be above it — asserted, because a floor below what the suite needs is a
        # floor that reddens every baseline.
        self.assertGreater(sweep.SUITE_ADDRESS_SPACE, 1580 * 1024 ** 2)

    # -- the wrapper ------------------------------------------------------------------

    def test_no_cap_means_no_wrapper_at_all(self):
        """On a platform that will not enforce it there is nothing to gain from a shell in
        the way, and something to lose: the pid the sweep holds is what `_kill_group` and
        both timeouts work through."""
        self.assertEqual(sweep.under_cap(["python", "-m", "unittest"], 0),
                         ["python", "-m", "unittest"])

    def test_the_wrapper_execs_so_the_pid_the_sweep_holds_is_the_interpreters(self):
        """A shell that *called* python would leave the sweep holding the shell's pid, and
        `_kill_group`, the process group and the timeout all reach for that pid."""
        argv = sweep.under_cap(["python", "-m", "unittest", "tests.test_x"], 3 * 1024 ** 3)
        self.assertEqual(argv[0], "/bin/sh")
        self.assertIn("exec", argv[2])
        self.assertIn("ulimit -v", argv[2])
        self.assertEqual(argv[-4:], ["python", "-m", "unittest", "tests.test_x"])
        # `ulimit -v` counts KiB, and getting that wrong by 1024 is a cap that either
        # never fires or reddens the baseline.
        self.assertEqual(argv[4], str(3 * 1024 ** 2))

    def test_the_probe_agrees_with_what_the_platform_actually_does(self):
        """`cap_holds` is the one thing that decides whether anything is wrapped at all, so
        a probe that disagreed with reality would either leave every run uncapped in
        silence or fail every mutation on a platform that was fine. Asserted as an
        equivalence, so it holds on whichever platform runs it — Linux says yes, macOS says
        no, and both are measured here rather than assumed."""
        cap = 512 * 1024 ** 2
        done = subprocess.run(sweep.under_cap([sys.executable, "-c", "pass"], cap),
                              capture_output=True, timeout=120)
        self.assertEqual(done.returncode == 0, sweep.cap_holds(cap), done.stderr)

    def test_a_preexec_fn_that_cannot_set_the_limit_takes_the_parent_down_with_it(self):
        """Why this is a shell rather than `resource.setrlimit` in a `preexec_fn`, which is
        the form the issue proposed. On macOS that call RAISES — and a `preexec_fn` that
        raises surfaces as `SubprocessError` **in the parent**, so the proposal as written
        makes every mutation on the operator's own machine fail to run at all. #572 is the
        issue about this tool being unusable on macOS.

        (The second reason has no case of its own because it is a hazard rather than a
        behaviour: `preexec_fn` calls back into Python between `fork` and `exec`, which the
        standard library says is unsafe in the presence of threads, and this sweep runs its
        sandboxes from a thread pool.)
        """
        def refuses():
            raise ValueError("[Errno 22] Invalid argument")

        with self.assertRaises(subprocess.SubprocessError):
            subprocess.run([sys.executable, "-c", "pass"], preexec_fn=refuses,
                           capture_output=True, timeout=120)

    # -- the cap, measured through the code path a mutation uses ----------------------

    def test_the_cap_the_sandbox_was_given_is_the_cap_the_child_is_run_under(self):
        """End to end through `Sandbox.run`, and the child is the witness: it reads its own
        `RLIMIT_AS` back and fails if it is not the number the sweep asked for. A wiring
        test that only read the argv would agree with itself."""
        cap = 512 * 1024 ** 2
        if not sweep.cap_holds(cap):
            self.skipTest(f"{sys.platform} does not enforce an address-space limit, so "
                          "there is no cap here to measure — see #773")
        self.box.memory_cap = cap
        os.environ["WANT_SOFT_LIMIT"] = str(cap)
        self.addCleanup(os.environ.pop, "WANT_SOFT_LIMIT", None)
        outcome = self.box.run(["tests.test_limit"], timeout=120)
        self.assertTrue(outcome.green, outcome.detail)
        self.assertEqual(outcome.ran, 1)

    def test_with_no_cap_the_child_is_left_exactly_as_it_was(self):
        """The other half, and it runs everywhere: an uncapped sandbox must not quietly
        acquire a limit from somewhere, because a limit nobody chose is a red nobody can
        explain."""
        os.environ["WANT_SOFT_LIMIT"] = str(resource.getrlimit(resource.RLIMIT_AS)[0])
        self.addCleanup(os.environ.pop, "WANT_SOFT_LIMIT", None)
        outcome = self.box.run(["tests.test_limit"], timeout=120)
        self.assertTrue(outcome.green, outcome.detail)

    def test_a_mutation_that_allocates_without_end_comes_back_red_and_named(self):
        """The whole point. Uncapped, this is the run that takes the machine with it and
        reports `exit 143`; capped, it is a red on a named test — which `decide` compares
        against the same module-set unmutated and turns into a **pin**."""
        cap = 512 * 1024 ** 2
        if not sweep.cap_holds(cap):
            self.skipTest(f"{sys.platform} does not enforce an address-space limit, so a "
                          "runaway mutation can still take the machine — see #773")
        self.box.memory_cap = cap
        outcome = self.box.run(["tests.test_runaway"], timeout=300)
        self.assertFalse(outcome.green)
        # A red, and not a timeout: `conclusive` is what separates "the guard is tested"
        # from "the machine was busy", and a mutant killed by the cap is the first of those.
        self.assertTrue(outcome.conclusive, outcome.detail)
        self.assertEqual(outcome.ran, 1)
        self.assertEqual(outcome.failing,
                         frozenset({"tests.test_runaway.T.test_allocates_without_end"}))

    def test_every_sandbox_a_sweep_builds_is_given_the_cap(self):
        """One uncapped sandbox in a pool of four is a pool that can still lose the
        machine, and the mutation that did it would be reported as infrastructure."""
        boxes = []
        real = sweep.Sandbox
        # Inside the fixture's own directory, and that is load-bearing: `sweep()` ends by
        # `shutil.rmtree`-ing every box's `path`. A stub that pointed one at `Path(".")`
        # deleted the working tree it was being written in — measured, once, the hard way.
        where = self.tmp / "boxes"

        class _Counted(real):
            def __init__(self, *a, **k):
                boxes.append(self)
                self.path = where / f"w{len(boxes)}"
                self.path.mkdir(parents=True)
                self._pristine, self._clean_failures = {}, {}

        sweep.Sandbox = _Counted
        self.addCleanup(setattr, sweep, "Sandbox", real)
        plan = [sweep.Mutation("charter/m.py", n, n, "drop-if", "q?", "if x: pass",
                               "", "f") for n in range(3)]
        for name, stub in (("plan_for", lambda *a: (plan, {})),
                           ("decide", lambda box, m, mods:
                            ("pinned", sweep.Outcome(False, 1, "x"), None))):
            was = getattr(sweep, name)
            setattr(sweep, name, stub)
            self.addCleanup(setattr, sweep, name, was)
        with contextlib.redirect_stdout(io.StringIO()):
            sweep.sweep(self.tmp, "HEAD", {"charter/m.py": {1}}, {}, self.tmp, 2, {},
                        0, print, 60.0, None, cap=7 * 1024 ** 2)
        self.assertEqual(len(boxes), 2)
        self.assertEqual([b.memory_cap for b in boxes], [7 * 1024 ** 2] * 2)


class TheVerdictIsTheSetOfNewlyFailingTests(unittest.TestCase):
    """An exit code cannot say WHY a run died, and this project has measured the confusion
    in both directions.

    `release.yml`'s `-z "$claimed"` refusal (#558) exits 1 with the line deleted *and*
    without it, for two different reasons — so a real deletion read as pinned. And a sweep
    run in a tree copied without `.git` errors twelve `test_workflows` cases in the
    baseline and in every mutant alike — so every mutation read as pinned, 37 of 37, and
    that whole sweep had to be thrown away.

    Both are the same mistake: crediting a mutation with a red it did not cause. So the
    verdict here is `failing(mutant) - failing(same command, unmutated)`.
    """

    def test_a_red_the_subset_was_already_red_on_pins_nothing(self):
        already = frozenset({"unittest.loader._FailedTest.tests.test_workflows"})
        box = _FakeBox(subset=sweep.Outcome(False, 40, "FAILED", failing=already),
                       full=sweep.Outcome(True, 6000, "OK"),
                       clean=already)
        verdict, subset, _ = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "survived")
        self.assertIn("nothing new", subset.detail)

    def test_only_the_tests_this_mutation_broke_are_held_against_it(self):
        box = _FakeBox(
            subset=sweep.Outcome(False, 40, "FAILED",
                                 failing=frozenset({"pre.C.existing", "new.C.broken"})),
            clean=frozenset({"pre.C.existing"}))
        verdict, subset, _ = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "pinned")
        self.assertEqual(subset.failing, frozenset({"new.C.broken"}))
        self.assertIn("new.C.broken", subset.detail)
        self.assertNotIn("pre.C.existing", subset.detail)

    def test_the_clean_baseline_is_measured_before_the_mutation_is_applied(self):
        """Otherwise it would measure the mutant and every verdict would be 'no change'."""
        order = []

        class Ordered(_FakeBox):
            def clean_failures(self, modules):
                order.append("baseline")
                return frozenset()

            def apply(self, m):
                order.append("apply")

        box = Ordered(subset=sweep.Outcome(True, 40, "OK"),
                      full=sweep.Outcome(True, 6000, "OK"))
        sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(order[:2], ["baseline", "apply"])

    def test_a_whole_suite_red_only_on_pre_existing_failures_is_not_a_pin(self):
        already = frozenset({"tests.test_flaky.C.t"})
        box = _FakeBox(subset=sweep.Outcome(True, 40, "OK"),
                       full=sweep.Outcome(False, 6000, "FAILED", failing=already),
                       clean=already)
        verdict, _, full = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "survived")
        self.assertIn("nothing new", full.detail)


class ACouldNotMeasureIsNotAPin(unittest.TestCase):
    """The distinction this tool got wrong and had to be taught.

    Measured, on a box under a load average of 100 with two other agents on it: two of
    #553's six known-unpinned guards came back "pinned" with `ran=0` — the full suite had
    run past its timeout and been killed. No test failed. The harness had turned machine
    load into evidence, and the evidence it manufactured was the one verdict that
    certifies a guard as tested and is never revisited. Both were confirmed by hand
    afterwards as genuine survivors.
    """

    def test_a_timeout_is_marked_inconclusive_and_not_merely_red(self):
        tmp = Path(tempfile.mkdtemp(prefix="charter-sweep-incon-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(tmp, ignore_errors=True))
        (tmp / "tests").mkdir()
        (tmp / "tests" / "__init__.py").write_text("")
        (tmp / "tests" / "test_slow.py").write_text(textwrap.dedent("""
            import time, unittest
            class T(unittest.TestCase):
                def test_slow(self):
                    time.sleep(120)
        """))
        box = object.__new__(sweep.Sandbox)
        box.path, box._pristine = tmp, {}
        outcome = box.run(["tests.test_slow"], timeout=4)
        self.assertFalse(outcome.green)
        self.assertFalse(outcome.conclusive)

    def test_a_real_failure_is_conclusive(self):
        self.assertTrue(sweep._verdict(
            _Completed(1, "Ran 3 tests in 1s\n\nFAILED (failures=1)\n")).conclusive)

    def test_a_mutation_the_machine_could_not_measure_twice_has_no_verdict(self):
        """Not pinned. "I could not look" must never render as "nothing to see" — the same
        distinction `plugincache.content_hash` keeps when it answers None rather than an
        empty hash."""
        box = _FullFlake(sweep.Outcome(False, 0, "timed out after 2400s", conclusive=False),
                         sweep.Outcome(False, 0, "timed out after 2400s", conclusive=False))
        verdict, _, _ = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "unresolved")

    def test_a_timeout_that_does_not_repeat_is_decided_by_the_run_that_worked(self):
        box = _FullFlake(sweep.Outcome(False, 0, "timed out", conclusive=False),
                         sweep.Outcome(True, 6000, "OK"))
        verdict, _, _ = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "survived")

    def test_an_unresolved_mutation_is_named_in_the_report_and_not_buried(self):
        m = sweep.Mutation(path="charter/frame/layout.py", line=298, end_line=298,
                           operator="collapse-ifexp", question="?",
                           before="size.n if isinstance(size, Fixed) else 1",
                           after="size.n", symbol="_policy_cells")
        r = sweep.Result(m, "unresolved", sweep.Outcome(True, 45, "OK"),
                         sweep.Outcome(False, 0, "timed out after 2400s", conclusive=False),
                         [])
        text = sweep.report([r], Path("."), "abc", "def", None, 1.0)
        self.assertIn("UNRESOLVED        : 1", text)
        self.assertIn("charter/frame/layout.py:298", text)
        self.assertIn("timed out", text)
        self.assertIn("emphatically not a pin", text)


class AStaleBytecodeCacheCannotDecideAnything(unittest.TestCase):
    """The third way this harness invented a false pin, and the least obvious.

    CPython validates a cached `.pyc` against the source's size and its mtime **truncated
    to whole seconds**, and nothing else. A sandbox applies one mutation after another to
    the same file, and two mutations of one file routinely differ from the original by the
    same number of bytes — `contain.one_line(x)` -> `x` removes exactly 18 characters
    wherever it appears. Measured on `charter/frame/panel.py` at `5b02b3f`: the mutation at
    line 210 and the mutation at line 458 are both 29159 bytes against a 29177-byte
    original. Applied within one second of each other, the second is indistinguishable from
    the first, and line 458 — a real survivor — was reported PINNED by line 210's bytecode.
    """

    def test_the_sandbox_refuses_to_write_bytecode_at_all(self):
        """Belt, not braces: rather than trying to out-guess the validator, the sandbox
        never writes a `.pyc`, so there is nothing stale to reuse."""
        box = object.__new__(sweep.Sandbox)
        tmp = Path(tempfile.mkdtemp(prefix="charter-sweep-pyc-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(tmp, ignore_errors=True))
        box.path, box._pristine = tmp, {}
        (tmp / "tests").mkdir()
        (tmp / "tests" / "__init__.py").write_text("")
        (tmp / "tests" / "test_env.py").write_text(textwrap.dedent("""
            import os, sys, unittest
            class T(unittest.TestCase):
                def test_no_bytecode(self):
                    assert os.environ.get("PYTHONDONTWRITEBYTECODE") == "1"
                    assert sys.dont_write_bytecode
        """))
        outcome = box.run(["tests.test_env"], timeout=120)
        self.assertTrue(outcome.green, outcome.detail)
        self.assertEqual(list(tmp.rglob("__pycache__")), [])

    def test_two_mutations_of_one_file_can_be_byte_identical_in_length(self):
        """The precondition, stated as a property rather than as a story: if this stops
        being possible the guard above is still right, and if it starts being common the
        guard above is the only thing standing between the sweep and a wrong answer."""
        src = textwrap.dedent("""
            def f(a, b):
                x = contain.one_line(a)
                y = contain.one_line(b)
                return x, y
        """).lstrip("\n").encode()
        muts = [m for m in sweep.mutations_for("charter/x.py", src, set(range(1, 6)))
                if m.operator == "uncontain"]
        self.assertEqual(len(muts), 2)
        self.assertEqual(len(muts[0].source), len(muts[1].source))
        self.assertNotEqual(muts[0].source, muts[1].source)


class TheVerdictIsReadFromTheRun(unittest.TestCase):
    def test_ok_with_tests_is_green(self):
        self.assertTrue(sweep._verdict(_Completed(0, "Ran 42 tests in 1s\n\nOK\n")).green)

    def test_a_run_with_no_tests_is_not_green(self):
        """`Ran 0 tests` exits 0. Counting that as a pass would mark every mutation in a
        file whose module names were mistyped as pinned, silently."""
        v = sweep._verdict(_Completed(0, "Ran 0 tests in 0.0s\n\nOK\n"))
        self.assertFalse(v.green)
        self.assertIn("no tests", v.detail)

    def test_a_failure_carries_the_ids_of_the_tests_that_failed(self):
        v = sweep._verdict(_Completed(1,
            "FAIL: test_x (tests.test_mod.C.test_x)\n"
            "ERROR: test_y (tests.test_mod.C.test_y)\n"
            "Ran 42 tests in 1s\n\nFAILED (failures=1, errors=1)\n"))
        self.assertFalse(v.green)
        self.assertEqual(v.ran, 42)
        self.assertEqual(v.failing,
                         frozenset({"tests.test_mod.C.test_x", "tests.test_mod.C.test_y"}))
        self.assertIn("tests.test_mod.C.test_x", v.detail)

    def test_an_import_error_is_collected_as_a_failing_id_too(self):
        """A module that will not import errors as `unittest.loader._FailedTest`, and a
        tree copied without `.git` produces twelve of them at once. If those are counted
        against the mutation, every mutation in the run scores pinned."""
        v = sweep._verdict(_Completed(1,
            "ERROR: tests.test_workflows "
            "(unittest.loader._FailedTest.tests.test_workflows)\n"
            "Ran 3 tests in 1s\n\nFAILED (errors=1)\n"))
        self.assertEqual(v.failing,
                         frozenset({"unittest.loader._FailedTest.tests.test_workflows"}))


class TheSubsetIsNarrowedByFunctionAndWidenedByDoubt(unittest.TestCase):
    """Selection narrows to the function, and every fallback runs MORE, never less."""

    MAP = {"charter/frame/overlay.py": ["tests.test_a", "tests.test_b", "tests.test_c"],
           "charter/frame/overlay.py::_window": ["tests.test_a"]}

    def _m(self, symbol):
        return sweep.Mutation(path="charter/frame/overlay.py", line=1, end_line=1,
                              operator="unclamp", question="?", before="max(1, h)",
                              after="h", symbol=symbol)

    def test_a_measured_function_selects_only_the_modules_that_ran_it(self):
        """File granularity is not enough in this tree: `layout`, `slots` and `builtins`
        are reached by nearly every test module through a shared fixture, so a file-keyed
        map answers 'run all 322' for a helper only four modules ever call."""
        self.assertEqual(sweep.select_for(self.MAP, self._m("_window")), ["tests.test_a"])

    def test_a_function_the_trace_never_saw_falls_back_to_the_whole_file(self):
        """A `<lambda>`, or a comprehension frame on 3.11, is never seen under its own
        name. Falling back to the file runs more than needed; guessing narrower would
        certify a guard as tested by a subset that never ran it."""
        self.assertEqual(sweep.select_for(self.MAP, self._m("_paint")),
                         ["tests.test_a", "tests.test_b", "tests.test_c"])

    def test_a_file_the_trace_never_saw_selects_nothing_and_decide_runs_everything(self):
        self.assertEqual(sweep.select_for({}, self._m("_window")), [])


# ======================================================================================
# Second order — two guards in sequence mask each other
# ======================================================================================

class TwoGuardsCanHideBehindEachOther(unittest.TestCase):
    SRC = textwrap.dedent("""
        def _placed_here():
            out = {}
            for placed in config.FRAME.get("components") or ():
                name = placed.get("slot")
                if isinstance(name, str) and name not in SLOT_SIZE:
                    out[name] = placed
            return out
    """).lstrip("\n").encode()

    def test_two_mutations_in_one_function_compose_into_one_file(self):
        """Deleting `_placed_here`'s `name not in SLOT_SIZE` alone changes nothing any
        caller can observe, because `_edge_of`/`_size_of` read the shipped tables first.
        One mutation at a time invites a reviewer to call it equivalent. It is not — its
        consequence is hidden behind a SECOND unpinned line, and composing the pair is
        how the harness asks that question."""
        muts = sweep.mutations_for("charter/frame/layout.py", self.SRC, set(range(1, 9)))
        a = next(m for m in muts if m.operator == "no-fallback")
        # The half this test is about, named rather than taken by sort order: it is
        # `name not in SLOT_SIZE` whose consequence hides behind the fallback, and the
        # mutation that asks about it is the one that KEEPS `isinstance`.
        b = next(m for m in muts
                 if m.operator == "drop-conjunct" and "isinstance" in m.after)
        combined = sweep.compose(self.SRC, (a, b))
        self.assertIsNotNone(combined)
        text = combined.decode()
        self.assertIn('config.FRAME["components"]', text)
        self.assertIn("if isinstance(name, str):", text)

    def test_a_composed_pair_keeps_every_line_number(self):
        muts = sweep.mutations_for("charter/frame/layout.py", self.SRC, set(range(1, 9)))
        a = next(m for m in muts if m.operator == "no-fallback")
        b = next(m for m in muts if m.operator == "drop-conjunct")
        combined = sweep.compose(self.SRC, (a, b))
        self.assertEqual(len(combined.splitlines()), len(self.SRC.splitlines()))

    def test_overlapping_mutations_are_refused_rather_than_spliced_into_nonsense(self):
        """`drop-if` covers the whole statement and `drop-conjunct` covers a piece of its
        condition. Splicing both would write one inside the hole left by the other."""
        muts = sweep.mutations_for("charter/frame/layout.py", self.SRC, set(range(1, 9)))
        whole = next(m for m in muts if m.operator == "drop-if")
        part = next(m for m in muts if m.operator == "drop-conjunct")
        self.assertIsNone(sweep.compose(self.SRC, (whole, part)))


# ======================================================================================
# Evidence, caching, and the report
# ======================================================================================

class TwoMutantsOfOneNodeAreNeverReportedAlike(unittest.TestCase):
    """#721, asserted on the whole operator table rather than on the two that cost money.

    `Mutation.before` is the mutated NODE, so **ambiguity appears exactly where the node
    is larger than the thing being varied**: every `drop-conjunct` mutant of
    `if A and B:` prints the same line, and the two mean opposite things. Dropping the
    cheap prefilter is genuinely equivalent and correctly dismissed; dropping the test
    that decides is a fail-open hole. Two of those, in a security guard, were read as the
    first across two full sweep runs and only caught on a third — 150,585 and 119,278
    distinguishing inputs, both verified against a real `bash`. A report line satisfied by
    two mutants meaning different things discriminates neither, and that is worse than
    printing nothing, because it supplies a wrong reading.

    So the property is the table's and not one operator's: **an operator yielding more
    than one mutant for one node must give them distinct questions, distinct tags and
    distinct report lines.** That is what would have caught `unclamp` — whose two mutants
    printed identically AND carried one question, while asking whether the floor is pinned
    and whether the value is — before anybody was bitten by it, and `drop-term`, which had
    the same shape and was in nobody's table.
    """

    #: One instance of every shape the table can recognise, so that the property below is
    #: measured over all of them and not over the ones somebody remembered. The seven
    #: shapes that yield siblings for one node are all here; the rest are here so that an
    #: operator added to `_iter_operators` without a line here fails the coverage test.
    FIXTURE = """
        LIMIT = 28
        MODE = 0o600
        SPAN = ROWS + COLS


        def render(rows, name, d, path, text, x, y, ch, i, n, in_class):
            if not isinstance(name, str):
                return []
            if name and name not in d:
                return []
            if in_class and i + 2 < n and text[i + 1] == "-" and text[i + 2] != "]":
                return []
            if text.startswith("focus:"):
                return []
            if x == 1:
                head = 1
            else:
                head = 2
            width = max(1, LIMIT - head)
            if 0 <= x < width:
                return []
            if " " <= ch <= "~":
                return []
            kept = [r for r in rows if r]
            label = contain.one_line(text)
            slot = d.get(name) or ()
            other = d.get(name, "-")
            tail = "left" if y else "right"
            try:
                value = int(text)
            except ValueError:
                return []
            return [label.lower(), path.resolve(), kept, slot, other, tail, value, width]
    """

    def siblings(self) -> dict[tuple, list]:
        """Every group of mutants sharing one node and one operator.

        Keyed on the byte span, which is the node — not on the line, because two nodes on
        one line are two questions and are entitled to read alike.
        """
        groups: dict[tuple, list] = {}
        for m in _mutations(self.FIXTURE):
            groups.setdefault((m.span, m.operator), []).append(m)
        return {k: v for k, v in groups.items() if len(v) > 1}

    def test_no_sibling_pair_shares_a_question_a_tag_or_a_report_line(self):
        """The whole issue, in one assertion, over every shape that has siblings.

        Three fields and not one: `question` is what the gate summary prints, `__str__` is
        what the per-mutation progress line prints — the stream that is all a reclaimed
        shard leaves behind, and the stream that hid the two holes — and `tag` is what the
        not-applied and no-verdict lists render.
        """
        self.assertTrue(self.siblings(), "the fixture offers no sibling group at all")
        for (_, operator), ms in sorted(self.siblings().items()):
            for field, read in (("question", lambda m: m.question),
                                ("tag", lambda m: m.tag),
                                ("report line", str)):
                with self.subTest(operator=operator, line=ms[0].line, field=field):
                    seen = {read(m) for m in ms}
                    self.assertEqual(
                        len(seen), len(ms),
                        f"{operator} yields {len(ms)} mutants for one node and only "
                        f"{len(seen)} distinct {field}s, so a survivor reported this way "
                        f"is satisfied by mutants meaning different things: "
                        + "  ||  ".join(sorted(seen)))

    def test_every_shape_that_yields_siblings_is_in_the_fixture(self):
        """A guard on the guard. The assertion above is only as general as this list, and
        a fixture that quietly stopped covering `unclamp` would still pass it.

        `retune-constant` is here because a module-level constant written in a base other
        than ten is offered twice — once by the module-constant rule as `n + 1` in decimal,
        once by the non-decimal rule re-spelled in its own base. Those two are the same
        VALUE, so it is a question asked twice rather than an ambiguity, and it is left
        alone here: this test pins that the two are told apart, not that both exist.
        """
        self.assertEqual(
            sorted({operator for _, operator in self.siblings()}),
            ["collapse-ifexp", "disable-branch", "drop-conjunct", "drop-term",
             "retune-constant", "shift-boundary", "unclamp"])

    def test_the_fixture_exercises_every_operator_the_table_names(self):
        """An operator added to `_iter_operators` and not to the fixture is a shape this
        property was never asserted over — which is how `unclamp` and `drop-term` came to
        be live instances of #721 that nothing had tripped over. The table names its
        operators as literals in the yielded tuple, so it can be read without running a
        sweep."""
        table = set()
        tree = ast.parse(Path(sweep.__file__).read_text(encoding="utf-8"))
        fn = next(n for n in ast.walk(tree)
                  if isinstance(n, ast.FunctionDef) and n.name == "_iter_operators")
        for node in ast.walk(fn):
            if isinstance(node, ast.Yield) and isinstance(node.value, ast.Tuple):
                name = node.value.elts[2]
                self.assertIsInstance(name, ast.Constant)
                table.add(name.value)
        self.assertGreater(len(table), 10)
        self.assertEqual(sorted(table - {m.operator for m in _mutations(self.FIXTURE)}),
                         [])

    def test_the_clamp_question_names_the_operand_the_mutant_keeps(self):
        """`unclamp` was the worst instance and the one no field could disambiguate:
        `max(a, b) -> a` asks whether anything requires the value to clear the floor,
        `-> b` whether anything requires the floor at all, and both used to read "is the
        clamp pinned, or only its inner value?"."""
        self.assertEqual(
            sorted(m.question for m in _by(_mutations("""
                def f(h):
                    return max(1, h - 3)
            """), "unclamp")),
            ["is the clamp pinned, or is `1` always the answer?",
             "is the clamp pinned, or is `h - 3` always the answer?"])

    def test_a_branch_question_locates_itself_without_the_operator_table_open(self):
        """"is this branch pinned?" against "is the other branch pinned?" are two
        different strings and still not a legible reason: which is which is knowable only
        with `_iter_operators` open, and the reviewer holding the report does not have it.
        The spec asks that a survivor's reason be legible, so each one now names the branch
        it is about — by keyword for the conditional expression, by the direction the
        condition was forced for the statement."""
        self.assertEqual(
            sorted(m.question for m in _by(_mutations("""
                def f(y):
                    return "left" if y else "right"
            """), "collapse-ifexp")),
            ['is the `else` branch pinned, or is `"left"` always the answer?',
             'is the `if` branch pinned, or is `"right"` always the answer?'])
        forced = sorted(m.question for m in _by(_mutations("""
            def f(y):
                if y:
                    return 1
                else:
                    return 2
        """), "disable-branch"))
        self.assertEqual(forced, [
            "is the rest of the chain pinned, or does nothing change when this condition "
            "always holds?",
            "is this branch pinned, or does nothing change when its condition never "
            "holds?"])

    def test_the_tag_keeps_its_prefix_and_adds_what_tells_siblings_apart(self):
        """`tag` is rendered into the not-applied and no-verdict lists and into the
        `NotApplied` message, and `path:line:operator` is what a reader greps for — so the
        edit is appended rather than substituted for it."""
        m = sweep.Mutation(path="charter/hooks.py", line=3658, end_line=3658,
                           operator="drop-conjunct",
                           question='is the `c == "$"` half pinned?',
                           before='c == "$" and text.startswith("$(", i)',
                           after='text.startswith("$(", i)', symbol="_scan")
        self.assertTrue(m.tag.startswith("charter/hooks.py:3658:drop-conjunct"))
        self.assertIn('text.startswith("$(", i)', m.tag)
        gone = dataclasses.replace(m, after="")
        self.assertTrue(gone.tag.endswith("(deleted)"),
                        "a deletion has no `after` to name, and an empty discriminator "
                        "would put every deletion on one line back")
        # Both survivors this branch's own self-sweep found, pinned. A `drop-conjunct`
        # replacement is short far more often than not — p90 is 25 characters — and a
        # digest on one of those is noise the reader has to decode for nothing.
        short = dataclasses.replace(m, after="a and b")
        self.assertTrue(short.tag.endswith(" -> a and b"), short.tag)
        edge = dataclasses.replace(m, after="x" * 48)
        self.assertTrue(edge.tag.endswith(" -> " + "x" * 48),
                        "48 characters is short ENOUGH, and a boundary one notch in "
                        "digests an edit that would have fitted: " + edge.tag)
        over = dataclasses.replace(m, after="x" * 49)
        self.assertRegex(over.tag, r"…#[0-9a-f]{6}$")
        self.assertEqual(len(over.tag.split(" -> ", 1)[1]), 48)

    def test_a_long_edit_is_shortened_without_becoming_its_siblings_tag(self):
        """The first fix for #721 contained #721, and this fixture line is why it was
        caught. `after` is everything the mutant KEEPS, so `drop-conjunct` siblings share
        a long PREFIX — `_regex_shape`'s four-conjunct guard in `tools/sweep.py` has two
        whose replacements agree for their first 48 characters — and a plain truncation
        merged them back onto one line. Not truncating is not the answer either:
        replacements reach 7,149 characters in this tree. So the shortening stays and
        carries a digest of the whole replacement."""
        four = [ms for (_, operator), ms in self.siblings().items()
                if operator == "drop-conjunct" and len(ms) == 4]
        self.assertEqual(len(four), 1, "the fixture's four-conjunct guard is gone, and "
                                       "with it the only case that catches this")
        ms = four[0]
        edits = [m.tag.split(" -> ", 1)[1] for m in ms]
        self.assertLess(
            len({e[:40] for e in edits}), len(ms),
            "no two of these tags share their visible prefix any more, so this no longer "
            "measures what a plain truncation did to them: " + " | ".join(sorted(edits)))
        self.assertEqual(len(set(edits)), 4,
                         "two mutants of one line render as one tag again")
        for m in ms:
            with self.subTest(after=m.after):
                self.assertLessEqual(
                    len(m.tag.split(" -> ", 1)[1]), 48,
                    "the discriminator is bounded, or a 7,149-character replacement "
                    "puts a bullet list past anything a reader will read")

    def test_the_progress_line_says_what_the_gate_summary_says(self):
        """The asymmetry that hid the two holes: the gate summary printed `question` and
        the per-mutation log did not, so a shard reclaimed before it could emit a summary
        left behind the one stream that could not be read."""
        m = sweep.Mutation(path="charter/hooks.py", line=3658, end_line=3658,
                           operator="drop-conjunct",
                           question='is the `c == "$"` half pinned?',
                           before='c == "$" and text.startswith("$(", i)',
                           after='text.startswith("$(", i)', symbol="_scan")
        self.assertIn('is the `c == "$"` half pinned?', str(m))
        self.assertIn("charter/hooks.py:3658 [drop-conjunct]", str(m))


class TheReportSaysWhatTheTestsAsserted(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="charter-sweep-ev-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp, ignore_errors=True))
        (self.tmp / "tests").mkdir()
        (self.tmp / "tests" / "test_a.py").write_text(textwrap.dedent("""
            class T:
                def test_window_keeps_the_selection(self):
                    self.assertEqual(surface._window(8), (0, 6))
                def test_something_else(self):
                    self.assertTrue(other())
        """))

    def test_a_test_naming_the_symbol_is_reported_with_its_assertions(self):
        """Deleting `release.yml`'s `-z "$claimed"` refusal left the run still exiting 1,
        for a different reason (#558). Most 'equivalent mutant' claims are really 'the
        test asserts too little', so the assertions are put next to the survivor and the
        reviewer's first question becomes 'did my test look closely enough'."""
        m = sweep.Mutation(path="charter/frame/overlay.py", line=453, end_line=453,
                           operator="unclamp", question="?", before="max(1, h)",
                           after="h", symbol="_window")
        ev = sweep.evidence_for(self.tmp, m, ["tests.test_a"])
        self.assertEqual([(mod.split(".")[-1], name) for mod, name, _ in ev.naming],
                         [("test_a", "test_window_keeps_the_selection")])
        self.assertIn("assertEqual(surface._window(8), (0, 6))", ev.naming[0][2][0])

    def test_a_symbol_no_covering_test_names_is_said_so_plainly(self):
        """The loudest possible evidence, and the commonest: `_placed_here` is a function
        #553 added whole and nothing in the suite ever mentioned it."""
        m = sweep.Mutation(path="charter/frame/layout.py", line=291, end_line=291,
                           operator="drop-conjunct", question="?", before="a and b",
                           after="a", symbol="_placed_here")
        ev = sweep.evidence_for(self.tmp, m, ["tests.test_a"])
        self.assertTrue(ev.nothing_names_it)


class TheMapIsCachedAgainstTheTreeItMeasured(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="charter-sweep-hash-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp, ignore_errors=True))
        (self.tmp / "charter").mkdir()
        (self.tmp / "tests").mkdir()
        (self.tmp / "charter" / "a.py").write_text("x = 1\n")
        (self.tmp / "tests" / "test_a.py").write_text("y = 1\n")

    def test_the_same_tree_hashes_the_same(self):
        self.assertEqual(sweep.tree_hash(self.tmp, ("charter",)),
                         sweep.tree_hash(self.tmp, ("charter",)))

    def test_changing_a_source_file_invalidates_the_map(self):
        before = sweep.tree_hash(self.tmp, ("charter",))
        (self.tmp / "charter" / "a.py").write_text("x = 2\n")
        self.assertNotEqual(before, sweep.tree_hash(self.tmp, ("charter",)))

    def test_changing_a_test_invalidates_the_map_too(self):
        """A new test module can reach a file no module reached before. A map keyed only
        on the sources would keep selecting the old, smaller subset for it."""
        before = sweep.tree_hash(self.tmp, ("charter",))
        (self.tmp / "tests" / "test_b.py").write_text("z = 1\n")
        self.assertNotEqual(before, sweep.tree_hash(self.tmp, ("charter",)))


_TRACED_SOURCE = """\
def work():
    return 1


def other():
    return 2
"""

_TRACED_TEST = """\
import unittest

from pkg import thing


class T(unittest.TestCase):
    def test_it(self):
        self.assertEqual(thing.work(), 1)
        self.assertEqual(thing.other(), 2)
"""


#: A module the loader cannot turn into a suite. What it raises has to be something OTHER
#: than an `ImportError`: `loadTestsFromName` catches those and hands back a suite holding
#: one failing test, which is a red suite and not a broken module. Anything else comes
#: straight back out of the import, which is what `build_map` counts.
_WILL_NOT_LOAD = "raise RuntimeError('this module cannot be imported')\n"


def _mini_tree(where: Path, good: int = 1, broken: int = 0) -> Path:
    """A repository small enough to trace in a subprocess and real enough to trace.

    `build_map` measures by *running* the tests, one module per process, so none of this
    can be faked with a stub: the tree needs a package the tests import, a `tests/` the
    loader can find by name, and functions the tracer can watch being called.
    """
    (where / "pkg").mkdir(parents=True)
    (where / "tests").mkdir()
    (where / "pkg" / "__init__.py").write_text("")
    (where / "pkg" / "thing.py").write_text(_TRACED_SOURCE)
    (where / "tests" / "__init__.py").write_text("")
    for i in range(good):
        (where / "tests" / f"test_good{i:02d}.py").write_text(_TRACED_TEST)
    for i in range(broken):
        (where / "tests" / f"test_broken{i:02d}.py").write_text(_WILL_NOT_LOAD)
    return where


class _Traced(unittest.TestCase):
    """A miniature tree, plus a second name for it that goes through a symlink."""

    good = 1
    broken = 0

    def setUp(self):
        # Resolved, so that the fixture's own `$TMPDIR` cannot be what makes a case here
        # pass or fail. The only symlink in play is the one this class makes on purpose.
        self.tmp = Path(tempfile.mkdtemp(prefix="charter-sweep-trace-")).resolve()
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp, ignore_errors=True))
        self.tree = _mini_tree(self.tmp / "tree", self.good, self.broken)
        self.link = self.tmp / "reached-through-here"
        self.link.symlink_to(self.tree, target_is_directory=True)
        self.log: list[str] = []
        self._scratches = 0

    def build(self, root=None, jobs=1):
        self._scratches += 1
        return sweep.build_map(self.tree if root is None else root, ("pkg",), jobs,
                               self.tmp / f"scratch{self._scratches}", log=self.log.append)

    @property
    def logged(self) -> str:
        return "\n".join(self.log)


class TheMapIsMeasuredThroughEverySpellingOfItsRoot(_Traced):
    """The tracer COMPARES paths, so the root it is handed has to be one spelling.

    The trace runner puts `os.getcwd()` on `sys.path` and the kernel answers that with
    every symlink already resolved, so `co_filename` is always the resolved spelling. A
    prefix built from a root that still carries a symlink matches nothing at all — and
    that is not an exotic case, it is the DEFAULT: `tempfile.gettempdir()` on macOS is
    `/var/folders/…` and `/var` is a symlink to `/private/var`. Measured (#572): 329
    modules traced, 0 files, 0 broken modules, and the tool refused to sweep at all.

    The symlink below is made explicitly rather than inherited from `$TMPDIR`, so these
    cases go red on a `resolve()`-less `build_map` on Linux too. A case that only
    asserted the map came back non-empty would have passed on CI forever.
    """

    def test_a_root_reached_through_a_symlink_still_measures_what_ran(self):
        found = self.build(self.link)
        self.assertIn("pkg/thing.py::work", found)
        self.assertEqual(found["pkg/thing.py::work"], ["tests.test_good00"])

    def test_the_spelling_of_the_root_cannot_change_the_map(self):
        """The strong form: not merely non-empty, the SAME map either way. A prefix is an
        implementation detail of the measurement and must not be visible in its answer."""
        self.assertEqual(self.build(self.link), self.build(self.tree))

    def test_the_map_is_keyed_by_file_and_by_function(self):
        """File alone is far too coarse in a tree where everything imports `layout`, and
        function alone loses the `<lambda>`s and comprehension frames `select_for` falls
        back on. The map carries both keys or the selection narrows in silence."""
        found = self.build()
        self.assertIn("pkg/thing.py", found)
        self.assertIn("pkg/thing.py::other", found)


class AnUnusableMapSaysWhichKindOfUnusable(_Traced):
    """`0 file(s) and 0 broken module(s)` was the entire diagnosis of #572 — the tool had
    it, printed it, and left the operator with nowhere to go from it.

    The refusal is not softened here. A map that measured nothing still stops the sweep,
    because the alternative is sending every mutation to the full suite and calling a
    hundredfold slowdown a success. What changes is that it now says which of the two
    failures it hit, and the failure that hides — the tracer matching nothing at all —
    prints the two paths that had to be one path.
    """

    def test_a_tracer_that_matched_nothing_prints_both_spellings(self):
        (self.tree / "tests" / "test_good00.py").write_text("import unittest\n")
        with self.assertRaisesRegex(RuntimeError, "refusing to sweep blind"):
            self.build()
        self.assertIn("every module loaded and ran", self.logged)
        self.assertIn(f"matched against : {self.tree}{os.sep}", self.logged)
        # Measured by the runner, not assumed by the caller: this line is the one that
        # would have shown `/private/var/…` against a `/var/…` prefix in a single glance.
        self.assertIn(f"the runners ran : {self.tree}", self.logged)

    def test_modules_that_would_not_load_are_named_instead(self):
        """The other way to an empty map, and it wants the opposite thing done about it.
        Printing a prefix here would be a red herring; the traceback is the answer."""
        (self.tmp / "tree" / "tests" / "test_broken00.py").write_text(_WILL_NOT_LOAD)
        with self.assertRaisesRegex(RuntimeError, "refusing to sweep blind"):
            self.build()
        self.assertIn("tests.test_broken00", self.logged)
        self.assertNotIn("matched against", self.logged)

    def test_a_runner_that_writes_nothing_is_a_broken_module_and_not_a_crash(self):
        """A trace runner that dies before writing its JSON leaves no file at all. That
        is a module this sweep could not measure, which the map already knows how to
        report — it is not an exception for the whole run to die on."""
        self.addCleanup(setattr, sweep, "_TRACE_RUNNER", sweep._TRACE_RUNNER)
        sweep._TRACE_RUNNER = "import sys\nsys.exit(0)\n"
        with self.assertRaisesRegex(RuntimeError, "refusing to sweep blind"):
            self.build()
        self.assertIn("the trace runner wrote nothing", self.logged)


class TheTraceReportsWhatItMeasuredAsItGoes(_Traced):
    good = 40

    def test_a_minority_of_modules_that_will_not_load_is_a_note_and_not_a_refusal(self):
        """Under a quarter broken is a usable map with a hole in it, and the hole is
        stated: those files fall back to the full suite, which is slower and correct."""
        (self.tmp / "tree" / "tests" / "test_broken.py").write_text(_WILL_NOT_LOAD)
        found = self.build(jobs=4)
        self.assertIn("pkg/thing.py::work", found)
        self.assertIn("1 module(s) would not load", self.logged)

    def test_the_trace_says_how_far_it_has_got(self):
        """Tracing the real tree is 329 processes and ten minutes. A tool that prints
        nothing for ten minutes cannot be told from a hung one, and gets killed."""
        self.build(jobs=4)
        self.assertIn("traced 40/40 modules", self.logged)


class TheMapIsRetracedOnlyWhenItHasTo(_Traced):
    """`cached.exists() and not refresh` — both halves, because either one alone is a
    different tool: without the first it reads a cache that is not there, and without the
    second `--refresh-map` hands back the very map the operator asked it to throw away."""

    def cache(self):
        return self.tmp / "cache"

    def load(self, refresh=False):
        return sweep.load_map(self.tree, ("pkg",), self.cache(), 1, refresh,
                              log=self.log.append)

    def test_the_first_call_traces_and_the_second_reads_what_it_wrote(self):
        first = self.load()
        self.assertIn("pkg/thing.py::work", first)
        self.log.clear()
        self.assertEqual(self.load(), first)
        self.assertIn("cached", self.logged)

    def test_refreshing_re_traces_even_though_the_cache_is_sitting_there(self):
        fresh = self.load()
        stale = next(iter(self.cache().glob("selection-*.json")))
        stale.write_text('{"pkg/stale.py": ["tests.test_gone"]}')
        self.assertEqual(self.load(refresh=True), fresh)


class TheWorkdirIsOutsideTheTreeAndHasNoSymlinkInIt(unittest.TestCase):
    """#572 at the exact place it bit: the DEFAULT workdir, which nobody passes.

    `--workdir /private/tmp/…` was the published workaround, and that is another way of
    saying the default was the one spelling of this path that could not work — the tool
    was unusable as invoked and usable only as corrected. `$TMPDIR` is not this tool's to
    choose, so the resolving is.
    """

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="charter-sweep-workdir-")).resolve()
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp, ignore_errors=True))
        (self.tmp / "real").mkdir()
        self.link = self.tmp / "tmp-through-a-symlink"
        self.link.symlink_to(self.tmp / "real", target_is_directory=True)

    def _tmpdir_is_a_symlink(self):
        """Exactly the shape macOS ships: `$TMPDIR` names a directory through a link."""
        self.addCleanup(setattr, tempfile, "tempdir", tempfile.tempdir)
        tempfile.tempdir = str(self.link)

    def test_the_default_workdir_is_resolved_even_when_tmpdir_is_a_symlink(self):
        self._tmpdir_is_a_symlink()
        where = sweep.workdir_for(self.tmp / "checkout", None)
        self.assertEqual(where.parent, self.tmp / "real")

    def test_an_explicit_workdir_is_resolved_the_same_way(self):
        """`--workdir` is the escape hatch, so it is the last place to leave unnormalised
        — and resolving it also makes a relative one mean what the operator meant."""
        where = sweep.workdir_for(self.tmp / "checkout", str(self.link / "here"))
        self.assertEqual(where, self.tmp / "real" / "here")

    def test_one_checkout_reached_by_two_names_gets_one_workdir(self):
        """The digest is the cache key. Taken from an unresolved root, the same tree
        traced from two spellings would pay for the map twice and share neither."""
        checkout = self.tmp / "real"
        self.assertEqual(sweep.workdir_for(checkout, None),
                         sweep.workdir_for(self.link, None))


class TheSandboxIsOneSpellingOfOnePath(unittest.TestCase):
    """A sandbox's path becomes the trace runners' `cwd` and the root the map is measured
    against, which makes it the last place a symlink can get in — `--workdir` under a
    linked directory is #572 by another route and nothing downstream can tell them
    apart."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="charter-sweep-box-")).resolve()
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp, ignore_errors=True))
        self.repo = self.tmp / "repo"
        (self.repo / "charter").mkdir(parents=True)
        (self.repo / "charter" / "m.py").write_text("x = 1\n")
        # Sealed off from the machine's git, for the reason `TheDiffIsReadWithNoContext`
        # gives: a global hooksPath or signing key would run this fixture through whatever
        # the developer happens to have installed.
        env = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_SYSTEM=os.devnull,
                   GIT_CONFIG_NOSYSTEM="1", GIT_TERMINAL_PROMPT="0")

        def run(*a):
            subprocess.run(("git", "-c", "core.hooksPath=", "-c", "commit.gpgsign=false")
                           + a, cwd=self.repo, check=True, env=env, timeout=60,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        run("init", "-q", "-b", "main")
        run("config", "user.email", "sweep@example.invalid")
        run("config", "user.name", "sweep")
        run("add", "-A")
        run("commit", "-qm", "base")
        self.head = sweep.git("rev-parse", "HEAD", cwd=self.repo).strip()
        (self.tmp / "boxes").mkdir()
        self.link = self.tmp / "boxes-through-a-symlink"
        self.link.symlink_to(self.tmp / "boxes", target_is_directory=True)

    def test_a_sandbox_asked_for_under_a_symlink_reports_where_it_really_is(self):
        box = sweep.Sandbox(self.repo, self.link / "w0", self.head, {})
        self.assertEqual(box.path, self.tmp / "boxes" / "w0")
        self.assertEqual((box.path / "charter" / "m.py").read_text(), "x = 1\n")


class TheReportNamesTheMaskingRisk(unittest.TestCase):
    def _result(self, line, symbol):
        m = sweep.Mutation(path="charter/frame/layout.py", line=line, end_line=line,
                           operator="drop-conjunct", question="is the half pinned?",
                           before="isinstance(name, str) and name not in SLOT_SIZE",
                           after="isinstance(name, str)", symbol=symbol)
        return sweep.Result(m, "survived", sweep.Outcome(True, 40, "OK"),
                            sweep.Outcome(True, 6000, "OK"), [],
                            sweep.Evidence([], []))

    def test_two_survivors_in_one_function_are_flagged_as_maskable(self):
        text = sweep.report([self._result(291, "_placed_here"),
                             self._result(289, "_placed_here")],
                            Path("."), "abc", "def", None, 1.0)
        self.assertIn("2 survivors sit in `_placed_here`", text)
        self.assertIn("mask each other", text)

    def test_a_lone_survivor_is_not_flagged(self):
        text = sweep.report([self._result(291, "_placed_here")],
                            Path("."), "abc", "def", None, 1.0)
        self.assertNotIn("mask each other", text)

    def test_a_clean_sweep_says_so_and_lists_nothing(self):
        """A sweep that MEASURED something and found nothing wrong. It used to be written
        with an empty result set, which is a sweep that measured nothing — the same
        sentence for the two facts #782 exists to tell apart, in the terminal report this
        time rather than in the check's name."""
        text = sweep.report([_result("pinned")], Path("."), "abc", "def", None, 1.0)
        self.assertIn("Every mutation this diff offered goes red", text)
        self.assertNotIn("NOTHING TO SWEEP", text)

    def test_the_survivor_line_carries_file_line_operator_and_both_spellings(self):
        text = sweep.report([self._result(291, "_placed_here")],
                            Path("."), "abc", "def", None, 1.0)
        self.assertIn("charter/frame/layout.py:291", text)
        self.assertIn("drop-conjunct", text)
        self.assertIn("shipped :", text)
        self.assertIn("mutant  :", text)


class ASurvivorIsASurvivorOnTHISPlatform(unittest.TestCase):
    """A clause the operating system never reaches is unreachable, not untested.

    Measured on this project: `except OSError: return None` around a pty read is dead code
    on macOS, where closing the far end returns `b""`, and live on Linux, where it raises
    `[Errno 5]`. A local sweep sees a survivor either way. Scoring that as a missing test
    is a false positive, and false positives are what get a gate switched off — so the
    harness labels it rather than silently counting it.
    """

    def _survivor(self, before, operator="narrow-except"):
        m = sweep.Mutation(path="charter/frame/overlay.py", line=400, end_line=400,
                           operator=operator, question="?", before=before,
                           after="ZeroDivisionError", symbol="run")
        return sweep.Result(m, "survived", sweep.Outcome(True, 40, "OK"),
                            sweep.Outcome(True, 6000, "OK"), [], sweep.Evidence([], []))

    def test_a_narrowed_os_level_catch_is_flagged_for_a_second_opinion(self):
        text = sweep.report([self._survivor("OSError")], Path("."), "a", "b", None, 1.0)
        self.assertIn("    PLATFORM:", text)
        self.assertIn("OSError", text)
        self.assertIn("throwaway branch", text)

    def test_a_catch_charter_itself_raises_is_not_flagged(self):
        text = sweep.report([self._survivor("ControlPlaneNotFound")],
                            Path("."), "a", "b", None, 1.0)
        self.assertNotIn("    PLATFORM:", text)

    def test_a_deleted_guard_is_not_flagged_merely_for_naming_an_oserror(self):
        """The caveat is about a CATCH the OS may never trigger, not about any line that
        happens to mention one."""
        text = sweep.report([self._survivor("if isinstance(e, OSError): return None",
                                            operator="drop-if")],
                            Path("."), "a", "b", None, 1.0)
        self.assertNotIn("    PLATFORM:", text)

    def test_a_clause_naming_two_os_level_types_names_one_of_them_the_same_way_twice(self):
        """`sorted(...)[0]` and not `next(iter(...))`: a set's iteration order is not the
        set's, so an unsorted pick would name `OSError` on one run and `TimeoutError` on
        the next for the same clause — and a caveat that changes between runs is one nobody
        can act on or diff. Found by the sweep on this file: `swap-synonym` turned the
        `sorted` into a `list` and every case still passed."""
        m = sweep.Mutation("charter/a.py", 1, 1, "narrow-except", "q?",
                           "(TimeoutError, OSError, PermissionError)", "ZeroDivisionError",
                           "f")
        self.assertEqual(sweep.platform_caveat(m), "OSError")
        self.assertEqual(sweep.platform_caveat(m), sweep.platform_caveat(m))

    def test_the_report_says_which_platform_it_measured_on(self):
        text = sweep.report([], Path("."), "a", "b", None, 1.0)
        self.assertIn(sys.platform, text)


class ThereIsNoSuppressionList(unittest.TestCase):
    def test_no_marker_or_ignore_or_skip_key_exists_anywhere_in_the_tool(self):
        """#370's reasoning, applied here: one charter could read is one a committed file
        could flip. If deleting a line genuinely changes nothing observable, the line
        should be deleted — 'equivalent mutant' and 'dead code' are the same finding, so
        there is nothing for a suppression list to hold."""
        source = Path(sweep.__file__).read_text()
        tree = ast.parse(source)
        names = {n.id for n in ast.walk(tree) if isinstance(n, ast.Name)}
        names |= {n.attr for n in ast.walk(tree) if isinstance(n, ast.Attribute)}
        for forbidden in ("suppress", "ignore_list", "allowlist", "equivalent",
                          "nosweep", "skip_mutation", "exclusions"):
            self.assertNotIn(forbidden, names, f"a suppression hatch named {forbidden}")


class _Refusing:
    """A sandbox whose `apply` refuses, so `decide` can be asked what it does about it."""

    def __init__(self, why="the mutant is byte-identical to the tree it replaces"):
        self.why = why
        self.subset_calls = 0
        self.full_calls = 0

    def clean_failures(self, modules):
        return frozenset()

    def apply(self, m):
        raise sweep.NotApplied(self.why)

    def restore(self):
        pass

    def subset(self, modules):        # pragma: no cover - never reached, and that is the point
        self.subset_calls += 1
        return sweep.Outcome(True, 9, "OK")

    def full(self):                   # pragma: no cover - same
        self.full_calls += 1
        return sweep.Outcome(True, 9, "OK")


class AMutationThatNeverAppliedIsNotASurvivor(unittest.TestCase):
    """The fifth way a sweep lies (#586), closed by construction.

    The edit does not match, so the "mutant" tree is the UNMUTATED tree, the suite passes,
    and the guard is reported as a survivor. It is the only one of the five that errs
    toward *more* work rather than less, and it is the one that ends adoption: somebody
    writes a test for a line already covered, finds out, and stops believing the tool.
    """

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="sweep-applied-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp,
                                                            ignore_errors=True))
        (self.tmp / "charter").mkdir()
        (self.tmp / "charter" / "m.py").write_bytes(b"def f(x):\n    if x:\n        return 1\n")
        self.box = sweep.Sandbox.__new__(sweep.Sandbox)
        self.box.path = self.tmp
        self.box._pristine = {}
        self.box._clean_failures = {}

    def _mutation(self, **over):
        source = (self.tmp / "charter" / "m.py").read_bytes()
        m = _mutations(source.decode())[0]
        return dataclasses.replace(m, path="charter/m.py", **over)

    def test_a_mutant_identical_to_the_tree_is_refused_rather_than_run(self):
        pristine = (self.tmp / "charter" / "m.py").read_bytes()
        m = self._mutation(source=pristine,
                           origin=hashlib.sha256(pristine).hexdigest())
        with self.assertRaises(sweep.NotApplied):
            self.box.apply(m)

    def test_a_sandbox_holding_a_different_file_is_refused(self):
        """A mutation carries a digest of the file it was READ FROM. A sandbox at another
        ref, or one an earlier restore did not undo, fails here rather than producing a
        verdict about bytes nobody looked at."""
        m = self._mutation(source=b"x = 1\n", origin="0" * 64)
        with self.assertRaises(sweep.NotApplied) as caught:
            self.box.apply(m)
        self.assertIn("not the file the mutation was read from", str(caught.exception))

    def test_the_mutation_that_does_apply_is_written_and_can_be_undone(self):
        pristine = (self.tmp / "charter" / "m.py").read_bytes()
        m = self._mutation(source=b"def f(x):\n    pass\n",
                           origin=hashlib.sha256(pristine).hexdigest())
        self.box.apply(m)
        self.assertEqual((self.tmp / "charter" / "m.py").read_bytes(),
                         b"def f(x):\n    pass\n")
        self.box.restore()
        self.assertEqual((self.tmp / "charter" / "m.py").read_bytes(), pristine)

    def test_every_mutation_this_tool_offers_carries_the_digest_of_its_own_source(self):
        source = b"def f(x):\n    if x:\n        return 1\n"
        for m in sweep.mutations_for("charter/m.py", source, {1, 2, 3}):
            self.assertEqual(m.origin, hashlib.sha256(source).hexdigest())

    def test_decide_calls_it_unapplied_and_never_survived(self):
        box = _Refusing()
        verdict, subset, full = sweep.decide(box, self._mutation(), ["tests.test_a"])
        self.assertEqual(verdict, "unapplied")
        self.assertIsNone(full)
        self.assertFalse(subset.conclusive)
        self.assertEqual(box.full_calls, 0)

    def test_the_report_says_so_in_its_own_section(self):
        m = self._mutation()
        r = sweep.Result(m, "unapplied",
                         sweep.Outcome(False, 0, "this edit did not happen",
                                       conclusive=False), None, [])
        text = sweep.report([r], self.tmp, "a" * 12, "b" * 12, None, 1.0)
        self.assertIn("NOT APPLIED", text)
        self.assertIn("this edit did not happen", text)
        self.assertNotIn("Every mutation this diff offered goes red", text)

    def test_two_sweeps_of_one_checkout_do_not_share_a_sandbox(self):
        """The collision that produced the assertion's first real catch.

        `workdir_for` gives one workdir per checkout, which is right for the trace cache
        and wrong for the sandboxes: two sweeps of the same tree — two agents on one box,
        or one person who forgot the first run was still going — would apply mutations to
        each other's files. Measured on `tools/sweep.py` by accident: two overlapping runs
        and 486 of 489 mutations came back `unapplied`, which is the digest check refusing
        to answer about a tree it did not recognise. Before that check the same run would
        have printed a plausible table of pins and survivors.
        """
        self.assertIn(str(os.getpid()), str(sweep.run_dir(Path("/w"))))
        self.assertEqual(sweep.run_dir(Path("/w")).parent, Path("/w"))

    def test_the_cache_is_still_shared_between_runs(self):
        """Keyed by a hash of the tree, so two runs of one checkout want the same map and
        paying for it twice is pure loss. Only the sandboxes are private."""
        self.assertNotIn(str(os.getpid()), str(sweep.workdir_for(Path.cwd(), None)))

    def test_a_run_with_one_is_not_a_clean_exit(self):
        m = self._mutation()
        clean = [sweep.Result(m, "pinned", None, None, [])]
        self.assertEqual(sweep.exit_code(clean), 0)
        self.assertEqual(sweep.exit_code(clean + [sweep.Result(m, "unapplied", None,
                                                               None, [])]), 4)


# ======================================================================================
# Stage C — the gate
# ======================================================================================

def _result(verdict, path="charter/a.py", line=1, symbol="f", operator="drop-if",
            before="if x:\n    return", evidence=None):
    m = sweep.Mutation(path=path, line=line, end_line=line, operator=operator,
                       question="is the refusal pinned?", before=before, after="",
                       symbol=symbol)
    return sweep.Result(m, verdict, None, sweep.Outcome(True, 10, "OK"), ["tests.test_a"],
                        evidence)


class TheGateSortsSurvivorsIntoWhatItMayActOn(unittest.TestCase):
    """Three categories stage A already knows, kept apart because the responses differ.

    Collapsing any two of them is how this gate gets switched off inside a week — most of
    all the platform one, because a gate that fails a pull request for a clause the
    runner's kernel cannot reach deserves to be disabled.
    """

    def test_a_lone_survivor_is_unpinned_and_actionable(self):
        gate = sweep.classify([_result("survived")])
        self.assertEqual(len(gate.unpinned), 1)
        self.assertEqual(gate.masked, [])
        self.assertEqual(len(gate.actionable), 1)

    def test_two_survivors_in_one_function_are_a_masked_cluster(self):
        """Two guards in sequence mask each other, so neither is safe to call equivalent
        on its own. Still actionable — more so — but read together, which is why it is its
        own bucket rather than a note on a row."""
        gate = sweep.classify([_result("survived", line=1), _result("survived", line=9)])
        self.assertEqual(gate.unpinned, [])
        self.assertEqual(len(gate.masked), 2)
        self.assertEqual(len(gate.actionable), 2)

    def test_a_platform_survivor_is_deferred_and_never_actionable(self):
        gate = sweep.classify([_result("survived", operator="narrow-except",
                                       before="OSError")])
        self.assertEqual(len(gate.platform), 1)
        self.assertEqual(gate.actionable, [])

    def test_two_platform_survivors_in_one_function_stay_platform(self):
        """The platform question is asked first, and it has to be: a clause the kernel
        never enters is unreachable whether or not a second one sits beside it, and
        promoting the pair to a masked cluster would fail the gate on exactly the finding
        it is not allowed to fail on."""
        gate = sweep.classify([_result("survived", operator="narrow-except",
                                       before="OSError", line=n) for n in (1, 9)])
        self.assertEqual(len(gate.platform), 2)
        self.assertEqual(gate.actionable, [])

    def test_unresolved_and_unapplied_and_pinned_are_their_own_answers(self):
        gate = sweep.classify([_result("unresolved"), _result("unapplied"),
                               _result("pinned"), _result("pinned")])
        self.assertEqual((len(gate.unresolved), len(gate.unapplied), gate.pinned),
                         (1, 1, 2))
        self.assertEqual(gate.actionable, [])


class TheGateBlocksNothingUntilItIsToldTo(unittest.TestCase):
    """The spec's staging argument, applied to the gate's own credibility.

    "A gate whose baseline nobody has seen gets disabled the first time it is
    inconvenient." So the first version of this job reports its numbers and blocks
    nothing, and `--enforce` is the single flag that changes that.
    """

    def test_a_survivor_does_not_fail_a_reporting_run(self):
        gate = sweep.classify([_result("survived")])
        self.assertEqual(sweep.gate_exit_code(gate, enforce=False), 0)

    def test_the_same_survivor_fails_an_enforcing_run(self):
        gate = sweep.classify([_result("survived")])
        self.assertEqual(sweep.gate_exit_code(gate, enforce=True), 1)

    def test_a_platform_survivor_passes_even_when_enforcing(self):
        gate = sweep.classify([_result("survived", operator="narrow-except",
                                       before="OSError")])
        self.assertEqual(sweep.gate_exit_code(gate, enforce=True), 0)

    def test_an_unmeasured_mutation_is_its_own_exit_code_and_not_a_pass(self):
        """A timeout is not a red and not a survivor. Under load this repository hits it
        repeatedly, and "I could not look" must never render as "nothing to see"."""
        gate = sweep.classify([_result("unresolved"), _result("pinned")])
        self.assertEqual(sweep.gate_exit_code(gate, enforce=True), 3)

    def test_a_mutation_that_never_applied_outranks_everything(self):
        gate = sweep.classify([_result("unapplied"), _result("survived"),
                               _result("unresolved")])
        self.assertEqual(sweep.gate_exit_code(gate, enforce=True), 4)

    def test_a_clean_branch_passes_either_way(self):
        gate = sweep.classify([_result("pinned")])
        self.assertEqual(sweep.gate_exit_code(gate, enforce=False), 0)
        self.assertEqual(sweep.gate_exit_code(gate, enforce=True), 0)


class TheGateSaysWhatTheCoveringTestsAssert(unittest.TestCase):
    """The field that made 82 survivors triageable rather than merely alarming.

    `release.yml`'s `-z "$claimed"` refusal (#558) is why: deleting it left the run still
    exiting 1, for a different reason. So the honest first question about a survivor is
    "did my test look closely enough", and nobody can answer that from a line number.
    """

    def test_a_survivor_carries_the_assertions_that_were_supposed_to_hold_it(self):
        ev = sweep.Evidence(["tests.test_a"],
                            [("tests.test_a", "test_it_refuses",
                              ["self.assertEqual(f(None), [])"])])
        gate = sweep.classify([_result("survived", evidence=ev)])
        text = sweep.gate_summary(gate, "a" * 40, "b" * 40, 60.0, enforce=False)
        self.assertIn("test_it_refuses", text)
        self.assertIn("self.assertEqual(f(None), [])", text)

    def test_a_symbol_no_covering_test_names_is_said_plainly(self):
        ev = sweep.Evidence(["tests.test_a", "tests.test_b"], [])
        gate = sweep.classify([_result("survived", evidence=ev)])
        text = sweep.gate_summary(gate, "a" * 40, "b" * 40, 60.0, enforce=False)
        self.assertIn("not one names", text)

    def test_a_file_nothing_executes_is_said_plainly_too(self):
        gate = sweep.classify([_result("survived", evidence=sweep.Evidence([], []))])
        text = sweep.gate_summary(gate, "a" * 40, "b" * 40, 60.0, enforce=False)
        self.assertIn("nothing measured executes this file", text)

    def test_the_three_categories_are_headed_separately(self):
        gate = sweep.classify([
            _result("survived", path="charter/a.py"),
            _result("survived", path="charter/b.py", line=1),
            _result("survived", path="charter/b.py", line=9),
            _result("survived", path="charter/c.py", operator="narrow-except",
                    before="OSError"),
            _result("unresolved", path="charter/d.py")])
        text = sweep.gate_summary(gate, "a" * 40, "b" * 40, 60.0, enforce=False)
        for heading in ("### Unpinned", "### Masked cluster",
                        "### Platform-deferred", "### Unresolved"):
            self.assertIn(heading, text)

    def test_a_reporting_run_says_on_the_page_that_it_blocks_nothing(self):
        gate = sweep.classify([_result("survived")])
        self.assertIn("Reporting only",
                      sweep.gate_summary(gate, "a" * 40, "b" * 40, 60.0, enforce=False))
        self.assertNotIn("Reporting only",
                         sweep.gate_summary(gate, "a" * 40, "b" * 40, 60.0, enforce=True))

    def test_a_run_with_an_unapplied_mutation_says_to_read_nothing_else(self):
        gate = sweep.classify([_result("unapplied"), _result("survived")])
        text = sweep.gate_summary(gate, "a" * 40, "b" * 40, 60.0, enforce=False)
        self.assertIn("until this is zero", text)

    def test_a_clean_branch_says_so(self):
        text = sweep.gate_summary(sweep.classify([_result("pinned")]),
                                  "a" * 40, "b" * 40, 60.0, enforce=False)
        self.assertIn("Nothing added here is a line the suite would not miss", text)


class TheGateSaysWhichOfThreeThingsItFound(unittest.TestCase):
    """#617's urgent half: a green gate is not "no survivors".

    The sweep reported `success` on a branch with **eight survivors** under it, which to
    anyone who does not open the run summary reads as "this branch is clean" — the
    opposite of what the job exists to say. A conclusion cannot carry the difference, so
    the answer is a sentence, and the sentence is what the check is named with. There are
    exactly three things it may say, and no two of them may collapse into one.
    """

    def test_a_branch_with_nothing_surviving_says_no_survivors(self):
        gate = sweep.classify([_result("pinned"), _result("pinned")])
        self.assertEqual(sweep.gate_conclusion(gate), sweep.CLEAN)
        self.assertEqual(sweep.headline(gate), "no survivors")

    def test_a_branch_with_survivors_says_how_many(self):
        """The whole finding. `success` with eight under it said nothing; this says 8."""
        gate = sweep.classify([_result("survived", path=f"charter/{n}.py")
                               for n in range(8)])
        self.assertEqual(sweep.gate_conclusion(gate), sweep.SURVIVORS)
        self.assertEqual(sweep.headline(gate), "8 survivors")

    def test_one_survivor_is_not_called_survivors(self):
        gate = sweep.classify([_result("survived")])
        self.assertEqual(sweep.headline(gate), "1 survivor")

    def test_a_shard_that_never_reported_is_no_verdict_and_not_no_survivors(self):
        """The regression that matters most. Every mutation that DID report went red, so
        every count on the page is zero — and reading that as a clean branch is exactly
        the mistake, because the shard that vanished is the one with the answer in it."""
        gate = sweep.classify([_result("pinned")])
        self.assertEqual(sweep.gate_conclusion(gate, missing=2), sweep.NO_VERDICT)
        self.assertEqual(sweep.headline(gate, missing=2, shards=3),
                         "no verdict: 2 of 3 shards did not report")
        self.assertNotIn("no survivors", sweep.headline(gate, missing=2, shards=3))

    def test_a_run_that_never_sized_itself_does_not_invent_a_denominator(self):
        """"1 of 1 shard did not report" describes a sweep that was planned and then lost
        one. A plan job that failed before it counted anything is a different fact, and a
        denominator nobody computed is not a denominator."""
        gate = sweep.classify([])
        self.assertEqual(sweep.headline(gate, missing=1, shards=0),
                         "no verdict: the sweep never sized itself")
        self.assertEqual(sweep.headline(gate, missing=1, shards=1),
                         "no verdict: 1 of 1 shard did not report")
        page = sweep.gate_summary(gate, "a" * 40, "b" * 40, None, False, 1, 0)
        self.assertIn("never said how many shards it needed", page)
        self.assertNotIn("1 of 1", page)

    def test_a_mutation_nobody_measured_is_no_verdict_too(self):
        gate = sweep.classify([_result("unresolved"), _result("pinned")])
        self.assertEqual(sweep.gate_conclusion(gate), sweep.NO_VERDICT)
        self.assertEqual(sweep.headline(gate), "no verdict: 1 not measured")

    def test_a_mutation_that_never_applied_is_no_verdict(self):
        gate = sweep.classify([_result("unapplied"), _result("pinned")])
        self.assertEqual(sweep.gate_conclusion(gate), sweep.NO_VERDICT)
        self.assertEqual(sweep.headline(gate), "no verdict: 1 mutation never applied")

    def test_survivors_outrank_an_unmeasured_mutation_and_both_are_said(self):
        """"8 survivors, 1 not measured" is a true statement about 8 real findings, and
        burying them under "no verdict" would lose the only actionable thing on the page.
        Precedence is `gate_exit_code`'s — 4 over 1 over 3 — because the two answers are
        one answer in two vocabularies."""
        gate = sweep.classify([_result("survived", path=f"charter/{n}.py")
                               for n in range(8)] + [_result("unresolved",
                                                             path="charter/z.py")])
        self.assertEqual(sweep.gate_conclusion(gate), sweep.SURVIVORS)
        self.assertEqual(sweep.headline(gate), "8 survivors, 1 not measured")

    def test_a_missing_shard_outranks_survivors_and_still_names_them(self):
        """A count taken from two thirds of a plan is a number nobody should quote, so the
        conclusion is `no verdict` — but the survivors that DID arrive are real and stay
        on the line."""
        gate = sweep.classify([_result("survived", path="charter/a.py"),
                               _result("survived", path="charter/b.py")])
        self.assertEqual(sweep.gate_conclusion(gate, missing=1), sweep.NO_VERDICT)
        self.assertEqual(sweep.headline(gate, missing=1, shards=3),
                         "no verdict: 2 survivors so far, 1 of 3 shards did not report")

    def test_a_platform_survivor_does_not_make_the_headline_say_survivor(self):
        """The gate never fails on one of these and it must not shout about one either:
        a check named "1 survivor" for a clause the runner's kernel cannot reach is the
        noise that gets a gate muted."""
        gate = sweep.classify([_result("survived", operator="narrow-except",
                                       before="OSError")])
        self.assertEqual(sweep.gate_conclusion(gate), sweep.CLEAN)
        self.assertEqual(sweep.headline(gate), "no survivors")

    def test_the_headline_is_one_line_because_it_is_a_checks_name(self):
        for gate, missing in ((sweep.classify([_result("pinned")]), 0),
                              (sweep.classify([_result("survived")]), 0),
                              (sweep.classify([_result("unapplied")]), 2)):
            said = sweep.headline(gate, missing, 3)
            self.assertNotIn("\n", said)
            self.assertLess(len(said), 90, said)

    def test_the_page_is_headed_with_the_same_sentence_the_check_is_named_with(self):
        """One sentence, both readers. A summary whose title disagreed with the row on the
        pull request would be worse than either of them alone."""
        gate = sweep.classify([_result("survived", path="charter/a.py"),
                               _result("survived", path="charter/b.py")])
        text = sweep.gate_summary(gate, "a" * 40, "b" * 40, 60.0, enforce=False)
        self.assertIn("## Deletion sweep — 2 survivors", text)

    def test_the_page_says_how_long_it_took_or_that_it_did_not_measure_that(self):
        """`elapsed is None`, found unpinned by the sweep on the branch that added it.

        A merge adds up several machines' results in about a second, and printing that
        second as the sweep's wall clock would understate a forty-minute run by two orders
        of magnitude, on the one line a reader skims. So the merged page says where its
        number came from instead — and the *measured* page has to actually carry the
        minutes, or "says so instead" is the only thing either page ever says.
        """
        gate = sweep.classify([_result("pinned")])
        measured = sweep.gate_summary(gate, "a" * 40, "b" * 40, 1234.0, enforce=False)
        merged = sweep.gate_summary(gate, "a" * 40, "b" * 40, None, enforce=False)
        self.assertIn("20.6 min on ", measured)
        self.assertNotIn("merged from its shards", measured)
        self.assertIn("merged from its shards on ", merged)

    def test_a_page_missing_a_shard_refuses_to_call_the_branch_clean(self):
        gate = sweep.classify([_result("pinned")])
        text = sweep.gate_summary(gate, "a" * 40, "b" * 40, 60.0, enforce=False,
                                  missing=2, shards=3)
        self.assertIn("did not report", text)
        self.assertNotIn("Nothing added here is a line the suite would not miss", text)

    def test_the_table_carries_the_missing_shards_as_a_row_with_a_count_in_it(self):
        """`[59]` and `[60]` from the self-sweep, and both are one mistake.

        The test above looks for "did not report" *anywhere on the page* — and the section
        further down carries that phrase in its heading, so deleting the table row
        outright left the suite green, and so did collapsing the cell that holds the
        count. A row is a number; assert the number, in both of the shapes it has.
        """
        gate = sweep.classify([_result("pinned")])
        counted = sweep.gate_summary(gate, "a" * 40, "b" * 40, None, False,
                                     missing=2, shards=3)
        self.assertIn("| **did not report** | 2 of 3 |", counted)
        # One shard, and it was the one that vanished — the ordinary shape for a small
        # diff, and the edge `shards >= 1` actually turns on.
        lone = sweep.gate_summary(gate, "a" * 40, "b" * 40, None, False,
                                  missing=1, shards=1)
        self.assertIn("| **did not report** | 1 of 1 |", lone)
        unsized = sweep.gate_summary(gate, "a" * 40, "b" * 40, None, False,
                                     missing=1, shards=0)
        self.assertIn("| **did not report** | ? |", unsized)

    def test_a_page_with_every_shard_in_still_says_the_branch_is_clean(self):
        text = sweep.gate_summary(sweep.classify([_result("pinned")]),
                                  "a" * 40, "b" * 40, 60.0, enforce=False)
        self.assertIn("Nothing added here is a line the suite would not miss", text)

    def test_saying_it_is_not_failing_on_it(self):
        """"Distinguish the three states" and "block on two of them" are different asks,
        and only the first one is #617. `--enforce` remains the single flag that decides."""
        for results in ([_result("survived")], [_result("unresolved")],
                        [_result("unapplied")]):
            gate = sweep.classify(results)
            self.assertEqual(sweep.verdict_exit_code(gate, missing=2, enforce=False), 0)

    def test_a_shard_that_never_reported_exits_three_once_the_gate_enforces(self):
        """The same code an unresolved mutation gets, for the same reason: the run has not
        shown the branch to be clean, and "I could not look" is not "nothing to see"."""
        gate = sweep.classify([_result("pinned")])
        self.assertEqual(sweep.verdict_exit_code(gate, missing=1, enforce=True), 3)
        self.assertEqual(sweep.verdict_exit_code(gate, missing=0, enforce=True), 0)

    def test_a_missing_shard_does_not_downgrade_a_worse_answer(self):
        gate = sweep.classify([_result("unapplied")])
        self.assertEqual(sweep.verdict_exit_code(gate, missing=1, enforce=True), 4)
        gate = sweep.classify([_result("survived")])
        self.assertEqual(sweep.verdict_exit_code(gate, missing=1, enforce=True), 1)


class NothingToSweepIsNotNoSurvivors(unittest.TestCase):
    """#782, which is #617 and #630 one level further in.

    Each earlier fix removed one way for silence to read as success: a green tick that was
    not "no survivors" (#617), a check whose NAME had to carry the verdict because
    GitHub's conclusions cannot (#630), a run that never happened and therefore had no row
    at all (#646/#561). This is the last one, and the most misleading of them, because
    nothing looks wrong: the row is present, the job succeeded, and the sentence is
    affirmative. Measured on #779 — `no survivors: pass` beside `mutations applied: 0`.

    It is a fourth ANSWER and not a fourth failure. The branches that land here are docs,
    config, a news entry, a one-line reordering — all legitimate — and failing them would
    teach people to route around the gate, which is worse than a gate that says too little.
    """

    def test_a_sweep_that_applied_no_mutation_does_not_say_it_found_none(self):
        gate = sweep.classify([])
        self.assertEqual(gate.measured, 0)
        self.assertEqual(sweep.gate_conclusion(gate), sweep.NOTHING)
        self.assertEqual(sweep.headline(gate), "nothing to sweep")
        self.assertNotIn("no survivors", sweep.headline(gate))

    def test_one_mutation_that_went_red_is_a_sweep_that_checked(self):
        """The other side of the same line, and the one that keeps `no survivors` meaning
        what it means: a single pinned mutation IS a measurement, and calling that
        "nothing to sweep" would spend the sentence on the branches that earned the
        other one."""
        gate = sweep.classify([_result("pinned")])
        self.assertEqual(gate.measured, 1)
        self.assertEqual(sweep.gate_conclusion(gate), sweep.CLEAN)
        self.assertEqual(sweep.headline(gate), "no survivors")

    def test_every_bucket_that_reached_a_sandbox_counts_as_measured(self):
        """`unresolved` and `unapplied` are not "nothing to sweep" — they are mutations
        that exist and came back without an answer, which is `no verdict` and stays there.
        A run that folded them into the empty case would turn the loudest state this gate
        has into the quietest."""
        for verdict in ("pinned", "survived", "unresolved", "unapplied"):
            gate = sweep.classify([_result(verdict)])
            self.assertEqual(gate.measured, 1, verdict)
            self.assertNotEqual(sweep.gate_conclusion(gate), sweep.NOTHING, verdict)

    def test_a_shard_that_vanished_can_never_wear_the_empty_sentence(self):
        """The dangerous confusion, and the reason `missing` is asked first. A sweep whose
        every shard was cancelled merges to zero results too, and "nothing to sweep" said
        over that would be the #617 defect returning under a new name."""
        gate = sweep.classify([])
        self.assertEqual(sweep.gate_conclusion(gate, missing=1), sweep.NO_VERDICT)
        self.assertEqual(sweep.headline(gate, missing=1, shards=2),
                         "no verdict: 1 of 2 shards did not report")

    def test_it_is_a_verdict_and_not_a_failure_even_once_the_gate_enforces(self):
        """A docs-only branch is legitimate and common. Failing it would train people to
        bypass the gate, which is the one outcome worse than a gate that says too little.
        Asserted under `--enforce`, because that is the flag every other bucket changes
        behaviour on and this one must not."""
        gate = sweep.classify([])
        self.assertEqual(sweep.gate_exit_code(gate, enforce=True), 0)
        self.assertEqual(sweep.verdict_exit_code(gate, missing=0, enforce=True), 0)

    def test_the_page_says_what_did_not_happen_instead_of_congratulating_the_branch(self):
        page = sweep.gate_summary(sweep.classify([]), "a" * 40, "b" * 40, 12.0, False)
        self.assertIn("## Deletion sweep — nothing to sweep", page)
        self.assertIn("Not one mutation was applied on this branch", page)
        self.assertNotIn("Nothing added here is a line the suite would not miss", page)

    def test_a_page_that_did_measure_something_keeps_the_clean_sentence(self):
        page = sweep.gate_summary(sweep.classify([_result("pinned")]),
                                  "a" * 40, "b" * 40, 12.0, False)
        self.assertIn("Nothing added here is a line the suite would not miss", page)
        self.assertNotIn("Not one mutation was applied on this branch", page)


class TheSweepIsSplitAcrossMachinesAndNothingIsDropped(unittest.TestCase):
    """#617's other half: five runs cancelled at `timeout-minutes: 60` across two branches.

    The answer is more machines and never fewer questions. A cap on the mutation count
    reads as "covered everything" to every reader downstream — the spec says so about
    silent truncation — so the tests below are mostly one property said several ways:
    **every mutation lands in exactly one shard.**
    """

    @staticmethod
    def _plan(n):
        return [sweep.Mutation(f"charter/f{i // 7}.py", i, i, "drop-if", "q?",
                               f"if x{i}: pass", "", "f") for i in range(n)]

    def test_the_shards_partition_the_plan(self):
        for total in (0, 1, 7, 62, 78, 82, 225):
            plan = self._plan(total)
            for count in (1, 2, 3, 8):
                dealt = [m for i in range(1, count + 1)
                         for m in sweep.shard_of(plan, i, count)]
                self.assertEqual(sorted(m.line for m in dealt),
                                 sorted(m.line for m in plan),
                                 f"{total} mutations across {count} shard(s)")
                self.assertEqual(len(dealt), total)

    def test_no_shard_carries_more_than_one_extra_mutation(self):
        """Round-robin, so the slowest shard decides the wall clock and the slowest shard
        is within one mutation of the fastest."""
        plan = self._plan(82)
        sizes = [len(sweep.shard_of(plan, i, 3)) for i in (1, 2, 3)]
        self.assertEqual(sorted(sizes), [27, 27, 28])

    def test_a_shard_number_outside_its_count_is_refused_rather_than_clamped(self):
        """Off-by-one here does not fail loudly: it sweeps one slice twice and another
        never, and the merge step then reports a complete sweep of an incomplete plan."""
        plan = self._plan(9)
        for index, count in ((0, 3), (4, 3), (-1, 3), (1, 0)):
            with self.assertRaises(ValueError):
                sweep.shard_of(plan, index, count)

    def test_the_shard_argument_is_parsed_strictly(self):
        self.assertEqual(sweep.parse_shard("2/3"), (2, 3))
        for bad in ("2", "2/", "/3", "0/3", "4/3", "", "two/three", "2/3/4"):
            with self.assertRaises(ValueError, msg=bad):
                sweep.parse_shard(bad)

    def test_one_shard_measures_what_it_can_finish_in_its_budget(self):
        """Forty minutes a shard, eleven of which are gone before the first mutation — the
        checkout, the sandbox clone, the selection map and the unmutated baseline. The
        literal is here rather than the arithmetic on purpose: a test that recomputed it
        from the constants would pass whatever the constants became."""
        self.assertEqual(sweep.per_shard(), 28)

    def test_a_budget_smaller_than_its_own_fixed_cost_still_measures_something(self):
        """`max(1, …)`, found unpinned by the sweep on this very branch.

        Raise the fixed cost past the budget — a slower runner, a longer suite, a map that
        stops being cached — and the subtraction goes to zero or below. Without the clamp
        the division in :func:`shards_for` is by zero, the plan job raises, and the
        workflow gets an empty shard count: no plan, no shards, no numbers. Which is the
        #617 failure exactly, arriving out of the arithmetic written to prevent it.
        """
        real = sweep.SHARD_FIXED
        self.addCleanup(lambda: setattr(sweep, "SHARD_FIXED", real))
        for fixed in (sweep.SHARD_BUDGET, sweep.SHARD_BUDGET + 3600):
            sweep.SHARD_FIXED = fixed
            self.assertEqual(sweep.per_shard(), 1, fixed)
            self.assertEqual(sweep.shards_for(50), 8, fixed)   # capped, and not a crash

    def test_the_two_diffs_that_ran_out_of_time_now_fit(self):
        """#608's 62 mutations were cancelled twice and #626's 78 three times, at
        `timeout-minutes: 60`, each run reaching about two thirds of its plan."""
        self.assertEqual(sweep.shards_for(62), 3)
        self.assertEqual(sweep.shards_for(78), 3)
        for total in (62, 78):
            biggest = max(len(sweep.shard_of(self._plan(total), i, 3))
                          for i in (1, 2, 3))
            self.assertLessEqual(biggest, sweep.per_shard())

    def test_a_branch_phase_two_would_have_produced_still_gets_one_job(self):
        self.assertEqual(sweep.shards_for(1), 1)
        self.assertEqual(sweep.shards_for(28), 1)
        self.assertEqual(sweep.shards_for(29), 2)

    def test_a_branch_with_nothing_to_sweep_still_gets_a_job(self):
        """"The sweep ran and found nothing to do" and "the sweep did not run" are the two
        answers this whole change is about. A plan of zero shards would make them one."""
        self.assertEqual(sweep.shards_for(0), 1)

    def test_the_fan_out_is_capped_and_the_sweep_is_not(self):
        """`MAX_SHARDS` limits how much of the runner pool one branch may hold. It does
        not limit what gets asked: past the ceiling the shards simply carry more."""
        self.assertEqual(sweep.shards_for(10_000), 8)
        plan = self._plan(400)
        dealt = [m for i in range(1, 9) for m in sweep.shard_of(plan, i, 8)]
        self.assertEqual(len(dealt), 400)

    def test_a_diff_past_the_ceiling_says_so_out_loud(self):
        """The spec is explicit that a cap the reader cannot see is worse than the cap."""
        self.assertEqual(sweep.over_budget(224), "")
        loud = sweep.over_budget(225)
        self.assertIn("225 mutations", loud)
        self.assertIn("still swept", loud)
        self.assertIn("nothing here is dropped", loud)


#: Every duration a piece of prose quotes in seconds. The shape the workflow's comments
#: used — ``350 s``, ``250 s``, ``about 240 seconds`` — and deliberately not minutes, so
#: `timeout-minutes: 30` (a YAML key, not a claim about a measurement) is not one.
#:
#: A function rather than a regex inlined into an assertion, because the assertion below
#: is green on an empty list and would therefore be green against a reader that matched
#: nothing at all. `TheWorkflowQuotesNoCostTheToolDoesNotState` proves the reader first.
_DURATION = re.compile(r"\b(\d+)\s?(?:s\b|seconds\b)")


def durations_stated(prose: str) -> list[int]:
    return [int(n) for n in _DURATION.findall(prose)]


class TheWorkflowQuotesNoCostTheToolDoesNotState(unittest.TestCase):
    """#670: one measurement, written down twice and not identically.

    `SHARD_FIXED`'s itemisation said the selection map costs **250 s** and `sweep.yml`'s
    cache step said **350 s**, about the same trace, and *neither was asserted* — so
    nothing in the repository was able to notice. It survived because nothing downstream
    turns on it: `SHARD_FIXED` is twelve minutes and the itemised total fits under it at
    either reading, so no arithmetic moved and no run failed. It was only a number a
    reader would quote, with no way to know which one to quote.

    Re-measured across nine cache-miss runs on `ubuntu-latest` — the tool prints its own
    ``selection map: … in Ns`` — the trace is 242 to 285 s. 250 was honest when #630 wrote
    it and the suite has grown past it; 350 matches no run at all.

    **The fix is the second copy, not the digits.** `SHARD_FIXED_COSTS` is now the one
    place the itemisation is written, `sweep.yml` names the constant instead of quoting a
    figure, and this class holds any duration the workflow's comments *do* quote to one
    the tool states. A third copy is then either right or red.
    """

    WORKFLOW = Path(__file__).resolve().parents[1] / ".github" / "workflows" / "sweep.yml"

    def test_the_reader_finds_the_copy_this_class_is_about(self):
        """The control, and the reason the reader is a function.

        The assertion below is satisfied by an empty list, which is the same shape of
        defect as #669 next door — an assertion about absence standing in for one about a
        value. This one runs the reader over the exact line #670 reported."""
        self.assertEqual(
            durations_stated("# The selection map is one trace of the whole suite — "
                             "350 s measured, and the"), [350])
        self.assertEqual(durations_stated("under 5 s, and about 240 seconds"), [5, 240])
        self.assertEqual(durations_stated("timeout-minutes: 30"), [])
        self.assertEqual(durations_stated("python-version: \"3.12\""), [])

    def test_every_duration_the_workflow_quotes_is_one_the_tool_states(self):
        stated = set(sweep.SHARD_FIXED_COSTS.values())
        loose = [n for n in durations_stated(self.WORKFLOW.read_text()) if n not in stated]
        self.assertEqual(
            loose, [],
            f"sweep.yml quotes {loose} s, which `SHARD_FIXED_COSTS` does not state. That "
            "is the #670 shape: a cost written down in two files, in words, in neither of "
            "which anything can check it. Name the constant in the comment instead, or "
            "move the measurement into `SHARD_FIXED_COSTS` and let the comment cite it.")

    def test_the_budget_covers_the_itemisation_it_is_written_from(self):
        """`SHARD_FIXED`'s note claims to be the no-cache figure rounded up past the items.
        Asserted, so a re-measurement that outgrows the budget is red here rather than in
        a shard that gets cancelled at `timeout-minutes` and reports nothing at all."""
        itemised = sum(sweep.SHARD_FIXED_COSTS.values())
        self.assertLess(itemised, sweep.SHARD_FIXED,
                        "the fixed costs no longer fit the fixed-cost budget")
        self.assertLess(sweep.SHARD_FIXED, sweep.SHARD_BUDGET,
                        "a shard would have no time left to measure a mutation in")

    def test_the_itemisation_names_every_cost_and_the_map_is_the_largest(self):
        """A dict is a source of truth only while it is complete: drop an entry and the
        sum still fits the budget, silently, which is how a shard comes to be sized
        against a cost it no longer counts. And "the largest fixed cost a shard pays" is
        the claim `sweep.yml`'s cache step makes about the map — held to the numbers."""
        self.assertEqual(sorted(sweep.SHARD_FIXED_COSTS), [
            "checkout at fetch-depth 0, and the interpreter",
            "the sandbox clone",
            "the selection map, traced",
            "the unmutated baseline",
        ])
        self.assertEqual(max(sweep.SHARD_FIXED_COSTS,
                             key=sweep.SHARD_FIXED_COSTS.__getitem__),
                         "the selection map, traced")


class EverySurvivorLandsOnTheLineItIsAbout(unittest.TestCase):
    """The other half of "without failing": the finding goes where the reviewer is looking.

    An annotation renders in the margin of the diff, on the guard itself, and changes
    nothing about the job's conclusion — which is the shape #617 asks for. The check's
    name says how many; this says where.
    """

    def test_an_unpinned_survivor_is_a_warning_on_its_own_file_and_line(self):
        gate = sweep.classify([_result("survived", path="charter/gitconfig.py", line=42)])
        said = sweep.annotations(gate)
        self.assertEqual(len(said), 1)
        self.assertTrue(said[0].startswith("::warning file=charter/gitconfig.py,line=42,"),
                        said[0])
        self.assertIn("Unpinned guard", said[0])

    def test_a_platform_survivor_is_a_notice_and_never_a_warning(self):
        """The gate never fails on one of these, so it never shouts about one either."""
        gate = sweep.classify([_result("survived", operator="narrow-except",
                                       before="OSError")])
        said = sweep.annotations(gate)
        self.assertEqual(len(said), 1)
        self.assertTrue(said[0].startswith("::notice "), said[0])

    def test_a_mutation_that_never_applied_is_an_error_because_it_is_a_defect_here(self):
        gate = sweep.classify([_result("unapplied")])
        self.assertTrue(sweep.annotations(gate)[0].startswith("::error "))

    def test_a_masked_cluster_says_so_rather_than_reading_as_two_lone_survivors(self):
        gate = sweep.classify([_result("survived", line=1), _result("survived", line=9)])
        self.assertEqual(len(gate.masked), 2)
        for line in sweep.annotations(gate):
            self.assertIn("Masked cluster", line)

    def test_an_unmeasured_mutation_is_annotated_too(self):
        gate = sweep.classify([_result("unresolved", path="charter/z.py", line=3)])
        said = sweep.annotations(gate)
        self.assertIn("::warning file=charter/z.py,line=3,", said[0])
        self.assertIn("No verdict", said[0])

    def test_past_the_cap_the_ones_not_drawn_are_counted_out_loud(self):
        """GitHub draws ten annotations of a level and then stops, with no error and no
        warning. Silence there is the silent-truncation shape again."""
        gate = sweep.classify([_result("survived", path=f"charter/{n}.py")
                               for n in range(14)])
        said = sweep.annotations(gate)
        self.assertEqual(len(said), 10)                     # nine findings and the note
        self.assertEqual(sum(1 for s in said if "file=" in s), 9)
        self.assertIn("5 of these 14 are not shown", said[-1])
        self.assertIn("all 14", said[-1])

    def test_exactly_the_cap_is_drawn_whole_and_one_more_costs_a_slot(self):
        """The boundary the reserved slot turns on, found unpinned by the sweep.

        At exactly ten there is nothing to announce, so all ten are findings. At eleven
        the note has to fit inside the same ten, so nine are. Without a case sitting on
        the boundary, "always ten" and "always nine" both pass — and "always ten" is the
        one that loses the note, which is the only line that distinguishes *these ten*
        from *these ten of twenty-two*.
        """
        def drawn(n):
            gate = sweep.classify([_result("survived", path=f"charter/{i}.py")
                                   for i in range(n)])
            said = sweep.annotations(gate)
            return sum(1 for s in said if "file=" in s), len(said)

        self.assertEqual(drawn(9), (9, 9))       # under the cap: no note
        self.assertEqual(drawn(10), (10, 10))    # exactly the cap: still no note
        self.assertEqual(drawn(11), (9, 10))     # one over: nine findings and the note

    def test_the_cap_is_a_levels_budget_and_not_one_kind_of_findings(self):
        """Masked clusters, lone survivors and unmeasured mutations are all warnings, so
        they share one budget of ten. Capping each family at ten instead would emit thirty
        and have GitHub silently draw the first ten of them."""
        gate = sweep.classify(
            [_result("survived", path="charter/m.py", line=n) for n in (1, 9)]
            + [_result("survived", path=f"charter/u{n}.py") for n in range(9)]
            + [_result("unresolved", path=f"charter/z{n}.py") for n in range(9)])
        said = sweep.annotations(gate)
        self.assertEqual(sum(1 for s in said if s.startswith("::warning file=")), 9)
        self.assertEqual(len(said), 10)

    def test_the_urgent_families_are_the_ones_that_survive_the_cap(self):
        """A masked cluster is the finding a reviewer is least able to reach alone, and an
        unmeasured mutation is a fact about the runner rather than about the branch. When
        only nine of twenty fit, that is the order they go in."""
        gate = sweep.classify(
            [_result("survived", path="charter/m.py", line=n) for n in (1, 9)]
            + [_result("unresolved", path=f"charter/z{n}.py") for n in range(20)])
        drawn = [s for s in sweep.annotations(gate) if "file=" in s]
        self.assertEqual(sum(1 for s in drawn if "Masked cluster" in s), 2)
        self.assertEqual(sum(1 for s in drawn if "No verdict" in s), 7)

    def test_a_notice_does_not_spend_a_warnings_budget(self):
        """Platform-deferred survivors are notices, which is a separate ten."""
        gate = sweep.classify(
            [_result("survived", path=f"charter/u{n}.py") for n in range(9)]
            + [_result("survived", path=f"charter/p{n}.py", operator="narrow-except",
                       before="OSError") for n in range(4)])
        said = sweep.annotations(gate)
        self.assertEqual(sum(1 for s in said if s.startswith("::warning ")), 9)
        self.assertEqual(sum(1 for s in said if s.startswith("::notice ")), 4)

    def test_a_path_with_a_comma_or_a_colon_in_it_cannot_move_the_annotation(self):
        """A `,` starts the next property and a `:` can close the list early, so an
        unescaped one annotates the wrong place or nothing at all."""
        gate = sweep.classify([_result("survived", path="charter/a,b:c.py")])
        self.assertIn("file=charter/a%2Cb%3Ac.py,line=1,", sweep.annotations(gate)[0])

    def test_a_newline_in_the_message_cannot_end_the_command_early(self):
        gate = sweep.classify([_result("survived", before="if x:\n    return 100%")])
        said = sweep.annotations(gate)
        self.assertEqual(len(said), 1)
        self.assertNotIn("\n", said[0])

    def test_the_escaping_is_the_encoding_the_runner_decodes_and_not_merely_removal(self):
        """Found by the sweep on this branch: retuning `"%0D"` left every test green.

        Asserting that the newline is *gone* is not asserting that it became `%0A`. A
        wrong encoding does not break the command — the runner prints a mangled message,
        which is the kind of wrong nobody files and nobody can read either. So the bytes
        are pinned, and pinning them pins the ORDER too: `%` has to be escaped first, or
        the `%` of `%0A` gets escaped again and `a\\nb` comes out as `a%250Ab`.
        """
        self.assertEqual(sweep._escape("100% done\r\nnext"), "100%25 done%0D%0Anext")
        self.assertEqual(sweep._property("a,b:c%d\ne"), "a%2Cb%3Ac%25d%0Ae")

    def test_an_annotation_is_not_a_failure(self):
        gate = sweep.classify([_result("survived"), _result("unapplied",
                                                            path="charter/b.py")])
        self.assertTrue(sweep.annotations(gate))
        self.assertEqual(sweep.gate_exit_code(gate, enforce=False), 0)


class TheAnswerSurvivesTheTripThroughAFile(unittest.TestCase):
    """One question, several machines, one answer — which means a round trip through JSON.

    Everything `classify` and the summary read has to survive it, not merely the verdict
    string. The strongest form of that is the one asserted first: the page a merge writes
    is the page a single run would have written, character for character.
    """

    @staticmethod
    def _mixed():
        ev = sweep.Evidence(["tests.test_a"],
                            [("tests.test_a", "test_it_refuses",
                              ["self.assertEqual(f(None), [])"])])
        return [_result("survived", path="charter/a.py", evidence=ev),
                _result("survived", path="charter/b.py", line=1),
                _result("survived", path="charter/b.py", line=9),
                _result("survived", path="charter/c.py", operator="narrow-except",
                        before="OSError"),
                _result("unresolved", path="charter/d.py"),
                _result("pinned", path="charter/e.py")]

    def test_the_merged_page_is_the_page_one_machine_would_have_written(self):
        results = self._mixed()
        here = sweep.gate_summary(sweep.classify(results), "a" * 40, "b" * 40, 60.0, False)
        there = sweep.gate_summary(
            sweep.classify(sweep.results_from_json(sweep.as_json(results))),
            "a" * 40, "b" * 40, 60.0, False)
        self.assertEqual(here, there)
        self.assertIn("self.assertEqual(f(None), [])", there)

    def test_the_outcome_of_a_mutation_that_never_applied_survives_the_trip(self):
        """`[31/43]` from the sharded self-sweep, in the half no earlier run reached.

        `results_from_json`'s `outcome()` collapsed to `None` and every page still
        matched — because the one place a subset's detail is printed is the not-applied
        section, and the round-trip fixture had no not-applied result in it. A shard
        reporting "the edit never landed" is the case where the merge has to say WHY, and
        it was the case nothing round-tripped.
        """
        r = _result("unapplied", path="charter/a.py")
        r.subset = sweep.Outcome(False, 0, "the tree did not match the mutation's origin")
        back = sweep.results_from_json(sweep.as_json([r]))
        self.assertEqual(back[0].subset, r.subset)
        self.assertEqual(back[0].full, r.full)
        page = sweep.gate_summary(sweep.classify(back), "a" * 40, "b" * 40, None, False)
        self.assertIn("the tree did not match the mutation's origin", page)

    def test_a_survivors_own_platform_is_recomputed_and_not_read_from_the_file(self):
        """The shard that measured it and the machine that merges it are two computers.
        Trusting the shard's answer would let one platform's verdict be reported as
        another's."""
        back = sweep.results_from_json(sweep.as_json(self._mixed()))
        self.assertEqual(len(sweep.classify(back).platform), 1)

    def test_a_file_nothing_executes_does_not_come_back_as_a_file_that_ignores_it(self):
        """The absence that has to survive the trip. "Nothing measured executes this file"
        and "one module executes it and does not name the symbol" are different findings
        and different next moves, and an evidence pass that never ran must not arrive
        looking like one that ran and found nothing."""
        never = _result("survived", path="charter/a.py")          # evidence pass not run
        ran = _result("survived", path="charter/b.py",
                      evidence=sweep.Evidence(["tests.test_a"], []))
        back = sweep.results_from_json(sweep.as_json([never, ran]))
        self.assertIsNone(back[0].evidence)
        self.assertEqual(back[1].evidence, sweep.Evidence(["tests.test_a"], []))
        page = sweep.gate_summary(sweep.classify(back), "a" * 40, "b" * 40, 60.0, False)
        self.assertIn("nothing measured executes this file", page)
        self.assertIn("**not one names `f`**", page)

    def test_the_shards_add_up_to_the_whole_sweep(self):
        results = self._mixed()
        with tempfile.TemporaryDirectory() as tmp:
            for i in range(3):
                (Path(tmp) / f"sweep-results-{i + 1}.json").write_text(
                    sweep.as_json(results[i::3]))
            merged, missing = sweep.merge(Path(tmp), 3)
        self.assertEqual(missing, 0)
        self.assertEqual(len(merged), len(results))
        self.assertEqual(sorted(r.mutation.path for r in merged),
                         sorted(r.mutation.path for r in results))

    def test_a_masked_cluster_split_across_two_machines_is_still_a_masked_cluster(self):
        """The reason the sorting happens at the merge and not in the shard.

        Two guards in sequence mask each other, and the round-robin deal puts neighbours
        in *different* shards on purpose — so neither shard can see the pair. Classify one
        shard at a time and a masked cluster reads as two lone survivors in two jobs, each
        of which looks equivalent on its own, which is the exact reading the bucket exists
        to prevent.
        """
        pair = [_result("survived", path="charter/a.py", line=1, symbol="close"),
                _result("survived", path="charter/a.py", line=9, symbol="close")]
        for one in pair:                       # one shard at a time sees a lone survivor
            self.assertEqual(len(sweep.classify([one]).unpinned), 1)
        with tempfile.TemporaryDirectory() as tmp:
            for i, r in enumerate(pair, start=1):
                (Path(tmp) / f"sweep-results-{i}.json").write_text(sweep.as_json([r]))
            merged, missing = sweep.merge(Path(tmp), 2)
        gate = sweep.classify(merged)
        self.assertEqual((missing, len(gate.masked), gate.unpinned), (0, 2, []))
        self.assertIn("### Masked cluster",
                      sweep.gate_summary(gate, "a" * 40, "b" * 40, None, False))

    def test_a_shard_that_wrote_nothing_is_counted_as_a_shard_that_did_not_report(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "sweep-results-1.json").write_text(sweep.as_json([]))
            merged, missing = sweep.merge(Path(tmp), 3)
        self.assertEqual((len(merged), missing), (0, 2))

    def test_a_result_file_that_will_not_parse_is_not_a_shard_that_answered(self):
        """There is no third reading of a truncated upload."""
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "a.json").write_text(sweep.as_json([]))
            (Path(tmp) / "b.json").write_text('[{"path": "charter/a.py"')
            (Path(tmp) / "c.json").write_text('[{"path": "charter/a.py"}]')
            merged, missing = sweep.merge(Path(tmp), 3)
        self.assertEqual((len(merged), missing), (0, 2))

    def test_an_empty_sweep_that_ran_is_not_a_sweep_that_did_not(self):
        """A shard with nothing to do writes `[]`, and `[]` is an answer. The directory
        being empty is the other thing entirely.

        And `[]` is `NOTHING`, not `CLEAN` (#782). This case asserted `CLEAN` and the
        assertion was the defect written down: a shard that measured no mutation at all
        reached the pull request as `no survivors`, which is the sentence for having
        measured everything. The distinction this case exists for — an answer against no
        answer — is untouched, and there are now three of them rather than two.
        """
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "sweep-results-1.json").write_text(sweep.as_json([]))
            ran, ran_missing = sweep.merge(Path(tmp), 1)
        with tempfile.TemporaryDirectory() as tmp:
            gone, gone_missing = sweep.merge(Path(tmp), 1)
        self.assertEqual((ran, ran_missing), ([], 0))
        self.assertEqual(gone_missing, 1)
        self.assertEqual(sweep.gate_conclusion(sweep.classify(ran), ran_missing),
                         sweep.NOTHING)
        self.assertEqual(sweep.gate_conclusion(sweep.classify(gone), gone_missing),
                         sweep.NO_VERDICT)

    def test_a_directory_that_is_not_there_is_no_verdict_and_not_a_clean_one(self):
        merged, missing = sweep.merge(Path("/nonexistent-sweep-shards"), 2)
        self.assertEqual((merged, missing), ([], 2))

    def test_a_plan_that_never_said_how_many_shards_is_not_no_shards(self):
        """A plan job that fails leaves the output empty. Reading an empty string as zero
        would turn the loudest failure this workflow has into the quietest kind of pass."""
        self.assertEqual(sweep.expected_shards("3"), 3)
        self.assertEqual(sweep.expected_shards(" 3 "), 3)
        for nothing in ("", None, "0", "-1", "many", "3.5", []):
            self.assertEqual(sweep.expected_shards(nothing), 0, repr(nothing))


class TheWorkflowAsksTheToolAndNotTheOtherWayAround(unittest.TestCase):
    """The three commands `.github/workflows/sweep.yml` runs, run.

    Every decision the workflow makes now lives here rather than in YAML — how many jobs,
    which slice, what the check is called — for #572's reason: a rule inside a `run:`
    block is not reachable from a test, so it cannot be swept, so it is a guard the
    harness is structurally unable to hold itself to. What is left in the YAML is which
    values are passed, and these cases run the commands that consume them.
    """

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="sweep-gate-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp, ignore_errors=True))
        env = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull,
                   GIT_CONFIG_SYSTEM=os.devnull, GIT_CONFIG_NOSYSTEM="1",
                   GIT_TERMINAL_PROMPT="0")

        def run(*a):
            subprocess.run(("git", "-c", "core.hooksPath=", "-c", "commit.gpgsign=false")
                           + a, cwd=self.tmp, check=True, env=env, timeout=60,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        run("init", "-q", "-b", "main")
        run("config", "user.email", "sweep@example.invalid")
        run("config", "user.name", "sweep")
        (self.tmp / "charter").mkdir()
        (self.tmp / "charter" / "m.py").write_text("a = 1\n")
        run("add", "-A")
        run("commit", "-qm", "one")
        self.base = sweep.git("rev-parse", "HEAD", cwd=self.tmp).strip()
        run("checkout", "-q", "-b", "side")
        (self.tmp / "charter" / "m.py").write_text(textwrap.dedent("""
            def close(arm, pane):
                if arm is None:
                    return []
                if pane is None:
                    return []
                return [arm, pane]
        """).lstrip())
        run("add", "-A")
        run("commit", "-qm", "two")
        self.workdir = self.tmp / "wd"
        self.outputs = self.tmp / "outputs.txt"
        self.summary = self.tmp / "summary.md"

    def _cli(self, *args):
        said = io.StringIO()
        cwd = os.getcwd()
        os.chdir(self.tmp)
        try:
            with contextlib.redirect_stdout(said):
                code = sweep.main(list(args))
        finally:
            os.chdir(cwd)
        return code, said.getvalue()

    def _read_outputs(self):
        return dict(line.split("=", 1)
                    for line in self.outputs.read_text().splitlines() if line)

    def test_the_plan_costs_no_test_runs_and_says_how_many_jobs_it_needs(self):
        """The number the job used to guess. It was sized against 30 to 52 mutations and
        met 78, five times, and every one of those runs was cancelled at the cap."""
        code, said = self._cli("--plan", "--base", self.base,
                               "--workdir", str(self.workdir),
                               "--github-output", str(self.outputs))
        self.assertEqual(code, 0, said)
        out = self._read_outputs()
        self.assertEqual(int(out["mutations"]), 2)      # the two refusals added above
        self.assertEqual(out["shards"], "1")
        self.assertEqual(out["matrix"], "[1]")
        # And the sha those two mutations were read against, for the merge step's page
        # header — the one machine that cannot work it out for itself (#776).
        self.assertEqual(out["base"], self.base)

    def test_the_plan_publishes_the_base_it_resolved_and_not_the_one_it_was_given(self):
        """No `--base` at all is the shape CI runs, and the output has to carry a real sha
        even then — an empty `base` reaches the merge step's header as the literal string
        it falls back to, and the page would stop naming what was charged."""
        code, said = self._cli("--plan", "--workdir", str(self.workdir),
                               "--github-output", str(self.outputs))
        self.assertEqual(code, 0, said)
        self.assertEqual(self._read_outputs()["base"], self.base)
        self.assertIn(f"diff against {self.base[:12]}", said)

    def test_a_diff_past_the_fan_out_ceiling_warns_on_the_pull_request(self):
        """A log line is not loud. `over_budget` reaching the plan's stdout as a workflow
        command is what puts it in the run's annotations instead of in a fold, and the
        spec's rule is that a cap the reader cannot see is worse than the cap."""
        plan = [sweep.Mutation("charter/m.py", n, n, "drop-if", "q?", "if x: pass",
                               "", "f") for n in range(300)]
        real = sweep.plan_for
        sweep.plan_for = lambda *a: (plan, {})
        self.addCleanup(lambda: setattr(sweep, "plan_for", real))
        code, said = self._cli("--plan", "--base", self.base,
                               "--workdir", str(self.workdir),
                               "--github-output", str(self.outputs))
        self.assertEqual(code, 0, said)
        self.assertIn("::warning title=The deletion sweep is over its budget::", said)
        self.assertIn("nothing here is dropped", said)
        # And it still asks about all three hundred, on the eight machines it is allowed.
        self.assertEqual(self._read_outputs()["shards"], "8")
        self.assertEqual(len([m for i in range(1, 9)
                              for m in sweep.shard_of(plan, i, 8)]), 300)

    def test_a_diff_inside_the_ceiling_says_nothing_about_the_ceiling(self):
        code, said = self._cli("--plan", "--base", self.base,
                               "--workdir", str(self.workdir),
                               "--github-output", str(self.outputs))
        self.assertEqual(code, 0, said)
        self.assertNotIn("::warning", said)

    def test_the_matrix_names_every_shard_the_plan_asked_for(self):
        """The workflow fans out over exactly this list, so a plan of three shards that
        emitted two entries would sweep two thirds of the branch and say nothing.

        Driven through the CLI rather than by recomputing `shards_for` here: a test that
        rebuilds the answer out of the same function it is checking agrees with that
        function whatever it says, which is the shape that survives its own mutation.
        """
        real = sweep.plan_for
        self.addCleanup(lambda: setattr(sweep, "plan_for", real))
        for total, shards, matrix in ((0, "1", "[1]"), (28, "1", "[1]"),
                                      (29, "2", "[1, 2]"), (78, "3", "[1, 2, 3]")):
            plan = [sweep.Mutation("charter/m.py", n, n, "drop-if", "q?", "if x: pass",
                                   "", "f") for n in range(total)]
            sweep.plan_for = lambda *a, _p=plan: (_p, {})
            self.outputs.unlink(missing_ok=True)
            code, said = self._cli("--plan", "--base", self.base,
                                   "--workdir", str(self.workdir),
                                   "--github-output", str(self.outputs))
            self.assertEqual(code, 0, said)
            out = self._read_outputs()
            self.assertEqual((out["mutations"], out["shards"], out["matrix"]),
                             (str(total), shards, matrix), f"{total} mutations")

    def test_the_shard_the_workflow_names_is_the_shard_the_sweep_runs(self):
        """`--shard "$SHARD/$SHARDS"` is a string assembled in YAML, and the one thing it
        must not do is arrive as something else. A slice that is silently wrong does not
        fail — it sweeps some mutations twice and others never, and the merge then reports
        a complete sweep of an incomplete plan."""
        seen = []
        for name, stub in (("sweep", lambda *a, **k: (seen.append(a[-1]), ([], []))[1]),
                           ("load_map", lambda *a, **k: {})):
            real = getattr(sweep, name)
            setattr(sweep, name, stub)
            self.addCleanup(setattr, sweep, name, real)
        code, said = self._cli("--gate", "--jobs", "1", "--base", self.base,
                               "--shard", "2/3", "--no-baseline",
                               "--workdir", str(self.workdir))
        self.assertEqual(code, 0, said)
        self.assertEqual(seen, [(2, 3)])

    def test_the_slice_a_shard_takes_is_the_slice_it_says_it_took(self):
        """The log line a cancelled run leaves behind is the only trace of what it was
        doing, so it names the shard and how much of the plan the shard was given."""
        said = io.StringIO()
        plan = [sweep.Mutation("charter/m.py", n, n, "drop-if", "q?", "if x: pass",
                               "", "close") for n in range(1, 6)]
        real = sweep.plan_for
        sweep.plan_for = lambda *a: (plan, {})
        self.addCleanup(lambda: setattr(sweep, "plan_for", real))
        with contextlib.redirect_stdout(said):
            results, _ = sweep.sweep(self.tmp, "HEAD", {"charter/m.py": {1}}, {},
                                     self.workdir, 1, {}, 0, print, 60.0, (2, 3))
        self.assertIn("5 mutations across 1 file(s); shard 2 of 3 takes 2 of them",
                      said.getvalue())
        self.assertEqual([r.mutation.line for r in results], [2, 5])

    def test_no_shard_at_all_still_means_the_whole_plan(self):
        """Found by the sweep, on this branch, against this file: forcing `if shard is not
        None` to always-true left every test green, because nothing in the suite ran an
        UNSHARDED sweep any more. A local `tools/sweep.py --gate` is the ordinary way this
        tool is used, and it had quietly become the path no test walked."""
        said = io.StringIO()
        plan = [sweep.Mutation("charter/m.py", n, n, "drop-if", "q?", "if x: pass",
                               "", "close") for n in range(1, 6)]
        real = sweep.plan_for
        sweep.plan_for = lambda *a: (plan, {})
        self.addCleanup(lambda: setattr(sweep, "plan_for", real))
        with contextlib.redirect_stdout(said):
            results, _ = sweep.sweep(self.tmp, "HEAD", {"charter/m.py": {1}}, {},
                                     self.workdir, 1, {}, 0, print, 60.0, None)
        self.assertEqual([r.mutation.line for r in results], [1, 2, 3, 4, 5])
        self.assertIn("5 mutations across 1 file(s)\n", said.getvalue())
        self.assertNotIn("shard", said.getvalue())

    def test_a_branch_with_nothing_to_sweep_writes_an_empty_answer_and_not_no_answer(self):
        """A shard that writes nothing is indistinguishable from a shard that was
        cancelled, which is the whole of #617 arriving one level down.

        And it says so in the sentence the check is named with (#782). This path used to
        publish a private paragraph of its own — "there was nothing to sweep" — into the
        step summary while the check on the pull request said `no survivors`. Two readers,
        two answers, and the affirmative one is the one people read.
        """
        code, said = self._cli("--gate", "--base", "HEAD", "--path", "charter",
                               "--workdir", str(self.workdir),
                               "--summary", str(self.summary),
                               "--github-output", str(self.outputs),
                               "--json", str(self.tmp / "r.json"))
        self.assertEqual(code, 0, said)
        self.assertEqual((self.tmp / "r.json").read_text(), "[]")
        merged, missing = sweep.merge(self.tmp, 1)
        self.assertEqual((merged, missing), ([], 0))
        self.assertEqual(self._read_outputs(),
                         {"conclusion": "nothing", "headline": "nothing to sweep"})
        self.assertIn("## Deletion sweep — nothing to sweep", self.summary.read_text())

    def test_a_diff_that_offers_no_mutation_is_not_a_branch_that_was_checked(self):
        """#779's own shape, run end to end, and the half `--base HEAD` cannot reach.

        Here a file DID change and lines WERE added — the scope is not empty — and still
        not one mutation exists, because no operator has anything to say about
        `rows.append(1)`. That is the normal result for exactly the changes a reviewer is
        least able to check by eye, and it published `no survivors: pass` beside
        `mutations applied: 0`.
        """
        was = sweep.git("rev-parse", "HEAD", cwd=self.tmp).strip()
        source = (self.tmp / "charter" / "m.py").read_text()
        (self.tmp / "charter" / "m.py").write_text(
            source + "\n\ndef record(rows):\n    rows.append(1)\n")
        subprocess.run(("git", "-c", "core.hooksPath=", "commit", "-qam", "one statement"),
                       cwd=self.tmp, check=True, timeout=60,
                       env=dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull,
                                GIT_CONFIG_SYSTEM=os.devnull, GIT_CONFIG_NOSYSTEM="1"),
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        # The fixture repository has no `tests/`, so the trace would refuse to sweep
        # blind. The plan is what this case is about and `plan_for` is untouched.
        real = sweep.load_map
        sweep.load_map = lambda *a, **k: {}
        self.addCleanup(lambda: setattr(sweep, "load_map", real))
        code, said = self._cli("--gate", "--base", was, "--no-baseline", "--jobs", "1",
                               "--workdir", str(self.workdir),
                               "--summary", str(self.summary),
                               "--github-output", str(self.outputs))
        self.assertEqual(code, 0, said)
        # The diff is real. The plan is not.
        self.assertIn("1 file(s), 4 added line(s)", said)
        self.assertIn("0 mutations across 1 file(s)", said)
        self.assertIn("NOTHING TO SWEEP", said)
        self.assertNotIn("Every mutation this diff offered goes red", said)
        self.assertEqual(self._read_outputs(),
                         {"conclusion": "nothing", "headline": "nothing to sweep"})
        self.assertIn("Not one mutation was applied on this branch",
                      self.summary.read_text())

    def test_a_merged_sweep_that_measured_nothing_says_so_on_the_pull_request(self):
        """The same answer through the file every shard writes and the step that adds them
        up — which is where the pull request actually reads it."""
        code, said = self._cli("--verdict", str(self._shards([])), "--shards", "1",
                               "--summary", str(self.summary),
                               "--github-output", str(self.outputs))
        self.assertEqual(code, 0, said)
        self.assertEqual(self._read_outputs(),
                         {"conclusion": "nothing", "headline": "nothing to sweep"})

    def _shards(self, *payloads):
        where = self.tmp / "shards"
        where.mkdir(exist_ok=True)
        for i, results in enumerate(payloads, start=1):
            (where / f"sweep-results-{i}.json").write_text(sweep.as_json(results))
        return where

    def test_the_merge_names_the_branch_with_its_own_answer(self):
        where = self._shards([_result("survived", path="charter/a.py")],
                             [_result("pinned", path="charter/b.py")])
        code, said = self._cli("--verdict", str(where), "--shards", "2",
                               "--ref", "a" * 40, "--base", "b" * 40, "--annotate",
                               "--summary", str(self.summary),
                               "--github-output", str(self.outputs))
        self.assertEqual(code, 0, said)
        self.assertEqual(self._read_outputs(),
                         {"conclusion": "survivors", "headline": "1 survivor"})
        self.assertIn("## Deletion sweep — 1 survivor", self.summary.read_text())
        self.assertIn("::warning file=charter/a.py,line=1,", said)

    def test_a_shard_that_never_reported_reaches_the_check_as_no_verdict(self):
        where = self._shards([_result("pinned", path="charter/b.py")])
        code, said = self._cli("--verdict", str(where), "--shards", "3",
                               "--summary", str(self.summary),
                               "--github-output", str(self.outputs))
        self.assertEqual(code, 0, said)
        self.assertEqual(self._read_outputs()["conclusion"], "no-verdict")
        self.assertEqual(self._read_outputs()["headline"],
                         "no verdict: 2 of 3 shards did not report")
        self.assertIn("did not report", self.summary.read_text())

    def test_a_plan_that_never_ran_is_no_verdict_and_says_which_job_failed(self):
        """`needs.plan.outputs.shards` is the empty string when that job failed. An empty
        string is not zero shards."""
        code, said = self._cli("--verdict", str(self._shards()), "--shards", "",
                               "--summary", str(self.summary),
                               "--github-output", str(self.outputs))
        self.assertEqual(code, 0, said)
        self.assertEqual(self._read_outputs(),
                         {"conclusion": "no-verdict",
                          "headline": "no verdict: the sweep never sized itself"})
        self.assertIn("never said how many shards it needed", self.summary.read_text())
        self.assertIn("an unknown number of shard(s)", said)

    def test_a_complete_merge_of_a_clean_branch_says_no_survivors(self):
        where = self._shards([_result("pinned", path="charter/a.py")],
                             [_result("pinned", path="charter/b.py")])
        code, said = self._cli("--verdict", str(where), "--shards", "2",
                               "--summary", str(self.summary),
                               "--github-output", str(self.outputs))
        self.assertEqual(code, 0, said)
        self.assertEqual(self._read_outputs(),
                         {"conclusion": "clean", "headline": "no survivors"})

    def test_the_merge_blocks_nothing_until_it_is_told_to(self):
        where = self._shards([_result("survived", path="charter/a.py")])
        self.assertEqual(self._cli("--verdict", str(where), "--shards", "3")[0], 0)
        self.assertEqual(
            self._cli("--verdict", str(where), "--shards", "3", "--enforce")[0], 1)


class TheCheckIsNamedWithTheAnswer(unittest.TestCase):
    """`$GITHUB_OUTPUT` is the whole mechanism, so what goes into it is load-bearing."""

    def test_the_conclusion_and_the_headline_are_written_where_a_job_name_can_read_them(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out.txt"
            sweep._write_output(str(out), conclusion="survivors", headline="8 survivors")
            self.assertEqual(out.read_text(), "conclusion=survivors\nheadline=8 survivors\n")

    def test_a_value_with_a_line_break_of_either_spelling_is_refused(self):
        """`key=value` is one line. A value carrying a break would not set an output, it
        would inject the next one — and a bare `\\r` ends a line for the runner's parser
        just as `\\n` does, so both are refused rather than only the one that is easy to
        picture."""
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out.txt"
            for injected in ("8 survivors\nheadline=none", "8 survivors\rheadline=none"):
                with self.assertRaises(ValueError, msg=repr(injected)):
                    sweep._write_output(str(out), headline=injected)
            self.assertFalse(out.exists())

    def test_nothing_is_written_when_there_is_nowhere_to_write_it(self):
        sweep._write_output(None, headline="8 survivors")


class TheWorkflowSaysTheAnswerWhereItCanBeSeen(unittest.TestCase):
    """`.github/workflows/sweep.yml`, read as a tree rather than grepped.

    Everything above is about what `sweep.py` can say. These are about whether anything
    ever hears it — which is the half #617 was actually about, and the half no unit test
    of this module can reach. The reader is `tests/test_workflows.py`'s, because a second
    YAML parser in this repository would be a second thing to be wrong.
    """

    @classmethod
    def setUpClass(cls):
        from tests import test_workflows
        root = Path(__file__).resolve().parent.parent
        cls.wf = test_workflows.load(
            (root / ".github/workflows/sweep.yml").read_text(encoding="utf-8"))
        cls.jobs = cls.wf["jobs"]

    def _run(self, job, step_name):
        for step in self.jobs[job]["steps"]:
            if step.get("name") == step_name:
                return step["run"]
        self.fail(f"{job} has no step named {step_name!r}")

    def test_the_check_a_reviewer_sees_is_named_with_the_sweeps_own_answer(self):
        """The whole fix. A job name may interpolate `needs.<job>.outputs.*`, so the row
        on the pull request reads `deletion sweep / 8 survivors` instead of a tick beside
        a name that says the same thing on every branch."""
        name = self.jobs["verdict"]["name"]
        self.assertIn("needs.collect.outputs.headline", name)
        self.assertIn("no verdict", name)          # when collect itself did not answer
        self.assertEqual(self.jobs["verdict"]["needs"], "collect")

    def test_the_job_whose_exit_code_would_block_has_a_name_that_never_moves(self):
        """Branch protection matches a check by name, and the answering job's name IS the
        answer — different on every branch, which is the point and also what makes it
        useless as a required check. `collect` is the one to require, because it is where
        `--enforce` would land and its name is a constant."""
        self.assertNotIn("${{", self.jobs["collect"]["name"])
        self.assertIn("${{", self.jobs["verdict"]["name"])

    def test_the_job_that_carries_the_answer_runs_even_when_the_sweep_did_not(self):
        """`always()` on both, because the state this exists to report is the one where a
        shard was cancelled. A chain that only runs on success can never say so."""
        for job in ("collect", "verdict"):
            self.assertEqual(self.jobs[job]["if"], "always()", job)

    def test_one_shards_trouble_does_not_cancel_the_others(self):
        self.assertEqual(self.jobs["sweep"]["strategy"]["fail-fast"], "false")

    def _collect_step(self, step_id):
        for step in self.jobs["collect"]["steps"]:
            if step.get("id") == step_id:
                return step
        self.fail(f"collect has no step with id {step_id!r}")

    def test_a_cancelled_sweep_does_not_borrow_the_shard_sentence(self):
        """#654. `cancel-in-progress` means a second push cancels the first run, and
        `always()` carries that run into this job anyway — where the shard arithmetic
        answered for a run that had been stopped. Measured on this branch: run 99, cancelled
        by run 100, published `no verdict: 1 of 1 shard did not report`.

        The property is not that a step exists. It is that the words this path publishes are
        NOT the words the shard path publishes — #626's sentence has to keep meaning what it
        means, and a cancelled run must not be able to say it.

        **And the words have to be true of every cancelled `plan`, which is why "did not
        size itself" is refused here by name.** A cancelled sizing job has two causes and
        only two — a newer push, or `plan`'s own `timeout-minutes` — and either way it may
        have written its outputs before it was stopped. Run 99 is the proof: `1 of 1` is
        `expected_shards` reading a real `shards=1`, so that run HAD sized itself and was
        cancelled afterwards. What is true of all of them is that the sweep did not
        finish."""
        headline = self._collect_step("superseded")["run"]
        self.assertIn("did not finish", headline)
        self.assertNotIn("size", headline)
        self.assertNotIn("shard", headline)
        self.assertNotIn("survivor", headline)

    def test_the_two_answers_are_exact_negations_so_neither_can_shadow_the_other(self):
        """`headline` is `say || superseded`, which is only a choice between one value and
        one empty string while exactly one of them can run. Two conditions that merely
        looked different would make the `||` a precedence question, and a run could answer
        twice — the second answer silently losing to whichever `||` saw first."""
        yes = self._collect_step("superseded")["if"]
        no = self._collect_step("say")["if"]
        self.assertEqual(yes, "${{ needs.plan.result == 'cancelled' }}")
        self.assertEqual(no, yes.replace("==", "!="))

    def test_the_discriminator_is_the_sizing_job_and_not_the_runs_own_cancellation(self):
        """`cancelled()` was the obvious answer and it is the wrong one — measured, not
        reasoned. Run 99 was cancelled (`conclusion=cancelled`, `Size the sweep` cancelled,
        the shard cancelled) and `collect` still took the `say` branch. In a job running
        under `always()`, `cancelled()` does not answer for the run.

        `needs.plan.result` does, and it is also the narrower question: it is `cancelled`
        exactly when the sizing job did not finish, which is the only state in which the
        shard arithmetic is answering for a run that was stopped rather than about a branch.
        A SHARD that exceeds its own `timeout-minutes` leaves `plan` succeeded, so #626 is
        untouched."""
        for step_id in ("superseded", "say"):
            self.assertNotIn("cancelled()", self._collect_step(step_id)["if"], step_id)
            self.assertIn("needs.plan.result", self._collect_step(step_id)["if"], step_id)

    def test_the_superseded_answer_needs_nothing_a_cancellation_would_have_skipped(self):
        """The one state this step exists for is the state in which every step above it was
        cancelled — checkout, setup-python and the artifact download included. A step that
        reached for the tree, the interpreter or the shard files could not run there, and
        the run would fall back to no headline at all."""
        step = self._collect_step("superseded")
        self.assertNotIn("uses", step)
        for reach in ("python", "tools/sweep.py", "shards", "$GITHUB_STEP_SUMMARY"):
            self.assertNotIn(reach, step["run"], reach)

    def test_both_step_ids_reach_the_jobs_output(self):
        """A step that answers into `$GITHUB_OUTPUT` and is not named in `outputs:` is a
        verdict nothing collects — which is this file's own failure mode, one job over."""
        for key in ("headline", "conclusion"):
            out = self.jobs["collect"]["outputs"][key]
            self.assertIn(f"steps.say.outputs.{key}", out, key)
            self.assertIn(f"steps.superseded.outputs.{key}", out, key)

    def _commands(self):
        """Every line of every `run:` body that is a command rather than a comment.

        The comments matter here: the merge step's own comment SAYS `--enforce`, at length
        and on purpose, because that is where somebody will come looking to turn the gate
        on. Grepping the file would find it and pass, which would make this check the
        thing it is meant to catch — a guard that is satisfied by prose.
        """
        return [line for job in self.jobs.values() for step in job["steps"]
                for line in step.get("run", "").splitlines()
                if line.strip() and not line.strip().startswith("#")]

    def test_the_gate_still_blocks_nothing(self):
        """`--enforce` is the single flag, and it is absent. The spec's staging argument
        applies to this job's own credibility: a gate whose numbers nobody has read gets
        disabled the first time it is inconvenient."""
        self.assertEqual([line for line in self._commands() if "--enforce" in line], [])

    def test_the_gate_never_charges_the_whole_tree_from_ci_either(self):
        """Stage B is fourteen hours. It is not a pull-request check and never will be."""
        self.assertEqual([line for line in self._commands() if "--all" in line], [])

    def test_a_shard_is_told_which_slice_it_is_and_where_to_write_it(self):
        run = self._run("sweep", "Sweep the guards this branch adds")
        self.assertIn('--shard "$SHARD/$SHARDS"', run)
        self.assertIn('--json "sweep-results-$SHARD.json"', run)
        # And nothing that would have it write a page of its own: a slice of the answer
        # must never render as the answer.
        self.assertNotIn("--summary", run)
        self.assertNotIn("--annotate", run)

    def test_the_merge_is_the_only_step_that_tells_the_branch_anything(self):
        run = self._run("collect", "One answer for the whole sweep")
        for flag in ("--verdict shards", '--shards "${SHARDS:-}"', "--annotate",
                     '--summary "$GITHUB_STEP_SUMMARY"',
                     '--github-output "$GITHUB_OUTPUT"'):
            self.assertIn(flag, run)

    def test_the_shard_count_is_passed_through_without_a_default_that_hides_a_failure(self):
        """`${SHARDS:-}` and not `${SHARDS:-1}`. An empty value means the plan job never
        answered, and defaulting it to a number here would turn the loudest failure this
        workflow has into a complete-looking sweep of one shard."""
        run = self._run("collect", "One answer for the whole sweep")
        self.assertIn('--shards "${SHARDS:-}"', run)
        self.assertNotIn('${SHARDS:-1}', run)

    def test_the_plan_sizes_the_run_and_leaves_the_map_for_the_shards(self):
        run = self._run("plan", "How many mutations, and how many jobs they need")
        self.assertIn("--plan", run)
        self.assertIn("--warm-map", run)
        self.assertIn('--github-output "$GITHUB_OUTPUT"', run)
        self.assertEqual(self.jobs["sweep"]["strategy"]["matrix"]["shard"],
                         "${{ fromJSON(needs.plan.outputs.matrix) }}")

    def test_no_step_charges_a_branch_against_the_payloads_idea_of_the_base(self):
        """#776, and it is refused by NAME because the one-line convenience is exactly
        what was there. `github.event.pull_request.base.sha` reads like the right thing —
        it is even called the base — and it is the payload's record of where the branch
        started, which lags the merge commit `actions/checkout` puts on disk. Measured on
        run 33331151759: the checkout merged into `c29f3a8`, the payload said `d40d998`,
        and a branch touching one Python file was swept for three.

        Every value of every `env:` in the file, so a future reader cannot reintroduce it
        under a different variable name or in a different job.
        """
        values = [str(v) for job in self.jobs.values() for step in job["steps"]
                  for v in (step.get("env") or {}).values()]
        self.assertTrue(values)     # the reader found the env blocks at all
        self.assertEqual([v for v in values if "pull_request.base.sha" in v], [])

    def test_the_two_jobs_that_sweep_ask_the_tool_where_the_branch_starts(self):
        """`base_for` resolves the merge-base of the checked-out merge commit with
        `origin/main`, which for a merge IS the parent it was merged into. `fetch-depth: 0`
        on both jobs is what makes `origin/main` an object they have — which is why the
        depth is asserted here beside the flag it exists for."""
        for job, step in (("plan", "How many mutations, and how many jobs they need"),
                          ("sweep", "Sweep the guards this branch adds")):
            self.assertNotIn("--base", self._run(job, step), job)
            depths = [s["with"]["fetch-depth"] for s in self.jobs[job]["steps"]
                      if "checkout@" in s.get("uses", "")]
            self.assertEqual(depths, ["0"], job)

    def test_the_page_names_the_base_the_sweep_actually_charged(self):
        """The merge step runs on its own machine at `fetch-depth: 1`, where neither
        `HEAD^` nor `origin/main` is an object it has — so it cannot work the base out and
        has to be told. Told by the job that resolved it, and not by the payload, or the
        header drifts back to naming a sha nobody charged."""
        self.assertEqual(self.jobs["plan"]["outputs"]["base"],
                         "${{ steps.size.outputs.base }}")
        self.assertEqual(self._collect_step("say")["env"]["BASE_SHA"],
                         "${{ needs.plan.outputs.base }}")

    def test_the_shards_restore_the_map_the_plan_measured(self):
        """Otherwise a second machine costs a whole trace, which is the largest fixed cost
        there is and the reason sharding looked affordable in the first place."""
        keys = [step["with"]["key"] for job in ("plan", "sweep")
                for step in self.jobs[job]["steps"]
                if "cache@" in step.get("uses", "")]
        self.assertEqual(len(keys), 2)
        self.assertEqual(keys[0], keys[1])
        # And keyed on the tree the map was measured on, the way `tree_hash` is. A key
        # that did not move with the sources would hand a shard a map of another tree,
        # which is a selection that certifies guards it never ran.
        self.assertIn("hashFiles('charter/**/*.py', 'tests/**/*.py')", keys[0])
        # And the directory that is cached has to be the one the tool writes into. Two
        # spellings of the same path in two languages is where this silently becomes a
        # cache of an empty directory that always misses and never says so.
        paths = [step["with"]["path"] for job in ("plan", "sweep")
                 for step in self.jobs[job]["steps"] if "cache@" in step.get("uses", "")]
        self.assertEqual(set(paths), {"${{ runner.temp }}/sweep/cache"})
        for job, step in (("plan", "How many mutations, and how many jobs they need"),
                          ("sweep", "Sweep the guards this branch adds")):
            self.assertIn('--workdir "$RUNNER_TEMP/sweep"', self._run(job, step))
        with tempfile.TemporaryDirectory() as tmp:
            here = Path(tmp) / "sweep"
            # `Path.resolve` and not `sweep.resolved`: comparing the tool against its own
            # spelling of a path would agree with whatever that spelling became, and the
            # bug this pins is the two languages disagreeing (#572 was that, on macOS,
            # where `$TMPDIR` goes through `/var -> /private/var`).
            self.assertEqual(sweep.workdir_for(Path(tmp), str(here)), here.resolve())

    def test_nothing_the_sweep_writes_needs_more_than_a_read_only_token(self):
        """The reason the answer is a name and not a `neutral` conclusion: `neutral` needs
        `checks: write`, in the job that runs this repository's own code against mutated
        copies of it."""
        self.assertEqual(self.wf["permissions"], {"contents": "read"})


class TheGateNeverChargesTheWholeTree(unittest.TestCase):
    """The refusal, and — as of the self-sweep — the fact that it comes FIRST.

    This case used to call `main(["--gate", "--all"])` and check stderr, which pins the
    refusal only as long as the refusal is there. Delete it and `main()` does not fail:
    it goes on and sweeps the whole tree, in the test process, for fourteen hours. The
    sweep reported that mutation as *unresolved* rather than pinned — a hang is not a red
    — so the line had no verdict behind it at all.

    `repo_root` is the first thing `main()` reaches for after the argument check, so
    standing a tripwire there turns "the refusal is gone" from a hang into a red in
    milliseconds. That is the same rule the tool applies to itself: a run that wedges has
    not passed.
    """

    def setUp(self):
        original = sweep.repo_root
        self.addCleanup(setattr, sweep, "repo_root", original)
        self.reached = []

        def tripwire(start):
            self.reached.append(start)
            raise RuntimeError("main() got past the argument check and started working")

        sweep.repo_root = tripwire

    def test_gate_and_all_together_are_refused_before_any_work(self):
        """Stage B is a fourteen-hour job and stage C is a pull-request check. Letting the
        two be asked for at once is how the gate becomes the reason nobody runs either."""
        said = io.StringIO()
        with contextlib.redirect_stderr(said), self.assertRaises(SystemExit):
            sweep.main(["--gate", "--all"])
        self.assertIn("never sweeps the whole tree", said.getvalue())
        self.assertEqual(self.reached, [])

    def test_all_on_its_own_is_the_supported_way_to_charge_the_whole_tree(self):
        """The other half of `args.gate and args.all`, and the half that makes it a
        conjunction. Dropping either name from the condition refuses a flag the tool
        documents — `--all` is stage B, and it is how the number in `docs` was measured —
        and nothing in the suite ran `--all` at all."""
        said = io.StringIO()
        with contextlib.redirect_stderr(said), self.assertRaises(RuntimeError):
            sweep.main(["--all"])
        self.assertEqual(said.getvalue(), "")
        self.assertEqual(len(self.reached), 1)


# ======================================================================================
# The CLI's own rules — extracted so they can be swept at all
# ======================================================================================

class ARuleInsideMainCannotBeSwept(unittest.TestCase):
    """#572's structural lesson, applied to the rest of the CLI.

    `workdir_for` had to come out of `main()` before the bug that made this tool unusable
    on macOS could be pinned: a rule inside `main()` is not reachable from a test, so it
    cannot be swept, so it is a guard the harness is structurally unable to hold itself to.
    The self-sweep's "renderer + CLI, ~0 of 49" row is what that hazard looks like at scale.
    """

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="sweep-cli-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp,
                                                            ignore_errors=True))
        env = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull,
                   GIT_CONFIG_SYSTEM=os.devnull, GIT_CONFIG_NOSYSTEM="1",
                   GIT_TERMINAL_PROMPT="0")

        def run(*a):
            subprocess.run(("git", "-c", "core.hooksPath=", "-c", "commit.gpgsign=false")
                           + a, cwd=self.tmp, check=True, env=env, timeout=60,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        self.run = run
        run("init", "-q", "-b", "main")
        run("config", "user.email", "sweep@example.invalid")
        run("config", "user.name", "sweep")
        (self.tmp / "charter").mkdir()
        (self.tmp / "charter" / "m.py").write_text("a = 1\n")
        run("add", "-A")
        run("commit", "-qm", "one")
        self.first = sweep.git("rev-parse", "HEAD", cwd=self.tmp).strip()
        run("checkout", "-q", "-b", "side")
        (self.tmp / "charter" / "m.py").write_text("a = 1\nb = 2\n")
        run("add", "-A")
        run("commit", "-qm", "two")
        self.side = sweep.git("rev-parse", "HEAD", cwd=self.tmp).strip()
        run("checkout", "-q", "main")
        (self.tmp / "charter" / "other.py").write_text("z = 9\n")
        run("add", "-A")
        run("commit", "-qm", "main moved on")
        self.main_tip = sweep.git("rev-parse", "HEAD", cwd=self.tmp).strip()

    def test_a_branch_is_charged_against_the_merge_base_and_not_the_tip(self):
        """A branch is answerable for what IT added, never for what main gained while it
        was open. Charging against the tip would hand every long-lived branch a diff full
        of somebody else's lines."""
        self.assertEqual(sweep.base_for(self.tmp, self.side, None), self.first)

    def test_an_explicit_base_is_resolved_to_a_commit(self):
        self.assertEqual(sweep.base_for(self.tmp, self.side, "main"), self.main_tip)

    def test_uncommitted_work_is_carried_only_when_the_sweep_is_about_the_working_tree(self):
        """`--ref HEAD` means "what I have here". Any other ref names a specific historical
        tree, and pouring today's uncommitted files into it would sweep a tree that has
        never existed."""
        (self.tmp / "charter" / "dirty.py").write_text("q = 1\n")
        self.assertIn("charter/dirty.py", sweep.dirty_for(self.tmp, ("charter",), "HEAD"))
        self.assertEqual(sweep.dirty_for(self.tmp, ("charter",), self.first), {})

    def test_the_full_suite_cap_is_taken_from_this_machines_own_baseline(self):
        """Measured at a load average of 100: a five-minute suite ran past the fixed
        forty-minute cap and two known-unpinned guards came back "pinned" on the strength
        of a stopwatch. Six times the measured run, and never below the floor."""
        self.assertEqual(sweep.timeout_for(1.0), sweep.FULL_TIMEOUT)
        self.assertEqual(sweep.timeout_for(sweep.FULL_TIMEOUT), sweep.FULL_TIMEOUT * 6)

    def test_the_exit_code_separates_a_survivor_from_a_mutation_nobody_measured(self):
        m = sweep.Mutation("charter/a.py", 1, 1, "drop-if", "q?", "if x: pass", "", "f")
        self.assertEqual(sweep.exit_code([sweep.Result(m, "pinned", None, None, [])]), 0)
        self.assertEqual(sweep.exit_code([sweep.Result(m, "survived", None, None, [])]), 1)
        self.assertEqual(sweep.exit_code([sweep.Result(m, "unresolved", None, None, [])]),
                         3)


# ======================================================================================
# The operator table's own guards
# ======================================================================================

class TheOperatorTableHoldsItsOwnEdges(unittest.TestCase):
    """The rows the self-sweep found unpinned in `_iter_operators` and its helpers.

    "The operator table decides verdicts" — an unpinned line here does not mislead a
    reviewer about one guard, it silently changes which questions the tool asks of every
    branch that follows.
    """

    def test_a_statement_that_is_not_alone_in_its_block_is_deleted_and_not_passed(self):
        """`_drop_statement` chooses between deleting and `pass`, and BOTH arms have to be
        held: collapsing it to `"pass"` leaves a mutant that still refuses (a false pin),
        and collapsing it to `""` is caught only by a fallback two functions away."""
        muts = _mutations("""
            def f(x):
                if x:
                    return 1
                return 2
        """)
        self.assertEqual(_afters(muts, "drop-if"), [""])

    def test_a_refusal_inside_an_else_block_is_found_through_the_orelse(self):
        """`_body_of` looks in `body`, `orelse` AND `finalbody`. A version that knew only
        `body` would call this the sole statement of its block and replace it with `pass`
        — a mutant that still refuses, reported as a deletion."""
        muts = _mutations("""
            def f(x, y):
                if x:
                    y = 1
                else:
                    if y:
                        return 3
                    return 4
        """)
        self.assertEqual(_afters(muts, "drop-if"), [""])

    def test_a_refusal_in_a_block_this_tool_cannot_name_is_still_offered(self):
        """`_body_of` returns `None` for a block shape it does not know — a `match` case is
        one — and the `or []` behind it is what stops that being a `TypeError` in the
        middle of somebody's sweep. Without the fallback this file does not even mutate."""
        muts = _mutations("""
            def f(x, y):
                match x:
                    case 1:
                        if y:
                            return 3
        """)
        self.assertEqual(_afters(muts, "drop-if"), ["pass"])

    def test_the_innermost_enclosing_function_is_the_one_reported(self):
        """`_enclosing` picks the SMALLEST span containing the node, and the symbol is what
        the selection map is keyed by. Reporting the outer name sends the mutation to the
        modules that exercise the wrapper and not the ones that exercise the helper."""
        muts = _mutations("""
            def outer(x):
                def inner(y):
                    if y:
                        return 1
                return inner(x)
        """)
        self.assertEqual([m.symbol for m in _by(muts, "drop-if")], ["inner"])

    def test_a_statement_at_module_scope_is_not_attributed_to_a_function(self):
        muts = _mutations("""
            def f(y):
                return y

            if f:
                pass
        """)
        self.assertEqual([m.symbol for m in _by(muts, "drop-if")], ["<module>"])

    def test_an_annotated_module_constant_is_retuned_like_any_other(self):
        muts = _mutations("""
            WIDTH: int = 28
        """)
        self.assertEqual(_afters(muts, "retune-constant"), ["29"])

    def test_a_constant_built_with_anything_but_addition_offers_no_term_to_drop(self):
        """`drop-term` asks "is every term pinned?", which only means something for a sum.
        Dropping a factor of a product is a different and much larger question."""
        muts = _mutations("""
            SPAN = ROWS * COLS
        """)
        self.assertEqual(_by(muts, "drop-term"), [])

    def test_a_get_with_no_default_and_no_fallback_is_left_alone(self):
        """`d.get(k)` on its own is not a fallback this tool knows how to remove — the
        two spellings it does know are `d.get(k) or ()` and `d.get(k, v)`. Mutating the
        bare form would report a question the operator table never asked."""
        muts = _mutations("""
            def f(d, k):
                d.get(k)
                return 1
        """)
        self.assertEqual(_by(muts, "no-fallback"), [])

    def test_an_empty_dict_is_an_empty_literal_like_the_other_three(self):
        muts = _mutations("""
            def f(a):
                return a or {}
        """)
        self.assertEqual(_afters(muts, "no-fallback"), ["a"])

    def test_a_file_that_does_not_parse_offers_nothing_rather_than_raising(self):
        """A sweep runs over whatever the branch contains, including a file mid-edit."""
        self.assertEqual(sweep.mutations_for("charter/x.py", b"def (:\n", {1}), [])

    def test_a_mutation_whose_result_would_not_parse_is_dropped(self):
        """`disable-branch` rewrites a test to `True`/`False`, and the parse check is what
        stops a shape whose replacement is not a legal expression from being offered."""
        muts = _mutations("""
            def f(x):
                if x:
                    return 1
                elif x > 2:
                    return 2
                else:
                    return 3
        """)
        for m in muts:
            ast.parse(m.source)

    def test_an_elif_is_never_offered_for_deletion_because_its_span_is_not_a_statement(self):
        """The `elif` keyword is part of the enclosing `if`'s source, so an `elif` node's
        span does not re-parse into itself. `span_is_sound` refuses it and the branch is
        asked about with `disable-branch` instead — which is the same question, spelled so
        that the mutant is still a legal file."""
        muts = _mutations("""
            def f(x):
                if x == 1:
                    return 1
                elif x == 2:
                    return 2
                else:
                    return 3
        """)
        self.assertEqual([m.line for m in _by(muts, "drop-if")], [])
        self.assertIn(4, [m.line for m in _by(muts, "disable-branch")])

    def test_the_same_edit_offered_twice_is_reported_once(self):
        """Several rows of the table recognise the same node, and a mutation listed twice
        is a run paid for twice and a survivor a reviewer reads as two findings."""
        muts = _mutations("""
            def f(a, b):
                if isinstance(a, str) and b:
                    return 1
        """)
        keys = [(m.line, m.operator, m.after) for m in muts]
        self.assertEqual(len(keys), len(set(keys)))

    def test_a_replacement_identical_to_what_is_there_is_not_a_mutation(self):
        muts = _mutations("""
            def f(a):
                try:
                    return a
                except ZeroDivisionError:
                    return None
        """)
        self.assertEqual(_by(muts, "narrow-except"), [])

    def test_a_splice_that_adds_lines_does_not_lose_the_ones_below_it(self):
        """`max(0, lost)` in `_Spans.splice`: a replacement with MORE newlines than the
        span it replaces has nothing to pad, and padding by a negative count would be a
        `bytes * -1` — silently the empty string, and every line below renumbered."""
        source = b"a = 1\nb = (2)\nc = 3\n"
        sp = sweep._Spans(source)
        tree = ast.parse(source)
        node = tree.body[1].value
        spliced = sp.splice(node, "(\n4\n)")
        self.assertEqual(spliced.decode().splitlines()[-1], "c = 3")
        self.assertEqual(ast.parse(spliced).body[-1].lineno, 5)

    def test_two_mutations_that_add_lines_still_compose(self):
        source = b"def f(a, b):\n    x = (a)\n    y = (b)\n    return x, y\n"
        sp = sweep._Spans(source)
        tree = ast.parse(source)
        first, second = tree.body[0].body[0].value, tree.body[0].body[1].value
        pair = (sweep.Mutation("m.py", 2, 2, "t", "q", "(a)", "(\na\n)", "f",
                               span=sp.span(first)),
                sweep.Mutation("m.py", 3, 3, "t", "q", "(b)", "(\nb\n)", "f",
                               span=sp.span(second)))
        out = sweep.compose(source, pair)
        self.assertIsNotNone(out)
        self.assertIn("return x, y", out.decode())

    def test_a_composed_pair_that_would_not_parse_is_refused(self):
        source = b"x = (1)\n"
        sp = sweep._Spans(source)
        node = ast.parse(source).body[0].value
        pair = (sweep.Mutation("m.py", 1, 1, "t", "q", "(1)", "(", "f",
                               span=sp.span(node)),
                sweep.Mutation("m.py", 1, 1, "t", "q", "(1)", "(", "f", span=(0, 0)))
        self.assertIsNone(sweep.compose(source, pair))

    def test_a_test_module_that_does_not_parse_is_skipped_by_the_evidence_pass(self):
        tmp = Path(tempfile.mkdtemp(prefix="sweep-ev-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(tmp, ignore_errors=True))
        (tmp / "tests").mkdir()
        (tmp / "tests" / "test_broken.py").write_text("def test_window(:\n")
        m = sweep.Mutation("charter/a.py", 1, 1, "drop-if", "q", "if x: pass", "",
                           "_window")
        ev = sweep.evidence_for(tmp, m, ["tests.test_broken"])
        self.assertEqual(ev.naming, [])

    def test_a_shortened_line_is_marked_and_a_short_one_is_left_alone(self):
        self.assertEqual(sweep._oneline("abc def"), "abc def")
        self.assertTrue(sweep._oneline("x" * 200).endswith("…"))
        self.assertEqual(len(sweep._oneline("x" * 200)), 96)


class TheScopeReaderHoldsItsOwnEdges(unittest.TestCase):
    """`git`, `_blob_at` and `added_lines` — the rows the self-sweep found unpinned."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="sweep-scope-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp,
                                                            ignore_errors=True))
        env = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull,
                   GIT_CONFIG_SYSTEM=os.devnull, GIT_CONFIG_NOSYSTEM="1",
                   GIT_TERMINAL_PROMPT="0")

        def run(*a):
            subprocess.run(("git", "-c", "core.hooksPath=", "-c", "commit.gpgsign=false")
                           + a, cwd=self.tmp, check=True, env=env, timeout=60,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        run("init", "-q", "-b", "main")
        run("config", "user.email", "sweep@example.invalid")
        run("config", "user.name", "sweep")
        (self.tmp / "charter").mkdir()
        (self.tmp / "charter" / "m.py").write_text("a = 1\n")
        (self.tmp / "charter" / "gone.py").write_text("b = 2\n")
        (self.tmp / "charter" / "notes.txt").write_text("hello\n")
        run("add", "-A")
        run("commit", "-qm", "one")
        self.base = sweep.git("rev-parse", "HEAD", cwd=self.tmp).strip()
        (self.tmp / "charter" / "gone.py").unlink()
        (self.tmp / "charter" / "m.py").write_text("a = 1\nc = 3\n")
        (self.tmp / "charter" / "notes.txt").write_text("hello\nagain\n")
        run("add", "-A")
        run("commit", "-qm", "two")
        self.head = sweep.git("rev-parse", "HEAD", cwd=self.tmp).strip()

    def test_a_failing_git_command_raises_with_what_git_said(self):
        """The `check` half of `git()`. Without it a broken invocation returns empty
        output, and a sweep quietly charges an empty diff — a green report for a question
        never asked."""
        with self.assertRaises(RuntimeError) as caught:
            sweep.git("rev-parse", "--verify", "no/such/ref", cwd=self.tmp)
        self.assertIn("no/such/ref", str(caught.exception))

    def test_the_same_command_can_be_asked_without_raising(self):
        self.assertEqual(
            sweep.git("rev-parse", "--verify", "--quiet", "no/such/ref",
                      cwd=self.tmp, check=False).strip(), "")

    def test_a_blob_that_is_not_there_is_empty_bytes_and_not_a_crash(self):
        self.assertEqual(sweep._blob_at(self.tmp, self.base, "charter/nope.py"), b"")
        self.assertEqual(sweep._blob_at(self.tmp, self.base, "charter/m.py"), b"a = 1\n")

    def test_a_deleted_file_is_not_charged_to_the_branch(self):
        """A deletion's hunk header names `/dev/null` on the `+` side. Read as a path, it
        would put the deleted file's line numbers into the scope and every mutation of it
        would come back with no source at all."""
        found = sweep.added_lines(self.tmp, self.base, self.head, ("charter",))
        self.assertEqual(found, {"charter/m.py": {2}})

    def test_a_file_that_is_not_python_is_not_charged_either(self):
        found = sweep.added_lines(self.tmp, self.base, self.head, ("charter",))
        self.assertNotIn("charter/notes.txt", found)

    def test_a_path_that_is_not_in_the_tree_charges_nothing_rather_than_raising(self):
        """`all_lines` has no `if base.exists()` — the sweep deleted it and the suite
        stayed green, because `rglob` on a missing directory yields nothing anyway. §4:
        an equivalent mutant and a dead line are one finding, so it went."""
        self.assertEqual(sweep.all_lines(self.tmp, ("nowhere",)), {})
        self.assertEqual(sweep.tree_hash(self.tmp, ("nowhere",)),
                         sweep.tree_hash(self.tmp, ("nowhere",)))

    def test_uncommitted_work_under_the_swept_paths_is_collected(self):
        (self.tmp / "charter" / "fresh.py").write_text("d = 4\n")
        (self.tmp / "charter" / "fresh.txt").write_text("not python\n")
        dirty = sweep.dirty_files(self.tmp, ("charter",))
        self.assertEqual(set(dirty), {"charter/fresh.py"})


# ======================================================================================
# The report renderer's own guards — #569's thin row, measured again
# ======================================================================================
#
# Stage A's self-sweep put "report renderer + CLI" at roughly 0 of 49 pinned, and the
# reason it was allowed to stay thin was that the renderer decides no verdict. #630 took
# that argument away: the gate's answer is now carried by the check's NAME and by the page
# a reviewer reads, so a renderer that prints the wrong count is a gate that says the wrong
# thing. Re-measured on this branch over the 274 mutations from `report()` to the end of
# the file: 99 survived. The cases below are the ones whose survival changes what an
# operator is told, and `docs/news` says plainly what was left.

def _outcome(green=True, ran=10, detail="OK"):
    return sweep.Outcome(green, ran, detail)


def _survivor(line=291, symbol="_placed_here", evidence=None, full=None, subset=None,
              operator="drop-conjunct", before="isinstance(n, str) and n not in SLOTS"):
    m = sweep.Mutation(path="charter/frame/layout.py", line=line, end_line=line,
                       operator=operator, question="is the half pinned?", before=before,
                       after="isinstance(n, str)", symbol=symbol)
    return sweep.Result(m, "survived", subset, full, ["tests.test_a"], evidence)


class TheReportCountsWhatItFound(unittest.TestCase):
    """The header of the terminal report: five numbers, and nothing asserted them.

    Every one is computed by a comprehension filtering on `r.verdict`, and forcing any of
    those filters to `True` — which is what `drop-comprehension-if` does — left the whole
    suite green. A report that calls four pinned mutations four survivors is worse than no
    report: it is the tool making the finding it exists to find.
    """

    def _mixed(self):
        return [_result("pinned"), _result("pinned"), _result("survived", line=1),
                _result("survived", line=9), _result("survived", line=17),
                _result("unresolved"), _result("unapplied")]

    def test_each_count_in_the_header_is_its_own_verdict(self):
        text = sweep.report(self._mixed(), Path("."), "a" * 12, "b" * 12, None, 60.0)
        self.assertIn("mutations applied : 7", text)
        self.assertIn("pinned            : 2", text)
        self.assertIn("SURVIVED          : 3", text)
        self.assertIn("UNRESOLVED        : 1", text)
        self.assertIn("NOT APPLIED       : 1", text)

    def test_a_sweep_with_nothing_unapplied_does_not_print_that_row_at_all(self):
        """The row is the loudest thing the report can say — "every number above is
        suspect" — so it appears only when it is true."""
        text = sweep.report([_result("pinned")], Path("."), "a" * 12, "b" * 12, None, 1.0)
        self.assertNotIn("NOT APPLIED", text)

    def test_the_baseline_line_carries_ok_or_carries_what_went_wrong(self):
        """A tree that was red before any mutation makes every mutation look pinned, so
        the baseline is the first thing the reader has to be able to check. Both arms
        matter: collapsing the conditional to `'OK'` reports a red baseline as a green
        one, which is the sweep's second way of lying with a straight face."""
        green = sweep.report([], Path("."), "a" * 12, "b" * 12,
                             _outcome(True, 7909, "OK"), 1.0)
        self.assertIn("baseline          : Ran 7909 tests — OK", green)
        red = sweep.report([], Path("."), "a" * 12, "b" * 12,
                           _outcome(False, 7909, "tests.test_a.T.test_x"), 1.0)
        self.assertIn("baseline          : Ran 7909 tests — tests.test_a.T.test_x", red)
        # `--no-baseline` measured nothing, and silence is the honest rendering of that.
        self.assertNotIn("baseline  ", sweep.report([], Path("."), "a" * 12, "b" * 12,
                                                    None, 1.0))

    def test_the_wall_clock_is_minutes_to_one_decimal(self):
        """1236 s is 20.6 minutes and `.2g` calls it 21. A run that took twenty minutes
        forty and reports "21 min" is a number nobody can check against a job's own
        clock."""
        text = sweep.report([], Path("."), "a" * 12, "b" * 12, None, 1236.0)
        self.assertIn("wall clock        : 20.6 min", text)

    def test_a_shape_this_interpreter_cannot_reach_is_named_on_both_pages(self):
        """`reach()` is empty on 3.12 and later, so on a modern interpreter both of these
        lines are dead and deleting either changed nothing — which is exactly how the
        sweep reported them. The claim is not about this machine's version: it is that
        when there IS something out of reach, both pages say so. A sweep that asks fewer
        questions and does not mention it reads precisely like a clean one.
        """
        original = sweep.reach
        sweep.reach = lambda: "f-string literals"
        self.addCleanup(setattr, sweep, "reach", original)
        text = sweep.report([], Path("."), "a" * 12, "b" * 12, None, 1.0)
        self.assertIn("NOT ASKED ABOUT  : f-string literals", text)
        page = sweep.gate_summary(sweep.classify([_result("pinned")]),
                                  "a" * 40, "b" * 40, 60.0, enforce=False)
        self.assertIn("| not asked about | — | f-string literals |", page)
        sweep.reach = lambda: ""
        self.assertNotIn("NOT ASKED ABOUT",
                         sweep.report([], Path("."), "a" * 12, "b" * 12, None, 1.0))
        self.assertNotIn("not asked about", sweep.gate_summary(
            sweep.classify([_result("pinned")]), "a" * 40, "b" * 40, 60.0, enforce=False))

    def test_a_survivor_carries_the_full_run_that_measured_it(self):
        """"Survived" means the WHOLE suite stayed green, and the count of tests that ran
        is how a reader tells that from a subset that never covered the file."""
        text = sweep.report([_survivor(full=_outcome(True, 7909, "OK"))],
                            Path("."), "a" * 12, "b" * 12, None, 1.0)
        self.assertIn("full    : Ran 7909 tests — OK, with the line gone", text)


class TheReportTellsThreeKindsOfSilenceApart(unittest.TestCase):
    """The `covered:` field — the one that made 82 survivors triageable.

    Its three shapes are three different claims and the suite asserted none of them, so
    forcing either branch of the chain in either direction left everything green. "Nothing
    measured executes this file", "eleven modules execute it and not one names the
    symbol", and "two tests name it, here is what they assert" are the difference between
    a survivor a reviewer can act on and a line number.
    """

    def _rendered(self, evidence):
        return sweep.report([_survivor(evidence=evidence)], Path("."), "a" * 12,
                            "b" * 12, None, 1.0)

    def test_no_evidence_at_all_prints_no_claim_about_coverage(self):
        """`evidence=None` is the shard-merge shape (#617): the evidence pass never ran,
        which is not the same as running and finding nothing. Dropping the guard that
        says so reaches for `.modules` on `None`."""
        text = self._rendered(None)
        self.assertIn("charter/frame/layout.py:291", text)
        self.assertNotIn("covered :", text)

    def test_nothing_executing_the_file_is_said_in_those_words(self):
        self.assertIn("covered : nothing measured executes this file at all",
                      self._rendered(sweep.Evidence([], [])))

    def test_modules_that_execute_it_and_never_name_it_are_counted(self):
        text = self._rendered(sweep.Evidence(["tests.test_a", "tests.test_b"], []))
        self.assertIn("covered : 2 module(s) execute this file and NOT ONE", text)
        self.assertIn("names `_placed_here` — tests.test_a, tests.test_b", text)

    def test_tests_that_do_name_it_arrive_with_their_assertions(self):
        text = self._rendered(sweep.Evidence(
            ["tests.test_a"],
            [("tests.test_a", "test_window", ["assertEqual(_window(8), (0, 6))"])]))
        self.assertIn("covered : 1 test(s) name `_placed_here`; what they assert:", text)
        self.assertIn("test_a.test_window", text)
        self.assertIn("assertEqual(_window(8), (0, 6))", text)

    def test_a_list_longer_than_three_says_how_many_it_did_not_show(self):
        """Silent truncation, in the tool that exists to refuse it. Three are printed and
        the rest were simply absent — and "three tests name this" reads as the whole
        answer. The boundary is asserted at three and at four, because `> 3` and `>= 3`
        both passed while the only case was a list of one."""
        naming = [("tests.test_a", f"test_{n}", ["assertTrue(x)"]) for n in range(5)]
        text = self._rendered(sweep.Evidence(["tests.test_a"], naming))
        self.assertIn("… and 2 more", text)
        exactly_three = self._rendered(sweep.Evidence(["tests.test_a"], naming[:3]))
        self.assertNotIn("… and", exactly_three)


class TheArtifactAShardWritesIsReadBackWhole(unittest.TestCase):
    """`as_json` and `results_from_json` are one contract across two machines (#617).

    Everything `classify` and `gate_summary` read has to survive the trip, not merely the
    verdict string — and the outcomes did not: re-spelling `"green"`, `"ran"` or
    `"detail"` on the writing side, or collapsing either outcome to `null`, left the suite
    green because nothing ever round-tripped one.
    """

    def test_every_field_the_merge_reads_survives_the_file(self):
        written = sweep.as_json([_survivor(
            subset=_outcome(True, 12, "OK"),
            full=_outcome(False, 7909, "tests.test_a.T.test_x"),
            evidence=sweep.Evidence(["tests.test_a"],
                                    [("tests.test_a", "test_x", ["assertTrue(x)"])]))])
        row = json.loads(written)[0]
        self.assertEqual(row["subset"], {"green": True, "ran": 12, "detail": "OK"})
        self.assertEqual(row["full"], {"green": False, "ran": 7909,
                                       "detail": "tests.test_a.T.test_x"})
        back = sweep.results_from_json(written)[0]
        self.assertEqual((back.subset.green, back.subset.ran, back.subset.detail),
                         (True, 12, "OK"))
        self.assertEqual((back.full.green, back.full.ran, back.full.detail),
                         (False, 7909, "tests.test_a.T.test_x"))

    def test_an_outcome_that_was_never_measured_arrives_as_nothing(self):
        row = json.loads(sweep.as_json([_survivor()]))[0]
        self.assertIsNone(row["subset"])
        self.assertIsNone(row["full"])
        self.assertIsNone(sweep.results_from_json(sweep.as_json([_survivor()]))[0].full)

    def test_the_row_carries_the_platform_it_was_measured_on(self):
        """Named keys and not a shape: the merge recomputes `platform_caveat` from the
        operator rather than trusting the file, and the field is still written so that a
        person reading the artifact can see which machine said what."""
        row = json.loads(sweep.as_json([_survivor(operator="narrow-except",
                                                  before="OSError")]))[0]
        self.assertEqual(row["platform"], sys.platform)
        self.assertEqual(row["platform_caveat"], "OSError")


class TheGatePageSaysWhatItWouldDoAndWhatItCouldNotSee(unittest.TestCase):
    def test_the_page_says_whether_this_branch_would_fail_under_enforce(self):
        """The one sentence on a reporting-only page that tells a reviewer whether the
        gate is about to start blocking them. Both arms survived: the page could have said
        "would fail" on every branch, or "would pass" on every branch, and nothing in the
        suite would have noticed."""
        survivors = sweep.gate_summary(sweep.classify([_result("survived")]),
                                       "a" * 40, "b" * 40, 60.0, enforce=False)
        self.assertIn("it **would fail** with `--enforce`", survivors)
        clean = sweep.gate_summary(sweep.classify([_result("pinned")]),
                                   "a" * 40, "b" * 40, 60.0, enforce=False)
        self.assertIn("it **would pass** with `--enforce`", clean)

    def test_a_lost_shard_and_a_sweep_that_never_sized_itself_read_differently(self):
        """#617's own distinction, in the paragraph that explains the row. "1 of 3 did not
        report" is a number; "the plan job died before it sized anything" is the absence
        of one, and rendering the second as `1 of 0` would describe a sweep that was never
        planned as a sweep that was planned and lost."""
        gate = sweep.classify([_result("pinned")])
        many = sweep.gate_summary(gate, "a" * 40, "b" * 40, None, False,
                                  missing=1, shards=3)
        self.assertIn("1 of 3 shard(s) wrote no result", many)
        # `shards >= 1` and not `> 1`: the one-shard plan whose only shard vanished is the
        # ordinary shape for a small diff, and it is the case the boundary turns on.
        lone = sweep.gate_summary(gate, "a" * 40, "b" * 40, None, False,
                                  missing=1, shards=1)
        self.assertIn("1 of 1 shard(s) wrote no result", lone)
        unsized = sweep.gate_summary(gate, "a" * 40, "b" * 40, None, False,
                                     missing=1, shards=0)
        self.assertIn("never said how many shards it needed", unsized)
        self.assertNotIn("wrote no result", unsized)


class TheMergeStepHoldsItsOwnEdges(unittest.TestCase):
    """`merge`, `_merge_step` and `_append` — the last thing that runs on a sharded gate.

    Nothing in the suite ran `main --verdict` at all, so every rule inside `_merge_step`
    was a rule no mutation could be caught by. That is #572's structural lesson again: a
    rule reachable only from a `main()` nobody calls is a guard the harness cannot hold
    itself to.
    """

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="sweep-merge-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp,
                                                            ignore_errors=True))
        self.shards = self.tmp / "shards"
        self.shards.mkdir()

    def _shard(self, name, results):
        (self.shards / name).write_text(sweep.as_json(results), encoding="utf-8")

    def test_more_answers_than_the_plan_asked_for_is_not_a_negative_absence(self):
        """A count of shards that did not report can only be zero or more. Without the
        clamp, four answers against a plan of three make `missing` negative — which is
        truthy, so the page would announce "-1 of 3 did not report" and refuse to call a
        complete sweep complete."""
        for n in (1, 2, 3, 4):
            self._shard(f"s{n}.json", [_result("pinned")])
        results, missing = sweep.merge(self.shards, 3)
        self.assertEqual((len(results), missing), (4, 0))

    def test_a_file_that_will_not_parse_is_a_shard_that_did_not_report(self):
        """There is no third reading of a truncated upload."""
        self._shard("good.json", [_result("pinned")])
        (self.shards / "torn.json").write_text("[{\"path\":", encoding="utf-8")
        results, missing = sweep.merge(self.shards, 2)
        self.assertEqual((len(results), missing), (1, 1))

    def test_the_merge_step_says_how_many_of_how_many_answered(self):
        self._shard("s1.json", [_result("survived")])
        out = self.tmp / "out.txt"
        said = io.StringIO()
        with contextlib.redirect_stdout(said):
            code = sweep.main(["--verdict", str(self.shards), "--shards", "3",
                               "--gate", "--github-output", str(out)])
        self.assertEqual(code, 0)
        self.assertIn("merged 1 result(s) from 1 of 3 shard(s)", said.getvalue())
        self.assertIn("gate: reporting only — nothing here blocks.", said.getvalue())
        self.assertIn("conclusion=no-verdict", out.read_text())
        self.assertIn("headline=no verdict: 1 survivor so far, 2 of 3 shards did not "
                      "report", out.read_text())

    def test_a_sweep_that_never_sized_itself_is_missing_a_shard_it_cannot_count(self):
        """An empty `--shards` is the plan job failing before it sized anything, and that
        has to travel as *no denominator* rather than as `1 of 1`. Without the guard the
        loudest failure the workflow has arrives as the quietest kind of pass."""
        self._shard("s1.json", [_result("pinned")])
        said = io.StringIO()
        with contextlib.redirect_stdout(said):
            sweep.main(["--verdict", str(self.shards), "--shards", "", "--gate"])
        self.assertIn("merged 1 result(s) from an unknown number of shard(s)",
                      said.getvalue())
        self.assertIn("gate: no verdict: the sweep never sized itself", said.getvalue())

    def test_appending_to_the_summary_adds_to_it_and_ends_it_once(self):
        """`$GITHUB_STEP_SUMMARY` is shared by every step of a job, so this appends. The
        trailing newline is normalised from the RIGHT: stripping the other end leaves the
        blank lines a markdown block ends with and pushes the next step's heading down
        the page."""
        page = self.tmp / "summary.md"
        sweep._append(str(page), "## one\n\n")
        sweep._append(str(page), "## two")
        self.assertEqual(page.read_text(), "## one\n## two\n")


class TheCLIsOwnArithmeticIsReachable(unittest.TestCase):
    """The rules `main()` used to hold in-line, pulled out so a mutation can reach them."""

    def test_the_exit_code_reads_every_result_and_not_only_one(self):
        """`any` against `all`: every existing case passed a single-element list, where
        the two are the same function. A real sweep is one survivor among forty pins."""
        pinned, survived = _result("pinned"), _result("survived")
        unresolved, unapplied = _result("unresolved"), _result("unapplied")
        self.assertEqual(sweep.exit_code([pinned, survived]), 1)
        self.assertEqual(sweep.exit_code([pinned, unresolved]), 3)
        self.assertEqual(sweep.exit_code([pinned, survived, unapplied]), 4)
        self.assertEqual(sweep.exit_code([pinned, pinned]), 0)

    def test_the_default_job_count_is_half_the_machine_and_never_zero(self):
        """`(os.cpu_count() or 4) // 2` is zero on a one-core runner, and zero sandboxes
        is a sweep that measures nothing while reporting that it ran. The clamp was
        written inside `main()`'s argument list, where no mutation could reach it."""
        original = os.cpu_count
        self.addCleanup(setattr, os, "cpu_count", original)
        os.cpu_count = lambda: 1
        self.assertEqual(sweep.default_jobs(), 1)
        os.cpu_count = lambda: 8
        self.assertEqual(sweep.default_jobs(), 4)
        # `os.cpu_count()` is documented to be able to return None.
        os.cpu_count = lambda: None
        self.assertEqual(sweep.default_jobs(), 2)

    def test_a_shard_argument_is_read_exactly_or_refused(self):
        """A shard argument that parses loosely does not fail: it sweeps one slice twice
        and another never, and the merge reports a complete sweep of an incomplete plan.
        Both ends of `1 <= index <= count` are asserted, because shard 1 of 3 and shard 3
        of 3 are the two the boundary drops."""
        self.assertEqual(sweep.parse_shard("1/3"), (1, 3))
        self.assertEqual(sweep.parse_shard("3/3"), (3, 3))
        self.assertEqual(sweep.parse_shard("2/3"), (2, 3))
        # Whitespace either side of either number — a workflow's `$SHARD/$SHARDS` is a
        # shell expansion, and both ends of both numbers get the same treatment.
        self.assertEqual(sweep.parse_shard(" 2 / 3 "), (2, 3))
        for bad in ("2", "2/", "/2", "0/3", "4/3", "a/3", "2/b", ""):
            with self.assertRaises(ValueError, msg=bad):
                sweep.parse_shard(bad)
        # `+2` is what the refusal itself catches and nothing else does: `int()` accepts a
        # signed number happily, so without the `isdigit()` check `"+2/3"` parses to shard
        # 2 of 3 and the guard is a line every remaining case would redden anyway.
        for signed in ("+2/3", "2/+3"):
            with self.assertRaises(ValueError, msg=signed):
                sweep.parse_shard(signed)


class TheBranchIsChargedAgainstItsUpstream(unittest.TestCase):
    """`base_for`'s two spellings of upstream, told apart by a repo that has each."""

    def _repo(self, name):
        tmp = Path(tempfile.mkdtemp(prefix=f"sweep-base-{name}-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(tmp, ignore_errors=True))
        env = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull,
                   GIT_CONFIG_SYSTEM=os.devnull, GIT_CONFIG_NOSYSTEM="1",
                   GIT_TERMINAL_PROMPT="0")

        def run(*a):
            subprocess.run(("git", "-c", "core.hooksPath=", "-c", "commit.gpgsign=false")
                           + a, cwd=tmp, check=True, env=env, timeout=60,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        run("init", "-q", "-b", "main")
        run("config", "user.email", "sweep@example.invalid")
        run("config", "user.name", "sweep")
        (tmp / "charter").mkdir()
        (tmp / "charter" / "m.py").write_text("a = 1\n")
        run("add", "-A")
        run("commit", "-qm", "one")
        return tmp, run

    def test_with_no_remote_the_local_main_is_the_upstream(self):
        tmp, run = self._repo("local")
        first = sweep.git("rev-parse", "HEAD", cwd=tmp).strip()
        run("checkout", "-q", "-b", "side")
        (tmp / "charter" / "m.py").write_text("a = 1\nb = 2\n")
        run("add", "-A")
        run("commit", "-qm", "two")
        side = sweep.git("rev-parse", "HEAD", cwd=tmp).strip()
        self.assertEqual(sweep.base_for(tmp, side, None), first)

    def test_when_origin_main_exists_it_is_the_one_that_counts(self):
        """A fresh clone, a linked worktree and CI disagree about which of the two names
        is there, and picking the wrong one does not fail — it is a different merge-base,
        so the branch is charged for lines it did not add.

        The two are made to disagree on purpose: `origin/main` is left at the first commit
        while local `main` moves on, and the branch is cut from the LATER one. Reading
        `main` gives the second commit and reading `origin/main` gives the first, so the
        answer says which name was consulted rather than passing either way.
        """
        tmp, run = self._repo("remote")
        first = sweep.git("rev-parse", "HEAD", cwd=tmp).strip()
        run("update-ref", "refs/remotes/origin/main", first)
        (tmp / "charter" / "other.py").write_text("z = 9\n")
        run("add", "-A")
        run("commit", "-qm", "main moved on, the remote did not")
        second = sweep.git("rev-parse", "HEAD", cwd=tmp).strip()
        run("checkout", "-q", "-b", "side")
        (tmp / "charter" / "m.py").write_text("a = 1\nb = 2\n")
        run("add", "-A")
        run("commit", "-qm", "two")
        side = sweep.git("rev-parse", "HEAD", cwd=tmp).strip()
        self.assertNotEqual(first, second)
        self.assertEqual(sweep.base_for(tmp, side, None), first)


class ABranchIsNotChargedForWhatMainGainedWhileItWasOpen(unittest.TestCase):
    """#776, built as the tree CI actually checks out rather than as a diff of two tips.

    `actions/checkout` puts `refs/pull/N/merge` on disk: a merge commit whose FIRST parent
    is the base branch tip it was merged into and whose second is the branch head. The
    workflow used to charge that tree against `github.event.pull_request.base.sha`, which
    is the payload's idea of where the branch started and lags the merge GitHub computed.
    Measured on run 33331151759's own log:

        HEAD is now at 58411a9 Merge 04bf8e6 into c29f3a8
        diff against d40d998e06bd: 3 file(s), 458 added line(s)

    `d40d998` is three main commits and three hours behind `c29f3a8`, so a branch touching
    exactly one Python file was charged for three. A survivor in a file its author has
    never opened destroys the one premise the gate runs on — that a survivor is YOUR
    untested line — and it compounds: the plan grows, so the shard count grows, so #773's
    runaway mutations are spread across more shards.

    The tool's own default was already right and is what the workflow now uses. The
    property below is why: **nothing later than a merge's first parent is reachable from
    the merge**, so the merge-base of the merge with `origin/main` IS that first parent,
    exactly, however far `main` has moved since.
    """

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="sweep-merge-ref-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.tmp, ignore_errors=True))
        env = dict(os.environ, GIT_CONFIG_GLOBAL=os.devnull,
                   GIT_CONFIG_SYSTEM=os.devnull, GIT_CONFIG_NOSYSTEM="1",
                   GIT_TERMINAL_PROMPT="0")

        def run(*a):
            subprocess.run(("git", "-c", "core.hooksPath=", "-c", "commit.gpgsign=false")
                           + a, cwd=self.tmp, check=True, env=env, timeout=60,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        def sha(ref="HEAD"):
            return sweep.git("rev-parse", ref, cwd=self.tmp).strip()

        run("init", "-q", "-b", "main")
        run("config", "user.email", "sweep@example.invalid")
        run("config", "user.name", "sweep")
        (self.tmp / "charter").mkdir()
        (self.tmp / "charter" / "shared.py").write_text("a = 1\n")
        run("add", "-A")
        run("commit", "-qm", "where the branch was cut from")
        #: What the pull request payload says the base is, recorded when the branch opened.
        self.payload_base = sha()

        run("checkout", "-q", "-b", "side")
        (self.tmp / "charter" / "mine.py").write_text(
            "def close(arm, pane):\n"
            "    if arm is None:\n"
            "        return []\n"
            "    return [arm, pane]\n")
        run("add", "-A")
        run("commit", "-qm", "the branch's own file")
        head = sha()

        # `main` moves on twice while the pull request sits open. Nothing forces a rebase:
        # `required_status_checks.strict` is false on this repository, so a branch may
        # merge while behind, which is the mechanism by which the staleness accumulates.
        run("checkout", "-q", "main")
        (self.tmp / "charter" / "theirs.py").write_text(
            "def other(y):\n"
            "    if y < 3:\n"
            "        return y - 1\n"
            "    return y\n")
        run("add", "-A")
        run("commit", "-qm", "main gains a file")
        (self.tmp / "charter" / "theirs_too.py").write_text(
            "def more(z):\n"
            "    while z > 0:\n"
            "        z -= 1\n"
            "    return z\n")
        run("add", "-A")
        run("commit", "-qm", "main gains another")
        self.main_tip = sha()

        # And GitHub recomputes `refs/pull/N/merge` against the tip it has now.
        run("checkout", "-q", "--detach", self.main_tip)
        run("merge", "-q", "--no-ff", "-m", f"Merge {head} into {self.main_tip}", head)
        self.merge = sha()
        run("update-ref", "refs/remotes/origin/main", self.main_tip)

    def _charged(self, base):
        return sweep.added_lines(self.tmp, base, self.merge, ("charter",))

    def test_the_merge_base_of_a_merge_commit_is_the_parent_it_was_merged_into(self):
        """The property the whole fix rests on, asserted rather than assumed. If this ever
        stops holding, the default silently starts charging a branch for main again — and
        it would not fail, it would report."""
        self.assertEqual(sweep.base_for(self.tmp, self.merge, None), self.main_tip)
        self.assertEqual(sweep.git("rev-parse", f"{self.merge}^1",
                                   cwd=self.tmp).strip(), self.main_tip)
        self.assertNotEqual(self.main_tip, self.payload_base)

    def test_the_default_charges_the_branch_for_its_own_file_and_no_other(self):
        charged = self._charged(sweep.base_for(self.tmp, self.merge, None))
        self.assertEqual(sorted(charged), ["charter/mine.py"])

    def test_the_payloads_base_charges_the_branch_for_mains_files_too(self):
        """The defect, kept as a measurement rather than described in a comment. These are
        real files, added by real commits on `main`, and every mutation in them would have
        arrived on this branch's check as this branch's finding."""
        charged = self._charged(self.payload_base)
        self.assertEqual(sorted(charged),
                         ["charter/mine.py", "charter/theirs.py", "charter/theirs_too.py"])

    def test_the_over_charge_is_a_longer_plan_and_therefore_more_shards(self):
        """Not merely noise. A longer plan is more shards, and more shards spread #773's
        runaway mutations over more of them — three of six shards died on the over-charged
        plan where the same branch on a correct one lost one of five."""
        def plan(base):
            return sweep.plan_for(self.tmp, self.merge, self._charged(base), {})[0]

        honest = plan(sweep.base_for(self.tmp, self.merge, None))
        inflated = plan(self.payload_base)
        self.assertGreater(len(inflated), len(honest))
        self.assertEqual({m.path for m in honest}, {"charter/mine.py"})
        self.assertIn("charter/theirs.py", {m.path for m in inflated})


if __name__ == "__main__":      # pragma: no cover
    unittest.main()
