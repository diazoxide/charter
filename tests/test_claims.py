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

The unit is the sentence, not the page — see the note above `_plain` for why the audit's
"within N lines" was too kind to the wording it was written to catch.

**What the second round changed, and why.** The first version of this file was handed to a
reviewer whose only instruction was to defeat it, and it fell twice.

It fell first on its own subject line. The rule short-circuited on
`not _ACTOR.search(sentence)` — *does the word "charter" appear anywhere in this sentence* —
so `Because charter resolves it in its own process, the model never sees the value.` passed:
the retracted claim, with one clause bolted on the front. So did *"With charter, the
password never appears in your transcript."* Close to a quarter of the sentences in
`_scope()` contain that word. A guard against overclaiming that anyone can silence by mentioning the
project's own name is not a guard, and it is the same failure as every other guard broken
that night: **it matched on a name instead of the identity of the thing it was protecting.**
So the question is now positional and asked of the clause the absolute word sits in
(`_promise_clause`, `_promise_is_charters`), the bound has to be in the *same sentence*
whoever is named, and a promise whose subject is the model or the value itself fails
whatever else the sentence says. The verb lists grew for the same reason: `stripped` and
`enters` were each one synonym away from the words the first draft knew.

It fell second on the sentence written to replace the false one. *"charter never prints the
value into the conversation"* went into `SECURITY.md`, the README, `docs/secrets.md`, the
news entry — and `charter secret cp <vault> <key> /dev/stdout` prints the value to stdout,
in charter's own process, with no child command anywhere in it, and then reports
`Value not shown.` (#421, #422). The replacement guarantee was false the same way the
original was, which is why `TestEveryClaimSurfaceCarriesTheLimit` now also requires every
page that makes the claim to *name the two commands that break it*. A guarantee without its
exceptions on the same page is the original defect with a longer sentence.

Three things keep this from being unfailable. `TestTheDetectorFires.SHIPPED` runs the
historical wording — the actual sentences that shipped — back through the detector.
`BYPASSED` runs what the reviewer got past round one, plus the next spellings found by
asking the same question of round two. And the qualifier vocabulary is deliberately about
*this* limit (capture, transform, whose command it is, which paths); "guard rails, not
guarantees" two paragraphs down is a different limit and does not count as covering this
one.

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
#: `nobody` and `nothing` are deliberately absent: "with nothing redacting it" is how
#: three pages *describe the leak they are warning about*, and a rule that flags a hazard
#: notice as an overclaim is a rule someone deletes. `no value`/`no secret` stay, because
#: "charter puts no value in a context window" is the promise wearing a different hat.
_ABSOLUTE = re.compile(
    r"\b(never|cannot|can't|impossible|at no point|in every case|always"
    r"|not one|not a single|never once"
    r"|no (?:step|value|secret|credential|password|token))\b",
    re.I)
_SUBJECT = re.compile(r"\b(secret|credential|password|token|value)s?\b", re.I)
#: …about where the value ends up. "context" on its own is a column heading somewhere in
#: these docs, so the exposure has to be named as a place a reader would see it.
_EXPOSURE = re.compile(r"(context window|transcript|conversation|later prompt|summary|"
                       r"never sees|never see|see the value|sees the value)", re.I)
#: The actor the guarantee can be a property of. Round one asked only whether this word
#: appeared *somewhere* in the sentence, and a reviewer walked through the gap in one
#: move: `Because charter resolves it in its own process, the model never sees the value.`
#: is README's retracted claim with a clause bolted on the front, and the substring test
#: exempted it. Close to a quarter of the sentences in `_scope()` contain the word, so
#: that fraction of the prose this file governs was auto-exempt. A guard against overclaiming that is
#: silenced by mentioning the project's own name is not a guard. Naming charter is now
#: *necessary and not sufficient*: see `_promise_is_charters` for the structural test, and
#: `_QUALIFIERS` for the bound that has to sit in the same sentence either way.
_ACTOR = re.compile(r"\bcharter\b", re.I)

#: The parties a promise about a secret can be *made about* — the reader it must not
#: reach, and the value itself. When one of these heads the clause the absolute word sits
#: in, the sentence is describing the world, and charter cannot underwrite the world: "the
#: model never sees the value", "the password never appears in your transcript", "no step
#: here ever put the value in a context window". No quantity of "charter" elsewhere in the
#: sentence changes what the clause is about, which is the entire round-two correction.
#:
#: An ordinary noun is not in here on purpose. "so an accidental `cat` cannot put a secret
#: in the transcript" (docs/git-policy.md) is a bounded, true sentence about a guard, and a
#: rule that cannot tell it from "the model never sees the value" is a rule that gets
#: deleted by the third person it annoys — so for those, the bound still decides.
_PROTECTED_SUBJECT = re.compile(
    r"\b(model|agent|assistant|llm|value|secret|credential|password|token"
    r"|step|nobody|no one|nothing|it|its|they|them|this|that|those|these)\b", re.I)

#: Where a clause may *begin*. Broad on purpose — this walks backwards from the absolute
#: word to find what the promise is about, and "…and it is that charter never…" has to
#: resolve to `charter` while "Because charter resolves it, the model never…" has to
#: resolve to `the model`.
_CLAUSE_START = re.compile(
    r"[,;:()\u2014\u2013]|\s--\s"
    r"|\b(?:because|since|so that|so|but|while|whilst|which|when|where|if|unless"
    r"|although|though|and|or|then|that|yet|whereas)\b", re.I)

#: Where a clause *ends*, walking forward. Deliberately narrower than `_CLAUSE_START`:
#: "charter never puts the value in an agent's context or transcript" is one promise, and
#: ending the clause at "or" would drop the exposure it names. Only a punctuation break or
#: a word that starts a genuinely new clause counts.
#:
#: `you'd` and friends end a clause too, and the reason is README.md:63 — *"Your agent
#: touches no credential you'd mind seeing in a transcript"*, a bullet under **You don't
#: need charter if**. The transcript there is inside a hypothetical about what the reader
#: would mind, not a claim about where a value goes, and a rule that cannot tell a
#: conditional from a guarantee teaches people to write vaguer conditionals.
_CLAUSE_END = re.compile(
    r"[;:()\u2014\u2013]|\s--\s"
    r"|\b(?:because|since|so that|so|but|while|whilst|which|when|where|if|unless"
    r"|although|though|whereas|you'd|you would|i'd|we'd|they'd)\b", re.I)

#: Redaction claimed as total — "in every case", "every occurrence", "from whatever that
#: command prints" — or claimed to make a leak impossible. Both are false for a transform
#: and for `--exec`/`--stream`.
#:
#: The quantifier has to be attached to the redaction, not merely present in the sentence:
#: "every consuming path (--env, --file, --dotenv, redaction) already worked" is a release
#: note about coverage of *paths*, and a rule that cannot tell those apart gets deleted by
#: the third person it annoys.
#:
#: The verb list is a list because the round-one one was three words long, and the
#: reviewer swapped in a fourth: *"The value is stripped from every output, in every case,
#: so a command that echoes it is safe."* A rule that a thesaurus defeats is a rule about
#: spelling. Anything that means "took the value out of the bytes" belongs here.
_REDACT_VERB = (r"redact|scrub|mask|strip|remove|filter|censor|sanitis|sanitiz|elid"
                r"|suppress|blank|obscur|hide|hidden|hides|withh|replace|swap out"
                r"|take out|takes out|taken out|keep out|kept out")
#:
#: The quantifier is allowed a few words of object between it and the verb — "charter
#: filters the secret **out of all** output" and "charter sanitizes the value **from
#: whatever** the command prints" are the same claim as "redacts it from every", and a
#: pattern that only knows the adjacent spelling is back to matching on a string.
_TOTAL_REDACTION = re.compile(
    r"(in every case\b.{0,140}?\b(" + _REDACT_VERB + r")"
    r"|(" + _REDACT_VERB + r")\w*(?:\W+\w+){0,4}?\W+(?:from|out of|off)\W+"
    r"(every|whatever|all|any|each)\b"
    r"|(" + _REDACT_VERB + r")\w*\s+(it\s+)?(from\s+)?(every|whatever|all|any)"
    r"|every occurrence)", re.I)
#: "cannot leak into the transcript" — impossibility, with the place named. The place is
#: required: "it reaches the API through the same bridge" is a sentence about plumbing.
#:
#: Same lesson as `_REDACT_VERB`: round one knew `leak|escape|reach|end up`, and
#: *"charter ensures the value never enters an agent's context or transcript"* got through
#: on the verb "enters". Every ordinary way of saying "arrives somewhere" is listed.
_ARRIVE_VERB = (r"leak|escape|reach|end up|enter|appear|show up|surface|land|arrive"
                r"|get into|gets into|make it|makes it|be seen|turn up|turns up"
                r"|be printed|be written|be logged|be recorded|cross|pass into|go into"
                r"|goes into|be exposed|be disclosed|be revealed")
_IMPOSSIBLE = re.compile(
    r"\b(cannot|can't|never|impossible|at no point)\b(?:\W+\w+){0,4}?\W+("
    + _ARRIVE_VERB + r")\w*"
    r"(?:\W+\w+){0,3}?\W+(transcript|context|conversation|prompt|summary"
    r"|chat|window|log|logs|history)", re.I)
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
    # The bound the second round added, because the first round's replacement sentence
    # was itself false: `charter secret cp <v> <k> /dev/stdout` is charter's own process
    # printing the value into this transcript, with no child command anywhere in it. What
    # is true is narrower — charter prints only where you named somewhere to print to.
    "destination you named", "destination you name", "destination you chose",
    "path you named", "path you name",
    "only where you ask", "only when you ask", "only where you tell it",
    "paths that consume it", "consuming path", "consuming paths",
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


#: A pronoun standing in for the noun in the sentence before it. Splitting one claim
#: across a full stop — "The value is resolved inside charter's process. It cannot leak
#: into the transcript." — put `_SUBJECT` in the first half and the promise in the
#: second, and a strictly per-sentence rule saw neither. Only the *antecedent* is
#: borrowed across the boundary; a bound sitting in a different sentence is precisely
#: what this file refuses to accept, so `_QUALIFIERS` and the actor are never inherited.
_PRONOUN = re.compile(r"\b(it|its|they|them|their|this|that|those|these|the same)\b",
                      re.I)


def _resolve_antecedent(sentence: str, earlier: str, later: str = "") -> str:
    """Lend a pronoun clause the nearest noun in its own paragraph, before or after.

    Backwards covers the split the reviewer used. Forwards covers the same split written
    the other way round — *"The transcript never sees it. charter holds the secret in its
    own process."* — where the promise lands before its own subject and a backwards-only
    search finds nothing at all.
    """
    if _SUBJECT.search(sentence) or not _PRONOUN.search(sentence):
        return sentence
    found = None
    for m in _SUBJECT.finditer(earlier):
        found = m.group()
    if found is None:
        m = _SUBJECT.search(later)
        found = m.group() if m else None
    return f"{found} {sentence}" if found else sentence


def _split(block: str) -> list[tuple[str, str]]:
    """(sentence as written, sentence as judged) for one block.

    Quotes are removed before the split, not after: a quoted sentence ends
    `transcript."` — period inside — so a per-sentence strip sees an unterminated span
    and leaves the retracted wording in place, which is how this file first failed on its
    own news entry.
    """
    out = []
    text = _unquoted(block)
    for m in re.finditer(r"[^.!?]+[.!?]*", text):
        s = m.group().strip()
        if s:
            out.append((s, _resolve_antecedent(s, text[:m.start()], text[m.end():])))
    return out


def flagged(block: str) -> list[str]:
    """The sentences in one block that either detector catches.

    The block, not the sentence, is the entry point, because a claim can be split across
    a full stop — the reviewer's *"The value is resolved inside charter's own process. It
    cannot leak into the transcript."* — and a case written as one string has to travel
    the same road the files do.
    """
    return [s for s, judged in _split(block)
            if unqualified_promise(judged) or total_redaction(judged)]


def _sentences(path: Path) -> list[tuple[int, str, str]]:
    """(line number, sentence as written, sentence as judged).

    The line is the block's first, not the sentence's: the claim that shipped longest
    wrapped across two lines, so a per-line grep would have missed it entirely. The judged
    form differs from the written one only by a borrowed antecedent; failures quote what is
    on the page.
    """
    return [(line, s, judged)
            for line, block in _blocks(path)
            for s, judged in _split(block)]


def _promise_clause(sentence: str, m: re.Match) -> tuple[str, str]:
    """(what precedes the absolute word in its own clause, the clause itself).

    Round one asked its three questions of the whole sentence, and
    `docs/personas.md:181` is what that costs: one 200-character sentence carrying
    "(never --reveal)" in one clause, "the parent conversation" in another and
    "credentials live" in a third matched all three patterns and meant nothing. The
    promise has to be somewhere, and it is in the clause the absolute word sits in.
    """
    start = 0
    for brk in _CLAUSE_START.finditer(sentence[:m.start()]):
        start = brk.end()
    nxt = _CLAUSE_END.search(sentence, m.end())
    end = nxt.start() if nxt else len(sentence)
    return sentence[start:m.start()], sentence[start:end]


def _promise_is_charters(head: str) -> bool:
    """Is *charter* the party this clause's promise is about?

    Round one asked `_ACTOR.search(sentence)` — does the word appear anywhere — and the
    reviewer rescued four of this file's own historical sentences by pasting "charter"
    into a subordinate clause. So the question is now positional: of what precedes the
    absolute word *inside its own clause*, is charter the last thing named?
    `Because charter resolves it in its own process, the model never sees the value.`
    resolves to *the model*; `…and it is that charter never puts the value…` resolves to
    *charter*; `charter ensures the value never enters…` resolves to *the value*, because
    that is what the promise is about however the sentence opens.
    """
    actor = None
    for a in _ACTOR.finditer(head):
        actor = a.start()
    if actor is None:
        return False
    return not any(f.start() > actor for f in _PROTECTED_SUBJECT.finditer(head))


def unqualified_promise(sentence: str) -> bool:
    """An absolute claim that a value does not reach a reader, and no bound on it.

    **The bound must be in this sentence, and naming charter is no longer a substitute
    for one.** Round one let the word "charter" exempt a sentence outright, and then
    wrote *"charter never prints the value into the conversation"* — which
    `charter secret cp <vault> <key> /dev/stdout` falsifies inside charter's own process,
    with no child command anywhere in it. Naming the actor was necessary and never
    sufficient.

    **And a promise about the model, or about the value itself, fails whatever else the
    sentence says** — those are claims about the world, and no bound charter can write
    makes them charter's to keep. Rewrite them with charter as the subject, which forces
    the sentence to say what charter actually does.
    """
    m = _ABSOLUTE.search(sentence)
    if not m:
        return False
    head, clause = _promise_clause(sentence, m)
    if not _EXPOSURE.search(clause):
        return False
    # A pronoun clause inherits its noun from earlier in the same sentence — "The model
    # names the secret; it never sees it." puts the subject in the first clause and the
    # promise in the second. The antecedent travels; the bound never does.
    if not _SUBJECT.search(_resolve_antecedent(clause, sentence[:m.start()])):
        return False
    if not _promise_is_charters(head):
        if _PROTECTED_SUBJECT.search(head) or not head.strip():
            return True
    return not any(q in sentence.lower() for q in _QUALIFIERS)


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
        not to delete the sentence and not to sprinkle "charter" into it — that was round
        one's escape hatch and it is closed. Say, in this sentence, which paths the
        promise holds on and what happens on the others."""
        for path in _scope():
            for line, sentence, judged in _sentences(path):
                with self.subTest(file=path.relative_to(ROOT).as_posix(), line=line):
                    self.assertFalse(
                        unqualified_promise(judged),
                        f"{path.relative_to(ROOT)}:{line} makes an absolute promise about "
                        f"where a value ends up, with no bound in the same sentence — or "
                        f"makes it about somebody other than charter: {sentence!r}")

    def test_no_page_claims_redaction_is_total(self):
        """Redaction is `str.replace` over captured bytes. A sentence that says it covers
        every output, or that a value therefore cannot leak, is false for `base64`, for
        `rev`, and for `--exec`/`--stream`, which capture nothing at all."""
        for path in _scope():
            for line, sentence, judged in _sentences(path):
                with self.subTest(file=path.relative_to(ROOT).as_posix(), line=line):
                    self.assertFalse(
                        total_redaction(judged),
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
        then prints "Value not shown." The CLI refusal is the fix and it is not merged
        (#421, #422); this is the rule the model reads until it is."""
        text = (ROOT / "skills" / "secrets" / "SKILL.md").read_text()
        self.assertIn("/dev/stdout", text)

    #: The pages that state the guarantee.
    PROMISING = ("README.md", "SECURITY.md", "docs/secrets.md", "docs/mcp.md")

    def test_a_page_that_promises_also_names_what_prints(self):
        """The round-one replacement said *charter never prints the value into the
        conversation* and named neither command that does.

        A guarantee is only as good as its exceptions, and a page carrying the guarantee
        without them is the same defect with a longer sentence — the reader who stops
        reading stops on the promise. `secret get --reveal` and `secret cp <dest>` print;
        both have to appear on any page that makes the claim, whatever the claim's
        current wording."""
        for rel in self.PROMISING:
            text = _plain((ROOT / rel).read_text())
            with self.subTest(file=rel):
                self.assertIn("--reveal", text, f"{rel} promises without naming --reveal")
                self.assertIn("secret cp", text, f"{rel} promises without naming secret cp")


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

    #: What round one's detector let through, verbatim from the review. Every one of these
    #: is a sentence charter already retracted with one clause bolted on, and the whole
    #: round-two correction is that none of them may pass again.
    #:
    #: The first four are the `_ACTOR` substring hole: one mention of the project's own
    #: name anywhere in the sentence short-circuited the rule. The fifth and sixth are
    #: vocabulary — a redaction synonym the verb list did not know, and an arrival verb it
    #: did not know. The seventh is a full stop: the subject in one sentence, the promise
    #: in the next, so a per-sentence rule saw neither half. The eighth is round one's own
    #: replacement guarantee, which `charter secret cp <vault> <key> /dev/stdout`
    #: falsifies in charter's own process.
    BYPASSED = (
        "Because charter resolves it in its own process, the model never sees the value.",
        "charter ensures the value never enters an agent's context or transcript.",
        "With charter, the password never appears in your transcript.",
        "no step here ever put the value in a context window, because charter runs it",
        "The value is stripped from every output, in every case, so a command that "
        "echoes it is safe.",
        "charter removes the secret from all output, so it can never appear in the "
        "transcript.",
        "The value is resolved inside charter's own process. It cannot leak into the "
        "transcript.",
        "charter never prints the value into the conversation.",
        # And the next spellings, found by asking the same question of the round-two
        # rule rather than waiting for the round-three reviewer to ask it.
        "Not one secret ever reaches the transcript.",
        "Never once does the password appear in the conversation.",
        "charter filters the secret out of all output, so it is safe.",
        "charter sanitizes the value from whatever the command prints.",
        "charter resolves the secret. charter strips it out of every output the command "
        "produces.",
        "The transcript never sees it. charter holds the secret in its own process.",
        # A bound in the sentence does not rescue a promise made about the model: this
        # one carries a real `_QUALIFIERS` clause and is still a claim about the world.
        "Because charter transforms nothing on this path, the model never sees the value.",
        # Impossibility named at a place `_EXPOSURE` does not list, with an arrival verb
        # round one did not know. Only `_IMPOSSIBLE` catches this, which is the point of
        # keeping it a separate rule from the promise one.
        "charter scrubs it, so the token can never appear in the session log.",
    )

    #: The corrected sentences. A rule that flags these too would push the docs back
    #: towards saying nothing, which is the other way to be useless.
    #:
    #: The first two are the wording that now ships, and the difference from round one is
    #: that the bound is *inside the sentence* rather than standing on the word "charter".
    #: The last two are honest prose from elsewhere in the docs — a guard note and a
    #: front-page conditional — that a tighter rule must not sweep up.
    CORRECTED = (
        "On the paths that consume a value — secret exec with --env/--file/--dotenv, and "
        "the MCP launcher — charter never prints the value into the conversation, and "
        "everywhere else charter prints it only into a destination you named yourself.",
        "charter never puts the value in an agent's context or transcript on the paths "
        "that consume it — secret exec, --dotenv, MCP — while secret get --reveal and "
        "secret cp <dest> print it into the destination you named.",
        "Charter injects the value into the subprocess and scrubs it from captured "
        "output, so a command that accidentally echoes it is masked.",
        "secret exec scrubs the value from captured output, so a curl -v that echoes an "
        "Authorization header is masked.",
        "The same guard covers the vault: it refuses --reveal on a non-interactive "
        "stdout, so an accidental cat cannot put a secret in the transcript.",
        "Your agent touches no credential you'd mind seeing in a transcript.",
    )

    def test_every_shipped_sentence_is_caught(self):
        for sentence in self.SHIPPED:
            with self.subTest(sentence=sentence[:48]):
                self.assertTrue(flagged(_plain(sentence)),
                                "the detector no longer flags a sentence that shipped")

    def test_every_reviewed_bypass_is_caught(self):
        """The round-two half. A reviewer whose only instruction was to defeat this file
        found these; if a later loosening lets one back through, the loosening fails here
        rather than in the next security assessment."""
        for sentence in self.BYPASSED:
            with self.subTest(sentence=sentence[:48]):
                self.assertTrue(flagged(_plain(sentence)),
                                "a wording the reviewer got past round one passes again")

    def test_the_corrected_sentences_pass(self):
        for sentence in self.CORRECTED:
            with self.subTest(sentence=sentence[:48]):
                self.assertFalse(flagged(_plain(sentence)))

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
