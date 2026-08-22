"""One name, one directory: the containment rule for names charter reads out of files.

Charter validated a name when a human typed it and never when it read one from a
committed file (#328). `valid_name` lives in :mod:`charter.persona` and
:mod:`charter.workspace` and is called from six places, all of them commands. The reading
side — `extends:`, `uses:`, `[persona] default`, a `workspace.json` repo name, an
`inventory/repos.json` name — joined the value onto a path with nothing in between, so a
committed file could name a target outside the directory charter meant to look in.

:mod:`charter.docsrc` already had the answer and it was never reused: a topic is matched
against a shape before it becomes a page, because ``charter docs show ../../etc/passwd``
"must not be a file-read primitive wearing a documentation command". This module is that
idea, extracted so the five reading sites share one implementation instead of four
near-misses and a fifth that forgets.

**Two questions, kept apart on purpose.**

*Shape* — "could this string name one entry in a directory?" — is :func:`segment_ok`.
*Containment* — "does joining it stay inside the base?" — is :func:`child`. Callers use
both, and the pair is deliberate rather than redundant: the shape check belongs where an
identity is decided (`inventory.merge`, beside the bare-name collision logic that already
treats the name as load-bearing), and the containment assertion belongs at every join,
because a hand-edited or PR-modified tracked file never passes through the code that
decided the identity. An identity-layer check alone is a guard the attacker walks around.

**Who gets which shape rule.** Charter mints persona and workspace names — `persona
create` and `workspace create` enforce their own `valid_name` — so those keep answering
to `valid_name`, and lint agrees with the resolver by construction rather than by a second
check kept in step by hand. A **forge** mints repo names, and `org/.github` is a real repo
GitHub itself tells organisations to create, while `MyRepo` is merely ordinary. Imposing
charter's creation-time alphabet on someone else's forge would refuse to clone both.
:func:`segment_ok` is therefore the permissive rule: it forbids traversal and separators
and nothing else.

**Two layers, and the second one resolves.** :func:`segment_ok` and :func:`child` stay
lexical: they answer a question about a *string*, and asking the filesystem would make a
traversal succeed exactly when the attacker's target happens to exist. That left every
read charter performs — not only the ones named by a name — open to a committed symlink,
which is #336 and is now :func:`file_refusal` / :func:`dir_refusal` below. The two are
deliberately separate functions: a name is refused before anything is opened, and a *path*
is refused before it is read, and neither can stand in for the other.

**What "inside the plane" had to mean.** The obvious rule — refuse a path whose `realpath`
leaves ROOT — admits #336's own demonstration unchanged, because ``.charter/`` sits
*under* ROOT (:func:`config._migrate_state_dir`), so ``personas/x/persona.md`` →
``../../.charter/vaults/devops.json`` never leaves the plane while doing precisely what the
`pretooluse-read` guard exists to stop. The boundary is therefore the directories a plane
keeps its **data** in — :func:`data_roots` — which excludes the secrets home and includes
the one part of it that is data (ephemeral memory, which lives inside ``.charter/``).

**Links are followed; escapes are refused.** Refusing every symlink would close the hole
and break a plane that legitimately links a persona directory. Resolving keeps that plane
working, and #342's reason for staying lexical was never that symlinks are acceptable —
only that doing half of this while claiming all of it would be worse than filing it.

**No refusal function here raises.** These checks sit under `doctor`, the status line and
SessionStart, where the rule is that a hook may cost a session its briefing and never its
turn. The single exception is :func:`writable`, which exists to raise and says so: a write
that refuses has no fallback the way a skipped read does. A refused
name is reported as data — see :data:`NOT_A_SEGMENT` and the vocabulary in
:mod:`charter.news`, which says five kinds of "no answer" five different ways for the same
reason: folding a defect in a file behind a generic message hides the defect, and somebody
has to fix it.
"""

from __future__ import annotations

import os
import stat
from pathlib import Path

