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

**And it says which of three things it found.** A gate that blocks nothing still has to
say something, and reporting `success` on a branch with eight survivors under it says the
opposite of what the job exists for. Completed-and-clean, completed-with-N-survivors, and
*did not complete* are three answers, they are not two, and the last of them is what a
sweep spread over several machines has to be able to report when one of them is cancelled.

    python3 tools/sweep.py                      # this branch, against its merge-base
    python3 tools/sweep.py --ref 5b02b3f        # some other commit
    python3 tools/sweep.py --path tools         # sweep the sweep
    python3 tools/sweep.py --all                # the whole tree, as a number
    python3 tools/sweep.py --gate               # as CI runs it (stage C)
    python3 tools/sweep.py --plan               # how many mutations, how many jobs
    python3 tools/sweep.py --gate --shard 2/3   # one job's slice of that plan
    python3 tools/sweep.py --verdict shards/ --shards 3      # all of them, added up
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


def reach() -> str:
    """What this interpreter puts out of the sweep's reach, or "" when nothing does.

    A sweep that asks fewer questions than it could, and does not say so, is the quietest
    way this tool can mislead: fewer mutations, no survivors among them, and a report that
    reads exactly like a clean one.

    PEP 701 is the one case measured so far. Before 3.12, `ast` gives an f-string's
    internal nodes approximate positions, so `retune-string` cannot prove that the bytes at
    a segment's span are the segment's value — and it refuses rather than splicing over a
    position it cannot vouch for. The consequence is concrete: `f"{name:<28}"` is where a
    width literal lives in this tree, it is #508's whole defect, and on 3.11 the sweep
    cannot see it. CI's gate job pins 3.12 for exactly this reason.
    """
    if sys.version_info < (3, 12):
        return ("literals inside an f-string (positions are approximate before PEP 701; "
                "run the sweep on 3.12+ to reach them)")
    return ""


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
    """Every line of every swept file — what ``--all`` charges instead of a diff.

    There is no ``if base.exists()`` here, and its absence is a finding rather than an
    oversight. It was written, the sweep deleted it, and the suite stayed green on 3.12
    and 3.14: `rglob` on a directory that is not there yields nothing, so the guard
    refused a loop that was already empty. Per §4 of the spec that makes it dead code and
    dead code gets deleted — "equivalent mutant" and "unreachable line" are one finding.
    """
    found: dict[str, set[int]] = {}
    for p in paths:
        base = root / p
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
    #: A digest of the file this mutation was READ FROM. The sandbox refuses to apply a
    #: mutation to anything else, which is the fifth way a sweep lies (#586) closed by
    #: construction: a mutant tree that is really the pristine tree passes its tests and is
    #: reported SURVIVED, and a false survivor is the failure that ends adoption — it sends
    #: somebody to write a test for a line that is already covered, and the first person
    #: who chases one and finds that out stops believing the tool.
    origin: str = ""
    #: Why this mutation was never run, or ``""`` for the ordinary case (#698).
    #:
    #: **A mutation the tool declines is a question it did not ask, and the one thing it
    #: may not do is decline one in silence** — that is the same failure as a shard that
    #: never reported, arriving from inside the plan instead of from a runner. So a
    #: declined mutation is still planned, still sharded and still reported; it simply
    #: carries its reason here and gets its verdict without a sandbox. `unevaluated` and
    #: `reach` are the two subtractions that came before this one, and the difference is
    #: that those remove a POSITION the operator never reaches while this removes a
    #: mutant the operator did produce — which is exactly the one worth counting.
    withheld: str = ""

    @property
    def tag(self) -> str:
        """`path:line:operator`, and then the edit that makes this one not its sibling.

        The prefix is unchanged, because it is what a reader greps for; the discriminator
        is appended rather than substituted. What is appended is the one field that is
        different by construction: :func:`mutations_for` de-duplicates on
        ``(line, col, operator, replacement)``, so two mutants that survive that key
        together differ in `after` and in nothing else the report round-trips. Without it
        a single `if A and B:` renders both of its `drop-conjunct` mutants as one string
        in the not-applied and no-verdict lists, and a list that names two different
        mutations identically discriminates neither (#721).
        """
        edit = _oneline(self.after, 40) or "(deleted)"
        return f"{self.path}:{self.line}:{self.operator} -> {edit}"

    def __str__(self) -> str:
        """The per-mutation progress line's whole account of one mutation.

        `before` is the mutated NODE, and **ambiguity appears exactly where the node is
        larger than the thing being varied** — for `drop-conjunct` it is the whole
        condition, identical for every conjunct. `question` is the field that says which
        one went, and until #721 only the gate summary printed it. A shard reclaimed
        before it could emit a summary leaves this log as the only record, and it read the
        same whether the dropped half was a cheap prefilter (equivalent, and correctly
        dismissed) or the test that decides. Two fail-open holes in a security guard were
        read as the first and dismissed across two full sweep runs on exactly that. The
        two streams now say the same thing, which is the property that failed.
        """
        return (f"{self.path}:{self.line} [{self.operator}] "
                f"{_oneline(self.before, 72)} — {self.question}")


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


#: Expression nodes whose source text **re-associates** with whatever it is spliced next
#: to. `ast` positions do not include the grouping parentheses a programmer wrote, so
#: `sp.text` of the `x or ""` inside `(x or "").strip()` is `x or ""` — and splicing that
#: back somewhere tighter silently rebuilds the expression. See :func:`parenthesised`.
LOOSE = (ast.BoolOp, ast.NamedExpr, ast.BinOp, ast.UnaryOp, ast.Lambda, ast.IfExp,
         ast.Await, ast.Yield, ast.YieldFrom, ast.Compare, ast.Tuple)

#: And the ones that do not: a name, a call, a subscript, a display, a literal. Their
#: source text already carries its own delimiters, so it means the same thing wherever it
#: lands.
#:
#: `Starred` and `Slice` are here because a parenthesis around either is a `SyntaxError`,
#: and both are unreachable by construction rather than by luck: :func:`parenthesised`
#: only ever sees text that ``ast.parse(..., mode="eval")`` accepted, and `*a` and `a:b`
#: are not expressions that mode can parse. `TheSpliceIsTheEditDescribed` asserts that
#: these two tuples between them name **every** subclass of `ast.expr`, which is the same
#: protection `CMP_TEXT` gets and for the same reason — the day Python grows a new
#: expression node, the suite says so rather than the tool quietly guessing.
#:
#: The tail is PEP 750's template strings, which 3.14 added and 3.12 — the version the
#: gate pins — does not have. They are the ``t"…"`` analogues of `JoinedStr` and
#: `FormattedValue`, delimited by their own quotes and braces, so they are tight for the
#: same reason those are. Reached through `getattr` because naming them outright is an
#: `AttributeError` on the interpreter CI actually runs, and the completeness test is
#: what keeps the conditional honest rather than a guess: on 3.14 it demands them, on
#: 3.12 there is nothing to demand.
TIGHT = (ast.Dict, ast.Set, ast.ListComp, ast.SetComp, ast.DictComp, ast.GeneratorExp,
         ast.Call, ast.FormattedValue, ast.JoinedStr, ast.Constant, ast.Attribute,
         ast.Subscript, ast.Name, ast.List, ast.Starred, ast.Slice) + tuple(
    t for t in (getattr(ast, "TemplateStr", None), getattr(ast, "Interpolation", None))
    if t is not None)


def parenthesised(text: str) -> str:
    """*text*, spelled so that splicing it cannot rebuild the expression around it.

    **The mutant has to be the edit the report describes.** Measured over `charter/` and
    `tools/` before this existed: 144 of 8,903 expression mutations spliced something
    else. The shape is everywhere — `(x or "").strip()` is 137 of them — and it comes
    from one fact about `ast`: a node's span excludes the parentheses the programmer put
    around it, because they are grouping and not syntax. So `sp.text` of the receiver in
    `(p.stderr or p.stdout or "").strip()` is `p.stderr or p.stdout or ""`, and the
    `swap-synonym` mutant built from it reads ``p.stderr or p.stdout or "".lstrip()`` —
    which does not swap which end is stripped, it deletes the strip on every path but
    one.

    Two ways that lies, and they are the two failures this file is built around:

    * **A survivor answering a question nobody asked.** The report prints "is `how much`
      pinned?" beside a mutant that dropped the normalisation entirely. Under
      ``--enforce`` that is a blocked branch whose author is sent to write a test for a
      property the mutation never perturbed.
    * **A false pin**, which is worse and is the one this file exists to prevent.
      ``(vc or {}).get("config", {})`` becomes ``vc or {}["config"]``, and `{}["config"]`
      is a `KeyError` — so the suite goes red for a crash and the fallback is certified
      as tested. Four sites in this tree are exactly that.

    The rule is a property of Python, not a list of the shapes that happened to be
    caught: an expression whose top node is in :data:`LOOSE` binds more weakly than
    something it could be spliced beside, so it is wrapped; one in :data:`TIGHT` carries
    its own delimiters and is left alone. Text that is not an expression at all — the
    empty string of a statement deletion, the ``pass`` that replaces it, the raw
    ``{:<28}`` of an f-string's format spec — does not parse here and is returned
    untouched, which is the only correct answer for a splice that is not an expression.
    """
    try:
        top = ast.parse(text, mode="eval").body
    except SyntaxError:
        return text
    if not isinstance(top, LOOSE):
        return text
    # Idempotent, because two operators reach this with text that has already been
    # through it: `drop-conjunct` spells each half and then :func:`mutations_for` spells
    # the join. `((prog != "export"))` is a mutant nobody can read.
    #
    # A bracket already around the WHOLE expression is the only thing that pushes its
    # first token off line one, column zero, and that holds because of what reaches here:
    # a node's own source text, which by definition starts where the node starts, or a
    # replacement built by joining such texts. Neither can begin with whitespace or a
    # comment. `(a) and b` is the case this must NOT match and does not — the `and`
    # begins at column zero however many brackets sit inside it.
    #
    # A `text.startswith("(")` was written beside this and the deletion sweep took it:
    # with the column test present it is a second answer to the same question, and a
    # guard nothing can redden is a line this file's own rule says to delete.
    if (top.lineno, top.col_offset) > (1, 0):
        return text
    return f"({text})"


def _spell(sp: "_Spans", node: ast.AST) -> str:
    """*node*'s source, safe to interpolate into a replacement built around it.

    The outer half of :func:`parenthesised` is applied once, in :func:`mutations_for`, to
    every replacement. That cannot reach an operand spliced INTO the middle of one —
    ``f"{receiver}.{other}"`` produces a `swap-synonym` mutant whose top node is an
    `Attribute` however loose the receiver is — so an operand is spelled here instead.
    """
    return parenthesised(sp.text(node))


def _fstring_segments(tree: ast.AST) -> set[int]:
    """The ids of the constants that are an f-string's *literal text*.

    These are the one place a replacement is raw bytes rather than an expression: the
    span holds no quotes, so `retune-string` splices `<28` -> `<39` as text. Running
    :func:`parenthesised` over that would be a category error and occasionally a
    corruption — a segment spelling `a-b` parses as a `BinOp` and would come back
    `(b-c)`, parentheses and all, inside the string.
    """
    return {id(part) for node in ast.walk(tree) if isinstance(node, ast.JoinedStr)
            for part in node.values if isinstance(part, ast.Constant)}


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


#: The two comparisons that differ by their boundary and by nothing else. `<` and `<=`
#: accept the same values but one number apart, so a mutant is always type-correct, always
#: runs, and asks exactly one question: **is the edge pinned, or only the direction?**
#: `==` is deliberately absent — `!=` is not a near-synonym of `==`, it is its negation,
#: and a whole-condition inversion is a different (and much coarser) question.
BOUNDARY = {ast.Lt: ("<", "<="), ast.LtE: ("<=", "<"),
            ast.Gt: (">", ">="), ast.GtE: (">=", ">")}

#: Every comparison operator, spelled. A chained comparison (`0 <= i < n`) is one node, so
#: moving the boundary of ONE link means re-spelling the whole chain, and a link this table
#: could not spell would be silently dropped — a mutation that is not the mutation the
#: report describes.
#:
#: Subscripted directly, with no `.get` and no skip-if-missing guard, because the guard
#: would be a line nothing can reach: `ast` has exactly these ten comparison operators and
#: they are all here. `TheBoundaryShape` asserts that completeness against
#: `ast.cmpop.__subclasses__()` instead, which is the same protection moved to the one
#: place that can actually fail — the suite, on the day Python grows an eleventh.

CMP_TEXT = {ast.Eq: "==", ast.NotEq: "!=", ast.Lt: "<", ast.LtE: "<=", ast.Gt: ">",
            ast.GtE: ">=", ast.Is: "is", ast.IsNot: "is not", ast.In: "in",
            ast.NotIn: "not in"}

