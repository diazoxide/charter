"""``charter persona`` commands: manage shared personas and use their vaults.

Persona *definitions* are committed (``personas/<name>.md``); their *secrets*
live in a local vault the definition names. ``charter persona secret …`` proxies to
that vault, so an agent adopting a persona reads/writes only that persona's
credentials — and, per the secrets contract, never sees the plaintext.
"""

from __future__ import annotations

import json
import os

from . import commands_secrets, config, mcpseen, persona, trace, util
from .secrets import base, registry

#: The scaffold a new persona starts from.
#:
#: Every line here is a *true statement about this persona*, never an instruction to
#: whoever is writing it. The earlier scaffold carried slots — `(describe what this
#: persona owns and does)`, `(which repos this role typically works in)` — and `create`
#: generated `.claude/agents/<name>.md` from it straight away, so a persona dispatched
#: before anyone edited it handed those parentheticals to a sub-agent as its actual
#: remit. Author-facing guidance belongs in the CLI output and `persona lint`; what
#: belongs in this file is only what an agent should read as fact.
#:
#: The unfinished-ness is carried by `draft: true` instead, which blocks generation
#: outright — an honest flag rather than prose hoping to be noticed.
_TEMPLATE = """---
name: {name}
role: {role}
vault: {vault}
draft: true
---

# {role}

You are the **{name}** persona — {role}. When this persona is
active, adopt this role: its responsibilities, focus, and conventions.

## How to work as this persona
- Credentials: use `charter persona secret …` (this persona's vault: `{vault}`).
  Never print secret values.
- Defer to each repo's own `CLAUDE.md` / `AGENTS.md` and its tooling over general habits.
- Record durable facts with `charter persona remember {name} "<fact>"`. Never store
  secrets there — those belong in the vault.
"""

#: Appended when the persona states its own routing intent (i.e. not purely inherited).
_TEMPLATE_DELEGATE = """
## When to delegate here
{delegate_when}
"""


# --------------------------------------------------------------------------- #
# management                                                                   #
# --------------------------------------------------------------------------- #
def cmd_persona_create(args) -> int:
    if not persona.valid_name(args.name):
        util.err(f"invalid persona name '{args.name}' (lowercase letters, digits, '.', '_', '-')")
        return 1
    p = persona.path(args.name)
    if p.exists() and not args.force:
        util.err(f"persona '{args.name}' already exists ({p.relative_to(config.ROOT)}). "
                 "Edit it, or pass --force to overwrite.")
        return 1

    extends = getattr(args, "extends", None)
    if extends and not persona.load(extends):
        util.err(f"--extends '{extends}': no such persona to inherit from (see `charter persona list`).")
        return 1

    # Routing intent is required up front — unlike the charter body, it is knowable at
    # creation, and it is the field that decides whether the steward ever routes anything
    # here. A persona created without it lint-warned from birth and quietly lost every
    # dispatch to `general-purpose`. `--extends` is the one exemption: the parent's
    # delegate-when is inherited like any other scalar.
    delegate_when = (getattr(args, "delegate_when", None) or "").strip()
    if not delegate_when and not extends:
        util.err(
            f"--delegate-when is required: say when the steward should route work to "
            f"'{args.name}', e.g.\n"
            f"  charter persona create {args.name} --delegate-when "
            f"\"CI/CD pipelines, k8s deploys, cluster access\"\n"
            "It becomes the persona's routing line in its dispatchable description. "
            "(Inheriting one? Pass --extends <parent> instead.)")
        return 1

    vault = args.vault or args.name
    p = persona.dir_of(args.name) / "persona.md"  # always create in the directory layout
    p.parent.mkdir(parents=True, exist_ok=True)
    text = _TEMPLATE.format(name=args.name, role=args.role or args.name.title(), vault=vault)
    if delegate_when:
        text = text.replace("vault: {v}\n".format(v=vault),
                            f"vault: {vault}\ndelegate-when: {delegate_when}\n", 1)
        text += _TEMPLATE_DELEGATE.format(delegate_when=delegate_when)
    if extends:
        # inherit charter + tools from the parent; this file adds the child's specialization
        text = text.replace(f"vault: {vault}\n", f"vault: {vault}\nextends: {extends}\n", 1)
        text = text.replace(
            "active, adopt this role: its responsibilities, focus, and conventions.",
            f"active, adopt this role. It **inherits from `{extends}`** (that persona's charter + "
            f"tools apply); the sections below are what THIS persona ADDS on top.", 1)
    p.write_text(text)
    persona.scaffold_memory(args.name)  # memory/ + refs/ (committed, with keep-files)
    persona.ensure_shared()             # the cross-persona _shared/ namespace
    util.ok(f"Created persona '{args.name}' → {p.parent.relative_to(config.ROOT)}/ "
            "(persona.md + memory/ + refs/; edit the charter, then commit — personas are shared).")
    outcome = _write_agent(args.name)
    if outcome == "written":
        util.info(f"  generated .claude/agents/{args.name}.md — invokable as subagent '{args.name}'.")
    elif outcome == "draft":
        util.info(
            f"  marked `draft: true` — no sub-agent yet, so '{args.name}' cannot be "
            f"dispatched.\n"
            f"  Write what it owns and how it works in {p.relative_to(config.ROOT)}, "
            f"drop the `draft: true` line,\n"
            f"  then: charter persona sync-agents")

    if args.with_vault:
        cfg = {"file": str(config.VAULTS_DIR / f"{vault}.json")}
        try:
            registry.add_vault(vault, "plain-file", cfg, persona=args.name)
            util.ok(f"Registered local vault '{vault}' (plain-file) for persona '{args.name}'.")
        except base.VaultError as e:
            util.warn(f"vault: {e}")
    else:
        util.info(f"Set up its vault locally when ready: "
                  f"charter vault add {vault} --provider plain-file --persona {args.name}")

    if args.use:
        scope = persona.set_active(args.name)
        util.ok(f"Active persona set to '{args.name}'{_scope_note(scope)}.")
        _warn_env(args.name)
    return 0


def cmd_persona_list(args) -> int:
    names = persona.list_personas()
    active = persona.resolve_active()
    if not names:
        util.info('No personas yet. Create one: charter persona create <name> --role "<Role>"')
        return 0
    print(f"Active persona: {active or '—'}  (via {persona.source()})\n")
    roles = {n: ((persona.load(n) or {"meta": {}})["meta"].get("role") or "") for n in names}
    vaults = {n: (persona.vault_of(n) or "—") for n in names}
    # dynamic column widths so long persona/role/vault names don't collide
    nw = max([len("PERSONA")] + [len(n) for n in names]) + 2
    rw = min(38, max([len("ROLE")] + [len(r) for r in roles.values()])) + 2
    vw = max([len("VAULT")] + [len(v) for v in vaults.values()]) + 2
    fmt = f"{{}}{{:<{nw}}}{{:<{rw}}}{{:<{vw}}}{{}}"
    print(fmt.format("  ", "PERSONA", "ROLE", "VAULT", "VAULT STATUS"))
    for n in names:
        mark = "* " if n == active else "  "
        print(fmt.format(mark, n, roles[n][:rw - 2], vaults[n], _vault_status(vaults[n])))
    return 0


def _vault_status(vault: str | None) -> str:
    if not vault or vault == "—":
        return "no vault"
    try:
        if vault not in registry.vaults():
            return "not set up (local)"
        _ok, detail = registry.provider_for(vault).health()
        return detail
    except base.VaultError as e:
        return str(e)


def cmd_persona_show(args) -> int:
    d = persona.resolve(args.name)  # effective persona (inheritance applied)
    if not d:
        util.err(f"no persona '{args.name}'")
        return 1
    m = d["meta"]
    print(f"{m.get('name', args.name)} — {m.get('role', '')}")
    chain = d.get("lineage") or [args.name]
    if len(chain) > 1:  # inherits: child → parent → …
        print(f"inherits: {' → '.join(chain)}  (charter + tools merged below)")
    vault = persona.vault_of(args.name)
    if vault:
        print(f"vault:   {vault}  ({_vault_status(vault)})")
    tools = persona.tools_of(args.name)
    if tools:
        print(f"tools:   {', '.join(sorted(tools))}  (auto-approved when this persona is active)")
    scripts = persona.bin_scripts(args.name)
    if scripts:
        # Disclosure, not a warning. A LIVE persona is committed and synced, so these reach
        # a teammate's disk and run with their credentials — worth stating plainly wherever
        # somebody inspects the persona (ADR 0017's "state it" half).
        print(f"scripts: {', '.join(sorted(scripts))}  "
              f"(executables this persona carries — run by path)")
    print(f"file:    {persona.path(args.name).relative_to(config.ROOT)}")
    _print_memory_summary(args.name)
    print()
    print(d["charter"])
    return 0


