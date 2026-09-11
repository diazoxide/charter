"""Harness profiles — a named way to launch one harness kind, read from `charter.local.toml`.

Charter launched one program per harness kind, one way: `Harness.binary` is a class
attribute and `launch_argv` returns `[binary, *extra]`. An operator with two Claude Code
accounts — a work `CLAUDE_CONFIG_DIR` and a personal one — or a Codex pinned to an older
release had no way to tell charter so, and a shell alias cannot help, because nothing charter
launches runs through a shell. A profile is that way: a kind, a command, an environment
(`docs/superpowers/specs/2026-09-11-harness-profiles.md`).

**Only in `charter.local.toml`, beside `charter.toml` and never committed.** A profile's
command runs on a click with no harness permission prompt in between, so a command in the
committed file could be changed by a merged PR or by a chat and then run on every machine. A
`[harness.<name>]` table in `charter.toml` is refused with a pointer here. The local file
carries `[harness]` and nothing else: a full overlay would let an ignored file change plane
policy — `[[forge]]` hosts steer the credential guard — with no trace in git.

**Nothing here runs on import (ruling 43).** `config` reads nothing from
`charter.local.toml` and runs none of this code: every command and every hook process
derives config, and `hooks/hooks.json` fires on Bash, Read, Grep, Write, Edit, Task, Skill
and SendMessage — and the deletion sweep charged every line imported there to the whole
suite. :func:`current` reads and validates the file when a surface asks — `charter harness
list` and `charter doctor` today — and runs no subprocess. The one git call — would git
carry this file? — is :func:`ignore_check`, asked by the same two surfaces. Doctor is also
what the SessionStart hook runs, so until the plan's Task 4 gives it a `--preflight` mode, a
session start on a plane that has the file pays that one lock-free `git status` (ruling 40).

**A broken profile is refused alone, by name, with its reason**, and the rest still load.
`[[frame.component]]` refuses its whole arrangement over one bad entry, and that is the right
trade there: a missing panel is easy to miss, while a missing profile is a row that is not in
the list.

**Every piece of profile-derived text is contained before charter shows it** (ruling 35).
The file is one a chat can write, and a carriage return or an ESC in a command could
otherwise redraw a line to show a harmless command while another one runs.
"""

from __future__ import annotations

import os
import re
import shlex
import tomllib
from pathlib import Path
from typing import NamedTuple

from . import contain
from . import root as _root

#: The file profiles live in, beside the plane's `charter.toml`. Named after
#: `.claude/settings.local.json`, which planes already ignore, and so it sorts beside the
#: file it sits next to.
LOCAL_FILE = "charter.local.toml"

#: Words an env NAME may not contain, matched case-insensitively. Anything set on the
#: harness process reaches the shell the model runs — measured on Claude Code and Codex — so
#: charter declines to hold a credential in a profile. A pattern can refuse an innocent name;
#: the docs say so.
SECRET_WORDS = ("KEY", "TOKEN", "SECRET", "PASSWORD")

#: What a refused secret-shaped name is pointed at instead, by registry name: the harness's
#: own login, kept in the config folder a profile's environment already moves.
LOGIN = {"claude-code": "set CLAUDE_CONFIG_DIR and run /login inside Claude Code",
         "codex": "set CODEX_HOME and run codex login",
         "opencode": "set XDG_DATA_HOME and run opencode auth login"}

#: Ruling 5. No dot: a dot in a workspace name broke tmux targets in #695, and a profile's
#: name reaches the same places. Always `fullmatch`: `$` also matches before a final newline,
#: and a quoted TOML key can end in one.
NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_-]*$")

#: `Profile.source` for a registered kind nobody declared.
BUILTIN = "built-in"

#: The one key under `[harness]` that is not a profile: it names the default profile rather
#: than declaring one.
_DEFAULT = "default"

#: The one table a profile's own table may hold: its environment. Any other table inside it
#: is a dotted name written without quotes — `[harness.claude.alt]` — and is refused by that
#: spelling (F4).
_ENV = "env"

#: Everything a profile table may hold. Any other key refuses the profile (review 13): a typo
#: such as `enviroment` would otherwise drop `CLAUDE_CONFIG_DIR` and launch the default
#: account without a word.
_PROFILE_KEYS = frozenset({"kind", "command", _ENV})

