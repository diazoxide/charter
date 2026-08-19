"""``charter vault`` and ``charter secret`` command implementations.

The overriding rule here: **secret values must not leak into the model.**
- write paths take values via stdin/file, never argv (argv shows in ps/history);
- ``list`` shows keys only, ``get`` is masked by default;
- ``get --reveal`` refuses a non-interactive stdout (an agent) unless ``--force``;
- the real consumption paths are ``exec`` (inject + redact) and ``cp`` (0600 file).
"""

from __future__ import annotations

import hashlib
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

from . import config, util
from .secrets import base, registry


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
        util.info(f"  charter creates one 1Password item per secret, tagged "
                  f"'charter:{args.name}', in vault '{cfg['op-vault']}'.")
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
    util.ok(f"Set '{args.key}' in vault '{args.vault}' ({len(value)} bytes). Value not shown.")
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


def cmd_secret_get(args) -> int:
    try:
        value = _provider(args.vault).get(args.key)
    except base.VaultError as e:
        util.err(str(e))
        return 1

    if not args.reveal:
        digest = hashlib.sha256(value.encode()).hexdigest()[:12]
        print(f"{args.vault}/{args.key}: present · {len(value)} bytes · sha256:{digest}")
        print("(value hidden — use `charter secret exec`/`cp` to consume it, or --reveal to print)")
        return 0

    # --reveal: only for a human at an interactive terminal. Refuse the exact
    # channel (piped stdout) through which a secret would reach an agent's context.
    if not sys.stdout.isatty() and not args.force:
        util.err(
            "Refusing to print a secret to non-interactive stdout — this is how it would "
            "leak into an agent's context. Use `charter secret exec`/`cp` to consume it safely, "
            "or pass --force if you truly intend to print it."
        )
        return 2
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


def cmd_secret_cp(args) -> int:
    """Materialize a secret to a file at 0600 (e.g. a kubeconfig). Prints path only."""
    try:
        value = _provider(args.vault).get(args.key)
    except base.VaultError as e:
        util.err(str(e))
        return 1
    dest = Path(args.dest).expanduser()
    dest.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(str(dest), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as f:
        f.write(value)
    os.chmod(dest, 0o600)
    util.ok(f"Wrote '{args.vault}/{args.key}' to {dest} (0600). Value not shown.")
    return 0


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
    # `--exec` REPLACES this process, so nothing survives to shred a temp file. A
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
                 "Use --stream instead — it forks, inherits stdio, and shreds the file "
                 "when the child exits.")
        return 2

    env = dict(os.environ)
    secret_values: list[str] = []
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
            for spec in args.file or []:
                name, sep, key = spec.partition("=")
                if not sep or not name:
                    util.err(f"--file expects ENVVAR=key, got '{spec}'")
                    return 2
                val = prov.get(key)
                secret_values.append(val)
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
                for name, key in entries:
                    val = prov.get(key)
                    secret_values.append(val)
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
            # process stays alive for one reason — the `finally` that shreds the temp files
            # once the child exits.
            #
            # The honest limit, stated here and in --help rather than implied away: a
            # SIGKILLed parent runs no cleanup, so the 0600 file survives in the system temp
            # directory until it is removed or the machine reboots. That is strictly better
            # than the alternative it replaces (every persona re-implementing the same trap
            # in shell, with the same hole) but it is not a guarantee, and describing it as
            # one would fail silently at the moment something has already gone wrong.
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


def _safe_unlink(path: str) -> None:
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
