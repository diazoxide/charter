"""PreToolUse gate: auto-approve the tools the ACTIVE persona declares.

Wired via a Bash ``PreToolUse`` hook. Reads the tool JSON on stdin; if the
command is a **single, simple** invocation of a tool the active persona's
``tools:`` lists, it emits an ``allow`` decision so it runs without a prompt.
Otherwise it stays silent → the normal permission flow applies.

Two deliberate properties:

- **Never denies.** The worst case is "no auto-approval" → a normal prompt. So a
  bug here can't block work, only fail to smooth it.
- **Conservative parsing.** It smooths only a command whose every character the shell
  hands to the program unchanged (:data:`_LITERAL`) — so no pipe, no ``;``, no ``&&``, no
  redirect, and equally no brace expansion, no ``$'…'``, no glob, no ``~``, because each
  of those is the shell rewriting a word before the program sees it. A wrapper
  (``sudo``/``bash -c``) becomes the "binary" and won't match a tool. The gate can't be
  used to smuggle an unapproved command past the prompt.

  The cost of that is real and worth stating: `git commit -m "fix #12"`, `ls *`,
  `kubectl get -o jsonpath={.items}` and `cat ~/notes` are no longer smoothed. Each is
  one prompt. Admitting any of those characters back is what each earlier round did, and
  each time it was the bypass.

The unit of approval is a **program**, and every argument rides along with it. That
is the feature (an operator writing ``tools: gh`` means `gh`), and it is also where
the whole class of holes lives, so seven rules bound it — each of them "decline to
smooth", never "deny":

- :func:`_shell_literal` — a command containing a character the shell would REWRITE is not
  read at all. Every rule below reads a token, and a token is only the word the program
  gets if the shell had nothing left to do to it (#450).
- :data:`_DANGEROUS` — a declared binary's destructive subcommands still prompt,
  ``charter secret``/``charter vault`` among them.
- :func:`_argv_names_another_program` — an argument that IS a program means the command
  runs two programs, and only one of them was declared. Asked of the filesystem (is this
  path a file this machine would execute?), which is what makes it true of a wrapper
  nobody here has heard of.
- :data:`_INTERPRETERS` — a binary whose *argument* is the real command (``bash``,
  ``python3``, ``xargs``, ``sudo``…) is a declaration of every command, so it is
  never smoothed. Declaring one has to stay a declaration of one thing. This one is a
  LIST of names, it is best-effort, and round four walked past it with `caffeinate -s
  ./evil` — see its own docstring and `docs/hooks.md`, both of which now say so.
- :func:`_touches_control_surface` — whatever the binary, an argument that reaches a
  vault or charter's own state is never smoothed. That is the same rule the Bash leak
  guard applies to `cat`, applied to the argv rather than to a list of programs charter
  happened to think of. "Reaches" is decided against the FILE, not its spelling: each
  token — a token being the word the program is handed, which is what
  :func:`_shell_literal` plus ``shlex`` together establish — is resolved and compared by
  ``(st_dev, st_ino)`` to what :mod:`charter.config` says the state directory is. Round
  one grepped the raw command string for a hardcoded ``.charter/`` and was defeated by a
  quote character, a backslash, a `?`, the bare directory name, and every plane with
  ``$CHARTER_HOME`` set (#443). Round two resolved the token but let the shell rewrite it
  afterwards, and was defeated by `{r..r}` (#450).
- :func:`frozen_tools` — the answer is bounded by what ``tools:`` said when the session
  began. ``persona.md`` is a file the model can write; without this, one approved
  edit is unprompted execution for the rest of the session (#432).
- :func:`_runs_the_declared_program` — the FILE that will execute has to be the one the
  declared name refers to. Every rule above reads arguments; this one reads the word in
  command position, which decides which program reads those arguments at all. A basename
  is a name, and `./gh`, `/tmp/gh`, `bin/kubectl` and `PATH=/tmp gh` each wore a declared
  name over a program the agent had just written, and each was answered ``allow`` (#450,
  round four).

Kept dependency-light (only imports :mod:`charter.persona`, plus a lazy
:mod:`charter.hooks`/:mod:`charter.session` on the paths that need them) so it's
cheap to run on every Bash call.
"""

from __future__ import annotations

import fnmatch
import json
import os
import re
import shlex
import shutil
import string
import sys

_ENV_ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")

#: The ASCII characters a POSIX shell passes to the program **unchanged**, wherever in the
#: command they appear. Everything outside this set makes :func:`_tokens` decline.
#:
#: An allowlist, and that is the whole point. Every previous version of this rule was a
#: list of the characters that were known to be dangerous — `_UNSAFE = (";", "|", "&",
#: "`", "\n", ">", "<")` plus a substring test for `$(` — and each round of review found
#: one more character that had never been on it. Brace expansion (`.charte{r..r}`), ANSI-C
#: quoting (`$'\x2echarter'`), and pathname expansion (`charter secre*`, when a file named
#: `secret` sits in the cwd) each turned one word into a DIFFERENT word before the program
#: saw it, while every rule downstream read the word that was typed (#450).
#:
#: Listing the safe characters instead inverts who has to be exhaustive: a spelling nobody
#: here thought of is refused by default rather than admitted by default. Three kinds of
#: character are absent that a reader may expect to find:
#:
#: * `~` — tilde expansion is not something ``shlex`` performs, and bash really does hand
#:   `/Users/you` to the program where `~` was typed. `ls ~` is one prompt.
#: * `*`, `?`, `[` — pathname expansion rewrites a word using files that exist at run
#:   time, which is not a property this process can evaluate: `charter secre*` is
#:   `charter secret` the moment something creates `./secret`, and an agent can create it
#:   with a plain Write. `ls *` is one prompt.
#: * `$`, `` ` `` — substitution, in every spelling including `$'…'` and `$( )`.
#:
#: Non-ASCII is admitted, but by the same property rather than as an exception — see
#: :func:`_passes_through`. A path or a message in Cyrillic or Chinese is literal to bash
#: and to ``shlex`` alike, so refusing it would be a cost with nothing bought.
_LITERAL = frozenset(string.ascii_letters + string.digits + "_-./:,=+@%")

