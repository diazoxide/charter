"""#457: every surface that renders `charter {version}` for a human also renders the
channel beside it.

`statusline._brand` had the `dev` chip (#454); `frame/slots.py:_top` did not — it never
imported `channel`, or anything that does, at all. #386 made a frame SUPPRESS the status
line, so the two gaps stacked: inside a frame on the dev channel there was no indication
of the channel anywhere on screen. Neither #454 nor #386 could have caught this alone,
because each only tests the surface it touched.

**The fix is one function, `statusline._dev_chip()`, called from both places** — not two
copies of `channel.is_dev()` and its try/except. `_brand` and `_top` differ in
punctuation and colour around it but not in the one fact that matters: whether this
process's plane is on the dev channel. See `_dev_chip`'s own docstring, and `_brand`'s
(which it defers to), for why that is a CHANNEL fact and not a build fact.

**The property test, and where it stops.** `EveryCharterVersionIdiomShowsTheChannel`
below walks the whole package's AST (the same technique
`tests/test_self_relaunch_argv.py`'s `_hand_built_relaunch_argvs` already established in
this suite, for the identical reason a grep would not do) for the exact idiom `_brand`
and `_top` share: the bare name `__version__`, interpolated with the literal word
`charter` glued directly in front of it. That is deliberately narrower than "every place
`__version__` appears" — this package interpolates it in a dozen other places
(`report.py`'s JSON export, `hooks.py`'s and `doctor.py`'s pin/plugin-drift warnings,
`statusline._alerts`'s pinned-version alert) that are answering "does what's running
match what's declared", not "which channel is this plane on" — a different question `_brand`'s docstring is not
making a claim about. A scanner broad enough to flag all of those too would have to
judge INTENT, which no syntax-level check can do honestly; a scanner narrowed by a
hand-picked exclusion list for each of them would just be today's two-item allowlist
wearing a longer coat. The "charter {version}" idiom is not a guess at that boundary —
it is the literal shape of the bug: `_top`'s line was `f"... charter {__version__} "`
with nothing else in the expression naming a channel at all, and every one of the
excluded sites above fails that literal test on its own text (see each one's inline
comment in `_charter_version_idiom_sites`'s docstring). One real site DOES match the
idiom and wasn't fixed by #457: `commands_report._warn_if_stale` — filed as #458 rather
than folded into that PR (out of its stated scope) or silently exempted, and carried for
one release as a named, sourced `_KNOWN_GAPS` entry. #458 has since landed; the entry is
gone and `test_the_staleness_nudge_calls_the_chip_too` asserts the chip directly, which
is exactly the hand-off that entry was written to make possible.

Frame ids are `<workspace>-<launcher pid>` (`frame/state.py:frame_id`), and pid 1 is
`launchd` — permanently alive on this machine. `_top` does not consult liveness at all
(`verbosity` only reads `state.density`), so no fixture below needs a dead pid; `_fid()`
still builds one from `os.getpid()` rather than a literal `-1`, so a test copied
somewhere that DOES check liveness inherits a safe fixture rather than an unfailable one.
"""

from __future__ import annotations

import ast
import os
import unittest
from pathlib import Path
from unittest import mock

import charter
from charter import config, statusline
from charter.frame import slots

from tests._isolation import PersonaIso

#: The package this test process actually imported — not a path guessed from
#: `__file__`'s neighbours, matching `tests/test_self_relaunch_argv.py`'s own `_PKG`.
_PKG = Path(charter.__file__).resolve().parent


def _fid() -> str:
    return f"vshow-{os.getpid()}"


