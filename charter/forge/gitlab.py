"""GitLab, over the `glab` CLI."""
from __future__ import annotations

import json
import urllib.parse

from .. import util
from .base import (CHECKS_FAILED, CHECKS_PASSED, CHECKS_RUNNING, CHECKS_UNKNOWN,
                   CI_STATES, Checks, ForgeError, ForgeWriteError, LIST_TIMEOUT,
                   REQUEST_CLOSED, REQUEST_MERGED, REQUEST_OPEN, STATUS_TIMEOUT,
                   Request, worst)

#: GitLab pipeline status → the neutral vocabulary. Anything unlisted becomes None
#: rather than being invented, so a new upstream state degrades to "unknown", not a lie.
_CI_MAP = {
    "success": "success", "failed": "failed", "running": "running",
    "canceled": "canceled", "skipped": "skipped", "manual": "manual",
    "pending": "pending", "created": "pending", "preparing": "pending",
    "waiting_for_resource": "pending", "scheduled": "pending",
}

#: GitLab **pipeline** status → charter's check vocabulary, read at pipeline level and
#: never at job level. A GitLab job carrying ``manual`` is an ordinary deploy step waiting
#: for somebody to click it, and the pipeline is ``success`` regardless unless that job
#: blocks — so reading jobs would refuse the gate on most real GitLab repositories, which
#: is the same mistake ``skipped``/``neutral`` would be on GitHub.
#:
#: ``manual`` at the PIPELINE level is different: it is a blocking manual job the pipeline
#: is waiting on, which is GitLab's ``action_required`` and is therefore not a pass.
#: ``canceled`` is a failure for the same reason it is on GitHub — nobody saw the result.
#: Unlisted degrades to :data:`CHECKS_UNKNOWN`, never to passed.
_PIPELINE_STATES = {
    "success": CHECKS_PASSED, "skipped": CHECKS_PASSED,
    "failed": CHECKS_FAILED, "canceled": CHECKS_FAILED, "cancelled": CHECKS_FAILED,
    "manual": CHECKS_FAILED, "blocked": CHECKS_FAILED,
    "running": CHECKS_RUNNING, "pending": CHECKS_RUNNING, "created": CHECKS_RUNNING,
    "preparing": CHECKS_RUNNING, "waiting_for_resource": CHECKS_RUNNING,
    "scheduled": CHECKS_RUNNING,
}


