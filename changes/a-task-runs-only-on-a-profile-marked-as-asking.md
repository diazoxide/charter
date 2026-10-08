### Security

- **A task starts only on a harness profile marked as asking.** A chat one chat starts for
  another (a task, a handoff, your own "Ask a persona" from a tab, and such a chat started again
  or reopened) now starts only on a profile known to ask a person before its harness acts: the
  built-in `claude` and `codex` profiles, or a profile of the project's local file you marked
  `asks = true`. Anything unmarked is taken not to ask, because purlis cannot read every way a
  harness is told to ask nobody (a settings file, an environment variable, a wrapper script).
  The built-in `opencode` profile does not count, since opencode allows every action unless
  configured otherwise. **To keep dispatching on a profile of your own, add `asks = true` to its
  `[harness.<name>]` table** once its harness asks; the refusal says so. Your own chats are not
  held to it (#1522).