#: Said once, so every site refuses in the same words. A reader who hits this has a defect
#: in a committed file, not a typo, and the sentence has to be enough to act on: "no such
#: repo" would send them looking for a repo that was never the point.
NOT_A_SEGMENT = ("'{name}' is not a name — it is a path. This is read from a committed "
                 "file and joined onto a directory, so it may name one entry there and "
                 "nothing else: no '/', no '\\', no '.' or '..', and nothing absolute")

#: The separators every platform charter runs on will honour. Backslash is included on
#: POSIX too: the file is committed and shared, so the machine that *wrote* the name is
#: not necessarily the machine that resolves it.
_SEPARATORS = ("/", "\\")


def segment_ok(name: str) -> bool:
    """True when *name* could name one entry inside some directory.

    Deliberately a question about the *string*, never about the disk. Asking the
    filesystem would make a traversal succeed exactly when the attacker's target happens
    to exist, which is the one case where the answer must not change.
    """
    if not name or not isinstance(name, str):
        return False
    if name in (".", ".."):
        return False
    if "\x00" in name:
        # A NUL terminates the string inside the C library, so the name Python checked and
        # the name the kernel opened would be two different strings.
        return False
    if any(sep in name for sep in _SEPARATORS):
        return False
    # Catches a Windows drive-qualified name ("C:x") and anything else the running
    # platform considers rooted, without charter maintaining its own list of what those
    # look like.
    if os.path.isabs(name) or os.path.splitdrive(name)[0]:
        return False
    return True


def child(base, name: str) -> Path | None:
    """``base / name`` when that is a direct child of *base*, else ``None``.

    ``None`` rather than an exception because every caller is on a path that must not
    crash, and rather than a sanitised name because silently rewriting a name invents a
    second identity for the same thing and hides the defect in the file that somebody
    still has to fix.

    The normalised comparison is belt and braces over :func:`segment_ok` — which already
    forbids every separator, so the join cannot escape today. It is kept because it is the
    half that still holds if the shape rule is ever loosened to admit some new name, which
    is exactly the drift `docsrc`'s own comment warns about.
    """
    if not segment_ok(name):
        return None
    base = Path(base)
    joined = base / name
    if Path(os.path.normpath(joined)).parent != Path(os.path.normpath(base)):
        return None
    return joined


def refusal(name: str) -> str:
    """The one sentence every site uses to say why *name* was refused."""
    return NOT_A_SEGMENT.format(name=name)


# --------------------------------------------------------------------------- #
# the resolving layer — a PATH charter is about to read (#336)                 #
# --------------------------------------------------------------------------- #

class Refused(Exception):
    """A path charter declined to write to, carrying the sentence that says why.

    Deliberately **not** a `ValueError`. The write sites sit under `except ValueError`
    handlers that already mean "the text you passed was empty", and three more swallow it
    bare — inheriting from it would let a containment refusal be caught by a clause
    written about something else and disappear, which is the one outcome worse than the
    corruption this exists to stop.

    Caught once, in `cli.main`, beside `util.ProcTimeout` and for the same stated reason:
    *a child that outlived its budget is a condition, not a bug*. So is a committed file
    that redirects a write. Reaching the `except Exception` below it would file a crash
    report against charter for a defect in the plane's own data, and send the reader
    looking for the bug in the wrong repository.

    The refusal functions themselves still never raise — this is thrown by *callers* that
    have nothing useful to do with a refusal except stop.
    """


#: Said once, like :data:`NOT_A_SEGMENT`, and for the same reason: the reader has a defect
#: in a committed file. It names both ends because "refused" without the target is
#: unactionable — the whole point is that the path charter opened is not the path it read.
#:
#: ``{verb}`` is read/write rather than two near-identical constants: which operation was
#: redirected is the one word that differs, and it is the word that tells the reader
#: whether they are looking at a leak or at corruption.
NOT_PLANE_DATA = ("'{name}' resolves to '{target}', outside the directories a control "
                  "plane keeps its data in ({roots}). A committed symlink there redirects "
                  "the {verb}, so charter follows a link that lands inside them and "
                  "refuses one that leaves")

