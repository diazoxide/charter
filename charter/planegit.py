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

import json
import os
import subprocess
import time
from pathlib import Path
from typing import NamedTuple

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
    the turn — the same mechanism the workspace Stop-hook auto-save uses
    (`commands_workspace._spawn_pushbg`). `util.self_relaunch_argv` (#390) is what keeps
    the child from importing whatever `charter/` package happens to sit under *root*
    instead of the installed one."""
    try:
        subprocess.Popen(util.self_relaunch_argv("workspace", "_pushbg"),
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


#: Signatures a forge uses to say "this branch requires a pull request". Matched, never
#: interpolated, and each one observed rather than imagined — ADR 0009's rule: charter may
#: name a cause it RECOGNISED, never one it inferred. An unmatched rejection falls through
#: to the generic "push failed" warning, which costs precision and can never mislead.
_PROTECTED_SIGNATURES = (
    "protected branch",              # GitHub + GitLab both use this wording
    "GH006",                         # GitHub's protected-branch push rejection code
    "pre-receive hook declined",     # generic server-side policy hook
    "required status check",
    "merge_request",                 # GitLab: "you are not allowed to push … open a MR"
    "not allowed to push",
)


def _is_protected_rejection(stderr: str) -> bool:
    blob = (stderr or "").lower()
    return any(s.lower() in blob for s in _PROTECTED_SIGNATURES)


def _compare_url(https: str, branch: str) -> str | None:
    """A one-click "open a pull request for this branch" URL.

    A plain HTTPS link, deliberately: charter has no PR-creation capability in any forge
    adapter, and this closes the PR-gated workflow **without** adding one — no API call, no
    extra token scope, no new adapter surface.
    """
    base = (https or "").removesuffix(".git")
    if not base.startswith("https://"):
        return None
    if "github.com" in base:
        return f"{base}/compare/{branch}?expand=1"
    # GitLab, and self-hosted GitLab, use the same new-MR form.
    return (f"{base}/-/merge_requests/new?merge_request%5Bsource_branch%5D={branch}")


#: What a push of the plane root's HEAD did, in one word. Recorded as well as printed —
#: see :func:`record_push` — so the answer survives a pusher nobody was listening to.
PUSHED = "pushed"              # it landed on the branch HEAD is on
BRANCHED = "branched"          # the branch requires a PR → it landed on `charter/<sha>`
STRANDED = "stranded"          # the branch requires a PR and THAT push failed too
FAILED = "failed"              # an ordinary push failure, reported rather than diagnosed
CONFLICT = "conflict"          # the remote moved and the rebase onto it conflicted
UNREACHABLE = "unreachable"    # no origin on a forge charter knows — nothing to push to


class PushResult(NamedTuple):
    """What :func:`push_head` did, in a shape both a human and `doctor` can read.

    ``branch`` is the branch charter tried to advance (the plane root's HEAD). ``landed``
    is the remote branch the commit actually reached when that is a *different* one, which
    is the distinction #373 turns on: "it is on the remote under another name" and "it is
    on this laptop only" are one exit code apart and worlds apart in consequence.
    """
    outcome: str
    branch: str
    landed: str | None = None
    url: str | None = None
    detail: str = ""


def push_record_path():
    """Where the push record lives — public because `doctor` NAMES it, which is what gives
    the recorded ``detail`` (git's own words about a push nobody heard) a reader.

    Read from :mod:`charter.config` at CALL time, never bound at import — the test harness
    re-points the plane with `config.use`, and a path captured at import would write into
    the developer's real ``.charter/``."""
    return config.STATE_DIR / "plane-push.json"


def record_push(res: PushResult, head: str = "") -> PushResult:
    """Write down what a push of the plane root's HEAD did, and return *res* unchanged.

    **Why a file and not a printed line.** The reactive-memory push is the one path
    SessionStart tells an agent to use, and it runs DETACHED with stdout and stderr on
    ``/dev/null`` (`_spawn_bg_push`) so a slow push cannot block the turn. That process has
    no caller to tell. Before #373 it therefore told nobody: a push rejected by a protected
    `main` returned 0 into the void, and the memory commit sat on a local branch until the
    next ``git reset --hard origin/main`` — the standard move on noticing a divergence —
    deleted it without a trace.

    A record is the honest shape for that. Charter's house rule is to NAME a limit rather
    than degrade quietly, and the limit here is real: a background push cannot report to a
    caller that has already returned. So it reports to the next `doctor` instead
    (`doctor._stranded_push`), which is the surface ADR 0008 already chose for the plane
    root.

    ``head`` is the commit the outcome is ABOUT, and it is what lets the reader of the
    record check it against the world instead of trusting it: `doctor` treats a record
    whose commit has since become an ancestor of the upstream as spent. Without it the
    file could only be believed, and a stale believed file is the failure ADR 0013 names.

    A clean push DELETES the file rather than writing "pushed": the record exists to carry
    a condition that outlives the process, and there is no condition left to carry.

    **ADR 0011 is the ADR that governs whether this file may exist at all**, and it is
    cited here rather than left for a reader to notice is missing. Its rule is that a record
    holds only what git cannot know, and that what is written is the PAST tense — never a
    "current status" field, because a description of how things are can become false while
    nobody is looking. What is written here is a past observation: *a push of commit
    ``head`` at time ``at`` came out this way*. There is no reality it can contradict,
    because the push happened in a process with ``/dev/null`` for a voice and nothing else
    recorded it. That is the same carve-out ADR 0011 makes for liveness, which is likewise
    overwritten rather than appended.

    The ADR's other demand is that the present tense be RECONSTRUCTED at read time by
    joining the record against git, and that is what ``head`` is for: `doctor._stranded_push`
    joins on it (:func:`is_spent`) and reports nothing once git says the commit reached the
    upstream. Without that join this would be exactly the marker the ADR forbids.

    Two residual tensions, stated rather than hidden. ``branch`` and ``url`` are derivable
    — the first from the root's HEAD, the second from ``origin`` plus ``landed`` — and
    ADR 0011's forbidden list names "which branch a piece is on" outright. They are kept
    because they are derivable *now* and the record is about *then*: an origin that has
    since been re-pointed (which is how the `unreachable` overwrite above was found) would
    re-derive a URL for a push that never went there. Deriving them would make the record
    agree with a present that is not the one it describes.

    Never raises. Every caller is a push, and a push that cannot write a note must still
    have pushed.
    """
    p = push_record_path()
    try:
        if res.outcome == PUSHED:
            p.unlink(missing_ok=True)
            return res
        config.private_mkdir(p.parent)
        p.write_text(json.dumps({
            "outcome": res.outcome, "branch": res.branch, "landed": res.landed,
            "url": res.url, "detail": res.detail, "head": head, "at": time.time(),
        }, indent=2))
    except OSError:
        pass
    return res


def push_record() -> dict | None:
    """The last recorded push outcome, or ``None`` when there is nothing to report.

    ``None`` for a missing file AND for an unreadable or malformed one, deliberately: the
    only consumer runs from the SessionStart hook, where a hook may cost a session its
    briefing and must never cost it the turn. A defect in this file is charter's own to
    fix, not a reason to take the session down."""
    try:
        data = json.loads(push_record_path().read_text())
    except (OSError, ValueError):
        return None
    return data if isinstance(data, dict) and data.get("outcome") else None


def is_spent(head: str, run) -> bool:
    """Has the commit a record is ABOUT since reached the tracked upstream?

    The join ADR 0011 demands: a record is a past observation, and the present tense is
    reconstructed by asking git rather than by believing the file. Once ``head`` is an
    ancestor of ``@{upstream}`` the condition the record describes is over, whatever the
    file still says — so `doctor` stops warning and :func:`_land_via_branch` stops reusing
    the pull-request branch it named.

    ONE definition, two callers, because a "spent" the reporter and the pusher disagreed
    about is how a warning that cannot clear itself and a branch that is reused after its
    pull request merged both arrive. *run* takes git's arguments and returns something with
    a ``returncode``; it is a parameter rather than a call because `doctor` must run this
    under its SessionStart timeout budget (`doctor._git_in`) and the pusher must not.

    ``--is-ancestor`` exits 0 for yes, 1 for no, and non-zero-not-1 for a ref it cannot
    resolve — a plane with no tracking branch, say. Only a clean 0 counts: "I could not
    check" must never read as "it landed", which is rule 1 of ADR 0013 in the one place
    where getting it wrong loses the memory."""
    if not head:
        return False
    return run(["merge-base", "--is-ancestor", head, "@{upstream}"]).returncode == 0


def unlanded(run) -> dict | None:
    """The recorded push outcome that is STILL true, or ``None``.

    **One decision, three renderings.** `doctor._stranded_push` turns this into a row with
    a remedy, `statusline._plane_root_alert` into one word that fits beside "dirty", and
    :func:`_open_pull_request_branch` into "may I still advance that branch". #373 was two
    implementations of *pushing*; letting three surfaces each decide for themselves whether
    a record is still worth acting on is the same mistake with a longer fuse — the one that
    disagrees is always the one nobody was looking at.

    *run* takes git's arguments and returns something with a ``returncode``; see
    :func:`is_spent` for why the runner is the caller's to choose.
    """
    rec = push_record()
    if not rec:
        return None
    return None if is_spent(str(rec.get("head") or ""), run) else rec


def _open_pull_request_branch(root) -> str | None:
    """The ``charter/<sha>`` branch an earlier reactive push is STILL waiting on a pull
    request for, or ``None`` — the branch :func:`_land_via_branch` should advance instead
    of minting a new one.

    Before #373 the reactive path pushed nothing, so #167's one-branch-per-rejection shape
    was a handful of branches from a human typing `charter save`. Routing `persona
    remember`, `workspace remember`, dispatch backfill, curate and autosave through the
    same path makes it one abandoned remote branch PER MEMORY, each superseding the last
    and none of them referenced by anything charter will ever say again. Each new HEAD is a
    descendant of the one before, so advancing the recorded branch is a fast-forward: one
    open pull request that accumulates the memory commits, rather than N branches nobody
    will ever close.

    Two things keep this safe rather than clever. It is only ever offered as a FIRST
    attempt — the caller pushes it without ``--force``, so git itself refuses anything that
    is not a fast-forward and the caller falls back to a fresh name; nothing here has to be
    right about the remote's tip. And a record whose commit has reached the upstream is
    :func:`is_spent`, so a merged pull request's branch is never resurrected.
    """
    rec = unlanded(lambda args: _git(args, cwd=root))
    if not rec or rec.get("outcome") != BRANCHED:
        return None
    landed = rec.get("landed")
    return landed if landed and isinstance(landed, str) else None


def _land_via_branch(root, https: str, cred: list, default_branch: str,
                     announce: bool = True) -> PushResult:
    """Push the commit that is already on HEAD to a NEW remote branch, and hand back a URL.

    The sanctioned path for a control plane whose own repo requires pull requests (#167).
    charter's guidance is that control-plane content is committed with `charter save`,
    straight to the default branch — which works only where a direct push lands. On a
    protected `main` it cannot, and 0.30.0's guard (#157) refuses the obvious workaround of
    branching the plane root. The operator was left making the same edit twice and
    discarding one.

    **The plane root's HEAD never moves.** That is what keeps this compatible with the guard
    instead of poking a hole in it: `git push HEAD:refs/heads/<new>` needs no checkout, no
    branch creation and no worktree — only a different *remote* ref for a commit that
    already exists locally. (A throwaway worktree was the first design; it turned out to be
    machinery for a problem that does not exist, since nothing here needs a second working
    tree.)

    The cost, stated rather than hidden: local `<default_branch>` is left one commit ahead
    of the remote until the pull request lands, and the caller says so with the command that
    reconciles it.

    **An open pull request is advanced rather than replaced.** The branch this pushed last
    time is tried FIRST while its record is still live (:func:`_open_pull_request_branch`),
    without ``--force``, so git refuses anything that is not a fast-forward and a fresh
    ``charter/<sha>`` is minted instead. That is what keeps the reactive path from leaving
    one abandoned remote branch per memory — see that function for why this only became a
    problem once the reactive push existed at all.

    ``announce`` is False for the background pusher, whose stdout and stderr are
    ``/dev/null``. It still returns the same `PushResult`, because that is what `doctor`
    reads back — saying it and recording it are two audiences, not two policies.
    """
    sha = _git(["rev-parse", "--short", "HEAD"], cwd=root).stdout.strip() or "change"
    fresh = f"charter/{sha}"
    reuse = _open_pull_request_branch(root)
    branch = fresh
    p = None
    # A reuse that did not fast-forward is not a failure to report: the fresh name has not
    # been tried yet, and naming a branch charter chose on the operator's behalf as the
    # thing that went wrong would send them after a problem they do not have.
    for candidate in ([reuse] if reuse and reuse != fresh else []) + [fresh]:
        branch = candidate
        p = _git([*cred, "push", https, f"HEAD:refs/heads/{candidate}"], cwd=root)
        if p.returncode == 0:
            break
    if p is None or p.returncode != 0:
        tail = "\n".join((p.stderr or p.stdout or "").splitlines()[-4:]) if p else ""
        if announce:
            util.err(f"'{default_branch}' requires a pull request, and pushing the branch "
                     f"'{fresh}' also failed:")
            for ln in tail.splitlines():
                util.err("  " + ln)
        return PushResult(STRANDED, default_branch, detail=tail)
    url = _compare_url(https, branch)
    if announce:
        util.ok(f"'{default_branch}' requires a pull request — pushed {branch} instead.")
        if url:
            util.info(f"  open it: {url}")
        util.info(f"  the commit is also on your local {default_branch}, one ahead of the "
                  f"remote. After the PR merges: git -C {root} pull --rebase")
    return PushResult(BRANCHED, default_branch, landed=branch, url=url)


def push_head(root, announce: bool = True) -> PushResult:
    """Push the plane root's HEAD to its own branch on origin — **the one pusher**.

    Extracted from `commit_push` for the reason this module exists one layer up: there were
    two committers, and the copy was the one breaking the rule. #373 was that same fault
    repeated on the push. `commands_workspace.cmd_workspace_pushbg` — the detached
    background half of every reactive memory commit — had grown its own push and
    rebase-retry and stopped there, with no protected-branch recognition and ``return 0``
    on every failure. So on a plane whose `main` requires a pull request, `charter save`
    landed the change on `charter/<sha>` (#167) while `charter persona remember` stranded
    it on local `main` and said nothing:

        charter save            → rejected → _land_via_branch → PR URL printed
        persona remember (bg)   → rejected → return 0

    With one implementation the protected-branch policy is true on both paths by
    construction, rather than by two lists of forge signatures somebody keeps in step.

    **Why the branch is never predicted.** Nothing here asks the forge whether `main` is
    protected before committing, which is the first thing #373 proposes. charter cannot
    know that without a network call it has no business making from a Stop hook, and
    guessing it from the branch name is precisely the unearned diagnosis ADR 0009 forbids.
    The rejection IS the evidence, and it arrives only after the commit exists — so the
    commit is made, and the *outcome* is what gets reported honestly.

    Always records its outcome (:func:`record_push`), foreground and background alike. The
    foreground caller also has a human in front of it, so it *says* it too — but a printed
    line is not a record, and `charter save` on a protected plane leaves a pull request to
    open whether or not the operator was reading at the time.
    """
    branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=root).stdout.strip()
    head = _git(["rev-parse", "HEAD"], cwd=root).stdout.strip()
    https = _origin_https(root)
    if not https:
        if announce:
            util.warn("origin isn't on a forge charter knows (gitlab.com/github.com/…) — "
                      "committed locally; push manually.")
        # Returned UNRECORDED, deliberately. `unreachable` is the one outcome
        # `doctor._stranded_push` never reports — a plane with no origin charter knows has a
        # CONFIGURATION to fix, not a commit to rescue — so writing it could only ever
        # DESTROY a real notice, and it did. `cmd_workspace_autosave` reaches
        # `_spawn_pushbg` through `commit_push(no_push=True)`, which returns before the
        # `_origin_https` pre-check in `commit_push`, so re-pointing origin at an
        # unrecognised host and letting the background child run turned
        #     warn | a memory commit was committed but never pushed, 1 ahead of origin/main
        # into
        #     ok   | clean on main, 1 ahead of origin/main
        # over a commit that existed nowhere but that laptop — measured against a real bare
        # remote with a real pre-receive hook. A "nothing to report" outcome must never be
        # able to erase one that had something to report.
        return PushResult(UNREACHABLE, branch)

    from . import gitpolicy
    forge = gitpolicy.forge_for(root)
    cred = _cred_flag(forge)

    def push():
        return _git([*cred, "push", https, f"HEAD:{branch}"], cwd=root)

    p = push()
    if p.returncode != 0 and _is_protected_rejection(p.stderr or p.stdout or ""):
        # Asked BEFORE the non-fast-forward retry, not after, and that ordering is load-
        # bearing rather than tidy. git prints its own `! [remote rejected]` line above a
        # server-side hook decline, so the word "rejected" in the retry test below matches a
        # protected branch too — and taking the retry first was not merely a wasted round
        # trip. Measured against a real bare remote with a real pre-receive hook refusing
        # refs/heads/main in GitHub's GH006 wording: fetching an origin that is unreachable
        # fails, which REMOVES FETCH_HEAD, so `rebase FETCH_HEAD` fails, and the outcome was
        # recorded as `conflict` — #167's pull-request path never reached, no `charter/<sha>`
        # branch created, the commit genuinely stranded, and charter telling the operator to
        # resolve a conflict that did not exist.
        return record_push(_land_via_branch(root, https, cred, branch, announce), head)
    if p.returncode != 0 and any(s in (p.stderr or "") for s in ("fetch first", "non-fast-forward", "rejected")):
        if announce:
            util.info("remote moved — fetching + rebasing, then retrying …")
        if _git([*cred, "fetch", https, branch], cwd=root).returncode != 0:
            # A failed fetch leaves NO FETCH_HEAD to rebase onto, so rebasing anyway fails
            # for a reason that has nothing to do with a conflict. Calling that `conflict`
            # is the unearned diagnosis ADR 0009 forbids, and it costs more than precision:
            # it sends the reader to resolve a conflict that does not exist while the real
            # answer — the push failure below, in git's own words — is discarded.
            if announce:
                util.warn("Could not reach origin to fetch, so the retry was skipped.")
        elif _git(["rebase", "FETCH_HEAD"], cwd=root).returncode != 0:
            _git(["rebase", "--abort"], cwd=root)
            if announce:
                util.warn("Committed locally, but rebase hit a conflict — resolve manually, then `charter save`.")
            return record_push(PushResult(CONFLICT, branch), head)
        else:
            # The rebase rewrote it, so the commit the record names is a different one now.
            head = _git(["rev-parse", "HEAD"], cwd=root).stdout.strip()
            p = push()
    if p.returncode == 0:
        _git(["update-ref", f"refs/remotes/origin/{branch}", "HEAD"], cwd=root)  # sync tracking
        if announce:
            util.ok(f"Pushed {branch} via {forge.cli} (HTTPS token — no SSH, no 1Password).")
        return record_push(PushResult(PUSHED, branch), head)
    if _is_protected_rejection(p.stderr or p.stdout or ""):
        # The plane's own repo requires a pull request. That is a supported shape (#167),
        # not a failure to report and abandon: without this the change is stranded in the
        # one tree #157 forbids branching, and the operator has to make the same edit again
        # somewhere else and discard this one.
        #
        # Not redundant with the identical check above the retry: the FIRST push can fail
        # plain non-fast-forward, and the SECOND — after a rebase that succeeded — is the
        # one the protected branch refuses. Same policy, reached from the other side.
        return record_push(_land_via_branch(root, https, cred, branch, announce), head)
    tail = "\n".join((p.stderr or p.stdout or "").splitlines()[-4:])
    if announce:
        util.warn(f"Committed, but the {forge.cli} push failed:")
        for ln in tail.splitlines():
            util.warn("  " + ln)
        util.info(f"Check `{forge.cli} auth status`.")
    return record_push(PushResult(FAILED, branch, detail=tail), head)


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
    # Asked here as well as inside `push_head`, and now for ONE reason only: on the
    # background path this is the only voice the operator can hear. `_spawn_bg_push` gives
    # the child `/dev/null` for stdout and stderr, so `push_head`'s identical warning is
    # written to be discarded, and a plane whose origin is on no forge charter knows would
    # say nothing at all about a configuration only a human can fix.
    #
    # It is NOT a guard against a spurious `unreachable` record any more. That was the
    # earlier justification and it was the wrong shape — `cmd_workspace_autosave` reaches
    # `_spawn_pushbg` through `commit_push(no_push=True)`, which returns above this line, so
    # the guard was bypassed by a sibling door and a real `branched`/`stranded` notice was
    # overwritten with "nothing to report". `push_head` no longer records `unreachable` at
    # all, which is the rule rather than a second place to remember it.
    if not _origin_https(root):
        util.warn("origin isn't on a forge charter knows (gitlab.com/github.com/…) — "
                  "committed locally; push manually.")
        return 0
    if background:
        _spawn_bg_push(root)
        # Says WHERE the answer will turn up, because this line cannot carry it: the push
        # happens in a detached child with `/dev/null` for a voice. `charter doctor` reads
        # what that child recorded (`record_push`). Before #373 this was the last word on a
        # push that could still be rejected seconds later, unheard — a success line about
        # something charter had not done yet, which is rule 1 of ADR 0013.
        util.info("→ pushing to the control plane in the background "
                  "(`charter doctor` reports the outcome).")
        return 0
    # rc 1 only when the commit reached NOWHERE — a pull-request branch that also failed.
    # An ordinary push failure has been reported and stays rc 0, unchanged: `charter save`
    # having committed successfully is not a failed command.
    return 1 if push_head(root).outcome == STRANDED else 0