def _print_memory_summary(name: str) -> None:
    own = len(persona.memories(name))
    shared = len(persona.memories(name, shared=True))
    eph = len(persona.memories(name, ephemeral=True)) + len(persona.memories(name, shared=True, ephemeral=True))
    refs = persona.refs_dir(name)
    nrefs = len([p for p in refs.glob("*") if p.name != "README.md"]) if refs.exists() else 0
    print(f"memory:  {own} own · {shared} shared (persistent) · {eph} ephemeral · {nrefs} refs")
    print(f"         {persona.memory_dir(name).relative_to(config.ROOT)}/  ·  "
          f"recall: charter persona recall {name}")


def _scope_note(scope: str) -> str:
    """How far this selection reaches, in the words the reader needs.

    Three answers that differ in ways that matter, so none of them may be left implicit —
    the reader who is not told goes looking for a bug the next time the status line
    disagrees with what they picked. Only a terminal pointer survives closing and
    reopening Claude; a session-scoped choice is gone with the session, and what the next
    session starts as is the plane's declared front door, which may be nothing at all.
    """
    if scope == "terminal":
        return " for this terminal (kept across closing/reopening Claude)"
    if scope == "session":
        nxt = persona.declared_default() or persona.default_persona()
        starts = f"starts as '{nxt}'" if nxt else "starts with no persona"
        return (f" for this session only — this terminal reports no pane id, "
                f"so a new session {starts}")
    return " for this control plane (no session or pane id to scope it to)"


def cmd_persona_use(args) -> int:
    if not persona.load(args.name):
        util.err(f"no persona '{args.name}' (create it: charter persona create {args.name})")
        return 1
    scope = persona.set_active(args.name)
    util.ok(f"Active persona set to '{args.name}'{_scope_note(scope)}.")
    _warn_env(args.name)
    _say_tool_ceiling(args.name)
    _say_mcp_boundary(args.name)
    return 0


def _say_tool_ceiling(name: str) -> None:
    """Say so when this persona's ``tools:`` has grown since the session began (#432).

    The tool-gate answers within the set declared *before* this session could rewrite it,
    because `persona.md` is a file the model can write and re-reading it on every call
    made one approved edit into unprompted execution for the rest of the session. The
    price of that is real and lands on a person: an operator adds a tool by hand, watches
    it keep prompting, and has nothing to read that explains why.

    So charter names the boundary rather than moving it — the same call
    :func:`_say_mcp_boundary` makes directly below, for the same reason: a scoping claim
    that is not true of the session you are in is worse than a sentence saying when it
    becomes true.
    """
    try:
        from . import toolgate
        frozen = toolgate.frozen_tools(name)
        if frozen is None:                    # no session to freeze against
            return
        added = sorted(persona.effective_tools(name) - frozen)
    except Exception:
        return
    if not added:
        return
    util.info(f"  {len(added)} tool(s) declared since this session started: "
              f"{', '.join(added)}.")
    util.info("  Those still prompt HERE. The tool-gate answers within the set that "
              "existed at session start — `tools:` is read from a file this session can "
              "write, and freezing it is what stops an edit from becoming an unprompted "
              "command. A new session picks them up.")


def _say_mcp_boundary(name: str) -> None:
    """Say plainly which boundary persona-scoped MCP servers apply at (#186).

    Charter scopes a persona's servers at DISPATCH: `_render_agent` emits them inline, the
    harness connects them when the sub-agent starts and disconnects them when it finishes, and
    their tool descriptions never reach the main conversation. That is a stronger guarantee
    than an allowlist — the server does not run at all for this session.

    It is also not the guarantee someone reads into `persona use`. The reporter switched to a
    generalist, found another persona's five analytics servers still live, and reasonably
    concluded charter had no MCP story at all — while the story existed for a boundary they
    were not using (#186).

    So charter names the boundary rather than moving it. Scoping the main session would mean
    writing `disabledMcpServers` into a user-owned settings file and then OWNING it forever:
    stateful, subtractive, and effective only on the next session — so `persona use` would
    print a scoping claim that is not true of the session you are in. Naming beats resolving
    where the tool cannot honestly deliver, which is the call #140 made for the same reason.
    """
    try:
        servers = persona.mcp_servers(name)
    except Exception:
        return
    if not servers:
        return
    util.info(f"  {len(servers)} MCP server(s) declared: {', '.join(sorted(servers))}.")
    util.info("  These are scoped to DISPATCH — the host starts them when this persona runs "
              "as a sub-agent and stops them after, and their tools never enter this "
              "conversation. They are NOT started for the session you are in, and servers "
              "already live here (from .mcp.json or an enabled plugin) stay live.")


def cmd_persona_current(args) -> int:
    active = persona.resolve_active()
    print(active or "(none)")
    util.info(f"resolved via {persona.source()}")
    return 0


def cmd_persona_clear(args) -> int:
    persona.clear_active()
    util.ok("Active persona cleared.")
    d = persona.default_persona()
    if d:
        util.info(f"Resolves to the committed default '{d}' now (personas/.default).")
    return 0


def cmd_persona_default(args) -> int:
    """Show / set / clear the plane's declared front door — ``charter.toml``'s
    ``[persona] default``, the persona adopted when no ``--persona`` / ``$CHARTER_PERSONA``
    / ``charter persona use`` is set.

    Writes ``charter.toml`` rather than the legacy ``personas/.default`` dotfile. Both
    still resolve (a plane that adopted the dotfile keeps working), but only one of them is
    in the file a consumer opens to read their plane, and the invisible one is the one that
    shipped, tested green, and was adopted by nobody — including this repo (#255).
    """
    from . import instance as _instance

    legacy = config.PERSONAS_DIR / ".default"
    if getattr(args, "clear", False):
        # Both rungs, or a plane would still declare a front door after being told it no
        # longer does — the dotfile resolves whenever charter.toml is silent.
        had = bool(persona.declared_default() or persona.default_persona())
        _instance.set_default_persona(config.ROOT, None)
        if legacy.exists():
            legacy.unlink()
        if had:
            util.ok("Cleared the declared default persona (commit with `charter save`).")
        else:
            util.info("No default persona was declared.")
        return 0
    if getattr(args, "name", None):
        if not persona.load(args.name):
            util.err(f"no persona '{args.name}' — create it first (`charter persona create {args.name}`).")
            return 1
        if not _instance.set_default_persona(config.ROOT, args.name):
            util.err(f"could not write {config.ROOT / 'charter.toml'} — is this a control plane?")
            return 1
        util.ok(f"Default persona declared: '{args.name}' → charter.toml [persona] default "
                "(shared; commit with `charter save`).")
        if legacy.exists():
            # Say it at the moment it becomes true, not in a doctor run somebody may never
            # do: from now on the two files disagree and the dotfile is the one that loses.
            util.warn(f"personas/.default also exists (naming '{persona.default_persona() or '?'}') "
                      "and is now IGNORED — charter.toml outranks it. Delete it: "
                      "rm personas/.default")
        util.info("Overridden per-developer by `charter persona use` / $CHARTER_PERSONA / --persona.")
        return 0
    declared = persona.declared_default()
    if declared:
        print(declared)
        util.info("declared front door (charter.toml [persona] default). "
                  "Change: charter persona default <name>  ·  clear: --clear")
        return 0
    d = persona.default_persona()
    if d:
        print(d)
        util.info("committed team-wide default (personas/.default) — the legacy location. "
                  f"Move it: charter persona default {d}")
    else:
        util.info("No default persona declared. Set one: charter persona default <name>")
    return 0


def _dependents_of(name: str) -> list[str]:
    """Personas that would be left with a DANGLING reference if *name* went away —
    either `extends: <name>` (inherits its charter) or `uses: …, <name>, …` (shares its
    vault/tools). `charter persona lint` reports a dangling ref as an error, but only after
    the fact; this lets `remove` refuse up front."""
    out = []
    for other in persona.list_personas():
        if other == name:
            continue
        meta = (persona.load(other) or {}).get("meta", {})
        if (meta.get("extends") or "").strip() == name:
            out.append(f"{other} (extends)")
            continue
        uses = [u.strip() for u in (meta.get("uses") or "").split(",") if u.strip()]
        if name in uses:
            out.append(f"{other} (uses)")
    return sorted(out)


