### Fixed

- **A chat can dispatch several tasks in one step.** Two `purlis dispatch` calls made at once
  from one chat used to collide: the second was refused with "try again in a few seconds",
  because the app kept one live ticket per chat. Each run of the command now has a ticket of
  its own, on its own connection, spent once and gone in ten seconds either way, so six tasks
  sent together start six chats. A handoff is asked for and opened exactly as before (#1441).
