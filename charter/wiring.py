"""Is a harness profile wired — does charter's guard actually run in the folder it names?

A profile exists to point a harness at another config folder, and **every kind loses
charter's wiring when its folder moves**. Measured 2026-09-11 and again on 2026-09-12
against claude 2.1.269, codex-cli 0.147.0 and opencode 1.18.23, in throwaway folders:

* Claude Code under an empty `$CLAUDE_CONFIG_DIR` has no charter plugin at all — not even
  "enabled but not installed" — because the `charter` marketplace is known only to the
  folder it was added in.
* Codex under an empty `$CODEX_HOME` has no plugin, no hook trust and no
  `shell_environment_policy.set`.
* opencode under a throwaway `$XDG_CONFIG_HOME` has no shim, loads no plugin, and its
  shells get no `$CHARTER_HARNESS`.

So a chat started on such a profile *looks* guarded and is not, which is the failure this
module exists to stop. A profile that is not wired refuses to launch and prints the fix —
**built-ins included** (ruling 10): `charter codex` on a plane where nobody wired Codex now
refuses where it used to start, because a chat that looks guarded and is not is the same
failure whichever profile started it.

**Wiring is detected by ASKING the harness under the profile's own environment**, never by
reading the profile's variable names. One account can be reached through variables that do
or do not move the plugin — `$XDG_DATA_HOME` moves opencode's login and leaves its plugins
where they are — so a rule written from variable names is a rule about the wrong thing.
`claude plugin list --json` and `opencode debug config` follow the environment they are
given; Codex's three marks are all in one file.

**An unknown is never a pass** (ADR 0009, ruling 12). A probe that times out, exits
non-zero or answers something unparseable refuses the launch with a sentence naming the
probe to run by hand. No flag launches a profile unguarded.

**Nothing here runs on a hook path** (ruling 11). A probe costs 137-718 ms and writes into
the folder it asks about; `hooks/hooks.json` fires `charter doctor --preflight` at every
session start, and that mode builds no profile row and calls nothing here.

**A launch never trusts the cache** (review B2, ruling 21). :func:`cached` is for the
selector's rows and nothing else: the file sits under `.charter/`, which no path guard
covers (ADR 0014 — a path pattern is host policy), so a chat can write it, compute its key,
and date an entry ahead. :func:`refusal` always probes.
"""

from __future__ import annotations

import hashlib
import json
import os
import shlex
import time
import tomllib
from pathlib import Path
from typing import Mapping, NamedTuple

from . import config, contain, plugincache, profiles, util
from .harness import claude_code, codex, opencode

#: The three answers. `UNKNOWN_STATE` rather than `UNKNOWN`, because `plugincache.UNKNOWN`
#: is a sentinel object and these are strings that end up in a JSON cache.
WIRED, UNWIRED, UNKNOWN_STATE = "wired", "unwired", "unknown"

#: The launcher refusal kind this module produces. Callers branch on the KIND and never on
#: the text (ruling 27): a sentence gets reworded, and a comparison against one is a guard
#: that stops guarding on the day it does.
KIND_WIRING = "wiring"

#: Under `config.STATE_DIR`.
CACHE = "cache/harness-wiring.json"

#: How long a remembered answer may be reused by the selector. Both bounds matter: without
#: the lower one, an entry a chat dated into the future passes the age test forever.
MAX_AGE = 24 * 3600

#: The key prefix Codex writes its hook-trust ledger under, per plugin.
CODEX_TRUST_PREFIX = f"{plugincache.PLUGIN_ID}:hooks/hooks.json:"

#: How a Codex plugin is installed, pinned against codex-cli 0.147.0 by running it
#: (D4, 2026-09-12): `codex plugin` has add / list / marketplace / remove and no `install`.
#: Printed with `CODEX_HOME=` in front of it, never run — it installs software into an
#: account folder, and running this command IS the consent for the one line charter writes.
CODEX_STEPS = ("codex plugin marketplace add https://github.com/diazoxide/charter",
               f"codex plugin add {plugincache.PLUGIN_ID}",
               "start codex once and approve charter's hooks when it asks")

