### Added

- **Dispatch limits, set on Settings › Project › Dispatch.** A project now says how many persona
  chats one chat may have running (6 unless you change it), how many live chats one lineage may
  hold (16), how many dispatches deep a chain may go (3, and never above 8) and how many messages
  a minute one chat may send another (10). A limit of 0 switches dispatch off. The limits are kept
  in the project's committed file, under `[dispatch]`, and a chat cannot change them. The page
  also shows what an administrator's policy caps, as locked (#1439).
