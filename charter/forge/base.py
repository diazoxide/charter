"""The forge-agnostic contract.

`charter` talks to a code-hosting forge for five things: authentication, enumerating an
owner's repos, reading a repo's file list, and — per branch — the open change and the CI
result. Everything above this module is written against the protocol, so adding a forge
means adding one file rather than touching call sites.

Vocabulary is deliberately neutral. GitLab says "merge request" and has exactly one
pipeline per commit; GitHub says "pull request" and has N check-runs with no inherent
single value. `ci_status` therefore returns one of :data:`CI_STATES` from either, and
`change_sigil` carries each forge's native rendering so neither audience reads the
other's jargon.
"""
from __future__ import annotations

from typing import NamedTuple, Protocol, runtime_checkable

#: The neutral CI vocabulary every implementation maps onto.
CI_STATES = frozenset({"success", "failed", "running", "pending",
                       "manual", "canceled", "skipped"})

# --------------------------------------------------------------------------------- #
# How long one forge CLI invocation may take                                          #
# --------------------------------------------------------------------------------- #
# `util.run` has taken a `timeout` since the day its docstring named `gh api` and
# `glab api` as paths that "could hang indefinitely", and no forge call site passed one
# (#324). The bound lives here rather than at each site so a new backend inherits it.
#
# #324 left the number open, on the grounds that a status refresh, a `discover` paging
# through hundreds of repos and a `doctor` preflight have different budgets. They do —
# but that is a difference in TOTAL, not per call. Every site below is one CLI
# invocation making one API request; `discover`'s hundreds of repos are hundreds of
# separate invocations, each of which should still answer promptly. So the split that
# earns two numbers is not caller-by-caller, it is the permissive/strict split both
# backends already draw, because the two differ in what being WRONG costs.

#: The best-effort path (`_api`, `ci_status`) — what the status line renders from.
#: Being wrong is nearly free: a blank column, retried at the next `REFRESH_TTL`. This
#: is also the path that runs detached, holding the forge credential, on a surface that
#: asks for another refresh every two minutes, so a generous bound here is the expensive
#: mistake. Ten seconds is roughly an order of magnitude over a healthy call (a CLI
#: start plus one HTTPS round trip) and still covers a cold handshake to a self-hosted
#: instance over a VPN. It sits in charter's existing scale for a network call on a
#: rendering path — `doctor.CHECK_TIMEOUT` is 5s, `update.NET_TIMEOUT` 5s — and below
#: `glstate.SPAWN_COOLDOWN`, so one call can never on its own outlive the cooldown.
STATUS_TIMEOUT = 10.0

#: The strict path (`_paged_strict`, `_api_strict`, `repo_tree_strict`) — human-invoked,
#: and a failure aborts the whole `discover` rather than blanking a cell, so a bound
#: tight enough for the status line would trade a cheap wrong answer for an expensive
#: one. One page here is a hundred records with descriptions and topics, not a single
#: `per_page=1` lookup. Sixty seconds is the same number `secrets.reference` already
#: uses for "a human is waiting and this is allowed to be slow".
LIST_TIMEOUT = 60.0


class ForgeError(Exception):
    """A forge CLI is missing, unauthenticated, or returned something unusable."""


class ForgeWriteError(ForgeError):
    """A WRITE to a forge failed, and the caller must be told rather than handed a None.

    Modelled on `report.ReportingError`, which exists for exactly this and says why:
    ``_api`` is documented best-effort and answers ``None`` on any failure, which is right
    for the status line and catastrophic for a write — *"swallow it and return None" means
    the Reporter's report vanishes while they are told it worked* (ADR 0002). Applied here
    it means a pull request that was never opened, a cross-link block that was never
    written, or a merge that never happened, each reported as a success.

    A subclass of :class:`ForgeError` so a caller that already handles "the forge said no"
    keeps working; raised instead of it so a caller that cares which side of the read/write
    line failed can ask.
    """


# --------------------------------------------------------------------------------- #
# Checks at one head sha — the vocabulary #561 did not have                            #
# --------------------------------------------------------------------------------- #
# `ci_status` collapses six worlds into `None`: a CLI failure, a timeout, a non-zero exit,
# malformed JSON, an auth failure, and "no check ever ran". That is correct for the status
# line, where being wrong costs a blank column — and it is the shape that cannot answer the
# one question a landing gate has to ask. `gh pr checks` says "no checks reported" and
# `mergeStateStatus` says CLEAN when no run was ever created, identically to a clean pass,
# and the merge button is offered anyway. Both are named as FORBIDDEN inputs here, in the
# spec and in the test, because the failure they cause is a green light rather than an error.
#
# So this is two fields and not one string. A single string still cannot separate "there are
# no runs" from "I could not look", and adding a `not_run` member to `CI_STATES` would
# repeat the mistake with an extra name.

