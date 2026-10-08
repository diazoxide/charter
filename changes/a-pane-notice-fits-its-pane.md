### Changed

- **A refused vault points to the request the chat already made.** When a chat has asked the
  persona a vault is tagged for and that dispatch is waiting for your answer, the vault's Notice
  says so and offers *Show the request* in place of *Dispatch to {persona}…*. The request is the
  first Notice on the pane, with its Allow buttons under its sentence and the brief under them.
  Ask {persona}, opened from a chat's Notice, says that the chat's words are not copied into it
  (#1481).

### Fixed

- **A Notice on a narrow pane is readable, and stays inside its pane.** With two panes side by
  side, a Notice's sentence was squeezed to one word a line, its buttons were broken over
  several lines, and what a button opened was drawn beside the Notice and off the window. The
  sentence now has the row, the buttons go under it when they do not fit beside it, and what a
  button opens is under them at the same width. Several Notices stack and scroll inside the
  pane. A dialog is drawn over a pane's Notices, and the window is no longer scrolled sideways
  by one (#1481).
- **Ask {persona}'s box for what to ask is as wide as the dialog**, the dialog fits a narrow
  window, and its buttons stay at its bottom edge while the form scrolls (#1481).
