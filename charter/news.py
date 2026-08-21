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
no argv is ever handed to one, so an entry is meant not to be an arbitrary-command
primitive. `docsrc._TOPIC` keeps the same restraint for `docs show` — "must not be a
file-read primitive wearing a documentation command" — and the stakes are higher here,
because this one runs rather than prints. Being in-process is also what makes a dozen
probes cheap enough for `doctor` to run on demand. That restraint currently leaks: `secret
exec` takes a pass-through argv, so a `check:` naming it reaches any binary, with a vault's
credential in its environment (#317). Said out loud here rather than left as a claim this
module does not keep.

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
"""

from __future__ import annotations

import io
import os
import tempfile
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
    """
    global _depth, _refused
    if not _depth:
        # Cleared on the way IN, not on the way out, and before the early returns below:
        # every top-level dispatch has to start clean, or a refusal recorded while probing
        # the previous entry is read as this entry's answer.
        _refused = None
    tokens = _tokens(argv)
    if tokens is None:
        return None
    if probing():
        _refused = _PROBES
        _mark_refusal()
        return None
    from . import cli

    _depth += 1
    outer = os.environ.get(_ENV)
    mark = Path(tempfile.gettempdir()) / f"charter-probe-{os.getpid()}"
    mark.unlink(missing_ok=True)   # a PID gets reused; a mark from its last owner is not
                                   # this probe's answer
    os.environ[_ENV] = f"{os.getpid()}:{mark}"
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


#: Four ways to have no answer, said four ways. "Did not run here" points the reader at
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