#: At least one check at this head sha, and everything that concluded, concluded well.
CHECKS_PASSED = "passed"
#: A check at this head sha concluded badly — failure, cancelled, timed out, startup
#: failure, or the forge asking for a human (``action_required``).
CHECKS_FAILED = "failed"
#: A check at this head sha is queued or in progress. Not a verdict yet.
CHECKS_RUNNING = "running"
#: **Zero** checks exist at this head sha. Not green. Six months of zero is still silence,
#: and ADR 0011's rule is that no threshold ever converts silence into a verdict.
CHECKS_NOT_RUN = "not_run"
#: Charter could not ask, or could not ask *completely*. Not green either, and distinct
#: from :data:`CHECKS_NOT_RUN` on purpose: that one asserts nothing ran, and charter may
#: only assert it having enumerated everywhere it knows to look.
CHECKS_UNKNOWN = "unknown"

#: Worst first, and fixed here so two readers agree. ``UNKNOWN`` outranks everything
#: because it is the only value that means charter did not look, and "I did not look" must
#: never be outranked by "I looked and it was fine".
CHECKS_PRECEDENCE = (CHECKS_UNKNOWN, CHECKS_FAILED, CHECKS_RUNNING,
                     CHECKS_NOT_RUN, CHECKS_PASSED)

#: The closed set. Five values, and no two collapse.
CHECK_STATES = frozenset(CHECKS_PRECEDENCE)


class Checks(NamedTuple):
    """What charter read at one head sha.

    ``total`` is how many checks charter **enumerated** — and ``total is None`` is the only
    way to say *I could not ask*. ``total == 0`` says *I asked, everywhere I know to look,
    and there is nothing there*, which is :data:`CHECKS_NOT_RUN` and is not a pass.

    Keyed to the head sha, which is the whole staleness story: a check run at any other sha
    is not a check on this head, so a pushed fixup returns a member to ``NOT RUN``
    immediately and loudly rather than leaving the previous sha's green result standing.
    There is deliberately no ``STALE`` state, because there is nothing for it to describe.
    """

    total: int | None
    state: str


def worst(states) -> str:
    """Fold per-check states into one by :data:`CHECKS_PRECEDENCE`.

    An empty sequence is :data:`CHECKS_NOT_RUN` — nothing was enumerated, which is exactly
    what that word means. A state this module does not know is :data:`CHECKS_UNKNOWN`,
    never :data:`CHECKS_PASSED`: the asymmetry decides it, because a false ``NOT RUN`` or a
    false ``UNKNOWN`` costs a re-run and a false ``PASSED`` merges untested code.
    """
    seen = set(states)
    if not seen:
        return CHECKS_NOT_RUN
    if seen - CHECK_STATES:
        return CHECKS_UNKNOWN
    for state in CHECKS_PRECEDENCE:
        if state in seen:
            return state
    return CHECKS_UNKNOWN


# --------------------------------------------------------------------------------- #
# The request as more than a number                                                   #
# --------------------------------------------------------------------------------- #

#: Open, and therefore landable.
REQUEST_OPEN = "open"
#: Merged. Half of "landed" — the other half is git still containing the sha.
REQUEST_MERGED = "merged"
#: Closed without merging. This is ``REJECTED``, and `open_change` cannot see it.
REQUEST_CLOSED = "closed"

#: The closed set of request states.
REQUEST_STATES = frozenset({REQUEST_OPEN, REQUEST_MERGED, REQUEST_CLOSED})


class Request(NamedTuple):
    """One pull/merge request, as more than a number.

    :meth:`Forge.open_change` answers ``int | None`` for **open** requests only, so a
    closed-unmerged member and a member with no request at all are the same value — and
    ``PARTIALLY LANDED`` and ``REJECTED`` are both underivable from it. This carries the
    state, the head sha the checks must be read at, and the merge commit when there is one.

    ``merge`` is set only when ``state`` is :data:`REQUEST_MERGED`. GitHub populates
    ``merge_commit_sha`` on an *open* pull request too — with the sha of a throwaway test
    merge — so reading it unconditionally would hand `revert` a commit that is on no branch.
    """

    number: int
    state: str
    head: str
    merge: str | None = None


#: The merge methods charter will perform, and the whole set.
#:
#: ``rebase`` is absent deliberately and the refusal is charter constraining **its own
#: act**, not the repository's policy: a rebase merge replays the author's own commits, and
#: charter authors none of them — so there is no commit to carry the ``Charter-Change:``
#: trailer and no single sha for `revert` to run against. A human merging by rebase is
#: unaffected and shows up as a member landed outside charter.
MERGE_METHODS = ("merge", "squash")


#: Keys every implementation must produce for each repo from :meth:`Forge.list_repos`.
#: ``forge`` names the implementation that produced it, so a mixed inventory stays
#: unambiguous once records from several forges are merged.
REPO_KEYS = ("name", "path_with_namespace", "default_branch", "description",
             "web_url", "ssh_url", "topics", "id", "forge")