#: Quoting and separation — the two things ``shlex.split(posix=True)`` DOES model, and
#: models the same way bash does. Checked by running a corpus drawn from this alphabet
#: through a real bash and comparing argv, rather than asserted
#: (`tests/test_toolgate.py::TestTheParseMatchesARealShell`); the corpus is a sample, and
#: a 16 000-case random differential over the same alphabet found no divergence either.
#:
#: `\r` and `\n` are deliberately not here: ``shlex`` treats both as whitespace, while bash
#: treats `\r` as an ordinary word character and `\n` as a command separator. Both are
#: divergences, and `kubectl\rget pods` is a command that does not exist.
_QUOTING = frozenset("'\"" + chr(92))
_SEPARATORS = frozenset(" \t")

#: Even when a persona declares a tool, these subcommands are too destructive to
#: auto-approve — they fall back to a normal prompt. A subcommand matches if it appears as
#: a WORD anywhere in the arguments (:func:`_words`), not as a whole token: `git -c
#: alias.z=clean z -xfd` defines the alias and runs it in one command, and a rule reading
#: whole bare tokens saw `-c`, `alias.z=clean` and `z`, matched none of them, and answered
#: `allow` while git deleted every untracked file — the state directory included (#450,
#: round four). Read-only verbs (kubectl get/describe/logs/…, glab … list/view) are
#: unaffected; `kubectl describe pod my-run-1` is not `kubectl run`, because a word ends at
#: a character a word cannot contain, not in the middle of one.
_DANGEROUS = {
    "kubectl": {
        "delete", "drain", "cordon", "uncordon", "taint", "evict",
        "replace", "exec", "attach", "cp", "port-forward", "proxy", "run",
    },
    "glab": {"delete", "remove"},
    # `git clean -xfd` deletes every untracked file — which, in a control plane, is the
    # gitignored `.charter/` state directory: the session ceiling below, the active-persona
    # pointer, the vaults. Deleting it and re-snapshotting was an unprompted path from an
    # edited `tools:` line to every tool it now names (#443). It is also destructive on its
    # own terms — untracked work is unrecoverable — which is what `_DANGEROUS` is for.
    "git": {"clean"},
    # AgentMail: reads (list/get/search) auto-approve; sending mail is an outward
    # action and deletes are destructive — those keep prompting.
    "agentmail": {"send", "reply", "forward", "delete", "remove"},
    # charter itself (#424). `charter secret exec|cp|get --reveal` is the same verb
    # `kubectl exec` already carves out, doing something strictly more sensitive: it
    # puts a credential in a process. `vault` writes the registry those paths read.
    # Everything else charter does — `persona show`, `workspace save`, `trace` — is
    # untouched, so a plane that declares `tools: charter` keeps what it declared it
    # for. `edm` is charter's pre-rename name, kept for the reason `hooks._CHARTER_PROGS`
    # keeps it: one extra string against a silent gap on a machine still running it.
    #
    # `change` joins them because `charter change land` merges code into a repository,
    # and a persona declaring `tools: charter` must not have that auto-approved. **The
    # grain is coarse on purpose and the asymmetry is what decides it**: this also
    # declines to auto-approve `charter change show`, which costs one permission prompt
    # on a read-only command, while the other direction auto-approves a merge. This gate
    # never denies — the worst it can do is fall back to the prompt the operator would
    # have had anyway — so the cost of over-refusing is bounded at that prompt, and the
    # cost of under-refusing is not bounded at all. A finer rule keyed on the verb would
    # have to read `land` out of an argv the shell has already had its way with, which is
    # the reading `_is_dangerous`' own docstring records going wrong for `git clean`.
    "charter": {"secret", "vault", "change"},
    "edm": {"secret", "vault", "change"},
}

#: Binaries whose ARGUMENT is a program charter cannot resolve to a file. Declaring one
#: of these declares every command there is — `tools: python3` reads as "this persona
#: writes Python", not as "this persona may read its own vault and POST it anywhere" — so
#: the gate never smooths them (#439). It still never denies: the operator who genuinely
#: wants this gets a normal prompt, which is the control that was being removed.
#:
#: **This list is not the rule, and reading it as one is what round four found.** It is a
#: literal list of names, and a name absent from it walked straight past: `caffeinate -s
#: ./evil` and `arch -arm64 ./evil` were each an affirmative `allow` through the real hook,
#: and `./evil` — a program the agent wrote seconds earlier — really ran. So did
#: `flock /tmp/l ./evil`, `taskset 1 ./evil`, `chrt 0 ./evil`, `nsenter ./evil`,
#: `runuser -u u -- ./evil`, `systemd-run ./evil` and `proot ./evil`. `nice`, `ionice` and
#: `timeout` were on the list while their exact peers `chrt`, `taskset` and `flock` were
#: not, which is what a list rather than a property always looks like from the outside.
#:
#: What answers that class now is :func:`_argv_names_another_program`, which asks the
#: FILESYSTEM — is any argument a file this machine would execute? — and so covers the
#: wrapper nobody here has heard of. This list survives for the residue that question
#: cannot reach, and only that: a program named as **text rather than as a file**.
#: `bash -c hostname`, `awk 'BEGIN{system("id")}'` and `python3 -c …` name no file at all;
#: `npx`/`uvx` name one that does not exist yet and will be fetched. There is no property
#: in argv that separates those strings from an ordinary argument, so they are a list, they
#: are best-effort, and `docs/hooks.md` says so rather than implying a closed category.
#:
#: Wrappers whose argument is a *file* are still listed — `env`, `xargs`, `sudo` — because
#: a second rule costs nothing and the day `_argv_names_another_program` is wrong about one
#: of them is the day the overlap is what holds.
_INTERPRETERS = frozenset("""
    sh bash zsh fish dash ksh mksh csh tcsh ash busybox
    python python2 python3 pypy pypy3 ipython node nodejs deno bun ts-node tsx
    perl ruby irb php lua luajit tclsh osascript groovy scala jshell java
    Rscript R julia elixir iex erl escript
    awk gawk mawk nawk sed expect
    env xargs nohup setsid sudo doas su nice ionice stdbuf script chroot unshare
    time timeout watch command builtin exec eval parallel find make
    npx pnpx bunx uvx uv pipx pip pip3 poetry rye deno_run
""".split())

