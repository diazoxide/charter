# Harnesses

> **In this version** the CLI has `charter harness list` only, and chats are started from the app's window; the `charter <profile>` terminal launch is not in it yet. There is no `charter harness install` and there will not be one: the app installs nothing into a harness, and arms each chat it starts for that session alone (*Per profile — armed at launch*, below). Where it describes the tmux frame, read [frame.md](frame.md): this app has no frame, and its window takes the frame's place.

You use Claude Code. A teammate uses opencode. CI runs Codex.

Same repos, same rules — or three sets of habits that drift until nobody knows which guard
is actually running. charter runs inside all three and enforces the same invariants in
each: the plane-root guard, the one-credential rule, the secret-leak check, and the
persona's declared tools.

```bash
charter harness list          # every profile, every harness, what it can't carry, and which one you're in
```

```
  NAME      KIND      COMMAND   FROM
* claude    claude    claude    built-in
  opencode  opencode  opencode  built-in
  codex     codex     codex     built-in

  claude-code
* opencode
      ↳ status-bar: no status-bar socket: opencode has no `statusLine` config …
          → charter statusline --watch
      ↳ prompt-hook: no per-turn prompt hook: charter's mid-session nudges ride …
      ↳ ask-decisions: no ask channel at tool time: opencode's `tool.execute.before` …
  codex
      ↳ status-bar: `tui.status_line` takes a list of built-in segments, not a command …
          → charter statusline --watch
      ↳ session-lock: no workspace lock outside a frame: `shell_environment_policy.set` …
      ↳ wiring-scope: no project-level config: hooks live only in `~/.codex/config.toml` …
```

