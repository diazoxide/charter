"""Personas: shared, committable role identities an agent can adopt.

A **persona** (e.g. ``devops``, ``qa``, ``developer``) is a role the agent
imitates. It lives in a **committed** directory ``personas/<name>/``:

- ``persona.md`` — frontmatter metadata plus a prose *charter* (the definition).
- ``memory/``    — **persistent** knowledge the persona has learned (committed,
  shared with the team): a ``MEMORY.md`` index plus one file per durable fact.
- ``refs/``      — curated docs / links / snippets for the role (committed).

Two more stores live **per-developer** under ``.charter/persona-state/`` (gitignored):

- **ephemeral** memory — session-scoped scratch the persona can jot down and that
  is deleted after the session (``ephemeral/<session>/<name>/``), and
- an activity **log** (``log/<name>.jsonl``).

So a persona has a 2×2 memory: *own vs shared* × *persistent vs ephemeral*. The
persona decides which quadrant a note belongs in (see :func:`remember`).

The active persona is resolved by precedence, mirroring workspaces:
``--persona`` flag → ``$CHARTER_PERSONA`` env → ``.charter/active-persona`` file → none.

The legacy flat layout ``personas/<name>.md`` still resolves for read, so old
checkouts keep working until migrated (``charter persona migrate``).
"""

from __future__ import annotations

import json
import os
import re
import shutil
import time
from pathlib import Path

from . import config, contain, mcpseen

_NAME_RE = re.compile(r"^[a-z0-9][a-z0-9._-]*$")


def valid_name(name: str) -> bool:
    # A leading '_' is reserved (the shared namespace) and rejected by the regex.
    return bool(name) and _NAME_RE.match(name) is not None


def reference_ok(ref: str) -> bool:
    """Can *ref*, read out of a committed file, name a persona at all?

    **The one place that answers this**, so the resolver and the operator's own check
    cannot disagree — which they did, and that divergence is #328's tell rather than a
    detail of it. `structural_errors` tested membership in a *name* set while `lineage`
    resolved a *path*, so `charter persona lint` printed

        ✗ frontdoor: uses: '<relative path>' — no such persona (dangling)

    about a reference `resolve()` loaded without complaint and whose `tools:` the
    PreToolUse gate then honoured. The signal an operator would actively check said the
    grant was inert while it was live (#329, #337).

    `valid_name` is the right rule here and a *forge* name would not be: charter mints
    persona names itself — `persona create` already enforces exactly this — so a reference
    outside that alphabet cannot name a persona this plane contains. That makes lint and
    the resolver agree by construction instead of by two checks kept in step by hand.

    The containment join is belt and braces on top, per :mod:`charter.contain`: it is the
    half that still holds if `_NAME_RE` is ever widened.
    """
    return valid_name(ref) and contain.child(config.PERSONAS_DIR, ref) is not None


# --------------------------------------------------------------------------- #
# paths: directory layout, with legacy flat-file fallback                      #
# --------------------------------------------------------------------------- #
def dir_of(name: str) -> Path:
    """The persona's directory ``personas/<name>/`` (may not exist yet)."""
    return config.PERSONAS_DIR / name


def def_path(name: str) -> Path:
    """The definition file. Prefers the directory layout
    (``personas/<name>/persona.md``), falls back to the legacy flat
    ``personas/<name>.md``; if neither exists, returns the *canonical new*
    location so writers create the directory layout."""
    d = dir_of(name) / "persona.md"
    if d.exists():
        return d
    flat = config.PERSONAS_DIR / f"{name}.md"
    if flat.exists():
        return flat
    return d


#: Back-compat alias — existing callers use ``persona.path(name)``.
path = def_path


def is_dir_layout(name: str) -> bool:
    return (dir_of(name) / "persona.md").exists()


def memory_dir(name: str, shared: bool = False) -> Path:
    """Persistent (committed) memory dir — own or the shared namespace."""
    base = config.PERSONAS_DIR / (config.SHARED_PERSONA if shared else name)
    return base / "memory"


def refs_dir(name: str, shared: bool = False) -> Path:
    base = config.PERSONAS_DIR / (config.SHARED_PERSONA if shared else name)
    return base / "refs"


def _session_id(session: str | None = None) -> str:
    """This session's bucket name — see :mod:`charter.session`, which owns the question."""
    from . import session as _session
    return _session.bucket(session)


def ephemeral_dir(name: str, shared: bool = False, session: str | None = None) -> Path:
    """Ephemeral (gitignored, session-scoped) scratch dir — own or shared. The
    whole session directory is pruned by :func:`gc_ephemeral`."""
    ns = config.SHARED_PERSONA if shared else name
    return config.PERSONA_STATE_DIR / "ephemeral" / _session_id(session) / ns


def index_of(mem_dir: Path) -> Path:
    return mem_dir / "MEMORY.md"


# --------------------------------------------------------------------------- #
# listing / loading                                                            #
# --------------------------------------------------------------------------- #
def list_personas() -> list[str]:
    d = config.PERSONAS_DIR
    if not d.exists():
        return []
    names: set[str] = set()
    for p in d.glob("*.md"):  # legacy flat files
        if p.stem.lower() != "readme":
            names.add(p.stem)
    for sub in d.iterdir():  # directory layout
        if sub.is_dir() and not sub.name.startswith("_") and (sub / "persona.md").exists():
            names.add(sub.name)
    return sorted(names)


def parse(text: str) -> tuple[dict, str]:
    """Split frontmatter markdown into (metadata, charter body).

    Minimal, dependency-free: flat ``key: value`` lines. Good enough for our
    small, controlled frontmatter (``role``, ``vault``, …).
    """
    meta: dict = {}
    body = text
    if text.startswith("---"):
        parts = text.split("---", 2)
        if len(parts) >= 3:
            frontmatter, body = parts[1], parts[2]
            for line in frontmatter.strip().splitlines():
                if ":" not in line:
                    continue
                key, _, value = line.partition(":")
                key, value = key.strip(), value.strip()
                if key:
                    meta[key] = value
    return meta, body.strip()


def definition_refusal(name: str) -> str | None:
    """Why this persona's definition file must not be read, or ``None``.

    The path half of what :func:`reference_ok` does for the name half, and split out for
    the same reason: :func:`load` and :func:`structural_errors` must not be able to
    disagree about it. When they did — lint calling a live grant dangling — the signal an
    operator would actively check was the one that lied (#329, #337).

    Both questions are asked, because they catch different links. The *directory* resolving
    out of the plane's data is the case where `persona.md` is an ordinary file and there is
    nothing about it to object to; the *file* resolving out is #336's demonstration,
    ``persona.md`` → ``../../.charter/vaults/…``, which `sync-agents` then writes into a
    sub-agent's system prompt while `pretooluse-read` denies the agent that same read.
    """
    p = def_path(name)
    if not p.exists():
        return None
    return contain.dir_refusal(p.parent) or contain.file_refusal(p)