# Every sentence says the rule worked and names the fix in the same breath (CONTEXT.md,
# *A refusal is the rule working*). No "charter:" prefix — each caller says it its own way,
# the way `launcher`'s own texts do. Every profile-derived value is contained first
# (ruling 35): `charter.local.toml` is a file a chat can write, and a `\r` or an ESC in a
# command could redraw the line above to show a harmless command while another one runs.
NOT_WIRED = ("profile '{name}' is not wired — {detail}, so a chat on it would run without "
             "charter's guard. Nothing was started. Wire it: {fix}")
CANNOT_TELL = ("charter could not ask {kind} whether profile '{name}' is wired ({detail}), "
               "so it will not start it unguarded. Nothing was started. Run "
               "charter harness install {name}, or check by hand: {probe}")

#: Why charter may not run a declared profile's own command yet. **Task 3's launch record
#: is what replaces this**; see :func:`approval_needed`.
NOT_APPROVED_YET = ("not approved yet — this charter cannot ask before a declared "
                    "command runs, so it runs none")

CODEX_POLICY_BY_HAND = (
    "{path} already has a [shell_environment_policy] table without charter's line, and "
    "charter does not edit TOML it did not write — nothing was changed. Add this line "
    'inside that table:\n  set = {{ CHARTER_HARNESS = "codex" }}\n'
    'or, if the table already has a `set`, add CHARTER_HARNESS = "codex" to it.')

#: Most specific first. Claude Code 2.1.269 resolves `enabled` itself — measured
#: 2026-09-12, every listed entry of one plugin id carries the same effective value, merged
#: local > project > user at the probe's own cwd — so this order decides nothing today. It
#: is here for the day that stops being true, and it is the order that fails CLOSED: a chat
#: that disables charter in `.claude/settings.local.json` is unwired whatever the user
#: entry says (ruling 28).
_SCOPE_ORDER = ("local", "project", "user")


class Wiring(NamedTuple):
    """What was asked, what it answered, and what to do about it."""

    #: WIRED | UNWIRED | UNKNOWN_STATE.
    state: str
    #: What was asked and what it answered, already contained — it is displayed.
    detail: str
    #: The command that would fix it; ``""`` when wired.
    fix: str


def approval_needed(p: profiles.Profile) -> str:
    """Why charter may not run *p*'s own command yet — ``""`` when it may.

    Detection runs `[*command, "plugin", "list", "--json"]` and `[*command, "debug",
    "config"]`: a probe is a launch of the profile's command by another name, so it is
    gated by the same rule (ruling 1, and the Global Constraint *No profile's command runs
    before that profile passes the ignored check and the trust check*).

    **Today that rule is Task 2's standing refusal of every declared profile.** Nothing on
    `main` yet stands for the operator's approval of a `command`, and `charter.local.toml`
    is a file a chat can write with no diff to show for it. Task 3's merge replaces this
    body with `profiletrust.approval_needed(p)` — one line, and
    `NothingUnapprovedIsRun.test_the_launcher_and_the_gate_agree_about_every_profile` is
    what fails if only one of the two places moves.
    """
    return "" if p.source == profiles.BUILTIN else NOT_APPROVED_YET


def environment(p: profiles.Profile) -> dict[str, str]:
    """The environment *p*'s command would be exec'd with, for a probe to run under.

    `launcher.environment` and not a second merge of the same three things: a probe that
    asked under a different environment than the launch would use is asking about a session
    nobody is about to start. ``framed=True`` because that is the merge that drops nothing —
    the unframed one removes `$CHARTER_SESSION_ID`, and `util.run`'s ``env`` is an overlay
    that cannot express a removal anyway.

    Imported at call time. `launcher` imports this module at the top, and ruling 43 keeps
    profile code off every `charter hook …` process's import path.
    """
    from .frame import launcher

    return launcher.environment(p, os.environ, framed=True)


