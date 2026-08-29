"""What a version brought, and whether THIS plane has taken it up.

A *news entry* is a shipped, per-item note that a version introduced something, carrying
an optional probe for whether this plane has adopted it. Not a changelog: an entry exists
to be **acted on**, and one with nothing to adopt is one line.

Seven properties are load-bearing.

**One entry, two consumers, one answer.** The GitHub Release body and the offline `charter
news` suggestion are the same entries rendered twice — `release.yml`'s announce job pipes
`charter news --for <version>` straight into `gh release create --notes-file`. So anything
that decides how an entry is presented has to be decided once, where both reach it, and
never at a call site. It was not, and #486 is what that cost: ORDER was left to
`sorted(glob("*.md"))`, which for a stamped release is alphabetical by slug, so 0.52.0's
vault-spending fix rendered eighth, under a docs correction. :func:`all` now applies the
declared order and :func:`marker` the label, and both views come through them.

**And the body FITS where it is sent.** That pipe ends at an API with a limit —
:data:`RELEASE_BODY_MAX` — which refuses an over-long body outright rather than trimming
it, in the `announce` job, which is `needs: publish`: *after* the PyPI upload, which cannot
be undone, and out of reach of the documented retry, which re-enters `publish` and is
rejected for a version PyPI already has. Rendering was never the failing step and the
release guard was never wrong to check it; the claim nobody was making is this one.
:func:`render_body` therefore bounds what it returns and :func:`commands.cmd_news` refuses
what it cannot bound, so the refusal lands in `guard` — before `test`, `build` and
`publish` — and 69 entries stay a releasable release instead of becoming a release-stopper
nobody sees until the irreversible step is behind them.

**What an entry declares is honoured or reported, never neither.** An entry is a committed
file and the release notes are the one document nobody re-derives, so an entry that does
not render is indistinguishable from an entry nobody wrote. :func:`_flag` holds that line
for a VALUE charter cannot read. It was held for nothing else: `Security: true` is a
different dict key from `security:`, so `meta.get("security")` found nothing, the entry
sorted as though it had declared nothing, and `charter news --for` — the release gate —
exited 0 with an empty stderr (#503). :data:`_KNOWN_FIELDS` closes the key half, and
:func:`unreadable` closes the half below it, where a miscased ``Version:`` costs the file
its `Entry` altogether and no per-entry check can be asked about it.

And the report saying so is charter's own output, so every span in it is contained as the
sentence is assembled (`contain.sentence`) rather than at the spans somebody was thinking
about: the ordering VALUE was contained and the committed FILENAME beside it was not, so a
filename holding a newline forged an extra line of charter's report (#502).

**It ships in the wheel.** Entries travel with the code that implements them, resolved the
way :mod:`charter.docsrc` resolves documentation — packaged copy first, the repo's
``docs/news/`` as the checkout fallback. A control plane has no reason to vendor a copy and
every reason not to: a copy drifts from the binary, invisibly and in both directions.

**A probe is checked, never assumed.** ``check:`` answers "does this plane already have
it?" with an exit code. A probe that *cannot run* is :data:`UNKNOWN` — not "adopted" and
not "pending". Reporting it as pending invents work; reporting it as adopted hides the
entry forever. This is `doctor`'s ``_NOT_CHECKED_HINT`` in another costume: the absence of
information is not evidence of health (ADR 0013).

**A probe reads, and its argv is charter's rather than the entry's.** That is the whole
restraint, and it is a property of the *command* — being dispatched in-process is not it.
`_tokens` refusing shell syntax and an unregistered first token was, and the claim built on
it was false for as long as it stood: `secret exec` takes the rest of the line as a
pass-through argv, so a `check:` naming it reached any binary on the machine with a vault's
credential in the child's environment, on every plane that upgraded, from a SessionStart
hook (#317). So an entry now chooses from :data:`_PROBEABLE` — command paths a human has
confirmed — rather than from the whole CLI.

A list, and not a rule derived from the parser, because only one half of the restraint is
derivable. argparse can be asked whether a command takes a pass-through positional; it
cannot be asked whether the command writes to the disk, and `check: update …` reaching a
real `uv tool install` is the same defect wearing no argv at all. What the parser can
answer is asserted over the list instead of at runtime — entries, list and parser ship in
one wheel, so a test across the three is a proof rather than a sample, and the SessionStart
path stays free of a walk through argparse's internals.

`docsrc._TOPIC` keeps the same restraint for `docs show` — "must not be a file-read
primitive wearing a documentation command" — and the stakes are higher here, because this
one runs rather than prints. Being in-process is what makes a dozen probes cheap enough for
`doctor` to run on demand; it was never what made them safe.

**A probe never runs from inside a probe.** Some charter commands probe every entry
themselves — `doctor` does, and so does `charter news --pending` — so an entry naming one
puts the dispatcher inside itself, a full sweep at every level, forever (#311). The guard
in :func:`_dispatch` refuses the nested call AND withholds the outer command's exit code,
because the two halves fix different things: refusing bounds it, withholding is what keeps
the answer honest.

**And that guard has to survive `exec`.** Some charter commands start another charter —
`commands_update._handoff` runs `charter news --since` in a fresh process of the newly
installed binary — so re-entry does not have to come back up this module's stack. A depth
counter is blind to that, which is why one also travels in the environment (:data:`_ENV`):
a charter started underneath a probe is inside that probe, whatever spawned it (#314). Both
halves cross with it — the refusal going down, and word of it coming back up, so an entry
whose command spawned a charter that declined is unchecked rather than adopted.

**A probe reads; it does not act.** The marker only ever reaches a CHILD, and the frightening
half of #314 was never in the child: `check: update …` runs a real `uv tool install` in the
process that IS the probe, at the depth the counter permits by design. So the mutation
declines on its own account — :func:`probing` is public for exactly that, and refusing is
only half of it, because a command that declined has no exit code worth reading.

`update` is not in :data:`_PROBEABLE`, so an entry naming it is now refused before any of
that. Both stay, because they answer different questions: the list says what an entry may
*name*, and :func:`probing` says what a command does when it finds itself inside a probe —
which a command reached from a listed one still is.
"""

from __future__ import annotations

import argparse
import io
import os
import tempfile
import urllib.parse
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from typing import NamedTuple

from . import contain, persona, update

#: The version a staged entry carries until a release stamps it. A feature PR cannot know
#: which version will ship it — the next release may be a patch, or the PR may sit through
#: three of them — so it does not guess. `charter news stamp <version>` renames the file
#: and rewrites this field. Until then the entry is invisible to every user-facing view:
#: an entry naming a version that was never true is the one failure staging exists to
#: prevent.
UNRELEASED = "unreleased"

ADOPTED, PENDING, UNKNOWN, INFORMATIONAL = "adopted", "pending", "unknown", "informational"

#: Anything a shell would treat as syntax. Present only as a belt to the braces: nothing
#: here is ever passed to a shell, so this rejects an entry whose AUTHOR believed it might
#: be — which is a broken entry either way, and better reported than silently truncated.
_SHELLISH = set(";|&<>$`()\\\n\"'")

_PACKAGED = Path(__file__).resolve().parent / "_news"
_CHECKOUT = Path(__file__).resolve().parents[1] / "docs" / "news"

#: How many :func:`_dispatch` calls are in flight, and — when the outermost one has no
#: answer to give — why. Plain module state rather than a :class:`~contextvars.ContextVar`:
#: a ContextVar reads its default in every new thread, so a probing command that fanned its
#: own work out to a pool would walk straight past the guard — which is the one failure this
#: exists to make impossible. The global's failure mode is the opposite and far cheaper: two
#: probes racing in different threads would make each other `unknown`, which is wrong but
#: bounded, honest, and not reachable today — `doctor` and `charter news` each walk their
#: entries in one thread.
_depth = 0
_refused: str | None = None