#: The prefix of charter's own variables, which charter sets itself (Ruling 14).
_CHARTER_PREFIX = "CHARTER_"


class Profile(NamedTuple):
    name: str                          # "claude-work"; a built-in is named after its kind
    kind: str                          # the word after `charter`: "claude" | "codex" | "opencode"
    harness: str                       # the registry NAME: "claude-code" | "codex" | "opencode"
    command: tuple[str, ...]           # as declared, before `~` expansion
    env: tuple[tuple[str, str], ...]   # as declared, sorted by name
    source: str                        # BUILTIN | LOCAL_FILE


class Refused(NamedTuple):
    name: str      # contained with contain.readable; "" for a whole-file refusal
    source: str    # LOCAL_FILE | "charter.toml"
    reason: str    # one sentence: the rule, then the fix


class ProfileSet(NamedTuple):
    """Every profile a plane has, every declared one refused, and the default.

    :func:`derive`, :func:`current` and :func:`with_ignore_check` all answer in this shape,
    so a caller reads attributes, and a misspelt one is an `AttributeError` at the line that
    misspelt it rather than a `KeyError` wherever the dict was next read.
    """
    profiles: dict[str, Profile]
    refused: tuple[Refused, ...]
    default: str | None
    default_from: str | None
    default_refused: str | None


class IgnoreCheck(NamedTuple):
    """Whether git would carry the local file: the refusal, and the one fix for its state.
    Both are ``""`` when the file may be used."""
    reason: str
    fix: str


# Each refusal says the rule worked and names the fix in the same breath (CONTEXT.md,
# *A refusal is the rule working*), and says only what is true of this charter — no feature a
# later release brings. Every `{}` field is filled with a contained value, and no value of
# `env` is ever repeated back — only its NAME.
PROFILE_IN_COMMITTED = (
    "[harness.{name}] is in charter.toml, which is committed — a profile's command runs on a "
    "click, so charter reads profiles only from charter.local.toml, which stays on this "
    "machine. Move the table there; charter.toml's [harness] keeps `default` alone.")
LOCAL_UNREADABLE = (
    "charter.local.toml could not be read ({why}), so no declared profile was loaded — the "
    "built-in profiles still are. Fix the file and run charter harness list.")
HARNESS_NOT_A_TABLE = (
    "harness in charter.local.toml is not a table, so no declared profile was read — the "
    "file holds a [harness] table with one [harness.<name>] table per profile. Write it that "
    "way.")
LOCAL_SECTION = (
    "[{section}] in charter.local.toml is not read — that file carries [harness] and nothing "
    "else, because an ignored file must not change plane policy with no trace in git. Put "
    "[{section}] in charter.toml.")
NOT_A_TABLE = (
    "[harness] {name} in charter.local.toml is not a table — a profile is [harness.{name}] "
    "with kind, command and optionally env.")
NESTED_TABLE = (
    "[harness.{parent}] holds a table {key}, which charter reads neither way — {key} is not a "
    "key a profile has (kind, command and env), and if a profile named '{dotted}' was meant, a "
    "profile's name is letters, digits, '_' and '-', with no dot, because a dot breaks tmux "
    "targets. Rename the key, or give that profile a name of its own.")
ILLEGAL_NAME = (
    "profile '{name}' is not a name charter accepts — letters, digits, '_' and '-', starting "
    "with a letter or digit, and no dot, because a dot breaks tmux targets. Rename the table.")
RESERVED_NAME = (
    "a profile cannot be named 'default' — `default` is the one key under [harness] that is "
    "not a profile. Rename the table.")
UNKNOWN_KIND = (
    "profile '{name}' has kind {kind}, which is not a harness charter can launch — one of: "
    "{kinds}. Set kind to one of them.")
BAD_COMMAND = (
    "profile '{name}' has no usable command — command is a list of arguments, [\"claude\"], "
    "never a shell string, because no shell runs it. Write it as a list.")
BAD_ENV = (
    "profile '{name}' has an env that is not a table of text values — write "
    "env = {{ NAME = \"value\" }}.")