#: The same names carrying a version suffix — `python3.12`, `php8.2`, `node20`. A
#: guard that knows `python3` and not `python3.12` is the demo, not the class.
_VERSIONED = re.compile(
    r"^(?:python|pypy|node|deno|bun|perl|ruby|php|lua|bash|sh|zsh|ksh|tclsh|"
    r"julia|scala|pip|uv)[0-9]+(?:[._-][0-9]+)*$")

#: charter's own control surface, by NAME — the cwd-independent half of the check below.
#: Two kinds of file, one rule: a *vault* (the leak guard's
#: :data:`charter.hooks._VAULT_PATH_RE`, imported rather than re-spelled), and the files
#: that decide what this gate itself will answer — the state directory (the
#: active-persona pointer, session pointers, the tool ceiling below) and the persona
#: definitions that carry `tools:`.
#:
#: `.edm` is charter's pre-rename state directory, kept for the reason
#: `hooks._CHARTER_PROGS` keeps the old binary name. Both spellings match the DIRECTORY
#: ITSELF as well as a path inside it: `tar -cf /tmp/o.tar .charter` archives every vault
#: while naming no file, and a pattern that required a trailing slash never saw it (#443).
#:
#: A name is only ever the second answer here. `_resolves_into` below asks the
#: filesystem, which is what covers `$CHARTER_HOME`, a legacy `.edm/` plane, a symlink
#: and a case-folded spelling.
#:
#: The bare-directory alternative overlaps `hooks._VAULT_PATH_RE`, deliberately: they are
#: read together, so no test can tell which one answered, and that is the point. That
#: pattern states what the LEAK guard calls a vault and may be narrowed for the leak
#: guard's own reasons; this one states what the TOOL GATE calls charter's state. The day
#: those two diverge is the day the overlap is the only thing holding.
_SELF_PATH_RE = re.compile(r"\.(?:charter|edm)(?:/|$)|persona\.md|personas/\.default")

def _norm(text: str) -> str:
    """Fold the path spellings that mean the same file: `//`, `/./`, and case.

    `.charter//vaults/x.json` and `.Charter/vaults/x.json` name the same file on the
    filesystems charter runs on, and a guard that only knows the canonical spelling is
    one substitution away from silence.

    It used to rewrite `\\` to `/` as well, on the theory that a backslash was a Windows
    separator. That did not fold a spelling, it INVENTED one: `.chart\\er/vaults/x.json`
    — which a POSIX shell hands to the program as `.charter/vaults/x.json` — came out of
    here as `.chart/er/vaults/x.json` and matched nothing (#443). Quoting and escaping are
    now undone by :func:`_tokens`, which does that with ``shlex`` and refuses any command
    carrying a character ``shlex`` and the shell would read differently, and this function
    folds only spellings the *filesystem* treats as equal.

    Which is why the backslash is not handled here at all any more, rather than handled
    correctly: after tokenising, the only backslash left in a token is one the shell
    QUOTED, and `.chart\\er/x` in single quotes names a file whose name really does
    contain a backslash — a different file, and not charter's.
    """
    t = text
    for _ in range(4):                      # bounded: each pass strictly shortens
        n = re.sub(r"/(?:\./)+", "/", re.sub(r"/{2,}", "/", t))
        if n == t:
            break
        t = n
    return t.lower()


def _control_roots() -> list[str]:
    """The paths a smoothed command may not reach, asked of :mod:`charter.config`.

    Derived, never re-spelled. ``config.STATE_DIR`` is ``$CHARTER_HOME`` verbatim when the
    operator set one, and the legacy ``.edm/`` directory on a plane whose migration failed
    (`config._migrate_state_dir`) — so a check hardcoded to the literal `.charter/`
    matched *nothing at all* on either plane and silently smoothed every vault path
    (#443). `hooks._state_write_reason` already answers this question this way; two guards
    answering one question two ways is how the gap gets in.
    """
    from . import config
    out = []
    for p in (config.STATE_DIR, config.VAULTS_DIR):
        try:
            out.append(os.path.realpath(str(p)))
        except (OSError, ValueError):
            continue
    out.extend(_registered_vault_files())
    return out


def _control_names() -> list[str]:
    """Every absolute spelling of a control root this process can name, folded by `_norm`.

    Both the spelling :mod:`charter.config` holds and the ``realpath`` of it, because on
    macOS those differ for every plane under `/tmp`: `/var` is a symlink to `/private/var`,
    so the resolved root is `/private/var/…` while a command naturally spells `/var/…`.
    A substring test that knew only the resolved spelling matched neither the command an
    operator writes nor the one an attacker writes.

    Used ONLY by the substring reading in :func:`_touches_control_surface`, which can only
    add refusals. Identity — `(st_dev, st_ino)` — remains the answer that decides whether
    two different names are the same file; this list is names, and names are the second
    answer here, never the first.
    """
    from . import config
    out = []
    for p in (str(config.STATE_DIR), str(config.VAULTS_DIR),
              *_registered_vault_files(resolve=False)):
        for spelling in (p, _real(p)):
            n = _norm(spelling)
            if n.strip("/"):
                out.append(n)
    return sorted(set(out))


def _real(p: str) -> str:
    try:
        return os.path.realpath(p)
    except (OSError, ValueError):
        return p


