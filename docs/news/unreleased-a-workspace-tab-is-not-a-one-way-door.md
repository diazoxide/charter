---
version: unreleased
headline: A workspace tab is no longer a one-way door — the tab back works too
---

*"I switched to `fleet` workspace, it switched with a new empty chat session, then when I
want to switch back — I get an error. When I tried to terminate the chat session with
Ctrl-C, charter closed and I see our current session, so the first session survived and was
not closed."*

The change before this one made every workspace tab open the workspace it names. It opened
one. It did not let you leave it: in the chat the tab had just opened, every tab — including
the one you came from — answered

```
cannot switch: this chat is a window in your own tmux, where a workspace is not a session
```

Nothing was lost. `Ctrl-C` closed the new chat and put the original one back, exactly as
that operator found. But the only way out of a workspace you had clicked into was to kill
the chat you had clicked into it with.

Both tabs work now, in both directions.

## Two spellings of one socket

Charter runs its frames on a private tmux server of its own, which it starts and reaches by
name: `charter`. Every other server — the tmux you had open before charter existed — it
reaches by the path of the socket file, and it treats a frame there completely differently:
your window, your prefix key, no charter hotkey, and no workspace switching, because inside
a tmux you already have, a workspace is not a session and there is nothing correct for a
switch to do.

Charter told those two apart by asking whether the name started with a `/`.

Which is true of the operator's own server, and *also* true of charter's own — because tmux
writes the socket into every process it starts in a pane, and it writes it absolute. So a
click, which runs in a process tmux started in one of charter's own panes, read
`/private/tmp/tmux-502/charter` where an ordinary launch reads `charter`, and every later
question about that chat was answered for a server charter had started itself.

The refusal was not the bug and it has not been deleted — inside a tmux you already have it
is still exactly right. What was wrong was believing this frame was in one. Charter now
compares the *servers*, resolving either spelling to the socket file it names, so `charter`
and `/private/tmp/tmux-<uid>/charter` are one server and a tmux you started yourself is
still yours.

## The tab was opening the workspace in the wrong place, too

The same false premise reached further than the message. The launcher chose between
"a session on charter's own server" and "a window in the session I am already in" on whether
`$TMUX` could be read at all — so a tab click opened its chat as a window *inside the
workspace you were leaving*. There was never a session for the workspace you clicked, which
is why nothing could switch you into it, and the launcher on that path stays awake for the
life of the harness, so the click never even finished. That is what a "new empty chat
session" with no way back actually was.

On charter's own server a workspace is a session. The launcher now asks whose server it is
in, not merely whether it is in one.

## What this changes if you type `charter <harness>` inside a frame

Rare, and worth stating rather than leaving you to find. Typed at a shell *inside* a charter
frame, `charter claude` used to make a window in the session you were standing in — a chat
for one workspace living inside another workspace's session, whose own workspace tabs then
refused with the message above. It was the same one-way door, reached by typing instead of
clicking.

It now builds the workspace's own session, correctly, and then attaches to it from a
terminal that is already inside that server — a tmux nested in a tmux, which works (measured
on 3.7c and at the 3.2 floor) and stacks two prefix layers on one terminal. That is worse to
look at than what it replaces and better than what it replaces was doing. Opening a second
workspace from inside a frame is what the workspace tabs are for.

## Verification

The end-to-end case that was missing, and the reason this shipped broken: every real-tmux
test charter has starts from a socket charter created, reaches it by the name it created it
under, and runs with `$TMUX` unset. A click never happens in that environment. It happens in
a process tmux started in a pane.

So the whole of the previous change's end-to-end suite is now re-run with the environment a
click really has — a real server, a real client on a real 132-column pty, a real launch — and
with the trip the operator actually reported: click `beta`, land in `beta`, click `alpha`,
arrive in `alpha`, with the chat left behind keeping its pid through both legs. The click
runs under a deadline, because on the old code it did not return a wrong answer, it did not
return at all.

The refusal keeps its own test: a frame genuinely inside a tmux charter did not start is
still refused by name, and asks the server nothing at all before it refuses.

Run against real tmux on **3.7c and on tmux 3.2**, the floor charter promises. CI has tmux
and runs everything that needs only a tmux server; the cases that need a client on a real pty
skip there, because the runner has no terminal tmux will hand one.

Nothing to adopt — the `workspaces` bar is still off by default (`[frame] slots`), and a tab
is still one click.
