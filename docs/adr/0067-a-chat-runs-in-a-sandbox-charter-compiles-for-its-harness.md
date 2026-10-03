# A chat runs in a sandbox charter compiles for its harness, and it shows when one does not

**Accepted 2026-09-30** by the operator (ruling V21; program map SD-1; rulings Q10, C9, W8, V16
and X18). It re-opens [ADR 0028](0028-containment-checks-a-path-and-does-not-hold-it.md), whose
own re-opening clause this decision triggers.

## Where charter is today

Chat isolation is whatever each harness does by default, and charter does not add to it or make
it the same across harnesses (gap G1 in the security review). A Claude Code chat runs as the
operator, with charter's hooks and nothing else. A Codex chat runs under Codex's own default
sandbox, and charter passes it no sandbox setting on purpose: the test
`a_codex_chat_is_handed_no_approval_or_sandbox_setting` stops charter from loosening it. An
opencode chat is not confined at all. Egress is open wherever the harness leaves it open, and a
secret goes to whatever process the model names (gap G2).

The one thing that stands between a chat and a vault is the `PreToolUse` guard. `hooks.md` says
what that guard is: *"a guard against mistakes, not an attacker with shell access as your user"*.
It recognises spellings, and a model that is working against it can get round it (gap G13).

ADR 0028 accepted a check-then-open race in `contain` because *"nothing in charter makes an agent
less privileged than charter itself"*. Its 2026-09-22 amendment names the trigger that re-opens
it: *"The day a sandbox lands … for any subprocess charter starts."* This ADR is that day.

## The decision

**Every chat charter starts in a new plane runs sandboxed. The policy is one harness-agnostic
schema, and charter compiles it into each harness's own mechanism. If the sandbox cannot be
applied, the chat does not start. The operator can turn it off for one chat, and that chat's tab
shows it for as long as the chat lives. Every opt-out is audited.**

### 1. On by default for new planes, and it fails closed

- A plane that charter creates after this ships records `sandbox = "on"` in its committed
  settings. Turning the sandbox on is a restriction, so a plane may carry it (ADR 0035: *"a plane
  can restrict, never grant"*). **A plane can never carry `off`.** A committed file cannot
  loosen what a chat is confined to, just as it cannot pre-approve a permission.
- An existing plane keeps running as it does today. The first time charter opens it after the
  upgrade, a notice offers to turn the sandbox on, and nothing flips on its own (see open
  question 1).
- **`failIfUnavailable` is the only mode.** If this machine cannot apply the compiled policy (no
  bubblewrap, user namespaces disabled, a backend missing), the chat does not start. The refusal
  names what is missing, offers SD-30's prerequisite install, and puts the per-chat opt-out
  inside the refusal itself, so a first run still gets to a working chat within FR-1's five
  minutes (X18).

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

| Harness | What charter compiles the policy into |
|---|---|
| Claude Code | The `--settings` blob charter already passes gets a `sandbox` object: enabled, `allowUnsandboxedCommands: false`, `failIfUnavailable: true`, filesystem read and write rules, and `network.allowedDomains` from the egress presets. `--settings` sits above project settings, and project settings cannot turn filesystem isolation off. |
| Codex | **Held back until #1123** (ruling V87f): a sandboxed project refuses Codex until charter runs it inside its own compiled sandbox, with what follows still on inside it. Codex's own workspace-write, set explicitly rather than inherited: a permissions profile that extends `:workspace`, selected with `default_permissions`, which holds the denied paths, and Codex's own network proxy (`--enable network_proxy`) holding egress to the presets. *Amended 2026-09-30 (SD-2 slice 2):* this row first said `-s workspace-write`. On codex-cli 0.147.0 any `-s` switches Codex to its legacy sandbox mode, which reads no permissions profile, so the denied paths and the proxy's allowlist would be dropped. The profile is the only form that carries them, and a `-s` in a chat's own words refuses the chat. The test that forbids Codex sandbox arguments is rewritten so it forbids only *looser* values: charter may tighten Codex's sandbox and still may never loosen it. |
| opencode | opencode has no sandbox of its own, so charter generates a Seatbelt profile (macOS) or a bubblewrap invocation (Linux) and starts opencode inside it. The profile is generated by charter's Rust core: charter does not ship `sandbox-runtime` (no language mixing in the shipped app), which stays available as a test oracle. |

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
writes. Charter makes opencode's own directories before the wrap, and a chat may neither make
nor move one, so none is moved out, changed and moved back. Every denial class in section 5 still wins over these grants. The Linux wrap is
#1040.

*Amended 2026-10-03 (ruling V73c):* opencode's hooks run inside the wrap, so every line they
send is the chat's own claim about itself (the class of #873), never proof that a hook ran.
Its hook spool stays denied, so a line the app does not take cannot be kept: it is shown in
the chat's window instead, never dropped in silence. Moving opencode's hooks outside the wrap
is #1069.

**The compile is total, or the chat is wrapped.** Every denial class in section 5 must hold for
every harness. When a harness's native sandbox cannot express a class (for example, a read-deny
that its write-only confinement cannot state), charter wraps the harness in its own generated
profile as well. Where both apply, the stricter answer wins. A harness that has neither route on
this machine fails closed, as in section 1.

