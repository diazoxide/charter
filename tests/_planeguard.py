"""Suite-wide tripwires: no test may WRITE into the developer's real ``.charter/``, none
may READ a setting off the developer's real ``charter.toml``, and none may SPAWN a charter
that would resolve it.

`PersonaIso` isolates a test that remembers to derive from it. Nothing made the
*forgetting* visible, so the same defect kept arriving in a new file: the suite wrote
fixture data into a contributor's real ``.charter/vaults.json`` and orphaned every vault
on that machine (#recorded in `_isolation.py`), and later three plain `unittest.TestCase`
classes ran the real `cmd_launch` — `state.reap`'s ``rmtree`` of every frame directory the
tmux server does not report live — against whatever frames the developer had open (#402).
Both were found by hand, months apart, by someone who happened to look.

This makes the class of mistake self-reporting instead. Every call that could CREATE,
DELETE or REPLACE something under the real state directory is wrapped once, at import of
the `tests` package — before any test module is collected, so no test can opt out by
forgetting a base class. A test that reaches the real plane now fails, by name, on the
line that reached it, having changed nothing.

**Only the state directory, for writes.** ``.charter/`` is per-developer, gitignored, and
holds live session state — frame directories, the vault registry, session pointers. The
rest of the plane (``personas/``, ``docs/``, ``workspaces/``) is committed content some
tests do legitimately generate into a tmp root, and guarding the real copies of those
would trade this defect for a stream of false alarms.

**Reads of the state directory stay untouched**, for the same reason: a test may
legitimately look at the real plane (that `render()` survives a real environment, say),
and `_isolation.isolate_state_dir` exists for exactly that shape.

**One class of read IS refused: a setting the developer's own `charter.toml` declares.**
#459. `[update] channel` is opt-in and new, so for its whole life `config.UPDATE` was
``{"channel": "stable"}`` on every machine — which is what `test_statusline_brand`'s
``UpdateIndicator`` and `test_version_lock`'s ``AutoSync`` assumed while isolating neither.
The day charter's own dogfood plane declared ``channel = "dev"``, six tests started failing
on that machine and nowhere else, and the feature became unusable: `charter update` on a
charter checkout only refreshes the plugin on the dev channel, so the operator has to
declare it — in a file `charter save` commits, which turns CI red for everyone.

A value like that is not a fact about charter; it is a fact about whoever is running the
suite. A test reading it is not "reading the real plane" in the benign sense above — it is
asserting against a fixture it did not write and cannot see. So the values named in
:data:`_GUARDED_SETTINGS` are replaced, whenever `charter.config` points at the real plane,
by a `dict` that refuses to be read (:class:`RealPlaneRead`). Isolate the case and the real
value is never in place to refuse: `config.use` re-derives every setting from the tmp root
(`_isolation.PersonaIso`), and `mock.patch.object(config, "UPDATE", …)` replaces the object
outright.

The arming is by ROOT rather than a one-shot swap at import: :func:`charter.config.derive`
is wrapped, so a derivation that resolves back to the real plane re-arms (a bare
`config.use(config.ROOT)`, which two modules here do) and a derivation from a tmp root does
not. `config.restore` needs nothing — it puts back the snapshot `config.use` took, and the
refusing value is what was in place to snapshot.

**Raised as a `BaseException`, deliberately.** charter is full of `except Exception`
fallbacks that turn an unreachable path into a degraded one — `config.derive` catches
everything so a malformed `charter.toml` cannot break import, `root.find_root` swallows
`OSError`. A tripwire those can catch is a tripwire that reports "no vault configured"
instead of failing. `unittest` still records a `BaseException` against the test that raised
it, so the failure keeps its name.

**The subprocess half, which this used to say it could not see.** A `tmux run-shell` hook,
or any spawned `charter`, resolves its own plane from its own environment and its own cwd,
so isolating THIS process does nothing for it — the parent must hand the throwaway plane
across the boundary with ``$CHARTER_ROOT`` on the child's (or the tmux server's)
environment, the way `PanelIntegration` and `MenuIntegration` in
`test_frame_tmux_integration.py` do. That paragraph stood here as a warning for as long as
it took someone to measure it, and the measurement (#527) was 131 detached
``charter _version-check`` / ``charter gl-refresh`` children in one run, every one of them
carrying no ``$CHARTER_ROOT`` and therefore landing on the operator's live plane —
refreshing its forge state and rewriting its caches, from a suite that had isolated
everything it could see.

So the warning is now a tripwire: :class:`RealPlaneSpawn`. Before any `subprocess.Popen`
that launches charter itself, this asks `root.find_root` **the child's** question — with
the child's environment and the child's cwd — and refuses if the answer is the real plane.
Asking `find_root` rather than re-deriving the walk is deliberate: the walk is where the
subtleties live (a linked worktree redirects to the tree it was cut from, a plane inside a
plane's ``workspaces/`` hops outward), and a guard with its own private copy of that logic
is a guard that stops agreeing with the thing it guards.

**By any spelling, because the question is a property and not a pattern.** The first
version of that tripwire recognised the two argvs charter's own code writes, and measuring
it against its own ``_REAL_ROOT`` showed what that leaves open: ``[python, "-m", "charter",
"--version"]`` was refused, while ``[python, "-c", "from charter import config"]``,
``["/bin/sh", "-c", "<python> -m charter --version"]`` and the same command as a
``shell=True`` string all RAN, against the operator's live plane. Two of those were already
in the suite — nine charter-importing ``python -c`` children per run, plus a self-spawning
chain of them in `test_news_cross_process`, each resolving the operator's plane at import
and only then calling `config.use`. `_charter_argv` now asks "will this child resolve the
operator's plane as charter", and where a spelling cannot be decided it answers "charter"
and lets the plane check settle it: a false refusal in a test is loud, named and one line
from the fix, and a false allow writes to a live machine.

**Charter's own spawners now hand the plane over** (`util.child_env`), so an isolated case
satisfies this without knowing it exists. What is left for the tripwire is the case that
was never covered: a test that spawns charter by hand, and a test that never isolated
`config.ROOT` in the first place.
"""