class DevChipUnit(unittest.TestCase):
    """`statusline._dev_chip()` in isolation: the one fact both surfaces defer to."""

    def test_dev_channel_renders_the_word_dev(self):
        with mock.patch.object(config, "UPDATE", {"channel": "dev"}):
            self.assertIn("dev", statusline._dev_chip())

    def test_stable_channel_renders_nothing(self):
        with mock.patch.object(config, "UPDATE", {"channel": "stable"}):
            self.assertEqual(statusline._dev_chip(), "")

    def test_a_channel_lookup_failure_renders_nothing_rather_than_raising(self):
        """`_brand`/`_top` both call this with no guard of their own around it — the same
        trust `_right` places in `_persona_chips`'s internal swallow. If `_dev_chip` ever
        let an exception through, both callers would need a try/except apiece, which is
        exactly the duplication this helper exists to remove."""
        with mock.patch("charter.channel.is_dev", side_effect=RuntimeError("boom")):
            self.assertEqual(statusline._dev_chip(), "")


class BrandCallsTheSharedChipRatherThanReassemblingIt(unittest.TestCase):
    """`_brand` used to derive `channel.is_dev()` and the ``if dev:`` branch itself. A
    fix to what the chip says must land in both surfaces the moment it lands here —
    pinned by handing `_dev_chip` a value nothing in `_brand` could have produced on its
    own, the same technique `RightRenderer.test_calls_persona_chips_rather_than_
    reassembling_it` already uses for `_persona_chips`."""

    def test_the_chips_return_value_reaches_the_rendered_line_untouched(self):
        with mock.patch("charter.statusline._dev_chip",
                        return_value=" SENTINEL-CHIP-0xF00D"), \
             mock.patch("charter.update.maybe_spawn", lambda: None), \
             mock.patch("charter.update.newer_than", lambda v: None):
            out = statusline._brand()
        self.assertIn("SENTINEL-CHIP-0xF00D", out)


class TopRendersTheChannelBesideTheVersion(PersonaIso, unittest.TestCase):
    """`frame/slots.py:_top` — the surface #457 is actually about. Same behaviour
    `_brand` already had (see `tests/test_dev_channel.py`'s
    `TheRenderPathNeverReachesTheNetwork`), now pinned on the frame's own renderer."""

    def setUp(self):
        super().setUp()
        self.fid = _fid()

    def test_a_dev_plane_shows_dev_beside_the_version(self):
        from charter import __version__
        with mock.patch.object(config, "UPDATE", {"channel": "dev"}):
            out = slots.render("top", self.fid)
        self.assertIn(__version__, out)
        self.assertIn("dev", out)

    def test_a_stable_plane_shows_no_dev_chip(self):
        with mock.patch.object(config, "UPDATE", {"channel": "stable"}):
            out = slots.render("top", self.fid)
        self.assertNotIn("dev", out)

    def test_top_calls_the_shared_chip_rather_than_reassembling_it(self):
        with mock.patch("charter.statusline._dev_chip",
                        return_value=" SENTINEL-CHIP-0xF00D"):
            out = slots.render("top", self.fid)
        self.assertIn("SENTINEL-CHIP-0xF00D", out)

    def test_a_chip_lookup_failure_still_yields_a_line_rather_than_an_exception(self):
        """`_top` carries no guard of its own around the call — the same trust `_brand`
        and `_right` already place in the helpers they call (`_dev_chip`'s own docstring
        promises never to raise; this is the consumer side of that promise)."""
        with mock.patch("charter.channel.is_dev", side_effect=RuntimeError("boom")):
            out = slots.render("top", self.fid)
        self.assertIn("charter", out)


# --------------------------------------------------------------------------- #
# the whole-tree property                                                     #
# --------------------------------------------------------------------------- #
def _calls_dev_chip(expr: ast.AST) -> bool:
    """True if *expr* contains a call recognisable as the shared helper — bare
    `_dev_chip()`, or qualified as `statusline._dev_chip()` / any other attribute access
    ending in that name."""
    return any(
        (isinstance(n, ast.Attribute) and n.attr == "_dev_chip")
        or (isinstance(n, ast.Name) and n.id == "_dev_chip")
        for n in ast.walk(expr))


