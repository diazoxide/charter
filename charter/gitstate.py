"""What git says about a working tree, **and the third answer** — the one charter used to
print as the first.

A `git status` that failed writes nothing to stdout, so ``.stdout.strip()`` is empty. A
`git add` that failed stages nothing, so the ``git diff --cached --quiet`` after it says
there is no difference. In both spellings the value that means *charter could not tell* is
byte-for-byte the value that means *there is nothing there* — and every caller in this
package that read one of them read it as the second.

This module is where the two stop being the same value. :func:`read` answers a question
about a tree in three states rather than two, and :class:`IndexLock` carries the fact that
explains the commonest cause of the third.

**#917, on the operator's own plane.** `charter save` printed *"Nothing to save — the
control-plane working tree is clean"* while `git status --porcelain` listed thirteen
modified files. `.git/index.lock` had been sitting there for twenty-three hours — zero
bytes, no process holding it, left by a git that crashed the day before. With the file
removed, the identical command on the identical tree committed twenty files.

Measured against git 2.50.1, in a repository holding one modified tracked file and one
untracked file, with a zero-byte ``.git/index.lock`` planted in it::

    git status --porcelain     → rc 0, both files listed  (reading the tree needs no lock)
    git add -A                 → rc 128, "Unable to create '…/index.lock': File exists."
    git diff --cached --quiet  → rc 0                     (nothing staged, so no diff)

That asymmetry is the whole defect. The lock is invisible to the question `commit_push`
asks *last* and fatal to the one it asks *first*, so a failed stage arrives at the check
below it as an empty index — indistinguishable from a tree with nothing in it. **"Clean"
and "charter could not tell" were the same value**, and the one charter printed is the one
whose next act is the operator deciding to stop worrying about the work.

This is the family this repository has been finding all week, and the two before it were
fixed the same way — by giving the failure a category of its own rather than a shared
default: `tools.sweep.NoSandbox` (#905/#909) says *the machine*, not *the branch*, and
`config.PLANE_REFUSAL` (#913/#914) says *refused*, not *carry on with defaults*. This
module is that category for "charter asked git and git could not say".

**charter reports a lock. It never clears one.**

A lock held by a live git is real, and nothing inside one command can reliably tell a held
lock from an abandoned one: the holder may be a `git commit` with an editor still open, a
`git gc` a second from finishing, or another machine's view of the same network mount.
Removing it would corrupt an index that was about to be written, and a program that clears
locks teaches its operator that locks do not mean anything. So charter states the three
facts the person deciding actually needs — the path, the size and the age — and leaves the
`rm` to them. That is the division of labour the operator used to fix #917 by hand: `ps`,
then the file's mtime, then the removal.

The three facts are enough to tell a crash from contention because of how git takes the
lock: it creates ``index.lock``, writes the new index *into* it, and renames it over the
old one. A lock that never grew past zero bytes is one whose writer died before writing
anything, and one that has not grown in hours has no writer left at all.
"""

from __future__ import annotations

import time
from pathlib import Path
from typing import NamedTuple

from . import util

#: The name git gives the file. Not configurable and not worth a lookup — it is
#: ``$GIT_DIR/index.lock`` in every git there has ever been — but named once so the
#: refusal, the doctor row and the tests cannot drift into three spellings of it.
LOCK_NAME = "index.lock"

#: How long a zero-byte lock must have sat there before charter will call it a crash
#: rather than contention.
#:
#: Fifteen minutes, and the number matters less than which side of it errs. Below it
#: charter says only that a lock is present and something may be holding it, which is
#: true of a real one and harmless to say about a stale one. Above it charter says a git
#: process crashed — a stronger claim, and the one that lets an operator act — so it is
#: only made once no plausible live git is still starting up.
#:
#: The size does most of the work: `git commit` writes the new index into the lock before
#: it opens an editor, so the long-lived legitimate holder is not zero bytes. This
#: threshold is what covers the window between git creating the file and git filling it.
STALE_AFTER = 15 * 60


