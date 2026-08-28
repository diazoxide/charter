"""`charter change` — the cross-repo change, as records. **No forge, no network.**

Seven verbs over :mod:`charter.change`: ``create``, ``add``, ``drop``, ``list``, ``show``,
``forget`` and ``revert``. Everything here is a file read, a file write, a directory
listing and — for ``revert`` and the divergence report — **git in a clone the operator
already has**. Nothing in this module opens a socket or runs a forge CLI, which is what
keeps "what did the operator declare, and what does this disk say happened" answerable
without asking anybody's API.

**Membership is enumerated by hand, and that is the containment property.** There is no
glob, no pattern, no ``--all-repos``, no "every repo in the workspace", and nothing here
calls ``inventory.list_repos``. Every member in a record was typed by somebody, and a member
must resolve to a clone the operator already put in this workspace — so the reach of a
change is bounded by what is already on the disk in front of you, and by nothing that
travelled in a committed file. A record can name a repository you do not have; it is refused
for that, by name, and it can never name a *place*.

**What this module will not do, in a change action, including during a revert** (§3.7):
force-push to any branch; delete any branch, merged or not; ``reset --hard`` a default
branch; or close a request charter did not open. Those are not absent by oversight and
they are not guarded by a flag — the argv is never constructed, and
`tests/test_change_revert.py` records every git invocation a revert makes and asserts it.
An emergency is exactly when somebody reaches for one of them, and a force-push over three
default branches leaves a world where the change happened, was undone, and no repository's
history mentions either — the failure the whole design exists to prevent.

**A branch out of a record is argv, and ref grammar is not argv safety.** Every git call
below that carries one spells it ``refs/heads/<branch>``, which cannot begin with ``-``
whatever the record says — `git check-ref-format` **accepts** ``refs/heads/-b`` (measured
on git 2.50.1), so the grammar answers a different question. That is the positional half;
`change.branch_refusal` at the record boundary is the other, and neither substitutes for
the other. It is spelled this way rather than with a trailing ``--`` because the commands
that need it — `merge-base --is-ancestor`, `rev-parse --verify` — take no ``--`` at all,
and a separator that is not accepted is a guard that is not there.
"""

from __future__ import annotations

import datetime
import os
import re
from pathlib import Path

from . import change, contain, instance, tui, util, workspace

#: Exit code for a **named refusal** — the request was understood and charter will not do
#: it — as distinct from the generic 1 that means something went wrong. `commands_worktree`
#: spends its 2 the same way (``CLAIM_TAKEN``) and for the same reason: a caller must not
#: have to parse English to tell "you asked for something I refuse" from "the disk is full".
#:
#: Four conditions share the code and **none shares a message**: a repo with no clone, a
#: change that does not exist, a member added twice, and an ordering that cannot be true.
#: Three gates in sequence mask each other, and an exit-code assertion cannot tell them
#: apart — so every refusal test here asserts *which* one fired.
REFUSED = 2

#: How long one read-only git question may take. `doctor.CHECK_TIMEOUT`'s reason, one
#: command out: a clone on a stalled network mount makes git hang, and a command the
#: operator is waiting on must fail rather than sit there.
GIT_TIMEOUT = 20

#: What a merge sha read out of the landing log must look like before it reaches git.
#:
#: **The log is a file, so it is untrusted for the same reason the record is.** It is
#: never committed, which makes it *local* rather than *trustworthy* — a hand edit, a
#: half-written line from a killed process, or an older charter all reach here — and the
#: value is handed to `git revert` and `git rev-list` as argv, where ``-X`` is a flag.
#: Anchored at both ends and hexadecimal, which is what a sha is; anything else is refused
#: by name rather than repaired, because a sha charter repaired is a commit nobody chose.
#:
#: **Asked with `fullmatch`, and the `^…$` is kept anyway.** Python's ``$`` matches at the
#: end of the string *or just before a trailing newline*, so `.match` would admit
#: ``"e0c9d13\n"`` — a value with a newline in it on its way to a git argv, which is the
#: class `tests/test_the_end_of_a_name_is_the_end_of_the_string.py` exists for. The anchors
#: are redundant under `fullmatch` and are load-bearing for something else: that test builds
#: its inventory by FINDING anchored constants, and unanchoring this would silently drop it
#: out of the table (#629).
_SHA_RE = re.compile(r"^[0-9a-f]{7,64}$")

#: The prefix of the branch a revert is seeded on. The revert is an ordinary change with an
#: ordinary name, so its branch is `change.default_branch` of its own slug — this exists
#: only to build that slug, and it is a constant so the record and the report cannot spell
#: it two ways.
REVERT_PREFIX = "revert-"