@runtime_checkable
class Forge(Protocol):
    """What `charter` needs from a code-hosting forge."""

    kind: str            #: "gitlab" | "github"
    host: str            #: "gitlab.com", "github.com", or a self-hosted host
    cli: str             #: the CLI binary that holds the credential
    change_sigil: str    #: "!" for a GitLab MR, "#" for a GitHub PR
    owner_noun: str      #: "group" (GitLab) | "org" (GitHub) — the human word for what
                         #: `owner` names; naming the wrong one is a real, user-visible
                         #: bug (a GitHub control plane talked about "groups").

    def check_auth(self) -> None:
        """Raise :class:`ForgeError` unless the CLI is installed and logged in."""

    def list_repos(self, owner: str) -> list[dict]:
        """Every repo under *owner* (a GitLab group, or a GitHub org/user), each record
        carrying :data:`REPO_KEYS`."""

    def repo_tree(self, repo: dict, ref: str | None = None) -> list[str]:
        """Top-level file names in *repo*, used to detect its stack. Permissive by
        contract: degrades to ``[]`` on any failure, never raises."""

    def repo_tree_strict(self, repo: dict, ref: str | None = None) -> list[str]:
        """Like :meth:`repo_tree`, but raises :class:`ForgeError` on failure instead of
        silently degrading to an empty list. Used by `discover`'s stack probe, which
        must distinguish "the probe failed" (network/auth/rate-limit) from "this repo
        genuinely has no recognised root-level stack file" — both looked identical
        through the permissive `repo_tree` (stack silently became "unknown" either way,
        and `discover` saved + exited 0 with no visibility into which happened)."""

    def open_change(self, path: str, branch: str) -> int | None:
        """The open MR/PR number for *branch*, or None."""

    def ci_status(self, path: str, branch: str) -> str | None:
        """The branch's CI result as one of :data:`CI_STATES`, or None.

        Left exactly as it is, and the change surface does not use it. The permissive
        discipline is correct where it renders; "improve it" is a separate change with its
        own blast radius."""

    def checks_at(self, path: str, sha: str, number: int | None = None) -> Checks:
        """Every check the forge would show a human at *sha*, as a :class:`Checks`.

        The requirement is a **property, not an endpoint**. GitHub's
        ``commits/<sha>/check-runs`` returns Check Runs only — Actions and Apps — so CI
        reporting through the Commit Statuses API instead (Jenkins, Buildkite, CircleCI)
        yields zero at a fully green head, which is this method's own failure arriving from
        the other direction: a permanent ``NOT RUN`` and a gate that never opens. GitLab has
        the mirror image, because a merged-results pipeline runs against
        ``refs/merge-requests/:iid/merge`` and its sha is not the branch head.

        So each backend reads everywhere it knows to look, and **where it cannot enumerate
        completely the answer is :data:`CHECKS_UNKNOWN`, never :data:`CHECKS_NOT_RUN`.*
        *number* is the request this head belongs to when the caller has it; it is what
        lets GitLab reach the merge request's own pipeline instead of a sha filter that
        cannot see one.

        Never raises: there is a designated value for *could not ask*, and it is
        ``total is None``.
        """

    def request_for(self, path: str, branch: str) -> Request | None:
        """The most recent request whose SOURCE branch is *branch*, in any state.

        ``None`` means the forge answered and there is no such request. A call that
        **failed** raises :class:`ForgeError` instead, because unlike :meth:`checks_at`
        this return type has no value that means *I could not ask* — and answering "no
        request" for a rate-limited lookup is how a member that is open reads as one that
        was never pushed."""

    def change_body(self, path: str, number: int) -> str:
        """Request *number*'s body, verbatim.

        A read, and strict: :meth:`update_change_body` replaces a body wholesale, so a
        failed read that degraded to ``""`` would splice charter's block into an empty
        document and delete whatever the author had written. Raises :class:`ForgeError`."""

    def create_change(self, path: str, base: str, head: str,
                      title: str, body: str) -> int:
        """Open a request from *head* onto *base*. Returns its number.

        Raises :class:`ForgeWriteError` on any failure. Never ``None`` — see that class."""

    def update_change_body(self, path: str, number: int, body: str) -> None:
        """Replace request *number*'s body. Raises :class:`ForgeWriteError` on failure."""

    def merge_change(self, path: str, number: int, method: str,
                     title: str, message: str) -> str:
        """Merge request *number* by *method* (one of :data:`MERGE_METHODS`), with
        *title* and *message* as the landing commit's subject and body — which is how the
        ``Charter-Change:`` trailer gets onto a commit charter authors.

        Returns the merge commit's sha. Raises :class:`ForgeWriteError` on failure,
        including a forge that reports success without one: there is deliberately no
        mergeability gate above this, so **the forge's refusal is the evidence** and it is
        reported in the forge's own words rather than re-diagnosed."""

    def credential_helper(self) -> str:
        """The git ``credential.helper`` value that makes git use this forge's token."""

    def insteadof(self) -> tuple[str, tuple[str, ...]]:
        """``(https_base, ssh_forms)`` — the SSH prefixes git must rewrite to HTTPS, so a
        repo whose remote is an SSH URL still transports over HTTPS with a token."""
