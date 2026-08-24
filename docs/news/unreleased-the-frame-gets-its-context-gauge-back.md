---
version: unreleased
headline: A framed Claude Code session shows ctx and cache again, on the top row
---

0.52.0's own news entry named this as the one thing the frame genuinely lost: `ctx NN%`
and `cache NN%` lived on the status line, the frame blanked the status line, and no panel
drew them. The recording was deliberately kept alive so a panel could be given one. This is
that panel.

**Why it took a release, in one sentence:** the usage history is keyed by *Claude Code's*
session id, and a panel only ever knows the *frame's* id — the one set in its environment
by the launcher. There is exactly one process that sees both at the same moment: the
suppressed `charter statusline`, which has the frame id in its environment and Claude
Code's in the JSON payload on its stdin. Inventing that mapping as a side effect of the
suppression bugfix would have been a design decision smuggled in under a fix, so it was
reported instead. It now writes the mapping into the frame's own state directory, and
`top` reads it back.

**One number had to start being written down.** The cache ratio and the rebuild counter
are both computable from the token counts already in the history; the context percentage
is not — it exists only in Claude Code's per-turn payload. So each recorded turn carries it
now, as a fourth field. Rows written by an older charter still read fine, and rows written
by this one are still readable by an older charter, so an upgrade mid-session neither loses
the history nor corrupts it.

**The two surfaces are one implementation.** A panel's gauge and Claude Code's footer draw
the same numbers from two different sources, so they go through the same formatting and the
same colour thresholds. A green 60% in a frame and a green 60% in a footer mean the same
thing, and there is no second set of numbers to keep in step.

**Not knowing shows nothing, and that is the rule rather than a fallback.** A frame whose
harness has recorded no turns yet, one whose harness is not Claude Code at all (nothing
else is handed a usage payload), one whose history predates the fourth field, one whose id
was recycled from a dead frame — each of those draws no gauge rather than a confident
`ctx 0%`. A gauge silently reading zero is worse than no gauge; a gauge reading somebody
else's 78% is worse than either.

**The panels are woken once per turn, not once per render.** A panel repaints on a version
bump and on nothing else, and recording usage bumps nothing — so a gauge left to the next
unrelated hook would sit stale through any turn that called no tools. Claude Code re-renders
the status line several times per turn, though, and each bump repaints every panel, so the
frame is woken only when a turn was actually recorded or when the session id on file
changed.

`ctx` also survives `density = "minimal"`, where the charter version does not: the version
is a standing fact about the install, and the gauge is the one field on that row with
something new to say every turn.

codex and opencode still show no gauge, and for the harder reason that has not moved:
nothing hands either of them a per-turn usage payload, so there is no history to read.

Nothing to adopt — upgrading is the whole of it.
