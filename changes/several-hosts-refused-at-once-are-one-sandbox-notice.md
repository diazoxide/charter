### Fixed

- **Several hosts refused at once are one sandbox Notice.** A chat refused three hosts in one
  command showed only the newest, so allowing it restarted the chat into the next refusal. Now
  the chat's tab shows one Notice listing every host whole, and one Allow allows each of them,
  every host checked as a single one is, with one restart to take them all. A host refused
  while the answer is on its way joins the Notice and stays up, asking. A task's hosts are held
  the same way, so its session's question for several tasks stays answerable host by host.
- **A sandbox block Notice never ends with Dismiss alone.** A refused certificate check, a
  system service, starting a program, or a write whose folder the sandbox did not name now
  offers Start without the sandbox for this chat, saying why nothing can be allowed. Policy
  still has the last word, and says who set it.
- **Docker's "permission denied" no longer reads as a sandbox block on Linux.** There it means a
  user outside the docker group, which no grant or restart mends. On macOS, where the sandbox
  refuses Docker's socket in those words, it is still a block, and a socket refused with
  "operation not permitted" is one everywhere.
