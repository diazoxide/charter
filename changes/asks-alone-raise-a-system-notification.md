### Changed

- **Only what waits on your decision raises a system notification, and a click opens the Inbox
  at it.** A permission prompt, a dispatch grant, a sandbox host, a prompt in a harness's
  terminal and a chat whose turn ended on you each notify, titled by who is asking
  (`steward 12 › #3046 drill`) and saying what kind of ask it is, never a command line or a
  host. A task that failed, a refused commit and other information no longer do. One chat's
  asks a few seconds apart share one notification. Nothing is sent while the Inbox is open in
  the window in front, or the chat is on screen. A click brings purlis forward and opens the
  Inbox at that chat's group (#1694).
