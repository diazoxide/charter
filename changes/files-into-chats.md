### Added

- **Hand a file to a chat without typing its path.** Drag a file or folder from the explorer or a
  file tab, a search hit, or the lines you selected in the preview onto a chat's tab or its pane,
  and charter types a reference to it into the chat in that harness's own words: `@src/main.rs#L10-20`
  in Claude Code, `src/main.rs:10-20` in Codex, `@src/main.rs#10-20` in opencode. It is typed and
  never sent, so you add your own words before pressing Enter. The preview has _Ask a chat about
  this_ and _Add to a chat's context_, which pick a chat, and every file and folder row has
  _Start a chat here_, which opens a chat on that branch with the reference already typed. A chat
  that is mid-turn, asking you something, or not heard from yet is never typed into: the
  reference goes on the clipboard instead, and charter says why. A file whose name begins with
  `!` or another character a harness reads as a command is written `./!…`, and a name holding an
  invisible or direction-changing character is refused. opencode cannot be typed into yet, so it always gets
  the reference on the clipboard, with a note (FM-9, #1112).
