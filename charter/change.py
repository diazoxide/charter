"""The cross-repo **change** — one intent, N repositories, stored as intent only.

A change is one JSON file, ``workspaces/<ws>/changes/<slug>.json``, holding the six things
git and the forge cannot know: what this work is called, what it is *for*, which
repositories are meant to be part of it, which branch in each is *this change's* branch as
against the eleven others in that clone, which member must land before which, and which
repository was considered and deliberately left out — with the reason somebody wrote down
while they still knew it.

**There is no state field.** No request number, no CI result, no branch position, no
``landed`` flag. Those are all things git or the forge already knows, and ADR 0011 says
what caching one costs: *"The moment a derivable fact is cached for convenience, this ADR
has been reversed whether or not anyone says so."* Ordering is the case worth naming twice,
because it looks like state and is not: ``needs`` is **declared** (only a human knows that
repo B's change needs repo A's merged), and *blocked* is **derived** at read time from that
declaration plus a fresh reading of what has landed — see :func:`blocked_members`, which
writes nothing anywhere.

**The key set is closed, at both ends, and an unknown key is refused by name.** #503's rule,
for #503's reason: a key charter does not read reads as nothing at all, so ``need`` where
``needs`` was meant is an ordering constraint that silently ceased to exist. The check runs
on :func:`write` as well as :func:`read`, because a validator only the reader consults is
one a caller can walk around by holding the record in memory.

**Membership is committed. Destination is local.** The record carries bare repository
names — never a URL, a host, a remote name, a forge kind or a base branch. A remote in a
committed file is a *destination* that arrives from someone else's machine, which is
``charter.toml``'s own rule (*"Arrangement is committed. Execution is local."*) applied to
a different committed file. Where a member's work is pushed is resolved from that clone's
own ``origin``, by the caller, at the time it pushes.

A committed record is also **untrusted input**, and two of its fields cross out of this
module into places where a string is not just a string:

* a **branch** reaches ``git`` as argv, and ref grammar is not argv safety —
  ``git check-ref-format`` accepts ``refs/heads/-b`` (measured on git 2.50.1), so a
  ``{"branch": "-b"}`` in a record somebody else wrote is a flag. :func:`branch_refusal`
  is the boundary half of that guard; every git invocation carrying one places it after
  ``--`` as the other half, and neither substitutes for the other.
* a **slug**, a **repo name**, a **why** and a **branch** all reach a report line somebody
  reads, so every printing site sends them through :func:`contain.one_line` *before* any
  width arithmetic.

Vocabulary, because the noun is already taken: :mod:`charter.forge.base` calls **one pull
or merge request** a change (``open_change``, ``change_sigil``). Here a **change** is the
cross-repo object, a **member** is one repository's part of it, and a member's pull or
merge request is a **request**. Neither name is renamed; both are shipped.
"""

from __future__ import annotations

import json
from pathlib import Path

from . import config, contain, instance, workspace


class RecordError(Exception):
    """A change record charter will not act on, with the reason in the message.

    Raised rather than answered with ``{}``, and that is the whole point of the class.
    :func:`add_member`-shaped callers are read-modify-write, so a read that degraded to an
    empty record would write back a record holding only the new member and drop every
    sibling it never saw. That is not hypothetical: ``onepassword._fields`` returned ``{}``
    for every non-zero ``op item get``, a rate-limited vault reported "has no secrets"
    (#322), and the read-modify-write behind it piped back a template holding one key.
    """


#: The directory, relative to the workspace, that holds the records.
DIRNAME = "changes"

#: The landing log's directory, inside it. A separate name because it is a separate
#: lifetime: the records are committed when the workspace is LIVE and the log is committed
#: **never**, exactly as ``pieces/`` is never committed. It holds the past-tense
#: declaration git cannot make — *charter merged this commit, for this change* — and the
#: present tense is reconstructed by asking git, not by reading a flag.
LOG_DIRNAME = "log"