#: Near-synonyms: two names from the **standard library** documented as doing the same job
#: and differing along exactly one axis. The axis is written down for each pair because it
#: is the whole justification — a swap along one axis produces a mutant that is still
#: type-correct, still runs, and asks one question rather than "does this code work at all".
#:
#: Nothing charter-specific is in this table and nothing may be added to it from a finding.
#: That is the same discipline as :data:`NARROW_TO`: the perturbation is chosen from what
#: Python guarantees, never from what this project's answer key happened to contain. A
#: table drawn from findings is a table that scores well on the findings it was drawn from.
SYNONYMS = {
    "lower": ("upper", "case"),        "upper": ("lower", "case"),
    "startswith": ("endswith", "which end"), "endswith": ("startswith", "which end"),
    "lstrip": ("rstrip", "which side"), "rstrip": ("lstrip", "which side"),
    "strip": ("lstrip", "how much"),
    "split": ("rsplit", "which end"),  "rsplit": ("split", "which end"),
    "partition": ("rpartition", "which end"),
    "rpartition": ("partition", "which end"),
    "find": ("rfind", "which end"),    "rfind": ("find", "which end"),
    # `index`/`rindex` is deliberately NOT here, and the reason is the rule the table is
    # built on. A pair belongs only if the swap is type-correct wherever the name appears,
    # and `index` is on `list`, `tuple` and `str` while `rindex` is on `str` alone — so
    # `args.index("-m")` would mutate into an `AttributeError`. That reddens the suite for
    # a reason that has nothing to do with which end was searched, which is a FALSE PIN,
    # which is the failure this whole file exists to prevent. Checked against the tree
    # before dropping it: all five `.index(` calls in `charter/` are on lists.
    "min": ("max", "which extreme"),   "max": ("min", "which extreme"),
    "any": ("all", "how many"),        "all": ("any", "how many"),
    "sorted": ("list", "ordering"),
}

#: Calls that only ever *normalise* their receiver and return the same kind of thing, so
#: dropping one is a mutation that always parses and always runs. This is #572's own shape:
#: the map keys were `resolve()`d on both sides and the prefix that chose them was not, and
#: "a normalisation applied at one site and missing at another" is the bug that hid behind
#: the double `resolve()` two lines away. A test that never sees a symlink, a relative path
#: or a trailing separator cannot tell the mutant from the shipped line.
NORMALISERS = ("resolve", "absolute", "expanduser", "casefold")


def indistinguishable(name: str, other: str, call: ast.Call) -> str:
    """Why re-spelling a call to *name* as *other* would be the SAME PROGRAM, or ``""``.

    #655 asks for a third verdict beside `pinned`/`survived`, for the mutant no honest
    test can redden. The measurement it asked for first says no: across every sweep this
    repository still holds a result for — 461 distinct survivors on ten branches — the
    number a rule could decide is **one**, and the cases the proposal names are not
    decidable at all. `path.partition("/")` is equivalent only if a GitHub
    `path_with_namespace` really does hold one slash, which is a fact about a remote API
    and not about the code; a tool asserting it is a suppression list with a rule's
    manners. `GIT_TIMEOUT = 20 -> 21` would have to be decided from "no covering test
    names the symbol", which is the report's *loudest finding* and not an equivalence —
    and it would be lifted by renaming the constant.

    So there is no third verdict here, and the one genuinely decidable case is answered
    where #632 answered its sibling: the mutation is **not offered**. A survivor no test
    can kill is a false positive, and the place to fix a false positive is the question,
    not the answer. `unevaluated` took the same line about a forward reference under PEP
    563, for the same reason — under ``--enforce`` an unkillable mutant is a blocked
    branch whose author has no move to make, so the operator must not offer it at all.

    **`split` and `rsplit` are one function when no `maxsplit` is given.** Not "usually",
    not "on the values this project happens to pass" — `str.split(sep)` and
    `str.rsplit(sep)` return the same list for every string and every separator, because
    the only thing the `r` decides is which end runs out of splits first and with an
    unlimited budget neither does. Verified exhaustively over every string of up to six
    characters from an alphabet containing the separator, for `str` and `bytes`, and for
    the no-argument whitespace form. `SYNONYMS` justifies a pair by the axis it moves;
    with `maxsplit` absent this pair moves nothing, so the mutation asks nothing and its
    survival says nothing.

    Measured over `charter/` and `tools/`: 67 of the 98 `split`/`rsplit` call sites the
    table would mutate pass no `maxsplit`, against 31 that do and stay a real question.
    Two of them turned up as survivors in the corpus above; every one of the 67 becomes a
    permanent survivor the moment :func:`parenthesised` lands, because until now the
    receiver's lost parentheses were accidentally making the mutant a different program.

    The rule is about the arguments and not about the receiver, so what it claims is
    exactly what it can check. On a receiver that is not a `str` the mutant is not
    equivalent, it is an `AttributeError` — `re.Pattern` has `split` and no `rsplit` —
    and that is a FALSE PIN, the failure this file exists to prevent. Three sites in this
    tree are that shape (`_BACKTICK_RE.split`, `_OPERATOR_SPLIT_RE.split` twice) and all
    three pass no `maxsplit`, so this rule happens to withdraw them; a compiled pattern
    split with a `maxsplit` would still be offered and would still crash. That gap is
    named here rather than papered over, because the fix for it is a receiver-type
    question this pass cannot answer and `SYNONYMS`' own comment on `index`/`rindex`
    already measures by hand.

    Nothing in this tree defines a `split` or an `rsplit` of its own. If something ever
    does, the cost of this rule is one question not asked, which is the direction this
    file errs in everywhere else.
    """
    if {name, other} != {"split", "rsplit"}:
        return ""
    if len(call.args) >= 2 or any(k.arg == "maxsplit" for k in call.keywords):
        return ""
    return ("`split` and `rsplit` are the same function with no `maxsplit`, so the "
            "mutant is the shipped program")


def _shift_char(ch: str) -> str:
    """A different character of the same class, or *ch* when it has no class.

    Letters rotate within their case, digits within the digits. Everything else — the
    punctuation, the whitespace, the escapes, the control bytes, the non-ASCII — is left
    exactly where it was, because that is the string's *shape* and this operator asks
    about its *value*.
    """
    if "a" <= ch <= "z":
        return "a" if ch == "z" else chr(ord(ch) + 1)
    if "A" <= ch <= "Z":
        return "A" if ch == "Z" else chr(ord(ch) + 1)
    if "0" <= ch <= "9":
        return "0" if ch == "9" else chr(ord(ch) + 1)
    return ch


def _regex_shape(text: str) -> set[int]:
    """The indices in *text* that spell a regex's SHAPE rather than its value.

    :func:`retune`'s backslash rule, generalised to the two other places where shifting a
    character does not produce a different pattern but an unparseable one. The rule they
    are all three instances of: **a character that says what kind of thing comes next is
    part of the syntax, and the syntax is not the value this operator asks about.**

    * the character after a ``\\`` — `\\d` -> `\\e` is ``re.error: bad escape``. That one is
      applied in :func:`retune` itself, on every string, because an escape is an escape in
      a replacement template too.
    * the character after ``(?`` — `(?i)` -> `(?j)` is ``unknown extension ?j``, and
      `(?P<n>` -> `(?Q<n>` the same. Four of these in `charter/hooks.py` alone.
    * the HIGH end of a ``[a-b]`` range — `[0-9]` -> `[0-0]`… or rather `[1-0]`, which is
      ``bad character range``. The LOW end is left shiftable on purpose and is the whole
      reason this function names *which* end: `[0-9]` -> `[1-9]` is a valid pattern and a
      genuinely different one (it stops matching `0`), so the question survives. Holding
      both ends would keep the pattern valid by making the mutation a no-op, which is the
      same as not asking — and a character class is the whole of many patterns in this
      tree, so that would withdraw the question for most of them.

    **Measured over `charter/` and `tools/` before choosing.** 108 string constants are
    compiled as regexes. Under the escape rule alone, **56 of them** retune into something
    `re.compile` refuses — more than half of every pattern in the tree, each one a question
    that could never be answered and, since #698, a `no verdict` on the day its line moved.
    With these two rules the 56 become **1** (a `\\x1f` -> `\\x2g` hex escape in
    `charter/tui.py`), and the 20 patterns whose retune was already a no-op are unchanged.
    That last one is left to the backstop in :func:`mutations_for` rather than chased with
    a fourth rule: the rules recover questions, and the backstop is what makes being wrong
    about them safe.

    A `[` inside a class is a literal and a `]` first in a class is too, so the scan tracks
    both; getting that wrong cannot produce a bad mutation, only a missed or a needless
    hold, because nothing here is offered without `re.compile` agreeing it is a pattern.
    """
    held: set[int] = set()
    i, n, in_class = 0, len(text), False
    while i < n:
        ch = text[i]
        if ch == "\\":
            i += 2                      # `retune` holds the escaped character itself
            continue
        if not in_class and ch == "(" and i + 1 < n and text[i + 1] == "?":
            held.add(i + 2)
            i += 3
            continue
        if not in_class and ch == "[":
            in_class = True
            i += 1
            if i < n and text[i] == "^":
                i += 1
            if i < n and text[i] == "]":    # a `]` first in a class is a literal `]`
                i += 1
            continue
        if in_class and ch == "]":
            in_class = False
            i += 1
            continue
        if in_class and i + 2 < n and text[i + 1] == "-" and text[i + 2] != "]":
            held.add(i + 2)
            i += 3
            continue
        i += 1
    return held


def retune(text: str, *, regex: bool = False) -> str:
    """*text*, re-spelled: same length, same character classes, a different value.

    **This is the principled general form the spec said a string constant did not have,
    and the argument for it is that it perturbs the value while holding every structural
    property the surrounding code can depend on.** Same type, same length, same
    punctuation, same escapes, same case pattern, same digits-are-digits. So a format
    string is still a format string (`{:<28}` -> `{:<39}`), a regex is still a regex, a
    terminal escape is still an escape (`\\x1b[?1000h` -> `\\x1b[?2111i`), a path is still
    a path. It is the string analogue of :data:`retune-constant`'s ``n + 1``: the smallest
    edit guaranteed to be a *different value of the same kind*.

    It is derived from the constant and from nothing else. There is no table of "the other
    mouse-tracking mode" and no list of colour names — the objection to fitting the answer
    key is what this form answers, because it cannot fit an answer key it never reads.

    A character **immediately after a backslash** is left alone. In a raw string that
    backslash and its neighbour are one escape — `\\d` in a regex, `\\1` in a replacement —
    and shifting the neighbour turns a working pattern into `re.error: bad escape \\e`.
    That is a red for a reason that has nothing to do with the property, which is the one
    outcome this whole file exists to refuse.

    **`regex` widens that same rule to the two other places a shift changes a pattern's
    shape instead of its value** — the letter after `(?`, and the high end of a `[a-b]`
    range. See :func:`_regex_shape` for both, for which end of a range moves and why, and
    for the measurement that says this is 56 of the 108 patterns in the tree rather than a
    tidy-up. It is a keyword and it is off by default because a string is only a regex
    where the program compiles one: `read_positions` knows that and this function does not,
    so the caller says so rather than this one guessing from the bytes.
    """
    shape = _regex_shape(text) if regex else frozenset()
    out: list[str] = []
    escaped = False
    for i, ch in enumerate(text):
        out.append(ch if escaped or i in shape else _shift_char(ch))
        escaped = ch == "\\" and not escaped
    return "".join(out)


#: Calls whose string argument — or whose string *receiver* — is read by the program
#: rather than shown to a person. A key, a pattern, a separator, a prefix. Standard-library
#: names only, so this is a statement about Python and not about charter.
READERS = frozenset({
    "get", "setdefault", "pop", "getattr", "setattr", "hasattr", "delattr", "getenv",
    "startswith", "endswith", "removeprefix", "removesuffix", "split", "rsplit",
    "partition", "rpartition", "replace", "count", "find", "rfind", "index", "join",
    "compile", "match", "search", "fullmatch", "sub", "subn", "findall", "finditer",
    "glob", "rglob", "encode", "decode", "format",
})


def defers_annotations(tree: ast.AST) -> bool:
    """Does this module carry ``from __future__ import annotations``?

    PEP 563. With it, **every** annotation in the file — a function's `returns`, an
    argument's, an `AnnAssign`'s — is stored as source text and never evaluated. Without
    it every one of them is an ordinary expression, evaluated where it is written.

    A `__future__` import is only legal at the top of a module, so `tree.body` is the
    whole search and a walk would only be able to find an illegal one.
    """
    return any(
        isinstance(stmt, ast.ImportFrom) and stmt.module == "__future__"
        and any(alias.name == "annotations" for alias in stmt.names)
        for stmt in getattr(tree, "body", []))


