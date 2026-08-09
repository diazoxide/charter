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
└── refs/          # curated docs/links/snippets for the role, also committed
```

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
| `tools` | Commands auto-approved (no permission prompt) while this persona is active — the `PreToolUse` tool-gate reads this. |
| `agent-tools` | Narrows the *generated sub-agent's* tool access specifically (omit to inherit everything). |
| `extends` | Inherit another persona's charter + tools; this persona's own charter is appended on top rather than replacing it (see "Inheritance" below). |
| `uses` | Other personas this one may reuse — read their vault, run their tools, or delegate to their sub-agent, without becoming them. |
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

## Everyday commands

```
charter persona list                       # who exists, who's active, vault status
charter persona show devops                # effective (inheritance-merged) charter
charter persona use devops                 # make it the active persona (this session)
charter persona secret set API_TOKEN --stdin   # store a credential (never on argv)
charter persona secret exec --env TOKEN=API_TOKEN -- some-cli   # use it without ever seeing it
charter persona lint                       # dangling uses:/extends:, missing role/vault, drafts, stale agents
charter persona stats                      # roster health: usage, verification rate, dispatch count
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

The vault dot beside each chip has four states, matching what `persona list` says in
words: `✓` healthy · `◦` registered but not created yet · `!` unhealthy · `·` not set
up locally (the *normal* state for most of a committed roster — personas are committed,
vaults are private).
