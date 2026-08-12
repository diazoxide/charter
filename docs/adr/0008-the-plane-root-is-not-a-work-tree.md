# The plane root is not a work tree

The directory holding `charter.toml` is the **plane root**. It holds the control plane —
personas, inventory, workspaces, config — and nothing anyone is meant to edit or switch
branches in. Work happens in a **workspace tree**: a clone a workspace owns.

This needs writing down because nothing in the code makes it true. The plane root *is* a
git repo, `charter init` is documented as something you run inside your existing repo, and
the status line lists the root tree alongside the repos you work in. Every signal says
"this is a place to work". Only a warning says otherwise.

## Why it matters

Two sessions in two workspaces are perfectly isolated right up until both of them are
actually sitting in the plane root — at which point they share one working tree and one
HEAD, and they thrash each other's branches while charter reports two different workspaces.
That is the failure this constraint exists to prevent, and removing the embedded plane
shape (ADR 0007) does not prevent it: `repo_trees` lists the root tree in every shape.

Observed rather than theorised. In the session that produced this decision the agent was
nominally in two different workspaces in turn while doing every git operation in the plane
root — six branches, and at one point a `git checkout main` that silently reverted
in-flight work out of the tree.

## Considered options

Preventing it outright — refusing commands that would operate in the root — is the real
answer and is deliberately not what ships first. Which commands count is a judgement that
wants evidence: `charter` itself must obviously work there, and so must whatever a user
does to configure their plane. Shipping a refusal built on a guess would either block
legitimate work or be so narrow it protects nothing.

So the first step is signal, in the two places already designed to carry it: the status
line marks the root row as infrastructure rather than drawing it as a peer of the clones,
and `doctor` — which runs at SessionStart, while acting on it is still cheap — checks the
root is clean and on its default branch.

## Consequences

A warning you can work through is a warning you learn to work through, and this one is
expected to be ignored at first. That is accepted for now, and it is the reason this ADR
exists: so that when someone later proposes making it an error, the note explaining why it
was not one on day one is already here.
