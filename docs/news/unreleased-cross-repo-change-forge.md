---
version: unreleased
headline: A change can now be pushed and landed one member at a time, and a head sha with no check runs says NOT RUN instead of passing
---

`charter change` gained its forge half: `push` opens each member's request and maintains a
cross-link block in every body, and `land` merges **one** member behind three gates.

```
$ charter change push component-api-2
✓ charter          opened  -> #601
✓ charter-metrics  opened  -> #14
✓ cross-link block written into 2 request bodies

$ charter change land component-api-2 --repo charter-metrics
✗ charter-metrics: blocker 'charter' has not landed (has no request).

$ charter change land component-api-2 --repo charter
• charter: checks PASSED at 4b1e77a (7 runs)
✓ merged #601 as e0c9d13, trailer Charter-Change: component-api-2
```

## The part this is really for: a check that never ran does not read as a pass

`gh pr checks` reports *"no checks reported"* and `mergeStateStatus` reports `CLEAN`
**identically when no run was ever created**, and the merge button is offered anyway. That
is #561, and it nearly merged an unverified branch on this repository.

So the forge protocol gained a read with **two fields, not one string** — because a single
string is what made `None` mean six different things — and five closed states that do not
collapse into each other:

| state | means |
|---|---|
| `PASSED` | at least one check at this head sha, and everything that concluded, concluded well |
| `FAILED` | one concluded badly — failure, cancelled, timed out, startup failure, or the forge asking for a human |
| `RUNNING` | one is queued or in progress; not a verdict yet |
| `NOT RUN` | **zero** checks exist at this head sha |
| `UNKNOWN` | charter could not ask, or could not ask *completely* |

`NOT RUN` and `UNKNOWN` are different words and neither is green. `UNKNOWN` outranks
everything, because it is the only value that means charter did not look, and "I did not
look" must never be outranked by "I looked and it was fine".

**Checks are keyed to the head sha**, so a pushed fixup returns a member to `NOT RUN`
immediately and loudly rather than leaving the previous sha's green result standing. There
is no `STALE` state because there is nothing for it to describe. Charter does not sleep,
poll or retry: it names the sha and stops, and you run the command again.

## Charter reads everywhere it knows to look, and says so when it cannot

The obvious implementation reads GitHub's check-runs endpoint, and that endpoint returns
Check Runs **only**. A repository whose CI reports through Commit Statuses instead —
Jenkins, Buildkite, CircleCI's status integration — is `total_count: 0` there at a fully
green head, which would be a permanent false `NOT RUN` against a gate that deliberately
offers no `--force`. GitLab has the mirror image: a merged-results pipeline runs against
`refs/merge-requests/:iid/merge`, whose sha is not the branch head.

So charter reads check runs **and** the combined commit status on GitHub, and the merge
request's own head pipeline on GitLab. **Where it cannot enumerate completely it answers
`UNKNOWN`, never `NOT RUN`** — that word asserts nothing ran, and charter may only assert it
having looked everywhere it knows to look.

This is also the honest limit: charter's read is better than `gh pr checks` in the direction
that merges untested code, and it has a failure mode `gh pr checks` does not — refusing a
green repository whose checks it cannot see. Both directions are stated because only one of
them is a merge.

## There is no `--all`

Not a flag that defaults off. **The flag does not parse**, and `--repo` is required, so
`charter change land` cannot mean "everything" by omission either.

`--all` would have to answer a question with no answer: when member 3 of 5 is rejected
mid-loop it must stop and leave two landed, continue and land the independents, or roll back
what it did — and each is wrong in a case the others handle. A flag that must guess a policy
is an unearned diagnosis wearing an argument parser.

The shell loop you write instead is not what is refused. A five-iteration loop over `charter
change land` is **five gated landings**, because the gates live in the command; `--all` would
be one ungated one.

`--rebase` *does* parse, and is refused with the reason: a rebase merge replays the author's
own commits and charter authors none of them, so there is no commit to carry the
`Charter-Change:` trailer and no single sha to revert. Use `--merge` (the default) or
`--squash`.

## What charter refuses, collected

- **No `gh pr checks`, no `mergeStateStatus`** — pinned by a test over the string literals
  these modules can build, so the ban survives the reasoning being written down beside it.
- **No mergeability gate.** Charter attempts the merge and reports the forge's refusal in the
  forge's own words rather than re-diagnosing it. The only field that would predict it is
  `mergeStateStatus`.
- **No stored state.** The record still holds no request number, no CI result and no `landed`
  flag. `show`'s new columns are a reading taken at one moment and thrown away.
- **No destination in a committed file.** The record carries repository names; the remote,
  the forge and the base branch all come from the clone the operator put there.
- **No editing a body charter did not author.** The cross-link block lives between two
  markers and charter refuses to write when they are absent, doubled, out of order, or
  inside a fenced code block.
- **No unattended landing.** `charter change land` joins the release floor, so a run under
  `bypassPermissions` is denied — not asked, because since 0.46.0 an unattended ask is an
  allow.

## Landed means both halves

A member is *landed* when the forge reports its request **merged** and git shows the
member's default branch **containing the sha charter's landing log recorded**. The forge
alone cannot see a revert; the log alone cannot see a browser merge. A member the forge calls
merged with no log line is landed *and* named as having landed outside charter.

The log is `workspaces/<ws>/changes/log/<host>.jsonl` — past tense, append-only, and
**never committed**, exactly as `pieces/` is not. Nothing on disk can disagree with git,
because nothing on disk claims to know.

## For a prompt before every landing

Charter adds no second consent of its own — an operator prompted constantly rubber-stamps
within a day, which is worse than no gate. If you want one:

```
charter guard ask --local 'charter change land *'
```

`--local` is load-bearing: without it the rule lands in the plane's *committed* settings and
enrols your whole team on one person's click.
