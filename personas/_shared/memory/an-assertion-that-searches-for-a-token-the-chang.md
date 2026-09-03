# AN ASSERTION THAT SEARCHES FOR A TOKEN THE CHANGE DELETES PASSES VACUOUS

_2026-08-31 03:28 · persistent_

AN ASSERTION THAT SEARCHES FOR A TOKEN THE CHANGE DELETES PASSES VACUOUSLY — a test can stop testing without going red. Found 2026-08-31 in charter's EXISTING suite by the #771 author, not in their new code: test_every_row_of_the_plane_is_on_screen_and_nothing_admits_otherwise searched the rendered row for the substring 'more)'. The change replaced '…(+8 more)' with '…(4 above, 4 below)', deleting the word 'more' — so the test would have kept passing against a line it could no longer find, asserting nothing. It now asks for '…(' instead. Two OTHER existing tests turned out to pin the defect itself ('at the top and at the bottom alike', asserting the identical string at three different scroll offsets). Operational rule: when a change alters rendered text, GREP THE SUITE FOR THE OLD TOKENS FIRST — every assertion keyed on a string you are deleting is a candidate vacuous pass, and it will not announce itself. This is the same family as [[the-assertion-sat-on-the-path-that-already-satis]] but the mechanism is different: there the assertion was true on the wrong path; here the assertion has no path at all.

## Generalised 2026-08-31: THREE mechanisms, all the same class

The class is **a test that passes because it stopped being able to see the thing it
measures**. Three instances turned up in a single branch (#771), one of them the author's
own, which is why it is worth naming as a class rather than as three tips:

1. **The assertion greps for a token the change deletes.** `…(+8 more)` became
   `…(4 above, 4 below)`; a test searching for `more)` now matches nothing and asserts
   nothing. Remedy: grep the suite for the old strings BEFORE changing rendered text.
2. **The assertion is identical at several inputs, so it pins the bug.** Two tests asserted
   the same overflow string at three different scroll offsets — which is precisely the
   defect (one string for 8-below, 4-and-4, and 8-above). A test that passes at every input
   may be describing a constant, not a behaviour.
3. **The test is defined below `if __name__ == "__main__": unittest.main()`.** `unittest.main()`
   raises `SystemExit`, so a class appended after it never runs and the file reports OK about
   the cases above the line only. Caused by `cat >>` appending to a test file. charter already
   guards this: `tests/test_no_test_hides_below_the_main_trailer`, and it fired.

What unites them: **none goes red.** Ordinary breakage announces itself; this class removes
the announcement. So the counter-measure cannot be "run the tests" — it has to be an
independent check that the test still observes its subject (a guard, a deliberate
red-then-green, or re-running the measurement that found the bug over the fixed tree).

## Final count from #771: FIVE instances in one branch, and a fourth mechanism

Four were pre-existing in charter's suite; the fifth was the author's own. One was caught
by CI, not by the author — so a careful reader looking for this class still missed one.

**Mechanism 4, the nastiest: asserting the ABSENCE of a token the change deletes.**
`test_every_row_of_the_plane_is_on_screen_and_nothing_admits_otherwise` used
`assertNotIn("more)", ...)`. Once the change deletes the word "more", that assertion is
**vacuously true forever** — and unlike a positive assertion, an absence assertion looks
correct in review precisely because it is passing. Positive assertions at least fail loudly
when the token moves; negative ones cannot fail at all once their needle stops existing.

Two more instances of mechanism 2 (identical assertion at several inputs) turned up in the
same branch, one with a docstring stating the defect as an intention: *"at the top and at
the bottom alike"* — the bug written down as an expectation, which is why nobody questioned
it. And `TerseSaysLess` asked for the bare substring `more` to prove "a panel showing less
says how much less"; it now asserts the real derived count (`6 below` from `_TERSE_ROWS - 1`).

**Practical form of the rule:** before changing rendered text, grep the suite for the old
tokens in BOTH directions — `assertIn` and `assertNotIn`, positive and negative — and treat
every hit as a candidate vacuous pass until re-verified against the new text.
