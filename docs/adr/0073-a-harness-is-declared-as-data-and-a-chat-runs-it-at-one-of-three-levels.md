# A harness is declared as data, and a chat runs it at one of three levels

**Proposed 2026-09-30**, drafted for program-map ticket FD-12 (#652). It follows these of the
operator's rulings:

- **Q6′:** *"Three harness levels (terminal-only, hooks/native, ACP). Harnesses are declared as
  data in the plane, and charter never ships harness code."*
- **X34**, accepted with the consistency review: *"never ships harness code"* means charter never
  ships, patches or wraps a harness binary. Per-harness adapters are charter code, and they are
  data where a template suffices.
- **W8:** the agent run is the unit of governance. Audit reads *"every tool call a level-2 or
  level-3 harness reports"*, with a coverage label per level and an org key
  `minimumHarnessLevel`. **Level 1 stays a complete, supported product, and every paid feature
  degrades to level 1, tested each release.** FD-20 attaches to a harness's own host where it has
  one.
- **W10:** the harness capability card (HP-19), built from these declarations and shown in the
  picker, the chat header and disabled controls' tooltips.
- **V11:** daily-driver parity, whose steer, *Continue on <harness>* and checkpoint features each
  depend on what a harness can do.
- **V21c** (ADR 0066): a run is defined by fixed attributes, and a change to one starts a new
  run.
- **V22** (ADRs 0068 to 0071), and **V23** (ADR 0072, PR #757): "plane" is retired, so this
  record says **project**, and "harness" and "capability" stay in code and Settings.

It builds on [ADR 0050](0050-a-harness-plugin-is-chosen-per-project-through-one-adapter-per-harness.md),
[ADR 0058](0058-an-opencode-chat-loads-charters-shim-for-that-session-alone.md) and
[ADR 0063](0063-every-harness-a-chat-runs-on-is-handed-charters-skills-by-its-own-route.md)
(one neutral model and one adapter per harness), on
[ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md) (the sandbox
compiled per harness) and on [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(`charterd` owns every chat's terminal). It keeps to charter-plane's ADR 0022 (a harness profile
belongs to one machine). FD-13 (the neutral model and `HarnessAdapter`), FD-14 (the declaration
schema), HP-1 (ACP as level 3), HP-19 (the capability card) and PE-29 (the extension-point
catalogue) build on it. Its concept is **Chat**. The declarations themselves are a Project
setting (ST6).

## Where charter is today

charter starts three harnesses: Claude Code, Codex and opencode. What it knows about each is
Rust code:

- **`charter_core::harness::Harness`** is a closed enum of the three, with about twenty methods,
  and each method is a `match` over them. Most are facts measured on one version of one harness:
  the flags that start, name and resume a session; whether charter chooses the session id; the
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

So every harness charter supports is an edit to four modules and a release, and a harness that
charter has never measured cannot run as a chat at all, not even as a plain terminal. The three
it has all run at one level: a terminal that the harness's own hooks report into. None of them
yet runs over ACP or a harness's own protocol.

## The decision

**A harness is a declaration: data that says how to start it, what it can do and which levels
it can run at. Built-in harnesses are declarations too. A chat runs its harness at one of three
levels, fixed for the run: 1, the terminal alone; 2, the terminal with the harness's own hooks
reporting to charter; 3, a structured protocol. Levels 1 and 3 need no charter code for a new
harness. Level 2 needs an adapter in charter. charter never ships, patches or wraps a harness's
program.**

### 1. Three levels, named by what charter learns

| Level | What charter gets | How | What a new harness needs from charter |
|---|---|---|---|
| **1. Terminal** | The process: started, writing, exited. A snapshot of the pane. | `charterd` runs the declared program in a PTY (ADR 0068). | A declaration. |
| **2. Hooks** | The chat's state (working, waiting, idle), its session id, and the tool calls its hooks report, on the hook channel (ADR 0068 §6). | The harness's own hook or plugin mechanism, armed for this chat alone (ADRs 0050, 0058, 0063). | A declaration and an adapter. |
| **3. Structured** | Typed turns, items, plans, usage, and asks that carry their options, answered as data. | charter is the client of a protocol: **ACP** (one client in charter for every ACP harness, HP-2), or where ACP lags, the harness's own protocol (the Codex app-server, the opencode server) mapped into the same neutral model (HP-3). | A declaration, for ACP. An adapter, for a harness's own protocol. |

**Settled by Q6′ as the map reads it.** Q6′'s "hooks/native" is level 2, the harness's native
hooks. The program map puts the Codex app-server at level 3 (HP-3: *"Codex chat at level 3"*,
and HP-16 answers app-server approvals as level-3 asks), so level 3 is "a structured protocol",
with ACP as the one charter writes once.

**Settled by W8: level 1 is a complete product.** A level-1 chat starts, is sandboxed, is shown,
is stopped by the kill switch, is saved and is resumed where its declaration says how. Its asks
are answered from the pane (HP-6: a snapshot and a typed reply). What it lacks is said on its
capability card, never discovered.

**Every level keeps the ones below it where it can.** A level-2 chat is still a terminal. A
level-3 chat has no terminal of its own unless the harness's host serves both (FD-20), so a
level-3 chat is not "level 2 plus more": see §6 and the ruling questions.

### 2. A declaration is data, in the project or in charter

A **harness declaration** says one harness's name, its program, how to start, name and resume a
session, the levels it offers, and its capabilities (§5). FD-14 fixes the schema in
`docs/plane-format.md` (the project format's specification) before any code reads it. This
sketch shows the shape only:

```toml
# harnesses/gemini.toml: a harness declared by data alone
name = "gemini"                     # the word a profile's `kind` names
title = "Gemini CLI"                # what the operator calls it, on every surface
program = "gemini"                  # a bare name found on PATH: never a path, never a shell string
env = ["GEMINI_*"]                  # the harness's own configuration namespace (Harness::env_passed)
tested = ">=0.9, <0.12"             # the versions these facts were measured on (TS1)

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

[capabilities]                      # §5: each is yes, no with a reason, or unknown
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
| `unreported` | derived from the capabilities that are *no* (§5), so the sentence and the card never disagree |
| `skills` | a capability: `Plugin` and `Config` need an adapter; `Briefing` (ADR 0063's neutral fallback) is the default for every declared harness |
| `state_hooks`, `disarmed_by`, `armed_with` | the level-2 adapter |
| `harness_plugin::Adapter` | the adapter; a harness without one is *not supported yet*, as ADR 0050 already says |
| `sandbox::Form` | the adapter where the harness has a sandbox of its own; otherwise ADR 0067's generated OS profile |

**The built-ins are declarations charter ships.** Claude Code, Codex and opencode are three
declaration files in charter's source, read by the same reader as a project's, and shown by
`charter harness show <name>` (FD-14 names the command). Moving them is FD-14's migration, and a
project on an older format reads them through FR-9. The enum `Harness` becomes the adapter
registry for level 2, keyed by declared name.

**The harness × model matrix is not in the declaration.** It is its own project data (M10,
MS-2), keyed by the declared name.

### 3. What "never ships harness code" means (X34)

**charter never ships, patches or wraps a harness's program.** *Wrap* means standing between the
harness and its user or its model and changing what either sees: a repackaged binary, a
replacement under the harness's name, a proxy that edits its traffic. These are not wrapping:

- **An adapter**: charter code that arms a harness through the harness's own documented
  mechanism, for one chat, with nothing written into the harness's config (ADR 0050). The
  bundled Claude Code plugin, the Codex hooks and the opencode shim are adapters.
- **Running the unmodified program in charter's PTY, sandbox or external backend** (ADRs 0067,
  0068). The harness gets the same argv and sees the same files it would outside, minus what the
  sandbox denies.
- **The shell-tab shims** (ADR 0062). A shim prints one line and `exec`s the real program with
  the same argv and environment. It does not stay between them.
- **An ACP adapter program** such as `claude-agent-acp` or `codex-acp`. The user installs it, and
  charter spawns it like any declared program. **Settled by Q6′ and the HP-1 row:** charter
  never ships one. HP-1 writes that record.

**Data where a template suffices.** Everything level 1 needs is a template: argv, environment
names, and the facts in `[terminal]`. Everything level 3 over ACP needs is the ACP client charter
already has, plus the command that starts the agent. So a new harness at level 1 or at level 3
over ACP is **a declaration and no release of charter** (FD-14's acceptance, HP-14). Level 2 is
code: each harness's hooks differ in where they are registered, what they report and how a chat
is armed for itself alone, and each of today's three took an ADR of its own.

### 4. Who may declare a harness, and what runs

A declaration names a program, and a program runs on a click. charter-plane's ADR 0022 keeps
profiles out of the committed file for exactly this reason: a merged pull request could change
what runs on every machine that pulls it. So:

- **A declaration never carries a shell string or a path.** `program` is a bare name, found on
  `PATH` as a profile's `command[0]` is. Templates take `{id}` and `{name}` and no other
  substitution. Env names are limited to the declared namespace, and ADR 0022's refusals
  (`CHARTER_*`, names that read as a credential) apply to them.
- **A project declares new harnesses. It may not take a built-in's name.** Claude Code's
  declaration decides how its chats are armed, including the guard (ADR 0050's pins). A file that
  a merged pull request can change must not decide that.
- **This machine approves a project's declaration once before it first runs, and again when it
  changes**, recorded as `.charter/harness-profiles-launched.json` records a profile today
  (clone state, and it fails towards asking). A built-in needs no approval, since it came with
  charter's signature.
- **`charter.local.toml` may declare a harness, or replace a built-in's declaration**, as a local
  profile may replace a built-in profile today (ADR 0022). It is this machine's own file, under
  ADR 0050's rule that a Local file git would carry chooses nothing.
- **A profile still says which program a chat runs on this machine.** Its `kind` names any
  declared harness, not only the three.

### 5. Capabilities: silence never reads as "yes"

A **harness capability** is one thing a harness does or does not do for a chat, such as report
that it is waiting. It is not an extension's capability, and the word is always qualified, as
ADR 0070 did for forges. Each is one of:

- **yes**;
- **no**, with the reason and the fallback charter uses (for example, *no* to "reports waiting",
  with the fallback "the chat does not show needs you");
- **unknown**, which charter treats as *no* and says so. This is ADR 0070 §2's rule and
  `checks.rs`'s: no word for silence reads as passing.

**The flags are the ones a ticket reads now:** the three the table in §2 moves out of `Harness`
(`reports_its_process`, `reports_its_start_before_the_first_prompt`,
`keeps_conversations_by_directory`), `reports_waiting` (needs you), `resumes_by_id`,
`steers_mid_turn` (HP-15), `reports_child_agents` (FD-18), `reports_usage`,
`denies_before_run` (a tool call can be refused before it runs, which the guard needs),
`rewinds` (MH-6), the skills route and per-chat plugins. A later ticket adds the flag it reads,
and a flag's answer can differ by level, since the same harness can report more over its own
protocol than through its hooks.

**The capability card reads the declaration.** Settled by W10 and ADR 0072 §3: the card is
labelled *What <product> can do here*, each *no* is one plain line that says what the user will
or will not see, and neither "capability", "harness" nor "level" appears on it. A disabled
control's tooltip is that line. `unreported` becomes those lines, so there is one source.

### 6. A chat's level is chosen when it starts, and fixed for the run

- **The level is one of the run's fixed attributes.** Settled by V21c: a run is defined by fixed
  attributes (persona, harness, profile, model source, device, sandbox), and a change to one
  starts a new run. The level joins them. *Continue on <harness>* (V11) and a reattach at another
  level are therefore new runs.
- **The declared level is a ceiling, and the chat shows what it actually gets.** A level-2 Codex
  chat whose hooks the operator has not yet trusted reports nothing, which is level 1 in effect.
  The chat says so from its first turn (as `unreported` does today), and the audit records the run
  at the level it reached, not the one it asked for.
- **Which level a chat with a tab starts at** is ruling question 1 below. A chat with no
  terminal shown (a dispatched, headless or remote chat) starts at the highest level its harness
  offers on this machine, as the synthesis map's Q-6 recommended for dispatch, headless, remote
  and inbox answers.

### 7. Governance follows the level

**Settled by W8:**

- **The audit labels each run's coverage by its level:** level 1, *process only*; level 2, *the
  tool calls its hooks report*; level 3, *every tool call its protocol reports*.
- **`minimumHarnessLevel`** is an org key, compiled through C9's strictest-wins policy. A chat
  below it does not start, and says why.
- **Every paid feature degrades to level 1**, and a release test holds it there.

What W8 does not settle is whether a level promises a particular guarantee (ruling question 4
below).

### 8. Versions

A declaration's facts hold for the versions it names in `tested`. **Settled by TS1:** outside
that range the chat starts, and the card says *untested version*. The nightly compatibility run
updates the built-ins' ranges, and a project's declaration is measured by whoever wrote it.

## What changes where

The code does not change with this record.

| Where | What changes |
|---|---|
| `docs/plane-format.md` | FD-14: the declaration schema, first, with its tier (committed; the approvals are Clone state, ADR 0069) |
| `charter-core` | FD-13: the neutral model (session, turn, item, ask, plan, usage) and `HarnessAdapter`. FD-14: the declaration reader; `Harness`'s facts move into declarations; `profiles::KINDS` reads them; the enum becomes the level-2 adapter registry |
| ACP | HP-1 records ACP as level 3 and the adapter programs as user-installed; HP-2 builds the client |
| The capability card | HP-19, from the capabilities in §5 |
| Sandbox | SD-2 compiles per adapter; a declared harness with no adapter takes ADR 0067's generated OS profile |
| Migration | FR-9 moves existing projects to the format with declarations |
| Extensions | PE-29 lists the level-2 adapter as an extension point; until then adapters are core code |
| `CONTEXT.md` | Gains **Harness declaration**, **Harness level**, **Harness adapter** and **Harness capability** |

## What this costs

- **Level 2 is still a release per harness.** Most of a harness's value to charter today comes
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
  mechanisms charter uses share no shape. A template general enough to cover them would be a
  small language, and a hook arming that half works reads as level 2 while it misses asks.
- **Keeping the enum, and adding harnesses by release.** It is what Q6′ ruled against, and it
  leaves every ACP harness waiting on charter.
- **Levels named by channel** ("the ACP level"). A Codex chat over its app-server gets what an ACP
  chat gets, and W8's coverage label and `minimumHarnessLevel` are about what charter learns, not
  which wire it came over.
- **Letting a project replace a built-in's declaration.** It would let a merged pull request
  change how Claude Code chats are armed, including whether the guard is loaded.
- **Shipping the ACP adapter programs with charter.** X34 and Q6′ rule it out, and they are Node
  or Rust programs that trail their harnesses.

## For the operator's ruling

Each of these is left open by Q6′, X34, W8 and W10. Each has a recommendation. The calls those
rulings already make are marked *Settled by* in the body.

1. **A chat with a tab starts in its terminal (level 2, or level 1 without an adapter), even when
   its harness offers level 3.** Level 3 is the default only for chats with no terminal shown, and
   comes to tab chats through FD-20, where the harness's own host serves its terminal and charter
   at once. *Recommend yes.* A level-3 chat has no harness terminal, so defaulting to it means
   charter draws the conversation itself, a second product the map has no ticket for. The cost is
   W1's: a Codex chat in a tab keeps its missing needs you until FD-20 or HP-8's structured
   chats.
2. **A committed declaration may name its program**, as a bare name on `PATH`, and this machine
   approves it once before it runs and again when it changes. It may not take a built-in's name.
   *Recommend yes.* FD-14's acceptance needs a harness to run from a declaration alone, and the
   approval the profiles already use answers ADR 0022's concern about committed commands.
3. **Level 2 always needs an adapter in charter**: no hook templates in a declaration for now.
   *Recommend yes.* The three hook mechanisms share no shape, and a templated arming that half
   works would miss asks while claiming level 2.
4. **A policy that needs a specific guarantee names the harness capability, not a level.**
   `minimumHarnessLevel` stays as W8's coarse gate, and the policy schema also takes required
   capabilities (for example `denies_before_run`). *Recommend yes.* An ACP agent asks only when it
   chooses to, so level 3 does not by itself promise what the guard needs, and a hooked level-2
   Claude Code chat can refuse a tool call that a level-3 chat cannot.
