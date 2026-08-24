"""``charter vault`` and ``charter secret`` command implementations.

The overriding rule here: **secret values must not leak into the model.**
- write paths take values via stdin/file, never argv (argv shows in ps/history);
- ``list`` shows keys only, ``get`` is masked by default;
- ``get --reveal`` refuses a non-interactive stdout (an agent) unless ``--force``;
- the real consumption paths are ``exec`` (inject + redact) and ``cp`` (0600 file).

And the rule's shadow: **every route by which a value leaves this process records that it
did.** ``exec``, ``cp`` and ``get --reveal`` each write one ``charter trace`` event naming
the vault, the key and the command — never a value (#441, and see
:func:`_trace_secret_use`). A plane that hands out credentials and cannot say afterwards
which command got which one has the audit half of the story missing.
"""

from __future__ import annotations

import os
import re
import signal
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

from . import config, util
from .secrets import base, fingerprint, registry


# --------------------------------------------------------------------------- #
# vault management                                                             #
# --------------------------------------------------------------------------- #
def _portable_file(p) -> str:
    """A vault ``file`` as it should be STORED: relative to the control-plane root when it
    lives inside it, otherwise absolute and untouched (issue #21).

    The registry used to record one developer's home directory, so a team that commits its
    reference vaults — they hold ``op://`` URIs, never values — found the vault files
    present on a fresh clone and the index that locates them useless, and had to script
    `charter vault add` calls to rebuild state already in git.

    Absolute survives deliberately: a vault kept outside the plane has no portable form,
    and rewriting it would silently re-point it at somewhere inside.
    """
    from pathlib import Path
    p = Path(p).expanduser()
    try:
        return str(p.resolve().relative_to(Path(config.ROOT).resolve()))
    except (ValueError, OSError):
        return str(p)


#: A POSIX-ish environment variable name. Validated at registration because the failure
#: it prevents surfaces much later and in disguise: a typo'd source name is simply unset,
#: and an unset source is (deliberately) a hard error at read time about a credential.
_ENV_NAME = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")


def _env_bindings(args) -> dict | None:
    """``{TARGET: SOURCE}`` from ``--env``/``--token-env``, or None if none were given.

    ``--token-env X`` is sugar for ``--env OP_SERVICE_ACCOUNT_TOKEN=X``. The general form
    exists because reference vaults already resolve two schemes — `op://` and `vault://` —
    so a binding that could only ever mean "the 1Password token" would be wrong on arrival
    for half of what charter ships.
    """
    pairs: dict = {}
    token_env = getattr(args, "token_env", None)
    if token_env:
        pairs["OP_SERVICE_ACCOUNT_TOKEN"] = token_env
    for raw in getattr(args, "env", None) or []:
        target, sep, source = raw.partition("=")
        if not sep:
            raise ValueError(f"--env expects TARGET=SOURCE, got {raw!r}")
        pairs[target.strip()] = source.strip()
    for target, source in pairs.items():
        for label, v in (("target", target), ("source", source)):
            if not _ENV_NAME.match(v or ""):
                raise ValueError(f"--env {label} {v!r} is not a valid environment "
                                 f"variable name")
    return pairs or None


def _unignored_plaintext(configured: str):
    """The path, if a plain-file vault would store plaintext somewhere git tracks.

    ``None`` when it is outside the plane (git here has no say), when the plane is not a
    git repo, or when git already ignores it. Asks `git check-ignore` rather than reading
    `.gitignore`, so nested ignore files, negations and global excludes all count — the
    question is only ever "would git take this file", and git is the authority on that.
    """
    from pathlib import Path as _P
    p = _P(configured).expanduser()
    if not p.is_absolute():
        p = _P(config.ROOT) / p
    try:
        p.resolve().relative_to(_P(config.ROOT).resolve())
    except (ValueError, OSError):
        return None                                   # outside the plane
    ignored = util.git_ignores(config.ROOT, p)
    if ignored is None or ignored:
        return None                # not a repo (nothing to commit to), or already ignored
    try:
        return str(p.resolve().relative_to(_P(config.ROOT).resolve()))
    except (ValueError, OSError):
        return str(p)


def cmd_vault_add(args) -> int:
    cfg: dict = {}
    if args.provider == "plain-file":
        cfg["file"] = _portable_file(args.file or config.VAULTS_DIR / f"{args.name}.json")
    elif args.provider == "reference":
        # A reference vault stores URIs, not values, but it still needs somewhere to
        # keep them. Defaulting this (it used to apply only to plain-file) is why
        # `vault add x --provider reference` registered and then warned that it had no
        # file configured — technically correct and useless as a first experience.
        cfg["file"] = _portable_file(args.file or config.VAULTS_DIR / f"{args.name}.json")
    elif args.file:
        cfg["file"] = _portable_file(args.file)
    # A plain-file vault holds PLAINTEXT. The default lives under `.charter/`, which
    # `charter init` gitignores — but `--file` accepts any path, and a vault pointed at
    # one git tracks commits the credentials on the next `charter save`. Nothing said so:
    # `doctor` reported "all healthy", because from the vault's point of view it was.
    #
    # Refusing the PATH rather than refusing `--share` (which would be the obvious guess)
    # is deliberate: a team that provisions the file out of band has a legitimate use for
    # a shared pointer, and the unignored path is the actual defect — it also catches the
    # far more common case where `--share` was never passed at all.
    if args.provider == "plain-file" and cfg.get("file"):
        unignored = _unignored_plaintext(cfg["file"])
        if unignored:
            util.err(f"'{unignored}' is inside the control plane and NOT gitignored — a "
                     f"plain-file vault stores plaintext, so the next `charter save` would "
                     f"commit these credentials.")
            util.info("  Keep it out of git (add it to .gitignore, or use the default "
                      f"under {config.VAULTS_DIR.name}/), or point --file outside the plane.")
            util.info("  A `reference` vault stores op:// URIs rather than values and IS "
                      "safe to commit: --provider reference")
            return 1

    if args.provider == "1password":
        if not getattr(args, "op_vault", None):
            util.err("--op-vault is required for a 1password vault: which 1Password "
                     "vault should charter create its items in?\n"
                     f"  charter vault add {args.name} --provider 1password --op-vault Engineering")
            return 1
        cfg["op-vault"] = args.op_vault
        if getattr(args, "op_item", None):
            cfg["op-item"] = args.op_item
        if getattr(args, "account", None):
            cfg["account"] = args.account
    try:
        env_map = _env_bindings(args)
    except ValueError as e:
        util.err(str(e))
        return 1
    if env_map:
        cfg["env"] = env_map

    force = getattr(args, "force", False)
    share = getattr(args, "share", False)
    replaced = registry.vaults().get(args.name) if force else None
    try:
        registry.add_vault(args.name, args.provider, cfg, persona=args.persona,
                           force=force, share=share)
    except base.VaultError as e:
        util.err(str(e))
        return 1
    if replaced is not None:
        # Say what was let go of, and where it still is. `--force` does not migrate, so
        # the old file survives on disk with nothing pointing at it — the user needs its
        # path to recover anything from it.
        old_where = (replaced.get("config") or {}).get("file")
        util.warn(f"Replaced the previous '{args.name}' registration "
                  f"(provider: {replaced.get('provider', '?')}). Its secrets were NOT "
                  f"migrated." + (f" They remain at {old_where}." if old_where else ""))
    tag = f", persona: {args.persona}" if args.persona else ""
    where = "shared — commit vaults.json" if share else "local only"
    util.ok(f"Vault '{args.name}' registered (provider: {args.provider}{tag}) [{where}].")
    if not share:
        util.info(f"  Teammates won't see it. Publish the wiring (never the secrets) with: "
                  f"charter vault add {args.name} --provider {args.provider} --share")
    prov = registry.provider_for(args.name)
    ok, detail = prov.health()
    (util.info if ok else util.warn)(f"  {detail}")
    if not prov.available:
        util.warn(f"  provider '{args.provider}' is not implemented yet — registered for later use.")
    elif args.provider == "1password":
        # Name the ITEM. This is the one moment the operator learns where to look in the
        # 1Password UI, and the schema is one item per vault whose concealed fields are
        # the secrets — not one item per secret, which is what this said until #400 and
        # what sent people hunting for a `charter-<vault>-<KEY>` that charter has not
        # created for several releases. `prov.op_item` rather than the config, so an
        # adopted `--op-item` is named as itself and the default resolves in one place.
        util.info(f"  charter keeps this vault in one 1Password item, "
                  f"'{prov.op_item}' in vault '{cfg['op-vault']}', tagged "
                  f"'charter:{args.name}' — each secret a concealed field of it.")
        util.info(f"  add secrets with: charter secret set {args.name} <key> --stdin")
    elif args.provider == "reference":
        util.info(f"  stores op:// or vault:// URIs; values are fetched at read time.")
        util.info(f"  add one with: charter secret set {args.name} <key> "
                  f"--value 'op://<vault>/<item>/<field>'")
    else:
        util.info(f"  add secrets with: charter secret set {args.name} <key> --stdin")
    return 0


