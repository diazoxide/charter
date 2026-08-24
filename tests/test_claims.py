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
news entry — and `charter secret cp <vault> <key> /dev/stdout` printed the value to stdout,
in charter's own process, with no child command anywhere in it, and then reported
`Value not shown.` (#421, #422). The replacement guarantee was false the same way the
original was, which is why `TestEveryClaimSurfaceCarriesTheLimit` now also requires every
page that makes the claim to *name the two commands that break it*. A guarantee without its
exceptions on the same page is the original defect with a longer sentence.

**What the third round changed, and why.** The reviewer got six more classes past round
two, and every one of them is the same defect the two earlier rounds had:

* The rule **read in one direction**. `_resolve_antecedent` searched backwards, so round
  one's bypass came back verbatim with the clauses swapped and the object pronominalised.
  `_TOTAL_REDACTION` matched *verb → quantifier*, so *"Every byte of the value is taken
  out of the output"* passed. Both now read both ways, and `unqualified_promise` asks
  every absolute word in the sentence rather than the first.
* The rule **spelled one concept twice**. "A place a reader sees text" existed in
  `_EXPOSURE` and again in `_IMPOSSIBLE`; one knew `chat` and the other did not, so the
  shipped sentence with one noun swapped matched neither. There is one `_PLACE` now, one
  `_MOTION`, one `_SECRET_NOUN`, and every rule is built from them.
* The rule **enumerated an open class as five words**. `_SUBJECT` was
  `secret|credential|password|token|value`, so *"Your kubeconfig never appears in the
  transcript"* passed while `docs/secrets.md` uses a kubeconfig as `secret cp`'s worked
  example. It matches on shape now as well as on spelling.
* The rule **knew one member of a construction**. `at no point` and nothing else from
  *under no circumstances / at no time / in no case / there is no way*; `in every case`
  and not `in all cases`. Those families are generated, not listed.
* The rule **failed open on quotations**. Any double-quoted span was blanked before the
  split, so quoting a single word of a live promise deleted the trigger. A span is
  blanked now only when the span is itself a claim — which is what a retraction quotes
  and an evasion does not.
* The rule **matched a file extension instead of a reader**. `*.md` only, while the
  retracted sentence still stood word-for-word in `cmd_secret_exec`'s docstring and in
  `commands_persona.py`'s module docstring, both of which a model reads. Docstrings under
  `charter/` are in scope now — see `_source_prose`.

**And then the same question was asked of the fix, which is the part rounds one and two
skipped.** Four more classes came out of it, one of them created by the fix itself:

* The rule **read the encoding instead of the rendering**. `charter ne​ver puts the
  value in the transcript.` renders as *never* on every surface that shows this file and
  matches no `\\bnever\\b` anywhere in it — U+200D, U+00AD and U+2060 do the same, and so
  does `ne<!-- -->ver` with no exotic codepoint at all. That is round two's U+3164
  HANGUL FILLER lesson moved up a layer, so it is fixed the same way: `_plain` drops
  Unicode category `Cf` — *renders as nothing* is the definition of that category — and
  strips the markup that is not on the page, rather than listing four codepoints.
* The rule **could not see a homoglyph, and cannot**: `nеver` is Cyrillic ie, and no
  confusables table ships in the standard library. The class is refused rather than
  matched — `mixed_script_words` fails the build on any word spelled out of two
  alphabets, which is a property of words and not a list of letters.
* The rule **read one clause boundary as a wall in one place and as nothing in another**.
  `(?:\\W+\\w+){0,3}?\\W+` between a verb and its object matches across a comma, so
  README.md:290 — *"…what it **reads**, or where **it** writes"* — was read as a promise
  that a reader never reads a secret, with the `it` being the next clause's subject.
  `_gap` is words and spaces now: a verb and its object are in one clause.
* **And that narrowing was itself a bypass, ten minutes old.** With the window unable to
  cross punctuation, *"charter never puts the value in, of all places, the transcript."*
  passed, and so did the parenthesised and dashed spellings of the same trick. A reader
  skips an aside, so `_readings` judges the sentence both ways. It is the fifth time in
  three rounds that a fix for one spelling opened another, which is the argument for
  writing down the next spelling every time instead of only when a reviewer finds it.

**What this file does not do, stated plainly, because the alternative is the defect it
exists to catch.** It is a regular expression over prose. It cannot tell whether a
sentence is true. It matches vocabulary — an open class of credential nouns, an open class
of verbs meaning "arrives somewhere", an open class of words meaning "no exceptions" — and
every one of those lists is incomplete and will stay incomplete, because English is. What
it *can* do is refuse the shapes it knows, and the honest claim is that narrow: **a
sentence in these files that makes an absolute claim about where a value ends up, in a
wording this file recognises, fails the build unless the bound sits in the same
sentence.** Anything stronger than that is a sentence someone will disprove in round four.

Two of its limits are worth naming rather than leaving to be found. It reads the page as
rendered, not as typed, so an invisible codepoint or a markdown wrapper no longer hides a
word from it — but a **homoglyph** it cannot read at all, and `mixed_script_words` refuses
that class instead of matching it. And a **borrowed antecedent** and a **skipped aside**
are each a guess about what a sentence means; the two are never stacked, which means a
promise that needs both to be visible is a promise this file does not see.

Four things keep it from being unfailable in the other direction — passing because it
stopped looking. `TestTheDetectorFires.SHIPPED` runs the historical wording, the actual
sentences that shipped. `BYPASSED` runs everything three reviewers got past rounds one and
two, plus the next spellings found by asking the same question of round three's own fix —
one of which, *"Nobody watching this conversation ever sees the password."*, defeated the
first draft of it, and ten of which are the invisible-codepoint, markup and aside
spellings that defeated the second. `CONFUSABLE` pins the one class `flagged` does **not**
catch, and asserts that it does not catch it, so the file cannot quietly come to claim
otherwise. And the qualifier vocabulary is deliberately about *this* limit
(capture, transform, whose command it is, which paths); "guard rails, not guarantees" two
paragraphs down is a different limit and does not count as covering this one.

Persona charters and `.claude/agents/*` are out of scope on purpose: those are a plane's
own content, generated once and then owned by whoever runs the plane, not the claim
surface charter ships. `docs/superpowers/` is out of scope because the security assessment
quotes the wrong wording verbatim in order to correct it.
"""

from __future__ import annotations

import re
import unicodedata
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


def _source_prose() -> list[tuple[str, int, str]]:
    """(where, line, docstring) for every docstring under `charter/`.

    **A file extension is not the claim surface.** Rounds one and two policed `*.md`
    only, and round three found the retracted sentence still standing, word for word, in
    the docstring of the function that implements the path it is about
    (`cmd_secret_exec`: *"The model constructs the command using env-var names and never
    sees any value"*) and in `commands_persona.py`'s module docstring (*"never sees the
    plaintext"*, falsified by `persona secret get --reveal --force`). The `.md` twin of
    the second was corrected in this PR and the source one was left, which is exactly the
    shape of every miss in this audit: the rule matched the spelling of the container
    rather than the property of the content.

    The property is **prose charter ships that a reader — including a model reading the
    source — takes as charter's own claim**. A docstring is that. So it is in scope, and
    the same two rules run over it.
    """
    import ast

    out: list[tuple[str, int, str]] = []
    for path in sorted((ROOT / "charter").rglob("*.py")):
        try:
            tree = ast.parse(path.read_text())
        except SyntaxError:                                    # pragma: no cover
            continue
        for node in ast.walk(tree):
            if not isinstance(node, (ast.Module, ast.ClassDef,
                                     ast.FunctionDef, ast.AsyncFunctionDef)):
                continue
            doc = ast.get_docstring(node)
            if doc:
                out.append((path.relative_to(ROOT).as_posix(),
                            getattr(node, "lineno", 1), doc))
    return out


# ---------------------------------------------------------------------------
# One vocabulary per concept.
#
# Round two kept two separate lists for the single idea "a place a reader would see
# text": `_EXPOSURE` and the tail of `_IMPOSSIBLE`. They drifted apart — the second knew
# `chat`, the first did not — and the reviewer walked between them with the shipped
# sentence and one noun swapped: *"charter never puts the value in the chat."* Two lists
# for one concept is the same defect as one list for an open class, and it is fixed the
# same way: name the concept once, derive every rule from it.
# ---------------------------------------------------------------------------

#: The thing the promise is about. **This is the one genuinely open class in the file**,
#: and round two wrote it as five words — `secret|credential|password|token|value` — so
#: *"Your kubeconfig never appears in the transcript."* and *"The API key never enters an
#: agent's context window."* both passed, while `docs/secrets.md` uses a kubeconfig as
#: `secret cp`'s worked example. English will always have one more word for a credential
#: than any list has, so the list is not the whole rule: the compound arms match on
#: **shape** — a word that *ends in* `-config`, `-key`, `-cert`, `-token`, `-password`,
#: `-secret` — and catch the compounds nobody wrote down (`kubeconfig`, `apikey`,
#: `hostkey`, `bearertoken`). The leading `\w+` is required and not decorative: `\w*`
#: matches the bare word, and bare `config` is `charter/root.py`'s settings object, not a
#: credential.
#:
#: **It is still not complete.** Two-word compounds only match through the modifier list,
#: which stops at what charter's own docs use as vault contents; "the deploy key never
#: reaches the transcript" is caught and a phrase nobody here has written yet may not be.
#: Saying otherwise would be this file committing the defect it exists to catch.
_SECRET_NOUN = (r"secrets?|credentials?|creds?|passwords?|passphrases?|passwds?"
                r"|tokens?|values?|plaintexts?|plain text|cleartexts?|clear text"
                r"|cookies?|certificates?|service[- ]accounts?|dotenv|netrc|htpasswd"
                r"|(?:api|ssh|private|public|signing|access|host|deploy|encryption"
                r"|master|session|auth|gpg|pgp|tls|ssl|vault|secret|account)[- ]"
                r"(?:keys?|certs?|tokens?|files?|configs?)"
                r"|\w+keys?|\w+configs?|\w+certs?|\w+tokens?|\w+passwords?|\w+secrets?")
_SUBJECT = re.compile(r"\b(?:" + _SECRET_NOUN + r")\b", re.I)

#: A place a reader would see text. Some of these words (`context`, `output`, `session`,
#: `window`, `message`) are ordinary nouns elsewhere in these docs, which is why nothing
#: matches this list on its own: a motion verb has to point at it. That is what lets the
#: list be generous instead of careful — narrowing the *scope* of a match is what makes
#: widening its *vocabulary* safe, and it is the trade this whole file now runs on.
_PLACE = (r"transcript|context window|context|conversation|chat|prompt|summary"
          r"|log|logs|logfile|history|terminal|screen|console|output"
          r"|stdout|stderr|window|scrollback|clipboard")

#: Motion towards a place, in both voices. Round one knew `leak|escape|reach|end up` and
#: fell to *"enters"*; round two grew the arriving half and fell to *"puts"*, because
#: **the value arriving somewhere and somebody putting it there are the same event told
#: from two ends**, and only one end had been written down. Both halves are here now.
_ARRIVE_VERB = (r"leak|escape|reach|end up|enter|appear|show up|surface|land|arrive"
                r"|get into|gets into|make it|makes it|be seen|turn up|turns up"
                r"|be printed|be written|be logged|be recorded|cross|pass into|go into"
                r"|goes into|be exposed|be disclosed|be revealed|wind up|winds up"
                r"|stay out of|stays out of|come out|comes out|find its way")
_DELIVER_VERB = (r"put|print|write|show|display|expose|emit|send|echo|copy|render"
                 r"|reveal|leave|paste|dump|spill|log")
_MOTION = _ARRIVE_VERB + r"|" + _DELIVER_VERB


def _gap(n: int) -> str:
    """Up to *n* intervening words, **inside one clause**.

    Every two-part rule in this file — verb and its place, verb and its object,
    quantifier and its verb — is a window between two words, and round three's own first
    draft wrote that window as ``(?:\\W+\\w+){0,n}?\\W+``. `\\W` matches a comma. So the
    window reached across a clause boundary and read the *next* clause's subject as this
    verb's object, and README.md:290 is what that costs::

        …cannot choose what it runs, what it **reads**, or where **it** writes.

    `it` there is the subject of "it writes", the sentence is about a persona name and
    not about a credential, and the rule flagged it as a promise that a reader never
    reads the secret. Two false positives of that shape are how a guard gets deleted.

    The property is grammatical, not typographic: **a verb and its object are in the same
    clause.** So the window is made of words and the spaces between them, and nothing
    else — a comma, a dash, a semicolon or a full stop ends it, because each of those is
    where the clause the verb governs ends. Intra-word punctuation stays inside the
    token, so `agent's`, `env-var` and `--dotenv` are each one word and not a boundary.
    """
    return r"(?:[ \t]+[-\w'’]+){0," + str(n) + r"}?[ \t]+"


#: "…lands in the transcript", "…puts it in the chat" — a motion verb and a place, near
#: enough to each other to be one statement.
_DELIVERY = re.compile(
    r"\b(?:" + _MOTION + r")\w*" + _gap(4) + r"(?:" + _PLACE + r")\b", re.I)

#: Somebody whose reading of a value is the leak. `it` and `they` are in here because
#: "The model names the secret; **it** never sees it." is a sentence charter shipped.
_READER = (r"model|agent|assistant|llm|you|your|yours|user|human|reader|anyone"
           r"|anybody|someone|somebody|it|they|them|their")

#: "…never sees it" — the same exposure told as perception rather than motion.
#:
#: **The verb has to have the value as its object.** Without that, *"a credential vault
#: the model never reads from"* — the README's own subtitle, where what is not read is
#: the vault — reads as a promise about a value.
_PERCEIVE = re.compile(
    r"\b(?:see|sees|seen|seeing|view|views|viewing|watch|watches|watching"
    r"|observe|observes|witness|witnesses|lay eyes on|look at)\b"
    + _gap(3) + r"(?:it|them|" + _SECRET_NOUN + r")\b", re.I)

#: `read` is the one perception verb that is also what a program does to a file, and
#: these docs are full of the second sense: *"charter doctor and vault list never read a
#: value."* and *"Never read a filled secret back."* are a checkable fact and an
#: instruction, not promises about a reader. So `read` counts only when the sentence also
#: names somebody with eyes. That extra condition is *necessary*, never sufficient — it
#: can only make the rule miss less, and it is not round one's "does the word appear
#: anywhere" test wearing a hat: that one was an **exemption**, and this is a requirement.
_PERCEIVE_READ = re.compile(
    r"\b(?:read|reads|reading)\b" + _gap(3) + r"(?:it|them|"
    + _SECRET_NOUN + r")\b", re.I)
_READER_RE = re.compile(r"\b(?:" + _READER + r")\b", re.I)

#: Exposure with no verb in it at all. *"At no stage is the token **visible** to the
#: model."* and *"The password is **invisible** to the model, always."* are the same
#: promise as "the model never sees the value" written as a predicate adjective, and a
#: rule built only out of verbs cannot see them. Requires a reader, for the same reason
#: `_PERCEIVE_READ` does: "the file is readable" is a permission bit.
_VISIBLE = re.compile(r"\b(?:in)?(?:visible|readable|legible|viewable)\b"
                      r"|\bin (?:the clear|plain sight|plaintext)\b", re.I)

#: "…for the transcript to **contain** the value" — the place as subject and the value as
#: object, which is `_DELIVERY` with the sentence turned around. Round three's own first
#: draft had only the one direction, which is the mistake this whole file keeps making.
_HOLDING = re.compile(
    r"\b(?:" + _PLACE + r")\b" + _gap(4)
    + r"(?:contain|hold|carry|include|keep|end up with|wind up with)\w*"
    + _gap(3) + r"(?:it|them|" + _SECRET_NOUN + r")\b", re.I)

#: Absolute words whose scope is **the clause they sit in**.
#:
#: Bare `nobody` and `nothing` are deliberately absent: "with nothing redacting it" is how
#: three pages *describe the leak they are warning about*, and a rule that flags a hazard
#: notice as an overclaim is a rule someone deletes. `no value`/`no secret` stay, because
#: "charter puts no value in a context window" is the promise wearing a different hat.
#:
#: But **`nobody … ever` is not `nothing` — it is `never` with the negation moved onto the
#: subject**, and it was the one next-spelling that got past the first draft of round
#: three's own fix: *"Nobody watching this conversation ever sees the password."* The
#: `ever` is what tells the two apart, and it costs nothing: no hazard notice in these
#: pages carries it.
_ABSOLUTE_LOCAL = re.compile(
    r"\b(?:never|cannot|can't|impossible|not one|not a single|never once|always"
    r"|no (?:step|byte|part|copy|trace|" + _SECRET_NOUN + r"))\b"
    r"|\b(?:nobody|no one|no-one|nothing|none of|zero)\b" + _gap(5) + r"ever\b", re.I)

#: Absolute markers whose scope is **the whole sentence**, because that is what a
#: sentence adverbial modifies.
#:
#: Round two had one list and gave every entry clause scope, so
#: *"The secret stays out of the transcript, without exception."* put the promise in one
#: clause and its absolute marker in another and the rule read the wrong one — it looked
#: at `without exception.` and found no exposure there, correctly, and passed the
#: sentence. The fix is not a longer list; it is that **an absolute word's scope is a
#: property of the word**, and these words scope over everything.
#:
#: The `no <noun>` arm is generative rather than enumerated: "under no circumstances",
#: "at no time", "in no case", "there is no way" and "in no event" are one construction,
#: and writing them out one at a time is how round two ended up with `at no point` and
#: nothing else from the family.
#:
#: Kept deliberately short. Sentence scope is powerful and therefore noisy — it will pair
#: an absolute marker in one clause with an exposure in another — so only markers that
#: *are* exception-denials belong here. `always` is not one: it is usually an intensifier
#: on its own verb ("Values are **always** written via `--stdin`"), so it stays clause-
#: local, where round two had it.
_ABSOLUTE_SENTENCE = re.compile(
    r"\b(?:(?:under|at|in|on|by) no (?:\w+ )?"
    r"(?:circumstances?|cases?|times?|points?|stages?|moments?|instances?|events?"
    r"|conditions?|situations?|scenarios?|way)"
    r"|there(?:'s| is| was| are| were) no way|no way (?:that|for|to|the|an?|it|you)"
    r"|without (?:exception|fail)|no exceptions?|bar none|no matter what|come what may"
    r"|in (?:every|all|each|any) (?:cases?|instances?|situations?|circumstances?"
    r"|events?|scenarios?)"
    r"|guaranteed|100%|zero chance|not ever)\b", re.I)

#: The actor a guarantee can be a property of. Naming charter is *necessary and not
#: sufficient*: see `_promise_is_charters` for the structural test.
_ACTOR = re.compile(r"\bcharter\b", re.I)

#: The parties a promise about a secret can be *made about* — the reader it must not
#: reach, and the value itself. When one of these heads the clause the absolute word sits
#: in, the sentence is describing the world, and charter cannot underwrite the world: "the
#: model never sees the value", "the password never appears in your transcript", "no step
#: here ever put the value in a context window". No quantity of "charter" elsewhere in the
#: sentence changes what the clause is about, which is the entire round-two correction.
#:
#: Composed from `_SECRET_NOUN` rather than repeating it, because the two lists went out
#: of step once already and one of them was five words long.
#:
#: An ordinary noun is not in here on purpose. "so an accidental `cat` cannot put a secret
#: in the transcript" (docs/git-policy.md) is a bounded, true sentence about a guard, and a
#: rule that cannot tell it from "the model never sees the value" is a rule that gets
#: deleted by the third person it annoys — so for those, the bound still decides.
_PROTECTED_SUBJECT = re.compile(
    r"\b(?:model|agent|assistant|llm|step|nobody|no one"
    r"|it|its|they|them|this|that|those|these|" + _SECRET_NOUN + r")\b", re.I)

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
#:
#: `where` was in this list and is not any more. *"charter never puts the value **where**
#: the model can see it."* and *"There is not a single case **where** the value reaches
#: the transcript."* both ended their clause one word before the thing being promised
#: about, and both passed. `where` almost always introduces the destination of the verb
#: it follows, which is the half of the sentence this rule exists to read.
_CLAUSE_END = re.compile(
    r"[;:()\u2014\u2013]|\s--\s"
    r"|\b(?:because|since|so that|so|but|while|whilst|which|when|if|unless"
    r"|although|though|whereas|you'd|you would|i'd|we'd|they'd)\b", re.I)

#: Redaction claimed as total — "in every case", "every occurrence", "from whatever that
#: command prints" — or claimed to make a leak impossible. Both are false for a transform
#: and for `--exec`/`--stream`.
#:
#: The verb list is a list because the round-one one was three words long, and the
#: reviewer swapped in a fourth: *"The value is stripped from every output, in every case,
#: so a command that echoes it is safe."* A rule that a thesaurus defeats is a rule about
#: spelling. Anything that means "took the value out of the bytes" belongs here.
_REDACT_VERB = (r"redact|scrub|mask|strip|remove|filter|censor|sanitis|sanitiz|elid"
                r"|suppress|blank|obscur|hide|hidden|hides|withh|replace|swap out"
                r"|take out|takes out|taken out|keep out|kept out|purge|wipe|erase")

#: "no exceptions to this" as a quantifier over the redaction's object.
_UNIVERSAL = (r"every|everything|all|any|anything|each|whatever|whichever|whole"
              r"|entire|complete|total")

#: "in every case", "in all cases" — totality as a sentence adverbial rather than as a
#: quantifier on the object. Round two knew `in every case` and nothing else from the
#: family, so *"charter redacts the secret from output, in all cases."* passed.
_EXHAUSTIVE = (r"100%|every single|in (?:every|all|each|any) (?:cases?|instances?|situations?"
               r"|circumstances?|events?|scenarios?)|without exception|no exceptions?")

#: The association between the quantifier and the redaction is what matters, and
#: **it runs in both directions, and the adverbial can sit at either end**. Round two
#: knew *verb → quantifier* only — `redacts … from every` — so the reviewer wrote the
#: quantifier first: *"Every byte of the value is taken out of the output."* passed. It is
#: the same sentence. That was the second of the two "the rule only reads left-to-right"
#: holes round three found; the other was in `unqualified_promise`.
#:
#: The three arms are shaped, not padded. A few words of object may sit between verb and
#: quantifier when a removal preposition joins them — "charter filters the secret **out
#: of all** output", "charter sanitizes the value **from whatever** the command prints" —
#: because that is one construction. The quantifier→verb arm may not cross a comma or a
#: full stop: *"it names **anything** unapproved, printing the exact command it
#: **withheld** the vault from"* is two statements, and a window wide enough to join them
#: is a window that flags any page with a quantifier and a verb on it.
_TOTAL_REDACTION = re.compile(
    r"\b(?:" + _REDACT_VERB + r")\w*" + _gap(4) + r"(?:from|out of|off|of)\W+"
    r"(?:" + _UNIVERSAL + r")\b"
    r"|\b(?:" + _REDACT_VERB + r")\w*\s+(?:it\s+)?(?:from\s+)?(?:" + _UNIVERSAL + r")\b"
    r"|\b(?:" + _UNIVERSAL + r")\b[^,;:.()—–]{0,60}?\b(?:" + _REDACT_VERB + r")\w*"
    r"|\b(?:" + _EXHAUSTIVE + r")\b[^.]{0,140}?\b(?:" + _REDACT_VERB + r")\w*"
    r"|\b(?:" + _REDACT_VERB + r")\w*[^.]{0,140}?\b(?:" + _EXHAUSTIVE + r")\b"
    r"|\bevery occurrence\b", re.I)

#: "cannot leak into the transcript" — impossibility, with the place named. The place is
#: required: "it reaches the API through the same bridge" is a sentence about plumbing.
#: Built from the same `_MOTION` and `_PLACE` as `_DELIVERY`, so a word added for one
#: rule cannot go missing from the other — which is how `chat` was reachable here and not
#: there.
_IMPOSSIBLE = re.compile(
    r"\b(?:cannot|can't|never|impossible|at no point|no way)\b" + _gap(4) + r"(?:"
    + _MOTION + r")\w*" + _gap(3) + r"(?:" + _PLACE + r")\b", re.I)

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
    # was itself false: `charter secret cp <v> <k> /dev/stdout` was charter's own process
    # printing the value into this transcript, with no child command anywhere in it.
    # #449 has since closed that path — `cp` now refuses any destination whose
    # `(st_dev, st_ino)` is one of charter's own streams — so the remaining printing path
    # is `secret get --reveal`, which a human has to ask for. The bound stays either way,
    # because it is the true shape of the promise and not a workaround for one bug.
    "destination you named", "destination you name", "destination you chose",
    "path you named", "path you name",
    "only where you ask", "only when you ask", "only where you tell it",
    "asked for it yourself", "ask for it yourself",
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


#: Markup that is not there on the rendered page. An HTML comment is the strongest case:
#: `ne<!-- -->ver` renders as **never** and matches nothing this file knows, which is the
#: `describe()`-padding trick of round two moved into prose. A markdown link is the same
#: shape — `[transcript](/docs/x.md)` reads as *transcript*, and only the URL is between
#: the verb and its place.
_INVISIBLE_MARKUP = (
    (re.compile(r"<!--.*?-->", re.S), ""),                  # HTML comment: renders as nothing
    (re.compile(r"!?\[([^\]]*)\]\([^)]*\)"), r"\1"),        # [text](url) renders as text
    (re.compile(r"</?[A-Za-z][^>]*>"), ""),                 # <b>, <br/>: renders as nothing
)


def _plain(text: str) -> str:
    """The sentence **as a reader sees it**, not as it is encoded.

    Round two's `_safe` decided "blank" with `str.isprintable()` and lost to U+3164
    HANGUL FILLER. This is the same lesson one layer up, and the same three spellings
    beat the first draft of round three:

    * `charter ne\\u200bver puts the value in the transcript.` — a ZERO WIDTH SPACE
      inside the trigger word. It renders as *never*, and `\\bnever\\b` does not match it.
      U+200D, U+00AD SOFT HYPHEN, U+2060 WORD JOINER and U+FEFF all do the same job.
    * `charter ne<!-- -->ver …` — an HTML comment doing it in markdown instead.
    * `…in the [transcript](/docs/x.md).` — the place wrapped in a link, so the URL sits
      between the verb and its object.

    None of these is a list to extend. The property is **what the page shows**, so the
    normalisation is by Unicode property, not by codepoint: NFKD folds the compatibility
    spellings (fullwidth letters, ligatures, U+00A0) onto their ordinary ones, and every
    character in category `Cf` (format — zero-width and bidi controls, all of which
    render as nothing by definition) and `Mn`/`Me` (combining marks, which render *onto*
    the previous character rather than as a character of their own) is dropped. A
    codepoint invented tomorrow that renders as nothing will be `Cf` too.

    What this does **not** normalise is a confusable: `n\\u0435ver` — Cyrillic ie — looks
    identical and is a different letter under every Unicode property there is. No stdlib
    table maps it, so it is not handled here. It is refused instead, by
    `test_no_word_mixes_scripts`, which is a rule about words rather than about that
    letter.
    """
    return unicodedata.normalize("NFKD", _rendered(text))


def _rendered(text: str) -> str:
    """`_plain` without the NFKD fold — the same characters the page shows.

    Split out because `mixed_script_words` has to see the character that is *written*.
    NFKD maps U+00B5 MICRO SIGN onto GREEK SMALL LETTER MU, and `20µs` in
    `charter/contain.py` and the frame news entry would then read as a Greek letter next
    to a Latin one — a mixed-script word, reported as a homoglyph, in prose that has
    none. U+00B5's Unicode script is Common, not Greek; folding first threw that fact
    away. Order matters, and this is the order.
    """
    for pattern, repl in _INVISIBLE_MARKUP:
        text = pattern.sub(repl, text)
    text = "".join(c for c in text if unicodedata.category(c) not in ("Cf", "Mn", "Me"))
    return text.replace("*", "").replace("`", "").replace("_", " ")


#: A parenthesis, a paired dash or a paired comma around a span a reader skips.
#: **Judging only the sentence as written is judging one of the two sentences on the
#: page.** Round three's own `_gap` — words and spaces, so a construction cannot reach
#: across a clause boundary for its object — made three asides into bypasses the moment
#: it was written:
#:
#:     charter never puts the value in, of all places, the transcript.
#:     charter never puts the value (or any part of it) in the transcript.
#:     The model never sees — and cannot see — the value.
#:
#: Widening the gap back to `\W` is what put README.md:290 on the failure list; the
#: answer is not a wider window but a second reading. A reader skips the aside, so the
#: rule reads the sentence both ways and a promise in either one counts.
_ASIDES = (
    re.compile(r"\([^()]*\)"),
    re.compile(r"[—–][^—–.;:]*[—–]"),
    re.compile(r",[^,.;:()]*,"),
)


def _readings(sentence: str) -> list[str]:
    """The sentence as written, and with each removable aside taken out.

    One pass per aside kind rather than all at once, and each pass repeated to a fixed
    point, so nested and successive asides collapse. The aside is replaced by a single
    space and not by a comma: leaving the punctuation behind leaves the clause boundary
    behind, and the clause boundary is the whole of what the aside was hiding behind.
    """
    out = [sentence]
    for pattern in _ASIDES:
        text = sentence
        for _ in range(4):
            stripped = pattern.sub(" ", text)
            if stripped == text:
                break
            text = stripped
        if text != sentence:
            out.append(text)
    return out


#: A double-quoted span is somebody else's words — sometimes. This file has to let the
#: news entry print the sentence it is retracting, and round two did that by blanking
#: **every** quoted span before splitting. Which meant that quoting one word of a live
#: promise deleted the trigger from it: *The model never sees the `"value"` in your
#: transcript.* and *charter never prints the value into the `"conversation"`.* both
#: passed, and these very pages quote single terms inline all the time
#: (`docs/secrets.md`: *"Known reader programs" covers the shell*).
#:
#: The property is not "is it in quotation marks". It is **is the claim inside the
#: quotation marks**. A retraction quotes a claim; an evasion quotes a token. So a quoted
#: span is blanked only when that span, read on its own, is itself something this file
#: would flag — which is exactly when blanking it removes a claim rather than removing a
#: word from one. Everything else stays in the sentence and is judged with it.
#:
#: The default therefore fails *closed*: an unattributed quotation is judged. The cost is
#: false positives on retraction prose the rule cannot recognise, which breaks a build and
#: gets fixed; the cost of the other default was a bypass, which does not.
_QUOTED = re.compile(r"[\"\u201c][^\"\u201c\u201d]*[\"\u201d]")


def _unquoted(text: str) -> str:
    def keep_or_blank(m: re.Match) -> str:
        inner = m.group()[1:-1]
        return " " * len(m.group()) if _is_a_claim(inner) else inner
    return _QUOTED.sub(keep_or_blank, text)


def _is_a_claim(text: str) -> bool:
    """Would this span, standing alone, be flagged? Used only to decide quoting."""
    return any(unqualified_promise(s) or total_redaction(s)
               for s in re.findall(r"[^.!?]+[.!?]*", text) if s.strip())


def _blocks(path: Path) -> list[tuple[int, str]]:
    return _blocks_of(path.read_text())


def _blocks_of(text: str) -> list[tuple[int, str]]:
    """(line number, block) — paragraphs, list items and mermaid labels; code dropped.

    Blocks first, then sentences inside them. A naive split on `.` runs a heading, a
    shell example and the paragraph after it into one 400-character "sentence" that
    matches every pattern in this file and means nothing.

    Takes text rather than a path so that a docstring travels the same road a page does
    — see `_source_prose`. A rule that only knows how to read one container is one
    rename away from reading nothing.
    """
    out, cur, start, fenced, diagram = [], [], 0, False, False
    for n, raw in enumerate(text.splitlines(), start=1):
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


def _resolve_antecedent(sentence: str, earlier: str, later: str = "",
                        sep: str = " ") -> str:
    """Lend a pronoun clause the nearest noun in its own paragraph, before or after.

    Backwards covers the split the reviewer used. Forwards covers the same split written
    the other way round — *"The transcript never sees it. charter holds the secret in its
    own process."* — where the promise lands before its own subject and a backwards-only
    search finds nothing at all.

    `sep` is why this takes an argument at all. Round two joined the borrowed noun to the
    sentence with a space, and everything downstream that slices a clause starts at index
    0 — so the noun landed inside the sentence's *first* clause whether or not the pronoun
    did. #449's *"charter now removes every identity variable declared by a vault…"* is a
    true sentence about environment variables, and it was read as a claim about the
    service-account credential named in the sentence before it. Joining across a sentence
    boundary with `"; "` puts a clause break between them, so the noun is reachable by a
    clause that actually contains a pronoun and by no other.
    """
    if _SUBJECT.search(sentence) or not _PRONOUN.search(sentence):
        return sentence
    found = None
    for m in _SUBJECT.finditer(earlier):
        found = m.group()
    if found is None:
        m = _SUBJECT.search(later)
        found = m.group() if m else None
    return f"{found}{sep}{sentence}" if found else sentence


def _split(block: str) -> list[tuple[str, str]]:
    """(sentence as written, sentence as judged) for one block.

    Quotes are removed before the split, not after: a quoted sentence ends
    `transcript."` — period inside — so a per-sentence strip sees an unterminated span
    and leaves the retracted wording in place, which is how this file first failed on its
    own news entry.

    An antecedent is borrowed from the **neighbouring sentence only**, not from the whole
    block. A block here can be a 2,000-character audit bullet, and lending a pronoun a
    noun from four sentences away is how *"the `||` wrapper hides everything short of a
    blocker"* — a sentence about `charter doctor` output — came to be judged as a claim
    about a credential. A pronoun refers to something near it or it refers to nothing.
    """
    out = []
    text = _unquoted(block)
    spans = [m for m in re.finditer(r"[^.!?]+[.!?]*", text) if m.group().strip()]
    for i, m in enumerate(spans):
        s = m.group().strip()
        before = spans[i - 1].group() if i else ""
        after = spans[i + 1].group() if i + 1 < len(spans) else ""
        out.append((s, _resolve_antecedent(s, before, after, sep="; ")))
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


def _exposure(clause: str, sentence: str) -> bool:
    """Does this clause name somewhere the value ends up, or someone who sees it?

    One question, two shapes: motion towards a place (`_DELIVERY`) and perception of the
    value (`_PERCEIVE`). Round two spelled the first shape twice — once in `_EXPOSURE`
    and once inside `_IMPOSSIBLE` — with different word lists, and the reviewer walked
    between the two copies with the shipped sentence and one noun swapped. Both shapes
    now read the same `_PLACE` and `_MOTION`.

    `sentence` is consulted for one thing only: whether anybody with eyes is named, which
    is what licenses the `read` sense of perception. English elides the subject of a
    coordinated clause — *"The model constructs the command using env-var names **and**
    never sees any value"* is a promise about the model, and the model's name is in the
    other conjunct — so a rule that will not look past the clause boundary for the
    *perceiver* misses the sentence still standing in `cmd_secret_exec`'s docstring.
    """
    if _DELIVERY.search(clause) or _PERCEIVE.search(clause) or _HOLDING.search(clause):
        return True
    if _VISIBLE.search(clause) and _READER_RE.search(sentence):
        return True
    return bool(_PERCEIVE_READ.search(clause) and _READER_RE.search(sentence))


def _promise_fails(sentence: str, head: str, clause: str, m: re.Match,
                   written: str, borrow: bool) -> bool:
    """The three questions, asked of one absolute word's scope.

    `written` is the sentence **as it stands on the page**, which may differ from
    `sentence` when an aside has been skipped (`_readings`). The bound is always looked
    for in the written form and never in the skipped one, because skipping a span must
    not be able to *delete a qualifier*: `docs/secrets.md:347` bounds its promise with
    "…prints it only where you asked for it yourself — secret get --reveal to your
    terminal, or…", and the comma pair around that clause is a comma pair. A reading is
    evidence about the shape of a claim; it is never evidence that the claim is unbounded.

    `borrow` is off for a skipped reading for the same reason in the other direction. A
    borrowed antecedent is a guess, an aside-skip is a guess, and stacking the two is
    what flagged `commands_frame.py:630` — *"cmd launch filters everything else out
    (unimplemented) before a pane is ever split for it"*, a sentence about tmux panes,
    which became a redaction claim only once the parenthesis went away and the word
    "values" was fetched in from the sentence before it. One guess at a time."""
    if not _exposure(clause, sentence):
        return False
    # A pronoun clause inherits its noun from the rest of the sentence — "The model
    # names the secret; it never sees it." puts the subject in the first clause and the
    # promise in the second. **In either order.** Round two searched backwards only, and
    # the reviewer reversed the clauses: *"The model never sees it, because charter
    # resolves the value in its own process."* — round one's exact bypass with the
    # subordinate clause moved behind the promise and the object pronominalised, and the
    # noun that would have identified it sat where nothing was looking. The antecedent
    # travels in both directions; the bound never travels at all.
    subject = (_resolve_antecedent(clause, sentence[:m.start()], sentence[m.end():])
               if borrow else clause)
    if not _SUBJECT.search(subject):
        return False
    if not _promise_is_charters(head):
        if _PROTECTED_SUBJECT.search(head) or not head.strip():
            return True
    return not any(q in written.lower() for q in _QUALIFIERS)


def unqualified_promise(sentence: str) -> bool:
    """An absolute claim that a value does not reach a reader, and no bound on it.

    **The bound must be in this sentence, and naming charter is no substitute for one.**
    Round one let the word "charter" exempt a sentence outright, and then wrote
    *"charter never prints the value into the conversation"* — which
    `charter secret cp <vault> <key> /dev/stdout` falsified inside charter's own process,
    with no child command anywhere in it. Naming the actor was necessary and never
    sufficient.

    **And a promise about the model, or about the value itself, fails whatever else the
    sentence says** — those are claims about the world, and no bound charter can write
    makes them charter's to keep. Rewrite them with charter as the subject, which forces
    the sentence to say what charter actually does.

    Every absolute word in the sentence is asked, not just the first, and each is asked
    about **its own scope**: a clause for `never`, the whole sentence for an adverbial
    like *without exception*. Round two took `_ABSOLUTE.search` — the first match — and
    gave it clause scope unconditionally, which meant a sentence could carry a harmless
    absolute in front of a real promise and be judged on the harmless one.

    And every *reading* of the sentence is asked, not just the one on the page: a
    parenthetical is a span a reader skips, so a promise that only appears once the aside
    is skipped is a promise. See `_readings`.
    """
    written, *skipped = _readings(sentence)
    return (_promise_in(written, written, borrow=True)
            or any(_promise_in(r, written, borrow=False) for r in skipped))


def _promise_in(sentence: str, written: str, borrow: bool) -> bool:
    """`unqualified_promise` for one reading of the sentence."""
    for m in _ABSOLUTE_LOCAL.finditer(sentence):
        head, clause = _promise_clause(sentence, m)
        if _promise_fails(sentence, head, clause, m, written, borrow):
            return True
    for m in _ABSOLUTE_SENTENCE.finditer(sentence):
        if _promise_fails(sentence, sentence[:m.start()], sentence, m, written, borrow):
            return True
    return False


def total_redaction(sentence: str) -> bool:
    """Redaction claimed over every output, or claimed to make a leak impossible.

    Naming charter does not rescue this one: redaction is `str.replace` over captured
    bytes whoever is speaking, so "cannot leak" is false for `base64` either way.

    **The claim has to be about a secret, and "about" means in the same clause.** Round
    two asked only whether a secret noun appeared anywhere in the sentence, and the
    merge of #449 produced the false positive that showed why: *"charter now removes
    every identity variable declared by a vault other than the one being read — both
    halves of each binding…"* is a true sentence about environment variables, and it was
    flagged because `removes … every` matched in one clause while a credential was named
    in another. Scoping the subject to the clause is also what lets `_UNIVERSAL` include
    ordinary words like `everything`: narrow the scope, and the vocabulary can be
    generous instead of careful.

    Read with each aside skipped as well as as-written, for the reason `_readings` gives.
    """
    if _CAPTURED.search(sentence) or any(q in sentence.lower()
                                         for q in _QUALIFIERS):
        return False
    written, *skipped = _readings(sentence)
    return (_total_in(written, borrow=True)
            or any(_total_in(r, borrow=False) for r in skipped))


def _total_in(sentence: str, borrow: bool) -> bool:
    """`total_redaction` for one reading, with the exemptions already applied.

    `borrow` is off for a skipped reading, for the reason `_promise_fails` gives."""
    for pattern in (_TOTAL_REDACTION, _IMPOSSIBLE):
        for m in pattern.finditer(sentence):
            _, clause = _promise_clause(sentence, m)
            subject = (_resolve_antecedent(clause, sentence[:m.start()],
                                           sentence[m.end():]) if borrow else clause)
            if _SUBJECT.search(subject):
                return True
    return False


#: The script a letter belongs to, from its Unicode name: `LATIN SMALL LETTER E` is
#: Latin and `CYRILLIC SMALL LETTER IE` is Cyrillic, and the two render identically in
#: every font either of them ships in. Nothing but letters has a script — digits,
#: punctuation and spaces are shared, so they end a word rather than joining two.
#:
#: The exempt heads are the characters Unicode itself gives script **Common** — a symbol
#: that happens to be a letter under `str.isalpha`. U+00B5 MICRO SIGN is the one these
#: files actually use (`20µs`), and it is not a Greek letter sitting in an English word;
#: it is a unit symbol, and NFKD-folding it to GREEK SMALL LETTER MU is what would make
#: it look like one. See `_rendered` for why the fold runs after this and not before.
_SCRIPTLESS = frozenset({"MICRO", "MASCULINE", "FEMININE", "MODIFIER", "OHM", "KELVIN",
                         "ANGSTROM", "INFORMATION", "NUMERO", "ESTIMATED"})


def _script(ch: str) -> str | None:
    """The script of one letter, or `None` for something that has no script.

    A presentation variant is not a script of its own: FULLWIDTH LATIN SMALL LETTER N is
    Latin, and saying otherwise would report `ｎever` as two alphabets when it is one
    alphabet in disguise — a true thing said wrongly. So an unrecognised head is folded
    once through NFKD and asked again, which walks every compatibility form back to the
    letter it is a form *of*.
    """
    if not ch.isalpha():
        return None
    try:
        head = unicodedata.name(ch).split()[0]
    except ValueError:                                        # pragma: no cover
        return None
    if head in _SCRIPTLESS:
        return None
    folded = unicodedata.normalize("NFKD", ch)
    if folded != ch and folded[:1].isalpha():
        return _script(folded[0])
    return head


def mixed_script_words(text: str) -> list[str]:
    """Words whose letters do not all come from one script.

    `charter n\\u0435ver puts the value in the transcript.` is round two's U+3164 lesson
    written for prose: one letter replaced by a Cyrillic one that renders identically,
    and every `\\bnever\\b` in this file stops matching. `\\u0440assword`, `t\\u043euken`
    and a Greek omicron in `transcript` are the same attack, and there are hundreds of
    them — the Unicode confusables table is thousands of pairs long and is not in the
    standard library.

    So this does not ask "is this letter a known confusable". It asks the property that
    makes the whole class work: **a word that mixes scripts is not a word.** No sentence
    of English prose needs one, none of these pages has one, and a homoglyph cannot be
    written without making one. A list of confusables would be round four's bypass; this
    is not a list.

    `_rendered` runs first — markup and zero-width characters gone — but **not** the NFKD
    fold, because the fold is what turns `µs` into a Greek letter beside a Latin one.
    `_script` does its own folding, per character, after the exemptions.
    """
    bad = []
    for word in re.findall(r"[^\W\d_]+", _rendered(text), re.UNICODE):
        scripts = {s for s in (_script(c) for c in word) if s}
        if len(scripts) > 1:
            bad.append(word)
    return bad


class TestTheVaultClaimIsQualifiedWhereverItAppears(unittest.TestCase):
    def test_no_word_mixes_scripts(self):
        """A homoglyph is how you write `never` so that no rule in this file sees it.

        Failure means a word on a claim page is spelled out of two alphabets. Retype it.
        """
        for path in _scope():
            with self.subTest(file=path.relative_to(ROOT).as_posix()):
                self.assertEqual(
                    [], mixed_script_words(path.read_text()),
                    f"{path.relative_to(ROOT)} spells a word out of two alphabets — a "
                    f"homoglyph renders as an ordinary letter and matches none of the "
                    f"vocabulary below")
        for where, _, doc in _source_prose():
            with self.subTest(file=where):
                self.assertEqual([], mixed_script_words(doc),
                                 f"{where} spells a word out of two alphabets")

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
                        f"than captured output, or that a leak is impossible: "
                        f"{sentence!r}")


class TestTheSourceSaysTheSameThingAsThePages(unittest.TestCase):
    """The same two rules, over charter's own docstrings.

    Rounds one and two policed `*.md`. The sentence the README retracted was still
    standing, word for word, in `cmd_secret_exec`'s docstring — *"The model constructs
    the command using env-var names and never sees any value"* — with its own next
    paragraph contradicting it (*"nothing is captured, so nothing can be redacted"*). And
    `commands_persona.py`'s module docstring still said an agent adopting a persona
    *"never sees the plaintext"*, while the `.md` twin of that exact sentence
    (`skills/secrets/SKILL.md`) was corrected in this same PR and the source one was left.

    Correcting the twin and leaving the original is the shape of every miss in this
    audit: the rule matched the spelling of the container instead of the property of the
    content. A model reads `charter/` — it is the first thing an agent greps when a
    command surprises it — so a docstring is claim surface whatever its file extension.

    This does cost something, and it is worth naming: extending the scope flagged five
    honest sentences elsewhere in `charter/` (`doctor.py`, `hooks.py`, `news.py`,
    `secrets/base.py`, `secrets/onepassword.py`) that had to be reworded to say the same
    thing without an absolute. Two of those — `redact`'s *"every occurrence"* and
    1Password's *"replace every sibling secret"* — were worth rewording on their own.
    The other three were the rule being crude, which it says on the tin.
    """

    def test_no_docstring_makes_an_unbounded_promise(self):
        for where, line, doc in _source_prose():
            for _, block in _blocks_of(doc):
                for sentence, judged in _split(block):
                    with self.subTest(file=where, line=line):
                        self.assertFalse(
                            unqualified_promise(judged),
                            f"{where}:{line} makes an absolute promise about where a "
                            f"value ends up, with no bound in the same sentence: "
                            f"{sentence!r}")

    def test_no_docstring_claims_redaction_is_total(self):
        for where, line, doc in _source_prose():
            for _, block in _blocks_of(doc):
                for sentence, judged in _split(block):
                    with self.subTest(file=where, line=line):
                        self.assertFalse(
                            total_redaction(judged),
                            f"{where}:{line} claims redaction covers more than captured "
                            f"output, or that a leak is impossible: {sentence!r}")

    def test_the_source_scope_is_not_empty(self):
        """A glob that matches nothing passes every assertion above it — and this one
        walks a package rather than a file list, so an import error or a moved directory
        would silence it without failing anything."""
        found = {where for where, _, _ in _source_prose()}
        self.assertIn("charter/commands_secrets.py", found)
        self.assertIn("charter/commands_persona.py", found)
        self.assertIn("charter/secrets/base.py", found)
        self.assertGreater(len(found), 20)

    def test_the_two_docstrings_that_were_wrong_now_carry_the_bound(self):
        """Absence of the retracted sentence is not presence of a true one — the same
        argument `TestEveryClaimSurfaceCarriesTheLimit` makes about the pages.

        `cmd_secret_exec` is the function the whole issue is about, and
        `commands_persona.py` proxies to it. Each has to name the limit where a reader of
        that function will hit it, not one directory away."""
        import charter.commands_persona as cp
        import charter.commands_secrets as cs

        exec_doc = cs.cmd_secret_exec.__doc__ or ""
        self.assertTrue(any(q in exec_doc.lower() for q in _QUALIFIERS),
                        "cmd_secret_exec's docstring names no limit on redaction")
        self.assertNotIn("never sees any value", exec_doc)

        persona_doc = cp.__doc__ or ""
        self.assertNotIn("never sees the plaintext", persona_doc)
        for flag in ("--reveal", "secret cp"):
            self.assertIn(flag, persona_doc,
                          f"commands_persona.py promises without naming {flag}")


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
        """`secret cp <v> <k> /dev/stdout` used to print the credential into the
        transcript and then print "Value not shown."

        #449 merged the CLI refusal, and it is an identity check rather than a name
        check — `(st_dev, st_ino)` of the descriptor charter opened, against its own
        three streams — so the rule holds for `/dev/fd/1`, `/proc/self/fd/1`, the
        transcript's real path and any hardlink to it. Verified against the merged code
        while writing this: `tests/test_secret_cp_destination.py` is where it is pinned.

        The skill still names the case, because the model is the reader that will reach
        for `/dev/stdout` and it should know why it gets a refusal rather than treating
        the refusal as a bug to route around."""
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

        # ---- Round three. Six classes, all run in situ against the shipped files
        # ---- before anything here was written. Each is grouped with the property the
        # ---- rule was missing, because the property is what the next reviewer attacks.

        # 1. Round one's bypass, reordered. `_resolve_antecedent` searched *backwards*
        #    only, so moving the subordinate clause behind the promise and
        #    pronominalising the object put the identifying noun where nothing looked.
        #    The rule reads both ways now, which is the same correction `_TOTAL_REDACTION`
        #    needed for its quantifier.
        "The model never sees it, because charter resolves the value in its own process.",
        "The model never sees it — charter resolves the value in its own process.",
        # 2. Secret nouns charter documents but the five-word `_SUBJECT` list did not.
        #    `docs/secrets.md` uses a kubeconfig as `secret cp`'s worked example.
        "Your kubeconfig never appears in the transcript.",
        "The API key never enters an agent's context window.",
        "The service-account token never reaches your terminal.",
        # 3. The shipped sentence with one noun swapped, walking between the two lists
        #    that both meant "a place a reader sees text".
        "charter never puts the value in the chat.",
        "charter never leaves the credential on your screen.",
        # 4. Ordinary English absolutes. The `no <noun>` family is one construction and
        #    round two knew one member of it; the adverbials scope over the sentence, not
        #    over the clause they happen to sit in.
        "Under no circumstances does the value reach your transcript.",
        "At no time does the secret enter the conversation.",
        "The secret stays out of the transcript, without exception.",
        "There is no way the model sees the value.",
        # 5. Quotation erasure. Round two blanked every quoted span before splitting, so
        #    quoting one word of a live promise deleted the trigger from it.
        'The model never sees the "value" in your transcript.',
        'charter never prints the value into the "conversation".',
        # 6. Totality with the quantifier in front of the verb, and the adverbial family
        #    round two knew exactly one member of.
        "Every byte of the value is taken out of the output.",
        "charter redacts the secret from output, in all cases.",

        # ---- And the next spellings, asked of the round-three rule. Each of these is a
        # ---- property the fixes above should already cover; they are here so that a
        # ---- later loosening fails on the general case and not only on the reported one.
        #
        # Two absolute words in one sentence, the harmless one first: round two took
        # `_ABSOLUTE.search` — the first match — and judged the sentence on it.
        "charter cannot do much about a bad command, and the value never reaches the "
        "transcript.",
        # `plaintext` as the noun, and the promise about a persona rather than a vault —
        # the wording that was still standing in `commands_persona.py`.
        "An agent adopting a persona never sees the plaintext.",
        # The perception verb with a reader who is not the model.
        "Nobody watching this conversation ever sees the password.",
        # Delivery told from the actor's end at a place named late.
        "charter never writes the token to your terminal.",

        # ---- Round three, second pass: the next spellings of round three's OWN fix,
        # ---- found by asking the question of this file rather than of round two's.
        # ---- Every one of these passed the draft that fixed the six classes above.
        #
        # a. An aside inside the construction. `_gap` was narrowed to words and spaces so
        #    that a verb could not reach across a clause boundary for its object — and
        #    that narrowing made three punctuation marks into bypasses on the spot. The
        #    answer is `_readings`: a reader skips an aside, so the rule reads both ways.
        "charter never puts the value in, of all places, the transcript.",
        "charter never puts the value (or any part of it) in the transcript.",
        "The model never sees — and cannot see — the value.",
        # b. A codepoint that renders as nothing, inside the trigger word. This is round
        #    two's U+3164 lesson in prose: `​` between two letters of `never`
        #    renders as *never* and defeats every `\b`-anchored word in this file.
        #    U+200D, U+00AD and U+2060 do the same job, and `_plain` drops the property
        #    (Unicode category `Cf`) rather than the four codepoints.
        "charter ne​ver puts the value in the transcript.",
        "charter ne‍ver puts the value in the transcript.",
        "charter ne\xadver puts the value in the transcript.",
        "charter never puts the value in the trans​cript.",
        "charter never puts the sec​ret in the transcript.",
        # c. Markdown and HTML doing the same thing without any exotic codepoint at all.
        "charter ne<!-- -->ver puts the value in the transcript.",
        "charter never puts the value in the [transcript](/docs/secrets.md).",
        # d. A harmless absolute in front of a real promise, with a hard clause break
        #    between them. `unqualified_promise` asks every absolute word, and this is
        #    the case that pins that: mutate the loop to `return` on the first one and
        #    this sentence passes.
        #
        #    Two near-misses are worth recording, because both look like they pin it and
        #    neither does. The comma-joined version two entries above does not — a comma
        #    is not a `_CLAUSE_END`, so the first clause runs to the end of the sentence
        #    and swallows the promise anyway. And *"…; the value never reaches the
        #    transcript."* does not either: `total_redaction` catches that one through
        #    `_IMPOSSIBLE`, so `flagged` stays true with the promise rule crippled and
        #    the mutant lives. The promise here is a **perception** of the value with no
        #    place named, which is the one shape only `unqualified_promise` sees.
        "charter cannot promise much here; the model never sees the value.",
    )

    #: Homoglyphs. These are **not** in `BYPASSED`, because `flagged` does not catch them
    #: and saying it did would be this file overclaiming about itself — which is the
    #: defect the whole audit was called to remove. `nеver` is Cyrillic ie: it renders
    #: as `never`, it is a different letter under every Unicode property there is, and no
    #: confusables table ships in the standard library. `mixed_script_words` refuses the
    #: class instead — a word spelled out of two alphabets — and that is what these pin.
    CONFUSABLE = (
        "charter nеver puts the value in the transcript.",
        "The model never sees the vaӏue.",
        "charter never puts the value in the trаnscript.",
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
        # Two guesses, not stacked. Skipping the parenthesis lengthens the clause until
        # it reaches a pronoun, and borrowing would then fetch "values" from the other
        # side of the sentence and read a note about tmux panes as a promise about a
        # credential. This is `commands_frame.py:630`'s shape, written for the promise
        # rule rather than the redaction one, and it is what pins `borrow=False` on a
        # skipped reading in `_promise_fails`.
        "Only two values are interpolated, and the pane never displays a name on screen "
        "(unimplemented) before it is split.",
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

    def test_a_homoglyph_is_refused_as_a_word(self):
        """The one class `flagged` does not catch, caught by the rule that does.

        Each of these renders exactly like a sentence in `BYPASSED` and is invisible to
        every pattern in this file. `mixed_script_words` is what stands between them and
        the page — so if it is loosened, this fails, and the fact that `flagged` still
        returns nothing for them is asserted here too rather than left to be discovered.
        """
        for sentence in self.CONFUSABLE:
            with self.subTest(sentence=sentence[:48]):
                self.assertTrue(
                    mixed_script_words(sentence),
                    "a homoglyph spelling is no longer refused as a mixed-script word")
                self.assertFalse(
                    flagged(_plain(sentence)),
                    "flagged() now catches a homoglyph — say so above, and move the "
                    "case into BYPASSED")

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
