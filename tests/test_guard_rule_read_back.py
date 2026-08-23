"""`charter guard` prints the rule the way the harness's FILE spells it (#395).

The line under test is the tick:

    ✓ opencode: allowing ('slack_send', '*') → …/opencode.json

`Harness.ask_rule` returns the harness's rule in the harness's own shape — a string for
Claude Code, a `(permission, glob)` pair for opencode — and `cmd_guard_ask` /
`cmd_guard_allow` interpolated it straight into that sentence. So opencode's operator was
shown Python's repr of a 2-tuple: a spelling that appears in no `opencode.json` there has
ever been, which holds `{"permission": {"slack_send": {"*": "allow"}}}`.

That was cosmetic until #374, which made the same line load-bearing. #374 TRANSLATES an
MCP pattern rather than writing it verbatim — `mcp__slack__send` becomes `slack_send` —
and charter cannot check that the operator's own `mcp` block spells the server the same
way. The read-back became the only thing standing between a mistyped server and a rule
that is inert for it, and 0.49.0's argument for printing what was written ("a guard whose
output cannot be trusted is worse than no guard, because the tick is what stops you
checking") applies harder to a name the operator did not type than to one they did.

The fix is `Harness.rule_text`, a rendering for humans beside `ask_rule`'s structure for
the writer — the cheap half of the two the issue names, and deliberately so: making a rule
a string across all three harnesses would trade opencode's writer out of the shape it
needs to choose the file's form.

**So these tests are about FINDABILITY, not about a wording.** Asserting the string
`permission.slack_send."*"` would pass just as happily over a rendering that had drifted
from what `_apply_rule` writes — which is the whole defect, one costume over. They write
the rule for real and then follow the printed text into the file it names, so a rendering
that cannot be followed fails no matter how plausible it reads.
"""

from __future__ import annotations

import io
import json
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands, config
from charter.harness import registry
from charter.harness.base import Harness
from tests._isolation import PersonaIso

#: Patterns spanning every shape charter's translation can produce, with the verb left
#: out — one rendering serves both, which `TestBothVerbsAreShownTheSamePlace` pins.
PATTERNS = (
    "mcp__slack__send",      # translated MCP tool id — the name the operator never typed
    "mcp__slack",            # whole server, so the KEY carries the glob
    "git push *",            # falls through to `bash`, and the glob carries spaces
    "Read(./secrets/**)",    # a `TOOL_NAMES` id with a path glob
    "mcp__doom__loop",       # a flat-only permission: the file holds no glob at all
    "WebFetch(*)",           # the other reachable flat-only one
)


def _segments(text: str) -> list[str]:
    """``permission.bash."git push *"`` → ``["permission", "bash", "git push *"]``.

    A JSON-quoted segment is decoded with the stdlib decoder rather than stripped of its
    quotes, so a glob containing a `.` or a `"` comes back as one segment and this
    walker cannot be the reason a test passes.
    """
    out: list[str] = []
    i = 0
    while i < len(text):
        if text[i] == '"':
            seg, i = json.JSONDecoder().raw_decode(text, i)
            out.append(seg)
        else:
            j = text.find(".", i)
            j = len(text) if j < 0 else j
            out.append(text[i:j])
            i = j
        if i < len(text):
            if text[i] != ".":
                raise AssertionError(f"{text!r} is not a dotted path at {i}")
            i += 1
    return out


def _at_the_dotted_path(doc: dict, text: str):
    """What `opencode.json` holds at the path the operator was shown, or raise."""
    here = doc
    for seg in _segments(text):
        if not isinstance(here, dict) or seg not in here:
            raise AssertionError(f"{text!r} is not in {json.dumps(doc)}")
        here = here[seg]
    return here


def _in_the_rule_list(doc: dict, text: str):
    """Claude Code's file is a list of rule strings, so the whole text is the entry."""
    for bucket in ("ask", "allow"):
        for rule in doc.get("permissions", {}).get(bucket, []):
            if rule == text:
                return bucket
    raise AssertionError(f"{text!r} is in no bucket of {json.dumps(doc)}")


