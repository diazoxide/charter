---
version: unreleased
headline: F10 closes charter — detaching only the terminal you pressed it in, or stopping every chat after a confirmation — and F2 → detach detaches again
---

Leaving charter was four gestures with no single answer: closing the window detached it,
`F2 → detach` was broken, `F2 → charter: quit` stopped the plane, and `chat: close` stopped
one tab. There is one way out now, and it is in one place.

**`F10` opens it wherever the frame draws**, and so does `F10 close` at the right end of the
identity row. Two rows, with the harmless one already selected:

- **Close charter (keep chats running)** detaches the terminal you pressed it in and nothing
  else. Every chat keeps running, every other terminal attached to the project stays
  attached, and `charter` in that project puts you back.
- **Close charter and stop all chats…** opens the confirmation you already know — every chat
  listed with what it gets back, ended tabs included — and then records and stops them.
  `charter reopen` brings the plane back.

The same two rows are in `F2`, in the same words.

**Ctrl+C is not disabled and this does not replace it.** Ctrl+C is still your harness's
interrupt. What changed earlier in this release line is that a harness exiting no longer
destroys a chat.

**`F2 → detach` works again (#1097).** It named the chat as a tmux session, and tmux reads a
target with a dot in it as `session.pane` — so on a chat with no strips it detached nothing
while reporting success, and on any chat with a strip it detached *every* terminal attached
to that workspace. It now names the terminal that asked, after proving on the chat's own
server that the terminal is attached to that chat's session and that the session belongs to
this plane.

**What it will not do.**

- On charter's own server the binding is a root key-table entry, so tmux matches `F10`
  before the pane your harness runs in — and a `[[frame.component]]` key of `F10` and a
  `[frame] hotkey = "F10"` are both refused for the same reason. Inside a tmux you already
  have charter binds nothing, so `F10` reaches your harness there as it always did.
- **Inside a tmux you already have, charter binds no key and draws no button.** The two rows
  are in the palette, where *Close charter* is listed refused because your own prefix key
  detaches that window.
- The button needs `[frame] mouse`; with the mouse off — the default — it is a label that
  teaches the key.
- A detach charter cannot attribute does not happen. Where charter cannot tell which terminal
  asked — a frame whose key binding predates this release is still in the running tmux server
  — the row is listed with its reason and does nothing. Detaching every terminal instead would
  close somebody else's on a project two people have open.
