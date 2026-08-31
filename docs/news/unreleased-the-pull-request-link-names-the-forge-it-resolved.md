---
version: unreleased
headline: The pull-request link `charter save` prints is built from the forge charter resolved, not from a hostname it found somewhere in the URL
security: true
---

**Affected: every release that has shipped `charter save`'s pull-request path. Fixed
here.** Read the last section first if you run a self-hosted forge; if your plane's origin
is github.com or gitlab.com, nothing you can see changes.

When a plane's default branch is protected, `charter save` pushes the commit to
`charter/<sha>` and prints one line you are meant to click:

```
• open it: https://…/compare/charter/abc?expand=1
```

Which form to build was decided by asking whether the string `github.com` appeared
**anywhere in the remote URL**. That is the substring check `registry._host_of` was
written to replace after it misresolved a remote whose *path* began `gitlab.com-` — the
same bug, in the one place the earlier fix did not reach. GitHub's compare form and
GitLab's new-MR form are different URLs, and the choice between them was being made by a
string that a repository path can contain as easily as a hostname can.

## What it actually did

A self-hosted GitLab with a mirror namespace called `github.com` — an ordinary thing to
call a group that mirrors GitHub — got GitHub's compare URL pointed at a GitLab host:

```
https://git.internal/mirrors/github.com/acme/plane/compare/charter/abc?expand=1
```

GitLab does not serve that. The operator, on a protected branch, following the one link
charter gave them, landed on a 404 — which is precisely the case the other branch exists
for. The commit was pushed and safe throughout; what was lost was the route to opening the
pull request, in the workflow that has no other route.

The reason this is filed as security rather than as a broken link is the direction the
mistake runs in. The line is a **link charter blesses and an operator clicks**, and its
host was being chosen by a string that appears in a part of the URL the host does not
control. Charter now refuses to print a link for a remote it cannot resolve to a forge at
all, rather than assembling a plausible one — the same refusal `_origin_https` already
makes about the same URL, one call earlier.

Two things bound how far that went, and they are worth stating rather than leaving you to
work out: charter only reaches this line for a remote whose **host component** already
resolved to a forge that is either a registered default or one your own `charter.toml`
declares, and it declines entirely for anything that is not an `https://` remote. A host
you have not declared never got a link and still does not.

## What it does now

The forge is **resolved** — `registry.resolve_host`, the call `_origin_https` makes on the
same URL one step earlier — and the link is built from `forge.kind`. There is no hostname
literal left in that function for a path to impersonate.

Resolving instead of string-matching also fixes the sibling the old check could never have
got right: a declared **GitHub Enterprise** host says nothing about `github.com`, so a GHE
plane was handed GitLab's new-MR form. It gets its compare form now, for the first time.

Unchanged, deliberately: self-hosted GitLab keeps the new-MR form it always had, and
github.com and gitlab.com behave exactly as before.

## If you run a self-hosted forge

Nothing to adopt beyond upgrading. If `charter save` on a protected branch has been
handing you a link that 404s, this is why, and it will now be the right one. If you run
GitHub Enterprise, the link changes shape from a GitLab form to a GitHub one — that is the
fix, not a regression.
