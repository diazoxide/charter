# Changelog

What each release of charter, the desktop app, brought. About Charter shows the section for the
version you are running, and the same section is that version's GitHub release notes.

The app has its own version line, starting at 0.1.0. It is not the version of the Python
`charter` it was rebuilt from. `charter news` prints this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **The event log keeps 30 days and can be followed from a cursor.** The host's event log
  (`<data>/events/<device>/`) is now written in segments: at 16 MiB a segment is sealed as
  `events.<first seq>.jsonl`, and a sealed segment whose newest event is more than 30 days old is
  deleted when the log is opened or a segment sealed. A client subscribes from the last `seq` it
  holds and gets every later event once and in order, across segments and across a host that was
  killed and started again; a cursor older than what is kept is told what it missed (FD-24,
  #661).
- **A first task, beside your first chat.** After the first run opens your repo, a **First task**
  tab sits beside the chat. It gives one small, real task to two chats, on two harnesses or two
  profiles of one, each on a branch of its own in charter's copy of your repo, with the task typed
  in for you to read and send. The task asks each chat to record what it learned, so the second
  chat starts with the first one's lesson in its briefing, whichever harness it runs on. **Show its
  diff** opens a shell in that chat's branch with its diff. Nothing is written into your repo, and
  the task asks for nothing to be pushed (FR-28, #621).

- **Read any file of a worktree in charter.** A worktree's menu in the explorer has *Browse the
  files of …*: a tab listing the worktree's files, narrowed as you type, with the file you pick
  drawn beside the list in the light editor, charter's read-only CodeMirror 6 viewer with syntax
  colours from your theme. *Open in a tab of its own* gives a file a tab to itself. A binary file
  or one past 5 MiB is named rather than drawn. The diff view the Review tab will use is built
  on the same component (RC-5, #706).

- **Agent commits say who made them.** A commit an agent makes in a chat, with its own
  `git commit` in a workspace repo or with `charter save` in the project, ends with git
  trailers: `Assisted-by: <harness>:<model>`, `Charter-Chat: <chat id>`,
  `Charter-Persona: <persona>`, and `Charter-Change: <change>` when its branch is a change's. A
  trailer charter does not know is left out, a commit you make by hand gets none, and your own
  `Co-Authored-By` lines stay. Set `assisted_by = "llm"` under `[plane]` or `[repos.<name>]` for
  the Linux kernel's bare `Assisted-by: LLM` (V67, GL-8, #702).

- **Link a chat to a work item.** A chat tab's menu and the palette have **Link to work item…**,
  which asks for the item's tracker key (`github:github.com/owner/repo#12`, or a todo's
  `todo:<workspace>/<todo>`), and **Unlink work item**. A linked chat shows `Work item: <key>`
  in its tab's tooltip and in its pane's corner. A chat works on one work item at a time. The
  link goes in the workspace's work link log, so it reaches your other devices when the
  workspace is LIVE, and it follows a todo when that todo is promoted to an issue. A chat at
  the project root is not offered it (V60, ADR 0088, FW-5, #732, #914).

- **Promote a todo to an issue.** `charter ws todo promote <slug> --repo <repo>` opens an issue
  with the todo's title and text in one of the workspace's repos, on GitHub or GitLab, as you.
  It names the repo, whether it is public and the workspace label it adds (private repos only)
  before it sends anything, and refuses a repo that is archived or takes no issue from your
  account. The todo is then closed, journalled as `Promoted todo:
  <title> → <issue>`, and an alias in the workspace's new work link log
  (`workspaces/<ws>/work/<device>.jsonl`, committed with a LIVE workspace) makes every work link
  on the todo reach the issue. The Work list shows the issue once, not the issue and the todo
  Claude Code and opencode ask you before
  any spelling of it runs: `charter init` and `charter reinit` write the ask rule
  `charter *todo*promote*` (V42) (ADR 0088, FW-5, #732).

- **The changes view pushes a change and lands a member, and asks first.** Each change in the
  changes tab has a **Push** button, and each member row a **Land** button. Push names every
  repo, branch, commit and destination, and where each pull request or merge request goes, before
  anything is pushed. Land runs every gate first: a refusal is shown beside the row in the same
  words as `charter change land`, and otherwise it names the request, the head commit its
  checks passed at, and whether charter merges it now or puts it in the merge queue or merge
  train. Pressing Push or Land does exactly what was shown, and nothing if it has changed since
  (#474).

- **A chat keeps its id across a relaunch.** The app records each chat's id, the device that
  made it and its current run in `.charter/app/reopen.json`, so the event log names a reopened
  chat as the chat it was, and its first event after the relaunch is `run.started` with cause
  `reopen`. A chat that comes back without its conversation, or that **Resume** starts again
  after its harness could not find the conversation, begins a `fresh` run in the same chat
  (ADR 0066, #834).

- **A copied project's chats get ids of their own.** A project copied with its `.charter/`
  directory (`cp -R`, rsync, a backup restore) used to carry its original's chat ids, so the two
  clones' events named one chat. The record now says which clone and device wrote it, and the
  first launch of a copy, at another path or on another machine, gives every chat a new id. A
  moved project keeps its ids. Its chats also get new ids when the project moves to another
  machine, when the machine store is reset, and when a different `CHARTER_CONFIG_HOME` opens
  it. A copy keeps its ids only when its original has
  been deleted and was never opened on this machine (V43,
  amending ADR 0066).

- **The supported platforms are written down.** macOS 27 and 26 on Apple silicon, Ubuntu 26.04
  and 24.04 LTS, and Fedora 44, each with the CI job that covers it and what that job proves,
  are in the README and in `docs/platforms.md`. macOS 27, Ubuntu 26.04 and Fedora are covered
  by evidence jobs, which report and never gate. Windows 11 joins the list when the
  Windows port lands; iOS 16.4 and the current desktop browsers are the floor for clients that
  do not exist yet (FR-25, #619).

- **A project switcher.** ⌘P (Ctrl+Shift+P off a Mac), the new button at the start of the
  project strip's controls, or *Switch project…* in the palette lists the projects open in the
  window, the last one you were in first and already selected, so the key and Enter take you
  back to it. Pressing the key again moves down the list, and typing narrows it by name. The
  button shows once a window holds two projects (FR-27, #620).

- **`charter change revert` undoes a cross-repo change as a new one.** `charter change revert
  <change>` creates `revert-<change>`: for every member charter landed, a branch off that repo's
  default branch carrying `git revert` of the commit charter recorded, ordered so a member that
  depended on another is reverted first. It pushes and merges nothing; you push and land the
  revert with `charter change push` and `charter change land` like any other change. A member
  merged outside charter is named for you to revert by hand. A recorded commit that is not a
  commit id, or that the default branch no longer holds, is refused by name, and so is a clone
  with uncommitted work. A revert that conflicts is aborted and its branch left for you to
  finish. Run it again once a refusal is put right, and it seeds only the members it has not
  seeded yet. It never force-pushes, deletes a branch or resets one (#473).

- **`charter change land` lands one member of a cross-repo change.** `charter change land
  <change> --repo <name>` merges that member's request at the commit its checks passed on, and
  only once every member it needs has landed. Each refusal says which gate stopped it: a
  blocker not landed, checks failed, running, not run or unreadable at that commit, a branch
  that moved after the checks were read, or more than one member named. Where the target
  branch has a merge queue (GitHub) or a merge train (GitLab), the request goes into it at that
  commit, and running `land` again after it merged records the landing. A merge charter did
  not start, in the browser or before a queue existed, is never recorded as charter's. It never
  turns on auto-merge. The landing commit carries a `Charter-Change:` trailer where the forge lets
  charter write the message, and a line goes into the change's landing log once the forge
  confirms the merge. Like `gh pr merge`, it is refused in a session nobody is watching.
  `charter doctor` now also names a member landed ahead of one it needs, a member's pushed
  branch merged outside charter, and a request GitLab was left set to merge later (#472).

- **`charter change push` pushes a cross-repo change.** For each member it prints the repo,
  the branch and where it goes, then pushes the branch, opens its pull request into the repo's
  default branch (or finds the one already there), and writes a block into each request's
  description that lists every member's request, so a partial landing can be read from any one
  repo. Running it again opens no second request, and the block is rewritten in place when the
  members change; the rest of the description is left alone. It commits nothing, never
  force-pushes, sends no tags with the branch, and pushes repos whose save mode is `off` too.
  A repo whose git config sends pushes somewhere other than the URL printed is refused. A
  member that is not a repo in this workspace is refused by name and the others are still
  pushed. GitLab members are pushed and their merge requests opened and updated the same way,
  on gitlab.com and on a self-managed GitLab declared in `charter.toml`; a draft merge request
  is adopted and stays a draft, and one from a fork with the same branch name is never touched.
  When members are on different hosts, a member elsewhere is named by its request's link, so
  a GitLab description never points at a GitHub number as if it were a GitLab one. The guard
  that stops a live substitution in `charter change create` or `drop` now names `charter
  change push` as what writes the `why` into requests (#471).

- **The first run lays your project out for the stack you work in.** Project templates for
  Rust, TypeScript, Python, Go, monorepos and docs-only repos each add an engineer and a
  reviewer persona, a review checklist (`REVIEW.md`, in the reviewer's refs), how a change is
  checked in the workspace's `workspace.md`, and the publish and deploy commands, such as
  `cargo publish` or `twine upload`, that Claude Code and opencode ask you about before they run
  them. Codex's command rules live in `CODEX_HOME` or a trusted project's `.codex/rules`, which
  charter does not write, so charter's own guard applies there. The first run picks the
  template that fits your repo from the files at its top level, and says which when you type the
  path; pick another, or None, before you open it. Nothing is written into your repo, nothing you
  already have is replaced, and a command you deny stays denied.
- **A project can say which charter it needs.** When a newer charter starts writing something an
  older one would get wrong, it lists the feature in `charter.toml`'s `requires` and sets
  `schema = 2`. A charter that lacks the feature, or does not understand the project's `schema`,
  or cannot read its `charter.toml`, treats the project as read-only. `charter` still reads it
  (`status`, `recall`, `statusline`, `doctor`, and the `list` commands of workspaces, personas,
  changes, sessions, harnesses and guards, plus `workspace recall`), saying once that it is
  read-only and why. Every command that could write it is refused, extension commands,
  `doctor --fix`, `init` and `reinit` included, with the version to upgrade to (update the app); `charter
  doctor`'s `schema` row says the same. Keys and sections charter does not know are kept when it
  rewrites `charter.toml` or a workspace's `workspace.json`. The window and the chat hooks will
  follow the same rule before any release carries it (#826).

- **A chat that starts in a repo gets a branch of its own.** Start a chat with *New tab in
  <repo>*, or after *Start new chats in <repo>*, and charter cuts a new branch for it, in a folder
  beside the repo's clone, so two chats in one repo no longer edit the same files. The branch is
  named after the chat, or `chat-1`, `chat-2` and on. Clear *start on a new branch* in the picker
  to work on the branch the repo has checked out, as before. If the chat does not start, the
  branch goes with it. *New branch in <repo>…* on the repo's menu and in the palette cuts one
  without starting a chat, and new chats start on it.

- **A chat in its own worktree leaves an `AGENTS.md` behind for any agent that opens it.** When
  a chat starts in a piece of a repo that has no `AGENTS.md`, charter writes one there, hidden
  from `git status`. It says which persona the chat was started as and which piece it holds, so
  a Codex, opencode or other agent started there by hand is told the same. It never carries
  memory, todos or session records, and it is never written into a shared clone. A repo that has
  its own `AGENTS.md` keeps it. Because git hides the file by one line for every worktree of the
  repo, `charter doctor`, a note on the chat's pane and the chat's briefing name any `AGENTS.md`
  of yours that line hides (ADR 0085).

- **charter keeps a log of every hook call its chats make.** Each state hook and each tool call
  is one line in the event log in charter's data home (`~/Library/Application Support/charter`
  on macOS, `~/.local/share/charter` on Linux, or `CHARTER_DATA_HOME`). A line records the chat
  and the run it happened in, the tool, what charter's guard answered and how long it took. A
  tool call's arguments are never written, only a digest keyed to this machine. The log is
  never committed or sent anywhere; a backup of charter's data carries it, and anything running
  as your user can read it. The coming timeline, audit and fleet views are built from it (#649).

- **The first run offers your repo's agent instructions to its workspace's memory.** When the
  repo you open has a `CLAUDE.md`, an `AGENTS.md` or Cursor rules in `.cursor/rules`, a *Memory
  from the repo* tab opens beside the first chat, showing each file whole. Press Add to memory
  and every chat in that workspace starts with them, whichever agent runs it. Nothing is written
  until you press it, and nothing is ever written into your repo. A link, a file over 64 KB, or
  one that looks like it holds a secret is left out and says which line. A file over 8 KB, or one
  with invisible characters, starts unticked, and the preview shows each invisible character by
  its code point.

- **Where to get help, in the app and the repository.** About Charter now links to Discussions
  for questions and ideas, to the bug report form, and to `SUPPORT.md`, which says where each
  kind of question goes and how soon it gets a first response. The repository gains a Code of
  Conduct (Contributor Covenant 2.1), a `CONTRIBUTING.md` with DCO sign-off, issue and
  discussion forms, and a pull request template (#609).

- **Every release says what it is made of.** Each release carries a CycloneDX SBOM per platform
  (`charter-macos-arm64.cdx.json`, `charter-linux-x86_64.cdx.json`). It lists the Rust crates
  read out of the built binaries and the front end's npm packages from its lockfile. Every
  Rust binary charter ships now carries its own dependency list, so
  `cargo audit bin` can check an installed copy against the RustSec advisories. `SECURITY.md`
  says how to read both (#584).

- **Every build file a release publishes carries signed build provenance.** Each installer and
  updater archive now has a SLSA build provenance attestation, so you can check that a download
  came out of charter's own release workflow with `gh attestation verify`. `SECURITY.md` has
  the command (#583). The update manifest (`latest.json`, `dev.json`) is not attested; the
  updater checks each build's own minisign signature, as before.

- **The app keeps a log you can read after the fact.** What the app notices while it runs, such
  as a chat that would not start, a project it could not watch or a record it could not write,
  now goes to `charter.<date>.log` in the app's log directory (`~/Library/Logs/dev.charter.app`
  on macOS, `~/.local/share/dev.charter.app/logs` on Linux), as well as to standard error. An app
  started from the Dock or a desktop launcher used to lose all of it. A new file starts each
  day and the last seven are kept. A line that looks like it holds a credential is replaced by
  a note saying what kind it looked like. `CHARTER_LOG_DIR` moves the log.

- **A kill switch that stops every chat and shell charter started.** Stop all on the title bar,
  or `charter stop --all` in any terminal, interrupts and ends every chat's and shell's program in
  every project and every window within seconds. No chat starts again, not even from a relaunch,
  until you re-arm it from the title bar; you can still open a shell to look around. The stopped
  chats stay as tabs to reopen. Each stop, re-arm and tamper is one line in `kill-switch.jsonl`
  in charter's config directory (ADR 0071).

- **A plane can run every Claude Code chat in a sandbox.** Add `[sandbox]` with `mode = "on"`
  to `charter.toml`, and every Claude Code chat charter starts there runs inside Claude Code's
  own sandbox, which charter compiles from one policy. It can reach only the hosts of the
  plane's egress presets. Claude Code's web tools are turned off, because the host list does
  not hold them. The chat can never read a vault's storage or write charter's own state. A
  plane can turn the sandbox on but never off, and a mistyped `mode` turns it on. If the
  sandbox cannot be applied, the chat does not start, and charter says what is missing and how
  to install it. `SECURITY.md` says what the sandbox covers and what it does not. Planes that
  say nothing run as before (#695).

  **What this cannot do yet:**

  - A plane with a keyring vault starts no Claude Code chat while the sandbox is on. Keyring is
    the default vault provider, so on most machines this release can sandbox only a plane with
    no keyring vault. That lifts when charter can wrap the harness, or resolve secrets for the
    chat.
  - opencode chats are not started in a sandboxed plane yet.

- **Codex chats run in the sandbox too.** In a plane with `mode = "on"`, every Codex chat
  charter starts runs in Codex's own workspace-write sandbox, which charter selects explicitly
  and compiles from the same policy. It reaches only the plane's egress hosts, through Codex's
  own proxy, and never reads a vault's storage or writes charter's own state. Codex never asks
  to run a command outside the sandbox, and its web search is off. A Codex chat whose command
  would drop or widen the sandbox is refused, and the refusal names the flag and where it is.
  That covers `-s`, `--add-dir`, `--cd`, `--enable`, `--disable`, an approval flag other than
  `-a never`, and a `-c` for anything but the model and how it is shown. A Codex without the
  proxy feature starts no sandboxed chat. As with Claude Code, a plane with a keyring vault
  starts no sandboxed Codex chat yet (#695).

- **A first run that goes straight to a chat.** On a machine with no project, charter asks for
  one thing: the repo to work on. It makes a project for you on this machine only
  (`~/.config/charter/local-plane`, with no remote), copies your repo into a workspace named
  after it, and opens the first chat there. When exactly one of Claude Code, Codex and opencode
  is signed in, the chat starts on it without asking. The screen shows what is installed and
  signed in, and offers **Sign in to GitHub**, which runs `gh auth login` in a shell tab. New
  project asks for a repo the same way, and the two-folder form is under Advanced (#603).
- **A chat's commits are scanned for secrets and personal data before they are made, in any
  repository.** Every chat the app starts on a harness commits through charter's own git
  hooks. Before each commit, including a merge commit, charter reads the lines the commit adds.
  These refuse the commit:

  - a token or key in a vendor's known shape (charter's forge-prefix rules plus gitleaks'
    prefix-anchored vendor rules);
  - an email address;
  - a card number;
  - a US Social Security number.

  git prints where each one is and what kind it is, with the value masked, and the chat joins
  the needs-you queue saying why. This holds in a workspace repo, a piece, or a repository
  outside any plane. The repository's own hooks (husky, pre-commit, git-lfs) still run after
  charter's check, except `post-index-change` and `reference-transaction`, which charter does
  not forward. The guard also refuses `git commit --no-verify`, `-n`, and a
  `core.hooksPath` or `GIT_CONFIG_*` set on the command line. These are not treated as anyone's
  email: documentation and private-use domains (`example.com`, `.test`, `.internal`, `.local`),
  no-reply addresses, `git@host` remotes, and image names like `logo@2x.png`. The allowlist
  below lets any other false positive through. Commits made by `cherry-pick`, `rebase` and
  `am` are not scanned (#592, ADR 0074).

- **An allowlist for the commit scan.** A repository's `.charter-scan-allow.toml` lets a
  finding through by its rule and paths. A key can also be let through by its fingerprint;
  personal data never is. Every entry gives its reason, and the file is reviewed like code.
  `charter scan --explain` names a finding's rule and prints the entry that would let it
  through. The file is read as committed. A chat's own commit may not change it, and neither may
  its `git revert` or `git merge --ff-only`, so the operator is the one who commits an entry.
  This is a guard against mistakes; the sandbox is where it becomes a boundary (#784). An
  author's email in `Cargo.toml`, `package.json`, `.mailmap`, `AUTHORS`, `CONTRIBUTORS` or a
  `CHANGELOG` passes with no entry at all, unless `[builtin] enabled = false` turns that off
  (#593).

### Changed

- **Switching into a project with many chats is quicker.** Each chat's row in the explorer and its
  state mark on the tab are dimmed in their colour rather than by transparency, and the mark for a
  chat that has said nothing yet is a ring broken into two arcs rather than a dashed one, so fifty
  of them no longer cost the switch a slow paint (FR-27, #891).

- **Per-machine logs are named by the device id, not the hostname.** The dispatch and skill
  logs (`personas/_dispatch/`, `personas/_skills/`), the piece claim log, the landing log and
  pending landings are now filed under this device's id from the machine store. Two machines
  that share a hostname no longer write into one file, and renaming a machine no longer starts
  a new one. The hostname stays as a label: a piece claim still says which machine made it.
  Files an earlier version wrote under a hostname are still read; a later migration renames them
  (FD-25, #662).

- **Fewer redraws when switching projects.** A project's tab strip comes back at the width it
  had instead of drawing every tab and then folding them away, the palette closes in the same
  frame as the switch, and the project's theme and its repos' save standing are drawn from what
  the window already knows, then checked again behind them (FR-27, #620).

- **A new project tracks the forge its repo is on, and asks when it cannot tell.** The first
  run, the New project dialog and `charter init` now follow one rule. The forge, and its owner or
  group, come from the `origin` of the repo the project is made for, when that remote is on
  github.com or gitlab.com. Otherwise the window asks GitHub or GitLab with two buttons, and
  `charter init` writes nothing and asks for `--forge` or `--adopt`. Before, the first run made
  a GitHub project with no owner, and `charter init` defaulted to GitLab. **A script that runs a
  bare `charter init` now exits 2 and makes nothing, where it used to make a GitLab project:
  add `--forge gitlab` to keep the old result.** `--forge` and `--owner` still win when given,
  and running `init` again on an existing project asks nothing.

- **charter runs far fewer git processes in your repos while it is idle.** Auto-save, the
  title bar's save indicator, the Saving tab and the alerts used to each run `git status` and
  its friends on their own timers — about ten git processes every two seconds per project, and
  more in a busy repo, where they could pile up on each other. They now share one reading per
  repo, made again only when the repo's git files move, its project's watcher sees a change, a
  save or fetch finishes, a chat ends, or ten seconds have passed. Only one git process runs in
  a repo at a time for them. Where your git config says nothing about `core.untrackedCache`,
  charter turns git's untracked cache on for these reads and for its saves, so `status` stays
  quick in a large repo: the cache is then kept in that project's and those clones' index, as
  git keeps it (`git update-index --no-untracked-cache` removes it, and setting
  `core.untrackedCache` yourself, to anything, stops charter asking for it).

### Fixed

- **A memory or session record written by an agent shows up in its panel straight away.**
  Before, a workspace's or persona's `memory/`, and the session records in `sessions/`, reached
  the Memory, Personas and Sessions panels only when something else in the project changed.
  Each panel now reads again only for the kind of change it draws, so a todo closed in one
  workspace no longer re-reads another workspace's panels. Saving a memory re-reads only the
  panels and views that show it. When auto-save is on and commits that memory, the window still
  reads everything again after the commit (FD-10).
- **`charter guard ask` never turns a command opencode denies into one it asks about.** opencode
  goes by the last rule that matches a command, so a rule added after `"*": "deny"` used to win
  over it, and a rule for a command that was denied exactly used to replace the deny. Now a
  denied command stays denied, and the command says so. Claude Code was never affected: there a
  deny always wins.
- **The first run checks `glab` as well as `gh`.** Under **On this machine**, the first window
  now says whether GitLab's `glab` is installed and signed in, beside GitHub's `gh`. If it is
  installed but not signed in, **Sign in to GitLab** opens a shell tab running `glab auth login`.

- **Charter opened from the Finder or the Dock can hold two hundred chats.** macOS starts such
  an app with room for only 256 open files, and each chat needs several, so the app could run
  out well short of two hundred. It now raises its own limit as it starts, to 10,240 or the
  system's hard limit if that is lower, and never lowers a limit it was given (#683).
- **`charter statusline --watch` says it cannot watch instead of drawing one frame.** It has
  no repaint yet, and one frame followed by exit 0 looked like a watch that had stopped. It now
  exits 1 with the reason and points you to `charter statusline` run once per turn (#574).
- **Two GitLab pipeline states are no longer read as unknown.** A pipeline that is
  `waiting_for_callback` now shows as pending and counts as running, and auto-merge waits for it
  instead of saying there is nothing to wait for. A pipeline that is `canceling` shows as
  canceled and did not pass. GitLab lists both in its pipelines API (#711).
- **The status line no longer shows a GitLab fork's merge request as the branch's own.** When a
  fork had an open merge request from a branch of the same name, the status line could name it.
  It now names only a merge request from the project itself, as `charter change show` already
  did (#711).
- **Linux no longer waits half a minute for a desktop portal that cannot start.** Before GTK
  starts, charter asks the session bus to start the portal and gives it 300 ms. If the bus is
  still silent, charter restarts itself without the session bus, and the window comes up in
  under 2 s where it used to take 26–31 s. For that run there is no tray icon and no desktop
  notifications, and a second launch is refused rather than handed over. A per-user lock keeps
  that refusal in place without the bus. Cold start is now measured in CI on X and on i3, and
  every launch is held to the limit (#24).
- **A run without the session bus says so in the window, and chats keep the bus.** The line at
  the top names what is off (the tray icon, notifications, a keyring vault from the window,
  handing a second launch over). charter asks the bus again after 5, 15 and 45 s, and when the
  portal answers the line offers **Restart with the full desktop integration**, asking first
  about any chat that could be mid-turn; charter never restarts by itself. Chats, and the
  programs charter starts itself, get the session bus charter was given rather than the one it
  turned off, so a keyring that git or a CLI uses through D-Bus keeps working. On i3 and other
  X11 sessions with no session bus, charter now starts on the bus the X display holds, as GTK
  would, so notifications reach a notification daemon started from the window manager's config
  (#746).
- **The guard that stops a live substitution in `charter change create` or `drop` no longer
  says `charter change` is missing.** It has shipped since 0.4.0; what is not in this version
  yet is `charter change push`, which will write the `why` into request bodies (#571).
- **A heredoc `cat` reads inside a quoted command substitution is read as data.**
  `x="$(cat <<'EOF' … EOF)"` no longer has `charter handoff` in its body refused as a handoff a
  shell runs. The guard now reads the program that opens a heredoc inside `"$( … )"` as the
  command in the substitution, and still refuses the body when either that command or the one
  the substitution stands in runs it: a shell (anywhere in that command's words), `ssh`, `su`,
  `runuser`, `script`, `source` or `.`, a program it cannot name, or the substitution standing
  in the program's place, prefixes included. A live substitution in a body two shells read
  differently is searched too (#488).

### Security

- **The floor guard refuses more ways an unattended agent could merge.** When nobody is
  watching (`bypassPermissions`), a forge API call to a merge endpoint, a setting that makes a
  pull or merge request merge on its own later (auto-merge, a merge queue or train), an alias
  that stands for a held command or the start of one, and the same commands inside a script a
  shell runs or a command substitution are refused, as `gh pr merge` already was. A forge API call whose effect on a merge
  charter cannot read is refused too. Reads, opening a request and ordinary pushes are
  unchanged, and attended use is untouched (#866).
- **An unattended agent cannot print the forge token or read other stored secrets.** The CLIs'
  token commands, the auth-status and config reads that show the token, git's credential fill,
  and commands that read the OS keychain or a password store are refused when nobody is
  watching, since a run holding a secret could act where no guard sees it. Attended use is
  untouched (#866).
- **A guard that cannot answer refuses the call.** The tool-call guards run with a deep stack and
  a deadline inside the harness's own hook timeout, and the floor bounds how deep it reads a command inside a command, so a line
  built to exhaust them is refused instead of slipping through when the hook dies (#866).
- **The handoff guard finds a handoff in a shell's script past the shell's options**, including
  options that take a value (#866).
- **The hook channel reads only your own user's connections.** Besides each chat's token, the
  app now checks which user is on the other end of its hook socket, and closes a connection
  from any other user before reading it. The socket's private directory already kept other
  users out; this still holds if that directory's permissions are ever wrong (FD-6, #645).

## [0.4.2] - 2026-09-30

0.4.2 is a hardening release: the release pipeline runs only the code it pinned, a chat starts from an allowlisted environment instead of the app's whole one, a plane save scans every file it commits for secrets, plane trust covers the tools a persona is granted, and hook calls are authenticated per chat.

### Security

- **The release runs only the code it pinned.** Every GitHub Action in charter's workflows is
  pinned to a commit rather than a tag, and the release jobs that hold the signing keys restore
  no build cache (ADR 0042).
- **A chat starts from an allowlisted environment.** A chat is no longer given the app's whole
  environment minus a few names: it gets a keep-list (`PATH`, `HOME`, the locale, the proxies,
  `XDG_*`, `CHARTER_*` and the like), the variables its harness declares, and whatever the plane's
  `charter.local.toml` lists under `[chat_env] pass`. Cloud, forge and model-provider credentials
  pass only when listed by exact name.
- **A plane save scans every file it commits.** `charter save` reads every staged file for
  secrets, not only memory and refs, and also catches a bare token by its forge's prefix. A hit
  names the path and line, never the value; a binary or over-10-MiB file outside memory and refs
  is named as not scanned.
- **Plane trust covers persona tool grants.** Each persona's `tools:`, with a digest of any
  `bin/` script a tool names, is part of the approval, and a grant that changed since asks again
  before it runs without a prompt. Old per-session tool ceilings are removed when a plane opens
  (ADR 0035).
- **Hook calls are authenticated per chat.** Each chat gets its own token when it starts, and the
  hook channel accepts a call only with that chat's token.

## [0.4.1] - 2026-09-28

0.4.1 is a chat that ends well and a memory you can look after from the window. Closing a chat
offers **Smart close**: the chat writes a session record of what it did and what is left, goes
into the background as a chip while it does, and its tab closes once the record is saved. Each
workspace, and the plane root, lists its records in a **Sessions** panel, and **Resume** brings
one back as a new chat on its conversation, harness, profile and directory, with the record in
its briefing. A memory opens in a tab of its own, rendered as Markdown, to be edited in place or
deleted with an Undo, and the right column lists a workspace's memory and the shared memory,
with a `+` for a new one. Beyond the window: `charter session record|list|show`, command-line
verbs that edit, archive and unarchive a memory, and Codex and opencode chats that come back to
their own conversation at a relaunch.

### Added

- **Smart close.** Closing a chat now asks **Cancel**, **Close** or **Smart close**. Smart close
  sends the chat one line asking charter's `smart-close` skill to write its session record — at
  once to a chat that is waiting, at the end of its turn to one that is running — and the tab
  closes only when `charter session record` says the record is saved. Meanwhile the chat goes
  into the background: its tab shrinks to a chip — the chat's icon and a breathing amber mark,
  its name in the tooltip — at the left edge of the chat strip, before the pinned tabs, and the
  chat that was beside it comes to the front, as Close would bring it. Click the chip to watch
  the chat work; it stays a chip until it is done, and it cannot be dragged. The chip's menu
  offers **Cancel smart close**, and typing into the chat cancels it too, except to answer a
  question the chat asks on the way. When the record is saved the chip goes, and a quiet notice
  says **Session saved — <title>** with **Open record**. If no record arrives within five
  minutes, the chat ends first, or its prompt could not be sent or Smart close could not start,
  the tab comes back where it was, the window says so in a sentence, and the title bar's ✋ list
  names the chat and why. In a Claude Code chat the app starts, `charter session record` runs
  without asking permission — that command and no other, and only as a command of its own: your
  own and the project's permission rules all still apply beside it. Codex runs it without asking
  in its default sandbox, which keeps the command from reaching the app, so the command leaves
  the news beside the chat and the chat's own `Stop` hook, which runs outside the sandbox, passes
  it on at the end of the same turn — only for that chat and conversation, and only within the
  five minutes the app waits (#517). Smart close is not offered on a shell tab, a chat never
  prompted, a chat charter has heard nothing from, or one asking you something — answer it
  first. **Close** is the default for a chat that has had at most one turn (ADR 0064, ADR 0039).
- **Session records.** `charter session record --title "…"`, with the record on standard input,
  files a summary of a chat's session — Goal, Done, Decisions, Open, How to resume — in its
  workspace's `sessions/` (the plane's own `sessions/` at the plane root), keeps
  `sessions/index.md` newest first and a `## Sessions` line in `workspace.md` pointing at it,
  and tells the app the chat's record is saved. Which chat, persona, harness, harness profile,
  directory, conversation and pieces a record is about is charter's to say, never the model's.
  `charter session list` and `charter session show` read them back, a chat's briefing names the
  last one, and charter's new `smart-close` skill writes one. A new workspace's `workspace.md`
  has a `## Sessions` section (ADR 0064).
- **Old sessions come back from the window.** Each workspace has a **Sessions** panel — and the
  plane root's tab one of the plane's own — listing its session records newest first, with
  when, persona and harness, and `↻ resumable` on a record that holds a conversation. A row
  opens the record as a read-only tab. **Resume**, on a row's menu and on the record's tab,
  starts a new chat in the record's place, on its harness profile and in its directory — in a
  piece, for a record written from one — given its conversation back (`claude --resume`,
  `codex resume`, `opencode -s`, as a relaunch does) and as its persona where the plane still
  has it, with the record quoted in its briefing. Where the profile or the directory is gone, or
  the record is older than them, Resume uses the project's default profile or the workspace's
  directory and says so. Where the conversation cannot be given — the record holds none, names
  no harness a profile here runs, or the harness can no longer find it — the chat starts fresh
  with the record in its briefing, and says why. The palette has **Open session record: …** and
  **Resume session: …** for the place in front.
- **A memory opens in a tab of its own, and can be edited and deleted from the window.** A
  memory row opens the memory as a preview tab — in italics, and replaced by the next memory you
  click, as VS Code's preview is; a double-click on the tab, or starting an edit, keeps it. The
  tab says which store the memory is in, when it was written and which file it is, and renders
  its body as Markdown. **Edit** turns the same tab into a title field and the raw body; a save
  over a change made on disk since you opened it is refused, with **Reload** and **Overwrite**.
  **Delete** moves the memory to its store's archive and closes its tab, with an **Undo** for a
  few seconds. A row's menu offers Open, Edit and Delete, and a list's search now matches a
  memory's body too, with the matching words shown under the row. Each acts on the file it
  names and no other: `deploy` never reaches `prod-deploy.md` (ADR 0065).
- **A workspace's memory, and the shared memory, in the window — and a new memory from a `+`.**
  The right column has a **Memory** section directly under Todos: the focused workspace's
  memories, newest first, with the same search and the same Open, Edit and Delete as a
  persona's memories. It is not shown at the plane root, which keeps no memory of its own. The
  Personas section ends with a **shared** row saying how many memories every persona shares;
  it opens them as a list in a tab of its own. A `+` on the Memory section, on a persona's tab
  and on the shared list opens a new memory in edit mode, and Save lists it where it was made.
  The palette offers the same: New memory in, for or shared, and Open shared memory (ADR 0065).
- **Edit, archive and unarchive a memory from the command line.** `charter workspace
  edit|archive|unarchive` and `charter persona edit-memory|archive-memory|unarchive-memory
  [--shared]` do what the window does. Edit takes `--title`, and the body as an argument or as
  `-` to read standard input; a slug is the memory's exact name, or the one file whose name
  ends `-<slug>.md`; and an archive that had to number its file prints an Undo that restores
  the original name with `--as` (ADR 0065).

### Changed

- **A short slug that more than one file ends in is refused, and the refusal names them.**
  `charter persona forget` and `charter ws todo done|forget` took the first in sorted order.
  `MEMORY`, the index, is refused as a slug, on a case-insensitive filesystem too.
- **A LIVE workspace publishes its session records** with its memory and todos: the managed
  `.gitignore` block un-ignores `sessions/`, and `charter reinit` brings an older block up to
  date. **The plane root's session records stay on this machine**: `charter init` writes, and
  `charter reinit` adds, `/sessions/` to the plane's `.gitignore`.

### Fixed

- **A memory another memory's title mentions comes back to the index when it is unarchived.** A
  title holding `(b.md)` counted as listing `b.md`, so unarchiving `b` added no index line for
  it. The index now lists only the file each line links, as editing and deleting read it. And
  `optimize --apply`'s index repair can no longer lose a line to an edit saved at the same
  moment (ADR 0065).
- **An older workspace's `notes.md` is indexed even when a memory's title mentions it.** A
  title holding `(notes.md)` stood in for the memo's own index line, so the memo stayed
  unindexed; and that line is now added under the same lock as every other, so an edit saved
  at the same moment cannot drop it.
- **The title bar's look at a plane no longer rewrites git's index.** Asking which files are
  unmerged ran `git diff`, which refreshes `.git/index` under `index.lock` whenever a tracked
  file has been touched without changing — so a save starting at that moment could fail on the
  lock. It now asks `git diff-files`, which only reads.
- **Codex and opencode chats come back to their conversation at a relaunch, and a Claude Code
  chat comes back to the one it was in after `/clear`.** A chat's conversation was recorded
  only when it started, which only Claude Code has, so every Codex and opencode chat came back
  as a new one, and a cleared Claude Code chat came back to the conversation it had cleared
  away. The id the chat's own harness reports through its hook is now written into
  `.charter/app/reopen.json` as it arrives. A harness started inside the chat's shell still
  moves nothing (ADR 0024).
- **Forgetting a memory leaves every other memory's index line alone.** A line of `MEMORY.md`
  whose title mentioned the forgotten memory's file was dropped with it. Only the line whose
  own link names the file goes now, and a title's `\`, `[` and `]` are escaped in the index so
  a title can never read as the end of its own link.

## [0.4.0] - 2026-09-27

0.4.0 is the IDE growing up. The plane root is a tab of its own, first on the workspace strip,
and a **Curate ▸** menu opens a chat for a curation action, in Claude Code or Codex, with its
prompt typed and never sent. Shell tabs are plain shells that warn when a harness is started in
one by hand, every tab strip can be reordered by dragging, a chat's pane has find and takes
Shift+Enter as a new line, and a trackpad scrolls it the way a native terminal does. Vaults,
personas and todos can be created and deleted in the window, charter's skills reach Codex and
opencode chats, and the window's own chrome no longer selects as text. Beyond the window:
cross-repo changes with `charter change`, `charter report` for filing issues, workspace rename,
projects in windows of their own, and opencode chats started by the app.

### Added

- **The plane root is the first tab of the workspace strip.** It is always there, drawn as an
  icon (its tooltip: *Plane — chats here start at the plane root*), and it cannot be dragged,
  pinned, renamed or deleted. Its menu starts a chat or a shell at the plane root, and so do
  `New tab` and `New shell` while it is focused. A chat started there is in no workspace on
  purpose: it is not asked which workspace to use, its briefing lists the plane's workspaces as
  ones it may manage, and `charter` asks it to name one with `-w` rather than acting on one it
  picked (`charter workspace use` does not move it). The Todos box is not offered there, and
  the region says why. It replaces the "Outside every workspace" tab, and the chats it held are
  on it.
- **Anywhere in the plane outside every workspace is the plane root, and says so.** A chat
  started in `docs/`, `.charter/` or any other directory of the plane that is not a
  workspace's is marked as a plane-root chat, like one started in the plane's own directory,
  and is not asked which workspace to use. A terminal session standing there that has not
  chosen a workspace (no `-w`, no `$CHARTER_WORKSPACE`, no `charter workspace use`) is at the
  plane root too: `charter` asks it to name a workspace with `-w` instead of acting on the
  plane's default, which now answers only for a caller outside the plane, and
  `charter workspace use` still moves such a session. A root chat's footer
  (`charter statusline`) shows `⬢ plane root` rather than a workspace; a handoff from it is
  stamped `plane root` (the new chat still starts in the workspace you handed it to); and a
  report back to a root chat that has closed is kept for the next chat started at the plane
  root.
- **Curation actions: chats that open with their prompt typed, for you to read and send.** In
  the app, right-click a workspace, a persona or the plane root for **Curate ▸**, or type
  `curate` in the palette. The chat opens on your default profile as the action's persona,
  named `<action> · <subject>`, and the prompt appears in its input once the harness has
  started — never sent. If you type, paste or click in that chat before the prompt appears, it
  is dropped rather than typed after what you began. An action a persona's file could not
  offer is listed, greyed, with why. It needs a Claude Code or Codex default profile. Codex
  says nothing until your first prompt, so its prompt is typed once its terminal has gone raw
  and then stayed quiet for a second. opencode goes quiet while it is still starting, and a
  prompt pasted then is lost, so charter has no moment to type into it and the menu says so
  (ADR 0061).
- **charter ships three curation actions, and a persona can add its own.** Safe remove,
  Compact & improve, and Add curation action each name a new skill in charter's plugin
  (`safe-remove`, `compact`, `add-curation-action`), so the prompt you read is one line — for
  example *Use charter's safe-remove skill to remove the workspace alpha.* — and the skill
  holds the steps. A persona adds its own as `personas/<name>/curation/<id>.md`, or with
  `charter persona curation add`. `charter curation show workspace:<name>` (or
  `persona:<name>`, or `plane`) lists what a subject is offered, who runs each action, where,
  and the exact prompt. A prompt types a literal brace as `{{` or `}}`, so `{{word}}` types
  `{word}`. Claude Code shows a paste over 800 characters or of 4 lines or more as
  `[Pasted text #N +M lines]`, and Codex one over 1,000 characters as
  `[Pasted Content N chars]`, so an action whose prompt would be shown that way opens no chat
  on that harness and says why, and `charter persona lint` warns about it, naming the harness.
  `charter persona lint` also reports an action that is broken or that takes a built-in's
  name. `charter` is now a reserved persona name, because its actions would read
  `charter/<id>` like the built-ins: `charter persona create charter` is refused, and a persona
  that already has the name is reported by `charter persona lint` and offers no curation
  actions (ADR 0061).
- **Plain shell tabs, and a warning when a harness starts inside one.** `New shell` sits beside
  `New tab` in the palette, on the panes' menu and on each workspace's menu (`New shell in
  <workspace>`), on ⌘⇧T (Ctrl+Shift+T off a Mac): your own shell, where a new chat would start,
  with a terminal's mark on its tab and no chat-state mark. Typing `claude`, `codex` or
  `opencode` in one still starts it, after one line saying it runs outside charter's session
  tracking, and the tab shows a banner whose **Open as chat** opens the picker there with that
  harness picked. Detection is the command being started — charter's shims stand first on a
  shell tab's `PATH`, and stay first after zsh's and bash's own start files — and nothing
  reads what the harness prints (ADR 0062).
- **Drag a tab to reorder it, in every strip.** Projects in the title bar, workspaces and chats
  each take a drag with the pointer, or from the keyboard: Shift+Space picks the focused tab
  up, the arrows move it, Space or Enter drops it and Escape puts it back. Dropping a tab
  across the pinned boundary pins or unpins it, where you dropped it, and a pin the store
  refuses goes back with the reason. A click is still a click: a drag starts only once the
  pointer has moved. Each order is kept on this machine and comes back on relaunch (ADR 0039,
  ADR 0040).
- **Find in a chat's pane, and Shift+Enter for a new line.** ⌘F (Ctrl+Shift+F off a Mac, so
  Ctrl+F stays with the shell) opens a find bar over the focused pane: every match is
  highlighted, Enter and Shift+Enter step through them with a count, and Esc closes it and
  puts the keyboard back in the terminal. In a Claude Code, Codex or opencode chat, Shift+Enter
  now adds a new line to the prompt instead of sending it; in a shell it is Enter, as before.
- **Create and delete vaults, personas and todos in the window.** A `+` on the Vaults and
  Personas headings opens *New vault…* and *New persona* (name, role, the work that comes to it,
  and what it inherits from; it is created as a draft and its tab opens). *Delete vault…* lists
  the secrets the vault holds by name, says plainly that a keychain vault's secrets are
  destroyed and cannot be recovered (a plain-file, references or 1Password vault's file or item
  is left where it is), and stays disabled until you type the vault's name. *Delete persona…*
  runs `charter persona remove`, never forced, so a persona another one extends or uses is
  refused with their names. *Edit persona.md* opens the file in the app your system opens `.md`
  files with. The Todos panel has a box for a new todo in the focused workspace, and each todo's
  menu has *Mark done* and *Forget*; the window refuses a todo with the same rules and words as
  `charter ws todo`.
- **Codex and opencode chats get charter's skills too.** `safe-remove`, `compact`, `handoff`
  and the other six reached only Claude Code chats. An opencode chat the app starts now finds
  them as its own skills, beside any `skills.paths` your `opencode.json` names, and a Codex chat
  is told about each one when it starts, with the file to read, since Codex cannot take a
  skills folder for one session. Nothing is written into `~/.codex` or `~/.config/opencode`
  (ADR 0063).
- **The harness asks you before `charter report` files an issue.** `charter init` now writes an
  ask rule for `charter report *--yes*` in `.claude/settings.json` and `opencode.json`, beside
  the one for `charter handoff`, so a chat cannot file a public report without your yes.
  `charter reinit` adds it to an existing plane and carries it into your workspaces.
  `charter doctor`'s `ask rules` row warns when it is missing, and `charter guard report` or
  `charter doctor --fix` puts it back. So `--fix` now writes the plane's committed harness
  settings too, not only this machine's. Codex has no rule that can say this, so nothing is
  written there (ADR 0059).
- **`charter change` declares a piece of work that spans several repos.** `create` names it
  and says why, `add` puts in a repo already cloned in the workspace (on `change/<slug>` or a
  branch you name, with `--needs` for the repos that must land first), `drop` takes one out
  with the reason, and `list`, `show` and `forget` read and end it. The record is
  `workspaces/<ws>/changes/<slug>.json` and holds intent only; it is committed when the
  workspace is LIVE. An unknown change, a repo with no clone, a repo added twice or an order
  that cannot be true is refused with exit 2. Pushing, landing and reverting come later
  (ADR 0060).
- **`charter doctor` checks cross-repo changes in every workspace.** Its `changes` row fails on
  a change record charter cannot read, naming the file and what is wrong with it, and on a
  change's branch sitting in a clone that is a member of no change. It reads only this disk and
  says which `changes/` directory it could not look at.
- **`charter change show` says where each member's pull request stands.** Under the record it
  prints each member's request number, whether it is open, merged or rejected, and its checks at
  the request's exact head commit: PASSED, FAILED, RUNNING, NOT RUN or UNKNOWN. Zero checks is
  NOT RUN and a check charter could not read is UNKNOWN, and neither is ever shown as passing.
  It says which members still wait on a blocker, and when the reading was taken. If the forge
  cannot be asked, the record still prints and each member says why. Nothing it reads is
  written back.
- **A workspace's cross-repo changes open in a tab.** "Open changes" in the palette (`F2`)
  opens a tab for the focused workspace. It shows each change, each member's branch, its pull
  request and its checks at the head commit, which members are blocked, and when that was read.
  It asks the forge when the tab opens and when you press Refresh, and never when you switch
  workspaces. A workspace with no changes says how to create one.
- **charter's plugin teaches personas, vaults and the browser again.** It now ships the
  `charter:persona`, `charter:secrets` and `charter:browser` skills beside `handoff`,
  `update` and `working-in-a-clone`, rewritten for this charter's commands. `charter browser
  install [--version X.Y.Z]` generates Playwright's own page-driving skill into the plane's
  `.claude/skills/playwright-cli/` with `npx`, and gitignores `.playwright-cli/`, where traces
  of logged-in runs land. The version must be an exact version: anything else npm would read
  there, such as a tag or a git URL, is refused.
  ([#370](https://github.com/diazoxide/charter/issues/370))
- **Worktrees can be cut, declared and removed from the command line.** `charter worktree`
  (alias `wt`): `add <repo> <piece>` cuts a piece off the clone's HEAD and records the claim;
  `done`, and `abandon "<why>"`, run from inside a piece, say it is finished or given up;
  `list` shows each piece with what it declared or how long it has been silent; `history`
  shows what happened to pieces, removed ones included; `remove` takes a piece away through
  git. The session briefing and the footer now see those declarations. In the window, each
  worktree row shows the same word, and its menu can mark it done.
  ([#368](https://github.com/diazoxide/charter/issues/368))
- **A persona's memory can be kept up like a workspace's.** `charter persona forget <name>
  <slug>` deletes one memory, `charter persona dedupe` lists near-duplicate pairs to prune,
  `charter persona optimize` runs the curation `charter workspace optimize` runs over each
  persona's memory and the shared store (read-only unless `--apply`), and `charter persona
  log` notes to, or shows, a persona's activity in this session.
  ([#366](https://github.com/diazoxide/charter/issues/366))
- **A persona can be made, read, cleared and removed from the command line.** `charter persona
  create <name> --delegate-when "<the work that comes to it>"` writes
  `personas/<name>/persona.md` as a draft, with its memory and refs; `--extends` inherits from
  another persona, `--with-vault` registers its vault and `--use` selects it. `charter persona
  show` prints what a persona adopts, `charter persona clear` drops your selection, and
  `charter persona remove` refuses while another persona still extends or uses it (`--force`
  overrides). `charter persona lint` finds dangling `uses:`/`extends:`, keys charter cannot
  read, a missing role, vault or `delegate-when`, and stale generated sub-agents, and `charter
  doctor`'s `personas` and `persona grant` rows now run it instead of saying "not checked".
  A refusal about a missing persona suggests `charter persona create` again.
  ([#365](https://github.com/diazoxide/charter/issues/365))
- **The app starts opencode chats.** Pick an opencode profile in the new-chat picker and the
  chat runs with charter's guard: a tool call the guard refuses does not run, and opencode is
  told why. The chat shows when it is working, when opencode asks your permission, and when its
  turn ends. It also gets the briefing and handed-back reports with your prompt. Nothing is
  written into opencode's configuration for this. An opencode chat reports nothing before your
  first prompt, and it reads waiting after you answer a permission prompt until its turn ends.
  A profile that passes `--pure` would load no plugin, so it is refused with the reason.
  `charter plugin install --harness opencode` installs the guard for opencode chats you start
  in a terminal, and replaces the retired Python charter's opencode plugin if it is there.
  ([#371](https://github.com/diazoxide/charter/issues/371))
- **A workspace can be renamed.** `charter workspace rename <old> <new>` (or `mv`), or *Rename
  workspace…* on the workspace tab's menu and in the palette. The folder moves, every git
  worktree of its clones is repaired so it keeps working, and everything that names the
  workspace follows: its manifest, the LIVE list, the default and each session's choice, the
  app's open tabs and what a relaunch reopens, and your pins. A LIVE workspace is saved once
  afterwards. It is refused while a chat is running in the workspace, naming the chats, and when
  the new name is taken or is not a valid name. Unpushed or uncommitted work is not a reason to
  refuse, because a rename moves it whole. If a rename is interrupted, running the same command
  again finishes it. Claude Code keeps a conversation under the folder it ran in, and charter
  does not move that folder, so `charter workspace rename`, and the Rename dialog before you
  confirm, list by name each Claude Code chat in the workspace that will start a fresh
  conversation. When you reopen one of those chats, it starts a new conversation and says why
  once, instead of failing to resume. Codex and opencode chats resume as before and are not
  listed. ([#367](https://github.com/diazoxide/charter/issues/367))
- **`charter report bug` and `charter report feature` file an issue on charter's own tracker.**
  Each run shows the draft and files nothing. To file it, answer `y` at the prompt in a
  terminal, or run the same command again with `--yes` and the digest the draft printed. If the
  draft has changed since, nothing is sent. The issue is filed under your own `gh` login, never
  under a token from the environment. Before you see the draft, charter removes secrets,
  environment values, your plane's path, home paths and the names of your workspaces, repos,
  personas and vaults, and says what it removed. It also lists possible duplicates.
  `charter report bug --panic` drafts the last panic the app saved, with where it happened and
  the charter version, which panic records now include.
  ([#363](https://github.com/diazoxide/charter/issues/363))
- **A project tab can move into a window of its own, and back.** Right-click a project tab, or
  use the palette, and choose *Move project … to a new window*. Its chats keep running. In that
  window, *Move project … to the main window* brings it back, and so does closing the window.
  Each window has its own palette and project tabs. The ✋ list still shows every chat that needs
  you, whichever window it is in, and pressing one takes you to that window. Quitting warns
  about the chats in every window. The next launch opens each window again.
  ([#126](https://github.com/diazoxide/charter/issues/126))
- **`charter guard` is back: rules that always ask, or stop asking.** `charter guard ask
  '<pattern>'` and `charter guard allow '<pattern>'` write the rule in each harness's own
  file: Claude Code's `.claude/settings.json`, or `.claude/settings.local.json` with
  `--local`, and opencode's `opencode.json`. They touch nothing else in either file, and if
  one of those files cannot be read, they write nothing anywhere. `charter guard handoff` puts
  back the handoff consent rule a plane lost, and `charter guard` on its own lists the rules
  by file. `charter doctor` now checks its `handoff gate` and `ask rules` rows instead of
  saying "not checked". ([#364](https://github.com/diazoxide/charter/issues/364))
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

- **The window's chrome no longer selects as text.** Tab names, headings, buttons, menus and
  rows stay put when you click or drag across them, as a native app's do. What you would copy
  still selects: fields, code, paths, names and commands, a view's answer, error text, and a
  dialog's text. Pulling on a link or an image no longer drags out a ghost of it. A terminal
  selects as it always did.
- **Removing a worktree that holds work now says which work.** The refusal lists the
  uncommitted files and the commits no other branch has, so you can see what `--force` (or
  "Discard that work and remove the worktree anyway", in the window) would discard. A merge
  refused over uncommitted changes no longer tells you to remove or force.
  ([#368](https://github.com/diazoxide/charter/issues/368))
- **The app keeps the plugin you installed for terminal chats up to date.** When it starts,
  it re-runs `charter plugin install` for each harness (Claude Code, Codex, opencode) whose
  installed copy runs the app's own `charter` and is older than what the app ships. It never
  installs for a harness you did not install for, and leaves a copy that runs another
  `charter` alone. ([#449](https://github.com/diazoxide/charter/issues/449))
- **`charter guard ask` puts a new rule into your workspaces straight away.** It rewrites the
  generated settings of every workspace, as a launch or `charter workspace reinit` would,
  and names any workspace whose settings it could not rewrite. Before, the rule reached a
  workspace only after `charter workspace reinit --all`.
  ([#449](https://github.com/diazoxide/charter/issues/449))
- **A vault file that is a symlink is refused.** `charter secret set` and `charter secret rm`
  on a plain-file or reference vault whose file is a link now stop with a message saying so,
  and write nothing. They used to write the secrets to wherever the link pointed. Point the
  vault's `file` at the real path instead.
  ([#429](https://github.com/diazoxide/charter/issues/429))
- **Releases are signed from a protected `release` environment.** The release workflow's
  signing and publishing jobs now run in a GitHub environment that only `main` and `v*` tags
  can reach, and the signing keys live there instead of in the repository. A build started by
  hand from the Actions tab publishes nothing and is now always unsigned for the updater.
  `docs/updating.md` has the setup.
- **`charter news` prints this changelog.** It shows every version of the app, newest first,
  and `charter news --for <version>` shows one, the same notes as the release page and About
  Charter. It used to read the Python charter's news and told every plane it had no update
  baseline. `--since`, `--until` and `--pending` are retired and say what to run instead.
  `charter update` points at `charter news`, and the pin dialog no longer has an empty news
  list.
- **Pinned workspaces stay in the order you pinned them.** The workspace strip draws them in
  that order, and a workspace you pin later goes after the others, until you drag it. Unpinning
  one leaves the rest where they were. Pins from an earlier version keep the order they had.
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

- **A chat started in a workspace is no longer asked which workspace it is in.** The app filed
  it under its workspace and told it nothing, so its briefing asked you to confirm one. Every
  chat the app starts now carries `$CHARTER_WORKSPACE` in a workspace, or
  `$CHARTER_PLANE_ROOT_SESSION=1` at the plane root, and never inherits either from whatever
  launched the app. `charter` also counts a workspace's own directory, `workspaces/<ws>`, as
  being in that workspace, as the window already did.
- **A chat's terminal scrolls as far as your fingers move, from the first pixel.** A trackpad
  or wheel now moves the history one row for every row's height of travel, and a harness in
  full screen — Claude Code's `"tui": "fullscreen"`, opencode — is sent one wheel report per
  row, what is left over carried to the next event. Before, a slow start barely moved a
  full-screen harness (one report per ~50 px) and a flick was cut to one report per event, so
  scrolling felt slow to start and then fast.
- **A program that asks for mouse reports in the default encoding gets them.** A full-screen
  program in a chat's terminal that turns on mouse tracking without SGR reports got no clicks
  and no wheel from the pane; they were dropped. They now reach it, byte for byte, columns and
  rows past 95 included. Claude Code and opencode use SGR reports and were never affected.
  ([#493](https://github.com/diazoxide/charter/issues/493))
- **The prose guards treat a process substitution as the substitution it is.** A `gh` or
  `glab` command that publishes prose, a charter command that persists it, and `charter
  handoff` now refuse `<(…)` and `>(…)` wherever the shell runs them, and zsh's `=(…)`, exactly
  as they refuse `$(…)`. Quoted, or in a heredoc body, they are text and are left alone. The
  refusal names a process substitution rather than calling it a command substitution.
- **Where the shells may disagree about a heredoc, the guards read it both ways.** Inside
  `(( … ))`, `$(( … ))`, `$[ … ]`, `${ … }` or an array subscript, `<<` can be a shift rather
  than a heredoc; and when a heredoc is opened inside `$(…)`, backticks, `<(…)` or `>(…)` and
  the substitution does not close on that line, bash 3.2, bash 5 and zsh can disagree about
  which of the following lines are its body. In both cases the guards now read those lines both
  as commands and as a heredoc body, and never set them aside as a body alone. After such a
  body, or after a heredoc whose delimiter the shells read differently, they no longer set
  aside any later heredoc body either. The leak guard also reads the command inside zsh's
  `=(…)` as a command of its own, as it already did for `<(…)`, and a heredoc opened inside a
  process substitution that closes on the same line is read the way a heredoc inside `$(…)`
  already was.
- **The nightly mutation run finishes again.** Its shards were sized for a test suite half as
  long as today's, so two of them ran out of time. The run now uses smaller shards and a longer
  per-mutant limit, and its slowest test, the plane-root replay, takes about a quarter of the
  time it did because it asks git each distinct question once instead of once per recorded row.
  It also stops reporting slow survivors as timeouts. New tests now cover the extension,
  executor, secrets, save, settings and `CHARTER_*` steering behaviour the run found untested.
  ([#464](https://github.com/diazoxide/charter/issues/464))
- **`charter plugin uninstall --harness codex` no longer leaves Codex's trust record for the
  guard behind** in `[hooks.state]`. A record for a hook of yours that sat after the guard is
  moved to its new position, so Codex does not ask you to trust it again.
  ([#449](https://github.com/diazoxide/charter/issues/449))
- **`charter doctor` quotes every path and your git identity on one line.** A newline, a
  carriage return or a terminal escape in the plane's path, the working directory,
  `$CLAUDE_CONFIG_DIR` or your git `user.name` and `user.email` is shown escaped instead of
  reaching your terminal. ([#449](https://github.com/diazoxide/charter/issues/449))
- **Every guard reads a heredoc the same way, and the way the shell does.** The secret-leak
  guard used a second, narrower reading of where a heredoc starts than the one that decides
  where its body ends, and on some lines the two disagreed, so a command after the heredoc could
  be taken for part of its body. There is now one reading. It also understands delimiters it
  used to miss: one with a blank in it (`<<'A B'`), one in ANSI-C quoting (`<<$'…'`), and one in
  double quotes that holds an escape or a backslash-newline. In a heredoc that expands, a line
  joined to the one before by a trailing backslash no longer ends the body. The lines after a
  heredoc opened inside a `$( … )` or backticks that close on the same line are read as
  commands too, because bash 3.2 and zsh run them. The body of `charter handoff` spelled in
  other letter cases (`CHARTER handoff`) is read as its brief, as it is for the plain spelling.
  ([#359](https://github.com/diazoxide/charter/issues/359))
- **A terminal pane that opens late no longer adds a line when a wide character sits in the
  last column.** If a program had pushed a wide character, such as a CJK character, into the
  last column with wrapping turned off, the catch-up redraw printed that character again. That
  wrapped it onto the next row, and on the bottom row it scrolled the pane by one line. The
  pane now draws the blank that the terminal holds there.
  ([#435](https://github.com/diazoxide/charter/issues/435))
- **A pane that opens late shows what a pane that was open all along shows, around wide
  characters.** When a program deleted, inserted, erased or wrote over half of a wide
  character, such as a CJK character, the open pane blanked it and a pane opened later still
  showed it, until the program redrew that row. The same could happen after a program deleted
  or inserted characters, or moved a row down or up, right after writing the last column: the
  next character landed in the last column in one pane and on the next row in the other. A few
  more cases, such as deleting more characters than the row had left, now come out the same in
  both panes too. ([#441](https://github.com/diazoxide/charter/issues/441))
- **A pane that opens late puts the cursor where a pane that was open all along puts it.** A
  tab, or a restored cursor, right after a program wrote the last column sent the next
  character to the next row in one pane and not in the other. So did inserting or deleting
  lines, which move the cursor to the first column, and moving the cursor up or down inside a
  scroll region, which stops at the region's edge. A restored cursor now also comes back to
  the same row in both panes after the screen scrolled. A pane that opens late now also keeps
  the program's scroll region, so a full-screen program such as an editor or a pager scrolls
  the right rows in it. ([#447](https://github.com/diazoxide/charter/issues/447))
- **A pane that opens late no longer shows a screen you cleared in its scrollback.** After
  `clear`, or after a program deleted lines at the top of the screen or scrolled it up, a pane
  that opened later had the old lines in its scrollback, and a pane open all along did not.
  Now both panes show the same scrollback.
  ([#452](https://github.com/diazoxide/charter/issues/452))
- **The extension and `secrets exec` tests no longer fail on a busy machine.** Extensions
  still get the same time as before: 5 seconds for a view, an action, an event or a command,
  and at a chat's start 2 seconds each and 3 seconds for all of them together; a program
  `secrets exec` runs still gets a quarter of a second to finish after a Ctrl-C. A debug build
  of `charter` now lets the test suite set a different limit, so a test that is not about the
  limit gives a slow machine room, and a test that is about it uses a short limit and a program
  that never answers. A test whose program has to get somewhere before the limit now waits for
  it to get there, and the one `secrets exec` test that replaces its own process runs apart
  from the others. ([#422](https://github.com/diazoxide/charter/issues/422),
  [#465](https://github.com/diazoxide/charter/issues/465))
- **The guards read more of the shell's quoting the way the shell does.** The shared command
  reader behind every guard now decodes ANSI-C quoting (`$'…'`) and bash's `$"…"` strings, and
  drops a backslash-newline line continuation before it reads a word, inside double quotes as
  well as outside. A command substitution split by a line continuation is recognised as one,
  and so is bash 5.3's `${ …; }`. A heredoc delimiter split by a continuation is read as the
  unquoted delimiter it is. A shell named in capitals (`BASH -c`) is recognised when it runs a
  string, as it is on a filesystem that ignores case. A git alias that runs through the shell
  is read with the same rules.
- **A save no longer commits unresolved conflicts.** When git has stopped part-way through a
  merge, rebase, cherry-pick, revert or bisect, or files still have conflicts, every save now
  refuses and stages nothing: `charter save`, the Save button, auto-save and repo saves alike.
  It says which files conflict and the git command that finishes or aborts the operation. The
  Saving tab shows the plane or repo as Blocked until you do, and auto-save waits.
  ([#433](https://github.com/diazoxide/charter/issues/433))
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
- **Charter's private files are read without following a symlink, too.** The fingerprint key,
  both halves of the vault registry, a plain-file vault and its rotation record, a keyring
  vault's key index, and the hook bookkeeping (the session's tool ceiling, the commit-gate
  and memory counters, the sub-agent map and the running-dispatch records) are now refused
  when they are a symlink or reached through one, where they used to be read from wherever
  the link pointed. What each does then:
  - A fingerprint key that is a symlink gives no fingerprint; `secret get` shows only the
    size band.
  - A vault registry, vault file or key index that is a symlink is an error that names it.
  - A session tool ceiling that is a symlink grants nothing, so every tool asks.
  - When a vault registry half is a symlink, the persona tool gate does not auto-allow a
    command, since it cannot tell which files are vaults.
  - Other bookkeeping reads as nothing recorded.
- **A `.charter/` directory that is a symlink is refused for the vault registry and the
  fingerprint key.** Neither is read from nor written to where it points. A `$CHARTER_HOME`
  outside the plane is still used as you set it.
- **`charter reinit` reports a `workspaces/.gitkeep` that is a symlink.** It used to take it
  as present when the link stayed inside the plane or pointed at nothing. Now it is an error,
  as a persona's `.gitkeep` already was.
- **Temp files an older charter left behind are cleaned up.** An older charter killed
  mid-write could leave `reopen.json.writing` or a temp of the profile approval record in a
  plane's `.charter/`, or a `*.writing` temp beside the app's machine store, extension record
  or window layout. Nothing removed them. The app now deletes them when it opens a plane:
  only those exact names, only plain files, and only when they are more than ten minutes old.
  ([#440](https://github.com/diazoxide/charter/issues/440))
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

  *Erratum:* 0.1.0 could remove a worktree from the window but not cut one, and no later
  version can yet. Cut one with `charter worktree add` (since 0.4.0); cutting from the window
  is [#701](https://github.com/diazoxide/charter/issues/701).
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

[Unreleased]: https://github.com/diazoxide/charter/compare/v0.4.2...HEAD
[0.4.2]: https://github.com/diazoxide/charter/releases/tag/v0.4.2
[0.4.1]: https://github.com/diazoxide/charter/releases/tag/v0.4.1
[0.4.0]: https://github.com/diazoxide/charter/releases/tag/v0.4.0
[0.3.0]: https://github.com/diazoxide/charter/releases/tag/v0.3.0
[0.2.0]: https://github.com/diazoxide/charter/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/diazoxide/charter/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/diazoxide/charter/releases/tag/v0.1.0