def cmd_persona_remove(args) -> int:
    import shutil
    if not persona.load(args.name):
        util.err(f"no persona '{args.name}'")
        return 1
    deps = _dependents_of(args.name)
    if deps and not getattr(args, "force", False):
        util.err(f"Refusing to remove '{args.name}' — it is still referenced by:")
        for d in deps:
            util.err(f"  {d}")
        util.info("Repoint or remove those first (an `extends:` parent's charter is inherited, "
                  "so folding it into the child before removing keeps the discipline).")
        util.info(f"Override with: charter persona remove {args.name} --force")
        return 1
    if persona.is_dir_layout(args.name):
        d = persona.dir_of(args.name)
        shutil.rmtree(d)  # removes persona.md + committed memory/ + refs/
        util.ok(f"Removed persona directory {d.relative_to(config.ROOT)}/ "
                "(definition, memory, and refs — commit the deletion).")
    else:
        p = persona.path(args.name)
        p.unlink()
        util.ok(f"Removed persona definition {p.relative_to(config.ROOT)} (commit the deletion).")
    if _remove_agent(args.name):
        util.info(f"  also removed generated .claude/agents/{args.name}.md.")
    util.info("Its local vault (if any) is left untouched — remove with `charter vault remove <vault>`.")
    if persona.resolve_active() == args.name and persona.source() == "active-file":
        persona.clear_active()
        util.info("Active persona cleared.")
    return 0


# --------------------------------------------------------------------------- #
# persona-scoped secrets (proxy to the persona's vault)                        #
# --------------------------------------------------------------------------- #
def _resolve_vault(args) -> str | None:
    name = persona.resolve_active(getattr(args, "persona", None))
    if not name:
        util.err("no active persona. Select one: charter persona use <name>  (or pass --persona).")
        return None
    vault = persona.vault_of(name)
    if not vault:
        # Two different situations, and conflating them sends the user to fix the wrong
        # thing: one persona was never given a vault, the other says it holds no
        # credentials at all — for which the answer is a different persona, not a vault.
        if persona.declares_no_vault(name):
            util.err(f"persona '{name}' declares `vault: {persona.NO_VAULT}` — it holds no "
                     f"credentials by design. Use a persona that owns this secret, or "
                     f"replace that line with a real vault name.")
        else:
            util.err(f"persona '{name}' has no vault. Add `vault:` to its file, or "
                     f"`charter vault add <v> --persona {name}`.")
        return None
    if vault not in registry.vaults():
        util.err(f"persona '{name}' vault '{vault}' isn't set up on this machine. "
                 f"Create it: charter vault add {vault} --provider plain-file --persona {name}.")
        return None
    return vault


def _proxy(secret_fn):
    def run(args) -> int:
        vault = _resolve_vault(args)
        if not vault:
            return 1
        args.vault = vault
        return secret_fn(args)
    return run


cmd_persona_secret_get = _proxy(commands_secrets.cmd_secret_get)
cmd_persona_secret_set = _proxy(commands_secrets.cmd_secret_set)
cmd_persona_secret_list = _proxy(commands_secrets.cmd_secret_list)
cmd_persona_secret_rm = _proxy(commands_secrets.cmd_secret_rm)
cmd_persona_secret_cp = _proxy(commands_secrets.cmd_secret_cp)
cmd_persona_secret_exec = _proxy(commands_secrets.cmd_secret_exec)
cmd_persona_secret_audit = _proxy(commands_secrets.cmd_secret_audit)


def _warn_env(name: str) -> None:
    env = os.environ.get("CHARTER_PERSONA")
    if env and env.strip() != name:
        util.warn(f"$CHARTER_PERSONA='{env}' is set and takes precedence — commands use "
                  f"'{env}', not '{name}'.")


# --------------------------------------------------------------------------- #
# sync-agents: generate a Claude Code sub-agent per persona                    #
# --------------------------------------------------------------------------- #
_AGENT_MARKER = "GENERATED by `charter persona sync-agents`"


def _agents_dir():
    return config.ROOT / ".claude" / "agents"


def _yaml_str(s: str) -> str:
    s = (s or "").replace("\n", " ").strip()
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def _agent_description(name: str, meta: dict) -> str:
    role = meta.get("role") or name
    when = (meta.get("delegate-when") or "").strip()
    tools = meta.get("tools")
    # The vault the persona actually uses, which is NOT always its name — `vault:` may
    # point elsewhere, or say `none` for a persona that holds no credentials at all.
    # Describing a vault that does not exist is worse than saying nothing: this string is
    # what the router reads when choosing an agent, and it would advertise a capability
    # the sub-agent cannot use.
    vault = persona.vault_of(name)
    if tools and vault:
        tail = f" Runs {tools} and pulls credentials from the '{vault}' vault."
    elif tools:
        tail = f" Runs {tools}. Holds no credentials of its own."
    elif vault:
        tail = f" Pulls credentials from the '{vault}' vault."
    else:
        tail = " Holds no credentials of its own."
    # `isolation` USED to be a dispatch-time parameter of the Agent tool with no agent-side
    # way to declare it, so this string was the only place a persona could ask for it —
    # advisory, aimed at whoever chose. The host has since gained an `isolation:` frontmatter
    # field, and `_render_agent` now emits it, so the persona isolates ITSELF and the router
    # has nothing to remember. The sentence stays because it still tells the router why this
    # persona behaves differently, but it no longer asks for anything (#185).
    if (meta.get("dispatch-isolation") or "").strip() == "worktree":
        tail += " Runs in its own git worktree — it writes code, and parallel dispatches would otherwise share one working tree."
    # The `delegate-when` triggers are what let the agent auto-route work here; a
    # persona without them falls back to a generic (weakly-triggering) description.
    if when:
        return f"The {role} persona. Delegate to it for {when}.{tail}"
    return f"The {role} persona. Delegate {role.lower()} tasks to it.{tail}"


def _credential_rule() -> str:
    """The golden one-credential rule, worded for THIS control plane's own declared
    forge(s) — never hardcoded to `glab`/GitLab. Generated `.claude/agents/<name>.md`
    files land in the user's own repo, so a GitHub-only control plane's sub-agent must
    never mention `glab` (a tool it never uses), and a GitLab-only one must never
    mention `gh`. Routed through `forge.registry.declared_or_default`, the same
    resolution `doctor` uses to decide which forge CLI/auth to preflight — so the two
    surfaces can never drift apart on what this control plane's forge set is.

    A persona's OWN `tools:` declaration (e.g. `tools: kubectl, glab`) is untouched by
    this — it's the persona's explicit choice, rendered verbatim elsewhere; this is only
    charter's own generated prose."""
    from .forge import registry
    forges = registry.declared_or_default(config.ROOT)
    clis = sorted({f.cli for f in forges})
    if len(clis) == 1:
        cli = clis[0]
        return (
            f"**🔑 git = the {cli} token over HTTPS. Never SSH, never signing.** Repos are "
            f"pre-configured, so a plain `git push` works. If git ever asks for a key or "
            f"passphrase, run `charter git-policy --apply` — don't reach for SSH "
            f"(`{cli} auth status` checks the credential)."
        )
    clis_joined = " / ".join(f"`{c}`" for c in clis)
    checks_joined = " / ".join(f"`{c} auth status`" for c in clis)
    return (
        f"**🔑 git = each repo's own forge's CLI token over HTTPS ({clis_joined}). Never "
        f"SSH, never signing.** Repos are pre-configured, so a plain `git push` works. If "
        f"git ever asks for a key or passphrase, run `charter git-policy --apply` — don't "
        f"reach for SSH (check the credential with {checks_joined})."
    )


#: Frontmatter keys `_render_agent` copies straight into the generated sub-agent.
#: Anything a charter sets that isn't here (and isn't consumed by charter itself)
#: reaches `.claude/agents/<name>.md` never — a typo is silently inert today, which
#: `persona lint` now reports.
_AGENT_PASSTHROUGH_KEYS = ("model", "color", "memory")