#: The same guard, in a form that survives `exec`. :func:`_dispatch` sets this for the
#: length of a probe, so every process started underneath one inherits it and declines to
#: probe: `commands_update._handoff` starts a fresh `charter news --since`, and `secret exec`
#: will start whatever an entry names. A counter in this module's memory sees none of that.
#:
#: The value is ``<pid>:<path>``, and it carries both halves of the guard.
#:
#: **The PID** is the process running the probe, which is what keeps the marker from
#: becoming the worse bug. An environment belongs to a process, so this cannot escape into
#: the shell that started charter; it is restored in the same `finally` that releases the
#: counter, so a probe that raises leaves nothing behind; and if a copy ever does turn up
#: somewhere charter did not put it, it names a process — one that no longer exists guards
#: nothing, and is ignored rather than believed. Believed, a scrap of stale environment
#: would turn every probe on that machine into `unknown` for good and say nothing about why.
#:
#: **The path** is where a descendant that was refused leaves a mark, because bounding the
#: loop is not the same as answering honestly. A child cannot reach into the memory of the
#: process probing, and its exit code belongs to whatever it was actually asked to do — so
#: without a way back up, a `check:` whose command exits 0 while the charter it spawned
#: quietly declined would report the entry ADOPTED. That is #311's second half, one process
#: further away.
_ENV = "CHARTER_NEWS_PROBE"


#: The two opt-in ordering fields, and the only two. `security:` is a CLASS — a version
#: may ship any number of security entries, and they sort above everything else. `lead:`
#: is a POSITION, so at most one entry per version may claim it; a second is a
#: contradiction :func:`entry_errors` reports rather than resolving.
#:
#: Split rather than collapsed into one `rank: <int>`, because the two answer different
#: questions and only one of them can be answered by an author working alone. "Is this a
#: security fix?" is a fact about the entry, knowable while writing it and true forever.
#: "Does this go first?" is a fact about the RELEASE, which the author cannot know — 24
#: entries were staged for 0.52.0 and none of their authors could see the other 23. A
#: numeric rank would make every author guess at that, and guess wrong quietly; these two
#: let an author state only what they actually know.
LEAD, SECURITY = "lead", "security"
_ORDERING_FIELDS = (LEAD, SECURITY)

#: Every frontmatter key a news entry may declare, and a CLOSED set: a key outside it is
#: reported by :func:`entry_errors` rather than read past.
#:
#: Closed because the alternative failure is silent. `persona.parse` keeps a key exactly as
#: written, so ``Security: true`` is the key ``Security``, ``meta.get("security")`` finds
#: nothing, and the entry declares a security fix, sorts as though it declared nothing, and
#: leaves `charter news --for` exiting 0 with an empty stderr (#503). That is #486's own
#: defect reached through the KEY instead of the value, and the key half fails the more
#: quietly of the two, because an unfound key leaves no value to report.
#:
#: **Loud rather than liberal, and the case is why.** Folding the lookup to lower case
#: answers ``Security:`` and nothing else: ``securiy:``, ``leads:``, ``security-fix:`` and
#: ``sec urity:`` all parse cleanly, are never looked up, and sink the entry in exactly the
#: same silence. Accepting more spellings is a guard against a spelling — the shape this
#: module's docstring names six times over — where the property is "a key the author
#: declared is honoured or reported, never neither". With no unspoken key left, case stops
#: being a special case of anything.
#:
#: There is a second reason not to fold. `persona.parse` keeps the LAST of two lines with
#: the same key, so a case-folding parser would owe an answer to
#: ``{"Security": "true", "security": "false"}`` — which wins, or does it refuse (#509).
#: Reporting needs no such rule, and needs no change to a parser whose dict is also read
#: for ``vault:`` and ``extends:``.
#:
#: Every name here is also an :class:`Entry` field, asserted rather than assumed by
#: `tests/test_a_news_key_is_honoured_or_reported.py`: a name added here that `_read` does
#: not actually read would be a key charter accepts and then ignores — the silence this set
#: exists to remove, wearing a commit.
_KNOWN_FIELDS = ("version", "headline", "check", "adopt") + _ORDERING_FIELDS


class Entry(NamedTuple):
    version: str
    slug: str
    headline: str
    check: str
    adopt: str
    body: str
    path: Path
    #: Declared position and class. Both default False — 24 entries do not each need a
    #: number, and an entry that says nothing sorts exactly where it always did.
    lead: bool = False
    security: bool = False
    #: ``(field, raw)`` for every ordering field whose value was not understood. Carried
    #: rather than raised, and rather than silently read as false, because false is the
    #: answer that SINKS the entry — an author who wrote `security: yes` and got the
    #: bottom of the release notes would have been failed by the field that was supposed
    #: to help them. :func:`entry_errors` turns these into sentences, and
    #: `charter news --for` — which is the release workflow's own gate — refuses on them.
    bad: tuple[tuple[str, str], ...] = ()
    #: Frontmatter keys this entry declared that charter does not read, in the order the
    #: file wrote them. Beside :attr:`bad` and for the same reason, one half of the
    #: declaration each: `bad` is a value charter could not read, this is a key it never
    #: looked up. Both are carried rather than raised, and neither is dropped, because
    #: dropping is what makes an entry that declared something indistinguishable from one
    #: that declared nothing (#503).
    unknown: tuple[str, ...] = ()


def _is_checkout(d: Path) -> bool:
    """True when *d* is the repo's own ``docs/news``, not a directory that merely sits
    where one would.

    Installed, ``_CHECKOUT`` resolves to ``<site-packages>/docs/news`` — a path belonging
    to nobody, which another distribution can create by shipping a stray top-level
    directory. `docsrc` carries this same guard for the same reason.
    """
    return (d.parents[1] / "pyproject.toml").is_file()


def _dir() -> Path | None:
    """Where entries live, packaged first. A developer with both is running one specific
    tree, and the packaged copy is the one that travelled with the code being executed."""
    if _PACKAGED.is_dir():
        return _PACKAGED
    if _CHECKOUT.is_dir() and _is_checkout(_CHECKOUT):
        return _CHECKOUT
    return None


def checkout_dir() -> Path | None:
    """The repo's own ``docs/news``, or ``None`` when this is not a checkout.

    Deliberately not :func:`_dir`. Reading prefers the packaged copy; *writing* has only
    one legitimate target, because ``charter/_news`` is force-included from ``docs/news``
    on every build. A stamp applied to the packaged copy is discarded by the next wheel —
    silently, and after the release engineer was told it worked.
    """
    return _CHECKOUT if _CHECKOUT.is_dir() and _is_checkout(_CHECKOUT) else None


def _read(p: Path) -> Entry | None:
    try:
        meta, body = persona.parse(p.read_text())
    except (OSError, UnicodeDecodeError):
        return None
    version = (meta.get("version") or "").strip()
    if not version:
        return None
    # The slug is the filename's, the version is the frontmatter's. Two sources for one
    # fact would drift the moment `news stamp` renamed a file and missed the field.
    slug = p.stem.split("-", 1)[1] if "-" in p.stem else p.stem
    flags: dict[str, bool] = {}
    bad: list[tuple[str, str]] = []
    for field in _ORDERING_FIELDS:
        # `field in meta`, not `meta.get(field) or ""`: absent and present-but-empty are
        # different facts, and `.get(…) or ""` is the line that made them one. See
        # :func:`_flag` — a `security:` whose value went onto the continuation line is a
        # declaration charter could not read, not a declaration that was never made.
        raw = meta[field].strip() if field in meta else None
        value = _flag(raw)
        if value is None:
            bad.append((field, raw or ""))
        flags[field] = bool(value)
    # In the file's own order, not sorted: an author reading the report is looking for the
    # line they typed, and `persona.parse` hands its keys back in the order it read them.
    unknown = tuple(k for k in meta if k not in _KNOWN_FIELDS)
    return Entry(version=version, slug=slug,
                 headline=(meta.get("headline") or "").strip(),
                 check=(meta.get("check") or "").strip(),
                 adopt=(meta.get("adopt") or "").strip(),
                 body=body, path=p,
                 lead=flags[LEAD], security=flags[SECURITY], bad=tuple(bad),
                 unknown=unknown)


