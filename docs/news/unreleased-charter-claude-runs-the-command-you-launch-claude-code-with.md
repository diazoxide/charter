---
version: unreleased
headline: `charter claude` runs the command you launch Claude Code with
---

`charter claude` used to exec `claude` by name, and there was no way to tell it otherwise
short of `charter frame -- ccs work`, which runs the words and forgets the harness: no
`$CHARTER_HARNESS` in the pane, no workspace layer, nothing `charter reopen` can resume.

Now a per-developer file names your launcher, one table per harness:

```toml
# .charter/local.toml — yours, never committed
[harness.claude]
command = ["ccs", "work"]
```

`charter claude` runs `ccs work` in `claude`'s place, your arguments follow it, and it is
still the Claude Code harness charter is launching. `.charter/` is already gitignored on
every plane, and that is the point rather than a convenience: the containment rule keeps a
committed `charter.toml` from choosing what runs on a teammate's machine, and which account
you bill a chat to is yours to decide. `charter doctor` has a `local.toml` row that names a
table charter cannot honour, and `charter harness list` shows the command in force.
