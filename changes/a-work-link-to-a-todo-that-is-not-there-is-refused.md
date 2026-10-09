### Fixed

- **A chat can no longer be linked to a todo that does not exist.** Linking a chat to a
  `todo:` work item now checks that the workspace it names has that todo open, or that a
  promote moved it to an issue. A mistyped or closed todo is refused with a sentence saying so,
  and nothing is written, where before the link was kept and the Work list could never show
  it (#918).