def _workspace(args) -> str:
    ws = workspace.resolve(getattr(args, "workspace", None))
    workspace.banner(ws, getattr(args, "workspace", None))
    return ws


def _now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")


def _author() -> str:
    """Who is recorded as having created this change.

    First line only, and contained: the value comes out of ``git config``, which is a file
    on this machine that charter did not write, and it lands in a record that a LIVE
    workspace commits and that every ``show`` prints back.
    """
    out = util.run(["git", "config", "user.name"], check=False).stdout or ""
    name = contain.one_line((out.splitlines() or [""])[0].strip())
    return name or os.environ.get("USER") or "unknown"


def _why(args, tail: str) -> str | None:
    """The ``--why`` this command was given, or ``None`` with the refusal printed.

    Two checks, and argparse can make neither. ``required=True`` cannot see ``--why ""``,
    and it cannot see a newline — which is the one that matters, because this value is
    repeated back on report rows and, later, written into pull request bodies where a
    newline forges a second row. A ``why`` that cannot be one line is prose, and prose
    belongs in ``workspace.md``.
    """
    why = (getattr(args, "why", None) or "").strip()
    if not why:
        util.err(f"--why is required: one line saying {tail}.")
        return None
    # `change.TEXT_LIMIT`, not `contain`'s row budget: the record's own bound and this one
    # have to be the same number, or a `why` charter accepts here is a record charter then
    # refuses to read back. Where it is drawn, `contain.one_line` clips it to a row.
    if contain.one_line(why, limit=change.TEXT_LIMIT) != why:
        util.err("--why must be one plain line — it is repeated back on a report row and "
                 "written into a pull request body. Longer reasoning belongs in "
                 "workspace.md, which is where this plane keeps prose.")
        return None
    return why


def _load(ws: str, slug: str) -> tuple[dict | None, int]:
    """``(record, 0)``, or ``(None, code)`` with the refusal already printed.

    Two different failures, two different codes. *No such change* is a refusal of what was
    asked (:data:`REFUSED`); a record that exists and does not parse is a defect in a file
    somebody has to go and fix, which is an error (1). Collapsing them would tell an agent
    to create a change that is already there.
    """
    if not change.exists(ws, slug):
        util.err(f"no change {contain.readable(slug)} in workspace '{ws}'.")
        util.info("List them: charter change list"
                  f"  ·  create it: charter change create {contain.readable(slug)} --why \"…\"")
        return None, REFUSED
    try:
        return change.read(ws, slug), 0
    except change.RecordError as exc:
        util.err(str(exc))
        return None, 1


def _save(ws: str, slug: str, rec: dict) -> int:
    """Write *rec* back, turning either refusal into an exit code.

    :class:`change.RecordError` here is an ordering that cannot be true — the cycle gate —
    and it is a refusal of the request. :class:`contain.Refused` is a path charter must not
    write, which is an error about the plane rather than about the request.
    """
    try:
        change.write(ws, slug, rec)
    except change.RecordError as exc:
        util.err(str(exc))
        return REFUSED
    except contain.Refused as exc:
        util.err(str(exc))
        return 1
    return 0


def _clone_for(ws: str, repo: str) -> Path | None:
    """The clone this member resolves to, or ``None``.

    :func:`contain.child` is the single gate, and it refuses rather than sanitises —
    *"silently rewriting a name invents a second identity"*. It is what makes ``..``, an
    absolute path, a drive-qualified name and a NUL unable to name a member at all. The
    rule underneath it is :func:`contain.segment_ok` and deliberately **not**
    ``workspace.valid_name``: ``.github`` is a real repository name, it comes from a forge
    rather than from charter, and ``valid_name`` rejects it for the leading dot.

    ``is_clone`` is the second half and answers a different question — git itself draws
    that line, since a clone's ``.git`` is a directory — so a symlink pointing out of the
    workspace, a plain file, or a directory that is not a checkout are all refused here
    rather than becoming a member charter would later try to push.
    """
    path = contain.child(workspace.workspace_dir(ws), repo)
    if path is None or not workspace.is_clone(path):
        return None
    return path


