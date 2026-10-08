# A chat runs in a sandbox purlis compiles for its harness, and it shows when one does not

**Accepted 2026-09-30** by the operator (ruling V21; program map SD-1; rulings Q10, C9, W8, V16
and X18). It re-opens [ADR 0028](0028-containment-checks-a-path-and-does-not-hold-it.md), whose
own re-opening clause this decision triggers.

## Where purlis is today

Chat isolation is whatever each harness does by default, and purlis does not add to it or make
it the same across harnesses (gap G1 in the security review). A Claude Code chat runs as the
operator, with purlis's hooks and nothing else. A Codex chat runs under Codex's own default
sandbox, and purlis passes it no sandbox setting on purpose: the test
`a_codex_chat_is_handed_no_approval_or_sandbox_setting` stops purlis from loosening it. An
opencode chat is not confined at all. Egress is open wherever the harness leaves it open, and a
secret goes to whatever process the model names (gap G2).

The one thing that stands between a chat and a vault is the `PreToolUse` guard. `hooks.md` says
what that guard is: *"a guard against mistakes, not an attacker with shell access as your user"*.
It recognises spellings, and a model that is working against it can get round it (gap G13).

ADR 0028 accepted a check-then-open race in `contain` because *"nothing in charter makes an agent
less privileged than charter itself"*. Its 2026-09-22 amendment names the trigger that re-opens
it: *"The day a sandbox lands … for any subprocess charter starts."* This ADR is that day.

## The decision

**Every chat purlis starts in a new plane runs sandboxed. The policy is one harness-agnostic
schema, and purlis compiles it into each harness's own mechanism. If the sandbox cannot be
applied, the chat does not start. The operator can turn it off for one chat, and that chat's tab
shows it for as long as the chat lives. Every opt-out is audited.**

### 1. On by default for new planes, and it fails closed

