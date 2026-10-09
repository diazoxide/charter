### Fixed

- **A task no longer stops to ask before reading where it works.** A Claude Code chat purlis
  starts, a task included, now runs `purlis persona where` without asking, as it already ran
  the `persona_where` tool. A task whose brief read its own place stopped on a permission prompt
  in a tab nobody was looking at.

- **A task stopped on a permission prompt is said where you are.** Its session's tab wears the
  hand from the moment the prompt is held, the session's pane names the task and what it asks,
  with **Show the task**, while purlis holds the prompt (up to a minute), and the chat that asked
  for it is told once, at its next turn, that the task waits on you. You answer it in the task's own tab, as before; purlis answers nothing for
  you.