def cmd_change_create(args) -> int:
    """Create a change: a name, a reason, and no members yet."""
    ws = _workspace(args)
    slug = args.change
    if not instance.change_name_ok(slug):
        util.err(f"{contain.readable(slug)} is not a change name (letters, digits, '.', "
                 "'_', '-'; must not start with a dot or a dash).")
        util.info("The slug names a file in this plane, a branch in every member, and the "
                  "`Charter-Change:` trailer on each landing commit — so it is refused "
                  "rather than rewritten.")
        return 1
    # `required=True` on the parser as well. Both, because a change with no stated reason
    # is unreadable six months later — the one job the record has that git cannot do — and
    # argparse can see neither an empty value nor a newline in one.
    why = _why(args, "what this work is for")
    if why is None:
        return 1
    if change.exists(ws, slug):
        util.err(f"change '{slug}' already exists in workspace '{ws}'.")
        util.info(f"Show it: charter change show {slug}")
        return REFUSED
    rec = change.new_record(slug, why, _author(), _now())
    code = _save(ws, slug, rec)
    if code:
        return code
    util.ok(f"change '{slug}' created")
    util.info(f"Add its repos: charter change add {slug} <repo>")
    return 0


def cmd_change_add(args) -> int:
    """Add one member — one repository's part of the change — by literal name."""
    ws = _workspace(args)
    slug = args.change
    rec, code = _load(ws, slug)
    if rec is None:
        return code

    repo = args.repo
    if _clone_for(ws, repo) is None:
        util.err(f"{contain.readable(repo)}: no clone in workspace '{ws}'.")
        util.info(f"Clone it first: charter clone {contain.readable(repo)} -w {ws}")
        return REFUSED
    if change.member(rec, repo) is not None:
        shown = contain.readable(repo)
        util.err(f"'{shown}' is already a member of '{slug}'.")
        util.info(f"Change its branch or blockers by editing "
                  f"workspaces/{ws}/changes/{slug}.json, or drop it first: "
                  f"charter change drop {slug} {shown} --why \"…\"")
        return REFUSED

    branch = getattr(args, "branch", None) or change.default_branch(slug)
    complaint = change.branch_refusal(branch)
    if complaint:
        util.err(complaint)
        return 1
    needs = list(getattr(args, "needs", None) or ())

    rec = dict(rec)
    rec["members"] = rec["members"] + [{"repo": repo, "branch": branch, "needs": needs}]
    # A repo cannot be a member and excluded at once, so re-adding one lifts its exclusion —
    # loudly, because the exclusion carried a reason somebody wrote down and its removal is
    # a decision rather than bookkeeping.
    lifted = change.exclusion(rec, repo)
    if lifted is not None:
        rec["excluded"] = [e for e in rec["excluded"] if e["repo"] != repo]
    code = _save(ws, slug, rec)
    if code:
        return code
    if lifted is not None:
        util.warn(f"the exclusion recorded on {contain.one_line(lifted['at'])} is lifted: "
                  f"{contain.one_line(lifted['why'])}")
    print(_member_line(rec, {"repo": repo, "branch": branch, "needs": needs}))
    util.ok(f"'{contain.readable(repo)}' is a member of '{slug}'")
    return 0


def cmd_change_drop(args) -> int:
    """Drop a member (or record a repo that was never one) into ``excluded``, with a reason.

    ``--why`` is required and it is the cheapest line in this design: if members have
    already landed, the world stays partially changed permanently, and the only thing that
    makes that readable six months later is that somebody wrote down the reason at the
    moment they still knew it.
    """
    ws = _workspace(args)
    slug = args.change
    rec, code = _load(ws, slug)
    if rec is None:
        return code

    repo = args.repo
    why = _why(args, "why this repo is out")
    if why is None:
        return 1
    # No `segment_ok` gate here, deliberately, and it was measured rather than reasoned:
    # `..` reaches `change.write` below, `_repo_name` refuses the exclusion it would have
    # written, and the operator gets the same exit code and the same sentence — so a gate
    # here is a line no test can redden. `add` is the opposite case and keeps its own: there
    # the resolver has to answer *before* a name is joined onto a directory.
    if change.exclusion(rec, repo) is not None:
        util.err(f"'{contain.readable(repo)}' is already excluded from '{slug}'.")
        return REFUSED
    was_member = change.member(rec, repo) is not None
    blocked = change.dependents(rec, repo)
    if blocked:
        util.err(f"'{contain.readable(repo)}' cannot be dropped from '{slug}': "
                 + ", ".join(f"'{contain.readable(b)}'" for b in blocked) + " still need"
                 + ("" if len(blocked) > 1 else "s") + " it to land first.")
        util.info("Drop those members first, or edit their `needs` — a blocker nothing "
                  "will land is a member that never becomes ready.")
        return REFUSED

    rec = dict(rec)
    rec["members"] = [m for m in rec["members"] if m["repo"] != repo]
    rec["excluded"] = rec["excluded"] + [{"repo": repo, "why": why, "at": _now()}]
    code = _save(ws, slug, rec)
    if code:
        return code
    shown = contain.readable(repo)
    if was_member:
        util.ok(f"'{shown}' excluded — {len(rec['members'])} member(s) left")
    else:
        util.ok(f"'{shown}' excluded (never a member)")
    return 0