def _registered_vault_files(resolve: bool = True) -> list[str]:
    """Every vault file the registry names, including the ones stored OUTSIDE the plane.

    `vaults.json` can point a vault at any path on the machine, and
    `commands_secrets` actively recommends that for a plain-file vault git would otherwise
    commit (`base.vault_file_path`). A check that knew only `.charter/vaults/` was
    therefore true of the default layout and false of the layout charter itself
    recommends.

    Asked of `secrets.registry` and resolved by `base.vault_file_path` — the same two
    functions every other reader uses, because two answers to "where is this vault" is how
    one of them quietly keeps resolving the old way.

    The cost is one small JSON read, and only on the paths that would otherwise be
    smoothed: `decide` has already established an active persona and a declared binary
    before anything here runs, so an ordinary Bash call that this gate has no opinion about
    never reaches it. Any failure reads as "no extra roots" — the state directory above is
    unaffected, and this function can only ever ADD refusals.

    *resolve* picks which spelling comes back — the ``realpath`` for the identity checks,
    the registry's own for :func:`_control_names`, which needs the name a command would
    plausibly be written with.
    """
    try:
        from .secrets import registry as _registry
        from .secrets.base import vault_file_path
        entries = (_registry.load_registry() or {}).get("vaults") or {}
    except Exception:
        return []
    out = []
    for entry in entries.values():
        f = (entry.get("config") or {}).get("file") if isinstance(entry, dict) else None
        if not isinstance(f, str) or not f:
            continue
        try:
            p = str(vault_file_path(f))
            out.append(os.path.realpath(p) if resolve else p)
        except Exception:
            continue
    return out


def _ids(path: str):
    """``(st_dev, st_ino)`` for *path*, or ``None`` when it does not exist.

    The identity of the object, not its name: on a case-insensitive filesystem `.Charter`
    stats to the same inode as `.charter`, and a symlink planted into the plane stats to
    what it points at. That is the comparison a name-based guard keeps losing.
    """
    try:
        st = os.stat(path)
    except (OSError, ValueError):
        return None
    return (st.st_dev, st.st_ino)


def _component_match(root_part: str, cand_part: str) -> bool:
    """One path component of a control root against one component of a candidate path.

    Case-insensitively, because `.Charter` and `.charter` are the same directory on the
    filesystems charter runs on and this branch is reached exactly when the path does not
    exist, so `os.stat` cannot say so.

    `fnmatch` rather than `==` because *cand_part* may still be a pattern where it came
    from somewhere other than a command line. It no longer can come from one:
    :func:`_tokens` refuses `*`, `?` and `[` outright.

    This used to carry the shell's leading-dot rule — `*` never expands onto a dotfile —
    so that `ls *` would not read as naming `.charter` and would keep being smoothed. That
    exemption is gone with the character it protected. It was also the wrong trade: `*`
    does not only rewrite paths, and `charter secre*` is `charter secret` the moment
    something creates `./secret` (#450). Dropping the rule can only widen what matches
    here, which means more prompts and never fewer.
    """
    return fnmatch.fnmatchcase(root_part.lower(), cand_part.lower())


def _chain_ids(roots: list[str]) -> set:
    """Identities of every control root AND of every directory that CONTAINS one.

    A command that names a containing directory reaches everything inside it:
    `tar -cf /tmp/o.tar .` in the plane root archives every vault exactly as
    `tar -cf /tmp/o.tar .charter` does, and the second spelling being the one the guard
    knew about is how this class of hole keeps being spelled around (#443). Both are
    "decline to smooth", which costs one prompt for `ls ~` and closes the archive.
    """
    out = set()
    for root in roots:
        cur, hops = root, 0
        while hops < 64:
            i = _ids(cur)
            if i is not None:
                out.add(i)
            parent = os.path.dirname(cur)
            if parent == cur:
                break
            cur, hops = parent, hops + 1
    return out


def _resolves_into(cand: str, base: str, roots: list[str],
                   root_ids: set, chain_ids: set) -> bool:
    """True when *cand*, as the shell would hand it to the program, reaches a control root
    — by BEING one, by sitting under one, or by containing one.

    Two answers, in order of strength:

    1. **Identity.** Resolve against *base*, then compare ``(st_dev, st_ino)``, never
       text: the path itself against *chain_ids* (is it a root, or a directory holding
       one?) and each of its ancestors against *root_ids* (is it inside one?). That is one
       answer for `.charter/vaults/x.json`, `./.charter/…`, `~/plane/.charter/…`,
       `.Charter/…` on a case-insensitive filesystem, a symlink pointing into the state
       directory, and the bare directory `.charter` itself.
    2. **Pattern.** A path that does not exist has no identity — a write target, most
       often. So the resolved spelling is compared to each root component by component,
       case-insensitively, and `fnmatch` honours a glob metacharacter if one is present.

       A pattern can no longer arrive from the COMMAND: :func:`_tokens` refuses `*`, `?`
       and `[` outright, because whether `charter secre*` means `charter secret` depends
       on what files exist when bash runs, which is not a question this process can
       answer. It used to be answered here, for paths, and that left the same character
       free to rewrite a subcommand (#450). What can still carry one is a ROOT: the vault
       registry may name a path this process cannot resolve, and matching it as a pattern
       costs nothing and can only add refusals.

    ``expanduser``/``expandvars`` are applied to *cand* for the same reason: `~` and `$`
    are refused by :func:`_tokens`, so on the gate's own path they are no-ops, and they
    stay because this function is also the honest answer for a caller that has a path from
    somewhere other than a command line.
    """
    try:
        c = os.path.expanduser(cand)
        if "$" in c:
            c = os.path.expandvars(c)
        p = os.path.realpath(os.path.join(base, c))
    except (OSError, ValueError):
        return False
    cur, hops = p, 0
    while hops < 64:
        i = _ids(cur)
        if i is not None and (i in chain_ids if cur == p else i in root_ids):
            return True
        parent = os.path.dirname(cur)
        if parent == cur:
            break
        cur, hops = parent, hops + 1
    cparts = p.split(os.sep)
    for root in roots:
        rparts = root.split(os.sep)
        if len(cparts) < len(rparts):
            continue
        if all(_component_match(r, c) for c, r in zip(cparts, rparts)):
            return True
    return False