#: Keys charter reads itself rather than emitting. Together with the passthrough set
#: this is the full vocabulary of a persona charter's frontmatter.
_CHARTER_OWN_KEYS = (
    "name", "role", "vault", "extends", "uses", "delegate-when", "description",
    "agent-description", "agent-tools", "tools", "activity", "dispatch-isolation",
    "draft", "skills", "disallowed-tools", "routing", "routes-to", "borrows",
)


def _rel(path) -> str:
    """Plane-relative when it can be, absolute when it cannot — a path outside the plane is
    still better than a name the reader has to resolve."""
    try:
        return str(path.relative_to(config.ROOT))
    except ValueError:
        return str(path)


def _render_agent(name: str, meta: dict, charter: str) -> str:
    role = meta.get("role") or name.title()
    desc = meta.get("agent-description") or meta.get("description") or _agent_description(name, meta)
    fm = [f"name: {name}", f"description: {_yaml_str(desc)}"]

    # Declared MCP servers, wrapped in the persona's vault. Claude Code connects an inline
    # server when the sub-agent starts and disconnects it when it finishes, and its tool
    # descriptions never reach the parent conversation — so this is per-persona scoping of
    # the server itself, not merely of the tools.
    servers = persona.mcp_servers(name)
    vault = meta.get("vault")

    if meta.get("agent-tools"):
        tools = meta["agent-tools"]
        # Declaring a server grants its tools. Otherwise `tools:` and the server list are
        # two hand-kept lists that must agree, and disagreement surfaces at DISPATCH time
        # as "unresolved entries" — a message about the symptom, not the cause.
        grants = [f"mcp__{s}__*" for s in servers
                  if f"mcp__{s}" not in tools]
        fm.append(f"tools: {', '.join([tools, *grants]) if grants else tools}")
    # No `agent-tools` means the sub-agent inherits every tool, so adding a narrowing
    # `tools:` line here to carry the grant would be a downgrade rather than a grant.

    # Declared skills, PRELOADED into the sub-agent at startup — the host injects each
    # skill's full text, not just its description. That is what makes a persona's skills
    # standing equipment rather than something it might discover mid-task, and it is the
    # one thing charter can do with the list that reading the charter prose cannot.
    #
    # NOT an allowlist: the host has none. A sub-agent can still invoke unlisted skills
    # through the Skill tool, and the only real restriction is withholding `Skill` itself
    # via `agent-tools`. Saying "allowed skills" here would promise an enforcement charter
    # cannot deliver.
    #
    # The cost is why lint is strict about dead entries: full text, injected on EVERY
    # dispatch of this persona, for as long as the line is there.
    skills = persona.declared_skills(name)
    if skills:
        fm.append(f"skills: {', '.join(skills)}")

    # The host's own field, emitted from the charter key that already means this. NOT a
    # second spelling: `dispatch-isolation:` predates the host gaining `isolation:`, and
    # renaming it would churn every persona to say the same thing. One name in a charter,
    # one behaviour in the agent (#185).
    if (meta.get("dispatch-isolation") or "").strip() == "worktree":
        fm.append("isolation: worktree")

    # A DENYLIST, which is often the honest shape where `agent-tools` forces an allowlist:
    # "everything except Bash" is one line here and an enumeration of every other tool
    # there. Passed straight through — unlike `permissionMode`/`maxTurns`, which would let
    # a persona charter widen its own permissions and are deliberately left out.
    if (meta.get("disallowed-tools") or "").strip():
        fm.append(f"disallowedTools: {meta['disallowed-tools'].strip()}")

    if servers:
        fm.append("mcpServers:")
        for server_name in sorted(servers):
            entry = persona.mcp_render_entry(name, vault, servers[server_name])
            # JSON, because JSON is valid YAML — this emits a nested block without
            # hand-rolling a YAML writer or taking a dependency to do it.
            fm.append(f"  - {server_name}: {json.dumps(entry, ensure_ascii=False)}")
    for k in _AGENT_PASSTHROUGH_KEYS:  # pass through when the charter sets them
        if meta.get(k):
            fm.append(f"{k}: {meta[k]}")

    # `memory:` gives the agent Claude Code's own per-agent store *in addition to*
    # charter's. Both are useful and they are not duplicates — but an agent told to
    # "record what's durable" then has two plausible places and no stated precedence.
    # Say which is which, generated, so every persona gets it rather than the one
    # whose store happened to be annotated by hand.
    memory_note = ""
    if meta.get("memory"):
        memory_note = (
            f"\n- **Two memory stores, different jobs.** `.claude/agent-memory/{name}/` is "
            f"the generic Claude-agent memory (user / feedback / project / reference). Your "
            f"durable domain knowledge — the team-shared, committed base — lives in "
            f"`personas/{name}/memory/` and is reached with `charter recall`. **Search "
            f"charter's first**; write there for anything a teammate would want, and keep "
            f"the generic store to what isn't already in it."
        )

    # A script the dispatched agent never learns about is the same as no script at all.
    # PATH would have made these discoverable by habit — but a PreToolUse hook decides
    # whether a Bash call runs, not what environment it runs in, so the brief has to name
    # them outright (#283). Paths, not just names: without one the agent has to guess, and
    # a bare name resolves through PATH, which the tool guard deliberately refuses.
    scripts = persona.bin_scripts(name)
    bin_note = ""
    if scripts:
        listed = "\n".join(
            f"  - `{_rel(path)}`" for _n, path in sorted(scripts.items()))
        bin_note = (
            f"\n- **You carry your own executables.** Run them by path, not by name:\n"
            f"{listed}\n"
            f"  A bare name is refused by the tool guard — it resolves through PATH, which "
            f"charter cannot vouch for."
        )

    uses = [u.strip() for u in (meta.get("uses") or "").split(",") if u.strip() and u.strip() != name]
    borrows = persona.borrows_of(name)
    uses_note = ""
    if uses:
        joined = ", ".join(f"`{u}`" for u in uses)
        if borrows is None:
            # Legacy: `uses:` still grants all three. Wording unchanged for a plane that
            # has not opted into the split.
            uses_note = (
                f"\n- **You may also use these personas: {joined}.** Read their vault "
                f"(`charter persona secret list --persona <name>`), run their tools, or delegate a "
                f"sub-task to their sub-agent (Agent tool, `subagent_type: <name>`)."
            )
        else:
            # Opted in: `uses:` is a routing edge. The charter must say so, because it is
            # what a dispatched agent believes about itself — if it still read "run their
            # tools", the tool-gate would refuse and the agent would have no idea why.
            uses_note = (
                f"\n- **You may delegate to these personas: {joined}.** Hand them a sub-task "
                f"(Agent tool, `subagent_type: <name>`). Their tools are not auto-approved "
                f"for you, and their vaults are not yours to open — do not name one to "
                f"`charter secret …`. That second half is a rule you keep, not a wall "
                f"charter holds: nothing refuses the vault name (`docs/personas.md` → "
                f"Reusing another persona), which is exactly why it is written here. That is "
                f"what `borrows:` is for, and it is deliberate: doing their work yourself "
                f"should cost more than handing it over."
            )
    if borrows:
        joined_b = ", ".join(f"`{b}`" for b in borrows)
        uses_note += (
            f"\n- **You borrow from: {joined_b}.** Read their vault "
            f"(`charter persona secret list --persona <name>`) and run their tools without a "
            f"prompt."
        )

    # Generic capability handoff — every persona gets this, so it hands work outside its
    # role to the owning persona instead of guessing with partial credentials. Awareness,
    # not access: delegation runs the owner with *its own* vault; only `uses:` shares creds.
    #
    # Deliberately names no persona. This used to special-case a persona literally called
    # `devops`, which silently rendered nothing for a control plane whose infra persona is
    # `sre` or `ops` — and it sat on top of the `charter persona list` pointer below, which
    # is generic and always correct. Derive from declared config or say nothing.
    handoff = (
        f"\n- **Outside your domain, hand off — don't guess with partial credentials.** "
        f"Delegate to the owner via the Agent tool (`subagent_type: <persona>`; it runs "
        f"with *its own* vault — you never see its secrets); `charter persona list` shows who owns "
        f"what. Never `charter persona use` to switch the active persona — that's user-request-only."
    )

    # A persona declaring `vault: none` has nothing to open, so the "run tools through
    # your vault" instruction is a dead end — it names a command that errors, in a system
    # prompt nobody proofreads. The one rule that must survive is the prohibition: not
    # having a vault is exactly when a sub-agent might be tempted to improvise with a
    # credential it found lying around.
    vault = persona.vault_of(name)
    if vault:
        # SUBCOMMAND FIRST, then `--persona`. `charter persona secret --persona X list`
        # is rejected by argparse ("invalid choice: 'X'"), and this is the ONLY credential
        # instruction a generated sub-agent carries — an agent that follows a broken one
        # may reach for `op` directly, which is what the vault abstraction exists to
        # prevent. `test_the_generated_credential_command_actually_parses` runs what is
        # written here through charter's real parser, because this shipped wrong twice:
        # once originally, and once when this string was rewritten and the order carried
        # forward unread.
        creds = (
            f"\n- **Credentials** come only from this persona's vault, and are **never "
            f"printed**:\n  `charter persona secret list --persona {name}` — use `exec`/`cp` "
            f"to consume a secret, never `--reveal`.\n  Run tools through it, e.g. "
            f"`charter persona secret exec --persona {name} "
            f"--file KUBECONFIG=kubeconfig -- kubectl -n <ns> get pods`"
        )
    else:
        creds = (
            f"\n- **This persona holds no credentials** (`vault: {persona.NO_VAULT}`). Never "
            f"read another persona's vault, and never improvise with a credential you find "
            f"in the environment — if a task needs one, hand off to the persona that owns it."
        )

    try:
        rel = persona.def_path(name).relative_to(config.ROOT)
    except ValueError:
        rel = f"personas/{name}/persona.md"

    mem = f"""
## Memory
- **Search before acting** — never bulk-read the indexes: `charter recall "<keywords>"` searches
  every base at once (your memory, shared, the active workspace) and labels hits by source.
- **Record what's durable** as you learn: `charter persona remember {name} "<one fact>"` — one
  curated idea each; `--shared` if every persona benefits, `--ephemeral` for session scratch.
  Persistent memory commits + pushes itself immediately, so it reaches the team as you write it.
- **Never** put secrets in memory/refs — credentials live only in the vault."""

    body = f"""<!-- {_AGENT_MARKER} from {rel} — edit the persona, not this file. -->

This sub-agent acts as the **{name}** persona — {role} — in an
isolated context. Adopt the charter below as your role.

{charter}

## As a persona sub-agent
- {_credential_rule()}{creds}{bin_note}{uses_note}{memory_note}{handoff}
- Follow the control plane's conventions (see CLAUDE.md); report results concisely to the caller.
{mem}
"""
    return "---\n" + "\n".join(fm) + "\n---\n" + body