# --------------------------------------------------------------------------- #
# verify — the check that actually resolves                                    #
#                                                                              #
# `health()` asks whether a vault is REACHABLE and how many items it holds. It #
# deliberately never resolves: `vault list` and `doctor` call it routinely, and #
# a resolve would hit 1Password every time and could prompt for re-auth. Good  #
# reason to skip it — and no reason to then call the result "healthy".         #
#                                                                              #
# Issue #55, from a live incident: `doctor` said "6 configured, all healthy"   #
# while every resolution through those vaults was failing. Both true. A        #
# reference can point at an item that no longer exists while the vault holding #
# it is perfectly reachable, and nothing tested that until something failed at #
# runtime — with the green line steering people away from the cause for forty  #
# minutes. So the expensive check gets its own command, and the cheap one      #
# stops overclaiming.                                                          #
# --------------------------------------------------------------------------- #
def verify_vault(prov) -> list[dict]:
    """Resolve every reference in *prov*, reporting whether each one worked.

    Returns ``[{"key", "ok", "error"}]`` and **never the resolved value** — a verify
    result is printed to a terminal and may end up in a log or a CI transcript.

    A vault whose key list itself cannot be read reports one failing row rather than
    raising: `verify` is the command you run when something is already wrong, so it has
    to survive the thing being wrong.
    """
    try:
        keys = prov.keys()
    except Exception as e:  # noqa: BLE001 - see docstring
        return [{"key": "*", "ok": False, "error": str(e)}]

    rows = []
    for key in keys:
        try:
            prov.get(key)
        except Exception as e:  # noqa: BLE001 - any resolver failure is a failed verify
            rows.append({"key": key, "ok": False, "error": str(e)})
        else:
            rows.append({"key": key, "ok": True, "error": ""})
    return rows


def _vaults_to_verify(name: str | None = None):
    names = [name] if name else list(registry.vaults())
    out = []
    for n in names:
        try:
            out.append(registry.provider_for(n))
        except base.VaultError as e:
            util.err(f"{n}: {e}")
    return out


def cmd_vault_verify(args) -> int:
    """Resolve every reference in every vault (or one named vault) and report failures.

    Exits non-zero when anything fails to resolve, because the exit status is what a CI
    step or a `&&` chain reads — printing a dead reference and exiting 0 would be the same
    lie in a new place.
    """
    provs = _vaults_to_verify(getattr(args, "name", None))
    if not provs:
        util.info("No vaults to verify.")
        return 0

    failed = 0
    for prov in provs:
        rows = verify_vault(prov)
        bad = [r for r in rows if not r["ok"]]
        failed += len(bad)
        if not rows:
            util.info(f"{prov.name}: no references to verify")
            continue
        if not bad:
            util.ok(f"{prov.name}: {len(rows)} reference(s) resolved")
            continue
        util.err(f"{prov.name}: {len(bad)} of {len(rows)} reference(s) did NOT resolve")
        for r in bad:
            print(f"    {r['key']}: {r['error']}")
    if failed:
        util.info("A reference can be registered and still not resolve — the vault being "
                  "reachable says nothing about the item behind the reference.")
        return 1
    return 0


def cmd_vault_list(args) -> int:
    doc = registry.load_registry()
    vs = registry.vaults(doc)
    if not vs:
        util.info("No vaults configured. Add one: "
                  "charter vault add <name> --provider plain-file --file <path>")
        return 0
    # SCOPE earns a column because a two-file registry invites exactly two questions —
    # "why can't my teammate see this vault" and "why is this vault in git" — and both are
    # answered by where it is registered.
    fmt = "{:<18} {:<16} {:<12} {:<7} {}"
    print(fmt.format("VAULT", "PROVIDER", "PERSONA", "SCOPE", "STATUS"))
    print(fmt.format("-" * 18, "-" * 16, "-" * 12, "-" * 7, "-" * 28))
    for name in sorted(vs):
        try:
            prov = registry.provider_for(name, doc)
            # `env_overlay` raises when a declared identity's variable is unset, which is
            # exactly the state a listing should show rather than let a later read
            # discover. It reads as "no secrets" everywhere else.
            prov.env_overlay()
            ok, detail = prov.health()
        except base.VaultError as e:
            detail = str(e).split("\n")[0]
        persona = vs[name].get("persona") or "—"
        print(fmt.format(name, vs[name].get("provider", "?"), persona,
                         registry.scope_of(name), detail))
    return 0


def cmd_vault_remove(args) -> int:
    try:
        registry.remove_vault(args.name)
    except base.VaultError as e:
        util.err(str(e))
        return 1
    util.ok(f"Vault '{args.name}' removed from the registry. "
            "(Any underlying file is left on disk untouched.)")
    return 0


# --------------------------------------------------------------------------- #
# secret read / write                                                          #
# --------------------------------------------------------------------------- #
def _provider(name: str):
    prov = registry.provider_for(name)
    if not prov.available:
        raise base.ProviderUnavailable(
            f"vault '{name}' uses provider '{prov.id}', which is not implemented yet."
        )
    return prov