def load(name: str) -> dict | None:
    """The persona's OWN (unmerged) definition. For the effective persona with
    inheritance applied, use :func:`resolve`.

    **The choke point for every reference read out of a file.** `lineage`, `resolve`,
    `tools_of`, `vault_of` and `effective_tools` all reach a definition through here, so
    refusing a non-name here is what stops `extends:`/`uses:`/`borrows:` naming a file
    outside `personas/` — rather than four guards at four call sites, three of which stay
    correct. Returns None for a refused reference exactly as it does for one that names
    nothing: the caller has no reason to tell a typo from an escape, and
    `structural_errors` is where the difference is spelled out for the human.
    """
    if not reference_ok(name):
        return None
    p = def_path(name)
    if not p.exists() or definition_refusal(name):
        return None
    meta, charter = parse(p.read_text())
    meta.setdefault("name", name)
    return {"meta": meta, "charter": charter}


# --------------------------------------------------------------------------- #
# inheritance — a persona may ``extends:`` a parent, deriving its charter + tools #
# and layering its own on top. Charters concatenate (parent base → child adds);  #
# tools/agent-tools/uses union; scalar fields (role, model, vault, …) child wins, #
# else inherited. Chains resolve root→child; cycles are broken.                   #
# --------------------------------------------------------------------------- #
def _csv_set(v) -> set[str]:
    return {x.strip() for x in (v or "").strip("[]").split(",") if x.strip()}


def _csv_list(v) -> list[str]:
    return [x.strip() for x in (v or "").strip("[]").split(",") if x.strip()]


#: The sidecar naming a persona's MCP servers, same schema as `.mcp.json` plus a
#: charter-only `secrets` map per server. A separate FILE rather than frontmatter because
#: `parse` above is line-based and charter carries no runtime dependencies: nested YAML can
#: be emitted (JSON is valid YAML) but not read. A sidecar also takes a server's own README
#: snippet unchanged, which is the form these arrive in.
MCP_FILE = "mcp.json"

#: A persona's own executables. The fifth capability, alongside memory, refs, ephemeral
#: scratch and `mcp.json` — and the one whose absence kept a whole Claude Code plugin alive
#: as a file carrier (#283).
#:
#: Deliberately NOT put on PATH, because it cannot be: a `PreToolUse` hook decides whether a
#: Bash call runs, not what environment it runs in, and wrapping every Bash call to inject
#: one is the takeover of a host mechanism ADR 0014 exists to refuse. Scripts are invoked by
#: path — which already worked; what was missing is charter knowing they are there, so it can
#: surface them to the agent and vouch for them at the guard.
BIN_DIR = "bin"


def bin_dir(name: str) -> Path:
    """Where *name*'s own executables live. Not created eagerly — an empty `bin/` in every
    persona is noise in a directory a human reads."""
    return dir_of(name) / BIN_DIR


def bin_scripts(name: str) -> dict:
    """``{script_name: path}`` this persona can run, inheritance applied.

    Unioned along ``lineage()`` — the same chain ``mcp_servers`` uses, and for the same
    reason: a child that silently lost what its parent declared would be a surprise. Child
    wins on a name collision, so a persona can override an inherited script.

    ``uses:``/``borrows:`` deliberately do NOT carry scripts. Borrowing is a grant to run
    another persona's *declared tools*; shipping its code is a different thing, and conflating
    them is how #257 got the tool grant wrong in the first place.

    Executable files only. Git preserves the mode bit, so a file committed without ``+x``
    reaches every clone unable to run — failing at the moment somebody needs it, with a
    shell error that points nowhere near here. `lint` names those separately.
    """
    out = {}
    # REVERSED: `lineage()` is child-first, so iterating it directly would let the most
    # distant ancestor overwrite the child — the opposite of the rule. `mcp_servers` below
    # had exactly that bug (#296); this is the shape both now share, and the reason to reach
    # for `reversed()` in any future union along the chain.
    for anc in reversed(lineage(name)):
        d = bin_dir(anc)
        if not d.is_dir():
            continue
        for f in sorted(d.iterdir()):
            if f.is_file() and os.access(f, os.X_OK):
                out[f.name] = f
    return out


def bin_issues(name: str) -> list[tuple[str, str]]:
    """A file in `bin/` that cannot run. Its own check because the failure is invisible
    until dispatch and reads as a broken script rather than a missing mode bit."""
    d = bin_dir(name)
    if not d.is_dir():
        return []
    return [("warn", f"`{BIN_DIR}/{f.name}` is not executable — it will fail at the moment "
                     f"it is needed; `chmod +x {f.relative_to(config.ROOT)}`")
            for f in sorted(d.iterdir())
            if f.is_file() and not os.access(f, os.X_OK)]


def mcp_servers(name: str) -> dict:
    """``{server_name: config}`` a persona declares, inheritance applied.

    Unioned along the lineage the way ``tools``/``uses`` are — parent first, child wins on
    a name collision. A child that silently lost the server its parent declared would be a
    surprise, and this is the rule the rest of the frontmatter already follows.

    Never raises. The file is hand-edited, and a stray comma in it must not take down
    `sync-agents` for every persona in the plane.
    """
    out = {}
    # REVERSED, and that is the whole fix for #296. `lineage()` is child-first, so feeding it
    # straight into `dict.update` — last write wins — let the most distant ancestor overwrite
    # the child: the exact inversion of the rule stated above. Nothing surfaced it, because
    # the child's entry was parsed and applied and then thrown away, so `sync-agents`
    # succeeded and the generated agent simply carried somebody else's server.
    for anc in reversed(lineage(name)):
        declaration = dir_of(anc) / MCP_FILE
        if contain.file_refusal(declaration):
            continue          # same promise as the `except` below, kept before the open
        try:
            doc = json.loads(declaration.read_text())
            servers = doc.get("mcpServers") or {}
        except (OSError, ValueError, AttributeError):
            continue
        if isinstance(servers, dict):
            out.update(servers)
    return out


def mcp_render_entry(name: str, vault: str | None, entry: dict) -> dict:
    """One declared server, as the generated agent should carry it.

    A ``secrets`` map — ``{ENV_VAR: vault-key}`` — turns the entry into a
    ``charter secret exec … --exec -- <original command>`` invocation, so the value reaches
    the server's environment without ever reaching a context window. The env var names
    belong to the third-party server, which is why they are declared rather than inferred:
    charter cannot know them, and injecting every key in the vault would hand a server
    every credential the persona holds instead of the two it needs.

    ``--exec`` is not optional. It replaces the process rather than capturing it, and an
    MCP stdio server never returns — the capturing form would hang holding output nobody
    reads.

    A server with no ``secrets`` is passed through untouched: dragging a credential-free
    server through charter would buy nothing and add a process.
    """
    out = {k: v for k, v in entry.items() if k not in ("secrets", "secret_files")}
    secrets = entry.get("secrets") or {}
    # A separate key, not a marker inside `secrets`, because these are different MECHANISMS
    # rather than two spellings of one: `secrets` puts a VALUE in the environment,
    # `secret_files` materialises a 0600 file and puts its PATH there. Google ADC needs the
    # second — `GOOGLE_APPLICATION_CREDENTIALS` takes a path, not a value — and before this a
    # persona whose servers authenticate that way could not declare them at all (#190).
    # A reader should see which mechanism is in play without decoding a value.
    files = entry.get("secret_files") or {}
    if not (secrets or files) or not vault:
        return out
    # The committed sidecar chooses `command`, and this function is where that choice
    # becomes "…and it receives the vault's value" (#330). The gate belongs HERE rather
    # than in `sync-agents`, even though `sync-agents` is the only caller today: the
    # decision being guarded is this one, and a guard sitting one frame above the decision
    # is a guard the next caller forgets to ask for.
    #
    # NOT an allowlist of commands, which is how #317 was closed on a news `check:`. That
    # worked because a `check:` names a charter subcommand — a closed grammar with an
    # enumerable answer. An MCP `command` is an arbitrary binary followed by arbitrary
    # `args`, so a list holding the launchers real servers use (`npx`, `uvx`, `docker`) is
    # walked straight past by `args` alone, and a list excluding them refuses every server
    # anyone actually runs. See `mcpseen` for the full argument.
    if mcpseen.fingerprint(vault, entry) not in mcpseen.approved(name):
        return out
    args = ["secret", "exec", vault]
    for env_name, key in secrets.items():
        args += ["--env", f"{env_name}={key}"]
    for env_name, key in files.items():
        args += ["--file", f"{env_name}={key}"]
    # `--stream` whenever a file is involved: `--exec` replaces charter, so nothing would
    # survive to shred the tempfile. Streaming is unaffected — a forked child inherits this
    # process's descriptors — so the only thing given up is process replacement, which is
    # precisely what made cleanup impossible.
    args += ["--stream" if files else "--exec", "--"]
    if out.get("command"):
        args.append(out["command"])
    args += list(out.get("args") or [])
    out["command"] = "charter"
    out["args"] = args
    return out