def age_phrase(seconds: float) -> str:
    """A coarse age — ``4s``, ``12m``, ``23h``, ``3d``.

    Coarse on purpose, the same way `pieces.since` is: the number is context for a human
    deciding whether to run `rm`, never an input to a decision charter is making. There is
    no threshold here that charter branches on except :data:`STALE_AFTER`, and that one is
    stated where it is used.
    """
    secs = max(0, int(seconds))
    if secs < 60:
        return f"{secs}s"
    if secs < 3600:
        return f"{secs // 60}m"
    if secs < 86400:
        return f"{secs // 3600}h"
    return f"{secs // 86400}d"


class IndexLock(NamedTuple):
    """A ``$GIT_DIR/index.lock`` charter found, and the three facts about it.

    A record and not a verdict: :attr:`crashed` is the only judgement in it, and every
    caller still prints the path, the size and the age beside it so the reader can
    disagree.
    """

    #: The lock file itself, absolute, so the sentence charter prints can be pasted.
    path: Path
    #: Bytes. Zero means git created the file and never wrote the index into it.
    size: int
    #: Seconds since its mtime.
    age: float

    @property
    def crashed(self) -> bool:
        """Zero bytes and older than :data:`STALE_AFTER` — a crash, not contention.

        The conjunction is the claim. A big lock is a git that got as far as writing an
        index, and a young lock is a git that may still be running; neither is something
        charter should describe as abandoned to someone who is about to delete it.
        """
        return self.size == 0 and self.age >= STALE_AFTER

    def describe(self) -> str:
        """One line naming the lock, its size and its age — the sentence #917 needed.

        The age is here rather than in a hint because it is the fact that changes what the
        operator does: a lock seconds old is somebody else's `git commit` and the answer is
        to wait, and a lock hours old is a corpse and the answer is `rm`. A message that
        said only "a lock is present" would send both of them to the same place.
        """
        what = ("a git process crashed here and left it behind"
                if self.crashed else "something may still be holding it")
        return (f"{self.path} — {self.size} byte(s), {age_phrase(self.age)} old; "
                f"{what}.")

    def remedy(self) -> list[str]:
        """What the operator does next, in the order they should do it.

        Two lines, and the `rm` is second and labelled, because the order is the safety:
        checking for a holder first is what makes removing the file safe, and a remedy that
        printed the removal on its own would teach the habit of skipping the check.
        """
        return ["who holds it:  ps -eo pid,lstart,command | grep '[g]it'",
                f"nobody does:   rm -f {self.path}   "
                f"(charter never removes a lock — a held one is real)"]


def find(git_dir: Path | str) -> IndexLock | None:
    """The index lock inside *git_dir*, or ``None`` when there is not one.

    ``None`` also when the directory cannot be stat-ed at all: this module answers "is
    there a lock", and turning an unreadable git directory into a lock-shaped answer would
    be the same conflation one layer down.

    The path is **resolved**, because the whole point of printing it is that the operator
    can paste it into an `rm`, and macOS hands out temp and home paths through symlinks
    (``/var`` → ``/private/var``) — `planegit._refuse_unreadable` passes the git dir it got
    from `rev-parse` without normalising it first.

    The age is **not** clamped here. A future mtime — a clock that went backwards, a bad
    archive — makes this negative, and `age_phrase` clamps on the way out, which is the one
    place it can be observed. A second clamp read as belt and braces and was really a line
    nothing could tell apart from its own absence; the deletion sweep found it, and this
    repository treats that as dead code rather than as an equivalent mutant.
    """
    p = Path(git_dir) / LOCK_NAME
    try:
        st = p.stat()
    except OSError:
        return None
    return IndexLock(p.resolve(), st.st_size, time.time() - st.st_mtime)


