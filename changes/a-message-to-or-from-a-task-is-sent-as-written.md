### Changed

- **A task's report and the messages between a chat and its tasks are sent as written.** A
  call that runs `purlis dispatch report`, `tell`, `answer`, `note` or `ask` beside a live
  `$(…)`, a backtick or a process substitution is refused, on every harness, as the handoff's
  report already was: the shell would have replaced that text before purlis read it. The
  refusal says to write the text out in plain words (#1456, #1463).
