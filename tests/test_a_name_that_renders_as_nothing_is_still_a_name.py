"""Three reports told somebody to go and fix a thing, without naming the thing (#498).

`contain.one_line` escapes a character whose Unicode general category is in
``{"Cc", "Cf", "Cs", "Zl", "Zp"}``, or that is whitespace other than ``" "``. That is a
list of **spellings**, and the class that renders as nothing is not inside it: U+3164
HANGUL FILLER and U+115F/U+1160 are ``Lo``, U+2800 BRAILLE PATTERN BLANK is ``So``, none of
the four is `isspace`, and all four survive `strip`.

`one_line`'s docstring is honest about that — it promises a value cannot forge a second
ROW, and says in the same breath that it does not make the value readable. So the defect
was never in that function. It was in three callers that need the second property and asked
this one for it:

* the `lint` row prefix, which read ``✗ : no role`` — a finding about a persona it does not
  name, out of a report whose every row ends in *go and fix it*;
* ``persona '' does not load``, the one sentence whose whole job is to say WHICH;
* the `bin/` bullet in the brief a sub-agent reads, which read ``- `personas/<name>/bin/```
  — an instruction to run a directory, in the one document written for the model.

**What this file pins, and why it is three things and not one.**

1. *The property, over the whole codespace.* `contain.readable` decides on the COMPLEMENT —
   printable ASCII is what may reach a report line, everything else prints as its escape —
   so "renders as nothing" is decidable rather than enumerable, and no codepoint has to be
   added to anything. `TestReadableIsDecidedNotEnumerated` asks that of all 1,114,112
   codepoints rather than of the four in the issue. The four are here too, by name, so a
   failure says which one.
2. *The three reports.* Asserted end to end, on the printed row, because a bound on a
   helper is not a bound on a report — that gap is #498's own shape, and `persona.lint`
   bounding its MESSAGE while `cmd_persona_lint` built the row prefix out of the raw name
   is exactly how it survived a round.
3. *That `one_line` was NOT widened.* It has some ninety callers printing workspace names,
   roles, component titles and forge text into a TUI, where a non-ASCII value is content
   that should reach the screen as its glyphs. `TestOneLineKeptItsContract` fails if a later
   round "fixes" #498 by widening the shared function instead, which would escape every one
   of them to make these three legible.

The cost is stated rather than hidden: a legitimately non-ASCII value at one of these three
sites prints as escapes. `TestOrdinaryNamesAreUntouched` holds the half that matters —
charter mints persona names out of ``[a-z0-9][a-z0-9._-]`` (`persona.valid_name`), so a name
these reports are asked about is ASCII by construction and comes back byte-identical — and
`TestReadableLosesNothing` holds the other half: the escape is reversible, so a non-ASCII
value is re-spelled and never destroyed.
"""

from __future__ import annotations

import io
import shutil
import string
import unicodedata
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands_persona, config, contain, mcpseen, persona, tui
from tests._isolation import PersonaIso

#: The four from the issue. Written as escapes, never as the literal character: a raw
#: U+3164 in a fixture is invisible in the editor of whoever reads this next, which is the
#: same property that let it through three report surfaces. NOT the bound — the bound is the
#: codespace sweep below. This exists so a failure names which spelling failed.
BLANK_RENDERING = {
    "U+3164 HANGUL FILLER": "\u3164",
    "U+2800 BRAILLE PATTERN BLANK": "\u2800",
    "U+115F HANGUL CHOSEONG FILLER": "\u115f",
    "U+1160 HANGUL JUNGSEONG FILLER": "\u1160",
}

#: Every spelling of "this line ends here" beyond the `\r`/`\n` pair everyone thinks of.
#: Escapes, for the same reason as above. The BOUND is the codespace sweep; this exists so
#: a failure names the separator.
SEPARATORS = ("\n", "\r", "\u2028", "\u2029", "\x85", "\v", "\f", "\x1c")

#: Values a person might legitimately carry that are not ASCII. The control group: this fix
#: must not destroy them, and must not leak into the surfaces that print them as content.
ORDINARY_NON_ASCII = ("Աարոն", "дизайн", "Ünal", "日本語", "José", "Χάρης")


