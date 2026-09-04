# The manifest is a snapshot, not an inventory

`workspaces/<name>/workspace.json` records **restorable state**: which repos, on which
branches, captured at a moment somebody chose. It is not the answer to "which repos are in
this workspace right now". That answer comes from scanning the directory.

Both questions are legitimate and neither source can answer the other's. The directory is
always current and never portable — a teammate who has just cloned the plane has no repos
on disk at all. The manifest is portable and always stale — it is written only by
`charter workspace snapshot`, which *refuses* while a repo has unpushed work, precisely so
that a recorded branch is one `restore` can actually check out.

## Why it matters

`status` and `workspace list` scanned the directory. `fork` and `fork --restore` read the
manifest. Nothing reconciled them, and nothing said they were answering different
questions.

So a workspace with nine clones and no snapshot reported `(9 cloned)` everywhere a human
looked, and forked to a workspace with nothing in it — reporting *"No repos recorded"*,
which was true of the manifest and read as true of the workspace. Nine of ten workspaces on
that machine had no manifest at all, so divergence was the ordinary state, hidden
everywhere by the scan (#81).

The tempting fix is to make the manifest self-heal: write it on `clone`, or reconcile it on
every workspace-touching command. That is worse than it looks. It makes read commands mutate
a committed, shared file, and it fills the manifest with branches recorded outside
`snapshot`'s guarantee — so `restore` starts failing on branches that were never pushed,
which is the one thing the manifest exists to prevent. The guard would still be there,
and everything would route around it.

## What this means in practice

`workspace.merge_repo_rows(manifest_rows, disk_rows)` returns the **union**, with the
manifest's branch winning where both know a repo — it was recorded under a promise the
working tree cannot make. `fork` uses it, so an un-snapshotted workspace forks its clones
and a freshly cloned plane still forks from the snapshot.

It also returns which repos came from disk alone, and `fork` says so: their branch is
whatever is checked out and may not be pushed. That reporting is the alternative to a
`doctor` drift check, which was considered and rejected — a manifest only exists once
somebody snapshots, so drift is the *intended* state for most workspaces, and a warning
that fires on the intended state teaches people to ignore warnings.

## Consequences

`snapshot` stays the only writer, and its enforce-push guard keeps meaning what it says.

**Amended by #884 (2026-09-04): `snapshot` is the only writer of BRANCHES.** The paragraph
above stated the rule one level too broadly, and the file it protected went on not existing:
5 of 17 workspaces on the plane this was measured on had a `workspace.json` at all. A file
that is committed *precisely so a teammate can restore someone else's workspace* cannot be a
side effect of a command somebody happened to run, so its presence is now an invariant —
`workspace.ensure` creates it, `charter workspace reinit` backfills the ones that predate
this, and `charter clone` records what it just cloned.

Read what this section actually names as the harm: *"it fills the manifest with branches
recorded outside `snapshot`'s guarantee — so `restore` starts failing on branches that were
never pushed, which is the one thing the manifest exists to prevent."* That is a statement
about **branches**, and every writer added here records **membership only** —
`workspace._membership_rows` writes `{"name": …}` and no `branch`, on every path. So no
branch enters this file except through `snapshot` and its enforce-push guard, and there is
nothing for `restore` to fail on: an unpinned row is restored by the clone existing.

The second objection stands and is honoured rather than argued with: *"it makes read
commands mutate a committed, shared file."* None of the writers is a read command. `ensure`
and `clone` are write paths by definition; `reinit` is the repair command an operator types.
`status` and `workspace list` still only scan.

One reading is stronger after this, not weaker. **Absence from disk is still not removal** —
`restore --on-demand` deliberately leaves every recorded repo uncloned and `restore` skips
what this machine cannot reach — so the automatic maintenance is additive and never
reconciles the file against the directory. A workspace's membership shrinks when an operator
runs `snapshot`, which sets the list outright because they asked for it. The union
`merge_repo_rows` returns is unchanged and still the answer to "which repos", because a
manifest still cannot see a clone nobody has recorded yet.

And the ownership rule is new, because presence brought a file charter maintains into
workspaces where somebody may already have written one by hand: a manifest whose contents no
longer match the digest charter stamped into it is the operator's, and the automatic writers
leave it completely alone.

The union is not a merge of equals and must not become one. If a future change starts
writing the manifest automatically, the guard is gone whether or not the code still calls
it — and `restore` will fail on someone else's machine, which is the failure that is
hardest to attribute back here.

Where two sources answer a question, name which question each answers. Both were right;
only the code that read one and reported the other was wrong.
