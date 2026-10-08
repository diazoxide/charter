### Added

- **A teammate's dispatch grant is shown when it arrives.** When the project's settings come to
  let one persona's chats dispatch to another, after a pull, a branch switch or an edit, purlis
  says so once in the window: "The project now lets steward dispatch to devops." Everything
  that arrived together is one Notice, with **Accept** for the pairs it lists, **Not on my
  machine** for all it lists, and **Decide each in Settings**. Until you answer, nothing
  listed is in force for you, and a chat that needs one of the pairs meanwhile asks on its own
  tab in the same words; answering either clears both. Not on my machine is remembered and
  shown in Settings › Project › Dispatch, and is not asked again. A project's "any persona"
  is told in its own sentence and is accepted in Settings only. When a commit takes a grant
  you had accepted out of the project's settings, purlis says so once and asks nothing
  (#1506).

### Security

- **Your yes to a project grant is bound to what you accepted.** purlis keeps the commit your
  acceptances were last checked through and reads the history of the project's settings
  since. A grant that a commit took out is no longer accepted on your machine, so one that is
  taken out and put back waits for a new yes, also when both commits arrive in one pull,
  through a merge, or with the settings file renamed; the Notice says why it asks again.
  Where that history cannot be read, every acceptance is asked again. Where it cannot be
  asked at all, no accepted project grant counts until it can. A grant that is only missing
  from the file on disk, as after a branch switch, is not in force and nothing is dropped. A
  history rewritten so that no commit you have took the grant out is not detected. A late
  answer to the Notice answers only what is still exactly as it was shown, putting the Notice
  away accepts nothing, no Notice accepts "any persona", accepting never writes the project's
  file, and a grant naming a persona the project does not define is never accepted. Accept
  and Not on my machine are recorded in purlis's event log, and each is a press in the
  window: no command and nothing a chat sends reaches them (#1506).