def _flag(raw: str | None) -> bool | None:
    """An ordering field's declared value: True, False, or ``None`` for "not understood".

    ``raw is None`` means **the key is not in the frontmatter at all**, and that is False —
    the whole of what opt-in means, and why 24 entries needed no edit when this landed.
    Absence is not a value, so it is spelled as the absence of one; every ``str`` reaching
    here, ``""`` included, is something an author typed.

    **Empty is a declaration charter could not read, not a declaration never made**, and
    reading the two as one is how #486 came back through the field added to prevent it.
    charter's frontmatter is flat ``key: value`` — `persona.parse` drops any line without a
    colon — so the YAML habit of putting the value on the continuation line::

        security:
          true

    leaves ``security`` present with nothing after it, and so does ``security:`` typed with
    the value forgotten or with a trailing space. Folded into "absent", that entry declares
    a security fix, renders below the ordinary ones, and `charter news --for` exits 0 with
    an empty stderr: the exact silence #486 is about, from an author who did opt in. The
    first cut of this function did fold them, by way of ``(meta.get(field) or "").strip()``
    at the one call site — a spelling of "missing" that a present key also matches, which
    is the failure this module's docstring names six times over.

    Present-but-unrecognised is **None**, not False, and that distinction is the point of
    this function. The obvious spelling of "is this true?" is a truthy set —
    ``{"true", "yes", "1", "on"}`` — and it fails the way every other guard in this
    codebase has failed: it matches a spelling. The next author writes ``security: Y``,
    or ``yes  # per the security charter``, or a full-width ``ｔｒｕｅ``, and the set does
    not hold it, so the entry reads as false and sinks to the bottom of the release notes
    — silently, in exactly the position the field was added to prevent. Widening the set
    only moves that edge somewhere less obvious.

    So the property is not "which words mean yes" but **"was this value understood?"**.
    Two literals are recognised, case-folded; every other string an author typed is
    reported by :func:`entry_errors` naming the value it could not read. An author who
    writes something outside the pair is told so at the release gate rather than being
    quietly overruled by it.

    **The next spelling was not in this function: it was the KEY.** `persona.parse` matches
    exactly and case-sensitively, so ``Security: true`` never arrives as ``security`` and
    is genuinely absent here — this function is handed ``None`` and answers False, which is
    the right answer to the question it was asked and the wrong outcome for the entry
    (#503). The key half is answered where the key lives, by :data:`_KNOWN_FIELDS`: a key
    outside that set is a sentence rather than a shrug, so ``Security:`` is reported and so
    is ``securiy:``, which no amount of case-folding here would have caught.

    What is still open is the same dict keeping the LAST of two lines with one key, so an
    entry declaring ``security:`` twice has one of them dropped without a word (#509).
    That one is reached through `persona.parse` itself, whose result is also read for
    ``vault:`` and ``extends:``, and is filed rather than folded in here.
    """
    if raw is None:
        return False
    folded = raw.strip().casefold()
    return folded == "true" if folded in ("true", "false") else None


def rank(e: Entry) -> int:
    """Where *e* sits among its own version's entries: 0 leads, 1 is a security fix, 2 is
    everything else. Lower first."""
    return 0 if e.lead else 1 if e.security else 2


def marker(e: Entry) -> str:
    """The word that precedes a security entry's headline, everywhere a headline is
    rendered — ``""`` for an ordinary entry.

    One function rather than a literal at each call site, for the same reason `_dev_chip`
    is one function: the Release body and the offline `charter news` view are deliberately
    the same answer printed twice (that is #486's whole premise), and two copies of the
    label is how they start disagreeing. `lead:` gets no marker — it is a position, not a
    kind, and "this was listed first" is already visible from being listed first.

    ASCII, and a word rather than a glyph. It renders into a GitHub Release body, into a
    `TERM=dumb` CI log, and into a terminal charter does not control.
    """
    return "security: " if e.security else ""


#: An ordering field whose value charter could not read, quoted back to its author.
_BAD_VALUE = ("{name}: `{field}: {raw}` is not a value charter reads — a news ordering "
              "field is `true` or `false`. Left unread, this entry sorts as though it "
              "never declared anything.")

#: The same field declared with nothing after the colon. Distinct from :data:`_BAD_VALUE`
#: because the fix is different: there is no value to correct, and quoting one back
#: ("`security: `") would read like a rendering bug. Name the shape that produces it
#: instead — the value on the continuation line is the way this gets typed.
_EMPTY_VALUE = ("{name}: `{field}:` is declared with no value on that line — a news "
                "ordering field is `true` or `false`, written after the colon. charter's "
                "frontmatter is flat `key: value` and drops a line without a colon, so a "
                "value indented onto the NEXT line never reaches charter. Left unread, "
                "this entry sorts as though it never declared anything.")

#: Charter's own list of what an entry may declare, built from :data:`_KNOWN_FIELDS` so the
#: sentence cannot drift from the set it describes.
_FIELDS_SAID = ", ".join(f"`{f}:`" for f in _KNOWN_FIELDS)

#: A key that is one of charter's fields in another case. Its own sentence, because the
#: author has already written the right word and needs to be told only that the key is
#: matched exactly — pointing them at the whole list would make them hunt for a difference
#: that is not there.
_MISCASED_KEY = ("{name}: `{key}:` differs from `{known}:` only in case, and charter "
                 "matches a frontmatter key exactly — so this entry declared no "
                 "`{known}:` at all and was read as though the line were not there. "
                 "Spell the key `{known}:`.")

#: Any other key charter does not read. Reported rather than ignored because ignoring is
#: what makes `securiy: true` indistinguishable from a line nobody wrote (#503).
_UNKNOWN_KEY = ("{name}: `{key}:` is not a field a news entry declares, so nothing read "
                "it. A news entry declares " + _FIELDS_SAID + ", matched exactly — a near "
                "miss parses like anything else, is looked up by nothing, and would "
                "otherwise sink this entry without a word.")

#: Two entries in one version both claiming the position only one of them can have.
_TWO_LEADS = ("{version}: {count} entries declare `lead: true` ({names}) — only one entry "
              "can be the one a reader sees first. Leave `lead:` off all but one; a "
              "security fix that need not be first can say `security: true` instead, "
              "which any number of entries may.")


