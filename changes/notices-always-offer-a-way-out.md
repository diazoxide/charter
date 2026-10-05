### Changed

- **A pin to a workspace that is gone is kept dormant, not dropped.** The window used to say
  "Unpin from the palette, or put the workspace back", and the palette had no such row. Now it
  reads "ide is gone, kept dormant", and the pin comes back in its place by itself when the
  workspace does (a pull, another branch). Forget drops it for good, with an Undo that puts it
  back where it was (NO-1, #1223).
- **Every standing line in a project's window has a way out.** A chat that came back as a new
  chat, a resumed or guessed chat, a chat that did not start, a project the last quit could not
  reopen, a project that is gone from your recents, and the window's trouble line can each be
  dismissed now. They were lines you could only read (NO-1, #1223).
