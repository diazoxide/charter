### Added

- **One table of who may dispatch to whom.** Settings › Project › Dispatch now lists every
  persona and, under it, each persona its chats may dispatch to, with every grant that covers
  the pair and where it comes from: one chat, you on this machine, or the project. Each source
  has its own action. **Revoke** takes back a chat's grant or yours. A project grant offers
  **Remove for everyone**, which changes the project's committed file and says so first, and
  **Not on my machine**, which stops following it here and leaves the file to your team. A
  teammate's grant you have not accepted is shown as waiting, with **Accept**. "Any persona"
  is set and cleared here, per persona, and nowhere else. The pairs you said never to are rows
  with **Lift**, and a pair kept blocked for one chat is shown with that chat. Every press
  asks first and says what it will do. Taking a grant back stops new dispatches only: tasks
  already running are left as they are (#1504).

### Security

- **A dispatch grant counts only while both personas exist, and a new persona under an old
  name is asked about once.** A grant that names a persona the project does not have now
  allows nothing: a chat still running as a removed persona dispatches to no one, and nothing
  is dispatched to a name that is no persona. Nothing is moved while a persona is away, so
  switching to a branch without it and back changes no grant. When a persona that purlis saw
  gone is there again with a different definition, or when you create a persona under a name
  your grants still hold, those grants are set aside: Settings shows them greyed, and one
  **Give back** for the name returns them. A never you said keeps holding for the name.
  **What this does not catch:** a persona removed and another made under its name, by hand or
  in one pull, with no dispatch and no look at Settings in between, is not noticed, and the
  new one has the old one's grants. Accepting a project grant, declining it and giving grants
  back are each recorded in purlis's event log, and each is a press in the window: no command
  and nothing a chat sends reaches them (#1504).