def cmd_change_forget(args) -> int:
    """Delete a change record by slug.

    A store with no way to end an entry grows without bound — ADR 0004's argument for
    ``charter workspace forget``, one noun out. It deletes the record and **nothing else**:
    no landing-log line, no branch, no request. The log is a past-tense declaration of
    something that happened, and deleting history to tidy a list is how a store starts
    lying; the branches are the repositories' business.
    """
    ws = _workspace(args)
    slug = args.change
    if not change.exists(ws, slug):
        util.err(f"no change {contain.readable(slug)} in workspace '{ws}'.")
        return REFUSED
    # No `RecordError` handler: `exists` above has already put this slug through
    # `path_for`, so the only way out of `forget` is a path charter must not write — a
    # committed link at the record's own name — which is the `None` below.
    removed = change.forget(ws, slug)
    if removed is None:
        util.err(f"could not delete the record for '{slug}'.")
        return 1
    util.ok(f"change '{slug}' forgotten — the record is gone; branches, requests and the "
            "landing log are untouched.")
    return 0


def cmd_change_list(args) -> int:
    """Every change in the workspace, one row each."""
    ws = _workspace(args)
    records, refused = change.all_for(ws)
    if not records and not refused:
        util.info(f"No changes in workspace '{ws}'. "
                  "Create one: charter change create <slug> --why \"…\"")
        return 0
    names = [contain.one_line(r["change"]) for r in records]
    # gap=0: the gutter is the two literal spaces in the row below, so the width
    # is the widest cell and nothing else. `tui.column` never `len()` — a name whose
    # glyphs are two cells wide is two cells wide (#472).
    w = tui.column("", names, gap=0)
    for rec, name in zip(records, names):
        counts = f"{len(rec['members'])} member(s)"
        if rec["excluded"]:
            counts += f", {len(rec['excluded'])} excluded"
        print(f"{tui.pad(name, w)}  {counts}  ·  {contain.one_line(rec['why'])}")
    for slug, complaint in refused:
        util.err(f"{contain.readable(slug)}: {complaint}")
    return 1 if refused else 0


def cmd_change_show(args) -> int:
    """One change, whole: what it is for, who is in it, and what must go first.

    §3.4 calls this the monorepo view, and being a *view* is what makes it correct — the
    clones are already siblings under one workspace directory, so ``rg`` already spans the
    change; what was missing is knowing which of those subdirectories are one piece of
    work. Every derived column (request numbers, check state, landing dates) belongs to the
    forge-reading phase; what prints here is the record and nothing else, so it is true
    without asking anybody.
    """
    ws = _workspace(args)
    slug = args.change
    rec, code = _load(ws, slug)
    if rec is None:
        return code

    print(f"{contain.one_line(rec['change'])} · {len(rec['members'])} member(s)"
          + (f" · {len(rec['excluded'])} excluded" if rec["excluded"] else ""))
    print(f"  why: {contain.one_line(rec['why'])}")
    print(f"  created {contain.one_line(rec['created'])} by {contain.one_line(rec['by'])}")

    if rec["members"]:
        print("")
        for m in rec["members"]:
            print(_member_line(rec, m))
    if rec["excluded"]:
        print("")
        print("  excluded:")
        w = tui.column("", [contain.one_line(e["repo"]) for e in rec["excluded"]], gap=0)
        for e in rec["excluded"]:
            print(f"  {tui.pad(contain.one_line(e['repo']), w)}  "
                  f"{contain.one_line(e['why'])}  ({contain.one_line(e['at'])})")
    return 0


# --------------------------------------------------------------------------- #
# git, in a clone the operator already has — never a forge, never a network     #
# --------------------------------------------------------------------------- #

def _git(clone: Path, *args: str):
    """One read-only git question about *clone*, never raising on a non-zero exit.

    `doctor._git_in`'s shape and its reason. Read-only is a property of the ARGUMENTS
    rather than of this function, and the two callers that are not read-only —
    :func:`_seed_revert`'s checkout and revert — go through here too, so that every git
    invocation a change action makes passes one place a test can record.
    """
    return util.run(["git", "-C", str(clone), *args], check=False, timeout=GIT_TIMEOUT)


