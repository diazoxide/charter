### Added

- **A persona chat that fails, or outlives the chat that asked for it, is handled and never
  lost.** If its program ends before it reported, purlis tells the chat that asked `failed:
  ended without a report`, with the path of its session record where one was written; if you
  close its tab first, that chat is told the operator stopped it. Stopping every
  agent, quitting and restarting a chat report nothing: those chats report when they run
  again. A persona chat stays open after it reports, marked *reported* under the chat that
  asked, and you can still open it and type in it. Closing a chat asks you once about every
  chat at work below it, in the same dialog: *Keep them running*, and a persona chat's report
  goes to that chat's workspace for the next chat to read, or *Stop them*, and they
  are stopped as Stop stops a chat, each after one short turn to write what it did, while the
  chat closes in the same step. The reported persona chats that are at rest close with it,
  each once its session record is written, and the dialog says which. The report of a task
  you started yourself stays with its chat when the tab you asked from has closed, and that
  chat is marked as needing you. A persona chat stopped on a permission prompt or a question
  reads `waiting on the operator` to the chat that asked (#1443).