def entry_errors(entries: list[Entry]) -> list[str]:
    """What *entries* declare that charter cannot honour, as sentences.

    Empty is the ordinary answer. Three kinds of failure are reported, and none of them is
    resolved quietly, because a quiet resolution is what #486 was about:

    * an ordering field whose value was not understood (see :func:`_flag`), named with the
      value, so an author who wrote ``security: yes`` learns which word charter reads —
      and, when the field was declared with nothing after the colon, naming the shape
      instead of the empty value, because the author who wrote it almost certainly put
      the value on the next line and needs to be told that line is not read;
    * a key outside :data:`_KNOWN_FIELDS`, which is where ``Security: true`` lands and
      where ``securiy: true`` lands with it (#503). This is the quietest of the four:
      an unfound key leaves no value to report, so before it was reported the entry
      declared a security fix, rendered below the ordinary ones, and let the release gate
      pass with an empty stderr;
    * two entries in one version both declaring ``lead: true``. One of them is not going
      to be first, and picking silently would hand back the accident #486 already
      diagnosed — a position decided by something other than a person deciding it.

    Named for the ENTRY rather than for the ordering it used to be named for, because an
    unrecognised key is not an ordering claim and the narrower name is how the middle one
    would have ended up somewhere else, reported by nobody. What they share is the
    property: a thing the author declared is honoured or reported, never neither.

    Callers rather than this function decide the consequence: `charter news --for`, which
    IS the release workflow's publish gate, refuses; the range view warns and prints on.
    Every sentence is assembled through `contain.sentence`, so a committed filename in
    one cannot write a second line into the report it appears in (#502).
    """
    out: list[str] = []
    for e in sorted(entries, key=lambda e: e.path.name):
        for field, raw in e.bad:
            template = _EMPTY_VALUE if raw == "" else _BAD_VALUE
            out.append(contain.sentence(template, name=e.path.name, field=field, raw=raw))
        for key in e.unknown:
            known = next((f for f in _KNOWN_FIELDS if f == key.casefold()), None)
            if known is not None:
                out.append(contain.sentence(_MISCASED_KEY, name=e.path.name,
                                            key=key, known=known))
            else:
                out.append(contain.sentence(_UNKNOWN_KEY, name=e.path.name, key=key))
    leads: dict[str, list[Entry]] = {}
    for e in entries:
        if e.lead:
            leads.setdefault(e.version, []).append(e)
    for version, claimants in sorted(leads.items()):
        if len(claimants) > 1:
            out.append(contain.sentence(_TWO_LEADS, version=version, count=len(claimants),
                                        names=sorted(c.path.name for c in claimants)))
    return out


#: A file in the news directory that :func:`_read` declined, and the four ways that
#: happens today — plus the one for a way it does not. Each names the edit, because "not an
#: entry" alone sends a release engineer to read a file and guess.
_UNREADABLE_FILE = ("{name}: charter could not read this file ({error}), so it is a file "
                    "in the news directory and an entry in no release.")
_NO_FRONTMATTER = ("{name}: no `key: value` frontmatter, so charter reads it as no entry "
                   "at all. Every file in the news directory is an entry — give it "
                   "`version:` and `headline:`, or move it out of the directory.")
_MISCASED_VERSION = ("{name}: `{key}:` differs from `version:` only in case, and charter "
                     "matches a frontmatter key exactly — so this file names no version "
                     "and is in no release, in the offline view or the published notes. "
                     "Spell the key `version:`.")
_NO_VERSION = ("{name}: no `version:` charter could read in its frontmatter, so this file "
               "is in no release, in the offline view or the published notes. A staged "
               "entry writes `version: " + UNRELEASED + "` until `charter news stamp` "
               "moves it.")

#: For a decline this version cannot explain. It says so rather than guessing at one of the
#: four above, because a file reported with a reason that is not the reason is worse than a
#: file reported with none: the reader makes the edit it names, nothing changes, and the
#: sentence has spent its credibility.
_NOT_AN_ENTRY = ("{name}: charter reads this file as no entry at all, for a reason this "
                 "version has no sentence for. It is in the news directory and in no "
                 "release.")


def _not_an_entry(p: Path) -> str:
    """Why *p* produced no :class:`Entry`, as a sentence — best effort, always something.

    The SET this explains comes from :func:`_read` returning ``None``; only the wording is
    here. That split is deliberate: a fifth way for `_read` to decline would otherwise have
    to be remembered in two places, and the one that gets forgotten is this one — where
    being forgotten means a file reported with a reason that is not the reason. So the
    fourth answer is asked as a question (*is there a readable version?*) rather than
    assumed from having reached the end, and a decline this version cannot explain gets
    :data:`_NOT_AN_ENTRY`: still a sentence, still naming the file, claiming nothing.
    """
    try:
        text = p.read_text()
    except (OSError, UnicodeDecodeError) as exc:
        return contain.sentence(_UNREADABLE_FILE, name=p.name,
                                error=f"{type(exc).__name__}: {exc}")
    meta, _body = persona.parse(text)
    if not meta:
        return contain.sentence(_NO_FRONTMATTER, name=p.name)
    miscased = next((k for k in meta if k != "version" and k.casefold() == "version"), None)
    if miscased is not None:
        return contain.sentence(_MISCASED_VERSION, name=p.name, key=miscased)
    if not (meta.get("version") or "").strip():
        return contain.sentence(_NO_VERSION, name=p.name)
    return contain.sentence(_NOT_AN_ENTRY, name=p.name)


def unreadable() -> list[str]:
    """Every file in the news directory that is not an entry, as sentences.

    The sibling of :func:`entry_errors`, and it exists because the loudest version of
    #503's defect is the one that never reaches that function. ``Security: true`` sinks an
    entry; ``Version: 0.60.0`` **deletes** it — `_read` finds no ``version`` and returns
    ``None``, so the file is dropped before any of `all`'s consumers see it, and there is
    no `Entry` for `entry_errors` to have an opinion about. The release guard does not
    catch it either: `stamped()` answers from FILENAMES, so a file named
    ``0.60.0-fix.md`` satisfies "every published version ships an entry" while rendering
    into neither the Release body nor `charter news`.

    So the two questions are asked separately and answered the same way — reported, not
    swallowed — and `cmd_news` puts both behind the same gate. Asked over the whole
    directory rather than per version on purpose: a file with no readable version has no
    version to be filtered by, and the release being cut is exactly when somebody wants to
    know that the entry they wrote for it is not in it.
    """
    d = _dir()
    if d is None:
        return []
    return [_not_an_entry(p) for p in sorted(d.glob("*.md")) if _read(p) is None]


def all() -> list[Entry]:
    """Every entry that parses, oldest version first; staged entries last, and within one
    version: the entry that declared `lead:`, then security fixes, then the rest.

    **This sort is the whole of #486.** Both public views of an entry come through here —
    `render_body` (the GitHub Release body, via :func:`for_version`) and `charter news`
    (via :func:`released`/:func:`between`) — so an ordering honoured by one is honoured by
    the other by construction, rather than by two call sites agreeing to sort the same
    way. The release engineer's charter says the shipped entry is the single source for
    both and that hand-editing a Release forks them; a fix that taught only the Release
    body to lead with a security note would be that same fork wearing a commit.

    Within a rank the order is still the filename's, and `sorted` is stable, so an entry
    that declares nothing lands exactly where it landed before this existed.
    """
    d = _dir()
    if d is None:
        return []
    found = [e for e in (_read(p) for p in sorted(d.glob("*.md"))) if e is not None]
    return sorted(found,
                  key=lambda e: (e.version == UNRELEASED, update._parse(e.version),
                                 rank(e)))


def released() -> list[Entry]:
    return [e for e in all() if e.version != UNRELEASED]


def between(lo: str, hi: str) -> list[Entry]:
    """Entries newer than *lo*, up to and including *hi*.

    Exclusive at the bottom because *lo* is where you already were: you have seen it.
    """
    try:
        low, high = update._parse(lo), update._parse(hi)
    except Exception:
        return []
    return [e for e in released() if low < update._parse(e.version) <= high]


def for_version(version: str) -> list[Entry]:
    return [e for e in all() if e.version == version]


#: The most characters GitHub's create-release API accepts in a body. **Its number, not
#: charter's**: ``POST /repos/{owner}/{repo}/releases`` refuses a longer one with ``body is
#: too long (maximum is 125000 characters)``, and `gh release create` forwards that refusal
#: rather than trimming to fit — so a version whose notes render past it does not publish
#: shorter notes, it publishes **none**.
#:
#: Characters, not bytes, and the distinction is load-bearing in both directions. The API
#: counts code points and charter's entries are not ASCII — they carry ``—``, ``✗``, ``⬢``
#: — so ``len(body.encode())`` reports a bigger number than the one GitHub applies and
#: would refuse a release GitHub would have accepted. The 69 notes staged for 0.54.0 differ
#: by 3,023 between the two measures.
RELEASE_BODY_MAX = 125_000