#: How to find a harness's printed text inside the file it named, per harness.
#:
#: A locator per harness rather than one generic matcher, because "findable" means
#: something different in a list of rule strings than in a nested permission object, and
#: a matcher loose enough to cover both (a substring test) would pass over the tuple repr
#: this file exists about — `('slack_send', '*')` contains `slack_send`.
#:
#: ``None`` is a claim, not a skip: that harness writes no rule at all, and
#: `test_the_harness_with_no_locator_really_writes_nothing` holds it to that.
LOCATORS = {
    registry.CLAUDE_CODE: (".claude/settings.json", _in_the_rule_list),
    registry.OPENCODE: ("opencode.json", _at_the_dotted_path),
    registry.CODEX: None,
}


class TestTheWalkersTheseTestsLeanOn(unittest.TestCase):
    """Anti-vacuity, first: everything below is worthless if these find anything."""

    def test_a_quoted_segment_survives_a_dot_and_a_space(self):
        self.assertEqual(_segments('permission.bash."git push *"'),
                         ["permission", "bash", "git push *"])
        self.assertEqual(_segments('permission.read."./a.b/**"'),
                         ["permission", "read", "./a.b/**"])
        self.assertEqual(_segments("permission.doom_loop"), ["permission", "doom_loop"])

    def test_a_path_that_is_not_in_the_document_raises(self):
        doc = {"permission": {"slack_send": {"*": "ask"}}}
        self.assertEqual(_at_the_dotted_path(doc, 'permission.slack_send."*"'), "ask")
        with self.assertRaises(AssertionError):
            _at_the_dotted_path(doc, 'permission.slack_recv."*"')
        with self.assertRaises(AssertionError):
            _at_the_dotted_path(doc, "('slack_send', '*')")
        # A path one segment short lands on the object rather than on the decision, so
        # the callers' `== "ask"` is what tells the two shapes apart — this walker
        # returning something is not the same as it finding the rule.
        self.assertEqual(_at_the_dotted_path(doc, "permission.slack_send"),
                         {"*": "ask"})

    def test_a_rule_that_is_in_no_bucket_raises(self):
        doc = {"permissions": {"ask": ["Bash(git push *)"]}}
        self.assertEqual(_in_the_rule_list(doc, "Bash(git push *)"), "ask")
        with self.assertRaises(AssertionError):
            _in_the_rule_list(doc, "Bash(git pull *)")


