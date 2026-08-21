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
from .base import CI_STATES, ForgeError

#: GitHub rollup state → the neutral vocabulary. Anything unlisted becomes None.
_CI_MAP = {
    "SUCCESS": "success", "FAILURE": "failed", "ERROR": "failed",
    "PENDING": "pending", "EXPECTED": "pending",
}

_ROLLUP_QUERY = """
query($owner:String!, $name:String!, $ref:String!) {
  repository(owner:$owner, name:$name) {
    ref(qualifiedName:$ref) { target { ... on Commit {
      statusCheckRollup { state } } } }
  }
}
"""


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
        line, which renders every turn and must never crash."""
        p = util.run([self.cli, "api", "--hostname", self.host, path], check=False)
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
            p = util.run([self.cli, "api", "--hostname", self.host, path], check=False)
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
        p = util.run([self.cli, "auth", "status", "--hostname", self.host], check=False)
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
        p = util.run([self.cli, "api", "--hostname", self.host,
                      f"repos/{enc_owner}/{enc_name}/git/trees/{enc_ref}"], check=False)
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
        p = util.run([self.cli, "api", "graphql", "--hostname", self.host,
                      "-f", f"query={_ROLLUP_QUERY}",
                      "-f", f"owner={owner}", "-f", f"name={name}",
                      "-f", f"ref={branch}"], check=False)
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

    def credential_helper(self) -> str:
        return f"!{self.cli} auth git-credential"

    def insteadof(self) -> tuple[str, tuple[str, ...]]:
        return (f"https://{self.host}/",
                (f"git@{self.host}:", f"ssh://git@{self.host}/"))