def unevaluated(tree: ast.AST) -> set[int]:
    """The ids of every node the interpreter never evaluates: #632's property.

    **A position whose value is never evaluated at runtime is not a read position.** That
    is the whole rule, and it is the general form of the defect rather than the one line
    that found it. `sweep(...) -> tuple[list[Result], list["Pair"]]` reported `"Pair"` as
    a survivor on #630's own gate, and it is a survivor **no test can ever kill** — under
    PEP 563 that annotation is the string `"tuple[list[Result], list['Pair']]"` and
    nothing reads it. `tools/sweep.py` carries four of these on its own, and any branch
    that adds a function forward-referencing a class defined below it grows another.

    An unkillable survivor is not a finding, it is the false positive the spec (#565)
    names as the thing that gets a gate switched off. **Under `--enforce` it is worse than
    noise: it is a blocked branch whose author has no move to make.** They cannot write the
    test, because there is no test to write; they cannot suppress it, because #370 refuses
    a suppression list on principle — a config key charter could read is a config key a
    committed file could flip. A gate that blocks with no remedy available is a gate
    somebody turns off, and they are right to. So the operator must not offer this at all.

    **Only under the future import**, which is the version of the rule with no judgement
    in it — and the version where "no remedy" is actually true. Without PEP 563 an
    annotation is an evaluated expression, and what happens to a string inside one depends
    on what evaluates it: `typing.List["Pair"]` compiles the text into a `ForwardRef` right
    there at import, while `list["Pair"]` stores it. So there the string is live, a test
    that exercises whatever resolves the hint can go red without it, and the author of a
    blocked branch has the ordinary move available. "Never evaluated" is provable for the
    deferred case and an argument for the other, and this predicate claims only the half it
    can prove.

    The one thing that *does* re-evaluate a deferred annotation is a program that asks it
    to — `typing.get_type_hints`, or a library that calls it. Measured on this tree:
    there are none, in `charter/`, `tools/` or `tests/`. If one arrives, the cost of this
    rule is a question not asked rather than a guard wrongly certified, which is the
    direction this file errs in everywhere else.

    **The one sibling shape this deliberately does not cover, named so it is not
    rediscovered:** the body of `if typing.TYPE_CHECKING:` never runs either, so a read
    position inside one would be unkillable in exactly the same way. It is not here for
    two reasons. Measured on `charter/`, `tools/` and `tests/`: **zero** occurrences, and
    this file's rule is that a widening carries a measurement. And `TYPE_CHECKING` is a
    NAME, which a module may rebind or import under an alias, so an `ast` pass can only
    guess at it — where `from __future__ import annotations` is a statement the language
    defines. The proof is what makes the rule above have no judgement in it. If such a
    block ever appears in this tree, it belongs in this function and nowhere else.
    """
    if not defers_annotations(tree):
        return set()
    out: set[int] = set()
    for node in ast.walk(tree):
        annotation = None
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            annotation = node.returns
        elif isinstance(node, (ast.arg, ast.AnnAssign)):
            annotation = node.annotation
        if annotation is None:
            continue
        # The annotation only. An `AnnAssign`'s VALUE is evaluated exactly as any other
        # assignment's is, so `SLOT: dict[str, "Pair"] = {"left": …}` loses the `"Pair"`
        # and keeps the `"left"`.
        for part in ast.walk(annotation):
            out.add(id(part))
    return out


#: The `re` functions whose FIRST argument is a pattern. `split` is here and `str.split`
#: is not, which is the whole reason this is keyed on the module: the two share a name and
#: only one of them takes a regex.
_RE_PATTERN_ARG = frozenset({
    "compile", "match", "search", "fullmatch", "sub", "subn", "split", "findall",
    "finditer",
})


def regex_positions(tree: ast.AST) -> set[int]:
    """The ids of the string constants the program compiles as a REGEX.

    Two callers and one reason between them: a pattern's *shape* is not the value
    `retune-string` asks about, so :func:`retune` needs to know which strings are patterns
    (`_regex_shape`), and `mutations_for` needs to know which mutants have to survive
    `re.compile` before they may be offered at all.

    **`re.<fn>(…)` spelled that way, and nothing cleverer.** Not `READERS`, which holds
    `match`, `search` and `split` because `str` has them too — `"a,b".split(",")` takes a
    separator and `re.split(r"[,;]", s)` takes a pattern, and treating the first as a regex
    would hold characters in a string that has no syntax to protect. Not `RE.match(s)`
    either: the pattern there is the receiver's own constant, which was already recognised
    where `re.compile` was called on it, and the argument is the subject.

    The first positional argument only, because that is where every function in
    :data:`_RE_PATTERN_ARG` takes its pattern. A `pattern=` keyword is legal and nobody in
    this tree writes one; missing it costs a question asked in the older, blunter way,
    which the backstop then declines — a bounded loss that says so, rather than a rule
    guessing at an argument's role from its name.
    """
    out: set[int] = set()
    for node in ast.walk(tree):
        if not (isinstance(node, ast.Call) and node.args
                and isinstance(node.func, ast.Attribute)
                and isinstance(node.func.value, ast.Name)
                and node.func.value.id == "re"
                and node.func.attr in _RE_PATTERN_ARG):
            continue
        first = node.args[0]
        if isinstance(first, ast.Constant) and isinstance(first.value, str):
            out.add(id(first))
    return out


def read_positions(tree: ast.AST) -> set[int]:
    """The ids of the string constants whose **value the program reads**.

    `retune-string` is scoped to these, and the scoping is the honest half of the operator.
    A string is a claim in two different ways, and only one of them is a claim a test can
    hold. A key, a comparison operand, a pattern, a separator, a format spec, a table entry
    — those decide what the program *does*, and a test that does not notice one changing is
    a test with a hole in it. A log line decides what the program *says*, and nothing in a
    suite is obliged to assert it.

    **Measured on `charter/` before choosing:** 8,471 string constants are mutable at all,
    of which 2,458 sit in a read position. Mutating all 8,471 took the tree's mutation
    count from 7,006 to 14,801 — more than double, and the difference spent almost entirely
    on prose (2,328 f-string fragments alone). Scoped as below it is 9,319, and on real
    pull requests from this phase the gate goes from 15–35 mutations to roughly 30–50,
    which is still the twenty-odd minutes the cost model was built around. A gate that
    doubles in price to report unasserted log messages is a gate somebody switches off, and
    the spec's own staging argument says the credibility is the deliverable. Widening this
    is one predicate; it should be done with a measurement behind it, as this was.

    Every position below is a property of Python, not of this project. Nothing here was
    chosen because a finding happened to have that shape.

    **The one subtraction, and the same measurement behind it (#632):** a position the
    interpreter never evaluates is not a read position, however read-shaped it looks. A
    forward reference in an annotation is a `Subscript` slice, which is right for
    `d["components"]` and wrong for `list["Pair"]` — see :func:`unevaluated`.

    **Measured over the swept paths, `charter/` and `tools/`:** 15 string constants sit
    inside a deferred annotation. Three of them were read positions, and the subtraction
    takes the read positions from 3,492 to 3,489 and the mutation count from 10,412 to
    10,409. That is a small number and it is the honest one — the other twelve are whole
    annotations (`x: "Path"`), which were never `Subscript` slices and were never offered.
    All three are in this file, and `charter/` has none today.

    So the case for it is not the volume, it is the KIND: each of the three was a
    survivor that no test could ever have killed, and one of those in a report teaches
    the reader to skim the rest. It costs nothing to remove and it is one predicate that
    stops every future signature forward-referencing a class below it from growing one.
    """
    out: set[int] = set()
    inert = unevaluated(tree)

    def take(node: ast.AST | None) -> None:
        if isinstance(node, ast.Constant) and isinstance(node.value, (str, bytes)) \
                and id(node) not in inert:
            out.add(id(node))

    for node in ast.walk(tree):
        # `x == "yes"`, `k in ("a", "b")` — the value decides a branch.
        if isinstance(node, ast.Compare) and all(
                isinstance(o, (ast.Eq, ast.NotEq, ast.In, ast.NotIn, ast.Is, ast.IsNot))
                for o in node.ops):
            for operand in (node.left, *node.comparators):
                take(operand)
                for elt in getattr(operand, "elts", []):
                    take(elt)
        # `{"components": …}` and `d["components"]` — the value selects the data.
        if isinstance(node, ast.Dict):
            for key in node.keys:
                take(key)
        if isinstance(node, ast.Subscript):
            take(node.slice)
        # `d.get("k")`, `re.compile(p)`, `"-".join(xs)`, `"{:<12}".format(n)`.
        if isinstance(node, ast.Call):
            name = node.func.attr if isinstance(node.func, ast.Attribute) else (
                node.func.id if isinstance(node.func, ast.Name) else "")
            if name in READERS:
                for arg in node.args:
                    take(arg)
                if isinstance(node.func, ast.Attribute):
                    take(node.func.value)
        # `"%s says" % x` — the template is machinery, however prose-like it reads.
        if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Mod):
            take(node.left)
        # The `<28` of `f"{name:<28}"`. A width is a layout claim and #508 is what one
        # costs when nothing measures it.
        if isinstance(node, ast.FormattedValue) and node.format_spec is not None:
            for part in ast.walk(node.format_spec):
                take(part)

    # A module-level `NAME = "…"`: a constant with a docstring making a specific claim for
    # its value and nothing measuring it. `MOUSE_ON` and `_MARK` are two of round two's
    # five, and both are scalars — as are `_CHROME_ROWS`, `_SPLIT_ROWS` and `_MIN_TITLE`,
    # the three the integer operator already covers.
    #
    # The VALUE only, deliberately, and not every string inside a container assigned to an
    # upper-case name. Measured on `charter/`: walking into containers takes the read
    # positions from 2,826 to 4,065, and what those 1,239 extra mutations ask for is a test
    # per member of every membership table in the tree. A member is read through the
    # container, not at the assignment — so where one genuinely decides something, the
    # lookup that reads it is a read position on its own and is already covered above.
    for stmt in getattr(tree, "body", []):
        target = None
        if isinstance(stmt, ast.Assign) and len(stmt.targets) == 1 \
                and isinstance(stmt.targets[0], ast.Name):
            target = stmt.targets[0].id
        elif isinstance(stmt, ast.AnnAssign) and isinstance(stmt.target, ast.Name):
            target = stmt.target.id
        if target and target.lstrip("_").isupper():
            take(stmt.value)
    return out


def module_names(tree: ast.AST) -> set[str]:
    """Every name this file binds by importing something.

    A near-synonym pair is justified by two methods living on the same *type*, and a
    module is not an instance of anything. `shlex.split` and `re.split` are functions in a
    namespace that has no `rsplit` at all, so swapping them raises `AttributeError` —
    a red for a reason that has nothing to do with which end was searched, which is a
    false pin, which is the failure this file exists to prevent. Measured on `charter/`:
    eight call sites, all of them `shlex.split` or `re.split`.

    `from x import y` counts too. `y` may be a module or it may be a class, and this side
    cannot tell; skipping a swap that would have been fine costs one mutation, and offering
    one that crashes costs a guard wrongly certified as tested.
    """
    out: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, (ast.Import, ast.ImportFrom)):
            for alias in node.names:
                out.add(alias.asname or alias.name.split(".")[0])
    return out


def _receiver_root(node: ast.AST) -> str:
    """The leftmost name of an attribute chain — `os` in `os.path.split`."""
    while isinstance(node, ast.Attribute):
        node = node.value
    return node.id if isinstance(node, ast.Name) else ""


def _docstrings(tree: ast.AST) -> set[int]:
    """The ids of every docstring constant — prose, not a value, and never mutated."""
    out: set[int] = set()
    for node in ast.walk(tree):
        body = getattr(node, "body", None)
        if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef,
                             ast.AsyncFunctionDef)) and isinstance(body, list) and body \
                and isinstance(body[0], ast.Expr) \
                and isinstance(body[0].value, ast.Constant) \
                and isinstance(body[0].value.value, str):
            out.add(id(body[0].value))
    return out


