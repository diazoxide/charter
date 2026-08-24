# Personas

A **persona** is a role identity an agent adopts — `devops`, `qa`, `keycloak-master`,
whatever your work needs. It's the piece that turns "an LLM with tools" into "a team of
specialists with their own knowledge and credentials," and it's `charter`'s
differentiator: workspaces isolate *which repos*, personas isolate *who's working* and
*what they know and can reach*.

A persona lives in a **committed** directory, `personas/<name>/`:

```
personas/devops/
├── persona.md     # frontmatter (role, vault, tools, …) + the charter itself (prose)
├── memory/        # persistent, committed knowledge — MEMORY.md index + one file per fact
├── refs/          # curated docs/links/snippets for the role, also committed
└── bin/           # optional: executables this persona carries (see below)
```

### `bin/` — executables the persona carries

A persona that needs its own scripts puts them in `personas/<name>/bin/` and makes them
executable. They are committed like everything else, and they are **inherited down the
`extends:` chain** (child wins on a name collision), exactly as `mcp.json` servers are.

```bash
charter persona show devops      # lists them under `scripts:`
```

They are **not** put on `PATH`, and cannot be: a `PreToolUse` hook decides *whether* a Bash
call runs, not what environment it runs in, and wrapping every Bash call to inject one would
be charter taking over a mechanism the host owns ([ADR 0014](adr/0014-policy-that-fits-a-pattern-belongs-to-the-host.md)).
Call them by path. Charter names each script's path in the persona's generated sub-agent, so
a dispatched agent knows what it is carrying without being told twice.

Declare the ones the persona should run without a prompt, the same as any other tool:

```
tools: site-health.sh, gh
```

**Provenance is checked for these, unlike system binaries.** The tool guard matches a command
by basename, which is right for `gh` — the plane does not own it. For a script the persona
*does* own, the same rule would auto-approve any file of that name, including one an agent
had just written elsewhere. So a declared name that matches a script in the persona's `bin/`
is approved only when the command reaches **that file**; a bare name (which resolves through
`PATH`) and any other copy both fall back to a prompt.

`bin/` travels with the persona, so on a LIVE persona it reaches teammates' machines and runs
with their credentials. That is disclosed wherever the persona is inspected rather than
gated: anyone who can commit `bin/` can commit an `mcp.json` pointing at the same file.

Definitions, memory, and refs are all **committed and shared** with the team — every
engineer (and every agent) sees the same devops persona and everything it has learned.
Its **secrets are not**: a persona's `vault:` field only *names* a vault; the credentials
themselves live in a local, gitignored, mode-0600 file (see `docs/secrets.md`) that each
developer sets up on their own machine. Definitions travel with the repo; credentials
never do.

## Creating one

```
charter persona create devops --role "DevOps Engineer" \
  --delegate-when "CI/CD pipelines, k8s deploys, cluster access" --with-vault
```

writes `personas/devops/persona.md` from a template, scaffolds `memory/` and `refs/`
(with keep-files so git tracks the empty dirs), and registers a local plain-file vault
named `devops`.

**`--delegate-when` is required** (unless `--extends`, which inherits it). It is the
line that decides whether the steward ever routes anything here, it becomes the
persona's dispatchable description, and unlike the charter body it is knowable at
creation. Without it a persona lint-warns from birth and quietly loses its work to
`general-purpose` — the failure `charter persona stats` reports as `⚑ never dispatched`.

**A new persona is a `draft`.** The template stamps `draft: true`, and while it is set
charter generates **no** sub-agent — so the persona can be *adopted* (`persona use`, how
you work on it) but not *dispatched*. That asymmetry is mechanical: dispatch bakes the
whole charter into a sub-agent's system prompt with nobody reading it, while adoption
injects only the identity line. Write what the persona owns, drop the `draft: true`
line, then:

```
charter persona sync-agents        # now .claude/agents/devops.md is generated
```

Until then `charter persona lint` says so, `charter doctor` counts it, and the status
line marks the chip `⚑`.

## The charter format

```markdown
---
name: devops
role: DevOps Engineer
vault: devops
delegate-when: CI/CD pipelines, Kubernetes/GitOps, deploys, service infrastructure
tools: kubectl, glab
extends: platform-base
uses: qa
---

# DevOps Engineer

You are the **devops** persona — DevOps Engineer. …

## Responsibilities
- …
```

The frontmatter is a small set of flat `key: value` lines — no YAML parser, just enough
structure for `charter` to route on:

