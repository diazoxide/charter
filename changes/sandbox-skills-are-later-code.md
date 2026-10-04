### Security

- **A sandboxed chat can no longer author or edit a project's skills at all.** In every
  harness, a sandboxed chat may never write `.claude/skills/`, `.agents/`, or opencode's
  `tui.json` and `tui.jsonc`, at any depth. These join the other harness config it already may
  not write. Each of them can make a later chat, run outside the sandbox, start code. In a
  sandboxed project, `charter browser install` run from a chat cannot write the skill's pages
  either. It now says so and asks you to run it in your own terminal (#1057).