def _iter_operators(tree: ast.Module, sp: _Spans):
    """Yield ``(node, replacement, operator, question)`` for every recognised shape.

    Each row of this table is a guard one of the three review rounds actually deleted by
    hand. Nothing here is speculative and nothing here is a general-purpose mutation
    engine: a shape earns a place by having caught a real unpinned line.
    """
    parents = _parents(tree)
    docstrings = _docstrings(tree)
    reads = read_positions(tree)
    patterns = regex_positions(tree)
    modules = module_names(tree)
    in_fstring = {id(part) for node in ast.walk(tree) if isinstance(node, ast.JoinedStr)
                  for part in ast.walk(node) if part is not node}

    # A module-level constant is a guard too, and the spec's shape table has no row for
    # one. Five of round two's eighteen overlay findings are exactly this — `_CHROME_ROWS`,
    # `_SPLIT_ROWS`, `_MIN_TITLE`, `MOUSE_ON`, `_MARK` — a number or a string with a
    # docstring making a specific claim for the value, and nothing measuring it. An integer
    # is moved by one and a sum of named parts is dropped to each part; the STRING half of
    # this shape used to be a declared gap ("picking 1003 over 1000 for MOUSE_ON is fitting
    # the answer key") and is now `retune-string` below, whose general form is stated at
    # :func:`retune` — the value moved, every structural property held.
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
            # The question names the term that WENT, not "every term": the mutated node
            # is the whole sum, so both mutants of `A + B` print identically, and until
            # #721 they carried one question too — the `unclamp` shape, where no field at
            # all told a reviewer which of two opposite claims had survived.
            for kept, gone in ((value.left, value.right), (value.right, value.left)):
                yield (value, sp.text(kept), "drop-term",
                       f"is the `{_oneline(sp.text(gone), 40)}` term of `{target}` "
                       f"pinned?")

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
            # "this" and "the other" are two different questions and they are also not
            # self-locating: which is which is knowable only by reading this file, and the
            # reviewer holding the report does not have it open. Naming the direction the
            # condition was forced makes each one checkable against the printed line,
            # which is what #721 asks of a question that has to carry the disambiguation.
            yield (node.test, "False", "disable-branch",
                   "is this branch pinned, or does nothing change when its condition "
                   "never holds?")
            yield (node.test, "True", "disable-branch",
                   "is the rest of the chain pinned, or does nothing change when this "
                   "condition always holds?")

        # The same refusal in expression clothes: `[x for x in xs if C]`. Round two's
        # first finding, `harness_rows`' `if _edge_of(slot) not in _COLUMN_EDGES`, lives
        # inside a `sum(...)` generator, and a shape table that only knows the statement
        # spelling of `if` cannot see it at all.
        # No `and node.ifs` on this line, and its absence is a finding of this tool against
        # itself. It was written, the self-sweep deleted it, and `tests.test_sweep` stayed
        # green: the guard refused to enter a loop over the very list it was testing for
        # emptiness. §4 again — an equivalent mutant and a dead line are one finding.
        if isinstance(node, ast.comprehension):
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
                kept = " and ".join(_spell(sp, v) for v in rest)
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
            # The operand the mutant collapses to, named. This was the worst instance of
            # #721 in the table and the one nothing had tripped over: both mutants of
            # `max(a, b)` print identically AND shared one question, while asking opposite
            # things — `-> a` asks whether anything requires the value to clear the floor,
            # `-> b` whether anything requires the floor at all. No field distinguished
            # them, so a survivor here could not be read at all, only guessed at.
            for arg in node.args:
                yield (node, sp.text(arg), "unclamp",
                       f"is the clamp pinned, or is `{_oneline(sp.text(arg), 40)}` "
                       f"always the answer?")

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
            yield (node, f"{_spell(sp, call.func.value)}[{sp.text(call.args[0])}]",
                   "no-fallback", "is the fallback pinned?")
        if _is_get(node) and len(node.args) == 2:
            yield (node, f"{_spell(sp, node.func.value)}[{sp.text(node.args[0])}]",
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
            # Technically two questions and practically one: "this" and "the other" are
            # only decodable with this function open, and 767 nodes in `charter/` carry
            # the pair. The branch that went is named by keyword and the branch that
            # stayed by its own text, so the report is readable on its own (#721).
            yield (node, sp.text(node.body), "collapse-ifexp",
                   f"is the `else` branch pinned, or is "
                   f"`{_oneline(sp.text(node.body), 40)}` always the answer?")
            yield (node, sp.text(node.orelse), "collapse-ifexp",
                   f"is the `if` branch pinned, or is "
                   f"`{_oneline(sp.text(node.orelse), 40)}` always the answer?")

        # ------------------------------------------------------------------------------
        # String and regex constants (#569). The general form is :func:`retune`.
        # ------------------------------------------------------------------------------
        if isinstance(node, ast.Constant) and isinstance(node.value, (str, bytes)) \
                and id(node) in reads and id(node) not in docstrings:
            raw = node.value if isinstance(node.value, str) \
                else node.value.decode("latin-1")
            moved = retune(raw, regex=id(node) in patterns)
            if moved != raw:
                if id(node) in in_fstring:
                    # A literal segment of an f-string, including a `{x:<28}` FORMAT SPEC,
                    # which is where a width literal actually lives in this tree. Its span
                    # holds no quotes, so the replacement is raw text and not a repr — but
                    # only when the source and the value are the same bytes. If the segment
                    # spells anything with an escape, a brace or a quote, the two disagree
                    # and splicing raw text is a guess; those are dropped rather than
                    # guessed at. Below 3.12 the positions themselves are approximate, and
                    # `span_is_sound` cannot vet an unquoted fragment, so this check is the
                    # only thing standing between the tool and an edit it cannot describe.
                    if isinstance(node.value, str) and sp.text(node) == node.value:
                        yield (node, moved, "retune-string",
                               "is this literal pinned, or would any spelling do?")
                else:
                    rep = repr(moved) if isinstance(node.value, str) \
                        else repr(moved.encode("latin-1"))
                    yield (node, rep, "retune-string",
                           "is this literal pinned, or would any spelling do?")

        # An integer written in a base other than ten was written that way because its
        # digits are the point: `0o600` is a permission, `0x1b` is a byte. So the spelling
        # is the evidence that the value is deliberate, and a deliberate value is a claim.
        # Decimal literals are deliberately NOT mutated here — every `0`, `1` and `2` index
        # in the tree would become a mutation, and the thresholds that matter are reached
        # instead by `shift-boundary`, which asks the same question of `x > 28` without
        # asking it of `xs[0]`.
        if isinstance(node, ast.Constant) and isinstance(node.value, int) \
                and not isinstance(node.value, bool) \
                and sp.text(node)[:2].lower() in ("0o", "0x", "0b"):
            # Re-spelled in its own base, because the report is read by a person: a
            # permission that came back `385` instead of `0o601` is a mutation nobody can
            # check at a glance.
            base = {"0o": oct, "0x": hex, "0b": bin}[sp.text(node)[:2].lower()]
            yield (node, base(node.value + 1), "retune-constant",
                   "is this value pinned, or would any number do?")

        # ------------------------------------------------------------------------------
        # Semantic near-synonyms (#569): one axis moved, everything else held.
        # ------------------------------------------------------------------------------

        # `a < b` -> `a <= b`. The boundary, and only the boundary. A guard written one
        # notch out is the defect this catches, and the whole-condition operators above
        # cannot see it: `drop-if` asks whether the refusal exists at all, which a test
        # that never approaches the edge answers perfectly well.
        if isinstance(node, ast.Compare):
            operands = [node.left, *node.comparators]
            for i, op in enumerate(node.ops):
                swap = BOUNDARY.get(type(op))
                if not swap:
                    continue
                spelled = [f"({sp.text(x)})" for x in operands]
                ops = [CMP_TEXT[type(o)] for o in node.ops]
                ops[i] = swap[1]
                parts = [spelled[0]]
                for o, right in zip(ops, spelled[1:]):
                    parts += [o, right]
                # WHICH boundary, because a chained comparison is one node with two
                # edges: `" " <= c <= "~"` yields two mutants that print identically and,
                # when both edges spell the same operator, carried the same question as
                # well — 8 of the 13 chained comparisons in `charter/` and `tools/` (#721).
                edge = (f"{_oneline(sp.text(operands[i]), 24)} {swap[0]} "
                        f"{_oneline(sp.text(operands[i + 1]), 24)}")
                yield (node, " ".join(parts), "shift-boundary",
                       f"is `{edge}`'s boundary pinned, or only the direction "
                       f"({swap[0]} vs {swap[1]})?")

        # `x.lower()` -> `x.upper()`, `sorted(xs)` -> `list(xs)`, and the rest of
        # :data:`SYNONYMS`. The mutant is type-correct by construction, so a red here is a
        # test noticing the AXIS rather than a test noticing a crash.
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) \
                and node.func.attr in SYNONYMS \
                and _receiver_root(node.func.value) not in modules:
            other, axis = SYNONYMS[node.func.attr]
            # A swap that produces the same program asks nothing, and #655's answer is
            # that such a mutation is not offered rather than given a verdict of its own.
            if not indistinguishable(node.func.attr, other, node):
                yield (node.func, f"{_spell(sp, node.func.value)}.{other}",
                       "swap-synonym",
                       f"is `{axis}` pinned, or does `{other}` pass the same tests?")
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) \
                and node.func.id in SYNONYMS and not node.keywords \
                and len(node.args) == 1 and not any(
                    isinstance(a, ast.Starred) for a in node.args):
            # One positional argument and no keywords, deliberately. `sorted(xs, key=f)`
            # -> `list(xs, key=f)` is a `TypeError`, which reddens the suite for a reason
            # that has nothing to do with the ordering — a false pin, and the false pin is
            # the failure this file exists to prevent.
            other, axis = SYNONYMS[node.func.id]
            yield (node.func, other, "swap-synonym",
                   f"is `{axis}` pinned, or does `{other}` pass the same tests?")

        # `p.resolve()` -> `p`. #572's own shape: a normalisation applied at one site and
        # missing at another, where the two spellings agree on every path a test happens
        # to use and disagree on the one the operator's machine actually has.
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) \
                and node.func.attr in NORMALISERS and not node.args \
                and not node.keywords \
                and _receiver_root(node.func.value) not in modules:
            yield (node, sp.text(node.func.value), "drop-normalise",
                   f"is `{node.func.attr}()` pinned, or does every test use a path that "
                   "needs no normalising?")


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
    # An f-string's literal segment — ` ok` in `f"{n} ok"`, and the `<28` of a `{n:<28}`
    # format spec, which is where a width literal actually lives in this tree. Its span
    # holds no quotes, so it cannot be re-parsed as an expression, and the arms below would
    # refuse every one of them. The round-trip is proved a different way and a stronger
    # one: the bytes at the span ARE the characters of the value, so a splice here replaces
    # exactly the text the mutation claims to replace. A quoted literal never satisfies
    # this — its span includes the quotes and its value does not.
    if isinstance(node, ast.Constant) and isinstance(node.value, str) \
            and text == node.value:
        return True
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


def _not_a_pattern(node: ast.AST, replacement: str, is_pattern: bool) -> str:
    """Why *replacement* may not be offered here, or ``""`` when it may.

    **The backstop, and the reason the shift rules are allowed to be approximate.**
    `_regex_shape` keeps a retuned pattern parseable in the three places this tool knows
    how; a fourth it does not know about would produce a mutant `re.compile` refuses, the
    module would raise on import, every selected test would fail to load, and the sweep
    would report `no verdict` on a question it could have declined to ask. That is #698: a
    `retune-string` on a `[0-9]` pattern produced a `[1-0]` one, and a reviewer had to read a
    CI log and a stack trace to find out that the tool had asked something unanswerable.

    So the two layers hold each other up. The shift rules recover the question where they
    can — measured, 56 patterns to 1 — and this refuses to offer what is left, by name,
    so being wrong about a rule costs a question rather than a verdict.

    `ast.literal_eval` and not a strip of the quotes: *replacement* is a `repr`, and the
    one thing this must not do is guess at its own encoding. Anything it cannot read back
    is not a pattern it can vouch for and is left alone — the mutation goes out as it
    always did, and `re.compile` in the sandbox is nobody's problem but the suite's.
    """
    if not is_pattern:
        return ""
    try:
        value = ast.literal_eval(replacement)
    except (ValueError, SyntaxError):
        return ""
    if not isinstance(value, str):
        return ""
    try:
        re.compile(value)
    except re.error as why:
        return f"the retuned pattern is not one re.compile accepts ({why})"
    return ""


def mutations_for(path: str, source: bytes, lines: set[int]) -> list[Mutation]:
    """Every mutation this file offers on the lines the branch is answerable for."""
    try:
        tree = ast.parse(source, filename=path)
    except SyntaxError:
        return []
    sp = _Spans(source)
    raw = _fstring_segments(tree)
    # The one place a mutant is checked for being a PROGRAM before it is offered (#698).
    # `_regex_shape` keeps a retuned pattern parseable where the tool can see how; this is
    # what happens when it could not — a `\x1f` -> `\x2g` hex escape is the one case left
    # in `charter/` today. Withheld and not dropped: see `Mutation.withheld`.
    patterns = regex_positions(tree)
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
        # Spelled AFTER the no-op check above and in one place for every operator, so
        # that a shape added to the table below cannot forget it. `parenthesised` is a
        # no-op on a replacement that is already tight, which is most of them.
        if id(node) not in raw:
            replacement = parenthesised(replacement)
        key = (node.lineno, node.col_offset, operator, replacement)
        if key in seen:
            continue
        seen.add(key)
        refused = _not_a_pattern(node, replacement, id(node) in patterns)
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
            symbol=_enclosing(tree, node), source=mutated, span=sp.span(node),
            origin=hashlib.sha256(source).hexdigest(), withheld=refused))
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
        # No `if base.exists()`: see :func:`all_lines`. Measured on 3.12.13 and 3.14.4 —
        # deleting the guard left the suite green, because the `rglob` it guarded yields
        # nothing for a directory that is not there.
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


def run_dir(workdir: Path) -> Path:
    """Where THIS run's sandboxes live — one directory per process.

    :func:`workdir_for` gives one workdir per checkout, which is right for the trace cache
    (content-addressed, so sharing it is the whole point) and wrong for the sandboxes. Two
    sweeps of the same checkout — two agents on one box, or one person who forgot the first
    run was still going — would otherwise apply mutations to each other's trees: each one
    restoring a file the other was about to run against, and each one's verdicts about
    bytes neither of them chose.

    **Measured, on this file, by accident.** Two sweeps overlapped and 486 of 489 mutations
    came back `unapplied` — the digest check refusing to produce a verdict from a tree it
    did not recognise. That refusal is what made the collision visible at all; before it,
    the same run would have printed a plausible table of pins and survivors. This is the
    other half of that fix: the collision should not happen in the first place.

    The cache stays shared. It is keyed by a hash of the tree, so two runs of one checkout
    want exactly the same map and paying for it twice is pure loss.
    """
    return workdir / f"run-{os.getpid()}"


class NotApplied(Exception):
    """The mutant tree is not a mutant. See :meth:`Sandbox.apply`."""


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
        """Write the mutant, having first proved this IS the tree the mutation came from.

        The fifth way a sweep lies (#586): the edit does not match — a quoting difference,
        an anchor that moved, a sandbox at the wrong ref — so the "mutant" tree is the
        **unmutated** tree, the suite passes, and the guard is reported as a SURVIVOR. That
        is the only one of the five that errs toward *more* work rather than less, and it
        is the one that ends adoption: somebody writes a test for a line already covered,
        discovers it, and never trusts the tool again.

        Two assertions, and they are different questions. The digest says *this is the file
        I read* — a sandbox at another ref, or a file some earlier restore did not undo,
        fails here rather than producing a verdict about the wrong bytes. The inequality
        says *and the bytes really changed*. Neither is expensive and neither is optional:
        a verdict from an edit that did not happen is not a verdict.
        """
        current = (self.path / mutation.path).read_bytes()
        if mutation.origin and hashlib.sha256(current).hexdigest() != mutation.origin:
            raise NotApplied(
                f"{mutation.path} in this sandbox is not the file the mutation was read "
                f"from, so nothing here would be a verdict about {mutation.tag}")
        self.apply_source(mutation.path, mutation.source)

    def apply_source(self, rel: str, blob: bytes) -> None:
        """Write one file, remembering what was there so :meth:`restore` can undo it."""
        target = self.path / rel
        was = target.read_bytes()
        if was == blob:
            raise NotApplied(f"the mutant of {rel} is byte-identical to the tree it "
                             f"replaces — this edit did not happen")
        self._pristine.setdefault(rel, was)
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
    # Before anything is measured, and before a sandbox is touched: a mutation the plan
    # declined has its verdict already (#698, `Mutation.withheld`). It is answered here
    # rather than filtered out of the plan so that it is sharded, serialised and counted
    # like every other one — a question not asked is a fact about the sweep, and the only
    # way to report it is to carry it.
    if mutation.withheld:
        return "withheld", Outcome(False, 0, mutation.withheld, conclusive=False), None
    # Measured BEFORE the mutation goes anywhere near the tree.
    clean = box.clean_failures(modules) if modules else frozenset()
    try:
        box.apply(mutation)
    except NotApplied as why:
        # Its own outcome, not a survivor and not a pin. This is a defect in the TOOL, and
        # the one thing it must never do is quietly become a finding about the code.
        return "unapplied", Outcome(False, 0, str(why), conclusive=False), None
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


