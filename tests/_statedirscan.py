"""Which ``mkdir`` calls in `charter/` can create a directory under the state directory.

Static, and deliberately so: the question is *coverage* — has every writer that can make
``.charter/`` been routed through `config.private_mkdir` / `config.mkdir_for` — and
coverage is a question about code that was not executed. The behavioural half lives in
`test_the_state_directory_is_charters_to_choose.py`, which runs real commands and measures
the mode that comes out; this half is what notices the writer nobody ran.

Not a ``test_*`` module, so discovery skips it. Its own accuracy is tested there, against
sources built for the purpose.

**The property is "a mkdir that can be reached with a state path", not "a mkdir whose own
line spells one".** Those are different questions, and the first cut asked only the
second: it matched the *spelling* ``.STATE_DIR`` in the path expression at the call site.
`memstore.write(mem_dir, …)` spells nothing — it is *handed* the committed
``personas/<n>/memory`` on one call and the gitignored
``PERSONA_STATE_DIR/ephemeral/<session>/<n>`` on the next — so the scan reported a clean
package while ``charter persona remember … --ephemeral`` created ``.charter/`` itself at
the umask default (#470). Two halves now, and the second is the one that class needs.

**How a path is judged to be state-derived, and why it is derived rather than listed.**
The names that live under ``STATE_DIR`` are asked of `config` itself — every entry of
`config.DERIVED` whose value is a path under the state directory — so a setting added to
`config.derive` is covered the day it is added rather than the day somebody remembers this
file. Matching is on the ATTRIBUTE name (``.STATE_DIR``), never on the module alias in
front of it: `hooks` reaches it as ``_cfg.STATE_DIR`` and `frame.state` as
``config.STATE_DIR``, and a scan keyed to one spelling would silently skip the other.

One level of indirection is followed, transitively: a module-level function whose returns
mention a state name is itself a state path source (``_cache_file()``, ``_route_mark(sid)``,
``persona.ephemeral_dir()``), and so is one that returns a call to such a function. That
fixpoint is now **cross-module** — a caller reaching `persona.ephemeral_dir` through the
alias it imported it under is the same question as one calling a helper in its own file.
Local assignments inside the calling function are substituted before the test, so
``f = _cache_file()`` … ``f.parent.mkdir(…)`` is seen for what it is.

**The handed half.** :func:`handed_violations` propagates *arguments*: a call that passes
a state path (or a parameter already known to carry one) into a package function taints
that function's parameter, transitively, and a ``mkdir`` on a tainted parameter is a
violation exactly as a ``mkdir`` on ``config.STATE_DIR / …`` is. This is what makes the
scan match reachability rather than a spelling, and what closes the gap the docstring
above it used to describe and excuse.

**Which subexpression is the path is a property, not a spelling.** ``p.mkdir(…)`` keeps its
path in the RECEIVER and ``os.makedirs(p)`` in the first ARGUMENT, and both are attribute
calls — so the first cut, which read the receiver of every attribute call, scanned
``os.makedirs(config.STATE_DIR / 'x')`` as the expression ``os`` and the advertised
``makedirs`` coverage was dead for the only spelling anyone writes. The shape is no longer
decided: `_mkdir_sites` yields every position that could hold the path — receiver, first
argument, and the ``path=``/``name=`` keywords the stdlib signatures use — and a state path
in any of them is a violation. A wrong guess now costs a false positive, which is loud and
in the safe direction, rather than a silence.

**What it still cannot see, said out loud.** A directory maker reached through a local
rebinding (``mk = os.makedirs`` … ``mk(p)``) is not recognised as one: the aliased *import*
is followed, an assignment is not. A path assembled from a string
(``Path(str(config.ROOT) + "/.charter")``) is invisible: no attribute is named and no
call is made. A path arriving from outside the package — read out of JSON, taken from
``argv`` — is invisible for the same reason. Neither is closed here, and the honest reason
they are not an exposure is no longer "``.charter/`` is 0700 so a loose directory under it
does not matter" — that was false on the exact flow above, where the loose directory *was*
``.charter/``. It is that `config.mkdir_for` decides at **runtime**, on where the path
actually is, so a writer routed through it is right about a path this reader cannot name.
The scan's job is to prove every writer is routed; the guard is `config`.
"""

from __future__ import annotations

import ast
import re
from pathlib import Path