def mcp_credentialed(name: str) -> list[tuple[str, dict, str]]:
    """``(server, entry, fingerprint)`` for every server of *name* that would carry a
    credential — i.e. declares ``secrets``/``secret_files`` against a real vault.

    The list `sync-agents --approve-mcp` records and the list it reports on are both
    derived from this one, so "what you were shown" and "what got approved" cannot drift
    apart — which is the failure mode of a consent prompt that computes its own list.
    """
    vault = (resolve(name) or {}).get("meta", {}).get("vault")
    out = []
    for server, entry in sorted(mcp_servers(name).items()):
        fp = mcpseen.fingerprint(vault, entry)
        if fp:
            out.append((server, entry, fp))
    return out


def mcp_withheld(name: str) -> list[tuple[str, dict]]:
    """The credentialed servers this operator has NOT approved — what `mcp_render_entry`
    is about to render without its vault wrapper."""
    ok = mcpseen.approved(name)
    return [(s, e) for s, e, fp in mcp_credentialed(name) if fp not in ok]


def lineage(name: str) -> list[str]:
    """The inheritance chain, child-first: ``[name, parent, grandparent, …]``.
    Cycle-safe (stops if it revisits a persona)."""
    out, seen, cur = [], set(), name
    while cur and cur not in seen:
        d = load(cur)
        if not d:
            break
        out.append(cur)
        seen.add(cur)
        cur = (d["meta"].get("extends") or "").strip() or None
    return out


def resolve(name: str) -> dict | None:
    """The EFFECTIVE persona with inheritance applied: merged meta + concatenated
    charter. Returns ``{meta, charter, lineage}`` (lineage child-first), or None."""
    chain = lineage(name)
    if not chain:
        return None
    meta: dict = {}
    tools, agent_tools, uses = [], [], []  # ordered + deduped (parent first, child appends)
    charter_parts, prev = [], None

    def _extend(dst, vals):
        for v in vals:
            if v and v not in dst:
                dst.append(v)

    for a in reversed(chain):  # root → child, so the child's values win
        d = load(a)
        m = d["meta"]
        for k, v in m.items():
            if k in ("tools", "agent-tools", "uses", "extends") or not v:
                continue
            meta[k] = v  # later (more-derived) wins
        _extend(tools, _csv_list(m.get("tools")))
        _extend(agent_tools, _csv_list(m.get("agent-tools")))
        _extend(uses, [u for u in _csv_list(m.get("uses")) if u != name])
        c = (d["charter"] or "").strip()
        if c:
            charter_parts.append(c if prev is None
                                 else f"\n\n---\n\n### ⤷ `{a}` extends `{prev}` — its own charter\n\n{c}")
        prev = a
    meta["name"] = name
    if tools:
        meta["tools"] = ", ".join(tools)
    if agent_tools:
        meta["agent-tools"] = ", ".join(agent_tools)
    if uses:
        meta["uses"] = ", ".join(uses)
    return {"meta": meta, "charter": "".join(charter_parts), "lineage": chain}


# --------------------------------------------------------------------------- #
# active persona resolution                                                    #
# --------------------------------------------------------------------------- #
def default_persona() -> str | None:
    """The committed, team-wide default persona (``personas/.default``) — adopted when
    nothing else is selected. Shared/versioned (unlike the local ``.charter/active-persona``);
    ignored if it names a persona that no longer exists."""
    p = config.PERSONAS_DIR / ".default"
    # Gated like every other committed file charter reads: this one answers "who am I" on
    # every turn, so a FIFO here hangs the status line rather than costing a briefing.
    if contain.file_refusal(p):
        return None
    try:
        val = p.read_text().strip()
    except OSError:
        return None
    # `reference_ok` before `.exists()`: this dotfile is committed, so the value is a
    # teammate's, and "a path that exists" was never the question being asked (#337).
    return val if val and reference_ok(val) and def_path(val).exists() else None


#: The outbound postures a persona may declare, least → most insistent. `off` is what an
#: absent or unrecognised `routing:` means: a typo must not silently switch a gate on, and
#: an upgrade must change nothing for a plane that has declared nothing.
ROUTING_LEVELS = ("off", "advise", "require")


def routing_level(name: str) -> str:
    """How insistently *name* hands work away — its ``routing:``, inheritance-merged.

    ``delegate-when`` says what a persona accepts; this says when it should stop doing the
    work itself. Two directions, two words, deliberately unalike: a field called
    ``delegation`` beside ``delegate-when`` would be one letter and one glance apart from
    its own opposite.

    Read from the ACTING persona only — the one whose session this is. That is the whole
    reason there is no plane-level setting to merge with here: a floor would apply to
    personas that never declared anything, which is the action-at-a-distance this design
    was reshaped to avoid.
    """
    try:
        r = resolve(name)
    except Exception:
        return "off"
    if not r:
        return "off"
    val = str(r["meta"].get("routing") or "").strip().lower()
    return val if val in ROUTING_LEVELS else "off"


def routes_to(name: str) -> list[str]:
    """Personas *name* considers first — its ``routes-to:``, inheritance-merged.

    Priority, never restriction: it reorders the roster and removes nobody. A restricting
    form would silently hide every persona created after the line was written, and nothing
    would report the omission — the same failure shape `lint` had to grow a dangling-`uses:`
    check for.
    """
    r = resolve(name)
    return _csv_list(r["meta"].get("routes-to")) if r else []


def roster_for(active: str | None) -> list[dict]:
    """Every persona this session could hand work to, as ``{name, role, delegate_when,
    last_dispatched}`` — the facts charter owns, and nothing more.

    charter does not decide which of these owns the prompt (ADR 0016). It states who
    exists, what each one claims, and when each was last dispatched; the reader routes.

    Excludes the acting persona (routing to yourself is noise in a block that cannot
    afford any) and drafts (charter generates no sub-agent for a draft, so offering one
    would advertise a route that does not exist).
    """
    from . import dispatch
    rows = []
    for n in list_personas():
        if n == active or is_draft(n):
            continue
        try:
            r = resolve(n) or {}
            meta = r.get("meta", {})
        except Exception:
            continue
        rows.append({
            "name": n,
            "role": meta.get("role") or n,
            "delegate_when": (meta.get("delegate-when") or "").strip(),
            "last_dispatched": dispatch.last_seen(n),
        })
    first = routes_to(active) if active else []
    order = {n: i for i, n in enumerate(first)}
    # Declared order first, then everyone else alphabetically — a stable order matters on a
    # block that reappears: a roster that reshuffles between prompts reads as new content.
    rows.sort(key=lambda r: (order.get(r["name"], len(order)), r["name"]))
    return rows


