#!/usr/bin/env python3
"""The deletion sweep, run by the repo instead of promised by an agent.

Three rounds of Phase 2 review found the same defect thirty-six times: a guard with no
test behind it. Correct code, shipped, that a later refactor could delete in silence.
Every one was found by the same move — delete the line, run the suite, see if it stays
green — and every one was found by a *human-driven* sweep that is mechanical, slow and
invisible in the diff, which is why it is the step that gets **reported** rather than
**run**. Nothing in the repository could tell the two apart. This can.

Four things make it affordable and make its answer worth trusting:

**Scoped to the diff.** A branch is answerable for the guards *it* adds, so the default
is `git diff` against the merge-base with ``origin/main``. ``--all`` charges the whole
tree instead — not as a gate, as a number.

**Mutated by statement shape, not by line.** Deleting arbitrary lines mostly produces
``SyntaxError`` and ``NameError``, which redden tests for reasons that have nothing to do
with the property under test. Those are false pins and they are worse than no signal.
Every operator in :data:`OPERATORS` is drawn from a guard one of the three rounds
actually found, and any mutation whose result does not parse is dropped.

**Selected by trace, not by guess.** The suite is ~6000 tests and four minutes; a full
run per mutation is the four and a half hours the last round really cost. Running the
suite once under `sys.settrace`, one test module at a time, says which source files each
module executes, and a mutation then runs only the modules that reach its file — seconds
instead of minutes. The map is cached against a hash of the tree it was measured on.

**Never trusted when it says "survived", and never allowed to guess when it says
"pinned".** Selection is an optimisation and must never be the final word: anything that
survives its subset is re-run against the FULL suite. A false survivor costs one full run;
a false *pin* is the exact bug this file exists to prevent, so the asymmetry decides the
design — a red is confirmed before it pins anything, and a run that TIMED OUT is neither
green nor red but **unresolved**, because no test failed there and machine load must not
become evidence. That last distinction was learned the hard way: on a box under a load
average of 100, two of #553's six known-unpinned guards came back "pinned" with `ran=0`.

There is deliberately **no suppression list.** The usual escape hatch — mark a mutant
"equivalent" and move on — is how this kind of gate becomes a rubber stamp, and charter
already refuses the analogous thing (#370: no config key lifts a guard denial). If
deleting a line genuinely changes nothing observable, the line should be deleted;
"equivalent mutant" and "dead code" are the same finding, and the sweep reporting it is
the sweep working.

And because most "equivalent mutant" claims turn out to be "the test asserts too little"
— measured on `release.yml`'s version check (#558), where deleting the refusal left the
run still exiting 1 for a *different reason* — a survivor is reported together with what
the covering tests actually assert about the mutated symbol. The reviewer's first
question should be "did my test look closely enough", not "can I suppress this".

Stdlib only. `pyproject.toml` says ``dependencies = []`` and that is load-bearing.

    python3 tools/sweep.py                      # this branch, against its merge-base
    python3 tools/sweep.py --ref 5b02b3f        # some other commit
    python3 tools/sweep.py --path tools         # sweep the sweep
    python3 tools/sweep.py --all                # the whole tree, as a number
"""
from __future__ import annotations

import argparse
import ast
import concurrent.futures
import dataclasses
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

#: Python files under these directories are swept unless ``--path`` says otherwise.
DEFAULT_PATHS = ("charter",)

#: What a subset run and a full run are allowed to take before they count as a failure.
#: The full suite is ~5 minutes; one round-3 mutation HUNG rather than passing, which is
#: a failure and not a green, so a timeout is reported as RED-by-timeout and never as a
#: survivor.
SUBSET_TIMEOUT = 900
FULL_TIMEOUT = 2400

#: `except ZeroDivisionError` is the narrowing the round-2 sweep used by hand: it is a
#: real builtin (so the clause still compiles and still evaluates), it derives from
#: `Exception` (so it is a *narrowing* of the common case and not a widening), and
#: nothing in charter divides by zero.
NARROW_TO = "ZeroDivisionError"

_RAN = re.compile(r"^Ran (\d+) tests? in ", re.MULTILINE)

#: Exception types whose *behaviour* is decided by the operating system, not by charter.
#: A `narrow-except` survivor on one of these may not be an unpinned guard at all: the
#: clause may simply be unreachable on the platform the sweep ran on. Measured on this
#: project — `except OSError: return None` around a pty read is dead code on macOS, where
#: closing the far end returns `b""`, and live on Linux, where it raises `[Errno 5]`. The
#: pin was proved by pushing the mutant to a throwaway branch and letting CI redden it.
PLATFORM_SENSITIVE = frozenset({
    "OSError", "IOError", "EnvironmentError", "BlockingIOError", "BrokenPipeError",
    "ConnectionResetError", "ConnectionAbortedError", "InterruptedError", "TimeoutError",
    "PermissionError", "FileNotFoundError", "NotADirectoryError", "IsADirectoryError",
})


def platform_caveat(mutation: "Mutation") -> str:
    """Why this survivor might be a platform artefact rather than a missing test."""
    if mutation.operator != "narrow-except":
        return ""
    named = {w for w in re.findall(r"[A-Za-z_]+", mutation.before)}
    hit = sorted(named & PLATFORM_SENSITIVE)
    return hit[0] if hit else ""


def resolved(path: Path | str) -> Path:
    """*path*, absolute and with every symlink taken out of it.

    One spelling, applied at every boundary, because a path this tool merely *joins* is
    harmless and a path it *compares* is not. The selection map is built by matching each
    traced `co_filename` against a prefix, and `co_filename` is written by the import
    system from whatever `sys.path` entry the module was found under — which, for the
    trace runner, is `os.getcwd()`. The kernel's answer to that never contains a symlink.
    So a prefix built from a path that still has one in it matches **nothing, ever**.

    Measured (#572): the default workdir is `$TMPDIR`, macOS spells that
    `/var/folders/…`, `/var` is a symlink to `/private/var`, and a sweep on macOS traced
    all 329 test modules, matched not one file, and refused. The guard held; ten minutes
    of tracing bought a hard stop and no diagnosis.

    A `resolve()` at one site and a raw path at another is the same bug in a different
    spelling, so this is the only normalisation there is: the repository root, the
    workdir, every sandbox and the root the map is measured against all come through
    here, and everything below them is repo-relative and joined rather than compared.
    """
    return Path(path).resolve()


# --------------------------------------------------------------------------------------
# 1. Scoping: which lines is this branch answerable for
# --------------------------------------------------------------------------------------

