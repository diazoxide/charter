# Changelog

What each release of charter, the desktop app, brought. About Charter shows the section for the
version you are running, and the same section is that version's GitHub release notes.

The app has its own version line, starting at 0.1.0. It is not the version of the Python
`charter` it was rebuilt from. `charter news` prints this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`charter save --pull` brings in what the remote has before it saves.** A chat the app did
  not start, such as a `claude` or `codex` in a terminal, gets no auto-save and no incoming
  changes. Outside the app, the plane is saved only through `charter save`. `--pull` fetches
  the plane's target branch and fast-forwards a clean tree first, the same way the app does.
  With unsaved work in the tree, what came in is left alone and the save still runs. If the
  tree has conflicts, or there is no remote to pull from, the command stops and saves nothing.
  ([#375](https://github.com/diazoxide/charter/issues/375))
- **A chat's tab says when the plane's instructions changed after it started.** A running chat
  keeps the `CLAUDE.md`, harness settings, sub-agents and persona charter it read when it
  started. When one of them changes on disk, its tab gets a quiet mark naming the files, so you
  know it will run on the old ones until you start it fresh. It is not a needs-you item.
  ([#369](https://github.com/diazoxide/charter/issues/369))
- **The commitment gate is back.** When a prompt asks for work and leaves a real choice open
  (open-ended wording, a broad scope, something irreversible, a long many-part ask), the chat
  is told to look first and then ask you at that choice before it builds. It stays quiet for
  questions, for work with nothing to ask about, for slash commands and for unattended runs,
  and for the three prompts after it fires.
  ([#369](https://github.com/diazoxide/charter/issues/369))
- **`charter plugin install` guards the `claude` and `codex` chats you start in a terminal.**
  The app arms only the chats it starts, so a terminal chat ran charter's guard only if the
  retired Python charter's plugin happened to still be installed. `charter plugin install`
  registers the app's own plugin with Claude Code, and charter's Bash guard with Codex, for
  every chat on this machine. It prints each change, `--dry-run` shows them without writing,
  and a second run changes nothing. It never turns on the retired `charter@charter` plugin,
  and turns it off where it writes. `charter plugin uninstall` takes it back.
  ([#374](https://github.com/diazoxide/charter/issues/374))

### Changed

- **A vault file that is a symlink is refused.** `charter secret set` and `charter secret rm`
  on a plain-file or reference vault whose file is a link now stop with a message saying so,
  and write nothing. They used to write the secrets to wherever the link pointed. Point the
  vault's `file` at the real path instead.
  ([#429](https://github.com/diazoxide/charter/issues/429))
- **`charter news` prints this changelog.** It shows every version of the app, newest first,
  and `charter news --for <version>` shows one, the same notes as the release page and About
  Charter. It used to read the Python charter's news and told every plane it had no update
  baseline. `--since`, `--until` and `--pending` are retired and say what to run instead.
  `charter update` points at `charter news`, and the pin dialog no longer has an empty news
  list.
- **Pinned workspaces stay in the order you pinned them.** The workspace strip draws them in
  that order, and a workspace you pin later goes after the others. Unpinning one leaves the
  rest where they were. Pins from an earlier version keep the order they had.
  ([#402](https://github.com/diazoxide/charter/issues/402))
- **The project strip's show-more menu lists the most recently active projects first**, after
  the ones that need you. It used to list them in the strip's order.
  ([#401](https://github.com/diazoxide/charter/issues/401))
- **The title bar shows incoming commits as their own `↓N`.** It sits after the save
  indicator's words and is never cut off when the words are. A long stage is still cut short
  in the bar and read in full from its tooltip or the Saving view. The window can no longer be
  made narrower than 1024 px; at that width the title bar keeps room for two project tabs.
  ([#403](https://github.com/diazoxide/charter/issues/403))
- **`routing:` in a persona is retired.** Personas are offered to the harness as sub-agents,
  which is where work is routed. A persona that still declares `routing:` loads as before,
  `charter doctor` says the key is ignored, and `charter init` no longer writes it.
  ([#369](https://github.com/diazoxide/charter/issues/369))

### Fixed

- **A terminal pane that opens late no longer adds a line when a wide character sits in the
  last column.** If a program had pushed a wide character, such as a CJK character, into the
  last column with wrapping turned off, the catch-up redraw printed that character again. That
  wrapped it onto the next row, and on the bottom row it scrolled the pane by one line. The
  pane now draws the blank that the terminal holds there.
  ([#435](https://github.com/diazoxide/charter/issues/435))
- **The extension tests no longer fail on a busy machine.** Extensions still get the same time
  as before: 5 seconds for a view, an action, an event or a command, and at a chat's start 2
  seconds each and 3 seconds for all of them together. A debug build of `charter` now lets the
  test suite set a different limit, so a test that is not about the limit gives a slow machine
  room, and a test that is about it uses a short limit and a program that never answers.
  ([#422](https://github.com/diazoxide/charter/issues/422))

- **The dispatch log, the session trace and a memory index refuse to write through a link.**
  They now open the file without following a link, and refuse it when it is one.
  ([#420](https://github.com/diazoxide/charter/issues/420))
- **A vault is never left half-written.** Setting or removing a secret in a plain-file or
  reference vault now writes a new file beside it and swaps it in, instead of rewriting the
  vault in place, so a crash or a full disk mid-write leaves the old vault whole. The file is
  still private to you (0600) from the moment it exists.
  ([#429](https://github.com/diazoxide/charter/issues/429))
- **Every file charter replaces whole is written the same careful way.** `workspace.json`, the
  settings charter generates for a workspace or a checkout, the profile approval record and
  the hook bookkeeping now share one writer. Each is flushed to disk with its directory, keeps
  the permissions it had (or stays private, for charter's own state), and is never replaced
  when it is a symlink. A `workspace.json` or generated settings file you made read-only is
  now left alone and reported, where it used to be replaced.
  ([#430](https://github.com/diazoxide/charter/issues/430))
- **More of charter's files are replaced whole and never written through a symlink.** The
  vault registry (both halves), the fingerprint key, memory files and a memory index's
  rewrite, the front-door persona charter scaffolds, a checkout's presence record, the
  remembered open chats (`reopen.json`), the app's machine store, extension record and window
  layout, and charter's own state files (the active persona and workspace, MCP approvals, the
  forge cache and its lock) now go through the same writer: a new file beside the old one,
  flushed and swapped in. A crash mid-write leaves the old file whole, and a file that is a
  symlink is refused, where some of these used to write to wherever the link pointed and
  others replaced the link. What else changes:
  - The local vault registry and the fingerprint key must be private (0600). On a filesystem
    that cannot hold that mode, the write is now refused instead of made at a looser mode.
  - The shared vault registry keeps the permissions it has, where it used to be reset to 0644
    on every write. A new one gets your usual file permissions.
  - `reopen.json` is now private to you (0600). It used to get your usual file permissions.
    So is a memory index under `.charter/` when charter removes a line from it.
  - Charter's own state files that you made read-only are replaced, as charter owns their
    mode. A read-only shared vault registry, local registry or fingerprint key is refused.
  - A `.gitkeep` that is a symlink stops `charter init`'s front-door persona with an error.
  ([#434](https://github.com/diazoxide/charter/issues/434))
- **`charter doctor` knows the Python charter is retired.** Its `python3` row no longer warns.
  Its three plugin rows, which said "not checked", now check charter's plugin for chats started
  outside the app: whether it is installed, whether it is current, and whether the `charter` its
  hooks run still exists. A new `superseded plugin` row names every settings file that still
  turns on the retired `charter@charter`. `charter doctor --fix` works again: it runs
  `charter plugin install`, prints each change, then reports. A workspace or worktree layer no
  longer copies `charter@charter` from the plane's settings.
  ([#373](https://github.com/diazoxide/charter/issues/373))
- **A guard that crashes now refuses the tool call instead of letting it run.** If charter hit
  an internal error while checking a tool call, the crash ended the process with a status
  Claude Code and Codex read as a non-blocking error, so the call went ahead unchecked. Any
  crash in a `PreToolUse` hook now exits 2, which both read as "block", with one line on
  stderr saying the guard could not answer. A crash in any other hook still never blocks.
  ([#349](https://github.com/diazoxide/charter/issues/349))
- **A handoff leaves a todo in the workspace it went to.** Once the app has opened the new
  chat, `charter handoff` records a todo there: the brief's first line and which chat handed it
  off, never the rest of the brief. If a todo about the same work is already open there, it
  says so and records nothing twice. It also adds one row to the dispatch log saying whether
  the chat went to this workspace or another, and whether the handoff created it; the row
  names no workspace, persona or brief. A write that fails is said and never undoes the open.
  ([#372](https://github.com/diazoxide/charter/issues/372))
- **`charter doctor` quotes a value it read from a file, a folder name or the environment on
  one line.** The plane root row's memory-push record, the session layer row's harness name,
  and the workspace and persona names in the clone and memory rows used to be printed as they
  were, so a value with a line break in it could print a row that looked like one of doctor's
  own. ([#353](https://github.com/diazoxide/charter/issues/353))
- **`charter.toml` and the plane's `.gitignore` can no longer be left cut short.** charter
  now writes the new version beside the file and swaps it in, so a crash or a full disk leaves
  the old file whole. Two edits at once, such as `charter persona default` while the settings
  tab saves, or two workspaces made live together, now both land instead of one overwriting
  the other. A `.gitignore` charter cannot read as text is now left alone rather than
  rewritten from nothing. ([#357](https://github.com/diazoxide/charter/issues/357),
  [#358](https://github.com/diazoxide/charter/issues/358))
- **`charter init` creates `workspaces/.gitkeep`**, the file its `.gitignore` already
  expected, so an empty `workspaces/` can be committed. `charter reinit` adds it to older
  planes. ([#355](https://github.com/diazoxide/charter/issues/355))
- **A reference vault's file is private from the moment it is created.** charter used to
  write it first and restrict its permissions afterwards; it now sets them before any content,
  as plain-file vaults already did.
  ([#356](https://github.com/diazoxide/charter/issues/356))
- **`charter doctor` describes Codex correctly.** Its session layer row said Codex ignores a
  project `.codex/config.toml` and gets charter's layer from a plugin. The app arms Codex with
  flags on each chat's command line, and Codex reads a project's `.codex/config.toml` once you
  trust the project. The harness guide says the same.
  ([#354](https://github.com/diazoxide/charter/issues/354))
- **The plane root's branch guards can no longer be walked past by spelling.** A branch switch
  or a commit-destroying `git reset` in the plane root was let through when `git` was typed in
  capitals (`GIT`, which runs git on macOS and Windows) or with quotes inside it (`g''it`), when
  the root was named with different letter case or through `/System/Volumes/Data`, when the
  command ran from a folder inside the root such as `docs/`, when `env -C` or `sudo --chdir`
  moved it there, or when an alias was defined in one case and used in another. All of these
  are refused now. ([#346](https://github.com/diazoxide/charter/issues/346))
- **A `cd` that fails no longer hides the command after it.** `cd somewhere; git checkout x`
  was judged as running in `somewhere` even when the `cd` failed and git ran in the plane root.
  Now only `cd somewhere && …` counts as having moved, and only up to the end of that `&&`
  chain. A `cd` in a pipeline or a subshell, `pushd`, `~`, and a destination charter can't read
  (`cd "$DIR"`, `cd -`) are followed the way the shell follows them.
  ([#345](https://github.com/diazoxide/charter/issues/345))
- **`GH issue create` and `CHARTER persona remember` are checked like their lower-case
  spellings.** The check that refuses a live `` `…` `` or `$(…)` in a forge body or a charter
  memory skipped a program name typed in capitals or split by quotes.
  ([#347](https://github.com/diazoxide/charter/issues/347))
- **An unattended run can no longer publish just because it listed the tags first.** Listing
  or deleting local tags in the same command as a release, a tag, a tag push or a merge no
  longer lets that command past the release floor.
  ([#348](https://github.com/diazoxide/charter/issues/348))
- **The secret-leak guard reads search options the way the search tools do.** A `--glob` that
  selects files is no longer taken for one that excludes them. A program or pattern read from a
  file with `-f`/`--file` is checked like any other file the command opens. A search's pattern
  and file options are read however they are spelled: with `=`, bundled together, shortened, or
  after `--`. `rg`'s, `grep`'s and `ag`'s other options that take a value are read too.
  ([#350](https://github.com/diazoxide/charter/issues/350),
  [#351](https://github.com/diazoxide/charter/issues/351))

## [0.3.0] - 2026-09-25

0.3.0 is about extensions you can act through and workspaces that carry their repos. An
extension can now add commands to `charter`, palette entries and row actions, hear what happens
and add to a chat's briefing, and show badges and repo columns, each capability named in the
approval dialog; persona statistics ships built in. A workspace picks its repos when you make it
and saves each one by its own mode, and a project says what is not saved yet and carries its
commits on. It is also the first release from the repository's new name, `diazoxide/charter`,
and charter's own plugin is now called `charter`.

### Added

- **Pick a workspace's repos when you make it, and change them later.** The new-workspace
  dialog lists the repos your own `gh` or `glab` login can reach under the plane's forges,
  private ones included, and clones the ones you tick into the workspace after it is made. Each
  repo clones on its own, so you can start a chat while they land; one that fails says why and
  can be tried again. A workspace's settings have a Repos section with the same list: tick to
  clone, untick to remove. A repo with uncommitted or unpushed work, or a worktree, is never
  removed from there. If you're not logged in to a forge, the dialog says so and you can still
  make the workspace. (ADR 0055)
- **Extensions can add commands to `charter`.** An extension that asks for the `cli`
  capability runs as `charter <its id> <command> …`, from a terminal, a script or a chat. What
  its program prints and its exit status come back unchanged. Each command says whether it
  writes; the approval dialog lists the ones that do, and they are held to the plane paths the
  extension declares, with anything else they change reported. A chat's call goes through the
  same guard as any `charter` call, and a persona's grant never lets one that writes run
  without asking. An extension turned off, not yet approved or changed since you approved it
  says so and runs nothing. An extension can never take one of charter's own words as its id.
  `charter <id>` alone lists its commands. The command line doesn't reach the app's built-in
  extensions yet. ([#342](https://github.com/diazoxide/charter/issues/342))
- **Extensions can hear what happens, and add to a chat's briefing.** An extension that asks
  for the `events` capability is told when a workspace is focused, created, forked or removed,
  when a handoff is made, when a chat starts and when the plane is saved. It is told after the
  thing is done, so a slow or broken extension never holds it up or changes how it went. It
  shows as a note naming the extension instead. A fork copies the folder an extension keeps in
  each workspace, even while the extension is off. One that asks for `briefing` adds a section
  to every chat's first message. The section is quoted under its name as data, not
  instructions, is cut at 1,500 characters, and is left out if it holds text that can't be
  drawn. The approval dialog says it "adds text to every chat's first message". A chat's start
  waits at most three seconds for all extensions together. Both need protocol 2.
  ([#343](https://github.com/diazoxide/charter/issues/343))
- **An extension can be acted on, not only read.** Three capabilities, each named in the
  approval dialog: `palette` adds commands to the palette, named with the extension's name, that
  open one of its views or run one of its actions; `actions` puts the extension's own actions on
  the rows of its views, and the answer can refresh the view; `writes` declares the plane paths
  it writes, such as `workspaces/*/todos/`. charter asks before an action when the extension
  says to, and always before one that deletes. Each request tells the extension where it may
  write, and after each one charter says what changed outside those paths, naming the
  extension. That is a report, not a fence: an extension still runs as you. The protocol is now
  2; an extension written for protocol 1 is asked exactly as before and keeps its approval.
  ([#341](https://github.com/diazoxide/charter/issues/341))
- **Workspace repos are saved too, each by its own mode.** The Saving tab has a row for every
  repo in the workspace: its stage, the branch it is on, its pull request and its own Save
  button, with *Save all* for the project and every repo at once. The title bar counts the
  workspace's repos in: *1 repo changed*. A repo is saved by `[repos.<name>] mode`, `pr` by
  default. A save commits on the branch the repo is on. `push` pushes that branch. `pr` pushes it
  and opens or updates a pull request into `branch`, the repo's default branch by default.
  `pr-merge` also asks for auto-merge, and says so when the forge won't queue it. On the default
  branch itself, a PR mode pushes a new `charter/<workspace>/<short-sha>` branch instead, so it
  never pushes to the default branch. A repo is saved only between turns. A save you press while
  a chat in that workspace is working is refused, with a sentence naming the chat. Auto-save
  skips that round. Repos are saved by themselves only when `[repos.<name>] autosave = true`,
  which is off by default, and quitting saves only those, and not one whose chat's turn the
  quit cut off. A pull request you opened yourself from the branch is never rewritten or set to
  merge. A repo save refuses a secret-shaped file (`.env`, a private key, `credentials.json`, a
  `.npmrc` with a token, …) or a private key or forge token in what it would commit, and names
  the file.
  ([#299](https://github.com/diazoxide/charter/issues/299))
- **Persona statistics comes with the app.** charter now ships its own extensions, and persona
  statistics is the first: there is no folder to assemble and add by hand, and no approval to
  give, because the app's signature covers it. The Extensions list marks it "built-in" and
  offers turning it off on this machine in the place of Remove. A project or a workspace can
  still turn it off, as it can any extension. A copy of it anywhere else is an ordinary
  extension that has to be approved. If you added it by hand before, that copy is set aside and
  the built-in one is used. Its numbers are now `charter persona stats`'s: it counts the same
  memories and dates them the same way, and "recent" means the last 14 days in both. For
  extension authors: a view about personas is now handed the day each memory was written,
  rather than its minute. ([#339](https://github.com/diazoxide/charter/issues/339))
- **Extensions can show badges and repo columns.** An extension that asks for the `badges`
  capability can show values in the status bar and in `charter statusline`'s footer, and one
  that asks for `repo-columns` can add columns to the repo table in the bottom bar. It declares
  each one in its manifest, with how long a value stays fresh, and the approval dialog lists
  them. The values come from a facts file the extension keeps in its state directory, and
  charter never starts the extension's program to draw them. A value older than its freshness
  is dimmed and shows its age. A facts file that is too big, isn't JSON, or fills something the
  manifest didn't declare shows nothing and says why. So does an extension that changed since
  you approved it. Turning an extension off for a project or a workspace hides its badges and
  columns there. ([#340](https://github.com/diazoxide/charter/issues/340))
- **Every open project says whether it has unsaved work.** A dot on a project's tab marks work
  a save would take, or a save that is blocked (red), so a project behind the one in front is
  not where work is forgotten. Each project keeps its own save state and its own auto-save,
  and quitting saves every one of them.
  ([#302](https://github.com/diazoxide/charter/issues/302))
- **An extension says which capabilities it asks for.** An extension's `charter-extension.json`
  can list them in `capabilities`. The approval dialog and the Extensions list name each one,
  and changing the list asks you again. An extension that asks for a capability this charter
  doesn't know is refused as a whole, with a sentence naming it. Each capability arrives in
  its own change. An extension with no `capabilities` loads exactly as
  before and keeps its approval. `version` in the manifest is now the protocol its program
  speaks. ([#338](https://github.com/diazoxide/charter/issues/338))
- **LIVE and LOCAL, from the window.** A LIVE workspace, whose charter, memory and todos are
  published with the project, is marked on its tab, in the title bar and in the Explorer, and
  the Saving tab names the live ones. Its menu, the palette and its settings page offer
  *Make live…* or *Make local…*. Before anything changes, a confirmation says which files and
  where they go (the remote, or "this machine only"). The project is saved at once. Making a
  workspace LOCAL stops publishing its files and keeps them on disk; what was already pushed
  stays in history, and the confirmation says so. The new-workspace dialog has a *Live* box,
  unticked by default. ([#301](https://github.com/diazoxide/charter/issues/301))
- **A blocked save shows its way out.** When a save can't go further (a conflict with the
  remote, a secret the scan caught, a pull request mode on a remote charter can't open pull
  requests on), the Saving tab says why, names the files a conflict is in, and offers
  *Resolve in a chat* (the chat picker, starting in the project) or *Open terminal here* (a plain
  shell in the project). The alerts drawer says so too: at once for a secret, and after ten
  minutes for anything else.
- **Fewer conflicts in the first place.** `charter init` and `charter reinit` write a
  `.gitattributes` block that merges the logs and memory indexes which only ever grow line by
  line, so two machines adding to the same one no longer conflict.
  ([#295](https://github.com/diazoxide/charter/issues/295))
- **Saving through a pull request.** A project whose `[plane] mode` is `pr` or `pr-merge` now
  saves the whole way. Each save commits on the project's branch, pushes it to this machine's
  own save branch (`[plane] save_branch`, `charter/save/<this machine>-<this clone>` unless
  you name one),
  and opens one pull request from there into `[plane] branch`. The next save updates that same
  pull request. `pr-merge` also asks GitHub or GitLab to merge it once its checks pass. If the
  forge will not queue the merge, the Saving tab says why and the pull request stays open for
  you. The Saving tab shows *Pushed — waiting on its pull request* with the link. Once the pull
  request has merged, by a merge commit, a rebase or a squash, charter moves your branch onto
  the remote's and keeps anything newer you have not saved. If the pull request was closed
  without merging, or the branch no longer holds what was pushed, the project is **blocked**
  and nothing is moved; save again to open a new pull request. A file in the way of the move
  just waits for the next look. Charter force-pushes only its own save branch, and only over
  what that clone pushed there itself, so a second machine with the same name never
  overwrites the first's. It never pushes to the project's branch in these modes. ([#298](https://github.com/diazoxide/charter/issues/298))
- **Commits left behind are carried on.** In a project whose mode pushes, a save with nothing
  new to commit still pushes the commits this machine has not pushed yet. That covers a push
  that quitting did not have time for, and a commit a chat made with plain git.
- **Auto-save.** While charter is open, a project with auto-save on (`[plane] autosave`,
  on by default) saves by itself: 30 seconds after the last change (`autosave_after`), as soon
  as a chat in it ends, and when you quit. At quit it commits at once and gives the push a few
  seconds; whatever did not get pushed is pushed the next time charter opens the project. It
  pauses while a save is blocked, and a push that fails is retried every five minutes, not
  every 30 seconds.
- **What came in.** Every five minutes, and when the window comes back into focus, charter
  fetches the project's branch. The title bar and the Saving tab say how many commits came in
  (*2 incoming*). With auto-save on, a project with nothing unsaved is fast-forwarded onto
  them. Otherwise they wait for your next save.
- **One question per project.** A project that has never said how it is saved (no
  `[plane] mode`, including every project whose `charter.toml` says `share = "local"`) is asked
  in the Saving tab: *Push*, *Commit only* or *Off*. The answer is written into `charter.toml`,
  and until there is one, nothing saves the project by itself.
  ([#296](https://github.com/diazoxide/charter/issues/296))
- **The title bar says what is not saved yet.** Beside the needs-you button, the project in
  front shows where its unsaved work sits: *3 changed*, *committed, not pushed*,
  *waiting on its pull request*, *blocked*, or *Saved*. A save button sits next to it while
  there is anything to save. Press the words to open the project's **Saving** tab, which lists
  the files the next save takes, lets you type a message (leave it empty and charter writes one
  that says what changed), and shows the last 50 saves and how each one ended. The tab is also on
  the project tab's menu and in the palette, as *Saving…*. The button runs the same save as
  `charter save`, so both follow `[plane] mode`.
  ([#294](https://github.com/diazoxide/charter/issues/294))
- **Project settings has Plane and Repos sections.** Both files, Shared (`charter.toml`) and
  Local (`charter.local.toml`), now have a **Plane** group — mode, target branch, save branch,
  signing, auto-save and how long auto-save waits — and a **Repos** group with the same keys
  (bar the save branch) for every repo in `inventory/repos.json`. Beside each control is what
  the project actually uses and which file decided it, and a Shared value that Local overrides
  says so. A value charter would not read is refused on save, in the words `charter doctor`
  uses. The old `[memory] share` choice moved into the Shared Plane group, marked as the
  deprecated stand-in for Mode, and it says whether it is in force or a Mode set in either
  file wins. The rest of the old Plane group, `[plane] worktrees` included, is now called
  General.
  ([#300](https://github.com/diazoxide/charter/issues/300))

### Changed

- **charter lives at `diazoxide/charter`.** The repository that was `diazoxide/charter-app` took
  the name, and the plane that held it before is `diazoxide/charter-plane`. Updates, releases and
  issues come from the new name; a build from before reaches them through GitHub's redirect.
  (ADR 0056)
- **charter's own plugin is called `charter`.** Its skills are `charter:handoff`,
  `charter:update` and `charter:working-in-a-clone`, and a chat loads it as `charter@inline`.
  It was `charter-app`. A persona whose `skills:` lists a `charter-app:` skill needs it
  renamed, and `charter persona sync-agents` carries that into `.claude/agents/`. A project or
  workspace setting that still turns `charter-app@inline` on is refused with the new id, and
  the Python charter's `charter@charter` is still turned off in every chat. Turning that one off
  never turns charter's own off. (#406, ADR 0056)
- **Save in the title bar now saves only the project.** Before, when the workspace in front had
  repos with changes, the title bar's Save became Save all. It committed every changed file in
  those repos and pushed their branches, without asking. Now the title bar counts the repos but
  never saves them. Save all lives only in the Saving tab, and it first asks you to confirm a
  list of each repo, its branch, what it would take and where its save goes. Each repo row says
  where its Save goes, too. A repo nobody has configured is now `off` instead of `pr`: charter
  saves no repo until `[repos.<name>] mode` says how. To keep saving a repo as before, set its
  mode to `pr`.
- **The project tabs are in the title bar**, after the window controls, and the breadcrumb
  is gone: the project tab says which project and the workspace strip says which workspace.
  That is one tab row fewer above the panes. The tabs give way before About, the update
  button and the ✋ menu do, and a stretch of the bar is always left free to drag the window
  by. How many chats are running is now on the status line.
  ([#394](https://github.com/diazoxide/charter/issues/394))
- **The right sidebar's sections are easier to tell apart.** A line now separates Todos,
  Personas, Vaults and every panel an extension adds. Each heading is a smaller, bolder title
  in brighter text, so it no longer looks like the first row of its list. The left sidebar's
  "Not cloned here" heading matches.
- **`charter discover` adds to the inventory instead of replacing it.** Engineers on one plane
  reach different repos, and each run used to drop every repo the last person's login could see
  and theirs could not. A repo now leaves `inventory/repos.json` only when `[[forge]].exclude`
  names it. (ADR 0055)
- The alerts drawer no longer repeats what the title bar's save indicator already says about
  the plane: a plane-root alert there now names only a detached HEAD or a branch other than
  the default. Its remedy, in the drawer and on the terminal status line, now reads "save the
  plane, or move the work to a workspace clone".
  ([#332](https://github.com/diazoxide/charter/issues/332))
- `charter save` follows `[plane] mode`. `off` commits nothing, `commit` stops after the
  commit, and `push` pushes to `[plane] branch` when one is set. Until charter can open the
  pull request, `pr` and `pr-merge` commit but never push to the target branch. A plane that
  names no mode is saved exactly as before.
- A save with no message says what changed in the plane's own words, for example
  `charter save: 3 files (steward memory 2, ide todos 1)`, instead of only counting files.
- `[plane] sign = true`, or `--sign`, now signs the save whatever the machine's own
  `commit.gpgsign` says. Before, `--sign` only allowed signing. A signer that fails still
  leaves an unsigned commit, and says so.
- What charter tells an agent a memory will do, and what `charter remember` prints, now
  follow `[plane] mode`: a memory travels with the plane's next save. The old text promised
  that `share = "push"` pushed each memory immediately, which this charter never did.
  ([#293](https://github.com/diazoxide/charter/issues/293))

### Fixed

- **A plane with no workspace offers to make one.** The window drew no way to create the first
  workspace; only the command palette could. The middle of the window now offers "Create a
  workspace", and the workspace strip with its `+` is always drawn.
- **A chat outside every workspace starts in the plane, not in `/`.** With no workspace to start
  in, a chat took the app's own working directory, which is `/` for an app opened from the
  Finder or the Dock, and so also ran without the plane's vault variables stripped.

- A save that deletes a memory file is no longer refused. The secret check asked for the
  deleted file's staged contents, found none, and stopped the save, so making a workspace LOCAL
  could never be saved. ([#301](https://github.com/diazoxide/charter/issues/301))

## [0.2.0] - 2026-09-25

0.2.0 is about settings that belong to a project or a workspace rather than to the machine, and
about vaults you can work with in the window. Project settings and Workspace settings are tabs of
their own, over `charter.toml`, a workspace's `workspace.json` and `charter.local.toml`, and they
choose the extensions, the theme, a workspace's colour and each harness's plugins. A vault can
live in the system keyring and opens in a tab of its own, which reveals or copies a value without
it reaching a chat, and a 1Password token moves into the keyring and out of every chat's
environment. Text size has a Preferences tab, the needs-you queue moves into the title bar, a
handoff is named for its task and can report back, and a relaunch or an update asks before it
reopens your sessions. Repos have right-click menus. The window can invoke only the commands an
allow-list grants it, and a `charter.local.toml` that git would carry no longer decides anything,
and every settings group that it would have changed says so.

### Added

- **Project settings**, a tab of its own: right-click a project's tab and choose _Project
  settings…_, or find it in the palette. It has two sections — **Shared**, `charter.toml`,
  which is committed and your team sees, and **Local**, `charter.local.toml`, which stays on
  this machine — each as a form over the keys charter documents and as raw TOML for everything
  else. Saving keeps your comments and the order of your keys, and refuses what charter would
  refuse when it next reads the file, in the same words: a forge it cannot resolve, a profile
  in the committed file, a value that looks like a credential. Local is created on the first
  save, and never where git would commit it. ([#252](https://github.com/diazoxide/charter/issues/252))
- **Extensions per project.** Each project can turn an installed extension on or off, and set
  what it declares, in either section of Project settings: Shared for the team, Local for you,
  and Local wins key by key. The tab shows every extension with what it is in this project —
  on, off, _needs approval here_, or _not installed here_ — and which file decided it. Approval
  stays with this machine: a project that enables an extension you have not approved leaves it
  off until you approve it in Extensions. A project that says nothing keeps every approved
  extension on, as before. Panels, views and themes follow the project in front, and a view
  refuses to run in a project that turned its extension off. ([#253](https://github.com/diazoxide/charter/issues/253))
- **Harness plugins per project.** Project settings has a *Harness plugins* group for each
  harness charter knows, in Shared and in Local. For Claude Code it lists the plugins installed
  on this machine, and each one can be on, off or not set for the chats charter starts in the
  project. Local wins plugin by plugin, and not set leaves the plugin to Claude Code's own
  settings. charter's own plugin is always on and the old `charter@charter` always off. No file
  can change either, and a save that tries is refused. Codex and opencode list what they have
  installed and say their plugins are not supported yet, with the reason: Codex ignores a
  plugin's on/off given for one session, and charter does not start opencode chats yet.
  ([#274](https://github.com/diazoxide/charter/issues/274))
- **A theme per project.** Project settings has a Theme select in Shared and in Local: charter's
  dark or light theme, *Follow the system*, or any theme an extension you approved contributes.
  Local wins over Shared. While that project is in front the window and every terminal draw its
  theme, and switching projects switches it live. A pick whose extension is off in the project,
  or not approved on this machine, draws the built-in dark theme, and the tab says why. A
  project's pick wins over your `theme.json`; a project that picks nothing keeps it. ([#273](https://github.com/diazoxide/charter/issues/273))
- **Workspace settings**, a tab of its own for each workspace: right-click a workspace's tab and
  choose _Workspace settings…_, or find it in the palette. A workspace can turn an extension on
  or off and set what it declares, for everyone who works in it: it is kept in the workspace's
  `workspace.json`, committed with a LIVE workspace. It sits between the project's two files —
  `charter.toml`, then the workspace, then `charter.local.toml` — so a workspace refines its
  project and this machine still has the last word, and none of them reaches past this
  machine's approval. Each extension says which of them decided it. The panels and views
  follow the workspace in front, and a view a workspace turned off says so and where. Saving
  changes nothing else in the manifest, and a `workspace.json` from before reads as it always
  did. ([#280](https://github.com/diazoxide/charter/issues/280))
- **A workspace's theme and colour.** Workspace settings has a Theme group: a theme for this
  workspace, over the project's `charter.toml` pick and under your `charter.local.toml` — each
  says which file the theme drawn there came from — and a **colour**: red, orange, yellow,
  green, teal, blue, purple, pink, or one of your own. The colour tints the same theme rather
  than replacing it: the accent and the focus ring while the workspace is in front, its tab and
  its chat strip, and a dot on its tab and in the title bar. Text and the terminal keep the
  theme's colours, so everything stays as readable as the theme was. Every workspace tab shows
  its own colour whether or not it is in front, and switching workspaces switches the theme and
  the tint live — the window's theme now follows the workspace in front, not only the project.
  ([#281](https://github.com/diazoxide/charter/issues/281))
- A workspace can also turn each harness's plugins on or off, in **Workspace settings**: one
  **Harness plugins** group per harness, as in Project settings, with each plugin saying whether
  `charter.toml`, the workspace's `workspace.json` or `charter.local.toml` decided it, or that
  nothing did. A Claude Code chat started in the workspace gets that set; Codex and opencode say
  their plugins are not supported yet, for a workspace as for a project. charter's own plugin
  stays on and the old one stays off whatever a workspace says.
  ([#282](https://github.com/diazoxide/charter/issues/282))
- `charter.toml` and `charter.local.toml` accept a `[plane]` section and a `[repos.<name>]`
  table for each repo, which say how far a save goes: `mode` (`off`, `commit`, `push`, `pr`
  or `pr-merge`), `branch`, `save_branch`, `sign`, `autosave` and `autosave_after`. The local
  file overrides the shared one key by key. Nothing saves by these settings yet. For now,
  `charter doctor` and the Project settings tab check them, and the doctor names
  `[memory] share` as the deprecated way of saying `mode`.
  ([#292](https://github.com/diazoxide/charter/issues/292))
- A vault can live in your system's own credential store: the Keychain on macOS, the Secret
  Service on Linux. `charter vault add <name>` makes one by default, and every `charter secret`
  and `charter vault` command works on it as on the other kinds. Each secret is its own
  Keychain item, and on macOS only the charter program that stored it can read it without the
  Keychain asking you first. A plaintext vault file is now something you ask for, with
  `--provider plain-file`. ([#233](https://github.com/diazoxide/charter/issues/233))
- **Vaults in the app.** A Vaults section in the Attention panel lists each vault with its
  provider and how many secrets it holds. Clicking one opens the vault in a tab of its own, as a
  persona opens, and the tab comes back at the next launch. The tab has a search box, **Add**,
  and a table of name, size and when each secret was written. Each row's menu has Edit value,
  Rename, Copy and Delete. The palette has *Open vault…* and *New vault…*, and a new vault is a
  keyring one unless you pick another kind. Nothing the window lists or writes ever carries a
  value back. ([#234](https://github.com/diazoxide/charter/issues/234),
  [#235](https://github.com/diazoxide/charter/issues/235))
- **Reveal and copy.** A secret's eye shows its value for 30 seconds, until you press it again,
  or until you press Escape. **Copy** puts the value on the clipboard without it reaching the
  window, marked for clipboard histories to skip. charter clears the clipboard a minute later, or
  when it quits, but only if the clipboard still holds that value. Each reveal and copy writes
  the trace event `charter secret get --reveal` writes, `secret-reveal`, with a `to` field saying
  `window` or `clipboard`. ([#236](https://github.com/diazoxide/charter/issues/236))
- **1Password tokens go into the Keychain, not your chats.** A 1Password vault's tab has a box
  to paste its service-account token straight into the system keyring; the token never enters
  charter's own environment. From then on every `charter secret` command reads it from the keyring,
  so the vault works in a chat and in a terminal that exports nothing. charter runs only the `op`
  it pinned when the token was stored, verified by path and code-signing team, so a chat cannot
  redirect the token to an `op` of its own; the keyring item is random per vault and machine, and
  the binding it was stored against is pinned locally, so a committed registry change cannot steer
  it. No chat the app starts is given any `OP_*` variable (case insensitively) or any other
  identity variable a vault declares. A tab can also move a token an app was launched with, and
  then warns to relaunch charter so the export leaves its process.
  ([#237](https://github.com/diazoxide/charter/issues/237))
- **Text size and Preferences.** The window's text and the terminal's each have a size, kept
  per machine, and a change applies at once. Cmd with `=`, `-` or `0` (Ctrl off macOS) makes
  whichever has focus larger, smaller or back to its default; `Ctrl+Shift+-` is left to the
  shell. The defaults are one step larger than before: 14px in the window, 13 in the terminal.
  The sizes live in a **Preferences** tab, which opens from the app menu (`Cmd+,`, or `Ctrl+,`
  off macOS), the palette, and the opener when no project is open.
  ([#283](https://github.com/diazoxide/charter/issues/283))
- An Ignore (✕) on each chat in the needs-you queue takes it out of the queue and out of the red
  counts on its project and workspace tabs at once, without touching the chat. It lasts until
  that chat asks again: its next stop puts it back as a new item. Delete on a focused item does
  the same (Backspace on a Mac), and the palette lists it as "Ignore … until it asks again".
  ([#248](https://github.com/diazoxide/charter/issues/248))
- A launch that has sessions to put back asks first: **Reopen all sessions**, or **Start
  fresh**, naming how many chats and view tabs each project had. Start fresh puts nothing back.
  Escape, closing the question, or no answer at all reopens them, as before.
  ([#250](https://github.com/diazoxide/charter/issues/250))
- When an update is installed, the title bar says **Restart to update**. It restarts charter
  into the new version and offers every chat and view tab back, with **Reopen all** as the
  answer in front and a line saying charter restarted to install an update. A chat that is
  mid-turn is named first, and you choose to restart now or wait. If the restart does not come
  back, the next launch offers the same sessions.
  ([#251](https://github.com/diazoxide/charter/issues/251))
- A chat is named for its persona and a number, such as `steward 1`, or for its harness, such as
  `claude 4`, when it has no persona. The picker has an optional Name field, and a chat's tab
  renames by a double-click, F2, its menu's Rename row or the palette. A blank name gives the
  default back. The name is charter's label only, so a rename never touches the running program,
  and it comes back with the chat after a relaunch.
  ([#254](https://github.com/diazoxide/charter/issues/254))
- A handed-off chat is named for its task. `charter handoff --name "<short task>"` names the new
  chat's tab, and the handoff skill always writes one from the brief; without it the tab is the
  ordinary `<persona> <N>`, so four handoffs from one chat are four tabs you can tell apart. The
  chat it came from is shown by name, never by number — `↳ from steward 3 · platform-next` in the
  tab's tooltip and the chat's corner, and in the new chat's first line.
  ([#258](https://github.com/diazoxide/charter/issues/258))
- A handoff can ask for an answer. With `charter handoff --report`, the new chat is told to
  finish with `charter handoff report "<summary>"`, and the chat that asked gets a needs-you item
  (`<chat> reported back`) and the report as context on its next turn — quoted as data, and never
  typed into it. The report goes only to the chat that asked, and exactly once per handoff —
  another needs another `--report` handoff; if that chat has closed, the next chat in its workspace learns it when
  it starts. Without `--report`, nothing changes. ([#259](https://github.com/diazoxide/charter/issues/259))
- Right-click a repo — its heading in the explorer, or its row in the bottom bar — for **New tab
  in** it, which starts that one tab's chat in the clone, and **Start new chats in** it, which
  makes the clone where every new chat starts until you pick somewhere else, as picking a
  worktree does one level down, and the explorer marks it.
  Shift+F10 or the menu key opens any of charter's menus on the row the keyboard is on.
  ([#174](https://github.com/diazoxide/charter/issues/174))
- The explorer is a tree to a screen reader and to the keyboard: Right opens a clone or moves to
  a row's first child, Left closes it or moves to its parent, and a typed letter moves to the
  next row starting with it. ([#238](https://github.com/diazoxide/charter/issues/238))
- Delete on a focused project or chat tab closes it, as its × does, and so does Backspace on a
  Mac. Ending a chat still asks first, and closing a project that has chats open now asks too,
  from the ×, Delete, the tab's menu and the palette, naming each chat it would end.
  ([#239](https://github.com/diazoxide/charter/issues/239))

### Changed

- The needs-you queue is in the title bar now, and nowhere else. A hand and a count sit left of
  About when anything needs you. When nothing has asked but a chat that can't report is open — a
  shell, or a harness without charter's hooks — it is a faint hand with no number, and its
  tooltip and list name those chats ("shell 2 can't tell charter it's waiting"). With neither,
  nothing is there. Pressing it lists every
  chat asking in every open project — its name, then its workspace and project — each with
  **Go**, which brings that chat to the front and switches project and workspace to get there,
  and **✕**, which ignores it. The Attention panel no longer has the queue; its other sections
  are unchanged. From the keyboard, Tab reaches the button, Enter opens the list, the arrows
  move, Delete ignores, and Escape closes it.
  ([#249](https://github.com/diazoxide/charter/issues/249))
- Tab reaches the whole window, in the order it is drawn. Each strip and each list is one stop,
  and the arrows, Home and End move inside it. A terminal keeps Tab for its shell, and
  Ctrl+Tab and Ctrl+Shift+Tab leave it. A pane's split and close controls show on hover and when
  the keyboard is on them, not all the time on the pane you are typing in.
  ([#189](https://github.com/diazoxide/charter/issues/189))
- Nothing in the window rubber-bands on macOS any more. A panel scrolls and the window does not,
  and a scroll no longer carries out of a panel into the page.
  ([#263](https://github.com/diazoxide/charter/pull/263))

### Fixed

- On Linux and Windows the app menu no longer takes a key the chat's shell owns: `Ctrl-C` in a
  chat is the interrupt again, not Copy, and the same goes for `Ctrl-A`, `Ctrl-Z`, `Ctrl-Y`,
  `Ctrl-V`, `Ctrl-X` and `Ctrl-H`. Quit is `Ctrl+Shift+Q` there, as in a terminal app. macOS is
  unchanged. ([#241](https://github.com/diazoxide/charter/pull/241))
- `charter doctor`'s `git auth` row checks the one-credential git policy, the check
  `charter git-policy` runs, instead of saying it is not checked. It only reads, and names
  `charter git-policy --apply` for a clone that drifted.
  ([#241](https://github.com/diazoxide/charter/pull/241))
- Closing a chat that was asking for you, with the × on its tab or by ending its pane, takes it
  out of the needs-you queue and out of the red counts on its project and workspace tabs. It
  used to stay there until some other chat moved.
  ([#247](https://github.com/diazoxide/charter/issues/247))
- A chat's report that raced a close, or an Ignore, can no longer put the chat back in the
  needs-you queue: every update the window gets is numbered, and it keeps the newest.
  ([#248](https://github.com/diazoxide/charter/issues/248))
- A persona's card names the vault `charter persona list` names. A persona whose definition has
  no `vault:` line but that `vaults.json` tags a vault to used to be shown as "not declared in
  its definition"; the card now shows that vault's name and says it came from the vault
  registry. A persona nothing names a vault for says so, a `vault: none` still says it holds no
  credentials of its own, and a registry that does not read is shown with charter's reason.
  ([#185](https://github.com/diazoxide/charter/issues/185))
- `charter persona stats` reads a dispatch log's timestamps as Python's
  `datetime.fromisoformat` did, digit for digit. A stamp such as `2026-03-04T100`, with three
  digits for the time, is skipped rather than read as ten o'clock, and so is a one-digit hour,
  minute or second. Any one character between the date and the time, a comma before the
  fraction and an offset with seconds all read as Python read them.
  ([#315](https://github.com/diazoxide/charter/issues/315))
- The panels follow the plane on disk. A todo closed with `charter ws todo done` in a terminal
  leaves the Todos panel and its count at once, and a workspace made in a terminal is watched
  from then on. Before, a panel changed only when you focused another workspace and came back.
  ([#264](https://github.com/diazoxide/charter/issues/264))
- `charter save` against a remote that moved no longer waits on a signer that never answers.
  The rebase that replays its commit has a two-minute deadline, as the commit itself does, and a
  rebase stopped at it is reported as out of time, not as a conflict. `charter save --sign`
  replays its commit signed, and a save without `--sign` never asks a signer.
  ([#242](https://github.com/diazoxide/charter/issues/242))

### Security

- The window can only invoke the commands on the app's allow-list. Every command it calls is
  now listed in one place and granted to the main window by name. Anything not on the list is
  refused before it runs, and so is a call from any other window. A vault's reveal and copy
  have a grant of their own and reach the main window only, so a window added later does not
  get them by default. The Content-Security-Policy is tighter as well: the window loads no
  plugins or frames, submits no forms, and accepts no `<base>`. The policy and the allow-list are
  now separate guards on reveal and copy. Before, the policy was the only one.
  ([#276](https://github.com/diazoxide/charter/issues/276))
- A `charter.local.toml` that git tracks, or would commit, no longer decides anything. The file
  is meant to stay on one machine, and charter already refused the harness profiles in it when
  git would carry it. The extensions, theme and harness plugins it chose were still applied,
  though, so a copy committed by mistake reached every clone of the plane. Now charter reads
  nothing in such a file, and the workspace and `charter.toml` decide instead. The Local section
  of Project settings still shows the file and says why it is not read and how to fix it: add
  `/charter.local.toml` to `.gitignore` (`charter reinit` does that), or, if git already tracks
  it, `git rm --cached` it first. ([#308](https://github.com/diazoxide/charter/issues/308))
  The Extensions, Theme and Harness plugins groups say it too, in the same words, in Project
  settings and in every Workspace settings tab, wherever the file set something. Before, a
  value you set in Local showed as decided by `charter.toml` or the workspace, with no reason
  given.
  ([#319](https://github.com/diazoxide/charter/issues/319))

## [0.1.1] - 2026-09-24

0.1.1 brings back what 0.1.0 left out and a working plane still used: vault access through
`charter secret` and `charter persona secret`, and the `persona use`, `list`, `sync-agents` and
`stats` commands. A chat started by charter 0.1.0 finds the app's own `charter` first on its
`PATH`, so a plane whose instructions call those commands lost them. This release restores them.

### Fixed

- `charter persona list`, `persona use`, `persona sync-agents` and `persona stats` work again,
  and answer as the charter your plane was set up with did. Re-syncing a plane's sub-agents
  changes only the ones whose persona changed since they were last generated.
  ([#228](https://github.com/diazoxide/charter/pull/228))
- `charter ws todo` says what it recorded, closed or dropped, and a slug that is not there
  says so instead of passing silently. ([#228](https://github.com/diazoxide/charter/pull/228))
- `charter secret`, `charter persona secret` and `charter vault` are back. A chat that runs
  `charter secret exec <vault> --file KUBECONFIG=<key> -- kubectl …` or `charter secret list
<vault>` got a usage error from 0.1.0, which put charter first on the chat's `PATH` without
  them; they now answer as the Python charter did, with the plain-file, reference and 1Password
  providers, and a value still never reaches the chat: `list` prints names, `get` a size band
  and a keyed fingerprint, and `exec` hands values to the command's environment or to 0600 temp
  files it removes, redacting what the command prints.
  ([#227](https://github.com/diazoxide/charter/pull/227))
- The Bash guard refuses a vault file read that is wrapped in `charter secret exec … --`, the
  way it refuses one wrapped in `env`. ([#227](https://github.com/diazoxide/charter/pull/227))
- The Bash guard no longer mistakes text for a handoff. A multi-line quoted string that
  mentions `charter handoff`, such as a commit message, is read as the text it is, and a real
  `charter handoff` after it is still judged. ([#226](https://github.com/diazoxide/charter/pull/226))
- A harness profile that wraps another program (`["ccs", "work"]`) starts as
  `ccs work --plugin-dir …`, with charter's flags after the profile's own words, so a wrapper
  that expects its subcommand first works. A plain `claude` or `codex` profile starts exactly as
  before. ([#226](https://github.com/diazoxide/charter/pull/226))
- What `charter docs show` serves, and every message charter prints, name only commands this
  charter has. A page about something it does not do is gone, and a planned command says "not in
  this version yet". ([#226](https://github.com/diazoxide/charter/pull/226))

## [0.1.0] - 2026-09-23

### Added

- charter is a desktop app for macOS and Linux. A window holds your projects as tabs, each
  project's workspaces, and each workspace's chats, and a chat is a live terminal running
  Claude Code or Codex.
  ([#14](https://github.com/diazoxide/charter/pull/14),
  [#111](https://github.com/diazoxide/charter/pull/111),
  [#125](https://github.com/diazoxide/charter/pull/125),
  [#131](https://github.com/diazoxide/charter/pull/131))
- Opening a project that you have not approved shows what it would run first, and nothing runs
  until you say yes. ([#110](https://github.com/diazoxide/charter/pull/110),
  [#121](https://github.com/diazoxide/charter/pull/121),
  [#145](https://github.com/diazoxide/charter/pull/145))
- A new chat starts on the harness profile and persona you pick, in the workspace or in one of
  its worktrees. ([#32](https://github.com/diazoxide/charter/pull/32),
  [#41](https://github.com/diazoxide/charter/pull/41))
- Every chat says what it is doing, and the chats waiting for you are listed first and counted
  in the window. ([#26](https://github.com/diazoxide/charter/pull/26),
  [#51](https://github.com/diazoxide/charter/pull/51),
  [#157](https://github.com/diazoxide/charter/pull/157))
- Chats open as tabs and split side by side. The split and close buttons sit on the pane they
  act on, and ending a chat asks first. ([#14](https://github.com/diazoxide/charter/pull/14),
  [#176](https://github.com/diazoxide/charter/pull/176))
- The window has four regions: a worktree explorer on the left, the chats in the centre, and a
  bottom bar with each repository's branch, changes, worktrees and running pipeline.
  ([#141](https://github.com/diazoxide/charter/pull/141),
  [#154](https://github.com/diazoxide/charter/pull/154),
  [#156](https://github.com/diazoxide/charter/pull/156))
- A worktree can be cut and removed from the window, and a workspace or a project can be made
  and deleted there too. ([#31](https://github.com/diazoxide/charter/pull/31),
  [#34](https://github.com/diazoxide/charter/pull/34),
  [#172](https://github.com/diazoxide/charter/pull/172),
  [#192](https://github.com/diazoxide/charter/pull/192))
- A command palette and right-click menus reach every action the bars have.
  ([#45](https://github.com/diazoxide/charter/pull/45),
  [#172](https://github.com/diazoxide/charter/pull/172),
  [#194](https://github.com/diazoxide/charter/pull/194))
- A project, a workspace and a chat can each be pinned.
  ([#143](https://github.com/diazoxide/charter/pull/143))
- A status line along the bottom of the window carries the doctor, the alerts drawer for every
  open project, and a note when a project pins an older charter.
  ([#153](https://github.com/diazoxide/charter/pull/153),
  [#160](https://github.com/diazoxide/charter/pull/160),
  [#162](https://github.com/diazoxide/charter/pull/162),
  [#165](https://github.com/diazoxide/charter/pull/165))
- Each chat shows its context and cache gauge in its pane's corner, with a bar per turn for its
  usage trend. ([#164](https://github.com/diazoxide/charter/pull/164),
  [#167](https://github.com/diazoxide/charter/pull/167))
- The personas panel lists each persona with its memories, searchable and loaded a page at a
  time. ([#173](https://github.com/diazoxide/charter/pull/173),
  [#206](https://github.com/diazoxide/charter/pull/206))
- An extension is a directory you point charter at. Nothing it declares is in force until you
  approve it, and charter asks again when anything in that directory changes.
  ([#150](https://github.com/diazoxide/charter/pull/150),
  [#180](https://github.com/diazoxide/charter/pull/180))
- A request that belongs in another chat can be handed off from inside the app.
  ([#207](https://github.com/diazoxide/charter/pull/207))
- The title bar says which project, workspace and chat you are in, and opens About Charter.
  ([#205](https://github.com/diazoxide/charter/pull/205))
- The app updates itself from a stable or a dev channel, and installs only what the release
  key signed. ([#158](https://github.com/diazoxide/charter/pull/158))
- The `charter` command ships inside the app, so hooks and the Bash guard answer without a
  Python install. ([#168](https://github.com/diazoxide/charter/pull/168),
  [#181](https://github.com/diazoxide/charter/pull/181))
- A tab can hold a view, not only a chat. A persona opens as its own tab: what it is for, its
  tools and vault, and its memories, searchable. An approved extension can add a view of its
  own. The first is persona statistics, with charts of each persona's memories, which charter
  asks one question at a time and only when you open it. ([#212](https://github.com/diazoxide/charter/pull/212))
- The view tabs you had open come back at the next launch, and wait for a click before an
  extension is asked anything.
  ([#212](https://github.com/diazoxide/charter/pull/212))
- The palette can put the app's own `charter` on your terminal's `PATH` on macOS: **Install
  `charter` command in PATH** links `/usr/local/bin/charter` to it, and never replaces a
  `charter` something else put there. ([#219](https://github.com/diazoxide/charter/pull/219))
- A chat opens knowing who it is: the persona you picked, what it remembers, and the
  workspace's todos arrive with its first message. The guards on reading a vault, on writing
  into charter's own state, and on sending a sub-agent run in the app's own `charter`.
  ([#220](https://github.com/diazoxide/charter/pull/220))

### Changed

- A chat needs nothing installed from the Python charter. The app carries its own Claude Code
  plugin with charter's hooks, its Bash guard and its skills, and loads it into each chat it
  starts, for that chat alone. The chat turns the Python charter's plugin off for itself, and
  finds the app's own `charter` first on its `PATH`. A chat starts offline.
  ([#219](https://github.com/diazoxide/charter/pull/219))
- The light and dark themes are data files, and the window and the terminal are both drawn from
  them. ([#144](https://github.com/diazoxide/charter/pull/144))
- The terminal follows a theme switch while it is open. The window's layout lives in
  `charter/layout.json`, which you can edit by hand, and it is in place before the first
  frame is drawn. ([#215](https://github.com/diazoxide/charter/pull/215))
- About Charter tells this app's own story: its version and what that version brought.
  ([#215](https://github.com/diazoxide/charter/pull/215))
- The region toggles sit at the left end of the status line. The context gauge floats over its
  pane rather than taking a row from it, and a very light line divides one tab from the next.
  ([#214](https://github.com/diazoxide/charter/pull/214))
- The project, workspace and chat strips nest, and tabs that do not fit collapse into a
  _N more_ button instead of scrolling. ([#139](https://github.com/diazoxide/charter/pull/139),
  [#171](https://github.com/diazoxide/charter/pull/171))
- Closing the window hides it to the tray. Quitting says which chats it will end, and the next
  launch puts back the projects and chats you had open.
  ([#23](https://github.com/diazoxide/charter/pull/23),
  [#125](https://github.com/diazoxide/charter/pull/125))
- Tab moves through every dialog, and Ctrl-K belongs to the chat that has the keyboard.
  ([#188](https://github.com/diazoxide/charter/pull/188),
  [#190](https://github.com/diazoxide/charter/pull/190))
- `charter init` adopts the repository it is pointed at instead of turning it into a plane.
  ([#115](https://github.com/diazoxide/charter/pull/115),
  [#197](https://github.com/diazoxide/charter/pull/197))
- The app, the dock and the menu bar carry charter's own mark.
  ([#159](https://github.com/diazoxide/charter/pull/159),
  [#203](https://github.com/diazoxide/charter/pull/203))
- A macOS build is ad-hoc signed when no Apple Developer ID is set up. The first install needs
  one command, and the release page says which.
  ([#201](https://github.com/diazoxide/charter/pull/201))
- `charter version` prints the app's own version. A plane pinned to a release
  of the Python charter is reported as that older line, not as drift. `charter doctor` and
  every other message stop sending you to the Python charter, and `charter docs show`
  describes this app. ([#219](https://github.com/diazoxide/charter/pull/219),
  [#223](https://github.com/diazoxide/charter/pull/223))

### Fixed

- A program that starts a screen update and never finishes it no longer freezes the pane.
  ([#7](https://github.com/diazoxide/charter/pull/7),
  [#19](https://github.com/diazoxide/charter/pull/19))
- A pane opened late catches up on what the chat already printed.
  ([#11](https://github.com/diazoxide/charter/pull/11))
- A chat started from an app opened in Finder finds `charter` and its harness.
  ([#135](https://github.com/diazoxide/charter/pull/135),
  [#168](https://github.com/diazoxide/charter/pull/168))
- An extension's program that crashes is reported with its exit status and its last words,
  not as a lost connection. ([#217](https://github.com/diazoxide/charter/pull/217))
- A slow `git` is no longer reported as a broken repository.
  ([#44](https://github.com/diazoxide/charter/pull/44))
- No program charter starts can hold a chat's terminal open after the chat ends.
  ([#105](https://github.com/diazoxide/charter/pull/105))

[Unreleased]: https://github.com/diazoxide/charter/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/diazoxide/charter/releases/tag/v0.3.0
[0.2.0]: https://github.com/diazoxide/charter/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/diazoxide/charter/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/diazoxide/charter/releases/tag/v0.1.0