#: A path charter cannot examine at all (vanished mid-listing, a broken link, no
#: permission). Refused rather than raised — every caller here is on a path that must not
#: crash — and said in its own words so it is not read as a containment failure.
UNREADABLE = "'{name}' cannot be examined ({error})"

#: Not a file at all: a FIFO, a device, a socket, a directory. Named for what it is,
#: because "could not be read" would send the reader looking for a permissions problem
#: when what they have is a path that blocks for ever or yields for ever.
#:
#: A FIFO blocks a *writer* just as completely as a reader — ``open(fifo, "a")`` waits for
#: a reader to appear and never stops waiting — so this is one sentence for both sides,
#: and the write side has no `hooks.json` timeout above it to end the wait.
NOT_A_FILE = ("'{name}' is not a regular file (it is {kind}). Charter opens plane data at "
              "names a committed file can occupy, so an entry that blocks or never ends "
              "would take the {verb} with it")

#: The bound on one plane file charter reads whole. **1 MiB, and it is meant never to fire
#: on anything a human wrote**: the largest persona charter in charter's own plane is
#: 6.8 KB, its largest memory index 5 KB, its largest document under `docs/` 34 KB. The
#: number is not tuned to those — a cap sitting just above real content fires on the first
#: long runbook somebody curates — it is set where nothing an editor produces can reach it
#: and no single read can cost anything. `os.lstat` reports it in the syscall the
#: containment check already makes, so the bound is free.
MAX_BYTES = 1_048_576

TOO_LARGE = ("'{name}' is {size} bytes, over the {cap}-byte bound on one plane file. "
             "Nothing a memory, todo, ref or persona charter is meant to hold comes near "
             "that, so this is a defect in the file rather than a limit to raise")

#: `stat` module names for the shapes that are not files, so a refusal says which.
_KINDS = ((stat.S_ISDIR, "a directory"), (stat.S_ISFIFO, "a FIFO"),
          (stat.S_ISSOCK, "a socket"), (stat.S_ISCHR, "a character device"),
          (stat.S_ISBLK, "a block device"))


def data_roots() -> tuple[Path, ...]:
    """The directories a control plane keeps readable data in.

    Read from :mod:`charter.config` **at call time**, never captured at import: the test
    harness re-points the plane with `config.use`, and a root captured at import would
    quietly contain against the developer's real checkout.

    ``PERSONA_STATE_DIR`` is here because ephemeral memory is data charter is *supposed* to
    read, and it lives under the secrets home. That is the whole reason this is a list of
    data directories rather than "the plane, minus ``.charter/``".
    """
    from . import config
    return (config.PERSONAS_DIR, config.WORKSPACES_DIR, config.PERSONA_STATE_DIR)


def within_data(path) -> bool:
    """True when *path* **resolves** inside one of :func:`data_roots`.

    Both ends are resolved. The roots have to be too: on macOS a temp plane lives under
    ``/var/folders/…``, which is itself a link to ``/private/var/…``, so comparing a
    resolved path against an unresolved root refuses every read in the test harness and
    on any plane behind a linked mount. Resolving the roots is also what keeps a plane
    that *relocates* ``personas/`` or ``workspaces/`` behind a link working, which is the
    same legitimate case :func:`file_refusal` preserves one level down.

    **The fast path is one ``lstat``, and it is exact.** ``persona.load`` asks this of
    ``personas/<name>`` on every call, and four ``realpath``s (20µs) there doubled a
    status-line sweep. When a path's own parent *is* a data root and the path itself is
    not a link, it cannot have moved relative to that root — whatever the root resolves
    to, this resolves inside it — so the answer is already known. Anything else (a deeper
    base, a link, a lexical outsider) still pays the full resolve.
    """
    try:
        target = os.path.abspath(path)
        roots = [os.path.abspath(r) for r in data_roots()]
        if os.path.dirname(target) in roots and not stat.S_ISLNK(os.lstat(target).st_mode):
            return True
    except (OSError, ValueError):
        pass                                   # vanished or unreadable — ask the slow path
    try:
        target = os.path.realpath(path)
        for r in data_roots():
            root = os.path.realpath(r)
            # `+ os.sep` so a sibling named like a root ("personas-old") is not a child.
            if target == root or target.startswith(root + os.sep):
                return True
    except (OSError, ValueError):
        return False
    return False


