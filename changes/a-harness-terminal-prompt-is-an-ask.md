### Added

- **A Codex or opencode permission prompt is also asked in purlis's window.** Codex's
  `PermissionRequest` is now armed on each Codex chat, as Claude Code's is, so its approval
  prompt is an ask you can answer with Allow or Deny for this call; Codex asks once to trust the
  new hook. purlis's opencode plugin hands opencode's permission prompt to the app the same way
  and replies on opencode's own client. From the window an opencode command or edit can only be
  rejected; allow it in the chat. Nothing is allowed unless you choose it, in any permission mode
  (#1691).

### Changed

- **A prompt waiting in a harness's own terminal says what it is.** The ask reads, for example,
  "Waiting in its terminal: a permission prompt", "a form to fill" or "a question", with Go to
  chat. A Claude Code notice that asks nothing, such as a finished sign-in, no longer reads as a
  prompt, and a question opencode asks now counts as one (#1691).