from __future__ import annotations

import ast
import builtins
import io
import os
import re
import shlex
import shutil
import subprocess
import sys
import tokenize
import unittest
from pathlib import Path

#: The state directory of the plane this test PROCESS resolved at import — before any
#: `setUp` could repoint `charter.config`, so unavoidably the developer's real one.
#:
#: A TUPLE, because one directory has two spellings a caller may use: the path as written
#: and the path with every symlink resolved. A checkout under a symlinked home (or under
#: macOS's ``/tmp`` → ``/private/tmp``) makes those differ, and a guard that knew only the
#: resolved one would wave through every write spelled the other way — silence that reads
#: exactly like safety. Computed once, at install; the per-call check is a string compare.
_REAL: tuple[str, ...] = ()


class RealPlaneWrite(BaseException):
    """A test tried to write into the developer's own control-plane state directory."""


def _explain(op: str, path) -> str:
    return (
        f"REFUSED: {op} {path}\n"
        f"This test is writing into the developer's REAL control-plane state directory "
        f"({_REAL[0]}) — live vaults, session pointers, and the frame state of running "
        f"sessions. Derive the test case from `tests._isolation.PersonaIso`, or, if it "
        f"deliberately exercises the real plane, call "
        f"`tests._isolation.isolate_state_dir(self)` to redirect just this directory. "
        f"A subprocess needs the throwaway plane passed to it as $CHARTER_ROOT.")


def _hits(path, dir_fd=None) -> bool:
    """Does *path* land inside the real state directory?

    ``dir_fd`` is answered ``False``: the path is then relative to an open directory this
    cannot resolve, and guessing against the cwd would refuse the wrong calls. The one
    caller that uses it is `shutil.rmtree`'s fd-based recursion, which is refused whole at
    its own front door below — so nothing is lost by declining to guess here.

    On the hot path: this runs on every `open` in the suite, so an already-absolute,
    already-normal path is answered by string compares alone. `os.path.abspath` — a
    `getcwd` syscall per call — is reached only by a relative path or one containing
    ``..``, which is where it is actually needed.
    """
    if not _REAL or dir_fd is not None:
        return False
    try:
        p = os.fspath(path)
    except TypeError:
        return False                      # an int fd, or something not a path at all
    if isinstance(p, bytes):
        p = os.fsdecode(p)
    for real in _REAL:
        if p == real or p.startswith(real + os.sep):
            return True
    if p.startswith(os.sep) and os.pardir not in p:
        return False
    p = os.path.abspath(p)
    return any(p == real or p.startswith(real + os.sep) for real in _REAL)


def _guard_os(name: str, *, arg: int = 0, both: bool = False) -> None:
    """Wrap ``os.<name>``, watching the argument(s) naming the path it would change.

    *arg* because the created path is not always the first one: ``os.symlink(target,
    link)`` and ``os.link(src, dst)`` both CREATE their SECOND argument and only read
    their first. *both* because ``os.rename``/``os.replace`` change two paths at once —
    the source vanishes and the destination is overwritten — so watching either alone
    leaves a way in.
    """
    original = getattr(os, name)

    def guarded(*args, **kw):
        watched = (0, 1) if both else (arg,)
        for i in watched:
            if len(args) > i and _hits(args[i], kw.get("dir_fd")):
                raise RealPlaneWrite(_explain(f"os.{name}", args[i]))
        return original(*args, **kw)

    guarded.__name__ = name
    setattr(os, name, guarded)


#: The settings no test may read off the developer's own ``charter.toml``.
#:
#: One name, and it is a list rather than a special case for exactly one reason: the next
#: opt-in setting to arrive has the same shape, and the day it arrives is the day it should
#: be guarded, not the day someone re-derives this argument from a red CI run.
#:
#: Not every `config.DERIVED` name belongs here, and the boundary is what the value is a
#: fact ABOUT. ``ROOT``, ``STATE_DIR`` and the paths under them are facts about the
#: machine's layout, and a test that reads them deliberately (`isolate_state_dir`'s whole
#: purpose) is doing something this suite supports. ``UPDATE`` is a fact about the operator's
#: *preference*, declared in a committed file that one contributor may edit and the rest may
#: not have — so a test reading it asserts against a fixture written by whoever happens to
#: run it. Values that are dicts, and only dicts: the refusal works by handing back an
#: object that will not answer, and a `str` or a `Path` cannot be made to refuse without
#: breaking the formatting of every message that legitimately quotes it.
_GUARDED_SETTINGS = ("UPDATE",)