def _not_plane_data(path, verb: str = "read") -> str:
    roots = ", ".join(sorted(Path(r).name for r in data_roots()))
    try:
        target = os.path.realpath(path)
    except (OSError, ValueError):
        target = path            # unresolvable — say what was asked for, never raise here
    return NOT_PLANE_DATA.format(name=path, target=target, roots=roots, verb=verb)


def dir_refusal(directory, verb: str = "read") -> str | None:
    """Why charter must not list — or write inside — *directory*, or ``None``.

    Separate from :func:`file_refusal` because a listing pays this **once** while paying
    the per-file check N times, and because it is what catches the variant the file check
    structurally cannot see: when the *directory* is the link, every file inside it is an
    ordinary regular file that no per-file check has anything to object to.

    That variant is worse on the write side, where charter creates the directory it is
    about to write into: ``mkdir(parents=True, exist_ok=True)`` accepts a symlink to an
    existing directory without complaint, so a committed link at ``memory/`` silently
    relocates every file written under it. Hence *verb* — the same question, asked before
    the ``mkdir`` rather than before the listing.
    """
    if not within_data(directory):
        return _not_plane_data(directory, verb)
    return None


def file_refusal(path) -> str | None:
    """Why charter must not read *path*, or ``None``.

    Three questions, **one syscall**, and that is why both halves of #336 close at the
    same gate. ``lstat`` says whether this is a link (containment), whether it is a
    regular file (a FIFO blocks the read for ever, a device never ends) and how big it is
    (the bound) — and it says all of it *without opening anything*, which is the property
    a deadline around the read could never have given: there is nothing to time out,
    because nothing is opened.

    Cheap on purpose. These run under the status line and SessionStart, where the read is
    already the cheapest thing in the frame and a guard costing more than it guards gets
    reverted the first time somebody profiles it. The expensive answer
    (:func:`within_data`, which resolves) is asked only when the entry **is** a link — a
    path that is not one cannot have moved relative to the directory it was listed from,
    which the caller checked once with :func:`dir_refusal`.

    Not a TOCTOU guard, and not sold as one: the attacker here holds a commit, not a
    process racing the read.
    """
    return _path_refusal(path, missing_ok=False, verb="read")


def write_refusal(path) -> str | None:
    """Why charter must not **write** to *path*, or ``None``.

    #348 gated every read of plane data and left the write side untouched, so a committed
    link at a name charter writes redirected the write instead of the read (#349). The
    same three questions apply — is this link contained, is it a file at all, is it a
    sane size — and each of them matters *more* here: an append to a credential store
    corrupts it where a read merely leaked it, ``write_text`` through a **dangling** link
    creates the target wherever it points, and ``open(fifo, "a")`` blocks for ever with a
    human sitting at the command rather than a hook timeout overhead.

    **One rule differs, and it is the whole reason this is a second function.** On a read,
    a path that is not there is a refusal. On a write it is the ordinary case — the file
    is about to be created — so ENOENT answers ``None``. Getting that backwards refuses
    every first write on a fresh plane, and getting it *too* right (treating "not there"
    as "nothing to check" before the link is resolved) misses the dangling-link case
    entirely, because ``exists()`` is false for exactly the link that is most dangerous.
    Both halves are held by tests that fail in opposite directions.

    **The parent is checked here, not by the caller.** On the read side that split is
    right — a listing pays :func:`dir_refusal` once and the per-file check N times. A
    write has no such loop, and the directory variant is the one a per-file check
    structurally cannot see: when ``memory/`` is the link, ``MEMORY.md`` inside it is an
    ordinary regular file with nothing to object to, and every ``mkdir(exist_ok=True)`` in
    charter accepts a symlink to a directory without complaint. Folding it in costs one
    resolve on a path nobody writes in a loop, and makes it impossible for the fifteenth
    caller to remember the file and forget the directory.

    What this is not: a check on whether the *value* being written belongs there. It
    answers where the bytes will land, nothing more.
    """
    return (dir_refusal(Path(path).parent, "write")
            or _path_refusal(path, missing_ok=True, verb="write"))


