"""#576: "contain the sentence as it is assembled" had two private implementations.

`contain._sentence` bounded every field at `PATH_DISPLAY_LIMIT`. `news._report`, added a
version later (#573), bounded every field at `DISPLAY_LIMIT` and joined a field holding
several things element by element. Both were correct and both were tested; neither could
see the other, so the two differences — the budget and the sequence — were decided twice
and written down nowhere the other copy could read.

That is the argument `contain.py`'s own opening paragraph makes about `valid_name`, and
the one `marker()`, `_shell_syntax` and `tests/_planeguard.py` were each collapsed by:
**two lists for one concept drift, and the drift is the defect.** The third reporting
surface is what made it concrete — `commands_persona`'s tables, `frame/registry`'s
entry-point errors, `mcpseen` — because each one that writes its own gets a third budget
and a third answer to "what about a list?".

So there is one assembler, `contain._slots`, and two public spellings of the budget:
:func:`contain.sentence` at `DISPLAY_LIMIT` and :func:`contain.path_sentence` at
`PATH_DISPLAY_LIMIT`. This module states what that is supposed to buy.

**Why two functions and not one with `limit=`.** ``**fields`` puts the template's slot
names and the function's own parameter names in one namespace. A template that ever grew a
``{limit}`` slot would have its value eaten as the budget and then raise `KeyError` out of
`str.format`, inside the module whose stated rule is that nothing in it raises. Naming the
budget by which function you call is what makes a slot name unable to collide with it —
`test_no_slot_name_can_become_the_budget` is the check, and it is the same shape as every
other finding this month: a guard matching a spelling instead of a property.
"""

from __future__ import annotations

import ast
import inspect
import unittest
from pathlib import Path

from charter import contain, news

#: One of every way to end a line, plus one that ends no line and repaints the terminal —
#: the payload `tests/test_a_news_entry_cannot_forge_a_report_line.py` uses, for the same
#: reason: "newline" is four different characters depending on who is reading.
_BREAKS = ("\n", "\r", "\x85", " ", " ", "\x1b[31m")


class TheBudgetIsTheOnlyDifference(unittest.TestCase):
    """Two names for one implementation. If they diverge in anything but the clip, the
    merge did not happen and #576 is back with different function names."""

    #: Values that fit inside BOTH budgets, so the two functions must agree byte for byte.
    SHORT = ("plain", "", "with space", "a/b", "0.60.0-x.md", "\n\r ", "é", "\x00",
             ["one.md", "two.md"], (), 7, None, Path("/tmp/x"))

    def test_below_both_budgets_the_two_agree_byte_for_byte(self):
        for value in self.SHORT:
            with self.subTest(value=value):
                self.assertEqual(contain.sentence("{v}", v=value),
                                 contain.path_sentence("{v}", v=value))

    def test_and_they_disagree_where_the_budgets_do(self):
        """A path is legitimately long and a refusal that clips it is unactionable, which
        is the whole reason `PATH_DISPLAY_LIMIT` exists. Asserted at a length between the
        two so this fails if either budget moves onto the other."""
        long = "x" * (contain.DISPLAY_LIMIT + 40)
        self.assertLess(len(long), contain.PATH_DISPLAY_LIMIT)
        self.assertEqual(contain.path_sentence("{v}", v=long), long)
        self.assertNotEqual(contain.sentence("{v}", v=long), long)
        self.assertEqual(len(contain.sentence("{v}", v=long)), contain.DISPLAY_LIMIT + 1)

    def test_the_two_budgets_are_different_numbers(self):
        """The premise of having two. A change that quietly made them equal would leave
        every assertion above still passing on one of them."""
        self.assertGreater(contain.PATH_DISPLAY_LIMIT, contain.DISPLAY_LIMIT)

    def test_no_slot_name_can_become_the_budget(self):
        """The reason the budget is a function name and not a keyword argument.

        `{limit}` is an ordinary slot in charter's own template here. With a
        ``limit=DISPLAY_LIMIT`` parameter in the signature this call binds the parameter,
        substitutes nothing, and raises `KeyError` out of `str.format` — a refusal turned
        back into a crash, which is the bug `_path_refusal`'s empty-path branch already
        records once.
        """
        self.assertEqual(contain.sentence("{limit} is the cap", limit="160"),
                         "160 is the cap")
        self.assertEqual(contain.path_sentence("{limit}", limit="1024"), "1024")