def printable_ascii(s: str) -> bool:
    return all(" " <= c <= "~" for c in s)


def could_be_a_filename(ch: str) -> bool:
    """The two characters no POSIX filesystem can hold, which is all this can know up front.

    Everything past that is the filesystem's own answer and is asked BY CREATING THE
    DIRECTORY — see `ReportCase.sweep_categories`. APFS validates names against its own Unicode
    version and refuses an unassigned codepoint or a lone surrogate outright (EILSEQ),
    while ext4 takes any byte sequence, so a static predicate here would either drop
    categories Linux covers fine or claim ones macOS cannot reach.
    """
    return ch not in ("/", "\x00")


def one_per_category(usable=lambda ch: True) -> dict[str, str]:
    """One codepoint per Unicode general category, taken by sweeping the codespace.

    Generated rather than listed, so a category nobody here has thought about is covered
    without this file being edited — which is #498's own complaint restated as a test: a
    list of categories is a list of spellings.

    Printable ASCII is skipped because it is the TARGET of the rule, not a threat to it.
    """
    out: dict[str, str] = {}
    for cp in range(0x110000):
        ch = chr(cp)
        if " " <= ch <= "~":
            continue
        cat = unicodedata.category(ch)
        if cat not in out and usable(ch):
            out[cat] = ch
    return out


def unescape(shown: str) -> str:
    """`contain.escaped` read backwards. A left inverse is what "reversible" MEANS, so this
    is written out rather than asserted against a second copy of the forward table."""
    out: list[str] = []
    i = 0
    while i < len(shown):
        c = shown[i]
        if c != "\\":
            out.append(c)
            i += 1
        elif shown[i + 1] == "\\":
            out.append("\\")
            i += 2
        elif shown[i + 1] == "u":
            out.append(chr(int(shown[i + 2:i + 6], 16)))
            i += 6
        elif shown[i + 1] == "U":
            out.append(chr(int(shown[i + 2:i + 10], 16)))
            i += 10
        else:
            raise AssertionError(f"not an escape this function writes: {shown[i:i + 4]!r}")
    return "".join(out)


