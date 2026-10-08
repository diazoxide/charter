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

- **A removed persona leaves no dispatch grant behind, and a new one of the same name inherits
  none.** When a persona is removed, your grants that name it are set aside: they allow
  nothing, and Settings shows them greyed with **Remove**. The project's grants for that name
  wait for your yes again. A persona created later under the same name gets one back only if
  you press **Give back**. A never you said keeps holding for the name. Accepting a project
  grant, declining it, and giving a grant back are each recorded in purlis's event log, and
  each is a press in the window: no command and nothing a chat sends reaches them (#1504).
