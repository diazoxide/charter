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

**The operand is only half the command, and the first cut of this read only that half.**
What an operand means depends on the options beside it: `README` is a path after `--ours`
and a *branch to create* after `--orphan` — `git checkout --orphan README` answers
"Switched to a new branch 'README'". So the same rule now applies to the options, in the
same direction: the gate opens only when every option present is one charter can place as
restore-only, and an option it cannot place keeps the guard shut. That covers the value
forms git accepts, where the branch name hides inside the option token (`-bREADME`,
`--orphan=README`), and it covers the option git adds next, which no list of bad flags
could. A restore-only flag charter has not heard of yet is refused here until someone adds
it — and the two spellings that need no flags at all, `git restore <path>` and
`git checkout -- <path>`, are always allowed.

**The genuinely ambiguous case stays denied, and now says so.** Where a branch and a tracked
file share a name, git breaks the tie in favour of the ref — it really does switch. That is
the one case where refusing is right, so the denial no longer asserts a branch; it says the
command is ambiguous and names the two spellings that are not:
`git restore <name>` and `git checkout -- <name>`, both allowed.

**And `git checkout` is not the only way to spell `git checkout`.** With `co = checkout` in
your config — an alias on a large share of developer machines — `git co feature` moved the
plane root's HEAD and this guard never saw a `checkout` at all. It now asks git what the
subcommand really is before standing aside: config aliases, aliases to aliases, aliases
carrying their own options (`sw = switch -c`), `!git checkout`, and the setup-free
`git -c alias.zz=checkout zz feature` all reach the same rule now, and `git co <file>` is
still a restore. Two things it deliberately does not reach, so you know where the edge is: a
`!`-alias that is not a plain `git …` (refusing every shell alias in the plane root would
refuse `s = !git status` too), and `--config-env`, where the body is in an environment
variable. Ordinary commands cost nothing — charter only asks about a subcommand it does not
already take to be git's own, so `git status` and `git commit` are still a set lookup.

**The same misreading was making the guard too narrow, too.** It required an operand, so
`git checkout --detach`, `git switch --detach` and their short form `-d` — which take the
root off its branch with no operand at all — walked past it. So did `git checkout -bREADME`,
where the branch name is inside the option token and the operand list is empty. All refused
now, and the denial names the branch it would create rather than falling back to "switch
branches".

Nothing to adopt: upgrading is the whole of it.