def plan_for(root: Path, ref: str, scope: dict[str, set[int]], dirty: dict[str, bytes]
             ) -> tuple[list[Mutation], dict[str, bytes]]:
    """Every mutation this branch offers, and the sources they were read from.

    Split out of :func:`sweep` so that the gate can ask *how big is this going to be*
    without paying for a single test run — the whole answer is `ast`, and it costs a
    second. #617's plan job runs exactly this and nothing else, which is what lets the
    job that measures decide how many jobs the measuring needs.

    Extracted for the second reason too, the structural one #572 taught: a rule that
    lives inside a caller is not reachable from a test, so it cannot be swept, so it is a
    guard this harness is unable to hold itself to. The order is load-bearing now that
    more than one machine walks this list — see :func:`shard_of` — so it is a property
    something has to be able to assert.
    """
    plan: list[Mutation] = []
    sources: dict[str, bytes] = {}
    for rel, lines in sorted(scope.items()):
        blob = dirty.get(rel) or _blob_at(root, ref, rel)
        if not blob:
            continue
        sources[rel] = blob
        plan.extend(mutations_for(rel, blob, lines))
    return plan, sources


def sweep(root: Path, ref: str, scope: dict[str, set[int]], selection: dict[str, list[str]],
          workdir: Path, jobs: int, dirty: dict[str, bytes], second_order: int = 0,
          log=print, full_timeout: float = FULL_TIMEOUT,
          shard: tuple[int, int] | None = None
          ) -> tuple[list[Result], list["Pair"]]:
    """Every mutation, run; every survivor, re-run against the whole suite."""
    plan, sources = plan_for(root, ref, scope, dirty)
    whole = len(plan)
    if shard is not None:
        plan = shard_of(plan, *shard)
        log(f"  {whole} mutations across {len(scope)} file(s); "
            f"shard {shard[0]} of {shard[1]} takes {len(plan)} of them")
    else:
        log(f"  {whole} mutations across {len(scope)} file(s)")
    if not plan:
        return [], []

    log(f"  building {jobs} sandbox(es)…")
    boxes = [Sandbox(root, run_dir(workdir) / f"w{i}", ref, dirty) for i in range(jobs)]
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
                 "unresolved": "UNRESOLVED", "unapplied": "NOT APPLIED"}[verdict]
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
            try:
                box.apply_source(pair[0].path, combined)
            except NotApplied:
                return None
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
    unapplied = [r for r in results if r.verdict == "unapplied"]
    withheld = [r for r in results if r.verdict == "withheld"]
    pairs = pairs or []
    out: list[str] = []
    w = out.append
    w("=" * 86)
    w(f"deletion sweep — {ref[:12]} against {base[:12]}")
    w("=" * 86)
    w(f"measured on      : {sys.platform}, CPython "
      f"{'.'.join(str(n) for n in sys.version_info[:3])}")
    if reach():
        w(f"NOT ASKED ABOUT  : {reach()}")
    if baseline is not None:
        w(f"baseline          : Ran {baseline.ran} tests — {'OK' if baseline.green else baseline.detail}")
    w(f"mutations applied : {len(results)}")
    w(f"pinned            : {len(pinned)}")
    w(f"SURVIVED          : {len(survivors)}")
    w(f"UNRESOLVED        : {len(unresolved)}")
    if unapplied:
        w(f"NOT APPLIED       : {len(unapplied)}  (a defect in this tool, not a finding)")
    if withheld:
        w(f"WITHHELD          : {len(withheld)}  (questions this tool declined to ask)")
    w(f"wall clock        : {elapsed / 60:.1f} min")
    w("")

    if unapplied:
        w("-" * 86)
        w("NOT APPLIED — these mutations never reached the tree")
        w("-" * 86)
        w("The mutant was byte-identical to the tree it replaced, or the sandbox held a")
        w("different file from the one the mutation was read from. Either way the run that")
        w("followed would have measured the UNMUTATED tree and reported a survivor for a")
        w("line that is already covered. That is a bug here, not a finding about the code,")
        w("and the sweep is not complete until it is zero.")
        for r in sorted(unapplied, key=lambda r: (r.mutation.path, r.mutation.line)):
            m = r.mutation
            w(f"  {m.path}:{m.line}  [{m.operator}]  "
              f"{r.subset.detail if r.subset else ''}")
        w("")

    if withheld:
        w("-" * 86)
        w("WITHHELD — these questions were not asked, and here is why")
        w("-" * 86)
        w("Not a verdict and not a timeout: the mutant would not have been a program, so")
        w("running it would have measured an import error rather than the guard. Each line")
        w("below is one question this sweep is NOT answering — read the count, not the")
        w("silence. A rule in `_regex_shape` is what turns one of these back into a")
        w("question that can be asked.")
        for r in sorted(withheld, key=lambda r: (r.mutation.path, r.mutation.line)):
            m = r.mutation
            w(f"  {m.path}:{m.line}  [{m.operator}]  {m.withheld}")
            w(f"    shipped : {_oneline(m.before, 70)}")
            w(f"    mutant  : {_oneline(m.after, 70)}")
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
        if unresolved or unapplied:
            w("No survivor — but see above: not every mutation was measured, so this is")
            w("not the same claim as a clean sweep.")
        else:
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
        "withheld": r.mutation.withheld,
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
        # `null` and not `[]` when the evidence pass never ran, because the two say
        # different things and the report prints them differently: "nothing measured
        # executes this file" against "N modules execute it and not one names the symbol".
        # A shard writes this file and another machine reads it (#617), so an absence that
        # arrives as an empty list is an absence that arrives as the wrong answer.
        "naming": None if r.evidence is None else [
            {"module": m, "test": t, "asserts": a} for m, t, a in r.evidence.naming],
    } for r in results], indent=1)


# --------------------------------------------------------------------------------------
# 8. The gate — stage C
# --------------------------------------------------------------------------------------

@dataclasses.dataclass
class Gate:
    """One sweep, sorted into the outcomes a gate is allowed to act on differently.

    The buckets are not cosmetic and they are not a severity ranking. Each one is a
    different *kind of claim*, and collapsing any two of them is a way this gate gets
    switched off within a week:

    * ``unpinned`` — a guard with no test behind it. The finding. Actionable.
    * ``masked`` — two or more survivors inside one function. Also actionable, and MORE
      urgent, not less: two guards in sequence mask each other, so none of them can be
      called equivalent on its own. Separated because the advice differs — these are read
      together or not at all.
    * ``platform`` — a narrowed catch on an exception type the operating system decides.
      Measured on this project: `except OSError` around a pty read is dead code on macOS
      and live on Linux. **The gate never fails on one of these.** A gate that fails a pull
      request for a clause the runner's kernel cannot reach is a gate somebody disables,
      and they will be right to.
    * ``unresolved`` — no verdict. A timeout is not a red and not a survivor. Under load
      this repository hits it repeatedly, and "I could not look" must never render as
      "nothing to see".
    * ``unapplied`` — the mutation never reached the tree (#586). A defect in the tool.
    * ``withheld`` — the mutation was never offered, because the mutant was not a program
      (#698). **Its own bucket and not folded into `unresolved`**, because the two say
      opposite things about what the tool knows: `unresolved` is "I looked and could not
      tell", and this is "I decided not to ask, and here is why". Collapsing them would
      make a deliberate, bounded, explained subtraction read as a timeout — which is how
      #693 came to sit behind a `no verdict` that no re-run could ever clear. It never
      fails the gate, for the same reason `reach()` does not: a question not asked is not
      a finding about the branch. It is loud in the report so that it is not a finding
      about nothing either.
    """

    unpinned: list[Result] = dataclasses.field(default_factory=list)
    masked: list[Result] = dataclasses.field(default_factory=list)
    platform: list[Result] = dataclasses.field(default_factory=list)
    unresolved: list[Result] = dataclasses.field(default_factory=list)
    unapplied: list[Result] = dataclasses.field(default_factory=list)
    withheld: list[Result] = dataclasses.field(default_factory=list)
    pinned: int = 0

    @property
    def actionable(self) -> list[Result]:
        """Survivors a person should act on. Platform-deferred ones are not among them."""
        return self.unpinned + self.masked


def classify(results: list[Result]) -> Gate:
    """Sort a sweep into :class:`Gate`'s buckets."""
    gate = Gate()
    crowded: dict[tuple[str, str], int] = {}
    for r in results:
        if r.verdict == "survived":
            key = (r.mutation.path, r.mutation.symbol)
            crowded[key] = crowded.get(key, 0) + 1
    for r in results:
        if r.verdict == "pinned":
            gate.pinned += 1
        elif r.verdict == "unresolved":
            gate.unresolved.append(r)
        elif r.verdict == "unapplied":
            gate.unapplied.append(r)
        elif r.verdict == "withheld":
            gate.withheld.append(r)
        elif r.verdict == "survived":
            if platform_caveat(r.mutation):
                gate.platform.append(r)
            elif crowded.get((r.mutation.path, r.mutation.symbol), 0) > 1:
                gate.masked.append(r)
            else:
                gate.unpinned.append(r)
    return gate


def gate_exit_code(gate: Gate, enforce: bool) -> int:
    """What a gate run exits with. **Zero until somebody turns it on.**

    The spec's staging argument — "a gate whose baseline nobody has seen gets disabled the
    first time it is inconvenient" — applies to the gate's own credibility as much as to
    the tree's. So the first version of this job reports its numbers on every pull request
    and blocks nothing; `--enforce` is the one flag that changes that, and it should be set
    only once the numbers on real branches have been looked at and believed.

    When it does enforce, the codes are distinct because the responses are:

    * **1** — an actionable survivor. Write the test, or delete the line.
    * **3** — nothing actionable, but something could not be measured. Re-run it; do not
      read it as clean.
    * **4** — a mutation never applied. The tool is wrong, and its numbers are not
      evidence about this branch either way.
    """
    if not enforce:
        return 0
    if gate.unapplied:
        return 4
    if gate.actionable:
        return 1
    return 3 if gate.unresolved else 0


# --------------------------------------------------------------------------------------
# 8a. Sizing the gate — how many jobs this branch's mutations need (#617)
# --------------------------------------------------------------------------------------
#
# Two of the last three substantial branches could not finish the gate at all: #608's 62
# mutations were cancelled at `timeout-minutes: 60` twice, and #626's 78 were cancelled
# three times, each run reaching roughly 53 of them. The job had been sized against the
# 30–52 that Phase 2's branches produced, and `retune-string` roughly doubles the count on
# any diff that touches string constants — which a guard table and a config reader are
# made of almost nothing else.
#
# The one answer that was never available is "sweep fewer of them". A cap on the mutation
# count reads, to everyone downstream, as "covered everything" — the spec says so about
# silent truncation and it is the same lie the whole harness exists to refuse. So NOTHING
# below drops a mutation. The numbers here size the *fan-out*: how many machines the same
# complete question is spread across.

#: What one shard is allowed to spend on mutations and fixed costs together, in seconds.
#: Deliberately under `sweep.yml`'s `timeout-minutes`, because the two failures are not
#: symmetrical: a shard that finishes with time to spare costs a few runner-minutes, and a
#: shard cancelled at the cap reports *nothing at all* — not a partial answer, not the
#: survivors it had already printed, nothing the merge step can read.
SHARD_BUDGET = 40 * 60

#: What a shard pays before it measures its first mutation, itemised, in seconds. Measured
#: on `ubuntu-latest` and not on a workstation, because that is where the budget has to
#: hold.
#:
#: **Data and not a bulleted comment, which is what #670 cost.** The largest of these was
#: written in prose here and written again in `sweep.yml`'s cache step, and the two copies
#: said 250 and 350 for a month with nothing able to notice: a figure two files state in
#: words drifts, and neither copy was in reach of an assertion. This dict is the one place
#: it is written, `test_sweep` holds the workflow's comments to it, and `sweep.yml` names
#: the constant instead of quoting a number.
#:
#: The map figure is nine cache-miss traces on `ubuntu-latest` (242, 252, 257, 276, 277,
#: 279, 280, 282, 285 s — the tool prints its own `selection map: … in Ns`), taken at the
#: ceiling because a budget is sized against the slow run. 250 was honest when #630 wrote
#: it against 7,693 tests and the suite has grown since; 350 never matched a run.
#:
#: The baseline is the whole suite once, the same run `test.yml` makes in about four
#: minutes; the clone is the sandbox `git clone` of this checkout.
SHARD_FIXED_COSTS = {
    "checkout at fetch-depth 0, and the interpreter": 3,
    "the selection map, traced": 285,
    "the sandbox clone": 15,
    "the unmutated baseline": 240,
}