#: How much of :data:`RELEASE_BODY_MAX` :func:`render_body` will actually spend. **Charter's
#: number, and deliberately well below GitHub's**, for three reasons that are not one
#: reason said three ways:
#:
#: *The string charter measures is not quite the string GitHub counts.* `charter news --for`
#: ``print``s this body, so the file `announce` redirects into is one character longer than
#: what was measured here, and nothing downstream re-measures. One character is enough at a
#: bound set exactly at the ceiling, and that is the smallest of the three.
#:
#: *A ceiling reached is a ceiling with nothing left for the next change.* A preamble, a
#: footer, a wrapper someone adds to the announce step — any of them lands on a body already
#: at the limit, and lands there **after** the PyPI upload, which is the failure this whole
#: mechanism exists to move to the cheap end.
#:
#: *And "just under" is a state nobody notices.* 0.52.0 published at 111,723 characters —
#: 3,277 short of the refusal — and nothing in the repository remarked on it, because
#: nothing was looking. Entries accumulate one pull request at a time and each author sees
#: only their own.
#:
#: 100,000 rather than a fraction of the limit, because a fraction invites re-deriving it:
#: this is a round number a human holds, it leaves a fifth of the API's allowance unspent,
#: and it is above every release charter has ever cut but two — so the bound below does not
#: start eliding until a version is genuinely larger than any that has shipped.
_BODY_BUDGET = 100_000


def _part(e: Entry) -> str:
    """One entry rendered whole: its headline, its label, and the body its author wrote."""
    return f"### {marker(e)}{e.headline}\n\n{e.body}".rstrip()


def _entry_file(e: Entry) -> str:
    """*e*'s path in the repository — the name a reader with a checkout or a wheel opens.

    The filename is the committed one (`_read` takes the slug from it), so it crosses into
    a document with structure and is contained on the way, for #502's reason one surface
    over: a name holding a newline would forge a heading in the release notes, which is the
    one document nobody re-derives. The body beside it is deliberately *not* contained — an
    entry's body IS Markdown its author wrote — but this line is charter's own sentence and
    the filename is a field in it.
    """
    return f"docs/news/{contain.one_line(e.path.name)}"


def _entry_url(e: Entry) -> str:
    """Where *e* can be read in full, on the web.

    The ref is the tag for a stamped version and the tracked branch for a staged one. A tag
    never moves, so a link in a published release body keeps pointing at the note **as that
    release shipped it** rather than at whatever main later made of it; ``unreleased`` has
    no tag to point at, and its render is a preview nobody publishes.

    The repository comes from :data:`update.DEV_REPO` — a constant, and it has to be one.
    `report.upstream_repo` is the other spelling of "charter's repo" in this codebase and it
    is overridable from the environment, which is exactly what a value interpolated into a
    *published* release body must not be.

    ``quote`` rather than containment for the href: a clipped URL is a broken link, and the
    property needed here is that no filename can close the ``](…)`` early and start writing
    its own Markdown after it. Percent-encoding gives that exactly, for every character.
    """
    ref = update.DEV_BRANCH if e.version == UNRELEASED else f"v{e.version}"
    return (f"https://github.com/{update.DEV_REPO}/blob/{ref}/docs/news/"
            f"{urllib.parse.quote(e.path.name)}")


def _brief(e: Entry) -> str:
    """One entry as its headline and a link to the note itself.

    The headline is the author's own one-line summary of the entry, rendered **whole** —
    nothing is clipped and no excerpt is invented from the body. An excerpt would be the
    shape this function exists to refuse: a paragraph that reads like the note and is not
    it, with no mark saying where it stopped. What a reader loses here is the body, and the
    line under the headline says precisely where the body is.
    """
    return f"### {marker(e)}{e.headline}\n\nFull note: [`{_entry_file(e)}`]({_entry_url(e)})"


def _elision(shown: int, total: int, whole: int) -> str:
    """The section that says, in the body itself, that the body is not all of it.

    Placed at the cut rather than at the top, and one copy: a banner above the notes would
    be a second statement of one fact, free to disagree with this one. It can be one copy
    because the elision is *also* visible at every point it applies — every elided note
    keeps its own heading, in its own place in the order, with :func:`_brief`'s link
    directly under it. A reader looking for a particular note therefore meets the elision
    where they are looking, which is more than a banner at the top would give them.

    It states the arithmetic — how many, how long, and the limit — because "some notes are
    linked" is the sentence a reader has no way to check. These numbers they can.

    The rule is written only when something is above it to be ruled off. Entry bodies open
    with prose, so it never lands on nothing — except when *no* note fitted, and a body
    that begins with ``---`` is a body some renderers read as frontmatter.
    """
    listed = total - shown
    return (
        ("---\n\n" if shown else "")
        + f"## {listed} of these {total} notes are listed by headline only\n\n"
        f"Rendered whole, {total} notes come to {whole:,} characters, and GitHub refuses a "
        f"release body over {RELEASE_BODY_MAX:,}.\n\n"
        f"**Every note this version shipped is in this list.** {shown} are above in full; "
        f"the {listed} below are a headline and a link. No note was dropped, and no note's "
        f"text was cut short — the text of each one below is in the note it links to, "
        f"which ships in the wheel and in the repository as well."
    )


def render_body(version: str) -> str:
    """One version's entries as the body of a GitHub Release.

    The shipped entry is the single source for both the offline suggestion and the public
    notes, so the two cannot drift: one is printed from the other.

    Order comes from :func:`all`, not from this function — see its docstring, and #486.
    The label comes from :func:`marker`, which the offline view calls too.

    **And the result is bounded, because the far end of it refuses a long one.** The whole
    body is returned whenever it fits :data:`_BODY_BUDGET`, which is what every release but
    two has done and what keeps this function's output byte-identical to what it has always
    been. Past that, the notes that fit are rendered whole and the rest become a headline
    and a link, with :func:`_elision` between them saying so.

    Three properties decide the shape, and each of them rules out an easier one:

    **Nothing is dropped and nothing is truncated.** Cutting the string at 125,000
    characters is the obvious fix and it is the "convincing empty" this codebase refuses
    everywhere: a release body that ends mid-sentence with a dozen notes simply absent
    reads exactly like a release that shipped a dozen fewer things. Every entry keeps its
    heading, in its own place in the order, and every entry's full text stays one click
    away — so what the reader loses is a scroll, never a fact.

    **The cut is one point, not a per-entry decision.** A greedy fill — skip the big ones,
    keep packing the small ones — would give an ordinary note its body while a security
    note above it lost one, which is #486's defect wearing a size limit. So the first *k*
    render whole and everything after them is brief.

    **And *k* is measured against the order :func:`all` already decided.** There is no
    second rule in here that promotes security entries, for the reason `render_body` does
    no sorting: the order that decides what leads is the order that decides what keeps its
    body, and a rule stated twice is a rule that can be honoured in one view and not the
    other. The consequence — a security note is never demoted while an ordinary one keeps
    its body — is asserted rather than assumed, in
    `tests/test_release_notes_fit_the_release.py`.

    Each candidate is **built and then measured**, rather than measured by adding up part
    lengths. Deriving the length is a second answer to "how long is this?", free to drift
    from the string actually returned by a separator's width; the string measured here is
    the string handed back. It costs a few joins of a document a third of a megabyte long,
    on the release path, once.

    A body that cannot be brought under the limit even with every note brief is returned
    **as it is**, not silently cut: :func:`commands.cmd_news` refuses to print it, which is
    the refusal `release.yml`'s `guard` job runs before `test`, `build` and `publish`.
    """
    entries = for_version(version)
    whole = [_part(e) for e in entries]
    body = "\n\n".join(whole)
    if len(body) <= _BODY_BUDGET:
        return body
    brief = [_brief(e) for e in entries]
    smallest = ""
    for k in range(len(entries) - 1, -1, -1):
        candidate = "\n\n".join([*whole[:k], _elision(k, len(entries), len(body)),
                                 *brief[k:]])
        if len(candidate) <= _BODY_BUDGET:
            return candidate
        smallest = candidate
    return smallest