class SeveralThingsIsAPropertyNotAType(unittest.TestCase):
    """`news._report` asked ``isinstance(value, (list, tuple))``, which is a spelling.

    A caller handing a `set`, a `frozenset`, a `dict`'s keys or a generator to a sentence
    that names several files fell off the end of that test and got Python's own `repr` of
    the container printed into charter's prose — brackets, quotes and all — which is the
    exact failure `test_a_sequence_is_contained_element_by_element` was written to catch
    for a `list` and could not see one container class over.
    """

    def test_every_container_a_caller_can_reach_for_is_joined_by_charter(self):
        for label, value in (
            ("list", ["a.md", "b.md"]),
            ("tuple", ("a.md", "b.md")),
            ("set", {"a.md"}),                 # one element: a set has no order to pin
            ("frozenset", frozenset({"a.md"})),
            ("generator", (n for n in ("a.md", "b.md"))),
            ("dict keys", {"a.md": 1, "b.md": 2}.keys()),
            ("a dict itself", {"a.md": 1, "b.md": 2}),
            ("sorted()", sorted(("b.md", "a.md"))),
        ):
            with self.subTest(container=label):
                said = contain.sentence("{names}", names=value)
                self.assertNotIn("[", said, said)
                self.assertNotIn("'", said, said)
                self.assertIn("a.md", said, said)

    def test_a_string_is_one_thing_however_iterable_it_is(self):
        """`str` and `bytes` are iterable and are not several things. Named as the
        exception, so the rule can stay "can this be iterated" rather than growing into a
        list of the container classes somebody has passed so far."""
        self.assertEqual(contain.sentence("{v}", v="abc"), "abc")
        self.assertEqual(contain.sentence("{v}", v=b"ab"), "b'ab'")

    def test_and_the_things_that_are_not_containers_are_unchanged(self):
        self.assertEqual(contain.sentence("{v}", v=7), "7")
        self.assertEqual(contain.sentence("{v}", v=None), "None")
        self.assertEqual(contain.path_sentence("{v}", v=Path("/tmp/x")), "/tmp/x")

    def test_the_separator_is_charters_own(self):
        """The literal, not `contain.SEQUENCE_SEPARATOR` interpolated into the expectation.

        Written the second way first, and it survived changing the constant — a test that
        reads the value it is checking asserts that the code equals itself. The constant
        exists so two report surfaces cannot separate their lists differently; what pins it
        is a rendering somebody read.
        """
        self.assertEqual(contain.sentence("{n}", n=["one.md", "two.md"]), "one.md, two.md")
        self.assertEqual(contain.SEQUENCE_SEPARATOR, ", ")


class ContainmentIsPerElementAndNotAfterTheJoin(unittest.TestCase):
    """A sentence that names a list exists to name every entry in it.

    Containing the joined string clips the tail off, so the last entries read as though
    they were never there — which is the one thing that sentence was written to say.
    """

    def test_a_long_first_entry_does_not_cost_the_last_one(self):
        names = ["x" * (contain.DISPLAY_LIMIT * 2), "last.md"]
        said = contain.sentence("{n}", n=names)
        self.assertTrue(said.endswith("last.md"), said[-40:])

    def test_and_the_long_one_is_still_bounded(self):
        """Per element, so a list of N is bounded at N budgets rather than at one — which
        is the cost of the paragraph above and is stated rather than hidden."""
        said = contain.sentence("{n}", n=["x" * 500])
        self.assertEqual(len(said), contain.DISPLAY_LIMIT + 1)

    def test_a_line_break_in_any_element_still_writes_one_line(self):
        for brk in _BREAKS:
            with self.subTest(payload=repr(brk)):
                said = contain.sentence("{n}", n=[f"a{brk}b", "c.md"])
                self.assertEqual(len(said.splitlines()), 1, repr(said))
                self.assertNotIn("\x1b", said)

    def test_the_template_is_read_for_slots_and_the_values_are_not(self):
        """`str.format` reads the TEMPLATE and never the substituted values, which is what
        lets this helper take a template at all. Held for both budgets, because a second
        format pass added to one of them would be invisible from the other."""
        for fn in (contain.sentence, contain.path_sentence):
            with self.subTest(fn=fn.__name__):
                self.assertEqual(fn("{a}", a="{b} {0} {}"), "{b} {0} {}")