def probe_argv(p: profiles.Profile) -> list[str]:
    """What charter runs to ask *p*'s harness whether it is wired — ``[]`` for Codex, whose
    three marks are a file read."""
    command = profiles.expanded_command(p)
    if p.harness == claude_code.NAME:
        return [*command, "plugin", "list", "--json"]
    if p.harness == opencode.NAME:
        return [*command, "debug", "config"]
    return []


def by_hand(p: profiles.Profile) -> str:
    """The probe as an operator would type it: the profile's variables, then the command.

    Contained piece by piece, `profiles.display`'s rule — this goes to a terminal, and both
    halves come out of a file a chat can write.
    """
    pieces = [contain.readable(f"{name}={value}") for name, value in p.env]
    argv = probe_argv(p)
    pieces.append(contain.readable(shlex.join(argv) if argv
                                   else f"read {codex.config_path(environment(p))}"))
    return " ".join(pieces)


def detect(p: profiles.Profile, *, cwd) -> Wiring:
    """Ask *p*'s harness, under *p*'s environment, whether charter's guard runs there.

    *cwd* is the directory the chat would start in — Claude Code resolves `enabled` there,
    and an install record is bound to the directory it was installed from, so this is not a
    detail that can be defaulted.

    Never cached and never memoised: the whole point of :func:`cached` living beside this
    is that only one of them may start a chat.

    **The approval gate is here and not only in the callers** (ruling 1). A probe IS a run of
    the profile's command, and there are four callers — the launcher, the selector, `harness
    install` and `doctor`. A gate each of them has to remember is a gate one of them will not,
    and the thing it lets through is a command out of a file a chat can write.
    """
    why = approval_needed(p)
    if why:
        return Wiring(UNKNOWN_STATE, contain.readable(why),
                      f"charter {contain.readable(p.name)}")
    env = environment(p)
    if p.harness == claude_code.NAME:
        return _claude(p, cwd=cwd, env=env)
    if p.harness == codex.NAME:
        return _codex(p, env=env)
    if p.harness == opencode.NAME:
        return _opencode(p, env=env)
    # A kind charter has no wiring check for is an UNKNOWN and therefore a refusal, not a
    # pass: the day a kind joins the registry without one, this is the row that says so.
    return Wiring(UNKNOWN_STATE, f"charter has no wiring check for {contain.readable(p.kind)}",
                  f"charter harness install {contain.readable(p.name)}")


def refusal(p: profiles.Profile, *, cwd) -> str:
    """Why *p* may not start, or ``""``. Always a fresh probe (review B2).

    An UNKNOWN refuses with its own sentence (ruling 12) rather than sharing the unwired
    one: "charter looked and the guard is absent" and "charter could not look" are
    different things to be told, and only one of them has `charter harness install` as its
    whole answer.
    """
    w = detect(p, cwd=cwd)
    name = contain.readable(p.name)
    if w.state == WIRED:
        return ""
    if w.state == UNWIRED:
        return NOT_WIRED.format(name=name, detail=w.detail, fix=w.fix)
    return CANNOT_TELL.format(kind=contain.readable(p.kind), name=name, detail=w.detail,
                              probe=by_hand(p))


# --------------------------------------------------------------------------- #
# Per kind                                                                     #
# --------------------------------------------------------------------------- #