#: The whole top-level key set, matched exactly. Six keys, no seventh, none missing.
KEYS = ("change", "why", "created", "by", "members", "excluded")

#: One member's keys. ``needs`` is the ordering *declaration*; there is no ``state``,
#: ``landed``, ``pr`` or ``ci``, and the closed set is what makes them unrepresentable
#: rather than merely unwritten.
MEMBER_KEYS = ("repo", "branch", "needs")

#: How long a record's own text may be. `contain`'s PATH budget rather than its ROW budget,
#: and the difference is load-bearing in both directions: a `why` a little over one row is a
#: legitimate `why` and refusing it would send somebody to hand-edit the file, while the row
#: budget still applies where rows are drawn — `contain.one_line` clips to `DISPLAY_LIMIT` at
#: every printing site, and that clipping is a thing a test can see precisely because this
#: bound is the looser one. What is refused here is a character with no glyph, which is a
#: question about the value's shape rather than its length.
TEXT_LIMIT = contain.PATH_DISPLAY_LIMIT

#: One exclusion's keys — a repository considered and deliberately left out. ``why`` is
#: required by the same rule that requires the change's own: the exclusion is the only
#: artifact that makes a permanently partial world explicable six months later.
EXCLUSION_KEYS = ("repo", "why", "at")


def changes_dir(ws: str) -> Path:
    """``workspaces/<ws>/changes/`` — a *sibling* of ``memory/``, ``todos/`` and
    ``pieces/``, never a child.

    **Not created by** :func:`workspace.scaffold`, and that is a decision with a failure
    behind it rather than laziness. ``commands_workspace._ws_meta_paths`` filters its list
    by existence, and that existence filter is doing double duty as a *non-emptiness*
    filter: the paths go to git as literal arguments, and ``git rm --cached`` on one that
    was never tracked fails the whole call. An always-present, always-empty ``changes/``
    would break ``charter workspace live --off`` whole — untracking nothing, and leaving
    the manifest and the memory committed on a workspace the operator has just made
    private. So it is created by the first ``charter change create``, and ``changes/log/``
    by the first landing, for the same reason ``todos/`` is created by the first todo.
    """
    return workspace.workspace_dir(ws) / DIRNAME


def log_dir(ws: str) -> Path:
    """``workspaces/<ws>/changes/log/`` — never committed. See :data:`LOG_DIRNAME`."""
    return changes_dir(ws) / LOG_DIRNAME


def path_for(ws: str, slug: str) -> Path:
    """The record's path, or :class:`RecordError` when *slug* is not a change name.

    The name is asked of :func:`instance.change_name_ok` — one rule, in one place, where
    ``workspace_name_ok`` already lives — and it is asked *here* rather than only at
    creation because a record can arrive from a hand edit, from an older charter, or from
    somebody else's machine. Creation-time validation and containment answer different
    questions, and #442 and #503 are both what happens when one is mistaken for the other.
    """
    if not instance.change_name_ok(slug):
        raise RecordError(
            f"{contain.readable(slug)} is not a change name (letters, digits, '.', '_', "
            "'-'; must not start with a dot or a dash). This names a file in the plane and "
            "a branch in every member, so it is refused rather than rewritten.")
    return changes_dir(ws) / f"{slug}.json"


def exists(ws: str, slug: str) -> bool:
    """Is there a record for *slug*? False for a name that is not a change name."""
    try:
        return path_for(ws, slug).exists()
    except RecordError:
        return False


def has_records(ws: str) -> bool:
    """Does this workspace hold at least one change record — i.e. is there anything under
    ``changes/`` that a LIVE workspace could have committed?

    A sharper question than "does the directory exist", and it has to be, because that is
    what ``commands_workspace._ws_meta_paths`` really needs and its existence filter is
    only a *proxy* for. ``todos/`` can rely on the proxy: it is created with its index and
    is never empty afterwards. ``changes/`` can be emptied — ``charter change forget`` on
    the last record leaves the directory behind, and a directory holding nothing but the
    never-committed ``log/`` is the same case. Either way ``git rm --cached`` on a path
    with nothing tracked under it fails the **whole** call, taking the manifest and the
    memory down with it on a ``workspace live --off`` the operator ran to make a workspace
    private. Asking the sharp question here keeps that knowledge in the module that owns
    the record's shape.
    """
    try:
        return any(p.name.endswith(".json") for p in changes_dir(ws).iterdir())
    except OSError:
        return False


