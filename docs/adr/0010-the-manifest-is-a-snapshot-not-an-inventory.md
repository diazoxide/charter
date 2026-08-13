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

The union is not a merge of equals and must not become one. If a future change starts
writing the manifest automatically, the guard is gone whether or not the code still calls
it — and `restore` will fail on someone else's machine, which is the failure that is
hardest to attribute back here.

Where two sources answer a question, name which question each answers. Both were right;
only the code that read one and reported the other was wrong.
