---
version: unreleased
headline: The frame draws the workspace you launched it for, and says "gathering" instead of "no repos"
---

Open a session and the frame's repo table was empty. Resize the window and the repos
appeared. The resize was a coincidence — it happened alongside a tool call, and a tool
call is what refreshed the table.

Two separate things were wrong, and both are fixed.

**A panel could not tell which workspace it was drawing.** `charter` resolves the active
workspace from, in order: `--workspace`, `$CHARTER_WORKSPACE`, the tree you are standing
in, a per-session pointer, a per-terminal pointer, and the declared default. A panel is a
long-lived tmux pane command, and at launch it reaches **none** of the rungs that could
speak for the frame. `$CHARTER_WORKSPACE` is empty unless you pinned one by hand (a frame
is given every identity variable explicitly, present or not, so that a second frame cannot
inherit the first one's pin). Its working directory is the pane's, which is the plane root
for anyone who typed `charter claude` there. The per-session pointer is keyed on the
session id — which inside a frame names the *frame*, and nothing has written one yet at
launch; the per-terminal pointer is keyed on the asking pane, which is the panel's own. So
every panel fell all the way through to `default` — while the launcher, one ordinary shell
in your own terminal, had answered correctly one rung up.

On the plane this was reported from, that is the whole bug: three terminal pointers naming
`harness-wrapper`, one naming `user-reporting`, and a `default` workspace with no clones in
it at all. The launcher sized `bottom` for the real workspace's repos; the panel drew
`default`'s empty list into it. The header said `default` too. The rows only appeared once
a tool call's hook refreshed the cache from inside the harness, which resolves it the way
you would expect.

**The launcher writes the frame's workspace down now**, and every surface of the frame
reads that one answer — the header, the todo count, the alerts, the repo table, and the
height the pane is given. Nothing is pinned into your environment to achieve it: exporting
`$CHARTER_WORKSPACE` would have fixed the same reads and taken `charter workspace use`
away from every framed session on the way past. A frame recording what it drew is not the
same thing as a session being pinned.

**It is a seed, not an override**, which matters if you switch mid-session. `charter
workspace use <name>` typed at the agent still moves the panels, exactly as it always has
and for the same reason — inside a frame the frame *is* the charter session, so the
pointer is written under the frame's id and the panels read it back under the same one.
The order is: what you chose inside this frame, then what the launch resolved, then
whatever the panel can resolve for itself. A frame already running across the upgrade has
no record and lands on that third rung, so it behaves exactly as it does today; relaunch
it and it draws yours.

**And the table it draws was never being filled at launch.** A launch deliberately deletes
the cached repo scan before it draws anything — a frame id is `<workspace>-<launcher pid>`,
pids get recycled, and a new frame must not adopt a dead one's rows. Nothing refilled it.
The only thing in charter that refreshed that cache was the hook on your tool calls, so the
table was populated by your first tool call and by nothing else.

It is filled at launch now, by a detached `charter frame-gather` — the same shape charter
already uses for its version check and for forge state: the frame appears immediately, a
child gathers alongside it, and the rows arrive a beat later when it bumps the frame.
Nothing is added to the path you are waiting on; a cold scan is around 35ms and three git
invocations per repo, and `charter claude` is the default way charter starts.

**In the gap, the pane says what is true.** `⋯ gathering this workspace's repos…`, not an
empty table. An empty table on a plane with fourteen repos reads as "no repos", which is
the same confidently-wrong output the 22-column left sidebar was retired for. The line goes
the moment the rows land, and a workspace that genuinely has no clones still draws exactly
the one-row strip it always did — the two claims are kept apart rather than spelled the
same way.

**A panel is a pure cache reader again, which it was supposed to be already.** The cache
reader it calls used to fall back to a live `git` sweep whenever there was nothing to
read — and a fresh frame is exactly that state, by design, on the repaints you are
watching. `bottom` is the one slot that animates, so that fallback was a full sweep five
times a second for as long as anything was in flight. It no longer has one: the panel reads
the cache or says it does not have one.

Nothing to adopt — upgrading is the whole of it.

One surface still re-derives it and is filed rather than fixed here: the **harness pane**
itself (**#524**). Your agent session reaches the same dead rungs a panel did, so on a
plane whose workspace came from a per-terminal pointer the frame can now draw
`harness-wrapper` correctly while the session inside it resolves `default`. Reconciling
those two is a design decision rather than a bug fix — pinning the session to the frame
would take `charter ws use` away from every framed session — and it is the same question
**#517** (switch workspace from inside the frame) and **#518** (pick one at launch) are
approaching from the other end.