def _path_candidates(token: str):
    """The path-shaped readings of one argv token.

    A token is not always a bare path: `curl --data-binary @<path>` prefixes it, and
    `VAULT=<path> gh api /x` (or `--output=<path>`) carries it after an `=`. Both prefixes
    compose — `--data-binary=@<path>` is one token wearing both — so this strips to a
    fixed point rather than one layer. It terminates because every step is strictly
    shorter.
    """
    out, frontier = [token], [token]
    while frontier:
        t = frontier.pop()
        for nxt in (t[1:] if t.startswith("@") else "",
                    t.split("=", 1)[1] if "=" in t else ""):
            if nxt and nxt not in out:
                out.append(nxt)
                frontier.append(nxt)
    return out


def _touches_control_surface(tokens: list[str], cwd: str | None = None) -> bool:
    """True when any argv token names a vault file or one of charter's own files.

    The binary is not consulted on purpose. `_leak_reason` asks "is this program a
    reader?", which is answerable for `cat` and hopeless for `python3 -c …` or
    `curl --data-binary @…` — and this gate's job is narrower than denying: it only
    has to decline to *remove the prompt* from a command reaching for a credential.

    Given TOKENS, not the raw command string. Grepping the raw string was the whole
    defect: `--data-binary @".charter"/vaults/devops.json` is the same file with two
    quote characters in it, and a substring search for `.charter/` does not see it
    (#443). :func:`_tokens` undoes quoting first, and refuses outright any command whose
    remaining characters the shell would have rewritten, so a token that arrives here is
    the word the program will be handed.

    Three readings of each token, and a token needs to fail all three to be smoothed:

    1. Each path-shaped reading (:func:`_path_candidates`) against the two NAME patterns.
    2. Each path-shaped reading resolved and compared by identity (:func:`_resolves_into`).
    3. The WHOLE token against each root as a substring. Readings 1 and 2 both start from
       `_path_candidates`, which knows two prefixes — a leading `@` and everything after an
       `=` — and `curl -d@<abs path to a vault>` wears a third. On the default plane the
       name patterns still caught it (they are substring searches for `.charter`), but on a
       `$CHARTER_HOME` plane the state directory is not called that, and the command was
       smoothed while curl POSTed the vault (measured, #450). This reading asks nothing
       about where in the token the path starts, so a fourth prefix needs no fourth rule.
       It can only ever ADD refusals: a token that spells out an absolute control root is
       reaching for it whatever surrounds it.

    Reading 3 is bounded to ABSOLUTE roots, which is what makes it cheap and exact. A
    RELATIVE path hidden behind an unknown prefix on a plane whose state directory is
    named neither `.charter` nor `.edm` is what is left, stated rather than smoothed over:
    `_path_candidates` would have to grow that prefix for reading 2 to see it.

    Scanned over the WHOLE argv including leading `VAR=value` assignments, so a
    `VAULT=.charter/vaults/x.json` prefix cannot carry the path past it.
    """
    from .hooks import _VAULT_PATH_RE       # one regex for one question, not two
    roots = _control_roots()
    root_ids = {i for i in (_ids(r) for r in roots) if i is not None}
    chain = _chain_ids(roots)
    base = cwd or os.getcwd()
    named = _control_names()
    for tok in tokens:
        low = _norm(tok)
        if any(r in low for r in named):
            return True
        for cand in _path_candidates(tok):
            text = _norm(cand)
            if _VAULT_PATH_RE.search(text) or _SELF_PATH_RE.search(text):
                return True
            if _resolves_into(cand, base, roots, root_ids, chain):
                return True
    return False


def _is_interpreter(binary: str) -> bool:
    return binary in _INTERPRETERS or bool(_VERSIONED.match(binary))


def _argv_names_another_program(tokens: list[str], cwd: str | None = None) -> bool:
    """True when any argument names a FILE this machine would execute.

    The wrapper class, asked as a property of the command instead of as a property of the
    binary's name. `caffeinate -s ./evil` runs `./evil`; so do `arch -arm64 ./evil`,
    `flock /tmp/l ./evil`, `taskset 1 ./evil`, `nsenter ./evil`, `runuser -u u -- ./evil`
    and every wrapper written after this sentence. :data:`_INTERPRETERS` answered none of
    them, because it is a list of names and they were not on it — the same failure as round
    one (a name), round two (a character class) and round three (a longer list). What all
    of them have in common is not the binary. It is that an ARGUMENT is a program, and
    whether a path is a program is a question the filesystem answers.

    So: for each argument, each path-shaped reading of it (:func:`_path_candidates`, so a
    `@` prefix and a `--flag=` prefix are seen) is resolved against *cwd* and asked two
    things — is it a regular file, and would the OS execute it. Both come from one `stat`
    of the object the shell will hand over, not from its spelling, which is why `./evil`,
    its absolute spelling, `./logs/../evil` and a symlink to it are one answer.

    **A bare name is deliberately not looked up on PATH**, and this is a real boundary, not
    an oversight. `caffeinate evil` reaches `evil` only if it sits on `PATH`, so covering it
    means treating every argument that `shutil.which` resolves as a program — and measured
    on an ordinary machine that is `git log`, `gh pr list`, `docker ps`, `kubectl top`,
    `kubectl apply` and `kubectl cp`, because `log`, `pr`, `ps`, `top`, `apply` and `cp` are
    all binaries. That is not one prompt; it is the end of the allowance. The residue it leaves
    has exactly the precondition :func:`_runs_the_declared_program` already discloses for
    the command word: an agent that can write into a directory already on `PATH`. Closing
    it there and leaving it here would be an inconsistency; charter closes neither and says
    so in `docs/hooks.md`.

    Two smaller residues, stated for the same reason. A program named as text rather than
    as a file (`bash -c hostname`, `ssh host rm`) has no file here to stat — that is what
    :data:`_INTERPRETERS` is a best-effort list for. And a file that is not executable when
    this runs but is when bash runs is not something one process can see; making it
    executable is a `chmod` of its own, which is a command of its own.

    Directories are excluded on purpose: every directory carries an execute bit, and
    `ls -la workspaces` names one. `os.path.isfile` is what separates them.

    Like every rule here this can only ever ADD refusals — a refusal is one prompt, never a
    denial — so the cost of it being over-eager (`git add ./release.sh`) is a prompt, and
    the cost of it being absent was measured: `./evil` ran.
    """
    i = 0
    while i < len(tokens) and _ENV_ASSIGN.match(tokens[i]):
        i += 1
    base = cwd or os.getcwd()
    for tok in tokens[:i] + tokens[i + 1:]:
        for cand in _path_candidates(tok):
            if os.sep not in cand:
                continue        # a bare name is resolved by PATH; see above
            try:
                p = os.path.realpath(os.path.join(base, os.path.expanduser(cand)))
                if os.path.isfile(p) and os.access(p, os.X_OK):
                    return True
            except (OSError, ValueError):
                continue
    return False