class TestTheTextNamesWhereTheRuleLanded(PersonaIso):
    """Write the rule, then follow the printed line into the file it names.

    One fresh plane per pattern and verb, so a key an earlier rule left behind cannot be
    the thing a later assertion finds.
    """

    def plane(self, *parts: str) -> Path:
        root = Path(config.ROOT) / "planes" / "-".join(parts).replace("/", "_")
        root.mkdir(parents=True, exist_ok=True)
        return root

    def test_every_harness_in_the_registry_has_a_locator(self):
        """A fourth harness fails here rather than silently going unchecked — the same
        reason `registry.KINDS` is iterated everywhere instead of harnesses being named
        one by one."""
        self.assertEqual(set(LOCATORS), set(registry.KINDS))

    def test_the_harness_with_no_locator_really_writes_nothing(self):
        """`None` above says "answers no rule", and that has to be checked or it becomes
        a way to excuse a harness from this file."""
        h = registry.get(registry.CODEX)
        for pattern in PATTERNS:
            with self.subTest(pattern=pattern):
                root = self.plane("codex", pattern)
                self.assertEqual(h.apply_ask_rule(root, pattern)[0], "unsupported")
                self.assertEqual(h.rule_text(pattern), "")

    def test_the_printed_text_leads_to_the_rule_that_was_written(self):
        """The whole point. Every harness, every pattern, both verbs: what `guard` would
        print has to lead to the decision that is actually in the file."""
        for name, locator in LOCATORS.items():
            if locator is None:
                continue
            filename, find = locator
            h = registry.get(name)
            for verb, decision in (("apply_ask_rule", "ask"),
                                   ("apply_allow_rule", "allow")):
                for pattern in PATTERNS:
                    with self.subTest(harness=name, verb=verb, pattern=pattern):
                        root = self.plane(name, verb, pattern)
                        status, detail = getattr(h, verb)(root, pattern)
                        if status != "added":
                            continue
                        doc = json.loads((root / filename).read_text())
                        text = h.rule_text(pattern)
                        self.assertTrue(text, f"{name} printed nothing for {pattern}")
                        self.assertEqual(find(doc, text), decision,
                                         f"{name}: {text!r} does not lead to the rule "
                                         f"in {detail}")

    def test_the_flat_only_shape_is_shown_flat(self):
        """`_apply_rule` picks between two shapes; the rendering has to pick the same
        one. `doom_loop` takes a bare action, so the file holds no glob key — printing
        `permission.doom_loop."*"` would send the reader looking for a key that is not
        there, over the one rule shape charter's own writer already knew better about."""
        h = registry.get(registry.OPENCODE)
        root = self.plane("shape", "flat")
        self.assertEqual(h.apply_ask_rule(root, "mcp__doom__loop")[0], "added")
        self.assertEqual(json.loads((root / "opencode.json").read_text()),
                         {"permission": {"doom_loop": "ask"}})
        self.assertEqual(h.rule_text("mcp__doom__loop"), "permission.doom_loop")

    def test_the_object_shape_is_shown_with_its_glob(self):
        """The other branch, so the flat one above is a choice rather than the answer to
        everything."""
        h = registry.get(registry.OPENCODE)
        root = self.plane("shape", "object")
        self.assertEqual(h.apply_ask_rule(root, "mcp__slack__send")[0], "added")
        self.assertEqual(h.rule_text("mcp__slack__send"), 'permission.slack_send."*"')

    def test_a_glob_carrying_spaces_is_quoted_the_way_the_file_quotes_it(self):
        """`git push *` run together with a dotted path is unreadable at exactly the
        moment the line is being read carefully."""
        h = registry.get(registry.OPENCODE)
        self.assertEqual(h.rule_text("git push *"), 'permission.bash."git push *"')


class TestBothVerbsAreShownTheSamePlace(unittest.TestCase):
    """One rendering for `ask` and `allow`, because it names WHERE the rule lives and the
    sentence around it carries the decision.

    The sibling of `TestBothVerbsTranslateTheSameWay` in
    `test_guard_opencode_mcp_rule.py`, and it fails in the same circumstance: a harness
    that overrides `allow_rule` away from `base`'s shared default would have two
    spellings and one renderer, and this is what says so rather than letting the allow
    line quietly describe the ask rule.
    """

    def test_no_harness_spells_the_two_verbs_differently(self):
        for h in registry.all():
            for pattern in PATTERNS:
                with self.subTest(harness=h.name, pattern=pattern):
                    self.assertEqual(h.allow_rule(pattern), h.ask_rule(pattern))


class TestNoHarnessPrintsAPythonRepr(unittest.TestCase):
    """The shape of the defect, named directly, for a harness added later."""

    def test_what_guard_would_print_is_a_string_from_every_harness(self):
        for h in registry.all():
            for pattern in PATTERNS:
                with self.subTest(harness=h.name, pattern=pattern):
                    self.assertIsInstance(h.rule_text(pattern), str)

    def test_the_base_renders_a_structural_rule_as_nothing_rather_than_a_guess(self):
        """A harness whose rule is a structure MUST override `rule_text`. The base class
        does not know how that harness's file writes it down, and both available guesses
        are the defect again: `str()` is the tuple repr, and a generic join would print a
        plausible path nothing on disk holds. An empty line is visibly broken, which is
        the only honest thing a class that does not know can print."""

        class Structural(Harness):
            name = "structural"

            def ask_rule(self, pattern):
                return ("some_key", "*")

        self.assertEqual(Structural().rule_text("mcp__x__y"), "")

    def test_a_harness_whose_rule_is_already_a_string_needs_no_override(self):
        """The default earns its place: Claude Code's rule IS its spelling."""
        h = registry.get(registry.CLAUDE_CODE)
        self.assertEqual(h.rule_text("git push *"), h.ask_rule("git push *"))
        self.assertEqual(h.rule_text("git push *"), "Bash(git push *)")


