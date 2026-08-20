"""What a version brought, and whether THIS plane has taken it up.

A *news entry* is a shipped, per-item note that a version introduced something, carrying
an optional probe for whether this plane has adopted it. Not a changelog: an entry exists
to be **acted on**, and one with nothing to adopt is one line.

Four properties are load-bearing.

**It ships in the wheel.** Entries travel with the code that implements them, resolved the
way :mod:`charter.docsrc` resolves documentation — packaged copy first, the repo's
``docs/news/`` as the checkout fallback. A control plane has no reason to vendor a copy and
every reason not to: a copy drifts from the binary, invisibly and in both directions.

**A probe is checked, never assumed.** ``check:`` answers "does this plane already have
it?" with an exit code. A probe that *cannot run* is :data:`UNKNOWN` — not "adopted" and
not "pending". Reporting it as pending invents work; reporting it as adopted hides the
entry forever. This is `doctor`'s ``_NOT_CHECKED_HINT`` in another costume: the absence of
information is not evidence of health (ADR 0013).

**Actions are charter subcommands, dispatched in-process.** No shell is ever involved and
no argv is ever handed to one, so an entry cannot become an arbitrary-command primitive.
`docsrc._TOPIC` keeps the same restraint for `docs show` — "must not be a file-read
primitive wearing a documentation command" — and the stakes are higher here, because this
one runs rather than prints. Being in-process is also what makes a dozen probes cheap
enough for `doctor` to run on demand.

**A probe never runs from inside a probe.** Some charter commands probe every entry
themselves — `doctor` does, and so does `charter news --pending` — so an entry naming one
puts the dispatcher inside itself, a full sweep at every level, forever (#311). The guard
in :func:`_dispatch` refuses the nested call AND withholds the outer command's exit code,
because the two halves fix different things: refusing bounds it, withholding is what keeps
the answer honest.
"""

from __future__ import annotations

import io
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from typing import NamedTuple

from . import persona, update

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

#: How many :func:`_dispatch` calls are in flight, and whether one of them was refused for
#: re-entry. Plain module state rather than a :class:`~contextvars.ContextVar`: a ContextVar
#: reads its default in every new thread, so a probing command that fanned its own work out
#: to a pool would walk straight past the guard — which is the one failure this exists to
#: make impossible. The global's failure mode is the opposite and far cheaper: two probes
#: racing in different threads would make each other `unknown`, which is wrong but bounded,
#: honest, and not reachable today — `doctor` and `charter news` each walk their entries in
#: one thread.
_depth = 0
_reentered = False


class Entry(NamedTuple):
    version: str
    slug: str
    headline: str
    check: str
    adopt: str
    body: str
    path: Path


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
    return Entry(version=version, slug=slug,
                 headline=(meta.get("headline") or "").strip(),
                 check=(meta.get("check") or "").strip(),
                 adopt=(meta.get("adopt") or "").strip(),
                 body=body, path=p)


def all() -> list[Entry]:
    """Every entry that parses, oldest version first; staged entries last."""
    d = _dir()
    if d is None:
        return []
    found = [e for e in (_read(p) for p in sorted(d.glob("*.md"))) if e is not None]
    return sorted(found, key=lambda e: (e.version == UNRELEASED, update._parse(e.version)))


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
    """
    parts = []
    for e in for_version(version):
        parts.append(f"### {e.headline}\n\n{e.body}".rstrip())
    return "\n\n".join(parts)


def _tokens(argv: str) -> list[str] | None:
    """*argv* as a charter subcommand's tokens, or ``None`` if it is not one.

    Two refusals, both structural rather than advisory: anything a shell would read as
    syntax, and any first token that is not a registered subcommand. `charter` is implied
    and must not be written, so an entry cannot reach a different binary.
    """
    if not argv or set(argv) & _SHELLISH:
        return None
    tokens = argv.split()
    if not tokens:
        return None
    from . import cli

    if tokens[0] not in cli._subcommand_names(cli.build_parser()):
        return None
    return tokens


def resolves(parser, argv: str) -> bool:
    """Does ``charter <argv>`` PARSE? Never runs it.

    Used by the suite over every shipped entry, so a flag removed by some future PR fails
    that PR's tests rather than degrading a probe to permanent `unknown` in the field.
    """
    tokens = _tokens(argv)
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
    """
    global _depth, _reentered
    if not _depth:
        # Cleared on the way IN, not on the way out, and before the early returns below:
        # every top-level dispatch has to start clean, or a refusal recorded while probing
        # the previous entry is read as this entry's answer.
        _reentered = False
    tokens = _tokens(argv)
    if tokens is None:
        return None
    if _depth:
        _reentered = True
        return None
    from . import cli

    _depth += 1
    try:
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            args = cli.build_parser().parse_args(tokens)
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
        # one — turning every later probe into `unknown` with no way to tell why.
        _depth -= 1
    return None if _reentered else code


#: Three ways to have no answer, said three ways. "Did not run here" points the reader at
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


def probe(entry: Entry) -> tuple[str, str]:
    """Has this plane adopted *entry*? ``(status, why)``."""
    if not entry.check:
        return INFORMATIONAL, ""
    # Read before dispatching: inside another probe, this one is refused before it runs,
    # and blaming THIS entry's `check:` for probing would be a guess — the command already
    # in flight is the one that probes, and it may not be this one.
    in_flight = _depth > 0
    code = _dispatch(entry.check)
    if code is None:
        why = _IN_FLIGHT if in_flight else (_PROBES if _reentered else _NOT_RUN)
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