def _passes_through(ch: str) -> bool:
    """True when the shell hands *ch* to the program unchanged.

    ASCII is answered by :data:`_LITERAL`, :data:`_QUOTING` and :data:`_SEPARATORS`.

    Above ASCII the question is asked of BYTES, not of the character, because that is what
    bash parses. "Every shell metacharacter is ASCII, so no non-ASCII character can be one"
    is true in UTF-8 and false in general: in GBK, Big5 and Shift-JIS a multi-byte
    character's *trail* byte lands in the ASCII range, and `0x7C` there is a `|` to bash
    however Python spells the character. So the character is encoded the way this process
    encodes argv, and admitted only if no byte of it could be read as ASCII at all.

    In a UTF-8 locale — every machine charter is known to run on — that admits exactly the
    same set as the simpler claim, at the cost of one `encode` per non-ASCII character.
    Anywhere else it declines, which is a prompt.
    """
    if ch in _LITERAL or ch in _QUOTING or ch in _SEPARATORS:
        return True
    if ord(ch) < 128:
        return False
    try:
        return all(b >= 0x80 for b in os.fsencode(ch))
    except (UnicodeError, ValueError):
        return False


def _shell_literal(command: str) -> bool:
    """True when every character of *command* is one the shell hands over unchanged.

    See :data:`_LITERAL`. The test is positional-blind on purpose: a `{` inside single
    quotes really is harmless, but exempting it would mean deciding here which regions are
    quoted — a SECOND model of the shell, in a module whose entire defect history is
    models of the shell that were subtly wrong. So `git commit -m "fix #12"` is not
    smoothed either. That costs one prompt and removes a whole class of disagreement.
    """
    return all(_passes_through(ch) for ch in command)


def _tokens(command: str) -> list[str] | None:
    """The argv the shell would build, or ``None`` when this is not a simple command.

    ``shlex.split(posix=True)`` — the same splitter `hooks._segment_argv` uses — models
    exactly two of the shell's steps: quote removal and word splitting. It does NOT
    perform brace expansion, tilde expansion, parameter or command substitution, ANSI-C
    quoting, or pathname expansion, all of which the shell performs FIRST and any of which
    can replace a word with a different word. So the returned list is the argv the program
    receives only when the command contains none of the characters that trigger them —
    which is what :func:`_shell_literal` establishes, before ``shlex`` is called at all.

    That order matters. Round two called `shlex.split` and described the result as "what
    the program will actually receive"; `cat .charte{r..r}/vaults/devops.json` was handed
    to every rule below as the string `.charte{r..r}/…`, matched nothing, and was
    auto-approved while bash opened the vault (#450). ``shlex`` was not wrong; the sentence
    about it was.

    What the two together do guarantee, and it is now a checkable claim rather than a
    promise: over the alphabet :data:`_LITERAL` admits, ``shlex.split`` returns the same
    argv bash does. `TestTheParseMatchesARealShell` asserts that against a real bash rather
    than against this module's opinion of one.

    Unbalanced quotes and a trailing backslash raise, and the answer to a command this
    function cannot parse is the same as for every other doubt on this path: decline to
    smooth it, take the prompt.
    """
    if not command or not _shell_literal(command):
        return None
    try:
        toks = shlex.split(command, posix=True)
    except ValueError:
        return None
    return toks or None


def _parse(command: str):
    """(binary, arg_tokens, all_tokens) for a simple command, or a triple of None.

    Leading ``VAR=value`` assignments are stepped over to FIND the command word, and that
    is all this step does with them: they stay in *all_tokens*, so
    :func:`_touches_control_surface` still scans them, and
    :func:`_runs_the_declared_program` refuses the command outright for carrying one. Doing
    that refusal here instead would make this function the only guard a
    `VAR=… <cmd>` command ever reaches, which is how a new early refusal hides an older
    rule's bug.
    """
    tokens = _tokens(command)
    if tokens is None:
        return None, None, None
    i = 0
    while i < len(tokens) and _ENV_ASSIGN.match(tokens[i]):
        i += 1
    if i >= len(tokens):
        return None, None, None
    return os.path.basename(tokens[i]), tokens[i + 1:], tokens


#: One WORD of an argv token: a maximal run of the characters a subcommand can be spelled
#: with. Everything else — `=`, `.`, `/`, `:`, `,`, `@`, whitespace — ends a word, because
#: those are what a program uses to pack more than one word into one token
#: (`alias.z=clean`, `--use-compress-program=curl`, `svc/x`).
#:
#: A `-` does NOT end a word, so `port-forward` stays one word and `my-run-1` never reads
#: as `run`. The leading `-` of a flag is dropped by the "starts with an alphanumeric"
#: anchor, so `--delete` reads as `delete` while `--delete-source-branch` does not.
_WORD_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9_-]*")


def _words(token: str) -> list[str]:
    return _WORD_RE.findall(token)


