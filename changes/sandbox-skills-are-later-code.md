### Security

- **A sandboxed chat can no longer write Claude Code's project skills.** `.claude/skills/` joins
  the harness config a sandboxed chat may never write, at any depth, in every harness. A skill
  runs commands when it is invoked, so a chat could otherwise leave code that runs outside the
  sandbox in a later chat. In a sandboxed project, a chat that runs `charter browser install`
  is refused writing the skill's pages too; run it from your own terminal (#1057).
