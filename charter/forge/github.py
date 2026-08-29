"""GitHub, over the `gh` CLI.

Two asymmetries with GitLab are handled here rather than leaking upward:

* **An owner may be an org or a user.** A personal account 404s on ``/orgs/{o}/repos``
  and succeeds on ``/users/{u}/repos`` with an identical record shape, so
  :meth:`GitHubForge.list_repos` tries the org endpoint first and falls back — but only
  on a genuine 404. Any other failure of the org endpoint (auth, network, rate limit) is
  *not* evidence the owner is a personal account, so it is raised rather than silently
  swallowed into a fallback attempt; and a failure of the user endpoint after a real 404
  fallback is a genuine failure too. Collapsing any of these into "no repos" is exactly
  the shape that let a `list_repos` failure wipe the inventory in the GitLab backend
  (see ``gitlab.py``'s ``_api_strict``) — ``list_repos`` here follows the same strict
  discipline, split from the permissive ``_api`` used by ``open_change``/``ci_status``,
  which feed the status line and must never raise.
* **There is no single CI status in REST** — a commit carries N check-runs plus legacy
  commit statuses. GitHub does compute a rollup, exposed only in GraphQL as
  ``statusCheckRollup.state``; we use that rather than inventing an aggregation, which
  would either lose information or state something GitHub does not.
"""
from __future__ import annotations

import json
import urllib.parse

from .. import util
from .base import (CHECKS_FAILED, CHECKS_PASSED, CHECKS_RUNNING,
                   CHECKS_UNKNOWN, CI_STATES, Checks, ForgeError, ForgeWriteError,
                   LIST_TIMEOUT, REQUEST_CLOSED, REQUEST_MERGED, REQUEST_OPEN,
                   STATUS_TIMEOUT, Request, worst)

#: GitHub rollup state → the neutral vocabulary. Anything unlisted becomes None.
_CI_MAP = {
    "SUCCESS": "success", "FAILURE": "failed", "ERROR": "failed",
    "PENDING": "pending", "EXPECTED": "pending",
}

#: A concluded check run's ``conclusion`` → charter's check vocabulary.
#:
#: Three of these are judgements the table in the spec would otherwise have left to an
#: implementer, and they are decided here so nobody re-decides them per backend.
#: ``skipped`` and ``neutral`` **count as passed** — they are how a forge says *nothing to
#: do here*, and a ``paths:`` filter or an ``if:`` condition produces them constantly, so
#: any other reading refuses the gate on most real repositories. ``action_required`` is
#: **failed**: it is the forge asking for a human, and a check waiting on a person did not
#: pass. Anything unlisted degrades to :data:`CHECKS_UNKNOWN` — never to passed.
_CONCLUSIONS = {
    "success": CHECKS_PASSED, "neutral": CHECKS_PASSED, "skipped": CHECKS_PASSED,
    "failure": CHECKS_FAILED, "cancelled": CHECKS_FAILED, "canceled": CHECKS_FAILED,
    "timed_out": CHECKS_FAILED, "startup_failure": CHECKS_FAILED,
    "action_required": CHECKS_FAILED,
}

#: A run the forge itself has DISOWNED. It does not count toward ``total`` at all: if it is
#: the only run at the head, ``NOT RUN`` is the honest answer, because GitHub has said this
#: result no longer describes this commit.
_STALE = "stale"

#: ``status`` values that mean the run has not concluded. A run whose ``status`` is none of
#: these and whose ``conclusion`` charter cannot map is ``unknown``.
_UNCONCLUDED = frozenset({"queued", "in_progress", "waiting", "pending", "requested"})

#: Combined **Commit Status** state → charter's check vocabulary. This is the other half of
#: "every check the forge would show a human": Jenkins, Buildkite and CircleCI's status
#: integration POST here and appear in no check-runs reply at all.
_STATUS_STATES = {
    "success": CHECKS_PASSED, "pending": CHECKS_RUNNING,
    "failure": CHECKS_FAILED, "error": CHECKS_FAILED,
}