def _claude(p: profiles.Profile, *, cwd, env: Mapping[str, str]) -> Wiring:
    """Claude Code: `claude plugin list --json`, run as *p* would run `claude`.

    Two facts out of the same answer, and they are resolved differently — measured
    2026-09-12 on 2.1.269 in throwaway folders:

    * the ENTRIES are install records, listed whatever the cwd, each bound to the directory
      it was installed from (`plugincache.covers`);
    * `enabled` is the EFFECTIVE `enabledPlugins` value for the plugin id resolved at the
      probe's own cwd — local over project over user — and every entry of that id carries
      the same value.

    So the probe is given *cwd*, and charter reads no settings file of its own (ruling 36's
    "unless"): a disable written ONLY to `<cwd>/.claude/settings.local.json`, and only to
    `<cwd>/.claude/settings.json`, each flipped the listed entry to `enabled: false` beside
    an enabled user-scope install. A second reader would answer for a different merge than
    the binary's — measured, `<ancestor>/.claude/settings.json` does NOT reach a session
    below it while `settings.local.json` does — and a wrong UNWIRED refuses a chat that
    would have been guarded.

    **An install covering the PLANE covers a chat in that plane's workspace** (D3). `charter
    init` installs at project scope for the plane root, and a chat's directory is
    `workspaces/<ws>/` — a different `projectPath`, which `covers` alone reads as not this
    plane's. Charter mirrors the plane's `enabledPlugins` into that directory
    (`claude_code.WORKSPACE_KEYS`), and the probe's own `enabled` is resolved there, so the
    record that covers the plane is the one that answers for the chat. Measured end to end:
    an alternate `$CLAUDE_CONFIG_DIR`, `charter init`, `charter workspace create w`, and
    `claude plugin list --json` in `workspaces/w` answering `enabled: true`.
    """
    folder = claude_code.config_home(env)
    entries = plugincache.covering_entries(cwd, root=config.ROOT, env=dict(env),
                                           command=profiles.expanded_command(p))
    where = f"{contain.readable(str(folder))} for {contain.readable(str(cwd))}"
    if entries is plugincache.UNKNOWN:
        return Wiring(UNKNOWN_STATE,
                      f"claude plugin list --json could not be read in {where}",
                      f"charter harness install {contain.readable(p.name)}")
    if not entries:
        return Wiring(UNWIRED, f"{plugincache.PLUGIN_ID} is not installed in {where}",
                      f"charter harness install {contain.readable(p.name)}")
    best = min(entries, key=lambda e: _SCOPE_ORDER.index(e.get("scope"))
               if e.get("scope") in _SCOPE_ORDER else len(_SCOPE_ORDER))
    scope = contain.readable(str(best.get("scope")))
    if best.get("enabled") is True:
        return Wiring(WIRED,
                      f"claude plugin list: {plugincache.PLUGIN_ID} enabled at {scope} "
                      f"scope in {where}", "")
    # NOT `charter harness install`: `plugincache.install` answers `present` for an install
    # that exists, so pointing back at it would print a fix that changes nothing and loops
    # — review 7's objection, one harness over. The enable is the command that moves this.
    fix = " ".join([*(contain.readable(f"{n}={v}") for n, v in p.env),
                    contain.readable(shlex.join([*profiles.expanded_command(p), "plugin",
                                                 "enable", plugincache.PLUGIN_ID]))])
    return Wiring(UNWIRED,
                  f"{plugincache.PLUGIN_ID} is installed at {scope} scope and reads as "
                  f"disabled in {where}", fix)


