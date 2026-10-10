### Fixed

- **The dialogs say what they are doing, and every answer is reachable by Tab.** Closing a chat
  whose chats are still at work asks Keep them running or Stop them with a radio group Tab
  reaches, each answer saying what it does. New branch, New persona, New vault, New workspace,
  New project, Delete workspace, Delete vault, Delete persona, Remove from workspace and Link
  work item say _Creating…_, _Deleting…_, _Removing…_ or _Linking…_ while they run, and New
  project says it is copying your repo. Extensions says when it is reading, says a list it could
  not read, and says _No extensions installed yet_ rather than _None_. Acts name what they act
  on: _Open repo_ on the first run, _Link work item_, _Test sign-in_ and _Store sign-in_. Start a
  chat's labels are in sentence case, New project says _repo_ throughout, Open vault no longer
  points at a + it does not have, a repo filter that matches nothing says so, and the thrown
  kill switch is named by the words it shows (#630).
