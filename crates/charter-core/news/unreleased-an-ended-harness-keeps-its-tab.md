---
version: unreleased
headline: `/exit`, a double Ctrl+C or a crash no longer closes a chat — its tab stays and offers resume, a fresh start or close
---

Until now every harness exit was final. `pane-died[1]` was `kill-window`, so the moment a
harness stopped — `/exit`, Ctrl+D, a double Ctrl+C, a crash, a `kill -9` from another
terminal — charter killed the chat's window and reaped its directory. From the operator's
seat that reads as charter having closed, and there was nothing left to resume from.

**A harness that ends now keeps its tab, and the tab offers a choice.**

- **A clean exit** puts the profile selector back in the same pane, with **resume
  &lt;session&gt;** first and preselected when the conversation actually exists, then the
  profiles, and Esc to close the tab.
- **A crash** leaves the dead pane exactly as tmux keeps it — the harness's last lines and
  tmux's own `Pane is dead (status N)` stay on screen — and opens the same three choices in
  a small drawer split beneath it.
- **With no conversation yet** there is nothing to resume, so the tab offers a fresh start
  and close.
- **Background and handed-off chats** end the same way, marked `x` in the chat strip, with
  the choice waiting whenever somebody switches to them.

**Closing a tab is now the one way to end a chat, and it ends it for good.** It still asks
first while a harness is running; an already-ended tab closes without asking. A quit records
ended tabs with their session link, so `charter reopen` brings them back ended and still
resumable.

`charter claude` now returns when the tab closes or you detach, rather than when the harness
exits — the exit code is the one the chat recorded, as before, arriving later.
`charter frame -- <cmd>` is unchanged: its window closes when the command exits and the exit
code goes back to the caller.

Charter still restarts nothing by itself and never types into a harness (ADR 0018, amended
by this change). Every restart is an operator's Enter on a row: no timer, no retry, no
automatic resume, and neither end of input nor Ctrl+C is ever read as a choice.

**Limits.** Ended tabs stay until somebody closes them. Ctrl+C is not disabled — a double
Ctrl+C still ends Claude Code; what changed is that the tab survives it. A chat whose pane
charter never marked as its own — a session an older charter created — is left alone: its
harness's exit leaves a dead pane with tmux's own `Pane is dead` on it until the tab is
closed.