def _codex(p: profiles.Profile, *, env: Mapping[str, str]) -> Wiring:
    """Codex: three marks in `$CODEX_HOME/config.toml`, and it needs all three (ruling 7).

    The plugin declares the hooks, the policy line is the only thing that can tell a Codex
    shell which harness it is, and **a hook Codex has not trusted is inert** — so a plugin
    nobody approved is installed and does nothing, which reads exactly like wired to
    anything that stops at the plugin table.

    The trust rule is the measured one, not the planned one. `trusted_hash` cannot be
    recomputed from the plugin's `hooks/hooks.json`: the command string, the hook object as
    JSON in three spellings, the whole matcher group and the joined commands were all tried
    against a real hash on 2026-09-12 and none matched. And Codex writes an entry per hook
    **lazily**, as each first fires — the operator's own wired home held 12 of the plugin's
    18 keys — so "an entry for every hook key" would call a wired machine unwired. What is
    left is honest and weaker, and `docs/harnesses.md` says so: charter's hooks were
    approved in this home at least once.

    The home is the one charter can see. One a wrapper script exports on its way to `codex`
    is invisible here, and the docs state that limit.
    """
    path = codex.config_path(env)
    name = contain.readable(p.name)
    fix = f"charter harness install {name}"
    try:
        doc = tomllib.loads(path.read_text())
    except FileNotFoundError:
        doc = {}
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as e:
        return Wiring(UNKNOWN_STATE,
                      f"{contain.readable(str(path))} could not be read "
                      f"({contain.readable(str(e))})", fix)
    where = contain.readable(str(path))
    plugin = (doc.get("plugins") or {}).get(plugincache.PLUGIN_ID) or {}
    policy = ((doc.get("shell_environment_policy") or {}).get("set") or {})
    trusted = [k for k, v in ((doc.get("hooks") or {}).get("state") or {}).items()
               if str(k).startswith(CODEX_TRUST_PREFIX) and (v or {}).get("trusted_hash")]
    missing = []
    if plugin.get("enabled") is not True:
        missing.append(f"{plugincache.PLUGIN_ID} is not an enabled plugin")
    if policy.get("CHARTER_HARNESS") != codex.NAME:
        missing.append('shell_environment_policy.set has no CHARTER_HARNESS = "codex"')
    if not trusted:
        missing.append("no hook of charter's is trusted — approve them in a codex session")
    if not missing:
        return Wiring(WIRED, f"{where}: plugin enabled, harness named, "
                             f"{len(trusted)} trusted hook(s)", "")
    if policy.get("CHARTER_HARNESS") == codex.NAME:
        # Charter's own half is already written, so what is left is Codex's own commands and
        # a trust prompt only a person can answer. Naming `charter harness install` here
        # would name the command that has already done everything it can — review 7's
        # objection is about a fix that changes nothing, and this is the same fix one mark
        # further on.
        fix = codex_steps(path.parent)
    elif "shell_environment_policy" in doc:
        # `codex.install()` answers `present` for ANY `[shell_environment_policy]` table,
        # so `charter harness install` here would print a fix that changes nothing.
        fix = CODEX_POLICY_BY_HAND.format(path=where)
    return Wiring(UNWIRED, f"{where}: " + "; ".join(missing), fix)


def codex_steps(home) -> str:
    """:data:`CODEX_STEPS` with ``CODEX_HOME=`` in front of each command it can prefix.

    Printed and never run: they install software into an account folder and end in a trust
    prompt only a person can answer, and charter's own consent rule is that running the
    command IS the consent.
    """
    where = contain.readable(str(home))
    return "; ".join(f"CODEX_HOME={where} {step}" if step.startswith("codex ") else step
                     for step in CODEX_STEPS)


