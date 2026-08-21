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

from typing import Protocol, runtime_checkable

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
        """The branch's CI result as one of :data:`CI_STATES`, or None."""

    def credential_helper(self) -> str:
        """The git ``credential.helper`` value that makes git use this forge's token."""

    def insteadof(self) -> tuple[str, tuple[str, ...]]:
        """``(https_base, ssh_forms)`` — the SSH prefixes git must rewrite to HTTPS, so a
        repo whose remote is an SSH URL still transports over HTTPS with a token."""
