### Security

- **Every consent check reads every level of a string a shell runs.** A command that waits for
  your consent, or one your own ask and deny rules name, was refused inside a string a shell
  runs (`bash -c '…'`, `eval '…'`, a heredoc fed to a shell) only one level in. A string that
  handed it on to another shell got past both checks. They now read every level, through the
  same walk the handoff check uses. The walk also reads a string handed to a shell from inside
  a substitution, a function or a `case` branch, and reads a `\$` or an escaped backtick the
  way the shell does, in double quotes and in a heredoc the shell expands. A string handed on
  deeper than the checks read is refused as too big to check, whatever it holds. A test runs
  every shape through bash and zsh to confirm which ones really run the command (#1426).
- **The same checks read a script a shell is handed some other way.** A script piped into a
  shell, given to it as a here-string or a process substitution, or sourced from stdin, and
  the command a `find -exec` runs, are read as the commands they are. Before, only a `-c`
  string, an `eval` and a heredoc were (#1426).

### Fixed

- **Every wait on the hook spool's keys is bounded.** Issuing a chat's token and the spool drain
  at the app's start each wait at most a second for another process to let go of the spool's
  keys. The drain also waits at most a second for each emptied spool to be made durable (#1426).