def _parser():
    """A fresh parser. Never cached: the suite replaces command functions and every call
    site here has to see the replacement, which a parser built once at import would not."""
    from . import cli

    return cli.build_parser()


def _shell_syntax(argv: str) -> bool:
    """Would a shell read anything in *argv* as syntax?

    Its own function because two callers need the same answer and they need it for
    different purposes: :func:`_tokens` refuses on it, and :func:`_dispatch` has to know
    *which* of `_tokens`' two refusals fired to say whose defect it is (#321). A second
    ``set(argv) & _SHELLISH`` at the second call site is how the two would drift.
    """
    return bool(argv) and bool(set(argv) & _SHELLISH)


def _tokens(argv: str, parser=None) -> list[str] | None:
    """*argv* as a charter subcommand's tokens, or ``None`` if it is not one.

    Two refusals, both structural rather than advisory: anything a shell would read as
    syntax, and any first token that is not a registered subcommand. `charter` is implied
    and must not be written, so an entry cannot reach a different binary.

    They are not the same defect, and :func:`_dispatch` tells them apart before it reports
    one. Shell syntax can never run on any machine in any version; an unregistered first
    token is a command *this* charter does not have. ``None`` is the right answer to both,
    which is exactly why :func:`_dispatch` has to be told which refusal fired rather than
    deducing it from the return value.

    *parser* is optional and is threaded through rather than rebuilt because building one
    is ~6ms and this module runs on the `doctor` and SessionStart paths, once per entry.
    """
    if not argv or _shell_syntax(argv):
        return None
    tokens = argv.split()
    if not tokens:
        return None
    from . import cli

    if tokens[0] not in cli._subcommand_names(parser if parser is not None else _parser()):
        return None
    return tokens


#: Every command path a ``check:`` may name.
#:
#: An entry chooses from here rather than from the whole CLI, because what makes a probe
#: safe is a property of the command: it reads rather than acts, and any argv it hands to
#: another program is charter's rather than the entry's. Neither half can be read off the
#: parser — argparse cannot be asked whether a command writes — so this is a list a human
#: keeps, and `tests/test_news_probeable.py` pins the half the parser *can* answer: nothing
#: listed here takes a pass-through positional, and everything listed here is a command
#: that exists.
#:
#: A path is the subcommand tokens and nothing else. Flags are the entry's to choose; a
#: deeper subcommand is a different command and needs its own line, or listing ``news``
#: would silently list ``news stamp``, which renames files.
#:
#: Adding one is a deliberate act with two questions to answer, and the second is the one
#: that gets skipped: does it change this machine, and does its exit code actually mean
#: "this plane has the thing?". ``version`` fails the second — it always exits 0, so an
#: entry naming it would report adopted everywhere, forever — which is why the most
#: obviously harmless command in the CLI is not here.
_PROBEABLE = frozenset({
    ("doctor",),           # reads the plane and reports on it
    ("news",),             # reads entries; the ones that probe are #311's guard to answer
    ("persona", "lint"),   # reads the persona files — every shipped probe today
    ("frame-probe",),      # reads tmux's own version (charter.commands_frame.cmd_probe);
                            # a TOP-LEVEL command rather than `frame --probe` precisely so
                            # it carries no pass-through positional — `frame` itself always
                            # does (`rest`, its harness's verbatim argv) and so can never be
                            # listed here, `--probe` or not.
})


def _subparsers(parser) -> argparse._SubParsersAction | None:
    """*parser*'s subcommands, or ``None`` if it is a leaf.

    ``_SubParsersAction`` is nominally private and has been stable for the life of
    argparse; `cli._subcommand_names` reads it the same way and for the same reason.
    """
    for action in parser._actions:
        if isinstance(action, argparse._SubParsersAction):
            return action
    return None


def _parser_at(path: tuple[str, ...], parser=None):
    """The parser ``charter <path>`` reaches, or ``None`` when there is no such command."""
    parser = _parser() if parser is None else parser
    for name in path:
        sub = _subparsers(parser)
        if sub is None or name not in sub.choices:
            return None
        parser = sub.choices[name]
    return parser


def _pass_through(parser) -> list[str]:
    """*parser*'s positionals that swallow an open-ended list of words.

    The shape #317 was: ``secret exec``'s ``command`` is ``nargs="*"``, so everything after
    the vault name became an argv for :func:`subprocess.run`. Read off the parser rather
    than named, so it cannot come back under a different command's name.
    """
    if parser is None:
        return []
    return [a.dest for a in parser._actions
            if not a.option_strings and a.nargs in ("*", argparse.REMAINDER)]


def _all_command_paths(parser=None, path: tuple[str, ...] = ()) -> list[tuple[str, ...]]:
    """Every subcommand path the CLI accepts, depth first. For the suite, not the hot path."""
    parser = _parser() if parser is None else parser
    sub = _subparsers(parser)
    if sub is None:
        return [path] if path else []
    found = [path] if path else []
    for name, child in sub.choices.items():
        found.extend(_all_command_paths(child, path + (name,)))
    return found


def _command_path(tokens: list[str], parser=None) -> tuple[str, ...] | None:
    """The subcommand path *tokens* names, or ``None`` when it cannot be read off.

    Leading tokens are consumed while they name a subcommand at the level reached so far,
    which is where argparse would find them too. The walk then stops at the first token
    that does not — a flag, or an argument — and that is the case worth stating: argparse
    reads *past* a flag to find the subcommand behind it, so `news --pending stamp 9.9.9`
    runs `news stamp`. A walk that just stopped would score it as plain `news` and let a
    rename through a list that never named it. So a subcommand sighted anywhere further
    along means this walk cannot say what the tokens name, and ``None`` is the honest
    answer — refused, because every caller reads ``None`` as "not probeable".
    """
    parser = _parser() if parser is None else parser
    path: list[str] = []
    rest = list(tokens)
    while rest:
        sub = _subparsers(parser)
        if sub is None:
            break
        if rest[0] in sub.choices:
            parser = sub.choices[rest[0]]
            path.append(rest.pop(0))
            continue
        if any(t in sub.choices for t in rest):
            return None
        break
    return tuple(path)


def probeable(argv: str, parser=None) -> bool:
    """May a ``check:`` name ``charter <argv>``?

    Public because it is the entry author's rule, not only the dispatcher's: the suite
    holds every shipped ``check:`` to it, so an entry naming a command a probe may not run
    fails the PR that adds it rather than the machine that installs it.

    ``adopt:`` is deliberately NOT held to this. It is the line a human is told to run,
    once, on purpose — `adopt: browser install` installs a browser, which is the point —
    where ``check:`` runs unprompted on every plane that upgrades. Same grammar, opposite
    rules, and reading one as the other is how the restraint gets widened back out.
    """
    parser = _parser() if parser is None else parser
    tokens = _tokens(argv, parser)
    if tokens is None:
        return False
    path = _command_path(tokens, parser)
    return path is not None and path in _PROBEABLE


