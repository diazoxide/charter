---
version: unreleased
headline: Switching workspace moves your terminal and stops nothing, so several harnesses can run at once with one on screen
---

*"switching workspace — it means keep opened chat opened in background, so user can
simultaneously run many harnesses in one charter environment. Changing workspace or changing
sessions does not mean stopping old chat session."*

That is the feature, and until now charter had the opposite of it. The workspace tab and the
`F2 → workspace` row both refused: a chat belongs to its workspace for life, so the switch
that used to re-point one was removed and nothing replaced it. On a plane with fifteen
workspaces that left a bar of fifteen tabs where every click put a sentence on screen and
moved nothing.

**The sentence was right about the chat and wrong about the operator.** Moving a *chat*
between workspaces is still forbidden and still impossible — its id is `<workspace>.<n>`, and
the harness inside it has that workspace's directory as its cwd, that workspace's files open
and that workspace's work in its history. What was missing is a different operation on a
different noun: **moving the client.** A workspace is a tmux session, your terminal is a tmux
client, and `switch-client` puts one on the other.

So a workspace tab, an `F2 → workspace` row and `charter frame-switch --workspace <name>` now
all move your terminal to that workspace. Everything you left keeps running: the same harness,
the same pid, the same window, the same workspace. Come back and it is where you left it, and
the workspace you arrive in is whichever chat it was last showing.

Measured with a real client on tmux 3.7c and at the 3.2 floor, identically on both: after the
switch every pane on the server is still there with the same pid and none of them dead, and
the `attach` process itself is untouched. `switch-client` costs about **5 ms**. What it costs
beyond that is one re-layout at each end — the panels of the chat you left are torn down, and
the chat you arrive at gets fresh ones, because a tmux window that is not the current one
keeps the size it had when it was, so panels left running in one are drawing at a width that
is no longer their window's.

## Switching is restricted to the workspaces of this plane

One tmux server serves **every plane on this machine**. The socket charter uses had eleven
sessions on it from three different projects the day this was written, and `default` — a name
every plane has whether anybody chose it or not — was one of them. A switch decided on a
session *name* would therefore have been able to put you inside another project's harnesses,
across every isolation boundary charter has: a different `CHARTER_ROOT`, a different persona
set, different vaults, different memory.

Charter will not do that, and a workspace of another plane is refused by name rather than
silently landed in. Which session is yours is decided by two facts and neither of them is a
name:

* the `%<pane>` id **this plane's own launcher** wrote down for a chat in `.charter/frame/` —
  minted by the server, held by one plane;
* a new `@charter_plane` marker on the session, holding that plane's `.charter` path, which
  refuses any session that says it belongs to somebody else.

Each closes what the other cannot. Pane ids restart at `%0` when a tmux server does, so a
pane id recorded for a chat that is over can later name another plane's live pane — the
marker refuses that. And a session started by a charter older than this carries no marker at
all, including every session running on this machine today — those are still found by the
pane record, exactly as they were before. The marker is written once, by the launch that
*creates* a workspace's session, and never by a launch that joins one: joining is decided on
the session name, which is the collision itself, so re-marking there could relabel another
plane's session as yours.

## A workspace with no session yet is opened

The bar lists every workspace the plane has, and most of them are not running. Clicking one
opens it and takes you there — see *A workspace tab opens the workspace it names*, which
replaced the refusal this section used to describe and explains why the reason it gave
("opening ends in an attach, and a switch has no terminal to attach") was measured to be
false.

The other refusals are a name that cannot name a workspace, a name this plane does not have
(with the names it does have beside it), the workspace you are already in, and a terminal
that has moved nowhere — the teardown of your panels is gated on charter having *read* your
client on the other session, not on `switch-client` having exited 0.

## Verification

Two planes on one machine, one tmux server, and one workspace name they share — the only
arrangement the cross-plane flaw exists in, and a single-plane test is structurally blind to
it. The plane that opened the workspace resolves it; the plane that did not resolves nothing,
though its workspace name, its chat id and the live session's name all match. Then the same
fixture with the recycled pane id — plane B recording the pane id the server actually
minted, which is what a `kill-server` produces — is resolved for plane B **without** the
marker and refused **with** it, so the marker is measured against the hazard it exists for
rather than against a hypothetical.

Six hand-written mutations of the switch were run and every one goes red: dropping the plane
veto, dropping `-c <client>` from `switch-client`, never verifying the client moved, dressing
the seat charter matched instead of the window tmux landed on, leaving the old chat's panels
up, and writing the plane marker on a launch that joined a session rather than made one.

Run against real tmux on **3.7c and on tmux 3.2**, the floor charter promises. CI installs no
tmux, so the real-server half runs only by hand.

Nothing to adopt — the `workspaces` bar is still off by default (`[frame] slots`), and a
switch is the same command it always was.