def _charter_version_idiom_sites(pkg_root: Path) -> list[tuple[str, int, str, bool]]:
    """Every f-string in the package that glues the literal word ``"charter"`` directly
    onto the BARE name ``__version__`` — the exact idiom `_brand` and `_top` both had —
    together with whether that SAME f-string also calls `_dev_chip()`.

    An AST walk rather than a grep, for the reason `_hand_built_relaunch_argvs` in
    `tests/test_self_relaunch_argv.py` already gives for the identical choice: a line
    break, a different quote style, or the word appearing in a docstring (this module's
    own, for instance) defeats a regex. The AST sees the shape however it is spelled or
    wrapped, and sees nothing in prose.

    Two structural choices keep this from also flagging every other site that happens to
    mention `__version__`:

    * The interpolated value must be the BARE name, not a call — so `charter --version`'s
      own line (`cli.py`: ``f"charter {channel.build_label()}"``) never matches. That
      command's whole job is naming the exact build, which is deliberately NOT this
      chip's job (see `_brand`'s docstring, and the issue this test guards).
    * The literal text immediately before it, in the SAME joined string, must end with
      ``"charter"``. This is what excludes the drift/staleness diagnostics that also
      mention both words: `hooks.py`'s pin-conflict messages read "Working on
      {__version__}" (not "charter"); `doctor.py`'s stale-plugin warning reads "charter
      is {__version__}" (the word "is" sits between them); `statusline._alerts`'s
      pinned-version alert glues an ANSI reset between "charter" and the version
      (``{_DIM}charter{_R} {__version__}``), which is a SEPARATE constant, not text
      ending in "charter". None of those three needs to change for this test to pass —
      they are a different property from the one #457 is about, not an oversight here.

    Returns ``(file, line, source, has_chip)`` tuples so a failure names the exact site.
    """
    sites = []
    for path in sorted(pkg_root.rglob("*.py")):
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if not isinstance(node, ast.JoinedStr):
                continue
            values = node.values
            for i, v in enumerate(values):
                if not (isinstance(v, ast.FormattedValue)
                        and isinstance(v.value, ast.Name)
                        and v.value.id == "__version__"):
                    continue
                prev = values[i - 1] if i > 0 else None
                if not (isinstance(prev, ast.Constant) and isinstance(prev.value, str)
                        and prev.value.rstrip().endswith("charter")):
                    continue
                has_chip = any(
                    isinstance(w, ast.FormattedValue) and _calls_dev_chip(w.value)
                    for w in values)
                sites.append((str(path.relative_to(pkg_root.parent)), node.lineno,
                              ast.unparse(node), has_chip))
    return sites


#: Sites the scanner finds that are deliberately exempt — **empty**, and the machinery is
#: kept for the next one rather than deleted with it.
#:
#: It held exactly one entry, `commands_report._warn_if_stale`: the same idiom and the same
#: missing chip, left out of #457 as out of scope and filed upstream as #458 rather than
#: silently exempted. #458 has landed, so the entry is gone and
#: `test_the_staleness_nudge_calls_the_chip_too` below asserts the fix directly — which is
#: what the entry's own docstring promised would happen ("implement the chip, delete the
#: line, and the property test itself proves the fix").
#:
#: The shape, for whoever adds the next one: keyed by file rather than line, so a routine
#: edit elsewhere in the file doesn't rot the entry, and paired with the exact source
#: substring so a DIFFERENT, unrelated idiom landing in the same file later is not silently
#: swallowed by this one's exemption. An entry with no upstream issue behind it is just a
#: two-item allowlist wearing a longer coat.
_KNOWN_GAPS: dict[str, str] = {}