def declared_default() -> str | None:
    """The front door this control plane DECLARES — ``charter.toml``'s ``[persona] default``.

    Outranks the legacy ``personas/.default`` dotfile, which keeps resolving so no plane
    that adopted it breaks. The move is about findability, not capability: the dotfile is
    invisible to ``ls``, appears in no documentation page, and was used by nobody —
    including this repo, whose own front door resolved through a gitignored local file
    instead (#255). ``charter.toml`` is the file a consumer already opens to understand
    their plane, and ``[workspace] default`` is already in it.

    Validated against what exists, exactly like :func:`default_persona`: a declaration
    naming a persona that was renamed or deleted resolves to *nothing* rather than to a
    broken identity. Saying so out loud is `doctor`'s job — silence here is the fail-toward-
    no-change half, not the whole answer.

    Never raises. A malformed or too-new ``charter.toml`` is a real error, and every actual
    command surfaces it through :func:`instance.load`; but this rung is read by hooks on
    every session start and every status-line paint, where the rule is that a hook may cost
    a session its briefing and never its turn.
    """
    from . import instance as _instance
    try:
        val = _instance.default_persona_of(_instance.load(config.ROOT))
    except Exception:
        return None
    # Same as `default_persona`: `charter.toml` is committed, so this rung picks the
    # session's acting identity out of a teammate-authorable file. Existence of a path was
    # the wrong question — a reference climbing out of `personas/` named a file the plane
    # does not contain, and `resolve()` merged its `vault:`, `role:` and `tools:` (#337).
    return val if val and reference_ok(val) and def_path(val).exists() else None


def _pointer_files() -> tuple[Path | None, Path | None]:
    """``(session pointer, terminal pointer)`` for right now — either may be ``None``.

    Mirrors workspaces exactly (``.charter/sessions/<id>.workspace`` and
    ``.charter/terminals/<id>.workspace``), because the failure it prevents is the same
    one, one noun over: a single plane-wide file meant `charter persona use forge` in one
    pane changed the persona in every other pane and every future session, which is the
    opposite of what a fleet of parallel personas is for (#255).
    """
    from . import session as _session
    sid = _session.current()
    tid = _session.terminal()
    return (config.SESSIONS_DIR / f"{sid}.persona" if sid else None,
            config.TERMINALS_DIR / f"{tid}.persona" if tid else None)


def _read_pointer(f: Path | None) -> str | None:
    if f is None:
        return None
    try:
        return f.read_text().strip() or None
    except OSError:
        return None


def _resolved(explicit: str | None = None) -> tuple[str | None, str]:
    """``(persona, where it came from)`` — the whole precedence, decided ONCE.

    :func:`resolve_active` and :func:`source` are two questions about a single decision,
    and they used to walk the rungs separately. Two ladders answering one question is the
    shape this repo's own convention warns about: adding a rung to one and forgetting the
    other yields a session that adopts a persona while reporting it came from nowhere.
    """
    if explicit:
        return explicit, "--persona"
    env = os.environ.get("CHARTER_PERSONA")
    if env:
        return env.strip(), "$CHARTER_PERSONA"
    sf, tf = _pointer_files()
    val = _read_pointer(sf)
    if val:
        return val, "session"
    val = _read_pointer(tf)
    if val:
        return val, "terminal"
    f = config.ACTIVE_PERSONA_FILE
    if f.exists():
        val = f.read_text().strip()
        if val:
            return val, "active-file"
    declared = declared_default()
    if declared:
        return declared, "charter.toml"
    committed = default_persona()
    if committed:
        return committed, "committed-default"
    return None, "none"


def resolve_active(explicit: str | None = None) -> str | None:
    """Active persona by precedence: ``--persona`` → ``$CHARTER_PERSONA`` → local
    ``.charter/active-persona`` (``charter persona use``) → declared ``charter.toml``
    ``[persona] default`` → committed ``personas/.default`` → none."""
    return _resolved(explicit)[0]


def source(explicit: str | None = None) -> str:
    return _resolved(explicit)[1]


def set_active(name: str) -> str:
    """Select *name* for this session and this pane. Returns the reach of what was written
    (``session`` | ``terminal`` | ``plane``) so a caller can say how long it will last.

    The plane-wide file is written only when there is neither a session id nor a pane id —
    a bare shell with nothing to key on. That is the one case where it is still the right
    answer, and it is why the file is kept rather than removed.
    """
    config.STATE_DIR.mkdir(parents=True, exist_ok=True)
    sf, tf = _pointer_files()
    for f in (sf, tf):
        if f is not None:
            f.parent.mkdir(parents=True, exist_ok=True)
            f.write_text((name or "") + "\n")
    if sf is None and tf is None:
        config.ACTIVE_PERSONA_FILE.write_text((name or "") + "\n")
    try:
        from . import trace
        trace.record("persona-use", persona=name)
    except Exception:
        pass
    # The terminal pointer outlives the session one, so it names the longest-lived thing
    # that actually landed — the same reasoning `workspace.use` records for its own return.
    return "terminal" if tf is not None else "session" if sf is not None else "plane"


def clear_active() -> None:
    """Drop this session's and this pane's selection, and the plane-wide file with them.

    All three, because they are rungs of one ladder: clearing only the top rung would hand
    the session straight back to a lower one, and "cleared" would be a lie the very next
    command exposes.
    """
    sf, tf = _pointer_files()
    for f in (sf, tf, config.ACTIVE_PERSONA_FILE):
        try:
            if f is not None and f.exists():
                f.unlink()
        except OSError:
            pass


# --------------------------------------------------------------------------- #
# tools + reuse                                                                #
# --------------------------------------------------------------------------- #
def tools_of(name: str) -> set[str]:
    """Commands the persona is allowed to run without a prompt (its ``tools:``),
    **including inherited** ones (union across the ``extends`` chain)."""
    r = resolve(name)
    return _csv_set(r["meta"].get("tools")) if r else set()


def uses_of(name: str) -> list[str]:
    """Other personas this one may reuse — its ``uses:`` field (inherited-inclusive).
    Reusing a persona means it may read that persona's vault, run its tools, and
    delegate to its sub-agent (see the charter it's generated into)."""
    r = resolve(name)
    return _csv_list(r["meta"].get("uses")) if r else []


#: A `borrows:` value meaning "deliberately nothing" — the same spelling `vault: none`
#: already uses for the same idea, so the vocabulary stays one vocabulary.
BORROWS_NONE = "none"


