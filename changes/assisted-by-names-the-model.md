### Changed

- **A Claude Code chat's commits name its model.** Once the harness has said which model its
  session runs on, an agent's commit says `Assisted-by: claude-code:<model>` instead of
  `Assisted-by: claude-code`. Until it has, the trailer names the harness alone, as before
  (#1021).
