# purlis — one desktop app for running tons of harness sessions in parallel

> Moved here from purlis/purlis-plane's `docs/superpowers/specs/2026-09-17-charter-app.md` (at commit `0ae0961d`) by
> [ADR 0044](adr/0044-charter-apps-design-record-lives-in-charter-app.md). The text is unchanged.
> A path it names that is not in this repository — `docs/research/…`, `docs/superpowers/…`,
> `charter/*.py`, an ADR numbered below 0025 — is in
> [purlis/purlis-plane](https://github.com/purlis/purlis-plane/tree/0ae0961d8a6a8e59b48ba43b10d28de8fd87afb7).

**Status:** agreed 2026-09-17, amended 2026-09-18 (decisions 14-17 and the milestones: no Python in the app at any milestone) in a grill between the operator and the `steward` persona
(workspace `ide`), and amended the same day when the operator reordered the priorities (below).
Amended 2026-09-20 with decisions 22-28 and the milestone placement under them: the app had no
way to be given a plane, which this spec never noticed.
The decision and its reasons: `docs/adr/0025-charter-is-rebuilt-as-a-desktop-app-on-a-rust-core.md`.
The evidence: `docs/research/2026-09-17-gui-terminal-embedding.md`.

## The goal

A developer runs dozens of harness sessions (Claude Code, Codex) at once, across workspaces
and repos, and always knows which one needs them. It is one lightweight app on macOS, Linux
and Windows, with no daemon, carrying every purlis concept: the plane, workspaces, personas,
todos, memory, vaults, guards.

> **Amended by [ADR 0082](adr/0082-charter-serves-one-persons-agents-first-and-its-scale-is-a-hot-chat-count-per-ram-class.md)** (FD-1): one person's
> agents first, then teams with the same per-person budgets. "Dozens" becomes targets, not
> measurements: 200 open chats per device and a hot-chat target per RAM class, 50 at the top
> class. "No daemon" was already replaced by ADR 0068.

## Priorities, in order

1. **Development experience.** A change is quick to make, quick to check and pleasant to work on.
2. **Robustness, through standard practice.** Mature, released, widely used tools, the way they
   are meant to be used. Nothing custom where a standard tool exists.
3. **Speed a person can notice.** No lag, no freeze. Speedups nobody can feel never cost 1 or 2.

When two choices conflict, the higher priority wins.

## Why purlis is being rebuilt

- **Development is slow.** The tmux frame is 11.5k lines of plumbing, and tests are 3.3× the
  source.
- **Proof is missing.** Nothing tests end to end that a finished change works in the real app.
- **Reach is limited.** POSIX only; the frame does not run on Windows.
- **Scale hurts.** Watching many sessions in tmux panes is impractical.

## Language

- **App**: the desktop GUI. **Core**: the Rust library the app and the CLI share.
  **`purlis` binary**: the CLI on PATH, called by hooks, scripts and agents.
- **Session**: one harness process in one PTY, owned by the core. **Chat**: a session as the
  UI shows it: its tab, its workspace, its state. **Shell tab**: a chat running the operator's
  own shell, with no harness and no profile (ADR 0062).
- **Run state** (ADR 0076): `queued`, `starting`, `working`, `input-required`, `paused` and
  `hibernated` while a run lives, then `completed`, `failed` or `stopped`, once and for good.
  Every move names one cause: a hook, the harness's protocol, the program's exit, or an act of
  the host, the operator or a policy, and never harness output (`state::run`). A chat's state is
  its current run's, and needs you is a view over its runs and its items. Until the board moves
  onto runs, the sidebar still draws each chat as `running`, `waiting` (on you), `done`,
  `failed` or `unknown` (`state::State`; the move is #791). A shell tab draws no state mark until a harness in it
  reports one: its terminal mark says what it is, and `unknown` there read as a spinner.
- **Python charter**: the current implementation, frozen, and the reference for differential
  tests until it is retired.

## Decisions

### Product

1. **One window.**
   - **Left:** a sidebar listing every workspace with its chats, and each chat's live state.
   - **Top:** a global "needs you" queue in the title bar, plus OS notifications: a button (a
     hand and a count) that opens a list of every chat asking across every project, each with
     **Go** and **✕** (purlis#249). When nothing has asked but a chat that cannot report
     is open (a shell, a harness without purlis's hooks), it is a faint hand with no count
     that names those chats; with neither, nothing is drawn. Ignoring an item lasts until its
     chat asks again (purlis#248).
   - **Center:** tabs and free split panes.
   - **Right:** panels for the focused workspace: repos, branches, CI, todos, personas, and
     the plane's vaults. Amended 2026-09-26 (SI-3): the panels are not read-only any more.
     Each write goes through the core function its CLI command calls, so the window and a
     terminal refuse the same things in the same words:
     - **Todos** are the focused workspace's, and the panel names it. A box at the top records
       one (`purlis ws todo "<text>"`), and a row's menu marks it done (journalled) or
       forgets it (not journalled).
     - **Personas:** the heading's `+` makes one as a draft (`purlis persona create`: name,
       role, delegate-when, inherits-from). A row's menu and the persona's tab open its
       `persona.md` in the operator's own editor, because purlis has no editor for it, and
       delete it (`purlis persona remove`, never forced: a persona another one extends or
       uses is refused).
     - **Vaults:** the heading's `+` makes one. A row's menu and the vault's tab delete one,
       after a dialog that lists its secrets and takes the vault's name typed back. Deleting
       a keychain vault destroys every secret it holds in the keychain, and they cannot be
       recovered. Deleting any other kind of vault leaves its file or its 1Password item
       where it is. `purlis vault remove` only unregisters a vault, as before.
   - **Palette:** the command palette is the primary input, keyboard first.
2. **No TUI.** In the terminal, purlis is the CLI.
3. **Session state comes from hooks only.** A hook calls `charter hook …`, which hands an
   event to the app through a socket the app owns. A harness with no such hook shows
   `unknown`. Amended 2026-09-18, by what M1.3 had to settle to build it:
   - **A socket, and nothing else was needed** — which closes the open question below. The app
     binds it and names it in the environment of every session it starts, beside that chat's
     own number. A hook therefore looks nothing up: no plane read, no discovery, no polling
     loop, and no Tauri plugin beyond the single-instance one already in use.
   - **`failed` is a non-zero exit.** No hook can report it — the process is gone. An exit
     status is the program telling the app directly, so it is not the harness OUTPUT that ADR
     0018 forbids reading. Operator's ruling, 2026-09-18.
   - **A harness may carry only part of the set.** Codex fires no `Notification` at all
     (measured, codex-cli 0.147.0), so a Codex chat can say it is running and it is done and
     can never say it is waiting on you. It shows what it can, and the UI says what it cannot,
     rather than being flattened to `unknown`. Operator's ruling, 2026-09-18.
   - **Which chat a report belongs to is decided by the process id, not by the conversation
     alone.** ADR 0024's C5 (a harness nested in the chat's own shell) and C6 (`/clear`) both
     report a conversation the chat has not seen, and `$CLAUDE_PID` is the whole of what tells
     them apart. The payload and the environment must AGREE on the conversation: using one as
     a fallback for the other is a hole, because the environment holds the OUTER chat's id.
   - **A chat inherits none of that from purlis itself.** purlis may be launched from inside
     a harness session, and a chat that inherited its `CLAUDE_PID` would report the launcher's
     identity as its own.
   - **A harness started by hand in a shell tab is caught by the command that started it, not
     by anything it prints** (ADR 0062). A shell tab — the operator's own `$SHELL`, no harness,
     `New shell` beside `New tab` — has purlis's shims first on its `PATH`, kept first after
     zsh's and bash's own start files. `claude`, `codex` or `opencode` typed there runs
     `purlis shell-guard`, which says the harness runs outside purlis's session tracking,
     tells the app over this socket so the tab shows a banner with **Open as chat**, and then
     runs the real program with the shims off its `PATH`. It moves no chat's state.
4. **A worktree per writing chat.** The goal is that a chat that writes to a repo gets its own
   git worktree — a **piece** — by default. What ships today is narrower:
   - The explorer lists each clone's pieces under it. Picking one makes the next chat start
     there, and a chat's row shows the piece's branch.
   - A chat started in a piece gets the plane's harness layer written into that tree first
     (`start::layered_or_refusal`), or it is refused with the sentence naming what stopped it.
     `unwired` marks a tree the layer is not in yet.
   - Merging back is an explicit action on the piece's row. `merge` lands it locally,
     fast-forward only, into the branch the piece was cut from. `remove` refuses to discard
     uncommitted changes or commits no other ref reaches unless the operator forces it, and
     names the files and commits it would discard.
   - `purlis worktree` (alias `wt`) is the command line for a chat (charter#368): `add` cuts
     a piece and logs `claimed`, `done` and `abandon "<why>"` declare the piece the chat
     stands in, and `list`, `history` and `remove` read and clear them. Each piece's row in
     the window shows what it declared, or how long it has been silent, and its menu can mark
     it done.
   - **A writing chat gets a piece by default** (GL-1): a chat that starts in a repo's clone
     starts on a new branch of its own, named after the chat or `chat-<n>`, unless the picker's
     *start on a new branch* box is cleared. The piece is cut before the chat starts, taken back
     (folder and branch) if the start is refused or fails, and logged `claimed` once it has
     started; the chat's pane then says *On branch `chat-1` in api*. A chat's name that git
     would refuse or read as its own (`HEAD`, `*_HEAD`, a sha) falls back to `chat-<n>`, so the
     name never costs the start. What the window says of it is said of a branch and its folder.
   - **New branch** on a repo's row and in the palette cuts a piece from the window, under the
     name typed or purlis's `chat-<n>`, and makes it where new chats start (`worktree_add`).
   - **Not shipped yet:** `publish`.

   Neither `merge` nor `publish` takes `--all` (ADR 0020). Git is the only registry, reached
   through the git binary: **ADR 0027**. The design is purlis-plane's
   [`docs/superpowers/specs/2026-09-18-worktree-per-chat-design.md`](https://github.com/purlis/purlis-plane/blob/cli-final/docs/superpowers/specs/2026-09-18-worktree-per-chat-design.md).
5. **Lifecycle.** Closing the window hides the app to the tray. Quitting warns if a session is
   mid-turn and then ends every session. On the next launch, when anything was open, the window
   asks once — reopen every session, or start fresh — naming how many chats in which projects,
   and starts nothing before the answer. Reopening resumes each chat through its harness's own
   resume, on the conversation it was in when it was last recorded — which its own harness's
   hook keeps current, so a Codex or opencode chat past its first turn and a Claude Code chat
   after `/clear` come back where they were (Q10); Esc or closing the question reopens
   (purlis#250).
6. **Harnesses:** Claude Code, Codex and opencode (opencode since charter#371, ADR 0058).
7. **Remote sessions are not in v1.** Sessions sit behind one interface (spawn, read/write
   bytes, resize, exit) with local PTY as the first implementation, so SSH or devcontainers
   can be added later without a redesign.
8. **Extension points are not in v1.** The tmux frame's component/action seam is redesigned
   for the app later, not carried over.

### Architecture

9. **A Rust core owns every PTY and every terminal's state, headless**, using
   `alacritty_terminal` behind an engine trait. No second engine is built now. This holds
   because web UIs are capped at 16 WebGL contexts (research §5.1).
10. **The UI is Tauri 2 + React + TypeScript, and xterm.js draws only the panes on screen.**
    When a pane becomes visible, the core sends a snapshot of its screen, then streams its
    output. This is the VS Code reconnection pattern, not a hand-written renderer. GPUI is the
    fallback only if the M0 skeleton misses a limit below.
11. **Typed IPC.** Commands and events between core and UI are generated from Rust types
    (`tauri-specta`). No JSON shapes are written by hand on either side.
12. **Nothing in the core parses harness output to decide anything** (ADR 0018, carried over).

### Migration

13. **The plane format does not change.** `docs/plane-format.md` records every file and field
    the Python charter reads or writes (`charter.toml`, `charter.local.toml`, `personas/`,
    `workspaces/*/workspace.md|json`, memory files, `.charter/` state) and marks each one stable
    or internal. Fixture planes are generated from it, and both implementations test against
    them.
14. **The app never calls Python, at any milestone.** Amended 2026-09-18 on the operator's
    instruction: "final app should not use python, fully clean implementation in rust — no need
    to mix languages". The first plan had the app shelling out to Python `purlis` for writes
    and hooks until M3. It does not: whatever a feature needs is ported to Rust **before** the
    feature that needs it, so no shipped path ever crosses languages, and no scaffolding is
    written that only exists to be deleted.
15. **Python is the oracle, not a dependency.** Every ported module passes a **differential
    test**: the same fixture plane and the same input give the same output and the same
    resulting plane in both implementations. That runs in CI, where Python is a test fixture —
    it is never in the app, the `purlis` binary, or an installer.
16. **Security-critical parts are ported with the most care, not last:** hooks guard,
    gitpolicy, vaults and secrets each need the differential proof plus an external review
    before the Rust one answers for real. Ordering follows what a milestone needs; nothing
    ships on a Python fallback in the meantime.
17. **Python charter is frozen.** It keeps running as today's product until cutover, and gets
    bug fixes and security fixes only. New feature ideas go to the `purlis` backlog.

### Engineering

18. **Repository:** a new repo, `charter-app`, which takes over the `charter` name at M4. The
    product is still called purlis. This repo stays the Python implementation and the plane.
19. **Standard tooling, enforced in CI:**
    - **Rust:** a stable toolchain pinned in `rust-toolchain.toml` (#888), `rustfmt`,
      `clippy -D warnings`, `cargo-deny` (licences and advisories).
    - **TypeScript:** `strict`, ESLint, Prettier.
    - **Dependencies:** Dependabot (built into GitHub, nothing to install).
    - **Tests:** Vitest for UI units, `cargo test` for the core, and scenario tests through
      Tauri's WebdriverIO service.
    - **Releases:** the Tauri updater with signed artifacts.
20. **Tests:**
    - **Main gate:** scenario tests driving the real app against a **fake harness binary** that
      replays recorded output and fires recorded hooks.
    - **Unit tests** stay small.
    - **Mutation testing** runs nightly on the core crate alone, with `cargo-mutants --in-diff`,
      sharded. It never blocks a PR.
21. **Distribution:** one signed installer per OS. It installs the app and puts `purlis` on
    PATH. The Claude Code plugin keeps calling `charter hook …`. A final PyPI release points to
    the new install.

### Multi-plane — added 2026-09-20

**What this spec missed.** Decisions 1-21 never say where the app's plane comes from. In the
tmux frame it came from the shell that ran `purlis`, and ADR 0025 carried that assumption into
an app that has no shell: purlis resolves its plane from `std::env::current_dir()` and from
nothing else, so a double-clicked `.app` — working directory `/` — resolves none, shows
`No plane: …` and offers no way to give it one. **The app has only ever been usable when
launched from a terminal standing inside a plane.** The operator found this by opening it on
2026-09-20. There was no picker, no opener and no multi-plane anything in this spec, and there
is no ADR that decided against them; they were simply not seen.

These decisions continue the numbering rather than joining 1-8, because a decision's number is
how it is cited and nothing here is renumbered.

22. **A project is a plane.** The top-level switcher is a plane switcher, and "Open project…"
    opens a directory that has, or will get, a `charter.toml`. There is no second container:
    ADR 0007 removed the second plane shape so that no code would ask which shape it was in.
    **ADR 0033.**
23. **One window per plane, and a window may hold several.** Planes are top-level tabs in a
    window, merged into one window or split back out — Zed's project tabs, named by the
    operator. `tauri-plugin-single-instance` stays exactly as it is: one process, many windows.
    A second launch hands its plane to the running process, which raises the window holding it
    or opens one. Two processes on one plane would be two sets of sessions, two writers of that
    plane's `.charter/app/reopen.json`, two hook sockets and a Quit that ends half the work.
    **ADR 0033.**
24. **cwd is a first-launch hint, never ongoing truth.** Launched from a terminal inside a plane
    → open that plane. Launched any other way → the opener. Once a window has a plane, that
    plane is explicit and the working directory is never consulted again. The CLI keeps its own
    resolution untouched. M2.16 was exactly the cost of two resolvers disagreeing (`resolve`
    against `command_root`), and the app has the same split live today: `plane_root` asks
    `plane::resolve`, which honours `$PURLIS_ROOT`; `setup` asks `plane::find_root`, which does
    not. **ADR 0034.**
25. **purlis keeps a machine-level record outside every plane**, under the OS application-data
    directory: the planes on this machine, when each was last opened, the window arrangement,
    and the trust decisions of decision 26. `0600` where the OS has modes, gated, tolerant of
    paths that have moved or gone, and **never plane content** — deleting it must cost the
    arrangement and the approvals and nothing else. This is the second such file, not the first:
    `purlis report`'s consent has lived under the user's config home since ADR 0003.
    **ADR 0034.**
26. **A plane is untrusted until the operator opens it once and approves it.** A plane's
    committed `.claude/settings.json` travels — `layer.rs`'s `WORKSPACE_KEYS = ["enabledPlugins",
    "env"]` — into every directory a harness reads config from, so a stranger's plane chooses
    your plugins and sets your environment; and opening a plane starts programs out of its
    `.charter/app/reopen.json`. First open shows what the plane will contribute and asks, before
    the reopen runs; the answer is a fingerprint of what was approved, stored per decision 25.
    Two limits already exist and stay named: `permissions` travels only as `ask`/`deny`, never
    `allow` (`RESTRICTIVE`), and ADR 0022 keeps harness profiles out of the committed file.
    **ADR 0035.**
27. **`purlis init` on an existing repo adopts that repo as the plane's first clone by
    default**, making the plane beside it rather than writing plane scaffolding and `.gitignore`
    rules into somebody else's repository. "Make this repo itself the plane" stays available and
    becomes the non-default — it is how purlis's own plane exists. This **diverges from the
    Python oracle** (decision 15) on a command that is already ported; Python is frozen
    (decision 17), so the `init`-inside-a-repo differential scenario records an intended
    difference rather than being normalised. **ADR 0035.**
28. **Cold launch restores the window set from the last quit** — same planes, same tabs, same
    merges. Each plane's own `.charter/app/reopen.json` still restores that plane's chats and is
    unchanged; the two records are separate because an arrangement spans planes and a plane's
    chats travel with the plane. A plane that has moved or gone is dropped with a line saying
    so, never an error dialog. `--no-restore` starts clean. **ADR 0033.**
29. **A strip's order is the operator's.** Nothing reorders a strip by itself; the operator can
    drag a tab along its own strip, with the pointer or the keyboard, and a drop across the
    pinned tabs' boundary pins or unpins it. Each order is kept where that strip's arrangement
    already was, on this machine and never committed: projects in the window arrangement of
    decision 25, workspace pins in the same store, chats in the plane's own
    `.charter/app/reopen.json`. **ADR 0039, as amended 2026-09-26.**
30. **The plane root is the workspace strip's first tab, and a chat knows where it was
    started.** The root tab is always drawn, drawn as an icon whose tooltip is *"Plane — chats
    here start at the plane root"*, and it cannot be dragged, pinned, renamed or deleted. Chats
    and shells started from it start in the plane's own directory. It is not a workspace: it
    has no purlis, memory or todos, and the panels that are a workspace's say so while it is
    focused. Every chat the app starts is told where it started, in its environment:
    `$PURLIS_WORKSPACE=<name>` in a workspace, `$PURLIS_PLANE_ROOT_SESSION=1` at the plane
    root, neither anywhere else — so no chat the app started is asked which workspace it is in.
    A plane-root chat is in no workspace: `purlis` refuses a command that needs one unless it
    is named with `-w`, `purlis workspace use` does not move it, and its briefing lists the
    plane's workspaces as ones it may manage. Operator's rulings, 2026-09-26 (SI-1).
    **The plane root is anywhere in the plane outside every workspace** — `docs/` as much as
    the plane's own directory — and the app marks a chat it starts there the same way. A
    session standing there that nothing has chosen a workspace for (no `-w`, no
    `$PURLIS_WORKSPACE`, no session or terminal pointer) is at the plane root too, even with
    `workspaces/.default` or `[workspace] default` set: those answer for a caller outside the
    plane, not for one standing in it; unlike an app-started root chat, `purlis workspace use`
    still moves it. A root chat's handoff is stamped `plane root`, not a workspace, and the chat
    it opens starts in the workspace the brief names; a report back to a root chat that has
    closed is kept for the plane root. Its footer names the plane root. Operator's ruling,
    2026-09-26 (SI-1b).

### Curation actions — added 2026-09-26

31. **A curation action is a chat opened with its prompt typed and never sent.** The operator
    reviews the prompt and presses Enter, and nothing can opt out of that. It is offered on a
    workspace, a persona or the plane. purlis's own three come first (`charter/safe-remove`,
    `charter/compact` — "Compact & improve" — and `charter/add-curation-action`), ship inside the
    binary and cannot be overridden; then each persona's, declared one file per action at
    `personas/<persona>/curation/<id>.md` and run by that persona. A template has four
    variables and no expansion of anything else. A persona's file that is broken, or that takes
    a built-in's id or label, is left out with a warning and fails `purlis persona lint`.
    purlis's own run as the persona being curated, or else as the plane's default persona, or
    as no persona. The core resolves a subject's list (`purlis curation show <subject>`), and
    `purlis persona curation list|add|remove` manages a persona's files. In the app, a
    workspace's, a persona's and the plane root tab's right-click menu has a "Curate ▸" submenu — purlis's own, then
    a group per declaring persona, then every action left out, disabled, with the reason — and
    the palette lists each as `Curate <subject>: <label>`. Choosing one opens a new chat on the
    project's default profile, as the action's runner, where it runs, named `<label> ·
    <subject>`, and holds its prompt in the app until that chat's first `SessionStart` hook
    report; the prompt is then written once the terminal hands keys to the harness, as one
    bracketed paste with nothing after it. Codex, which reports its start only at the first
    prompt, is typed into instead once its terminal is raw and has then written nothing for a
    second — the fact of bytes arriving, never their content — within 15 seconds or not at all;
    a prompt over the 1,000 characters Codex draws whole is refused. opencode, which goes quiet
    while still starting and loses a paste then, is refused. Amended 2026-09-27, pending the
    operator's ruling (Q25). **A prompt is read whole before Enter** (operator's rulings Q28 and
    Q29, 2026-09-27): purlis's own three are one plain line each that names its skill and its
    subject — the skill holds the steps. A prompt a harness would draw as a placeholder
    (Claude Code 2.1.283 over 800 characters or at 4 lines, Codex 0.147.0 over 1,000
    characters; `Harness::longest_paste_drawn_whole`) opens no chat on that harness, and
    `purlis persona lint` warns about a persona's that would, rendered for a long subject name,
    naming the harness. A prompt still waiting is dropped the moment the operator sends that
    chat any input of their own; the terminal's answers to the harness's questions are not the
    operator's. **ADR 0061.**

### Session records — added 2026-09-28

32. **Smart close: a chat writes its session record, then closes.** A session record is a
    summary a chat writes of its own session, never the transcript: one Markdown file in
    `workspaces/<ws>/sessions/`, or the plane's own `sessions/` for a chat at the plane root,
    whose body is exactly Goal, Done, Decisions, Open and How to resume, and whose frontmatter
    is purlis's alone — the chat, its persona, harness and conversation, the place, and the
    pieces git reports. `purlis session record` is the only writer: it holds the record to
    that shape, writes it whole, rebuilds `sessions/index.md` (newest first) and
    `workspace.md`'s one `## Sessions` line, and tells the app over the hook socket which chat
    saved which record (`SessionSaved`, a fourth kind of line that is never a report or an
    ask). A workspace's records follow it LIVE or LOCAL; the plane root's stay on this machine.
    The next chat's briefing names the place's newest record in one line, its title quoted as
    data. **Smart close sends its prompt** — the operator's click is the consent, the one
    exception to decision 31 — and the tab closes only when the record is saved, never on
    anything the chat printed; with no record in about five minutes the tab goes back to normal
    and stays open. `purlis session list|show` read them back, and purlis's `smart-close`
    skill is the procedure. **In the window**, closing a chat asks Cancel, Close or Smart close;
    Smart close is sent (one bracketed paste and Enter, in one write) at once to a waiting chat
    and at the next `Stop` to a running one, and is refused to a chat asking a question
    mid-turn — a `Notification` while a turn runs, which the board now tells from the nudge of
    an idle chat — and offered to no shell tab, no chat never prompted and none reporting
    nothing. Close is the default at one turn or fewer. The tab wears a breathing amber mark
    while it wraps up; its menu's Cancel smart close, or the operator typing into the chat
    (not the terminal's own answers, not the mouse, and not an answer to the chat's own question)
    cancels it. A chat that ends on its own mid-close is an ended chat whose record was not
    written. **ADR 0064.** **The operator can always get an old session back**
    (SI-8d): each workspace has a **Sessions** panel, and the plane root's tab one of the
    plane's own, listing its records newest first; a row opens the record as a read-only view
    tab, and **Resume** — on the row's menu, the record's tab and the palette — starts a NEW
    chat in the record's place, on its harness, given its conversation through the relaunch's
    one argument builder, as its persona where the plane still has it, with the record quoted
    as data in its session-start briefing (`$PURLIS_RESUMING_RECORD`). A record with no
    conversation, or on a harness no profile here runs, starts fresh with the record and says
    why; a harness that can no longer find the conversation says so by its program failing
    before it reported anything, and the same record is then started fresh, once, and says so.
    **Finishing it** (SI-8e): a record also names the harness **profile** and the directory
    (**cwd**, plane-relative) the chat ran in, from the app's own record of the chat, and Resume
    starts on that profile where this machine still has it and in that directory where it is
    still a directory inside the record's place — otherwise on the old guess and in the place's
    own directory, and it says which. A Claude Code chat the app starts carries
    `Bash(purlis session record *)` as an `allow`, so a Smart close never stops to ask for
    the command that ends it, and, since V79, an `allow` for each of purlis's five read-only
    MCP tools (`todo_list`, `memory_search`, `session_record_list`, `session_record_read`,
    `change_status`) and for `persona_where` (#1450), while its writes and `ask_operator`
    still ask. Codex's approval and
    sandbox are whole-session switches, so it carries none (ADR 0064's measurements). A record's tab resumes that record whichever place is
    in front. **Smart close puts the chat into the background** (SI-8f): its tab shrinks to a
    fixed chip — the chat's icon and the breathing mark, the name in its tooltip — at the chat
    strip's left edge before the pinned tabs, and the front goes exactly where Close would send
    it. The chip, clicked, shows the chat working. Saved, it goes and a quiet notice offers
    **Open record**; given up, ended or refused, the tab comes back in its old place and the
    title bar's needs-you list says why. A line the harness's sandbox kept from the hook socket
    (Codex's default one) is left beside the chat and passed on by that chat's next `Stop` hook,
    only for that chat, that conversation and within the five minutes the app waits
    (`sessionrecord::relay`); the app closes on it once.

### Memory in the window — added 2026-09-28

33. **A memory opens in a view tab, and the window's Delete is archive.** A memory row opens a
    preview tab (a single click replaces it; a double-click or an edit pins it) with the title,
    the store (a persona, `shared` or the workspace), the stamp and the path above the body
    rendered as Markdown, and Edit and Delete; Edit flips the same tab into a title field and
    the raw body. It replaces the memory row's popover. Workspace memory is a Memory section
    under Todos for the focused workspace; shared memory is one row in Personas that opens its
    own list; each has a `+` that writes through `remember`. **An edit is in place**: the
    filename and the stamp are kept, the index line retitled, and a save over a file that
    changed since it was read is refused, offering Reload or Overwrite. **Delete moves the file
    to `archive/` with an Undo**; a hard delete stays `forget`, on the command line only. The
    command line has each operation too: `purlis workspace edit|archive|unarchive` and
    `purlis persona edit-memory|archive-memory|unarchive-memory [--shared]`. **A memory moves
    between scopes** (KN-3): a memory's tab has a Move to choice and a Move button, and the
    command line has `purlis workspace move-memory` and `purlis persona move-memory`, each
    with `--to-workspace`, `--to-persona` or `--to-shared`. The file is renamed whole, its title
    and stamp kept (a journal name moved away and back comes back to the minute); a target that
    already holds a memory of that name is refused. Browsing the archive is later work. **ADR
    0065.**

### The light editor — added 2026-10-02

34. **Any file of a worktree opens in the light editor, read only.** On screen a worktree is
    its **branch**, and its directory the branch's **folder** (#989). A branch's row in the
    explorer offers *Browse the files of*, followed by its folder's name: a view tab with the
    branch as the explorer's tree, read a folder at a time, beside a preview of the file picked,
    with *Open in a tab of its own* for a tab of that file alone. The divider between them is
    dragged or moved with the arrow keys and kept with the tab, across a close and a relaunch
    (FM-2). The window names the branch's folder, never a path. The core opens only a
    file that list offers (an ignored file, such as a `.env`, does not open by name), and
    refuses a path with a `.git` component or one that resolves, through a link, outside the
    worktree or to a file the list does not offer. An image is drawn as an image and markdown
    rendered, with its source a press away; a binary file, or one past 2 MiB, is said in a
    sentence rather than drawn. The light editor is CodeMirror 6, with purlis's fixed set of
    grammars and the theme's colours; a diff is drawn in its merge view, which marks only the
    lines git reported and finds the changed words inside them. Editing (RC-10) and the Review
    tab (RC-4) come after. **ADR 0081, ADR 0084.**
35. **A file of a worktree opens in your editor at a line** (RC-20). Beside a file the light
    editor shows is *Open in your editor at line N*, the line the cursor is on, and it is
    offered too for a file past the light editor's size. Your editor is chosen in
    Settings, at the You level, and kept in the layout file: VS Code, Zed and a JetBrains IDE are handed
    the file through their own `vscode://`, `zed://` and `idea://` links, and `$VISUAL`, else
    `$EDITOR`, is started as a program with `+line` and the file as arguments, never through a
    shell. The core checks the path exactly as it checks a read, so a file the light editor
    would refuse is never handed on. With no editor chosen, the button asks for one.
    **ADR 0081 §3.**

## Limits (acceptance)

Only what a person would notice. Each limit after *Open chats* and *Hot chats*, which are
ADR 0082's targets, is also a row of the *Performance budgets of record* below, which says which
job measures it ([ADR 0086](adr/0086-every-performance-budget-names-the-job-that-measures-it-and-ci-holds-charters-own-cost-by-regression.md)).

| | Limit |
| --- | --- |
| Open chats | **Target, not measured:** 200 per device, in every RAM class. This is the product's scale, not a speed target ([ADR 0082](adr/0082-charter-serves-one-persons-agents-first-and-its-scale-is-a-hot-chat-count-per-ram-class.md), which amends "Live sessions: 50") |
| Hot chats | **Target, not measured:** per RAM class (ADR 0082 §3), **50** at the top class. The limits below hold at the device's hot target |
| 2 MB and 13 MB output bursts | the UI never freezes; input and other panes stay responsive |
| Keystroke to screen | ≤ 50 ms while the device's hot target minus one other chats stream: 49 at the top class (ADR 0082) |
| Tab or pane switch | ≤ 100 ms |
| Project switch, among 10 open projects | ≤ 200 ms at the p95 (FR-27) |
| Hook call (`charter hook …`) | ≤ 50 ms |
| Cold start | ≤ 2 s |
| Idle hidden session | ≤ 50 MB, with scrollback at the shipped cap |
| Synchronized-output animation (`?2026`) | smooth, ≥ 30 fps (xterm.js#6071 is why this is listed) |

tmux's end-to-end throughput is re-measured in M0 on the same machine and recorded as a
**reference**, not a gate.

## Performance budgets of record

**The table of record for every performance budget** ([ADR 0086](adr/0086-every-performance-budget-names-the-job-that-measures-it-and-ci-holds-charters-own-cost-by-regression.md)).
Each row is a target, not a promise, and none is published until SC-8 and a release measurement
confirm it (D-0082b). A row is added in the pull request that adds its job.

- **Kinds.** *CI absolute*: going past it fails its job. It is for counts and sizes, and for the
  few timings whose budget is many times their median (L5, L6, T1, T2, E1 and ADR 0079's rows),
  always as a median. *CI relative*: through github-action-benchmark, a pull request fails its job
  when a row is more than 20% worse than the last value `main` recorded on the `benchmarks`
  branch; each value is a median of the job's own samples, and a row gates only once its first
  five green runs on `main` spread by at most about 7% (a starting value), being evidence before
  that. **The `bench` job's rows are held by V62 instead** (ADR 0086 as amended for SC-16): the
  pull request against main built in the same job, in alternating rounds, failing only on a
  sustained, repeated slowdown above 20%. The job is evidence only until five main runs show a
  tight spread (#932). github-action-benchmark keeps main's history on `benchmarks` for the
  graphs. *Release absolute*: measured with real harnesses on the operator's machine each release
  by the release scale run (#814); a miss is a bug filed before the release notes, not a block
  (ADR 0082 §3 and §4).
- **Load.** Every row is stated at the device's hot target ([ADR 0082](adr/0082-charter-serves-one-persons-agents-first-and-its-scale-is-a-hot-chat-count-per-ram-class.md) §3)
  with the rest of the 200 open chats hibernated, unless it says otherwise. CI measures the top
  class (50 hot, 150 hibernated) with the fake harness. Counts and sizes are a base plus a cost
  per hot chat and per hibernated chat, so each RAM class's total is arithmetic (ADR 0086 §3).
- **Jobs.** `stress` (`stress.yml`, required on macOS and Linux once SC-8 lands), `bench` (SC-16's
  job in `ci.yml`), `app builds` and `rust` (`ci.yml`), and the release scale run (#814).
- **Last measured** is filled in by the release scale run in its release PR. *—* means not yet.

| # | What | Budget | Kind and job | Owner | Last measured |
|---|---|---|---|---|---|
| M1 | purlis's own base: web content peak plus native side, at 200 open, scrollback excluded | ≤ 2 GB (ADR 0082's base; it binds over M2 to M5) | release absolute | SC-1, #814 | about 1.8 GB (ADR 0082) |
| M2 | web content process, peak, at the hot target | ≤ 1.5 GB. **At risk, expected to miss** | release absolute; CI relative (`stress`) | SC-1, #814 | 1,529 MB at 12 chats, not at the hot target |
| M3 | native side (app and `purlisd`), no chats | ≤ 384 MB | CI absolute and relative (`stress`) | SC-8 | 183 MB macOS, 311 MB Ubuntu |
| M4 | native cost of a hot chat, scrollback excluded | ≤ 4 MB | CI absolute (`stress`) | SC-8 | 3.1 MB macOS, 0.12 MB Ubuntu |
| M5 | native cost of a hibernated chat; no harness process | ≤ 1 MB | CI absolute (`stress`) | SC-4, SC-8 | — |
| M6 | idle hidden session's scrollback at the cap | ≤ 50 MB | release absolute; CI relative (`bench`) is #931 | SC-16 | 20.2 MB |
| M7 | no leak: after round 1, each round's close within 32 MB of round 1's, threads within 40 of the base | passes today | CI absolute (`stress`; threads asserted today, memory added by SC-8) | SC-8 | macOS 334.6 MB after round 1, flat in rounds 2 and 3 (base 182.6 MB); Ubuntu 321.7 MB after round 3; threads at 45 |
| C1 | threads | ≤ 64 at none; ≤ 5 a hot chat; 0 a hibernated | CI absolute (`stress`) | SC-8 | 49 / 24 at none; 3.9 / 4.0 a chat |
| C2 | descriptors | ≤ 64 at none; ≤ 4 a hot chat; 0 a hibernated | CI absolute (`stress`, macOS with SC-15) | SC-8, SC-15 | 54 at none, 3 a chat (Ubuntu) |
| C3 | open-file limit raised to `min(hard, OPEN_MAX)`; 200 fake chats in a launchd-started app | met | CI absolute (`stress`, macOS) | SC-15 | — |
| C4 | hook processes spawned per second | recorded | evidence only (`stress`) | SC-8 | — |
| E1 | idle, window hidden, 3 projects, no chats | ≤ 0.5% of a core, ≤ 1 wakeup/s | CI absolute on Linux (`stress`); release absolute on macOS | SC-18 | — |
| E2 | idle, window hidden, the hot target's chats at a turn boundary, harnesses excluded | ≤ 1% of a core | CI relative (`stress`); release absolute | SC-18 | — |
| L1 | keystroke to screen, hot target minus one streaming | ≤ 50 ms | release absolute; CI relative (`bench`, evidence only until #932): the session layer's half, a keystroke under ten flooding panes over `charterd.sock`, no window; the window's half in CI is #931 | SC-16 | worst 26 ms |
| L2 | tab or pane switch | ≤ 100 ms | release absolute; CI relative (`bench`) is #931 | SC-16 | worst 48 ms |
| L3 | 2 MB and 13 MB bursts | no freeze; input and other panes responsive | release absolute; CI relative (`bench`, evidence only until #932): the session layer's half, each burst asked to drawn over `charterd.sock`, no window; the window's half in CI is #931 | SC-16 | longest frame 42 / 52 ms |
| L4 | synchronized-output animation | ≥ 30 fps | release absolute | SC-16 | 52.4 and 52.0 draws/s against a 60 fps display |
| L5 | hook call p95, at 50,000 memories and the hot target | ≤ 50 ms | CI absolute (`stress`) | KN-22 | not yet measured |
| L6 | cold start to the first frame, no chats | ≤ 2 s | CI absolute on Linux (`app builds`, median of five; at most one past a 2.5 s ceiling, reported and not gated, ADR 0086 as amended); release absolute on macOS | FR-8 | 370 ms macOS |
| L7 | reattach with `purlisd` up: first paint of the focused pane | ≤ 1 s | CI relative (`bench`); release absolute | FD-5, FD-7 | — |
| L8 | relaunch with the hot target's chats to put back: interactive | ≤ 3 s | CI relative (`bench`); release absolute | SC-20 | — |
| L9 | project switch among 10 open projects, one chat each: the press of the switcher's row to the paint of that project's chat, p95 of 30 | ≤ 200 ms | release absolute; CI relative (`bench`) is #875 and #931; evidence only (`scenario tests`, macOS and Linux) | FR-27 | — |
| T1 | event log throughput | ≥ 1,000 events/s sustained, L5 inside its budget | CI absolute (`stress`) | FD-9 | — |
| T2 | audit throughput and group commit | 1,000 entries/s; ≤ 100 ms between commits (ADR 0075) | CI absolute (`stress`) | AU-3 | — |
| G1 | git standing at 300,000 files: git processes in one repo at once while the four pollers (auto-save's look, the title bar, the Saving tab, the alerts) read it; eight reads against one | 1 at a time; eight reads cost no more git processes than one standing, and one `status` | CI absolute (`stress`, *shared standing at 300,000 files*) | FD-11 | — |
| G2 | ⌘P on a generated repo of 100,000 and 300,000 files (a branch is listed up to 200,000): each keystroke's core match, and a palette session's first find (the listing plus a match) | keystroke median and worst ≤ 50 ms (L1's core half); first find ≤ 3 s | release absolute (by hand on the operator's machine: `search_is_measured_on_large_repos`); evidence only in CI (`stress`, *search at scale*, macOS and Linux, V70): each miss reported in the job summary, never failed (ADR 0086, amended 2026-10-04) | FM-7, FM-12 | macOS, loaded (#1115): keystroke median 2.9–3.7 ms, worst 17 ms at 100,000; 5.5–7.7 ms, worst 25 ms at 300,000. First find 1.7 s cold, 0.19 s warm at 100,000; 2.4 s cold, 0.53 s warm at 300,000 |
| G3 | ⌘⇧F on the same repos: the first file of hits for a common query | ≤ 1 s | as G2 | FM-8, FM-12 | macOS, loaded (#1115): 0.84 s cold, 0.19 s warm at 100,000; 2.5 s cold, 0.57 s warm at 300,000. A query found nowhere fills no page within its 10 s: the whole scan took 61–93 s at 100,000 and 115–131 s at 300,000 |
| G4 | a branch's status through the reader child, on the same repos | ≤ 30 s, the reader's deadline | as G2 | FM-4, FM-12, RC-2 | macOS, loaded (#1115): 7.4–24 s at 100,000; 14–23 s at 300,000. Read by RC-2's engine (#704), warm at 100,000: clean 0.25–0.56 s, six changes 0.25–0.28 s |
| G5 | a comparison through the reader child (RC-2), on the 100,000-file repo: a branch that changed 1,000 files against its base, the file list with its line counts; and one file's hunks | list ≤ 30 s, the reader's deadline; R3's 1,000-file first paint under 1 s is RC-6's, after RC-lite (W7) | as G2 | RC-2 | macOS, loaded (#704), two runs: list 0.37–0.51 s, `git diff --numstat` on the same range 0.15–0.22 s; a file's hunks 8–13 ms median, 13–29 ms worst |
| D1 | bytes per event, per audit entry, uncompressed | ≤ 1 KB; ≤ 512 B | CI absolute (`stress`) | FD-9, AU-3 | — |
| D2 | disk written over a busy day at the top class (50 hot, 0.3 tool calls/s, 8 h) | ≤ 100 MB compressed | CI absolute (`stress`, computed) | FD-9 | — |
| D3 | a hibernated chat's scrollback snapshot | ≤ 1 MB on disk | CI absolute (`stress`) | SC-4 | — |
| D4 | every growing store bounded: audit 1 year / 2 GiB; event log as FD-24 states (ADR 0066); sessions, traces, reports by SC-7; indexes rebuildable | each prune tested | CI absolute (`rust`) | FD-24, SC-7 | — |
| K1 | context tax at a chat's start, per harness, on the 50,000-memory, 1,000-persona fixture | ≤ 3,000 tokens | CI absolute (`stress`) | KN-30 | — |
| F1 | formula input: each harness's footprint per process, child runs included | value, not budget | release absolute | SC-1, #814 | Claude Code 385 MB; Codex, opencode not yet |
| F2 | formula input: one hot chat's scrollback at the cap | value, not budget | release absolute | #814 | 20.2 MB |
| F3 | formula input: purlis's own base | M1 | release absolute | #814 | as M1 |
| S | search and index budgets | as [ADR 0079](adr/0079-search-runs-on-derived-sqlite-indexes-one-per-project-clone-and-one-per-machine.md) §4 | as there | KN-1, KN-2, KN-22, KN-32 | — |

**Not a budget, a guard:** GL-15 warns when less than 20 GB of disk is left, and starts no new
branch folder when less than 5 GB is.

## Milestones

Each milestone is something the operator actually uses, not a layer.

- **M0: walking skeleton.** Tauri + Rust core + xterm.js, with 50 fake sessions and one
  scenario test green in CI on macOS and Linux. It is measured against the limits above and
  locks the stack. GPUI is tried only if a limit is missed. **Done** — the measurements and
  the lock are ADR 0026, and the benchmark is `node tools/bench.mjs` in purlis.
- **M1: daily driver on macOS, on Rust alone.** The app replaces the tmux frame for the
  operator, and every part of it is Rust:
  - the plane read *and written* in Rust: workspaces, chats, personas, todos, profiles
  - the workspace sidebar and the "needs you" queue
  - chats with the profile and persona picker
  - a worktree per chat (ADR 0027). The gap ADR 0027 named is closed: the harness layer is
    written into a piece when purlis cuts it and again when a chat starts there, so that chat
    has the plane's guards and its persona's agents
    (`crates/purlis-core/tests/a_chat_in_a_worktree_gets_the_planes_layer.rs`). `unwired` is
    left only on a tree whose layer is not in it yet, and a chat is refused there rather than
    started unguarded. What is still missing is in decision 4
  - panels and the palette (read-only at M1; the panels write todos, personas and vaults since
    SI-3 — decision 1)
  - lifecycle: tray, a quit warning mid-turn, reopen on relaunch
  - `charter hook …` answered by the Rust binary, which is also what meets the hook limit
    ADR 0026 measured at 107.6 ms through Python

  Each ported piece carries its differential test against Python.
- **M2: the rest of the CLI.** Every remaining command the plane needs — recall, save, sync,
  clone, doctor, news, report, git-policy — with differential tests, so the Rust `purlis` is
  the only one a plane needs.
- **M3: security-critical parts proven.** Hooks guard, gitpolicy, vaults and secrets get their
  external review against the Rust implementation (the pieces themselves land whenever a
  milestone needs them, never on a Python fallback).
- **M4: public release.** Linux, then Windows (ConPTY, bundled package), signed installers,
  the final PyPI release pointing to the new install, and Python charter retired.

### Where the multi-plane work goes (added 2026-09-20)

Decisions 22-28 are product work this spec missed, not a milestone of their own, so they are
placed inside the existing ones and the placement is part of the decision.

- **M2 gains the opener, the plane switcher and the machine-level record** — decisions 22, 23,
  24, 25 and 28. It goes here because M2 is where the app stops needing a terminal for
  anything, and an app that can only be started from a terminal is the largest remaining place
  where it still does. The expensive half is not the UI: `Hooks`, `Chats` and `Plane` are
  `app.manage(...)` process-wide singletons today, `Plane` is one `Option<PathBuf>` for the
  whole process, and fifteen commands take one of those as `tauri::State`. The window has to
  carry the plane and that state has to be keyed by it before a second plane in a window is
  anything but a second plane writing through the first one's state.
- **M2 also gains decision 27**, `purlis init`'s new default, because `init` is already ported
  and the opener is what makes the old default dangerous. Its differential scenario changes in
  the same commit that changes the default, marked as an intended divergence.
- **M3 gains the trust gate** — decision 26. It is an ask in front of a real exposure, it is
  measured against `layer.rs` and `reopen.rs` rather than assumed, and M3 is where the parts
  that decide what runs get their external review. It must land no later than the first build
  an operator other than this one installs, because until it does, opening an unknown plane is
  a decision made silently.
- **M1 is not reopened.** It was declared a daily driver against a definition that assumed a
  terminal launch. That is worth writing down rather than quietly re-scoping: the bar moved
  under it, and the app has been unusable from an icon for every day it has been called done.

## What M0 reported

M0 measured the skeleton against the limits above on the operator's machine and locked the
stack: **ADR 0026**. It answers the first of the questions this section left open.

- **The scrollback cap is 5000 lines**, and a hidden session holding that much at 150 columns
  costs 20.2 MB of the 50 MB the limit allows. Fifty of them add about 1 GB to the app.
- **xterm.js draws with its own DOM renderer.** WebGL was measured beside it: both meet every
  limit, and neither is faster at the same things, so the simpler one wins on priority 1. The
  addon stays behind a switch, because many panes on screen is the one thing it is better at.
- **The hook call is missed**: 107.6 ms through Python charter, against a 1.8 ms start for the
  Rust binary. M3 is where it is met, and it is not the stack's to fix.
- **A `?2026` animation falls to one frame a second** if a writer pauses inside an open update
  (xterm.js#6071), against 52 draws a second when repaints are written whole. The core can
  close it in M1 by never ending a chunk inside an open update, with a deadline of its own.

Still open:

- ~~Whether the file or socket for hook events (decision 3) needs anything beyond Tauri's own
  single-instance and IPC plugins.~~ **Answered 2026-09-18 by M1.3: neither.** A unix socket
  named in each session's own environment needs no discovery and no plugin. Decision 3 records
  what else that milestone had to settle.
