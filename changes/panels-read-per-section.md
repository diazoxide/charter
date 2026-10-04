### Changed

- **A workspace's panels read again only the section a change is part of.** The app keeps the
  focused workspace's repos, todos, memories and session records in memory once it has read
  them. A memory an agent saves now reads that workspace's memories again, and not its todos,
  its session records or its repos. A change the app cannot place still reads everything
  again. A closed todo no longer makes the window check which chats run on old instructions or
  read the curation actions again: only a change to the project's files, the harness settings,
  a persona or a workspace does (FD-10c, #934; FD-10d, #935).
