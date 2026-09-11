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

**Reading costs no subprocess.** :func:`derive` runs inside `config.derive`, which every
hook process runs, and `hooks/hooks.json` fires on Bash, Read, Grep, Write, Edit, Task, Skill
and SendMessage. The one git call — would git carry this file? — is :func:`ignored_refusal`,
and only the surfaces a person runs ask it.

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

#: The one key under `[harness]` that is not a profile: the row a selector starts on.
_DEFAULT = "default"

#: Everything a profile table may hold. Any other key refuses the profile (review 13): a typo
#: such as `enviroment` would otherwise drop `CLAUDE_CONFIG_DIR` and launch the default
#: account without a word.
_PROFILE_KEYS = frozenset({"kind", "command", "env"})

#: The prefix of charter's own variables. The launcher sets those at exec, and a profile's
#: value would tell every hook the wrong harness or plane (Ruling 14).
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


# Each refusal says the rule worked and names the fix in the same breath (CONTEXT.md,
# *A refusal is the rule working*). Every `{}` field is filled with a contained value, and no
# value of `env` is ever repeated back — only its NAME.
PROFILE_IN_COMMITTED = (
    "[harness.{name}] is in charter.toml, which is committed — a profile's command runs on a "
    "click, so charter reads profiles only from charter.local.toml, which stays on this "
    "machine. Move the table there; charter.toml's [harness] keeps `default` alone.")
LOCAL_UNREADABLE = (
    "charter.local.toml could not be read ({why}), so no declared profile was loaded — the "
    "built-in profiles still are. Fix the file and run charter harness list.")
LOCAL_SECTION = (
    "[{section}] in charter.local.toml is not read — that file carries [harness] and nothing "
    "else, because an ignored file must not change plane policy with no trace in git. Put "
    "[{section}] in charter.toml.")
NOT_A_TABLE = (
    "[harness] {name} in charter.local.toml is not a table — a profile is [harness.{name}] "
    "with kind, command and optionally env.")
ILLEGAL_NAME = (
    "profile '{name}' is not a name charter accepts — letters, digits, '_' and '-', starting "
    "with a letter or digit. Rename the table.")
RESERVED_NAME = (
    "a profile cannot be named 'default' — [harness] default names which profile the selector "
    "starts on. Rename the table.")
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
    "profile '{name}' sets {var}, one of charter's own variables — the launcher sets those at "
    "exec, and a profile's value would tell every hook the wrong harness or plane. Remove it.")
CHARTER_COMMAND = (
    "profile '{name}' runs charter itself — a new chat on it would open the profile selector "
    "again, forever. Give it the harness's own command.")
UNKNOWN_PROFILE_KEY = (
    "profile '{name}' has {key}, which charter does not read — a profile is kind, command and "
    "env. Remove it.")
CLASHING_NAME = (
    "profile '{name}' is named like the charter command `charter {name}`, which would keep it "
    "from ever launching. Rename the table.")
TRACKED = (
    "git tracks charter.local.toml, so the profiles in it would reach every clone of this "
    "plane — charter refuses them until it is untracked: git rm --cached charter.local.toml, "
    "then charter reinit.")
NOT_IGNORED = (
    "git would commit charter.local.toml, so the profiles in it are refused until it is "
    "ignored — charter reinit adds /charter.local.toml to .gitignore.")
GIT_CANNOT_TELL = (
    "git could not say whether charter.local.toml is ignored ({why}), so the profiles in it "
    "are refused — an unknown is not a pass. Nothing was started. Check it by hand: git "
    "check-ignore -v charter.local.toml")
DEFAULT_REFUSED = (
    "[harness] default = \"{value}\" names no profile this machine has — one of: {names}.")


def builtins() -> dict[str, Profile]:
    """One profile per registered harness with a `cli_name`, named after its kind.

    `command` is the harness's own `binary`, which is what `charter <kind>` runs today, so a
    plane that declares nothing sees no change. Asked of the registry rather than listed:
    a kind registered tomorrow is a profile the day it is registered.
    """
    from .harness import registry

    return {h.cli_name: Profile(h.cli_name, h.cli_name, h.name, (h.binary,), (), BUILTIN)
            for h in registry.all() if h.cli_name}


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
        from . import instance

        return UNKNOWN_KIND.format(name=shown, kind=contain.readable(kind),
                                   kinds=", ".join(instance.launchable_harnesses()))
    command = table.get("command")
    if not (isinstance(command, list) and command
            and all(isinstance(word, str) and word for word in command)):
        return BAD_COMMAND.format(name=shown)
    env = table.get("env", {})
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


