"""Which credentialed MCP command this operator has seen — and therefore approved.

A persona's ``personas/<name>/mcp.json`` declares an MCP server, and a ``secrets`` map on
that entry turns it into ``charter secret exec <vault> --env … --exec -- <command>``. The
mechanism is right: the value reaches the server's environment without passing through a
context window. What was missing is that **the same committed file chooses the command**
(#330). `sync-agents` then writes that argv into ``.claude/agents/<name>.md``, which the
harness loads and whose stdio servers it starts.

**Why this is consent and not an allowlist.** #317 was the same shape on a news ``check:``,
and PR #319 closed it with a list of commands a probe may run. That works there because a
``check:`` names a *charter subcommand* — a closed grammar charter defines, so "which
commands may run" has an enumerable answer, and `news._pass_through` can even read the
dangerous shape off the argparse parser rather than naming it. None of that transfers. An
MCP ``command`` is an arbitrary binary followed by arbitrary ``args``, so a list holding
the launchers real servers use (``npx``, ``uvx``, ``docker``, ``node``) is walked past by
``args`` alone — ``uvx --from git+https://… evil`` is #332's mechanism one field over —
and a list excluding them refuses every MCP server anyone actually runs. The axis with an
answer is not *what* the command is but *whether the operator has seen it*.

**The record IS the line.** :func:`fingerprint` is the SHA-256 of what :func:`describe`
printed and nothing else is mixed into it, so *two entries that render the same consent
line have the same fingerprint*. That is one line of code and it is the whole of round
four. Three earlier rounds kept two representations — a digest over the entry, a line over
a list of fields — and every bypass since was one field that lived in the first and not the
second: the vault name, ``env`` VALUES, ``cwd``, a clipped tail. Each time the operator was
correctly re-asked and shown a line byte-identical to the one they had already approved,
which is not consent but a second chance to make the same mistake. With one representation
there is nothing left to fall out of step.

The whole weight then rests on :func:`describe` being TOTAL — it loops over the entry's
keys rather than over a list, because `persona.mcp_render_entry` hands every key it does
not consume to the harness — and on an entry it cannot render not being approvable at all,
because the consent line IS the consent: an ``http`` server used to print a blank one under
the words "read the command above" (#427).

**What this is, and is not.** A guard against a COMMIT — a committed file changing under an
approval already given — answered by a person reading one line. ``SECURITY.md`` states the
boundary and nothing here exceeds it: an attacker who already runs code as this user can
edit the record under ``STATE_DIR``, the harness, or charter itself.

**Machine-local and gitignored, deliberately.** Under ``STATE_DIR``, the same as
:mod:`charter.guardseen` and for a sharper reason: if the approval travelled in git, the
commit that declares the server could also declare that the server was approved, which is
the finding restored with an extra step.

**Withholding, not refusing.** An unapproved server is still written to the agent file —
only its credential is withheld. Deleting the server would break a working persona to
prevent a hypothetical, and charter's rule is additive: name the blocker, refuse the
dangerous half, and leave everything else working. The server starts and fails to
authenticate, which is a visible failure rather than a silent one.

**A server NAME is not a label, and this is the sentence that cost the most.** The digest
is of the DESTINATION — :func:`describe` renders the vault and every key of the entry, and
:func:`fingerprint` hashes that string — and the name under which the entry was declared is
deliberately outside it, printed beside the line by :func:`label` as the identifier it is.
The reasoning for leaving it out was that a name merely says where an entry was declared,
not what would run. That was false for four review rounds. The name was interpolated raw
into the generated agent's YAML frontmatter as a bare mapping key, so a newline in it
declared a whole second server, outside the fingerprint and outside the prompt (#453).
The carrier entry declared no ``secrets``, which put it out of scope for consent
altogether: `fingerprint` returned ``None``, the withheld report had nothing to say about
it, and the run printed one green tick. It is true now only because
`persona._MCP_NAME_RE` makes it true — a name is bounded at the boundary that reads the
committed file, and the emission serialises the key through `contain.json_line` rather than
pasting it — and it stops being true the moment either of those is loosened without the
other. :func:`label` escapes the name a second time on the way to the screen, because that
is a different surface with its own finding (see :func:`label`), not a duplicate of this one.

The older wording is kept here as a quotation, corrected, rather than deleted: a branch in
flight that restores it is a regression and not a merge conflict to take either side of, and
a reviewer can only see that if the sentence it costs is written down next to the reason.

**Nothing here raises.** A missing or corrupt marker reads as *nothing approved*, so the
failure direction is "the credential was withheld", never "sync-agents crashed" and never
"the credential was handed over because the file was unreadable".
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

from . import config, contain

#: One file per plane, under the state dir. Never committed — see the module docstring.
FILE_NAME = "mcp-approved.json"

#: Printed in place of a consent line for an entry :func:`describe` cannot render. Such an
#: entry is reported as withheld and refused for approval, rather than silently dropped.
#: Covers both reasons: no destination at all, and a destination too big to show in full.
UNRENDERABLE = "(charter cannot show this entry in full — nothing to approve)"

#: The narrowest terminal charter assumes, and the most rows one PRINTED consent line may
#: take on it. Their product is the hard ceiling on that line — label, decoration and
#: destination together, because all three are on the same screen.
MAX_COLS = 80
MAX_ROWS = 10

#: Longest either half of the ``persona/server`` label prints as. See :func:`label`.
MAX_NAME = 35

#: The whole label as printed: both halves and the ``/`` between them.
MAX_LABEL = MAX_NAME * 2 + 1

#: Everything charter itself puts on a printed consent line and nobody committed: the
#: ``• `` bullet `util.info` prefixes, the two-space indent, and the ``→`` with a space on
#: each side. Counted here because the ceiling below is a screen, and a screen does not
#: care who put the columns on it.
_DECORATION = len("• ") + len("  ") + len(" → ")

#: Ceiling on the DESTINATION half — what :func:`describe` may return. Nothing is ever
#: shortened to fit it: an entry whose full rendering is longer than this is one the
#: operator cannot be shown in full, so it is not renderable and (via :func:`fingerprint`)
#: not approvable. That is what :data:`UNRENDERABLE` has always said out loud, and round
#: three's per-part clipping quietly contradicted it — a clipped part is a part of the
#: entry the operator did not see, and :func:`fingerprint` now digests only what they did.
#:
#: This is a SCREEN, not a byte count, and that is the whole reason it exists. The
#: operator answers the prompt printed *under* this line, so a line taller than the
#: terminal has already scrolled the command it names off the top by the time the
#: question is asked. Round two set it to 2000 — twenty-five rows of an 80-column tty —
#: and nine args of 200 padding columns each fit inside it with the destination out of
#: view. Escaping (see :func:`_esc`) makes such padding visible; it does not make it
#: short, so the ceiling has to be the screen itself.
#:
#: The label and the decoration are SUBTRACTED rather than ignored, which is the round
#: after that one: a ceiling that bounds the part charter was looking at and not the line
#: it prints is bounded by whatever the attacker puts in the other part. A committed
#: server name of a hundred thousand characters printed twelve hundred rows in front of a
#: destination that was itself comfortably inside the ceiling.
MAX_LINE = MAX_COLS * MAX_ROWS - MAX_LABEL - _DECORATION

#: Two or more ASCII spaces. After :func:`_safe` escapes every codepoint outside printable
#: ASCII, the ASCII space is the ONLY character that can still reach a consent line and
#: render as nothing — so collapsing this run is the whole blank class, not one member.
_SPACE_RUN = re.compile(" {2,}")


def path() -> Path:
    return Path(config.STATE_DIR) / FILE_NAME


def declares_credential(entry: dict) -> bool:
    """Does *entry* ask for a vault value at all — through ``secrets`` OR ``secret_files``?

    The vault-free half of :func:`needs_consent`, split out because a second caller asks
    this exact question about a persona that has **no** vault, where `needs_consent` is
    False by construction and therefore cannot be that caller's answer.

    `persona.lint` was that caller, and it asked with ``entry.get("secrets")`` alone. The
    two keys are two MECHANISMS for one thing — `secrets` puts a value in the environment,
    `secret_files` materialises a 0600 file and puts its path there (#190) — and every
    other reader treats them as one: `needs_consent` here, `persona.mcp_render_entry` when
    it decides whether to wrap the command, and `charter secret exec`'s own `--env`/`--file`
    pair. `lint` was the odd one out, so a server declaring only `secret_files` against a
    persona naming no vault rendered with no credential and no finding on any surface —
    and `secret_files` is not the exotic half: it is what Google ADC needs, which is the
    very declaration #489's own reproduction uses.

    One function, so the next key of this kind is added in one place rather than in the
    three that happen to be remembered.
    """
    if not isinstance(entry, dict):
        return False
    secrets, files = entry.get("secrets"), entry.get("secret_files")
    return bool((isinstance(secrets, dict) and secrets)
                or (isinstance(files, dict) and files))


def needs_consent(vault: str | None, entry: dict) -> bool:
    """Would rendering *entry* hand *vault*'s value to the command a committed file names?

    The one question that decides whether there is anything to consent to, asked in one
    place so the approve path, the withheld report and the digest cannot disagree about
    which servers are in scope. Kept separate from :func:`fingerprint` because a digest of
    ``None`` now means "no approval can exist", which includes entries that ARE in scope
    and must still be reported.
    """
    return bool(vault) and declares_credential(entry)


def fingerprint(vault: str | None, entry: dict) -> str | None:
    """What the operator is being asked to approve, as one digest — **of the line itself**.

    ``None`` when no approval can exist for this entry, which is two cases and both mean
    "render it without the vault wrapper":

    * **Nothing to consent to** — no ``secrets`` and no ``secret_files``, or no vault. The
      entry is passed through untouched by `persona.mcp_render_entry`, so no credential is
      at stake and requiring approval would be a prompt about nothing.
    * **Nothing to show** — :func:`describe` cannot render this entry, so the operator
      would be approving a blank line (#427). An entry nobody can be shown is not an entry
      anybody can approve. "Cannot be shown" is two properties: it names no destination at
      all (only ASCII spaces survive :func:`_safe`, so "renders as nothing" is decidable
      rather than enumerable), or it does not fit on the screen the question is asked on
      (:data:`MAX_LINE`). Round two decided the first on ``str.isprintable``, which is
      true of U+3164 HANGUL FILLER — so a line blank on every terminal got a real digest
      and was approvable.

    **The digest is the SHA-256 of the consent line, and nothing else is mixed in.** That
    is the whole of round four, and it is one line of code because the property is
    structural rather than enumerated:

        *two entries that render the same consent line have the same fingerprint.*

    Rounds one to three digested a parallel representation of the entry — a list of
    fields, then the whole entry through a canonicaliser — while :func:`describe` rendered
    a different, shorter list. Every bypass since has been one instance of that one gap,
    and each round closed the instance it was shown: the vault name was digested and never
    printed; ``env`` VALUES were digested and only their KEYS printed; ``cwd`` — and
    whatever the next committed key is — was digested and never printed; and the per-part
    clip made two different ``args`` print the same tail. In each, the operator was asked
    a second time under a line byte-identical to the one they had already approved, which
    is not consent but a second chance to make the same mistake.

    Hashing the line closes the gap in the only direction that cannot grow a new instance:
    there is no second representation to fall out of step with. It moves the whole weight
    onto :func:`describe` being TOTAL — a key it fails to print is a key a commit may
    change with the approval intact — so `describe` renders every key of the entry by
    construction rather than by enumeration, and `tests/test_mcp_approval.py` asserts that
    directly.

    What this is NOT: a guarantee that the operator understands the line, or that a
    different mechanism cannot re-point the same command. `SECURITY.md` states charter's
    actual scope — a guard against mistakes, not against an attacker who already runs code
    as this user — and nothing here exceeds it.
    """
    if not needs_consent(vault, entry):
        return None
    line = describe(vault, entry)
    if not line:
        return None
    return hashlib.sha256(line.encode("utf-8")).hexdigest()


def _read() -> dict:
    try:
        doc = json.loads(path().read_text())
    except (OSError, ValueError):
        return {}
    return doc if isinstance(doc, dict) else {}


def approved(persona_name: str) -> set[str]:
    """Fingerprints this operator has approved for *persona_name*. Never raises."""
    entry = _read().get(persona_name)
    return {f for f in entry if isinstance(f, str)} if isinstance(entry, list) else set()


def approve(persona_name: str, fingerprints) -> None:
    """Record *fingerprints* as this persona's approved set, REPLACING what was there.

    Replacing rather than adding is what makes the record self-pruning: a server the
    persona no longer declares stops being approved the next time the operator approves,
    so a stale entry cannot come back to life under a re-added server name.
    """
    doc = _read()
    doc[persona_name] = sorted({f for f in fingerprints if f})
    p = path()
    config.private_mkdir(p.parent)
    config.write_for(p, json.dumps(doc, indent=2, ensure_ascii=False) + "\n")


def _escape(ch: str) -> str:
    """One codepoint as an escape no other codepoint can also spell.

    Astral planes get the eight-digit ``\\U`` form rather than a long ``\\u``, because
    ``\\u1f600`` is five hex digits: U+1F600 and the two characters U+1F60 + ``0`` would
    render the same, and two different commands that read the same on a consent line is
    the homoglyph finding with a different alphabet.

    Lives in :mod:`charter.contain` now, because the same rule decides the same question on
    a second report (#498) and two copies of an escape table is how the two reports come to
    disagree about what a value is called. Kept as a name here so the reasoning above sits
    beside the consent line it was written for.
    """
    return contain.escape_char(ch)


def _safe(text: str) -> str:
    """*text* as printable ASCII — every other codepoint shown as its ``\\uXXXX`` escape.

    Every field here comes out of a committed file and the line IS the consent, so the
    question this answers is not "is this character printable" but "does what the operator
    reads still say what would run". Two earlier spellings of this guard each matched
    something narrower than that question, and each was walked past — in order:

    * a ``\\r`` or an ``ESC[2K`` in ``args`` repaints the line and a U+202E bidi override
      reverses it — caught by ``str.isprintable``, which is where round one stopped;
    * ``str.isprintable`` is nonetheless **true** for U+3164 HANGUL FILLER, U+2800 BRAILLE
      PATTERN BLANK and U+115F/U+1160. All are ``isspace() == False``, survive ``strip``,
      and render as nothing on every terminal, so a ``command`` of three of them was a
      line blank to the reader and truthy to charter — round one's ``"   "`` one spelling
      on, and round two's regex over the ASCII space did not reach it;
    * a U+0301 combining acute is printable and is neither, and repaints the rows above
      and below the line; Cyrillic ``а``/``с`` are printable and are neither, and spell an
      endpoint that reads identically to the ASCII one it re-points to, so an operator
      re-asked about a homoglyph cannot see what changed.

    No list of codepoints answers that, because the next spelling is always one codepoint
    further out. The class that does is the **complement**: printable ASCII is what a
    consent line may contain, and everything outside it — any category, any plane, any
    combining mark, any lookalike — is shown as its escape rather than its glyph. MCP
    commands, args, urls and env keys are ASCII in practice; anything else on a consent
    line is a reason to show the escape, not the glyph.

    Emptiness is then decided on the **escaped** form, which is what turns "renders as
    nothing" from a growing list into a decidable question: the escaped string holds only
    U+0020..U+007E, and the ASCII space is the only member of that range that renders as
    nothing. Collapsing runs of it and stripping the ends therefore returns ``""`` **if
    and only if** the part was nothing but ASCII spaces. That is an argument about the
    whole class, not a sample of it — and `tests/test_mcp_approval.py` checks it by
    sweeping every codepoint Python can hold rather than by listing four.

    The backslash escapes to ``\\\\`` for the same reason the astral form is eight digits:
    so that **every** ``\\uXXXX`` on a consent line is a codepoint that was really there.
    Without it, a committed ``command`` holding the six literal characters ``\\u3164``
    reads exactly like one holding U+3164 — one more pair of different commands the
    operator cannot tell apart by reading the line. A Windows path shows as
    ``C:\\\\Users\\\\x``; that is the cost, and it is unambiguous.

    **What this is for, and what it is not for.** Collapsing space runs and stripping the
    ends is what makes "renders as nothing" decidable — and it is exactly why this is NOT
    the function that renders a destination. It is lossy: ``a  b`` and ``a b`` come back
    the same, and so do ``  /bin/sh`` and ``/bin/sh``. That was harmless while the digest
    was computed from the entry; now that the digest IS the line (:func:`fingerprint`), a
    lossy rendering would be an approval covering an entry the operator never saw. So this
    is used for two things only — the ``persona/server`` label, where a name is an
    identifier rather than a destination and is clipped anyway, and the "does this entry
    name anything at all" test in :func:`describe`. The destination goes through
    :func:`_esc`, which loses nothing.

    The escape itself is `contain.escaped` — one implementation, because `contain.readable`
    decides the same question for the lint row and the "does not load" sentence (#498), and
    a second copy of this table is how those two reports come to spell the same name two
    ways. What stays here is the part that is about a CONSENT line and nothing else: the
    collapsing and the stripping below.
    """
    return _SPACE_RUN.sub(" ", contain.escaped(text)).strip()


def _esc(text: str) -> str:
    """*text* as printable ASCII, **reversibly** — the rendering the digest is taken over.

    The same complement argument as :func:`_safe`: printable ASCII is what a consent line
    may hold and everything else is shown as its fixed-width escape. Two differences, both
    forced by :func:`fingerprint` hashing the line rather than the entry:

    * **nothing is lost.** No collapsing, no stripping, no clipping. Every codepoint of
      *text* is recoverable from the output — ``\\\\`` for a real backslash, ``\\"`` for a
      real quote, ``\\uXXXX``/``\\UXXXXXXXX`` for everything outside printable ASCII, and
      itself for the rest — so the characters printed for a committed value determine that
      value. `tests/test_mcp_approval.py` decodes the output and checks it round-trips,
      which is what makes "reversible" a checked property rather than a claim.
    * **the ASCII quote is escaped**, which is what lets an unescaped ``"`` be a delimiter
      no committed byte can spell. :func:`describe` leans on that: charter's own words are
      printed bare, committed strings are printed between quotes, and the two cannot be
      confused for each other.

    A run of spaces survives as a run of spaces. It is visible between the quotes that
    :func:`_tok` and :func:`_val` put around it, and — being neither collapsed nor cut — it
    counts its full width against :data:`MAX_LINE`, so padding is refused rather than
    silently tidied away into a line that no longer says what would run.

    `contain.escaped(…, quote=True)` is the escape, shared with :func:`_safe` and with
    `contain.readable` (#498). The quote flag is the whole of the difference between the
    two calls, which is what the flag exists to make visible.
    """
    return contain.escaped(text, quote=True)


def _tok(text: str) -> str:
    """One argv word of the ``run`` segment: bare when it is one word, quoted when not.

    ``run uvx some-reddit-mcp --read-only`` is the common case and it should read like the
    command it is. A word that is empty, or that holds a space, is quoted instead, so the
    single space between words stays a boundary the reader and the digest agree on:
    ``run npx "-y" "my server"`` cannot be confused with three separate words. :func:`_esc`
    escapes the quote itself, so a bare word can never begin with one and the two forms
    stay tellable apart.
    """
    shown = _esc(text)
    return shown if shown and " " not in shown else f'"{shown}"'


def _val(value) -> str:
    """Any JSON value as one self-delimiting piece of a consent line.

    Strings are quoted, so charter's own bare words (``url``, ``env``, ``vault`` …) are
    never confused with committed text that happens to spell them. ``true``/``false``/
    ``null``/numbers keep their JSON spelling, so the string ``"true"`` and the boolean
    ``true`` do not read the same. Lists and objects nest with their JSON punctuation, and
    object keys are sorted, because JSON object order is not meaning — a prompt that fires
    on a re-serialised file is a prompt the operator learns to answer without reading.

    A value JSON cannot carry is tagged rather than stringified, so an exotic object
    cannot read as the plain string that happens to be its ``repr``.
    """
    if isinstance(value, str):
        return f'"{_esc(value)}"'
    if value is None:
        return "null"
    if isinstance(value, bool):            # before int: bool IS an int
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return _esc(json.dumps(value))
    if isinstance(value, (list, tuple)):
        return "[" + ", ".join(_val(v) for v in value) + "]"
    if isinstance(value, dict):
        return "{" + ", ".join(f"{_val(str(k))}: {_val(v)}" for k, v in _sorted(value)) + "}"
    return f"<not-json {_val(repr(value))}>"


def _sorted(mapping: dict):
    """*mapping*'s items in an order that depends on the items and not on the file."""
    return sorted(mapping.items(), key=lambda kv: (str(kv[0]), _val(kv[1])))


def _pairs(mapping: dict) -> str:
    """A ``{VAR: value}`` map as ``"VAR"="value"``, the shape it reaches the process in.

    Used for ``env``, ``secrets`` and ``secret_files`` so the line reads like the
    ``secret exec --env VAR=key`` argv the answer authorises rather than like a summary of
    it. Both halves go through :func:`_val`, so the ``=`` and the ``, `` between pairs stay
    charter's punctuation: ``{"A": "b, C=d"}`` and ``{"A": "b", "C": "d"}`` are two
    different environments and they print as two different lines.
    """
    return ", ".join(f"{_val(str(k))}={_val(v)}" for k, v in _sorted(mapping))


def _name(part) -> str:
    """One half of a :func:`label`: printable ASCII, never wider than :data:`MAX_NAME`.

    Clipped with a FIXED marker, and a bound is the only thing standing between a
    committed name and the rows it costs. A marker that counted what it cut would not be
    one: its own width grows with the input it describes, and a budget a longer input
    makes longer is not a budget.

    Clipped at all, unlike the destination, because a label is an IDENTIFIER — it says
    where the entry was declared, not what would run — and it is not in the digest. The
    destination is never shortened: see :data:`MAX_LINE`.

    A half that renders as nothing shows as ``""`` rather than as an invisible gap, so
    ``reddit/""`` reads as a server whose name is blank instead of as a missing word. On
    the destination side :func:`_val` and :func:`_tok` reach the same place by quoting
    everything committed, which is stronger: there, the empty string is one more value
    that reads as itself.
    """
    shown = _safe(str(part))
    if len(shown) > MAX_NAME:
        shown = shown[:MAX_NAME - 3] + "..."
    return shown or '""'


def label(*parts) -> str:
    """The ``persona/server`` a consent line puts in front of the destination.

    :func:`describe` has been hardened three times, and this half of the same printed line
    reached the terminal untouched on all three — which is this fix's own lesson arriving
    at its own expense. Each round put the guard on the FIELD that was attacked rather
    than on the SURFACE it is printed on, so the next spelling only had to move one field
    over. It moved here.

    The ``server`` half is the live one: it is a key of a committed ``mcp.json``, so it is
    an arbitrary JSON string, of arbitrary length, in any script. Confirmed end to end
    through ``sync-agents --approve-mcp`` before this existed — a server named with three
    U+3164 HANGUL FILLERs printed ``reddit/ → uvx some-reddit-mcp`` with nothing between
    the slash and the arrow; one carrying an ANSI erase wiped charter's own words standing
    beside it and repainted the row from column zero; a bidi override reversed the line;
    and a name of a hundred thousand characters printed twelve hundred rows before the
    destination reached the screen, with :data:`MAX_LINE` satisfied throughout because
    :func:`describe` never saw the name.

    The ``persona`` half is *already* contained: it is a directory under ``personas/``, and
    `persona.reference_ok` refuses any reference outside ``[a-z0-9][a-z0-9._-]*`` (#328),
    so a persona whose name is not printable ASCII resolves to nothing and never reaches
    this line. It goes through the same escape anyway, because charter joins guards rather
    than choosing between them — this one still holds if that alphabet is ever widened.
    And the half of it that is not hypothetical: `valid_name` bounds the ALPHABET and not
    the LENGTH, so only the clip here keeps a 255-character persona directory off four
    rows of the screen the question is asked on.
    """
    return "/".join(_name(p) for p in parts)


#: The keys :func:`describe` has a READABLE form for. It chooses how a key is shown and
#: never whether it is shown: every other key of the entry is rendered too, generically,
#: under its own quoted name. Adding a key here changes a line's wording, never its
#: coverage — which is the difference between this and the enumerations of rounds one to
#: three, each of which was walked past by the field it did not list.
_READABLE = ("command", "args", "type", "url", "env", "secrets", "secret_files")

#: Charter's own words on a consent line, each introducing one segment. They are printed
#: BARE; every committed string is printed between quotes (:func:`_esc` escapes the quote
#: itself), so no committed key can spell one of these and no segment can be mistaken for
#: another. This tuple is documentation and a test's checklist, not a guard.
_WORDS = ("run", "command", "args", "type", "url", "env", "secrets", "secret_files",
          "vault")


def _names_something(entry: dict) -> bool:
    """Does this entry name a destination at all — a command, an argument, or a url?

    Decided on the :func:`_safe` form, which is what makes it a decidable question rather
    than a growing list: after escaping, a part is printable ASCII, and the ASCII space is
    the only member of that range that renders as nothing. So a part is blank exactly when
    it held nothing but ASCII spaces. Round one tested ``""`` and missed ``"   "``; round
    two tested ``"   "`` and missed U+3164 HANGUL FILLER, which is printable, is not
    whitespace, survives ``strip`` and shows as nothing. Neither is a special case here.
    """
    raw = entry.get("args")
    argv = list(raw) if isinstance(raw, (list, tuple)) else ([raw] if raw else [])
    parts = [entry.get("command") or ""] + argv + [entry.get("url") or ""]
    return any(_safe(str(p)) for p in parts)


def describe(vault: str | None, entry: dict) -> str:
    """The whole of what would be approved, as the line the operator is asked to read.

    **This function IS the consent, and :func:`fingerprint` is its SHA-256.** That is the
    invariant round four exists for, and it is the reason this returns a rendering of the
    *entire* input rather than of the fields charter happens to know about:

        *two entries that render the same consent line have the same fingerprint, and an
        entry that renders a different line lapses the approval.*

    Both halves of that only hold if nothing reaches the harness without reaching the
    line, so the loop below is over ``entry`` — every key of it — instead of over a list
    of fields. A key charter has never been taught prints under its own quoted name and
    its JSON value: ``"cwd" "/tmp/attacker"``. `persona.mcp_render_entry` hands exactly
    those unconsumed keys to the harness, and the harness sets them on the process that
    ``execvpe``s the server, so a key it passes through and this line skipped would be a
    committed edit with the approval intact. That was ``env`` in #426, ``cwd`` and the
    vault after it, and it will be some other key next — which is why this is a loop.

    **What the line says, in order.** ``run`` and the argv; then whichever of ``args``,
    ``type``, ``url``, ``env``, ``secrets`` and ``secret_files`` are present and were not
    already spent on ``run``; then every remaining key, sorted, under its quoted name;
    then ``vault``. Charter's own words are bare and committed text is quoted, so the two
    are never confused.

    * **``env`` prints its VALUES, not only its keys.** The key was the half being shown
      and the value is the half that decides: ``PATH`` chooses which binary ``execvpe``
      finds, ``NODE_OPTIONS`` chooses what it loads. An ``env`` value is committed
      plaintext out of ``mcp.json`` — it is not a vault value and never was, and this
      module never opens a vault — so printing it discloses nothing that reading the repo
      would not.
    * **``vault`` is named.** ``vault:`` is a key of the committed ``persona.md``, so a
      one-line commit re-points which credential is spent. The digest covered it from the
      start and the line never did, so even a FIRST prompt could not say whose credential
      was at stake — while printing charter's own word ``(vault: …)`` in front of the
      variable name, where a reader has every reason to expect the vault to be.
    * **``secrets``/``secret_files`` name the vault KEY.** They are what
      `mcp_render_entry` turns into ``--env VAR=<vault key>`` and ``--file VAR=<vault
      key>``, so the key named there decides *what value* the command receives. A vault
      KEY name is not a credential; the value is, it is not in the entry, and it cannot
      reach this line because it never reaches this process.

    **Nothing is shortened.** Round three clipped each part to two hundred characters and
    announced the cut, which bounded the line but was not one-to-one: two ``args`` that
    agree on their first two hundred escaped characters and are the same length print the
    same tail and, once the digest is the line, would share one approval. A part the
    operator did not see is a part they did not consent to, so an entry whose full
    rendering exceeds :data:`MAX_LINE` is refused instead — the same fail-closed answer
    the ceiling has always given, applied to one more way of not being readable.

    ``""`` when there is no line, which :func:`fingerprint` turns into "not approvable":

    * **Nothing named** — no ``command``, no ``args``, no ``url``, or nothing among them
      that renders as more than ASCII spaces. An ``http`` server has no command, and
      building the line from ``command`` + ``args`` alone rendered it as an EMPTY string
      under the words *"Read the command above"* (#427). See :func:`_names_something`.
    * **Too much named** — the rendering does not fit on the screen the question is asked
      on (:data:`MAX_LINE`: :data:`MAX_ROWS` rows of an :data:`MAX_COLS`-column terminal,
      less the label and decoration beside it). Charter will not print a page of
      destination and call it a line the operator read, and it will not print half of one
      either. The ceiling is a screen because the operator answers the prompt printed
      UNDER this line: round two set it to 2000 characters, twenty-five rows, and nine
      args of 200 padding columns fit inside it with ``uvx evil-server`` scrolled off the
      top.

    **One equivalence is deliberate, and it is the only one.** An absent ``args`` and an
    empty ``args`` both print as bare ``run <command>``, because both hand ``execvpe`` the
    same argv — `mcp_render_entry` reads them through the same ``or []``. They share one
    approval on purpose. Dict key ORDER is the same kind of non-difference and is sorted
    away for the same reason. Everything else about the entry is on the line.
    """
    if not isinstance(entry, dict) or not _names_something(entry):
        return ""
    spent, shown = set(), []

    # `run`, the headline: the command and, when they are plain strings, its arguments.
    # An argv that is not a list of strings is not an argv, so it prints under `args`
    # instead of being flattened into words that would read like several of them.
    command, argv = entry.get("command"), entry.get("args")
    if isinstance(command, str):
        spent.add("command")
        words = [_tok(command)]
        if isinstance(argv, (list, tuple)) and all(isinstance(a, str) for a in argv):
            spent.add("args")
            words += [_tok(a) for a in argv]
        shown.append("run " + " ".join(words))

    for key in _READABLE:
        if key not in entry or key in spent:
            continue
        spent.add(key)
        value = entry[key]
        body = (_pairs(value) if key in ("env", "secrets", "secret_files")
                and isinstance(value, dict) and value else _val(value))
        shown.append(f"{key} {body}")

    # Everything charter has not been taught. NOT a fallback for a case somebody thought
    # of — this is the case, and the named ones above are its readable spellings.
    for key in sorted((k for k in entry if k not in spent), key=str):
        shown.append(f"{_val(str(key))} {_val(entry[key])}")

    shown.append(f"vault {_val(str(vault))}")
    line = "  ".join(shown)
    return "" if len(line) > MAX_LINE else line