def _pipeline_state(pipeline) -> str:
    """One pipeline's state in charter's check vocabulary. Unmapped is ``unknown``."""
    pipeline = pipeline if isinstance(pipeline, dict) else {}
    return _PIPELINE_STATES.get((pipeline.get("status") or "").lower(), CHECKS_UNKNOWN)


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

    # --- the change surface: reads ------------------------------------------------
    def checks_at(self, path: str, sha: str, number: int | None = None) -> Checks:
        """See :meth:`base.Forge.checks_at`.

        **GitLab's blind spot is the mirror image of GitHub's.** With merged results
        enabled a merge request's pipeline runs against ``refs/merge-requests/:iid/merge``,
        whose sha is a merge commit GitLab created and is **not** the branch head — so
        ``pipelines?sha=<head>`` comes back empty on a fully green merge request. Reading
        that as ``NOT RUN`` would be a permanent refusal on every GitLab project that
        enabled the feature.

        So the enumeration is the merge request's **head pipeline** — the one thing GitLab
        shows a human in the widget, whatever ref it ran against — and the staleness
        question is answered by the merge request's own ``sha`` rather than by the
        pipeline's. External CI is covered by the same read: a commit status POSTed to
        GitLab creates a pipeline of source ``external``, so pipelines are the complete
        enumeration here and no second endpoint is needed.

        Without *number* there is no merge request to ask, a bare sha filter cannot see a
        merged-results pipeline, and an empty answer therefore licenses **nothing**:
        charter says ``UNKNOWN``. ``NOT RUN`` asserts nothing ran, and charter may only
        assert it having looked everywhere it knows to look.
        """
        enc = urllib.parse.quote(path, safe="")
        enc_sha = urllib.parse.quote(sha, safe="")
        if number is None:
            arr = self._api(f"projects/{enc}/pipelines?sha={enc_sha}&per_page=100")
            if not isinstance(arr, list) or not arr:
                return Checks(None, CHECKS_UNKNOWN)
            return Checks(len(arr), worst([_pipeline_state(p) for p in arr]))
        mr = self._api(f"projects/{enc}/merge_requests/{int(number)}")
        if not isinstance(mr, dict):
            return Checks(None, CHECKS_UNKNOWN)
        if (mr.get("sha") or "") != sha:
            # The merge request has moved off the sha charter was asked about, so its head
            # pipeline is a check on some other head. Nothing here describes THIS one.
            return Checks(None, CHECKS_UNKNOWN)
        head = mr.get("head_pipeline")
        if not isinstance(head, dict):
            return Checks(0, worst([]))
        return Checks(1, _pipeline_state(head))

    def request_for(self, path: str, branch: str) -> Request | None:
        """See :meth:`base.Forge.request_for`. ``state=all``, newest first."""
        enc = urllib.parse.quote(path, safe="")
        br = urllib.parse.quote(branch, safe="")
        arr = self._api_strict(
            f"projects/{enc}/merge_requests?state=all&source_branch={br}"
            f"&order_by=created_at&sort=desc&per_page=1")
        if not arr:
            return None
        mr = arr[0] if isinstance(arr, list) else {}
        iid = mr.get("iid")
        if not isinstance(iid, int):
            raise ForgeError(
                f"GitLab returned a merge request for {path}@{branch} with no iid")
        gl_state = (mr.get("state") or "").lower()
        if gl_state == "merged":
            state = REQUEST_MERGED
            merge = mr.get("merge_commit_sha") or mr.get("squash_commit_sha") or None
        elif gl_state in ("closed", "locked"):
            state, merge = REQUEST_CLOSED, None
        else:
            state, merge = REQUEST_OPEN, None
        return Request(number=iid, state=state, head=(mr.get("sha") or ""), merge=merge)

    def change_body(self, path: str, number: int) -> str:
        enc = urllib.parse.quote(path, safe="")
        data = self._api_strict(f"projects/{enc}/merge_requests/{int(number)}")
        if not isinstance(data, dict):
            raise ForgeError(f"GitLab returned no merge request !{int(number)} on {path}")
        return data.get("description") or ""

    # --- the change surface: writes ------------------------------------------------
    def _write(self, args: list[str], what: str):
        """One write, and never through :meth:`_api` — ADR 0002's third discipline."""
        try:
            p = self._glab(["api", *args], check=False, timeout=LIST_TIMEOUT)
        except util.ProcTimeout as e:
            raise ForgeWriteError(f"{what} {e}") from e
        if p.returncode != 0:
            detail = (p.stderr or p.stdout or "").strip() or f"glab exited {p.returncode}"
            raise ForgeWriteError(f"{what} failed: {detail}")
        if not p.stdout.strip():
            return None
        try:
            return json.loads(p.stdout)
        except ValueError as e:
            raise ForgeWriteError(f"{what}: GitLab returned malformed JSON: {e}") from e

    def create_change(self, path: str, base: str, head: str,
                      title: str, body: str) -> int:
        # `-f/--raw-field`, never `-F/--field` (#323) — see `github.create_change`.
        enc = urllib.parse.quote(path, safe="")
        what = f"opening a merge request on {path} from '{head}' onto '{base}'"
        data = self._write([f"projects/{enc}/merge_requests", "-X", "POST",
                            "-f", f"source_branch={head}", "-f", f"target_branch={base}",
                            "-f", f"title={title}", "-f", f"description={body}"], what)
        iid = data.get("iid") if isinstance(data, dict) else None
        if not isinstance(iid, int):
            raise ForgeWriteError(f"{what}: GitLab returned no merge request iid")
        return iid

    def update_change_body(self, path: str, number: int, body: str) -> None:
        enc = urllib.parse.quote(path, safe="")
        self._write([f"projects/{enc}/merge_requests/{int(number)}", "-X", "PUT",
                     "-f", f"description={body}"],
                    f"updating merge request !{int(number)} on {path}")

    def merge_change(self, path: str, number: int, method: str,
                     title: str, message: str) -> str:
        enc = urllib.parse.quote(path, safe="")
        what = f"merging merge request !{int(number)} on {path} ({method})"
        args = [f"projects/{enc}/merge_requests/{int(number)}/merge", "-X", "PUT"]
        if method == "squash":
            args += ["-f", "squash=true",
                     "-f", f"squash_commit_message={title}\n\n{message}"]
        args += ["-f", f"merge_commit_message={title}\n\n{message}"]
        data = self._write(args, what)
        data = data if isinstance(data, dict) else {}
        sha = data.get("merge_commit_sha") or data.get("squash_commit_sha")
        if (data.get("state") or "") != "merged" or not isinstance(sha, str) or not sha:
            raise ForgeWriteError(
                f"{what}: GitLab did not confirm a merge "
                f"(state {(data.get('state') or 'unknown')!r})")
        return sha

    def credential_helper(self) -> str:
        return f"!{self.cli} auth git-credential"

    def insteadof(self) -> tuple[str, tuple[str, ...]]:
        return (f"https://{self.host}/",
                (f"git@{self.host}:", f"ssh://git@{self.host}/"))
