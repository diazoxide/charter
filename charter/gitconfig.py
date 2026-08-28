"""What a repository's own config says about where its files land — read, never asked.

``core.worktree`` is git's **third** spelling of the work tree, and the only one that is not
in the invocation. ``--work-tree`` and ``GIT_WORK_TREE`` are tokens `hooks._git_target`
already reads off the command line and the environment. This one is a key in the
repository's ``.git/config``: a repository carrying it has the named directory as its
working tree for **every** command, and nothing on the command line says so — so a guard
reading argv and environment sees a plain ``git checkout feature`` typed inside a workspace
clone (#504).

Verified end to end against git 2.50.1, not derived:

    git clone <plane> /tmp/cfgclone
    git -C /tmp/cfgclone config core.worktree <plane>
    git -C /tmp/cfgclone rev-parse --show-toplevel      # -> <plane>
    git -C /tmp/cfgclone checkout feature               # -> the PLANE's f.txt changed

**Read rather than asked, and that is the whole design constraint.** ``git rev-parse
--show-toplevel`` answers this exactly and costs a subprocess — some ten milliseconds — on a
path that runs inside every PreToolUse hook, where the common case currently exits on a
string comparison. So charter reads the file: one walk up to the repository (a handful of
``stat`` calls, and none at all when the invocation names its own ``--git-dir``) and one
read of a file that is under a kilobyte in every repository anybody has. Measured on this
repository, warm: see ``tests/test_a_repository_can_name_its_own_work_tree.py``, which
states the number rather than leaving it to be re-measured.

**What is deliberately NOT read**, because each one is a cost without a matching risk:

* ``git -c core.worktree=<dir>`` on the command line. Git ignores it — verified in both
  spellings, with and without an explicit ``--git-dir``, on 2.50.1 — so the form an agent
  would actually type reaches nothing and reading it would only manufacture refusals.
* ``include`` / ``includeIf`` directives. Following them means a second file read per
  invocation, with a conditional evaluation of git's own ``gitdir:``/``onbranch:`` matchers
  behind it, on the hot path. A repository that hides ``core.worktree`` behind an include
  is not covered; that is recorded here rather than implied away.
* The global and system configs. Git honours ``core.worktree`` from those only when
  ``$GIT_DIR`` is set, which the invocations this guards do not do.

**Every failure answers "no work tree named".** That is fail-open by construction and it is
the honest direction: this ADDS a subject to a guard's list, so an answer it could not
produce leaves the guard exactly as strong as it was before this module existed, while an
invented answer would refuse a command with nothing wrong with it.
"""

from __future__ import annotations

import os
from pathlib import Path

#: The most of a repository config that is read before this gives up.
#:
#: A git config is a small hand-written file — this repository's own is under a kilobyte,
#: and the largest thing anybody puts in one is a list of remotes. The bound is here so a
#: file that is not that cannot make a PreToolUse guard read a gigabyte off disk before it
#: can answer. Reaching it means the tail is not parsed, which answers "no work tree named"
#: for a key that lived past the bound — the fail-open direction this module's docstring
#: commits to, and preferable to a hook that never returns.
MAX_CONFIG_BYTES = 262_144

#: The most of a ``.git`` FILE that is read. It holds one ``gitdir: <path>`` line.
_MAX_POINTER_BYTES = 4096

#: The escapes git's config parser understands inside a quoted value. Anything else after a
#: backslash is that character — which is what makes ``\\`` a backslash and ``\"`` a quote.
_ESCAPES = {"n": "\n", "t": "\t", "b": "\b"}


def configured_work_tree(cwd, git_dir=None) -> Path | None:
    """The directory ``core.worktree`` makes this invocation's work tree, or ``None``.

    *cwd* is where the command runs — after any ``-C``, which is `hooks._git_target`'s
    business and not this module's. *git_dir* is the repository the invocation NAMED, if it
    named one: with ``--git-dir``/``GIT_DIR`` there is no discovery to do, and without one
    git ascends from the cwd, so this does the same.

    Relative values resolve against the **git directory**, which is git's own documented
    rule and was checked rather than assumed: ``worktree = ../../plane`` in
    ``<clone>/.git/config`` answers ``<clone>/../plane``, and ``../plane`` — the value that
    looks right if you resolve against the work tree — makes git refuse the repository
    outright.

    Never raises. See the module docstring: every way this can fail answers ``None``, which
    leaves the caller's subject list exactly as it was.
    """
    gd = _git_dir_at(cwd) if git_dir is None else _as_path(git_dir)
    if gd is None:
        return None
    value = _core_worktree(gd)
    if not value:
        return None
    return Path(value) if os.path.isabs(value) else gd / value


def _as_path(value) -> Path | None:
    try:
        return Path(value)
    except (OSError, TypeError, ValueError):
        return None