def _default_branch(clone: Path) -> str | None:
    """*clone*'s default branch, or ``None`` when charter cannot honestly say.

    `doctor._plane_default_branch`, asked rather than re-spelled — `hooks` already reaches
    for it the same way. Two answers to "what is this repo's default branch" would drift,
    and the drift would be a divergence report about the wrong branch: a plane whose
    default is `trunk` easily still carries a stale local `main`, and the guess-first
    version of this warns about the correct branch.

    ``None`` matters here more than it does there: every question below is *relative to
    the default branch*, so a clone charter cannot resolve one for produces no divergence
    findings at all rather than findings against a branch nobody uses.
    """
    from .doctor import _plane_default_branch
    return _plane_default_branch(clone)


def _contains(clone: Path, commitish: str, default: str) -> bool:
    """Does *default* contain *commitish*? False for anything charter could not resolve.

    ``refs/heads/<default>`` for the argv reason in this module's docstring, and a bare
    *commitish* only where the caller has already held it to :data:`_SHA_RE` — the two
    callers are :func:`_declared_landed` (a sha out of the log) and :func:`divergences` (a
    member's branch, spelled as a ref).
    """
    return _git(clone, "merge-base", "--is-ancestor", commitish,
                f"refs/heads/{default}").returncode == 0


def _branch_exists(clone: Path, branch: str) -> bool:
    """Is there a local branch of this name? ``refs/heads/`` — see the module docstring."""
    return _git(clone, "rev-parse", "--verify", "--quiet",
                f"refs/heads/{branch}").returncode == 0


#: The one trailer charter promises, on the one commit charter authors. Held here rather
#: than spelled at each site: `revert` builds it, the divergence check looks for it, and
#: the docs quote it, so a second spelling would be a second trailer.
TRAILER = "Charter-Change"


def _trailer_names(clone: Path, sha: str, slug: str) -> bool:
    """Does the commit *sha* carry ``Charter-Change: <slug>``?

    Read out of the commit's own body rather than asked of `git log --grep`, because the
    question is about **this** commit and a grep over a range answers about the range.
    `%B` is the raw body, so a trailer somebody wrote by hand reads the same as one charter
    wrote — which is correct: the claim being checked is *what the commit says*, and
    charter has no way to tell its own trailer from an identical one, nor any business
    trying.
    """
    got = _git(clone, "show", "--no-patch", "--format=%B", sha)
    if got.returncode != 0:
        return False
    want = f"{TRAILER}: {slug}"
    return any(line.strip() == want for line in (got.stdout or "").splitlines())


def _declared_landed(ws: str, slug: str) -> dict[str, dict]:
    """``{repo: the landing line}`` for every member this plane DECLARED it landed.

    File reads only, deliberately: this is the same answer `frame/gather.py` carries into
    the frame's one snapshot, and a second, git-joined answer computed here would be the
    second clock §4f names as the thing this codebase has already paid for twice.

    The git join lives in :func:`divergences`, which is ADR 0013's shape rather than an
    omission: the cheap surface shows what charter declared, and a separate read names
    every place git disagrees with it. One clock, one divergence report.
    """
    return change.declared_landings(ws, slug)


def out_of_order(rec: dict, landed) -> dict[str, list[str]]:
    """``{member: the blockers it went in ahead of}`` — §3.2's honest half.

    Charter refuses to land a member whose blockers have not landed, and it cannot stop a
    human merging in a browser. What would be theatre is a guard that *reports*
    enforcement it does not have, so the same read that refuses also names the landings
    that happened anyway.

    A pure intersection of two things `change` already derives, so it cannot disagree with
    the gate: the members that landed, and the members some blocker of which has not.
    """
    have = set(landed or ())
    blocked = change.blocked_members(rec, have)
    return {repo: pending for repo, pending in blocked.items() if repo in have}