def _read_value(args) -> str:
    """Obtain a secret value without exposing it on the command line.

    A non-tty stdin is still read — `… | charter secret set <vault> <key>` is the ordinary
    way to do this and predates `--stdin`. What is refused is the RESULT being empty (see
    `cmd_secret_set`), which is the actual failure: an agent's Bash tool, a CI step and
    `< /dev/null` all present a non-tty stdin with nothing behind it, so the read returned
    "" and overwrote the credential.

    Requiring an explicit `--stdin` for every pipe was the first fix here and it was worse
    than the bug in one direction: it broke every working pipeline to stop a mistake the
    empty check already catches. Narrow the refusal to the thing that is wrong.
    """
    if args.from_file:
        return Path(args.from_file).expanduser().read_text()
    if args.value is not None:
        util.warn("Value passed via --value is visible in shell history / process list; "
                  "prefer --stdin or --from-file.")
        return args.value
    if args.stdin or not sys.stdin.isatty():
        data = sys.stdin.read()
        return data[:-1] if data.endswith("\n") else data  # strip one trailing newline
    import getpass
    return getpass.getpass(f"Value for '{args.key}' (hidden): ")


def cmd_secret_set(args) -> int:
    try:
        prov = _provider(args.vault)
        value = _read_value(args)
        # An empty value is almost always an accident — a command substitution that
        # produced nothing, a file that was not there — and storing it is indistinguishable
        # afterwards from storing a real secret: `get` reports present, `vault list` counts
        # it, `doctor` says healthy. A warning is not enough for something that can only be
        # detected later, by a 401.
        if value == "" and not getattr(args, "allow_empty", False):
            how = ("" if sys.stdin.isatty() else
                   "\n  Nothing arrived on stdin. An agent's shell, a CI step and "
                   "`< /dev/null` all\n  look like a pipe with no data behind them.")
            raise base.VaultError(
                f"refusing to store an empty value for '{args.key}' — it would read as a "
                f"present, healthy secret everywhere charter looks.{how}\n"
                f"  If that is genuinely what you want: --allow-empty")
        prov.set(args.key, value)
    except base.VaultError as e:
        util.err(str(e))
        return 1
    # Banded, not counted, for the same reason `get` is (#436): this line is printed into
    # whatever ran the command — an agent's transcript for `--from-file`, where the length
    # of a value nobody in the conversation has seen is the one thing it would disclose.
    util.ok(f"Set '{args.key}' in vault '{args.vault}' "
            f"({fingerprint.size_band(value)}). Value not shown.")
    return 0


def cmd_secret_list(args) -> int:
    try:
        keys = _provider(args.vault).keys()
    except base.VaultError as e:
        util.err(str(e))
        return 1
    if not keys:
        util.info(f"Vault '{args.vault}' has no secrets.")
        return 0
    for k in keys:
        print(k)
    return 0


def cmd_secret_audit(args) -> int:
    """Flag secrets older than --days as stale (rotation hygiene). Providers that manage
    rotation externally (1Password, Vault) report nothing to do here."""
    try:
        prov = _provider(args.vault)
    except base.VaultError as e:
        util.err(str(e))
        return 1
    ages_fn = getattr(prov, "ages", None)
    if not callable(ages_fn):
        util.info(f"vault '{args.vault}' ({prov.id}) manages rotation externally — no age tracking.")
        return 0
    ages = ages_fn()
    if not ages:
        util.info(f"vault '{args.vault}' has no secrets.")
        return 0
    stale = sorted(((k, d) for k, d in ages.items() if d is not None and d >= args.days),
                   key=lambda x: -x[1])
    unknown = sorted(k for k, d in ages.items() if d is None)
    for k, d in stale:
        util.warn(f"{args.vault}/{k}: {d} days old — consider rotating")
    if not stale:
        util.ok(f"no secrets in '{args.vault}' older than {args.days} days.")
    if unknown:
        util.info(f"age unknown (set before tracking): {', '.join(unknown)}")
    return 1 if stale else 0


# --------------------------------------------------------------------------- #
# the access record — WHICH command got WHICH credential (#441)                #
# --------------------------------------------------------------------------- #
# The vocabulary is `trace.SECRET_USE_EVENTS`, and there are exactly three routes by which
# a plaintext leaves this process: a child's environment or a temp file (`exec`), a file on
# disk (`cp`), and a terminal (`get --reveal`). All three record. `secret get` WITHOUT
# `--reveal` does not, because nothing left — it prints a length and a digest.
#
# Recording two of the three would have been worse than recording none. An operator who
# greps the trace for `secret-exec` and `secret-cp`, finds nothing, and concludes the token
# never left the vault has been told something false by a record that looks complete.


def _value_free(field, values: list[str]):
    """*field* with every value in *values* replaced by ``***``, at any depth.

    Recurses through dicts, lists and tuples rather than checking a list of field names
    charter happens to record today: the next field added to a record is the one nobody
    scrubs, and a rule about *shapes* covers a field that has not been written yet.
    """
    if isinstance(field, str):
        return base.redact(field, values)
    if isinstance(field, dict):
        return {_value_free(k, values): _value_free(v, values) for k, v in field.items()}
    if isinstance(field, (list, tuple)):
        return [_value_free(v, values) for v in field]
    return field


def _trace_secret_use(event: str, resolved: list[str], **fields) -> None:
    """Record that a credential was handed out — WHICH command, WHICH names, never a value.

    `charter trace` knew about `secret-warn`, the scanner that notices a value in a file it
    is about to commit, and about nothing charter itself handed out. So after the fact
    there was no answer to *"which command received the prod token"* — the cheapest
    observability a plane holding credentials can have, and the one it did not have (#441).

    **Names, and one argv element.** `vault`, the key names, the environment variable names
    they were bound to, and ``argv[0]``. Not the rest of argv: charter never substitutes a
    value into a command line, but the caller may have *typed* one there, and a record
    whose purpose is to hold no values must not copy a line that might.

    *resolved* is the values this call actually read, and it is used for exactly one thing:
    removing them from the fields above, at any depth, before anything is written. That is
    belt and braces over the rule that only names are passed in — the half that still holds
    when somebody adds a field here in a year. Its bound is stated rather than implied: it
    can only remove values **this call resolved**, so it is not a filter that makes an
    arbitrary payload safe to record, and nothing may be passed here on the strength of it.

    Best-effort and silent, like every other trace site: observability must never break the
    thing it observes, and a credential that was successfully delivered must not be turned
    into a failure by the bookkeeping about it.
    """
    try:
        from . import trace
        trace.record(event, **{k: _value_free(v, resolved) for k, v in fields.items()})
    except Exception:
        pass


def cmd_secret_get(args) -> int:
    try:
        value = _provider(args.vault).get(args.key)
    except base.VaultError as e:
        util.err(str(e))
        return 1

    if not args.reveal:
        # NOT a hash of the value. `sha256(value)[:12]` plus the exact byte count turned
        # this line into an offline verification oracle for a guessed value — see
        # `secrets/fingerprint.py` (#436).
        print(f"{args.vault}/{args.key}: present · {fingerprint.masked(value)}")
        print("(value hidden — use `charter secret exec` to hand it to a command, "
              "`secret cp` for a tool that needs a file path, or --reveal to print)")
        return 0

    # --reveal: only for a human at an interactive terminal. Refuse the exact
    # channel (piped stdout) through which a secret would reach an agent's context.
    if not sys.stdout.isatty() and not args.force:
        util.err(
            "Refusing to print a secret to non-interactive stdout — this is how it would "
            "leak into an agent's context. Use `charter secret exec` to hand it to a command "
            "instead (`secret cp` materialises a 0600 file for a tool that needs a path — "
            "reading that file back leaks the value just the same), or pass --force if you "
            "truly intend to print it."
        )
        return 2
    # Before the write, not after: the record of a credential leaving must not depend on
    # the write that sends it away succeeding (a closed pipe, a full terminal).
    _trace_secret_use("secret-reveal", [value], vault=args.vault, key_names=[args.key],
                      forced=bool(args.force))
    util.warn("Revealing secret plaintext to this terminal.")
    sys.stdout.write(value if value.endswith("\n") else value + "\n")
    return 0