def _git_dir_at(cwd) -> Path | None:
    """The ``.git`` git would discover from *cwd*, without running git.

    Ascends the way git does, and stops at the first ``.git`` rather than at the first one
    that looks like a repository: git stops there too, and a broken one is not this
    module's to diagnose.

    A ``.git`` **file** — a submodule, or a linked worktree — is followed one hop to the
    directory it names. One, not a chain: git writes exactly one level, and a pointer that
    pointed at another pointer would be a loop to bound rather than a case to support.
    """
    start = _as_path(cwd)
    if start is None:
        return None
    try:
        start = Path(os.path.abspath(start))
    except (OSError, ValueError):
        return None
    for directory in (start, *start.parents):
        dot = directory / ".git"
        try:
            if dot.is_dir():
                return dot
            if dot.is_file():
                return _pointer_target(dot, directory)
        except OSError:
            return None
    return None


def _pointer_target(dot: Path, base: Path) -> Path | None:
    """The directory a ``.git`` file names, or ``None``."""
    try:
        with open(dot, "rb") as fh:
            head = fh.read(_MAX_POINTER_BYTES)
    except OSError:
        return None
    line = head.decode("utf-8", "replace").splitlines()[:1]
    if not line:
        return None
    label, sep, target = line[0].partition(":")
    if not sep or label.strip().lower() != "gitdir":
        return None
    target = target.strip()
    if not target:
        return None
    return Path(target) if os.path.isabs(target) else base / target


def _core_worktree(git_dir: Path) -> str | None:
    """``core.worktree`` out of ``<git_dir>/config``, as git reads it."""
    try:
        with open(git_dir / "config", "rb") as fh:
            raw = fh.read(MAX_CONFIG_BYTES)
    except OSError:
        return None
    text = raw.decode("utf-8", "replace")
    # The common case, answered without parsing anything: the overwhelming majority of
    # repositories have no `worktree` key at all, and a substring test over a sub-kilobyte
    # string is what keeps this affordable on the hook path.
    if "worktree" not in text.lower():
        return None
    found = None
    for section, subsection, key, value in _pairs(text):
        # A subsection makes it `core.<sub>.worktree`, which is a different key and does
        # not relocate anything. Named rather than ignored, because `[core "x"]` reads like
        # `[core]` to a scanner that only looks at the first word.
        if section == "core" and not subsection and key == "worktree":
            found = value
    return found


def _pairs(text: str):
    """Yield ``(section, subsection, key, value)`` for a git config file's text.

    A deliberately small subset of git's own parser, and the subset is stated: section
    headers with an optional quoted subsection, ``key = value`` lines, ``#``/``;`` comments
    outside quotes, quoted values with git's escapes, and a value continued onto the next
    line with a trailing backslash. What it does not do is in the module docstring; what it
    gets wrong yields a value nobody will match, which adds nothing to any caller's list.
    """
    section = subsection = ""
    for line in _logical_lines(text):
        rest = line.strip()
        if not rest:
            continue
        if rest.startswith("["):
            end = rest.find("]")
            if end < 0:
                continue
            head, rest = rest[1:end].strip(), rest[end + 1:].strip()
            if '"' in head:
                name, _, sub = head.partition('"')
                section, subsection = name.strip().lower(), sub.rsplit('"', 1)[0]
            else:
                section, subsection = head.lower(), ""
            if not rest:
                # `[core] worktree = x` on one line is legal git, so the remainder of a
                # header line is a key line rather than something to drop.
                continue
        if rest.startswith(("#", ";")):
            continue
        key, sep, value = rest.partition("=")
        if not sep:
            continue          # a valueless key is a boolean, and never names a directory
        yield section, subsection, key.strip().lower(), _scalar(value)


def _logical_lines(text: str):
    """*text*'s lines, with a trailing backslash joining a line to the next."""
    pending = ""
    for line in text.splitlines():
        line = pending + line
        pending = ""
        if line.endswith("\\") and not line.endswith("\\\\"):
            pending = line[:-1]
            continue
        yield line
    if pending:
        yield pending


def _scalar(raw: str) -> str:
    """A git config VALUE as git reads it.

    Leading whitespace is git's to drop; trailing whitespace is dropped only **outside**
    quotes, which is the whole reason this is a scanner and not a `strip` — ``"/pa th  "``
    is a directory whose name ends in two spaces and git honours it. A ``#`` or ``;``
    outside quotes starts a comment; inside them it is an ordinary character.
    """
    out: list[str] = []
    keep = 0                              # how much of `out` survives the trailing trim
    quoted = False
    i, raw = 0, raw.lstrip()
    while i < len(raw):
        ch = raw[i]
        if ch == "\\" and i + 1 < len(raw):
            nxt = raw[i + 1]
            out.append(_ESCAPES.get(nxt, nxt))
            keep = len(out)
            i += 2
            continue
        if ch == '"':
            quoted = not quoted
            i += 1
            continue
        if ch in "#;" and not quoted:
            break
        out.append(ch)
        if quoted or not ch.isspace():
            keep = len(out)
        i += 1
    return "".join(out[:keep])
