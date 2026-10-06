### Security

- **The leak guard reads a command substitution wherever the shell runs one.** A read of a vault
  or a revealed secret inside a substitution is now refused when the substitution sits in double
  quotes, in a parameter's default, in backticks or in an expanding heredoc body, exactly as the
  same read is refused unquoted. The guard reads quotes, comments, heredoc bodies and `case`
  patterns inside a substitution the way the shell does. A substitution nested more deeply
  than purlis reads (the same limit as the rest of its checks), or one that would take too long
  to read, is refused as too big to check. A test runs every shape through bash and zsh to
  confirm which ones really read (#1412).

### Fixed

- **A hook no longer waits without limit to spool its line.** When the app does not take a hook's
  line and another process holds that chat's spool, the hook gives up after a quarter of a second,
  says on stderr that the line is lost, and answers its harness on time (#1412).