class TestReadableIsDecidedNotEnumerated(unittest.TestCase):
    """The property, over the codespace — not over a list of four."""

    def test_no_codepoint_renders_as_nothing(self):
        """The whole class, one codepoint at a time.

        The assertion is the PROPERTY — what comes back is printable ASCII and holds
        something other than a space — rather than equality with whichever escape the code
        currently writes, which would pass for any escape including none.
        """
        for cp in range(0x110000):
            shown = contain.readable(chr(cp) * 3)
            if not (printable_ascii(shown) and shown.strip(" ")):
                self.fail(f"U+{cp:04X} ({unicodedata.category(chr(cp))}) → {shown!r}")

    def test_the_four_from_the_issue_are_named_by_the_output(self):
        """Redundant with the sweep, and kept: a failure that says U+2800 sends the reader
        to the right paragraph, where "some codepoint in the sweep" sends them to none."""
        for label, ch in BLANK_RENDERING.items():
            with self.subTest(codepoint=label):
                shown = contain.readable(ch * 3)
                self.assertTrue(shown.strip(" "), f"{label} renders as nothing")
                self.assertTrue(printable_ascii(shown), repr(shown))
                self.assertIn(f"{ord(ch):04x}", shown.lower())

    def test_only_a_run_of_spaces_is_shown_as_blank(self):
        """After the escape the string is printable ASCII, where the space is the only
        member that renders as nothing — so this is the ONE remaining input that names
        nothing, and it is named rather than printed as a gap."""
        self.assertEqual(contain.readable(""), contain.BLANK)
        self.assertEqual(contain.readable("     "), contain.BLANK)
        self.assertEqual(contain.readable("\t\n"), "\\u0009\\u000a")
        # …and NOT for a value that merely begins with spaces: that one has a name.
        self.assertNotEqual(contain.readable("   devops"), contain.BLANK)
        self.assertTrue(contain.BLANK.strip(" "), "the blank marker is itself blank")

    def test_a_clipped_value_is_still_printable_ascii(self):
        """The clip marker is ASCII dots and not `…`, so the promise holds for a long value
        too and no caller has to special-case the marker it appended itself."""
        shown = contain.readable("\u3164" * 5000, limit=64)
        self.assertTrue(printable_ascii(shown), repr(shown))
        self.assertLessEqual(len(shown), 64 + 3)

    def test_a_value_that_is_not_text_is_rendered_rather_than_raised(self):
        """`contain`'s rule is that nothing on a report path raises, and these are report
        paths: a row that crashes tells its reader less than a row that says `None`. The
        coercion is what delivers that, so it is asserted rather than assumed — the sweep
        found it unpinned, and deleting it turns `readable(Path(...))` into a TypeError
        one caller away."""
        self.assertEqual(contain.readable(None), "None")
        self.assertEqual(contain.readable(7), "7")
        self.assertEqual(contain.readable(Path("personas/seo/bin/x")),
                         "personas/seo/bin/x")

    def test_a_value_that_exactly_fits_is_not_clipped(self):
        """The boundary. An off-by-one here takes the last character off every name of
        exactly the limit's length, which is a quieter version of the finding this fixes:
        a report that names something slightly other than the thing."""
        self.assertEqual(contain.readable("a" * 64, limit=64), "a" * 64)
        self.assertEqual(contain.readable("a" * 65, limit=64), "a" * 64 + "...")

    def test_two_different_values_never_print_the_same(self):
        """A five-hex-digit escape would make U+1F600 and U+1F60 followed by '0' read
        identically, which is the blank-name finding wearing an alphabet. The astral form is
        eight digits, and a literal backslash is doubled, so every escape printed is a
        codepoint that was really there."""
        self.assertNotEqual(contain.readable(chr(0x1F600)),
                            contain.readable(chr(0x1F60) + "0"))
        self.assertNotEqual(contain.readable("\\u3164"), contain.readable("\u3164"))


class TestReadableLosesNothing(unittest.TestCase):
    """Escaped, never dropped — so a non-ASCII value is re-spelled rather than destroyed."""

    def test_an_ordinary_non_ascii_value_round_trips(self):
        for raw in ORDINARY_NON_ASCII:
            with self.subTest(raw=raw):
                self.assertEqual(unescape(contain.readable(raw)), raw)

    def test_every_codepoint_round_trips(self):
        for cp in range(0x110000):
            raw = "x" + chr(cp) + "y"
            if unescape(contain.readable(raw)) != raw:
                self.fail(f"U+{cp:04X} does not round-trip: {contain.readable(raw)!r}")


class TestOneLineKeptItsContract(unittest.TestCase):
    """#498 is a defect in three CALLERS, and this is what says so in code.

    `one_line` promises line structure and states that it does not promise readability. Its
    other callers — the frame's pickers, overlays, slots and switch messages, the roster
    table, the registry's diagnostics — print content into a TUI, where a Cyrillic or
    Japanese value should reach the screen as its glyphs. Widening `one_line` to fix these
    three reports would escape every one of them, which is why this fix did not.
    """

    def test_an_invisible_codepoint_still_reaches_one_line_unchanged(self):
        for label, ch in BLANK_RENDERING.items():
            with self.subTest(codepoint=label):
                self.assertEqual(contain.one_line(ch * 3), ch * 3)

    def test_ordinary_non_ascii_still_reaches_one_line_unchanged(self):
        for raw in ORDINARY_NON_ASCII:
            with self.subTest(raw=raw):
                self.assertEqual(contain.one_line(raw), raw)

    def test_one_line_still_stops_a_second_row(self):
        """The property it does promise, unchanged by this fix."""
        for sep in SEPARATORS:
            with self.subTest(sep=repr(sep)):
                self.assertEqual(len(contain.one_line(f"a{sep}b").splitlines()), 1)

    def test_readable_also_stops_a_second_row(self):
        """A readable rendering has to be a one-line rendering too, or the fix trades one
        finding for the other. It is, by construction: a separator is outside printable
        ASCII, so the complement escapes it without having to know it is a separator."""
        for sep in SEPARATORS:
            with self.subTest(sep=repr(sep)):
                self.assertEqual(len(contain.readable(f"a{sep}b").splitlines()), 1)

    def test_no_codepoint_at_all_can_forge_a_row(self):
        for cp in range(0x110000):
            if len(contain.readable(f"a{chr(cp)}b").splitlines()) != 1:
                self.fail(f"U+{cp:04X} ended the line")