class ThereIsNoSecondCopy(unittest.TestCase):
    """The merge is only worth anything while it stays merged."""

    def test_news_no_longer_carries_its_own(self):
        self.assertFalse(hasattr(news, "_report"),
                         "news grew a second assembler again — it belongs in contain")

    def test_every_format_call_in_these_two_modules_is_the_assemblers_own(self):
        """The structural half, and the reason it is worth an AST walk rather than a grep.

        A report sentence built with `.format` at a call site is exactly the shape #502
        closed: the spans somebody enumerated get contained and the next one does not. So
        in the two modules that assemble charter's report lines, the only `str.format`
        calls that may exist are the assembler's own two.

        Scoped to these two modules on purpose. Elsewhere in `charter/` a `.format` is
        ordinary — it builds an argv element, a URL, a label out of charter's own data —
        and a repo-wide ban would be a rule people route around rather than one they keep.
        """
        allowed = {"sentence", "path_sentence"}
        for module in (contain, news):
            source = Path(inspect.getsourcefile(module)).read_text()
            tree = ast.parse(source)
            # The enclosing function of every node, so a `.format` can be attributed to the
            # `def` it sits in rather than to the line it sits on.
            holder: dict[int, str] = {}
            for node in ast.walk(tree):
                if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    for child in ast.walk(node):
                        holder.setdefault(id(child), node.name)
            for node in ast.walk(tree):
                if (isinstance(node, ast.Call)
                        and isinstance(node.func, ast.Attribute)
                        and node.func.attr == "format"):
                    where = holder.get(id(node), "<module level>")
                    self.assertIn(
                        where, allowed,
                        f"{module.__name__}:{node.lineno} formats a template inside "
                        f"{where}() — a report line is assembled in contain.sentence")


class TheSentencesThemselvesStillSayWhatTheySaid(unittest.TestCase):
    """End to end through the real refusals, because the paragraphs above all talk to the
    assembler directly and a merge that broke a caller would not show up in any of them."""

    def test_a_refused_name_is_one_line_that_names_the_value(self):
        said = contain.refusal("a\nb/c")
        self.assertEqual(len(said.splitlines()), 1, said)
        self.assertIn("a\\x0ab/c", said)

    def test_a_refused_read_names_every_directory_a_plane_keeps_data_in(self):
        """`_not_plane_data` used to join the root names itself and hand the assembler one
        string, so the whole list shared one budget. It hands over the names now, and the
        thing that would go wrong — a root dropping out of the sentence that exists to say
        which directories are the plane's — is what this asserts against.
        """
        said = contain._not_plane_data("/nonexistent/elsewhere")
        for root in contain.data_roots():
            self.assertIn(Path(root).name, said, said)
        self.assertEqual(len(said.splitlines()), 1, said)

    def test_and_it_lists_them_alphabetically_rather_than_in_tuple_order(self):
        """`sorted`, doing work: `data_roots()` answers personas, workspaces, persona-state
        — which is not alphabetical — and two refusals read side by side should not differ
        by the order a tuple happens to be written in."""
        names = [Path(r).name for r in contain.data_roots()]
        self.assertNotEqual(names, sorted(names),
                            "the roots are already in order, so this proves nothing")
        said = contain._not_plane_data("/nonexistent/elsewhere")
        self.assertIn("(" + ", ".join(sorted(names)) + ")", said, said)


if __name__ == "__main__":
    unittest.main()
