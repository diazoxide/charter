### Added

- **A sandboxed chat can clone a repo and cut a worktree.** In a chat the app started,
  `purlis clone` and `purlis worktree add` now hand the work to the app, which runs the same
  commands and prints what they say. So a clone's git config and hooks,
  and the editor settings a checkout carries (`.vscode/`, `.idea/`, `.claude/settings.json`), are
  written by the app, while the chat's own writes to them stay refused. The app clones only into
  a workspace the chat already works in (any workspace for a chat at the project's top), only
  from a host the chat's sandbox may reach, and credits the worktree to the chat and its persona.
  Git run this way does not read your global or system git config, apart from your name and
  email: an LFS filter, an HTTP proxy or certificate setting and a URL rewrite you keep there do
  not apply to it, so an LFS repository checks out its pointer files, and a clone that needs
  your proxy fails where your terminal's would not. The app also refuses to run git in anything
  but a clone's own repository, or in one whose config names a program for git to run (a
  filter, a diff or merge driver, an include, a helper), and names what it refused. Outside a
  chat, or with no app listening, both commands run as before (#1335).

### Security

- **Git the app runs for a chat can no longer be pointed at a configuration the chat wrote.**
  A sandboxed chat's writes to `.git/commondir`, `.git/objects/info/alternates` and
  `.git/info/attributes`, in any clone, worktree or submodule, are now refused, as its writes
  to `.git/config` and hooks already were (#1335).