SECRET_ENV = (
    "profile '{name}' sets {var}, which is named like a credential — charter holds no "
    "credential in a profile, because anything set on the harness reaches the model's own "
    "shell. Log in inside that harness instead: {login}.")
CHARTER_ENV = (
    "profile '{name}' sets {var}, one of charter's own variables — charter sets those itself, "
    "and a profile's value would tell every hook the wrong harness or plane. Remove it.")
CHARTER_COMMAND = (
    "profile '{name}' runs charter itself — a profile names the harness a chat runs, and "
    "charter is not a harness. Give it the harness's own command.")
UNKNOWN_PROFILE_KEY = (
    "profile '{name}' has {key}, which charter does not read — a profile is kind, command and "
    "env. Remove it.")
CLASHING_NAME = (
    "profile '{name}' is named like the command `charter {name}`, and that name belongs to "
    "the command. Rename the table.")
TRACKED_FILE = (
    "git tracks charter.local.toml, so the profiles in it would reach every clone of this "
    "plane — charter refuses them until it is untracked: git rm --cached charter.local.toml, "
    "commit that removal, then charter reinit.")
NOT_IGNORED = (
    "git would commit charter.local.toml, so the profiles in it are refused until it is "
    "ignored — charter reinit adds /charter.local.toml to .gitignore.")
GIT_CANNOT_TELL = (
    "git could not say whether charter.local.toml is ignored ({why}), so the profiles in it "
    "are refused — an unknown is not a pass. Run git status --ignored -- charter.local.toml "
    "in the plane to see what git says.")
DEFAULT_REFUSED = (
    "[harness] default = \"{value}\" names no profile this machine has — one of: {names}.")

# One fix per state (F3). `charter reinit` adds the ignore line, which fixes exactly one of
# the three: it does not untrack a tracked file — until the removal is committed, status
# still prints `D ` beside `!!` — and it does not make git answer.
FIX_NOT_IGNORED = "charter reinit"
FIX_TRACKED = "git rm --cached charter.local.toml, commit that removal, then charter reinit"
FIX_CANNOT_TELL = ("run git status --ignored -- charter.local.toml in the plane by hand; "
                   "git said: {why}")
DEFAULT_FIX = "set [harness] default to one of: {names}, or delete the key"


def _kinds() -> dict:
    """Every launchable kind, by the word typed after `charter`, in registry order — the order
    `instance.launchable_harnesses` reports them in."""
    from .harness import registry

    return {h.cli_name: h for h in registry.all() if h.cli_name}


def builtins(kinds: dict | None = None) -> dict[str, Profile]:
    """One profile per registered harness with a `cli_name`, named after its kind.

    `command` is the harness's own `binary`, which is what `charter <kind>` runs today, so a
    plane that declares nothing sees no change. Asked of the registry rather than listed:
    a kind registered tomorrow is a profile the day it is registered. *kinds* is
    :func:`derive`'s own answer, passed through so one read asks the registry once.
    """
    if kinds is None:
        kinds = _kinds()
    return {word: Profile(word, word, h.name, (h.binary,), (), BUILTIN)
            for word, h in kinds.items()}


def _read_local(root: Path) -> tuple[dict, str]:
    """The local file's top level, and why it could not be read — ``""`` when it could.

    An absent file declares nothing and is not a refusal. Anything else that stops the read
    is, because a file that is there and says nothing is indistinguishable from one nobody
    wrote.
    """
    try:
        return tomllib.loads((Path(root) / LOCAL_FILE).read_bytes().decode("utf-8")), ""
    except FileNotFoundError:
        return {}, ""
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as e:
        return {}, str(e)


