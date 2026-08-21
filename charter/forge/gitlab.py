"""GitLab, over the `glab` CLI."""
from __future__ import annotations

import json
import urllib.parse

from .. import util
from .base import CI_STATES, LIST_TIMEOUT, STATUS_TIMEOUT, ForgeError

#: GitLab pipeline status → the neutral vocabulary. Anything unlisted becomes None
#: rather than being invented, so a new upstream state degrades to "unknown", not a lie.
_CI_MAP = {
    "success": "success", "failed": "failed", "running": "running",
    "canceled": "canceled", "skipped": "skipped", "manual": "manual",
    "pending": "pending", "created": "pending", "preparing": "pending",
    "waiting_for_resource": "pending", "scheduled": "pending",
}


class GitLabForge:
    kind = "gitlab"
    cli = "glab"
    change_sigil = "!"
    owner_noun = "group"

    def __init__(self, host: str = "gitlab.com") -> None:
        self.host = host

    # --- plumbing -----------------------------------------------------------------
    def _glab(self, args, check: bool = True, timeout: float = STATUS_TIMEOUT):
        """Every invocation is explicit about ITS OWN host (``--hostname``) — mirrors
        `GitHubForge`, which has always passed `--hostname self.host` on every call.
        Before this, a declared self-hosted GitLab (``host = "git.internal"``) silently
        queried gitlab.com instead (glab's ambient default host), and `check_auth`
        reported success for `git.internal` merely because gitlab.com happened to be
        logged in — a false green on the auth axis (FINDING I2).

        Bounded by ``timeout`` at every caller, defaulting to the tighter of the two
        budgets: this is the single chokepoint for `glab`, so an unbounded default here
        would be an unbounded call anywhere a future method forgets to say otherwise
        (#324)."""
        return util.run([self.cli, "--hostname", self.host, *args],
                        check=check, timeout=timeout)

    def _api(self, path: str):
        """Best-effort JSON GET. Returns None on any failure — callers feed the status
        line, which renders every turn and must never crash. A timeout is one of those
        failures: `ProcTimeout` is a `RuntimeError` and would otherwise escape this path
        as a traceback (#324)."""
        try:
            p = self._glab(["api", path], check=False, timeout=STATUS_TIMEOUT)
        except util.ProcTimeout:
            return None
        if p.returncode != 0:
            return None
        try:
            return json.loads(p.stdout) if p.stdout.strip() else None
        except ValueError:
            return None

    def _api_strict(self, path: str):
        """JSON GET for pagination-driven callers that must tell "the call failed" apart
        from "the result was empty" — an expired token and a genuinely empty group must
        never look the same, because collapsing them is how a `list_repos` failure
        silently wipes the inventory. Raises :class:`ForgeError` (naming the failing path
        and glab's own error text) on a non-zero exit or unparsable JSON; a clean empty
        body is returned as ``[]``, which is a legal, successful result. A timeout is
        reported the same way, for the same reason (#324)."""
        try:
            p = self._glab(["api", path], check=False, timeout=LIST_TIMEOUT)
        except util.ProcTimeout as e:
            raise ForgeError(f"GitLab API call ({path}) {e}") from e
        if p.returncode != 0:
            detail = (p.stderr or p.stdout or "").strip() or f"glab exited {p.returncode}"
            raise ForgeError(f"GitLab API call failed ({path}): {detail}")
        if not p.stdout.strip():
            return []
        try:
            return json.loads(p.stdout)
        except ValueError as e:
            raise ForgeError(f"GitLab API returned malformed JSON ({path}): {e}") from e

    # --- protocol -----------------------------------------------------------------
    def check_auth(self) -> None:
        try:
            p = self._glab(["auth", "status"], check=False, timeout=STATUS_TIMEOUT)
        except util.ProcTimeout as e:
            raise ForgeError(f"glab did not answer for {self.host}: {e}") from e
        blob = (p.stdout or "") + (p.stderr or "")
        if p.returncode != 0 or "Logged in" not in blob:
            raise ForgeError(
                f"glab is not authenticated for {self.host}. Run: glab auth login")

    def list_repos(self, owner: str) -> list[dict]:
        enc = urllib.parse.quote(owner, safe="")
        out, page = [], 1
        while True:
            try:
                batch = self._api_strict(
                    f"groups/{enc}/projects?per_page=100&page={page}"
                    "&include_subgroups=true&archived=false")
            except ForgeError as e:
                raise ForgeError(f"listing repos for GitLab group '{owner}' failed: {e}") from e
            if not batch:
                break
            out.extend(batch)
            if len(batch) < 100:
                break
            page += 1
        return [self._normalize(p) for p in out]

    def _normalize(self, p: dict) -> dict:
        return {
            "id": p.get("id"),
            "name": p.get("path") or p.get("name"),
            "path_with_namespace": p.get("path_with_namespace"),
            "default_branch": p.get("default_branch"),
            "description": p.get("description") or "",
            "web_url": p.get("web_url") or "",
            "ssh_url": p.get("ssh_url_to_repo") or "",
            "topics": p.get("topics") or [],
            "forge": self.kind,
        }

    def repo_tree(self, repo: dict, ref: str | None = None) -> list[str]:
        # `id` comes off a forge response, same as `ref` beside it — encoded for the same
        # reason (#323), rather than trusted because it is "normally an integer".
        rid = urllib.parse.quote(str(repo.get("id")), safe="")
        ref_q = f"&ref={urllib.parse.quote(str(ref), safe='')}" if ref else ""
        out, page = [], 1
        while True:
            batch = self._api(
                f"projects/{rid}/repository/tree?per_page=100&page={page}{ref_q}") or []
            if not batch:
                break
            out.extend(batch)
            if len(batch) < 100:
                break
            page += 1
        return [e.get("name", "") for e in out]

    def repo_tree_strict(self, repo: dict, ref: str | None = None) -> list[str]:
        """See :meth:`base.Forge.repo_tree_strict` — same pagination as :meth:`repo_tree`,
        but through :meth:`_api_strict` so a failure raises instead of degrading (FINDING
        I5)."""
        rid = urllib.parse.quote(str(repo.get("id")), safe="")
        ref_q = f"&ref={urllib.parse.quote(str(ref), safe='')}" if ref else ""
        out, page = [], 1
        while True:
            batch = self._api_strict(
                f"projects/{rid}/repository/tree?per_page=100&page={page}{ref_q}")
            if not batch:
                break
            out.extend(batch)
            if len(batch) < 100:
                break
            page += 1
        return [e.get("name", "") for e in out]

    def open_change(self, path: str, branch: str) -> int | None:
        enc = urllib.parse.quote(path, safe="")
        br = urllib.parse.quote(branch, safe="")
        arr = self._api(
            f"projects/{enc}/merge_requests?state=opened&source_branch={br}&per_page=1")
        return arr[0].get("iid") if arr else None

    def ci_status(self, path: str, branch: str) -> str | None:
        enc = urllib.parse.quote(path, safe="")
        br = urllib.parse.quote(branch, safe="")
        arr = self._api(f"projects/{enc}/pipelines?ref={br}&per_page=1")
        if not arr:
            return None
        state = _CI_MAP.get(arr[0].get("status") or "")
        return state if state in CI_STATES else None

    def credential_helper(self) -> str:
        return f"!{self.cli} auth git-credential"

    def insteadof(self) -> tuple[str, tuple[str, ...]]:
        return (f"https://{self.host}/",
                (f"git@{self.host}:", f"ssh://git@{self.host}/"))
