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

**Never trusted when it says "survived".** Selection is an optimisation and must never be
the final word. Any mutation that survives its subset is re-run against the FULL suite
before it is reported. A false survivor costs one full run; a false *pin* would be the
exact bug this file exists to prevent, so the asymmetry decides the design.

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
    json.dump({"files": sorted(seen), "error": error}, fh)
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
    modules = test_modules(root)
    scratch.mkdir(parents=True, exist_ok=True)
    runner = scratch / "_trace_runner.py"
    runner.write_text(_TRACE_RUNNER)
    prefix = str(root) + os.sep
    hits: dict[str, list[str]] = {}
    done = 0

    def one(module: str) -> tuple[str, list[str], str]:
        out = scratch / f"{module}.json"
        subprocess.run([sys.executable, str(runner), module, str(out), prefix],
                       cwd=str(root), check=False, timeout=SUBSET_TIMEOUT,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            payload = json.loads(out.read_text())
        except (OSError, ValueError):
            return module, [], "the trace runner wrote nothing"
        return module, payload["files"], payload["error"]

    broken: list[str] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        for module, files, error in pool.map(one, modules):
            done += 1
            if done % 40 == 0:
                log(f"    traced {done}/{len(modules)} modules")
            if error:
                broken.append(f"{module}: {error.strip().splitlines()[-1]}")
            for f in files:
                name, _, symbol = f.partition("::")
                rel = Path(name).resolve().relative_to(root.resolve()).as_posix()
                hits.setdefault(f"{rel}::{symbol}" if symbol else rel, []).append(module)

    # An empty or near-empty map is a broken map, not a tree with no coverage, and it is
    # the failure mode that hides: every mutation still gets the right verdict, because
    # a file with no covering module goes to the full suite — it just takes a hundred
    # times as long and looks like the tool working.
    if len(hits) < 2 or len(broken) > len(modules) // 4:
        log(f"  ! the trace produced {len(hits)} file(s) and {len(broken)} broken "
            f"module(s). The map is not usable:")
        for line in broken[:3]:
            log(f"  !   {line}")
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
    green: bool
    ran: int
    detail: str


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


def _verdict(proc) -> Outcome:
    text = (proc.stdout or "") + (proc.stderr or "")
    m = _RAN.search(text)
    ran = int(m.group(1)) if m else 0
    if proc.returncode == 0 and ran > 0:
        return Outcome(True, ran, "OK")
    if ran == 0:
        return Outcome(False, 0, "no tests ran")
    tail = [ln for ln in text.splitlines() if ln.startswith(("FAILED", "ERROR:", "FAIL:"))]
    return Outcome(False, ran, "; ".join(tail[:3]) or f"rc={proc.returncode}")


class Sandbox:
    """A private clone of the tree. The operator's checkout is never written to."""

    def __init__(self, root: Path, where: Path, ref: str, dirty: dict[str, bytes]):
        self.path = where
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

    def run(self, argv: list[str], timeout: int) -> Outcome:
        env = dict(os.environ)
        env.pop("PYTHONDONTWRITEBYTECODE", None)
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
            # A mutation that HANGS the suite has not survived it. Round three had one:
            # deleting `Surface.run`'s `if chunk is None: return None` wedged the suite
            # for 1800s rather than failing, and reading that as green would have pinned
            # the guard on a timeout.
            _kill_group(proc)
            proc.communicate()
            return Outcome(False, 0, f"timed out after {timeout}s")
        return _verdict(_Finished(proc.returncode, out))

    def subset(self, modules: list[str]) -> Outcome:
        return self.run(list(modules), SUBSET_TIMEOUT)

    def full(self) -> Outcome:
        return self.run(["discover", "-s", "tests", "-t", "."], FULL_TIMEOUT)


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
    box.apply(mutation)
    if modules:
        subset = box.subset(modules)
    else:
        subset = Outcome(True, 0, "no covering module measured")
    if not subset.green:
        return "pinned", subset, None
    full = box.full()
    return ("survived" if full.green else "pinned"), subset, full


def sweep(root: Path, ref: str, scope: dict[str, set[int]], selection: dict[str, list[str]],
          workdir: Path, jobs: int, dirty: dict[str, bytes], second_order: int = 0,
          log=print) -> tuple[list[Result], list["Pair"]]:
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
        if n % 10 == 0 or verdict == "survived":
            log(f"    [{n}/{len(plan)}] {'SURVIVED' if verdict == 'survived' else 'pinned  '}"
                f"  {mutation}")
        return Result(mutation, verdict, subset, full, modules)

    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        results = list(pool.map(run_one, plan))

    for r in results:
        if r.verdict == "survived":
            r.evidence = evidence_for(root, r.mutation, r.modules)

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
    pairs = pairs or []
    out: list[str] = []
    w = out.append
    w("=" * 86)
    w(f"deletion sweep — {ref[:12]} against {base[:12]}")
    w("=" * 86)
    if baseline is not None:
        w(f"baseline          : Ran {baseline.ran} tests — {'OK' if baseline.green else baseline.detail}")
    w(f"mutations applied : {len(results)}")
    w(f"pinned            : {len(results) - len(survivors)}")
    w(f"SURVIVED          : {len(survivors)}")
    w(f"wall clock        : {elapsed / 60:.1f} min")
    w("")

    if not survivors:
        w("Every mutation this diff offered goes red. Nothing added here is a line the")
        w("suite would not miss.")
        return "\n".join(out)

    w("A survivor is a line you can delete with the whole suite still green.")
    w("There is no suppression list: if deleting it genuinely changes nothing")
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
        "operator": r.mutation.operator, "symbol": r.mutation.symbol,
        "question": r.mutation.question,
        "before": r.mutation.before, "after": r.mutation.after,
        "verdict": r.verdict,
        "subset": None if not r.subset else {"green": r.subset.green, "ran": r.subset.ran},
        "full": None if not r.full else {"green": r.full.green, "ran": r.full.ran},
        "modules": r.modules,
        "naming": [] if not r.evidence else [
            {"module": m, "test": t, "asserts": a} for m, t, a in r.evidence.naming],
    } for r in results], indent=1)


# --------------------------------------------------------------------------------------
# 8. CLI
# --------------------------------------------------------------------------------------

def repo_root(start: Path) -> Path:
    return Path(git("rev-parse", "--show-toplevel", cwd=start).strip())


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

    # Outside the checkout, always. Sandboxes and a trace cache are not the working
    # tree's business, `.git` is a FILE and not a directory in a linked worktree, and a
    # sweep run from one worktree must not write into the repository another is using.
    workdir = Path(args.workdir) if args.workdir else (
        Path(tempfile.gettempdir())
        / f"charter-sweep-{hashlib.sha256(str(root).encode()).hexdigest()[:12]}")
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
    if not args.no_baseline:
        log("  baseline: full suite, unmutated…")
        baseline = ref_box.full()
        log(f"    Ran {baseline.ran} tests — {'OK' if baseline.green else baseline.detail}")
        if not baseline.green:
            log("  ! the tree is RED before any mutation. Every mutation below will look")
            log("  ! pinned for a reason that has nothing to do with the guard. Fix first.")
            return 2

    results, pairs = sweep(root, ref, scope, selection, workdir, args.jobs, dirty,
                           args.second_order, log)
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
    return 1 if any(r.verdict == "survived" for r in results) else 0


if __name__ == "__main__":       # pragma: no cover - entry point
    sys.exit(main())