def cmd_secret_rm(args) -> int:
    try:
        _provider(args.vault).delete(args.key)
    except base.VaultError as e:
        util.err(str(e))
        return 1
    util.ok(f"Removed '{args.key}' from vault '{args.vault}'.")
    return 0


#: What a destination turned out to be, for the refusal message. Same table and same
#: wording as `contain._KINDS`, because "that is not a file" is one question and the
#: operator should not have to learn two vocabularies for it.
_DEST_KINDS = ((stat.S_ISDIR, "a directory"), (stat.S_ISFIFO, "a FIFO"),
               (stat.S_ISSOCK, "a socket"), (stat.S_ISCHR, "a character device"),
               (stat.S_ISBLK, "a block device"))

#: charter's own three streams, by descriptor, for the refusal message.
_OWN_STREAMS = ((0, "standard input"), (1, "standard output"), (2, "standard error"))


def _own_stream_identities() -> dict[tuple[int, int], str]:
    """``(st_dev, st_ino) -> "standard output"`` for this process's own three streams.

    IDENTITY, not name. The first version of this guard asked what a path was *called*
    and `/dev/fd/1` walked straight through it: on macOS `/dev/fd/N` is neither a symlink
    nor a device but the underlying object re-opened, so `os.lstat` reported a plain
    regular file and the "already exists" arm took over — an arm `--force` switches off.
    `charter secret cp v k /dev/fd/1 --force` then wrote the credential into charter's own
    captured stdout and printed "Value not shown." on top of it. That is issue #421's
    symptom with a different spelling, and no list of spellings closes it: `/dev/stdout`,
    `/dev/fd/1`, `/proc/self/fd/1`, the path `readlink` gives for the transcript log, and
    any hardlink to it are five names for one inode.

    So the question asked here is "is this object the same object as one of my streams",
    which has exactly one answer per object however it is spelled.

    A stream charter cannot stat (a closed descriptor) contributes nothing rather than
    raising: the caller's other arms still apply.
    """
    out: dict[tuple[int, int], str] = {}
    for fd, name in _OWN_STREAMS:
        try:
            st = os.fstat(fd)
        except OSError:
            continue
        out.setdefault((st.st_dev, st.st_ino), name)
    return out


def _own_stream_refusal(dest, which: str) -> str:
    """The refusal for "that destination IS one of charter's own streams".

    Deliberately names no flag. The refusal this replaced ended "Pass --force to
    overwrite it deliberately" — and `--force` was the bypass, which is the exact
    pattern #421 filed against hooks.py's `--reveal` text. A guard that prints its own
    way around itself is a signpost, not a guard.
    """
    return (f"{dest} is charter's own {which} — the channel this conversation is read "
            f"from, whatever it is called here. Writing a credential to it puts the "
            f"plaintext straight into the transcript, which is the leak `secret cp` "
            f"exists to avoid. Name a real file that is not one of these streams.")


def _identify_dest(raw: str):
    """`fstat` of *raw* opened for writing without creating or truncating anything.

    `os.lstat` is not enough: on macOS it answers about the devfs entry for `/dev/fd/N`,
    whose `st_dev` is devfs's and NOT the underlying file's, so a `(st_dev, st_ino)`
    comparison against `os.fstat(1)` does not match. Measured on this platform::

        lstat /dev/fd/1  dev=18446744071623019954 ino=202112957
        fstat(1)         dev=16777234             ino=202112957
        fstat(open('/dev/fd/1', O_WRONLY))  dev=16777234 ino=202112957

    Only the descriptor tells the truth, so this opens one. The open is deliberately
    harmless — no ``O_CREAT``, no ``O_TRUNC``, ``O_NOFOLLOW`` so a symlink is not
    traversed, ``O_NONBLOCK`` so a FIFO planted since the `lstat` cannot block the
    process with the answer half-computed. Callers must have refused every non-regular
    kind *before* calling: opening a device is not free of side effects, and this must
    never be the thing that opens one.

    Returns ``None`` when the destination cannot be opened at all (it does not exist, or
    is not writable) — nothing to compare, and the real open below will say so.
    """
    flags = os.O_WRONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
    try:
        fd = os.open(raw, flags)
    except OSError:
        return None
    try:
        return os.fstat(fd)
    except OSError:
        return None
    finally:
        os.close(fd)


def _cp_dest_refusal(dest: Path, force: bool) -> str | None:
    """Why *dest* is not somewhere a secret may be materialised — or ``None``.

    `secret cp` is documented as the *safe* consumption path: "prints only the path,
    never the contents" (`docs/secrets.md`). That sentence was true of the command and
    false of the destination, because the destination decided what "writing a file"
    meant:

    * **A non-regular destination writes the plaintext to whatever is on the other end.**
      `charter secret cp <v> <k> /dev/stdout` printed the credential onto charter's own
      stdout — the agent's captured pipe — and then printed `Value not shown.` on the
      same stream. `/dev/stderr` and `/dev/fd/N` are the same channel by another name,
      and a FIFO is worse: the open blocks with the value already resolved. This is the
      exact channel `cmd_secret_get --reveal` refuses (`sys.stdout.isatty()` above);
      `cp` refused nothing.
    * **A symlink writes through to its target**, which is how a "regular file" check
      gets walked past — so the link is refused here *and* the open below carries
      ``O_NOFOLLOW``, which also closes the swap between this `lstat` and that `open`.
    * **An existing file was truncated and chmodded 0600 with no warning and no flag.**
      A pre-existing `~/.ssh/config` came back as the credential at 0600. Overwriting is
      now `--force`, and the default is ``O_EXCL``.

    Ordered so the *most specific* thing true of the path is what gets said: a symlink to
    a character device reads as a symlink, not as a device. The stream-identity arm sits
    AHEAD of the "already exists" arm on purpose — the exists arm names `--force`, and
    `--force` must never be the suggested next step for one of charter's own streams.
    """
    raw = str(dest)
    if not raw:
        return "the destination path is empty."
    try:
        st = os.lstat(raw)
    except FileNotFoundError:
        return None                       # nothing there yet — the ordinary case
    except (OSError, ValueError) as e:
        # `os.lstat` raises ValueError, not OSError, on a path holding a NUL.
        return f"{dest} cannot be inspected ({getattr(e, 'strerror', None) or e})."
    if stat.S_ISLNK(st.st_mode):
        return (f"{dest} is a symlink, and charter will not follow one to write a "
                f"credential — the link decides where the plaintext lands, not you. "
                f"Name the real path.")
    if not stat.S_ISREG(st.st_mode):
        kind = next((k for test, k in _DEST_KINDS if test(st.st_mode)), "not a file")
        return (f"{dest} is {kind}, not a regular file. `secret cp` materialises a "
                f"credential *to a file*; writing it to a device or a pipe puts the "
                f"plaintext on whatever is reading that — and /dev/stdout, /dev/stderr "
                f"and /dev/fd/* are this agent's own transcript.")
    streams = _own_stream_identities()
    # Two lookups because they answer different questions. The `lstat` pair catches a
    # second *name* for the same inode — a hardlink, or the path `readlink` gives for the
    # transcript log — without opening anything. The descriptor pair catches `/dev/fd/N`,
    # where `lstat`'s `st_dev` is devfs's rather than the file's. Neither is a spelling.
    which = streams.get((st.st_dev, st.st_ino))
    if which is None:
        opened = _identify_dest(raw)
        if opened is not None:
            which = streams.get((opened.st_dev, opened.st_ino))
    if which:
        return _own_stream_refusal(dest, which)
    if not force:
        return (f"{dest} already exists. Writing would destroy its contents and set it "
                f"to 0600. Pass --force to overwrite it deliberately, or choose a path "
                f"that does not exist.")
    return None


