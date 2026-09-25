# Harnesses

A **harness** is the agent program a chat runs: Claude Code or Codex. The app starts both,
arms each chat it starts with charter's hooks and its Bash guard for that session alone, and
installs nothing into either. Starting opencode chats is not in this version yet: an opencode
profile is read and listed, and a start on it is refused.

Chats are started from the app's window. On the command line, `charter harness list` shows
what a chat can be started on:

```
  NAME          KIND      COMMAND                                  FROM
  claude        claude    claude                                   built-in
  opencode      opencode  opencode                                 built-in
  codex         codex     codex                                    built-in
  claude-ccs    claude    ccs work                                 charter.local.toml
* claude-work   claude    CLAUDE_CONFIG_DIR=~/.claude-work claude  charter.local.toml
  codex-pinned  codex     npx -y @openai/codex@0.140.0             charter.local.toml
```

- **NAME** — what the profile is called; `*` marks `[harness] default`.
- **KIND** — which harness program it launches.
- **COMMAND** — the environment it sets, then its command. A control character in either is
  shown escaped, never interpreted: the file is one a chat can write, and a carriage return
  could otherwise redraw the line to show a different command.
- **FROM** — `built-in`, or the file that declared it.

Then `refused:`, one line per profile charter would not read and why, and a warning while
git would commit the file:

```
refused:
  claude-work: git would commit charter.local.toml, so charter reads nothing in it until it is ignored — charter reinit adds /charter.local.toml to .gitignore.
! to use the profiles in charter.local.toml: charter reinit
```

A `charter <profile>` command that starts a chat from a terminal is not in this version yet.

## Two accounts of one harness, or one version pinned