The profiles come first, and there `*` marks `[harness] default` (see
[Two accounts of one harness](#two-accounts-of-one-harness-or-one-version-pinned)).
Below them, `*` is the harness this session is in, and those names are what `$CHARTER_HARNESS`
holds. A harness charter has no record of is reported too, as a warning rather than a clean
row — an unverified integration and a complete one must not read the same.

## Two accounts of one harness, or one version pinned

A harness profile is a named way to launch one harness: its kind, its command, its
environment. Every harness charter knows is a built-in profile named after itself. Two
Claude Code accounts, or a Codex pinned to an older release, are profiles you declare in
`charter.local.toml`, a file charter keeps out of git. The shape, what is refused and why,
and how the file is kept uncommitted are in
[control-plane.md](control-plane.md#harness--profiles-and-the-default).

`charter harness list` shows every profile charter read, before the harnesses' ceilings:

```
  NAME          KIND      COMMAND                                  FROM
  claude        claude    claude                                   built-in
  opencode      opencode  opencode                                 built-in
  codex         codex     codex                                    built-in
* claude-work   claude    CLAUDE_CONFIG_DIR=~/.claude-work claude  charter.local.toml
  codex-pinned  codex     npx -y @openai/codex@0.140.0             charter.local.toml
refused:
  claude.alt: profile 'claude.alt' is not a name charter accepts — letters, digits, '_' and '-', starting with a letter or digit, and no dot, because a dot breaks tmux targets. Rename the table.
```

- **NAME** — what the profile is called; `*` marks `[harness] default`.
- **KIND** — which harness program it launches.
- **COMMAND** — the environment it sets, then its command. A control character in either is
  shown escaped, never interpreted: the file is one a chat can write, and a carriage return
  could otherwise redraw the line to show a different command.
- **FROM** — `built-in`, or the file that declared it.

Then `refused:`, one line per profile charter would not read and why, and a warning while
git would commit the file.

**A wrapper around a harness is a profile too.** If you reach Claude Code through something
else — `ccs work`, which picks the account a chat is billed to, or a script that runs a
binary kept off your `PATH` — declare the wrapper as the command:

```toml
[harness.claude-ccs]
kind = "claude"
command = ["ccs", "work"]
```

`charter claude-ccs` then runs `ccs work`, your arguments follow it (`charter claude-ccs -p
hi` runs `ccs work -p hi`, and a reopen hands it `--resume <id>`), and it is still the
Claude Code harness: the pane carries `$CHARTER_HARNESS`, the workspace layer and a session
`charter reopen` can resume. `charter frame -- ccs work` runs the same words and forgets all
three. The program charter looks for before it starts anything is the wrapper — a missing
`ccs` is refused and named, and a `claude` your `PATH` cannot see is the wrapper's to find.

**The wrapper has to pass its arguments on**, and not only yours: the app arms a chat with
words it puts after the profile's whole command — `--plugin-dir <bundled plugin> --settings
<json>` for Claude Code, `-c hooks.<Event>=…` for Codex (see *Per profile — armed at
launch*, below) — and its own session words after those. A `ccs work` profile starts as
`ccs work --plugin-dir … --settings … --session-id <id> --name <name>`, so the wrapper reads
its own `work` first, and the rest has to reach `claude`. A wrapper that swallows them
starts a chat with none of charter's hooks and none of its guard.

**There is no `charter harness add`.** A profile is added by editing `charter.local.toml`. A
chat can run a command as easily as it can edit a file, so a command could never stand for
your approval of what a profile runs; it would only save typing.

**What does stand for your approval is the launch itself.** The first time charter is asked
to run a profile you declared — and again whenever its command or environment changes — it
shows you what that is and asks `run this? [y/N]` before it runs it. Built-ins never ask.
See [*A new or changed command asks once*](control-plane.md#a-new-or-changed-command-asks-once).

## What each harness lets charter offer

**What differs is not what charter enforces — it is what each harness lets charter
offer**, and `charter doctor` prints the gap rather than leaving you to find it:

| | how charter reaches a chat | how it updates | what it cannot carry | what to do about it |
| --- | --- | --- | --- | --- |
| Claude Code | the app's own plugin, `charter-app`, loaded into each chat it starts with `--plugin-dir` — nothing installed | with the app | — | — |
| Codex | charter's hooks, armed on each chat's command line as `-c hooks.<Event>=…` — nothing installed; Codex asks once to trust them | with the app | no status bar; no command-pattern permissions; no project-level config *file*, so no per-workspace config (a project `.codex/skills/` **is** read — a skills surface, not config); no prompt in front of `charter handoff`; no word when it stops mid-turn for your approval | `charter statusline --watch`; `guard ask` rules stay in charter's own hook; that hook still refuses a handoff from a sub-agent or from a run reporting `permission_mode: bypassPermissions`, and an attended chat's handoff runs without asking |
| opencode | not started by this app — v1 starts Claude Code and Codex, and opencode follows | — | no status bar; no per-turn prompt hook; no ask at tool time; no per-workspace config | — until the app starts it |

Claude Code's row is empty because nothing charter offers is out of reach there, not
because charter fills every surface it has: the app sets Claude Code's `statusLine` for a
chat only where the settings in force fill it with nothing, so a footer the operator wired
stays theirs. That is a choice, not a ceiling, which is why it is written here and not in
the table.

The third column says the same thing twice because there is one thing to move. The plugin, the hooks it
declares and the `charter` they call ship inside the app's bundle, and the app's updater
moves all of them together ([updating.md](https://github.com/diazoxide/charter-app/blob/main/docs/updating.md)).
Nothing is installed into Claude Code or Codex, so there is nothing there to keep in step.

Nothing per harness is installed, and nothing is written into the repos you work in.
`charter doctor` and `charter harness list` print that last column against whichever
harness you are in, each ceiling carrying its own answer. Where it is empty it stays empty:
charter cannot conjure opencode a per-turn prompt hook, and a workaround that does not
exist costs more to chase than an honest gap.

### Resume: the session each tab is linked to

Every tab is linked to exactly one harness session, and `charter reopen` offers that
conversation back only when it exists. How each harness is linked, and what it costs, was read
on these versions (ADR 0024):

| Harness | Link | Offered when | Resumes with | Limits |
| --- | --- | --- | --- | --- |
| Claude Code 2.1.272 | the id charter hands it (`--session-id <uuid> --name <session name>`); follows `/clear`; ignores a nested `claude` | after the first prompt (the transcript exists) | `--resume <id> --name <session name>` | the in-session `/resume` hides the current session; the name shows in a fresh `claude --resume` picker, and in `/resume` from every other session |
| Codex 0.147.0 | the first id it reports in each start | after its first turn (it reports nothing before) | `codex resume <id>` | after `/new`, resume offers the start's first conversation; read from source, not run |
| opencode 1.18.23 | the first id its tool hooks report in each start | after its first tool call | `opencode -s <id>`, in the chat's recorded directory | a new session inside opencode is not followed; a chat moved to another directory reopens empty; read from source, not run |

### The name a session is started under

**Claude Code is the only harness charter names, and the name is `<title> · <chat id>`** —
the tab's title and its id, or the id alone for a tab nobody named (*A tab can carry a name
you chose*, `docs/frame.md`). It is composed at every start and at every resume and stored
nowhere: charter passes it with `--name` and never types `/rename` into a running harness
(ADR 0018). So **a rename you make now reaches Claude Code at that chat's next start or
resume, and not before.** Measured on 2.1.272: a name with spaces and ` · ` is accepted at a
start and at a resume, written with no prompt, and listed by a fresh `claude --resume` picker;
the in-session `/resume` hides the session it is run in.

**Codex and opencode take no name at launch, so their tabs carry the title in charter's own
surfaces only** — the strip, the tab menu, the quit and close rows, the ended tab's choice and
`F2 → chat`. Codex names no flag for it at all (openai/codex#14482 is open) and opencode's
`--title` exists only on `opencode run`, not on the TUI charter starts. Each keeps whatever
session name it chose for itself, and `/rename` inside them is yours to type, not charter's.
The notice after a rename says which of the two you just did.

**Which harness sent a report is decided by the report, never by the environment it
inherited.** A harness nested in a chat's shell inherits `CHARTER_HARNESS`, `CLAUDE_PID` and
`TMUX_PANE`, so none of those can say. A Claude Code report proves itself by
`CLAUDE_CODE_SESSION_ID` equal to its payload's `session_id`, and then counts only from the
`CLAUDE_PID` that adopted the chat's id. **Charter recognises a Codex report by the exact key
set of Codex's SessionStart input**, read from source at codex-cli 0.147.0 and not yet seen on
the wire. A Codex that adds a field is recognised as no report. The failure is closed: no link
is recorded, so resume is simply not offered for that chat until charter is updated — a
missing resume row is the only sign.

## Wiring, and when it happens

**`charter init` installs no software.** It writes the plane's own files — its settings, the
`ask` rule in front of `charter handoff` — and nothing into a harness's config folder or
anywhere outside the plane. `charter reinit` does the same. Charter's hooks and guard reach a
chat because the app started it: every chat is armed at launch, for that session alone
(*Per profile — armed at launch*, below), so there is no install to run, repair or keep up
to date.

**Plus one generated file per workspace, and only for Claude Code.** Claude Code reads
project settings from the session's working directory and does not walk up, so a chat
launched in `workspaces/<ws>/` — which is where the `+` and every workspace tab put it —
would otherwise get none of the plane's plugins and none of its `env`. Charter mirrors the
plane's `enabledPlugins` and `env` into `workspaces/<ws>/.claude/settings.json` at launch,
records what it wrote in a `.charter-generated` sidecar, and never touches a file whose hash
it cannot vouch for. Charter's own plugin does not depend on this file: it arrives on the
command line. (`statusLine` was the third key mirrored until 0.57.0; charter writes none into
any settings file now, so a plane that still carries one keeps it to itself.)
`charter doctor`'s `workspace layer` row reports staleness; `charter workspace reinit` is
the repair.

Beside it, the **`session layer`** row answers the other half — *can a session started in
this directory see any of that?* Three artefacts, three discovery rules, all measured on
Claude Code 2.1.259: `.claude/settings.json` is read from the session's own directory with
no walk-up, `.claude/agents/` and `.claude/skills/` walk up but stop at the git root, and
`CLAUDE.md` walks up and is not git-bounded. So "charter is set up here" was never one
fact, and the row names which part is missing and which rule decided. The rules live on
each harness (`Harness.layer`), so a harness added to the registry is answered for the day
it is registered rather than reported under Claude Code's rules by default.

opencode and Codex get nothing here and say why: a workspace **directory** is not a
config scope for either, so charter's layer is already live in every workspace and two
workspaces on one machine cannot be made to differ. That ceiling is in `charter harness
list` beside the others.

Both do read something from a project, and charter says which rather than implying the
project carries nothing — measured with real sessions, because a management CLI is not one:
opencode reads an `opencode.json` at the **repository root** and `.opencode/agent/`
(1.18.23); Codex reads `.codex/skills/` and ignores a project `.codex/config.toml`
(0.147.0). Charter writes nothing machine-global on the operator's behalf. Inside a
workspace's checkouts it mirrors the plane's copy of each *capability* surface, and of the
config files above it mirrors none — see below.

**A clone gets the same layer, plus what the walk-up could not carry there.**
`workspaces/<ws>/<repo>/` is a repo of its own, so a session inside it loses the settings
*and* the plane's `.claude/agents/` — the walk-up stops at that git root. Charter writes
both, and hides them in the clone's own `.git/info/exclude`: per-checkout, never
committed, not itself tracked, and the one file a guest may write. Charter never edits the
clone's `.gitignore`, hides only the exact paths it generated (never a `.claude/` glob,
which would take your own untracked files with it), never touches a file it did not write,
and removes its files and its exclude block when the workspace goes. `git status` in your
repo is unaffected, and nothing charter wrote can be staged. Linked worktrees included —
their `info/exclude` is the main repo's, which is also why removal is not just a
`rm -rf`.

**And it is every harness's layer.** What a git boundary cuts off is spelled by each
harness — `Harness.inherited_paths`, beside `layer` and `layer_note` — so charter's own code
names none of it:

| harness | carried into a checkout | binary |
|---|---|---|
| Claude Code | `.claude/agents`, `.claude/skills` | 2.1.259 |
| opencode | `.opencode/agent` | 1.18.23 |
| Codex | `.codex/skills` | 0.147.0 |

A harness registered tomorrow is carried the day it declares a surface. What charter mirrors
is **capability** — agents, skills, commands — and three things are deliberately absent:

- **A harness's config file.** `opencode.json` is read at a repository root, so a clone does
  stop seeing the plane's copy — and `charter guard` keeps this plane's `permission` grants
  in that same file. Copying it would put an `allow` in force in a repository nobody granted
  it in, which is what `.claude/settings.json`'s mirrored keys already refuse for
  Claude Code. A mirror cannot drop a key; that is the difference between the two lists —
  and it is why Claude Code's settings arrive **generated** instead: `permissions.ask` and
  `permissions.deny` from the plane's shared file into a workspace's and a clone's
  `.claude/settings.json`, and from its local file into a clone's own
  `.claude/settings.local.json` only, never `permissions.allow` (#942). Claude Code reads
  the local file at the git root (measured on 2.1.267), which a workspace directory shares
  with the plane and a clone does not — see *A chat standing here gets charter* in
  `docs/workspaces.md` for the measurement and for how charter keeps that file hidden once
  Claude Code writes its own approvals into it.
  The consequence for opencode is a standing gap and not a silence: a session rooted inside
  a clone does not have the plane's `guard ask` rules. A generated checkout `opencode.json`
  carrying only the restrictive half is the route, and is not built.
- **A project `.codex/config.toml`**, because Codex ignores it — writing it would look like
  wiring while being inert.
- **`CLAUDE.md` or any equivalent**, because a guest hides its own files and does not
  narrate the host's.

### Per profile — armed at launch

A profile points a harness at another config folder, and **charter's guard does not live in
that folder**, so moving it moves nothing of charter's. The app arms every chat it starts on
the command line, for that session alone, and writes nothing into any config folder:

- **Claude Code** gets `--plugin-dir <the bundled plugin>`: the app's own plugin,
  `charter-app`, with every hook charter answers — the state hooks and the Bash guard — and
  the `handoff`, `working-in-a-clone` and `update` skills, which reach the model as
  `charter-app:<skill>`. Beside it, `--settings` carries `enabledPlugins` with
  `charter-app@inline` pinned on — a project file a chat can write could otherwise turn it
  off — and the Python charter's `charter@charter` turned off, so a plane whose settings
  enable that plugin for your terminal sessions does not give an app chat two sets of hooks
  and two `handoff` skills. `--settings` merges with the settings in force and wins where it
  names a key (measured on claude 2.1.276 and 2.1.280), and it is for that session only: the
  project file still enables `charter@charter` for every `claude` you run yourself.
- **Codex** gets `-c hooks.<Event>=[…]` for the four state events Codex fires
  (`SessionStart`, `UserPromptSubmit`, `Stop`, `SessionEnd`) and the Bash guard on
  `PreToolUse`, each pointing at the app's `charter` by its absolute path. Codex lists them as
  "Session flags" and runs them beside any hooks of your own in `$CODEX_HOME/config.toml`
  (measured on codex-cli 0.147.0). They are inert until trusted: the Codex TUI asks at
  startup, and records the answer in its own `[hooks.state]`, so one binary path is asked
  about once, not once a chat. The app turns nothing of Codex's off, so a charter plugin the
  Python charter installed into that `$CODEX_HOME` still runs as well.
- **opencode** is not started by this app. A profile of that kind is read and listed, and a
  start is refused: *charter-app v1 starts Claude Code and Codex, and opencode follows.*

Every chat also carries `$CHARTER_HARNESS` (the registry's name for its kind, whatever the
profile is called), `$CHARTER_HARNESS_PROFILE` and `$CHARTER_ROOT` in its environment, and a
`PATH` with the app's own `charter` first unless the profile sets its own.

**What still stops a chat on a profile**, each said before anything opens:

- **A kind this app does not start** — opencode, above.
- **A profile charter may not run a command for** — not approved yet, or declared in a
  `charter.local.toml` git would commit. Nothing is run and nothing is written.

**`charter doctor` shows a row per profile and runs nothing to fill it.** A profile it may
ask about reads OK — *the app arms each chat with its own plugin, charter-app*, or for Codex
*the app arms each Codex chat with charter's hooks; Codex asks once to trust them* — with the
program it found. What can still be wrong is whether the harness can be found at all, and
that is a warning naming the directories searched and the fix: an absolute command in
`charter.local.toml`. `charter doctor --preflight`, which the SessionStart hook runs, shows no
profile row.

**The one place a chat can still be unarmed:** a build of the app that carries no plugin — a
development build without one — starts a Claude Code chat with nothing added, and the chat
reads `unknown` rather than being half-armed from somewhere else.

## `charter statusline --watch`

The one worth knowing. It repaints the plane state in place in any spare terminal — no
status-bar socket, no multiplexer, the same render on every harness including the one that
has a bar. It shows the plane, not the session, so the token and context columns are blank
and it says so.

**It is why `charter statusline` outlived charter's Claude Code footer.** #895 asked
whether the status line was used for anything but that footer. It is: this loop, opencode's
`/charter` slash command (whose body pipes the same command), and the frame, whose panels
are built out of the same renderers. So charter stopped WIRING a status line and kept the
command that draws one.

## Codex needs nothing extra now

The Python charter needed `charter harness install codex` for one line: its Codex hooks
arrived with a plugin, and nothing in a plugin could tell a shell which harness it was. The
app sets `$CHARTER_HARNESS` in the environment of every chat it starts and arms Codex's hooks
on the command line, so there is no line to write and no command to write it.

Why the boundary sat where it did, for the Python charter:
[ADR 0015](https://github.com/diazoxide/charter/blob/0ae0961d8a6a8e59b48ba43b10d28de8fd87afb7/docs/adr/0015-the-boundary-moves-with-the-harness.md).
