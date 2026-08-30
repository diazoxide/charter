---
version: unreleased
headline: A chat switch that cannot move you stops reporting success and tearing your panels down
---

`F2 → chat → <name>` could leave you exactly where you were, with the panels of the chat
you were looking at killed around you. Charter thought it had switched, because tmux told
it so.

## A command that exits 0 on the wrong target

`cmd_chat` aims `select-window` at the target chat's own harness pane, and then checks the
return code. Measured on real tmux 3.7c, own socket, two sessions with the client in the
first:

```
$ tmux -L … select-window -t %3        # %3 is a window of session B; client is in session A
rc=0
after:  A current=@0   B current=@2    # session B moved. The client did not.
```

So the check could never fire. What followed it was `_apply_arrangement(fid, want=[])` —
the teardown of the chat being left, which #668 rightly calls a correctness rule — applied
to the chat still on screen. Reproduced end to end against a real server on 3.7c and on
tmux 3.2, driving the real `cmd_chat`: nothing said, the client never moved, and both
panels of the chat in front of the operator gone.

## Why two chats of one workspace can be in two tmux sessions

Because membership is a **file**. `chats.of_workspace` lists the frame directories whose
`workspace` file names the workspace, and that file moves at runtime: `charter workspace
use beta` typed at the agent rewrites it — *"it moves the panels too"* is a documented
promise — and the workspace picker rewrites it again. The tmux **session**, meanwhile, is
fixed at launch: `cmd_launch` makes the workspace the session and a chat one window in it.

So after one `F2 → workspace → beta` inside chat `api.1`, the roster for `beta` holds
`api.1` (a window of session `api`) beside `beta.1` (a window of session `beta`), and
every check charter had said yes.

## What it asks now

The guard wanted a property, not a spelling, and there are two of them.

**Which server** is a record, so `chats.check` asks it: charter runs frames on its own
`-L charter` and, inside an operator's tmux, on theirs, and pane ids are per-server — `%3`
recorded by a chat on one names a real, live, unrelated pane on the other. No default is
filled in for a missing marker; every chat this charter launches records one on both
paths.

**Which session** is a fact only tmux holds and it moves while the palette is open, so
`cmd_chat` asks it, of both chats, before it aims anything anywhere. Same session: switch.
Different: refuse, with a sentence, having selected nothing — so the other session's screen
is left alone too.

And the teardown is gated on the client having **moved**, read back from tmux, rather than
on a command having exited 0. Those are two different facts and the panels ride on the
second one.

## The reading is the guard, not the status

`display-message -p` against a target tmux cannot resolve answers **rc 0, empty stdout and
no stderr** — measured on 3.7c. So the placement readings are held to their shape
(`$<digits>`, `@<digits>`) rather than to their exit status, which is the same
rc-0-on-the-wrong-thing this whole entry is about, one command over. An empty `-t` target
is not nothing either: it resolves to the current window, which would have charter report
the asker's own place as the target's.

## One consequence worth knowing about

A chat whose own harness-pane record is unreadable now refuses the switch instead of
moving the client anyway. That reversal is deliberate: the record that says where to tear
down is the record that says which session this client is in, so without it charter can
neither check the target nor tell afterwards whether anything moved — and selecting anyway
means aiming at a pane that may belong to somebody else's session.