def divergences(ws: str, rec: dict) -> list[str]:
    """Everything git says about this change that the plane's own records do not — as
    sentences, each naming the member it is about.

    **ADR 0013 rule 2, five ways.** *"A divergence charter can see, charter names… WARN is
    not a surface. A divergence worth naming under rule 2 is worth FAIL."* Every finding
    here is a FAIL at its call site, and none of them is a state anything stores.

    1. **Landed outside charter.** The member's recorded branch is contained in its
       clone's default branch and there is no landing declaration — so there is no
       ``Charter-Change`` trailer and no merge sha, and `revert` has nothing to run
       against. Named, and handed to a human.
    2. **A declared landing git no longer contains.** Charter said it merged that sha and
       the default branch does not have it: reverted, or the branch was rewritten.
       *"A member with a log line git no longer contains is not landed any more, and
       nothing had to notice or update a flag for that to become true."*
    3. **A declared landing whose commit does not carry the trailer.** The log names a
       commit charter did not author for this change.
    4. **An out-of-order landing** — :func:`out_of_order`.
    5. **A member's branch name in a clone that is a member of nothing.** The one check
       that looks outside the change: a `change/<slug>` branch sitting in a repository
       nobody declared is either a member somebody forgot to add or a name collision, and
       both are worth a sentence.

    A clone charter cannot resolve a default branch for contributes nothing rather than
    contributing a guess — see :func:`_default_branch`.
    """
    slug = rec["change"]
    declared = _declared_landed(ws, slug)
    out: list[str] = []
    for m in rec["members"]:
        repo, branch = m["repo"], m["branch"]
        clone = _clone_for(ws, repo)
        if clone is None:
            continue          # `charter change add` refused this once; `doctor` says so
        default = _default_branch(clone)
        if default is None:
            continue
        line = declared.get(repo)
        if line is None:
            if _branch_exists(clone, branch) and \
                    _contains(clone, f"refs/heads/{branch}", default):
                out.append(
                    f"{contain.readable(repo)}: branch {contain.readable(branch)} is "
                    f"already in '{contain.one_line(default)}' and charter did not land "
                    f"it — so there is no {TRAILER} trailer and no merge sha. "
                    f"`charter change revert` cannot reach this member; a human has to.")
            continue
        sha = line["merge"]
        if not isinstance(sha, str) or not _SHA_RE.fullmatch(sha):
            out.append(f"{contain.readable(repo)}: the landing log records "
                       f"{contain.readable(sha)} as the merge, which is not a sha.")
            continue
        if not _contains(clone, sha, default):
            out.append(
                f"{contain.readable(repo)}: charter declared it landed as {sha}, and "
                f"'{contain.one_line(default)}' no longer contains that commit — it was "
                f"reverted, or the branch was rewritten. This member is not landed any "
                f"more.")
        elif not _trailer_names(clone, sha, slug):
            out.append(
                f"{contain.readable(repo)}: the landing log names {sha}, and that commit "
                f"carries no `{TRAILER}: {slug}` trailer — charter did not author it for "
                f"this change.")
    for repo, pending in sorted(out_of_order(rec, declared).items()):
        out.append(
            f"{contain.readable(repo)}: landed while "
            + ", ".join(f"'{contain.readable(p)}'" for p in pending)
            + " had not. Charter refuses that landing and cannot stop a browser; this is "
              "the half that makes the guard honest rather than decorative.")
    return out


def stray_branches(ws: str) -> list[str]:
    """Members' branch names found in clones that are a member of no change at all.

    §3.2's second check, and the only one that looks outside the change: a branch named
    like a change's member, in a repository nobody declared, is either a member somebody
    forgot to `charter change add` or a collision with a name that means something else.
    Both are worth a sentence and neither is a state.

    A record charter cannot read contributes no names — `change.all_for` reports those
    separately and `doctor` says so — because guessing at branch names from a record that
    did not parse is the unearned diagnosis ADR 0009 forbids.
    """
    records, _refused = change.all_for(ws)
    wanted: dict[str, str] = {}          # branch name → the change that declares it
    members: set[str] = set()
    for rec in records:
        for m in rec["members"]:
            wanted.setdefault(m["branch"], rec["change"])
            members.add(m["repo"])
    if not wanted:
        return []
    out: list[str] = []
    for clone in sorted(workspace.clones(ws)):
        if clone.name in members:
            continue
        for branch, slug in sorted(wanted.items()):
            if _branch_exists(clone, branch):
                out.append(
                    f"{contain.readable(clone.name)}: has branch "
                    f"{contain.readable(branch)}, which change "
                    f"'{contain.readable(slug)}' declares — and this repo is a member of "
                    f"no change. Add it (charter change add), or the branch is a name "
                    f"collision worth knowing about.")
    return out


# --------------------------------------------------------------------------- #
# revert — a new change, because that is the only rollback a stranger can read  #
# --------------------------------------------------------------------------- #

def _parents(clone: Path, sha: str) -> int | None:
    """How many parents *sha* has, asked of git — or ``None`` when git would not say.

    **Asked, never remembered**, which is what decides whether the revert carries ``-m
    1``. A squash landing is an ordinary one-parent commit and ``-m`` on one fails; a
    merge landing has two and ``git revert`` refuses without it. Storing "this was a
    squash" in the log would be a derivable fact cached for convenience, which is the
    sentence ADR 0011 is.
    """
    got = _git(clone, "rev-list", "--parents", "-n", "1", sha)
    if got.returncode != 0:
        return None
    parts = (got.stdout or "").split()
    return len(parts) - 1 if parts else None