**A `charter` command that a chat runs is part of that chat.** It inherits the chat's sandbox and
its denials. Anything that needs to reach past them (resolving a secret, writing the audit,
recording a session outcome) is asked of `charterd` over its socket and is never done from
inside the chat. Each compiled profile allows the chat to connect to that socket and nothing more.

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
  generated profile allows network traffic only to charter's local egress proxy. That same proxy
  is the long-term home of gap G2's broker, where a secret is released only to the hosts it is
  bound to.

### 4. Charter never writes a vendor's managed tier. It only adds stricter overlays (W8, SD-32)

- Charter **never writes** a harness's managed or admin tier: the files, profiles and registry
  keys an organisation's MDM owns. Those belong to the customer's administrators.
- Charter **reads** the effective managed policy and shows each value it fixes as "locked by
  <vendor> admin" in the chat's sandbox view.
- Charter's compiled policy is an **overlay**. It may tighten what the managed tier allows and it
  may never loosen it. Where the managed tier is stricter, it wins, and charter shows that it did.
  Where the managed tier forbids an overlay charter needs for a denial class, the chat is wrapped
  (section 2) or fails closed. It never runs with that class missing.
- C9's layers (MDM profile, Windows policy key, `/etc/charter/policy.json`, server org policy)
  compile into charter's own overlay, with the strictest value winning. SD-14 **exports** org
  policy as each harness's native managed artifact, for the customer to push through their own
  MDM. Charter exports that artifact and does not install it.
- **The claim charter makes is exactly this:** *enforced for agents charter launches; for the
  whole fleet, charter exports each harness's managed settings.* Nothing in the product, the docs
  or the trust page may claim more.

### 5. What a chat's sandbox always denies (V16)

These are classes, not a list of paths. SD-2 turns each class into rules for each harness, and
each class has its own test. No plane, persona or preset can remove one. Only the per-chat
opt-out in section 7 lifts them, and the audit records when it does.

1. **Chats never read a vault directly.** Every vault provider's storage, whether a plane file,
   a keyring item or a provider's local session, is denied to the chat. `charterd` resolves a
   secret and hands it to the command it runs, so the approval gate that V15 sets (SD-37..SD-39)
   is enforced rather than advisory (SD-9). The `PreToolUse` guard stays, because it can explain
   a refusal and the sandbox cannot (gap G13).
