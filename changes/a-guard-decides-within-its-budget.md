### Security

- **Every tool-call guard decides within its own time budget, and refuses when it cannot.** A
  harness runs the tool when a hook times out, on Claude Code and Codex, so a guard made slow by
  a crafted command could let the call through. Every `PreToolUse` hook (Bash, Read, Edit and
  the dispatch ask) now reads its payload and decides within two seconds, well inside every
  harness's timeout, and refuses with one sentence when it runs out: the command is too long or
  too deeply nested, so split it. A payload that does not arrive in time is refused rather than
  judged as empty. Before any guard reads a command, three caps refuse the shapes that made the
  guards slow: more than 128 KB, substitutions nested more than 8 deep (in the command or in
  any string a guard derives from it), or more than 64 wrapper programs in front of one
  program. Commit messages and ASCII pull request bodies at a forge's largest still pass; a
  longer body goes in a file with `--body-file`, as the refusal says. `docs/hooks.md` records
  what each harness does when a hook times out, and that the budget starts when the hook does,
  not when the process does (#1355).
