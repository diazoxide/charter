"""Which ``mkdir`` calls in `charter/` can create a directory under the state directory.

Static, and deliberately so: the question is *coverage* — has every writer that can make
``.charter/`` been routed through `config.private_mkdir` — and coverage is a question about
code that was not executed. The behavioural half lives in
`test_the_state_directory_is_charters_to_choose.py`, which runs real commands and measures
the mode that comes out; this half is what notices the writer nobody ran.

Not a ``test_*`` module, so discovery skips it. Its own accuracy is tested there, against
sources built for the purpose.

**How a path is judged to be state-derived, and why it is derived rather than listed.**
The names that live under ``STATE_DIR`` are asked of `config` itself — every entry of
`config.DERIVED` whose value is a path under the state directory — so a setting added to
`config.derive` is covered the day it is added rather than the day somebody remembers this
file. Matching is on the ATTRIBUTE name (``.STATE_DIR``), never on the module alias in
front of it: `hooks` reaches it as ``_cfg.STATE_DIR`` and `frame.state` as
``config.STATE_DIR``, and a scan keyed to one spelling would silently skip the other.

One level of indirection is followed, transitively: a module-level function whose returns
mention a state name is itself a state path source (``_cache_file()``, ``_route_mark(sid)``,
``frame.state._root()``), and so is one that returns a call to such a function. Local
assignments inside the calling function are substituted before the test, so
``f = _cache_file()`` … ``f.parent.mkdir(…)`` is seen for what it is.

**What it cannot see, said out loud.** A path that reaches a writer as a *parameter* —
`memstore.write(mem_dir, …)` is handed either a committed persona directory or an
ephemeral one under ``PERSONA_STATE_DIR`` — is invisible to a static reader, because the
answer depends on the caller. A path assembled from a string (``Path(str(config.ROOT) +
"/.charter")``) is invisible too. Those are the next spellings, and neither is closed here:
what closes the exposure they would open is that ``.charter/`` itself is 0700, so a
directory created under it at the umask default is still reachable by nobody but its owner.
"""

from __future__ import annotations

import ast
import re
from pathlib import Path

#: The package under test, as a directory.
PACKAGE = Path(__file__).resolve().parent.parent / "charter"


def state_attribute_names() -> set[str]:
    """Every ``config`` attribute whose value is the state directory or lives under it.

    Asked of `config.derive`, not written down: `derive` is the single definition of what
    follows from the plane root, and this is a question about its output.
    """
    from charter import config

    values = config.derive(Path("/nonexistent-plane-root"))
    state = Path(values["STATE_DIR"])
    out = set()
    for name, val in values.items():
        if not isinstance(val, Path):
            continue
        try:
            val.relative_to(state)
        except ValueError:
            continue
        out.add(name)
    return out


def _returns(fn: ast.AST) -> list[str]:
    return [ast.unparse(n.value) for n in ast.walk(fn)
            if isinstance(n, ast.Return) and n.value is not None]


def _mentions_state(text: str, state_names: set[str]) -> bool:
    """Keyed to the ATTRIBUTE — ``.STATE_DIR`` — so `hooks`'s ``_cfg.STATE_DIR`` and
    `frame.state`'s ``config.STATE_DIR`` are one question, not two."""
    return any(re.search(rf"\.{n}\b", text) for n in state_names)


def _calls(text: str, names) -> bool:
    """Does *text* call one of *names*? Word-anchored: ``consent_path()`` is not a call to
    ``_path()``, and a substring test says it is."""
    return any(re.search(rf"(?<![\w.]){re.escape(n)}\s*\(", text) for n in names)


def state_path_functions(tree: ast.AST, state_names: set[str]) -> set[str]:
    """Module-level function names whose return value is a path under the state directory.

    A fixpoint, so ``path_for()`` returning ``_dir() / …`` counts when ``_dir()`` does.
    """
    funcs = {n.name: n for n in tree.body
             if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))}
    found: set[str] = set()
    changed = True
    while changed:
        changed = False
        for name, fn in funcs.items():
            if name in found:
                continue
            texts = _returns(fn)
            hit = any(_mentions_state(t, state_names) for t in texts) or \
                any(_calls(t, found) for t in texts)
            if hit:
                found.add(name)
                changed = True
    return found


def _expand(expr: str, assigns: dict[str, str], rounds: int = 4) -> str:
    """*expr* with local names replaced by what they were assigned, a few rounds deep."""
    text = expr
    for _ in range(rounds):
        before = text
        for name, value in assigns.items():
            if name in text:
                text = text.replace(name, f"({value})")
        if text == before:
            return text
    return text


def violations(source: str, state_names: set[str]) -> list[tuple[int, str]]:
    """``(line, path expression)`` for every ``mkdir``/``makedirs`` in *source* whose path
    is state-derived — i.e. every one that should be `config.private_mkdir` and is not."""
    tree = ast.parse(source)
    state_funcs = state_path_functions(tree, state_names)
    scopes = [(n.lineno, n.end_lineno, n) for n in ast.walk(tree)
              if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))]
    out = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        fn = node.func
        name = getattr(fn, "attr", None) or getattr(fn, "id", None)
        if name not in ("mkdir", "makedirs"):
            continue
        if isinstance(fn, ast.Attribute):
            expr = ast.unparse(fn.value)
        elif node.args:
            expr = ast.unparse(node.args[0])
        else:
            continue
        enclosing = sorted((s for s in scopes if s[0] <= node.lineno <= s[1]),
                           key=lambda s: s[1] - s[0])
        assigns = {}
        if enclosing:
            for n2 in ast.walk(enclosing[0][2]):
                if isinstance(n2, ast.Assign) and len(n2.targets) == 1 \
                        and isinstance(n2.targets[0], ast.Name):
                    assigns.setdefault(n2.targets[0].id, ast.unparse(n2.value))
        full = _expand(expr, assigns)
        if _mentions_state(full, state_names) or _calls(full, state_funcs):
            out.append((node.lineno, expr))
    return out


def scan_package() -> dict[str, list[tuple[int, str]]]:
    """``{relative path: violations}`` over the whole package, empty entries dropped."""
    names = state_attribute_names()
    found = {}
    for f in sorted(PACKAGE.rglob("*.py")):
        hits = violations(f.read_text(), names)
        if hits:
            found[str(f.relative_to(PACKAGE.parent))] = hits
    return found
