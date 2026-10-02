# Forges: GitLab and GitHub

A **forge** is a code-hosting platform `charter` talks to — GitLab or GitHub today. Every
forge operation `charter` performs goes through **one seam**: a small set of traits, one per
area, that each forge implements once ([ADR 0070](../../../docs/adr/0070-a-forge-is-one-seam-with-a-native-client-per-forge-and-gh-and-glab-are-its-fallback.md)).
A backend builds each request once, and a **transport** sends it. There are two. The **CLI
transport** is that forge's own official CLI (`gh api`, `glab api`), authenticated once, over
HTTPS, so the token stays in the CLI. The **native transport** (GitHub's, FW-2a; GitLab's is
FW-2b) sends the same request over HTTPS with a token charter holds, and makes repeated reads
conditional against a per-account ETag store. `forge::route` decides which a call takes, from
its account and its `Caller`: **only a human in the window, on an account charter holds a
sign-in for, takes the native transport**; a chat, an MCP call, a trigger and every `charter`
command take the CLI, and no failure is retried on the other. Until FW-1 and FW-3a give charter
a sign-in, nothing holds one, so every call still takes the CLI. [git-policy.md](git-policy.md) says why the CLI's own login
matters to an autonomous agent specifically.

## GitLab

- **What it needs:** [`glab`](https://gitlab.com/gitlab-org/cli) installed and
  authenticated (`glab auth login`, then `glab auth status` should say "Logged in").
  `charter doctor` lists a row for the CLI and one for its auth, and in this version says
  of both that they are not checked yet; `charter discover` asks `auth status` itself
  before it lists anything.
- **What "group" means:** the GitLab group (or subgroup) whose projects this forge block
  tracks. `include_subgroups` is always on, so a group tracks everything beneath it too.
- **Default host:** `gitlab.com`. Declare `host = "gitlab.example.com"` in the
  `[[forge]]` block for a self-hosted instance (GitLab Enterprise/CE) — every `glab`
  call is then made with `--hostname` set to that host explicitly, so it never silently
  falls back to whatever `glab`'s own ambient default happens to be.

## GitHub

- **What it needs:** [`gh`](https://cli.github.com/) installed and authenticated (`gh
  auth login`, then `gh auth status`).
- **What "owner" means:** a GitHub **org** or a personal **user** account. `charter`
  tries the org endpoint first and falls back to the user endpoint on a genuine 404 —
  you don't have to say which one it is.
- **Default host:** `github.com`. Declare `host = "github.example.com"` for a GitHub
  Enterprise Server instance, the same way as GitLab above.

## Which forge governs a given repo

Every repo record in `inventory/repos.json` carries a `forge` stamp (which backend
produced it) so a mixed inventory stays unambiguous, and every clone's *own* git policy
(`charter git-policy`) is resolved from **its own `origin` remote**, not from whichever
forge happens to be first in `charter.toml`. A self-hosted GitLab clone gets `glab`'s
credential helper and *its own host's* SSH→HTTPS rewrite; a `github.com` clone gets
`gh`'s. This is what lets a mixed-forge control plane's clones each authenticate
correctly without you telling `charter` which is which per repo.

## What charter asks a forge

Every operation is a method of an area trait in `src/forge/backend.rs`, with a GitHub body in
`src/forge/github.rs` and a GitLab body in `src/forge/gitlab.rs`. Nothing else in the core
builds a forge request or matches on the forge to do so. Every call carries a `Caller` (which
surface asked, and whether a person is waiting); the account and principal join it with FW-1.

There are two disciplines, and they stay with the caller: the trait returns what happened.

**Strict:** a failure is an error, because collapsing "the call failed" into "the result was
empty" is how a rate-limited lookup wipes an inventory, or a save opens a second request.

**Permissive:** the status line's open request and CI word (`charter gl-refresh`). Any failure
answers nothing. Being wrong costs a blank column, retried at the next refresh, and this path
must never break a surface that draws all the time.

### The parity table

Measured against GitHub's REST API (version `2022-11-28`) and GraphQL schema, and against the
GitLab 19.4 REST documentation (`gitlab-org/gitlab`, branch `19-4-stable-ee`, `doc/api/`).
Each row has a contract case on both forges in `tests/forge_contract.rs`, run against the
recordings in `tests/forge_contract/<forge>/`, and the CLI transport's argv for each is pinned
by `tests/a_forge_cli_is_asked_exactly_what_python_asked.rs` and
`tests/a_pr_is_opened_or_updated_and_set_to_auto_merge.rs`.

| Area · method | Who calls it | GitHub | GitLab | Discipline | Parity |
|---|---|---|---|---|---|
| `Repos::owned` | `charter discover` | `GET orgs/{owner}/repos`, then `users/{owner}/repos` on a 404 | `GET groups/{owner}/projects?include_subgroups=true&archived=false` | strict | **gaps 1, 2** (#803, #804) |
| `Repos::reachable` | the repo picker (ADR 0055) | `GET user/repos?affiliation=owner,collaborator,organization_member` | `GET projects?membership=true&archived=false` | strict | same |
| `Repos::top_level` | `discover`'s stack probe | `GET repos/{o}/{r}/git/trees/{ref}` (ref, else default branch, else `HEAD`) | `GET projects/{id}/repository/tree` (ref, else the default branch) | strict | same |
| `Requests::open_or_update` | a PR-mode save (ADR 0051), `charter change push` | `GET pulls?state=open&head={o}:{b}&base=…`, then `PATCH` or `POST pulls` | `GET merge_requests?state=opened&source_branch=…&target_branch=…`, own project only, then `PUT` or `POST` | strict | same |
| `Requests::state` | a PR-mode save | `GET pulls/{n}`: `closed` + `merged` is merged, at `merge_commit_sha` | `GET merge_requests/{iid}`: `merged`, at `squash_commit_sha` else `merge_commit_sha` | strict | same |
| `Requests::by_head` | `charter change show` and `push` (ADR 0060) | `GET pulls?state=all&head={o}:{b}`, checked against `head.repo.full_name` | `GET merge_requests?source_branch=…&state=all`, own project only | strict | same |
| `Requests::body` | `charter change push` (ADR 0060) | `GET pulls/{n}`, its `body` (`null` is empty) | `GET merge_requests/{iid}`, its `description` (`null` is empty) | strict | same |
| `Requests::set_body` | `charter change push` | `PATCH pulls/{n}` with `body` alone | `PUT merge_requests/{iid}` with `description` alone | strict | same |
| `Requests::request_auto_merge` | a PR-merge save | GraphQL `enablePullRequestAutoMerge` with `expectedHeadOid` | `PUT merge_requests/{iid}/merge` with `merge_when_pipeline_succeeds` and `sha`, only while a pipeline runs | strict | **gap 3** (#805) |
| `Requests::checks_at` | `charter change show` | check runs **and** commit statuses at the sha, each read whole | the request's pipelines at the sha; the newest decides | strict, `UNKNOWN` on failure | same, by design |
| `Requests::open_on_branch` | `gl-refresh` | `GET pulls?state=open&head={o}:{b}&per_page=1` | `GET merge_requests?state=opened&source_branch=…&per_page=100`, own project only | permissive | same (fixed here) |
| `Requests::ci_word` | `gl-refresh` | GraphQL `statusCheckRollup.state` (5 values) | `GET pipelines?ref=…&per_page=1`, its `status` (13 values) | permissive | same |

### `charter change push`

`charter change push <slug>` is the first change verb that writes to a forge (ADR 0060, #471).
For each member it reads the forge, the repository path and the HTTPS push URL from the
member's own repo's `origin`, and prints every repo, branch and destination before it pushes
anything. Each push is `git push <https-url> refs/heads/<branch>:refs/heads/<branch>`, through
the forge CLI's credential helper and nothing else: no `+`, no `--force` of any spelling, so it
can only create the branch or fast-forward it. The push's own command line turns off tags,
push options and submodules. The destination printed is the one git pushes to: a repo whose
git config would send the push elsewhere is refused, and the setting is named. It commits
nothing, and it pushes a repo whose `[repos.<name>] mode` is `off` too (ADR 0051, amended by
ADR 0060 D4).

Then `by_head` finds the member's request in any state. When there is none, `open_or_update`
opens one into the repo's default branch, titled `<slug>: <repo>`, with the change's `why` and
an empty cross-link block as its description. Last, every request's description is read
(`body`) and the block between charter's two markers is replaced with one that names every
member's request, `—` for a member charter could not reach. A description that is already
current is not written (`set_body`), so a second run asks the forge nothing new. Charter writes
only between its markers: a description without exactly one pair of them outside a code fence
(a fence closes only on its own kind) is left alone and named, and everything outside the block
keeps its line endings. The markers are the Python charter's, to the byte.

A row names its request the way the forge rendering the description resolves it: a reference
(`acme/widget#7` on GitHub, `acme/plat/widget!7` on GitLab, the full group path) for a request on
the description's own host, and the request's URL for one on another host. A reference is
looked up by the forge that renders it, so `acme/widget#7` in a GitLab description is GitLab's
issue 7 of a GitLab repo `acme/widget`, and a self-managed GitLab beside gitlab.com is a
different host too.

**On GitLab** (GL-3b), the same steps are GitLab's merge request calls in the table above. A
merge request in any state from the repo's own branch is adopted, a draft included: only
its `description` is written, so it stays a draft and keeps its title. One from a fork with the
same branch name is never adopted, written or merged; charter opens the repo's own. A
`"description": null` holds no block, and is named like any description without one. A
self-managed GitLab is pushed to and asked at the host `charter.toml` declares for it.

One member's failure costs only that member. A member that is not a repo in the workspace is
refused by name. The exit is 1 when any member was not pushed, opened or written, as the Python
charter answered, and 2 only when the whole command is refused.

**Not behind the seam, and why:**

| Call | Where | Why |
|---|---|---|
| `gh auth status` / `glab auth status` | `Forge::check_auth`, the CLI transport's own check | It asks whether the transport can speak as someone, not the forge anything. The native transport answers it from its own sign-in |
| `gh search issues`, `gh issue create` | `report.rs`, through `forge::gh_as_the_operator` | It files on charter's own tracker, which is on GitHub whatever forge a project uses, under the reporter's own login. It moves to the work-item area (FW-6a) with #806 |
| `gh auth git-credential`, `glab`'s helper | a clone's git credential helper | Git's own credential path is not part of the seam (ADR 0070 §4, #752) |

### Gaps the audit found

Fixed in this audit, each with a case in `tests/forge_contract.rs`. Both move charter away from
what the Python charter answered, on purpose (ADR 0046):

- **Two GitLab pipeline statuses were unread.** GitLab 19.4 lists thirteen (`doc/api/pipelines.md`,
  the `status` filter), and charter, like Python's `gitlab._CI_MAP`, mapped eleven.
  `waiting_for_callback` is now `pending` on the status line, `RUNNING` for `change show`, and a
  pipeline auto-merge waits for. `canceling` is now `canceled` and `FAILED`.
- **The GitLab status line could show a fork's merge request.** `open_on_branch` took the first
  open MR whose source branch had that name, while `by_head` and `open_or_update` keep only the
  project's own (`source_project_id == target_project_id`). It now reads a page of a hundred and
  keeps only the project's own, as GitHub's `head={owner}:{branch}` already does. Python asked
  for one MR (`per_page=1`); the pinned argv test moved with it.

Open, each filed:

1. **A GitLab user namespace cannot be discovered** (#803). `owned` asks `groups/{owner}/projects`,
   which does not answer for a user. GitHub falls back from the org endpoint to the user one;
   GitLab's equivalent is `GET users/{owner}/projects` (`doc/api/projects.md`).
2. **GitLab lists projects shared into the group** (#804). `groups/:id/projects` defaults
   `with_shared` to `true` (`doc/api/groups.md`), so a project another namespace shares with the
   group is discovered as if it were the group's. `with_shared=false` would match GitHub.
3. **GitLab's auto-merge parameter is deprecated** (#805, after FG-2, #802).
   `merge_when_pipeline_succeeds` was deprecated in GitLab 17.11 in favour of `auto_merge`, and
   19.4 still accepts it. charter keeps it on purpose: a GitLab older than `auto_merge` ignores an
   unknown parameter and would merge at once. Since 19.1, `auto_merge` on a project with merge
   trains joins the train.
4. **No self-managed GitLab recording** (#742). ADR 0070 §7 asks for one; the recordings here are
   GitHub's and GitLab's documented answers.
5. **`charter report` is outside the seam** (#806), until the work-item area (FW-6a) exists.

## The mixed-forge collision rule

Repos are addressed by their **bare name** — the last path segment — everywhere:
`charter clone api`, `charter status`, `docs/topology.md`. That's convenient until two
different forges (or two blocks of the *same* forge kind — e.g. two GitHub orgs, or a
GitLab group whose subgroups both have a repo called the same thing) expose a repo with
the same bare name. `charter discover` refuses to guess which one you meant:

- **Different forges, same bare name** (`gitlab:api` and `github:api`) — qualify it:
  `charter clone github:api`. The `<forge>:<name>` prefix disambiguates.
- **Same forge, different namespace, same bare name** (e.g.
  `acme/team-a/api` and `acme/team-b/api` under one GitLab group with subgroups, or two
  `[[forge]]` blocks of the same kind) — there is **no forge-qualifier that can tell
  these apart**, since they're already on the same forge. The only fix is excluding one
  of them via that block's `exclude = [...]` in `charter.toml`.

Either way, `charter discover` names both colliding repos (their full
`path_with_namespace`, not just the ambiguous bare name) and stops rather than picking
one silently — a workspace clone's on-disk path is derived from the bare name, so
guessing wrong would mean two unrelated repos could clone over each other.

See `docs/control-plane.md` for the full `charter.toml` reference, including a worked
mixed-forge example.