def git_dir_of(root: Path | str, timeout: float | None = None) -> Path | None:
    """Where *root*'s git directory is, or ``None`` when *root* is not in a repository.

    Asked of git rather than assumed to be ``root/.git``, because the two differ exactly
    where this matters most: a **linked worktree** keeps its own index — and therefore its
    own lock — under ``<plane>/.git/worktrees/<name>``, and charter runs from worktrees all
    day. Guessing ``.git`` there would look at a file belonging to a different index and
    report the wrong tree healthy.

    Asked of the FILESYSTEM first, via `util.git_dir`, which already knows both spellings
    and costs no process: a clone's ``.git`` is a directory, a linked worktree's is a file
    holding ``gitdir: <path>``. `doctor` runs this on every SessionStart preflight, where a
    subprocess it does not need is a subprocess against the hook's whole budget.

    git is asked only when the filesystem cannot answer — a plane that is a SUBDIRECTORY of
    some larger repository has no ``.git`` of its own and a real git directory above it,
    and that is a layout `check_plane_root` explicitly supports.

    ``rev-parse --git-dir`` answers relative to the cwd when the cwd is at the top of the
    working tree, so the result is joined onto *root* before it is returned. It is **not**
    resolved on that path, and the reason is measured rather than assumed: git 2.50.1 asked
    from a subdirectory answers with an absolute, already-physical path (a symlinked route
    in is resolved away), and a subdirectory is the only way this branch is reached at all —
    the filesystem above answers for every tree that has a ``.git`` of its own. A
    normalisation with nothing left to normalise is a line nothing can tell apart from its
    own absence, which the deletion sweep found and this repository deletes.
    """
    quick = util.git_dir(Path(root))
    if quick is not None:
        return quick.resolve()
    try:
        r = util.run(["git", "-C", str(root), "rev-parse", "--git-dir"],
                     check=False, timeout=timeout)
    except (util.ProcTimeout, OSError):
        return None
    out = r.stdout.strip()
    if r.returncode != 0 or not out:
        return None
    return Path(root, out)


def for_repo(root: Path | str, timeout: float | None = None) -> IndexLock | None:
    """The index lock on *root*'s own index — :func:`git_dir_of` then :func:`find`."""
    git_dir = git_dir_of(root, timeout=timeout)
    return None if git_dir is None else find(git_dir)


def said(proc) -> str:
    """The first line git actually wrote, or ``""``.

    Kept and printed, rather than replaced with charter's own guess at what went wrong —
    `sweep.Sandbox._must` is the same decision for the same reason (#905): "a command that
    failed for a stated reason arrives as a command that failed" is how a full disk read as
    a broken sweep for a day. git's stderr under a held lock is four lines of which the
    first names the file, so the first line is the useful one and the rest is the advice
    charter is giving anyway.

    Stripped ONCE, and bound. The first draft tested ``if line.strip()`` and then returned
    ``line.strip()`` — two calls, of which only the second is observable: ``bool(s.strip())``
    and ``bool(s.lstrip())`` agree for every string, so the test's half was an equivalent
    mutant by construction. The deletion sweep reported it as a survivor and was right;
    binding the value is what makes the question unaskable rather than merely unanswered.
    """
    for line in (getattr(proc, "stderr", "") or "").splitlines():
        first = line.strip()
        if first:
            return first
    return ""


