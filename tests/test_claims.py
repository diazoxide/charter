"""The vault claim has to survive being read by someone who stops reading (#444).

charter's headline promise was *"the model never sees the value"*, and it is false in a
default configuration by charter's own documented commands: `secret exec … -- sh -c
'printf %s "$T" | base64'` returns the value through the **redacting** path, because
redaction is `str.replace` on captured bytes and a transform is not the value. `--exec`
and `--stream` capture nothing and so redact nothing, and `charter/secrets/base.py`
already calls redaction *"a defence-in-depth net"*.

The defect was never the mechanism — it was that **the strength of the claim rose as the
reader's ability to check it fell**. `docs/secrets.md` said it correctly, on the page you
reach after going looking. The README said it unqualified. `skills/secrets/SKILL.md` said
it most strongly of all to the one reader who cannot go and check: the model.

So these are prose tests, and they are deliberately crude. They cannot tell whether a
sentence is *true*; what they can tell is whether a sentence promises something absolute
about where a secret ends up while the clause that bounds it is nowhere near. That is the
shape of every instance of this defect, and it is the check that would have caught all
four of them.

The unit is the sentence, not the page — see `WINDOW`'s note for why the audit's "within
N lines" was too kind to the wording it was written to catch.

Two things keep it from being unfailable. `TestTheDetectorFires` runs the historical
wording — the actual sentences that shipped — back through the detector and fails if it
does not flag them, so a rule loosened until nothing trips it stops passing. And the
qualifier vocabulary is deliberately about *this* limit (capture, transform, whose command
it is); "guard rails, not guarantees" two paragraphs down is a different limit and does
not count as covering this one.

Persona charters and `.claude/agents/*` are out of scope on purpose: those are a plane's
own content, generated once and then owned by whoever runs the plane, not the claim
surface charter ships. `docs/superpowers/` is out of scope because the security assessment
quotes the wrong wording verbatim in order to correct it.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

#: The prose charter ships as its own claim about the vault. Everything under
#: `docs/superpowers/` is excluded — the assessment quotes the defect to fix it.
def _scope() -> list[Path]:
    files = [ROOT / "README.md", ROOT / "SECURITY.md"]
    for base in ("docs", "skills"):
        files += sorted(p for p in (ROOT / base).rglob("*.md")
                        if "superpowers" not in p.parts)
    return [p for p in files if p.is_file()]


#: An unbounded promise: "never", "cannot", "in every case" — the words that turn a
#: property of charter's own process into a property of the world.
_ABSOLUTE = re.compile(r"\b(never|cannot|can't|no step|in every case|always)\b", re.I)
_SUBJECT = re.compile(r"\b(secret|credential|password|token|value)s?\b", re.I)
#: …about where the value ends up. "context" on its own is a column heading somewhere in
#: these docs, so the exposure has to be named as a place a reader would see it.
_EXPOSURE = re.compile(r"(context window|transcript|conversation|later prompt|summary|"
                       r"never sees|never see|see the value|sees the value)", re.I)
#: The actor the guarantee is a property of. This is the whole correction: charter can
#: promise what *charter* prints and nothing more, so a sentence that makes the promise
#: without naming who keeps it is making it about the world.
_ACTOR = re.compile(r"\bcharter\b", re.I)

#: Redaction claimed as total — "in every case", "every occurrence", "from whatever that
#: command prints" — or claimed to make a leak impossible. Both are false for a transform
#: and for `--exec`/`--stream`.
#:
#: The quantifier has to be attached to the redaction, not merely present in the sentence:
#: "every consuming path (--env, --file, --dotenv, redaction) already worked" is a release
#: note about coverage of *paths*, and a rule that cannot tell those apart gets deleted by
#: the third person it annoys.
_TOTAL_REDACTION = re.compile(
    r"(in every case\b.{0,140}?\b(redact|scrub|mask)"
    r"|(redact|scrub|mask)\w*\s+(it\s+)?(from\s+)?(every|whatever|all)"
    r"|every occurrence)", re.I)
#: "cannot leak into the transcript" — impossibility, with the place named. The place is
#: required: "it reaches the API through the same bridge" is a sentence about plumbing.
_IMPOSSIBLE = re.compile(
    r"\b(cannot|can't|never)\b(?:\W+\w+){0,4}?\W+(leak|escape|reach|end up)\w*"
    r"(?:\W+\w+){0,3}?\W+(transcript|context|conversation|prompt|summary)", re.I)
#: The one word that makes a redaction claim true: it covers what charter *captured*.
_CAPTURED = re.compile(r"\bcaptur", re.I)

#: Clauses that bound *this* limit. A guard-rail disclaimer about a different mechanism
#: does not qualify a redaction promise, so those words are not in here.
_QUALIFIERS = (
    "capture nothing", "captures nothing", "capturing nothing",
    "redact nothing", "redacts nothing",
    "not a boundary", "defence-in-depth net",
    "transform", "transforms", "transforming",
    "chose the command", "chooses the command", "choose the command",
    "chosen on purpose",
    "command you hand it to", "command you asked charter to run",
    "command you chose", "command charter hands it to",
    "by accident", "accidental", "accidentally",
    "goes wherever", "server's own business", "command's business",
)
#: Not in the list, and the reason is worth a line: "still can" — the last two words of
#: the README's corrected sentence — is a prefix of "still cannot", which is the last
#: clause of the worst sentence in the repo. Left in, it silently exempted the very
#: wording this file exists to catch, and `TestTheDetectorFires` is what noticed.

#: The unit of judgement is the **sentence**, not the paragraph or the page.
#:
#: The audit's sketch allowed the bound to sit within N lines of the promise. That is too
#: kind, and README.md is the proof: the corrected paragraph above line 201 would have
#: rescued *"the model never sees the value"* sitting six lines below it, and that sentence
#: is not unqualified — it is false. A sentence is the unit that gets quoted, pasted into a
#: release post, and read by a model whose attention stopped there.


def _plain(text: str) -> str:
    """Markdown emphasis removed, so `**never**` and `*never*` read as `never`."""
    return text.replace("*", "").replace("`", "").replace("_", " ")


#: A double-quoted span is somebody else's words. This file polices the claims charter
#: makes, not the ones it quotes in order to withdraw them — the news entry for #444 has
#: to be able to print the sentence it is retracting, and so does the next one. The
#: loophole is real and small: a live promise dressed as a quotation gets past this. A
#: retraction reads as a retraction to a human, which is the reader being protected.
_QUOTED = re.compile(r"[\"\u201c][^\"\u201c\u201d]*[\"\u201d]")


def _unquoted(sentence: str) -> str:
    return _QUOTED.sub(" ", sentence)


def _blocks(path: Path) -> list[tuple[int, str]]:
    """(line number, block) — paragraphs, list items and mermaid labels; code dropped.

    Blocks first, then sentences inside them. A naive split on `.` runs a heading, a
    shell example and the paragraph after it into one 400-character "sentence" that
    matches every pattern in this file and means nothing.
    """
    out, cur, start, fenced, diagram = [], [], 0, False, False
    for n, raw in enumerate(path.read_text().splitlines(), start=1):
        if raw.lstrip().startswith("```"):
            diagram = not fenced and "mermaid" in raw.lower()
            fenced = not fenced
            raw = ""
        elif fenced and not diagram:
            continue
        elif fenced:
            # A mermaid `Note over` is prose on the rendered page — and the README's
            # was one of the four wrong claims. Each label stands alone.
            raw = raw.strip()
            if cur:
                out.append((start, " ".join(cur)))
                cur = []
            if raw:
                out.append((n, _plain(raw)))
            continue
        line = _plain(raw).strip()
        breaks = (not line or line.startswith(("#", "- ", "* ", "> ", "|"))
                  or re.match(r"\d+[.)] ", line))
        if breaks and cur:
            out.append((start, " ".join(cur)))
            cur = []
        if line:
            if not cur:
                start = n
            cur.append(line)
    if cur:
        out.append((start, " ".join(cur)))
    return out


def _sentences(path: Path) -> list[tuple[int, str]]:
    """(line number, sentence). The line is the block's first — close enough for a
    ±WINDOW look-around, and the claim that shipped longest wrapped across two lines,
    so a per-line grep would have missed it entirely."""
    out = []
    for line, block in _blocks(path):
        # Quotes are removed before the split, not after: a quoted sentence ends
        # `transcript."` — period inside — so a per-sentence strip sees an unterminated
        # span and leaves the retracted wording in place, which is how this file first
        # failed on its own news entry.
        for m in re.finditer(r"[^.!?]+[.!?]*", _unquoted(block)):
            s = m.group().strip()
            if s:
                out.append((line, s))
    return out


def unqualified_promise(sentence: str) -> bool:
    """An absolute claim that a value does not reach a reader, made about the world:
    no actor whose behaviour it is, and no clause bounding it."""
    return bool(_ABSOLUTE.search(sentence)
                and _SUBJECT.search(sentence)
                and _EXPOSURE.search(sentence)
                and not _ACTOR.search(sentence)
                and not any(q in sentence.lower() for q in _QUALIFIERS))


def total_redaction(sentence: str) -> bool:
    """Redaction claimed over every output, or claimed to make a leak impossible.

    Naming charter does not rescue this one: redaction is `str.replace` over captured
    bytes whoever is speaking, so "cannot leak" is false for `base64` either way.
    """
    if _CAPTURED.search(sentence) or any(q in sentence.lower()
                                         for q in _QUALIFIERS):
        return False
    if not _SUBJECT.search(sentence):
        return False
    return bool(_TOTAL_REDACTION.search(sentence) or _IMPOSSIBLE.search(sentence))


class TestTheVaultClaimIsQualifiedWhereverItAppears(unittest.TestCase):
    def test_no_unqualified_promise_stands_alone(self):
        """The promise may be made — it is charter's whole point — but not on its own.

        Failure prints the file, the block's line and the sentence. What to do about it is
        not to delete the sentence: name charter as the one keeping the promise, or add
        the clause that says whose command decides where the value goes."""
        for path in _scope():
            for line, sentence in _sentences(path):
                with self.subTest(file=path.relative_to(ROOT).as_posix(), line=line):
                    self.assertFalse(
                        unqualified_promise(sentence),
                        f"{path.relative_to(ROOT)}:{line} promises that a value does not "
                        f"reach a reader, names nobody who keeps that promise, and "
                        f"carries no bound: {sentence!r}")

    def test_no_page_claims_redaction_is_total(self):
        """Redaction is `str.replace` over captured bytes. A sentence that says it covers
        every output, or that a value therefore cannot leak, is false for `base64`, for
        `rev`, and for `--exec`/`--stream`, which capture nothing at all."""
        for path in _scope():
            for line, sentence in _sentences(path):
                with self.subTest(file=path.relative_to(ROOT).as_posix(), line=line):
                    self.assertFalse(
                        total_redaction(sentence),
                        f"{path.relative_to(ROOT)}:{line} claims redaction covers more "
                        f"than captured output: {sentence!r}")


class TestEveryClaimSurfaceCarriesTheLimit(unittest.TestCase):
    """Absence of a false sentence is not presence of a true one. Each page that sells
    the vault has to say, on that page, what it does not do."""

    SURFACES = (
        "README.md",
        "SECURITY.md",
        "docs/secrets.md",
        "skills/secrets/SKILL.md",
        "skills/browser/SKILL.md",
    )

    def test_each_names_the_bound(self):
        for rel in self.SURFACES:
            with self.subTest(file=rel):
                text = _plain((ROOT / rel).read_text()).lower()
                self.assertTrue(any(q in text for q in _QUALIFIERS),
                                f"{rel} sells the vault and never names its limit")

    def test_the_model_facing_skill_names_the_uncaptured_paths(self):
        """`skills/secrets/SKILL.md` is loaded into the model's context and never
        mentioned `--exec` or `--stream`, while claiming "in every case". The model is the
        one reader that cannot go and check, so it gets the flags by name."""
        text = (ROOT / "skills" / "secrets" / "SKILL.md").read_text()
        for flag in ("--exec", "--stream"):
            self.assertIn(flag, text, f"the skill never names {flag}")

    def test_the_model_is_told_not_to_cp_to_a_device(self):
        """`secret cp <v> <k> /dev/stdout` prints the credential into the transcript and
        then prints "Value not shown." The CLI refusal is the fix; this is the rule the
        model reads before it gets there."""
        text = (ROOT / "skills" / "secrets" / "SKILL.md").read_text()
        self.assertIn("/dev/stdout", text)


class TestTheDetectorFires(unittest.TestCase):
    """The anti-vacuity half. These are the sentences charter actually shipped; if a
    loosened rule stops flagging them, this file stops passing rather than going quiet."""

    SHIPPED = (
        "The model names the secret; it never sees it.",
        "What every provider buys you is the same and it is the point — the model never "
        "sees the value.",
        "Note over M,A: no step here ever put the value in a context window",
        "the boundary is the same for all of them, and it is that the value never enters "
        "an agent's context or transcript.",
        "In every case the value is injected into the subprocess and redacted from its "
        "output, so a command that echoes it still cannot leak it into the transcript.",
        "substitutes the value and redacts it from output, so it never reaches the "
        "transcript.",
    )

    #: The corrected sentences. A rule that flags these too would push the docs back
    #: towards saying nothing, which is the other way to be useless.
    CORRECTED = (
        "charter never prints the value into the conversation. What the command you hand "
        "it to does with it is that command's business.",
        "charter never puts the value in an agent's context or transcript — the command "
        "charter hands it to still can.",
        "Charter injects the value into the subprocess and scrubs it from captured "
        "output, so a command that accidentally echoes it is masked.",
        "secret exec scrubs the value from captured output, so a curl -v that echoes an "
        "Authorization header is masked.",
    )

    def test_every_shipped_sentence_is_caught(self):
        for sentence in self.SHIPPED:
            with self.subTest(sentence=sentence[:48]):
                self.assertTrue(
                    unqualified_promise(_plain(sentence))
                    or total_redaction(_plain(sentence)),
                    "the detector no longer flags a sentence that shipped")

    def test_the_corrected_sentences_pass(self):
        for sentence in self.CORRECTED:
            with self.subTest(sentence=sentence[:48]):
                self.assertFalse(unqualified_promise(_plain(sentence)))
                self.assertFalse(total_redaction(_plain(sentence)))

    def test_the_scope_is_not_empty(self):
        """A glob that matches nothing passes every assertion above it."""
        names = {p.relative_to(ROOT).as_posix() for p in _scope()}
        self.assertIn("README.md", names)
        self.assertIn("SECURITY.md", names)
        self.assertIn("skills/secrets/SKILL.md", names)
        self.assertGreater(len(names), 10)
        self.assertFalse([n for n in names if "superpowers" in n])


if __name__ == "__main__":
    unittest.main()