def _seed_revert(clone: Path, *, sha: str, branch: str, default: str) -> str | None:
    """Create *branch* off *default* carrying a revert of *sha*. ``None``, or the reason.

    Four things this does not do, and none of them is behind a flag: it does not force
    anything, delete anything, reset anything, or touch a request. What it does is create
    a branch and add a commit, which is the whole of what a revert-as-a-new-change is.

    **The working tree is checked first and the refusal is named.** `git revert` commits
    into the checkout, so running it over uncommitted work would fold somebody's work in
    progress into a revert commit — and the recovery for that is worse than the wait.

    A conflicted revert is **aborted and named**, not left half-applied: `--abort` restores
    the checkout the revert started from, so the branch stays and the operator resolves it
    themselves. Charter has no business guessing which side of a conflict a rollback
    wanted.

    **`commit.gpgsign` is deliberately not passed on this call**, and the reason is that it
    is already settled one layer down. A revert is the operator's commit in the operator's
    repository, so charter silently unsigning it would be charter deciding somebody's
    signing policy — ADR 0014's rule, which puts that with the host. What actually keeps an
    autonomous run from hanging on a signer prompt is `gitpolicy`: `charter git-policy
    --apply` writes `commit.gpgsign = false` into each clone's *local* config, and `charter
    clone` applies it to everything it clones, precisely because *"a GPG signer prompt hangs
    an autonomous agent mid-run"*. In a clone charter did not set up, the prompt is the
    operator's own configuration doing what they asked it to — measured here as a suite that
    hung with `op-ssh-sign` waiting, which is why `tests/_changerepo.py` sets the policy on
    its fixture repos rather than this function setting it on everybody's.
    """
    dirty = _git(clone, "status", "--porcelain")
    if dirty.returncode != 0:
        return "git could not read the working tree"
    if (dirty.stdout or "").strip():
        return ("the working tree has uncommitted changes, and `git revert` commits into "
                "the checkout — commit or stash them first")
    if _branch_exists(clone, branch):
        return f"branch {contain.readable(branch)} already exists here"
    # `switch -c` rather than `checkout -b`: one verb that only ever moves the branch,
    # against a verb that also restores files and whose argv a path can slip into.
    made = _git(clone, "switch", "-c", branch, f"refs/heads/{default}")
    if made.returncode != 0:
        return contain.one_line((made.stderr or "could not create the branch").strip())
    n = _parents(clone, sha)
    if n is None:
        return f"git does not know the commit {sha}"
    # `-m 1` exactly when git says there is more than one parent — see `_parents`. Spread
    # into the call rather than built as a whole argv, so that the VERB stays a literal at
    # the call site: `tests/test_commands_change.py` reads every `_git` invocation in this
    # module statically and lists the git subcommands it can reach, which is what makes
    # "no network" a claim about the argv rather than about the imports.
    mflag = ["-m", "1"] if n > 1 else []
    done = _git(clone, "revert", "--no-edit", *mflag, sha)
    if done.returncode != 0:
        _git(clone, "revert", "--abort")
        return contain.one_line(
            (done.stderr or "the revert did not apply").strip()) + \
            " — the branch is here and the revert was aborted; finish it by hand"
    return None