class TestTheSharedEscapeKeptItsTwoCallersApart(unittest.TestCase):
    """`mcpseen._safe` and `_esc` now call the same `contain.escaped`, and the ONE thing
    that separates them is the `quote` flag. A flag is a thing somebody flips, so both
    settings are pinned here rather than left to the docstring that explains them.

    Neither direction was caught before: hand-checking the swap turned up that adding
    `quote=True` to `_safe` reddened nothing at all, which is a refactor that has quietly
    changed a printed label with nothing to say so.
    """

    def test_a_label_shows_a_quote_as_itself(self):
        """`_safe` renders an IDENTIFIER, and nothing delimits with a quote there — so
        escaping one only makes an ordinary name harder to read."""
        self.assertEqual(mcpseen.label('say "hi"'), 'say "hi"')
        self.assertNotIn('\\"', mcpseen.label('a"b'))

    def test_a_destination_shows_a_quote_as_an_escape(self):
        """`_esc` renders a value that charter prints BETWEEN quotes, so the quote has to
        be a delimiter no committed byte can spell."""
        self.assertEqual(mcpseen._esc('a"b'), 'a\\"b')

    def test_both_still_escape_everything_outside_printable_ascii(self):
        for fn in (mcpseen._safe, mcpseen._esc):
            with self.subTest(fn=fn.__name__):
                for ch in BLANK_RENDERING.values():
                    out = fn("x" + ch + "y")
                    self.assertTrue(printable_ascii(out), repr(out))
                    self.assertIn(f"{ord(ch):04x}", out.lower())


class TestOrdinaryNamesAreUntouched(unittest.TestCase):
    """The control. An over-eager fix mangles the names it was not about."""

    def test_a_persona_name_comes_back_byte_identical(self):
        """Charter mints these itself — `persona.valid_name` is a lowercase letter or digit
        then `[a-z0-9._-]` — so every name these three reports are asked about in practice
        is ASCII, and passes through unchanged."""
        for name in ("devops", "seo", "front-door", "a.b_c-1", "steward2"):
            with self.subTest(name=name):
                self.assertTrue(persona.valid_name(name))
                self.assertEqual(contain.readable(name), name)

    def test_every_character_the_alphabet_admits_is_unchanged(self):
        """Derived from `valid_name` itself rather than from a list of five, so the claim
        survives that alphabet being widened, and fails loudly if it is ever widened outside
        ASCII — which is the day this fix starts costing somebody their name."""
        admitted = sorted({c for c in map(chr, range(0x110000))
                           if persona.valid_name("a" + c)})
        self.assertGreater(len(admitted), 35, "the alphabet sweep found almost nothing")
        # This sweep used to hold a `"\n"` at the front. `_NAME_RE` ends in `$`, and in
        # Python `$` matches at the end of the string OR before a TRAILING newline, so
        # `valid_name("a\n")` was true and a `personas/evil<LF>/` directory resolved,
        # loaded, and wrote a blank line into a generated agent's frontmatter. It was
        # asserted here rather than filtered out precisely so that whoever switched to
        # `fullmatch` would see what the line was holding — #577 did, and the newline is
        # gone. The alphabet is now printable ASCII outright, which is what every sentence
        # in this file already claimed it was.
        self.assertEqual("".join(admitted),
                         "-." + string.digits + "_" + string.ascii_lowercase)
        joined = "a" + "".join(admitted)
        self.assertEqual(contain.readable(joined), joined)

    def test_an_ordinary_script_path_is_unchanged(self):
        for p in ("personas/seo/bin/site-health.sh", "personas/a.b/bin/run_me",
                  "/opt/plane/personas/x/bin/tool-2"):
            with self.subTest(path=p):
                self.assertEqual(contain.readable(p, contain.PATH_DISPLAY_LIMIT), p)

    def test_the_backslash_is_the_only_ascii_that_changes(self):
        """Doubled, so a literal `\\u3164` in a value cannot read as the codepoint. That is
        the one place an ASCII value is re-spelled, and it is stated rather than found.

        Probed inside ``x…y`` rather than alone, because a value that is only a space is
        the one input :data:`contain.BLANK` is for and would otherwise read as a second
        exception to a rule that has one.
        """
        changed = [c for c in map(chr, range(0x80))
                   if " " <= c <= "~" and contain.readable(f"x{c}y") != f"x{c}y"]
        self.assertEqual(changed, ["\\"])