def _write_agent(name: str) -> str | None:
    """Generate/refresh one persona's sub-agent. Returns 'written'|'skipped'|'draft'|None.
    Uses the RESOLVED persona (inheritance applied: merged charter + unioned tools)."""
    d = persona.resolve(name)
    if not d:
        return None
    if persona.is_draft(name):
        # The generated file IS the sub-agent's system prompt, so an unfinished charter
        # must not become one. Any agent generated BEFORE the persona was marked draft is
        # removed rather than left behind: a stale file keeps the persona dispatchable,
        # which is precisely what the flag exists to prevent. Hand-written agents (no
        # marker) are never charter's to touch.
        _remove_agent(name)
        return "draft"
    path = _agents_dir() / f"{name}.md"
    if path.exists() and _AGENT_MARKER not in path.read_text():
        util.warn(f"{path.relative_to(config.ROOT)} exists and isn't generated — "
                  "leaving the hand-written agent alone.")
        return "skipped"
    _agents_dir().mkdir(parents=True, exist_ok=True)
    path.write_text(_render_agent(name, d["meta"], d["charter"]))
    return "written"


def _remove_agent(name: str) -> bool:
    """Remove a persona's generated agent (never a hand-written one). Returns True if removed."""
    path = _agents_dir() / f"{name}.md"
    if path.exists() and _AGENT_MARKER in path.read_text():
        path.unlink()
        return True
    return False


# --------------------------------------------------------------------------- #
# memory: persistent (committed) + ephemeral (session scratch) + activity log  #
# --------------------------------------------------------------------------- #
def _require(name: str) -> bool:
    if not persona.load(name):
        util.err(f"no persona '{name}' (create it: charter persona create {name})")
        return False
    return True


def cmd_persona_remember(args) -> int:
    name = persona.resolve_active(getattr(args, "persona", None)) if not args.name else args.name
    if not name or not _require(name):
        return 1
    try:
        p = persona.remember(name, args.text, title=args.title,
                             shared=args.shared, ephemeral=args.ephemeral)
    except ValueError as e:
        util.err(str(e))
        return 1
    where = ("shared " if args.shared else "") + ("ephemeral" if args.ephemeral else "persistent")
    util.ok(f"Remembered ({where}) → {p.relative_to(config.ROOT) if _under_root(p) else p}")
    if args.ephemeral:
        return 0  # session scratch, gitignored — nothing to commit
    if getattr(args, "no_sync", False):
        util.info("  (--no-sync) recorded locally; share later with: charter persona memory-sync.")
        return 0
    # Reactive: the persistent memory reaches the shared repo the moment it's written.
    from .commands import commit_memory_reactive
    idx = persona.index_of(persona.memory_dir(name, args.shared))
    rels = [str(p.relative_to(config.ROOT)), str(idx.relative_to(config.ROOT))]
    commit_memory_reactive(rels, f"persona({name}): {p.stem}")
    return 0


def _under_root(p) -> bool:
    try:
        p.relative_to(config.ROOT)
        return True
    except ValueError:
        return False


def _snippet(path, width=90) -> str:
    try:
        for ln in path.read_text().splitlines():
            ln = ln.strip()
            if ln and not ln.startswith("#") and not ln.startswith("_"):
                return ln[:width] + ("…" if len(ln) > width else "")
    except OSError:
        pass
    return ""


def cmd_persona_recall(args) -> int:
    name = args.name or persona.resolve_active()
    if not name or not _require(name):
        return 1

    # Retrieval: pull just the relevant memories instead of the whole index.
    if getattr(args, "query", None):
        hits = persona.search_memories(name, args.query, limit=args.log or 8)
        if not hits:
            util.info(f"no memory of '{args.query}' for '{name}'.")
            return 0
        print(f"── memory matching '{args.query}' ({len(hits)})")
        for p, title, score in hits:
            rel = p.relative_to(config.ROOT) if _under_root(p) else p
            print(f"  [{score:>3}] {title}\n        {rel}\n        {_snippet(p)}")
        return 0

    printed = False
    for shared, label in ((False, name), (True, "_shared (all personas)")):
        idx = persona.index_of(persona.memory_dir(name, shared=shared))
        mems = persona.memories(name, shared=shared)
        if mems:
            printed = True
            print(f"── persistent memory · {label} ({len(mems)}) "
                  f"[{persona.memory_dir(name, shared=shared).relative_to(config.ROOT)}/]")
            print(idx.read_text().strip() if idx.exists() else "(no index)")
            print()
    eph = persona.memories(name, ephemeral=True) + persona.memories(name, shared=True, ephemeral=True)
    if eph:
        printed = True
        print(f"── ephemeral scratch · this session ({len(eph)})")
        for p in eph:
            print(f"- {p.stem}")
        print()
    acts = trace.for_persona(name, n=args.log)
    if acts:
        printed = True
        print(f"── recent activity · this session ({len(acts)})")
        for r in acts:
            extra = "  ".join(f"{k}={v}" for k, v in r.items() if k not in ("ts", "event", "persona"))
            print(f"  {r.get('ts', '')}  {r['event']:10} {extra}")
    if not printed:
        util.info(f"persona '{name}' has no memories yet. "
                  f"Add one: charter persona remember {name} \"<fact>\"")
    return 0


def cmd_persona_forget(args) -> int:
    name = args.name
    if not _require(name):
        return 1
    if persona.forget(name, args.slug, shared=args.shared, ephemeral=args.ephemeral):
        util.ok(f"Forgot '{args.slug}' from {name}'s "
                f"{'ephemeral' if args.ephemeral else 'persistent'} memory.")
        if args.ephemeral:
            return 0  # session scratch, gitignored — nothing to commit
        # Symmetric with `remember`: a removal that stays on one machine is not a
        # removal. Staging the memory DIRECTORY rather than the deleted file is
        # deliberate — `git add` on a path that no longer exists and was never tracked
        # fails the whole call, taking the index update down with it (#82).
        from .commands import commit_memory_reactive
        d = persona.memory_dir(name, args.shared)
        commit_memory_reactive([str(d.relative_to(config.ROOT))],
                               f"persona({name}): forget {args.slug}")
        return 0
    util.err(f"no memory '{args.slug}' in that store")
    return 1