class RealPlaneRead(BaseException):
    """A test read a setting declared by the developer's own ``charter.toml``."""


def _current_test() -> str:
    """The `TestCase` on the stack, ``module.Class.method``, or a placeholder.

    Walked rather than tracked, because there is no hook that fires per test in a plain
    ``python3 -m unittest discover`` run. Only ever called on the refusal path, so the walk
    costs nothing in a green suite. `unittest` already puts the test's name in the failure
    header; this repeats it INSIDE the message so a failure quoted into an issue, a CI log
    excerpt or a `git bisect` transcript still says which test it was.
    """
    frame = sys._getframe(1)
    while frame is not None:
        case = frame.f_locals.get("self")
        if isinstance(case, unittest.TestCase):
            method = getattr(case, "_testMethodName", "?")
            return f"{type(case).__module__}.{type(case).__name__}.{method}"
        frame = frame.f_back
    return "this test"


def _explain_read(name: str) -> str:
    return (
        f"REFUSED: read of config.{name}\n"
        f"{_current_test()} is reading `{name}` while `charter.config` still points at the "
        f"developer's REAL control plane ({_REAL_ROOT[0] if _REAL_ROOT else '?'}) — so what "
        f"it asserts depends on what that machine's own charter.toml declares, not on a "
        f"fixture. `[update] channel = \"dev\"` in a real plane is what turned six of these "
        f"red on one machine and nowhere else (#459). Derive the case from "
        f"`tests._isolation.PersonaIso`, which re-derives every setting from a tmp plane in "
        f"one `config.use()` call, or pin just this one with "
        f"`mock.patch.object(config, \"{name}\", {{...}})`.")


class _RefusesToBeRead(dict):
    """What a guarded setting holds while `charter.config` points at the real plane.

    A `dict` subclass rather than a sentinel of its own type, because the readers type-check
    first: `channel.channel` does ``isinstance(got, dict)`` and falls back to ``"stable"``
    for anything else — a sentinel that failed that check would be *silently* ignored, which
    is the behaviour this guard exists to end. Subclassing means the refusal happens where
    the value is actually used, one line later.

    **Every accessor that yields a KEY or a VALUE is overridden**, including the ones
    CPython would otherwise service from C without consulting this class: ``dict(x)`` and
    ``{**x}`` take the fast path only while ``tp_iter`` is dict's own, so overriding
    ``__iter__`` is what routes them through ``keys()`` and into the refusal. The list was
    audited by probing every entry point rather than reasoned about — `pop`, `popitem`,
    `setdefault` and `__reversed__` each handed back the operator's real value on the first
    pass, and no reader in `charter/` uses any of them, which is exactly why they would
    have stayed open until one did.

    ``__repr__`` and ``__len__`` (and so ``bool()``) are deliberately NOT overridden. A
    debugger, a traceback, or `unittest`'s own error formatting may print this object, and
    a guard that explodes while a test is already failing hides the failure it was supposed
    to explain. Neither answers "which channel", which is the fact being protected.
    """

    __slots__ = ("_setting",)

    def __init__(self, setting: str, value: dict) -> None:
        super().__init__(value)
        self._setting = setting

    def _refuse(self, *_args, **_kw):
        raise RealPlaneRead(_explain_read(self._setting))

    get = _refuse
    __getitem__ = _refuse
    __contains__ = _refuse
    __iter__ = _refuse
    __reversed__ = _refuse
    keys = _refuse
    values = _refuse
    items = _refuse
    copy = _refuse
    pop = _refuse
    popitem = _refuse
    setdefault = _refuse
    __eq__ = _refuse
    __ne__ = _refuse
    __hash__ = None


#: The control-plane root this test PROCESS resolved at import, in both spellings, for the
#: same reason :data:`_REAL` carries two.
_REAL_ROOT: tuple[str, ...] = ()


def _guard_reads(config) -> None:
    """Arm :data:`_GUARDED_SETTINGS` on every derivation that lands on the real plane."""
    global _REAL_ROOT
    if _REAL_ROOT:                        # idempotent: never wrap `derive` twice
        return
    written = os.path.abspath(str(config.ROOT))
    resolved = os.path.realpath(written)
    _REAL_ROOT = (written,) if written == resolved else (written, resolved)

    def _is_real(where) -> bool:
        try:
            here = os.path.abspath(str(where))
        except (OSError, TypeError, ValueError):
            return False
        return here in _REAL_ROOT or os.path.realpath(here) in _REAL_ROOT

    original = config.derive

    def derive(root, start=None):
        d = original(root, start)
        if _is_real(root):
            for name in _GUARDED_SETTINGS:
                value = d.get(name)
                if isinstance(value, dict) and not isinstance(value, _RefusesToBeRead):
                    d[name] = _RefusesToBeRead(name, value)
        return d

    derive.__module__ = __name__
    config.derive = derive

    # The bootstrap derivation already ran, at import of `charter.config`, before this
    # module existed to wrap anything. Arm what it left behind.
    for name in _GUARDED_SETTINGS:
        value = getattr(config, name, None)
        if isinstance(value, dict) and not isinstance(value, _RefusesToBeRead):
            setattr(config, name, _RefusesToBeRead(name, value))