def named_in(row: str) -> str:
    """The name a lint row is ABOUT, with charter's own decoration taken off.

    A row is ``<glyph> <name>: <message>``; the glyph is charter's (`util.err` writes ``✗``,
    `util.warn` writes ``!``). What this returns is the part a reader would copy in order to
    go and find the persona.
    """
    prefix = tui.strip_ansi(row).split(":", 1)[0]
    return prefix.split(" ", 1)[1].strip() if " " in prefix else ""


class ReportCase(PersonaIso):
    """A persona directory whose NAME a commit chose — which is what `personas/*/` is."""

    def reset(self) -> None:
        """Back to an empty roster, without tearing down the isolated plane. `tearDown` is
        not what restores this fixture (`addCleanup` is), so calling it in a loop would
        leave the sweeps running against a plane nobody had put back."""
        shutil.rmtree(config.PERSONAS_DIR, ignore_errors=True)
        config.PERSONAS_DIR.mkdir(parents=True, exist_ok=True)

    def make_named(self, name: str) -> None:
        d = config.PERSONAS_DIR / name
        d.mkdir(parents=True)
        (d / "persona.md").write_text("---\nrole: Victim\nvault: none\n---\n\nbody\n")

    def sweep_categories(self, check) -> None:
        """Run *check* over one codepoint per Unicode general category, and ASSERT the
        coverage instead of quietly skipping what did not fit.

        A category can be unreachable HERE for one reason only — this machine cannot hold a
        directory named with it. APFS validates a name against its own Unicode version and
        answers EILSEQ for an unassigned codepoint, where ext4 takes any byte sequence; and
        a lone surrogate has no UTF-8 encoding at all, so Python refuses it before the
        syscall. The reachable set is therefore a property of the machine, and the number is
        asserted so a run that silently covered two categories fails.

        Nothing is left unchecked by that. `TestReadableIsDecidedNotEnumerated` asks the
        same question of all 1,114,112 codepoints with no filesystem in the way; these
        sweeps are here to prove the REPORT is wired to it, which is #498's actual shape —
        `persona.lint` bounded its message while the row around it was built from the raw
        name.
        """
        samples = one_per_category(could_be_a_filename)
        self.assertGreater(len(samples), 20, "the sweep found almost no categories")
        refused: list[tuple[str, str]] = []
        covered = 0
        for cat, ch in sorted(samples.items()):
            name = ch * 3
            held, why = self.can_hold(name)
            if not held:
                refused.append((cat, why))
                continue
            with self.subTest(category=cat, cp=f"U+{ord(ch):04X}"):
                # Outside the try, deliberately. Wrapping `check` in one caught a genuine
                # assertion failure that happened to raise the same class as the
                # filesystem's refusal and recorded it as "this machine cannot hold the
                # name" — a green sweep hiding a red report.
                check(name)
                covered += 1
        self.assertGreater(covered, 20, f"only {covered} of {len(samples)} categories "
                                        f"reached the report; this machine refused "
                                        f"{refused}")

    def can_hold(self, name: str) -> tuple[bool, str]:
        """Can this machine hold a directory called *name*? Asked by creating one.

        The whole of what may be skipped, and the reason is recorded in the assertion above
        rather than swallowed. `UnicodeEncodeError` is caught alongside `OSError` because a
        lone surrogate has no UTF-8 encoding at all — Python refuses it before the kernel is
        asked, and "no filename on any machine" is the same answer as EILSEQ.
        """
        probe = config.PERSONAS_DIR / name
        try:
            probe.mkdir(parents=True)
        except (OSError, ValueError) as e:
            return False, getattr(e, "strerror", None) or str(e)
        shutil.rmtree(probe, ignore_errors=True)
        return True, ""

    def lint_output(self) -> str:
        err, out = io.StringIO(), io.StringIO()
        with redirect_stderr(err), redirect_stdout(out):
            commands_persona.cmd_persona_lint(SimpleNamespace(name=None, only=None))
        return err.getvalue() + out.getvalue()


