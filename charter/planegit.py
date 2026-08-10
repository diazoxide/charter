"""Git for the control plane itself: staging, the secret guard, committing, and pushing
over the plane's OWN forge's HTTPS token.

Extracted because there were two committers. `commands.commit_push` did it correctly —
`-c credential.helper=<forge>` and an HTTPS remote — while `commands_persona.
cmd_persona_memory_sync` had grown its own copy that ran `git push origin HEAD`, over SSH,
in violation of charter's headline one-credential rule, on the ONE memory path the
SessionStart hook explicitly tells an agent to use. Traced side by side against the same
plane:

    charter save          → push -c credential.helper=!gh auth git-credential https://…
    persona memory-sync   → push origin HEAD        (and "check git auth" while gh was fine)

The structural cause is visible in the import list it left behind: `commands_workspace`
already reached into a sibling COMMAND module for `_cred_flag`, `_git` and
`_origin_https`, so writing a second implementation looked cheaper than reusing the first
across that seam. With them here, reuse is the cheap path and there is one place where
"how charter commits to its own plane" is decided.

`commands` re-exports these names, so every existing caller and test is unchanged.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

from . import config, util


def _git(args, cwd=None):
    """Run git without raising, so callers can branch on the return code."""
    return util.run(["git", *args], cwd=cwd, check=False)


# Route git auth through THAT REPO'S OWN FORGE's token over HTTPS — no SSH keys, no
# 1Password agent (which is where all the signing/permission pain comes from). One
# credential PER FORGE: `!glab auth git-credential` for GitLab, `!gh auth git-credential`
# for GitHub, each forge's own git credential helper — git appends the get/store/erase
# operation. `_cred_flag` resolves this per call (never a single hardcoded forge), so a
# GitHub-hosted clone — like this control plane itself — authenticates correctly instead
# of silently being handed GitLab's helper.
def _cred_flag(forge) -> list[str]:
    """``-c credential.helper=<forge's>`` — makes ONE git invocation use *that forge's*
    token-holding CLI, regardless of what (if anything) local config already has. Belt
    and braces for the very first ``git clone``, before `gitpolicy.apply` has had a
    chance to write the repo's own local config."""
    return ["-c", f"credential.helper={forge.credential_helper()}"]


def _origin_https(root) -> str | None:
    """The control plane's own ``origin`` as an HTTPS URL (rewriting an SSH one via ITS
    forge's own rewrite rule — ``registry.resolve_host`` + ``insteadof()``), or ``None``
    when there's no origin yet, or its host isn't a forge this charter knows about — a
    DEFAULT host (gitlab.com/github.com) or one DECLARED in this control plane's own
    ``charter.toml`` (see ``gitpolicy.forge_for``, which resolves the same way). An
    unrecognised host deliberately returns ``None`` rather than guessing: `commit_push`
    then warns and skips the push instead of silently trying (and failing) against the
    wrong forge."""
    url = _git(["remote", "get-url", "origin"], cwd=root).stdout.strip()
    if not url:
        return None
    from .forge import registry
    forge = registry.resolve_host(url, config.ROOT)
    if forge is None:
        return None
    https_base, ssh_forms = forge.insteadof()
    if url.startswith(https_base):
        return url
    for prefix in ssh_forms:
        if url.startswith(prefix):
            return https_base + url[len(prefix):]
    return None


