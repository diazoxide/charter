### Fixed

- **A prompt too long to send no longer ends an ACP chat.** A prompt over 8 MiB is refused
  before it is sent, saying it is longer than purlis sends to an agent, and the chat goes on;
  it used to end the chat as if the agent had exited (#1117).