def cmd_secret_cp(args) -> int:
    """Materialize a secret to a real file at 0600 (e.g. a kubeconfig). Prints path only.

    The destination is checked BEFORE the value is resolved: a refused path never causes
    a vault read, so the plaintext is never in this process at all for the case that was
    about to print it.

    What it does NOT do is put the destination under a guard. A ledger of materialised
    paths, consulted by the vault read guards, was written and then dropped: #423 was
    closed the other way in 0.52.0, on the argument that such a ledger matches a SPELLING
    — `/tmp/./x`, a hardlink, a copy, `python3 -c open(...)` all walk past it — at the
    price of a ledger read on the hook's hot path. The `--reveal` and read denials no
    longer offer `secret cp` as a way to SEE a value, and they say in the same breath that
    no guard covers a path you chose. `docs/secrets.md` and `SECURITY.md` state that limit.
    """
    dest = Path(args.dest).expanduser()
    force = bool(getattr(args, "force", False))

    refusal = _cp_dest_refusal(dest, force)
    if refusal:
        util.err(f"Refusing to write a secret: {refusal}")
        return 2

    # ADR-0017, the same rule `vault add` applies to a plain-file vault: a credential
    # written somewhere git tracks is committed by the next `charter save`. `vault add`
    # refused this and `cp` did not, which is one door checked and its twin left open.
    # Absolute, because a relative destination is CWD-relative for the open below while
    # `_unignored_plaintext` resolves a relative path against the plane root.
    unignored = _unignored_plaintext(os.path.abspath(dest))
    if unignored:
        util.err(f"Refusing to write a secret: '{unignored}' is inside the control plane "
                 f"and NOT gitignored — the next `charter save` would commit it.")
        util.info("  Add it to .gitignore, write it under .charter/, or pick a path "
                  "outside the plane.")
        return 2

    parent = dest.parent
    if not parent.is_dir():
        # One missing level, not a tree: `mkdir -p` on a caller-supplied path builds
        # arbitrary directories anywhere the user can write, and a typo'd destination
        # should read as a typo rather than silently materialise.
        try:
            os.mkdir(parent, 0o700)
        except FileExistsError:
            pass                          # a race, or a non-directory — the open answers
        except OSError as e:
            util.err(f"Refusing to write a secret: cannot create {parent} "
                     f"({e.strerror or e}). charter creates at most one missing "
                     f"directory level; create the parent yourself if you meant this.")
            return 2

    # O_EXCL by default so an existing file is never clobbered; O_NOFOLLOW so a symlink
    # planted between the check above and this open cannot redirect the write; O_NONBLOCK
    # so a FIFO planted there cannot block the open with the destination undecided.
    # NOT O_TRUNC, even under --force: truncation is destruction, and nothing may be
    # destroyed until the descriptor has been asked what it actually is. `--force` on
    # `/dev/fd/1` used to truncate the transcript before writing the credential into it.
    flags = os.O_WRONLY | os.O_CREAT | getattr(os, "O_NOFOLLOW", 0)
    flags |= getattr(os, "O_NONBLOCK", 0)
    if not force:
        flags |= os.O_EXCL
    # Only what THIS call brought into existence may be removed on the way out. A
    # destination that was already there is someone else's file even when charter is
    # about to refuse it — and for the case that matters most, `/dev/fd/1`, unlinking
    # would delete the transcript charter is refusing to write into.
    created = not os.path.lexists(str(dest))
    try:
        fd = os.open(str(dest), flags, 0o600)
    except OSError as e:
        util.err(f"Refusing to write a secret: cannot open {dest} ({e.strerror or e}).")
        return 2

    def _abandon() -> None:
        os.close(fd)
        if created:
            _safe_unlink(str(dest))

    # The guarantee, as opposed to the courtesy. Everything above inspects a PATH, and a
    # path is a name that something else may be holding or may swap. This asks the open
    # descriptor what it is, which is the one question with a single answer, and it runs
    # BEFORE the vault is read — so a destination refused here still never resolves the
    # plaintext, and a `--force` aimed at charter's own stdout writes nothing and
    # truncates nothing.
    try:
        opened = os.fstat(fd)
    except OSError as e:
        _abandon()
        util.err(f"Refusing to write a secret: cannot inspect {dest} "
                 f"({getattr(e, 'strerror', None) or e}).")
        return 2
    # Same order as `_cp_dest_refusal`: the most specific true thing gets said, so a
    # device reads as a device rather than as "charter's stdin" on the many runs whose
    # stdin happens to be /dev/null.
    if not stat.S_ISREG(opened.st_mode):
        kind = next((k for test, k in _DEST_KINDS if test(opened.st_mode)), "not a file")
        _abandon()
        util.err(f"Refusing to write a secret: {dest} is {kind}, not a regular file — "
                 f"whatever the path looked like before it was opened.")
        return 2
    which = _own_stream_identities().get((opened.st_dev, opened.st_ino))
    if which:
        _abandon()
        util.err(f"Refusing to write a secret: {_own_stream_refusal(dest, which)}")
        return 2

    try:
        value = _provider(args.vault).get(args.key)
    except base.VaultError as e:
        _abandon()
        util.err(str(e))
        return 1

    # Before the write, and deliberately: from here on a plaintext is in this process with
    # a descriptor open on its destination, so this is the last point at which the record
    # is guaranteed to be made. Recorded after `f.write` returned, a partial write
    # interrupted by ENOSPC or a signal would leave the value on disk and no trace of it.
    _trace_secret_use("secret-cp", [value], vault=args.vault, key_names=[args.key],
                      dest=str(dest), overwrote=bool(force and not created))
    with os.fdopen(fd, "w") as f:
        # fchmod, not chmod: the mode lands on the file this process opened, not on
        # whatever the path names by the time the write finishes.
        os.fchmod(f.fileno(), 0o600)
        # O_TRUNC's job, done late: an overwrite of a longer file would otherwise leave
        # the tail of the old contents behind the new value.
        os.ftruncate(f.fileno(), 0)
        f.write(value)
    if force and not created:
        util.warn(f"Overwrote {dest} and set it to 0600.")
    util.ok(f"Wrote '{args.vault}/{args.key}' to {dest} (0600). Value not shown.")
    return 0