#: How many pages of check runs charter will read before it declares it could not
#: enumerate them. A head with more than this many checks is not one charter can answer
#: about honestly, so it answers ``UNKNOWN`` rather than the subset it managed to read.
_MAX_PAGES = 10

_ROLLUP_QUERY = """
query($owner:String!, $name:String!, $ref:String!) {
  repository(owner:$owner, name:$name) {
    ref(qualifiedName:$ref) { target { ... on Commit {
      statusCheckRollup { state } } } }
  }
}
"""


def _run_state(run) -> str | None:
    """One check run's state, or ``None`` when it does not count toward ``total`` at all.

    ``None`` is only ever ``stale`` — a run GitHub itself has disowned. Everything else
    produces a state, because a run charter cannot read is still a run that exists, and
    dropping it would let an unreadable check leave a head looking like ``NOT RUN``.
    """
    run = run if isinstance(run, dict) else {}
    conclusion = (run.get("conclusion") or "").lower()
    if conclusion == _STALE:
        return None
    if conclusion:
        return _CONCLUSIONS.get(conclusion, CHECKS_UNKNOWN)
    status = (run.get("status") or "").lower()
    return CHECKS_RUNNING if status in _UNCONCLUDED else CHECKS_UNKNOWN


class _NotAnOrg(Exception):
    """Internal signal only: the org endpoint 404'd on page 1, meaning *owner* is
    (probably) a personal account rather than an org. Caught by :meth:`list_repos` to
    trigger the user-endpoint fallback; never escapes this module."""


