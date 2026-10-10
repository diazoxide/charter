### Security

- **The app reads a branch's file lists in its bounded reader, and starts no git for them.**
  Opening a file, handing it to your editor, expanding a folder in the explorer, ⌘P and ⌘⇧F
  used to run git in the app's own process to list the files a branch offers, what it ignores
  and its submodules. They now ask the short-lived reader that already reads a branch's changes,
  with its time and memory limits, so a branch an agent built to hang or swell those reads costs
  the reader and not the app. A search you stop still stops at once. The folder watch and
  "Start a chat here" find a branch's folder the same way (#1189).
  A branch whose index the reader cannot read yet, such as one tracking a path of 4,095 bytes
  or more, is still listed as before: for that branch only, the app reads it with its hardened
  git, as it did until now (#1130).