def _identity_vars(doc: dict | None = None) -> dict[str, set[str]]:
    """Every environment-variable name a registered vault uses as an identity, by vault.

    A vault may bind the identity it is read through (`VaultProvider.env_overlay`)::

        "env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_ACME_DEVOPS_SERVICE_ACCOUNT_TOKEN"}

    BOTH halves are identity variables: the value is where this machine carries the
    credential, and the key is the variable the vault's CLI reads it out of. Names only
    — the registry never holds a value, and neither does this.
    """
    out: dict[str, set[str]] = {}
    for name, vc in (registry.vaults(doc) or {}).items():
        mapping = (vc or {}).get("config", {}).get("env") or {}
        if not isinstance(mapping, dict):
            continue                       # a committed registry is untrusted input
        names = {str(k) for k in mapping} | {str(v) for v in mapping.values()}
        out[name] = {n for n in names if n}
    return out


def _child_env(vault: str) -> dict:
    """The environment a `secret exec` child gets: this process's, minus every OTHER
    vault's declared identity variables.

    ``dict(os.environ)`` handed the child every credential on the machine. Measured, with
    fabricated values, `charter secret exec <v> --env T=K -- /usr/bin/env` returned the
    one secret the model named as ``***`` and every other vault's service-account token
    in the clear — into the caller's captured output. `base.redact` cannot help: it only
    knows the values *this* call resolved.

    `VaultProvider.env_overlay` sells the binding as least-privilege — "without this the
    mapping lives in every caller's shell… which is the property the vault abstraction
    otherwise removes." Inheriting the whole environment put it straight back.

    The vault being read keeps its own names, so `charter secret exec devops -- charter
    secret get devops K` still works; no other vault's identity crosses. Nothing here
    can break a working setup: a child was never meant to hold another vault's identity,
    and this removes nothing else — the resolution of THIS vault happens in charter's own
    process, before and independently of this dict.
    """
    declared = _identity_vars()
    # No `try` here on purpose. `_provider()` loads the same registry a few lines before
    # this is called and raises if it cannot, so there is no state in which this is asked
    # of a registry that failed to load — and an `except: return dict(os.environ)` would
    # be a fallback that quietly restores the exact behaviour this removes.
    strip = set().union(set(), *declared.values()) - declared.get(vault, set())
    return {k: v for k, v in os.environ.items() if k not in strip}