class RealPlaneSpawn(BaseException):
    """A test spawned a charter that would resolve the developer's own control plane."""


#: ``NAME=value`` — the shell's environment-assignment prefix, and `env`'s argument form.
_ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z_0-9]*=")

#: An interpreter whose ``-c`` is Python SOURCE and whose first bare argument is a Python
#: script. `sys.executable`'s own basename is accepted alongside it, because a virtualenv,
#: a framework build or a `uv`-managed toolchain may spell it something this pattern does
#: not — and `sys.executable` is what every child in this suite is launched with.
_PYTHON_NAME = re.compile(r"^(?:python|pypy)[0-9.]*t?$")

#: A shell whose ``-c`` is a command STRING, read by :func:`_shell_launches_charter`.
_SHELL_NAMES = frozenset(("sh", "bash", "zsh", "dash", "ksh", "ash", "mksh", "busybox"))

#: Shell operators that END one command and begin the next, as `shlex` with
#: ``punctuation_chars=True`` hands them back.
_SHELL_SEPARATORS = frozenset((";", ";;", "&", "&&", "|", "||", "(", ")", "\n"))

#: Shell operators whose NEXT token is a file, not a command word. Without this
#: ``>/dev/null charter doctor`` would be read as a command called ``/dev/null``.
_SHELL_REDIRECTS = frozenset((">", ">>", "<", "<<", "<<<", ">&", "<&", ">|", "&>", "&>>",
                              "<>"))

#: Commands whose ARGUMENTS are themselves a command. ``sudo charter doctor`` has ``sudo``
#: in the command position and charter one word later; a reader that looked only at the
#: head would call that an argument and allow it. Their argument grammars differ enough
#: (``timeout 5 charter``, ``xargs -n1 charter``) that following them exactly is not worth
#: attempting -- the word appearing anywhere after one of these is refused.
_COMMAND_WRAPPERS = frozenset((
    "eval", "exec", "command", "builtin", "sudo", "doas", "su", "nohup", "nice", "setsid",
    "stdbuf", "time", "timeout", "xargs", "watch", "script", "env", "flock", "ionice"))

#: ``charter`` as a WORD inside a shell command string. ``/`` is allowed BEFORE it, so an
#: absolute path to the binary (``/usr/local/bin/charter --version``) is a hit, and
#: forbidden AFTER it, so a path that merely contains the checkout
#: (``cd ~/IdeaProjects/charter/tests``) is not.
_CHARTER_WORD = re.compile(r"(?<![\w.-])charter(?![\w.\-/])")

#: The names of `subprocess.Popen.__init__`'s positional parameters, in order after *args*.
#: The guard has to know what the child was told, and a caller may say it either way:
#: ``Popen(argv, cwd=here)`` and ``Popen(argv, -1, None, None, None, None, None, True,
#: False, here)`` describe the same child. Reading only ``**kw`` answers the second one
#: "no cwd, no env" and asks `find_root` the wrong question — silently, and in the
#: direction that allows.
_POPEN_POSITIONAL = ("bufsize", "executable", "stdin", "stdout", "stderr", "preexec_fn",
                     "close_fds", "shell", "cwd", "env")


def _decoded(value) -> str | None:
    """*value* as a `str`, or ``None`` when it is not a path-like thing at all."""
    try:
        return os.fsdecode(value) if isinstance(value, (bytes, os.PathLike)) else str(value)
    except (TypeError, ValueError):
        return None


def _is_python(name: str) -> bool:
    return bool(_PYTHON_NAME.match(name)) or name == os.path.basename(sys.executable)


def _module_is_charter(name: str) -> bool:
    return name == "charter" or name.startswith("charter.")


def _code_imports_charter(code: str) -> bool:
    """Does this Python SOURCE name the ``charter`` module?

    Tokenized rather than pattern-matched, because the two things that have to be told
    apart are a NAME and a path that happens to contain the word: ``from charter import
    config`` is a charter child, and ``open('/Users/x/IdeaProjects/charter/canary', 'w')``
    — which the frame's tmux cases really do spawn — is not. A regex over the raw text
    cannot see that difference; `tokenize` can, and it is the lexer the child itself will
    use.

    A STRING whose whole value is ``charter`` (or ``charter.something``) counts too: that
    is `__import__("charter")` and `importlib.import_module("charter.util")`. A module name
    assembled at runtime would slip through, and is left to: nothing in this suite writes
    one, and the alternative — refusing on the bare word — refuses every child whose argv
    quotes a path inside the checkout.

    **``import tests`` counts as an import of charter**, and only in an import statement.
    This package's ``__init__`` imports `charter.config` to arm the guards, so a child that
    imports it resolves a plane without the word ever appearing —
    `test_no_test_reads_the_operators_shell` spawns exactly that, and it was the last child
    in the suite still landing on the operator's plane once the ``-c`` shape was closed.
    Restricting it to an import position is what keeps a local variable called ``tests``
    from being read as one.

    That child is also the one case where ``$CHARTER_ROOT`` is NOT the way out: `_envguard`
    scrubs the whole charter namespace at import of this package, *before* `charter.config`
    loads, so the pointer is gone by the time the plane is resolved and the child's CWD
    decides. Its fix is a cwd inside a throwaway plane, with ``$PYTHONPATH`` carrying the
    tree.

    **Source that will not tokenize is refused, not allowed.** It is either a child that
    cannot run at all — in which case the refusal is loud and one line from the fix — or a
    lexer disagreement, and a guard that settles its own uncertainty by waving the child
    through is the failure this one exists to end.
    """
    try:
        importing = False
        for tok in tokenize.generate_tokens(io.StringIO(code).readline):
            if tok.type == tokenize.NAME:
                if tok.string == "charter":
                    return True
                if tok.string in ("import", "from"):
                    importing = True
                    continue
                if importing and tok.string == "tests":
                    return True
            if tok.type in (tokenize.NEWLINE, tokenize.NL) or (
                    tok.type == tokenize.OP and tok.string == ";"):
                importing = False
            if tok.type == tokenize.STRING:
                try:
                    value = ast.literal_eval(tok.string)
                except (ValueError, SyntaxError, MemoryError, RecursionError):
                    continue
                if isinstance(value, str) and _module_is_charter(value):
                    return True
    except (tokenize.TokenError, IndentationError, SyntaxError, ValueError):
        return True
    return False