def resolves(parser, argv: str) -> bool:
    """Does ``charter <argv>`` PARSE? Never runs it.

    Used by the suite over every shipped entry, so a flag removed by some future PR fails
    that PR's tests rather than degrading a probe to permanent `unknown` in the field.

    Parsing only — whether a ``check:`` is *allowed* to name the command is
    :func:`probeable`, and the two are kept apart because this one also runs over
    ``adopt:``, where a mutating command is correct.
    """
    # The caller's parser, not another one: it was passed in so that the answer is about
    # THAT parser, and a first-token check against a freshly built one would quietly
    # disagree with it.
    tokens = _tokens(argv, parser)
    if tokens is None:
        return False
    try:
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            parser.parse_args(tokens)
    except SystemExit:
        return False
    except Exception:
        return False
    return True


def _outer_probe() -> int | None:
    """The PID of a process ABOVE this one that is running a probe, or ``None``.

    Three ways the marker means nothing, and each of them is a defect if believed. It is
    not a PID at all — debris, or somebody's guess at the name. It is **this** process's:
    :func:`_dispatch` sets the marker for its children to inherit, and a process that read
    its own marker back as an ancestor's would refuse the very probe it is running. Or it
    names a process that has exited, in which case the probe it stood for is gone too.
    """
    raw = os.environ.get(_ENV, "").partition(":")[0]
    if not raw.isdigit():
        return None
    pid = int(raw)
    if pid <= 0 or pid == os.getpid():
        return None
    if os.name != "posix":
        # `os.kill(pid, 0)` is a question on POSIX and an ANSWER on Windows, where it maps
        # to TerminateProcess and would kill whatever the marker named. So off POSIX the
        # marker is taken at its word: a stale one costs `unknown` — which is loud, and
        # says why — where getting this wrong costs somebody else's process.
        return pid
    try:
        os.kill(pid, 0)   # signal 0 asks whether it exists; it sends nothing
    except ProcessLookupError:
        return None
    except OSError:
        pass              # alive, and not ours to signal — still a probe
    return pid


def probing() -> bool:
    """Is an entry's ``check:`` running right now — here, or in a process above this one?

    Public, because the answer is not only this module's business. A probe asks whether
    this plane has something; anything that would answer by CHANGING the machine has to
    decline, and say so through :func:`refuse_mutation` so the entry comes back unchecked
    rather than pending. `commands_update.cmd_update` is the one that does today: its
    installer is a real `uv tool install`, and it runs in the process that IS the probe —
    at the depth the counter permits, where no marker in a child's environment reaches.
    """
    return _depth > 0 or _outer_probe() is not None


def _mark_refusal() -> None:
    """Leave word, for the process whose probe this is, that a descendant was refused.

    The only channel there is. Never raises: a temporary directory that cannot be written
    costs the outer probe its honesty — it falls back to reading an exit code — and must
    not cost the caller anything at all, which is the rule `news` is held to on the
    `doctor` and SessionStart paths.
    """
    if _outer_probe() is None:
        # This process's own probe. It already knows — the flag it reads is in memory, and
        # writing a file to tell ourselves would put the `doctor` path through the disk to
        # learn what it just decided.
        return
    _, _, mark = os.environ.get(_ENV, "").partition(":")
    if not mark:
        return
    try:
        Path(mark).touch()
    except OSError:
        pass


def refuse_mutation() -> None:
    """Record that a command declined to run because a probe is in flight.

    Stopping the mutation is the easy half. The command still has to return SOMETHING, and
    whatever it returns is not an answer to "has this plane adopted this entry?" — so the
    exit code is withheld here exactly as a re-entered one is, and the entry reports
    `unknown`. Reporting `pending` instead would invent a chore on a plane that may well
    have adopted the entry already.
    """
    global _refused
    _refused = _MUTATES


def _dispatch(argv: str) -> int | None:
    """Run ``charter <argv>`` in this process. Exit code, or ``None`` if there is no
    usable one.

    ``None`` is the whole reason this returns an Optional rather than an int: a probe that
    did not run — or that ran and answered a different question — must not be reported as
    an answer.

    **The guard is two halves, and only one of them is about the loop.** Refusing the
    nested call bounds the recursion; clearing the outer call's exit code is what keeps the
    result honest. A `doctor` inside a `doctor` exits 0 whenever nothing is broken, so an
    outer probe that read that code would report the entry ADOPTED and never offer it
    again — the entry would be hidden by the very bug it triggers. Bounded is not the same
    as correct (ADR 0013).

    Re-entry is refused by :func:`probing`, not by the counter alone, so it is refused the
    same way whether it came back up this stack or through a process charter spawned on the
    way (#314).

    Three refusals now, and they are not the same question. Is this a charter subcommand at
    all (:func:`_tokens`); is a probe already running (:func:`probing`); and is this a
    command a ``check:`` may name at all (:func:`probeable`, #317). Each returns ``None``,
    and each records its own reason, because the entry a reader has to go and fix is a
    different entry in each case.

    The first of those is really two, and reading its bare ``None`` as one was #321.
    Shell syntax is the entry's defect and reports as such; an unregistered first token is
    this machine's news about itself and keeps ``_NOT_RUN``. So the reason is taken from
    :func:`_shell_syntax` rather than from the ``None``, which cannot carry it.
    """
    global _depth, _refused
    if not _depth:
        # Cleared on the way IN, not on the way out, and before the early returns below:
        # every top-level dispatch has to start clean, or a refusal recorded while probing
        # the previous entry is read as this entry's answer.
        _refused = None
    # One parser, threaded through every use below. Building it is ~6ms and this runs once
    # per entry on the `doctor` and SessionStart paths.
    parser = _parser()
    tokens = _tokens(argv, parser)
    if tokens is None:
        if _shell_syntax(argv):
            # `_tokens` refuses two things and this is the one that is the ENTRY's defect:
            # a `check:` carrying shell syntax can never run — not here, not on any machine,
            # not in any version — so "did not run here" sends its author to look at a
            # laptop that has nothing wrong with it (#321). The other refusal, a first
            # token this CLI does not register, really is a fact about here: an entry
            # written against a charter this one is not. It keeps `_NOT_RUN`.
            #
            # Same reason as an unlisted command rather than a sixth string, because it is
            # the same finding — the entry names something a probe cannot run — and the fix
            # is the same one: pick from the short list.
            _refused = _UNLISTED
        return None
    if probing():
        _refused = _PROBES
        _mark_refusal()
        return None
    if not probeable(argv, parser):
        # After the re-entrancy guard rather than before it, so #318's marking is reached
        # on exactly the paths it was reached on before. Which of the two speaks first only
        # decides which reason is recorded, and inside a nested sweep `probe` prefers
        # `_IN_FLIGHT` over either.
        _refused = _UNLISTED
        return None

    _depth += 1
    outer = os.environ.get(_ENV)
    mark = Path(tempfile.gettempdir()) / f"charter-probe-{os.getpid()}"
    mark.unlink(missing_ok=True)   # a PID gets reused; a mark from its last owner is not
                                   # this probe's answer
    os.environ[_ENV] = f"{os.getpid()}:{mark}"
    try:
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            args = parser.parse_args(tokens)
            func = getattr(args, "func", None)
            if func is None:
                return None
            code = int(func(args) or 0)
    except SystemExit:
        return None
    except Exception:
        return None
    finally:
        # In a `finally` because this function swallows everything above: released only on
        # the happy path, the guard would stay armed for the life of the process the first
        # time a `check:` raised — and argparse raises `SystemExit` for every malformed
        # one — turning every later probe into `unknown` with no way to tell why. The
        # marker is put back rather than deleted, for the same reason: charter is not the
        # only thing that may have set a variable in its own environment.
        _depth -= 1
        if mark.exists():
            # Some charter below this one declined to probe, so the exit code about to be
            # returned is not this entry's answer — it is whatever the command did while a
            # descendant of it was refused. Same withholding as an in-process re-entry, and
            # for the same reason: bounded is not correct (ADR 0013).
            _refused = _PROBES
            mark.unlink(missing_ok=True)
        if outer is None:
            os.environ.pop(_ENV, None)
        else:
            os.environ[_ENV] = outer
    return None if _refused else code


