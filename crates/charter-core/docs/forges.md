# Forges: GitLab and GitHub

A **forge** is a code-hosting platform `charter` talks to — GitLab or GitHub today. Every
forge operation `charter` performs goes through **one seam**: a small set of traits, one per
area, that each forge implements once ([ADR 0070](../../../docs/adr/0070-a-forge-is-one-seam-with-a-native-client-per-forge-and-gh-and-glab-are-its-fallback.md)).
A backend builds each request once, and a **transport** sends it. There are two. The **CLI
transport** is that forge's own official CLI (`gh api`, `glab api`), authenticated once, over
HTTPS, so the token stays in the CLI. The **native transport** (GitHub's, FW-2a; GitLab's,
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
- **What "group" means:** the GitLab group (or subgroup) whose repos this forge block
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
`src/forge/github/` and a GitLab body in `src/forge/gitlab/`. Nothing else in the core
builds a forge request or matches on the forge to do so. Every call carries a `Caller` (which
surface asked, and whether a person is waiting); the account and principal join it with FW-1.

There are two disciplines, and they stay with the caller: the trait returns what happened.

**Strict:** a failure is an error, because collapsing "the call failed" into "the result was
empty" is how a rate-limited lookup wipes an inventory, or a save opens a second request.

**Permissive:** the status line's open request and CI word (`charter gl-refresh`). Any failure
answers nothing. Being wrong costs a blank column, retried at the next refresh, and this path
must never break a surface that draws all the time.

The seam speaks neutral types. A listing of repos answers `RepoRecord`s, one per repo, whose
fields are the keys the inventory writes (`name`, `path_with_namespace`, `default_branch`,
`description`, `web_url`, `ssh_url`, `topics`, `forge`) plus the forge's own `id`, and it is
asked for an `Owner`: a GitHub organisation or user, or a GitLab group by its full path.

### The parity table

Measured against GitHub's REST API (version `2022-11-28`) and GraphQL schema, and against the
GitLab 19.4 REST documentation (`gitlab-org/gitlab`, branch `19-4-stable-ee`, `doc/api/`).
Each row has a contract case on both forges in `tests/forge_contract.rs`, run against the
recordings in `tests/forge_contract/<forge>/` six times: through the CLI transport on each
forge, and through the native transport on github.com, gitlab.com, a GitHub Enterprise Server
and a self-managed GitLab, the native runs with no `gh` or `glab` on `PATH`. The same suite
checks that the network log lists every REST call in the recordings by a template that keeps no
owner, repo, branch, number or commit. The CLI transport's argv for each is pinned by
`tests/a_forge_cli_is_asked_exactly_what_python_asked.rs` and
`tests/a_pr_is_opened_or_updated_and_set_to_auto_merge.rs`.

| Area · method | Who calls it | GitHub | GitLab | Discipline | Parity |
|---|---|---|---|---|---|
| `Repos::owned` | `charter discover` | `GET orgs/{owner}/repos`, then `users/{owner}/repos` on a 404 | `GET groups/{owner}/projects?include_subgroups=true&archived=false` | strict | **gaps 1, 2** (#803, #804) |
| `Repos::reachable` | the repo picker (ADR 0055) | `GET user/repos?affiliation=owner,collaborator,organization_member` | `GET projects?membership=true&archived=false` | strict | same |
| `Repos::top_level` | `discover`'s stack probe | `GET repos/{o}/{r}/git/trees/{ref}` (ref, else default branch, else `HEAD`) | `GET projects/{id}/repository/tree` (ref, else the default branch) | strict | same |
| `Repos::about` | `charter ws todo promote` (ADR 0088 §5), before it sends; `charter doctor`'s `project remote` row (SQ-8) | `GET repos/{o}/{r}`: `visibility` (else `private`), `archived`, `has_issues`, `permissions.pull`, `security_and_analysis.secret_scanning_push_protection.status` (admins only, else unknown) | `GET projects/{path}`: `visibility`, `archived`, `issues_access_level` (else `issues_enabled`), membership from `permissions`, `secret_push_protection_enabled` (else `pre_receive_secret_detection_enabled`; else unknown) | strict | same |
| `WorkItems::create` | `charter ws todo promote` | `POST repos/{o}/{r}/issues` with `title`, `body` and `labels[]` (`ws:<name>`, private repos only); keyed by `html_url` and `number`, `node_id` beside; every todo value a literal `-f` | `POST projects/{path}/issues` with `title`, `description` and `labels` (`charter::ws::<name>`, private repos only); keyed by `web_url`'s host, `references.full` and `iid`, `id` beside; every todo value a literal `-f` | strict | same |
| `Requests::open_or_update` | a request-mode save (ADR 0051), `charter change push` | `GET pulls?state=open&head={o}:{b}&base=…`, then `PATCH` or `POST pulls` | `GET merge_requests?state=opened&source_branch=…&target_branch=…`, the repo's own only (never a fork's), then `PUT` or `POST` | strict | same |
| `Requests::state` | a request-mode save | `GET pulls/{n}`: `closed` + `merged` is merged, at `merge_commit_sha` | `GET merge_requests/{iid}`: `merged`, at `squash_commit_sha` else `merge_commit_sha` | strict | same |
| `Requests::by_head` | `charter change show` and `push` (ADR 0060) | `GET pulls?state=all&head={o}:{b}`, checked against `head.repo.full_name` | `GET merge_requests?source_branch=…&state=all`, the repo's own only | strict | same |
| `Requests::body` | `charter change push` (ADR 0060) | `GET pulls/{n}`, its `body` (`null` is empty) | `GET merge_requests/{iid}`, its `description` (`null` is empty) | strict | same |
| `Requests::set_body` | `charter change push` | `PATCH pulls/{n}` with `body` alone | `PUT merge_requests/{iid}` with `description` alone | strict | same |
| `Requests::request_auto_merge` | a PR-merge save | GraphQL `enablePullRequestAutoMerge` with `expectedHeadOid` | `PUT merge_requests/{iid}/merge` with `merge_when_pipeline_succeeds` and `sha`, only while a pipeline runs | strict | **gap 3** (#805) |
| `Requests::lands_through_queue` | `charter change land` (ruling Q16) | GraphQL `pullRequest.isMergeQueueEnabled` | `GET projects/{id}`, its `merge_trains_enabled` (absent below Premium is no) | strict | same |
| `Requests::merge_at` | `charter change land` (ADR 0060 D3) | `PUT pulls/{n}/merge` with `merge_method`, `commit_title`, `commit_message` and `sha` | `PUT merge_requests/{iid}/merge` with `squash`, `sha` and `merge_commit_message`, **never** `merge_when_pipeline_succeeds` or `auto_merge` | strict | same |
| `Requests::enqueue_at` | `charter change land` (ruling Q16) | GraphQL `enqueuePullRequest` with `expectedHeadOid` | `POST merge_trains/merge_requests/{iid}` with `squash` and `sha`, no `auto_merge` | strict | same, except the method (below) |
| `Requests::checks_at` | `charter change show` and `land` | check runs **and** commit statuses at the sha, each read whole | the request's pipelines at the sha; the newest decides | strict, `UNKNOWN` on failure | same, by design |
| `Requests::open_on_branch` | `gl-refresh` | `GET pulls?state=open&head={o}:{b}&per_page=1` | `GET merge_requests?state=opened&source_branch=…&per_page=100`, the repo's own only | permissive | same (fixed here) |
| `Requests::ci_word` | `gl-refresh` | GraphQL `statusCheckRollup.state` (5 values) | `GET pipelines?ref=…&per_page=1`, its `status` (13 values) | permissive | same |
| `Capabilities::support` | the fallbacks of ADR 0070 §2 | no request: epics are never there; sub-issues, dependencies, boards and iterations are there on github.com's instance; everything else is not asked yet | no request: epics, iterations, boards, child items and blocking links are there on gitlab.com's instance; a self-managed GitLab's are not asked yet | never yes unasked | same; the probes come with FG-2 (#802) |

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

### `charter change land`

`charter change land <slug> --repo <name>` merges one member of a cross-repo change (ADR 0060
§2–§4 and D3, ruling Q16, #472). It is the one change verb that merges, and it is attended
only: `floorguard::PUBLISH_FORGE` refuses it from a run nobody is watching, as it refuses
`gh pr merge`.

The gates come first, each refusal its own sentence and exit 2: one member named (`--repo`
once), not by rebase; a member with a clone here and an open request (`by_head`); every member
it `needs` landed, which is the forge reporting the blocker's request merged and charter having
landed it: a landing-log line whose commit the clone's default branch (its `origin/` tracking
ref, else the local branch) still holds, or a pending landing found merged at its head, which is
logged then; and the checks `PASSED` at the request's head
(`checks_at`), never at another commit.

Then `lands_through_queue` decides the call. On GitLab a repo whose answer has no
`merge_trains_enabled` (a token that cannot read its settings, a tier that does not report
trains) is refused and nothing is merged: a direct merge there could skip a train.

**Before the call, a pending landing is written** (`changes/log/pending/<device>.jsonl`, clone
state): the request, the head, `direct` or `queue`. It is charter's evidence that it started
this landing, and a later `land` records a merge it did not see happen only on that evidence.
A refusal moves it to `refused`, so a later merge of the same head by somebody else is never
taken for charter's.

- **No queue:** `merge_at`, now, pinned to the head whose checks were read. GitHub's `sha` and
  GitLab's `sha` make the forge refuse a head that moved, and charter, reading the request
  again, names the move. The landing commit's message is charter's, so it carries
  `Charter-Change: <slug>`. Charter never asks for auto-merge here: on GitLab
  `merge_when_pipeline_succeeds` (or `auto_merge`) would merge whatever head the branch has when
  a later pipeline passes. A GitLab that answers by setting it anyway has it cancelled at once,
  the cancel confirmed by reading the merge request again, and the landing refused; a cancel
  that fails is `MergedAt::Later`, kept as `merge-later` in the pending landing, and doctor names
  it. After the merge the request is read again, and only a request the forge reports merged,
  at that head, gets a line in the landing log. When that read-back fails, the next `land`
  finds the merge and records it.
- **A merge queue or merge train:** `enqueue_at`, pinned the same way (`expectedHeadOid`,
  `sha`), and with no `auto_merge` on GitLab, so the merge request is added now or refused. The
  queue runs its own checks and merges it later, so nothing is logged then. A later
  `charter change land` of the member finds the request merged at the pending head and records
  the landing. A request merged with no pending landing of charter's is never recorded, and as a
  blocker it is refused as merged outside charter. A queue writes its own merge commit message
  on both forges, so these landings carry no trailer. GitHub's queue merges by the method its
  rule sets, so `--squash` is not charter's to choose there; GitLab's train takes `squash`
  (`Kind::queue_takes_squash`).

### From the changes view

The changes view's **Push** and **Land** buttons (#474) call the same functions in two steps,
so the window can ask first. The question does nothing outward: `push::destinations` reads each
member's repo, branch and its commit, destination and request base from its clone, and
`land::verify` runs every gate above and names the request, the head its checks passed at, and
whether it merges now, goes into the queue, or is only recorded. It writes no pending landing
and asks no merge. The yes hands that answer back to `push::push_confirmed` or
`land::land_verified`, which take every gate again and refuse, before anything is pushed or
merged, when what they find is not what the operator confirmed (a destination or a branch's
commit changed, a head moved, a queue appeared). Everything else, the pinning and the pending
landing included, is the CLI's path unchanged.

### `charter change revert`

`charter change revert <slug>` (ADR 0060 §8, #473) asks no forge at all, so it is the same for a
GitHub member and a GitLab one. It seeds a new change, `revert-<slug>`: in each member the
landing log records, it branches `change/revert-<slug>` off the default branch (its `origin/`
tracking ref, else the local branch) and runs `git revert` of the logged commit there, with
`-m 1` only when git says the commit has two or more parents. The revert reaches a forge only
when someone runs `charter change push` and `charter change land` on it, through the same gates and
the same attended-only floor as any change. A request merged with no landing-log line is named
as a person's to revert; charter does not guess its commit from the forge or a branch. Run
again, it seeds only the members it has not seeded yet.

**Not behind the seam, and why:**

| Call | Where | Why |
|---|---|---|
| `gh auth status` / `glab auth status` | `Forge::check_auth`, the CLI transport's own check | It asks whether the transport can speak as someone, not the forge anything. The native transport answers it from its own sign-in |
| `gh search issues`, `gh issue create` | `report.rs`, through `forge::gh_as_the_operator` | It files on charter's own tracker, which is on GitHub whatever forge a project uses, under the reporter's own login. It moves to the work-item area (FW-6a) with #806 |
| `gh auth git-credential`, `glab`'s helper | a clone's git credential helper | Git's own credential path is not part of the seam (ADR 0070 §4, #752) |
| `gh auth status` / `glab auth status`, from the first run | the app's first run, through `Forge::check_auth` | The same check as the first row, asked of each forge whose CLI is installed |
| `gh …` and `glab …` in a chat's own commands | the guard tables (`personagate`, `proseguard`, `floorguard`, `heredoc`, `leakguard`) | They decide whether a chat's command asks first or is refused. charter sends nothing through them. Their parity between the two CLIs is #1068 |

**Work items beyond `create`.** Each forge's native client has its work items as that forge has
them, crate-private, with recorded-request tests (`src/forge/github/work.rs`,
`src/forge/gitlab/work_tests.rs`). Nothing outside the core calls them yet. They join the seam's
`WorkItems` area when FW-6a (#733) and FW-6b (#734) map them onto the work model:

| Concept | GitHub | GitLab |
|---|---|---|
| An issue: read, list, create, update | `repos/{o}/{r}/issues` | `projects/{id}/issues` |
| A child item | sub-issues | a GraphQL work item's parent |
| Blocked by | issue dependencies | issue links, `blocks` |
| Milestones | `repos/{o}/{r}/milestones` | `projects/{id}/milestones` |
| Types | issue types | — (a work item's type; FW-6b decides) |
| Epics | — | `groups/{id}/epics` (Premium) |
| Iterations | a Projects v2 iteration field | `groups/{id}/iterations` (Premium) |
| Boards | a Projects v2 board, its items and fields | a repo's boards and their label lists |
| Pipelines | — (checks are `Requests::checks_at`) | `projects/{id}/pipelines` |

### Gaps the audit found

Fixed in this audit, each with a case in `tests/forge_contract.rs`. The first two move charter
away from what the Python charter answered, on purpose (ADR 0046):

- **Two GitLab pipeline statuses were unread.** GitLab 19.4 lists thirteen (`doc/api/pipelines.md`,
  the `status` filter), and charter, like Python's `gitlab._CI_MAP`, mapped eleven.
  `waiting_for_callback` is now `pending` on the status line, `RUNNING` for `change show`, and a
  pipeline auto-merge waits for. `canceling` is now `canceled` and `FAILED`.
- **The GitLab status line could show a fork's merge request.** `open_on_branch` took the first
  open MR whose source branch had that name, while `by_head` and `open_or_update` keep only the
  project's own (`source_project_id == target_project_id`). It now reads a page of a hundred and
  keeps only the project's own, as GitHub's `head={owner}:{branch}` already does. Python asked
  for one MR (`per_page=1`); the pinned argv test moved with it.
- **A GitHub Enterprise Server never ran the native contract.** A self-managed GitLab did. Every
  case now runs on a GitHub Enterprise Server too (`github_self_managed`).
- **A GitHub ruleset that protects a branch was not read as a protected branch.** GitHub refuses
  that push with `GH013` and a branch-protection rule line, "Changes must be made through a pull
  request" or "Cannot update this protected ref". Neither matched a protected-branch signature,
  so the save said only "push failed". GitLab's protected branch already matched. Those two
  lines are now recognised like GitHub's `GH006`. `GH013` alone is not, because GitHub also
  uses it to refuse a secret or a file a ruleset forbids, and that is not a protected branch.
- **Nothing showed that the network log masks every call.** It is now checked for every recorded
  call on both forges.

Open, each filed:

1. **A GitLab user namespace cannot be discovered** (#803). `owned` asks `groups/{owner}/projects`,
   which does not answer for a user. GitHub falls back from the org endpoint to the user one;
   GitLab's equivalent is `GET users/{owner}/projects` (`doc/api/projects.md`).
2. **GitLab lists repos shared into the group** (#804). `groups/:id/projects` defaults
   `with_shared` to `true` (`doc/api/groups.md`), so a repo another namespace shares with the
   group is discovered as if it were the group's. `with_shared=false` would match GitHub.
3. **GitLab's auto-merge parameter is deprecated** (#805, after FG-2, #802).
   `merge_when_pipeline_succeeds` was deprecated in GitLab 17.11 in favour of `auto_merge`, and
   19.4 still accepts it. charter keeps it on purpose: a GitLab older than `auto_merge` ignores an
   unknown parameter and would merge at once. Since 19.1, `auto_merge` on a project with merge
   trains joins the train.
4. **No recording captured from a live forge** (#742). The native contract runs on each forge's
   own instance and on a self-managed one, but every run replays the forge's documented answers.
   The live nightly (FG-4, below) now runs the same cases against github.com and gitlab.com;
   re-recording from what it sees, and the self-managed GitLab, are FW-15's.
5. **`charter report` is outside the seam** (#806), until the work-item area (FW-6a) exists.
6. **A request-mode save on GitLab says "pull request"** (#1067). The project's and a repo's
   `pr` and `pr-merge` saves, their doctor rows and alerts, and the app's saving view name a
   merge request `pull request #12`. The changes view already says what each forge says.
7. **The guard tables treat the two forge CLIs unevenly** (#1068).

## The native GitLab client (FW-2b)

GitLab's native transport is the same `Http` as GitHub's, speaking GitLab's dialect:

- **Where it asks.** `https://<host>/api/v4` for REST and `https://<host>/api/graphql` for
  GraphQL, on gitlab.com and on a self-managed instance alike (`ApiRoot::gitlab`). The token is
  sent as `Authorization: Bearer`, which GitLab takes for a personal, group or OAuth token, with
  `Accept: application/json` and none of GitHub's headers.
- **What a refusal says.** GitLab's `message`, which is an object of field errors when it
  refuses a write, or an OAuth refusal's `error_description`, followed by `(HTTP <status>)`, as
  `glab api` prints it. A `401` is `Auth`, a `403` `Forbidden`, a `404` `NotFound`, and a `429`
  with GitLab's `ratelimit-remaining: 0` or a `Retry-After` is `RateLimited` with
  `ratelimit-reset` as its reset.
- **Conditional reads.** GitLab answers a read with a weak ETag (`W/"…"`) and a `304` to it, so
  a repeated read is answered from the same per-account ETag store GitHub's is.
- **What it can do** (`Capabilities`). gitlab.com has epics, iterations, boards, child items and
  blocking links, said of the instance. Whether one group or repo has them turns on its plan
  (epics, iterations and blocking links need Premium), which FG-2's probes ask, so a narrower
  reach is `Unknown` and takes its fallback. A self-managed GitLab shows its edition and licence
  to no one but an admin, so everything there is `Unknown`.
- **Work items** (`src/forge/gitlab/work.rs`, crate-private until FW-6b maps them): issues,
  blocking links, child items and their parent (GraphQL work items), milestones, epics and
  iterations (Premium), boards and their label lists, and pipelines. A label with a comma is
  refused, because GitLab reads it as two.
- **The network log** lists a GitLab path by position: the id after `projects` or `groups` is
  one `{}`, whether it is a number or a full path, encoded (`acme%2Fapi`, as charter sends it)
  or not (`acme/sub/api`, up to the next word GitLab puts under a repo or group); the segment
  after a collection word (`issues`, `merge_requests`, `epics`, `links`, …) is an id, masked
  however it is spelled.

## The live nightly (FG-4)

`.github/workflows/forge-live.yml` runs the contract suite's cases, the same ones the recordings
answer, against a real github.com repo and a real gitlab.com repo, every night, through the
native transport (ADR 0070 §7, the live run). The cases ask about a scene (`tests/forge_contract/
scene.rs`), not literal names, and the live run reads the scene's numbers and commits from the
fixture when it starts. It never gates a pull request; a red night keeps one issue open.

Three cases do not run live, each listed with its reason in `NOT_LIVE`: `request_auto_merge`,
`merge_at` and `enqueue_at` would land or queue the fixture's one open request. A forge with no
token or owner set says so in the run's summary and stops, green.

### What the operator provisions

Nothing below exists until the operator makes it; no charter run creates or deletes any of it.
Each forge gets a **fixture org or group**, a **machine account** with a token, one **secret**
and one **variable** in `diazoxide/charter`'s Actions settings.

| | GitHub | GitLab |
|---|---|---|
| Secret | `FORGE_LIVE_GITHUB_TOKEN` | `FORGE_LIVE_GITLAB_TOKEN` |
| Variable | `FORGE_LIVE_GITHUB_OWNER`: the org's login | `FORGE_LIVE_GITLAB_OWNER`: the group's full path |
| Owner | a new org holding only the two repos below | a new group holding only the two repos below, no subgroups |
| Account | a machine account that owns no repos, an outside collaborator with **Write** on `api` only, not an org member | a separate user that owns no repos, a **Developer** member of `api` only, not of the group |
| Token | a classic personal access token with the `repo` scope (an outside collaborator's fine-grained token cannot reach the org's private repo); what it can reach is bounded by the account | a personal access token with the `api` scope; what it can reach is bounded by the account |

The account matters as much as the token: the `reachable` case expects the account to reach
`api` and nothing else of the owner's, and the `about` case expects no admin rights, so
neither forge shows the push-protection setting.

Each forge's owner holds exactly two repos:

- **`web`**: public, no description, topic `frontend`, default branch `main`.
- **`api`**: private, no description, no topics, default branch `main`, issues on, a label
  `alpha`. Its `main` holds at the top level exactly `Cargo.toml` and `src/` (a `src/lib.rs`).

And `api` holds these branches and requests, made in this order:

1. **`charter/landed`**: one commit off `main` that changes `src/lib.rs` only. Its request into
   `main` is merged **with a merge commit** (GitHub's "Create a merge commit"; GitLab's merge
   method "Merge commit"), so `main`'s top level stays `Cargo.toml` and `src`.
2. **`charter/save`**: one commit off the new `main` that adds one CI job and nothing else, run
   on push only: `.github/workflows/check.yml` with `on: push` and one job that runs `true` on
   GitHub, `.gitlab-ci.yml` with one job that runs `true` on GitLab. The push runs it once and
   green: one check at the head, one pipeline. GitLab needs an instance runner for that, which
   gitlab.com grants once the account's identity is verified. Its request into `main` stays
   **open**, with the body exactly `Why this change.` + a blank line + `<!-- END charter change
   -->` and no trailing newline (`printf 'Why this change.\n\n<!-- END charter change -->'`
   into `--body-file`, or `--description` for `glab`).
3. **`charter/open`**: one commit off `main` that changes `src/lib.rs` only, with **no**
   request. The first live night opens one (`open_or_update`), and every later night updates it.

After that, nothing is pushed to the fixture. The nightly writes only three things there: it
sets the open request's body to the text above, opens or updates the `charter/open` request, and
opens an issue labelled `alpha` that it closes again at once. CI on the fixture runs only when
the operator pushes to it, never nightly.

A self-managed GitLab is FW-15's (#742): another row of the matrix, its host in
`FORGE_LIVE_GITLAB_HOST`, which `tests/forge_contract/live.rs` already reads.

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