def _script_imports_charter(path: str) -> bool:
    """Does the Python FILE at *path* name the charter module?

    ``python <plane>/charter/__main__.py --version`` is charter spelled without ``-m``, and
    so is a probe written to a temp file that imports the tree under test. Both are asked,
    in that order, because neither test alone answers both: charter's own ``__main__.py``
    reaches the CLI through ``from .cli import main`` and never writes the word, so reading
    it says "not charter" about the entry point itself — while a name test alone would have
    to decide what ``/tmp/probe.py`` is, and can only guess.

    So: a module of the ``charter`` package, run by path, is charter by its location.
    Anything else is read, because the file is on disk and reading it is the honest answer.
    Unreadable, or too large to be a probe, is answered ``True``: both mean this cannot be
    decided here, and refusal is the direction that is safe to be wrong in.
    """
    if os.path.basename(os.path.dirname(path)) == "charter":
        return True
    try:
        if os.path.getsize(path) > 1 << 20:
            return True
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            return _code_imports_charter(fh.read())
    except OSError:
        return True


def _strip_launcher_prefix(parts: list[str]) -> list[str]:
    """Drop ``VAR=value`` assignments and an `env` wrapper to reach the real command.

    ``env -u CHARTER_SESSION_ID python3 -m charter --version`` launches charter; a guard
    that stopped reading at ``env`` would answer that it does not. Only the option forms
    `env` itself documents are consumed — anything else ends the walk, because a guard
    guessing at an unknown option's arity would start skipping the command word itself.
    """
    i, n = 0, len(parts)
    while i < n:
        tok = parts[i]
        if _ASSIGNMENT.match(tok):
            i += 1
            continue
        if os.path.basename(tok) != "env":
            break
        i += 1
        while i < n:
            tok = parts[i]
            if _ASSIGNMENT.match(tok) or tok in ("-i", "--ignore-environment", "-0",
                                                 "--null", "-v", "--debug"):
                i += 1
            elif tok in ("-u", "--unset", "-C", "--chdir", "-S", "--split-string"):
                i += 2
            elif tok.startswith(("--unset=", "--chdir=", "--split-string=")):
                i += 1
            else:
                break
    return parts[i:]


def _python_launches_charter(rest: list[str]) -> bool:
    """The tail of a Python interpreter's argv: does it reach charter?

    ``-c``, ``-m`` and a bare script path are the three ways in, and each option may be
    spelled attached (``-mcharter``), separate (``-m charter``) or bundled behind other
    short options (``-Pmcharter``) — all of which CPython accepts, and one of which
    `util.self_relaunch_argv` is a single edit away from producing.
    """
    i, n = 0, len(rest)
    while i < n:
        tok = rest[i]
        if not tok.startswith("-") or tok == "-":
            return _script_imports_charter(tok)
        if tok.startswith("--"):
            i += 1
            continue
        skip = 1
        for j, ch in enumerate(tok[1:], start=1):
            if ch in "cm":
                value = tok[j + 1:] or (rest[i + 1] if i + 1 < n else "")
                return (_code_imports_charter(value) if ch == "c"
                        else _module_is_charter(value))
            if ch in "WXQ":               # takes a value, and is no way into charter
                if not tok[j + 1:]:
                    skip = 2
                break
        i += skip
    return False


def _substitution_bodies(command: str) -> list[str]:
    """Every ``$( … )`` and `` ` … ` `` body in *command*, unnested by one level.

    These are the reason a plain command-position reader is not enough, and the omission
    was not hypothetical: `hooks/hooks.json` — the file this guard's docstring cites as its
    reason for recognising a bare ``charter`` — spells one of its commands
    ``out="$(charter doctor 2>&1)" || …``. Every lexer in the world reads that as an
    ASSIGNMENT and moves on, so the charter invocation inside it is in no command position
    at all. It is one here, and :func:`_shell_launches_charter` reads it as its own command
    string.

    An unterminated substitution yields what there is rather than nothing: the string is
    malformed either way, and the half that can be read is the half that names the command.
    """
    bodies: list[str] = []
    i, n = 0, len(command)
    while i < n:
        ch = command[i]
        if ch == "`":
            j = command.find("`", i + 1)
            bodies.append(command[i + 1:] if j < 0 else command[i + 1:j])
            if j < 0:
                break
            i = j + 1
        elif ch == "$" and command.startswith("$(", i):
            depth, j = 1, i + 2
            while j < n and depth:
                depth += (command[j] == "(") - (command[j] == ")")
                j += 1
            bodies.append(command[i + 2:j - 1] if not depth else command[i + 2:])
            i = j
        else:
            i += 1
    return bodies


