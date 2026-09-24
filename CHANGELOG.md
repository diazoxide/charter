# Changelog

What each release of charter, the desktop app, brought. About Charter shows the section for the
version you are running, and the same section is that version's GitHub release notes.

The app has its own version line, starting at 0.1.0. It is not the version of the Python
`charter` it was rebuilt from, whose news the `charter news` command still reads.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- When an update is installed, the title bar says **Restart to update**. It restarts charter
  into the new version and offers every chat and view tab back, with **Reopen all** as the
  answer in front and a line saying charter restarted to install an update. A chat that is
  mid-turn is named first, and you choose to restart now or wait. If the restart does not come
  back, the next launch offers the same sessions.
  ([#251](https://github.com/diazoxide/charter-app/issues/251))
- **Project settings**, a tab of its own: right-click a project's tab and choose *Project
  settings…*, or find it in the palette. It has two sections — **Shared**, `charter.toml`,
  which is committed and your team sees, and **Local**, `charter.local.toml`, which stays on
  this machine — each as a form over the keys charter documents and as raw TOML for everything
  else. Saving keeps your comments and the order of your keys, and refuses what charter would
  refuse when it next reads the file, in the same words: a forge it cannot resolve, a profile
  in the committed file, a value that looks like a credential. Local is created on the first
  save, and never where git would commit it. ([#252](https://github.com/diazoxide/charter-app/issues/252))
- An Ignore (✕) on each chat in the needs-you queue takes it out of the queue and out of the red
  counts on its project and workspace tabs at once, without touching the chat. It lasts until
  that chat asks again: its next stop puts it back as a new item. Delete on a focused item does
  the same (Backspace on a Mac), and the palette lists it as "Ignore … until it asks again".
  ([#248](https://github.com/diazoxide/charter-app/issues/248))

### Fixed

- A chat's report that raced a close, or an Ignore, can no longer put the chat back in the
  needs-you queue: every update the window gets is numbered, and it keeps the newest.
  ([#248](https://github.com/diazoxide/charter-app/issues/248))

- On Linux and Windows the app menu no longer takes a key the chat's shell owns: `Ctrl-C` in a
  chat is the interrupt again, not Copy, and the same goes for `Ctrl-A`, `Ctrl-Z`, `Ctrl-Y`,
  `Ctrl-V`, `Ctrl-X` and `Ctrl-H`. Quit is `Ctrl+Shift+Q` there, as in a terminal app. macOS is
  unchanged. ([#241](https://github.com/diazoxide/charter-app/pull/241))
- `charter doctor`'s `git auth` row checks the one-credential git policy, the check
  `charter git-policy` runs, instead of saying it is not checked. It only reads, and names
  `charter git-policy --apply` for a clone that drifted.
  ([#241](https://github.com/diazoxide/charter-app/pull/241))
- Closing a chat that was asking for you, with the × on its tab or by ending its pane, takes it
  out of the needs-you queue and out of the red counts on its project and workspace tabs. It
  used to stay there until some other chat moved.
  ([#247](https://github.com/diazoxide/charter-app/issues/247))

## [0.1.1] - 2026-09-24

0.1.1 brings back what 0.1.0 left out and a working plane still used: vault access through
`charter secret` and `charter persona secret`, and the `persona use`, `list`, `sync-agents` and
`stats` commands. A chat started by charter 0.1.0 finds the app's own `charter` first on its
`PATH`, so a plane whose instructions call those commands lost them. This release restores them.

### Fixed

- `charter persona list`, `persona use`, `persona sync-agents` and `persona stats` work again,
  and answer as the charter your plane was set up with did. Re-syncing a plane's sub-agents
  changes only the ones whose persona changed since they were last generated.
  ([#228](https://github.com/diazoxide/charter-app/pull/228))
- `charter ws todo` says what it recorded, closed or dropped, and a slug that is not there
  says so instead of passing silently. ([#228](https://github.com/diazoxide/charter-app/pull/228))
- `charter secret`, `charter persona secret` and `charter vault` are back. A chat that runs
  `charter secret exec <vault> --file KUBECONFIG=<key> -- kubectl …` or `charter secret list
  <vault>` got a usage error from 0.1.0, which put charter first on the chat's `PATH` without
  them; they now answer as the Python charter did, with the plain-file, reference and 1Password
  providers, and a value still never reaches the chat: `list` prints names, `get` a size band
  and a keyed fingerprint, and `exec` hands values to the command's environment or to 0600 temp
  files it removes, redacting what the command prints.
  ([#227](https://github.com/diazoxide/charter-app/pull/227))
- The Bash guard refuses a vault file read that is wrapped in `charter secret exec … --`, the
  way it refuses one wrapped in `env`. ([#227](https://github.com/diazoxide/charter-app/pull/227))
- The Bash guard no longer mistakes text for a handoff. A multi-line quoted string that
  mentions `charter handoff`, such as a commit message, is read as the text it is, and a real
  `charter handoff` after it is still judged. ([#226](https://github.com/diazoxide/charter-app/pull/226))
- A harness profile that wraps another program (`["ccs", "work"]`) starts as
  `ccs work --plugin-dir …`, with charter's flags after the profile's own words, so a wrapper
  that expects its subcommand first works. A plain `claude` or `codex` profile starts exactly as
  before. ([#226](https://github.com/diazoxide/charter-app/pull/226))
- What `charter docs show` serves, and every message charter prints, name only commands this
  charter has. A page about something it does not do is gone, and a planned command says "not in
  this version yet". ([#226](https://github.com/diazoxide/charter-app/pull/226))

## [0.1.0] - 2026-09-23

### Added

- charter is a desktop app for macOS and Linux. A window holds your projects as tabs, each
  project's workspaces, and each workspace's chats, and a chat is a live terminal running
  Claude Code or Codex.
  ([#14](https://github.com/diazoxide/charter-app/pull/14),
  [#111](https://github.com/diazoxide/charter-app/pull/111),
  [#125](https://github.com/diazoxide/charter-app/pull/125),
  [#131](https://github.com/diazoxide/charter-app/pull/131))
- Opening a project that you have not approved shows what it would run first, and nothing runs
  until you say yes. ([#110](https://github.com/diazoxide/charter-app/pull/110),
  [#121](https://github.com/diazoxide/charter-app/pull/121),
  [#145](https://github.com/diazoxide/charter-app/pull/145))
- A new chat starts on the harness profile and persona you pick, in the workspace or in one of
  its worktrees. ([#32](https://github.com/diazoxide/charter-app/pull/32),
  [#41](https://github.com/diazoxide/charter-app/pull/41))
- Every chat says what it is doing, and the chats waiting for you are listed first and counted
  in the window. ([#26](https://github.com/diazoxide/charter-app/pull/26),
  [#51](https://github.com/diazoxide/charter-app/pull/51),
  [#157](https://github.com/diazoxide/charter-app/pull/157))
- Chats open as tabs and split side by side. The split and close buttons sit on the pane they
  act on, and ending a chat asks first. ([#14](https://github.com/diazoxide/charter-app/pull/14),
  [#176](https://github.com/diazoxide/charter-app/pull/176))
- The window has four regions: a worktree explorer on the left, the chats in the centre, and a
  bottom bar with each repository's branch, changes, worktrees and running pipeline.
  ([#141](https://github.com/diazoxide/charter-app/pull/141),
  [#154](https://github.com/diazoxide/charter-app/pull/154),
  [#156](https://github.com/diazoxide/charter-app/pull/156))
- A worktree can be cut and removed from the window, and a workspace or a project can be made
  and deleted there too. ([#31](https://github.com/diazoxide/charter-app/pull/31),
  [#34](https://github.com/diazoxide/charter-app/pull/34),
  [#172](https://github.com/diazoxide/charter-app/pull/172),
  [#192](https://github.com/diazoxide/charter-app/pull/192))
- A command palette and right-click menus reach every action the bars have.
  ([#45](https://github.com/diazoxide/charter-app/pull/45),
  [#172](https://github.com/diazoxide/charter-app/pull/172),
  [#194](https://github.com/diazoxide/charter-app/pull/194))
- A project, a workspace and a chat can each be pinned.
  ([#143](https://github.com/diazoxide/charter-app/pull/143))
- A status line along the bottom of the window carries the doctor, the alerts drawer for every
  open project, and a note when a project pins an older charter.
  ([#153](https://github.com/diazoxide/charter-app/pull/153),
  [#160](https://github.com/diazoxide/charter-app/pull/160),
  [#162](https://github.com/diazoxide/charter-app/pull/162),
  [#165](https://github.com/diazoxide/charter-app/pull/165))
- Each chat shows its context and cache gauge in its pane's corner, with a bar per turn for its
  usage trend. ([#164](https://github.com/diazoxide/charter-app/pull/164),
  [#167](https://github.com/diazoxide/charter-app/pull/167))
- The personas panel lists each persona with its memories, searchable and loaded a page at a
  time. ([#173](https://github.com/diazoxide/charter-app/pull/173),
  [#206](https://github.com/diazoxide/charter-app/pull/206))
- An extension is a directory you point charter at. Nothing it declares is in force until you
  approve it, and charter asks again when anything in that directory changes.
  ([#150](https://github.com/diazoxide/charter-app/pull/150),
  [#180](https://github.com/diazoxide/charter-app/pull/180))
- A request that belongs in another chat can be handed off from inside the app.
  ([#207](https://github.com/diazoxide/charter-app/pull/207))
- The title bar says which project, workspace and chat you are in, and opens About Charter.
  ([#205](https://github.com/diazoxide/charter-app/pull/205))
- The app updates itself from a stable or a dev channel, and installs only what the release
  key signed. ([#158](https://github.com/diazoxide/charter-app/pull/158))
- The `charter` command ships inside the app, so hooks and the Bash guard answer without a
  Python install. ([#168](https://github.com/diazoxide/charter-app/pull/168),
  [#181](https://github.com/diazoxide/charter-app/pull/181))
- A tab can hold a view, not only a chat. A persona opens as its own tab: what it is for, its
  tools and vault, and its memories, searchable. An approved extension can add a view of its
  own. The first is persona statistics, with charts of each persona's memories, which charter
  asks one question at a time and only when you open it. ([#212](https://github.com/diazoxide/charter-app/pull/212))
- The view tabs you had open come back at the next launch, and wait for a click before an
  extension is asked anything.
  ([#212](https://github.com/diazoxide/charter-app/pull/212))
- The palette can put the app's own `charter` on your terminal's `PATH` on macOS: **Install
  `charter` command in PATH** links `/usr/local/bin/charter` to it, and never replaces a
  `charter` something else put there. ([#219](https://github.com/diazoxide/charter-app/pull/219))
- A chat opens knowing who it is: the persona you picked, what it remembers, and the
  workspace's todos arrive with its first message. The guards on reading a vault, on writing
  into charter's own state, and on sending a sub-agent run in the app's own `charter`.
  ([#220](https://github.com/diazoxide/charter-app/pull/220))

### Changed

- A chat needs nothing installed from the Python charter. The app carries its own Claude Code
  plugin with charter's hooks, its Bash guard and its skills, and loads it into each chat it
  starts, for that chat alone. The chat turns the Python charter's plugin off for itself, and
  finds the app's own `charter` first on its `PATH`. A chat starts offline.
  ([#219](https://github.com/diazoxide/charter-app/pull/219))
- The light and dark themes are data files, and the window and the terminal are both drawn from
  them. ([#144](https://github.com/diazoxide/charter-app/pull/144))
- The terminal follows a theme switch while it is open. The window's layout lives in
  `charter/layout.json`, which you can edit by hand, and it is in place before the first
  frame is drawn. ([#215](https://github.com/diazoxide/charter-app/pull/215))
- About Charter tells this app's own story: its version and what that version brought.
  ([#215](https://github.com/diazoxide/charter-app/pull/215))
- The region toggles sit at the left end of the status line. The context gauge floats over its
  pane rather than taking a row from it, and a very light line divides one tab from the next.
  ([#214](https://github.com/diazoxide/charter-app/pull/214))
- The project, workspace and chat strips nest, and tabs that do not fit collapse into a
  *N more* button instead of scrolling. ([#139](https://github.com/diazoxide/charter-app/pull/139),
  [#171](https://github.com/diazoxide/charter-app/pull/171))
- Closing the window hides it to the tray. Quitting says which chats it will end, and the next
  launch puts back the projects and chats you had open.
  ([#23](https://github.com/diazoxide/charter-app/pull/23),
  [#125](https://github.com/diazoxide/charter-app/pull/125))
- Tab moves through every dialog, and Ctrl-K belongs to the chat that has the keyboard.
  ([#188](https://github.com/diazoxide/charter-app/pull/188),
  [#190](https://github.com/diazoxide/charter-app/pull/190))
- `charter init` adopts the repository it is pointed at instead of turning it into a plane.
  ([#115](https://github.com/diazoxide/charter-app/pull/115),
  [#197](https://github.com/diazoxide/charter-app/pull/197))
- The app, the dock and the menu bar carry charter's own mark.
  ([#159](https://github.com/diazoxide/charter-app/pull/159),
  [#203](https://github.com/diazoxide/charter-app/pull/203))
- A macOS build is ad-hoc signed when no Apple Developer ID is set up. The first install needs
  one command, and the release page says which.
  ([#201](https://github.com/diazoxide/charter-app/pull/201))
- `charter version` prints the app's own version. A plane pinned to a release
  of the Python charter is reported as that older line, not as drift. `charter doctor` and
  every other message stop sending you to the Python charter, and `charter docs show`
  describes this app. ([#219](https://github.com/diazoxide/charter-app/pull/219),
  [#223](https://github.com/diazoxide/charter-app/pull/223))

### Fixed

- A program that starts a screen update and never finishes it no longer freezes the pane.
  ([#7](https://github.com/diazoxide/charter-app/pull/7),
  [#19](https://github.com/diazoxide/charter-app/pull/19))
- A pane opened late catches up on what the chat already printed.
  ([#11](https://github.com/diazoxide/charter-app/pull/11))
- A chat started from an app opened in Finder finds `charter` and its harness.
  ([#135](https://github.com/diazoxide/charter-app/pull/135),
  [#168](https://github.com/diazoxide/charter-app/pull/168))
- An extension's program that crashes is reported with its exit status and its last words,
  not as a lost connection. ([#217](https://github.com/diazoxide/charter-app/pull/217))
- A slow `git` is no longer reported as a broken repository.
  ([#44](https://github.com/diazoxide/charter-app/pull/44))
- No program charter starts can hold a chat's terminal open after the chat ends.
  ([#105](https://github.com/diazoxide/charter-app/pull/105))

[Unreleased]: https://github.com/diazoxide/charter-app/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/diazoxide/charter-app/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/diazoxide/charter-app/releases/tag/v0.1.0