def _opencode(p: profiles.Profile, *, env: Mapping[str, str]) -> Wiring:
    """opencode: `debug config`, and all three marks (ruling 26).

    The entry has to name charter's shim, the shim's bytes have to be charter's, **and no
    foreign plugin may share its realm**. The third is not tidiness. opencode imports the
    whole plugin directory into ONE module realm and hands every plugin the same globals:
    reproduced against 1.18.21 and the real shim, a byte-perfect `charter.ts` beside
    `plugin/aaa_boot.ts` containing ``Object.hasOwn = () => false`` turns every guard lookup
    into `undefined`, so a vault read routes to the Bash guard and is allowed — while
    `shim_is_charters` says True throughout (`opencode.foreign_plugins`, ADR 0015's
    amendment). Measured 2026-09-12: `debug config` lists the foreign file too.

    `unvouched()` is not the test either — it answers ``()`` when the shim is missing, so an
    entry naming a deleted file would read as wired.
    """
    home = opencode.global_dir(env)
    name = contain.readable(p.name)
    fix = f"charter harness install {name}"
    argv = probe_argv(p)
    where = contain.readable(str(home))
    try:
        proc = util.run(argv, check=False, env=dict(env), timeout=plugincache.LIST_TIMEOUT)
    except (util.ProcTimeout, OSError) as e:
        return Wiring(UNKNOWN_STATE,
                      f"{contain.readable(shlex.join(argv))} did not answer "
                      f"({contain.readable(str(e))})", fix)
    if proc.returncode != 0:
        return Wiring(UNKNOWN_STATE,
                      f"{contain.readable(shlex.join(argv))} exited {proc.returncode}", fix)
    try:
        doc = json.loads(proc.stdout or "")
    except (ValueError, TypeError):
        return Wiring(UNKNOWN_STATE,
                      f"{contain.readable(shlex.join(argv))} answered something that is "
                      f"not JSON", fix)
    entries = doc.get("plugin") if isinstance(doc, dict) else None
    if isinstance(entries, str):
        entries = [entries]
    want = _resolved(home / opencode.SHIM_PATH)
    named = any(_resolved(_unprefixed(e)) == want for e in (entries or [])
                if isinstance(e, str))
    if not named:
        return Wiring(UNWIRED, f"opencode loads no plugin from {where}", fix)
    if not opencode.shim_is_charters(home):
        return Wiring(UNWIRED, f"{where}/{opencode.SHIM_PATH} is not the file charter "
                               f"writes — charter cannot vouch for it", fix)
    foreign = opencode.foreign_plugins(home)
    if foreign:
        return Wiring(UNWIRED,
                      f"{where}/{opencode.PLUGIN_DIR} also holds "
                      f"{contain.readable(', '.join(foreign))}, which shares one module "
                      f"realm with charter's shim", "remove it, or move it out of "
                                                   f"{where}/{opencode.PLUGIN_DIR}")
    return Wiring(WIRED, f"opencode debug config: {opencode.SHIM_PATH} under {where}, and "
                         f"nothing else in its realm", "")


def _unprefixed(entry: str) -> str:
    """A `plugin` entry as a path. Measured: opencode answers `file:///…` and keeps the
    un-resolved spelling, so both halves of the comparison are resolved below."""
    return entry[len("file://"):] if entry.startswith("file://") else entry


def _resolved(p) -> str:
    try:
        return str(Path(p).resolve())
    except (OSError, RuntimeError, ValueError):
        return str(p)


# --------------------------------------------------------------------------- #
# The cache — the selector's rows, and never a launch                          #
# --------------------------------------------------------------------------- #


def _fingerprint(p: profiles.Profile) -> dict:
    """What makes this the same profile as the one that was asked about.

    Task 3's `profiletrust.fingerprint` answers exactly this question for the launch
    record, and this becomes a call to it when that is on `main`. The `env` NAMES and their
    values both count: a profile whose `CLAUDE_CONFIG_DIR` moved is a different folder to
    ask about, and reusing an answer across that is the whole failure.
    """
    return {"name": p.name, "kind": p.kind, "command": list(p.command),
            "env": [list(pair) for pair in p.env]}


def _key(p: profiles.Profile, cwd) -> str:
    return hashlib.sha256(
        json.dumps({**_fingerprint(p), "cwd": str(cwd)}, sort_keys=True).encode()).hexdigest()


def _stamp_paths(p: profiles.Profile, cwd) -> list[Path]:
    """The files whose change accompanied every flip of this kind's answer (D2).

    `.claude.json` is deliberately absent: the probe itself writes it, so stamping it would
    invalidate the entry the probe just wrote. The chat DIRECTORY's settings files are here
    and not only the plane root's — that is the file Claude Code reads for that chat, and a
    stamp that missed it would keep saying `wired` after a chat wrote a `false` into it.
    """
    env = environment(p)
    if p.harness == claude_code.NAME:
        folder = claude_code.config_home(env)
        return [folder / "plugins" / "installed_plugins.json", folder / "settings.json",
                Path(cwd) / ".claude" / "settings.json",
                Path(cwd) / ".claude" / "settings.local.json"]
    if p.harness == codex.NAME:
        return [codex.config_path(env)]
    if p.harness == opencode.NAME:
        home = opencode.global_dir(env)
        return [home / opencode.PLUGIN_DIR, home / opencode.SHIM_PATH,
                home / "opencode.json"]
    return []