def derive(root: Path, cfg: dict) -> dict:
    """Every profile this plane has, and every declared one refused with its reason.

    ``{"profiles": {name: Profile}, "refused": tuple[Refused, ...], "default": str | None,
    "default_from": str | None, "default_refused": str | None}``.

    Never raises and runs no subprocess: `config.derive` calls it for every command and
    every hook, `charter --version` included.

    1. The built-ins, in registry order.
    2. `charter.toml`'s `[harness]`: a table there is refused with a pointer to the local
       file; `default` is the candidate default; any other key is ignored as it always was
       (review 13 — no warning on a plane that did nothing wrong).
    3. The local file: only `[harness]` is read; `default` there wins; every other key is a
       profile, validated by :func:`_profile_refusal`. A declared profile named like a
       built-in replaces it. A REFUSED one takes the name with it: the operator said how that
       name runs, and the built-in standing in would run the command they replaced — review
       13's `enviroment` typo launching the default account, and ruling 19's reason for
       never falling back to a replaced built-in.
    4. The default, kept only when it names a profile in the result.
    """
    from .harness import registry

    kinds = {h.cli_name: h for h in registry.all() if h.cli_name}
    found = builtins()
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
        refused.append(Refused("", LOCAL_FILE,
                               LOCAL_UNREADABLE.format(why="[harness] is not a table")))
        local = {}
    for name, table in local.items():
        if name == _DEFAULT and not isinstance(table, dict):
            default, default_from = table, LOCAL_FILE
            continue
        reason = _profile_refusal(name, table, kinds)
        if reason:
            found.pop(name, None)
            refused.append(Refused(contain.readable(name), LOCAL_FILE, reason))
            continue
        h = kinds[table["kind"]]
        found[name] = Profile(name, h.cli_name, h.name, tuple(table["command"]),
                              tuple(sorted(table.get("env", {}).items())), LOCAL_FILE)

    default_refused = None
    if default_from is not None:
        if isinstance(default, str) and default in found:
            # The result's own name, never the object the file supplied — the rule
            # `instance.harness_of` keeps for a value that ends up on a command line.
            default = found[default].name
        else:
            default, default_refused = None, contain.readable(default)
    return {"profiles": found, "refused": tuple(refused), "default": default,
            "default_from": default_from, "default_refused": default_refused}


def current(read: dict | None = None) -> dict:
    """:func:`derive`'s answer — ``config.PROFILES`` unless *read* is given — with the two
    refusals that need charter's own commands to decide.

    - A name `charter <name>` already means (`cli.command_words`), with
      :data:`CLASHING_NAME`. The kind clash in `cli._add_frame_parsers` raises at
      `build_parser()` and takes every command down, which is right for a registry mistake CI
      sees and wrong for one machine's file, so a profile is refused by name instead.
    - A command whose first word is charter itself, with :data:`CHARTER_COMMAND` (Ruling 14).
      `hooks._is_charter` already knows `charter`, `edm` and `python -m charter`, so there is
      no second list.

    A separate pass because `config.derive` runs before `cli` and `hooks` can be imported,
    and importing either there would be a cycle. Every surface reads this, never
    ``config.PROFILES`` directly. *read* is for a caller whose question is the file as it is
    now rather than as this process derived it — `doctor`'s row.
    """
    from . import cli, config, hooks

    if read is None:
        read = config.PROFILES
    words = cli.command_words()
    found = dict(read["profiles"])
    refused = list(read["refused"])
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
    default, default_refused = read["default"], read["default_refused"]
    if default is not None and default not in found:
        default, default_refused = None, contain.readable(default)
    return {"profiles": found, "refused": tuple(refused), "default": default,
            "default_from": read["default_from"], "default_refused": default_refused}


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


def ignored_refusal(root: Path) -> str:
    """Why git would carry *root*'s local file, or ``""`` when it would not.

    ``""`` when the file is absent (it declares nothing), when the plane is not a git
    repository (nothing to commit to), or when git ignores the file and tracks it not.
    :data:`TRACKED`, :data:`NOT_IGNORED` or :data:`GIT_CANNOT_TELL` otherwise — an answer git
    could not give is not a pass.

    The only function here that runs git, and never on a config read: one
    `util.git_path_state` call, which takes no `index.lock` (ruling 34) and never raises.
    """
    from . import util

    # `os.path.exists` and not `Path.exists`: on 3.11 the latter raises for a directory
    # nobody may search, and a check that raised would cost `doctor` every row after it.
    if not os.path.exists(os.path.join(root, LOCAL_FILE)):
        return ""
    state, why = util.git_path_state(root, LOCAL_FILE)
    if state == util.TRACKED:
        return TRACKED
    if state == util.COMMITTABLE:
        return NOT_IGNORED
    if state == util.UNKNOWN_GIT:
        return GIT_CANNOT_TELL.format(why=contain.readable(why))
    return ""
