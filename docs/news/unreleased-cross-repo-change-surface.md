---
version: unreleased
headline: A cross-repo change has a strip in the frame, a revert that is a new change, and a landing charter will not do unattended
adopt: workspace reinit --all
---

`charter change` kept the record. This is the rest of the surface around it: somewhere to
see a change while you are in the middle of one, a way back out when it was wrong, and the
floor that stops an unattended run merging across five repositories on its own.

## `charter change revert` — a revert is a new change

```
$ charter change revert component-api-2
✓ change 'revert-component-api-2' created — 2 member(s) to revert
  charter          branch change/revert-component-api-2  reverted
  charter-metrics  branch change/revert-component-api-2  reverted
✗ charter-jira: no landing record, so no merge sha — charter cannot revert this member.
```

Its members are the landed members of the original; each is seeded with a branch off that
clone's own default branch carrying a revert of the sha the landing log recorded. From
there it is an ordinary change with the ordinary gates.

**`-m 1` only when git says that sha has more than one parent.** A squash landing is an
ordinary commit and `-m` on one fails; a merge landing has two and `git revert` refuses
without it. The parent count is asked of git, never remembered — storing "this was a
squash" would be a derivable fact cached for convenience.

**The ordering is the original's, reversed.** If `charter-metrics` needed `charter` to land
first, undoing it goes the other way — reverting the base while the dependent still needs
the API it removes leaves the dependent broken, which is the world the revert exists to
restore. A revert that simply copied `needs` across would get this exactly backwards, and
would look right in every single-member example.

**A member charter did not land is named, not guessed.** No line in the landing log means
no merge sha, and charter will not pick the merge commit that looks about right. It also
refuses over uncommitted work, by name: `git revert` commits into the checkout, and folding
somebody's work in progress into a revert commit is worse than the wait.

What it will not do, in any change action and during a revert included: force-push to any
branch, delete any branch, `reset --hard` a default branch, or close a request charter did
not open. Those are not flags that default off — they are argv charter never builds, and
the tests read every git invocation a revert makes rather than trusting the absence.

## The floor: no unattended landing

`("charter", "change", "land")` joins the release floor, so a run the harness reports as
`bypassPermissions` is **denied** — not asked, because an unattended ask falls back to
allow and would be indistinguishable from no guard. Attended is untouched: an agent with a
person at the keyboard may land one member, because that is the merge the standing rule
already permits for a single repo. Every other `change` verb is untouched in every mode.

`change` also joins the persona tool gate's destructive list, so a persona declaring
`tools: charter` never gets a landing auto-approved. The grain is coarse on purpose — it
declines `charter change show` too — and the asymmetry decides it: over-refusing costs one
prompt on a read-only command, under-refusing auto-approves a merge, and the gate never
denies, so the cost is bounded at that prompt.

Charter adds **no second human gate of its own**. One is available if you want it, and
`charter doctor` names it without running it:

```
charter guard ask --local 'charter change land *'
```

`--local` is load-bearing: `guard ask` writes the plane's *committed* settings by default,
and consent that travels in a commit enrols a whole team on one person's click.

**[ADR 0020](../adr/0020-there-is-no-cross-repo-merge-loop.md)** records the decision this
all turns on: there is no `--all` on `land`, the flag does not parse, and atomicity is
replaced by legibility. When member 3 of 5 is rejected, `--all` would have to guess a
policy — and a flag that must guess a policy is a lie with an argument parser.

## Divergence, named at FAIL

`charter doctor` grows a `changes` row. It fails rather than warns, because a divergence
worth naming is worth the exit code that makes the SessionStart wrapper print. It names a
member landed outside charter, a declared landing the default branch no longer contains,
a declared landing whose commit carries no `Charter-Change` trailer, a member that landed
while a blocker had not, and a member's branch name sitting in a clone that is a member of
no change.

Charter cannot stop a human merging in a browser and does not pretend to. **What would be
theatre is a guard that reports enforcement it does not have**, so the same read that
refuses also names the landings that happened anyway. It reads what is already on this disk
and never fetches — so it can under-report, and can never invent.

## A section in the sidebar

```
▪ changes 2 UNKNOWN 4m
  component-api-2  2/3
* metrics-panel-f  0/1
```

`changes` is a part of the persona sidebar, beside the todos — and it **draws nothing at
all when this workspace has no changes**, exactly as the todo section does, so a plane that
never uses the feature pays no rows for it. `F2` then the **change** row picks one, and the
picked one carries the `*`.

It is a section rather than a pane of its own for two measured reasons, both recorded in
`frame/builtins.py`: the frame supports exactly one variable-height pane by construction
(`resize-pane -y` moves one boundary, so one pane is the remainder and that pane is the
repo table), and a placed component has to be in `[frame] slots` — which is pinned to agree
with the shipped default and with `density = full`, so placing it would put a pane saying
"no changes" on every operator's frame.

**Nothing on the repaint path calls a forge, and nothing on it starts a process at all.**
The section reads the one gather snapshot — the records and the landing declarations, both
file reads — under that snapshot's single timestamp, which is the age the heading draws.

`UNKNOWN` is a word rather than a blank, and that is the point of it: it means *charter did
not look*. What stands between a member and its landing is a request state and its checks
at its head sha, and those are a forge read the frame does not make. A change is never
reported greener than its worst member, and a change with no members at all reads
`unknown` rather than "everything landed" — an empty maximum is the classic way a report
comes out green over nothing.

## What `workspace reinit --all` is for

Unchanged from the record entry, and repeated because a plane may take both at once:
`STRUCTURE_VERSION` went 2 → 3 so every existing workspace flags itself, and one command
repairs them all. It creates no directory.