A harness profile is a named way to launch one harness: its kind, its command, its
environment. Every harness charter knows is a built-in profile named after itself. Two
Claude Code accounts, or a Codex pinned to an older release, are profiles you declare in
`charter.local.toml`, a file charter keeps out of git. The shape, what is refused and why,
and how the file is kept uncommitted are in
[control-plane.md](control-plane.md#harness--profiles-and-the-default).

**A wrapper around a harness is a profile too.** If you reach Claude Code through something
else — `ccs work`, which picks the account a chat is billed to, or a script that runs a
binary kept off your `PATH` — declare the wrapper as the command:

```toml
[harness.claude-ccs]
kind = "claude"
command = ["ccs", "work"]
```

Pick `claude-ccs` in the app's new-chat picker and the chat runs `ccs work`. It is still the
Claude Code harness, because the kind says so and not the program's name: the chat carries
`$CHARTER_HARNESS`, charter's hooks and a session id the app can resume it by. The program
charter looks for before it starts anything is the wrapper — a missing `ccs` is refused and
named, and a `claude` your `PATH` cannot see is the wrapper's to find.

**The wrapper has to pass its arguments on**: the app arms a chat with words it puts after
the profile's whole command — `--plugin-dir <bundled plugin> --settings <json>` for Claude
Code, `-c hooks.<Event>=…` for Codex (see *Per profile — armed at launch*, below) — and its
own session words after those. A `ccs work` profile starts as
`ccs work --plugin-dir … --settings … --session-id <id> --name <name>`, so the wrapper reads
its own `work` first, and the rest has to reach `claude`. A wrapper that swallows them
starts a chat with none of charter's hooks and none of its guard.

**There is no `charter harness add`.** A profile is added by editing `charter.local.toml`. A
chat can run a command as easily as it can edit a file, so a command could never stand for
your approval of what a profile runs; it would only save typing.

**What does stand for your approval is the launch itself.** The first time you pick a
profile you declared — and again whenever its command or environment changes — the picker
shows you what it runs and starts it only once you press *Approve and start*. Built-ins never
ask. See [*A new or changed command asks once*](control-plane.md#a-new-or-changed-command-asks-once).

## What each harness lets charter offer

**What differs is not what charter enforces — it is what each harness lets charter
offer:**

| | how charter reaches a chat | how it updates | what it cannot carry |
| --- | --- | --- | --- |
| Claude Code | the app's own plugin, `charter`, loaded into each chat it starts with `--plugin-dir` — nothing installed | with the app | — |
| Codex | charter's hooks, armed on each chat's command line as `-c hooks.<Event>=…` — nothing installed; Codex asks once to trust them | with the app | no status bar; no command-pattern permissions, so `guard ask` rules stay in charter's own hook; no project-level config *file*, so no per-workspace config (a project `.codex/skills/` **is** read — a skills surface, not config); no prompt in front of `charter handoff` — charter's hook still refuses a handoff from a sub-agent or from a run reporting `permission_mode: bypassPermissions`, and an attended chat's handoff runs without asking; no word when it stops mid-turn for your approval |

Claude Code's row is empty because nothing charter offers is out of reach there, not
because charter fills every surface it has: the app sets Claude Code's `statusLine` for a
chat only where the settings in force fill it with nothing, so a footer the operator wired
stays theirs. That is a choice, not a ceiling.

The plugin, the hooks it declares and the `charter` they call ship inside the app's bundle,
and the app's updater moves all of them together
([updating.md](https://github.com/diazoxide/charter/blob/main/docs/updating.md)).
Nothing is installed into Claude Code or Codex, so there is nothing there to keep in step,
and nothing is written into the repos you work in beyond charter's generated layer (below).

### Resume: the session each chat is linked to

Every chat is linked to one harness session, and when the app reopens a chat it resumes that
conversation only where one exists. How each harness is linked was read on these versions:

| Harness | Link | Offered when | Resumes with | Limits |
| --- | --- | --- | --- | --- |
| Claude Code 2.1.272 | the id the app hands it (`--session-id <uuid> --name <name>`); follows `/clear`; ignores a nested `claude` | after the first prompt (the transcript exists) | `--resume <id> --name <name>` | the in-session `/resume` hides the current session; the name shows in a fresh `claude --resume` picker, and in `/resume` from every other session |
| Codex 0.147.0 | the first id it reports in each start | after its first turn (it reports nothing before) | `codex resume <id>` | after `/new`, resume offers the start's first conversation; read from source, not run |

A profile whose own command already names a session (`--resume`, say) gets none of charter's
session words: two on one command line is not a harness anyone has measured, and the one you
wrote is the one you meant.

### The name a session is started under

**Claude Code is the only harness charter names.** The app passes the chat's name with
`--name` at every start and every resume, and never types `/rename` into a running harness,
so a rename reaches Claude Code at that chat's next start or resume, and not
before. Codex names no flag for it (openai/codex#14482 is open), so a Codex chat carries its
name in the app's own window only, and `/rename` inside it is yours to type.

**Which harness sent a report is decided by the report, never by the environment it
inherited.** A harness nested in a chat's shell inherits `CHARTER_HARNESS` and `CLAUDE_PID`,
so neither can say. A Claude Code report proves itself by `CLAUDE_CODE_SESSION_ID` equal to
its payload's `session_id`, and then counts only from the `CLAUDE_PID` that adopted the chat's
id. **Charter recognises a Codex report by the exact key set of Codex's SessionStart input**,
read from source at codex-cli 0.147.0. A Codex that adds a field is recognised as no report.
The failure is closed: no link is recorded, so resume is simply not offered for that chat
until charter is updated — a missing resume is the only sign.

## Wiring, and when it happens

**`charter init` installs no software.** It writes the plane's own files — its settings, the
`ask` rule in front of `charter handoff` — and nothing into a harness's config folder or
anywhere outside the plane. `charter reinit` does the same. Charter's hooks and guard reach a
chat because the app started it: every chat is armed at launch, for that session alone
(*Per profile — armed at launch*, below), so there is no install to run, repair or keep up
to date.

**Plus one generated layer per workspace, and only for Claude Code.** Claude Code reads
project settings from the session's working directory and does not walk up, so a chat
launched in `workspaces/<ws>/` would otherwise get none of the plane's ask/deny rules, none
of its `enabledPlugins` and none of its `env`. Charter writes the plane's copy into
`workspaces/<ws>/.claude/settings.json`, records what it wrote in a `.charter-generated`
sidecar, and never touches a file whose hash it cannot vouch for. Charter's own plugin does
not depend on this file: it arrives on the command line. `charter workspace reinit` is the
repair.

`charter doctor`'s **`session layer`** row answers the other half — *can a session started in
this directory see any of that?* Three artefacts, three discovery rules, all measured on
Claude Code 2.1.259: `.claude/settings.json` is read from the session's own directory with
no walk-up, `.claude/agents/` and `.claude/skills/` walk up but stop at the git root, and
`CLAUDE.md` walks up and is not git-bounded. So "charter is set up here" was never one
fact, and the row names which part is missing and which rule decided.

Codex gets nothing here and says why: a workspace **directory** is not a config scope for it,
so two workspaces on one machine cannot be made to differ. It does read something from a
project — `.codex/skills/` — and ignores a project `.codex/config.toml` (codex-cli 0.147.0).
Charter writes nothing machine-global on the operator's behalf.

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
harness in charter's registry, so charter's own code names none of it:

| harness | carried into a checkout | binary |
|---|---|---|
| Claude Code | `.claude/agents`, `.claude/skills` | 2.1.259 |
| Codex | `.codex/skills` | 0.147.0 |

What charter mirrors is **capability** — agents, skills, commands — and three things are
deliberately absent:

- **A harness's config file.** Copying one would put an `allow` in force in a repository
  nobody granted it in. That is why Claude Code's settings arrive **generated** instead:
  `permissions.ask` and `permissions.deny` from the plane's shared file into a workspace's
  and a clone's `.claude/settings.json`, and from its local file into a clone's own
  `.claude/settings.local.json` only, never `permissions.allow`. Claude Code reads the local
  file at the git root (measured on 2.1.267), which a workspace directory shares with the
  plane and a clone does not — see [workspaces.md](workspaces.md) for how charter keeps that
  file hidden once Claude Code writes its own approvals into it.
- **A project `.codex/config.toml`**, because Codex ignores it — writing it would look like
  wiring while being inert.
- **`CLAUDE.md` or any equivalent**, because a guest hides its own files and does not
  narrate the host's.

### Per profile — armed at launch

A profile points a harness at another config folder, and **charter's guard does not live in
that folder**, so moving it moves nothing of charter's. The app arms every chat it starts on
the command line, for that session alone, and writes nothing into any config folder:

- **Claude Code** gets `--plugin-dir <the bundled plugin>`: the app's own plugin,
  `charter`, with every hook charter answers — the state hooks and the Bash guard — and
  the `handoff`, `working-in-a-clone` and `update` skills, which reach the model as
  `charter:<skill>`. Beside it, `--settings` carries `enabledPlugins` with
  `charter@inline` pinned on — a project file a chat can write could otherwise turn it
  off — and a plugin named `charter@charter` turned off, so a plane whose settings enable an
  older charter plugin for your terminal sessions does not give an app chat two sets of hooks
  and two `handoff` skills. The Python charter's plugin is *named* `charter` too, and Claude
  Code loads one plugin per name: turned off, it cannot take the bundled one's place, and the
  bundled one pinned on cannot be swapped for it (measured on claude 2.1.282). The plugin's
  old id, `charter-app@inline`, is turned off as well, so a file that still names it is told
  the new one. `--settings` merges with the settings in force and wins where it
  names a key (measured on claude 2.1.276 and 2.1.280), and it is for that session only.
- **Codex** gets `-c hooks.<Event>=[…]` for the four state events Codex fires
  (`SessionStart`, `UserPromptSubmit`, `Stop`, `SessionEnd`) and the Bash guard on
  `PreToolUse`, each pointing at the app's `charter` by its absolute path. Codex lists them as
  "Session flags" and runs them beside any hooks of your own in `$CODEX_HOME/config.toml`
  (measured on codex-cli 0.147.0). They are inert until trusted: the Codex TUI asks at
  startup, and records the answer in its own `[hooks.state]`, so one binary path is asked
  about once, not once a chat.

Every chat also carries `$CHARTER_HARNESS` (the registry's name for its kind, whatever the
profile is called), `$CHARTER_HARNESS_PROFILE` and `$CHARTER_ROOT` in its environment, and a
`PATH` with the app's own `charter` first unless the profile sets its own.

**What still stops a chat on a profile**, each said before anything opens:

- **A kind this app does not start** — opencode, for now.
- **A profile charter may not run a command for** — not approved yet, or declared in a
  `charter.local.toml` git would commit. Nothing is run and nothing is written.

**`charter doctor` shows a row per profile and runs nothing to fill it.** A profile it may
ask about reads OK — *the app arms each chat with its own plugin, charter@inline*, or for Codex
*the app arms each Codex chat with charter's hooks; Codex asks once to trust them* — with the
program it found. What can still be wrong is whether the harness can be found at all, and
that is a warning naming the directories searched and the fix: an absolute command in
`charter.local.toml`. `charter doctor --preflight`, which the SessionStart hook runs, shows no
profile row.

**The one place a chat can still be unarmed:** a build of the app that carries no plugin — a
development build without one — starts a Claude Code chat with nothing added, and the chat
reads `unknown` rather than being half-armed from somewhere else.

## `charter statusline`

Claude Code's footer command. Inside the app it prints an empty line — the window already
draws the plane — and still records the turn's token usage. Run anywhere else, it draws the
plane's identity row and says in its body which parts it does not draw yet: repos, personas
and the session. `--watch` and `--interval` are accepted and answered with that same one
render, because there is nothing to repaint yet.
