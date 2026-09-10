---
version: unreleased
headline: On the dev channel, `charter version` stops calling an older release newer, and `version bump` pins only what it fetched
---

On a plane that declares `[update] channel = "dev"` while running the PyPI wheel,
`charter version` printed this on one screen:

```
• A newer charter is published (0.58.0).
•   update, commit and push the lock:  charter version bump --push
  installed  0.60.0
  latest     — (cached 0.58.0 is stale: it predates the 0.60.0 you are running)
```

The check compared `main`'s cached head with this build's commit. The headline printed the
cached PyPI number instead, which that check never looked at. The next step it named does
not suit a dev plane either: it writes a pin that the plane's own session start reports as
contradictory.

Now the verdict names only what was compared. On a git build it names the cached head and
your commit when they differ, and does not say which is newer: this cache is this plane's,
the binary is the machine's, so another plane's `charter update` can leave the head cached
here behind the build you are running. On a build that records no commit it says that this
plane follows `main` and this build was not installed from a commit of it. Either way it
names `charter update`, or a `git pull` when the charter you run is a clone you are working
in. `charter report send` prints the same sentence, so neither surface tells a dev build
that a commit it may already contain holds the fix. The stable channel's output is
unchanged.

`charter version bump` with no `--to` now pins only the version PyPI returned to that same
command. Before, it re-read the cache after fetching, and a failed request leaves the cache
as it was. So instead of refusing as "offline?", it installed the stale cached release over
the running one and pushed that pin to the team.