class TestThroughTheCommand(PersonaIso):
    """End to end, because the operator reads the command's output, not a method.

    **All four sentences, not one.** `guard` prints a rule from four places — `added` and
    `present`, each for `ask` and for `allow` — and each is its own interpolation of its
    own method call. A first pass here covered the `allow`/`added` and `ask`/`present`
    pair, and reverting either of the other two to `ask_rule` put the tuple straight back
    in front of an operator with the suite still green.
    """

    #: (command, decision, the word the tick uses, the word the second run uses).
    VERBS = ((commands.cmd_guard_ask, "ask", "asking for", "already asking for"),
             (commands.cmd_guard_allow, "allow", "allowing", "already allowing"))

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()

    def opencode_line(self, out: str, word: str) -> str:
        """The one line reporting opencode's rule. Not every `opencode:` line — the
        collision caveat wears the same prefix — and not every line carrying the word,
        because Claude Code's tick uses the same sentence."""
        lines = [ln for ln in out.splitlines()
                 if "opencode:" in ln and f" {word} " in f"{ln} "]
        self.assertEqual(len(lines), 1, out)
        return lines[0]

    def clean(self) -> None:
        """Back to a plane with no rules, so the next case is `added` and not `present`."""
        for rel in ("opencode.json", ".claude/settings.json"):
            p = Path(config.ROOT) / rel
            if p.exists():
                p.unlink()

    def test_every_sentence_that_reports_a_rule_names_the_key_in_the_file(self):
        for fn, decision, added, present in self.VERBS:
            for word, again in ((added, False), (present, True)):
                with self.subTest(verb=decision, sentence=word):
                    if not again:
                        self.clean()
                    rc, out = self.invoke(fn, pattern="mcp__slack__send", local=False)
                    self.assertEqual(rc, 0, out)
                    line = self.opencode_line(out, word)
                    self.assertIn('permission.slack_send."*"', line)
                    doc = json.loads((Path(config.ROOT) / "opencode.json").read_text())
                    self.assertEqual(
                        _at_the_dotted_path(doc, 'permission.slack_send."*"'), decision,
                        f"{line} does not lead to the rule in the file")

    def test_no_sentence_shows_the_repr_of_charters_own_data_structure(self):
        """The defect verbatim. `('slack_send', '*')` even contains the translated name,
        so a test that only looked for `slack_send` passed straight over it — which is
        why the assertion is on the repr's own punctuation."""
        for fn, decision, added, present in self.VERBS:
            for word, again in ((added, False), (present, True)):
                with self.subTest(verb=decision, sentence=word):
                    if not again:
                        self.clean()
                    _rc, out = self.invoke(fn, pattern="mcp__slack__send", local=False)
                    line = self.opencode_line(out, word)
                    self.assertNotIn("('slack_send', '*')", line)
                    self.assertNotIn("(", line)

    def test_claude_codes_line_did_not_move_to_fix_opencodes(self):
        """The half that was already right. Its rule is a string and prints as one."""
        _rc, out = self.invoke(commands.cmd_guard_ask, pattern="git push *", local=False)
        line = [ln for ln in out.splitlines() if "claude-code:" in ln][0]
        self.assertIn("Bash(git push *)", line)


if __name__ == "__main__":
    unittest.main()
