### Fixed

- **A host refused by Claude Code's sandbox proxy now raises the chat's block Notice.** Claude
  Code reports a refused connection as `deny network-outbound host:443 (reason)`, and purlis read
  only the other shape of violation line, so no host it refused ever showed a Notice or an Allow,
  and `purlis doctor` counted none. Now each host refused in one command gets its own block,
  naming the host to allow. A lookup the sandbox refused to a program that resolves its host
  itself (psql's "could not translate host name", curl's "Could not resolve host") and a refused
  Docker socket now raise a Notice too, saying why no grant would help and offering to start the
  chat without the sandbox. The chat is also told, in the turn it was blocked, that the person
  has a Notice, and to say what was blocked and wait instead of working around it (#1631).