#: The package under test, as a directory.
PACKAGE = Path(__file__).resolve().parent.parent / "charter"

#: The two routed spellings. A ``mkdir`` is never one of these — they are functions, not
#: methods named ``mkdir`` — so this is documentation of what "routed" means rather than a
#: filter, and is asserted against `config` so a rename cannot leave it stale.
ROUTED = ("private_mkdir", "mkdir_for")

#: `config`'s own walk, exempt because it **is** the routing. ``_mkdir_0700`` is the
#: private mkdir itself; ``mkdir_for``'s bare one is the branch it takes having just
#: decided at runtime that the path is not state. Flagging these would be asking the guard
#: to route through itself. Named in full — ``module.function`` — so an unrelated
#: ``_mkdir_0700`` appearing elsewhere would still be scanned.
THE_WALK = ("config.private_mkdir", "config._mkdir_0700", "config.mkdir_for")


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


#: One compiled alternation per set of state names, because the package-wide scan asks
#: this question tens of thousands of times and a regex per name per ask is the difference
#: between a test and a coffee break.
_STATE_RE: dict[frozenset, "re.Pattern"] = {}


def _state_re(state_names) -> "re.Pattern":
    key = frozenset(state_names)
    r = _STATE_RE.get(key)
    if r is None:
        r = _STATE_RE[key] = re.compile(
            r"\.(?:" + "|".join(sorted(map(re.escape, key))) + r")\b")
    return r


def _mentions_state(text: str, state_names: set[str]) -> bool:
    """Keyed to the ATTRIBUTE — ``.STATE_DIR`` — so `hooks`'s ``_cfg.STATE_DIR`` and
    `frame.state`'s ``config.STATE_DIR`` are one question, not two."""
    return bool(state_names) and bool(_state_re(state_names).search(text))


def _calls(text: str, names) -> bool:
    """Does *text* call one of *names*? Word-anchored: ``consent_path()`` is not a call to
    ``_path()``, and a substring test says it is."""
    return any(re.search(rf"(?<![\w.]){re.escape(n)}\s*\(", text) for n in names)


def _names(text: str, names) -> bool:
    """Does *text* mention one of *names* as a name (not as somebody's attribute)?"""
    if not names:
        return False
    return bool(re.search(r"(?<![\w.])(?:" + "|".join(sorted(map(re.escape, names)))
                          + r")\b", text))


#: ``foo(``/``a.b(`` — every callable named in an expression, in one pass.
_CALLED_RE = re.compile(r"(?<![\w.])([\w.]+)\s*\(")