| Key | Meaning |
| --- | --- |
| `role` | Human-readable role, shown in listings and injected as session context. |
| `vault` | Which vault (`docs/secrets.md`) this persona's secrets live in. Use the reserved value `none` to say it holds no credentials at all — `lint` then stops asking for one, and `charter persona secret` says so rather than hunting for a vault. A vault may therefore not be *named* `none`. Saying nothing still warns: the warning is for the author who never considered it, and only an explicit declaration is believed. |
| `delegate-when` | The trigger phrase for auto-routing — what a request has to look like for another agent (or the default `steward` persona) to hand work to this one. Also becomes the generated sub-agent's description. |
| `tools` | **Programs** auto-approved (no permission prompt) while this persona is active — the `PreToolUse` tool-gate reads this. The unit is the program and every argument rides along, so `tools: gh` is `gh` doing anything, `gh repo delete` included. Seven things are not smoothed however they are declared: destructive subcommands (`kubectl delete`, `charter secret`, `git clean`, …), read as *words* of the command so `git -c alias.z=clean z` is one of them; **an argument that is itself an executable file** — a command running a second program is not one program with its arguments, and that is decided by asking the filesystem about the path, so `caffeinate -s ./evil` and every wrapper charter has never heard of are one answer; interpreters whose *argument* is the command as *text* (`bash -c`, `python3 -c`, `awk`, `npx`, …), which is a **list of names and is best-effort** — see [hooks.md](hooks.md) for what it does not reach, including an argument that is a bare name on `PATH`; any command whose arguments reach a vault or charter's own state — decided by resolving each argument to the file it opens, not by matching its text, so quoting, escaping, a symlink, the bare directory name and a directory that contains it are one answer; anything added to this line **after the session started**, which takes effect in the next session; any command containing a character the shell would rewrite before the program sees it — a pipe, a redirect, a `;`, but equally a glob, a brace, a `~`, a `$` or a `` ` ``, quoted or not; and any command whose **command word is not the file the declared name refers to** — `./gh` is a file the agent can write, so a path in command position is smoothed only when it is, by inode, the persona's own `bin/` script or what a bare invocation resolves to, and a leading `VAR=value` (which picks the file too, via `PATH`) is never smoothed. `ls *`, `git commit -m "fix #12"` and `KUBECONFIG=… kubectl get pods` take a normal prompt for those reasons (see [hooks.md](hooks.md)). |
| `agent-tools` | Narrows the *generated sub-agent's* tool access specifically (omit to inherit everything). |
| `extends` | Inherit another persona's charter + tools; this persona's own charter is appended on top rather than replacing it (see "Inheritance" below). |
| `uses` | Other personas this one may reuse — read their vault, run their tools, or delegate to their sub-agent, without becoming them. The tool half is enforced by the tool-gate; the vault half is declared, not gated (see "Reusing another persona" below). |
| `activity` | `orchestrator` \| `standby` \| `advisory` — declares that memory *volume* isn't a fair usage signal for this persona (see `charter persona stats`), so it isn't flagged dormant for routing/reviewing rather than recording facts. |

Everything below the second `---` is the **charter** itself: free prose describing the
role's responsibilities, conventions, and focus areas. It's injected as session context
when the persona is active, and becomes the body of the generated sub-agent.

### Inheritance (`extends`)

A persona may `extends:` a parent. The child's charter is *appended* to the parent's
(not a replacement), `tools`/`agent-tools`/`uses` union across the chain, and scalar
fields (`role`, `vault`, `model`, …) use the most-derived value that's set. `charter
persona show <name>` prints the effective, merged result; `charter persona lint` catches
a dangling or cyclical `extends:`.

## Memory: a 2×2

Every persona's memory is two independent axes — **own vs. shared**, **persistent vs.
ephemeral**:

|  | Persistent (committed) | Ephemeral (session scratch) |
| --- | --- | --- |
| **Own** | `personas/<name>/memory/` | `.charter/persona-state/ephemeral/<session>/<name>/` |
| **Shared** | `personas/_shared/memory/` | `.charter/persona-state/ephemeral/<session>/_shared/` |

The persona decides the quadrant when it writes:

```
charter persona remember devops "prod kubeconfig lives in the devops vault, key KUBECONFIG"
charter persona remember devops "the migration runbook is at ..." --shared
charter persona remember devops "trying approach X for this task" --ephemeral
```

Persistent memory is **reactive**: recording it commits (and, by the control plane's
`[memory].share` posture, maybe pushes) it immediately — a fact reaches the team the
moment it's written, not at the end of a session. Ephemeral memory is gitignored,
session-scoped scratch, pruned automatically once the session ends. Read it back with
`charter persona recall devops [--query "kubeconfig"]`, or search everything at once
(this persona's own memory, the shared namespace, and the active workspace's journal
together) with `charter recall "<keywords>"`.

## Dispatching a persona as a sub-agent

`charter persona sync-agents` generates a **Claude Code sub-agent** per persona —
`.claude/agents/<name>.md` — from the same charter, so a persona isn't just something the
main session role-plays; it's something you can hand a task to *in an isolated context*:

```
charter persona sync-agents
✓ Synced 1 persona sub-agent(s) → .claude/agents/ (devops)
    invoke 'devops' via the Agent/Task tool (subagent_type: devops)
