---
version: unreleased
headline: The chat bar's + makes a chat, instead of naming the command that would
---

*"`+` button not working for creating new session."*

It was never a button. The chat bar's add-chat affordance was a **sentence** —

```
  chats  *api.1  + charter <harness> opens another
```

— which is true, which names the command that does it, and which sits at the end of a row
of clickable tabs beginning with a `+`. Every terminal an operator has ever used puts a `+`
there and every one of them means *new*. So it got pressed, and a sentence cannot be
pressed.

```
  chats  *api.1   api.2   +
```

Press it and you get another chat: same workspace, same harness you are already in, its id
allocated for you, your terminal on the new window, and the chat you left running behind
you with its harness and its conversation intact. It takes nothing and asks nothing,
because there is nothing to ask — a chat's id is allocated and its workspace is fixed for
life.

## Why it needed a new command

`charter <harness>` in the workspace has always opened a second chat, and the obvious fix
was to have the panel run that. It does not work, and the way it fails is instructive: a
click's work runs **detached, with all three streams on `/dev/null`**, and the launcher
reads a non-tty stdout as *this process cannot be the operator's terminal* and replaces
itself with a bare harness writing into the void. No frame, no chat, and no process left to
say so.

The seam that says *build the frame, do not become the terminal* has existed since
workspace tabs needed it, and until now nothing on the command line could ask for it.
`charter frame-new-chat` is that spelling. You can type it inside a frame; it is what the
`+` runs.

## When it will not, and what it says

A `+` that silently failed would be this same report one release later, so every stop puts
a line on the frame's own attention row:

- your frame is a **window in a tmux you already had** — charter makes no chats for you
  there, and says so with the command that does work: `charter <harness>` in the workspace;
- charter **cannot prove the workspace's tmux session is this plane's**. One tmux server
  serves every plane on your machine, and adding a window to a session charter cannot
  identify would put a chat inside another project's frame;
- this chat **records no harness** charter can launch and your plane declares no
  `[harness] default`;
- charter **cannot enter** the workspace's directory.

## The workspace bar still has no `+`

Deliberately. A new chat is nothing but a press. A new workspace is a directory and a
*name* — `charter workspace create`, with a validation pass behind it, because a picker
that creates on a typo leaves litter.

## Verification

Real tmux on **3.7c and the 3.2 floor**: a real frame, a real client on a real pty, the `+`
pressed through the client's own terminal, and the answer coming back on the frame's own
attention row — written there by a real detached `charter frame-new-chat` that resolved
which frame it belonged to, established it was on charter's own server, and proved the
workspace's session was this plane's with a real `list-panes`. Beside it: the keyboard
stays on the harness, the cell next to the `+` reaches nothing, an unpaired release reaches
nothing, and a press that stopped created nothing.

**One limit, stated rather than left to be found.** The launch itself is asserted against a
stand-in rather than run for real, because the launcher targets charter's own shared tmux
server by a module constant — so a test that let it run would build a session on the
operator's live server. What is faked is exactly one call; everything up to it is real on
both tmux versions.