def _stamp(p: profiles.Profile, cwd) -> dict:
    out = {}
    for path in _stamp_paths(p, cwd):
        try:
            st = path.stat()
            out[str(path)] = [st.st_mtime_ns, st.st_size]
        except OSError:
            # Absent is a state like any other: a shim that appears has to be a miss.
            out[str(path)] = None
    return out


def _read_cache() -> dict:
    try:
        doc = json.loads((Path(config.STATE_DIR) / CACHE).read_text())
    except (OSError, UnicodeDecodeError, ValueError):
        return {}
    return doc if isinstance(doc, dict) else {}


def cached(p: profiles.Profile, *, cwd) -> Wiring | None:
    """A remembered answer whose stamp still matches, or ``None``. **Display only.**

    `charter/frame/selector.py` draws its rows from this and probes on a miss; picking a row
    probes again. A launch never reads it (ruling 21): this file is as writable by a chat as
    `charter.local.toml` is, and a chat can compute the key, write the stamp and date the
    entry ahead — which is why the age test has two bounds.
    """
    entry = _read_cache().get(_key(p, cwd))
    if not isinstance(entry, dict):
        return None
    age = time.time() - (entry.get("checked_at") or 0)
    if not 0 <= age < MAX_AGE:
        return None
    if entry.get("stamp") != _stamp(p, cwd):
        return None
    try:
        return Wiring(entry["state"], entry["detail"], entry["fix"])
    except (KeyError, TypeError):
        return None


def remember(p: profiles.Profile, *, cwd, w: Wiring) -> None:
    """Record *w* for *p* at *cwd*. Best effort: a display cache is never worth a row."""
    doc = _read_cache()
    doc[_key(p, cwd)] = {"state": w.state, "detail": w.detail, "fix": w.fix,
                         "checked_at": time.time(), "stamp": _stamp(p, cwd)}
    path = Path(config.STATE_DIR) / CACHE
    try:
        config.private_mkdir(path.parent)
        config.write_for(path, json.dumps(doc, indent=2) + "\n")
    except OSError:
        pass


# --------------------------------------------------------------------------- #
# Installing                                                                   #
# --------------------------------------------------------------------------- #


def install(p: profiles.Profile, root: Path) -> list[tuple[str, str]]:
    """Wire *p*'s kind under *p*'s own environment. ``(status, label)`` pairs, `Harness.wire`'s
    shape.

    Codex is the one that cannot be finished from here, and that is a fact about Codex
    rather than a gap: charter writes the `shell_environment_policy` line — the one thing
    the plugin cannot write, because it is what tells a Codex shell which harness it is —
    and the plugin install and the hook approval are Codex's own commands, printed with
    `CODEX_HOME=` in front of them by the caller.
    """
    env = environment(p)
    if p.harness == claude_code.NAME:
        status, detail = plugincache.install(root, env=env,
                                             command=profiles.expanded_command(p))
        return [(status, detail)]
    if p.harness == codex.NAME:
        status, detail = codex.install(env=env)
        return [(status, detail)]
    if p.harness == opencode.NAME:
        return opencode.OpenCodeHarness().wire(root, env=env)
    return [("unavailable", f"charter has no wiring for {contain.readable(p.kind)}")]


def listed(read: profiles.ProfileSet | None = None) -> list[profiles.Profile]:
    """Every profile a selector would show, in the order `charter harness list` shows them.

    Declared profiles always; a built-in only when its program is installed, because a row
    about a harness this machine does not have is a row nobody can act on. One reader, so
    `doctor.check_names` and `doctor.check_profile_wiring` cannot disagree about how many
    rows there are — the pin `_FIXED_CHECK_NAMES` already keeps for every other check.
    """
    import shutil

    read = read if read is not None else profiles.current()
    order = list(profiles.builtins())
    rows = [p for p in read.profiles.values()
            if p.source != profiles.BUILTIN
            or shutil.which(profiles.expanded_command(p)[0]) is not None]
    return sorted(rows, key=lambda p: (p.name not in order,
                                       order.index(p.name) if p.name in order else 0,
                                       p.name))
