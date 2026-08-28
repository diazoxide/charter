"""`charter change` — the cross-repo change, as records. **No forge, no network.**

Six verbs over :mod:`charter.change`: ``create``, ``add``, ``drop``, ``list``, ``show`` and
``forget``. Everything here is a file read, a file write and a directory listing; nothing in
this module opens a socket, runs a forge CLI or pushes a branch. The forge half — push,
gate, land, revert — is a later phase, and keeping the record surface separable from it is
what makes "what did the operator declare" answerable without asking anybody's API.

**Membership is enumerated by hand, and that is the containment property.** There is no
glob, no pattern, no ``--all-repos``, no "every repo in the workspace", and nothing here
calls ``inventory.list_repos``. Every member in a record was typed by somebody, and a member
must resolve to a clone the operator already put in this workspace — so the reach of a
change is bounded by what is already on the disk in front of you, and by nothing that
travelled in a committed file. A record can name a repository you do not have; it is refused
for that, by name, and it can never name a *place*.
"""

from __future__ import annotations

import datetime
import os
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
        util.err(f"could not delete the record for {contain.readable(slug)}.")
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
