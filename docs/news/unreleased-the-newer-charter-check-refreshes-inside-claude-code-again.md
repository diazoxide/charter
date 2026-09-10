---
version: unreleased
headline: The newer-charter check refreshes again inside Claude Code — the frame and SessionStart start it, not only a footer nothing draws
---

**`charter version` could tell a plane two releases behind that it was up to date.** It
answers from `.charter/cache/update.json`, and one detached child refreshes that file:
`charter _version-check`. Only the status line's render started that child. Inside a frame
the footer command returns before it renders (since 0.52.0), and outside one `charter init`
has written no footer since 0.57.0. No hook ran the check either. In Claude Code the cache
moved only when someone typed `charter update` or `charter version bump`. On the plane that
reported it, the cached answer was five days old. `charter version` said "up to date", and
`charter report send` gave no staleness warning.

The check now has the same triggers the forge-state cache already had:

* **the frame's gather**, next to the forge refresh it already started, so a framed chat
  that stays open past a day checks again;
* **`charter hook sessionstart`**, called in-process, so a chat outside a frame checks when
  it starts. `hooks.json` gains no command, so there is no plugin to update for this.

The throttles did not change. The check runs at most once a day, and it is attempted at most
once an hour, including when an attempt fails offline. A frame panel with no gather cache
scans on every repaint and still forks one check, not one per tick.

The status line's own render still starts it too, so `charter statusline --watch` and
opencode's `/charter` refresh as they did before.

Nothing to adopt. The first session you start after updating refreshes a cache older than a
day.