def read(ws: str, slug: str) -> dict:
    """The record for *slug*, validated — or :class:`RecordError` saying what is wrong.

    **Both refusals, not one** (#336). ``file_refusal`` cannot see the variant where the
    *directory* is the link: every file inside a symlinked ``changes/`` is an ordinary
    regular file with nothing to object to, and only ``dir_refusal`` — which resolves —
    catches it.
    """
    # `path_for` refuses anything that is not a change name, so `slug` is
    # `[A-Za-z0-9][A-Za-z0-9._-]*` from here down and the messages below need no further
    # containment — `validate` does contain it, because `write` calls that one FIRST.
    p = path_for(ws, slug)
    refusal = contain.dir_refusal(p.parent) or contain.file_refusal(p)
    if refusal:
        raise RecordError(refusal)
    try:
        raw = p.read_text()
    except OSError as exc:
        raise RecordError(f"change '{slug}': cannot be read ({exc})") from exc
    try:
        rec = json.loads(raw)
    except ValueError as exc:
        raise RecordError(f"change '{slug}': the record is not JSON ({exc})") from exc
    validate(rec, slug)
    return rec


def write(ws: str, slug: str, rec: dict) -> Path:
    """Write *rec* as the record for *slug*, validated first. Returns the path.

    The validation is not a courtesy to the next reader: without it, the closed key set is
    a rule only the read path enforces, and anything that hangs a derived value off the
    in-memory record — a cached ``blocked`` set, a remembered request number — serialises
    it here and it is a stored state field from that moment on.
    """
    validate(rec, slug)
    p = contain.writable(path_for(ws, slug))
    config.mkdir_for(p.parent)
    config.write_for(p, _serialise(rec))
    return p


def forget(ws: str, slug: str) -> Path | None:
    """Delete the record for *slug*; return the path removed, or ``None`` if there was
    none. Deletes **no** landing-log line and no branch: the log is a past-tense
    declaration, and deleting history to tidy a list is how a store starts lying."""
    p = path_for(ws, slug)
    if contain.write_refusal(p) or not p.exists():
        return None
    p.unlink()
    return p


def all_for(ws: str) -> tuple[list[dict], list[tuple[str, str]]]:
    """Every record in the workspace, as ``(records, refused)``.

    Two lists rather than one, in ``workspace.merge_repo_rows``' shape and for its reason:
    a record charter could not read is something the caller **reports**, never something
    it silently passes off as absent. One bad file must not take the listing down, and it
    must not disappear either — those are the two failure modes, and returning only the
    good half picks one of them by accident.
    """
    records: list[dict] = []
    refused: list[tuple[str, str]] = []
    d = changes_dir(ws)
    complaint = contain.dir_refusal(d)
    if complaint:
        return records, [(DIRNAME, complaint)]
    try:
        names = sorted(p.name for p in d.iterdir())
    except OSError:
        return records, refused          # no changes/ yet: the lazy-creation case
    for name in names:
        if not name.endswith(".json"):
            continue                     # changes/log/, and anything else that is not one
        slug = name[: -len(".json")]
        try:
            records.append(read(ws, slug))
        except RecordError as exc:
            refused.append((slug, str(exc)))
    return records, refused


# --------------------------------------------------------------------------- #
# validation — the closed key set, the containment, and the ordering            #
# --------------------------------------------------------------------------- #

