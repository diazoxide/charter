"""A refused persona reference is told apart from a refused *path* (#361).

`reference_ok` is two checks — `valid_name` and `contain.child` — and `_reference_problem`
rendered `contain.refusal` for either. That message enumerates "no '/', no '\\', no '.' or
'..', nothing absolute". For ``extends: "parent"`` — quoted, because charter's frontmatter
parser does not strip quotes — **every one of those conditions is false**. The operator was
sent looking for a slash that is not there when the fix is to delete two quote marks.

The irony is the point: `_reference_problem`'s own docstring says "#328's whole shape is a
distinction like this one being collapsed", three lines above the line that collapsed one.

Each test asserts its **precondition** — which half of `reference_ok` actually failed —
because a message test that passes while the other branch produced the string proves
nothing about the branch it names.
"""
from __future__ import annotations

import unittest

from charter import config, contain, persona
from tests._isolation import PersonaIso

#: The live case from #361, exactly as a frontmatter parser hands it over.
QUOTED = '"parent"'


class ReferenceRefusalCase(PersonaIso):
    def assert_containment_passed(self, ref: str) -> None:
        """Precondition: the *containment* half is satisfied, so anything this reference
        is refused for is the alphabet — and `contain.refusal` would be a false sentence."""
        self.assertIsNotNone(
            contain.child(config.PERSONAS_DIR, ref),
            f"precondition failed: {ref!r} does not survive containment, so this test "
            f"would be asserting about the wrong half of reference_ok")

    def assert_containment_failed(self, ref: str) -> None:
        self.assertIsNone(
            contain.child(config.PERSONAS_DIR, ref),
            f"precondition failed: {ref!r} survives containment, so the containment "
            f"message is not the one under test")


class TestTheQuotedReferenceFromIssue361(ReferenceRefusalCase):
    def test_precondition_only_the_alphabet_half_rejects_it(self):
        """The whole basis of the issue, asserted rather than assumed: the quoted name is
        not a path by any of the tests the old message listed."""
        self.assert_containment_passed(QUOTED)
        self.assertFalse(persona.valid_name(QUOTED))
        self.assertFalse(persona.reference_ok(QUOTED))
        self.assertTrue(persona.reference_ok("parent"))

    def test_the_message_does_not_call_it_a_path(self):
        msg = persona.reference_refusal(QUOTED)
        self.assertNotIn("it is a path", msg)

    def test_the_message_does_not_send_the_reader_looking_for_a_separator(self):
        """Every condition the old sentence enumerated is false for this reference."""
        msg = persona.reference_refusal(QUOTED)
        for absent in ("no '/'", "no '\\'", "nothing absolute", "'.' or '..'"):
            self.assertNotIn(absent, msg)

    def test_the_message_names_the_quotes_and_says_to_remove_them(self):
        msg = persona.reference_refusal(QUOTED)
        self.assertIn("quote", msg)
        self.assertIn("remove", msg)


class TestAReferenceThatReallyIsAPath(ReferenceRefusalCase):
    """The containment half keeps its own sentence, and is asked first — where a reference
    is both a path and outside the alphabet, "it is a path" is the more serious and more
    useful thing to say."""

    def test_a_relative_path_still_gets_the_containment_message(self):
        self.assert_containment_failed("../evil")
        self.assertEqual(persona.reference_refusal("../evil"), contain.refusal("../evil"))

    def test_a_separator_still_gets_the_containment_message(self):
        self.assert_containment_failed("team/parent")
        self.assertIn("it is a path", persona.reference_refusal("team/parent"))

    def test_dot_dot_still_gets_the_containment_message(self):
        self.assert_containment_failed("..")
        self.assertIn("it is a path", persona.reference_refusal(".."))


class TestTheAlphabetMessageNamesWhatWasViolated(ReferenceRefusalCase):
    def test_a_leading_underscore_is_named_as_the_reserved_namespace(self):
        """`_` IS in the alphabet — it is only the FIRST character that is wrong, so
        listing it as a disallowed character would be its own small lie."""
        self.assert_containment_passed("_shared")
        msg = persona.reference_refusal("_shared")
        self.assertIn("_", msg)
        self.assertIn("reserved", msg)

    def test_an_uppercase_reference_names_the_character(self):
        self.assert_containment_passed("Frontdoor")
        self.assertIn("'F'", persona.reference_refusal("Frontdoor"))

    def test_a_space_is_named(self):
        self.assert_containment_passed("front door")
        self.assertIn("' '", persona.reference_refusal("front door"))

    def test_an_empty_reference_is_neither_a_path_nor_a_bad_character(self):
        msg = persona.reference_refusal("")
        self.assertNotIn("it is a path", msg)
        self.assertIn("empty", msg)


class TestOneFunctionDecidesVerdictAndSentence(ReferenceRefusalCase):
    """`reference_ok`'s docstring claims to be "the one place that answers this". It was
    for the verdict and not for the message — which is how the message came to describe a
    different failure from the one that happened."""

    CASES = ("parent", QUOTED, "../evil", "team/parent", "..", "", "_shared",
             "Frontdoor", "front door", "front.door", "front-door_2")

    def test_ok_is_exactly_the_absence_of_a_refusal(self):
        for ref in self.CASES:
            with self.subTest(ref=ref):
                self.assertEqual(persona.reference_ok(ref),
                                 persona.reference_refusal(ref) is None)

    def test_a_good_reference_has_no_refusal(self):
        self.assertIsNone(persona.reference_refusal("parent"))
        self.assertIsNone(persona.reference_refusal("front.door"))
        self.assertIsNone(persona.reference_refusal("front-door_2"))


class TestWhatLintPrints(ReferenceRefusalCase):
    """The message an operator actually reads. `structural_errors` is what `persona lint`
    and the status line both go through."""

    def test_lint_names_the_quotes_rather_than_a_missing_slash(self):
        self.make_persona("parent", role="Parent", vault="none")
        self.make_persona("child", role="Child", vault="none", extends=QUOTED)
        errs = persona.structural_errors("child")
        self.assertTrue(errs, "precondition: the quoted extends must be a lint error")
        text = " ".join(m for _, m in errs)
        self.assertIn("extends:", text)
        self.assertIn("quote", text)
        self.assertNotIn("it is a path", text)

    def test_lint_still_calls_a_real_path_a_path(self):
        self.make_persona("child", role="Child", vault="none", extends="../evil")
        text = " ".join(m for _, m in persona.structural_errors("child"))
        self.assertIn("it is a path", text)

    def test_a_dangling_but_well_formed_reference_is_still_dangling(self):
        """The third distinction, unchanged: a name that is a name and simply absent sends
        the reader hunting for the persona, which is the right place to look."""
        self.make_persona("child", role="Child", vault="none", extends="nosuchpersona")
        text = " ".join(m for _, m in persona.structural_errors("child"))
        self.assertIn("dangling", text)
        self.assertNotIn("quote", text)
        self.assertNotIn("it is a path", text)


if __name__ == "__main__":
    unittest.main()
