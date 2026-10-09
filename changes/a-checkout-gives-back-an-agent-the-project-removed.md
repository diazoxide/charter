### Fixed

- **A clone or worktree gives back an agent or skill the project no longer has.** purlis copies
  the project's `.claude/agents/` and `.claude/skills/` into each checkout, but a copy stayed
  after the project's own file was removed, for example by `purlis doctor --fix
  persona-agents`. The next wire of the checkout now removes it, with its line in the
  checkout's `.git/info/exclude`. Only a copy purlis wrote and that is still as it wrote it goes:
  a file you added or edited, one git tracks, one reached through a link, and one whose project
  file purlis could not read all stay (#1583, #1460).