# --------------------------------------------------------------------------- #
# discover                                                                     #
# --------------------------------------------------------------------------- #
def _spawn_bg_push(root) -> None:
    """Fire a detached background push of HEAD (best-effort) so a slow push never blocks
    the turn — the same mechanism the workspace Stop-hook auto-save uses."""
    import subprocess
    import sys
    try:
        subprocess.Popen([sys.executable, "-m", "charter", "workspace", "_pushbg"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         stdin=subprocess.DEVNULL, start_new_session=True, cwd=str(root))
    except Exception:
        pass


def commit_memory_reactive(paths: list[str], title: str) -> int:
    """**Reactive memory**: how far the just-written memory file(s) travel is declared per
    control plane via ``config.MEMORY_SHARE`` (``charter.instance.SHARE_MODES``), defaulting
    to ``local`` — safe for a control plane a stranger might run, where nothing should reach
    a remote without a human between writing and disclosure:

    - ``local``  — stays on disk only; nothing is committed.
    - ``commit`` — committed locally (scoped + secret-scanned), never pushed.
    - ``push``   — committed locally and pushed in the BACKGROUND — so a memory reaches the
      shared repo the moment it's recorded, without blocking the turn. Best-effort.

    Returns commit_push's rc (0 = committed / nothing to do / posture is local,
    1 = a secret-shaped value was refused)."""
    from . import instance as _instance
    # Re-clamp defensively — see `instance.clamp_share`: `config.MEMORY_SHARE` is always
    # pre-clamped at import time, but this reactive path must not itself rely on that.
    share = _instance.clamp_share(config.MEMORY_SHARE)
    if share == "local":
        return 0
    msg = f"memory: {title}"[:100]
    if share == "commit":
        return commit_push(config.ROOT, ["add", "--", *paths], msg, no_push=True)
    return commit_push(config.ROOT, ["add", "--", *paths], msg, background=True)


def commit_push(root, add_cmd: list, message: str | None,
                sign: bool = False, no_push: bool = False, background: bool = False) -> int:
    """Stage (``add_cmd``) → secret-scan staged memory/refs → commit → push via the
    control plane's OWN FORGE's token (`gitpolicy.forge_for`; rebase-retry on non-ff).
    Shared by `charter save` and `charter workspace save` so the secret-guard + no-SSH
    push path is identical everywhere, on whichever forge the control plane lives on.

    ``add_cmd`` is git's ARGUMENTS, not a command line — `_git` supplies `git` itself.
    One caller passed `["git", "add", …]`, so it ran `git git add`, which fails; the
    staged-nothing check below then took the "Nothing to save" branch and returned 0, and
    the caller printed `✓ committed + pushed`. `charter version bump --push` had therefore
    never committed anything. Asserting the shape here rather than fixing the one caller,
    because the failure was invisible at every layer above it."""
    if add_cmd and add_cmd[0] == "git":
        raise ValueError(f"commit_push(add_cmd=…) takes git's arguments, not a command "
                         f"line — drop the leading 'git' from {add_cmd!r}")

    # A plane is not always a git repo: `charter init` in a fresh directory does not run
    # `git init`, and that is exactly the README's 60-second path. Every git call below
    # runs with check=False (this is reached from hooks and background paths that must
    # never break a turn), so without this the add failed silently, `diff --cached
    # --quiet` returned 128 rather than 0 so the "Nothing to save" branch was skipped,
    # the commit failed too, and `rev-parse --short HEAD` came back empty — printing
    # `✓ Committed : charter save: 0 file(s)` and exiting 0. The personas and memory
    # charter had just told the user to commit had no history at all.
    if _git(["rev-parse", "--git-dir"], cwd=root).returncode != 0:
        util.err(f"{root} is not a git repository, so there is nothing to commit to.")
        util.info("  charter init does not create one. Run: git init && git remote add "
                  "origin <url>  — personas and memory are meant to be committed and shared.")
        return 1

    _git(add_cmd, cwd=root)
    if _git(["diff", "--cached", "--quiet"], cwd=root).returncode == 0:
        util.info("Nothing to save — the control-plane working tree is clean.")
        return 0
    staged = [ln for ln in _git(["diff", "--cached", "--name-only"], cwd=root).stdout.splitlines() if ln.strip()]

    # Secret guard: refuse if a staged memory/ref file looks like it holds a secret.
    from .hooks import _secret_kind
    flagged = []
    for p in staged:
        if "/memory/" in p or "/refs/" in p:
            try:
                kind = _secret_kind((root / p).read_text())
            except OSError:
                kind = None
            if kind:
                flagged.append((p, kind))
    if flagged:
        util.err("Refusing to save — a secret-shaped value in a memory/ref file:")
        for p, k in flagged:
            util.err(f"  {p}  ({k})")
        util.info("Secrets belong in the vault (`charter persona secret set`). Remove it, then retry.")
        return 1

    msg = message or f"charter save: {len(staged)} file(s)"
    # Unsigned by default so the 1Password signer never hangs; --sign to opt in.
    signcfg = [] if sign else ["-c", "commit.gpgsign=false"]
    _git([*signcfg, "commit", "-q", "-m", msg], cwd=root)
    if _git(["diff", "--cached", "--quiet"], cwd=root).returncode != 0:  # signed commit failed
        _git(["commit", "--no-gpg-sign", "-q", "-m", msg], cwd=root)
    # Still staged after both attempts = nothing was committed. Reporting success here is
    # how a failed commit became `✓ Committed :` with an empty sha — the sha was empty
    # precisely BECAUSE there was no commit, and that was the only visible symptom.
    if _git(["diff", "--cached", "--quiet"], cwd=root).returncode != 0:
        util.err(f"git commit failed — {len(staged)} file(s) are staged but not committed.")
        util.info("  Run `git -C {} status` to see why; charter has left them staged."
                  .format(root))
        return 1
    short = _git(["rev-parse", "--short", "HEAD"], cwd=root).stdout.strip()
    util.ok(f"Committed {short}: {msg}  ({len(staged)} file(s))")

    if no_push:
        util.info("Skipped push (--no-push).")
        return 0
    branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=root).stdout.strip()
    https = _origin_https(root)
    if not https:
        util.warn("origin isn't on a forge charter knows (gitlab.com/github.com/…) — "
                  "committed locally; push manually.")
        return 0
    if background:
        _spawn_bg_push(root)
        util.info("→ pushing to the control plane in the background.")
        return 0

    from . import gitpolicy
    forge = gitpolicy.forge_for(root)
    cred = _cred_flag(forge)

    def push():
        return _git([*cred, "push", https, f"HEAD:{branch}"], cwd=root)

    p = push()
    if p.returncode != 0 and any(s in (p.stderr or "") for s in ("fetch first", "non-fast-forward", "rejected")):
        util.info("remote moved — fetching + rebasing, then retrying …")
        _git([*cred, "fetch", https, branch], cwd=root)
        if _git(["rebase", "FETCH_HEAD"], cwd=root).returncode != 0:
            _git(["rebase", "--abort"], cwd=root)
            util.warn("Committed locally, but rebase hit a conflict — resolve manually, then `charter save`.")
            return 0
        p = push()
    if p.returncode == 0:
        _git(["update-ref", f"refs/remotes/origin/{branch}", "HEAD"], cwd=root)  # sync tracking
        util.ok(f"Pushed {branch} via {forge.cli} (HTTPS token — no SSH, no 1Password).")
    else:
        util.warn(f"Committed, but the {forge.cli} push failed:")
        for ln in (p.stderr or p.stdout or "").splitlines()[-4:]:
            util.warn("  " + ln)
        util.info(f"Check `{forge.cli} auth status`.")
    return 0