def _profile_refusal(name: str, table, kinds: dict) -> str:
    """Why the profile *name* is refused, or ``""``. The first failure wins, in the order the
    rules are written, so one profile gets one sentence."""
    shown = contain.readable(name)
    if not isinstance(table, dict):
        return NOT_A_TABLE.format(name=shown)
    if NAME_RE.fullmatch(name) is None:
        return ILLEGAL_NAME.format(name=shown)
    if name == _DEFAULT:
        return RESERVED_NAME
    kind = table.get("kind", "")
    if not isinstance(kind, str) or kind not in kinds:
        return UNKNOWN_KIND.format(name=shown, kind=contain.readable(kind),
                                   kinds=", ".join(kinds))
    command = table.get("command")
    if not (isinstance(command, list) and command
            and all(isinstance(word, str) and word for word in command)):
        return BAD_COMMAND.format(name=shown)
    env = table.get(_ENV, {})
    if not (isinstance(env, dict) and all(isinstance(v, str) for v in env.values())):
        return BAD_ENV.format(name=shown)
    for var in sorted(env):
        if var.upper().startswith(_CHARTER_PREFIX):
            return CHARTER_ENV.format(name=shown, var=contain.readable(var))
    for var in sorted(env):
        if any(word in var.upper() for word in SECRET_WORDS):
            # `.get`, never an index: this runs inside `config.derive`, where a kind
            # registered without a login sentence must cost a sentence, not every command.
            login = LOGIN.get(kinds[kind].name, "log in inside that harness")
            return SECRET_ENV.format(name=shown, var=contain.readable(var), login=login)
    for key in table:
        if key not in _PROFILE_KEYS:
            return UNKNOWN_PROFILE_KEY.format(name=shown, key=contain.readable(key))
    return ""


def derive(root: Path, cfg: dict) -> ProfileSet:
    """Every profile this plane has, and every declared one refused with its reason.

    Never raises and runs no subprocess. :func:`current` is its one caller outside the
    tests, and nothing calls it on import (ruling 43).

    1. The built-ins, in registry order.
    2. `charter.toml`'s `[harness]`: a table there is refused with a pointer to the local
       file; `default` is the candidate default; any other key is ignored as it always was
       (review 13 — no warning on a plane that did nothing wrong).
    3. The local file: only `[harness]` is read; `default` there wins; every other key is a
       profile, validated by :func:`_profile_refusal`.
       - A table inside a profile's table, other than `env`, is a dotted name written without
         quotes, and is refused by its dotted spelling for the dot, as `[harness."a.b"]` is.
         The parent is a declaration only when it carries keys of its own (F4), so
         `[harness.claude.alt]` alone leaves the built-in `claude` alone.
       - A declared profile named like a built-in replaces it. A REFUSED one takes the name
         with it (ruling 37, extending ruling 19): the operator said how that name runs, and
         the built-in standing in would run the command they replaced — review 13's
         `enviroment` typo launching the default account is that case exactly.
    4. The default, kept only when it names a profile in the result.
    """
    kinds = _kinds()
    found = builtins(kinds)
    refused: list[Refused] = []
    default = default_from = None

    committed = cfg.get("harness") if isinstance(cfg, dict) else None
    if isinstance(committed, dict):
        for key, value in committed.items():
            if isinstance(value, dict):
                shown = contain.readable(key)
                refused.append(Refused(shown, _root.MARKER,
                                       PROFILE_IN_COMMITTED.format(name=shown)))
            elif key == _DEFAULT:
                default, default_from = value, _root.MARKER

    top, why = _read_local(root)
    if why:
        refused.append(Refused("", LOCAL_FILE,
                               LOCAL_UNREADABLE.format(why=contain.readable(why))))
    for key in top:
        if key != "harness":
            shown = contain.readable(key)
            refused.append(Refused(shown, LOCAL_FILE, LOCAL_SECTION.format(section=shown)))
    local = top.get("harness", {})
    if not isinstance(local, dict):
        # Its own sentence (F5): the file parsed, so calling it unreadable would send the
        # reader to fix TOML that is fine — ADR 0009's rule that an answer says its kind.
        refused.append(Refused("", LOCAL_FILE, HARNESS_NOT_A_TABLE))
        local = {}
    for name, table in local.items():
        if name == _DEFAULT and not isinstance(table, dict):
            default, default_from = table, LOCAL_FILE
            continue
        if isinstance(table, dict):
            # A table under a key that is not a profile key. Once parsed it is either a dotted
            # name written without quotes or a typo'd key holding an inline table — the two
            # cannot be told apart — so the refusal names both readings. A table under `kind`,
            # `command` or `env` is that key's value and is judged as one below.
            nested = [key for key, value in table.items()
                      if key not in _PROFILE_KEYS and isinstance(value, dict)]
            for key in nested:
                dotted = contain.readable(f"{name}.{key}")
                refused.append(Refused(dotted, LOCAL_FILE, NESTED_TABLE.format(
                    parent=contain.readable(name), key=contain.readable(key), dotted=dotted)))
            # A parent holding nothing but sub-tables declares nothing, and the built-in of
            # its name stays. One with keys of its own is validated WITH its nested tables:
            # once parsed, `[harness.claude.alt]` and a typo'd `enviroment = { … }` are the
            # same thing — a table under the profile — and review 13 needs the typo to
            # refuse the profile rather than drop `CLAUDE_CONFIG_DIR` in silence.
            if nested and len(nested) == len(table):
                continue
        reason = _profile_refusal(name, table, kinds)
        if reason:
            found.pop(name, None)
            refused.append(Refused(contain.readable(name), LOCAL_FILE, reason))
            continue
        h = kinds[table["kind"]]
        found[name] = Profile(name, h.cli_name, h.name, tuple(table["command"]),
                              tuple(sorted(table.get(_ENV, {}).items())), LOCAL_FILE)

    default_refused = None
    if default_from is not None:
        if isinstance(default, str) and default in found:
            # The result's own name, never the object the file supplied — the rule
            # `instance.harness_of` keeps for a value that ends up on a command line.
            default = found[default].name
        else:
            default, default_refused = None, contain.readable(default)
    return ProfileSet(found, tuple(refused), default, default_from, default_refused)