def _is_dangerous(binary: str, args: list[str]) -> bool:
    """True when a destructive verb of *binary* appears as a word anywhere in *args*.

    Reading words rather than whole tokens can only ever ADD refusals, and a refusal here
    costs one prompt. It buys the spellings that pack a verb into a larger token —
    `git -c alias.z=clean z`, which really does run `git clean` — at the price of
    `git commit -m "clean up the tests"` taking a prompt too. That trade is the same one
    the rest of this module makes, and it is stated in `docs/hooks.md` rather than
    discovered.

    What it does not reach, said plainly because the sentence above would otherwise imply
    it: a verb that is not in `argv` at all. `git config alias.z clean` followed by
    `git z -xfd` runs `git clean -xfd` while neither command spells `clean` where this can
    see it. That is the same boundary `curl -K req.conf` sits on — the meaning of the argv
    lives in a file — and it is disclosed in `docs/hooks.md`, not closed here.
    """
    bad = _DANGEROUS.get(binary)
    if not bad:
        return False
    return any(w in bad for tok in args for w in _words(tok))


# --------------------------------------------------------------------------- #
# The session ceiling: what `tools:` said before this session could edit it     #
# --------------------------------------------------------------------------- #
def _ceiling_file(sid: str):
    from . import config
    return config.SESSIONS_DIR / f"{sid}.tools"


def _marker_file(sid: str):
    """The "a ceiling was taken for this session" marker — a different file on purpose.

    "What the ceiling says" and "that a ceiling exists" are two facts, and storing them in
    one file made the second deletable with the first: remove `<sid>.tools` and
    :func:`frozen_tools` read a session it had never seen, re-snapshotted the working tree,
    and granted the mid-session `tools:` edit in full (#443).

    Keyed on the ceiling specifically rather than on "any `<sid>.*` record charter holds".
    A per-session persona pointer is written by `charter persona use`, which under a
    harness with no SessionStart hook happens BEFORE the first gated Bash call — so the
    broader test would have read every opencode session as "already seen" and smoothed
    nothing in it, for the rest of its life.
    """
    from . import config
    return config.SESSIONS_DIR / f"{sid}.gate"


def snapshot(session_id: str | None = None) -> dict:
    """Freeze every persona's declared tools for this session; return what was written.

    ``personas/<n>/persona.md`` and the active-persona pointer are files in the working
    tree, read on every hook call — so before this, one approved edit to a `tools:` line
    was unprompted execution for the rest of the session, no restart and no commit
    (#432). This is the "before": the tools the operator authored, recorded at
    SessionStart, and consulted afterwards instead of re-reading a file the agent has had
    a turn to rewrite.

    **Every** persona, not just the active one, because a mid-session `charter persona
    use <other>` is an ordinary thing to do and must keep working. It moves within a set
    that existed before the session did; a persona invented afterwards is in no snapshot
    and is granted nothing.

    Returns ``{}`` when nothing could be persisted — including when there is no session
    id. A ceiling that cannot be stored must not read as "no ceiling", so :func:`frozen_tools`
    reads an empty map as "approve nothing".

    That sentence used to be written as an unconditional guarantee while `frozen_tools`
    directly below re-read the working tree on ANY read failure — so deleting the ceiling
    file restored the hole the ceiling exists to close (#443). The one remaining fallback
    is now named where it lives, in `frozen_tools`, and is bounded to a session charter
    has no other record of.
    """
    from . import persona, session as _session
    sid = _session.current(session_id)
    if not sid:
        return {}
    try:
        data = {n: sorted(persona.effective_tools(n)) for n in persona.list_personas()}
    except Exception:
        return {}
    try:
        from . import config

        f = _ceiling_file(sid)
        config.private_mkdir(f.parent)
        config.touch_for(_marker_file(sid))   # before the ceiling: see `_marker_file`
        tmp = f.with_name(f.name + ".tmp")
        config.write_for(tmp, json.dumps(data, sort_keys=True))
        os.replace(tmp, f)
    except OSError:
        return {}
    return data


def frozen_tools(name: str, session_id: str | None = None):
    """The tools *name* declared when this session began, or ``None`` when no session is
    identified at all.

    ``None`` is the one case that keeps today's behaviour: a harness that names no
    session has nothing to key a ceiling on, and a gate that silently stopped working
    there would be a regression nobody could see. Every other outcome is a real set —
    possibly empty, which means "approve nothing", which costs a prompt.

    Trust-on-first-use when no snapshot exists, and ONLY then: opencode has no
    SessionStart hook (`harness/opencode.py:160`), so its first gated Bash call takes the
    snapshot. That freezes the session from that point rather than from its beginning —
    weaker, stated rather than papered over, and still strictly better than re-reading the
    file every call.

    "No snapshot exists" is not the same question as "the read failed", and conflating
    them is what made the ceiling optional (#443): it caught ``ValueError`` too, so
    writing `not json` over the file re-read the working tree and granted the widened
    `tools:` line in full. Two rules now bound the fallback, and both fail toward fewer
    approvals:

    * **A file that is there but unreadable is tampering, not a fresh session.** A
      ``ValueError`` — the file exists and is not JSON — is ``set()``. So is a file whose
      JSON is not an object.
    * **A session that already took a ceiling is not first use.** :func:`snapshot` leaves
      a marker beside the ceiling, so a *missing* ceiling next to a present marker means
      one was removed rather than never taken → ``set()``.

    What remains, stated: a harness with no SessionStart hook whose very first charter
    interaction is a gated Bash call. That session is frozen from its first call. Wiping
    the whole state directory reaches the same place — which is why `git clean` is in
    :data:`_DANGEROUS` and why a command naming the state directory is never smoothed.
    """
    from . import session as _session
    sid = _session.current(session_id)
    if not sid:
        return None
    try:
        data = json.loads(_ceiling_file(sid).read_text())
    except ValueError:
        return set()            # present and unparseable → tampering, approve nothing
    except OSError:
        if _ceiling_was_taken(sid):
            return set()        # a ceiling was removed, not never taken
        data = snapshot(sid)    # trust-on-first-use, for a harness with no SessionStart
    if not isinstance(data, dict):
        return set()
    vals = data.get(name)
    return set(vals) if isinstance(vals, list) else set()


def _ceiling_was_taken(sid: str) -> bool:
    """Whether a ceiling has ever been snapshotted for *sid* (see :func:`_marker_file`).

    Errors read as "taken": a marker that cannot be checked is not evidence of a fresh
    session, and the branch this feeds is the one that grants less.
    """
    try:
        return _marker_file(sid).exists()
    except (OSError, ValueError):
        return True


