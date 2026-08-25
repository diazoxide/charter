---
version: unreleased
headline: A background refresh now refreshes the plane that asked for it — and charter's suite stopped running 131 of them against yours
---

The status line never blocks on the network. When its forge cache or its version cache goes
stale it forks a detached `charter gl-refresh` / `charter _version-check` and draws the old
values; the child does the work and rewrites the cache. Session-start hooks do the same for
`charter persona _gc`.

**The child was never told which plane it was refreshing.** It got a bare copy of the
environment and worked it out for itself, by walking up from whatever directory it happened
to inherit. Almost always that is the same plane the parent resolved — and the exception is
the one that matters, because a linked worktree redirects to the tree it was cut from
(`root._plane_of`, and rightly so). Render the status line for one plane from inside a
worktree, and the refresh landed on another.

```
render:  plane = /tmp/some-plane          (what the status line is drawing)
child:   plane = ~/work/charter           (what gl-refresh actually refreshed)
```

`glstate.maybe_spawn` already argues this exact point about the *workspace*: "the status
line resolves the workspace for the SESSION … while the child would resolve it for ITSELF,
from its own environment and its own directory". The plane is the same argument one level
up, and the bigger half — the workspace decides which rows get refreshed, the plane decides
whose `.charter/` gets written.

**Now the parent hands it over.** `util.child_env` puts the plane this process actually
resolved on the child's `$CHARTER_ROOT`, and all three spawn sites use it. The child agrees
with its parent, or there is no child.

**And with no plane, there is no background refresh at all.** Outside a control plane
`config.STATE_DIR` is `<cwd>/.charter`, so a spawn there scattered charter's caches into
whatever directory the render happened to run in. Both refreshers now decline to fork.

## What this was found by

charter's own test suite, forking 131 detached charter processes in a single run — every
one of them against the machine's live control plane, refreshing its forge state and
rewriting its caches. `tests/_planeguard.py` had said for months that it could not see this:

> **What this cannot see: a subprocess.** … isolating this process does nothing for it.

Six test modules had each remembered the precaution by hand (`update.maybe_spawn = lambda:
None`, with a comment saying *never fork a network child from the suite*). Twelve had not,
and one file had remembered it in one case and not in the three beside it.

So the warning is a tripwire now. Before any `Popen` that launches charter, the suite asks
`root.find_root` **the child's** question — with the child's environment and the child's cwd
— and refuses, by name, if the answer is your plane:

```
REFUSED: spawning charter against the real control plane
tests.test_cli_smoke.CliSmokeTest.test_doctor_runs_without_crashing is about to run
`… -m charter doctor`, and that child would resolve its plane as ~/work/charter …
```

It found eight more call sites the moment it was armed. One is a smoke test whose own
docstring claimed isolation — it set `$CHARTER_HOME`, which covers the per-human directory
and reads as though it covered the plane — while running `charter doctor` against the
developer's personas, workspaces and vault registry.

**Two of the eight only fire inside a charter frame**, which is the part worth keeping.
`$CHARTER_ROOT` wins outright over any walk, and every frame exports it — so two suites
that isolated their children by `cwd` were correct in a bare shell and wrong in the terminal
they are actually written in. One of them ran `charter init` against the operator's plane.

`root.find_root` grew an optional `env=` for it, so the guard asks the real resolver instead
of keeping a private copy of the walk that would drift away from it.

This reaches you as a correctness fix in `charter gl-refresh`; the rest only ever mattered
if you run charter's own test suite. Nothing to adopt — upgrading is the whole of it.
