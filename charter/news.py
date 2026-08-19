"""What a version brought, and whether THIS plane has taken it up.

A *news entry* is a shipped, per-item note that a version introduced something, carrying
an optional probe for whether this plane has adopted it. Not a changelog: an entry exists
to be **acted on**, and one with nothing to adopt is one line.

Three properties are load-bearing.

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
    """Run ``charter <argv>`` in this process. Exit code, or ``None`` if it could not run.

    ``None`` is the whole reason this returns an Optional rather than an int: a probe that
    did not run must not be reported as an answer.
    """
    tokens = _tokens(argv)
    if tokens is None:
        return None
    from . import cli

    try:
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            args = cli.build_parser().parse_args(tokens)
            func = getattr(args, "func", None)
            if func is None:
                return None
            return int(func(args) or 0)
    except SystemExit:
        return None
    except Exception:
        return None


def probe(entry: Entry) -> tuple[str, str]:
    """Has this plane adopted *entry*? ``(status, why)``."""
    if not entry.check:
        return INFORMATIONAL, ""
    code = _dispatch(entry.check)
    if code is None:
        return UNKNOWN, (f"`charter {entry.check}` did not run here, so this entry is "
                         f"unchecked — neither adopted nor pending")
    return (ADOPTED if code == 0 else PENDING), ""


def pending() -> list[Entry]:
    """Every entry, any version, whose probe says this plane has not adopted it."""
    return [e for e in released() if probe(e)[0] == PENDING]