_MEM_PATH = __import__("re").compile(r"personas/[^/]+/(?:memory|refs)/")


def _pending_memory(root) -> list[str]:
    """Repo-relative persona memory/refs paths with uncommitted changes."""
    import subprocess
    r = subprocess.run(["git", "-C", str(root), "status", "--porcelain", "--", "personas"],
                       stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, timeout=10)
    out = []
    for line in r.stdout.splitlines():
        if not line.strip():
            continue
        path = line[3:].strip()
        if " -> " in path:  # rename → take the destination
            path = path.split(" -> ", 1)[1]
        if _MEM_PATH.search("/" + path):
            out.append(path)
    return out


def cmd_persona_memory_sync(args) -> int:
    """Commit (and push) all pending persona memory/refs in one safe step — the counterpart
    to `remember`: durable knowledge gets shared instead of sitting uncommitted. Refuses if a
    file looks like it holds a secret (those belong in the vault, never in memory).

    Delegates to :func:`charter.planegit.commit_push`. It used to have its own committer,
    which had drifted to `git push origin HEAD` — over SSH, in violation of charter's
    headline one-credential rule, on the one memory path the SessionStart hook explicitly
    tells an agent to use. It reported "Committed locally, but push failed (check git
    auth)" while `gh auth status` was perfectly happy, because the token was never offered.
    """
    from . import planegit
    from .hooks import _secret_kind

    root = config.ROOT
    changed = _pending_memory(root)
    if not changed:
        util.ok("No uncommitted persona memory/refs — nothing to sync.")
        return 0

    # Kept ahead of `commit_push`'s own identical guard: this one runs BEFORE staging, so
    # a refusal leaves the index untouched, and it can name persona memory specifically.
    flagged = []
    for p in changed:
        try:
            kind = _secret_kind((root / p).read_text())
        except OSError:
            kind = None
        if kind:
            flagged.append((p, kind))
    if flagged:
        util.err("Refusing to commit — a secret-shaped value in persona memory/refs:")
        for p, kind in flagged:
            util.err(f"  {p}  ({kind})")
        util.info("Secrets live only in the vault (`charter persona secret set`). Remove it, then retry.")
        return 1

    touched = sorted({p.split("/")[1] for p in changed if p.startswith("personas/")})
    msg = (f"personas: sync memory/refs ({', '.join(touched)})\n\n"
           f"{len(changed)} persona memory/ref file(s) committed so the team's personas "
           f"share the knowledge. Synced via `charter persona memory-sync`.")
    return planegit.commit_push(root, ["add", "--", *changed], msg,
                                no_push=getattr(args, "no_push", False))


def cmd_persona_dedupe(args) -> int:
    name = args.name or persona.resolve_active()
    if not name or not _require(name):
        return 1
    dupes = persona.find_duplicates(name, threshold=args.threshold)
    if not dupes:
        util.ok(f"no near-duplicate memories for '{name}' (threshold {args.threshold}).")
        return 0
    print(f"Near-duplicate memory pairs for '{name}' (Jaccard ≥ {args.threshold}):\n")
    for jac, pa, ta, pb, tb in dupes:
        ra = pa.relative_to(config.ROOT) if _under_root(pa) else pa
        rb = pb.relative_to(config.ROOT) if _under_root(pb) else pb
        print(f"  {jac:.0%}  {ta}\n        ↔ {tb}\n        {ra}\n        {rb}\n")
    util.info("Review, then drop one: charter persona forget <name> <slug> [--shared]")
    return 0


# --------------------------------------------------------------------------- #
# lint: deterministic config eval for personas (routing/guards are in the tests) #
# --------------------------------------------------------------------------- #
def _agent_sync_issues(name: str) -> list[tuple[str, str]]:
    """Is the generated .claude/agents/<name>.md in sync with the persona?"""
    d = persona.resolve(name)  # resolved, so a parent charter/tool change marks children stale
    if not d:
        return []
    if persona.is_draft(name):
        return []  # by design, and the draft warning already says so — telling the author
                   # to run sync-agents here would be advice that cannot work
    path = _agents_dir() / f"{name}.md"
    if not path.exists():
        return [("warn", "no generated sub-agent — run `charter persona sync-agents`")]
    cur = path.read_text()
    if _AGENT_MARKER not in cur:
        return []  # hand-written agent — never touched
    if cur.strip() != _render_agent(name, d["meta"], d["charter"]).strip():
        return [("warn", "generated sub-agent is stale — run `charter persona sync-agents`")]
    return []


def cmd_persona_lint(args) -> int:
    names = [args.name] if getattr(args, "name", None) else persona.list_personas()
    if not names:
        util.info("No personas to lint.")
        return 0
    # `--only` narrows to ONE finding so an exit code can answer one question. A news
    # entry's probe needs exactly that: bare `lint` fails for dangling `uses:` too, and a
    # probe that fires on unrelated findings tells a plane to adopt what it already has.
    only = (getattr(args, "only", None) or "").strip()
    errors = 0
    for n in names:
        issues = list(persona.lint(n)) + _agent_sync_issues(n)
        if only:
            issues = [(lvl, msg) for lvl, msg in issues if only in msg]
            # Every match is an error under `--only`: the caller asked about one thing, so
            # "present but only as a warning" is still the answer "not adopted".
            issues = [("error", msg) for _lvl, msg in issues]
        if not issues:
            util.ok(f"{n}: ok")
            continue
        for level, msg in issues:
            if level == "error":
                errors += 1
                util.err(f"{n}: {msg}")
            else:
                util.warn(f"{n}: {msg}")
    if errors:
        util.err(f"{errors} error(s) — dangling reuse or unloadable persona.")
        return 1
    return 0