2. **Charter's integrity state is denied to chats:** the audit directory and the device key, and
   every chat's hook spool but its own. `charterd` is the only writer of the first two. A chat's
   hooks may append only to that chat's own spool, never to another chat's, and the host verifies
   and seals each spool as it drains it ([ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
   §6, corrected 2026-09-30 by ruling V22). A chat that could change them could change the record
   of what it did.
3. **Human powers are unreachable from a chat.** The credentials behind the terminal,
   fleet-MCP and approval client scopes are unreadable inside the sandbox. `charterd` also
   refuses those scopes to any connection from a chat's process tree, so an agent can never
   answer its own asks or approve its own secret requests (V16a).
4. **On a runner, the chat is also denied `charterd`'s install files and the git internals
   `charterd` operates on** (RR-5, V16d). A runner chat is sandboxed by default, and V19 makes
   this a blocker for the first runner slice.
5. **A chat never writes what a program run later, outside any sandbox, loads** (added
   2026-10-03, rulings V73b and V73d). At any depth of every directory it may write, its own
   directory, its temp directory and any other the harness is granted alike, so a protected
   name is neither made there nor built in one and moved into another with its parent: git's
   config and hooks in every clone, worktree and submodule (`.git/modules/…`), and the `.git`
   itself, so one is never moved into place; the hook managers' `.husky/` and `.githooks/`;
   shell startup files; `.mcp.json`; `.claude`'s settings, commands and agents; editor folders;
   opencode's and Codex's project config; and `charter.toml`, so a chat at the plane root
   cannot take its `[sandbox]` out. Resolved when the chat starts and denied as paths: the
   directory every `core.hooksPath` git would use names, and every script a protected config
   names for a harness to run (a hook command, an MCP server's command and arguments, a
   plugin file). A directory a later git takes for a bare repository holds no protected name,
   so it is a stated residual with its own checklist (#1100). Each harness's compiler adds
   this to what the harness's own sandbox already denies, after what it lets the chat write,
   and never replaces it. Where a harness's own sandbox can state a name only with what is
   below it, the `.git` itself is not held for that harness, and that gap is #1065.

*Amended 2026-10-03 (rulings V87d and V87f):*

- **Linked folders, every harness.** No sandboxed chat starts in a folder reached through a
  link. Every folder from the plane's own, as the kernel names it, down to the chat's must be a
  real directory, and the chat's folder must be inside the plane.
- **`charter.toml` must be readable.** A `charter.toml` that cannot be read starts no chat: it
  may say `[sandbox]`, so it never reads as "not set". That covers one that is not a regular
  file (a link, dangling or not, a FIFO, a device, a socket or a directory), one larger than
  charter reads, and such an entry above a chat on every path a chat starts by.
- **Folders between a chat's own and a denied path.** Each is held as an entry, so it is never
  moved away with the denied path inside it and replaced. The chat's own folder and its temp
  folder are held too, in charter's own compiled sandbox.
- **A sandboxed project refuses Codex until #1123.** Codex's own sandbox resolves its paths
  again at every command, so a running chat, or another chat that can write above its folder,
  could move what the compiled profile names, and no class holds (measured). Until charter runs
  Codex inside its own compiled sandbox, with Codex's own still on inside it (#1123), a
  sandboxed project starts Codex only on a person's audited opt-out. The Codex compiler is kept
  for #1123, behind the one refusal (`HarnessAdapter::sandbox_held_back`).
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
    its program a chat-writable file by mistake. The boundary for every sandboxed harness,
    Claude Code included, is to be charter's own wrap around the whole harness (#1123, not yet
    built for Claude Code). Encodings the check does not read are #1123's to close, not the
    check's.

  The probe and the terminal both run that real path, so the file checked is the file run. The probe runs with the chat's own environment and
  folder, and a probe that does not finish in time is killed with its whole process group. Each
  refusal is one sentence, and a person may still start the chat without the sandbox.
  - **What the check does not prove.** A program can print any answer. The `--version` check
    catches a profile that is not Claude Code by mistake, not one written to pass it. Against
    that, the guarantee rests on the person's approval of the profile, which names the program.
    Until #1123's wrap, the approval is likewise what catches a path no word spells, in any
    encoding: inline code or inline config (an inline-JSON `--mcp-config` or `--settings`) that
    builds the path of a script in the project, and a program outside those places that itself
    reads and runs a file a chat can write, one it is not handed as a word (by its own logic, its
    environment or a config it reads).
- **`charter.local.toml`** joins the later-code names: charter reads it outside any sandbox at
  every start.

### 6. External enforcement backends are an option, and never the default (SD-33)

A project or an org can choose a backend that wraps the whole harness: Docker Sandboxes, a
devcontainer, or an OS profile. It is one more layer on the same policy, compiled the same way.
**Where the harness's own sandbox and the backend both apply, the stricter answer wins.** The tab
badge names the backend. A backend never replaces section 5's classes. If a backend cannot
express a class, the chat is wrapped or fails closed, as in section 2.

### 7. The opt-out is per chat, visible and audited

- **Only a human can turn the sandbox off, and only for one chat:** from the window, or from
  `charter` on a human client scope. A chat cannot ask for it on its own behalf and succeed,
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
  chat's session record and charter's machine-state log (ADR 0034). AU-1 takes them over, and
  none is dropped when it does.
- The dogfood opt-out rate is SD-2's outcome bar: under 10% of chats, together with the false
  denials measured on SD-45's everyday corpus (V12).

## ADR 0028, re-opened

The condition ADR 0028 rested on is false for a sandboxed chat. That chat is a principal less
privileged than charter. It can write inside its worktree, and charter reads and writes paths
there by name. So the check-then-open race that ADR 0028 measured (a quarter of reads through
`contain::readable` with a racer present) becomes a way for a confined chat to get charter to do
what the chat itself cannot. ADR 0028 kept the fix and its order, and both still hold: the
`openat`-beneath-a-descriptor rewrite of the whole core, done in one piece, with the external
review decision 16 requires, and never one call site at a time. What changes is its standing.
**It is no longer optional, and until it lands, charter does not call the sandbox a boundary
against charter itself.** `SECURITY.md` and the sandbox view state that residual, in those terms,
from the release SD-2 ships in. Open question 2 asks whether SD-2 waits for the rewrite.

ADR 0041's extensions are not covered by this decision. An extension is still a subprocess with
no OS sandbox, by the ruling of 2026-09-22. Sandboxing extensions would be a separate decision.

## What this rules out

- A plane, persona, preset or harness setting that turns the sandbox off or removes a denial
  class.
- Running a chat unsandboxed because the sandbox was unavailable, without a person choosing that
  for that chat.
- Writing any harness's managed tier, or claiming enforcement over agents charter did not start.
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
4. **charter may turn Codex's own sandbox off only when its wrap is measured strictly stricter**;
   otherwise Codex keeps its own.
5. **A new plane's default egress** is `model-providers`, `forge` and `toolchains`.

## Ruled (V78, 2026-10-03, SD-2 slice 4)

The four questions SD-2's last slice (#1056) left open:

1. **No new CLI word.** One chat opts out from the window's new-chat picker, "Start without the
   sandbox", with the reason the sandbox cannot be applied shown beside it. §7's "or from
   `charter` on a human client scope" is not built: the picker is the one place an opt-out is
   made. A project's default stays `[sandbox] mode` in `charter.toml`.
2. **Windows starts are audited.** Every start without the sandbox writes `trust.sandbox.off`,
   the forced Windows ones (V21 3) included. For those, the actor is `charter (no backend on
   this OS)`, a host actor (ADR 0075 §2), never the operator.
3. **SD-30's install action types the distribution's install command into a shell tab at the
   project root, and does not run it.** Installing needs `sudo`. This is unlike FR-29's
   installers (V65), which one press runs.
4. **The opt-out rate is a local count.** `charter doctor` and Project settings show it, and it
   is never sent anywhere.

The offer to an existing project (§1, V21 1) is a notice in the project view, answered once
either way, never a dialog, so it costs nothing of the first run's interrupt budget.
