### Fixed

- **An older reporting handoff is pointed at the persona it named.** A chat still running an
  older `purlis handoff --report` line is told to send a `purlis dispatch` to the persona that
  line named, instead of a placeholder. When the line was also creating a workspace, the reply
  names `purlis workspace create` first (#1471).