def validate(rec, slug: str) -> None:
    """Raise :class:`RecordError` unless *rec* is a change record for *slug*.

    **The slug is asked about first, and that is what lets every sentence below name it
    plainly.** From the second line down, *slug* is `[A-Za-z0-9][A-Za-z0-9._-]*` — a value
    no report row can be forged out of — so containing it again at each of the fourteen
    messages would be fourteen calls no test could redden. :func:`read` has already asked
    this through :func:`path_for`; :func:`write` has not, because it validates the record
    before it resolves the path, so the invariant belongs here rather than in an order the
    callers have to keep.

    A record that calls *itself* something else is a different question, and that one IS
    contained: the record is the untrusted half.
    """
    if not instance.change_name_ok(slug):
        raise RecordError(f"{contain.readable(slug)} is not a change name")
    if not isinstance(rec, dict):
        raise RecordError(f"change '{slug}': the record is not an object")
    _closed(rec, KEYS, f"change '{slug}'")

    # No second `change_name_ok`, on `rec["change"]`: the line above has just established
    # that it EQUALS *slug*, which the first line established is a change name.
    if rec["change"] != slug:
        raise RecordError(
            f"change '{slug}': the record calls itself "
            f"{contain.readable(rec['change'])}. The filename and the name are one "
            "identity — the name is what a merge commit's trailer carries, so a record "
            "that disagrees with its own file has two of them.")
    for key in ("why", "created", "by"):
        _one_line(rec[key], f"change '{slug}': {key}")

    if not isinstance(rec["members"], list):
        raise RecordError(f"change '{slug}': 'members' is not a list")
    seen: set[str] = set()
    for m in rec["members"]:
        if not isinstance(m, dict):
            raise RecordError(f"change '{slug}': a member is not an object")
        _closed(m, MEMBER_KEYS, f"change '{slug}': member")
        repo = _repo_name(m["repo"], f"change '{slug}': member")
        if repo in seen:
            raise RecordError(
                f"change '{slug}': '{repo}' is a member twice")
        seen.add(repo)
        complaint = branch_refusal(m["branch"])
        if complaint:
            raise RecordError(f"change '{slug}': member '{repo}': {complaint}")
        if not isinstance(m["needs"], list):
            raise RecordError(
                f"change '{slug}': member '{repo}': 'needs' is not a list")
        for n in m["needs"]:
            _repo_name(n, f"change '{slug}': member '{repo}': needs")

    if not isinstance(rec["excluded"], list):
        raise RecordError(f"change '{slug}': 'excluded' is not a list")
    for e in rec["excluded"]:
        if not isinstance(e, dict):
            raise RecordError(
                f"change '{slug}': an exclusion is not an object")
        _closed(e, EXCLUSION_KEYS, f"change '{slug}': exclusion")
        repo = _repo_name(e["repo"], f"change '{slug}': exclusion")
        if repo in seen:
            raise RecordError(
                f"change '{slug}': '{repo}' is both a member and excluded")
        for key in ("why", "at"):
            _one_line(e[key], f"change '{slug}': exclusion '{repo}': {key}")

    complaint = order_refusal(rec)
    if complaint:
        raise RecordError(f"change '{slug}': {complaint}")


def _closed(obj: dict, keys: tuple[str, ...], where: str) -> None:
    """The closed key set, both directions, refused **by name**.

    Unknown *and* missing, because they are the same defect seen from two sides: a record
    with ``need`` where ``needs`` was meant has an unknown key and a missing one, and
    naming only the first would leave a reader hunting for the typo they already made.
    """
    unknown = sorted(k for k in obj if k not in keys)
    if unknown:
        raise RecordError(
            f"{where}: unknown key "
            + ", ".join(contain.readable(k) for k in unknown)
            + f". The key set is closed — {', '.join(keys)} — and a key charter does not "
            "read reads as nothing at all, so it is named rather than ignored.")
    missing = [k for k in keys if k not in obj]
    if missing:
        raise RecordError(f"{where}: missing key {', '.join(missing)}")