def cmd_change_revert(args) -> int:
    """`charter change revert <slug>` — derive a NEW change that reverts what landed.

    **A revert is a new change**, and that is the whole design rather than a limitation.
    Force-pushing three default branches back past the merges leaves a world where the
    change happened, was undone, and no repository's history mentions either — the exact
    failure this whole surface exists to prevent. `component-api-2` and
    `revert-component-api-2`, both named, both cross-referenced, in every repository they
    touched, reads as a decision six months later; a force-push reads as corruption.

    So from here it is ordinary: pushed, reviewed, gated and landed one member at a time,
    by the same commands with the same refusals. There is no ``--all`` on this either.

    **A member landed outside charter is named, not guessed.** No line in the landing log
    means no merge sha, and charter will not pick the merge commit that looks about right.
    ADR 0009: it degrades to silence, never to a confident wrong answer.

    **Two things it cannot do, said here rather than discovered.** It cannot revert a
    deploy — a merge to a default branch on a repo with continuous deployment has already
    had an effect in the world, and charter has no model of deployment. And it cannot
    revert what it did not record.
    """
    ws = _workspace(args)
    slug = args.change
    rec, code = _load(ws, slug)
    if rec is None:
        return code

    new_slug = REVERT_PREFIX + slug
    if not instance.change_name_ok(new_slug):
        util.err(f"{contain.readable(new_slug)} is not a change name, so this change "
                 "cannot name its own revert.")
        return REFUSED
    if change.exists(ws, new_slug):
        util.err(f"change '{new_slug}' already exists in workspace '{ws}'.")
        util.info(f"Show it: charter change show {new_slug}  ·  or forget it first: "
                  f"charter change forget {new_slug}")
        return REFUSED

    declared = _declared_landed(ws, slug)
    if not declared:
        util.err(f"'{slug}': charter has landed no member of this change, so there is "
                 "nothing to revert.")
        util.info("A member merged outside charter has no landing record and no merge "
                  "sha — charter names it rather than guessing which commit was the one.")
        return REFUSED

    # Named before anything is written, so the operator reads the whole picture rather
    # than discovering the un-revertible half after the record exists.
    handed_over = [m["repo"] for m in rec["members"] if m["repo"] not in declared]
    for repo in handed_over:
        util.warn(f"{contain.readable(repo)}: no landing record, so no merge sha — "
                  "charter cannot revert this member. If it was merged in a browser, the "
                  "commit is a human's to find.")

    members, seeded, refused = [], [], []
    for m in rec["members"]:
        repo = m["repo"]
        line = declared.get(repo)
        if line is None:
            continue
        clone = _clone_for(ws, repo)
        if clone is None:
            refused.append((repo, f"no clone in workspace '{ws}'"))
            continue
        sha = line["merge"]
        if not isinstance(sha, str) or not _SHA_RE.fullmatch(sha):
            refused.append((repo, f"the landing log records {contain.readable(sha)} as "
                                  "the merge, which is not a sha"))
            continue
        default = _default_branch(clone)
        if default is None:
            refused.append((repo, "charter cannot tell which branch is the default here, "
                                  "so it will not guess what to branch from"))
            continue
        branch = change.default_branch(new_slug)
        # The member joins the record whether or not the seeding worked. The record is the
        # statement of intent — these repositories are the ones to revert — and a branch
        # that did not get created is a thing to fix, not a member to drop silently.
        #
        # **The ordering is the original's, REVERSED**, and that is a decision rather than
        # bookkeeping. If `charter-metrics` needed `charter` to land first — because its
        # code depends on charter's new API — then undoing it has to go the other way:
        # reverting `charter` while `charter-metrics` still depends on the API it removes
        # leaves the dependent broken, which is the world the revert was supposed to
        # restore. So a member's blockers here are the members that declared IT as a
        # blocker there (`change.dependents`), filtered to the ones being reverted — a
        # member whose dependent charter did not land is not waiting for anybody.
        members.append({"repo": repo, "branch": branch,
                        "needs": [d for d in change.dependents(rec, repo)
                                  if d in declared]})
        why = _seed_revert(clone, sha=sha, branch=branch, default=default)
        if why is None:
            seeded.append(repo)
        else:
            refused.append((repo, why))

    new_rec = change.new_record(
        new_slug,
        f"reverts change '{slug}': {rec['why']}"[:change.TEXT_LIMIT],
        _author(), _now())
    new_rec["members"] = members
    code = _save(ws, new_slug, new_rec)
    if code:
        return code

    util.ok(f"change '{new_slug}' created — {len(members)} member(s) to revert")
    for repo in seeded:
        print(f"  {contain.one_line(repo)}  branch "
              f"{contain.one_line(change.default_branch(new_slug))}  reverted")
    for repo, why in refused:
        util.err(f"{contain.readable(repo)}: {why}")
    util.info(f"It is an ordinary change from here: charter change show {new_slug}")
    # Non-zero when a member charter put in the record has no branch behind it: the record
    # says these repositories are to be reverted, and one of them is not started.
    return 1 if refused else 0


def _member_line(rec: dict, m: dict) -> str:
    """One member's row. Every field is contained **before** the width arithmetic.

    That order is #472: `tui.pad` measures the string it is handed, so escaping a value
    after padding it pads it to the wrong width, and the column stops lining up at exactly
    the row whose content came out of a committed file. `contain.one_line` is the bound
    here, and it holds only what that function holds — line structure, not trustworthiness.
    """
    w = tui.column("", [contain.one_line(x["repo"]) for x in rec["members"]], gap=0)
    row = (f"  {tui.pad(contain.one_line(m['repo']), w)}  "
           f"branch {contain.one_line(m['branch'])}")
    if m["needs"]:
        row += "   needs: " + ", ".join(contain.one_line(n) for n in m["needs"])
    return row