class GitHubForge:
    kind = "github"
    cli = "gh"
    change_sigil = "#"
    owner_noun = "org"

    def __init__(self, host: str = "github.com") -> None:
        self.host = host

    # --- plumbing -----------------------------------------------------------------
    def _api(self, path: str):
        """Best-effort JSON GET. Returns None on any failure — callers feed the status
        line, which renders every turn and must never crash.

        A timeout is one of those failures. `ProcTimeout` is a `RuntimeError`, so left
        to escape it would reach `cli.main` — which catches only `KeyboardInterrupt` —
        as a traceback, from the one path documented never to crash (#324)."""
        try:
            p = util.run([self.cli, "api", "--hostname", self.host, path],
                         check=False, timeout=STATUS_TIMEOUT)
        except util.ProcTimeout:
            return None
        if p.returncode != 0:
            return None
        try:
            return json.loads(p.stdout) if p.stdout.strip() else None
        except ValueError:
            return None

    @staticmethod
    def _is_not_found(p) -> bool:
        """True when a failed call means "no such resource" (HTTP 404) — as opposed to
        an auth/network/rate-limit failure, which must never be mistaken for "this owner
        is a personal account, not an org". Verified live: `gh`'s stderr on a 404 reads
        ``gh: Not Found (HTTP 404)``, and the JSON error body it prints to stdout also
        carries ``"status":"404"``; check both since callers only control one in tests."""
        blob = f"{p.stdout or ''} {p.stderr or ''}"
        return "HTTP 404" in blob or '"status":"404"' in blob

    def _paged_strict(self, base_path: str, owner: str, org_probe: bool = False) -> list:
        """Page through *base_path*, raising :class:`ForgeError` on any failure so a
        failure can never be mistaken for "no repos" (see module docstring). When
        *org_probe* is set, a 404 on the very first page is *not* raised — it's reported
        via :class:`_NotAnOrg` so :meth:`list_repos` can fall back to the user endpoint;
        a 404 on any later page, or any non-404 failure at all, still raises."""
        out, page = [], 1
        while True:
            path = f"{base_path}?per_page=100&page={page}"
            try:
                p = util.run([self.cli, "api", "--hostname", self.host, path],
                             check=False, timeout=LIST_TIMEOUT)
            except util.ProcTimeout as e:
                # Reported in this path's own vocabulary rather than let a RuntimeError
                # traceback stand in for "the forge did not answer" (#324).
                raise ForgeError(
                    f"listing repos for GitHub owner '{owner}' {e}") from e
            if p.returncode != 0:
                if org_probe and page == 1 and self._is_not_found(p):
                    raise _NotAnOrg()
                detail = (p.stderr or p.stdout or "").strip() or f"gh exited {p.returncode}"
                raise ForgeError(
                    f"listing repos for GitHub owner '{owner}' failed ({path}): {detail}")
            if not p.stdout.strip():
                batch = []
            else:
                try:
                    batch = json.loads(p.stdout)
                except ValueError as e:
                    raise ForgeError(
                        f"GitHub API returned malformed JSON ({path}): {e}") from e
            if not batch:
                break
            out.extend(batch)
            if len(batch) < 100:
                break
            page += 1
        return out

    # --- protocol -----------------------------------------------------------------
    def check_auth(self) -> None:
        try:
            p = util.run([self.cli, "auth", "status", "--hostname", self.host],
                         check=False, timeout=STATUS_TIMEOUT)
        except util.ProcTimeout as e:
            raise ForgeError(f"gh did not answer for {self.host}: {e}") from e
        if p.returncode != 0:
            raise ForgeError(
                f"gh is not authenticated for {self.host}. Run: gh auth login")

    def list_repos(self, owner: str) -> list[dict]:
        # Encoded like every other URL segment here (#323). `owner` comes from a
        # `[[forge]]` block rather than from a forge response, so this is the ordinary
        # discipline rather than a fix for a reachable hole — but "which values happen to
        # be trusted today" is exactly the distinction that let `ci_status` drift away
        # from `open_change`, so the treatment is uniform instead of case-by-case.
        enc = urllib.parse.quote(owner, safe="")
        try:
            out = self._paged_strict(f"orgs/{enc}/repos", owner, org_probe=True)
        except _NotAnOrg:
            # A 404 on the org endpoint is expected for a personal account — not a
            # ForgeError. A failure of *this* call, though, is a genuine failure.
            out = self._paged_strict(f"users/{enc}/repos", owner)
        return [self._normalize(r) for r in out]

    def _normalize(self, r: dict) -> dict:
        return {
            "id": r.get("id"),
            "name": r.get("name"),
            "path_with_namespace": r.get("full_name"),
            "default_branch": r.get("default_branch"),
            "description": r.get("description") or "",
            "web_url": r.get("html_url") or "",
            "ssh_url": r.get("ssh_url") or "",
            "topics": r.get("topics") or [],
            "forge": self.kind,
        }

    def repo_tree(self, repo: dict, ref: str | None = None) -> list[str]:
        path = repo.get("path_with_namespace") or ""
        owner, _, name = path.partition("/")
        ref = ref or repo.get("default_branch") or "HEAD"
        enc_owner, enc_name = urllib.parse.quote(owner, safe=""), urllib.parse.quote(name, safe="")
        enc_ref = urllib.parse.quote(ref, safe="")
        data = self._api(f"repos/{enc_owner}/{enc_name}/git/trees/{enc_ref}")
        return [e.get("path", "") for e in (data or {}).get("tree", [])
                if "/" not in e.get("path", "")]

    def repo_tree_strict(self, repo: dict, ref: str | None = None) -> list[str]:
        """See :meth:`base.Forge.repo_tree_strict` — same lookup as :meth:`repo_tree`,
        but raises instead of degrading on failure (FINDING I5)."""
        path = repo.get("path_with_namespace") or ""
        owner, _, name = path.partition("/")
        ref = ref or repo.get("default_branch") or "HEAD"
        enc_owner, enc_name = urllib.parse.quote(owner, safe=""), urllib.parse.quote(name, safe="")
        enc_ref = urllib.parse.quote(ref, safe="")
        try:
            p = util.run([self.cli, "api", "--hostname", self.host,
                          f"repos/{enc_owner}/{enc_name}/git/trees/{enc_ref}"],
                         check=False, timeout=LIST_TIMEOUT)
        except util.ProcTimeout as e:
            raise ForgeError(f"listing tree for {path}@{ref} {e}") from e
        if p.returncode != 0:
            detail = (p.stderr or p.stdout or "").strip() or f"gh exited {p.returncode}"
            raise ForgeError(f"listing tree for {path}@{ref} failed: {detail}")
        if not p.stdout.strip():
            return []
        try:
            data = json.loads(p.stdout)
        except ValueError as e:
            raise ForgeError(f"GitHub API returned malformed JSON (tree {path}@{ref}): {e}") from e
        return [e.get("path", "") for e in (data or {}).get("tree", [])
                if "/" not in e.get("path", "")]

    def open_change(self, path: str, branch: str) -> int | None:
        owner, _, name = path.partition("/")
        enc_owner, enc_name = urllib.parse.quote(owner, safe=""), urllib.parse.quote(name, safe="")
        enc_branch = urllib.parse.quote(branch, safe="")
        arr = self._api(
            f"repos/{enc_owner}/{enc_name}/pulls?state=open&head={enc_owner}:{enc_branch}&per_page=1")
        return arr[0].get("number") if arr else None

    def ci_status(self, path: str, branch: str) -> str | None:
        owner, _, name = path.partition("/")
        # `-f/--raw-field`, never `-F/--field` (#323). These three values are not charter's:
        # `branch` is read out of the tree's `.git/HEAD` and `path` out of `git remote
        # get-url origin`, so both are written by whoever wrote the repo. `-F` gives a
        # value magic meaning — from `gh api --help`, "if the value starts with `@`, the
        # rest of the value is interpreted as a filename to read the value from. Pass `-`
        # to read from standard input" — which turned a status refresh into an arbitrary
        # local file read by a process holding the forge token, on a surface that renders
        # every 10s with no human in the loop. `-f` sends the value as a literal string.
        #
        # NOT percent-encoded, deliberately, and this is the one place that differs from
        # `open_change` above. Those are URL path/query segments, which the server decodes
        # again; these are GraphQL variables, which are JSON strings GitHub never decodes.
        # Encoding here would send `feature%2Fx` for `feature/x`, match no ref, and blank
        # the CI column for every branch with a slash in it. The flag is the fix, not the
        # value.
        try:
            p = util.run([self.cli, "api", "graphql", "--hostname", self.host,
                          "-f", f"query={_ROLLUP_QUERY}",
                          "-f", f"owner={owner}", "-f", f"name={name}",
                          "-f", f"ref={branch}"],
                         check=False, timeout=STATUS_TIMEOUT)
        except util.ProcTimeout:
            return None      # status-line path: a blank CI cell, never a raise (#324)
        if p.returncode != 0:
            return None
        try:
            data = json.loads(p.stdout)
        except ValueError:
            return None
        node = (((data.get("data") or {}).get("repository") or {}).get("ref") or {})
        rollup = ((node.get("target") or {}).get("statusCheckRollup") or {})
        state = _CI_MAP.get(rollup.get("state") or "")
        return state if state in CI_STATES else None

    # --- the change surface: reads ------------------------------------------------
    def _api_strict(self, path: str):
        """JSON GET that RAISES rather than degrading. The `_api_strict` GitLab already
        ships, in GitHub's own error vocabulary, for the one caller whose return type has
        no value meaning "I could not ask"."""
        try:
            p = util.run([self.cli, "api", "--hostname", self.host, path],
                         check=False, timeout=LIST_TIMEOUT)
        except util.ProcTimeout as e:
            raise ForgeError(f"GitHub API call ({path}) {e}") from e
        if p.returncode != 0:
            detail = (p.stderr or p.stdout or "").strip() or f"gh exited {p.returncode}"
            raise ForgeError(f"GitHub API call failed ({path}): {detail}")
        if not p.stdout.strip():
            return []
        try:
            return json.loads(p.stdout)
        except ValueError as e:
            raise ForgeError(f"GitHub API returned malformed JSON ({path}): {e}") from e

    def _check_runs(self, owner: str, name: str, sha: str) -> list[str] | None:
        """Per-run states for every Check Run at *sha*, or ``None`` if charter could not
        enumerate them **completely** — a failed call, or a reply whose ``total_count``
        says there is more than charter read. Incomplete is `unknown`, never `not_run`."""
        out: list[str] = []
        for page in range(1, _MAX_PAGES + 1):
            data = self._api(f"repos/{owner}/{name}/commits/{sha}/check-runs"
                             f"?per_page=100&page={page}")
            if not isinstance(data, dict):
                return None
            runs = data.get("check_runs")
            if not isinstance(runs, list):
                return None
            for r in runs:
                state = _run_state(r)
                if state is not None:                # `stale` is dropped, not counted
                    out.append(state)
            if len(runs) < 100:
                return out                           # a short page is the last page
        return None                                  # more pages than charter will read

    def _commit_statuses(self, owner: str, name: str, sha: str) -> list[str] | None:
        """Per-status states for every Commit Status at *sha*, or ``None``.

        The endpoint deduplicates to the latest status per context, which is what GitHub
        shows a human, so no aggregation is invented here."""
        data = self._api(f"repos/{owner}/{name}/commits/{sha}/status?per_page=100")
        if not isinstance(data, dict):
            return None
        statuses = data.get("statuses")
        if not isinstance(statuses, list):
            return None
        declared = data.get("total_count")
        if isinstance(declared, int) and declared > len(statuses):
            return None                       # more than one page: not enumerated
        return [_STATUS_STATES.get((s or {}).get("state") or "", CHECKS_UNKNOWN)
                for s in statuses]

    def checks_at(self, path: str, sha: str, number: int | None = None) -> Checks:
        """See :meth:`base.Forge.checks_at`. **Two reads, summed into one total.**

        The check-runs endpoint alone misses every Jenkins/Buildkite/CircleCI status and
        would render a green head as ``NOT RUN``, permanently — this section's own failure
        arriving from the other direction, against a gate that deliberately offers no
        ``--force``. *number* is unused here: GitHub keys checks to the commit, so the
        pull request adds nothing a sha lookup does not already have."""
        owner, _, name = path.partition("/")
        enc_owner = urllib.parse.quote(owner, safe="")
        enc_name = urllib.parse.quote(name, safe="")
        enc_sha = urllib.parse.quote(sha, safe="")
        runs = self._check_runs(enc_owner, enc_name, enc_sha)
        if runs is None:
            return Checks(None, CHECKS_UNKNOWN)
        statuses = self._commit_statuses(enc_owner, enc_name, enc_sha)
        if statuses is None:
            return Checks(None, CHECKS_UNKNOWN)
        seen = runs + statuses
        return Checks(len(seen), worst(seen))

    def request_for(self, path: str, branch: str) -> Request | None:
        """See :meth:`base.Forge.request_for`. ``state=all``, newest first — the one thing
        `open_change` cannot do, and the reason ``REJECTED`` is derivable at all."""
        owner, _, name = path.partition("/")
        enc_owner = urllib.parse.quote(owner, safe="")
        enc_name = urllib.parse.quote(name, safe="")
        enc_branch = urllib.parse.quote(branch, safe="")
        arr = self._api_strict(
            f"repos/{enc_owner}/{enc_name}/pulls?state=all"
            f"&head={enc_owner}:{enc_branch}&sort=created&direction=desc&per_page=1")
        if not arr:
            return None
        pr = arr[0] if isinstance(arr, list) else {}
        number = pr.get("number")
        if not isinstance(number, int):
            raise ForgeError(
                f"GitHub returned a pull request for {path}@{branch} with no number")
        merged_at = pr.get("merged_at")
        if merged_at:
            state, merge = REQUEST_MERGED, pr.get("merge_commit_sha") or None
        elif (pr.get("state") or "") == "closed":
            state, merge = REQUEST_CLOSED, None
        else:
            # `merge_commit_sha` is populated on an OPEN pull request too, with the sha of
            # a throwaway test merge that is on no branch. Read only when merged.
            state, merge = REQUEST_OPEN, None
        return Request(number=number, state=state,
                       head=((pr.get("head") or {}).get("sha") or ""), merge=merge)

    def change_body(self, path: str, number: int) -> str:
        owner, _, name = path.partition("/")
        enc_owner = urllib.parse.quote(owner, safe="")
        enc_name = urllib.parse.quote(name, safe="")
        data = self._api_strict(
            f"repos/{enc_owner}/{enc_name}/pulls/{int(number)}")
        if not isinstance(data, dict):
            raise ForgeError(f"GitHub returned no pull request #{int(number)} on {path}")
        return data.get("body") or ""

    # --- the change surface: writes ------------------------------------------------
    def _write(self, args: list[str], what: str):
        """One write, and never through :meth:`_api`. ADR 0002: *a write needs to fail
        loudly, which means it needs a path that is not ``_api``*."""
        try:
            p = util.run([self.cli, "api", "--hostname", self.host, *args],
                         check=False, timeout=LIST_TIMEOUT)
        except util.ProcTimeout as e:
            raise ForgeWriteError(f"{what} {e}") from e
        if p.returncode != 0:
            detail = (p.stderr or p.stdout or "").strip() or f"gh exited {p.returncode}"
            raise ForgeWriteError(f"{what} failed: {detail}")
        if not p.stdout.strip():
            return None
        try:
            return json.loads(p.stdout)
        except ValueError as e:
            raise ForgeWriteError(f"{what}: GitHub returned malformed JSON: {e}") from e

    def create_change(self, path: str, base: str, head: str,
                      title: str, body: str) -> int:
        # `-f/--raw-field`, never `-F/--field` (#323). `-F` gives an `@`-prefixed value
        # file-read semantics, which turned a status refresh into an arbitrary local file
        # read by a process holding the forge token. A change's title and body carry the
        # `why` and the member names — committed values from someone else's machine — so
        # this applies HARDER on a write than it did on the read it was found in.
        owner, _, name = path.partition("/")
        enc_owner = urllib.parse.quote(owner, safe="")
        enc_name = urllib.parse.quote(name, safe="")
        what = f"opening a pull request on {path} from '{head}' onto '{base}'"
        data = self._write([f"repos/{enc_owner}/{enc_name}/pulls", "-X", "POST",
                            "-f", f"title={title}", "-f", f"body={body}",
                            "-f", f"head={head}", "-f", f"base={base}"], what)
        number = data.get("number") if isinstance(data, dict) else None
        if not isinstance(number, int):
            raise ForgeWriteError(f"{what}: GitHub returned no pull request number")
        return number

    def update_change_body(self, path: str, number: int, body: str) -> None:
        owner, _, name = path.partition("/")
        enc_owner = urllib.parse.quote(owner, safe="")
        enc_name = urllib.parse.quote(name, safe="")
        self._write([f"repos/{enc_owner}/{enc_name}/pulls/{int(number)}", "-X", "PATCH",
                     "-f", f"body={body}"],
                    f"updating pull request #{int(number)} on {path}")

    def merge_change(self, path: str, number: int, method: str,
                     title: str, message: str) -> str:
        owner, _, name = path.partition("/")
        enc_owner = urllib.parse.quote(owner, safe="")
        enc_name = urllib.parse.quote(name, safe="")
        what = f"merging pull request #{int(number)} on {path} ({method})"
        data = self._write(
            [f"repos/{enc_owner}/{enc_name}/pulls/{int(number)}/merge", "-X", "PUT",
             "-f", f"merge_method={method}",
             "-f", f"commit_title={title}", "-f", f"commit_message={message}"], what)
        data = data if isinstance(data, dict) else {}
        sha = data.get("sha")
        if not data.get("merged") or not isinstance(sha, str) or not sha:
            raise ForgeWriteError(
                f"{what}: GitHub did not confirm a merge "
                f"({(data.get('message') or 'no message')!r})")
        return sha

    def credential_helper(self) -> str:
        return f"!{self.cli} auth git-credential"

    def insteadof(self) -> tuple[str, tuple[str, ...]]:
        return (f"https://{self.host}/",
                (f"git@{self.host}:", f"ssh://git@{self.host}/"))