def state_path_functions(tree: ast.AST, state_names: set[str]) -> set[str]:
    """Module-level function names whose return value is a path under the state directory.

    A fixpoint, so ``path_for()`` returning ``_dir() / …`` counts when ``_dir()`` does.
    Single-module; :func:`package_state_functions` is the same question across the package.
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


def _expand(expr: str, assigns: dict[str, str], rounds: int = 4,
            limit: int = 4000) -> str:
    """*expr* with local names replaced by what they were assigned, a few rounds deep.

    Substituted on a WORD boundary, and never after a dot: a local named ``d`` must not
    rewrite the middle of ``mem_dir``, and ``f.parent`` is not an assignment to ``parent``.
    A plain ``str.replace`` did both.

    *limit* stops the substitution once the text has grown past anything a path expression
    plausibly is. Expansion is quadratic in a long function with many locals — and this now
    runs on every call site in the package, not only the ``mkdir`` ones — while a 4000-character
    expression that has not yet named a state path is not about to start.
    """
    text = expr
    for _ in range(rounds):
        before = text
        for name, value in assigns.items():
            if len(text) > limit:
                return text
            text = re.sub(rf"(?<![\w.]){re.escape(name)}\b", lambda _m: f"({value})", text)
        if text == before:
            return text
    return text


def violations(source: str, state_names: set[str]) -> list[tuple[int, str]]:
    """``(line, path expression)`` for every ``mkdir``/``makedirs`` in *source* whose path
    is state-derived **by its own spelling** — the named half. The handed half, where the
    path arrives as a parameter, is :func:`handed_violations`."""
    tree = ast.parse(source)
    state_funcs = state_path_functions(tree, state_names)
    out = []
    for node, exprs, enclosing in _mkdir_sites(tree):
        assigns = _local_assigns(enclosing)
        for expr in exprs:
            full = _expand(expr, assigns)
            if _mentions_state(full, state_names) or _calls(full, state_funcs):
                out.append((node.lineno, expr))
                break
    return out


# --------------------------------------------------------------------------- #
# shared AST plumbing                                                          #
# --------------------------------------------------------------------------- #
def _scopes(tree: ast.AST):
    return [(n.lineno, n.end_lineno, n) for n in ast.walk(tree)
            if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))]


def _enclosing(scopes, lineno):
    """The innermost function containing *lineno*, or None at module level."""
    hits = sorted((s for s in scopes if s[0] <= lineno <= s[1]), key=lambda s: s[1] - s[0])
    return hits[0][2] if hits else None


def _local_assigns(fn) -> dict[str, str]:
    """``{name: source of what it was assigned}`` for simple local assignments in *fn*."""
    assigns: dict[str, str] = {}
    if fn is None:
        return assigns
    for n in ast.walk(fn):
        if isinstance(n, ast.Assign) and len(n.targets) == 1 \
                and isinstance(n.targets[0], ast.Name):
            assigns.setdefault(n.targets[0].id, ast.unparse(n.value))
    return assigns


#: The keywords the stdlib gives the path of a directory-making call: ``os.mkdir(path=…)``
#: and ``os.makedirs(name=…)``. Read from the signatures, not invented.
_PATH_KWARGS = ("path", "name")


def _maker_aliases(tree: ast.AST) -> set[str]:
    """Local names bound to ``os.mkdir`` / ``os.makedirs`` by an *aliased* import.

    ``from os import makedirs`` already lands under the bare name; ``from os import
    makedirs as md`` renames the call site, and a filter reading the local spelling would
    let the alias walk out of the scan.
    """
    out = set()
    for n in ast.walk(tree):
        if not isinstance(n, ast.ImportFrom) or n.level != 0 or n.module != "os":
            continue
        out |= {a.asname for a in n.names if a.name in ("mkdir", "makedirs") and a.asname}
    return out


def _mkdir_sites(tree: ast.AST):
    """``(call node, candidate path expressions, enclosing function)`` per ``mkdir``.

    **Which subexpression is the path is a property of the call shape, and the first cut
    matched a spelling instead.** ``p.mkdir(…)`` is a bound method and the path is the
    RECEIVER; ``os.mkdir(p)`` / ``os.makedirs(p)`` are module functions and the path is the
    first ARGUMENT. Both are attribute calls, and reading the receiver of every attribute
    call — which is what this did — scanned ``os.makedirs(config.STATE_DIR / 'x')`` as the
    expression ``os``, so the advertised ``makedirs`` coverage was dead for the only
    spelling anyone writes. Keying instead on the name (``makedirs`` takes an argument,
    ``mkdir`` a receiver) trades one spelling for another and still misses ``os.mkdir(p)``.

    So the shape is not decided at all: every position that COULD hold the path is
    yielded, and a state path in any of them is a violation. A wrong guess costs a false
    positive — loud, and in the safe direction — where a wrong shape costs silence.
    """
    scopes = _scopes(tree)
    aliases = _maker_aliases(tree)
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        fn = node.func
        name = getattr(fn, "attr", None) or getattr(fn, "id", None)
        if name not in ("mkdir", "makedirs") and name not in aliases:
            continue
        exprs = [ast.unparse(fn.value)] if isinstance(fn, ast.Attribute) else []
        exprs += [ast.unparse(a) for a in node.args[:1]]
        exprs += [ast.unparse(k.value) for k in node.keywords if k.arg in _PATH_KWARGS]
        if exprs:
            yield node, tuple(exprs), _enclosing(scopes, node.lineno)


def _params(fn) -> list[str]:
    """Positional parameter names of *fn*, in call order, ``self`` dropped."""
    a = fn.args
    out = [p.arg for p in (*a.posonlyargs, *a.args)]
    return out[1:] if out[:1] == ["self"] else out


def _kwparams(fn) -> set[str]:
    return {p.arg for p in (*fn.args.args, *fn.args.posonlyargs, *fn.args.kwonlyargs)}


# --------------------------------------------------------------------------- #
# the package as one graph                                                     #
# --------------------------------------------------------------------------- #
def load_package() -> dict[str, tuple[Path, ast.Module]]:
    """``{module name: (path, tree)}`` for every module in the package.

    Module names are dotted and relative to the package (``memstore``, ``frame.state``),
    which is how the import aliases below resolve.
    """
    out = {}
    for f in sorted(PACKAGE.rglob("*.py")):
        rel = f.relative_to(PACKAGE).with_suffix("")
        parts = list(rel.parts)
        if parts[-1] == "__init__":
            parts.pop()
        out[".".join(parts) or "__init__"] = (f, ast.parse(f.read_text()))
    return out


def modules_from(sources: dict[str, str]) -> dict[str, tuple[Path, ast.Module]]:
    """A package built out of *sources* — ``{module name: source}`` — for testing the scan.

    The handed half is a question about calls *between* modules, so its accuracy cannot be
    checked against one string the way :func:`violations` can. This gives the tests the
    same shape :func:`load_package` produces, without a package on disk.
    """
    return {name: (PACKAGE / (name.replace(".", "/") + ".py"), ast.parse(src))
            for name, src in sources.items()}


def _aliases(tree: ast.Module, module: str) -> dict[str, str]:
    """``{local name: module it refers to}`` for the package-relative imports in *tree*.

    ``from . import memstore`` and ``from . import config as _cfg`` and
    ``from .frame import state`` — the three shapes this package uses. An absolute import
    of something outside the package resolves to nothing and is dropped, so a third-party
    ``state.mkdir`` cannot be mistaken for `charter.frame.state`.
    """
    here = module.rsplit(".", 1)[0] if "." in module else ""
    out = {}
    for n in ast.walk(tree):
        if not isinstance(n, ast.ImportFrom) or n.level == 0:
            continue
        base = here if n.level == 1 else ".".join(here.split(".")[:-(n.level - 1)] or [])
        if n.module:
            base = f"{base}.{n.module}" if base else n.module
        for a in n.names:
            target = f"{base}.{a.name}" if base else a.name
            out[a.asname or a.name] = target
    return out


def package_state_functions(mods, state_names: set[str]) -> set[str]:
    """``{"module.func"}`` for every package function returning a path under the state dir.

    The cross-module fixpoint: `commands_persona` calling ``persona.ephemeral_dir()`` is
    the same question as `persona` calling its own ``_dir()``, and a per-file scan answers
    only the second.
    """
    funcs = {}   # "mod.name" -> (module, return expressions)
    for module, (_p, tree) in mods.items():
        for n in ast.walk(tree):
            if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef)):
                funcs.setdefault(f"{module}.{n.name}", (module, _returns(n)))
    aliases = {m: _aliases(t, m) for m, (_p, t) in mods.items()}
    found: set[str] = set()
    changed = True
    while changed:
        changed = False
        for qual, (module, texts) in funcs.items():
            if qual in found:
                continue
            if any(_mentions_state(t, state_names) for t in texts) or \
                    any(_is_state_call(t, module, aliases[module], found) for t in texts):
                found.add(qual)
                changed = True
    return found


def _is_state_call(text: str, module: str, aliases: dict, state_funcs: set[str]) -> bool:
    """Does *text*, read from inside *module*, call one of the qualified *state_funcs*?

    Every callable named in *text* is pulled out in one pass and then RESOLVED through the
    caller's own import aliases — rather than each state function being searched for by
    bare name. Not only faster: ``_dir()`` exists in more than one module here and only
    some of them are state, so a bare-name match would flag `dispatch`'s (which returns a
    committed ``personas/`` path) on the strength of somebody else's.
    """
    for cand in _CALLED_RE.findall(text):
        if "." in cand:
            head, _, attr = cand.rpartition(".")
            head = head.split(".")[0]
            target = aliases.get(head)
            if target is None:
                continue
            if f"{target}.{attr}" in state_funcs or target in state_funcs:
                return True
        elif f"{module}.{cand}" in state_funcs:
            return True
        elif aliases.get(cand) in state_funcs:
            return True
    return False


def _resolve_callee(call: ast.Call, module: str, mods, aliases) -> str | None:
    """``"module.func"`` for a call to a package function, else None."""
    fn = call.func
    if isinstance(fn, ast.Name):
        return f"{module}.{fn.id}"
    if isinstance(fn, ast.Attribute) and isinstance(fn.value, ast.Name):
        target = aliases.get(fn.value.id)
        if target is None:
            return None
        if target in mods:
            return f"{target}.{fn.attr}"
        return f"{target}.{fn.attr}"  # ``from . import x`` of a name, then ``x.attr``
    return None


def tainted_params(mods, state_names: set[str], state_funcs: set[str]) -> set[str]:
    """``{"module.func.param"}`` — every package parameter a state path can reach.

    Seeded by call sites that pass one (``memstore.write(ephemeral_dir(…), …)``) and
    closed under passing a tainted parameter on to the next function, so the depth of the
    call chain is not a way to fall out of the scan.
    """
    sig = {}     # "mod.func" -> (positional names, keyword names)
    for module, (_p, tree) in mods.items():
        for n in ast.walk(tree):
            if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef)):
                sig.setdefault(f"{module}.{n.name}", (_params(n), _kwparams(n)))

    # Every call that could taint something, flattened once: unparsing and expanding
    # arguments is the expensive part and none of it changes between rounds.
    sites = []   # (module, aliases, enclosing name, callee, [(param, expanded arg text)])
    for module, (_p, tree) in mods.items():
        aliases = _aliases(tree, module)
        scopes = _scopes(tree)
        for node in ast.walk(tree):
            if not isinstance(node, ast.Call):
                continue
            callee = _resolve_callee(node, module, mods, aliases)
            if callee is None or callee not in sig:
                continue
            positional, kw = sig[callee]
            here = _enclosing(scopes, node.lineno)
            assigns = _local_assigns(here)
            args = [(positional[i], _expand(ast.unparse(a), assigns))
                    for i, a in enumerate(node.args) if i < len(positional)]
            args += [(k.arg, _expand(ast.unparse(k.value), assigns))
                     for k in node.keywords if k.arg in kw]
            if args:
                sites.append((module, aliases, here.name if here else None, callee, args))

    tainted: set[str] = set()
    # Seed: an argument that names a state path outright, independent of any other round.
    seeds = []
    for module, aliases, hname, callee, args in sites:
        rest = []
        for param, text in args:
            if _mentions_state(text, state_names) or \
                    _is_state_call(text, module, aliases, state_funcs):
                tainted.add(f"{callee}.{param}")
            elif hname is not None:
                rest.append((f"{module}.{hname}.", callee, param, text))
        seeds.extend(rest)

    # Close under passing one on: depth of the call chain is not a way out of the scan.
    changed = True
    while changed:
        changed = False
        for prefix, callee, param, text in seeds:
            q = f"{callee}.{param}"
            if q in tainted:
                continue
            mine = [t.rpartition(".")[2] for t in tainted if t.startswith(prefix)]
            if mine and _names(text, mine):
                tainted.add(q)
                changed = True
    return tainted


def handed_violations(mods, state_names: set[str]) -> dict[str, list[tuple[int, str]]]:
    """``{relative path: [(line, expr)]}`` for every ``mkdir`` on a **handed** state path.

    The half the named scan cannot see, and the one `memstore` fell through.
    """
    state_funcs = package_state_functions(mods, state_names)
    tainted = tainted_params(mods, state_names, state_funcs)
    found: dict[str, list[tuple[int, str]]] = {}
    for module, (path, tree) in mods.items():
        scopes = _scopes(tree)
        for node, exprs, here in _mkdir_sites(tree):
            if here is None or f"{module}.{here.name}" in THE_WALK:
                continue
            mine = [q.rpartition(".")[2] for q in tainted
                    if q.startswith(f"{module}.{here.name}.")]
            if not mine:
                continue
            assigns = _local_assigns(here)
            for expr in exprs:
                if _names(_expand(expr, assigns), mine):
                    found.setdefault(str(path.relative_to(PACKAGE.parent)), []).append(
                        (node.lineno, expr))
                    break
    for hits in found.values():
        hits.sort()
    return found


def scan_package() -> dict[str, list[tuple[int, str]]]:
    """``{relative path: violations}`` over the whole package — both halves, merged.

    Named and handed are one answer because they are one property: a ``mkdir`` that can
    run on a path under ``.charter/`` without going through `config`.
    """
    names = state_attribute_names()
    mods = load_package()
    found: dict[str, list[tuple[int, str]]] = {}
    for module, (path, tree) in mods.items():
        hits = violations(path.read_text(), names)
        if hits:
            found[str(path.relative_to(PACKAGE.parent))] = hits
    for f, hits in handed_violations(mods, names).items():
        merged = sorted(set(found.get(f, [])) | set(hits))
        found[f] = merged
    return found
