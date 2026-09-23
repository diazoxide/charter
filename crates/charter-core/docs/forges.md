# Forges: GitLab and GitHub

A **forge** is a code-hosting platform `charter` talks to — GitLab or GitHub today. Every
git operation `charter` performs (listing repos, cloning, pushing the plane) goes through
that forge's own official CLI, authenticated once, over HTTPS. [git-policy.md](git-policy.md)
says why that matters to an autonomous agent specifically.

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

Every call is a read, and there are two disciplines.

**Permissive** — the open pull or merge request on a branch, and its last CI result: what
`charter gl-refresh` writes for the panels to draw. Any failure answers nothing. Being
wrong costs a blank column, retried at the next refresh, and this path must never break a
surface that draws all the time.

**Strict** — listing an owner's repos, and reading a repo's file tree for stack
detection. A failure is an error, because collapsing "the call failed" into "the result
was empty" is how a rate-limited lookup wipes an inventory.

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
