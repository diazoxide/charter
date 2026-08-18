# charter

[![PyPI](https://img.shields.io/pypi/v/charter-cp?label=charter-cp)](https://pypi.org/project/charter-cp/)
[![Python](https://img.shields.io/pypi/pyversions/charter-cp)](https://pypi.org/project/charter-cp/)
[![Tests](https://github.com/diazoxide/charter/actions/workflows/test.yml/badge.svg)](https://github.com/diazoxide/charter/actions/workflows/test.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**charter** is a control plane for coding agents working across many repos on GitHub or
GitLab: durable **personas**, isolated per-task **workspaces**, and a credential **vault**
the model never reads from. It runs inside **Claude Code, opencode and Codex**, enforcing
the same rules in each.

![The charter status line: the active workspace and its open todos, four cloned repos with their branches, dirty and unpushed markers, CI status and open pull requests, then the personas with their vault and memory state.](docs/assets/statusline.svg)

That is the status line under Claude Code, redrawn from disk every turn — the task you're on,
the repos it owns and what branch each is sitting on, which are dirty or unpushed, CI and
open PRs, and the roles you can hand work to. No git subprocess and no network on the
render path. It is generated, not mocked: `docs/assets/` holds the script that produced it.
On a harness with no status bar, `charter statusline --watch` puts the same render in any
spare terminal.

## 60 seconds

![Terminal recording: charter init scaffolds a control plane, charter discover writes an inventory of every repo in the org, charter clone pulls one into the active workspace, and charter status shows the result.](docs/assets/demo.svg)

```bash
uv tool install charter-cp                        # the CLI  — one per machine, for your terminal
claude plugin marketplace add diazoxide/charter   # Claude Code's plugin — one per project, and it carries the version
claude plugin install charter@charter

mkdir my-control-plane && cd my-control-plane
charter init --forge github --owner my-org
charter doctor
charter discover
charter clone some-repo
```

**Two artifacts, and they are not redundant.** The CLI is a single machine-global install —
it is what you type in your own terminal, and what CI or a cron job runs. The plugin is
installed **per project**, out of a cache Claude Code keeps holding every version at once,
which is why a *plane's* pinned version is the plugin's and not the binary's: two planes on
one laptop can sit on different charters without fighting.
→ [docs/install.md](docs/install.md)

- **`charter init`** scaffolds `charter.toml`, the baseline directories (`personas/`,
  `inventory/`, `workspaces/`), a `.gitignore` tuned for the layout, and the status line
  above. Additive and idempotent — re-running it is always safe. It converts *the directory
  it runs in* into a control plane, so run it somewhere you mean to.
- **`charter doctor`** preflights python, git, git identity, the forge CLI and its auth,
  and names what's missing before anything else trips over it.
- **`charter discover`** queries the forge and writes `inventory/repos.json` — the tracked
  map of every repo in the org, complete even when nothing is cloned yet.
- **`charter clone <repo>`** clones on demand into the active workspace, already carrying
  the one-credential git policy below.

charter ships as **two artifacts** and you want both: a CLI-only install leaves the
plugin's hooks inert, and the plugin ships no Python of its own — every hook it declares
shells out to the CLI. The plugin loads on the **next** session, so restart after
installing it. Full install notes, alternatives to `uv`, and a prompt you can paste into
Claude Code to do all of it: **[docs/install.md](docs/install.md)**.

## You use Claude Code. A teammate uses opencode. CI runs Codex.

Same repos, same rules — or three sets of habits that drift until nobody knows which
guard is actually running. charter runs inside all three and enforces the same invariants
in each: the plane-root guard, the one-credential rule, the secret-leak check, and the
persona's declared tools.

```bash
charter harness list          # every harness, what it can't carry, and which one you're in
charter harness install codex # Codex only — see below
```

```
  claude-code
* opencode
      ↳ status-bar: no status-bar socket: opencode has no `statusLine` config …
          → charter statusline --watch
      ↳ prompt-hook: no per-turn prompt hook: charter's mid-session nudges ride …
  codex
      ↳ status-bar: `tui.status_line` takes a list of built-in segments, not a command …
          → charter statusline --watch
      ↳ session-lock: `shell_environment_policy.set` holds constants, so no per-session …
      ↳ wiring-scope: no project-level config: hooks live only in `~/.codex/config.toml` …
```

The `*` is the harness this session is in, and those names are what `$CHARTER_HARNESS`
holds. A harness charter has no record of is reported too, as a warning rather than a
clean row — an unverified integration and a complete one must not read the same.

`charter init` writes each harness's wiring into the plane, and `charter clone` /
`charter worktree add` arm every tree as it is created, because a session starts in a
clone and not in the plane root. Nothing to install per harness, with one exception.

**What differs is not what charter enforces — it is what each harness lets charter
offer**, and `charter doctor` prints the gap rather than leaving you to find it:

| | wiring | what it cannot carry | what to do about it |
| --- | --- | --- | --- |
| Claude Code | the plugin, plus `.claude/settings.json` | — | — |
| opencode | generated into every work tree by `charter init` | no status bar; no per-turn prompt hook | `charter statusline --watch`; mid-session notes ride tool output already |
| Codex | **opt-in**: `charter harness install codex` | no status bar; no command-pattern permissions; config is machine-wide | `charter statusline --watch`; `guard ask` rules stay in charter's own hook |

`charter doctor` and `charter harness list` print that fourth column against whichever
harness you are in, each ceiling carrying its own answer. Where the last column is empty
it stays empty: charter cannot conjure opencode a per-turn prompt hook, and a workaround
that does not exist costs more to chase than an honest gap.

**`charter statusline --watch`** is the one worth knowing. It repaints the plane state in
place in any spare terminal — no status-bar socket, no multiplexer, the same render on
every harness including the one that has a bar. It shows the plane, not the session, so
the token and context columns are blank and it says so.

Codex is the exception because it has no project-level config: its hooks live in
`~/.codex/config.toml` and arming them affects **every repo on the machine**. `charter
init` will not do that behind your back, and Codex trusts hooks by hash — so the block
charter writes stays inert until you approve it. Why the boundary sits where it does:
**[docs/adr/0015-the-boundary-moves-with-the-harness.md](docs/adr/0015-the-boundary-moves-with-the-harness.md)**.

## Why charter

Each of these is something that went wrong often enough to get built around.

### Dozens of repos, and several tasks in flight at once

A team's work doesn't sit in one repository, and an engineer rarely has one task open. Two
features and a hotfix means three sets of branches across a shifting set of repos, and if
they share a checkout you spend the day stashing. A **workspace** is one directory of
clones per task (`workspaces/<task>/<repo>`), each repo on its own branch, so moving
between tasks is `charter workspace use <name>` and nothing follows you across.

### Two sub-agents that need the same repo

Splitting a task across parallel sub-agents falls apart the moment both want the same
repository on different branches. Cloning it twice wastes the disk and they still collide.
A **worktree** splits one clone into several checkouts (`.worktrees/<repo>/<piece>`), a
branch each, so the pieces genuinely run at the same time. `charter worktree remove`
refuses if it would drop uncommitted or unpushed work.

### You are always teaching the agent the same things

`CLAUDE.md` holds what you sat down and wrote. It doesn't hold what the agent worked out at
2am — that the flaky checkout test is a DNS timeout rather than the code, that billing
deploys gate on the e2e suite and not the unit ones. That knowledge dies with the session,
so next week you explain it again or watch it get rediscovered the slow way.

```mermaid
flowchart LR
    w1["charter persona remember devops<br/>“billing deploys gate on the e2e suite”"]
    w2["charter ws remember<br/>“the ledger queue must drain before cutover”"]

    w1 --> pm["personas/devops/memory/*.md<br/>one fact per file"]
    w2 --> wm["workspaces/billing-migration/memory/*.md"]

    pm --> r{{"charter recall &quot;ledger&quot;"}}
    wm --> r
    sh["shared memory<br/>every persona reads it"] --> r

    r --> s["next session — yours,<br/>or a teammate's agent"]
```

One fact per markdown file. `charter recall` searches the workspace's notes, the active
role's own notes and the shared ones in a single pass — by keyword, or with `--since 2w`
and `--all-workspaces` for when you remember roughly *when* something was decided but not
where or in what words. They are ordinary committed files, so a teammate's agent can start
where yours left off.

### One agent that holds every credential and every context

A single agent carrying every token, every convention and every repo's history is both a
security problem and a quality one — it has access it doesn't need for the task in front of
it, and a context full of things that don't apply. A **persona** is a small named scope
(`devops`, `qa`, `reviewer`) with its own charter, its own committed memory, its own vault,
and a `delegate-when` line saying what should be handed to it. `charter persona
sync-agents` turns each one into a real Claude Code sub-agent, so dispatching a role is
ordinary delegation rather than a prompt trick. They compose the way people do: `extends:`
inherits a parent's charter, `uses:` says this role routes work to that one, and
`agent-tools` narrows what the generated sub-agent may touch.

### Credentials that end up in the transcript

The moment an agent reads a token, that token is in the context window — and from there in
the transcript, the logs, and any summary fed into a later prompt. A **vault** hands the
value to a *command* instead of to the model:

```mermaid
sequenceDiagram
    autonumber
    participant M as Claude Code session
    participant A as devops sub-agent
    participant C as charter CLI
    participant V as devops vault
    participant K as kubectl

    M->>A: "did prod roll out cleanly?"
    A->>C: secret exec devops --env TOKEN=API_TOKEN
    C->>V: resolve API_TOKEN
    V-->>C: value, in charter's process only
    C->>K: spawn, TOKEN in the child env
    K-->>C: output, may echo the value
    C-->>A: output, value redacted
    A-->>M: "rollout 3/3 ready"
    Note over M,A: no step here ever put the value in a context window
```

Reads are masked by default, `--reveal` refuses a non-interactive stdout, and the plugin's
guard denies both that flag and `cat`-ing a vault file outright. Read
**[docs/secrets.md](docs/secrets.md)** before storing anything real: the default provider
is plaintext at mode 0600, so what this buys you is *the model never seeing the value* —
not encryption at rest. The vault is not a password manager.

### Also in the box

- **An unattended run that stops to ask a question.** Every git operation authenticates
  with that repo's own forge CLI token over HTTPS — never an SSH key, never signing —
  because a passphrase prompt hangs an agent until it times out.
  → [docs/git-policy.md](docs/git-policy.md)
- **What the task still means to do.** Claude Code's task list dies with the session;
  `charter ws todo` records intent against the workspace, and each workspace carries a
  one-line **Vision** that a fork inherits.
- **Coming back cold, or handing work over.** `charter workspace snapshot` writes repos and
  branches into a committed manifest; `restore` rebuilds the whole thing on another
  machine; `fork` copies a workspace's charter, manifest and memory.
- **Everyone on a slightly different charter.** `[charter].version` in `charter.toml` pins
  one version like a lockfile. The pin is measured against the **plugin**, which Claude Code
  installs *per project* out of a cache holding every version at once — so two planes on one
  laptop can sit on different charters, and `claude plugin update charter@charter` moves
  this plane and no other.
  → [docs/control-plane.md](docs/control-plane.md)
- **Two planes fighting over one binary.** They no longer do. The `charter` CLI stays a
  single machine-global install for your own terminal; the version a *plane* runs is its
  plugin's. `charter version sync --cli` is still there for a machine with no plugin.
- **A rule you want everyone prompted for.** `charter guard ask 'terraform apply *'` writes
  a Claude Code `permissions.ask` rule into the plane's committed settings. charter keeps no
  list of its own — one record, nothing to sync.
  → [docs/adr/0014-policy-that-fits-a-pattern-belongs-to-the-host.md](docs/adr/0014-policy-that-fits-a-pattern-belongs-to-the-host.md)
- **A tool that silently stopped existing.** After a rename removed the shim they launched
  through, MCP servers failed with ENOENT and their tools simply vanished from the session.
  `charter doctor` now names any registered launcher whose path does not exist, and the
  one-line fix.
- **Who is in which tree.** A repo or worktree row says which persona was last seen working
  in it and how long ago — `▸steward now`, `▸forge 7m +1`. An observation with an age, never
  a claim that anyone is still there.
- **Seeing what any of it is doing.** `charter doctor` preflights, `charter trace` shows
  guard denials and tool approvals and memory writes, and `charter persona stats` says
  whether a role is actually being dispatched or whether that work is quietly routing to a
  generic agent.
- **A plane writing down charter's own rules, and getting them wrong later.** The plugin
  ships the skills for its surface — `charter:secrets` (use a credential without its value
  reaching the transcript), `charter:working-in-a-clone` (the boundary between the plane's
  session and a repo's own `.claude/`), `charter:persona` (adopting a role, and routing
  work to one instead of switching), and `charter:browser`. They version with the CLI, so a
  plane no longer needs a copy that can drift out from under it.
- **A browser login without the password in the transcript.** `charter:browser` bridges a
  vault into Playwright, so a credential is referred to by name and redacted from output.
  Charter ships the bridge and none of Playwright's own pages — those are Apache-2.0 and
  move far faster than charter releases, so `charter browser install` generates them from
  the tool that owns them, at whatever version you ask for.

## The model

![A control plane holds an inventory of every repo in the org, personas each with their own memory and vault, and one workspace directory per task. Each workspace holds repo clones on their own branches, and a clone can be split into git worktrees so parallel sub-agents each get a branch of the same repo.](docs/assets/model.svg)

- **Control plane** — any directory marked by `charter.toml`. Not a fixed location: `cd`
  anywhere beneath one and commands resolve it by walking up, the way git resolves `.git`.
- **Workspace** — an isolated, per-task directory of repo clones (`workspaces/<name>/<repo>`).
  `default` always exists; `charter workspace create <name> --use` starts a new one.
- **Worktree** — a further split *within* one workspace's clone
  (`workspaces/<ws>/.worktrees/<repo>/<piece>`), so parallel sub-agents each get their own
  branch of the *same* repo without re-cloning it.
- **Piece** — one worktree seen as a unit of work. Creating it *is* the claim, because git
  already arbitrates who wins the path; the worker later declares `done` or `abandoned`.
  There is deliberately no `failed` or `blocked` — a worker that dies declares nothing, and
  that **silence**, with an age, is what gets reported.
  → [docs/adr/0011-the-record-holds-only-what-git-cannot-know.md](docs/adr/0011-the-record-holds-only-what-git-cannot-know.md)
- **Persona** — a specialist role identity with a committed charter, persistent memory, and
  a named vault — dispatchable as an isolated Claude Code sub-agent. charter's
  differentiator; see [docs/personas.md](docs/personas.md).
- **Memory** — durable notes a persona or workspace records as it works. How far a note
  travels — disk only, committed, or pushed to the team — is one setting, `[memory].share`,
  and it **defaults to `local`**.
- **Vault** — where a persona's credentials live: plaintext JSON at mode 0600, with **no
  encryption at rest**. What it protects is different and real — keeping the value out of
  an agent's context and transcript.

## Learn more

Every page below is also readable from the CLI, so an agent working in a control plane
does not need a vendored copy that can drift from the binary it is describing:

```bash
charter docs list             # the topics
charter docs show secrets     # the page, from the install that implements it
```

`charter doctor` names a plane that has kept its own copy of one of these, or of a shipped
skill. It never tells you to delete it — an override may be deliberate — only that a local
copy wins, is compared to nothing, and drifts unwatched in both directions.

- [docs/install.md](docs/install.md) — both artifacts, alternatives to `uv`, and what
  `charter init` writes before you let it.
- [docs/control-plane.md](docs/control-plane.md) — `charter.toml` in full: every key, a
  self-hosted example, a mixed-forge example, the memory posture, the version pin.
- [docs/personas.md](docs/personas.md) — the charter format, inheritance, the memory model,
  and dispatching a persona as a sub-agent, end to end.
- [docs/workspaces.md](docs/workspaces.md) — the session lock and how to get out of it,
  LIVE vs LOCAL and where your notes actually go, and what belongs in `workspace.md`
  versus memory versus the manifest.
- [docs/secrets.md](docs/secrets.md) — exactly what the vault does and does not protect
  against, `secret exec`, and feeding a tool that wants a dotenv file.
- [docs/git-policy.md](docs/git-policy.md) — the one-credential rule, and why a denial from
  the plugin's guard is the rule working rather than a bug.
- [docs/hooks.md](docs/hooks.md) — everything the plugin does without being asked: what
  fires when, the four guards, what gets injected, and what gets counted.
- [docs/mcp.md](docs/mcp.md) — giving an MCP server a persona's vault credentials without
  the value entering the model, and what the per-persona tool allowlist does and does not
  constrain.
- [docs/forges.md](docs/forges.md) — what GitLab and GitHub each need, self-hosted hosts,
  and the rule for a repo name that collides across forges.

## Contributing

Issues and pull requests are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md) for the dev
setup and what a good change looks like, and [SECURITY.md](SECURITY.md) for how to report
something sensitive. The test suite is stdlib `unittest`:

```bash
python3 -m unittest discover -s tests
```

MIT licensed.