def git(*args: str, cwd: Path, check: bool = True) -> str:
    """`git` in *cwd*, as text. Raises on a non-zero exit unless *check* is false."""
    p = subprocess.run(("git",) + args, cwd=str(cwd), check=False,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if check and p.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed in {cwd}:\n{p.stderr}")
    return p.stdout


def _blob_at(repo: Path, ref: str, rel: str) -> bytes:
    """A file's exact bytes at *ref*. Bytes, because `col_offset` counts UTF-8 bytes."""
    p = subprocess.run(["git", "show", f"{ref}:{rel}"], cwd=str(repo), check=False,
                       stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    return p.stdout if p.returncode == 0 else b""


_HUNK = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@")


def added_lines(repo: Path, base: str, ref: str, paths: tuple[str, ...]
                ) -> dict[str, set[int]]:
    """``{path: {line numbers added or modified at *ref*}}``, relative to *repo*.

    ``--unified=0`` so a hunk's ``+`` range is exactly the added lines and not three
    lines of untouched context on either side, which would let a branch be charged for a
    guard it merely stood next to.
    """
    spec = [f"{p}/**.py" for p in paths] + [f"{p}/*.py" for p in paths]
    out = git("diff", "--unified=0", "--no-color", "--no-renames",
              f"{base}", f"{ref}", "--", *spec, cwd=repo)
    found: dict[str, set[int]] = {}
    current: str | None = None
    for line in out.splitlines():
        if line.startswith("+++ b/"):
            current = line[6:]
            if current == "/dev/null":
                current = None
            continue
        if line.startswith("+++ "):
            current = None
            continue
        m = _HUNK.match(line)
        if m and current and current.endswith(".py"):
            start, count = int(m.group(1)), int(m.group(2) or 1)
            if count:
                found.setdefault(current, set()).update(range(start, start + count))
    return found


def all_lines(root: Path, paths: tuple[str, ...]) -> dict[str, set[int]]:
    """Every line of every swept file — what ``--all`` charges instead of a diff."""
    found: dict[str, set[int]] = {}
    for p in paths:
        base = root / p
        if not base.exists():
            continue
        for f in sorted(base.rglob("*.py")):
            rel = f.relative_to(root).as_posix()
            n = len(f.read_bytes().splitlines())
            found[rel] = set(range(1, n + 2))
    return found


# --------------------------------------------------------------------------------------
# 2. Mutation: by statement shape, because lines are not the unit of meaning
# --------------------------------------------------------------------------------------

@dataclasses.dataclass(frozen=True)
class Mutation:
    """One edit, one question, one test run."""

    path: str           #: repo-relative source file
    line: int           #: the line the reviewer should look at
    end_line: int
    operator: str       #: which shape was recognised
    question: str       #: what a survivor would mean
    before: str         #: the source that was there
    after: str          #: the source that replaced it
    symbol: str         #: enclosing def/class, for the evidence pass
    source: bytes = b""  #: the whole mutated file
    span: tuple[int, int] = (0, 0)   #: byte span replaced, so two can be composed

    @property
    def tag(self) -> str:
        return f"{self.path}:{self.line}:{self.operator}"

    def __str__(self) -> str:
        return f"{self.path}:{self.line} [{self.operator}] {_oneline(self.before)}"


def _oneline(s: str, limit: int = 96) -> str:
    flat = " ".join(s.split())
    return flat if len(flat) <= limit else flat[: limit - 1] + "…"


class _Spans:
    """Byte offsets for AST nodes.

    `col_offset` is a UTF-8 *byte* offset, not a character index — a docstring with an
    arrow in it is enough to make the two disagree — so every splice here is done on
    bytes and decoded only to check that the result still parses.
    """

    def __init__(self, source: bytes) -> None:
        self.source = source
        self.starts = [0]
        for chunk in source.splitlines(keepends=True):
            self.starts.append(self.starts[-1] + len(chunk))

    def offset(self, lineno: int, col: int) -> int:
        return self.starts[lineno - 1] + col

    def span(self, node: ast.AST) -> tuple[int, int]:
        return (self.offset(node.lineno, node.col_offset),
                self.offset(node.end_lineno, node.end_col_offset))

    def text(self, node: ast.AST) -> str:
        a, b = self.span(node)
        return self.source[a:b].decode("utf-8")

    def splice(self, node: ast.AST, replacement: str) -> bytes:
        """Replace *node*'s span, padded so every later line keeps its number.

        Line numbers are the whole output of this tool. A replacement that collapsed
        three lines into one would renumber the rest of the file, and a survivor
        reported at the wrong line is a survivor nobody acts on.
        """
        a, b = self.span(node)
        rep = replacement.encode("utf-8")
        lost = self.source[a:b].count(b"\n") - rep.count(b"\n")
        return self.source[:a] + rep + b"\n" * max(0, lost) + self.source[b:]


def _sole_statement(parent_body: list[ast.stmt], node: ast.AST) -> bool:
    return len(parent_body) == 1 and parent_body[0] is node


def _parents(tree: ast.AST) -> dict[int, ast.AST]:
    out: dict[int, ast.AST] = {}
    for node in ast.walk(tree):
        for child in ast.iter_child_nodes(node):
            out[id(child)] = node
    return out


def _enclosing(tree: ast.AST, node: ast.AST) -> str:
    """The nearest ``def``/``class`` around *node* — the name the evidence pass looks for."""
    best, best_span = "<module>", None
    for outer in ast.walk(tree):
        if not isinstance(outer, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            continue
        if outer.lineno <= node.lineno and node.end_lineno <= (outer.end_lineno or 0):
            span = outer.end_lineno - outer.lineno
            if best_span is None or span < best_span:
                best, best_span = outer.name, span
    return best


_SIMPLE_BODY = (ast.Return, ast.Raise, ast.Continue, ast.Break, ast.Pass,
                ast.Assign, ast.AugAssign, ast.AnnAssign, ast.Expr)


def _drop_statement(sp: _Spans, node: ast.stmt, holder: list[ast.stmt]) -> str:
    """The replacement text for deleting *node*: nothing, or ``pass`` if it was alone."""
    return "pass" if _sole_statement(holder, node) else ""


def _body_of(parent: ast.AST, node: ast.AST) -> list[ast.stmt] | None:
    for field in ("body", "orelse", "finalbody"):
        seq = getattr(parent, field, None)
        if isinstance(seq, list) and any(s is node for s in seq):
            return seq
    return None


def _iter_operators(tree: ast.Module, sp: _Spans):
    """Yield ``(node, replacement, operator, question)`` for every recognised shape.

    Each row of this table is a guard one of the three review rounds actually deleted by
    hand. Nothing here is speculative and nothing here is a general-purpose mutation
    engine: a shape earns a place by having caught a real unpinned line.
    """
    parents = _parents(tree)

    # A module-level constant is a guard too, and the spec's shape table has no row for
    # one. Five of round two's eighteen overlay findings are exactly this — `_CHROME_ROWS`,
    # `_SPLIT_ROWS`, `_MIN_TITLE`, `MOUSE_ON`, `_MARK` — a number or a string with a
    # docstring making a specific claim for the value, and nothing measuring it. Only the
    # two forms with a principled general perturbation are mutated here: an integer, moved
    # by one, and a sum of named parts, dropped to each part. A string constant has no
    # honest general perturbation — picking `1003` over `1000` for `MOUSE_ON` is fitting
    # the answer key, not recognising a shape — so it stays a known gap and is reported
    # as one rather than faked.
    for stmt in tree.body:
        target = None
        if isinstance(stmt, ast.Assign) and len(stmt.targets) == 1 \
                and isinstance(stmt.targets[0], ast.Name):
            target = stmt.targets[0].id
        elif isinstance(stmt, ast.AnnAssign) and isinstance(stmt.target, ast.Name):
            target = stmt.target.id
        value = getattr(stmt, "value", None)
        if target is None or value is None or not target.lstrip("_").isupper():
            continue
        if isinstance(value, ast.Constant) and isinstance(value.value, int) \
                and not isinstance(value.value, bool):
            yield (value, str(value.value + 1), "retune-constant",
                   f"is `{target}`'s value pinned, or would any number do?")
        if isinstance(value, ast.BinOp) and isinstance(value.op, ast.Add):
            for side in (value.left, value.right):
                yield (value, sp.text(side), "drop-term",
                       f"is every term of `{target}` pinned?")

    for node in ast.walk(tree):

        # `if C: return` / `raise` / `continue` — and, by the same move, `if C:` with any
        # single simple statement under it. Round three's two sharpest findings were
        # `if self._sel < top: top = self._sel` and `if size is None: size = SLOT_SIZE[slot]`,
        # both assignments; restricting the shape to `return`/`raise`/`continue` would
        # have written both of them off before they were asked about.
        if isinstance(node, ast.If) and not node.orelse and len(node.body) == 1 \
                and isinstance(node.body[0], _SIMPLE_BODY):
            holder = _body_of(parents.get(id(node), tree), node) or []
            yield (node, _drop_statement(sp, node, holder), "drop-if",
                   "is the refusal pinned?")

        # A branch in an `if`/`elif`/`else` chain cannot be excised — deleting it would
        # take the `else` with it — so it is disabled instead, which asks the same
        # question. Round two's finding 14, `decode`'s `elif ch == b"\x03"` Ctrl-C
        # branch, is this shape and only this shape. Restricted to chains that HAVE an
        # `else`, because in a bare `if` the fall-through is a `NameError` waiting to
        # happen and a mutation that reddens the suite for that reason is a false pin —
        # the one outcome worse than no signal.
        if isinstance(node, ast.If) and node.orelse:
            yield (node.test, "False", "disable-branch", "is this branch pinned?")
            yield (node.test, "True", "disable-branch", "is the other branch pinned?")

        # The same refusal in expression clothes: `[x for x in xs if C]`. Round two's
        # first finding, `harness_rows`' `if _edge_of(slot) not in _COLUMN_EDGES`, lives
        # inside a `sum(...)` generator, and a shape table that only knows the statement
        # spelling of `if` cannot see it at all.
        if isinstance(node, ast.comprehension) and node.ifs:
            for cond in node.ifs:
                # `True` rather than excising the `if` keyword: the filter is gone either
                # way, and this keeps the splice inside one expression's span, so every
                # line below it keeps its number.
                yield (cond, "True", "drop-comprehension-if",
                       "is the filter pinned?")

        # `if A and B:` — one conjunct at a time. Round three found `_placed_here`'s
        # `isinstance(name, str) and name not in SLOT_SIZE` unpinned in BOTH halves, and
        # a whole-condition mutation cannot tell those two findings apart.
        if isinstance(node, (ast.If, ast.While)) and isinstance(node.test, ast.BoolOp) \
                and isinstance(node.test.op, ast.And) and len(node.test.values) > 1:
            for i, part in enumerate(node.test.values):
                rest = [v for j, v in enumerate(node.test.values) if j != i]
                kept = " and ".join(sp.text(v) for v in rest)
                yield (node.test, kept, "drop-conjunct",
                       f"is the `{_oneline(sp.text(part), 48)}` half pinned?")

        # `if isinstance(x, str)` as the whole condition — the type filter, dropped.
        if isinstance(node, (ast.If, ast.While)) and _is_isinstance(node.test):
            yield (node.test, "True", "drop-isinstance",
                   "is the type filter pinned?")
        if isinstance(node, (ast.If, ast.While)) and isinstance(node.test, ast.UnaryOp) \
                and isinstance(node.test.op, ast.Not) and _is_isinstance(node.test.operand):
            yield (node.test, "False", "drop-isinstance",
                   "is the type filter pinned?")

        # `x = max(a, b)` / `min(…)` — dropped to each operand in turn. `_window`'s
        # `n = max(1, height - _CHROME_ROWS)` and its `_top` clamp are both this shape.
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) \
                and node.func.id in ("max", "min") and len(node.args) == 2 \
                and not node.keywords:
            for arg in node.args:
                yield (node, sp.text(arg), "unclamp",
                       "is the clamp pinned, or only its inner value?")

        # `except E:` — narrowed to something nothing raises. `_component_text`'s
        # `except Exception` was round two's finding 4.
        if isinstance(node, ast.ExceptHandler) and node.type is not None:
            if sp.text(node.type).strip() != NARROW_TO:
                yield (node.type, NARROW_TO, "narrow-except",
                       "is the catch pinned, or does nothing ever fail here?")

        # `f(contain.one_line(x))` -> `f(x)`. Four of round two's twelve and one of round
        # three's six are a containment call on a value that reaches a terminal.
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) \
                and isinstance(node.func.value, ast.Name) \
                and node.func.value.id == "contain" and node.args:
            yield (node, sp.text(node.args[0]), "uncontain",
                   "is the containment pinned?")

        # `d.get(k) or ()` -> `d[k]`, and `d.get(k, v)` -> `d[k]`. `_placed_here`'s
        # `config.FRAME.get("components") or ()` and `_derive`'s `SLOT_OF.get(c.id, c.id)`
        # are the two spellings, and both were found unpinned.
        if isinstance(node, ast.BoolOp) and isinstance(node.op, ast.Or) \
                and len(node.values) == 2 and _is_get(node.values[0]):
            call = node.values[0]
            yield (node, f"{sp.text(call.func.value)}[{sp.text(call.args[0])}]",
                   "no-fallback", "is the fallback pinned?")
        if _is_get(node) and len(node.args) == 2:
            yield (node, f"{sp.text(node.func.value)}[{sp.text(node.args[0])}]",
                   "no-fallback", "is the fallback pinned?")

        # `A or B` where B is an empty literal — `placed or []`, `paint or _paint`.
        if isinstance(node, ast.BoolOp) and isinstance(node.op, ast.Or) \
                and len(node.values) == 2 and not _is_get(node.values[0]) \
                and _is_empty_literal(node.values[1]):
            yield (node, sp.text(node.values[0]), "no-fallback",
                   "is the fallback pinned?")

        # `a if C else b` — collapsed to each side. `_policy_cells`' `else 1` was round
        # three's fifth finding and is exactly this.
        if isinstance(node, ast.IfExp):
            yield (node, sp.text(node.body), "collapse-ifexp",
                   "is the other branch pinned?")
            yield (node, sp.text(node.orelse), "collapse-ifexp",
                   "is this branch pinned?")


def span_is_sound(sp: _Spans, node: ast.AST) -> bool:
    """Does the source at *node*'s span actually parse back into *node*?

    Before 3.12 and PEP 701, `ast` gave only approximate positions for expressions
    **inside an f-string** — and `contain.one_line(…)` lives inside an f-string almost
    everywhere it appears in this tree, which is exactly what the `uncontain` operator
    exists to mutate. Splicing on a position that is off by a few bytes produces a mutant
    that is not the mutation the report claims, and it can still parse, so the parse check
    alone would not catch it.

    Re-parsing the span and comparing the tree (`ast.dump` omits positions) is the cheap
    way to be certain the tool is editing what it says it is. A node whose span does not
    round-trip is dropped rather than guessed at: a mutation reported at a line it did not
    change is worse than one not offered.
    """
    text = sp.text(node)
    try:
        if isinstance(node, ast.stmt):
            reparsed = ast.parse(text).body
            return len(reparsed) == 1 and ast.dump(reparsed[0]) == ast.dump(node)
        # Parenthesised, because an expression's span stops inside the brackets that
        # made it legal to break over two lines: `a if c\n else b` is a SyntaxError on
        # its own and a perfectly good mutation in place. Parentheses leave no trace in
        # the tree, so the comparison is still exact.
        return ast.dump(ast.parse(f"({text}\n)", mode="eval").body) == ast.dump(node)
    except (SyntaxError, ValueError, MemoryError, RecursionError):
        return False


def _is_isinstance(node: ast.AST) -> bool:
    return (isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
            and node.func.id == "isinstance")


def _is_get(node: ast.AST) -> bool:
    return (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
            and node.func.attr == "get" and len(node.args) in (1, 2)
            and not node.keywords)


def _is_empty_literal(node: ast.AST) -> bool:
    if isinstance(node, (ast.List, ast.Tuple, ast.Set)):
        return not node.elts
    if isinstance(node, ast.Dict):
        return not node.keys
    return isinstance(node, ast.Constant) and node.value in ("", 0, None)


def mutations_for(path: str, source: bytes, lines: set[int]) -> list[Mutation]:
    """Every mutation this file offers on the lines the branch is answerable for."""
    try:
        tree = ast.parse(source, filename=path)
    except SyntaxError:
        return []
    sp = _Spans(source)
    out: list[Mutation] = []
    seen: set[tuple[int, int, str, str]] = set()
    for node, replacement, operator, question in _iter_operators(tree, sp):
        if not (set(range(node.lineno, (node.end_lineno or node.lineno) + 1)) & lines):
            continue
        if not span_is_sound(sp, node):
            continue
        before = sp.text(node)
        if before.strip() == replacement.strip():
            continue
        key = (node.lineno, node.col_offset, operator, replacement)
        if key in seen:
            continue
        seen.add(key)
        mutated = sp.splice(node, replacement)
        try:
            ast.parse(mutated, filename=path)
        except SyntaxError:
            # Deleting the sole statement of a block leaves an empty block. The spec
            # says to skip a mutation that does not parse; `pass` is the same deletion
            # spelled so that it does.
            if replacement == "":
                mutated = sp.splice(node, "pass")
                try:
                    ast.parse(mutated, filename=path)
                except SyntaxError:
                    continue
                replacement = "pass"
            else:
                continue
        out.append(Mutation(
            path=path, line=node.lineno, end_line=node.end_lineno or node.lineno,
            operator=operator, question=question, before=before, after=replacement,
            symbol=_enclosing(tree, node), source=mutated, span=sp.span(node)))
    out.sort(key=lambda m: (m.path, m.line, m.operator, m.after))
    return out


def compose(pristine: bytes, pair: tuple[Mutation, Mutation]) -> bytes | None:
    """Both mutations at once, or ``None`` when their spans overlap or it will not parse.

    Two guards in sequence can mask each other. `_placed_here`'s `name not in SLOT_SIZE`
    is the worked example: deleting it alone changes nothing any caller can observe,
    because `_edge_of`/`_size_of` consult the shipped tables first — so a reviewer looking
    at the single mutation is invited to write it off as an equivalent mutant, when in
    fact its consequence is hidden behind a *second* unpinned line. Composing the pair is
    how the harness says "not equivalent — untested, at both orders".
    """
    a, b = sorted(pair, key=lambda m: m.span[0], reverse=True)
    if b.span[1] > a.span[0]:
        return None
    out = pristine
    for m in (a, b):
        start, end = m.span
        rep = m.after.encode("utf-8")
        lost = out[start:end].count(b"\n") - rep.count(b"\n")
        out = out[:start] + rep + b"\n" * max(0, lost) + out[end:]
    try:
        ast.parse(out)
    except SyntaxError:
        return None
    return out


# --------------------------------------------------------------------------------------
# 3. Selection: which test modules reach which source file
# --------------------------------------------------------------------------------------

#: Run inside a subprocess, one test module per process, with the tracer armed BEFORE the
#: module is imported so that what a module executes at import time counts as reaching a
#: file. Only `call` events are asked for — the tracer returns None, so no line or return
#: events are generated for the frame, which is the difference between a map that takes
#: minutes and one that takes an hour.
_TRACE_RUNNER = r'''
import json, os, sys, threading, traceback, unittest

module, out_path, prefix = sys.argv[1], sys.argv[2], sys.argv[3]
# The runner lives in the cache directory, so Python puts THAT on `sys.path` and not the
# tree under test. Without this the import of every `tests.*` module fails, the tracer
# sees nothing, and the map comes back empty — which still produces correct answers, by
# sending every mutation to the full suite, and takes a hundred times as long to do it.
#
# This line also decides how every `co_filename` below is SPELLED. The import system
# copies the `sys.path` entry a module was found under into its code objects, and
# `os.getcwd()` is the kernel's answer, which has no symlinks left in it. `prefix` is
# matched against those names raw — resolving one per call event would put a syscall on
# the hottest path in this tool — so the caller resolves it once instead (`resolved`),
# and #572 is what happens when it does not.
sys.path.insert(0, os.getcwd())
seen = set()
error = ""

def tracer(frame, event, arg):
    code = frame.f_code
    # `<module>` frames are import-time execution, and importing anything from `charter`
    # pulls in most of the package. Counted, they map every source file to every test
    # module and the selection map answers "run everything" for everything — measured:
    # `layout.py` mapped to all 322. What matters is which files a test module RUNS.
    if code.co_name != "<module>" and code.co_filename.startswith(prefix):
        # File AND function. File alone is far too coarse in this tree: `layout`,
        # `slots` and `builtins` are reached by almost every test module through some
        # shared fixture, so a file-keyed map answers "run all 322" for a mutation inside
        # one private helper nothing but four modules ever call.
        seen.add(code.co_filename)
        seen.add(code.co_filename + "::" + code.co_name)
    return None

try:
    # Armed AFTER the import, for the same reason: a module body that calls its own
    # helpers at import — `layout` derives the built-in geometry there — would otherwise
    # register itself for every test module that imports it, however little it uses.
    suite = unittest.defaultTestLoader.loadTestsFromName(module)
except Exception:
    error = traceback.format_exc(limit=3)
    suite = None

if suite is not None:
    sys.settrace(tracer)
    threading.settrace(tracer)
    try:
        with open(os.devnull, "w") as null:
            unittest.TextTestRunner(stream=null, verbosity=0).run(suite)
    except Exception:
        error = traceback.format_exc(limit=3)
    finally:
        sys.settrace(None)
        threading.settrace(None)

with open(out_path, "w") as fh:
    # `cwd` is reported because it is the ground truth for the spelling above — the
    # directory the tracer's filenames are actually written against. The one moment
    # anybody needs it is the moment the map refuses itself.
    json.dump({"files": sorted(seen), "error": error, "cwd": os.getcwd()}, fh)
'''


def tree_hash(root: Path, paths: tuple[str, ...]) -> str:
    """A hash of everything the map depends on: the swept sources and every test."""
    h = hashlib.sha256()
    for top in sorted(set(paths) | {"tests"}):
        base = root / top
        if not base.exists():
            continue
        for f in sorted(base.rglob("*.py")):
            h.update(f.relative_to(root).as_posix().encode())
            h.update(hashlib.sha256(f.read_bytes()).digest())
    return h.hexdigest()[:16]


def test_modules(root: Path) -> list[str]:
    return sorted(f"tests.{p.stem}" for p in (root / "tests").glob("test_*.py"))


def build_map(root: Path, paths: tuple[str, ...], jobs: int, scratch: Path,
              log=print) -> dict[str, list[str]]:
    """``{source file: [test modules that execute it]}``, measured once."""
    # Before anything is compared against it. `prefix` below is matched against filenames
    # the interpreter spelled for itself, and it is the caller's spelling of this path
    # that decides whether the two can ever meet — see :func:`resolved`.
    root = resolved(root)
    modules = test_modules(root)
    scratch.mkdir(parents=True, exist_ok=True)
    runner = scratch / "_trace_runner.py"
    runner.write_text(_TRACE_RUNNER)
    prefix = str(root) + os.sep
    hits: dict[str, list[str]] = {}
    #: Where the runners say they ran, as they measured it rather than as this side
    #: assumed it. Only ever read when the map has already refused itself.
    ran_in: set[str] = set()
    done = 0

    def one(module: str) -> tuple[str, list[str], str, str]:
        out = scratch / f"{module}.json"
        subprocess.run([sys.executable, str(runner), module, str(out), prefix],
                       cwd=str(root), check=False, timeout=SUBSET_TIMEOUT,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            payload = json.loads(out.read_text())
        except (OSError, ValueError):
            return module, [], "the trace runner wrote nothing", ""
        return module, payload["files"], payload["error"], payload["cwd"]

    broken: list[str] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        for module, files, error, cwd in pool.map(one, modules):
            done += 1
            if done % 40 == 0:
                log(f"    traced {done}/{len(modules)} modules")
            if error:
                broken.append(f"{module}: {error.strip().splitlines()[-1]}")
            ran_in.add(cwd)
            for f in files:
                name, _, symbol = f.partition("::")
                # Lexical, and it cannot fail: the runner only reported names that start
                # with `prefix`, which is this `root` and a separator. Resolving either
                # side again here is the move that made the bug survivable in the first
                # place — it made the KEYS come out right while the prefix that chose
                # them was wrong, so the map looked correct in every way except empty.
                rel = Path(name).relative_to(root).as_posix()
                hits.setdefault(f"{rel}::{symbol}" if symbol else rel, []).append(module)

    # An empty or near-empty map is a broken map, not a tree with no coverage, and it is
    # the failure mode that hides: every mutation still gets the right verdict, because
    # a file with no covering module goes to the full suite — it just takes a hundred
    # times as long and looks like the tool working.
    if len(hits) < 2 or len(broken) > len(modules) // 4:
        log(f"  ! the trace produced {len(hits)} file(s) and {len(broken)} broken "
            f"module(s). The map is not usable:")
        # Which of the two it is decides what to do about it, and the counts alone leave
        # the operator nowhere. `0 broken` is the tell: every runner loaded its suite,
        # ran it, reported no error, and the tracer still matched nothing — that is a
        # prefix that cannot match, not a tree without coverage. It is what #572 looked
        # like from the outside, and comparing the two spellings below IS the diagnosis.
        if broken:
            for line in broken[:3]:
                log(f"  !   {line}")
        else:
            log("  !   every module loaded and ran, so the tracer matched none of what")
            log("  !   they executed. These two are one directory or they are nothing:")
            log(f"  !     matched against : {prefix}")
            log(f"  !     the runners ran : {'; '.join(sorted(ran_in))}")
        raise RuntimeError("the selection map is empty — refusing to sweep blind")
    if broken:
        log(f"  selection map: {len(broken)} module(s) would not load; "
            f"their files fall back to the full suite")
    return {k: sorted(v) for k, v in sorted(hits.items())}


def load_map(root: Path, paths: tuple[str, ...], cache_dir: Path, jobs: int,
             refresh: bool, log=print) -> dict[str, list[str]]:
    """The map for this tree, from cache when the tree has not moved."""
    key = tree_hash(root, paths)
    cache_dir.mkdir(parents=True, exist_ok=True)
    cached = cache_dir / f"selection-{key}.json"
    if cached.exists() and not refresh:
        log(f"  selection map: cached ({cached.name})")
        return json.loads(cached.read_text())
    log(f"  selection map: tracing {len(test_modules(root))} test modules "
        f"({jobs} at a time)…")
    started = time.time()
    result = build_map(root, paths, jobs, cache_dir / f"trace-{key}", log=log)
    cached.write_text(json.dumps(result, indent=0, sort_keys=True))
    shutil.rmtree(cache_dir / f"trace-{key}", ignore_errors=True)
    log(f"  selection map: {len(result)} source files in {time.time() - started:.0f}s")
    return result


# --------------------------------------------------------------------------------------
# 4. Running: sandboxes, subsets, and the full suite that has the last word
# --------------------------------------------------------------------------------------

@dataclasses.dataclass
class Outcome:
    """What one run said — and whether it said anything at all.

    `conclusive` is the distinction this tool got wrong once and had to be taught. A run
    that TIMED OUT is not a red run: no test failed, the suite simply could not be
    measured. Folding the two together turns machine load into evidence, and the evidence
    it manufactures is "pinned" — the verdict that certifies a guard as tested and is
    never revisited. Measured, on a box under a load average of 100 with two other agents
    on it: two of #553's six known-unpinned guards came back "pinned" with `ran=0`, which
    is not a failure, it is a stopwatch.
    """

    green: bool
    ran: int
    detail: str
    conclusive: bool = True
    #: The ids of the tests that failed. The verdict is read from the DIFFERENCE between
    #: this and what the same command failed on unmutated — never from the exit code.
    failing: frozenset = dataclasses.field(default_factory=frozenset)


@dataclasses.dataclass
class _Finished:
    """What :func:`_verdict` needs from a run, whoever ran it."""

    returncode: int
    stdout: str
    stderr: str = ""


def _kill_group(proc: subprocess.Popen) -> None:
    """SIGKILL the whole process group a wedged run belongs to."""
    try:
        os.killpg(os.getpgid(proc.pid), 9)
    except (ProcessLookupError, PermissionError):       # pragma: no cover - it is gone
        proc.kill()


#: `FAIL: test_x (tests.test_mod.Class.test_x)` / `ERROR: ...`, and for an import failure
#: `ERROR: tests.test_mod (unittest.loader._FailedTest.tests.test_mod)`. The parenthesised
#: id is the stable name; a subtest's trailing `[value]` sits outside it.
_FAIL_ID = re.compile(r"^(?:FAIL|ERROR):\s+\S+\s+\(([^)]+)\)", re.MULTILINE)


def _verdict(proc) -> Outcome:
    """What a run said, as the SET OF TESTS THAT FAILED — never as an exit code.

    An exit code cannot tell "died because I deleted the guard" from "died for a reason
    that has nothing to do with it", and this project has now measured that confusion in
    both directions. `release.yml`'s `-z "$claimed"` refusal (#558) exits 1 with the line
    deleted *and* without it, for two different reasons, so a real deletion looked pinned.
    And a sweep run in a tree copied without `.git` errored twelve `test_workflows` cases
    in the baseline and in every mutant alike — every mutation came back rc=1 and every one
    would have scored "pinned".

    So the ids are collected here and the comparison is made in :func:`decide` against what
    the SAME command failed on before any mutation. A mutation is only credited with a red
    it actually caused.
    """
    text = (proc.stdout or "") + (proc.stderr or "")
    m = _RAN.search(text)
    ran = int(m.group(1)) if m else 0
    failing = frozenset(mm.group(1) for mm in _FAIL_ID.finditer(text))
    if ran == 0:
        # No `Ran N tests` line at all: the runner did not get far enough to answer.
        return Outcome(False, 0, "no tests ran", conclusive=False, failing=failing)
    if proc.returncode == 0:
        return Outcome(True, ran, "OK", failing=failing)
    return Outcome(False, ran, _named(failing) or f"rc={proc.returncode}", failing=failing)


def _named(ids) -> str:
    """A handful of failing test ids, shortest-name-first, for a one-line report."""
    short = sorted(ids, key=len)
    return "; ".join(short[:3]) + (f" (+{len(short) - 3} more)" if len(short) > 3 else "")


class Sandbox:
    """A private clone of the tree. The operator's checkout is never written to."""

    def __init__(self, root: Path, where: Path, ref: str, dirty: dict[str, bytes]):
        # Resolved on the way in, once. Everything downstream hangs off this path — the
        # `cwd` the trace runners are given, the prefix the tracer matches against, the
        # keys of the selection map — and a symlink surviving into any one of them is
        # #572 again, in a different spelling.
        self.path = where = resolved(where)
        if where.exists():
            shutil.rmtree(where)
        where.parent.mkdir(parents=True, exist_ok=True)
        # A real clone and not a `git archive`: `test_workflows` and `test_plugin_freshness`
        # read the repository, and a tree with no `.git` costs a dozen errors that have
        # nothing to do with any mutation. `--no-hardlinks` so a sandbox cannot reach back
        # into the objects of the checkout it came from.
        subprocess.run(["git", "clone", "--quiet", "--no-hardlinks", "--no-checkout",
                        str(root), str(where)], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        subprocess.run(["git", "checkout", "--quiet", "--detach", ref],
                       cwd=str(where), check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        for rel, blob in dirty.items():
            target = where / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(blob)
        self._pristine: dict[str, bytes] = {}
        #: What each module-set fails on with NOTHING mutated, measured once per sandbox.
        #: The full-tree baseline in `main` cannot answer this: a subset is a handful of
        #: modules run alone, and a module that only passes with its neighbours would
        #: redden every mutation mapped to it — a whole file of guards silently certified.
        self._clean_failures: dict[tuple[str, ...], frozenset] = {}

    def apply(self, mutation: Mutation) -> None:
        self.apply_source(mutation.path, mutation.source)

    def apply_source(self, rel: str, blob: bytes) -> None:
        """Write one file, remembering what was there so :meth:`restore` can undo it."""
        target = self.path / rel
        self._pristine.setdefault(rel, target.read_bytes())
        target.write_bytes(blob)

    def restore(self) -> None:
        for rel, blob in self._pristine.items():
            (self.path / rel).write_bytes(blob)
        self._pristine.clear()

    def run(self, argv: list[str], timeout: float) -> Outcome:
        # NO `__pycache__`, and this line is load-bearing. CPython decides a cached `.pyc`
        # is still valid by comparing the source's size and its mtime **truncated to whole
        # seconds** — nothing else. A sandbox applies one mutation after another to the
        # same file, and two mutations of one file routinely differ from the original by
        # the SAME number of bytes: `contain.one_line(x)` -> `x` removes exactly 18
        # characters wherever it appears, so `panel.py`'s mutation at line 210 and its
        # mutation at line 458 are both 29159 bytes against a 29177-byte original. Apply
        # them a fraction of a second apart — which a two-second subset run does — and the
        # second one is byte-for-byte indistinguishable from the first as far as the
        # validator is concerned. It reuses the first one's bytecode.
        #
        # Measured, with the mtimes pinned equal to make it certain: mutating line 458 ran
        # line 210's mutant, the test for line 210 went red, and line 458 — a real
        # survivor — was reported PINNED. A guard certified as tested by a stale cache.
        # With this variable set, the same pair answers RED then OK, correctly.
        env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
        # Its own process group, so a timeout can take the whole tree of children with
        # it. `subprocess.run(timeout=…)` kills only the direct child, and this suite
        # starts real tmux servers — a wedged run that left one behind would be inherited
        # by the next mutation and reported as ITS failure.
        proc = subprocess.Popen([sys.executable, "-m", "unittest", *argv],
                                cwd=str(self.path), text=True, env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                start_new_session=True)
        try:
            out, _ = proc.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            # Not green — round three had a mutation that wedged the suite for 1800s
            # instead of failing, and reading that as a pass would pin a guard on a
            # stopwatch. But not RED either, and that is the harder half: no test failed
            # here, so this run is `conclusive=False` and is not allowed to decide
            # anything on its own.
            _kill_group(proc)
            proc.communicate()
            return Outcome(False, 0, f"timed out after {timeout}s", conclusive=False)
        return _verdict(_Finished(proc.returncode, out))

    #: Set from the baseline's measured wall time, so the cap tracks the machine the
    #: sweep is actually running on rather than the one it was written on. A fixed
    #: forty-minute cap is generous on an idle box and far too tight on a shared one:
    #: measured, at a load average of 100, a five-minute suite ran past 2400 s and two
    #: known-unpinned guards came back "pinned" on the strength of a stopwatch.
    full_timeout: float = FULL_TIMEOUT

    def subset(self, modules: list[str]) -> Outcome:
        return self.run(list(modules), SUBSET_TIMEOUT)

    def clean_failures(self, modules: list[str]) -> frozenset:
        """What this module-set fails on unmutated. Call BEFORE applying a mutation."""
        key = tuple(modules)
        if key not in self._clean_failures:
            self._clean_failures[key] = self.subset(modules).failing
        return self._clean_failures[key]

    def full(self) -> Outcome:
        return self.run(["discover", "-s", "tests", "-t", "."], self.full_timeout)


def dirty_files(root: Path, paths: tuple[str, ...]) -> dict[str, bytes]:
    """Uncommitted work under the swept paths, so the tool is usable before a commit."""
    out: dict[str, bytes] = {}
    listing = git("status", "--porcelain", "--untracked-files=all", "--", *paths,
                  cwd=root, check=False)
    for line in listing.splitlines():
        rel = line[3:].strip()
        if rel.endswith(".py") and (root / rel).exists():
            out[rel] = (root / rel).read_bytes()
    return out


# --------------------------------------------------------------------------------------
# 5. Evidence: what the covering tests actually assert
# --------------------------------------------------------------------------------------

@dataclasses.dataclass
class Evidence:
    modules: list[str]
    naming: list[tuple[str, str, list[str]]]   # (module, test name, assert lines)

    @property
    def nothing_names_it(self) -> bool:
        return not self.naming


def evidence_for(root: Path, mutation: Mutation, modules: list[str]) -> Evidence:
    """Which covering tests name the mutated symbol, and what they assert about it.

    Deleting `release.yml`'s `-z "$claimed"` refusal (#558) left the run still exiting 1,
    because the check below it caught the empty string instead — same exit code, a
    different reason. That is why most "equivalent mutant" claims are wrong: the mutant
    is not equivalent, the test asserts too little. So a survivor is reported next to the
    assertions that were supposed to hold it, and a symbol no covering test even names is
    the loudest form of that answer.
    """
    naming: list[tuple[str, str, list[str]]] = []
    if mutation.symbol == "<module>":
        return Evidence(modules, naming)
    # Not `\b`: charter's private helpers all start with an underscore, which IS a word
    # character, so `\b_window\b` would refuse to match `surface._window(8)` at its
    # left edge and the evidence pass would report "nothing names it" for every private
    # symbol in the tree — which is most of them.
    word = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(mutation.symbol)
                      + r"(?![A-Za-z0-9_])")
    for module in modules:
        f = root / (module.replace(".", "/") + ".py")
        if not f.exists():
            continue
        src = f.read_bytes()
        if not word.search(src.decode("utf-8", "replace")):
            continue
        try:
            tree = ast.parse(src, filename=str(f))
        except SyntaxError:
            continue
        sp = _Spans(src)
        for node in ast.walk(tree):
            if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                continue
            if not node.name.startswith("test"):
                continue
            body = sp.text(node)
            if not word.search(body):
                continue
            asserts = [_oneline(sp.text(c), 110) for c in ast.walk(node)
                       if isinstance(c, ast.Call) and isinstance(c.func, ast.Attribute)
                       and c.func.attr.startswith("assert")]
            naming.append((module, node.name, asserts[:4]))
    return Evidence(modules, naming)


# --------------------------------------------------------------------------------------
# 6. The sweep
# --------------------------------------------------------------------------------------

@dataclasses.dataclass
class Result:
    mutation: Mutation
    verdict: str                 # "survived" | "pinned" | "unselectable"
    subset: Outcome | None
    full: Outcome | None
    modules: list[str]
    evidence: Evidence | None = None


def select_for(selection: dict[str, list[str]], mutation: Mutation) -> list[str]:
    """Which test modules to run for one mutation, narrowest first.

    Three levels, and each fallback is deliberately toward running MORE:

    1. the modules measured executing the mutated function — the usual case, and the
       one that turns a four-minute suite into a two-second one;
    2. the modules measured executing the mutated *file*, when the function was never
       seen under its own name (a `<lambda>`, a 3.11 comprehension frame, a constant at
       module scope);
    3. nothing at all, which :func:`decide` reads as "go straight to the full suite".

    Never the other way. A narrower answer than the truth is how a guard gets certified
    as tested by a subset that never ran it.
    """
    at_symbol = selection.get(f"{mutation.path}::{mutation.symbol}")
    if at_symbol:
        return at_symbol
    return selection.get(mutation.path, [])


def decide(box, mutation: Mutation, modules: list[str]) -> tuple[str, Outcome, Outcome | None]:
    """One mutation's verdict. The FULL suite has the last word, always.

    Three rules, and all three exist because the two errors are not symmetrical. A false
    survivor costs one full run and a reviewer's minute. A false *pin* is a guard the
    repository has quietly certified as tested when it is not — the exact defect this
    file was written to stop — so nothing may be called pinned on partial evidence:

    * a subset that goes green is never the answer; the whole suite is re-run.
    * a file no traced module was measured executing is not pinned by that silence.
      Absence of evidence goes to the full suite too.
    * a run that HANGS has not passed. Round three had one mutation that wedged the
      suite for 1800s instead of failing, and reading that as green would have pinned a
      guard on a timeout.
    """
    # Measured BEFORE the mutation goes anywhere near the tree.
    clean = box.clean_failures(modules) if modules else frozenset()
    box.apply(mutation)
    if modules:
        subset = box.subset(modules)
        if not subset.conclusive:
            subset = box.subset(modules)
            if not subset.conclusive:
                return "unresolved", subset, None
        caused = subset.failing - clean
        if not caused:
            # Either nothing failed, or the only failures are ones this module-set was
            # already failing on. Neither is evidence about the guard.
            subset = Outcome(True, subset.ran, "OK" if subset.green
                             else f"red, but on nothing new ({_named(subset.failing)})",
                             failing=subset.failing)
        else:
            # A red is the one verdict this tool never revisits, so it had better be a
            # real one. The suite starts real tmux servers, several sweeps share a
            # machine, and a flaky red here does not merely mislabel one mutation — it
            # certifies a guard as tested by a failure that had nothing to do with it.
            again = box.subset(modules)
            if not (again.failing - clean):
                subset = Outcome(True, again.ran, "red once, green on confirmation",
                                 failing=again.failing)
            else:
                return "pinned", Outcome(False, subset.ran, _named(caused),
                                         failing=caused), None
    else:
        subset = Outcome(True, 0, "no covering module measured")
    full = box.full()
    if full.failing and not (full.failing - clean):
        # The whole suite went red only on tests this mutation's own module-set was
        # already failing. Not evidence either.
        full = Outcome(True, full.ran, f"red on nothing new ({_named(full.failing)})",
                       failing=full.failing)
    if not full.green:
        # And the same for the run that has the last word. This is where the asymmetry
        # bites hardest: a flaky full suite reads as "pinned", and "pinned" is the verdict
        # that certifies a guard as tested and is never revisited. Six thousand tests, real
        # tmux servers, and a machine that may be running other sweeps — one confirming run
        # is expensive and still cheaper than one guard wrongly declared safe.
        confirm = box.full()
        if confirm.green:
            return "survived", subset, Outcome(
                True, confirm.ran, f"red once ({full.detail[:80]}), green on confirmation")
        if not (full.conclusive or confirm.conclusive):
            # Twice unmeasurable. The honest answer is that this mutation has no verdict,
            # and saying so is the whole point: "pinned" here would be a guard certified
            # as tested by a machine that was too busy to run its tests.
            return "unresolved", subset, confirm
        full = full if full.conclusive else confirm
    return ("survived" if full.green else "pinned"), subset, full


def sweep(root: Path, ref: str, scope: dict[str, set[int]], selection: dict[str, list[str]],
          workdir: Path, jobs: int, dirty: dict[str, bytes], second_order: int = 0,
          log=print, full_timeout: float = FULL_TIMEOUT
          ) -> tuple[list[Result], list["Pair"]]:
    """Every mutation, run; every survivor, re-run against the whole suite."""
    plan: list[Mutation] = []
    sources: dict[str, bytes] = {}
    for rel, lines in sorted(scope.items()):
        blob = dirty.get(rel) or _blob_at(root, ref, rel)
        if not blob:
            continue
        sources[rel] = blob
        plan.extend(mutations_for(rel, blob, lines))
    log(f"  {len(plan)} mutations across {len(scope)} file(s)")
    if not plan:
        return [], []

    log(f"  building {jobs} sandbox(es)…")
    boxes = [Sandbox(root, workdir / f"w{i}", ref, dirty) for i in range(jobs)]
    for box in boxes:
        box.full_timeout = full_timeout
    free: list[Sandbox] = list(boxes)
    import threading
    lock = threading.Lock()
    results: list[Result] = []
    counter = {"n": 0}

    def take() -> Sandbox:
        while True:
            with lock:
                if free:
                    return free.pop()
            time.sleep(0.05)

    def give(box: Sandbox) -> None:
        with lock:
            free.append(box)

    def run_one(mutation: Mutation) -> Result:
        modules = select_for(selection, mutation)
        box = take()
        try:
            verdict, subset, full = decide(box, mutation, modules)
        finally:
            box.restore()
            give(box)
        with lock:
            counter["n"] += 1
            n = counter["n"]
        # One line per mutation, always. A survivor costs a full suite run, so a sweep
        # can sit silent for a quarter of an hour, and a tool nobody can tell apart from
        # a hung one is a tool that gets killed and then not re-run.
        how = f"{len(modules)} module(s)" if modules else "no covering module"
        if full is not None:
            how += f", then all {full.ran or '?'}"
        label = {"survived": "SURVIVED", "pinned": "pinned  ",
                 "unresolved": "UNRESOLVED"}[verdict]
        log(f"    [{n}/{len(plan)}] {label}  {mutation}   ({how})")
        return Result(mutation, verdict, subset, full, modules)

    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        results = list(pool.map(run_one, plan))

    # A sandbox and not the checkout: the tests that are supposed to hold a guard are the
    # ones that existed AT THE REF. Reading the working tree's `tests/` instead would
    # answer with a test written after the fact — which, sweeping a historical head, is
    # precisely the test that was missing.
    for r in results:
        if r.verdict == "survived":
            r.evidence = evidence_for(boxes[0].path, r.mutation, r.modules)

    pairs: list[Pair] = []
    if second_order:
        pairs = _second_order(results, boxes, sources, second_order, log)
    for box in boxes:
        shutil.rmtree(box.path, ignore_errors=True)
    return results, pairs


@dataclasses.dataclass
class Pair:
    a: Mutation
    b: Mutation
    outcome: Outcome


def _second_order(results: list[Result], boxes: list["Sandbox"],
                  sources: dict[str, bytes], cap: int, log) -> list[Pair]:
    """Survivors in the same function, applied two at a time.

    Bounded on purpose. Full higher-order mutation is combinatorially out of reach and
    always will be; a second order over *survivors that share an enclosing function* is
    a few dozen runs, and it is the only order that answers the masking question — two
    lines whose consequences are invisible one at a time because each stands behind the
    other.
    """
    groups: dict[tuple[str, str], list[Mutation]] = {}
    for r in results:
        if r.verdict == "survived":
            groups.setdefault((r.mutation.path, r.mutation.symbol), []).append(r.mutation)
    todo: list[tuple[Mutation, Mutation]] = []
    for members in groups.values():
        for i in range(len(members)):
            for j in range(i + 1, len(members)):
                todo.append((members[i], members[j]))
    if not todo:
        return []
    if len(todo) > cap:
        log(f"  second order: {len(todo)} same-function pairs, capping at {cap}")
        todo = todo[:cap]
    else:
        log(f"  second order: {len(todo)} same-function pair(s)")

    import threading
    free = list(boxes)
    lock = threading.Lock()

    def run_pair(pair: tuple[Mutation, Mutation]) -> Pair | None:
        combined = compose(sources[pair[0].path], pair)
        if combined is None:
            return None
        while True:
            with lock:
                if free:
                    box = free.pop()
                    break
            time.sleep(0.05)
        try:
            box.apply_source(pair[0].path, combined)
            outcome = box.full()
        finally:
            box.restore()
            with lock:
                free.append(box)
        return Pair(pair[0], pair[1], outcome)

    with concurrent.futures.ThreadPoolExecutor(max_workers=len(boxes)) as pool:
        return [p for p in pool.map(run_pair, todo) if p is not None]


# --------------------------------------------------------------------------------------
# 7. The report
# --------------------------------------------------------------------------------------

def report(results: list[Result], root: Path, ref: str, base: str, baseline: Outcome | None,
           elapsed: float, pairs: list["Pair"] | None = None) -> str:
    survivors = [r for r in results if r.verdict == "survived"]
    unresolved = [r for r in results if r.verdict == "unresolved"]
    pinned = [r for r in results if r.verdict == "pinned"]
    pairs = pairs or []
    out: list[str] = []
    w = out.append
    w("=" * 86)
    w(f"deletion sweep — {ref[:12]} against {base[:12]}")
    w("=" * 86)
    w(f"measured on      : {sys.platform}, CPython "
      f"{'.'.join(str(n) for n in sys.version_info[:3])}")
    if baseline is not None:
        w(f"baseline          : Ran {baseline.ran} tests — {'OK' if baseline.green else baseline.detail}")
    w(f"mutations applied : {len(results)}")
    w(f"pinned            : {len(pinned)}")
    w(f"SURVIVED          : {len(survivors)}")
    w(f"UNRESOLVED        : {len(unresolved)}")
    w(f"wall clock        : {elapsed / 60:.1f} min")
    w("")

    if unresolved:
        w("-" * 86)
        w("UNRESOLVED — these mutations have no verdict")
        w("-" * 86)
        w("Twice the run could not be measured: it timed out rather than failing. That is")
        w("not a red and it is emphatically not a pin — nothing here has been shown to be")
        w("tested. Re-run these on a quieter machine, or raise the timeout, before reading")
        w("this sweep as clean.")
        for r in sorted(unresolved, key=lambda r: (r.mutation.path, r.mutation.line)):
            m = r.mutation
            w(f"  {m.path}:{m.line}  in `{m.symbol}`  [{m.operator}]  "
              f"{(r.full or r.subset).detail if (r.full or r.subset) else ''}")
        w("")

    if not survivors:
        w("Every mutation this diff offered goes red. Nothing added here is a line the")
        w("suite would not miss.")
        return "\n".join(out)

    w("A survivor is a line you can delete with the whole suite still green")
    w(f"ON {sys.platform.upper()}. That last part matters: a clause the operating system")
    w("never reaches here is unreachable, not untested, and the two look identical from")
    w("one machine. Anything marked PLATFORM below wants a second opinion from CI before")
    w("it is read as a missing test.")
    w("")
    w("Otherwise there is no suppression list: if deleting it genuinely changes nothing")
    w("observable, delete it — 'equivalent mutant' and 'dead code' are one finding.")
    w("")
    by_file: dict[str, list[Result]] = {}
    for r in survivors:
        by_file.setdefault(r.mutation.path, []).append(r)

    crowded: dict[tuple[str, str], int] = {}
    for r in survivors:
        key = (r.mutation.path, r.mutation.symbol)
        crowded[key] = crowded.get(key, 0) + 1

    for path, group in sorted(by_file.items()):
        w("-" * 86)
        w(path)
        w("-" * 86)
        for r in sorted(group, key=lambda r: r.mutation.line):
            m = r.mutation
            w("")
            w(f"  {path}:{m.line}  in `{m.symbol}`   [{m.operator}] — {m.question}")
            if crowded.get((m.path, m.symbol), 0) > 1:
                w(f"    NOTE    : {crowded[(m.path, m.symbol)]} survivors sit in `{m.symbol}`. "
                  "Two guards in sequence")
                w("              mask each other, so none of them is safe to call "
                  "equivalent on its own.")
            w(f"    shipped : {_oneline(m.before, 74)}")
            w(f"    mutant  : {_oneline(m.after, 74) or '(the statement, deleted)'}")
            if r.full:
                w(f"    full    : Ran {r.full.ran} tests — OK, with the line gone")
            caveat = platform_caveat(m)
            if caveat:
                w(f"    PLATFORM: `{caveat}` is the operating system's behaviour, not")
                w(f"              charter's. On {sys.platform} this clause may never be")
                w("              entered at all, which is unreachable rather than")
                w("              unpinned. Push the mutant to a throwaway branch and let")
                w("              CI answer before writing a test for it.")
            ev = r.evidence
            if ev is None:
                continue
            if not ev.modules:
                w("    covered : nothing measured executes this file at all")
            elif ev.nothing_names_it:
                w(f"    covered : {len(ev.modules)} module(s) execute this file and NOT ONE")
                w(f"              names `{m.symbol}` — {', '.join(ev.modules[:4])}"
                  + (" …" if len(ev.modules) > 4 else ""))
            else:
                w(f"    covered : {len(ev.naming)} test(s) name `{m.symbol}`; what they assert:")
                for module, name, asserts in ev.naming[:3]:
                    w(f"              {module.split('.')[-1]}.{name}")
                    for a in asserts[:3]:
                        w(f"                  {a}")
                if len(ev.naming) > 3:
                    w(f"              … and {len(ev.naming) - 3} more")

    if pairs:
        w("")
        w("-" * 86)
        w("second order — survivors in the same function, applied together")
        w("-" * 86)
        w("A line whose consequence is hidden behind a SECOND unpinned line looks")
        w("equivalent one mutation at a time. It is not. This is that question, asked.")
        for p in sorted(pairs, key=lambda p: (p.a.path, p.a.line, p.b.line)):
            state = ("still green together" if p.outcome.green
                     else f"caught together ({p.outcome.detail})")
            w("")
            w(f"  {p.a.path} `{p.a.symbol}`  — {state}")
            w(f"    {p.a.line}: {_oneline(p.a.before, 62)} -> {_oneline(p.a.after, 34) or '(deleted)'}")
            w(f"    {p.b.line}: {_oneline(p.b.before, 62)} -> {_oneline(p.b.after, 34) or '(deleted)'}")
    w("")
    w("=" * 86)
    w(f"{len(survivors)} survivor(s). Each is a guard with no test behind it, or a line")
    w("that should not be there.")
    return "\n".join(out)


def as_json(results: list[Result]) -> str:
    return json.dumps([{
        "path": r.mutation.path, "line": r.mutation.line,
        "end_line": r.mutation.end_line,
        "operator": r.mutation.operator, "symbol": r.mutation.symbol,
        "question": r.mutation.question,
        "before": r.mutation.before, "after": r.mutation.after,
        "verdict": r.verdict,
        # `detail` and not only the verdict: a run that went red says WHICH test went red,
        # and that is the first thing anyone asks of a mutation reported as pinned.
        "subset": None if not r.subset else {
            "green": r.subset.green, "ran": r.subset.ran, "detail": r.subset.detail},
        "full": None if not r.full else {
            "green": r.full.green, "ran": r.full.ran, "detail": r.full.detail},
        "modules": r.modules,
        "platform": sys.platform,
        "platform_caveat": platform_caveat(r.mutation),
        "naming": [] if not r.evidence else [
            {"module": m, "test": t, "asserts": a} for m, t, a in r.evidence.naming],
    } for r in results], indent=1)


# --------------------------------------------------------------------------------------
# 8. CLI
# --------------------------------------------------------------------------------------

def repo_root(start: Path) -> Path:
    return resolved(git("rev-parse", "--show-toplevel", cwd=start).strip())


def workdir_for(root: Path, override: str | None) -> Path:
    """Where the sandboxes and the trace cache live.

    Outside the checkout, always. Sandboxes and a trace cache are not the working tree's
    business, `.git` is a FILE and not a directory in a linked worktree, and a sweep run
    from one worktree must not write into the repository another is using.

    And resolved, which is the load-bearing half. `tempfile.gettempdir()` is `$TMPDIR`;
    macOS spells that `/var/folders/…` and `/var` is a symlink to `/private/var`. So the
    DEFAULT workdir was the one path in this tool guaranteed to carry a symlink down into
    the selection map, and on macOS the tool could not sweep at all (#572). An explicit
    `--workdir` gets the same treatment, which also makes a relative one mean what the
    operator meant. The digest is taken from the resolved root, so one checkout reached
    by two names gets one workdir and one cache instead of two.
    """
    if override:
        return resolved(override)
    digest = hashlib.sha256(str(resolved(root)).encode()).hexdigest()[:12]
    return resolved(Path(tempfile.gettempdir()) / f"charter-sweep-{digest}")


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(
        prog="tools/sweep.py",
        description=__doc__.split("\n\n")[0],
        formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--ref", default="HEAD", help="Commit to sweep (default: HEAD).")
    p.add_argument("--base", default=None,
                   help="Charge added lines against this (default: merge-base with "
                        "origin/main, or main).")
    p.add_argument("--path", action="append", default=None, dest="paths",
                   help="Directory to sweep, repeatable (default: charter).")
    p.add_argument("--all", action="store_true",
                   help="Charge the whole tree instead of the diff — a number, not a gate.")
    p.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 4) // 2),
                   help="Sandboxes to run at once.")
    p.add_argument("--workdir", default=None, help="Where sandboxes and the cache live.")
    p.add_argument("--refresh-map", action="store_true", help="Re-trace the selection map.")
    p.add_argument("--no-baseline", action="store_true",
                   help="Skip the unmutated full run. A red baseline makes every mutation "
                        "look pinned, so this is off by default for a reason.")
    p.add_argument("--json", default=None, help="Also write the full result set here.")
    p.add_argument("--second-order", type=int, default=0, metavar="MAX",
                   help="After the sweep, run up to MAX pairs of survivors that share an "
                        "enclosing function, together. Two guards in sequence mask each "
                        "other, and a masked mutant looks equivalent one at a time.")
    p.add_argument("--keep", action="store_true", help="Leave the sandboxes behind.")
    args = p.parse_args(argv)

    root = repo_root(Path.cwd())
    paths = tuple(args.paths or DEFAULT_PATHS)
    ref = git("rev-parse", args.ref, cwd=root).strip()
    if args.base:
        base = git("rev-parse", args.base, cwd=root).strip()
    else:
        upstream = "origin/main" if git("rev-parse", "--verify", "--quiet", "origin/main",
                                        cwd=root, check=False).strip() else "main"
        base = git("merge-base", ref, upstream, cwd=root).strip()

    workdir = workdir_for(root, args.workdir)
    workdir.mkdir(parents=True, exist_ok=True)
    cache_dir = workdir / "cache"

    def log(*a):
        print(*a, flush=True)

    log(f"  workdir: {workdir}")
    started = time.time()
    log(f"sweeping {ref[:12]} (paths: {', '.join(paths)})")
    dirty = dirty_files(root, paths) if args.ref == "HEAD" else {}
    if dirty:
        log(f"  carrying {len(dirty)} uncommitted file(s) into the sandboxes")

    if args.all:
        scope = all_lines(root, paths)
        log(f"  --all: the whole tree, {len(scope)} file(s)")
    else:
        scope = added_lines(root, base, ref, paths)
        log(f"  diff against {base[:12]}: {len(scope)} file(s), "
            f"{sum(len(v) for v in scope.values())} added line(s)")
    if not scope:
        log("  nothing under the swept paths changed. Nothing to do.")
        return 0

    # The map is measured on a clean checkout of the ref, so that a mutation is the only
    # thing that ever differs from what was traced.
    log("  preparing the reference sandbox…")
    ref_box = Sandbox(root, workdir / "ref", ref, dirty)
    selection = load_map(ref_box.path, paths, cache_dir, args.jobs, args.refresh_map, log)

    baseline = None
    full_timeout = FULL_TIMEOUT
    if not args.no_baseline:
        log("  baseline: full suite, unmutated…")
        started_baseline = time.time()
        baseline = ref_box.full()
        took = time.time() - started_baseline
        log(f"    Ran {baseline.ran} tests — {'OK' if baseline.green else baseline.detail}"
            f" in {took:.0f}s")
        # Six times what this machine, right now, took to run the suite once — and never
        # below the floor. The mutants that matter are the ones that make the suite SLOW,
        # and the sweep may be sharing the box with itself.
        full_timeout = max(FULL_TIMEOUT, took * 6)
        if full_timeout > FULL_TIMEOUT:
            log(f"    full-suite timeout raised to {full_timeout / 60:.0f} min for this box")
        if not baseline.green:
            log("  ! the tree is RED before any mutation. Every mutation below will look")
            log("  ! pinned for a reason that has nothing to do with the guard. Fix first.")
            return 2

    results, pairs = sweep(root, ref, scope, selection, workdir, args.jobs, dirty,
                           args.second_order, log, full_timeout)
    elapsed = time.time() - started
    text = report(results, root, ref, base, baseline, elapsed, pairs)
    log("")
    log(text)
    if args.json:
        Path(args.json).write_text(as_json(results))
    if not args.keep:
        for child in workdir.glob("w*"):
            shutil.rmtree(child, ignore_errors=True)
        shutil.rmtree(workdir / "ref", ignore_errors=True)
    if any(r.verdict == "survived" for r in results):
        return 1
    # 3, not 0: a sweep that could not measure some of its mutations has not shown the
    # branch to be clean, and a gate reading this must not treat "I could not look" as
    # "nothing to see".
    return 3 if any(r.verdict == "unresolved" for r in results) else 0


if __name__ == "__main__":       # pragma: no cover - entry point
    sys.exit(main())
