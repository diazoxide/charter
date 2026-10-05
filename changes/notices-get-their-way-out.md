### Changed

- **An AGENTS.md of yours that charter's exclude line hides has Open file and Move aside….**
  The note on a chat's pane used to tell you to commit it or move it aside, with nothing to
  press. Open file shows it in your editor. Move aside… asks first, then renames it to
  `AGENTS.aside.md` (or the next free name), never over another file, so `git status` shows it
  again (NO-4, #1231).
- **A slow start on Linux offers Copy command** for the relaunch without the session bus it
  suggests, so you don't retype the environment variable (NO-4, #1231).
- **A refused read in the explorer has Read again.** A workspace that could not be read, a
  repo charter will not read, and a repo whose branches could not be listed each offer Read
  again, and the line goes once a read works. The bottom bar shows the same lines and stays
  read-only (NO-4, #1231).

### Fixed

- **A refusal no longer outlasts its cause.** The window's line about a picker or a shell that
  could not open stayed after the next try worked, and a workspace read that failed once kept
  its line after a later read succeeded. Each now clears when the same action succeeds
  (NO-4, #1231).