- A plane that purlis creates after this ships records `sandbox = "on"` in its committed
  settings. Turning the sandbox on is a restriction, so a plane may carry it (ADR 0035: *"a plane
  can restrict, never grant"*). **A plane can never carry `off`.** A committed file cannot
  loosen what a chat is confined to, just as it cannot pre-approve a permission.
- An existing plane keeps running as it does today. The first time purlis opens it after the
  upgrade, a notice offers to turn the sandbox on, and nothing flips on its own (see open
  question 1).
- **`failIfUnavailable` is the only mode.** If this machine cannot apply the compiled policy (no
  bubblewrap, user namespaces disabled, a backend missing), the chat does not start. The refusal
  names what is missing, offers SD-30's prerequisite install, and puts the per-chat opt-out
  inside the refusal itself, so a first run still gets to a working chat within FR-1's five
  minutes (X18).

*Amended 2026-10-06 (the operator's grilling of 2026-10-06, Q11; spec #1330):* **the project
configures the sandbox once, and for hosts that is a grant.** The project's committed settings
choose the Internet access presets (section 3) and the project's own hosts, and everyone who
opens the project follows them, with no per-person approval. This departs from ADR 0035's
*"a plane can restrict, never grant"* for hosts only: `off` still never travels, and no
committed value removes a denial class (section 5). Three safeguards stand in for the approval:

- **A chat never writes sandbox settings.** The manifest denial at the project root (section 5,
  as amended below) keeps a chat from editing them, and a brokered write (section 2) refuses any
  change to a sandbox key.
- **Each teammate sees a one-time Notice** when the project's sandbox changes, naming what
  changed, so nothing widens unseen.
- **Policy locks values.** An admin's policy, kept on the machine or for the organisation (C9,
  section 4), can lock any value, and the strictest value wins. A locked value shows "locked by
  policy" in Settings.

The levels are **Policy**, **Project** (committed, the team's baseline) and **You** (personal
hosts on this machine, unless policy forbids them). Workspaces have no sandbox settings. A host
allowed from a block's Notice for one chat stays section 3's audited exception; allowing it in
the project is a change to the project's committed settings and follows this rule.

*Amended 2026-10-06 (#1362; D-1362-1 and D-1362-5):* **a persona carries the hosts its chats
reach, as a fourth level.** The levels are Policy, Project, You and **Persona**: the project's
committed settings may grant one persona's chats hosts of their own
(`[sandbox.personas.<persona>] hosts`), which a chat running as that persona reaches and no
other chat does. They follow the project's rule above: committed, told once to each teammate
when they change, and lockable by policy.

- **They live in the project's committed settings, never in the persona's own definition**
  (D-1362-1). A chat may edit its own persona's charter (section 2, as amended), so a grant
  written there would be a grant a chat could write itself; the project's settings file is one
  no chat writes.
- **They are fixed when the chat starts.** A chat that names no persona takes the default
  persona's; one that switches persona mid-chat keeps the grants it started with until its next
  start.
- **A handoff never widens what a chat reaches** (D-1362-5). A chat opened by another chat's
  handoff holds the grants the asking chat itself runs with, taken from purlis's own record of
  that chat (its own hold included, so a chain of handoffs never climbs) and never from the
  request, until the person allows its own on its tab. A handoff to a persona whose hosts the
  asking chat already reaches, and a chat the person starts from the window, hold their own.
- **A Resume never widens it either** (D-1362-6). A session record's persona is written from
  what the chat said it was, so a Resume as a persona whose hosts reach past the default
  persona's holds the default's grants until the person allows its own. The record's view says
  the persona's hosts and that a Resume holds them back.
- **Allowed grants apply at a start.** The person's Allow takes effect when the chat starts
  again; the tab offers Restart now, which starts it again resuming its conversation once its
  turn has ended.

*Amended 2026-10-07 (#1435, with ADR 0090 accepted as amended):* **a chat's persona is fixed for
its life, and a dispatch grant replaces the hold on a handoff.** No chat switches persona
mid-chat, so the second half of "they are fixed when the chat starts" has no case left. Once
dispatch ships (ADR 0090 as amended, change 3), a chat that another chat started under a
dispatch grant starts with its own persona's hosts, because the grant is the person's yes to
that, and D-1362-5's hold is retired for it. Until then a handoff holds as written above.

*Amended 2026-10-09 (#1362; D-1362-7):* **a persona's hosts are in force on a machine only once
the person there allowed them, bound to the list they were shown.** "Told once to each
teammate" above becomes "asked on each machine" for the Persona level. A persona's hosts are
committed settings a teammate can change, and they are the hosts that reach past the presets
into private networks, so a pull alone never widens what a chat reaches. The project view
shows each persona's list as a chat would reach it, and Allow keeps the digest of exactly that
list on this machine. A list that changes in any way after it was allowed grants none of it
until it is allowed again; never a part, never the old one. The Allow is the window's alone,
audited, listed in Settings' Granted list credited to the persona, and revocable there. The
project's own hosts keep the rule above: they apply and are told.

### 2. One schema, compiled per harness

The policy is neutral data. Each harness gets an adapter that compiles it, in the same shape as
ADR 0050 and ADR 0063: one model and one adapter per harness. The fields sketched here show the
shape, and SD-2 fixes their final names in `docs/plane-format.md` before any code reads them:

```toml
[sandbox]
mode = "on"                        # a plane may say "on"; only a human, per chat, says "off"
egress = ["model-providers", "forge", "toolchains"]   # named presets (section 3)
# writable: the chat's own worktree and a per-chat temp directory, nothing else
# denied: the classes in section 5, which no plane can remove
```

| Harness | What purlis compiles the policy into |
|---|---|
| Claude Code | The `--settings` blob purlis already passes gets a `sandbox` object: enabled, `allowUnsandboxedCommands: false`, `failIfUnavailable: true`, filesystem read and write rules, and `network.allowedDomains` from the egress presets. `--settings` sits above project settings, and project settings cannot turn filesystem isolation off. |
| Codex | **purlis's own wrap, as for opencode** (#1123, amended 2026-10-04): purlis runs the whole of Codex inside the Seatbelt profile it writes, with Codex's own sandbox off inside it (`--sandbox danger-full-access`, last among its flags) under ruling V21 4, because macOS applies one Seatbelt profile to a process and Codex's own could not be applied inside purlis's (measured: every command failed). The rest of this row is what purlis compiled before #1123, and no chat starts with it. *Before #1123:* held back (ruling V87f); Codex's own workspace-write, set explicitly rather than inherited: a permissions profile that extends `:workspace`, selected with `default_permissions`, which holds the denied paths, and Codex's own network proxy (`--enable network_proxy`) holding egress to the presets. *Amended 2026-09-30 (SD-2 slice 2):* this row first said `-s workspace-write`. On codex-cli 0.147.0 any `-s` switches Codex to its legacy sandbox mode, which reads no permissions profile, so the denied paths and the proxy's allowlist would be dropped. The profile is the only form that carries them, and a `-s` in a chat's own words refuses the chat. The test that forbids Codex sandbox arguments is rewritten so it forbids only *looser* values: purlis may tighten Codex's sandbox and still may never loosen it. |
| opencode | opencode has no sandbox of its own, so purlis generates a Seatbelt profile (macOS) or a bubblewrap invocation (Linux) and starts opencode inside it. The profile is generated by purlis's Rust core: purlis does not ship `sandbox-runtime` (no language mixing in the shipped app), which stays available as a test oracle. |

*Amended 2026-10-03 (rulings V73 and V73a, SD-2 slice 3):* opencode runs whole inside the
profile, so it can write more than the chat's worktree and per-chat temp directory that
section 2's sketch names, but only what a turn writes (measured): its sessions database, and
what is in its log and storage directories (never those directories themselves), and the one
file `.gitignore` in its config directory, without which a first run stops. It never writes
opencode's own state directory: it is started with one of its own in its temp directory, so
no lock or setting a later opencode reads can be planted there. No directory and no link is
made in the data directory, so none is moved in with links already in it. A chat whose
directory holds opencode's own directories, or is inside one, is not wrapped at all. Its config and cache directories
hold plugins and packages a later, unsandboxed opencode loads, and so does its data
directory: its credentials file can name a remote config that starts servers, and its
snapshot repositories are directories git is later run in. So the config and cache stay
read-only, the credentials files are never written, no snapshot is written (a wrapped chat
is started with opencode's snapshots off), and no link is made where a later opencode
writes. purlis makes opencode's own directories before the wrap, and a chat may neither make
nor move one, so none is moved out, changed and moved back. Every denial class in section 5 still wins over these grants. The Linux wrap is
#1040.

*Amended 2026-10-03 (ruling V73c):* opencode's hooks run inside the wrap, so every line they
send is the chat's own claim about itself (the class of #873), never proof that a hook ran.
Its hook spool stays denied, so a line the app does not take cannot be kept: it is shown in
the chat's window instead, never dropped in silence. Moving opencode's hooks outside the wrap
is #1069.

*Amended 2026-10-04 (#1123, D-88q):* Codex runs whole inside the same profile, with a Codex
home of the project's own (`CODEX_HOME`, under purlis's data home, one per project), never
the operator's `~/.codex`, which the profile neither reads nor writes (only Codex's installed
program in it is read, to run it). The reason is measured: Codex keeps state every thread
shares and a resume hands the model (each thread's goals, its queue, the threads' table, the
sessions), so a chat that could write the operator's home could plant instructions in a thread
the operator later resumes outside any sandbox. purlis seeds the project's home before the
wrap with the operator's login (copied when the project's is missing or older) and a config
it writes itself, which marks the chat's folder and every folder above it untrusted and trusts
exactly the hooks purlis arms (D-88r: Codex's own trust records, keyed and hashed as Codex
keys and hashes them, so purlis's hooks run and no other hook is trusted). What a
turn writes in that home is granted, as V73 and V73a grant opencode's (measured: a turn stops
without its locks and installation id): the threads' state, sessions (a folder for each day),
history, shell snapshots, writer locks, log, installation id and a refreshed login, and its
memories, goals and queue (*amended by D-88s*: Codex's interactive screen opens all three to
write at every start and does not start without them, measured; D-88q had refused them).
Never written: its config, skills, plugins and helper links. No link
is made in it, and no folder but a day's sessions; a folder can still be moved in under a
date-shaped name, since Seatbelt does not tell a rename from a create, so purlis takes every
link and every folder that is not a day's out of the home's folders before each start. One
sandboxed Codex chat can still influence what a later sandboxed Codex chat of the same project
loads (its memories, goals and queue, sessions, shell snapshots and history), inside the
wrap, but never an unsandboxed run's Codex state: no chat reads or writes the operator's own
home or another project's. Per-chat isolation of the shared stores is #1150. Codex is handed the system's certificate authorities as a file, since the
profile refuses the keychain's service. The Linux wrap is #1040 for both harnesses.

**The compile is total, or the chat is wrapped.** Every denial class in section 5 must hold for
every harness. When a harness's native sandbox cannot express a class (for example, a read-deny
that its write-only confinement cannot state), purlis wraps the harness in its own generated
profile as well. Where both apply, the stricter answer wins. A harness that has neither route on
this machine fails closed, as in section 1.

**A `purlis` command that a chat runs is part of that chat.** It inherits the chat's sandbox and
its denials. Anything that needs to reach past them (resolving a secret, writing the audit,
recording a session outcome) is asked of `purlisd` over its socket and is never done from
inside the chat. Each compiled profile allows the chat to connect to that socket and nothing more.

*Amended 2026-10-06 (the operator's grilling of 2026-10-06, Q1 to Q4; spec #1330):* **a write to
the project's own files is a brokered write.** The chat asks `purlisd` to make it, `purlisd`
makes it with the same core code the terminal's `purlis` uses, credited to the chat's run, and
the chat's sandbox does not widen. The brokered writes are named, and these are all of them:

- session records;
- memory: the workspace's own, a persona's and shared;
- todos;
- the workspace vision and the sections of `workspace.md`;
- persona files;
- creating and removing a workspace;
- creating a handoff;
- the git plumbing a sandbox forbids: clone, checkout and worktree creation, so a repo's
  `.git/config`, hooks and protected checkout files are written outside the sandbox. A chat's
  own edits to those files stay denied (section 5).

A write that is not on this list is not brokered until an amendment here adds it.

- **Two entrances, one operation.** purlis's MCP tools for the chat (a session record, a
  workspace note by section, the workspace vision, a persona's memory) and the `purlis` command
  run inside a sandboxed chat both reach the same operation in `purlisd`. The command does not
  write: it forwards the request over the chat's own hook socket, which each compiled profile
  allows and nothing past it (#1328), and prints the same output it would have.
- **Instruction files raise a Notice.** A brokered change to `workspace.md` or a persona charter
  is written with no approval. It raises a Notice with Review and Revert, and the chat's tab gets
  a quiet mark. Session records, memory and todos raise nothing.
- **Direct edits are refused, with the way round.** A chat's own edit or write to one of these
  files is refused, and the refusal names the tool to use instead.
- **No brokered write changes a sandbox key** (section 1, as amended).

### 3. Egress is named presets, and anything unlisted is a visible exception

- **Toolchain and forge presets (SD-4):** common package registries, plus github.com, gitlab.com
  and the self-managed forge hosts this machine's forge logins already name (ADR 0055).
- **Lane presets (SD-31):** `model-providers`, `forge`, `localhost`, `browser`, `database:<vault>`
  and `production`. `production` carries the two-person rule. Each lane runs under its own preset
  and does not open its own hole.
- **A new plane starts with `model-providers`, `forge` and `toolchains`.**
- **A host no preset lists is refused and shown to the operator.** It never passes silently.
  If the operator allows it, that is an audited exception for that chat, not a change to the
  plane's policy.
- Enforcement uses the harness's own proxy where it has one. For a harness without one, the
  generated profile allows network traffic only to purlis's local egress proxy. That same proxy
  is the long-term home of gap G2's broker, where a secret is released only to the hosts it is
  bound to.

### 4. purlis never writes a vendor's managed tier. It only adds stricter overlays (W8, SD-32)

- purlis **never writes** a harness's managed or admin tier: the files, profiles and registry
  keys an organisation's MDM owns. Those belong to the customer's administrators.
- purlis **reads** the effective managed policy and shows each value it fixes as "locked by
  <vendor> admin" in the chat's sandbox view.
- purlis's compiled policy is an **overlay**. It may tighten what the managed tier allows and it
  may never loosen it. Where the managed tier is stricter, it wins, and purlis shows that it did.
  Where the managed tier forbids an overlay purlis needs for a denial class, the chat is wrapped
  (section 2) or fails closed. It never runs with that class missing.
- C9's layers (MDM profile, Windows policy key, `/etc/charter/policy.json`, server org policy)
  compile into purlis's own overlay, with the strictest value winning. SD-14 **exports** org
  policy as each harness's native managed artifact, for the customer to push through their own
  MDM. purlis exports that artifact and does not install it.
- **The claim purlis makes is exactly this:** *enforced for agents purlis launches; for the
  whole fleet, purlis exports each harness's managed settings.* Nothing in the product, the docs
  or the trust page may claim more.

### 5. What a chat's sandbox always denies (V16)

These are classes, not a list of paths. SD-2 turns each class into rules for each harness, and
each class has its own test. No plane, persona or preset can remove one. Only the per-chat
opt-out in section 7 lifts them, and the audit records when it does.

1. **Chats never read a vault directly.** Every vault provider's storage, whether a plane file,
   a keyring item or a provider's local session, is denied to the chat. `purlisd` resolves a
   secret and hands it to the command it runs, so the approval gate that V15 sets (SD-37..SD-39)
   is enforced rather than advisory (SD-9). The `PreToolUse` guard stays, because it can explain
   a refusal and the sandbox cannot (gap G13).
2. **purlis's integrity state is denied to chats:** the audit directory and the device key, and
   every chat's hook spool but its own. `purlisd` is the only writer of the first two. A chat's
   hooks may append only to that chat's own spool, never to another chat's, and the host verifies
   and seals each spool as it drains it ([ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
   §6, corrected 2026-09-30 by ruling V22). A chat that could change them could change the record
   of what it did.
3. **Human powers are unreachable from a chat.** The credentials behind the terminal,
   fleet-MCP and approval client scopes are unreadable inside the sandbox. `purlisd` also
   refuses those scopes to any connection from a chat's process tree, so an agent can never
   answer its own asks or approve its own secret requests (V16a).
4. **On a runner, the chat is also denied `purlisd`'s install files and the git internals
   `purlisd` operates on** (RR-5, V16d). A runner chat is sandboxed by default, and V19 makes
   this a blocker for the first runner slice.
5. **A chat never writes what a program run later, outside any sandbox, loads** (added
   2026-10-03, rulings V73b and V73d). At any depth of every directory it may write, its own
   directory, its temp directory and any other the harness is granted alike, so a protected
   name is neither made there nor built in one and moved into another with its parent: git's
   config and hooks in every clone, worktree and submodule (`.git/modules/…`), and the `.git`
   itself, so one is never moved into place; the hook managers' `.husky/` and `.githooks/`;
   shell startup files; Claude Code's `.mcp.json` and `.claude`'s `settings.json`,
   `settings.local.json`, `commands/`, `agents/` and `skills/`; opencode's `opencode.json`,
   `opencode.jsonc`, `tui.json`, `tui.jsonc` and `.opencode/`; Codex's `.codex/` and
   `.agents/`; the editor folders `.vscode/` and
   `.idea/`, and direnv's `.envrc`; and `charter.toml`, so a chat at the plane root
   cannot take its `[sandbox]` out. Resolved when the chat starts and denied as paths: the
   directory every `core.hooksPath` git would use names, and every script a protected config
   names for a harness to run (a hook command, an MCP server's command and arguments, a
   plugin file). A directory a later git takes for a bare repository holds no protected name,
   so it is a stated residual with its own checklist (#1100). Each harness's compiler adds
   this to what the harness's own sandbox already denies, after what it lets the chat write,
   and never replaces it. Where a harness's own sandbox can state a name only with what is
   below it, the `.git` itself is not held for that harness, and that gap is #1065.

*Amended 2026-10-07 (#1452, D-1452-11; amends class 2):* **class 2 also holds the dispatch
records.** `<state>/app/dispatches/`, where the app keeps one record for each dispatch with its
brief and its report, is denied to a sandboxed chat for reading as well as writing, under every
spelling of the state folder and in what every harness is compiled, as the hook spool is. A
brief or a report written for one persona is not readable by a chat running as another. A chat
gets its own dispatch's report on the delivery path, and its own list from the app's answer,
never from the file.

*Amended 2026-10-07 (D-T59-19, train 59; amends class 2):* **class 2 also holds what waits to
be told to a chat on its next turn, where the harness's hooks run outside its sandbox.**
`<state>/handbacks/`, where the app leaves a dispatched chat's report and its own word on a
dispatch the person was asked about, is denied for reading as well as writing, under every
spelling of the state folder, to a chat of a harness whose sandbox confines its tools and not
its hooks (`sandbox_holds_what_it_starts` says no: Claude Code today). The app leaves those
files and purlis's hooks take them, so nothing inside such a chat needs the folder; a chat that
could write there could put a line in purlis's voice into another chat's turn, and one that
could read there could read a report written for another chat.

**It is not denied where the sandbox is a wrap around the whole harness** (Codex and opencode
today), because the hooks that deliver run inside the wrap (amended 2026-10-04, V73c) and read
and then remove each file: a denial there would cut every report, stop word and answer off.
On those harnesses a chat can still read and write that folder. What stands there is the check
each file gets as it is read (a name purlis would not have written, a summary it would not
have sent, drop the file), until delivery moves onto the hook socket (#1457) and the denial can
cover them too.

One command a chat can run did move a folder there: `purlis workspace rename` moves
`handbacks/workspace-<old>`. Run inside a chat that is denied the folder it now leaves it and
says so, and the app moves it when the project is next opened, from the rename's own journal.
Where the project's sandbox is off, nothing here holds, as for every class.

*Amended 2026-10-03 (rulings V87d and V87f):*

- **Linked folders, every harness.** No sandboxed chat starts in a folder reached through a
  link. Every folder from the plane's own, as the kernel names it, down to the chat's must be a
  real directory, and the chat's folder must be inside the plane.
- **`charter.toml` must be readable.** A `charter.toml` that cannot be read starts no chat: it
  may say `[sandbox]`, so it never reads as "not set". That covers one that is not a regular
  file (a link, dangling or not, a FIFO, a device, a socket or a directory), one larger than
  purlis reads, and such an entry above a chat on every path a chat starts by.
- **Folders between a chat's own and a denied path.** Each is held as an entry, so it is never
  moved away with the denied path inside it and replaced. The chat's own folder and its temp
  folder are held too, in purlis's own compiled sandbox.
- **A sandboxed project refused Codex until #1123.** Codex's own sandbox resolves its paths
  again at every command, so a running chat, or another chat that can write above its folder,
  could move what the compiled profile names, and no class held (measured). *Amended
  2026-10-04 (#1123):* Codex now runs inside purlis's own compiled sandbox, as opencode does,
  and holds every class there, the later-code class at any depth included (measured live: a
  folder holding a protected name moved aside, changed and moved back; the temp folder and the
  chat's folder swapped for a link; each refused). Codex's own sandbox is off inside it
  (ruling V21 4), since Seatbelt cannot apply a second profile. Its Codex state lives in a home of the
  project's own (D-88q), and the operator's own Codex home is neither read nor written, so
  nothing a sandboxed chat writes reaches a thread resumed outside the sandbox (measured: a
  goal planted for one of the operator's threads was refused, and the operator's resume of it
  carried none). Inside the project's home, the memories, goals and queue, which every thread
  shares and a resume hands the model, are written, as are the sessions, shell snapshots,
  history and the threads' state (D-88s), so one sandboxed chat can influence what a later
  sandboxed chat of the same project loads, inside the wrap, never an unsandboxed run's Codex
  state. Files a chat writes in its own folder
  (an instruction file, a skills folder) are read by a later Codex started there, unsandboxed
  too, as for every harness. On Linux, where purlis cannot
  wrap it yet (#1040), a sandboxed project still starts Codex only on a person's audited
  opt-out.
- **The program is the harness, and lies where no chat writes** (ruling V87g, widened by
  D-88d). A harness's own sandbox binds only that harness, so a Claude Code profile starts
  sandboxed only if its program answers `--version` as Claude Code. And no sandboxed chat starts
  on a program anywhere the compiled sandbox lets that chat write: the project, the chat's own
  folder, the folders the wrap grants it, and the system temp folders. The program is resolved
  once. A relative path is refused, and so is a program whose path as written, or whose real
  path after links, lies in one of those places. Every other word of the command is held to
  the same places (D-88g), wherever in the word a path begins: an interpreter outside, handed a
  script a chat wrote, would run the chat's code. A word is read every way a program might
  read it: as written, JSON-unescaped, percent-decoded, and each after the other. In each
  reading, every part that begins at a `/` is asked as written, with `.`, `..` and `//` folded
  by text, and as the system resolves it, so a flag's attached value, a `file:` URL, an argument
  file, inline code and inline JSON, and a not-yet-made file reached through `..` or a link, are
  all seen. Every piece between two characters no file name holds is asked against the chat's
  folder, and one that names something there is refused. A word that names no file, such as a
  flag or a model's name, is left alone. A word with a writable place's path inside it, or a
  piece naming a file in the chat's folder, is refused even where it means something else
  (D-88j). A word over 4 KiB is refused outright, so the check stays fast.
  - **The word check is a lint, not the boundary** (D-88k). It catches a profile that hands
    its program a chat-writable file by mistake. For opencode and Codex the boundary is
    purlis's own wrap around the whole harness. Claude Code is not wrapped: Seatbelt cannot
    apply Claude Code's own sandbox inside purlis's (measured: every command failed with
    `sandbox_apply: Operation not permitted`), and turning it off inside the wrap is a ruling
    not yet made (#1150). Encodings the check does not read stay a residual for Claude Code
    until then.
  - **The probe runs inside purlis's wrap** (D-88k, #1123): on macOS, the `--version` probe
    runs under a profile that lets it write nothing but a temp folder of its own and reach no
    network, so a command that loads a file a chat wrote gets nothing out of it. The Linux
    probe is not wrapped yet (#1040).

  The probe and the terminal both run that real path, so the file checked is the file run. The probe runs with the chat's own environment and
  folder, and a probe that does not finish in time is killed with its whole process group. Each
  refusal is one sentence, and a person may still start the chat without the sandbox.
  - **What the check does not prove.** A program can print any answer. The `--version` check
    catches a profile that is not Claude Code by mistake, not one written to pass it. Against
    that, the guarantee rests on the person's approval of the profile, which names the program.
    For Claude Code, the approval is likewise what catches a path no word spells, in any
    encoding: inline code or inline config (an inline-JSON `--mcp-config` or `--settings`) that
    builds the path of a script in the project, and a program outside those places that itself
    reads and runs a file a chat can write, one it is not handed as a word (by its own logic, its
    environment or a config it reads).
- **`charter.local.toml`** joins the later-code names: purlis reads it outside any sandbox at
  every start.

*Amended 2026-10-04 (#1057):* class 5 names each harness's project config by file, and the
list is `PLANTED` in `crates/purlis-core/src/sandbox.rs`. Three joined it, each checked
against the harness's own source or docs:

- **`.claude/skills/`.** A skill runs its `` !`…` `` commands when invoked and registers the
  hooks in its frontmatter, as a command or an agent can.
- **opencode's `tui.json` and `tui.jsonc`.** Their `plugin` key loads code into the next
  opencode started there. The `.opencode/` copy was already held.
- **The whole `.agents/`.** What Codex reads from a project's skills there can start code
  outside any sandbox. All of `.agents/` is held, failing closed, so its plugins are held too.

Checked and left out of the class:

- **Claude Code's `.claude/workflows/`.** A workflow has no file or shell access of its own,
  and the agents it runs get the session's own sandbox.
- **`.claude/output-styles/`, `.claude/rules/` and `.worktreeinclude`.** They are not code.

*Amended 2026-10-04 (ruling V90, #1179):* **class 1 holds a keyring vault at the store.** No
harness's sandbox keeps a chat off the operating system's credential store, which is a service,
not a path, so a sandboxed project with a keyring vault started no chat at all.

- **macOS (V90a).** Every keyring item purlis writes is held to purlis's app: only the app's
  own binary reads it without the person's confirmation, and any other program, the `purlis`
  command and every program a chat runs included, is refused or makes the system ask the person
  (ADR 0047 as amended). The class is then held for every harness, so a Claude Code chat
  starts, whose own sandbox lets a command reach the service (measured live: from inside it, an
  item was refused to every program but the one it is held to).
- **purlis's own wrap (V90b)** denies the service as well, so Codex and opencode cannot reach it
  at all (measured). Neither needs it for its own login: both keep it in a file.
- **Linux (V90c).** The Secret Service answers any process of the session, and no harness's
  sandbox has been measured keeping a chat off the session bus, so a Claude Code chat there is
  still refused. The refusal is never a dead end: it says to start that chat without the sandbox
  from the new-chat picker (§7), or to move those secrets to a plain-file or 1Password vault,
  which the sandbox can keep from a chat. A resumed or relaunched chat has no opt-out, so moving
  them is its way on. Codex and opencode wait for the Linux wrap (#1040).
- **Existing items (V90d)** are written again, held, the next time purlis reads them; one the
  `purlis` command made is moved by the command, since only an item's maker can delete it.
  Until then an item keeps the access it had.

*Amended 2026-10-06 (the operator's grilling of 2026-10-06, Q4; spec #1330; amends class 5 and
ruling V73b):* **the manifest denial is scoped to where a manifest can change a chat's
sandbox.** `charter.toml`, `purlis.toml` and their `.local.toml` variants are denied at the
project root and in each folder above the chat, its own folder included: those are the files
purlis reads when it starts a chat there. Everywhere else, in a clone's test fixtures for
example, they are ordinary files a chat may write and a checkout may carry. V87d's rule still
holds on every path a chat starts by: a manifest there that cannot be read starts no chat. The
rest of class 5, and every other class, is unchanged.

### 6. External enforcement backends are an option, and never the default (SD-33)

A project or an org can choose a backend that wraps the whole harness: Docker Sandboxes, a
devcontainer, or an OS profile. It is one more layer on the same policy, compiled the same way.
**Where the harness's own sandbox and the backend both apply, the stricter answer wins.** The tab
badge names the backend. A backend never replaces section 5's classes. If a backend cannot
express a class, the chat is wrapped or fails closed, as in section 2.

### 7. The opt-out is per chat, visible and audited

- **Only a human can turn the sandbox off, and only for one chat:** from the window, or from
  `purlis` on a human client scope. A chat cannot ask for it on its own behalf and succeed,
  because the opt-out goes through the same scopes V16a keeps out of a chat's reach. An opt-out
  is never inherited by a new chat, a resumed chat, a workspace or a plane.
- **The tab shows it for the chat's whole life** (SD-3): every tab carries a sandbox badge, and
  an unsandboxed chat's badge cannot be hidden or dismissed.
- **Org policy can forbid the opt-out** (C9, strictest wins). The window then offers no switch,
  and the fail-closed refusal says who locked it.
- **Every change is an audit event.** `trust.sandbox.off` records who turned it off, when, which
  chat, harness, persona and machine, the reason if one was typed, and which classes were
  lifted. `trust.sandbox.on` records it going back on. An unlisted-egress exception (section 3)
  is audited the same way. Until the audit chain (AU-1..AU-3) exists, these events go into the
  chat's session record and purlis's machine-state log (ADR 0034). AU-1 takes them over, and
  none is dropped when it does.
- The dogfood opt-out rate is SD-2's outcome bar: under 10% of chats, together with the false
  denials measured on SD-45's everyday corpus (V12).

## ADR 0028, re-opened

The condition ADR 0028 rested on is false for a sandboxed chat. That chat is a principal less
privileged than purlis. It can write inside its worktree, and purlis reads and writes paths
there by name. So the check-then-open race that ADR 0028 measured (a quarter of reads through
`contain::readable` with a racer present) becomes a way for a confined chat to get purlis to do
what the chat itself cannot. ADR 0028 kept the fix and its order, and both still hold: the
`openat`-beneath-a-descriptor rewrite of the whole core, done in one piece, with the external
review decision 16 requires, and never one call site at a time. What changes is its standing.
**It is no longer optional, and until it lands, purlis does not call the sandbox a boundary
against purlis itself.** `SECURITY.md` and the sandbox view state that residual, in those terms,
from the release SD-2 ships in. Open question 2 asks whether SD-2 waits for the rewrite.

ADR 0041's extensions are not covered by this decision. An extension is still a subprocess with
no OS sandbox, by the ruling of 2026-09-22. Sandboxing extensions would be a separate decision.

## What this rules out

- A plane, persona, preset or harness setting that turns the sandbox off or removes a denial
  class.
- Running a chat unsandboxed because the sandbox was unavailable, without a person choosing that
  for that chat.
- Writing any harness's managed tier, or claiming enforcement over agents purlis did not start.
- A per-harness policy dialect. Each harness has one schema and one compiler.
- Moving "chats never read a vault" back into the hook. The hook explains a refusal, and the
  sandbox is what enforces it.
- Describing the denial classes in more detail than this record gives, in public docs.

## Ruled (V21, 2026-09-30)

1. **Existing planes** get a one-time offer to turn the sandbox on; it never flips on at the
   upgrade.
2. **SD-2 ships with ADR 0028's residual stated** as a known gap, until the `openat` rewrite and
   its external review.
3. **Windows keeps the default on** (ADR 0031): chats there start at the visible opt-out until a
   backend exists.
4. **purlis may turn Codex's own sandbox off only when its wrap is measured strictly stricter**;
   otherwise Codex keeps its own. *Applied 2026-10-04 (#1123):* on macOS, inside purlis's
   wrap, where Codex's own cannot be applied at all.
5. **A new plane's default egress** is `model-providers`, `forge` and `toolchains`.

## Ruled (V78, 2026-10-03, SD-2 slice 4)

The four questions SD-2's last slice (#1056) left open:

1. **No new CLI word.** One chat opts out from the window's new-chat picker, "Start without the
   sandbox", with the reason the sandbox cannot be applied shown beside it. §7's "or from
   `purlis` on a human client scope" is not built: the picker is the one place an opt-out is
   made. A project's default stays `[sandbox] mode` in `charter.toml`.
2. **Windows starts are audited.** Every start without the sandbox writes `trust.sandbox.off`,
   the forced Windows ones (V21 3) included. For those, the actor is `purlis (no backend on
   this OS)`, a host actor (ADR 0075 §2), never the operator.
3. **SD-30's install action types the distribution's install command into a shell tab at the
   project root, and does not run it.** Installing needs `sudo`. This is unlike FR-29's
   installers (V65), which one press runs.
4. **The opt-out rate is a local count.** `purlis doctor` and Project settings show it, and it
   is never sent anywhere.

The offer to an existing project (§1, V21 1) is a notice in the project view, answered once
either way, never a dialog, so it costs nothing of the first run's interrupt budget.

## Amended (2026-10-06, #1342): a block is never a dead end

The operator's ruling of 2026-10-06 ("go with recommendations", option (a)) amends §1 and §7.
"Only a person turns the sandbox off, for one chat" stays. A person may also widen one chat's
sandbox, or every chat's, by exactly what a block named:

1. **A grant names one host or one folder to write.** The block's Notice on the chat's tab
   offers **Allow for this chat**, **Always allow** (every chat of this project on this
   machine; or, for a host, everyone in the project, as one of the project's own hosts) and
   **Keep blocked**. What it would allow is shown whole before the press. A folder is never the
   project's: it is a path on one machine. A wildcard host is never proposed from a block: a
   person types one in Settings.
2. **A folder comes only from an allowlist** (2026-10-06, D-1342-10, D-1342-11), judged on
   the folder the kernel would write (D-1342-12): it is resolved first, so a link judges as where
   it points, and only that resolved folder is kept and compiled, and judged again at every
   start (one later swapped for a link is dropped). On macOS names compare case-folded. The
   allowlist is the project tree and a folder the person lists in Settings › Sandbox, on this
   machine only, kept as it resolved when listed and dropped, with a word in Settings, once it
   resolves elsewhere; Settings warns when one holds launch agents, `PATH` folders or a
   harness's own. No temp folder is on it (D-1342-14): no per-chat temp folder is known to
   grant, and a harness's sandboxed temp folder is shared by every session of its user. No
   refusal is ever lifted for a project that happens to live under a refused folder. No cache folder is on it by default: a cache is shared by
   every project, and many hold code another program later runs. On top of it, never a folder on
   `PATH`, a harness's own home or temp root, never `/`, the home folder or a folder above the
   project, and never the classes of §5: vault storage, purlis's integrity state, human powers
   and what later code loads, which get the way that works instead (`purlis secret exec`,
   purlis's own commands, or the person's own change).
   Anything else (much of the machine is shared, or loaded later outside any sandbox) gets no Allow:
   the Notice says why and offers **Start without the sandbox for this chat**, the person's own
   §7 opt-out for that chat's next run, so a block never dead-ends. A local socket gets the same.
   A read is never granted: a chat's reads are denied only for the classes of §5. A granted
   folder that holds a denied path keeps the denial: every compiler writes its denials after its
   grants, and for Claude Code every later-code name is denied again under each granted folder
   by absolute path.
3. **A grant reaches a running chat by restarting it on its conversation** (spike #1347:
   Claude Code pins `--settings`, so nothing reloads a grant live in a terminal chat). The app
   restarts it once its turn has ended, and its first message says what was allowed, so it
   retries without anyone typing.
4. **Every grant and every revoke is audited** (`trust.sandbox.grant`, `trust.sandbox.revoke`),
   before it takes effect: no event log, no grant. Settings › Sandbox › Granted lists each one
   with Revoke (#1348). A policy can lock one out (`hosts::Locks`, #1343).
5. **Allow once** is ruled in: one exact command, run outside the sandbox through the harness's
   own per-command mechanism, with purlis's hook as the only approver, the command shown whole,
   never auto-approved in any mode, classes 1–3 refused, audited, and a policy can turn it off.
   It is a person's per-command opt-out, consistent with §7 narrowed to one command. Not built
   yet; until it is, `allowUnsandboxedCommands` stays `false`.

## Amended (2026-10-07, #1343 and #1423): this machine's policy file, and a policy that requires the sandbox

This amends §4, and with it the last lines of §1 ("Policy locks values") and §7 ("Org policy can
forbid the opt-out").

1. **The machine layer of C9 is `/etc/purlis/policy.json`.** §4 names it
   `/etc/charter/policy.json`; no file of that name was ever read or deployed, and the product
   is purlis (ADR 0091), so the old path is not read (D-1343-2). It is the only layer read so
   far (D-1343-1): an MDM profile, a Windows policy key and an organisation's policy on a server
   stay with SD-14, and until one exists no policy is read on Windows.
2. **Its format is JSON, documented in `docs/plane-format.md`**, which is the reference for its
   keys: `owner` (who set it, as the window names them), and under `sandbox` the locks
   `presets`, `hosts`, `personal-hosts`, `persona-hosts`, `opt-out` and `write-grants`. Absent
   and `true` lock nothing. A key this version does not know refuses the whole file
   (D-1343-4), so a lock a newer purlis would keep is never dropped by an older one.
3. **It is trusted only where no one but an administrator could have written it**: a regular
   file, never a link, owned by root, in a folder owned by root, neither writable by anyone
   else, and no chat may write its folder (the human-powers class of §5). Nothing in the
   environment or in a project moves where it is read from. A file or folder that fails that,
   a file that does not parse, and one with an unknown key are **refused closed** (D-1343-3,
   D-1343-11): every lock is set but the presets. Access-control lists are not read
   (D-1423-5): on Linux one shows in the permission bits, and on macOS reading one needs a C
   call the codebase's no-`unsafe` rule does not allow, so an ACL root added there is an
   administrator's misconfiguration that this check does not catch. The folders above the
   policy's own are not checked.
4. **A policy that forbids the opt-out requires the sandbox** (the operator's ruling of
   2026-10-07, D-1423-1; it replaces D-1343-9). `"opt-out": false` means every chat on that
   machine runs sandboxed, in every project:
   - A project with no `[sandbox]`, or one that has not turned it on, runs every chat sandboxed
     there as if it had turned it on with the default presets. What its `[sandbox]` already says
     applies, and the policy's own `presets` and `hosts` hold it as they hold any project, so
     where the policy fixes the presets, those are the most it reaches. Nothing is written to
     the project: the requirement is the machine's, and a teammate without the policy is not
     held to it. The one-time offer to turn the sandbox on is not shown there.
   - Settings shows the mode as "On, required by policy, set by <owner> in <file>", with no
     control.
   - A system with no sandbox backend (Windows today) **refuses** a harness chat. It is never
     started unconfined, and purlis is never the actor of a `trust.sandbox.off` there. The
     refusal names the policy and its owner. Ruling V21 3 (Windows starts at the opt-out, with
     purlis as the actor) stands only where no policy requires the sandbox.
   - No refusal sends the person to the opt-out: each names the policy and its owner instead.
   - §1's "absent is not off" stands for the project's own file. A policy is the one thing that
     turns the sandbox on where the project did not, and only on the machines it is on.
5. **What a lock drops is said where it is kept or added** (#1423): Settings refuses a host
   the policy does not allow when it is added, with the policy's sentence; the Granted list
   marks a kept grant the policy now drops as not in force; and the one-time Notice of a change
   to the project's hosts names only the hosts a chat reaches.

## Amended (2026-10-08, #1055): a commit in a linked worktree is a brokered write

This amends §2's list of brokered writes, which says a write not on it is not brokered until
an amendment adds it. Added by the operator's ruling of 2026-10-07 (V99h): **a commit in the
branch folder a sandboxed chat stands in.** The worktree's git directory is not opened to the
chat.

### What was measured

SD-2 slice 3 asked what `git add` and `git commit` do from a sandboxed chat, in a clone and
in a linked worktree. A linked worktree keeps its index, its HEAD and its objects in its
clone's `.git` (`<clone>/.git/worktrees/<name>` and `<clone>/.git/objects`). *Case A* is a chat
standing in the worktree's folder; *case B* is one standing in a folder that holds both the
clone and the worktree, such as the workspace's.

| | Claude Code | Codex (purlis's wrap) | opencode (purlis's wrap) |
|---|---|---|---|
| Clone: `git add`, `git commit` | work (measured) | work (by reading) | work (by reading) |
| Clone: a write to `.git/config`, `.git/hooks/`, `.git/commondir`, `.git/config.worktree`, `.git/info/attributes`, `.git/objects/info/alternates` | refused (measured) | refused (by reading) | refused (by reading) |
| Worktree, case A: `git add`, `git commit` | refused: the clone's `.git` is outside the chat's folder (by reading) | refused, the same (by reading) | refused, the same (by reading) |
| Worktree, case B: `git add`, `git commit` | refused: `.git/worktrees/` is a later-code name, so the index cannot be locked (the denial measured, the git call by reading) | refused, the same (by reading) | refused, the same (by reading) |
| The worktree's own `.git` file, rewritten by the chat | written (measured; §5's stated gap for this harness, #1065) | refused (by reading) | refused (by reading) |

- *Measured* is one sandboxed Claude Code chat's own commands: Claude Code 2.1.293, git
  2.50.1, macOS, in a repository built by hand, because that sandbox refuses `git init`.
- *By reading* is the compiled policy (`sandbox::seatbelt::profile` and `planted_rules`, and
  `sandbox::claude`). The wraps' live tests write below a clone's `.git` inside each wrap.
- **No linked worktree was measured.** One cannot be made inside a sandboxed chat (its git
  directory is a later-code name), and the measurement was not run in a person's own worktree.
- A clone commits inside the sandbox because §5 denies names, not the `.git` folder: its
  `config` and `hooks` are held, and its index, refs and objects are the chat's to write. The
  same recipe would let a worktree commit (the worktree's git directory, less the files that
  point elsewhere, and the clone's objects and one ref). It was not taken: it hands a chat the
  clone's shared objects and refs, which a chat in a worktree has none of today, it must be
  known when the chat starts, and it rests on a precision §5 says one harness lacks.

### The decision

1. **`purlis worktree commit -m <message> [--all | <path>…]`**, run by a sandboxed chat,
   sends the app an ask over the chat's hook socket, bound to its sender as every ask is. The
   ask names the message and what to stage, and nothing else: no folder, repository, branch,
   git directory, author, date or option. A line carrying any other field is not read.
2. **The app commits, outside the sandbox, in the folder it recorded the chat as standing
   in**, on the branch that folder is on, with the repository's own identity or the person's,
   and the chat's trailers (ADR 0074). The folder must be one purlis cut, and its `.git` file
   must name its own clone's worktree and be named back by it; every git call is given that
   git directory and work tree.
3. **Git runs nothing but git.** No global or system config, no hook, no file-system monitor,
   and no git of its own inside a submodule. Refused, each with a sentence: a repository whose
   own config names a program (as a brokered clone or worktree is), one with a
   `config.worktree` in the worktree's git directory, one with an `objects/info/alternates`,
   and one whose config signs its commits. **So the repository's own `pre-commit` does not
   run**, where it does for a chat's own commit in a clone; the answer says so for each hook
   that is there. purlis's scan of what the commit would publish runs in the app.
4. **Staging trusts nothing in the folder.** Paths are names inside it, reach git as data and
   are never read through a link; git stores a link as a link. Every call is held to the
   checked git directory wherever in the folder the chat stands. A commit that would record
   another repository, change `.gitmodules`, add a name that differs from another only by
   case, or hold a file its attributes mark for a content filter is unstaged and refused. The
   last is because the person's filters (an LFS one, most often) are in the config this commit
   does not read: the file would be stored as it is on disk. A folder with staged changes the
   app did not stage is left alone.
5. **It only commits.** No amend, reset, rebase, merge or push, and no other branch or folder.
6. **No approval.** The commit is the chat's own work in its own folder on its own branch, as
   a commit in a clone is. Any process inside the chat that holds its token can ask, a helper
   the chat started included. A chat that is not sandboxed, and a chat in a clone, is told to
   use git.
7. **One at a time, and never left half done.** One commit runs in a branch folder at a time.
   The app notes what it staged in the worktree's git directory, so a commit an app stopped
   in the middle of is unstaged by the next ask and not mistaken for a person's staging.
8. **The app's own git in a folder reads the `.git` file itself** before it runs
   (`worktree::link`), and gives git the directory it read. A folder purlis cut is read only
   as a worktree of the clone it was cut from, named back by it. A link that names a git
   directory inside the folder itself, or in a temp folder the folder is not in, is never
   followed. Elsewhere below a workspace a link must name a worktree of some clone, or a
   submodule of the repository that encloses the folder. And git that purlis runs starts no
   git inside a submodule, on any of its runners, so a submodule is read by its recorded
   commit: **uncommitted work inside a submodule no longer shows in purlis.**

   This narrows what a rewritten link reaches; it does not replace §5. It checks where a link
   points, not who made what is there, so where a harness cannot keep a prepared folder from
   being moved into a `.git` (#1065), a folder that is not one purlis cut is still that gap.
   A project's own top, and a checkout outside any project, keep the layout their owner gave
   them, and the runners that answer a chat's own git questions are not held to the check.

**The ticket's acceptance is not met yet**: its worktree case is decided and recorded here,
and measured only as the table says. #1055 stays open until a linked worktree is measured on
each harness in a running app.

Not covered: a chat in case B commits from a chat started in the branch folder, since the app
uses the folder it recorded and takes none from the ask.
