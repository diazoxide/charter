### Security

- **A chat from an older version is refused less often, and never on a shorter chain.** Before
  refusing a chat whose chain began under an older version and has a closed chat in it, purlis
  reads who dispatched whom from its own dispatch records; where they show the whole chain, as
  deep as the chat's own record says, it is held as a kept chain is. A chain the chats still
  open show longer than the chat's recorded depth is kept whole, and the depth raised to it
  (#1548).

