# Harnesses

You use Claude Code. A teammate uses opencode. CI runs Codex.

Same repos, same rules — or three sets of habits that drift until nobody knows which guard
is actually running. charter runs inside all three and enforces the same invariants in
each: the plane-root guard, the one-credential rule, the secret-leak check, and the
persona's declared tools.

```bash
charter harness list          # every profile, every harness, what it can't carry, and which one you're in
charter harness install codex # Codex only — see below
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
      ↳ session-lock: `shell_environment_policy.set` holds constants, so no per-session …
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

**The wrapper has to pass its arguments on**, and not only yours: charter asks the harness
whether its plugin is wired by running the profile's own command with the harness's probe
after it — `plugin list --json` for Claude Code, `debug config` for opencode (see *Per
profile — wired, or it refuses to start*, below) — so `ccs work plugin list --json` has to
reach `claude`. A wrapper that swallows arguments reads as a profile charter could not ask,
and it refuses to start.

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

| | how it is installed | how it updates | what it cannot carry | what to do about it |
| --- | --- | --- | --- | --- |
| Claude Code | `charter init` — the plugin, at `project` scope, for the plane it creates (`charter doctor --fix` for a plane that already exists) | `claude plugin update charter@charter` | — | — |

Claude Code's row is empty because nothing charter offers is out of reach there, not
because charter fills every surface it has: since 0.57.0 charter writes no `statusLine`
key, so its footer is the operator's to wire or leave empty. That is a choice, not a
ceiling, which is why it is written here and not in the table.
| opencode | `charter init` — one plugin under opencode's config dir, read by every project | charter moves it — its own file, compared byte for byte (`read_bytes`) with the one charter generates; anything else in that plugin directory is named too, and nothing charter did not write is ever overwritten | no status bar; no per-turn prompt hook; no ask at tool time; no per-workspace config; **no isolation from other plugins**; no refusal of a `charter handoff` from a sub-agent or an unattended run | `charter statusline --watch`; mid-session notes ride tool output already; charter's own tool-time asks allow and are not shown — denials are unaffected; a second plugin in that directory shares charter's globals and can disable its guards, so `doctor` names it — charter reports the realm, it cannot contain it; the plugin's tool payload carries neither `agent_id` nor `permission_mode`, so the `ask` rule in `opencode.json` is a handoff's whole gate |
| Codex | the same plugin (`codex plugin`), plus `charter harness install codex` to name the harness | `codex plugin marketplace upgrade charter && codex plugin add charter@charter` | no status bar; no command-pattern permissions; no project-level config *file*, so no per-workspace config (a project `.codex/skills/` **is** read — a skills surface, not config); no prompt in front of `charter handoff` | `charter statusline --watch`; `guard ask` rules stay in charter's own hook; that hook still refuses a handoff from a sub-agent or from a run reporting `permission_mode: bypassPermissions`, and an attended chat's handoff runs without asking |

You never have to remember that third column — `charter update` asks the harness you are in
and names its command (or, for opencode, just moves it). It is written down because a
column charter fills in from one place is a column that cannot quietly go stale in three.

One artifact per harness, installed once — nothing is written into the repos you work in.
`charter doctor` and `charter harness list` print that last column against whichever
harness you are in, each ceiling carrying its own answer. Where it is empty it stays empty:
charter cannot conjure opencode a per-turn prompt hook, and a workaround that does not
exist costs more to chase than an honest gap.

## Wiring, and when it happens

`charter init` writes each harness's wiring into the plane, and installs the one artifact
charter can install for you: Claude Code's charter plugin, at `project` scope, for the plane
it is creating. Codex is the exception (below) — its wiring is machine-global, so it waits
to be asked by name.

**`init` and `charter doctor --fix`, and nothing else.** Installation never happens as a
side effect of an ordinary command: `charter workspace list` does not install software, and
`charter reinit` — which re-runs the *wiring* — does not either. Both doors are commands
somebody typed, which is the same shape `charter harness install codex` has and the same
reason charter refuses to write `~/.claude/settings.json` unasked.

**Plus one generated file per workspace, and only for Claude Code.** Claude Code reads
project settings from the session's working directory and does not walk up, so a chat
launched in `workspaces/<ws>/` — which is where the `+` and every workspace tab put it —
would otherwise get no plugin and no `$CHARTER_HARNESS`. Charter mirrors the plane's
`enabledPlugins` and `env` into `workspaces/<ws>/.claude/settings.json` at launch, records
what it wrote in a `.charter-generated` sidecar, and never touches a file whose hash it
cannot vouch for. (`statusLine` was the third key mirrored until 0.57.0; charter no longer
writes one anywhere, so a plane that still carries one keeps it to itself.)
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

### Per profile — wired, or it refuses to start

A profile points a harness at another config folder, and **every kind loses charter's wiring
when its folder moves**. Measured in throwaway folders against claude 2.1.269, codex-cli
0.147.0 and opencode 1.18.23: under an empty `$CLAUDE_CONFIG_DIR` charter's plugin is not
merely disabled, it is unknown to that folder, because the marketplace is known only to the
folder it was added in. Under an empty `$CODEX_HOME` there is no plugin, no hook trust and
no `shell_environment_policy`. Under a throwaway `$XDG_CONFIG_HOME` opencode has no shim.

So a chat started on such a profile would look guarded and not be, and charter refuses it:

```
charter: profile 'claude-alt' is not wired — charter@charter is not installed in
/Users/you/.claude-alt for /plane/workspaces/w, so a chat on it would run without charter's
guard. Nothing was started. Wire it: charter harness install claude-alt
```

**This includes the built-ins.** `charter codex` on a plane where nobody wired Codex, and
`charter opencode` where `init` never wrote the shim, now refuse where they used to start:
a chat that looks guarded and is not is the same failure whichever profile started it. No
flag launches one unguarded.

**Wiring is detected by asking the harness under the profile's own environment**, never
inferred from the profile's variable names — one account can be reached through variables
that do or do not move the plugin. `$XDG_DATA_HOME` moves opencode's login and leaves its
plugins where they are; `$CLAUDE_CONFIG_DIR` and `$CODEX_HOME` move both.

| kind | what charter asks | what "wired" means |
|---|---|---|
| Claude Code | `<command> plugin list --json`, run in the chat's own directory with the profile's `env` | an install record of `charter@charter` covering that directory or the plane, whose `enabled` is true |
| Codex | reads `$CODEX_HOME/config.toml` — no subprocess | all three: `plugins."charter@charter".enabled`, `shell_environment_policy.set.CHARTER_HARNESS = "codex"`, and a trusted entry for charter's **guard** hook — `charter hook pretooluse`, at the position the installed plugin's `hooks.json` gives it |
| opencode | `<command> debug config` with the profile's `env` | all three: a `plugin` entry naming charter's shim, the shim byte-identical to what charter writes, and no other file in `plugin/` |

Measured costs, five runs each: `claude plugin list --json` 137 ms against an empty folder
and 178 ms against a full one; `opencode debug config` 631 ms and 718 ms; Codex parses one
file. A launch pays it twice — once before tmux and once in the pane — and always freshly.
The `+`, a workspace tab, `charter reopen` and a handoff ask before they open anything, so
their refusal has somewhere to be said, and the pane asks again; nothing in between does.

**Nothing is asked of a profile charter may not run.** A probe runs the profile's own
command, so it waits for the same two things a launch does: git would not commit
`charter.local.toml`, and you have approved that command (`run this? [y/N]`). Until then
`charter doctor` says which of the two it is waiting for, and probes nothing.

Three details that decide answers, all measured on 2026-09-12:

- **`enabled` is the effective setting at the directory charter asks from**, merged local >
  project > user, and every listed entry of the plugin carries the same value. A disable
  written only to `<dir>/.claude/settings.local.json` — or only to that directory's
  `.claude/settings.json` — flips it, so charter reads no settings file of its own.
- **An install covering the plane covers a chat in its workspace.** `charter init` installs
  at `project` scope for the plane root, while a chat stands in `workspaces/<ws>/`; charter
  mirrors the plane's `enabledPlugins` into that directory, so the record that covers the
  plane is the one that answers there.
- **Codex records hook trust lazily**, one entry per hook as each first fires — a machine
  that has been running charter under Codex for weeks held 12 of the plugin's 18 keys — and
  a `trusted_hash` cannot be recomputed from the plugin's `hooks/hooks.json`. So charter
  asks whether charter's **guard** hook — `charter hook pretooluse`, the one on `Bash` that
  refuses a command — was approved in that home at all. Neither an approved SessionStart
  hook nor the approved `Task|Agent` dispatch hook is enough: Codex asks about each hook on
  its own, and neither of those refuses a command. Codex numbers its trust entries by
  position in the installed plugin's own `hooks/hooks.json`
  (`$CODEX_HOME/plugins/cache/charter/charter/<version>/`), and those positions move between
  releases, so charter reads the guard's position there; when two cached versions disagree,
  it is not wired. An old entry survives a change to a hook's command.
- **A disabled plugin's fix is `claude plugin enable charter@charter --scope local`, run in
  the chat's own directory**, and charter prints it with that `cd` in front. Measured on
  claude 2.1.270 (2026-09-13): it undid a disable in that directory's `settings.local.json`,
  in its `settings.json`, and in a git plane root's `settings.local.json`, which reaches a
  session below it. `--scope project` and `--scope user` each exited 1 over a local disable
  and changed nothing.

**Charter cannot tell** — a probe that times out, exits non-zero or answers something
unparseable — is also a refusal, with the probe to run by hand printed beside it. An unknown
is not a pass.

**What each command does per profile:**

- `charter init` installs for every approved Claude Code and opencode profile, the same two
  doors an install may come through. A Codex profile is reported as opt-in.
- `charter reinit` installs nothing. It writes the opencode shim into each opencode
  profile's own config home, and for a Claude Code profile it reports the gap and names
  `charter harness install <name>`.
- `charter harness install <name>` resolves a profile first and a registry name second, so
  `charter harness install codex` still means what it always did. A profile you have not
  approved is shown and asked about first, as a launch would ask, and refused where there is
  no terminal to ask on. It wires that profile's own folder and then asks the harness
  whether that worked — and exits non-zero when it did
  not, which is the ordinary outcome for Codex, whose plugin install and hook approval are
  Codex's own commands. Charter prints them with `CODEX_HOME=` in front.
- `charter doctor` shows a row per profile and probes them concurrently.
  `charter doctor --preflight`, which the SessionStart hook runs, probes nothing and shows
  no profile row: a probe costs a subprocess and writes into somebody's account folder, and
  a hook's whole budget is 20 seconds.

**Limits, stated rather than designed around:**

- `claude plugin list --json` **writes** `.claude.json` and a `backups/` directory into the
  folder it asks about. Charter's probe is otherwise a read.
- **Codex's home is the one charter can see.** One a wrapper script exports on its way to
  `codex` is invisible here, and charter will report on the wrong file without knowing it.
- `charter dispatch`'s transcript lookup and charter's persona skill lookup answer for the
  **default** config folder, not for a profile's. Neither is about one chat.
- The launch record and the wiring cache under `.charter/` are **as writable by a chat as
  `charter.local.toml` is** — no path guard covers that directory. That is why a launch
  never trusts the cache and always probes: the cache exists to draw the selector's rows.
  An entry is stamped with the config folder's two files and every `.claude/settings.json`
  and `settings.local.json` from the chat's directory up to the plane root, and an entry
  that is a day old or dated ahead is not used.
- A config file a chat left in a shape the harness never writes — a `plugins."charter@charter"
  = true`, a trust entry that is a string, a NUL byte in a profile's path — is **charter
  could not tell**, and refuses like any other unknown.

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

## The one exception: Codex

Codex needs the extra command for one reason: its hooks arrive with the plugin, but nothing
in a plugin can tell a shell which harness it is, so `charter harness install codex` writes
that single line.

If it finds hooks declared in `~/.codex/config.toml` it refuses and says so — those would
run alongside the plugin's, and charter would fire twice a turn.

Why the boundary sits where it does:
[ADR 0015](adr/0015-the-boundary-moves-with-the-harness.md).