def _narrowed(derived: ProfileSet, found: dict, refused: list) -> ProfileSet:
    """*derived* with *found* as its profiles and *refused* as its refusals, and a default
    that named a profile no longer found refused by value."""
    default, default_refused = derived.default, derived.default_refused
    if default is not None and default not in found:
        default, default_refused = None, contain.readable(default)
    return derived._replace(profiles=found, refused=tuple(refused), default=default,
                            default_refused=default_refused)


#: The last answer :func:`current` gave, with the key it was computed for: the plane's root
#: and the bytes its two files held. One slot, per process — a second caller asks no parse
#: and no validation again, and an edit changes the key, so the answer is never stale.
_last: list = []


def _bytes(path: Path) -> bytes | tuple | None:
    """*path*'s bytes, as part of a memo key: ``None`` when there is no such file, and a marker
    of its own when there is one that cannot be read — so the two never share an answer."""
    try:
        return path.read_bytes()
    except FileNotFoundError:
        return None
    except OSError as e:
        return ("unreadable", e.errno)


def current() -> ProfileSet:
    """Every profile this plane has, validated — the one place profiles are read (ruling 43).

    `config` reads nothing from `charter.local.toml`, so this reads `charter.toml` and the
    local file itself, as they are now: `charter harness list` and `charter doctor` call it
    today, and the launcher, bare launch, `+` and the selector will from Task 2 on. The
    local `default` is resolved here and nowhere else, which is why `config.HARNESS` — what
    bare `charter` launches — stays exactly `charter.toml`'s.

    On top of :func:`derive`, the two refusals that need charter's own commands to decide:

    - A name `charter <name>` already means (`cli.command_words`), with
      :data:`CLASHING_NAME`. The kind clash in `cli._add_frame_parsers` raises at
      `build_parser()` and takes every command down, which is right for a registry mistake CI
      sees and wrong for one machine's file, so a profile is refused by name instead.
    - A command whose first word is charter itself, with :data:`CHARTER_COMMAND` (Ruling 14).
      `hooks._is_charter` already knows `charter`, `edm` and `python -m charter`, so there is
      no second list.

    Memoized per process, keyed on what the two files hold (:data:`_last`).
    """
    from . import cli, config, hooks, instance

    root = Path(config.ROOT)
    key = (str(root), _bytes(root / LOCAL_FILE), _bytes(root / _root.MARKER))
    if _last and _last[0][0] == key:
        return _last[0][1]
    try:
        cfg = instance.load(root)
    except Exception:
        # A malformed or too-new `charter.toml` is the `charter.toml` doctor row's to name;
        # the local file is still read rather than reporting on nothing.
        cfg = {}
    derived = derive(root, cfg)
    words = cli.command_words()
    found = dict(derived.profiles)
    refused = list(derived.refused)
    for name, p in list(found.items()):
        shown = contain.readable(name)
        if name in words:
            reason = CLASHING_NAME.format(name=shown)
        elif hooks._is_charter(p.command[0], list(p.command[1:])):
            reason = CHARTER_COMMAND.format(name=shown)
        else:
            continue
        del found[name]
        refused.append(Refused(shown, p.source, reason))
    answer = _narrowed(derived, found, refused)
    _last[:] = [(key, answer)]
    return answer