def cmd_persona_stats(args) -> int:
    """Roster health mined from committed memory — a persona's memory IS its activity
    trace, so this is the usage + (in-corpus) quality signal for the steward's observe
    loop. Read-only. Sorted by volume; flags idle/dormant personas as prune candidates."""
    names = [args.name] if getattr(args, "name", None) else persona.list_personas()
    if not names:
        util.info("No personas yet.")
        return 0
    if not getattr(args, "name", None):
        names = names + [config.SHARED_PERSONA]  # include the shared namespace
    from . import dispatch
    rows = [persona.stats(n, recent_days=getattr(args, "recent_days", 14)) for n in names]
    rows.sort(key=lambda r: (-r["count"], r["persona"]))
    # DISPATCH is the signal memory volume is blind to: a persona can hold plenty of
    # memory and still never be *used*, while the work it owns routes to a generic agent.
    disp = dispatch.tally()
    glyph = {"active": "●", "idle": "○", "dormant": "✗", "draft": "⚑",
             "orchestrator": "⬡", "standby": "◇", "advisory": "◇"}
    print(f"{'PERSONA':<28}{'MEM':>5}{'RECENT':>8}{'VERIFY':>8}{'DUP':>6}{'DISP':>6}  STATUS")
    dormant = idle = unused = drafts = 0
    for r in rows:
        v = f"{r['verify_pct']}%" if r["verify_pct"] is not None else "—"
        d = f"{r['dup_pct']}%" if r["dup_pct"] is not None else "—"
        rec = f"{r['recent']}" if r["count"] else "—"
        n_disp = disp.get(r["persona"], 0)
        shared_row = r["persona"] == config.SHARED_PERSONA
        # "never dispatched" outranks the memory-derived status — it's the louder problem.
        # A draft outranks BOTH: charter refuses to generate its sub-agent, so it *cannot*
        # be dispatched. Counting it as "never dispatched" would blame a persona for
        # obeying a rule we impose on it, and would bury the real signal among false ones.
        status = r["status"]
        if not shared_row and persona.is_draft(r["persona"]):
            status = "draft"
            drafts += 1
        elif not shared_row and n_disp == 0 and disp:
            status = "never dispatched"
            unused += 1
        print(f"{r['persona']:<28}{r['count']:>5}{rec:>8}{v:>8}{d:>6}"
              f"{(n_disp if not shared_row else '—'):>6}  "
              f"{glyph.get(status, '⚑' if status == 'never dispatched' else '·')} {status}")
        dormant += r["status"] == "dormant"
        idle += r["status"] == "idle"
    print()

    # Declared-vs-used skills. A separate block rather than a column, because it is per
    # persona and variable-length — and because it is the answer to a different question
    # than the table's: the table asks whether a persona is USED, this asks whether the
    # equipment it carries is worth carrying.
    from . import skilluse
    drifted = []
    for r in rows:
        if r["persona"] == config.SHARED_PERSONA:
            continue
        d = skilluse.drift(r["persona"])
        if d["unused"] or d["undeclared"]:
            drifted.append((r["persona"], d))
    if drifted:
        print("SKILLS — declared vs actually invoked")
        for name, d in drifted:
            if d["unused"]:
                # Not untidy: `skills:` preloads full text on EVERY dispatch, so an unused
                # declaration is a standing context cost bought for nothing.
                print(f"  {name:<26} unused: {', '.join(d['unused'])}"
                      f"   (preloaded every dispatch)")
            if d["undeclared"]:
                print(f"  {name:<26} used but not declared: {', '.join(d['undeclared'])}")
        print()
    util.info(f"RECENT = memories in the last {getattr(args, 'recent_days', 14)} days · "
              f"VERIFY = share carrying a verification marker (quality proxy) · DUP = share "
              f"in a near-dup pair (noise) · DISP = times DISPATCHED as a sub-agent (committed "
              f"tally) · ⬡/◇ = memory-blind role (activity: profile), not judged by volume.")
    gen, total = dispatch.generic_share()
    if total:
        util.info(f"Routing: {total - gen}/{total} dispatches went to a persona · {gen} to a "
                  f"generic agent ({100 * gen // total}%). A high generic share means the work "
                  f"a persona owns is being done without it.")
    # Fired-vs-followed. The roster block is a bet that showing who exists changes where
    # work goes; this is the pair of numbers that can falsify it. Silent when advice has
    # never fired — a "fired 0 · dispatched 0" row on every plane that has not opted in is
    # a line people learn to skip, and it takes the rest of the report with it.
    advice = dispatch.advice_tally()
    if advice:
        since = dispatch.first_advice()
        followed = dispatch.handoffs_since_first_advice()
        # SINCE the first advice, never the lifetime total: a dispatch older than the
        # roster cannot have followed it, and pairing the two made this line claim five
        # dispatches followed one piece of advice — three of them four days its senior.
        # "since" rather than "because", because a window is all the data supports.
        util.info(f"Routing advice: fired {advice} time(s) · work handed to a persona "
                  f"{followed} time(s) since the first one ({since:%Y-%m-%d}). Advice that "
                  f"fires and is never followed is the block failing, not the roster — "
                  f"read it that way before adding more personas.")
    if drifted:
        util.info("SKILLS drift is named, not resolved: an unused declaration may be dead "
                  "weight or a skill whose moment has not come, and an undeclared one may "
                  "be a charter out of date or a persona reaching past its remit. Which it "
                  "is depends on intent charter cannot read.")
    if not total:
        # Attached to the TALLY, not to the drift check above it. It was the `else` of
        # `if drifted:` — so a roster whose skills all lined up was told its dispatch tally
        # was empty, beside a table showing five. Invisible while every plane had some
        # drift; surfaced the moment one stopped.
        util.info("No dispatches recorded yet — the tally starts filling as sub-agents are "
                  "dispatched (`charter persona dispatch-backfill` seeds it from past sessions).")
    # How complete this tally is, stated rather than implied. `PostToolUse(Task|Agent)`
    # does not fire for every background dispatch, so DISP, ⚑ and the routing ratio are
    # all floors, not counts — three real dispatches were missing from one plane's store
    # while `stats` reported `0 / never dispatched` (#83). A count that silently omits
    # them reads exactly like one that includes them, and personas get retired on it.
    last = dispatch.last_backfill()
    when = f"last reconciled {last:%Y-%m-%d}" if last else "never reconciled"
    util.info(f"Tallied live from a PostToolUse hook, which can miss background "
              f"dispatches — treat DISP and ⚑ as a FLOOR ({when}). Reconcile against "
              f"this project's transcripts: charter persona dispatch-backfill.")
    if unused:
        util.warn(f"{unused} persona(s) NEVER dispatched — they exist, lint green, and are "
                  f"unused. Check whether their work is routing to a generic agent instead.")
    if drafts:
        util.info(f"{drafts} draft persona(s) — charter generates no sub-agent while "
                  f"`draft: true` is set, so they are undispatchable BY DESIGN and are not "
                  f"counted above. Finish the charter, drop the line, then sync-agents.")
    if dormant:
        util.warn(f"{dormant} dormant persona(s) — a REAL prune signal (old, zero memory, no "
                  f"declared activity: profile). The steward can quiz-propose removal (cite this).")
    if idle:
        util.info(f"{idle} idle persona(s) — have memory but none recent; watch, don't prune yet.")
    return 0


def cmd_persona_dispatch_backfill(args) -> int:
    """Seed the committed dispatch tally from this project's past transcripts, so the
    routing baseline exists now rather than a week from now. Reads only tool name,
    subagent_type and timestamp — no prompt text ever reaches the store."""
    from . import dispatch
    from .commands import commit_memory_reactive
    imported, skipped = dispatch.backfill()
    if not imported and not skipped:
        util.warn(f"No dispatches found in {dispatch._transcript_dir()} — nothing to seed.")
        return 0
    util.ok(f"Seeded {imported} past dispatch(es) into the committed tally.")
    if skipped:
        util.info(f"  skipped {skipped} already covered by live records (no double-count).")
    t = dispatch.tally()
    gen, total = dispatch.generic_share()
    for agent, n in t.most_common():
        mark = "  (generic)" if agent in dispatch.GENERIC else ""
        util.info(f"  {n:>5}  {agent}{mark}")
    if total:
        util.info(f"Baseline: {100 * gen // total}% of dispatches went to a generic agent.")
    rel = [str(p.relative_to(config.ROOT)) for p in sorted(dispatch._dir().glob("*.jsonl"))]
    if rel:
        commit_memory_reactive(rel, f"dispatch: backfill {imported} past dispatch(es)")
    return 0


def cmd_persona_optimize(args) -> int:
    """Optimize persona memory (one, or --all): analyze the corpus, AUTO-apply only the
    tier-1 safe & reversible ops with --apply (collapse exact duplicates → archive the
    redundant copies; repair the index), and print the tier-2 PROPOSALS (near-dup merges,
    stale archives, charter promotions) for the steward to quiz on — never auto-applied,
    never a silent charter edit. Read-only without --apply. Mirrors steward's Hat 2 model."""
    from . import curate
    from .commands import commit_memory_reactive
    names = [args.name] if getattr(args, "name", None) else persona.list_personas()
    if not names:
        util.info("No personas to optimize.")
        return 0
    if getattr(args, "all", False) or not getattr(args, "name", None):
        if config.SHARED_PERSONA not in names:
            names = names + [config.SHARED_PERSONA]
    apply = getattr(args, "apply", False)
    stale_days = getattr(args, "stale_days", 90)
    total_actions = 0
    for n in names:
        shared = n == config.SHARED_PERSONA
        mdir = persona.memory_dir(n, shared=shared)
        rep = curate.report(mdir, stale_days=stale_days)
        if rep["total"] == 0:
            continue
        st = persona.stats(n, shared=shared)
        print(f"\n◆ {n}  ({rep['total']} memories · {st['verify_pct'] or 0}% verified · "
              f"{len(rep['exact_dups'])} exact-dup group(s) · {len(rep['near_dups'])} near-dup "
              f"pair(s) · {len(rep['stale'])} stale)")
        if apply:
            actions = curate.apply_safe(mdir)
            for a in actions:
                util.ok(f"  auto: {a}")
            total_actions += len(actions)
            if actions:  # reactive: the persona's committed memory changed → share it now
                rel = str(mdir.relative_to(config.ROOT))
                commit_memory_reactive([rel], f"persona({n}): curate — {len(actions)} safe op(s)")
            rep = curate.report(mdir, stale_days=stale_days)  # refresh for proposals
        else:
            # A read-only run must name the ops --apply would perform. Silently
            # rewriting an index the report never mentioned is how "read-only"
            # stops meaning anything.
            pending = curate.pending_auto(rep)
            if pending:
                print("  would auto-apply (re-run with --apply):")
                for a in pending:
                    print(f"    + {a}")
        props = curate.proposals(rep)
        if props:
            print("  proposals (steward: quiz the engineer — not auto-applied):")
            for p in props:
                print(f"    ? {p}")
        elif apply:
            util.info("  clean — nothing to propose.")
    if not apply:
        util.info("\nRead-only. Re-run with --apply to auto-apply the safe/reversible ops "
                  "(exact-dup collapse + index repair); proposals always stay manual.")
    elif total_actions == 0:
        util.info("\nNo safe ops to apply — corpus is already tidy.")
    return 0


