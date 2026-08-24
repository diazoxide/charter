---
version: unreleased
headline: `git checkout <file>` in the plane root is a file restore again, not a refused branch switch
---

```
$ git checkout charter.toml
charter guard: would switch to 'charter.toml' in the PLANE ROOT, which is one working
tree every session shares — two agents here silently clobber each other's branches …
```

`charter.toml` is a file. `git checkout <path>` restores a path; `git checkout <branch>`
moves HEAD. The guard read the first as the second, and `git restore charter.toml` — the
modern, unambiguous spelling of the *same* operation — was allowed the whole time. So this
was never a policy against restoring a file in the plane root. It was charter refusing one
of the two ways git spells an operation it permits.

Which makes the denial worse than the inconvenience: it was confident, detailed, and wrong
about what the command does. An operator who believed it would go and create a workspace
clone in order to restore one file, and the message's escape hatch — *"run it yourself, in
your own terminal"* — is the right answer for a guard correctly refusing an agent and the
wrong outcome for one that has misread the command.

**The guard now asks git.** `git checkout` is two commands wearing one name, and git itself
disambiguates by whether the operand resolves as a revision and by `--`. charter asks the
same two questions of the same repo — does this resolve to a commit, and does git track a
path by this name — instead of matching the shape of the command. `git checkout <path>`,
`git checkout .`, `git checkout <tree-ish> -- <paths>` and `git checkout <tree-ish> <paths>`
all run in the plane root now. `git switch` is untouched: it takes branches and nothing
else, which is why it exists.

**Only a positive answer opens the gate**, and that is what keeps this a narrowing rather
than a hole. A name git cannot read as a commit *and* can read as a tracked path is a
restore. Everything else is still refused — including a branch that exists only on a remote
(git's DWIM checks it out here as a new local branch), a name git has never heard of, and
anything charter could not resolve, such as `git checkout "$BR"`.

**The genuinely ambiguous case stays denied, and now says so.** Where a branch and a tracked
file share a name, git breaks the tie in favour of the ref — it really does switch. That is
the one case where refusing is right, so the denial no longer asserts a branch; it says the
command is ambiguous and names the two spellings that are not:
`git restore <name>` and `git checkout -- <name>`, both allowed.

**The same misreading was making the guard too narrow, too.** It required an operand, so
`git checkout --detach` and `git switch --detach` — which take the root off its branch with
no operand at all — walked past it. They are refused now.

Nothing to adopt: upgrading is the whole of it.