#: What a shard pays before it measures its first mutation, as one number.
#:
#: The baseline is the expensive half and it is not negotiable: a shard that skips it
#: cannot tell a survivor from a tree that was already red, which is the spec's second way
#: a sweep lies. So every shard buys its own — the marginal cost of a machine.
#:
#: The map is the half that CAN be shared, and `sweep.yml` warms it once for all of them;
#: restored from that cache it costs under 5 s. This constant is deliberately the NO-CACHE
#: figure anyway, and rounded up past even `sum(SHARD_FIXED_COSTS.values())`: a pull
#: request from a fork cannot write that cache, and a budget that only holds when the cache
#: hits is a budget that fails on exactly the runs nobody is watching.
SHARD_FIXED = 12 * 60

#: What one mutation costs a shard. Read off the runs that ran out of time rather than off
#: a workstation: #626 reached about 53 of its 78 inside the hour, and the fixed cost of a
#: run with no map cache was about 515 s, which leaves **58 s** each. A workstation
#: measures 24 s at `--jobs 3`; CI is slower and the gate must be sized against CI.
#:
#: Together: 78 mutations over the three shards this sizing asks for is 26 each, so
#: 515 + 26×58 ≈ 34 min with no cache and under 30 with one — inside the budget, and well
#: inside `sweep.yml`'s hour.
SECONDS_A_MUTATION = 60

#: The most jobs one pull request may fan out to. Not a limit on what gets swept — every
#: mutation is still dealt to a shard past this point, they just get more each — but a
#: limit on how much of the runner pool one branch may hold at once.
MAX_SHARDS = 8