#: Five ways to have no answer, said five ways. "Did not run here" points the reader at
#: their own machine, which is right for a check this CLI could not resolve and wrong for
#: an entry whose `check:` can never run anywhere — folding those together hides the second
#: behind the first, and the second is a defect in the entry that somebody has to fix.
_NOT_RUN = ("`charter {check}` did not run here, so this entry is unchecked — neither "
            "adopted nor pending")
_PROBES = ("`charter {check}` probes news itself, so its exit code answers a different "
           "question than this entry's — unchecked, neither adopted nor pending. A "
           "`check:` has to name a command that does not probe")
_IN_FLIGHT = ("`charter {check}` was not run: a probe is already in flight, and a probe "
              "never runs from inside a probe — unchecked here")
_MUTATES = ("`charter {check}` changes this machine rather than reading it, so it was not "
            "run — a `check:` asks whether this plane already has something and cannot be "
            "the thing that goes and gets it. Unchecked, neither adopted nor pending")
_UNLISTED = ("`charter {check}` is not a command a `check:` may name. A probe reads, and "
             "the argv it hands anything else is charter's rather than this entry's, so an "
             "entry picks from a short list of read-only commands instead of from the whole "
             "CLI. Unchecked, neither adopted nor pending")


def probe(entry: Entry) -> tuple[str, str]:
    """Has this plane adopted *entry*? ``(status, why)``."""
    if not entry.check:
        return INFORMATIONAL, ""
    # Read before dispatching: inside another probe — this process's, or one it was
    # spawned by — this one is refused before it runs, and blaming THIS entry's `check:`
    # for probing would be a guess. The command already in flight is the one that probes,
    # and it may not be this one; across a process boundary it is not even in this list.
    in_flight = probing()
    code = _dispatch(entry.check)
    if code is None:
        why = _IN_FLIGHT if in_flight else (_refused or _NOT_RUN)
        # Through `contain.sentence` rather than `.format` directly: `check:` is
        # frontmatter, this sentence is printed as a line of `charter news --pending`'s
        # own report, and the five templates above are the third untrusted span #502
        # predicted would turn up in one of these messages. `_tokens` refuses `\n` as
        # shell syntax and never sees U+2028 or an ANSI escape, and in any case a guard
        # that decides whether a probe RUNS is not a guard on what the report PRINTS.
        return UNKNOWN, contain.sentence(why, check=entry.check)
    return (ADOPTED if code == 0 else PENDING), ""


def pending() -> list[Entry]:
    """Every entry, any version, whose probe says this plane has not adopted it."""
    return [e for e in released() if probe(e)[0] == PENDING]


# --------------------------------------------------------------------------- #
# stamping — the bump PR's one mechanical step                                #
# --------------------------------------------------------------------------- #
def _is_version(v: str) -> bool:
    """Is *v* a version number, rather than the tag that carries it?

    ``v0.45.0`` is the tag; ``0.45.0`` is what frontmatter holds. Stamping the tag name
    would produce an entry whose ``version:`` can never equal ``__version__``, so both
    release catches pass and `charter news` still shows the user nothing — the exact
    class of silent wrongness staging exists to remove. Refused rather than tidied up,
    because guessing which of the two the caller meant is how the guess gets shipped.
    """
    parts = v.strip().split(".")
    if len(parts) < 2:
        return False
    # A loop, not `all(...)`: this module's own `all()` shadows the builtin.
    for part in parts:
        if not part.isdigit():
            return False
    return True


def unstamped() -> list[Path]:
    """Entry files in the repo still waiting for a version."""
    d = checkout_dir()
    return sorted(d.glob(f"{UNRELEASED}-*.md")) if d else []


def stamped(version: str) -> list[Path]:
    """Entry files in the repo naming *version*.

    The read-back for :func:`stamp`, and it reads the directory that was *written*:
    :func:`all` prefers the packaged copy, so on a tree that has been built in place it
    would answer the same question from a different, staler file.
    """
    d = checkout_dir()
    return sorted(d.glob(f"{version}-*.md")) if d else []


def _restamp(text: str, version: str) -> str | None:
    """*text* with its frontmatter ``version:`` rewritten, byte-for-byte otherwise.
    ``None`` when there is no frontmatter version line to rewrite.

    Not `persona.parse` followed by re-serialisation: that parser is lossy by design — it
    flattens frontmatter into a dict and strips the body — so a round trip would rewrite
    the author's file, reordering keys and dropping whatever a flat parser does not keep.
    A rename must not become an edit.
    """
    if not text.startswith("---"):
        return None
    lines = text.splitlines(keepends=True)
    out, found = list(lines), False
    for i in range(1, len(lines)):
        if lines[i].strip() == "---":
            break
        key, sep, _ = lines[i].partition(":")
        if sep and key.strip() == "version":
            out[i] = f"version: {version}\n"
            found = True
    else:
        return None                      # unterminated frontmatter: not an entry
    return "".join(out) if found else None


#: Why a stamp was refused, one sentence each. Through `contain.sentence` for the reason
#: `entry_errors`' sentences are: these name FILES, a filename is chosen by whoever wrote
#: the commit, and this text is read out of a release engineer's terminal at the one moment
#: nobody re-derives it (#502). `{version}` is argv rather than a committed file and is
#: bounded anyway — a value charter refused is by definition one nothing has vouched for.
_NOT_A_VERSION = ("'{version}' is not a version — pass the number alone, as in 0.45.0, "
                  "not the tag name")
_NAME_TAKEN = ("{src} → {dst}: that name is already taken, and charter will not overwrite "
               "an entry that already shipped")
_NOTHING_TO_STAMP = "{src}: no `version:` line in its frontmatter to stamp"


def stamp(version: str) -> tuple[list[tuple[Path, Path]], list[str]]:
    """Move every staged entry onto *version*: rename the file, rewrite the field.

    Returns ``(renamed, blocked)``. ``renamed`` is ``(from, to)`` pairs; ``blocked`` is
    reasons, each already a sentence a release engineer can act on.

    **All or nothing.** A partially stamped release is the one outcome worse than a
    failed stamp: the guard asks only whether *some* entry names the version, so a run
    that stamped two of three entries publishes with the third missing from both the
    release body and `charter news`, and nothing anywhere reports it. A blocked run
    leaves every entry staged, which fails loudly at the next catch.
    """
    if not _is_version(version):
        return [], [contain.sentence(_NOT_A_VERSION, version=version)]
    d = checkout_dir()
    if d is None:
        return [], ["news entries are stamped in the repo, and this is not a charter "
                    "checkout — run it from a clone (`python3 -m charter news stamp …`)"]

    plan: list[tuple[Path, Path, str]] = []
    blocked: list[str] = []
    for src in unstamped():
        slug = src.stem.split("-", 1)[1]
        dst = d / f"{version}-{slug}.md"
        if dst.exists():
            blocked.append(contain.sentence(_NAME_TAKEN, src=src.name, dst=dst.name))
            continue
        text = _restamp(src.read_text(), version)
        if text is None:
            blocked.append(contain.sentence(_NOTHING_TO_STAMP, src=src.name))
            continue
        plan.append((src, dst, text))
    if blocked:
        return [], blocked

    renamed: list[tuple[Path, Path]] = []
    for src, dst, text in plan:
        # Write the new file BEFORE dropping the old one. The reverse order can leave a
        # file renamed but not rewritten — a filename saying 0.45.0 over frontmatter
        # still saying `unreleased`, which is invisible to every view. A leftover staged
        # copy, the failure mode of this order, blocks the next stamp and says so.
        dst.write_text(text)
        src.unlink()
        renamed.append((src, dst))
    return renamed, []