def decide(command: str, session_id: str | None = None, cwd: str | None = None):
    """Return ``(persona, tool)`` if the active persona may run this, else None.

    *cwd* is the directory the command will run in, as the harness reports it. It is what
    lets :func:`_touches_control_surface` resolve a RELATIVE argument to the file it will
    actually open; without it the check falls back to this process's own cwd, which is the
    same directory in every harness charter supports but is an assumption rather than a
    fact.
    """
    from . import persona

    name = persona.resolve_active()
    if not name:
        return None
    tools = persona.effective_tools(name)  # own tools + those of personas it `uses:`
    frozen = frozen_tools(name, session_id)
    if frozen is not None:
        # Intersection, not replacement: a `tools:` line the operator NARROWS takes
        # effect at once (fail toward less), while one widened mid-session grants
        # nothing until the next session (fail toward less again).
        tools &= frozen
    if not tools:
        return None
    binary, args, tokens = _parse(command)
    if not binary or binary not in tools:
        return None
    if _is_interpreter(binary):
        return None  # declaring an interpreter declares every command — keep the prompt
    if _argv_names_another_program(tokens, cwd):
        return None  # an argument is a program: two programs, not one with its arguments
    if _is_dangerous(binary, args):
        return None  # declared, but a destructive subcommand → fall back to a prompt
    if _touches_control_surface(tokens, cwd):
        return None  # reaches a vault or charter's own state → keep the prompt
    if not _runs_the_declared_program(name, tokens, binary, cwd):
        return None  # the file about to execute is not the one the declared name means
    return name, binary


def _runs_the_declared_program(name: str, tokens: list[str], binary: str,
                               cwd: str | None = None) -> bool:
    """True when the FILE this command will execute is the one ``tools: <binary>`` names.

    Every other rule in this module reads arguments. This one reads the word in command
    position, which decides which program reads those arguments at all — and until #450's
    round four it was the last check still answering by NAME. `_parse` reduces the command
    word to ``os.path.basename``, so an agent that writes a program and calls it `gh` gets
    the auto-approval an operator granted to `gh`. Measured through the real
    `hooks.pretooluse`, each of these was an affirmative `allow` for a script written
    seconds earlier by the agent that then ran it:

        ./gh          ./cat          /tmp/gh api /x         bin/kubectl get pods

    That is arbitrary unprompted execution under any `tools:` line at all, and it is the
    same shape as every earlier round: a guard that matched a name rather than the thing
    the name was supposed to refer to. The answer is the one that worked for `_cp_dest_refusal`
    and for :func:`_resolves_into` — compare the OBJECT, ``(st_dev, st_ino)``, not the text.

    Two things charter can point a declared name at, and nothing else:

    * **A persona's own script.** ``personas/<n>/bin/<binary>`` is ground truth: the
      operator committed it. The command word has to be that file, so `/tmp/site-health.sh`
      does not inherit `personas/seo/bin/site-health.sh`'s approval, and a bare
      `site-health.sh` does not either — that resolves through PATH, and PATH is precisely
      what charter cannot vouch for when it holds a file of that name itself.
    * **Whatever a bare invocation resolves to right now** (``shutil.which``). A command
      word spelled as a path is smoothed only when it is, by identity, that same file — so
      `/usr/bin/gh` and `gh` are one answer, `/bin/cat` and a symlink to it are one answer,
      and `./gh` is not `gh`. When ``which`` finds nothing there is nothing to compare
      against and the answer is a prompt.

    A **bare** name with no persona script behind it is approved as before. That is the
    declaration exactly as written, and there is nothing else to check it against.

    A leading ``VAR=value`` assignment is refused for the same reason, and this is where
    the refusal lives rather than in `_parse` — see there. `PATH=/tmp gh api /x` runs
    `/tmp/gh`, so the assignment decides which file executes just as surely as a `/` in the
    command word does. The refusal is of ALL assignments, not of a list of variables that
    matter, because that list cannot be written honestly: `KUBECONFIG=…  kubectl get pods`
    was the shape this skip existed for, and a kubeconfig may carry an `exec` credential
    plugin, which is a kubeconfig naming a program for kubectl to run. There is no inert
    subset to carve out, so the cost is one prompt for `VAR=… cmd` and the class is closed.

    **The residual, stated rather than implied.** For a bare name this asks the filesystem
    which file that name resolves to; it cannot ask whether that file is the *real* `gh`.
    An agent that can write into a directory already on `PATH` puts its own program where
    both bash and this check will find it, and both will agree. Charter has no notion of a
    canonical `gh`, so nothing here closes that; what closes it is that the directory is
    outside the plane and the write is not something a smoothed Bash call performs.
    """
    from . import persona

    if tokens and _ENV_ASSIGN.match(tokens[0]):
        return False        # the environment decides which file runs; see above
    prog = tokens[0] if tokens else ""
    owned = persona.bin_scripts(name).get(binary)
    if os.path.basename(prog) == prog:
        # A bare name is resolved by PATH at run time. Charter can only object when it
        # holds a file of that name itself and this is therefore reaching past it.
        return owned is None
    ref = str(owned) if owned is not None else shutil.which(binary)
    if not ref:
        return False
    try:
        here = os.path.realpath(os.path.join(cwd or os.getcwd(),
                                             os.path.expanduser(prog)))
    except (OSError, ValueError):
        return False
    a, b = _ids(here), _ids(ref)
    return a is not None and a == b


def main(argv=None) -> int:
    try:
        data = json.load(sys.stdin)
    except Exception:
        return 0
    command = ((data or {}).get("tool_input") or {}).get("command", "")
    try:
        result = decide(command, (data or {}).get("session_id"),
                        (data or {}).get("cwd"))
    except Exception:
        result = None
    if result:
        name, binary = result
        print(json.dumps({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason": f"persona '{name}' declares '{binary}' in its tools",
            }
        }))
    return 0
