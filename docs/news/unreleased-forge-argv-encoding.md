---
version: unreleased
headline: A branch name can no longer make `gh` open a file on your machine
---

`gh api`'s `-F/--field` flag is not a string pass-through. From its own help: "if the value
starts with `@`, the rest of the value is interpreted as a filename to read the value from.
Pass `-` to read from standard input." Charter used `-F` to send three values to GitHub's
CI rollup query — the repo owner, the repo name and the branch — and it did not write any of
them. The branch is read out of the tree's `.git/HEAD`; the owner and name are parsed out of
`git remote get-url origin`. Both are written by whoever wrote the repo.

So a value shaped like a path made `gh` open that file and put its contents in the body of
an authenticated request. One naming standard input made it block on the terminal. One
naming something that never ends made it allocate without bound — 12 GB in four seconds,
measured, still climbing. None of this needed anyone to type a command: the status line
renders every ten seconds, forks a refresh every two minutes, and `charter gl-refresh` runs
at session start. Checking out a branch from someone else's pull request was enough.

The fix is the flag, not the value. `-f/--raw-field` sends what it is given and interprets
nothing, so the same values now travel as literal strings. Percent-encoding — the treatment
`open_change` eight lines above already applied, and the obvious thing to reach for — would
have been wrong here and quietly expensive: a GraphQL variable is a JSON string GitHub never
decodes, so `feature/x` would have gone out as `feature%2Fx`, matched no ref, and emptied
the CI column for every team that puts a slash in a branch name. A URL path gets encoded; a
variable gets a flag with no magic. Both are now covered by a test that reads the argv
charter builds, across every backend call that takes a value from outside, so the next one
to drift fails in the suite rather than on someone's machine.

Two neighbours got the encoding they were missing while the rule was being written down: a
GitHub owner and a GitLab repo id, both interpolated raw into an API path. Neither was
reachable — one comes from your own `charter.toml`, the other is an integer — and that is
the point. "Which values happen to be trusted today" is the distinction that let `ci_status`
drift away from `open_change` in the first place, so the treatment is uniform now instead of
argued case by case.
