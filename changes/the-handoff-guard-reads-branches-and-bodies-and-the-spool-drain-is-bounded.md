### Security

- **The handoff check reads the same shapes as the other consent checks.** A handoff inside a
  `case` branch or a shell function the command defines is now refused. So is one inside a
  string or a heredoc that a shell hands to another shell, at any level. The refusal says where
  the handoff sits and asks for it to be run as a command of its own. A test runs every shape
  through bash and zsh to confirm which ones really run the handoff (#1419).

### Fixed

- **A commit or pull request body with code spans is no longer refused for what the spans
  name.** In `git commit -m "$(cat <<'EOF' … EOF)"` and the same form for `gh pr create --body`,
  a quoted heredoc's body is text. The guards read it as lines a shell might run whenever it held
  a `)` and a backtick, so a body that named a gated command inside a function, a `case` or a
  `$( … )` in a code span was refused. Now only a `)` that the oldest bash still in use really
  ends the substitution at counts, which a code span's balanced brackets never are (#1419).

- **The spool drain at the app's start can't get stuck on one chat.** If another process holds a
  chat's spool for more than a second, the drain reports that spool as `held`. It reports one
  larger than 16 MiB as `too-big`. Either way it moves on to the other chats and keeps that
  spool and its key for the next start (#1419).
- **A hook's spool append is bounded all the way through.** The hook waits at most 250 ms for
  the final sync of its line. If the sync is still running then, the hook says the line was
  spooled but isn't durable yet, rather than calling it lost. If a directory sync timed out
  earlier, the next append syncs the directory again. The append now reads only the end of the
  spool, so a long downtime no longer slows each hook. A line that would take the spool past
  what the drain reads is refused, and the hook says it is lost (#1419).
