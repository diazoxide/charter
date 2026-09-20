# Forges: GitLab and GitHub

A **forge** is a code-hosting platform `charter` talks to — GitLab or GitHub today. Every
git operation `charter` performs (listing repos, cloning, pushing memory) goes through
that forge's own official CLI, authenticated once, over HTTPS. See the README's
"one credential" section and `docs/secrets.md` for why that matters to an autonomous
agent specifically.

## GitLab

- **What it needs:** [`glab`](https://gitlab.com/gitlab-org/cli) installed and
  authenticated (`glab auth login`, then `glab auth status` should say "Logged in").
  `charter doctor` checks both.
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

## What charter asks a forge, and what it now tells one

The protocol has two disciplines and, since the cross-repo change surface, a third.

**Permissive** — `open_change`, `ci_status`, and everything the status line renders from.
Any failure answers `None`. Being wrong costs a blank column, retried at the next refresh,
and this path must never crash a surface that draws every turn.

**Strict** — `list_repos`, `repo_tree_strict`, `request_for`. A failure raises
`ForgeError`, because collapsing "the call failed" into "the result was empty" is how a
rate-limited lookup wipes an inventory, or reads an open pull request as one that was never
pushed.

**Loud** — `create_change`, `update_change_body`, `merge_change`. A failure raises
`ForgeWriteError` and never returns `None`, because a swallowed write failure means a pull
request that was never opened, or a merge that never happened, reported as a success. This
is the second place in charter that writes to a forge; `report` was the first, and ADR 0002
is amended rather than left quietly false about that.

### `checks_at(path, sha, number=None)` — and why it is not `ci_status`

`ci_status` collapses six worlds into `None`: a CLI failure, a timeout, a non-zero exit,
malformed JSON, an auth failure, and *no check ever ran*. That is correct where it renders
and useless as a landing gate, so `checks_at` answers a **record with two fields**:

- `total is None` — the only way to say *charter could not ask*, or could not ask
  completely. `state` is `unknown`.
- `total == 0` — charter asked, everywhere it knows to look, and there is nothing there.
  `state` is `not_run`, and **that is not a pass**.

The other three states are `passed`, `failed` and `running`, and precedence is fixed:
`unknown` > `failed` > `running` > `not_run` > `passed`. `unknown` is first because it is the
only value that means charter did not look.

**`gh pr checks` and `mergeStateStatus` are forbidden inputs**, by name, in the spec and in
the test. Both report a run that never happened identically to a clean pass (#561).

The requirement is a **property, not an endpoint** — *see every check the forge would show a
human at that head* — and each backend needs more than one read to satisfy it:

- **GitHub:** check runs **and** the combined commit status at that sha, summed into one
  total. The check-runs endpoint returns Check Runs only, so a repository reporting through
  the Commit Statuses API (Jenkins, Buildkite, CircleCI) is `total_count: 0` there at a fully
  green head.
- **GitLab:** the merge request's own head pipeline, which needs `number`. A merged-results
  pipeline runs against `refs/merge-requests/:iid/merge`, whose sha is not the branch head,
  so a bare sha filter is empty on a green merge request. Without `number` charter cannot
  rule that out, so an empty answer is `unknown` rather than `not_run`.

Where a backend cannot enumerate completely the answer is **`unknown`, never `not_run`**.
That word asserts nothing ran, and charter may only assert it having looked everywhere it
knows to look. The asymmetry decides it: a false `not_run` costs a re-run, a false `passed`
merges untested code.

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