class TestTheLintRowNamesItsPersona(ReportCase):
    """`charter persona lint`, whose every row ends in *go and fix this persona*."""

    def rows_about(self, name: str) -> list[str]:
        """The per-persona rows that are not about the control standing beside the hostile
        one. A control is declared alongside so "the bad name is not on a row" cannot pass
        against a lint that printed nothing at all."""
        self.reset()
        self.make_persona("control", role="Control", vault="none")
        self.make_named(name)
        lines = [ln for ln in self.lint_output().splitlines() if ln.strip()]
        self.assertTrue(any("control" in ln for ln in lines), lines)
        # `":" in ln` drops the trailing "N error(s) — …" summary, which is charter's own
        # sentence and not a row about anybody.
        return [ln for ln in lines if ":" in ln and "control" not in ln]

    def test_the_row_names_a_persona_that_renders_as_nothing(self):
        for label, ch in BLANK_RENDERING.items():
            with self.subTest(codepoint=label):
                rows = self.rows_about(ch * 3)
                self.assertTrue(rows, "lint said nothing about an unloadable persona")
                for row in rows:
                    named = named_in(row)
                    self.assertTrue(named, f"row names no persona: {row!r}")
                    self.assertTrue(printable_ascii(named), repr(named))
                    self.assertIn(f"{ord(ch):04x}", named.lower())

    def test_every_category_of_codepoint_reaches_a_row_that_names_something(self):
        def check(name: str) -> None:
            rows = self.rows_about(name)
            self.assertTrue(rows, "lint said nothing about an unloadable persona")
            for row in rows:
                named = named_in(row)
                self.assertTrue(named, f"row names no persona: {row!r}")
                self.assertTrue(printable_ascii(named), repr(named))

        self.sweep_categories(check)

    def test_an_ordinary_persona_keeps_its_own_name_on_the_row(self):
        """The control that would catch an over-eager fix: `devops` is `devops`."""
        self.reset()
        self.make_persona("devops", role="Ops", vault="none")
        out = self.lint_output()
        rows = [ln for ln in out.splitlines() if ":" in ln and ln.strip()]
        self.assertTrue(rows, out)
        for row in rows:
            self.assertEqual(named_in(row), "devops", row)
        self.assertNotIn("\\u", out)


class TestTheDoesNotLoadSentenceNamesItsPersona(ReportCase):
    """`persona '<name>' does not load` — the sentence whose whole job is to say which."""

    def sentence_for(self, name: str) -> str:
        self.reset()
        self.make_named(name)
        msgs = [m for lvl, m in persona.lint(name)
                if lvl == "error" and "does not load" in m]
        self.assertEqual(len(msgs), 1, msgs)
        return msgs[0]

    def test_the_sentence_contains_the_name(self):
        for label, ch in BLANK_RENDERING.items():
            with self.subTest(codepoint=label):
                named = self.sentence_for(ch * 3).split("'", 2)[1]
                self.assertTrue(named, "the sentence names no persona")
                self.assertTrue(printable_ascii(named), repr(named))
                self.assertIn(f"{ord(ch):04x}", named.lower())

    def test_every_category_reaches_a_sentence_that_names_something(self):
        def check(name: str) -> None:
            named = self.sentence_for(name).split("'", 2)[1]
            self.assertTrue(named, "the sentence names no persona")
            self.assertTrue(printable_ascii(named), repr(named))

        self.sweep_categories(check)

    def test_a_loadable_persona_says_nothing_at_all(self):
        """The complaint has to be caused by the name, not by having a persona."""
        self.reset()
        self.make_persona("devops", role="Ops", vault="none")
        self.assertEqual([m for _l, m in persona.lint("devops") if "does not load" in m], [])