def borrows_of(name: str) -> list[str] | None:
    """Personas whose tools and vault this one may use — its ``borrows:``, or ``None`` when
    the key is absent.

    ``None`` and ``[]`` are different answers and the difference is the whole feature:
    absent means "I have not opted in, keep the legacy `uses:` grant", while
    ``borrows: none`` means "I borrow nothing". Collapsing them would either break every
    existing plane or make opting out unsayable.
    """
    r = resolve(name)
    if not r or "borrows" not in r["meta"]:
        return None
    vals = _csv_list(r["meta"].get("borrows"))
    return [v for v in vals if v != BORROWS_NONE]


def effective_tools(name: str) -> set[str]:
    """The persona's own auto-approved tools, plus those it may borrow (one level).

    Which personas those are depends on whether this persona has opted into the split:

    * ``borrows:`` declared → its tools come from that list, and ``uses:`` is a routing
      edge only. This is the point of the field. ``uses:`` granted vault access, tool
      auto-approval and delegation in one word, and the middle grant is why delegating
      always lost: a front door declaring ``uses: forge, release`` could do both personas'
      work with both personas' tools and never pay a prompt, while handing the work over
      cost a dispatch and the context with it (#257).
    * ``borrows:`` absent → the legacy grant, unchanged. Fails toward no change, per
      persona: opting one persona in must never alter another's permissions, which is
      exactly what a plane-wide switch would have done.
    """
    tools = tools_of(name)
    borrows = borrows_of(name)
    for other in (uses_of(name) if borrows is None else borrows):
        tools |= tools_of(other)
    return tools


def _enabled_plugin_names() -> set[str]:
    """Plugin names enabled in the committed .claude/settings.json (the part before '@')."""
    try:
        import json
        data = json.loads((config.ROOT / ".claude" / "settings.json").read_text())
        return {str(k).split("@", 1)[0] for k in (data.get("enabledPlugins") or {})}
    except Exception:
        return set()


#: Process-global memo for the plugin-cache walk below. A sentinel rather than None,
#: because "no plugin cache on this machine" is itself a cacheable answer.
_SKILLS_UNSET = object()
_SKILLS_CACHE: object = _SKILLS_UNSET


def _reset_skill_cache() -> None:
    """Forget the memoised plugin-cache walk. For tests, and for any caller that
    would install a plugin mid-process."""
    global _SKILLS_CACHE
    _SKILLS_CACHE = _SKILLS_UNSET


def _installed_skills() -> dict[str, bool] | None:
    """:func:`_walk_installed_skills`, memoised for the life of the process.

    The walk reads every ``SKILL.md`` under ``~/.claude/plugins`` — 96 skills and
    ~27ms on a real machine — and :func:`lint` calls it once per persona, so a
    13-persona roster paid it 13 times: 358ms of a 364ms sweep. That cost is why
    ``doctor`` could not afford to lint the roster at all. The plugin cache cannot
    change during a single command, so one walk is enough.
    """
    global _SKILLS_CACHE
    if _SKILLS_CACHE is _SKILLS_UNSET:
        _SKILLS_CACHE = _walk_installed_skills()
    return _SKILLS_CACHE  # type: ignore[return-value]


def _skill_roots() -> list:
    """Every directory the harness resolves a skill from, in the order it reads them.

    Three, not one. Walking only the plugin cache made two charter commands disagree about
    what a skill is (#286): `charter browser install` writes
    `.claude/skills/playwright-cli/` — that path chosen because the harness reads project
    skills from there — and `persona lint` then called it "not installed here … Remove it or
    install the plugin". There is no plugin to install; charter deliberately does not vendor
    those pages, which is the whole reason `browser install` exists.

    The lint's own justification is that a declared skill costs context on every dispatch, so
    a dead entry is paid forever for nothing. That argues for accuracy about what EXISTS, not
    about what happens to be packaged.
    """
    from pathlib import Path as _P
    home = _P.home() / ".claude"
    return [home / "plugins", home / "skills", _P(config.ROOT) / ".claude" / "skills"]


def _walk_installed_skills() -> dict[str, bool] | None:
    """Map each available skill's leaf-name → is it **model-invokable** (i.e. not
    ``disable-model-invocation: true``), scanned from :func:`_skill_roots`.

    Returns None when the plugin cache is absent, so lint can skip gracefully on a fresh
    clone / CI without plugins. Keyed on the PLUGIN cache specifically, even though project
    skills are now walked too: without it charter cannot see plugin-provided skills at all,
    so checking anyway would report every one of them as missing — confidently wrong, which
    is worse than silent."""
    from pathlib import Path as _P
    roots = _skill_roots()
    if not roots[0].exists():
        return None
    out: dict[str, bool] = {}
    for sk in [f for r in roots if r.exists() for f in r.rglob("SKILL.md")]:
        try:
            lines = sk.read_text().splitlines()
        except OSError:
            continue
        if not lines or lines[0].strip() != "---":
            continue
        skname, dmi = None, False
        for ln in lines[1:]:
            if ln.strip() == "---":
                break
            if ln.startswith("name:"):
                skname = ln.split(":", 1)[1].strip().strip("\"'")
            elif ln.startswith("disable-model-invocation:"):
                dmi = ln.split(":", 1)[1].strip().lower() == "true"
        skname = skname or sk.parent.name
        out[skname] = out.get(skname, False) or (not dmi)  # invokable if ANY copy is
    return out or None


# `plugin:skill` inside backticks = a skill an AGENT invokes (must be model-invokable);
# the `/skill` slash form is a human step (allowed to be human-only). Charters follow this.
_SKILL_REF_RE = re.compile(r"`([a-z0-9][a-z0-9-]*):([a-z0-9][a-z0-9-]*)`")


def _skill_ref_issues(charter: str) -> list[tuple[str, str]]:
    """Every ```plugin:skill``` an enabled plugin's charter names for agent use must
    resolve to an installed, model-invokable skill — else it's a dead/human-only name that
    a sub-agent can't invoke (the exact rot that shipped un-caught before)."""
    skills = _installed_skills()
    if skills is None:  # no plugin cache here → can't verify; skip
        return []
    plugins = _enabled_plugin_names()
    if not plugins:
        return []
    out = []
    for m in _SKILL_REF_RE.finditer(charter or ""):
        plug, skill = m.group(1), m.group(2)
        if plug not in plugins:
            continue  # not a reference to an enabled plugin's skill
        if skill not in skills:
            out.append(("error", f"charter names `{plug}:{skill}` — not an installed skill "
                                 "(renamed/removed upstream?); fix it or pin the marketplace"))
        elif not skills[skill]:
            out.append(("error", f"charter names `{plug}:{skill}` for agent use, but it's "
                                 "human-only (disable-model-invocation) — a sub-agent can't "
                                 "invoke it; use the /slash form for a human step"))
    return out


#: Frontmatter values are plain strings — :func:`parse` does no type coercion — so
#: ``draft: false`` arrives as the string ``"false"``, which is *truthy*. Every read of
#: the flag goes through :func:`is_draft` for exactly this reason.
_DRAFT_TRUE = {"true", "yes", "1", "on"}