def cmd_secret_exec(args) -> int:
    """Run a command with secrets injected as env vars and/or files, then redact.

    The model constructs the command using env-var *names* and never sees any
    value; charter resolves the secrets at runtime and scrubs them from the output.

    With ``--exec`` the command *replaces* this process (``os.execvpe``) instead
    of being captured. Capturing buffers stdout until exit, which deadlocks any
    long-running or streaming child — an MCP stdio server never completes its
    handshake. Exec'ing inherits fds 0/1/2 so the stream is untouched; the
    trade-off is that nothing is captured, so nothing can be redacted.
    """
    try:
        prov = _provider(args.vault)
    except base.VaultError as e:
        util.err(str(e))
        return 1

    command = list(args.command or [])
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        util.err("No command given. Usage: "
                 "charter secret exec <vault> --env NAME=key -- <command...>")
        return 2

    exec_mode = bool(getattr(args, "exec_mode", False))
    dotenv_specs = list(getattr(args, "dotenv", None) or [])
    # `--exec` REPLACES this process, so nothing survives to delete a temp file. A
    # long-running child whose credential must be a FILE — every Google-ADC server, since
    # GOOGLE_APPLICATION_CREDENTIALS takes a path and not a value — therefore fitted neither
    # mode: --file was the only way to materialise it and --exec the only way to run it
    # (#190).
    #
    # `--stream` is the third mode: fork, inherit stdio, wait, then clean up. Streaming was
    # never what exec bought here — a forked child inherits the parent's descriptors — so the
    # only thing given up is replacing the process, which is exactly the thing that made
    # cleanup impossible.
    stream_mode = bool(getattr(args, "stream_mode", False))
    if exec_mode and stream_mode:
        util.err("--exec and --stream are two ways to run the same command; pick one. "
                 "--stream is the one that can clean up a --file credential.")
        return 2
    if exec_mode and ((args.file or []) or dotenv_specs):
        flags = " and ".join(f for f, on in (("--file", args.file or []),
                                             ("--dotenv", dotenv_specs)) if on)
        util.err(f"{flags} cannot be combined with --exec: exec replaces this "
                 "process, so the temp file would never be cleaned up. "
                 "Use --stream instead — it forks, inherits stdio, and deletes the file "
                 "when the child exits.")
        return 2

    env = _child_env(args.vault)
    secret_values: list[str] = []
    # The access record's payload, accumulated beside `secret_values` in the same three
    # loops so a route that resolves a value and forgets to name it is a visible omission
    # rather than a silent one (#441). Names only — see `_trace_secret_use`.
    key_names: list[str] = []
    env_names: list[str] = []
    tmpfiles: list[str] = []
    # Everything below that can create a tmpfile — --file, --dotenv, and the
    # subprocess.run call that consumes them — is one `try` with a single
    # `finally` at the bottom. That's the whole fix for "leaks a temp file on
    # an early return/exception": every exit from this point on (a `return`,
    # a handled VaultError/FileNotFoundError, or any *other* exception —
    # FileNotFoundError from mkstemp on a key with '/', UnicodeEncodeError,
    # ENOSPC, KeyboardInterrupt, ...) unwinds through the same `finally` and
    # unlinks every tmpfile created so far. No call site below needs its own
    # cleanup loop; do not add one.
    #
    # The `finally` unwinds an EXCEPTION. A signal whose default action is "terminate"
    # unwinds nothing at all, and Python installs a handler for exactly one of them
    # (SIGINT -> KeyboardInterrupt). So SIGTERM and SIGHUP used to kill charter with the
    # 0600 file still on disk — measured: `--stream --file F=K -- sh -c 'sleep 30'`,
    # SIGTERM at t+2s, survivor `charter-<v>-<k>-…` at `-rw-------` holding the value.
    # SIGTERM is the ORDINARY termination (a supervisor, a `kill`, a harness reaping a
    # hung tool call) and `--stream` exists for long-running children, which are exactly
    # what gets SIGTERMed at shutdown. `sys.exit` turns the signal back into an exception
    # so the `finally` below runs; `subprocess.run` kills and reaps its child on the way
    # out. Installed before the first tmpfile is created and restored after the last one
    # is gone, so charter's signal behaviour outside this block is unchanged.
    restore_signals = _exit_on_termination()
    try:
        try:
            for spec in args.env or []:
                name, sep, key = spec.partition("=")
                if not sep or not name:
                    util.err(f"--env expects NAME=key, got '{spec}'")
                    return 2
                val = prov.get(key)
                env[name] = val
                secret_values.append(val)
                key_names.append(key)
                env_names.append(name)
            for spec in args.file or []:
                name, sep, key = spec.partition("=")
                if not sep or not name:
                    util.err(f"--file expects ENVVAR=key, got '{spec}'")
                    return 2
                val = prov.get(key)
                secret_values.append(val)
                key_names.append(key)
                env_names.append(name)
                fd, path = tempfile.mkstemp(prefix=f"charter-{args.vault}-{key}-")
                # Register for cleanup *before* writing: a failure mid-write
                # (ENOSPC, EIO, a lone surrogate in the value) would otherwise
                # strand a 0600 file the `finally` never learns about.
                tmpfiles.append(path)
                with os.fdopen(fd, "wb") as fh:
                    fh.write(val.encode())
                os.chmod(path, 0o600)
                env[name] = path

            # --dotenv ENVVAR=NAME:key (repeatable). Entries sharing an ENVVAR
            # are merged into one file, in flag order, so a consumer that
            # wants several secrets (e.g. PLAYWRIGHT_MCP_SECRETS_FILE) gets
            # exactly one path.
            grouped: dict[str, list[tuple[str, str]]] = {}
            for spec in dotenv_specs:
                envvar, sep, entry = spec.partition("=")
                name, csep, key = entry.partition(":")
                if not sep or not envvar or not csep or not name or not key:
                    util.err(f"--dotenv expects ENVVAR=NAME:key, got '{spec}'")
                    return 2
                if any(n == name for n, _ in grouped.get(envvar, ())):
                    util.err(f"--dotenv defines '{name}' twice for {envvar}; "
                             "which value wins would be up to the reader of "
                             "the file. Use one entry per name.")
                    return 2
                grouped.setdefault(envvar, []).append((name, key))

            for envvar, entries in grouped.items():
                lines = []
                env_names.append(envvar)
                for name, key in entries:
                    val = prov.get(key)
                    secret_values.append(val)
                    key_names.append(key)
                    # Tier 3 writes an escaped form; redaction must match what
                    # is actually in the file, not just the raw value.
                    escaped = val.replace("\r", "\\r").replace("\n", "\\n")
                    if escaped != val:
                        secret_values.append(escaped)
                    try:
                        lines.append(_dotenv_line(name, val))
                    except ValueError as e:
                        util.err(str(e))
                        return 2
                fd, path = tempfile.mkstemp(prefix=f"charter-{args.vault}-dotenv-")
                # Register before writing — see the note on the --file path above.
                tmpfiles.append(path)
                with os.fdopen(fd, "wb") as fh:
                    fh.write(("\n".join(lines) + "\n").encode())
                os.chmod(path, 0o600)
                env[envvar] = path
        except base.VaultError as e:
            util.err(str(e))
            return 1

        # ONE site, above the branch, and that placement is the whole of the guarantee.
        # Three modes launch the child below — `execvpe`, a streaming `subprocess.run`, a
        # capturing one — and a record placed inside them would be three records kept in
        # step by hand, with a fourth mode arriving unrecorded. Everything that runs a
        # command passes through here.
        #
        # Above `execvpe` for a second reason: exec REPLACES this process, so a record
        # written after it is a record that is never written. And above all three because
        # the event being recorded is "charter handed these credentials to this command",
        # which is already true at this line — a child that segfaults on its first
        # instruction still received them.
        #
        # A run that resolves NOTHING is still recorded, with empty name lists. `secret
        # exec` with no `--env`/`--file`/`--dotenv` hands out no credential, but it does
        # run a command inside charter's vault machinery, and a trace that showed the
        # credentialed runs and hid the others would answer "what did this vault do" with
        # a filtered list that looks whole.
        _trace_secret_use(
            "secret-exec", secret_values,
            vault=args.vault,
            key_names=sorted(set(key_names)),
            env_names=sorted(set(env_names)),
            argv0=command[0],
            mode="exec" if exec_mode else ("stream" if stream_mode else "capture"),
        )

        if exec_mode:
            # Replaces this process: stdio is inherited untouched, so a
            # streaming child (MCP stdio server, REPL, tail -f) works. Never
            # returns on success. (tmpfiles is always empty here — --exec is
            # rejected above whenever --file/--dotenv is also given.)
            try:
                os.execvpe(command[0], command, env)
            except OSError:
                util.err(f"command not found: {command[0]}")
                return 127
            return 0  # unreachable: execvpe replaces the process or raises.
                      # Never fall through to the capturing path below — that
                      # would run the command a second time.

        if stream_mode:
            # Fork rather than exec, and do NOT capture: the child inherits this process's
            # stdio, so an MCP stdio server streams exactly as it does under --exec. This
            # process stays alive for one reason — the `finally` that deletes the temp files
            # once the child exits.
            #
            # The honest limit, stated here and in --help rather than implied away. Exit,
            # an exception, and every terminating signal charter may catch all clean up:
            # SIGINT via KeyboardInterrupt, and SIGTERM, SIGHUP, SIGQUIT, SIGUSR1/2,
            # SIGXCPU and the rest via the handlers installed above (see
            # `_SIGNALS_LEFT_ALONE` for what is excluded and why).
            #
            # Two things still leave the 0600 file in the system temp directory until it
            # is removed or the machine reboots. SIGKILL, which no process can catch. And
            # a fault — SIGSEGV, SIGBUS, SIGABRT — which charter deliberately does not
            # intercept, because a handler running on a process whose state is already
            # wrong can turn a crash into a hang. That is strictly better than the
            # alternative it replaces (every persona re-implementing the same trap in
            # shell, with the same hole) but it is not a guarantee, and describing it as
            # one would fail silently at the moment something has already gone wrong.
            #
            # This sentence has been wrong twice. First it said SIGKILL when SIGTERM leaked
            # too; then it said SIGKILL again once SIGTERM and SIGHUP were handled, while
            # SIGQUIT — Ctrl-\ — still leaked. Both times the list was of signals to catch.
            # It is now a list of signals to leave alone, which is why this text can name
            # the limit as a category rather than as three examples.
            #
            # Output is NOT redacted, for the same reason as --exec: nothing is captured.
            try:
                proc = subprocess.run(command, env=env)
            except FileNotFoundError:
                util.err(f"command not found: {command[0]}")
                return 127
            return proc.returncode

        try:
            proc = subprocess.run(command, env=env, text=True,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        except FileNotFoundError:
            util.err(f"command not found: {command[0]}")
            return 127

        out = base.redact(proc.stdout, secret_values)
        err = base.redact(proc.stderr, secret_values)
        if out:
            sys.stdout.write(out)
        if err:
            sys.stderr.write(err)
        return proc.returncode
    finally:
        for p in tmpfiles:
            _safe_unlink(p)
        restore_signals()


#: Signals charter deliberately does NOT take over, and why. Everything else in
#: `signal.Signals` defaults to terminating the process without unwinding, which is the
#: property that leaves a 0600 credential file behind.
#:
#: Written as an EXCLUSION list, not as a list of signals to handle. The first version
#: handled SIGTERM and SIGHUP and the docs then called SIGKILL the whole of the limit —
#: false, because SIGQUIT (Ctrl-\), SIGUSR1, SIGXCPU and the rest each left the file
#: exactly as SIGTERM had. A hand-written list of what to catch is a list of spellings,
#: and the next signal nobody thought of is not on it; a list of what to leave alone is
#: a list of reasons, and anything new lands on the safe side of it by default.
_SIGNALS_LEFT_ALONE = frozenset(
    name for name in (
        # Cannot be caught at all. SIGKILL is the honest limit; SIGSTOP only suspends.
        "SIGKILL", "SIGSTOP",
        # Default action is ignore or resume — nothing to unwind.
        "SIGCHLD", "SIGCLD", "SIGCONT", "SIGURG", "SIGWINCH", "SIGINFO",
        # Default action suspends the process. Taking these over would break job
        # control: Ctrl-Z would kill the command instead of backgrounding it.
        "SIGTSTP", "SIGTTIN", "SIGTTOU",
        # Python already ignores SIGPIPE (`cli` turns BrokenPipeError into 141), and
        # Python's own SIGINT handler raises KeyboardInterrupt, which unwinds `finally`
        # already. Both are handled; neither should be handled twice.
        "SIGPIPE", "SIGINT",
        # Faults. The handler would run on a process whose state is already wrong, and
        # intercepting a crash to tidy up risks turning it into a hang. These are named
        # in the stated limit alongside SIGKILL rather than quietly caught.
        "SIGSEGV", "SIGBUS", "SIGILL", "SIGFPE", "SIGSYS", "SIGTRAP", "SIGEMT",
        "SIGABRT", "SIGIOT",
    )
)

#: Terminating signals whose default action skips every `finally` in this process.
_TERMINATION_SIGNALS = tuple(
    s for s in getattr(signal, "Signals", ()) if s.name not in _SIGNALS_LEFT_ALONE
)


def _exit_on_termination():
    """Turn every catchable terminating signal into ``SystemExit``; return its undo.

    A default-action termination runs no `finally`, so a 0600 credential file outlives
    the process that owns it. Raising `SystemExit` from the handler puts the death back
    on the ordinary exception path — including out of a blocking `subprocess.run`, whose
    own `except BaseException: process.kill()` reaps the child before re-raising — so the
    cleanup that runs is the same cleanup a normal exit runs.

    Exit code 128+N, the shell's convention for "died on signal N", so a supervisor still
    reads the death as a signal rather than as a clean exit.

    Not installable off the main thread (`signal.signal` raises `ValueError` there), and
    charter's own callers are free to embed it, so that case restores nothing rather than
    failing the command.
    """
    previous: list[tuple] = []
    for sig in _TERMINATION_SIGNALS:
        try:
            previous.append((sig, signal.signal(sig, _terminate_now)))
        except (ValueError, OSError):
            pass

    def restore() -> None:
        for sig, handler in previous:
            try:
                signal.signal(sig, handler)
            except (ValueError, OSError):
                pass

    return restore


def _terminate_now(signum, _frame):
    raise SystemExit(128 + signum)


def _safe_unlink(path: str) -> None:
    """Delete *path*, ignoring "already gone".

    It deletes. It does not overwrite, and no comment, help string or doc may imply that
    it does — the word for the stronger thing was used in six places and this function
    never did it (`tests/test_wording.py` keeps it that way). Do not add the overwrite
    pass either: it is meaningless on a copy-on-write filesystem (APFS, ext4 with a
    journal, any SSD doing wear levelling), where the bytes it rewrites land in a NEW
    block and the old one stays wherever the drive left it. The properties that do hold
    are that the file is 0600 and that it is short-lived.
    """
    try:
        os.unlink(path)
    except OSError:
        pass


_ENV_NAME_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
_ESCAPE_SEQ_RE = re.compile(r"\\[nr]")


def _dotenv_line(name: str, value: str) -> str:
    """Render one ``KEY=value`` dotenv line that ``dotenv.parse`` round-trips.

    The consuming parser is the ``dotenv`` package (Playwright's
    ``dotenvFileLoader`` calls ``dotenv.parse``), verified at v17.4.2 by
    exhaustive fuzz (30,940 values, 0 corrupted). Three tiers, tried in order
    — the first that applies wins:

    * **Tier 1 — single quotes.** Fully literal: dotenv processes no escape
      at all inside a single-quoted value. Unsafe in two cases: a ``#``
      combined with a ``'`` (a failed quote match falls back to *unquoted*
      parsing, where ``#`` starts a comment — silently truncating the
      value), and a ``'`` combined with a real newline.
    * **Tier 2 — backticks.** Also fully literal, and unlike single quotes
      they carry a real newline safely. Requires the value to be
      backtick-free.
    * **Tier 3 — double quotes.** The only tier that can carry a literal CR
      (dotenv normalises a raw CR to LF in every other quote style), via
      escaping: real CR -> ``\\r``, real LF -> ``\\n``. Requires no ``"`` in
      the value, and — since dotenv has no backslash escape and so never
      unescapes ``\\n``/``\\r`` back — no *literal* ``\\n``/``\\r`` sequence
      already in the value (that substitution is not injective: a value
      already holding the two characters ``\\`` + ``n`` would come back as a
      real newline).

    If none of the three applies, the value is genuinely unrepresentable in
    dotenv and this raises rather than silently corrupting a credential.
    """
    if not _ENV_NAME_RE.match(name):
        raise ValueError(
            f"'{name}' is not a valid environment-variable name "
            "(expected [A-Za-z_][A-Za-z0-9_]*)")

    has_cr = "\r" in value
    has_lf = "\n" in value
    has_sq = "'" in value
    # Tier 1 — single quotes: fully literal. Unsafe when '#' meets a quote (dotenv
    # falls back to unquoted parsing and strips from '#'), or when a quote meets a
    # real newline.
    if not has_cr and not ("#" in value and has_sq) and not (has_sq and has_lf):
        return f"{name}='{value}'"
    # Tier 2 — backticks: also fully literal, and unlike single quotes they carry a
    # real newline safely. Needs the value to be backtick-free.
    if not has_cr and "`" not in value:
        return f"{name}=`{value}`"
    # Tier 3 — double quotes, the only tier that can carry a CR (dotenv normalises a
    # literal CR to LF). Escaping makes it ambiguous if the value already holds a
    # literal \n or \r sequence.
    if '"' not in value and not _ESCAPE_SEQ_RE.search(value):
        body = value.replace("\r", "\\r").replace("\n", "\\n")
        return f'{name}="{body}"'
    # Name the tier that was actually exhausted, so the operator knows what to
    # look for in a value this message deliberately does not print.
    if "\r" in value:
        why = ("it combines a real carriage return with a double quote. Only a "
               "double-quoted value can carry a carriage return, and that tier "
               "cannot also contain a '\"'")
    elif _ESCAPE_SEQ_RE.search(value):
        why = ("it combines a real newline, a double quote and a literal "
               "'\\n'/'\\r' escape sequence, so the escape would be "
               "indistinguishable from the real newline")
    else:
        why = ("it contains '#', a single quote, a double quote and a backtick "
               "all at once, which leaves no usable quote style")
    raise ValueError(
        f"the secret for '{name}' cannot be represented in dotenv: {why}. "
        "Store the value base64-encoded instead. "
        "(Value withheld from this message.)")