class TestTheBriefNamesTheScriptItTellsTheAgentToRun(ReportCase):
    """The `bin/` bullets in the generated sub-agent brief — the one document written for
    the model, telling it to run these by path."""

    def brief(self, script: str) -> str:
        self.reset()
        self.make_persona("seo", role="SEO", vault="none")
        d = persona.bin_dir("seo")
        d.mkdir(parents=True, exist_ok=True)
        f = d / script
        f.write_text("#!/bin/sh\necho hi\n")
        f.chmod(0o755)
        got = persona.load("seo")
        return commands_persona._render_agent("seo", got["meta"], got["charter"])

    def bullets(self, text: str) -> list[str]:
        return [ln for ln in text.splitlines() if ln.strip().startswith("- `personas/")]

    def test_the_bullet_names_the_script_and_not_just_its_directory(self):
        for label, ch in BLANK_RENDERING.items():
            with self.subTest(codepoint=label):
                bullets = self.bullets(self.brief(ch * 3))
                self.assertEqual(len(bullets), 1, bullets)
                path = bullets[0].split("`")[1]
                self.assertTrue(printable_ascii(path), repr(path))
                leaf = path.rsplit("/", 1)[-1]
                self.assertTrue(leaf.strip(), f"the bullet names a directory: {path!r}")
                self.assertIn(f"{ord(ch):04x}", leaf.lower())

    def test_every_category_reaches_a_bullet_that_names_a_file(self):
        def check(script: str) -> None:
            bullets = self.bullets(self.brief(script))
            self.assertEqual(len(bullets), 1, bullets)
            path = bullets[0].split("`")[1]
            self.assertTrue(printable_ascii(path), repr(path))
            self.assertTrue(path.rsplit("/", 1)[-1].strip(),
                            f"the bullet names a directory: {path!r}")

        self.sweep_categories(check)

    def test_a_long_path_gets_the_PATH_budget_and_not_the_display_one(self):
        """Which constant this call site passes is not a detail, and nothing pinned it.

        `DISPLAY_LIMIT` is 160 — wide enough for a name, and narrower than a plane's real
        paths. Swapping the two here clips a long script path to 160 characters and the
        agent is told to run something that is not a path at all, which is this issue's own
        failure with a different cause. Found by hand-checking a constant swap the deletion
        sweep has no operator for (#569).
        """
        deep = "d" * 200
        text = self.brief(deep + ".sh")
        bullets = self.bullets(text)
        self.assertEqual(len(bullets), 1, bullets)
        path = bullets[0].split("`")[1]
        self.assertTrue(path.endswith(deep + ".sh"), f"clipped: {path[-40:]!r}")
        self.assertGreater(len(path), contain.DISPLAY_LIMIT)
        self.assertLessEqual(len(path), contain.PATH_DISPLAY_LIMIT)

    def test_an_ordinary_script_reaches_the_brief_unchanged(self):
        """The control: an agent told to run `site-health.sh` is told exactly that."""
        text = self.brief("site-health.sh")
        self.assertIn("`personas/seo/bin/site-health.sh`", text)
        self.assertNotIn("\\u", text)

    def test_the_bullet_is_still_one_bullet(self):
        """#453's own property on this surface, unchanged: a separator in a filename must
        not write a second bullet formatted like charter's own."""
        for sep in ("\u2028", "\u2029", "\x85", "\v"):
            with self.subTest(sep=repr(sep)):
                self.assertEqual(len(self.bullets(self.brief(f"a{sep}- `evil`"))), 1)


if __name__ == "__main__":
    unittest.main()