def _shell_segments(command: str) -> list[list[str]]:
    """*command* split into the individual commands a shell would run, as word lists.

    Raises `ValueError` when it will not lex — an unbalanced quote — which the caller reads
    as "undecidable" and refuses.
    """
    lex = shlex.shlex(command, posix=True, punctuation_chars=True)
    lex.whitespace_split = True
    segments: list[list[str]] = [[]]
    skip = False
    for tok in lex:
        if skip:                          # a redirect's target, not a command word
            skip = False
        elif tok in _SHELL_REDIRECTS:
            skip = True
        elif tok in _SHELL_SEPARATORS:
            segments.append([])
        else:
            segments[-1].append(tok)
    return segments


def _shell_launches_charter(command: str, depth: int = 0) -> bool:
    """Does this shell command STRING run charter?

    The word has to be there at all — that gate is what keeps ``pwd > "$out"`` and the
    frame's tmux fixtures out of the lexer entirely. Once it is there, the only question
    left is whether it is a COMMAND or an ARGUMENT, and both spellings are live in this
    suite. `test_toolgate` asks a real bash to echo its corpus back — ``printf "%s\\x00"
    charter "sec"'ret' list v`` — and never runs charter; ``charter workspace _reconcile
    >/dev/null 2>&1`` is charter's own hook, and does.

    So the string is segmented into the commands a shell would run, and each is asked the
    same question as an argv. What cannot be followed is refused rather than allowed, and
    each of these is a real way to reach charter without ever putting the word in a command
    position:

    * a command substitution — read as its own command string, one level down;
    * an assignment whose value names charter — ``cmd=charter; $cmd doctor``;
    * a command word containing ``$`` or a backtick — the indirection that assignment sets
      up, and anything else that computes the name;
    * a wrapper that takes a command as its arguments — ``sudo``, ``eval``, ``xargs``,
      ``timeout`` — with the word anywhere after it;
    * a string that will not lex, or nesting past the fourth level.
    """
    if not _CHARTER_WORD.search(command):
        return False
    if depth > 3:                         # nesting nobody writes; stop, and refuse
        return True
    for body in _substitution_bodies(command):
        if _shell_launches_charter(body, depth + 1):
            return True
    try:
        segments = _shell_segments(command)
    except ValueError:
        return True
    for segment in segments:
        words = _strip_launcher_prefix(segment)
        if any(_CHARTER_WORD.search(w) for w in segment[:len(segment) - len(words)]):
            return True                   # an assignment, or `env`'s own options, name it
        if not words:
            continue
        if "$" in words[0] or "`" in words[0]:
            return True                   # the command word is computed: undecidable
        if _cmd_launches_charter(words, depth + 1):
            return True
        if (os.path.basename(words[0]) in _COMMAND_WRAPPERS
                and any(_CHARTER_WORD.search(w) for w in words[1:])):
            return True
    return False


def _cmd_launches_charter(parts: list[str], depth: int = 0) -> bool:
    """Does this argv launch charter — by any spelling that would resolve a plane?"""
    parts = _strip_launcher_prefix(parts)
    if not parts:
        return False
    head = os.path.basename(parts[0])
    if head == "charter":
        return True
    rest = parts[1:]
    if _is_python(head) and _python_launches_charter(rest):
        return True
    if head in _SHELL_NAMES:
        for i, tok in enumerate(rest):
            if tok == "-c" and i + 1 < len(rest):
                return _shell_launches_charter(rest[i + 1], depth + 1)
            if tok.startswith("-c") and len(tok) > 2:
                return _shell_launches_charter(tok[2:], depth + 1)
    # ``-m charter`` by adjacency, wherever it appears and whatever argv[0] is called.
    # `_is_python` knows the interpreter names this machine has; this catches the one it
    # does not.
    return any(parts[i] == "-m" and _module_is_charter(parts[i + 1])
               for i in range(len(parts) - 1))


