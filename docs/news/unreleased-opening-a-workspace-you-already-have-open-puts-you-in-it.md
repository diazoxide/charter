---
version: unreleased
headline: Opening a workspace somebody already has open attaches to it instead of dragging them off what they were reading
---

`charter -w foo` used to mean "add a chat to `foo`" whatever `foo` was doing. Where
somebody was already attached, that was measurably destructive: both clients of a tmux
session share one current window, so the `select-window` that put the *new* launch on its
new chat pulled the *existing* client onto it too, and charter then tore down the panels of
the chat that client had been reading.

**There is no small fix for the drag.** Measured with real ptys on tmux 3.7c and at the 3.2
floor alike: a client on `chat1`, then `new-window -d` plus `select-window`, lands that
client on `chat2` — identically on both. tmux has no per-client current window inside one
session. The one mechanism that does not drag is a session *group*, and adopting groups
would stop a workspace being a tmux session, which is what everything since chats has been
built on.

So charter stops making the situation instead of fixing it. **A workspace somebody is
already attached to is focused, not added to**: your terminal attaches to the session they
are on, looking at the window they are on. Nothing is started, no chat id is claimed, no
directory is made, and no window is selected. Where nobody is attached — you closed the
terminal, or never had one — the same command opens a chat exactly as it did before, because
there is nobody there to drag.

Measured on 3.7c and at the 3.2 floor, identically on both: a second client attaching to a
session does **not** move that session's current window, which is the fact that makes
focusing safe where adding was not.

To open a second chat in a workspace you are already in, run `charter <harness>` from inside
the frame. That is what the chat bar's own `+ charter <harness> opens another` names, and it
is unchanged — a charter started in a frame's pane is inside a tmux and takes the window
path, not this one.

**A launch that names something to run still runs it.** Attaching answers "put me in `foo`";
it cannot answer "run *this* in `foo`", and a focus taken over one would silently throw away
an argv you typed. So `charter frame -- <cmd>` — the escape hatch for a command charter has
never met — and `charter claude --resume <id>` both open a chat and run what they named,
attached client or not. Only a launch that asked for nothing but the workspace is answered
by focusing one.

## Which workspace is yours is read off this plane's disk, never off a session name

One tmux server serves **every plane on this machine** and session names are bare workspace
names, so `default` — a name every plane has whether anybody chose it or not — names one
session that any of them might have opened. A focus decided on that name would have turned
an existing cross-plane collision into charter's advertised behaviour: `charter -w default`
in one plane attaching to another plane's frame.

So the question starts on disk. Charter looks for a chat directory of *this* plane's for
that workspace, takes the `%<pane>` id its own launcher wrote down for it, and asks tmux
whether that pane is live and which `$<session>` holds it — ids the server minted, which
only one plane can be holding. A plane that has never opened a workspace has no directory
for it, makes no tmux call at all, and can never focus anybody.

The residual is stated rather than hidden: pane ids restart at `%0` when a tmux server does,
so a pane id recorded for a chat that is over can, after a `kill-server`, name another
plane's live pane. Closing that needs a plane marker on the window, which is a stage rather
than a line.

## Verification

Two planes on one machine, one tmux server, and one workspace name they share — including
the same chat id, `shared.1`, which both planes mint first — is the arrangement the flaw
exists in, and a single-plane test is structurally blind to it. The plane that opened the
workspace focuses it; the plane that did not focuses nothing, though its workspace name, its
chat id and the live session's name all match. Both wrong implementations were run against
that test and both go red: matching on the session name, and matching on the chat id.

And a whole `cmd_launch`, twice, against a real server with a real attached client and two
real plane roots — identical on 3.7c and at the 3.2 floor. On this branch the second launch
attaches to `$0`, adds no window and leaves the client on `shared.1`; on `main` the same
second launch adds `shared.2` and moves the attached client onto it. The third launch, from
the other plane, opens its own chat on both.

Run by hand on **tmux 3.7c and on tmux 3.2** — 29/29 on each. CI installs no tmux, so none
of it runs there.

Nothing to adopt.
