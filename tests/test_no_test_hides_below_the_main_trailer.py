"""Nothing in a test module may sit BELOW its `if __name__ == "__main__"` trailer (#531).

`unittest.main()` raises `SystemExit`. So in the one invocation the trailer exists for —
`python3 tests/test_x.py`, the way a developer runs the single file they are debugging —
every top-level statement written after it is dead: the classes are never defined, their
tests are never collected, and the run says `OK` about the subset that happened to be
above the line.

**Twenty-six modules had done it**, and the subset that ran was biased the wrong way. In
`test_doctor_shadowed.py` the eight tests above the trailer were the pure-function ones
and the six below were the `PersonaIso` classes that exercise the real seam;
`python3 tests/test_doctor_shadowed.py` reported `Ran 8 tests / OK` while
`python3 -m unittest tests.test_doctor_shadowed` ran 14. Across the twenty-six, **154
tests were invisible to a direct run**.

Nothing was lost in CI — `unittest discover` IMPORTS a module rather than executing it as
`__main__`, so the trailer never fires there and every class is collected. Measured, not
assumed: all 154 ran and passed under discovery. The gap opens only for the person
debugging one file, which is exactly the moment a green run is being trusted hardest, and
exactly the population #402/#492, #519/#521/#528 and #529 were also all about.

**A guard, not a sweep.** Moving twenty-six trailers is a fix that has to be made a
twenty-seventh time next month by whoever appends a class to a finished file and does not
notice the trailer sitting above the insertion point — which is how all twenty-six got
there, the shape being consistent enough to be diagnostic. This fails on the PR that adds
the twenty-seventh instead, naming the module, the line, what got hidden, and both ways
out.

**Anything, not just a class.** The rule is not "no class below the trailer" but "nothing
below the trailer", because `SystemExit` does not care what the statement is: a helper
function, a constant, a `mock.patch` teardown all fail the same way, and a rule that named
classes would wave the next shape through.
"""

from __future__ import annotations

import ast
import unittest
from pathlib import Path

#: The suite itself — the tree this walks, and the tree it is part of.
TESTS = Path(__file__).resolve().parent

#: `unittest discover`'s own default pattern, so this guard's idea of "a test module" and
#: the runner's cannot drift apart. A module the runner collects and this walker skipped
#: would be a hole exactly the shape of the defect.
PATTERN = "test*.py"


def _trailer(tree: ast.Module) -> ast.If | None:
    """The module's LAST `if __name__ == ...` block, or `None`.

    The last one rather than the first: a module with two would have the same defect
    measured from the lower one, and a check anchored on the first would report the
    statements between them as hidden when they are not.

    Matched on the NAME being compared, not on the string it is compared with — the
    idiom is spelled `"__main__"` everywhere in this suite today, but `'__main__'`,
    `!=`, or a comparison against a variable all still run at module-exec time and all
    still `SystemExit` past whatever follows.
    """
    found = None
    for node in tree.body:
        if (isinstance(node, ast.If)
                and isinstance(node.test, ast.Compare)
                and isinstance(node.test.left, ast.Name)
                and node.test.left.id == "__name__"):
            found = node
    return found


def hidden_below_the_trailer(source: str) -> tuple[int, list[tuple[int, str]]]:
    """What *source* defines after its trailer: `(trailer line, [(line, what), …])`.

    A pure function of the text so the guard below can be shown to FAIL on a module with
    the defect — see `TheGuardIsNotBlind`. A checker only ever run against a clean tree
    is a checker nobody has watched work.
    """
    tree = ast.parse(source)
    trailer = _trailer(tree)
    if trailer is None:
        return 0, []
    below = [n for n in tree.body if n.lineno > trailer.lineno]
    return trailer.lineno, [(n.lineno, getattr(n, "name", type(n).__name__))
                            for n in below]


