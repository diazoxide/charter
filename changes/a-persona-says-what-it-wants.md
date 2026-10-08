### Added

- **A persona says which personas it works with, and one answer allows several.** A persona's
  definition may carry `wants: [devops, qa]`. It grants nothing. When a chat of that persona
  asks to dispatch to another, the question shows, under its answers, "Also let steward
  dispatch to:" with a box for each wanted persona not yet allowed, unticked. Tick the ones
  you want and press an Allow: the asked pair and every ticked one are kept at that level.
  Keep blocked and Never for this pair are about the asked pair only. The question also says
  what the persona you are allowing works with: the names of its vaults and the hosts the
  project declares for it, the first few and how many more. Allowing a dispatch does not
  allow the use of a secret; that is asked as before. `purlis persona lint` says which names
  in a `wants` line are not offered (#1502).

### Security

- **What a persona wants is an offer, never a grant.** A chat can edit its own persona's
  file, so nothing that decides a dispatch reads `wants`: a persona that names another there,
  or `*`, is asked exactly as before, and a chat nobody is at is refused as before. A box is
  unticked until you tick it. Only finished personas of the project are offered, never the
  persona itself or "any persona", and no more than six. What the question says a persona
  works with comes from records no chat writes (the vault registry, your own vault grants and
  the project's file), never from the persona's own file or from what the chat sent, and
  never names a secret. The definition is read again when you answer: a name no longer
  offered is not granted, and if anything the question said has changed since it was shown,
  nothing is allowed and you are shown it again. Each pair allowed is recorded in purlis's
  event log as its own grant (#1502).
