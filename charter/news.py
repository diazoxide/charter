"""What a version brought, and whether THIS plane has taken it up.

A *news entry* is a shipped, per-item note that a version introduced something, carrying
an optional probe for whether this plane has adopted it. Not a changelog: an entry exists
to be **acted on**, and one with nothing to adopt is one line.

Five properties are load-bearing.

**One entry, two consumers, one answer.** The GitHub Release body and the offline `charter
news` suggestion are the same entries rendered twice — `release.yml`'s announce job pipes
`charter news --for <version>` straight into `gh release create --notes-file`. So anything
that decides how an entry is presented has to be decided once, where both reach it, and
never at a call site. It was not, and #486 is what that cost: ORDER was left to
`sorted(glob("*.md"))`, which for a stamped release is alphabetical by slug, so 0.52.0's
vault-spending fix rendered eighth, under a docs correction. :func:`all` now applies the
declared order and :func:`marker` the label, and both views come through them.

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
#: contradiction :func:`ordering_errors` reports rather than resolving.
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
    #: to help them. :func:`ordering_errors` turns these into sentences, and
    #: `charter news --for` — which is the release workflow's own gate — refuses on them.
    bad: tuple[tuple[str, str], ...] = ()


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
    return Entry(version=version, slug=slug,
                 headline=(meta.get("headline") or "").strip(),
                 check=(meta.get("check") or "").strip(),
                 adopt=(meta.get("adopt") or "").strip(),
                 body=body, path=p,
                 lead=flags[LEAD], security=flags[SECURITY], bad=tuple(bad))


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
    reported by :func:`ordering_errors` naming the value it could not read. An author who
    writes something outside the pair is told so at the release gate rather than being
    quietly overruled by it.

    **The next spelling** is not in this function: it is the KEY. `persona.parse` matches
    exactly and case-sensitively, so ``Security: true`` never arrives as ``security`` and
    is genuinely absent here (#503); and it keeps the LAST of two lines with the same key,
    so an entry declaring ``security:`` twice has one of them dropped without a word
    (#509). Both are reached through the dict this function is handed, not through the
    value it reads, and both change how every caller of `persona.parse` reads its result.
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


def ordering_errors(entries: list[Entry]) -> list[str]:
    """Why the declared ordering of *entries* cannot be honoured, as sentences.

    Empty is the ordinary answer. Two failures are reported, and neither is resolved
    quietly, because a quiet resolution is what #486 was about:

    * an ordering field whose value was not understood (see :func:`_flag`), named with the
      value, so an author who wrote ``security: yes`` learns which word charter reads —
      and, when the field was declared with nothing after the colon, naming the shape
      instead of the empty value, because the author who wrote it almost certainly put
      the value on the next line and needs to be told that line is not read;
    * two entries in one version both declaring ``lead: true``. One of them is not going
      to be first, and picking silently would hand back the accident #486 already
      diagnosed — a position decided by something other than a person deciding it.

    Callers rather than this function decide the consequence: `charter news --for`, which
    IS the release workflow's publish gate, refuses; the range view warns and prints on.
    """
    out: list[str] = []
    for e in sorted(entries, key=lambda e: e.path.name):
        for field, raw in e.bad:
            if raw == "":
                # Distinct from the sentence below, because the fix is different: there
                # is no value to correct, and quoting one back ("`security: `") would
                # read like a rendering bug. Name the shape that produces it instead —
                # the value on the continuation line is the way this gets typed.
                out.append(
                    f"{e.path.name}: `{field}:` is declared with no value on that line "
                    f"— a news ordering field is `true` or `false`, written after the "
                    f"colon. charter's frontmatter is flat `key: value` and drops a line "
                    f"without a colon, so a value indented onto the NEXT line never "
                    f"reaches charter. Left unread, this entry sorts as though it never "
                    f"declared anything.")
                continue
            out.append(
                f"{e.path.name}: `{field}: {contain.one_line(raw)}` is not a value "
                f"charter reads — a news ordering field is `true` or `false`. Left "
                f"unread, this entry sorts as though it never declared anything.")
    leads: dict[str, list[Entry]] = {}
    for e in entries:
        if e.lead:
            leads.setdefault(e.version, []).append(e)
    for version, claimants in sorted(leads.items()):
        if len(claimants) > 1:
            names = ", ".join(sorted(c.path.name for c in claimants))
            out.append(
                f"{version}: {len(claimants)} entries declare `lead: true` ({names}) — "
                f"only one entry can be the one a reader sees first. Leave `lead:` off "
                f"all but one; a security fix that need not be first can say "
                f"`security: true` instead, which any number of entries may.")
    return out


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


def render_body(version: str) -> str:
    """One version's entries as the body of a GitHub Release.

    The shipped entry is the single source for both the offline suggestion and the public
    notes, so the two cannot drift: one is printed from the other.

    Order comes from :func:`all`, not from this function — see its docstring, and #486.
    The label comes from :func:`marker`, which the offline view calls too.
    """
    parts = []
    for e in for_version(version):
        parts.append(f"### {marker(e)}{e.headline}\n\n{e.body}".rstrip())
    return "\n\n".join(parts)


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
        return UNKNOWN, why.format(check=entry.check)
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
        return [], [f"{version!r} is not a version — pass the number alone, as in 0.45.0, "
                    f"not the tag name"]
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
            blocked.append(f"{src.name} → {dst.name}: that name is already taken, and "
                           f"charter will not overwrite an entry that already shipped")
            continue
        text = _restamp(src.read_text(), version)
        if text is None:
            blocked.append(f"{src.name}: no `version:` line in its frontmatter to stamp")
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