def is_draft(name: str) -> bool:
    """Is this persona's charter still unfinished, and therefore undispatchable?

    ``charter persona create`` stamps ``draft: true``; the author removes the line when
    the charter says something real. While it is set, no sub-agent is generated — a
    generated agent file *is* a sub-agent's system prompt, and shipping an unwritten
    charter there tells an agent its responsibilities are whatever the scaffold said.

    Adoption is deliberately still allowed: ``persona use`` injects only the identity
    line and a pointer to ``charter persona show``, never the charter body, and a human
    is reading. That asymmetry is the whole rule.

    Resolved, not own — a child ``extends:``-ing a draft parent concatenates the
    parent's unfinished charter into its own dispatched prompt, so it inherits the
    label like any other scalar (and may override it with an explicit ``draft: false``).
    """
    d = resolve(name)
    if not d:
        return False
    return str(d["meta"].get("draft", "")).strip().lower() in _DRAFT_TRUE


def _reference_problem(field: str, ref: str, known: set[str]) -> tuple[str, str] | None:
    """Why *ref* cannot be used as a persona reference, or None when it can.

    Two failures, said two ways, because they send the reader to two different places. A
    name that is simply absent is a typo or a rename — "dangling" is right, and hunting
    for the persona is the fix. A reference that is a *path* is not a name at all: no
    amount of looking for that persona will help, and the fix is in the file. #328's whole
    shape is a distinction like this one being collapsed, so it gets its own sentence.

    Shares :func:`reference_ok` with :func:`load`, which is what makes lint and the
    resolver structurally unable to disagree again — the property whose absence let the
    gate honour a grant `lint` was calling dangling in the same run.
    """
    if not reference_ok(ref):
        return ("error", f"{field}: {contain.refusal(ref)}")
    if ref not in known:
        return ("error", f"{field}: '{ref}' — no such persona (dangling)")
    return None


def structural_errors(name: str, known: set[str] | None = None) -> list[tuple[str, str]]:
    """The ``error``-level half of :func:`lint`: references that do not resolve.

    A dangling ``extends:``/``uses:`` or an inheritance cycle makes a persona broken
    rather than untidy — the resolver cannot build it. Split out from :func:`lint`
    (which calls this, so there is still one implementation) because the status line
    needs exactly this subset on **every turn** and none of the rest: no ``vault_of``,
    no role/delegate-when, no unknown-key scan, and in particular no import of
    ``commands_persona`` to fetch the key whitelist.

    ``known`` lets a caller sweeping the whole roster pass ``list_personas()`` once
    instead of paying for it per persona — 1.6ms of a 5.2ms 13-persona sweep.
    """
    allnames = known if known is not None else set(list_personas())
    issues: list[tuple[str, str]] = []
    refused = definition_refusal(name)
    if refused:
        # First, and on its own terms: every check below reads through `load`, which
        # returns None for this persona, so without this line a definition charter
        # DECLINED to read is indistinguishable from one that says nothing — and the
        # operator is looking straight at the signal that would have told them (#336).
        issues.append(("error", f"persona.md: {refused}"))
    for u in uses_of(name):
        problem = _reference_problem("uses", u, allnames)
        if problem:
            issues.append(problem)
    for b in borrows_of(name) or ():
        problem = _reference_problem("borrows", b, allnames)
        if problem:
            issues.append(problem)
    d = load(name)
    ext = ((d["meta"] if d else {}).get("extends") or "").strip()
    if ext:
        problem = _reference_problem("extends", ext, allnames)
        if problem:
            issues.append(problem)
    cycle = _inherits_cycle(name)
    if cycle:
        issues.append(("error", f"extends: inheritance cycle ({cycle})"))
    return issues


def declared_skills(name: str) -> list[str]:
    """The skills this persona is accountable for, from its ``skills:`` frontmatter.

    Not an allowlist — the host has no such thing. `skills:` **preloads** the full text of
    each skill into the sub-agent's context at startup, so a declared skill is standing
    equipment the persona begins holding rather than something it might discover. Access is
    all-or-nothing and lives elsewhere: ``Skill`` in or out of ``agent-tools``.

    The prose and this list answer different questions, which is what keeps them from being
    the duplicate index ADR 0010 warns about. Prose says *when and how* to use a skill; this
    says *what the persona starts holding*. Only this one can be acted on — charter emits it
    into the generated agent, and prose can only mention.

    Comma-separated, matching ``uses:`` — one frontmatter idiom for one shape of list.
    """
    d = load(name)
    if not d:
        return []
    raw = (d["meta"].get("skills") or "").strip()
    return [t.strip() for t in raw.split(",") if t.strip()]


def declared_skill_issues(name: str) -> list[tuple[str, str]]:
    """Lint a persona's DECLARED skills, on the same terms as the ones its prose names.

    A declared skill that does not resolve is worse than a prose mention that does not: the
    prose is advice a reader can route around, while this is emitted into the agent, so the
    failure is silent at the moment it matters.

    Costs context on every dispatch, which is the part worth being strict about — `skills:`
    injects full content at startup, so a dead entry is paid forever for nothing.
    """
    return _declared_skill_issues(declared_skills(name))


def _declared_skill_issues(names: list[str]) -> list[tuple[str, str]]:
    """The check itself, over already-resolved names — so it can be tested against a
    controlled skill tree rather than a persona that has to exist on disk."""
    if not names:
        return []
    skills = _installed_skills()
    if skills is None:  # no plugin cache here → cannot verify; skip, as `_skill_ref_issues` does
        return []
    out: list[tuple[str, str]] = []
    for ref in names:
        leaf = ref.split(":", 1)[-1]
        if leaf not in skills:
            # ADR 0009 — name what was actually checked. The old remedy ("install the
            # plugin") was impossible for a skill charter generates INTO the plane, and sent
            # the reader hunting for a package that does not exist (#286).
            out.append(("error", f"declares skill `{ref}` — not found in ~/.claude/plugins, "
                                 f"~/.claude/skills or this plane's .claude/skills. It is "
                                 f"preloaded into the agent, so this fails silently at "
                                 f"dispatch. Install it, generate it, or drop the entry"))
        elif not skills[leaf]:
            out.append(("warn", f"declares skill `{ref}`, which is human-only "
                                f"(disable-model-invocation) — preloading its text is "
                                f"harmless but the agent can never invoke it"))
    return out