def _charter_argv(args, opts: dict) -> list[str] | None:
    """*args* as a list of strings when it launches charter itself, else ``None``.

    The question is not "is this argv one of the spellings charter's own code uses" — it is
    **"will this child resolve the operator's plane as charter"**. The difference was
    measured against the previous version of this function, which asked the first question:
    ``[python, "-m", "charter", "--version"]`` was refused, while ``[python, "-c", "from
    charter import config; print(config.ROOT)"]``, ``["/bin/sh", "-c", "<python> -m charter
    --version"]`` and the same command as a ``shell=True`` string all RAN, against the real
    plane. The ``-c`` shape was not hypothetical: nine charter-importing ``python -c``
    children per suite run were being waved through, each resolving the operator's own
    plane at import before calling `config.use` on the next statement — "a module-level
    charter import in a child that resolves its own plane" being exactly the shape that
    produced #527.

    So every spelling that gets there is recognised: a bare ``charter`` from ``PATH``,
    ``-m charter`` (attached, separate or bundled), ``-c`` source that names the module, a
    Python script file that imports it, ``executable=``, a shell ``-c`` string, and
    ``shell=True``. Where a spelling cannot be decided — source that will not tokenize, a
    script that will not read, any shell string — it is answered "charter" and the plane
    check decides. A false refusal in a test is loud, named and one line from the fix; a
    false allow writes to a live machine.
    """
    if args is None:
        return None
    if isinstance(args, os.PathLike):
        # `Popen` accepts one path-like as the whole command. It is not iterable, so the
        # sequence branch below would raise `TypeError` and answer "not charter" — which is
        # the shape of every hole this function has had.
        args = os.fspath(args)
    if isinstance(args, (str, bytes)):
        command = _decoded(args)
        if command is None:
            return None
        parts = [command]
    else:
        try:
            decoded = [_decoded(a) for a in args]
        except TypeError:
            return None
        if any(p is None for p in decoded):
            return None
        parts = decoded
    if not parts:
        return None

    executable = _decoded(opts.get("executable")) if opts.get("executable") else None
    if executable is not None and os.path.basename(executable) == "charter":
        return parts

    # ``shell=True`` hands args[0] to ``/bin/sh -c`` — as a string, or as the first element
    # of a sequence whose remaining elements become $0, $1... Either way it is a command
    # STRING, and the previous version answered it ``None``: it declined to parse the one
    # form that carries the whole command.
    if opts.get("shell"):
        return parts if _shell_launches_charter(parts[0]) else None
    if isinstance(args, (str, bytes)):
        # Without ``shell=True`` a string is one program name and no arguments.
        return parts if os.path.basename(parts[0]) == "charter" else None
    return parts if _cmd_launches_charter(parts) else None


def _explain_spawn(parts: list[str], plane) -> str:
    return (
        f"REFUSED: spawning charter against the real control plane\n"
        f"{_current_test()} is about to run `{' '.join(parts)}`, and that child would "
        f"resolve its plane as {plane} \u2014 the developer's REAL one. It is a separate "
        f"process: nothing this suite patches in memory reaches it, so it would refresh "
        f"that plane's forge state, rewrite its caches, and (for `persona _gc`) collect "
        f"against it. Two ways out. (1) If the test is not about the spawn, stub it \u2014 "
        f"`mock.patch.object(charter.update, \"maybe_spawn\", lambda: None)`, or patch "
        f"`subprocess.Popen` where the code under test looks it up. (2) If the test really "
        f"wants a child, hand it the throwaway plane as $CHARTER_ROOT: "
        f"`tests._isolation.child_plane_env(self)` returns exactly that environment, and "
        f"`PanelIntegration` in `test_frame_tmux_integration.py` does the same by hand. "
        f"charter's own spawners already do this for you (`util.child_env`), so a case "
        f"whose `config.ROOT` is isolated never gets here.")


def _child_plane(opts: dict):
    """The plane the child described by *opts* would resolve -- or ``None`` for none.

    `root.find_root` is asked the child's question directly, with the child's environment
    (``env=None`` means it inherits ours) and the child's cwd. That is what `find_root`'s
    ``env`` parameter is for: the alternative is a second copy of the walk living here,
    quietly disagreeing with the real one the day a worktree rule changes.

    When `find_root` raises, the child is not left with nothing: `find_root_or_cwd` -- what
    `charter.config` actually calls at import -- falls back to the child's own working
    directory, unwalked. So does this. A `charter init` in an empty temp directory takes
    exactly that path, and answering ``None`` there would be the guard waving through the
    one case the fallback makes dangerous: a bad ``$CHARTER_ROOT`` and a cwd of the
    checkout.

    ``env=None`` is answered with a ``dict`` COPY of `os.environ` rather than the mapping
    itself, and that is about the other tripwire in this package. `_envguard` refuses an
    undeclared TARGETED read of ``$CHARTER_ROOT`` and leaves bulk reads alone, so asking
    `find_root` to ``.get()`` it off the live mapping would raise `AmbientEnvRead` inside
    the guard — charging every test that spawns `bash` with a read it never made. A copy
    carries the same values (the ambient ones were scrubbed at install, so it is the same
    on every machine) and is exactly what the child will inherit.
    """
    from charter import root as _root

    env = opts.get("env")
    cwd = opts.get("cwd")
    try:
        start = Path(os.fsdecode(cwd)) if cwd is not None else Path.cwd()
    except (TypeError, ValueError, OSError):
        return None
    try:
        return _root.find_root(start, env=dict(os.environ) if env is None else env)
    except _root.ControlPlaneNotFound:
        try:
            return start.resolve()        # what `find_root_or_cwd` hands `config`
        except (OSError, RuntimeError):
            return None
    except Exception:
        return None


#: Whether :func:`_guard_spawns` has already wrapped ``Popen.__init__``. Separate from
#: :data:`_REAL` because `install` gives up early on a machine with no resolvable state
#: directory, and the spawn tripwire is armed before that point -- it keys off
#: :data:`_REAL_ROOT`, which `_guard_reads` fills in first.
_SPAWN_GUARDED = False


