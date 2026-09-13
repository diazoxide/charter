---
version: unreleased
headline: `charter persona clear` in a chat clears that chat's selection, and no longer erases the one the launching terminal made
---

Inside a charter frame launched from Terminal.app or iTerm2, every chat inherits the
launching terminal's `$TERM_SESSION_ID`. `charter persona clear` in such a chat deleted the
terminal pointer keyed on it, which was the launching terminal's own choice. Measured: the
launching shell ran `charter persona use forge`, a chat ran `charter persona clear`, and the
launching shell then had no persona. It printed:

```
✓ Active persona cleared.
```

Now, inside a chat, `clear` removes only the chat's own session pointer, leaves the terminal
pointer and the plane-wide `.charter/active-persona` alone, and says what is left:

```
✓ Active persona cleared for this chat only — other chats and terminals keep theirs.
• This chat now resolves to 'forge' (via terminal).
```

A chat that never selected a persona of its own is told so rather than told it cleared one.
This is the other half of `persona use` in a chat, which no longer writes a terminal
pointer. Outside a chat nothing changes: `clear` still removes the session
pointer, the terminal pointer and the plane-wide file together.
