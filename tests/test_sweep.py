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
import os
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
                         ["isinstance(name, str)", "name not in SLOT_SIZE"])

    def test_a_three_part_guard_drops_one_part_at_a_time_and_keeps_the_rest(self):
        muts = _mutations("""
            def f(a, b, c):
                if a and b and c:
                    return 1
        """)
        self.assertEqual(_afters(muts, "drop-conjunct"),
                         ["a and b", "a and c", "b and c"])

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
        self.assertEqual(_afters(muts, "unclamp"), ["1", "height - _CHROME_ROWS"])

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

    def test_a_bytes_constant_is_retuned_as_bytes(self):
        muts = _mutations("""
            def f(ch):
                return ch == b"\\x03q"
        """)
        self.assertEqual(_afters(muts, "retune-string"), ["b'\\x03r'"])


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
        self.assertEqual(_afters(muts, "shift-boundary"), ["(sel) <= (top)"])

    def test_an_inclusive_comparison_is_offered_one_notch_narrower(self):
        muts = _mutations("""
            def f(n, cap):
                return n <= cap
        """)
        self.assertEqual(_afters(muts, "shift-boundary"), ["(n) < (cap)"])

    def test_a_chain_moves_one_link_and_respells_the_rest(self):
        """`0 <= i < n` is ONE node. Moving one link means writing the whole chain back
        out, and a link this tool could not spell would vanish from the mutant — an edit
        that is not the edit the report describes."""
        muts = _mutations("""
            def f(i, n):
                return 0 <= i < n
        """)
        self.assertEqual(sorted(_afters(muts, "shift-boundary")),
                         ["(0) < (i) < (n)", "(0) <= (i) <= (n)"])

    def test_a_link_that_is_not_a_boundary_is_carried_through_untouched(self):
        muts = _mutations("""
            def f(a, b, c):
                return a < b == c
        """)
        self.assertEqual(_afters(muts, "shift-boundary"), ["(a) <= (b) == (c)"])

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
        instance of anything. Measured on `charter/`: eight such call sites."""
        muts = _mutations("""
            import shlex

            def f(command, name):
                return shlex.split(command), name.split(",")
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
        b = next(m for m in muts if m.operator == "drop-conjunct")
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
        text = sweep.report([], Path("."), "abc", "def", None, 1.0)
        self.assertIn("Every mutation this diff offered goes red", text)

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
        verdict, subset, full = sweep.decide(box, _mutations("x = 1\n")[0] if False
                                             else self._mutation(), ["tests.test_a"])
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


class TheGateNeverChargesTheWholeTree(unittest.TestCase):
    def test_gate_and_all_together_are_refused(self):
        """Stage B is a fourteen-hour job and stage C is a pull-request check. Letting the
        two be asked for at once is how the gate becomes the reason nobody runs either."""
        said = io.StringIO()
        with contextlib.redirect_stderr(said), self.assertRaises(SystemExit):
            sweep.main(["--gate", "--all"])
        self.assertIn("never sweeps the whole tree", said.getvalue())


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


if __name__ == "__main__":      # pragma: no cover
    unittest.main()