def lint(name: str, deep: bool = True) -> list[tuple[str, str]]:
    """Config-correctness checks for one persona → ``[(level, message)]`` where
    level is ``'error'`` (dangling ``uses:``, or a charter naming a human-only/unknown
    plugin skill for agent use) or ``'warn'`` (missing role/vault/delegate-when, or an
    unfinished ``draft:``). The agent-in-sync check lives in the CLI (it needs the
    renderer). A deterministic eval of the persona config — the routing/guard behaviours
    are covered by the test suite.

    ``deep=False`` drops the skill-reference check, the one part that walks the plugin
    cache. Everything else is frontmatter arithmetic (~0.1ms per persona), which is what
    lets the status line — rendered on *every turn* — show roster health at all. One
    implementation with a flag, rather than a second "cheap health" function that would
    drift from this one the first time a check was added to only one of them.
    """
    d = load(name)
    if not d:
        # WHY, when there is a why. "does not load" about a `persona.md` sitting right
        # there sends the reader looking for a missing file; the refusal names the path
        # charter actually resolved to, which is the whole defect (#336).
        refused = definition_refusal(name)
        return [("error", f"persona.md: {refused}" if refused
                 else f"persona '{name}' does not load")]
    meta = d["meta"]
    issues: list[tuple[str, str]] = []
    if deep:
        # Declared skills are checked on the same walk as the prose references — one plugin
        # cache read serves both, which is what `_installed_skills`' memo exists for.
        issues += declared_skill_issues(name)
    if not meta.get("role"):
        issues.append(("warn", "no role"))
    if not vault_of(name) and not declares_no_vault(name):
        issues.append(("warn", f"no vault named — add `vault:` or `vault: {NO_VAULT}` "
                               f"if this persona holds no credentials"))
    if not (meta.get("delegate-when") or "").strip():
        issues.append(("warn", "no delegate-when → weak auto-routing"))
    if is_draft(name):
        issues.append(("warn", "draft: true → charter unfinished; no sub-agent is "
                               "generated and it cannot be dispatched. Finish the "
                               "charter, drop the line, then `charter persona sync-agents`"))
    # A frontmatter key charter neither reads nor emits reaches nothing: it is not copied
    # into .claude/agents/<name>.md and no charter code consults it, so a typo (`modell:`,
    # `delegate_when:`) is silently inert. Imported lazily — persona.py is the lower layer
    # and must not import the command module at import time.
    from .commands_persona import _AGENT_PASSTHROUGH_KEYS, _CHARTER_OWN_KEYS
    for key in sorted(set(meta) - (set(_AGENT_PASSTHROUGH_KEYS) | set(_CHARTER_OWN_KEYS))):
        issues.append(("warn", f"frontmatter key '{key}' is neither read by charter nor "
                               f"emitted into the sub-agent — it does nothing (typo?)"))
    issues += structural_errors(name)
    issues += bin_issues(name)
    if deep:
        issues += _skill_ref_issues(d["charter"])
    # A declared MCP server whose `secrets` cannot be resolved. Reported rather than
    # refused: the persona charter is committed and SHARED, while a vault is machine-local
    # by design, so a teammate cloning this repo legitimately has neither the vault nor the
    # keys. The wrapper charter emits is correct either way — only the run would fail — and
    # refusing to render would break `sync-agents` on a fresh clone (ADR 0013: name the
    # divergence, do not resolve it).
    servers = mcp_servers(name)
    if servers:
        vault = (resolve(name) or {}).get("meta", {}).get("vault")
        needs_secrets = sorted(s for s, e in servers.items() if (e or {}).get("secrets"))
        if needs_secrets and (not vault or vault == "none"):
            issues.append(("error",
                           f"mcp: server(s) {', '.join(needs_secrets)} declare `secrets` but "
                           f"this persona names no vault — add `vault:` or drop the secrets"))

    return issues


def _inherits_cycle(name: str) -> str | None:
    """The cycle path (e.g. ``a → b → a``) if ``extends`` loops, else None."""
    seen, cur = [], name
    while cur:
        if cur in seen:
            return " → ".join(seen[seen.index(cur):] + [cur])
        seen.append(cur)
        d = load(cur)
        if not d:
            return None  # dangling parent, not a cycle
        cur = (d["meta"].get("extends") or "").strip() or None
    return None


#: Reserved ``vault:`` value meaning "this persona deliberately holds no credentials".
#: A vault may therefore not be named ``none``.
NO_VAULT = "none"


def declares_no_vault(name: str) -> bool:
    """True when the persona's ``vault:`` is the reserved :data:`NO_VAULT` value.

    Charter's model assumes a persona has a scoped vault, and `lint` warned whenever one
    had none. But plenty legitimately hold no credentials — a status-line or release
    persona touches nothing secret, or leans on a tool's own auth (`gh`) — so the warning
    fired forever on personas that were entirely correct. A lint with a permanent false
    positive is a lint people learn to scroll past, which costs more than the warning
    ever bought.

    Declared rather than inferred, the same rule `[plane] shape` follows: "no secrets" is
    an intent, not something readable off disk. That keeps the warning meaningful for the
    case it was written for — a persona whose author simply never thought about it — while
    letting one say so and be believed.
    """
    r = resolve(name)
    return bool(r) and (r["meta"].get("vault") or "").strip() == NO_VAULT


def vault_of(name: str) -> str | None:
    """The vault a persona uses: its ``vault:`` field (inherited if unset), else a
    vault tagged with it.

    ``None`` for a persona that declares :data:`NO_VAULT` — there is no vault to open, and
    returning the sentinel would send callers looking for one literally named ``none``.
    Use :func:`declares_no_vault` to tell "none, deliberately" from "none, unexamined";
    every caller that merely opens a vault wants this function and cannot tell the
    difference, which is the point.
    """
    r = resolve(name)
    if r and r["meta"].get("vault"):
        v = r["meta"]["vault"].strip()
        return None if v == NO_VAULT else (v or None)
    try:
        from .secrets import registry
        tagged = registry.vaults_for_persona(name)
        return tagged[0] if tagged else None
    except Exception:
        return None


# --------------------------------------------------------------------------- #
# memory: 2×2 (own/shared × persistent/ephemeral) + activity log              #
# --------------------------------------------------------------------------- #
def scaffold_memory(name: str, shared: bool = False) -> None:
    """Create the committed memory/ and refs/ dirs with keep-files so git tracks
    them and the persona always has an index to read."""
    # Two fixed names under a directory a commit controls, written HERE rather than
    # through `memstore.ensure_index` — so gating the store alone would have left the
    # scaffolder holding the same hole one function over (#349). Both checked before the
    # `mkdir`, which happily accepts a committed link to a directory outside the plane.
    mem = memory_dir(name, shared)
    idx = contain.writable(index_of(mem))
    mem.mkdir(parents=True, exist_ok=True)
    if not idx.exists():
        who = "shared (all personas)" if shared else name
        idx.write_text(
            f"# Memory Index — {who}\n\n"
            "One line per memory; each links a file holding a single durable fact.\n"
            "Written by the persona as it learns; committed and shared.\n"
        )
    refs = refs_dir(name, shared)
    readme = contain.writable(refs / "README.md")
    refs.mkdir(parents=True, exist_ok=True)
    if not readme.exists():
        who = "shared (all personas)" if shared else name
        readme.write_text(
            f"# References — {who}\n\n"
            "Curated docs, links, and snippets this role collects. Committed and "
            "shared. Never store secrets here — those live only in the vault.\n"
        )


def ensure_shared() -> None:
    scaffold_memory(config.SHARED_PERSONA, shared=True)


def remember(name: str, text: str, title: str | None = None, *,
             shared: bool = False, ephemeral: bool = False,
             session: str | None = None) -> Path:
    """Write one memory. The caller (the persona) picks the quadrant:

    - ``ephemeral=False`` → **persistent**, committed under ``personas/…/memory``
      (appears in ``git status`` for a human to commit & push).
    - ``ephemeral=True``  → session-scoped scratch under gitignored persona-state,
      deleted after the session.
    - ``shared`` → the cross-persona ``_shared`` namespace instead of this persona.

    Returns the written file path. Secrets must never be passed here.
    """
    from . import memstore
    text = (text or "").strip()
    if not text:
        raise ValueError("empty memory")
    title = (title or text.splitlines()[0]).strip()[:72]
    d = ephemeral_dir(name, shared, session) if ephemeral else memory_dir(name, shared)
    kind = "ephemeral" if ephemeral else "persistent"
    # Persona memory keeps slug-only filenames (addressed by slug in forget/recall); the
    # ephemeral quadrant needs no committed index.
    p = memstore.write(d, text, title, kind=kind, index=not ephemeral)
    try:
        from . import trace
        trace.record("memory", persona=name, scope="shared" if shared else "own",
                     kind=kind, title=title)
    except Exception:
        pass
    return p


