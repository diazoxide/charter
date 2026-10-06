### Security

- **Every chat is of the project it was opened in.** A chat that is not on a profile now takes
  its project, and so its sandbox, from the open project, as a chat on a profile does, and no
  longer from the folder it stands in. Its hooks are told that project too. In a project that
  runs chats sandboxed, such a chat in a folder that is a link, has a link above it, or is
  outside the project is refused, and is never started without the sandbox instead; the refusal
  says how to move on. The environment a chat is given and the vault names stripped from it are
  read from the open project too. A project whose own manifest has gone no longer reads as one
  that leaves the sandbox off: its chats are refused until it is restored. "Start without the
  sandbox" is still yours to pick for one chat (#1410).
