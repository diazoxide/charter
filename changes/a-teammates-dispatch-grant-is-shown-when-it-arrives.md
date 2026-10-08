### Added

- **A teammate's dispatch grant is shown when it arrives.** When the project's settings come to
  let one persona's chats dispatch to another, after a pull, a branch switch or an edit, purlis
  says so once in the window: "The project now lets steward dispatch to devops." Everything
  that arrived together is one Notice, with **Accept** and **Not on my machine** for all it
  lists and **Decide each in Settings**. "Any persona" is said in its own words. Until you
  answer, nothing listed is in force for you, and a chat that needs one of the pairs meanwhile
  asks on its own tab in the same words; answering either clears both. Not on my machine is
  remembered and shown in Settings › Project › Dispatch, and is not asked again. When the
  project takes a grant away, purlis says so once and asks nothing (#1506).

### Security

- **Your yes to a project grant is bound to what you accepted.** A grant that leaves the
  project's settings is no longer accepted on your machine, so one that is taken out and put
  back waits for a new yes, also when both changes arrive in one pull: purlis keeps the commit
  your acceptances were last checked through and reads the settings' history since. Where
  that history cannot be read, nothing is assumed to have stayed. A late answer to the Notice
  answers only what is still exactly as it was shown, putting the Notice away accepts nothing,
  and a grant naming a persona the project does not define is said to cover nothing and is
  never accepted. Accept and Not on my machine are recorded in purlis's event log, and each is
  a press in the window: no command and nothing a chat sends reaches them (#1506).