def cmd_persona_log(args) -> int:
    """Record a manual `note` into the session trace, or show this persona's activity
    from it. (One activity record: the trace — see `charter trace`.)"""
    name = args.name or persona.resolve_active()
    if not name or not _require(name):
        return 1
    if args.message:
        trace.record("note", persona=name, msg=args.message)
        util.ok(f"Noted to {name}'s session activity (see `charter trace` / `charter persona recall {name}`).")
        return 0
    entries = trace.for_persona(name, n=args.n)
    if not entries:
        util.info(f"no activity for '{name}' in this session yet.")
        return 0
    for r in entries:
        extra = "  ".join(f"{k}={v}" for k, v in r.items() if k not in ("ts", "event", "persona"))
        print(f"{r.get('ts', '')}  {r['event']:10} {extra}")
    return 0


def cmd_persona_migrate(args) -> int:
    persona.ensure_shared()  # make sure the cross-persona namespace exists
    names = [args.name] if args.name else persona.list_personas()
    migrated, already = [], []
    for n in names:
        r = persona.migrate(n)
        if r == "migrated":
            migrated.append(n)
        elif r == "already":
            already.append(n)
            persona.scaffold_memory(n)  # backfill memory/refs on already-dir personas
    if migrated:
        util.ok(f"Migrated to directory layout: {', '.join(migrated)}")
        _sync_all_agents()  # regenerate so the GENERATED marker points at persona.md
    if already:
        util.info(f"Already directory-layout: {', '.join(already)}")
    if not migrated and not already:
        util.info("No personas to migrate.")
    util.info("Review with `git status` and commit the moves + scaffolding.")
    return 0


def _sync_all_agents() -> None:
    for n in persona.list_personas():
        _write_agent(n)


def cmd_trace(args) -> int:
    """Observability: show a session's activity trace (guard denials, tool approvals,
    secret warnings, memory writes, persona switches) — raw or aggregated."""
    sess = getattr(args, "session", None) or trace._session()
    events = trace.read(sess)
    if not events:
        avail = ", ".join(trace.sessions()) or "none"
        util.info(f"no trace for session '{sess}'. Sessions with a trace: {avail}")
        return 0
    if getattr(args, "summary", False):
        from collections import Counter
        by_event = Counter(e["event"] for e in events)
        personas = sorted({e.get("persona") for e in events if e.get("persona")})
        tools = Counter(e["tool"] for e in events if e.get("event") == "allow" and e.get("tool"))
        denies = [e for e in events if e["event"] == "deny"]
        warns = [e for e in events if e["event"] == "secret-warn"]
        print(f"session {sess}: {len(events)} events")
        print("  by event : " + ", ".join(f"{k}={v}" for k, v in by_event.most_common()))
        if personas:
            print("  personas : " + ", ".join(personas))
        if tools:
            print("  approved : " + ", ".join(f"{k}×{v}" for k, v in tools.most_common()))
        if denies:
            print(f"  guard denials ({len(denies)}):")
            for e in denies[-5:]:
                print(f"    {e.get('ts', '')}  {e.get('reason', '')}")
        if warns:
            print(f"  secret warnings ({len(warns)}): "
                  + ", ".join(w.get("file", "") for w in warns[-5:]))
        # Credential hand-outs get their own line rather than only a tally, for the same
        # reason denials do: "which command received the prod token" is a question somebody
        # asks under pressure, and an aggregate that hides the answer one `charter trace`
        # invocation deeper is an aggregate they will not trust twice (#441).
        uses = [e for e in events if e["event"] in trace.SECRET_USE_EVENTS]
        if uses:
            print(f"  credentials handed out ({len(uses)}):")
            for e in uses[-5:]:
                keys = ",".join(e.get("key_names") or ()) or "-"
                where = e.get("argv0") or e.get("dest") or "this terminal"
                print(f"    {e.get('ts', '')}  {e['event']:14} "
                      f"{e.get('vault', '?')}/{keys} → {where}")
        return 0
    shown = events[-args.n:] if getattr(args, "n", 0) else events
    for e in shown:
        extra = "  ".join(f"{k}={v}" for k, v in e.items() if k not in ("ts", "event"))
        print(f"{e.get('ts', '')}  {e['event']:12} {extra}")
    return 0


def cmd_persona_gc(args) -> int:
    """Hidden: prune ephemeral scratch from ended sessions (SessionStart hook)."""
    # `--detach` is checked first: the point is to return BEFORE any of the work
    # below, in a process the harness will not tear down with the turn. This is
    # what a hook's `"async": true` used to buy — asked of the host, and one host
    # skips such entries outright, so charter does it itself.
    if getattr(args, "detach", False):
        util.detach_self(['persona', '_gc'])
        return 0

    current = os.environ.get("CLAUDE_CODE_SESSION_ID")
    if not current:
        try:
            import sys, json as _json
            current = (_json.load(sys.stdin) or {}).get("session_id")
        except Exception:
            current = None
    n = persona.gc_ephemeral(current)
    if n:
        util.info(f"pruned {n} ended-session ephemeral store(s)")
    return 0


def cmd_persona_sync_agents(args) -> int:
    one = getattr(args, "persona", None)
    if one and not persona.load(one):
        util.err(f"no persona '{one}'")
        return 1
    names = [one] if one else persona.list_personas()
    if not names:
        util.info("No personas to sync. Create one first: charter persona create <name>.")
        return 0

    # BEFORE the render, because the render is what consults the record: approving after
    # it would write this run's agents without their credentials and only take effect on
    # the next run, which reads as the flag not working.
    if getattr(args, "approve_mcp", False):
        for n in names:
            declared = persona.mcp_credentialed(n)
            if not declared:
                continue
            mcpseen.approve(n, [fp for _s, _e, fp in declared])
            for server, entry, _fp in declared:
                # Printed, not merely recorded: an approval nobody can see in the
                # transcript is not consent, it is a flag that was typed.
                util.info(f"  approved {n}/{server} → {mcpseen.describe(entry)}")

    outcomes = {n: _write_agent(n) for n in names}
    written = [n for n, o in outcomes.items() if o == "written"]
    drafts = [n for n, o in outcomes.items() if o == "draft"]
    withheld = {n: persona.mcp_withheld(n) for n in written}
    withheld = {n: v for n, v in withheld.items() if v}

    removed = []
    if not one and _agents_dir().exists():  # full sync also prunes orphaned generated agents
        existing = set(persona.list_personas())
        for f in _agents_dir().glob("*.md"):
            if f.stem not in existing and _AGENT_MARKER in f.read_text():
                f.unlink()
                removed.append(f.stem)

    util.ok(f"Synced {len(written)} persona sub-agent(s) → "
            f".claude/agents/ ({', '.join(written) or 'none'})")
    if drafts:
        util.warn(f"Skipped {len(drafts)} draft persona(s): {', '.join(drafts)} — "
                  "an unfinished charter must not become a sub-agent's system prompt. "
                  "Finish it, drop the `draft: true` line, then re-run.")
    if removed:
        util.info("Removed stale generated agents: " + ", ".join(removed))
    if withheld:
        # A warning, not an error: the agents were written and the personas still work.
        # What did not happen is the credential hand-off, and saying so in the words of
        # the command that would restore it is the whole point — a silent downgrade here
        # would surface as an MCP server failing to authenticate, three layers away.
        util.warn(f"Withheld the vault from {sum(len(v) for v in withheld.values())} MCP "
                  f"server(s): the committed `mcp.json` names the command that would "
                  f"receive it, and this one has not been approved on this machine.")
        for n, servers in sorted(withheld.items()):
            for server, entry in servers:
                util.info(f"  {n}/{server} → {mcpseen.describe(entry)}")
        util.info("  Read the command above. If it is what you expect, approve it with:")
        util.info("    charter persona sync-agents --approve-mcp")
    for n in written:
        util.info(f"  invoke '{n}' via the Agent/Task tool (subagent_type: {n})")
    if written:
        util.info("New/changed agents load on the next Claude Code session (restart to use now).")
    return 0
