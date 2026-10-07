### Changed

- **A task you start yourself from a chat's tab is yours to steer.** The chat whose tab you
  asked from sees it in `purlis dispatch list`, marked as started by you, and reads its report.
  It cannot wait on it, send it a follow-up, answer it or cancel it, and the task sends that
  chat no note or question: that chat never asked for it and holds no grant for the pair.
- **There is one stop, and one line for it.** *Stop them*, as you close a chat with others at
  work below it, now stops them the way **Stop** does: each gets one short turn to write what
  it did, then ends, and none starts another chat meanwhile. The chat that asked is told in
  the same line (`purlis: the operator stopped …`) whether you pressed Stop, answered *Stop
  them*, or closed the tab of a task that had not reported.
- **Cancel and Stop do not collide.** A chat can cancel a task it dispatched; only you stop any
  other chat. A task you are stopping cannot be cancelled as well, and a cancel under way
  stands down when you stop its task. You can still ask a persona from the tab of a chat you
  are stopping; that chat itself starts nothing.
- **A chat's own list uses the window's words.** `purlis dispatch list` says `waiting on the
  operator` for a task stopped on a permission prompt or a question (#1443).
- **A cancelled task is recorded as cancelled, and a stopped one as stopped**, in the
  Dispatches tab. Neither is shown as failed.
- **A chat you are stopping can send its one last report.** It is still refused a new chat,
  and the command that sends its report now gets through.
- **Claude Code no longer asks before `purlis dispatch wait`, `list`, `cancel`, `tell`, `note`,
  `ask` and `answer`**, each allowed by name. Each reaches only a task the chat itself
  dispatched, or the chat that dispatched it (ADR 0064, amended).
