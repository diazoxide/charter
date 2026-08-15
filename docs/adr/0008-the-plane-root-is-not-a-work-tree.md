# The plane root is not a work tree

The directory holding `charter.toml` is the **plane root**. It holds the control plane —
personas, inventory, workspaces, config — and nothing anyone is meant to edit or switch
branches in. Work happens in a **workspace tree**: a clone a workspace owns.

This needs writing down because nothing in the code makes it true, and because the code
looks like it does. After ADR 0007 charter never *lists* the plane root as a repo — a fleet
plane's `repo_trees` is its clones and nothing else. That is easy to mistake for the problem
being solved. It is not: the plane root remains a real git repo in a real directory that
`charter init` is documented as being run inside, and **not presenting a tree is not the
same as preventing work in it**.

## Why it matters

Two sessions in two workspaces are perfectly isolated right up until both are actually
sitting in the plane root — at which point they share one working tree and one HEAD and
thrash each other's branches, while charter reports two different workspaces and lists no
tree that would hint at why.

That last part is what makes it worth a signal rather than nothing. The failure is invisible
in exactly the surface a user would check.

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

So the first step is signal, in the two places already designed to carry it: the status line
gains an explicit warning when the plane root is dirty or off its default branch — a new
element, since the root is otherwise not rendered at all — and `doctor`, which runs at
SessionStart while acting on it is still cheap, checks the same thing.

## Consequences

A warning you can work through is a warning you learn to work through, and this one is
expected to be ignored at first. That is accepted for now, and it is the reason this ADR
exists: so that when someone later proposes making it an error, the note explaining why it
was not one on day one is already here.

## 2026-08-16 — the evidence arrived, so branch moves are now refused (#157)

The prediction above held exactly. In one session an agent switched the plane root between
branches **six times**, with `doctor` printing its correct, complete, actionable warning on
every run in between; it was read, judged "expected mid-work", and dismissed each time. Not
missed and not unclear — rationalised past, repeatedly, by precisely the consumer charter is
built for. The operator's notes supply the cost: two background agents in one working tree
clobber each other through `git checkout`, and the symptom presents as an unrelated bug.

That is the evidence this ADR said prevention was waiting on, so `pretooluse` now **denies**
a branch move in the plane root. The judgement it asked for — *which commands count* — is
answered as narrowly as the evidence supports:

* **`checkout` and `switch` only.** `reset`, `rebase` and `merge` also rewrite the shared
  tree, and are deliberately not covered: the evidence is about switching, and a guard that
  over-blocks is disabled once and then protects nothing.
* **`git commit` is untouched.** `charter save` commits here by design, and advancing HEAD
  along the branch you are on was never the failure.
* **Returning to the default branch is always allowed.** The warning's own remedy is
  `git -C <plane> checkout main`; a guard that blocks the fix it recommends is a trap, and
  would be routed around within a session.

What does **not** change: the warning stays, because it is what explains the denial when one
arrives, and because it still covers the states prevention cannot — a root left dirty, or
sitting on a non-default branch from before this shipped. Signal and refusal are not
alternatives here; the refusal stops the next switch, and the signal describes the tree you
are already in.