def writable(path) -> Path:
    """*path*, when charter may write there — else raise :class:`Refused`.

    The one place in this module that raises, and the exception is deliberate rather than
    an oversight in the "nothing here raises" promise above: that promise is about the
    *refusal* functions, which run under `doctor`, the status line and SessionStart and
    must return data. This is for callers on a command path, where a refusal has no
    sensible fallback — a read that refuses can skip the file, but a write that refuses
    and carries on leaves `charter persona remember` printing ✓ over a fact it never
    recorded, which is the same class of lie as the corruption being prevented.

    Callers that must *not* raise — the hook-driven tallies, which already promise a hook
    may never break a turn — ask :func:`write_refusal` directly and decline to write.
    """
    refusal = write_refusal(path)
    if refusal:
        raise Refused(refusal)
    return Path(path)


def _path_refusal(path, *, missing_ok: bool, verb: str) -> str | None:
    """The body :func:`file_refusal` and :func:`write_refusal` share.

    One implementation with the difference named, rather than two functions kept in step
    by hand — the divergence between two checks that read the same file and answered
    differently is what #328 and #342 were both about, and it is not a mistake worth
    making again inside the module that exists to stop it.
    """
    if not str(path):
        # ENOENT means "about to be created" only for a path that COULD be created, and
        # the empty path never can. Without this the write side answered "nothing to
        # object to" for it and handed the caller an unhandled `OSError` one line later —
        # a refusal turned back into a crash, which is the bug #348 shipped and fixed.
        return UNREADABLE.format(name=path, error="empty path")
    try:
        st = os.lstat(path)
    except FileNotFoundError:
        if missing_ok:
            return None       # about to be created — there is nothing there to object to
        return UNREADABLE.format(name=path, error="No such file or directory")
    except OSError as e:
        return UNREADABLE.format(name=path, error=e.strerror or e)
    except ValueError as e:
        # `os.lstat` raises ValueError, NOT OSError, on a path holding a NUL — the one
        # input shaped to get past a check (`segment_ok` refuses it for the same reason).
        # "Nothing here raises" is this module's promise; catching only OSError broke it.
        return UNREADABLE.format(name=path, error=e)
    if stat.S_ISLNK(st.st_mode):
        # Asked BEFORE `os.stat`, and that order is load-bearing on the write side: a
        # dangling link has no target to stat, so a containment check placed after the
        # stat would never run on the one link that can create a file out of nothing.
        if not within_data(path):
            return _not_plane_data(path, verb)
        try:
            # Follows the link. Still a `stat`, so a FIFO on the other end answers here
            # rather than blocking.
            st = os.stat(path)
        except FileNotFoundError:
            if missing_ok:
                return None   # a contained link naming a file charter is about to create
            return UNREADABLE.format(name=path, error="No such file or directory")
        except OSError as e:
            return UNREADABLE.format(name=path, error=e.strerror or e)
        except ValueError as e:
            return UNREADABLE.format(name=path, error=e)
    if not stat.S_ISREG(st.st_mode):
        kind = next((k for test, k in _KINDS if test(st.st_mode)), "not a file")
        return NOT_A_FILE.format(name=path, kind=kind, verb=verb)
    if st.st_size > MAX_BYTES:
        return TOO_LARGE.format(name=path, size=st.st_size, cap=MAX_BYTES)
    return None