class EveryCharterVersionIdiomShowsTheChannel(unittest.TestCase):
    """The property `unimplemented()` models for slots: a NEW site is covered on the
    day it is written, not the day someone remembers to add it to a list."""

    def test_the_scanner_finds_both_fixed_sites(self):
        """First, that the detector is not vacuous — `_brand` and `_top` are the two
        sites #457 exists because of, so a scanner that found neither would prove
        nothing by finding zero offenders below."""
        sites = _charter_version_idiom_sites(_PKG)
        found = {f for f, _ln, _src, _chip in sites}
        self.assertIn("charter/statusline.py", found)
        self.assertIn("charter/frame/slots.py", found)

    def test_both_fixed_sites_call_the_chip(self):
        sites = _charter_version_idiom_sites(_PKG)
        by_file = {f: chip for f, _ln, _src, chip in sites}
        self.assertTrue(by_file.get("charter/statusline.py"),
                        "_brand's idiom no longer calls _dev_chip")
        self.assertTrue(by_file.get("charter/frame/slots.py"),
                        "_top's idiom no longer calls _dev_chip")

    def test_the_staleness_nudge_calls_the_chip_too(self):
        """The third site, and the one that was carried as a `_KNOWN_GAPS` entry until
        #458 landed. Named here rather than left to the sweep below, because "no offenders"
        would also be true of a file the scanner had stopped finding at all."""
        sites = _charter_version_idiom_sites(_PKG)
        by_file = {f: chip for f, _ln, _src, chip in sites}
        self.assertIn("charter/commands_report.py", by_file,
                      "the staleness nudge no longer matches the idiom at all — if it was "
                      "rewritten, say so here rather than losing the coverage silently")
        self.assertTrue(by_file["charter/commands_report.py"],
                        "_warn_if_stale's idiom no longer calls _dev_chip (#458)")

    def test_no_charter_version_idiom_anywhere_skips_the_channel(self):
        """The rule: every site matching the idiom calls `_dev_chip`, except the
        documented, sourced, upstream-filed exceptions in `_KNOWN_GAPS`. A NEW site
        that glues "charter" onto the bare version and forgets the chip — the exact
        shape of #457 itself — fails here on the day it is written."""
        sites = _charter_version_idiom_sites(_PKG)
        offenders = []
        for f, line, src, has_chip in sites:
            if has_chip:
                continue
            gap = _KNOWN_GAPS.get(f)
            if gap is not None and gap in src:
                continue
            offenders.append((f, line, src))
        self.assertEqual(
            offenders, [],
            "a `charter {version}` idiom with no channel chip beside it — call "
            "`statusline._dev_chip()` in the same f-string, or add a sourced, "
            "upstream-filed entry to _KNOWN_GAPS if this one is a different "
            "property (#457's test module explains the distinction):\n" +
            "\n".join(f"  {f}:{line}  {src}" for f, line, src in offenders))

    def test_a_known_gap_still_matches_the_source_it_was_filed_against(self):
        """The other direction, for whatever `_KNOWN_GAPS` holds: when the filed issue
        lands and the site starts calling `_dev_chip`, the exemption stops being needed —
        and this is what notices, rather than the exemption quietly outliving its reason
        and starting to shadow a real, different regression in the same file. That is not
        hypothetical: it is exactly how the one entry this ever held, `_warn_if_stale`,
        left — #458 landed and this test said so. Asserts nothing while the dict is empty,
        and goes live again with the next entry."""
        sites = _charter_version_idiom_sites(_PKG)
        by_file = {f: (has_chip, src) for f, _ln, src, has_chip in sites}
        for f, gap in _KNOWN_GAPS.items():
            with self.subTest(file=f):
                self.assertIn(f, by_file, f"{f} no longer has ANY charter-version "
                             "idiom — the _KNOWN_GAPS entry is stale, remove it")
                has_chip, src = by_file[f]
                self.assertIn(gap, src, f"{f}'s idiom no longer reads {gap!r} — "
                             "the _KNOWN_GAPS entry no longer matches what it was "
                             "filed against, update or remove it")
                self.assertFalse(has_chip, f"{f} now calls _dev_chip — the issue this "
                                "entry was filed against is fixed, delete the entry")


if __name__ == "__main__":
    unittest.main()
