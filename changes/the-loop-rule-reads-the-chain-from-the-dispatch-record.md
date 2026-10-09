### Security

- **A chain of tasks still holds when a chat in the middle of it has closed.** A dispatch to a
  persona above the asking chat in its chain is refused, and the person's never for a chat
  above holds below it too. Both read the chain from the record purlis keeps of each task when
  it starts it, no longer from the chats that happen to be open, so a chat that finished,
  closed or was cleared is still counted. A finished task reopened as an ordinary chat starts a
  new chain. A chat whose chain began under an older version, which kept no record of it,
  and has a closed chat in it is refused any dispatch to another persona, and is told why; so
  is the person asking from its tab. A chat that closes while its own ask is being decided
  starts nothing (#1521).