class TreeState(NamedTuple):
    """What `git status` said about a tree — in three states, not two.

    ``said is None`` means git answered and :attr:`rows` is the answer, empty for a clean
    tree. ``said`` holding text means **charter does not know**: git could not be run, ran
    out of time, or exited non-zero, and the text is what it wrote. :attr:`rows` is empty
    on that path too, which is precisely why the two must be told apart by a field rather
    than by whether the rows are empty.

    Deliberately has no ``dirty`` property. Reading a tree's dirtiness off a state that may
    not be known is the defect this type exists to make unwritable, and a convenience
    attribute would put it back one dot away.
    """

    #: The tree that was asked about, for the sentence a refusal prints.
    where: Path
    #: Porcelain lines, blank ones dropped. Empty when the tree is clean AND when
    #: :attr:`said` is set — check :attr:`known` first.
    rows: tuple[str, ...]
    #: ``None`` when git answered; what git said when it could not.
    said: str | None
    #: The index lock found while working out why, when there was one. Looked up only on
    #: the failure path, so a healthy `git status` still costs one process.
    lock: IndexLock | None

    #: There is no git repository here at all — a **definite** answer, not an unreadable
    #: one, and the two must not be run together.
    #:
    #: `charter init` in a fresh directory does not run `git init`, and that is the
    #: README's own 60-second path: a plane with no repository is a supported resting
    #: state, not a fault. Callers for which that state means "nothing to commit" — the two
    #: memory paths — check this and stay quiet, rather than telling every such plane at
    #: every session start that charter cannot read it. Callers guarding a deletion do
    #: **not** consult it: a directory git will not stand in is one charter cannot clear
    #: for removal, whatever the reason.
    no_repo: bool = False

    @property
    def known(self) -> bool:
        return self.said is None

    def why(self) -> str:
        """One line for a caller that has to explain itself, ``""`` when :attr:`known`.

        Names the tree, because the commonest confusion in this package is which tree a
        command is acting on (#806, #809), and a caller printing this may be standing in a
        different one.
        """
        if self.known:
            return ""
        return (f"charter could not read the working tree at {self.where} — {self.said}. "
                f"That is not the same as it being clean.")

    def remedy(self) -> list[str]:
        """Command lines the operator can act on, when charter has any to offer."""
        extra = self.lock.describe() if self.lock else ""
        return ([extra] if extra else []) + (self.lock.remedy() if self.lock else [])


def read(where: Path | str, *pathspec: str, timeout: float | None = None) -> TreeState:
    """``git status --porcelain`` on *where*, as a :class:`TreeState`.

    One choke point on purpose. The survey behind #917 found eleven independent readings of
    ``.stdout.strip()`` across seven modules, and they failed in pairs — both halves of "is
    memory unsynced", both halves of the gate that decides whether `workspace remove` may
    `rmtree` a clone. A fix that leaves the next caller free to write ``if
    _git(["status"…]).stdout.strip():`` has not fixed the class, so the shape a caller
    reaches for is the one that cannot express the bug.

    No flag pass-through, deliberately: every caller here wants the same question, and
    `doctor.check_plane_root` — the one site that asks it with ``--untracked-files=no`` —
    keeps its own `_git_in` call, because every other git question in that function is
    asked and rc-checked the same way and a lone import would make it the odd one out.
    """
    argv = ["git", "-C", str(where), "status", "--porcelain"]
    if pathspec:
        argv += ["--", *pathspec]
    try:
        r = util.run(argv, check=False, timeout=timeout)
    except (util.ProcTimeout, OSError) as e:
        # A git that could not be RUN — no binary, a cwd that has been deleted, a mount
        # that stopped answering inside the budget. Nothing was measured, so nothing may
        # be reported.
        return TreeState(Path(where), (), f"{type(e).__name__}: {e}", None)
    if r.returncode != 0:
        git_dir = git_dir_of(where, timeout=timeout)
        return TreeState(Path(where), (), said(r) or f"git status exited {r.returncode}",
                         None if git_dir is None else find(git_dir), git_dir is None)
    # `splitlines()` and nothing else. A blank-line filter here was defensive against
    # something porcelain v1 does not produce — every record is `XY <path>` and the format
    # has no empty records — and `"a\n".splitlines()` yields no trailing empty string, so
    # nothing could tell the filter apart from its own absence. The deletion sweep found it.
    return TreeState(Path(where), tuple(r.stdout.splitlines()),
                     None, None)
