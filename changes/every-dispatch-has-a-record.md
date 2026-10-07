### Added

- **Every handoff leaves a record, and a Dispatches tab lists them.** purlis now keeps one
  record for each chat a chat starts: who asked, which persona it went to, where it worked,
  the brief, the report, when it started and ended, how often it needed you, and the tokens
  and cost its harness reported. Open **Dispatches** from the palette or from the Sessions
  panel's heading to see the running and past ones, filter them by persona and by asking
  chat, and open a row's chat, or its session record once the chat is closed. A session
  record's tab lists the dispatches its chat made. The records stay on this machine, are
  never committed, and are let go of 30 days after they were last written. A sandboxed chat
  can neither read nor write them. In a project with no sandbox, or a chat started without
  it, only the file's own permissions protect them: every such chat can read and change
  them. **Cost (reported)** is what the chat's harness reported, which a chat can alter; it
  is shown and never enforced, and reads *not reported* for a harness that reports none
  (#1452).
