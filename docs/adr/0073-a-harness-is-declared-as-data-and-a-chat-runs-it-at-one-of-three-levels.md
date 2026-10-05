# A harness is declared as data, and a chat runs it at one of three levels

**Accepted 2026-09-30** by the operator (ruling V24), drafted for program-map ticket FD-12 (#652). It follows these of the
operator's rulings:

- **Q6′:** *"Three harness levels (terminal-only, hooks/native, ACP). Harnesses are declared as
  data in the plane, and charter never ships harness code."*
- **X34**, accepted with the consistency review: *"never ships harness code"* means purlis never
  ships, patches or wraps a harness binary. Per-harness adapters are purlis code, and they are
  data where a template suffices.
- **W8:** the agent run is the unit of governance. Audit reads *"every tool call a level-2 or
  level-3 harness reports"*, with a coverage label per harness level and an org key
  `minimumHarnessLevel`. Level 1 stays a complete, supported product, and every feature degrades
  to level 1, tested each release. FD-20 attaches to a harness's own host where it has one.
- **W10:** the harness capability card (HP-19), built from FD-14's declarations and shown in the
  picker, the chat header and disabled controls' tooltips.
- **V11:** daily-driver parity, whose steer, *Continue on <harness>* and checkpoint features each
  depend on what a harness can do.
- **V21c** ([ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)):
  a run is defined by fixed attributes, and a change to one starts a new run.
- **V23** ([ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md)):
  "plane" is retired, so this record says **project** except in the names of existing files and
  tiers. ADR 0072 §3 keeps "harness" and "capability" to Settings, view tabs, the CLI and the
  docs, off the first-hour surfaces.

It builds on [ADR 0050](0050-a-harness-plugin-is-chosen-per-project-through-one-adapter-per-harness.md),
[ADR 0058](0058-an-opencode-chat-loads-charters-shim-for-that-session-alone.md) and
[ADR 0063](0063-every-harness-a-chat-runs-on-is-handed-charters-skills-by-its-own-route.md)
(one neutral model and one adapter per harness), on
[ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md) (the sandbox
compiled per harness), on [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`purlisd` owns every chat's terminal) and on
[ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md) (storage tiers). It
**amends** purlis-plane's
[ADR 0022](https://github.com/diazoxide/charter-plane/blob/main/docs/adr/0022-a-harness-profile-belongs-to-one-machine.md)
(a harness profile belongs to one machine), ADR 0066 (runs), and the rule in `CLAUDE.md` and
`AGENTS.md` that a session's state comes from hooks only. Each amendment has its own section
below. FD-13 (the neutral model and `HarnessAdapter`), FD-14 (the declaration schema), HP-1 (ACP
as level 3), HP-19 (the capability card) and PE-29 (the extension-point catalogue) build on it.
Its concept is **Chat**. The declarations themselves are a Project setting (ST6).

## Where purlis is today

purlis starts three harnesses: Claude Code, Codex and opencode. What it knows about each is
Rust code:

- **`charter_core::harness::Harness`** is a closed enum of the three, with about twenty methods,
  and each method is a `match` over them. Most are facts measured on one version of one harness:
  the flags that start, name and resume a session; whether purlis chooses the session id; the
  bytes a Shift+Enter sends; the biggest paste drawn whole; whether the harness reports its start
  before the first prompt; and the sentence a chat shows about what the harness does not report
  (`unreported`). Only a few are behaviour: `state_hooks` arms a chat (the plugin for Claude
  Code, hooks for Codex, the shim for opencode), and `disarmed_by` finds a command that would
  run without them.
- **`charter_core::profiles::KINDS`** names the same three again, with the program a built-in
  profile runs and where the operator logs in.
- **`harness_plugin::ADAPTERS`**, **`skills::Route`** and **`sandbox::Form`** each hold one entry
  per harness, in their own modules (ADRs 0050, 0063, 0067).
- **A profile** (`[harness.<name>]` in `charter.local.toml`) says which program a chat runs on
  this machine, with its `kind` (`claude`, `codex` or `opencode`) and its environment. A kind
  outside the three is refused.

So every harness purlis supports is an edit to four modules and a release, and a harness that
purlis has never measured cannot run as a chat at all, not even as a plain terminal. The three
it has all run at one level: a terminal that the harness's own hooks report into. None of them
yet runs over ACP or a harness's own protocol.

## The decision

**A harness is a declaration: data that says how to start it, what it can do and which levels
it can run at. Built-in harnesses are declarations too, shipped with purlis. A chat runs its
harness at one of three levels, chosen when the run starts and fixed for it: 1, the terminal
alone; 2, the terminal with the harness's own hooks reporting to purlis; 3, a structured
protocol. Levels 1 and 3 over ACP need no purlis code for a new harness. Level 2 and a
harness's own protocol need an adapter. purlis never ships, patches or stands in for a
harness's program.**

### 1. Three levels, named by what purlis learns

| Level | What purlis gets | How | What a new harness needs from purlis |
|---|---|---|---|
| **1. Terminal** | The process: started, writing, exited. A snapshot of the pane. | `purlisd` runs the declared program in a PTY (ADR 0068). | A declaration. |
| **2. Hooks** | The chat's state (working, waiting, idle), its session id, and the tool calls its hooks report, on the hook channel (ADR 0068 §6). | The harness's own hook or plugin mechanism, armed for this chat alone (ADRs 0050, 0058, 0063). | A declaration and an adapter. |
| **3. Structured** | Typed turns, items, plans, usage, and asks that carry their options, answered as data. | purlis is the client of a protocol: **ACP** (one client in purlis for every ACP harness, HP-2), or where ACP lags, the harness's own protocol (the Codex app-server, the opencode server) mapped into the same neutral model (HP-3). | A declaration, for ACP. An adapter, for a harness's own protocol. |

**Q6′'s "hooks/native" is level 2, the harness's native hooks.** The program map already puts
the Codex app-server at level 3 (HP-3: *"Codex chat at level 3"*; HP-16 answers app-server
approvals as level-3 asks), so level 3 is "a structured protocol", with ACP as the one purlis
writes once.

**Level 1 is a complete product. Settled by W8.** A level-1 chat starts, is sandboxed, is shown, is stopped
by the kill switch, is saved, and is resumed where its declaration says how. Its asks are
answered from the pane (HP-6: a snapshot and a typed reply). What it lacks is said on its
capability card, never discovered.

### 2. "Beside the PTY" is per harness, not per chat

HP-1's title, *"ACP is level 3, beside the PTY"*, and the synthesis map's Q-6(c), *"ACP beside
the PTY, as the default for dispatch, headless, remote and inbox answers"*, read as if one chat
could have both. It cannot. An ACP agent is a process purlis spawns and speaks to over stdio,
and it does not drive the program that is drawing in the chat's PTY. Codex's app-server and
opencode's server are the same: purlis reaches the host process, not the TUI already running.

**So "beside" holds per harness.** A harness offers its terminal and its protocol side by side,
and purlis picks one for each run. A level-2 chat is a terminal with hooks. A level-3 chat has
no harness terminal of its own. The one way one chat has both is a harness host that serves its
own TUI and purlis at once, which is FD-20 (W8: *"charterd attaches to it instead of owning the
PTY"*). **A chat in a tab starts in its terminal (V24a)**: level 2, or level 1 without an
adapter, even when its harness offers level 3, and FD-20's Codex half moves to Next ★ so that a
Codex tab chat gets both. A chat with no terminal shown
(dispatched, headless, remote) starts at the highest level its harness offers on this machine,
as Q-6(c) recommended.

### 3. A declaration is data

A **harness declaration** gives one harness's name, its program, how to start, name and resume
a session, the levels it offers, and its capabilities (§6). FD-14 fixes the schema in
`docs/plane-format.md` before any code reads it. This sketch shows the shape only:

```toml
# harnesses/gemini.toml: a harness declared by data alone
name = "gemini"                     # the word a profile's `kind` names
title = "Gemini CLI"                # what the operator calls it, on every surface
program = "gemini"                  # a bare name found on PATH: never a path, never a shell string
env = ["GEMINI_*"]                  # the harness's own configuration namespace
tested = ">=0.9, <0.12"             # the versions these facts were measured on (§8)

[session]
chosen_by = "harness"               # or "charter", with `new = ["--session-id", "{id}"]`
resume = ["--resume", "{id}"]       # templates take `{id}` and `{name}` and nothing else
named_by = ["--resume", "-r"]       # flags by which the operator names a session themselves

[terminal]
newline = "\u001b\r"
paste_drawn_whole = { lines = 2, chars = 150 }
ready_to_type = "never"             # "on-start" | "raw-and-quiet" | "never"

[levels]
terminal = true
hooks = false                       # true only where charter has an adapter for this name
acp = ["gemini", "--experimental-acp"]

[capabilities]                      # §6: each is yes, no with a reason, or unknown
reports_waiting = "no: it has no hook for a pending approval"
resumes_by_id = "yes"
```

**Where each of today's facts goes.** Every `Harness` method becomes a declaration field, or
stays in an adapter because it is behaviour:

| Today | Becomes |
|---|---|
| `of_kind`, `name`, `title`, `KINDS[].binary`, `KINDS[].login` | `name`, `title`, `program`, `login` |
| `of_command` (ADR 0062's shell-tab shims) | the set of declared `program` names |
| `env_passed` | `env` |
| `chooses_session_id`, `new_session_argv`, `resume_argv`, `session_words` | `[session]` |
| `newline`, `longest_paste_drawn_whole`, `ready_to_type` | `[terminal]` |
| `reports_its_process`, `reports_its_start_before_the_first_prompt`, `keeps_conversations_by_directory` | capabilities |
| `unreported` | derived from the capabilities that are *no* (§6), so the sentence and the card never disagree |
| `skills` | a capability: `Plugin` and `Config` need an adapter; `Briefing` (ADR 0063's neutral fallback) is the default for every declared harness |
| `state_hooks`, `disarmed_by`, `armed_with` | the level-2 adapter |
| `harness_plugin::Adapter` | the adapter; a harness without one is *not supported yet*, as ADR 0050 already says |
| `sandbox::Form` | the adapter where the harness has a sandbox of its own; otherwise ADR 0067's generated OS profile |

**The built-ins are declarations purlis ships, not files in the project. This departs from
Q6′'s "declared in the plane" on purpose.** Claude Code, Codex and opencode are three
declaration files in purlis's source, read by the same reader and in the same format as a
project's, so "declared as data" holds for them. They are not copied into each project because:

- **their arming carries the guard.** Claude Code's declaration decides how its chats are armed,
  including ADR 0050's pins (`charter-app@inline` always on). A copy in the project would be
  changeable by a merged pull request;
- **their facts are measured per harness version by purlis's own nightly run (TS1)**, and ship
  with the purlis that measured them. A copy in every project would go stale project by project;
- **a project still declares every harness purlis does not ship**, which is what Q6′ asks for.

`purlis harness show <name>` prints any declaration, built-in or not (FD-14 names the command).
Moving the built-ins is FD-14's migration, and a project on an older format reads through FR-9.
The enum `Harness` becomes the adapter registry for level 2, keyed by declared name.

**Tiers (ADR 0069).** A project's declarations are **Plane** tier. A declaration in
`charter.local.toml` is **Clone state**, as that file is. The built-ins are part of purlis's
install and are no store.

**The harness × model matrix is not in the declaration.** It is its own project data (M10,
MS-2), keyed by the declared name.

### 4. What "never ships harness code" means (X34)

**purlis never ships, patches or stands in for a harness's program.** X34 says *"wraps a
harness binary"*. In this record, and in `CONTEXT.md` from now on, that is called **standing in**:
putting purlis's own program where the harness's is expected (a repackaged binary, a
replacement under the harness's name, a proxy that edits its traffic) and changing what the
harness or its model sees. **"Wrap" keeps the one meaning ADR 0067 gave it**: running the
unmodified harness inside a sandbox profile or backend purlis generates. purlis wraps; it
never stands in.

None of these stands in:

- **An adapter**: purlis code that arms a harness through the harness's own documented
  mechanism, for one chat, with nothing written into the harness's config (ADR 0050). The
  bundled Claude Code plugin, the Codex hooks and the opencode shim are adapters.
- **Running the unmodified program in purlis's PTY, or wrapping it** (ADRs 0067, 0068). The
  harness gets the argv it was declared with and sees the files it would outside, minus what the
  sandbox denies.
- **The shell-tab shims** (ADR 0062). A shim prints one line and `exec`s the real program with
  the same argv and environment, and does not stay between them.
- **An ACP adapter program** such as `claude-agent-acp` or `codex-acp`. The user installs it, and
  purlis spawns it like any declared program. HP-1's row already says purlis never ships one,
  and HP-1 records it.

**Data where a template suffices.** Everything level 1 needs is a template: argv, environment
names, and the facts in `[terminal]`. Everything level 3 over ACP needs is the ACP client purlis
has, plus the command that starts the agent. So a new harness at level 1, or at level 3 over
ACP, is **a declaration and no release of purlis** (FD-14's acceptance, HP-14). Level 2 is code:
each harness's hooks differ in where they are registered, what they report and how one chat is
armed alone, and each of today's three took an ADR of its own. **Level 2 always needs purlis code**, in core or in an
extension after PE-29, and declarations get no hook templates for now (V24c).

### 5. Who may declare a harness: the minimum this record needs

A declaration names a program, and ADR 0022's reason holds: a program runs on a click. So:

- **A declaration never carries a shell string or a path.** `program` is a bare name found on
  `PATH`, templates take `{id}` and `{name}` only, and env names are limited to the declared
  namespace, with ADR 0022's refusals (`CHARTER_*`, names that read as a credential).
- **A project adds harnesses. It never takes a built-in's name** (§3's first reason).
- **`charter.local.toml` may declare a harness, or replace a built-in's declaration**, as a
  local profile may replace a built-in profile today.
- **This machine approves a project's declaration once before its program first runs, and again
  when the declaration changes** (V24b). The approval is a **new store**,
  `.charter/harness-declarations-approved.json`, keyed by declaration name and holding the digest
  of what was approved. `harness-profiles-launched.json` is keyed by profile and records a
  profile's command, which a declaration does not have, so the two are kept apart.
  - **Tier:** Clone state. The operator's consent to run a project's declaration on this clone.
    Deleting it makes every project declaration ask again; it fails towards asking. FR-10 backs
    it up, as it does the profile record.
- **A profile still says which program a chat runs on this machine.** Its `kind` names any
  declared harness, not only the three.

FD-14 settles the rest: file layout, schema, refusals, the approval prompt's words, and
`purlis doctor`'s reporting.

### 6. Capabilities: silence never reads as "yes"

A **harness capability** is one thing a harness does or does not do for a chat, such as report
that it is waiting. It is not an extension's capability, and the word is always qualified, as
ADR 0070 did for forges. Each is one of:

- **yes**;
- **no**, with the reason and the fallback purlis uses (for example, *no* to "reports waiting",
  with the fallback "the chat does not show needs you");
- **unknown**, which purlis treats as *no* and says so. This is ADR 0070 §2's rule: no word for
  silence reads as passing.

**The initial set** is what the code reads today: the three facts §3 moves out of `Harness`
(`reports_its_process`, `reports_its_start_before_the_first_prompt`,
`keeps_conversations_by_directory`), `reports_waiting` (needs you), `resumes_by_id`, the skills
route and per-chat plugins. **The tickets that read a flag add it**: HP-15 `steers_mid_turn`,
FD-18 `reports_child_agents`, MH-6 `rewinds`, and so on. A flag's answer can differ by level,
since a harness can report more over its own protocol than through its hooks.

**The capability card reads the declaration.** W10 places it in the picker, the chat header and
disabled controls' tooltips. ADR 0072 §3 sets its words: it is labelled *What <product> can do
here*, each *no* is one line that says what the user will or will not see, and "capability"
stays in its code and in Settings. "Level" is a purlis noun outside §3's seven, so it stays off
the card too. `unreported` becomes those lines, so there is one source.

### 7. Governance follows the level

**Settled by W8**, applied:

- **The audit labels each run's coverage by the level it was started at**: level 1, *process
  only*; level 2, *the tool calls its hooks report*; level 3, *every tool call its protocol
  reports*. A level-2 run whose hooks never report (a Codex chat whose hooks the operator has not
  yet trusted) is recorded as **unarmed** on that run, which the coverage label shows; the run
  does not change level.
- **`minimumHarnessLevel`** (AU-21) refuses to start a chat below it, and says why.
- **Every feature degrades to level 1**, and a release test holds it there.

**Recommended, and forwarded to the W8 policy work (SD-14's policy layer and SD-32), not ruled
here: a policy that needs a specific guarantee names the harness capability, not a level.**
`minimumHarnessLevel` stays the coarse gate, and the policy schema also takes required
capabilities, for example `denies_before_run` (a tool call is refused before it runs, which the
guard needs). The reason is that an ACP agent asks only when it chooses to, so level 3 alone does
not promise what the guard needs, while a level-2 Claude Code chat can refuse a tool call that a
level-3 chat cannot.

### 8. Versions: the minimum this record needs

A declaration keeps, in `tested`, the harness versions its facts were measured on. TS1 asks for
known-good pins per harness and an in-app *"untested harness version"* warning. `tested` is where
a declaration holds those pins, and the capability card is where the warning shows. FD-14 and
TS1 settle the rest: what the nightly run writes back, and how a project's declaration is
measured.

## ADR 0022, amended

purlis-plane's ADR 0022 says profiles live only in `charter.local.toml` because *"a command in
the committed file could be changed by a merged pull request … and then run on every machine
that pulls it"*, and that *"the local file carries `[harness]` and nothing else"*. This record
changes two things and keeps the reason:

- **A committed file may now name a program**: a project's harness declaration names its
  `program`, as a bare name on `PATH`, with argv templates. It still never holds a command line,
  a path or a shell string, and it cannot replace a built-in.
- **ADR 0022's ask-once is extended to declarations.** A project declaration's program runs on
  this machine only after the operator approves it, and again after any change to it, recorded in
  §5's store. This is the gate that answers 0022's concern for committed data (V24b).
- **`charter.local.toml` carries harness declarations** as well as `[harness]`, for a harness on
  this machine alone or a local replacement of a built-in's declaration.

The rest of ADR 0022 stands: profiles stay local, built-ins never ask, the env refusals hold, and
a tracked `charter.local.toml` is refused.

## ADR 0066, amended

ADR 0066's run section changes in three places:

- **The attribute table.** `harness` is the declared name of any harness (§3), not one of the
  three words. A new attribute, **`level`**, is the level the run was started at (1, 2 or 3). It is
  fixed like the others. What the run actually received, such as a level-2 run that was never
  armed, is recorded on the run (§7) and never changes `level`.
- **The `switch` cause** adds a change of level: a chat that carries on at another level
  (*Continue on*, or a reattach through FD-20) is a new run with cause `switch`.
- **A new cause, `fallback`.** When a run's structured channel ends while the chat carries on at a
  lower level, the run ends and the next run begins with cause `fallback`. Example: an ACP agent
  process exits, or the protocol reports an error purlis cannot continue past, and purlis starts
  the chat again in its terminal. A fall-back is never silent. The chat says it, and the audit has
  the cause.
- **At level 3, causes are learned from the protocol.** ADR 0066 says each cause is learned from
  a hook, the process exit or the host's own action. For a level-3 run, the protocol's session
  events stand in for the hooks: a new session in the same agent process is `clear`, a session
  load on reopen is `reopen`, and the agent process's exit is the process exit. HP-2 and HP-3 map
  each protocol's events.

## The hooks-only rule, amended

`CLAUDE.md` and `AGENTS.md` say: *"Nothing parses harness output to decide anything. A session's
state comes from hooks only."* A level-3 chat has no hooks. It has a protocol, which reports
state as typed messages, the way hooks do. The rule now reads: *"A session's state comes from
hooks only, or, for a chat at level 3, from its structured protocol (ADR 0073)."* A protocol's
messages are data the harness sends to purlis on purpose. They are not the text it draws.
"Nothing parses harness output" still holds for every level. This PR changes the rule in
`AGENTS.md`; `CLAUDE.md` is a link to it, so the two stay identical.

## What changes where

The code does not change with this record.

| Where | What changes |
|---|---|
| `docs/plane-format.md` | FD-14: the declaration schema, first, with the tiers in §3 and §5's approval store |
| `charter-core` | FD-13: the neutral model (session, turn, item, ask, plan, usage) and `HarnessAdapter`. FD-14: the declaration reader; `Harness`'s facts move into declarations; `profiles::KINDS` reads them; the enum becomes the level-2 adapter registry |
| Runs | The `level` attribute and the `fallback` cause join ADR 0066's run model when FD-9 builds it |
| ACP | HP-1 records ACP as level 3 and the adapter programs as user-installed; HP-2 builds the client |
| The capability card | HP-19, from §6 |
| Sandbox | SD-2 compiles per adapter; a declared harness with no adapter is wrapped in ADR 0067's generated OS profile |
| Migration | FR-9 moves existing projects to the format with declarations |
| Extensions | PE-29 lists the level-2 adapter as an extension point; until then adapters are core code |
| `CLAUDE.md`, `AGENTS.md` | The hooks-only rule, amended above (in this PR) |
| `CONTEXT.md` | Gains **Harness declaration**, **Harness level** and **Harness capability** beside Run and Device, **Harness adapter** beside Harness plugin, and **Wrap**; Run lists the level (in this PR) |

## What this costs

- **Level 2 is still a release per harness.** Most of a harness's value to purlis today comes
  from its hooks, and a declaration cannot give it that. A new harness that wants needs you
  without ACP waits for an adapter.
- **A project declaration is one more thing to approve.** It asks once per machine and again on
  every change, which an operator will meet the first time they pull a teammate's harness.
- **The facts become harder to see in review.** A fact in a `match` arm sits beside the code that
  reads it and the comment that says where it was measured. In a data file the comment has to
  travel with the value, and the schema must keep a place for it.
- **Two capability vocabularies** now use the qualified word: forge capabilities (ADR 0070) and
  harness capabilities. Both follow the same rule for silence.

## What was rejected

- **Every harness as a declaration alone, including hooks as templates.** The three hook
  mechanisms purlis uses share no shape. A template general enough to cover them would be a
  small language, and a hook arming that half works reads as level 2 while it misses asks.
- **Keeping the enum, and adding harnesses by release.** Q6′ ruled against it, and it leaves every
  ACP harness waiting on purlis.
- **Levels named by channel** ("the ACP level"). A Codex chat over its app-server gets what an ACP
  chat gets, and W8's coverage label and `minimumHarnessLevel` are about what purlis learns, not
  which wire it came over.
- **Built-in declarations copied into each project.** See §3: a merged pull request could change
  how Claude Code chats are armed, and the copies would go stale.
- **Shipping the ACP adapter programs with purlis.** X34 rules it out, and they are Node or Rust
  programs that trail their harnesses.

## Ruled (V24, 2026-09-30)

1. **A chat in a tab starts in its terminal**: level 2, or level 1 without an adapter, even when
   level 3 is offered. FD-20's Codex-daemon half moves from Later to Next ★, ahead of GM-2,
   starting with a measurement that Codex's own host serves both its TUI and purlis; Claude
   Code's Remote Control half stays Later (V24a).
2. **ADR 0022's ask-once extends to committed harness declarations**: the operator approves the
   program per machine, and again after any change, recorded in
   `.charter/harness-declarations-approved.json` (Clone state) (V24b).
3. **Level 2 always needs purlis code** (core, or an extension after PE-29): no hook templates in
   declarations for now (V24c).