def _one_line(value, where: str) -> str:
    """*value* when it is a non-empty single line, else :class:`RecordError`.

    A ``why`` that cannot be one line is a ``why`` that belongs in ``workspace.md``, which
    is where this plane keeps prose. Refusing it here is the boundary half; every printing
    site still calls :func:`contain.one_line`, because this cannot see what a *future*
    reader's terminal does with U+2028 and that function can.
    """
    if not isinstance(value, str) or not value.strip():
        raise RecordError(f"{where}: expected a non-empty string, got {contain.readable(value)}")
    if contain.one_line(value, limit=TEXT_LIMIT) != value:
        raise RecordError(
            f"{where}: {contain.readable(value)} is not one plain line. This is repeated "
            "back on a report row and written into a pull request body, where a newline "
            "forges a second row.")
    return value


def _repo_name(value, where: str) -> str:
    """A member's (or a blocker's, or an exclusion's) repository name.

    :func:`contain.segment_ok` and deliberately **not** ``workspace.valid_name``: the name
    comes from a forge rather than from charter, and ``.github`` is a real and common
    repository that ``valid_name`` rejects for starting with a dot. That distinction has
    already cost this project once. What is being asked is containment — could this string
    name one entry inside the workspace directory — and nothing more.
    """
    if not isinstance(value, str) or not contain.segment_ok(value):
        # `readable` inside `refusal`, not instead of it: the sentence tells somebody to go
        # and fix this name in a file, and a name they cannot read off the row is a name
        # they cannot find (#579). `refusal` bounds the result to one line either way.
        raise RecordError(f"{where}: {contain.refusal(contain.readable(value))}")
    return value


def branch_refusal(branch) -> str | None:
    """Why this branch name must not be handed to git, or ``None``.

    ``git check-ref-format`` **accepts** ``refs/heads/-b`` — measured on git 2.50.1 — so
    ref grammar answers a different question from the one being asked here. The value is
    read out of a committed file and reaches ``git`` as argv, where a leading dash is a
    flag. This is one of the two mechanisms; the other is that every invocation carrying a
    branch places it after ``--``, and either alone has already been enough to ship a bug
    in this repository.
    """
    if not isinstance(branch, str) or not branch:
        return f"{contain.readable(branch)} is not a branch name (a non-empty string)"
    if branch.startswith("-"):
        return (f"branch {contain.readable(branch)} begins with '-', so it reaches git as a "
                "FLAG rather than as a branch. `git check-ref-format` accepts "
                "`refs/heads/-b`, so ref grammar does not answer this.")
    if contain.one_line(branch, limit=TEXT_LIMIT) != branch:
        return (f"branch {contain.readable(branch)} is not one plain line — it carries a "
                "character with no glyph, or is longer than a committed value may be.")
    return None


# --------------------------------------------------------------------------- #
# ordering — declared per member, derived at read time, stored never            #
# --------------------------------------------------------------------------- #

def order_refusal(rec: dict) -> str | None:
    """Why this record's ``needs`` cannot be true, or ``None``.

    Three ways, and each is a record rather than a state: a member that blocks itself, a
    ``needs`` naming a repository that is not a member of this change (so nothing will ever
    land it *for this change*, and the member waits for ever), and a cycle. A cycle is not
    a condition to render — it is an ordering no sequence satisfies — so it is refused at
    write time with the members in it named, rather than reported at read time.
    """
    graph = {m["repo"]: list(m["needs"]) for m in rec["members"]}
    for repo, needs in graph.items():
        for n in needs:
            if n == repo:
                return (f"member '{repo}' declares itself as its own blocker — it would "
                        "wait for its own landing")
            if n not in graph:
                return (f"member '{repo}' needs '{n}', which is not a member of this "
                        "change. Add it first (charter change add), or drop the need — a "
                        "blocker nothing will land is a member that never becomes ready.")
    cycle = _cycle(graph)
    if cycle:
        return ("ordering cycle: " + " → ".join(f"'{r}'" for r in cycle)
                + " — no member can go first, so this is a record that cannot be true")
    return None