def per_shard() -> int:
    """How many mutations one shard can measure inside :data:`SHARD_BUDGET`."""
    return max(1, (SHARD_BUDGET - SHARD_FIXED) // SECONDS_A_MUTATION)


def shards_for(mutations: int) -> int:
    """How many jobs *mutations* of them need, so that none of them runs out of time.

    At least one, always: a branch with nothing to sweep still gets a job, because "the
    sweep ran and found nothing to do" and "the sweep did not run" are the two answers
    #617 is about and they must not arrive as the same silence.
    """
    return max(1, min(MAX_SHARDS, -(-mutations // per_shard())))


def over_budget(mutations: int) -> str:
    """Why this diff will be slow, said out loud, or ``""`` when it will not be.

    :data:`MAX_SHARDS` is a ceiling on machines and never on questions, so a diff past it
    is still swept whole — its shards simply carry more than the budget and some of them
    may be cancelled. That is a real risk to the run and it gets said here rather than
    discovered in a cancelled job: the spec's rule about silent truncation is that a cap
    the reader cannot see is worse than the cap.
    """
    ceiling = MAX_SHARDS * per_shard()
    if mutations <= ceiling:
        return ""
    return (f"{mutations} mutations is past the {ceiling} that {MAX_SHARDS} shards can "
            f"measure inside {SHARD_BUDGET // 60} minutes each. Every one of them is "
            f"still swept — nothing here is dropped — but a shard may be cancelled at "
            f"the job timeout, and a cancelled shard reports no verdict rather than a "
            f"short one.")


def shard_of(plan: list[Mutation], index: int, count: int) -> list[Mutation]:
    """Slice *index* of *count*, dealt one mutation at a time across the whole plan.

    **Dealt, and not cut by file** — which is the correction the numbers forced on #617's
    own proposal. Sharding by file sounds right and does nothing here: both diffs that ran
    out of time were dominated by a *single new file*, so a job-per-file hands one job all
    78 of `gitconfig.py`'s mutations and the other jobs nothing. Round-robin also spreads
    the expensive ones, and they arrive in clumps — a survivor costs a full suite run
    where a red costs seconds, and survivors cluster inside the function that has no test.

    *index* is 1-based because it is a job number a person reads on a check ("shard 2 of
    3"), and off-by-one here does not fail loudly: it silently sweeps one slice twice and
    another not at all, and the merge step would report a complete sweep of an incomplete
    plan. So it is refused rather than clamped.
    """
    # One comparison and not two: `count < 1` as a separate clause is unreachable, because
    # no *index* can satisfy `1 <= index <= 0` either. A second guard that can never be the
    # one that fires is a line the suite would not miss, and this file's own rule is that
    # such a line gets deleted rather than kept for shape.
    if not 1 <= index <= count:
        raise ValueError(f"shard {index} of {count} is not a shard")
    return plan[index - 1::count]


# --------------------------------------------------------------------------------------
# 8b. The conclusion — the three things a gate run can have found (#617)
# --------------------------------------------------------------------------------------
#
# A gate that blocks nothing still has to SAY something, and until #617 this one did not.
# It reported `success` on a branch with eight survivors under it, which to anyone who
# does not open the run summary reads as "this branch is clean" — the exact opposite of
# what the job exists to say. A run that was cancelled at the timeout reported `cancelled`,
# which reads as infrastructure noise rather than as a missing answer.
#
# **GitHub's vocabulary cannot express this, and that is why the answer is not a
# conclusion.** A job driven by a `run:` step concludes `success` or `failure` and nothing
# else; `neutral` — the one conclusion that means "I looked, here is what I found, this is
# not a pass/fail" — exists only on the Checks API, which needs `checks: write` and a
# check run created by hand. `sweep.yml`'s header refuses that in as many words: this job
# runs the repository's own code against mutated copies of it, and is the last place in
# the repository to hold a writable token. Trading that for a prettier icon is not a trade
# worth making.
#
# So the conclusion stays `success` and the *check's name* carries the answer. A job name
# can interpolate `needs.<job>.outputs.*`, so a one-step job that does nothing but exist
# renders on the pull request as `deletion sweep / 8 survivors` — green, blocking nothing,
# and no longer silent. The survivors additionally arrive as annotations on the lines they
# are about, which is where a reviewer is already looking.

#: Completed, and every mutation this branch offered goes red.
CLEAN = "clean"
#: Completed, and N of them do not. The interesting case, and the one that was
#: indistinguishable from `CLEAN` on the pull request until #617.
SURVIVORS = "survivors"
#: Did not complete. A shard that never reported, a mutation that timed out, or an edit
#: that never reached the tree — three spellings of the same thing, which is that the
#: numbers on this page are not an answer about this branch.
NO_VERDICT = "no-verdict"


def gate_conclusion(gate: Gate, missing: int = 0) -> str:
    """Which of the three a run found. *missing* is shards that never reported.

    The precedence is :func:`gate_exit_code`'s, deliberately — 4 outranks 1 outranks 3 —
    because the two answers are the same answer in two vocabularies and a branch that
    disagreed with itself about which would be worse than either. A shard that never
    reported joins `unapplied` at the top: both mean the counts below are not evidence.

    Survivors outrank an unresolved mutation and do not outrank a missing shard. That is
    not a severity ranking, it is what each one licenses a reader to believe: "8
    survivors, 2 not measured" is still a true statement about 8 real findings, whereas
    "8 survivors" from two thirds of a plan is a number nobody should quote.
    """
    if missing or gate.unapplied:
        return NO_VERDICT
    if gate.actionable:
        return SURVIVORS
    return NO_VERDICT if gate.unresolved else CLEAN


def _plural(n: int, one: str, many: str) -> str:
    return f"{n} {one if n == 1 else many}"


def headline(gate: Gate, missing: int = 0, shards: int = 1) -> str:
    """The answer in one line, short enough to be a check's NAME.

    This string is the whole fix for "a green gate is not no survivors": it is what the
    pull request's check list says next to the tick, so the count is readable without
    opening a run summary, downloading an artifact, or knowing the job exists.

    *shards* below one means the run never got as far as deciding how many it needed, and
    that is said as itself rather than as "1 of 1 did not report" — a denominator nobody
    computed is not a denominator, and inventing one here would describe a sweep that was
    never planned as a sweep that was planned and lost.
    """
    conclusion = gate_conclusion(gate, missing)
    # Said in the check's NAME when there is one, and said BESIDE "no survivors" rather
    # than instead of it (#698). A withheld mutation does not put the branch in doubt —
    # nothing about it was measured and found wanting, and nothing about it could not be
    # measured either; the tool declined to ask and can say why. Turning a clean sweep
    # into `no verdict` over that would spend the one signal a reviewer must stop on, and
    # #693 is what it costs when `no verdict` stops meaning "look at this".
    aside = f", {len(gate.withheld)} withheld" if gate.withheld else ""
    if conclusion == CLEAN:
        return f"no survivors{aside}"
    found = _plural(len(gate.actionable), "survivor", "survivors")
    unsure = []
    if missing:
        unsure.append(f"{missing} of {_plural(shards, 'shard', 'shards')} did not report"
                      if shards >= 1 else "the sweep never sized itself")
    if gate.unapplied:
        unsure.append(f"{_plural(len(gate.unapplied), 'mutation', 'mutations')} never "
                      "applied")
    if gate.unresolved:
        unsure.append(f"{len(gate.unresolved)} not measured")
    if conclusion == SURVIVORS:
        return (f"{found}, {'; '.join(unsure)}{aside}" if unsure else f"{found}{aside}")
    if gate.actionable:
        return f"no verdict: {found} so far, {'; '.join(unsure)}{aside}"
    return f"no verdict: {'; '.join(unsure)}{aside}"


#: How many annotations of one LEVEL, per step, GitHub will draw before it stops. Not per
#: kind of finding — three of the families below are all warnings, and they share this one
#: budget. Going past it is not an error and produces no warning: the extras are simply
#: not there, which is the silent-truncation shape again, so the cap announces itself in
#: the annotation stream — and reserves one of its own slots to do it, because the note
#: saying "and eleven more" is otherwise the first thing the cap eats.
ANNOTATION_CAP = 10


def _escape(text: str) -> str:
    """A workflow command's message, with the three characters that would end it early."""
    return text.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def _property(text: str) -> str:
    """A workflow command's *property* value — two more characters than a message.

    A `,` starts the next property and a `:` can close the property list early, so a file
    path with either in it would silently annotate the wrong place, or nothing. Escaped
    rather than trusted: these are paths and questions out of the tree, not constants.
    """
    return _escape(text).replace(":", "%3A").replace(",", "%2C")


def _annotation(level: str, r: Result, title: str, body: str) -> str:
    m = r.mutation
    return (f"::{level} file={_property(m.path)},line={m.line},endLine={m.end_line},"
            f"title={_property(title)}::{_escape(body)}")


def annotations(gate: Gate) -> list[str]:
    """One workflow command per finding, so a survivor lands ON THE LINE it is about.

    The check's name says how many there are; this says where. A reviewer reading the
    diff sees the marker in the margin of the guard itself, which is the one place the
    question "did my test look closely enough" can actually be answered — and none of it
    changes the job's conclusion, so the gate goes on blocking nothing.

    The families are in the order they should survive :data:`ANNOTATION_CAP`, because they
    share a level's budget and the ones past it are not drawn: a masked cluster before a
    lone survivor, because two guards hiding each other is the finding a reviewer is least
    able to reach on their own; both before an unmeasured mutation, which is a fact about
    the runner rather than about the branch.
    """
    families = (
        ("error", gate.unapplied, "Sweep defect — the mutation never applied",
         "The edit never reached the tree, so the run that followed measured the "
         "UNMUTATED file and would have called this a survivor. Every number on this "
         "sweep is suspect until it is zero."),
        ("warning", gate.masked, "Masked cluster — two guards hiding each other",
         "Another survivor shares this function. Two guards in sequence each look "
         "equivalent alone, so neither is pinned and they are read together."),
        ("warning", gate.unpinned, "Unpinned guard — no test goes red without this line",
         "Deleting it leaves the suite green. Write the test, or delete the line — "
         "\"equivalent mutant\" and \"dead code\" are one finding."),
        ("warning", gate.unresolved, "No verdict — this mutation was never measured",
         "The run timed out rather than failing. That is not a red and it is not a pin; "
         "nothing here has been shown to be tested."),
        ("notice", gate.platform, "Platform-deferred — never fails this gate",
         f"A narrowed catch on an exception the operating system decides. On "
         f"{sys.platform} the clause may be unreachable rather than untested, and one "
         f"machine cannot tell those apart."),
    )
    total: dict[str, int] = {}
    for level, results, _, _ in families:
        total[level] = total.get(level, 0) + len(results)
    # One slot back when a level is over budget, for the note that says so. Spending all
    # ten on findings and adding the note as an eleventh loses the note — which is the
    # one line that has to arrive, because it is the difference between "these ten" and
    # "these ten of twenty-two".
    room = {level: ANNOTATION_CAP - 1 if n > ANNOTATION_CAP else ANNOTATION_CAP
            for level, n in total.items()}
    out: list[str] = []
    drawn: dict[str, int] = {}
    for level, results, title, body in families:
        for r in sorted(results, key=lambda r: (r.mutation.path, r.mutation.line)):
            if drawn.get(level, 0) >= room[level]:
                break
            drawn[level] = drawn.get(level, 0) + 1
            out.append(_annotation(level, r, title, f"{r.mutation.question} — {body}"))
    for level, n in total.items():
        if n > ANNOTATION_CAP:
            out.append(f"::{level} title=Not every finding is drawn here::" + _escape(
                f"{n - room[level]} of these {n} are not shown — GitHub draws "
                f"{ANNOTATION_CAP} annotations of one level per step and then stops "
                f"without saying so. The run summary lists all {n}."))
    return out


def _summary_rows(results: list[Result]) -> list[str]:
    """One markdown bullet per survivor, carrying WHAT THE COVERING TESTS ASSERT.

    That field is what made 82 survivors triageable rather than merely alarming, and it is
    the difference between a gate that gets read and a gate that gets muted. `release.yml`'s
    `-z "$claimed"` refusal (#558) is why: deleting it left the run still exiting 1, for a
    different reason, so the honest first question about a survivor is "did my test look
    closely enough" — which nobody can answer from a line number alone.
    """
    out: list[str] = [""]
    for r in sorted(results, key=lambda r: (r.mutation.path, r.mutation.line)):
        m = r.mutation
        out.append(f"- **`{m.path}:{m.line}`** in `{m.symbol}` — _{m.question}_")
        out.append(f"  - shipped: `{_oneline(m.before, 100)}`")
        out.append(f"  - mutant `[{m.operator}]`: "
                   f"`{_oneline(m.after, 100) or '(the statement, deleted)'}`")
        ev = r.evidence
        if ev is None or not ev.modules:
            out.append("  - covered by: **nothing measured executes this file**")
        elif ev.nothing_names_it:
            out.append(f"  - covered by: {len(ev.modules)} module(s) execute this file and "
                       f"**not one names `{m.symbol}`**")
        else:
            asserted = [a for _, _, asserts in ev.naming for a in asserts]
            out.append(f"  - covered by: {len(ev.naming)} test(s) naming `{m.symbol}` — "
                       + ", ".join(f"`{module.split('.')[-1]}.{name}`"
                                   for module, name, _ in ev.naming[:3]))
            for a in asserted[:3]:
                out.append(f"    - asserts `{a}`")
    return out


def gate_summary(gate: Gate, ref: str, base: str, elapsed: float | None,
                 enforce: bool, missing: int = 0, shards: int = 1) -> str:
    """The markdown a reviewer reads on the pull request."""
    out: list[str] = []
    w = out.append
    verdict = ("**would fail**" if gate.actionable or gate.unapplied
               else "**would pass**")
    # The headline and not the count, and the same string the check is NAMED with. A page
    # whose title disagreed with the row on the pull request would be worse than either of
    # them alone, so there is one sentence and both readers get it.
    w(f"## Deletion sweep — {headline(gate, missing, shards)}")
    w("")
    # No wall clock when the answer was merged rather than measured: the step that adds
    # up several machines' results took a second and did not spend the sweep's time, and
    # printing its own second there would understate a forty-minute run by two orders of
    # magnitude on the one line a reader skims.
    took = f"{elapsed / 60:.1f} min" if elapsed is not None else "merged from its shards"
    w(f"`{ref[:12]}` against `{base[:12]}`, added lines only, "
      f"{took} on {sys.platform} / CPython "
      f"{'.'.join(str(n) for n in sys.version_info[:3])}.")
    w("")
    w("| outcome | n | what it means |")
    w("|---|---:|---|")
    if missing:
        w(f"| **did not report** | {f'{missing} of {shards}' if shards >= 1 else '?'} | "
          "a shard that never wrote a result — read nothing below as a count |")
    w(f"| pinned | {gate.pinned} | a test goes red without the line |")
    w(f"| **unpinned** | {len(gate.unpinned)} | a guard with no test behind it |")
    w(f"| **masked cluster** | {len(gate.masked)} | two or more in one function; "
      "none is safe to call equivalent alone |")
    w(f"| platform-deferred | {len(gate.platform)} | the clause may be unreachable on "
      f"{sys.platform}; never fails this gate |")
    w(f"| unresolved | {len(gate.unresolved)} | no verdict — timed out, not measured |")
    if gate.withheld:
        w(f"| withheld | {len(gate.withheld)} | the mutant was not a program, so the "
          "question was not asked |")
    if reach():
        w(f"| not asked about | — | {reach()} |")
    w(f"| not applied | {len(gate.unapplied)} | the edit never reached the tree — a bug "
      "in the sweep |")
    w("")
    if not enforce:
        w(f"> **Reporting only.** This job blocks nothing; it {verdict} with `--enforce`. "
          "The spec's own staging argument says a gate whose numbers nobody has seen gets "
          "disabled the first time it is inconvenient, so the numbers come first.")
        w("")
    if gate.withheld:
        w("### Withheld — questions this sweep did not ask")
        w("")
        w(f"{len(gate.withheld)} mutation(s) were planned and then declined: the mutant "
          "would not have been a program, so running it would have measured an import "
          "error rather than the guard. This is **not** `no verdict` — the tool knows "
          "what it did and why — but it is a question nobody answered, and a sweep that "
          "subtracted questions in silence would read exactly like a clean one.")
        w("")
        w("| line | operator | why |")
        w("|---|---|---|")
        for r in sorted(gate.withheld, key=lambda r: (r.mutation.path, r.mutation.line)):
            m = r.mutation
            w(f"| `{m.path}:{m.line}` | `{m.operator}` | {m.withheld} |")
        w("")
    if missing:
        w("### Did not report — this page is not a count")
        w("")
        if shards >= 1:
            w(f"{missing} of {shards} shard(s) wrote no result. A shard is cancelled at "
              "the job timeout rather than stopping short, so what it had already "
              "measured is gone with it — the tables below are the shards that *did* "
              "answer, and a branch with survivors in the missing slice looks exactly "
              "like this one. Re-run the sweep; do not read it as clean.")
        else:
            w("The sweep **never said how many shards it needed**, so nothing here knows "
              "how much of this branch was measured, or whether any of it was. That is "
              "the job that sizes the sweep failing before it sized anything — and it is "
              "**no verdict**, not a clean one.")
        w("")
    if gate.unapplied:
        w("### Not applied — read nothing else on this page until this is zero")
        w("")
        w("A mutation whose edit never landed runs the **unmutated** tree, passes, and is "
          "reported as a survivor. Every number above is suspect while this is non-zero.")
        for r in gate.unapplied:
            w(f"- `{r.mutation.tag}` — {r.subset.detail if r.subset else ''}")
        w("")
    if gate.masked:
        w("### Masked cluster")
        w("")
        w("Two guards in sequence hide each other, so each one looks equivalent on its "
          "own and neither is pinned. Read these together.")
        out.extend(_summary_rows(gate.masked))
        w("")
    if gate.unpinned:
        w("### Unpinned")
        w("")
        w("Delete the line and the suite stays green. Either write the test, or delete the "
          "line — there is no suppression list, and \"equivalent mutant\" and \"dead code\" "
          "are one finding.")
        out.extend(_summary_rows(gate.unpinned))
        w("")
    if gate.platform:
        w("### Platform-deferred (not a gate failure)")
        w("")
        w(f"A narrowed catch on an exception the operating system decides. On "
          f"{sys.platform} the clause may never be entered at all, which is *unreachable* "
          "rather than *untested*, and the two are indistinguishable from one machine.")
        out.extend(_summary_rows(gate.platform))
        w("")
    if gate.unresolved:
        w("### Unresolved — no verdict")
        w("")
        w("The run timed out rather than failing, twice. That is not a red and it is "
          "emphatically not a pin. Re-run on a quieter machine before reading this sweep "
          "as clean.")
        for r in gate.unresolved:
            w(f"- `{r.mutation.tag}` in `{r.mutation.symbol}`")
        w("")
    if gate_conclusion(gate, missing) == CLEAN:
        w("Every mutation this branch offered goes red. Nothing added here is a line the "
          "suite would not miss.")
    return "\n".join(out)


# --------------------------------------------------------------------------------------
# 8c. Merging shards back into one answer (#617)
# --------------------------------------------------------------------------------------

def results_from_json(payload: str) -> list[Result]:
    """A shard's results, read back from what :func:`as_json` wrote.

    The gate is one question answered on several machines, so the answer has to survive a
    round trip through a file — and everything :func:`classify` and :func:`_summary_rows`
    read has to survive it, not merely the verdict string. `platform_caveat` is recomputed
    from the operator and the shipped source rather than trusted from the file, so a shard
    that ran on a different platform than the merge cannot smuggle its own answer in.
    """
    def outcome(o: dict | None) -> Outcome | None:
        return None if not o else Outcome(o["green"], o["ran"], o["detail"])

    out: list[Result] = []
    for row in json.loads(payload):
        m = Mutation(path=row["path"], line=row["line"], end_line=row["end_line"],
                     operator=row["operator"], question=row["question"],
                     before=row["before"], after=row["after"], symbol=row["symbol"],
                     # `.get`, and it is the one field read that way: a shard built by an
                     # older sweep than the merge has no such key, and a merge that raised
                     # on it would turn a mixed-version run into no report at all.
                     withheld=row.get("withheld", ""))
        # `null` means the evidence pass never ran, which is not the same as running and
        # finding nothing: `_summary_rows` tells "nothing measured executes this file"
        # from "N modules execute it and not one names the symbol" by exactly that field,
        # and a round trip that flattened the two would report the wrong one.
        evidence = None
        if row["naming"] is not None:
            evidence = Evidence(row["modules"],
                                [(n["module"], n["test"], n["asserts"])
                                 for n in row["naming"]])
        out.append(Result(m, row["verdict"], outcome(row["subset"]), outcome(row["full"]),
                          row["modules"], evidence))
    return out


def merge(directory: Path, shards: int) -> tuple[list[Result], int]:
    """Every shard's results, and how many shards did not report.

    Counted, not named: the merge asks *how many of the answers arrived*, and answering
    that by parsing shard numbers out of filenames would put the workflow's shell quoting
    and this function's regex in a position to disagree — which is a way to report a
    complete sweep of an incomplete plan. A file is one shard's answer if it parses, and
    a file that will not parse is a shard that did not report; there is no third reading
    of a truncated upload.
    """
    results: list[Result] = []
    reported = 0
    for f in sorted(Path(directory).glob("*.json")) if Path(directory).is_dir() else []:
        try:
            results.extend(results_from_json(f.read_text(encoding="utf-8")))
        except (OSError, ValueError, KeyError, TypeError):
            continue
        reported += 1
    return results, max(0, shards - reported)


def verdict_exit_code(gate: Gate, missing: int, enforce: bool) -> int:
    """What the merge step exits with. **Zero until somebody turns it on**, as before.

    A shard that never reported is the same answer as a mutation that never resolved —
    *no verdict*, exit 3 — and for the same reason: the run has not shown the branch to
    be clean, and "I could not look" must not render as "nothing to see".
    """
    code = gate_exit_code(gate, enforce)
    return 3 if enforce and missing and code == 0 else code


# --------------------------------------------------------------------------------------
# 9. CLI
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


def base_for(root: Path, ref: str, override: str | None) -> str:
    """What the branch is charged against: the merge-base, not the tip.

    Extracted from `main()` for the reason #572 taught: **a rule that lives inside
    `main()` is not reachable from a test, so it cannot be swept, so it is a guard the
    harness is structurally unable to hold itself to.** `workdir_for` had to come out of
    `main()` before the bug that made this tool unusable on macOS could be pinned. This
    function, `dirty_for`, `timeout_for` and `exit_code` are the same move applied to the
    rest of the CLI, and the self-sweep's "renderer + CLI, ~0 of 49" row is what that
    hazard looks like when nobody applies it.

    `origin/main` when the remote is there, `main` when it is not — a fresh clone, a
    worktree, and CI all differ on that. The merge-base and not the tip, because a branch
    is answerable for what IT added and not for what main gained while it was open.
    """
    if override:
        return git("rev-parse", override, cwd=root).strip()
    upstream = "origin/main" if git("rev-parse", "--verify", "--quiet", "origin/main",
                                    cwd=root, check=False).strip() else "main"
    return git("merge-base", ref, upstream, cwd=root).strip()


def dirty_for(root: Path, paths: tuple[str, ...], ref_arg: str) -> dict[str, bytes]:
    """Uncommitted work, but only when the sweep is about the working tree.

    `--ref HEAD` means "what I have here", so the sandboxes carry what is not committed
    yet — that is what makes the tool usable before a commit. Any other ref names a
    specific historical tree, and pouring today's uncommitted files into it would sweep a
    tree that has never existed and report findings against it.
    """
    return dirty_files(root, paths) if ref_arg == "HEAD" else {}


def timeout_for(measured: float) -> float:
    """The full-suite cap, taken from THIS machine's own baseline rather than a constant.

    Six times what the suite just took, never below the floor. A fixed forty minutes is
    generous on an idle box and far too tight on a shared one: measured at a load average
    of 100, a five-minute suite ran past 2400 s and two known-unpinned guards came back
    "pinned" on the strength of a stopwatch. Six, and not two, because the mutants that
    matter most are exactly the ones that make the suite slow.
    """
    return max(FULL_TIMEOUT, measured * 6)


def parse_shard(text: str) -> tuple[int, int]:
    """``"2/3"`` as ``(2, 3)``. Anything else is refused rather than guessed at.

    A shard argument that parses loosely is a shard that runs the wrong slice, and a
    wrong slice does not fail: it sweeps some mutations twice and others never, and the
    merge step reports a whole plan. So `2`, `2/`, `0/3` and `4/3` are all errors here,
    where they would all be survivable further in.
    """
    index, sep, count = str(text).partition("/")
    if not sep or not index.strip().isdigit() or not count.strip().isdigit():
        raise ValueError(f"--shard wants N/M, not {text!r}")
    pair = (int(index), int(count))
    if not 1 <= pair[0] <= pair[1]:
        raise ValueError(f"--shard {text} is not a shard of anything")
    return pair


def expected_shards(text: str | None) -> int:
    """How many shards the plan asked for, or ``0`` when the plan never said.

    An empty value is not "no shards were needed" — it is a plan job that never got as
    far as sizing itself, which is the loudest failure this workflow has. Reading it as
    zero would turn that into the quietest kind of pass, which is the whole of #617.
    """
    try:
        n = int(str(text).strip())
    except (TypeError, ValueError, AttributeError):
        return 0
    return n if n > 0 else 0


def default_jobs() -> int:
    """How many sandboxes a run uses when nobody says: half the machine, never none.

    Out of `main()`'s argument list for #572's reason, which is the same one `base_for`
    and `timeout_for` are out here for: a rule written inside `main()` is not reachable
    from a test, so it cannot be swept, so it is a guard this harness is unable to hold
    itself to. The self-sweep found this exact line unpinned in both directions.

    Half, because a sandbox runs the suite and the suite starts real tmux servers, so the
    machine is doing about two things per job. Never zero, which is what the floor is for:
    `os.cpu_count()` is 1 on a small runner and `1 // 2` is a sweep that measures nothing
    while reporting that it ran.
    """
    return max(1, (os.cpu_count() or 4) // 2)


def exit_code(results: list[Result]) -> int:
    """What a plain (non-gate) run exits with.

    3 and not 0 for an unmeasured mutation: a sweep that could not measure some of its
    mutations has not shown the branch to be clean, and anything reading this code must
    not treat "I could not look" as "nothing to see". 4 outranks both, because a mutation
    that never applied means the other numbers are not evidence either.
    """
    if any(r.verdict == "unapplied" for r in results):
        return 4
    if any(r.verdict == "survived" for r in results):
        return 1
    return 3 if any(r.verdict == "unresolved" for r in results) else 0


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
    p.add_argument("--jobs", type=int, default=default_jobs(),
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
    p.add_argument("--gate", action="store_true",
                   help="Report as a CI gate: sort the survivors into unpinned, masked "
                        "cluster and platform-deferred, and exit by the gate's rules.")
    p.add_argument("--enforce", action="store_true",
                   help="Make --gate blocking. Off by default and deliberately: a gate "
                        "whose numbers nobody has seen gets disabled the first time it is "
                        "inconvenient.")
    p.add_argument("--summary", default=None, metavar="PATH",
                   help="Append the gate's markdown to this file — $GITHUB_STEP_SUMMARY "
                        "on CI, so the numbers are on the pull request rather than in a "
                        "log nobody opens.")
    p.add_argument("--plan", action="store_true",
                   help="Say how many mutations this branch offers and how many jobs "
                        "they need, then stop. Costs one `ast` pass and no test runs.")
    p.add_argument("--warm-map", action="store_true", dest="warm_map",
                   help="Measure the selection map into --workdir and stop, so that the "
                        "shards restore it instead of each measuring it again.")
    p.add_argument("--shard", default=None, metavar="N/M",
                   help="Sweep only slice N of M, dealt one mutation at a time across "
                        "the whole plan. Nothing is dropped: the other slices are other "
                        "jobs. --second-order sees only its own slice and is weaker "
                        "under it; the masked-cluster BUCKET is not, because --verdict "
                        "classifies the merged results and not one shard's.")
    p.add_argument("--verdict", default=None, metavar="DIR",
                   help="Merge every shard's --json from DIR into one answer, and say "
                        "which of the three a run found. Needs --shards.")
    p.add_argument("--shards", default=None, metavar="N",
                   help="How many shards --verdict should have heard from. An empty or "
                        "unreadable value is a sweep that never sized itself, which is "
                        "no verdict and not a clean one.")
    p.add_argument("--annotate", action="store_true",
                   help="Print each finding as a GitHub workflow command, so a survivor "
                        "lands on the line of the diff it is about.")
    p.add_argument("--github-output", default=None, metavar="PATH", dest="github_output",
                   help="Append `key=value` lines here — $GITHUB_OUTPUT on CI, which is "
                        "how the check that carries the answer gets its NAME.")
    args = p.parse_args(argv)
    if args.verdict:
        return _merge_step(args)
    if args.gate and args.all:
        p.error("--gate charges the lines a branch added; --all charges the whole tree. "
                "The gate never sweeps the whole tree — that is stage B, and it is a "
                "14-hour job, not a pull-request check.")

    root = repo_root(Path.cwd())
    paths = tuple(args.paths or DEFAULT_PATHS)
    ref = git("rev-parse", args.ref, cwd=root).strip()
    base = base_for(root, ref, args.base)

    workdir = workdir_for(root, args.workdir)
    workdir.mkdir(parents=True, exist_ok=True)
    cache_dir = workdir / "cache"

    def log(*a):
        print(*a, flush=True)

    log(f"  workdir: {workdir}")
    started = time.time()
    log(f"sweeping {ref[:12]} (paths: {', '.join(paths)})")
    dirty = dirty_for(root, paths, args.ref)
    if dirty:
        log(f"  carrying {len(dirty)} uncommitted file(s) into the sandboxes")

    if args.all:
        scope = all_lines(root, paths)
        log(f"  --all: the whole tree, {len(scope)} file(s)")
    else:
        scope = added_lines(root, base, ref, paths)
        log(f"  diff against {base[:12]}: {len(scope)} file(s), "
            f"{sum(len(v) for v in scope.values())} added line(s)")
    if args.plan or args.warm_map:
        return _plan_step(args, root, ref, scope, dirty, workdir, cache_dir, log)
    if not scope:
        log("  nothing under the swept paths changed. Nothing to do.")
        if args.summary:
            _append(args.summary, "## Deletion sweep\n\nNothing under the swept paths "
                                  "changed on this branch, so there was nothing to sweep.\n")
        # An empty result set and not a missing file. Downstream — the merge step, and
        # anyone reading the artifact — "the sweep ran and found nothing to do" and "the
        # sweep never reported" are the two answers #617 is about, and a shard that
        # writes nothing here is indistinguishable from a shard that was cancelled.
        if args.json:
            Path(args.json).write_text(as_json([]))
        return 0

    # The map is measured on a clean checkout of the ref, so that a mutation is the only
    # thing that ever differs from what was traced.
    log("  preparing the reference sandbox…")
    ref_box = Sandbox(root, run_dir(workdir) / "ref", ref, dirty)
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
        full_timeout = timeout_for(took)
        if full_timeout > FULL_TIMEOUT:
            log(f"    full-suite timeout raised to {full_timeout / 60:.0f} min for this box")
        if not baseline.green:
            log("  ! the tree is RED before any mutation. Every mutation below will look")
            log("  ! pinned for a reason that has nothing to do with the guard. Fix first.")
            if args.summary:
                _append(args.summary,
                        "## Deletion sweep — not run\n\nThe tree is **red before any "
                        "mutation**, so every mutation would have looked pinned for a "
                        f"reason that has nothing to do with any guard: {baseline.detail}\n")
            # A reporting gate blocks nothing, and that has to include this. A red baseline
            # is a real problem and the summary above says so in the loudest terms the page
            # has — but failing a pull request over it, on a job whose numbers nobody has
            # read yet, is precisely the "inconvenient the first time" that gets a gate
            # switched off. `--enforce` is the flag that decides, for this and everything
            # else, and it decides once.
            return 2 if args.enforce or not args.gate else 0

    results, pairs = sweep(root, ref, scope, selection, workdir, args.jobs, dirty,
                           args.second_order, log, full_timeout,
                           parse_shard(args.shard) if args.shard else None)
    elapsed = time.time() - started
    text = report(results, root, ref, base, baseline, elapsed, pairs)
    log("")
    log(text)
    if args.json:
        Path(args.json).write_text(as_json(results))
    if not args.keep:
        # This run's sandboxes and no others. A `workdir.glob("w*")` here would delete a
        # concurrent sweep's trees out from under it — the same collision `run_dir` exists
        # to prevent, arriving at the end instead of the beginning.
        shutil.rmtree(run_dir(workdir), ignore_errors=True)
    if not args.gate:
        return exit_code(results)
    gate = classify(results)
    if args.summary:
        _append(args.summary, gate_summary(gate, ref, base, elapsed, args.enforce))
    _say(args, gate, log)
    return gate_exit_code(gate, args.enforce)


def _say(args, gate: Gate, log, missing: int = 0, shards: int = 1) -> None:
    """The counts, the one-line answer, and the annotations — in that order.

    The headline is printed on every gate run and not only the sharded one. A local
    `--gate` and CI are answering the same question, and the value of one sentence that
    means the same thing everywhere is that nobody has to learn two vocabularies to read
    the same sweep twice.
    """
    log("")
    log(f"gate: {len(gate.unpinned)} unpinned, {len(gate.masked)} in masked cluster(s), "
        f"{len(gate.platform)} platform-deferred, {len(gate.unresolved)} unresolved, "
        f"{len(gate.unapplied)} not applied, {len(gate.withheld)} withheld, "
        f"{gate.pinned} pinned")
    log(f"gate: {headline(gate, missing, shards)}")
    if not args.enforce:
        log("gate: reporting only — nothing here blocks. Pass --enforce to make it.")
    if args.annotate:
        for line in annotations(gate):
            log(line)
    _write_output(args.github_output,
                  conclusion=gate_conclusion(gate, missing),
                  headline=headline(gate, missing, shards))


def _write_output(path: str | None, **values: str) -> None:
    """`key=value` into `$GITHUB_OUTPUT`, one line each.

    Single-line values only, and every value here is one by construction — :func:`headline`
    exists to be short. A value with a newline in it would need the heredoc form, and a
    caller that got that wrong would inject workflow outputs rather than set one, so a
    newline is refused instead of encoded.
    """
    if not path:
        return
    lines = []
    for key, value in values.items():
        if "\n" in value or "\r" in value:
            raise ValueError(f"{key} is not a single line: {value!r}")
        lines.append(f"{key}={value}")
    _append(path, "\n".join(lines))


def _plan_step(args, root: Path, ref: str, scope: dict[str, set[int]],
               dirty: dict[str, bytes], workdir: Path, cache_dir: Path,
               log) -> int:
    """Size the sweep, and optionally leave the selection map where the shards will find it.

    This is the job that decides how many jobs the gate needs, and it is cheap on purpose:
    the mutation count is an `ast` pass over the diff, so the decision costs a second and
    is made from the real number rather than from a guess about how big branches get. The
    guess is exactly what ran out — the job was sized against Phase 2's 30–52 mutations
    and met 78.
    """
    plan, _ = plan_for(root, ref, scope, dirty)
    shards = shards_for(len(plan))
    log(f"  {len(plan)} mutations → {shards} shard(s) of at most {per_shard()} each")
    loud = over_budget(len(plan))
    if loud:
        # An annotation and not a log line. A cap nobody can see is the failure the spec
        # names about silent truncation, and a warning in a fold nobody opens is a cap
        # nobody can see.
        log(f"::warning title={_property('The deletion sweep is over its budget')}::"
            + _escape(loud))
    _write_output(args.github_output, mutations=str(len(plan)), shards=str(shards),
                  matrix=json.dumps(list(range(1, shards + 1))))
    if args.warm_map:
        # From a sandbox, for the reason `main` builds it from one: the map is measured on
        # a clean checkout of the ref so that a mutation is the only thing that ever
        # differs from what was traced. The cache is keyed on the tree's own hash, so the
        # shards find this one only if they are asking about the very same tree.
        log("  warming the selection map for the shards…")
        box = Sandbox(root, run_dir(workdir) / "map", ref, dirty)
        load_map(box.path, tuple(args.paths or DEFAULT_PATHS), cache_dir, args.jobs,
                 args.refresh_map, log)
        shutil.rmtree(run_dir(workdir), ignore_errors=True)
    return 0


def _merge_step(args) -> int:
    """One answer for a sweep that ran on several machines.

    The merge and not the shards is where the pull request gets told anything, and that
    is the point: a shard writes a result file and says nothing, so there is exactly one
    place that decides what this branch is told and exactly one sentence it says. Before
    #617 there were N step summaries and no sentence at all.
    """
    shards = expected_shards(args.shards)
    results, missing = merge(Path(args.verdict), max(shards, 1))
    # `shards` stays 0 when the plan never answered, and travels that way: everything
    # downstream renders "how many did not report" differently from "how many there were
    # supposed to be is not known", and flattening the second into `1 of 1` would report
    # a sweep that was never planned as a sweep that was planned and lost one shard.
    if not shards:
        missing = max(missing, 1)
    gate = classify(results)
    if args.summary:
        # "merge-base" and not "the merge-base": the header shortens a sha to twelve
        # characters, and a fallback longer than that comes out clipped mid-word.
        _append(args.summary, gate_summary(gate, args.ref, args.base or "merge-base",
                                           None, args.enforce, missing, shards))
    print(f"merged {len(results)} result(s) from "
          f"{f'{shards - missing} of {shards}' if shards else 'an unknown number of'} "
          f"shard(s)")
    _say(args, gate, print, missing, shards)
    return verdict_exit_code(gate, missing, args.enforce)


def _append(path: str, text: str) -> None:
    """Add to a file rather than replacing it — `$GITHUB_STEP_SUMMARY` is shared.

    Every step of a job writes to the same file, so a `write_text` here would silently
    delete whatever an earlier step put on the pull request.
    """
    with open(path, "a", encoding="utf-8") as fh:
        fh.write(text.rstrip("\n") + "\n")


if __name__ == "__main__":       # pragma: no cover - entry point
    sys.exit(main())