def memories(name: str, shared: bool = False, ephemeral: bool = False,
             session: str | None = None) -> list[Path]:
    from . import memstore
    d = ephemeral_dir(name, shared, session) if ephemeral else memory_dir(name, shared)
    return memstore.files(d)


def _mem_dirs(name: str, include_shared: bool) -> list[Path]:
    dirs = [memory_dir(name)]
    if include_shared:
        dirs.append(memory_dir(name, shared=True))
    return dirs


def search_memories(name: str, query: str, limit: int = 8,
                    include_shared: bool = True) -> list[tuple[Path, str, int]]:
    """Keyword-rank a persona's persistent memories (own + shared) against *query*.
    Returns [(path, title, score)] best-first — pull just the relevant few."""
    from . import memstore
    return memstore.search(_mem_dirs(name, include_shared), query, limit)


def find_duplicates(name: str, threshold: float = 0.5,
                    include_shared: bool = True) -> list[tuple[float, Path, str, Path, str]]:
    """Near-duplicate memory pairs (Jaccard word overlap ≥ *threshold*) for a human to
    `forget` one — never auto-deletes."""
    from . import memstore
    return memstore.duplicates(_mem_dirs(name, include_shared), threshold)


def forget(name: str, slug: str, *, shared: bool = False, ephemeral: bool = False,
           session: str | None = None):
    """Delete one memory file (by slug/filename) and drop its index line.

    Returns the removed path (falsy when nothing matched) so the caller can stage the
    deletion — see `memstore.forget`."""
    from . import memstore
    d = ephemeral_dir(name, shared, session) if ephemeral else memory_dir(name, shared)
    return memstore.forget(d, slug)


# --------------------------------------------------------------------------- #
# roster health — a persona's committed memory IS its activity trace, so mine it #
# into a usage + (in-corpus) quality signal for the steward's observe loop. Read- #
# only: count/recency = usage; verification-marker & near-dup ratios = a quality  #
# *proxy* (a signal, not a verdict — volume ≠ value). Feeds `charter persona stats`.  #
# --------------------------------------------------------------------------- #
# The verification-discipline vocabulary personas actually use (freq-ranked in the
# committed corpus): a memory asserting it was checked, not merely guessed.
_VERIFY_RE = re.compile(r"\b(?:confirmed|validated|verified|proven|reproduced)\b", re.I)


def _memory_date(text: str, filename: str):
    """Date a memory was recorded — delegates to the memstore file-format owner."""
    from . import memstore
    return memstore.memory_date(text, filename)


# A persona may declare an `activity:` profile when memory volume is NOT a fair usage
# signal for it — so `stats` reports that profile instead of crying "dormant":
#   orchestrator — routes/delegates, writes no domain memory (e.g. steward)
#   standby      — invoked on-demand / rarely but valuable (e.g. performance-investigator)
#   advisory     — output is design/requirements/review, not durable facts (e.g. nx-code-reviewer)
# Absent → a normal investigative persona whose memory volume IS a real usage signal.
_MEMORY_BLIND = ("orchestrator", "standby", "advisory")


def stats(name: str, recent_days: int = 14, shared: bool = False, today=None) -> dict:
    """Read-only roster-health for one persona (or the ``_shared`` namespace), mined from
    its committed memory — the persona's own activity trace. Returns:

      count       total persistent memories (usage)
      recent      memories recorded within ``recent_days`` (recency/cadence)
      last        ISO date of the most recent memory, or None
      verify_pct  % of memories carrying a verification marker (quality proxy), or None
      dup_pct     % of memories in a near-duplicate pair (noise proxy), or None
      activity    the declared `activity:` profile (orchestrator/standby/advisory), or None
      status      declared profile if set (memory-blind role); else 'active' (recent memory)
                  | 'idle' (has memories, none recent) | 'dormant' (none) — a *real* prune signal
    """
    import datetime
    from . import memstore
    day = today or datetime.date.today()
    ents = memstore.entries(memory_dir(name, shared))
    total = len(ents)
    recent = verified = 0
    dates = []
    for p, _title, text in ents:
        if _VERIFY_RE.search(text):
            verified += 1
        d = _memory_date(text, p.name)
        if d:
            dates.append(d)
            if (day - d).days <= recent_days:
                recent += 1
    dup_files = set()
    for _jac, pa, _ta, pb, _tb in memstore.duplicates([memory_dir(name, shared)]):
        dup_files.add(pa)
        dup_files.add(pb)
    meta = (load(name) or {}).get("meta", {}) if not shared else {}
    activity = (meta.get("activity") or "").strip().lower() or None
    if activity in _MEMORY_BLIND:
        status = activity  # memory-blind by declared role nature — never "dormant"
    else:
        status = "dormant" if total == 0 else ("active" if recent else "idle")
    return {
        "persona": name, "count": total, "recent": recent,
        "last": max(dates).isoformat() if dates else None,
        "verify_pct": round(100 * verified / total) if total else None,
        "dup_pct": round(100 * len(dup_files) / total) if total else None,
        "activity": activity, "status": status,
    }


# (Persona activity now lives in the single session **trace** — see charter/trace.py.
# `charter persona log` writes `note` events there; there is no separate per-persona log.)


def gc_ephemeral(current: str | None = None, max_age_hours: float = 6.0) -> int:
    """Prune ephemeral scratch from sessions that have ended. A session dir is
    removed when it isn't the current session *and* hasn't been touched within
    ``max_age_hours`` (so concurrent live sessions are never clobbered). Called
    from the SessionStart hook. Returns the number of session dirs removed."""
    root = config.PERSONA_STATE_DIR / "ephemeral"
    if not root.exists():
        return 0
    # `current()`, not `_session_id()`. The latter falls back to the shared NO_SESSION
    # bucket, so when the GC itself ran without a session id — which is most of the time,
    # since it runs from a hook — that bucket compared equal to "the live session" and was
    # skipped every single time. Ephemeral scratch from every id-less session accumulated
    # there forever, which is the opposite of ephemeral. There is nothing to preserve when
    # there is no current session, so nothing is exempt.
    from . import session as _session
    cur = _session.current(current)
    now = time.time()
    removed = 0
    for sd in root.iterdir():
        if not sd.is_dir() or (cur is not None and sd.name == cur):
            continue
        try:
            newest = max([sd.stat().st_mtime] + [p.stat().st_mtime for p in sd.rglob("*")])
        except (OSError, ValueError):
            newest = 0
        if now - newest > max_age_hours * 3600:
            shutil.rmtree(sd, ignore_errors=True)
            removed += 1
    return removed


def migrate(name: str) -> str:
    """Convert the legacy flat ``personas/<name>.md`` to the directory layout
    ``personas/<name>/persona.md`` and scaffold memory/ + refs/. Returns
    'migrated' | 'already' | 'missing'."""
    target = dir_of(name) / "persona.md"
    if target.exists():
        return "already"
    flat = config.PERSONAS_DIR / f"{name}.md"
    if not flat.exists():
        return "missing"
    target.parent.mkdir(parents=True, exist_ok=True)
    flat.rename(target)  # content-identical move → git records a rename
    scaffold_memory(name)
    return "migrated"
