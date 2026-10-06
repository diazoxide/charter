### Security

- **The consent and operator-rule checks read every command substitution the shell runs.** A
  command that waits for your consent, or that one of your own ask or deny rules names, is now
  refused inside a substitution at any depth: in double quotes, a parameter's default, backticks
  or an expanding heredoc body. They also read a command in a `case` branch, with or without
  the pattern's leading parenthesis, and in a shell function the command defines. A heredoc
  body written to a file or another reader stays data: only its substitutions are read. The
  refusal says where the command sits and asks for it to be run as a command of its own. Both
  checks read substitutions once per command with the same scanner as the leak guard, and a
  substitution nested past purlis's limit is refused as too big to check. A handoff inside a
  substitution is refused too. A test runs every shape through bash and zsh to confirm which
  ones really run the command (#1417).
- **The leak guard reads a shell function the command defines, and a `case` pattern written
  with its leading parenthesis.** A read inside a function that the same command defines and
  calls is refused, including when the call comes after a change of directory or inside a
  substitution (#1417).

### Fixed

- **A hook can't get stuck on what sits at its spool path.** Anything there that is not a plain
  file is refused at once. A new spool's directory sync and the read before the append are
  bounded too, so the hook still answers its harness on time and says the line is lost. When the
  app starts, a spool path that does not open as a plain file (a directory, a link or a pipe) is
  reported as unreadable and every other chat's spool is still drained (#1417).