def with_ignore_check(derived: ProfileSet, check: IgnoreCheck) -> ProfileSet:
    """*derived* with every profile the local file declares refused, when *check* says git
    would carry that file (F1).

    `NOT_IGNORED`, `TRACKED_FILE` and `GIT_CANNOT_TELL` each say "the profiles in it are
    refused", so a surface that asked must show them refused — not as ordinary rows with a
    warning under them. A declared replacement of a built-in is refused with the rest and the
    built-in does not stand in (ruling 19). Takes the check rather than running it, so a
    caller that also prints the state's fix asks git once.
    """
    if not check.reason:
        return derived
    found = {name: p for name, p in derived.profiles.items() if p.source != LOCAL_FILE}
    moved = [Refused(contain.readable(name), LOCAL_FILE, check.reason)
             for name, p in derived.profiles.items() if p.source == LOCAL_FILE]
    return _narrowed(derived, found, [*derived.refused, *moved])


def expanded_command(p: Profile) -> list[str]:
    """*p*'s command with a leading `~` expanded in its first word only — the program. No
    shell runs it, so nothing else would; an argument is the harness's to interpret."""
    return [os.path.expanduser(p.command[0]), *p.command[1:]]


def expanded_env(p: Profile) -> dict[str, str]:
    """*p*'s environment with a leading `~` expanded in every value, which is where a config
    folder is named and no shell is there to do it."""
    return {name: os.path.expanduser(value) for name, value in p.env}


def display(p: Profile) -> str:
    """*p* as one line a person reads: `NAME=value` for each variable, then the command.

    Each piece through `contain.readable`, so a control byte is shown escaped and never
    interpreted (ruling 35): the file is one a chat can write.
    """
    pieces = [contain.readable(f"{name}={value}") for name, value in p.env]
    pieces.append(contain.readable(shlex.join(p.command)))
    return " ".join(pieces)


def ignore_check(root: Path) -> IgnoreCheck:
    """Whether git would carry *root*'s local file, as a refusal and the fix for its state.

    A pass — both ``""`` — when the file is absent (it declares nothing), when the plane is
    not a git repository (nothing to commit to), or when git ignores the file and tracks it
    not. Otherwise :data:`TRACKED_FILE`, :data:`NOT_IGNORED` or :data:`GIT_CANNOT_TELL`, each
    with its own fix (F3) — an answer git could not give is not a pass.

    The only function here that runs git, and never on a config read: one
    `util.git_path_state` call, which takes no `index.lock` (ruling 34) and never raises.
    """
    from . import util

    # `os.path.exists` and not `Path.exists`: on 3.11 the latter raises for a directory
    # nobody may search, and a check that raised would cost `doctor` every row after it.
    if not os.path.exists(os.path.join(root, LOCAL_FILE)):
        return IgnoreCheck("", "")
    state, why = util.git_path_state(root, LOCAL_FILE)
    if state == util.TRACKED:
        return IgnoreCheck(TRACKED_FILE, FIX_TRACKED)
    if state == util.COMMITTABLE:
        return IgnoreCheck(NOT_IGNORED, FIX_NOT_IGNORED)
    if state == util.UNKNOWN_GIT:
        shown = contain.readable(why)
        return IgnoreCheck(GIT_CANNOT_TELL.format(why=shown), FIX_CANNOT_TELL.format(why=shown))
    return IgnoreCheck("", "")


def ignored_refusal(root: Path) -> str:
    """:func:`ignore_check`'s refusal alone — ``""`` when the file may be used."""
    return ignore_check(root).reason