def _guard_spawns() -> None:
    """Refuse a charter child that would resolve the real plane. Idempotent.

    Wrapped on ``subprocess.Popen.__init__``, the CLASS, rather than on the
    ``subprocess.Popen`` module attribute. Two reasons, and both are holes the attribute
    version would leave open: `subprocess.run`, `check_output` and `check_call` construct
    the class directly, and so does any module that did ``from subprocess import Popen``.

    The other half of that choice is what makes it correct rather than merely thorough: a
    test that patches the module attribute -- `test_hooks_need_no_async` and
    `test_self_relaunch_argv` both do, around real `detach_self` calls -- never reaches the
    class at all, so it is never refused. That is the right answer, because nothing is
    spawned. This fires on spawns that really happen, which is the only kind that can touch
    a plane.

    **What it does NOT watch, stated rather than left to be discovered.** `os.execv*`,
    `os.posix_spawn*`, `os.spawn*` and `os.system` start a process without going anywhere
    near this class. They are not wrapped, and the reason is not that they are unreachable
    — it is that nothing reaches CHARTER through them, which is a checkable claim rather
    than an assumption: `NoCharterEscapesThroughTheExecFamily` in
    `test_plane_spawn_guard.py` parses every module in `charter/` and `tests/` and fails on
    a call to one of them that is not the two known non-charter uses. The day a charter
    spawn is written that way, that case turns red and this docstring is what it points at.
    """
    global _SPAWN_GUARDED
    if _SPAWN_GUARDED:
        return
    _SPAWN_GUARDED = True
    original = subprocess.Popen.__init__

    def __init__(self, args=None, *rest, **kw):
        opts = dict(zip(_POPEN_POSITIONAL, rest))
        opts.update(kw)
        parts = _charter_argv(args, opts)
        if parts is not None and _REAL_ROOT:
            plane = _child_plane(opts)
            if plane is not None:
                try:
                    here = os.path.abspath(str(plane))
                except (OSError, TypeError, ValueError):
                    here = None
                if here is not None and (here in _REAL_ROOT
                                         or os.path.realpath(here) in _REAL_ROOT):
                    raise RealPlaneSpawn(_explain_spawn(parts, plane))
        return original(self, args, *rest, **kw)

    __init__.__module__ = __name__
    subprocess.Popen.__init__ = __init__


def install() -> None:
    """Wrap every write primitive. Idempotent; called once at `tests` package import."""
    global _REAL
    if _REAL:
        return
    from charter import config
    _guard_reads(config)
    _guard_spawns()
    try:
        written = os.path.abspath(str(config.STATE_DIR))
        resolved = os.path.realpath(written)
    except (OSError, ValueError):         # no resolvable plane: nothing to protect
        return
    _REAL = (written,) if written == resolved else (written, resolved)

    # `os.mkdir` covers `Path.mkdir` AND `os.makedirs` (which calls the module global by
    # name, so it goes through this wrapper too) — one wrapper, both spellings.
    # `os.remove`/`os.unlink` are distinct objects, so both are wrapped.
    for name in ("mkdir", "rmdir", "remove", "unlink", "truncate", "chmod"):
        _guard_os(name)
    for name in ("rename", "replace"):
        _guard_os(name, both=True)
    for name in ("symlink", "link"):
        _guard_os(name, arg=1)

    # `shutil.rmtree` is wrapped at its FRONT DOOR rather than left to the primitives
    # above. On every platform that avoids symlink attacks it recurses with
    # `os.unlink(entry.name, dir_fd=topfd)` — a bare NAME plus a directory fd, which no
    # path-based check can resolve. `state.reap` deletes exactly this way, so a guard that
    # only watched the primitives would have watched the one call it exists to catch go
    # past. The path handed to `rmtree` itself is a real path, and refusing there is
    # earlier anyway: nothing is scanned, let alone removed.
    original_rmtree = shutil.rmtree

    def rmtree(path, *args, **kw):
        if _hits(path):
            raise RealPlaneWrite(_explain("shutil.rmtree", path))
        return original_rmtree(path, *args, **kw)

    shutil.rmtree = rmtree

    # `builtins.open` and `io.open` are the same function object but two separate module
    # attributes, looked up independently at call time: `Path.open` (and so `write_text`,
    # `write_bytes`) calls `io.open`, while charter's own code calls the builtin. Missing
    # either one leaves half the writes unwatched.
    original_open = builtins.open

    def open_(file, mode="r", *args, **kw):
        if any(c in mode for c in "wxa+") and _hits(file):
            raise RealPlaneWrite(_explain(f"open(..., {mode!r})", file))
        return original_open(file, mode, *args, **kw)

    builtins.open = open_
    io.open = open_

    # The low-level `os.open`, used by `tempfile` and by anything wanting O_EXCL. Only the
    # flags that can create or shorten a file are refused; O_RDONLY passes.
    original_os_open = os.open
    writing = os.O_WRONLY | os.O_RDWR | os.O_CREAT | os.O_APPEND | os.O_TRUNC

    def os_open(path, flags, *args, **kw):
        if flags & writing and _hits(path, kw.get("dir_fd")):
            raise RealPlaneWrite(_explain("os.open", path))
        return original_os_open(path, flags, *args, **kw)

    os.open = os_open