```

From inside Claude Code, the Agent tool then dispatches to it directly:

```
Agent(subagent_type: "devops", prompt: "Check whether the prod deployment rolled out cleanly.")
```

The generated file carries the resolved (inheritance-applied) charter, a reminder of the
one-credential rule, instructions to read/write *this persona's own* vault (never
`--reveal`), and a memory section — so the sub-agent, running with no memory of the
parent conversation, still knows how to act as devops and where its knowledge and
credentials live. Regenerate it any time the persona's `persona.md` changes
(`sync-agents` is safe to re-run — it refuses to touch a hand-written, non-generated
agent file it finds at the same path).

Every sub-agent dispatch is tallied (agent name + date only, never the prompt) into a
committed store, so `charter persona stats` can show *actual* routing health — a persona
that lints clean but has never once been dispatched is flagged `⚑ never dispatched`,
the blind spot memory volume alone can't see.

`charter persona dispatch-backfill` seeds that tally from past sessions' transcripts, so a
roster that predates the tally has a baseline immediately instead of reading as *"nothing
was ever dispatched"*. It skips anything already covered by live records, so re-running it
never double-counts.

A workflow run reaches the same personas: an `agent()` call resolves `agentType:
"<persona>"` from the registry the Agent tool uses, so a workflow step can run **as a
persona** rather than as a generic worker.

## Reusing another persona (`uses:`)

`extends:` inherits a charter. `uses:` is the other relationship — composition rather than
inheritance — and it is what makes one persona able to reach another's capability without
becoming it. A persona that `uses: devops` may:

- read that persona's vault (`charter persona secret --persona devops …`),
- run its declared `tools:` without a prompt while adopted — the tool gate unions them in,
- and delegate a sub-task to its sub-agent.

**Reuse is one level deep**, and it never touches the reused persona's own active state.
That bound is the point: a role that needs a cluster credential for one step should borrow
it explicitly and visibly, not acquire a transitive reach nobody declared.

**Vault reach is declared, not gated — the two halves are enforced differently, and this
is the disclosure rather than a promise.** The tool half is code: `toolgate` unions in
exactly the personas named here (or in `borrows:`), and a binary outside that set gets a
prompt. The vault half is not: any session can name any registered vault
(`charter secret list <vault>`, `charter persona secret list --persona <name>`), whether or
not it appears above. Nothing refuses it, and no warning is printed. Every persona runs as
the same user against the same `.charter/vaults/` files, so this is a boundary charter
*states* rather than one it holds — the vault-read guard stops an agent printing a vault
file into the transcript, which is a different question from which persona may name one.
`uses:`/`borrows:` is therefore a declaration of intent on the vault half and an enforced
grant on the tool half. Same posture as `bin/` two sections up: what containment cannot
deliver is written down instead of implied.

`charter persona remove` refuses to orphan — if another persona `extends:` or `uses:` the
one being removed, removal is refused and the dependents are named (`--force` overrides).
Before that, a dangling reference was only discovered later, by `lint`.

## Borrowing vs routing (`uses:` and `borrows:`)

`uses:` historically granted three things in one word: read that persona's vault, run its
tools without a permission prompt, and delegate to its sub-agent. The middle grant quietly
decides how much delegation happens — a persona declaring `uses: forge, release` can do both
their jobs with both their tools and never pay a prompt, while handing the work over costs a
dispatch and the context that goes with it.

`borrows:` separates them, **per persona**:

```yaml
uses: forge, release      # personas I may hand work to
borrows: release          # …and whose tools the gate auto-approves for me
```

The comment says *tools* on purpose. `borrows:` decides one thing in code — which
personas' `tools:` the gate unions in — and the vault half of the same sentence is a
declaration nothing enforces; see the disclosure above.

| Declared | What `uses:` means | Tools auto-approved from |
| --- | --- | --- |
| no `borrows:` | vault + tools + delegation (unchanged) | the `uses:` list |
| `borrows: <names>` | delegation only | the `borrows:` list |
| `borrows: none` | delegation only | nobody — own tools only |

Nothing changes for a persona that does not declare `borrows:`, and one persona opting in
never alters another's permissions — which is exactly why this is a frontmatter field and
not a setting in `charter.toml`.

The generated sub-agent charter says which is which, because that text is what a dispatched
agent believes about itself: if it still read "run their tools" while the gate refused, the
agent would have no way to find out why.

## Curating memory: `charter persona optimize`

Memory grows, and growth is not a defect — but a base that has accumulated near-duplicates
and years-old scratch answers a `recall` worse than a smaller one would. `optimize` curates
it in two tiers, split by whether a mistake is reversible:

```
charter persona optimize                 # read-only report, every persona + _shared
charter persona optimize devops          # just one
charter persona optimize --apply         # auto-apply only the safe, reversible half
charter persona optimize --stale-days 60 # age past which a memory is *proposed* for archival (default 90)
```

With `--apply` it performs only what can be undone: collapsing exact-duplicate memories
into `memory/archive/` and repairing the `MEMORY.md` index. Everything requiring judgement
— near-duplicate merges, age-based archival, promoting a recurring fact into the charter —
is **proposed and never applied**. A charter is a human artifact; a tool that silently
edited one would make every charter suspect.

The analysis is deterministic and stdlib-only. The judgement belongs to whoever reads the
proposals.

## The front door: which persona a session starts as

A plane declares its default persona in `charter.toml`, beside `[workspace] default`:

```toml
[persona]
default = "steward"
```

Set it with `charter persona default <name>` (or edit the file — it is yours). This is the
persona a session adopts when nobody has chosen one, and it is the only thing charter knows
about front doors: **no persona name appears anywhere in charter's own code.** The name is
your plane's, and the persona is an ordinary file you can rename, rewrite or delete.

`charter init` creates no personas at all, so a fresh plane has no front door until you
declare one. That is deliberate — charter never invents an identity for you.

### Precedence

Six rungs, highest first. The first one that names an existing persona wins:

| Rung | Where | Scope |
| --- | --- | --- |
| `--persona <name>` | the flag | one command |
| `$CHARTER_PERSONA` | the environment | one shell / one launched session |
| session pointer | `.charter/sessions/<id>.persona` | this session |
| terminal pointer | `.charter/terminals/<id>.persona` | this pane, across restarts |
| declared default | `charter.toml` `[persona] default` | the plane, committed |
| legacy default | `personas/.default` | the plane, committed — see below |

`charter persona use <name>` writes the session *and* terminal pointers, so two panes hold
two personas and neither moves the other. Only the terminal pointer survives closing and
reopening Claude, and only when your terminal reports a pane id — `use` says which of the
two it got, because the difference is one you act on.

A shell with neither a session id nor a pane id (a bare script, say) has nothing to key a
pointer on. There `use` writes `.charter/active-persona`, the plane-wide local file, which
is what that file is now for.

### `personas/.default` (legacy)

`personas/.default` is the older committed declaration. It still resolves, one rung below
`charter.toml`, so a plane that adopted it keeps working. Prefer the TOML key: it lives in
the file you already read to understand your plane, while a dotfile inside `personas/` is
invisible to `ls` and was, in practice, adopted by nobody. When both exist, `charter persona
default` tells you the dotfile is now ignored.

### When the declaration goes stale

Rename or delete the persona a plane declares and the declaration resolves to *nothing* —
no identity, rather than a broken one. Two surfaces say so rather than leaving you to notice
the absence: `charter doctor` reports it (`front door`, a warning, with the fix), and the
status line carries one row naming the missing persona.

## Routing: handing work to the persona that owns it

`delegate-when` is **inbound** — the advert other agents read when choosing where to send
work. `routing:` is the other direction: how insistently *this* persona, while acting,
hands work away.

```yaml
routing: advise      # off | advise | require   (absent means off)
routes-to: forge, release
```

| Level | What happens |
| --- | --- |
| `off` | nothing. The default when the key is absent. |
| `advise` | on a work-shaped prompt, the commitment gate leads with the roster. |
| `require` | the same, plus an **ask** on the first edit of a turn where the roster fired and nothing was dispatched. |

There is **no plane-level routing setting**. The level is only ever read from the one
persona acting in a session, so a plane-wide floor would apply to personas that never asked
for it. A new plane gets a sensible posture because `charter init --front-door` generates a
front door carrying `routing: advise` in its own frontmatter — config in a file you own,
not a constant inside charter.

### What the block says, and what it refuses to say

It lists every other persona, its `delegate-when`, and when it was last dispatched — then
states that nothing has been dispatched this turn and asks you to route or to say why the
work stays put.

It never names the owner. A keyword overlap between a prompt and a prose advert is not
evidence of ownership, and the first confident wrong answer would cost the block the reader
it needs. That decision, and its consequences, are recorded in `docs/adr/0016`.

At `require`, the same restraint shapes the permission prompt: it states that the roster was
shown and nothing was dispatched, lists who was on it, and says charter is not claiming the
work is theirs. It **asks and never denies** — a hard block would make a genuinely
cross-cutting change unworkable, and the fix people reach for then is `routing: off`,
permanently. It asks once per turn, not once per edit.

Sub-agents never see it: the mark is cleared the moment a dispatch begins, so a persona that
was handed work is never told to hand it on. That is a property of the sequence rather than
a guess about the harness.

`routes-to:` puts named personas first. It **prioritises and never restricts** — a
restriction would silently hide every persona created after the line was written.

### When it stays quiet

- the acting persona declares `off`, or declares nothing
- there is no acting persona at all (`charter doctor` says so once, under `front door`)
- the roster minus the acting persona is empty
- the prompt is a question, or the gate's cooldown is still running

### Whether it works

`charter persona stats` prints how often advice fired against how many dispatches
followed. Advice that fires and is never followed is the block failing — read it that way
before adding more personas.

## Everyday commands

```
charter persona list                       # who exists, who's active, vault status
charter persona show devops                # effective (inheritance-merged) charter
charter persona use devops                 # active persona for this session + this pane
charter persona default devops             # the plane's front door (charter.toml)
charter persona secret set API_TOKEN --stdin   # store a credential (never on argv)
charter persona secret exec --env TOKEN=API_TOKEN -- some-cli   # use it without ever seeing it
charter persona lint                       # dangling uses:/extends:, missing role/vault, drafts, stale agents
charter persona stats                      # roster health: usage, verification rate, dispatch count
charter persona optimize --apply           # curate memory: safe ops applied, the rest proposed
charter persona dispatch-backfill          # seed the dispatch tally from past sessions
```

## Health: where it surfaces

Persona health shows up in three places, deliberately at three depths — the same
checks, shown in proportion to how much room each surface has to explain.

| Surface | Shows | Cost |
| --- | --- | --- |
| **Status line** chips | only what's *wrong*: `⚑` draft (undispatchable), `✗` broken config (dangling `extends:`/`uses:`, cycle) | ~2.7ms, renders every turn |
| **`charter doctor`** | one line — how many personas have errors, drafts, warnings | ~6ms, run by hand |
| **`charter persona lint`** | every finding per persona, with how to fix it | ~5ms for 13 personas |

Two rules keep this honest:

- **`doctor` WARNs, never FAILs.** Its blockers list means *"you cannot work"*, and an
  untidy persona doesn't stop you cloning a repo or reaching the forge. Keeping the
  roster out of the exit code is what preserves that meaning.
- **The chips stay silent when healthy.** No `✓` per persona — a row of them becomes
  furniture within a day, and then a real `✗` inside it draws no more attention than a
  zero. Soft findings (no role, no `delegate-when`) stay in `lint`/`doctor`, which have
  room to explain them.

The vault mark beside each chip follows the same rule, so it appears only when the vault
**cannot be used**: `◦` dim — declared but not usable here (no vault of that name is
registered on this machine, or one is and its file does not exist yet) · `!` yellow —
registered and unhealthy. A persona that needs no vault, or whose vault is registered and
healthy, gets no mark at all.

The two unusable cases share one glyph deliberately. Their fixes differ (`charter vault
add` versus `charter secret set`), a chip can carry neither, and `charter persona list`
prints both in words. What the chip is for is the distinction that used to be missing:
a persona *declaring* a vault this machine has never registered is not the same as a
persona that needs none, and both used to render as a dim `·`.

A persona with sub-agents in flight carries `⚡` on its own chip, with the count when
there is more than one and the age of the oldest dispatch always — `▸ devops ⚡2 12m`.
The session strip keeps the bare total (`⚡ 3`), because the persona column caps at
fourteen rows and disappears on a narrow pane.

After thirty minutes a dispatch is **presumed dead** and the age takes a `?` —
`▫ forge ✎3 ⚡ 45m?`. The age keeps climbing; the mark says *presumed dead, not
confirmed*, which is all charter can honestly claim — it cannot tell a killed process
from a sub-agent that is genuinely still working. The record itself survives for a day
before it is discarded, so a stuck dispatch escalates instead of quietly vanishing. It
still counts on the strip's total; the chip is where live and presumed-dead are told
apart.
