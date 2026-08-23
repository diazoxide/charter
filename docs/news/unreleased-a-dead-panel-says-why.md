---
version: unreleased
headline: A panel that dies now says why, and comes back
---

A charter panel that failed to start showed you one line, and it was tmux's, not charter's:
`Pane is dead (status 2)`. Nothing else. The panel's own stderr did reach the pane — and
was then scrolled straight out of it, because tmux writes that message by pushing the pane
up exactly one line first, and `top` and `bottom` are one line tall. Diagnosing it meant
copying the panel's argv out of the frame and running it by hand.

Two changes, one for each half of a dead panel.

**A panel that can see its own failure no longer exits.** It paints the reason into its own
pane and stays there, so the pane keeps it: `charter: unknown slot 'sideways' (known:
bottom, left, right, top)`, or `charter: bottom panel stopped (ValueError: ...)`, clamped to
the pane's real width. This is the promise `slots.render` already made for a renderer that
raises — a panel never leaves a hole in the frame — extended to every failure that happens
on the way to it, which is the half it could never cover from where it sat.

**A panel whose process is gone is brought back.** Each panel pane now carries its own
`pane-died` hook, and a death respawns it after a growing pause — three attempts, then
charter stops and leaves the dead pane and its message exactly where you can read them. The
hook is scoped to the panel's own pane, so nothing about it touches the harness pane, whose
own `pane-died` hooks carry your agent's real exit code. A panel still cannot take the agent
down with it; now it also cannot stay dead for the frame's whole life.
