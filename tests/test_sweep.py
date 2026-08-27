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

    def __init__(self, subset=None, full: sweep.Outcome | None = None):
        self._subset = subset if isinstance(subset, list) else [subset] * 4
        self._full = full
        self.applied: list[sweep.Mutation] = []
        self.subset_calls = 0
        self.full_calls = 0

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
                       full=sweep.Outcome(False, 6000, "FAILED (failures=1)"))
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
        box = _FullFlake(sweep.Outcome(False, 6000, "FAILED (errors=1)"),
                         sweep.Outcome(True, 6000, "OK"))
        verdict, _, full = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "survived")
        self.assertEqual(box.full_calls, 2)
        self.assertIn("green on confirmation", full.detail)

    def test_a_full_suite_red_twice_pins_the_guard(self):
        box = _FullFlake(sweep.Outcome(False, 6000, "FAILED (failures=1)"),
                         sweep.Outcome(False, 6000, "FAILED (failures=1)"))
        verdict, _, _ = sweep.decide(box, _M, ["tests.test_x"])
        self.assertEqual(verdict, "pinned")
        self.assertEqual(box.full_calls, 2)

    def test_a_red_subset_is_confirmed_once_and_then_believed(self):
        """A red twice IS a red, and re-running the whole suite for it would spend four
        minutes to learn nothing. The asymmetry runs one way only."""
        box = _FakeBox(subset=sweep.Outcome(False, 40, "FAILED"))
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
        box = _FakeBox(subset=[sweep.Outcome(False, 40, "FAILED"),
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

    def test_a_failure_carries_its_reason(self):
        v = sweep._verdict(_Completed(1, "FAIL: test_x\nRan 42 tests in 1s\n\nFAILED (failures=1)\n"))
        self.assertFalse(v.green)
        self.assertEqual(v.ran, 42)
        self.assertIn("test_x", v.detail)


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


if __name__ == "__main__":      # pragma: no cover
    unittest.main()
