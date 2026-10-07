### Added

- **A workspace or a persona can have dispatch limits of its own.** Settings › Project ›
  Dispatch has a row for the project and one per override, with Add an override; the same limits
  are on a workspace's settings and on a persona's view. The most specific row wins: a persona's
  over a workspace's over the project's. A persona has two more: how many persona chats the
  chats running as it may have running between them, and how many chats may run as it at once,
  both counted across the project. A
  persona's limits are kept in the project's file, never the persona's own. Your own row, Me on
  this machine, can only lower a limit, and an administrator's policy file can cap each one
  (`dispatch` in `/etc/purlis/policy.json`). The last change on a limits table has Undo (#1440).
