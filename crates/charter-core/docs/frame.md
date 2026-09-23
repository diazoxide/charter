# The frame

**This charter has no tmux frame.** The frame was the earlier Python charter's surface: a
tmux screen that drew a workspace's chats as tabs, with panels around them. charter-app
replaces it with its own window, and nothing in this version starts, reads or checks a tmux
server.

The window is four regions, and anything that has no region is named rather than squeezed
in. Why it is shaped that way, and what each region holds, is
[ADR 0038](https://github.com/diazoxide/charter-app/blob/main/docs/adr/0038-the-window-is-four-regions-and-what-has-no-region-is-named.md).

What the frame's commands did has a place in the app instead:

- **Opening a chat** — the window's new-chat control, on the workspace you choose. The
  `charter claude`, `charter codex` and `charter opencode` terminal launches are not in this
  version's CLI.
- **Which chat needs you** — the window, fed by the hooks each chat's harness runs
  ([hooks.md](hooks.md)). Nothing reads the harness's screen to decide it.
- **The status line** — `charter statusline` still prints the plane's render in a terminal,
  and inside the app it prints an empty line so the harness's own footer stays blank.

The frame's own page, as the last Python release shipped it, is kept for history at
[diazoxide/charter@v0.62.1 docs/frame.md](https://github.com/diazoxide/charter/blob/v0.62.1/docs/frame.md).
It describes a program this app replaces, not this app.