class NoTestModuleHidesAnythingBelowItsTrailer(unittest.TestCase):

    def test_the_walker_sees_the_whole_suite(self):
        """The guard's own blind spot, checked first: a walk that found nothing to check
        would pass forever and say nothing. This suite is over two hundred modules."""
        modules = sorted(TESTS.glob(PATTERN))
        self.assertGreater(len(modules), 200,
                           f"only {len(modules)} test modules found under {TESTS} — the "
                           f"walk below is checking almost nothing")
        self.assertIn(Path(__file__).name, [p.name for p in modules],
                      "the guard does not match its own filename against "
                      f"{PATTERN!r}, so it cannot be walking what the runner collects")

    def test_nothing_is_defined_after_a_main_trailer(self):
        for path in sorted(TESTS.glob(PATTERN)):
            with self.subTest(module=path.name):
                line, below = hidden_below_the_trailer(path.read_text())
                self.assertEqual(
                    below, [],
                    f"{path.name} defines "
                    + ", ".join(f"{what!r} (line {at})" for at, what in below)
                    + f" AFTER the `if __name__` trailer on line {line}. "
                    "`unittest.main()` raises SystemExit, so `python3 "
                    f"tests/{path.name}` never defines any of that and reports OK about "
                    "the tests above the line only (`python3 -m unittest "
                    f"tests.{path.stem}` runs them all, which is why CI never saw it). "
                    "Move the trailer to the END of the file, or delete it and run the "
                    f"module as `python3 -m unittest tests.{path.stem}`.")


class TheGuardIsNotBlind(unittest.TestCase):
    """A guard nobody has watched fail is a guard nobody knows works — the same standard
    `tests/test_no_test_reads_the_operators_channel.py` holds its own tripwire to."""

    DEFECTIVE = (
        "import unittest\n"
        "class Above(unittest.TestCase):\n"
        "    def test_a(self):\n"
        "        pass\n"
        'if __name__ == "__main__":\n'
        "    unittest.main()\n"
        "class Below(unittest.TestCase):\n"
        "    def test_b(self):\n"
        "        pass\n"
    )

    def test_a_class_below_the_trailer_is_reported(self):
        line, below = hidden_below_the_trailer(self.DEFECTIVE)
        self.assertEqual(line, 5)
        self.assertEqual(below, [(7, "Below")])

    def test_the_same_module_with_the_trailer_last_is_clean(self):
        """The control for the control: the fix this guard asks for really does satisfy
        it, so "move the trailer down" is advice that works rather than advice that
        merely sounds right."""
        fixed = self.DEFECTIVE.replace(
            'if __name__ == "__main__":\n    unittest.main()\n', "")
        fixed += 'if __name__ == "__main__":\n    unittest.main()\n'
        self.assertEqual(hidden_below_the_trailer(fixed), (8, []))

    def test_a_helper_below_the_trailer_is_reported_too(self):
        """Not only classes — `SystemExit` hides a function or a constant just as
        completely, and the module that hides one is broken in a way that is harder to
        see, not easier."""
        source = ('import unittest\n'
                  'if __name__ == "__main__":\n'
                  '    unittest.main()\n'
                  'HELPER = 1\n'
                  'def helper():\n'
                  '    return HELPER\n')
        self.assertEqual(hidden_below_the_trailer(source),
                         (2, [(4, "Assign"), (5, "helper")]))

    def test_a_module_with_no_trailer_at_all_is_clean(self):
        """Three modules in this suite carry none. Deleting the trailer is one of the two
        ways out the failure message offers, so it has to be a way OUT and not a way into
        a different failure."""
        self.assertEqual(
            hidden_below_the_trailer("import unittest\nX = 1\n"), (0, []))

    def test_the_last_trailer_is_the_one_measured(self):
        """A module with two trailers is defective from the LOWER one. Anchored on the
        first, this would report the second trailer itself — and everything between them
        — as hidden, which is a failure message that sends the reader to the wrong line."""
        source = ('import unittest\n'
                  'if __name__ == "__main__":\n'
                  '    unittest.main()\n'
                  'if __name__ == "__main__":\n'
                  '    unittest.main()\n'
                  'X = 1\n')
        self.assertEqual(hidden_below_the_trailer(source), (4, [(6, "Assign")]))


if __name__ == "__main__":
    unittest.main()