def _cycle(graph: dict[str, list[str]]) -> list[str] | None:
    """One cycle in the ``needs`` graph as a path that returns to its start, or ``None``.

    The whole path, not the pair that closed it: with A → B → C → A, naming only C and A
    sends the reader to edit the one edge that is hardest to see is wrong.
    """
    state: dict[str, int] = {}          # 1 = on the current path, 2 = fully explored
    path: list[str] = []

    def walk(node: str) -> list[str] | None:
        state[node] = 1
        path.append(node)
        for nxt in graph[node]:
            if state.get(nxt) == 1:
                return path[path.index(nxt):] + [nxt]
            if state.get(nxt) is None:
                found = walk(nxt)
                if found:
                    return found
        path.pop()
        state[node] = 2
        return None

    for repo in graph:
        if state.get(repo) is None:
            found = walk(repo)
            if found:
                return found
    return None


def blocked_members(rec: dict, landed) -> dict[str, list[str]]:
    """``{member: the blockers of that member that have NOT landed}`` — derived, never stored.

    A pure function of the record plus a landing map, so it is verified by construction
    and disappears the moment either changes. Nothing writes ``blocked`` anywhere: ADR
    0011 forbids recording a state charter cannot verify, and this is the alternative
    rather than an exception to it.

    Iterating or membership-testing the result gives the *set* of blocked members; the
    values are what a refusal has to name, because "blocked" without the blocker sends the
    reader looking through five repositories for the one that has not gone in.

    Computed for **every** member, including one that has already landed — deliberately.
    ``set(blocked_members(rec, landed)) & set(landed)`` is then exactly §3.2's out-of-order
    landing: a member that went in while a blocker had not. Charter cannot stop a human
    merging in a browser, and the honest half of that guard is naming it when it happens.
    """
    have = set(landed or ())
    out: dict[str, list[str]] = {}
    for m in rec["members"]:
        pending = [n for n in m["needs"] if n not in have]
        if pending:
            out[m["repo"]] = pending
    return out


def dependents(rec: dict, repo: str) -> list[str]:
    """The members that declare *repo* as a blocker. What ``drop`` has to name."""
    return [m["repo"] for m in rec["members"] if repo in m["needs"]]


def member(rec: dict, repo: str) -> dict | None:
    """The member row for *repo*, or ``None``."""
    for m in rec["members"]:
        if m["repo"] == repo:
            return m
    return None


def exclusion(rec: dict, repo: str) -> dict | None:
    """The exclusion row for *repo*, or ``None``."""
    for e in rec["excluded"]:
        if e["repo"] == repo:
            return e
    return None


def new_record(slug: str, why: str, by: str, created: str) -> dict:
    """A change with no members and no exclusions yet."""
    return {"change": slug, "why": why, "created": created, "by": by,
            "members": [], "excluded": []}


def default_branch(slug: str) -> str:
    """The branch name ``charter change add`` offers. Stored in the record, never derived
    from a convention afterwards — a convention breaks the moment somebody names one
    differently, and git can tell you a branch exists but never that it is *this
    change's*."""
    return f"change/{slug}"


def _serialise(rec: dict) -> str:
    """The record's bytes: the keys in :data:`KEYS` order, two-space indent, one trailing
    newline — ``workspace.write_manifest``'s shape.

    Canonical rather than insertion-ordered so that a record read and written back is
    byte-identical whatever order it happened to be typed in, which is what makes "nothing
    derived reached disk" an assertion a test can make on the file itself.
    """
    ordered = {k: rec[k] for k in KEYS}
    ordered["members"] = [{k: m[k] for k in MEMBER_KEYS} for m in rec["members"]]
    ordered["excluded"] = [{k: e[k] for k in EXCLUSION_KEYS} for e in rec["excluded"]]
    return json.dumps(ordered, indent=2) + "\n"
