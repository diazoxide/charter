### Security

- **A task starts only on a harness profile that asks.** A chat one chat starts for another (a
  task, a handoff, your own "Ask a persona" from a tab, and such a chat started again or
  reopened) now starts only on a profile known to ask a person before its harness acts: the
  built-in `claude` and `codex` profiles, or a profile you mark under **Settings › Project ›
  Harness & profiles › Profiles that ask before they act** (in the local file, `[harness]` then
  `asks = ["work"]`). Anything unmarked is taken not to ask, because purlis cannot read every
  way a harness is told to ask nobody (a settings file, an environment variable, a wrapper
  script). The built-in `opencode` profile needs the mark, since opencode allows every action
  unless configured otherwise, and so does **a profile of your own with a built-in's name**
  (your own `[harness.codex]` table is not the built-in). The refusal names the file and the
  line to add. Your own chats are not held to it (#1522).
