# charter

**charter** is a control plane for Claude Code agents working across many repos on GitHub
or GitLab: durable **personas**, isolated per-task **workspaces**, and a credential
**vault** the model never reads from.

## What it solves

Each of these is something that went wrong often enough to get built around.

### Dozens of repos, and several tasks in flight at once

A team's work doesn't sit in one repository, and an engineer rarely has one task open.
Two features and a hotfix means three sets of branches across a shifting set of repos, and
if they share a checkout you spend the day stashing. A **workspace** is one directory of
clones per task (`workspaces/<task>/<repo>`), each repo on its own branch, so moving
between tasks is `charter workspace use <name>` and nothing follows you across. The
status line shows which one is active and what state every clone in it is in.
`charter discover` keeps the map of every repo in the org — including the ones nobody has
cloned yet — so an agent can find a repo before it exists locally.

### Two sub-agents that need the same repo

Splitting a task across parallel sub-agents falls apart the moment both want the same
repository on different branches. Cloning it twice wastes the disk and they still collide.
A **worktree** splits one clone into several checkouts (`.worktrees/<repo>/<piece>`), a
branch each, so the pieces genuinely run at the same time. `charter worktree remove`
refuses if it would drop uncommitted or unpushed work.

### You are always teaching the agent the same things

`CLAUDE.md` holds what you sat down and wrote. It doesn't hold what the agent worked out
at 2am — that the flaky checkout test is a DNS timeout rather than the code, that billing
deploys gate on the e2e suite and not the unit ones. That knowledge dies with the session,
so next week you explain it again or watch it get rediscovered the slow way.
`charter persona remember` and `charter workspace remember` write one fact per markdown
file; `charter recall` searches the workspace's notes, the active role's own notes and the
shared ones in a single pass. They're ordinary committed files, so a teammate's agent can
start where yours left off.

### One agent that holds every credential and every context

A single agent carrying every token, every convention and every repo's history is both a
security problem and a quality one — it has access it doesn't need for the task in front
of it, and a context full of things that don't apply. A **persona** is a small named scope
(`devops`, `qa`, `reviewer`) with its own charter, its own committed memory, its own vault,
and a `delegate-when` line saying what should be handed to it. `charter persona
sync-agents` turns each one into a real Claude Code sub-agent, so dispatching a role is
ordinary delegation rather than a prompt trick. They compose the way people do: `extends:`
inherits a parent's charter, `uses:` says this role routes work to that one, and
`agent-tools` narrows what the generated sub-agent is allowed to touch.

### Credentials that end up in the transcript

The moment an agent reads a token, that token is in the context window — and from there in
the transcript, the logs, and any summary fed into a later prompt. A **vault** holds the
value and hands it to a *command* rather than to the model:

```
charter secret exec devops --env TOKEN=API_TOKEN -- some-tool
```

charter resolves the secret in its own process, puts it in the child's environment, and
redacts every occurrence of it from whatever that command prints. Reads are masked by
default, `--reveal` refuses a non-interactive stdout, and the plugin's guard denies both
that flag and `cat`-ing a vault file outright. Read [docs/secrets.md](docs/secrets.md)
before storing anything real: the default provider is plaintext at 0600, so what this buys
you is the model never seeing the value — not encryption at rest.

### An unattended run that stops to ask a question

An SSH key passphrase or a GPG signing prompt hangs an autonomous agent until it times
out, because there is nobody at the keyboard to answer it. Every git operation charter
performs authenticates with that repo's own forge CLI token over HTTPS — `gh` or `glab`,
never a key, never signing. `charter git-policy --apply` writes that into a repo's local
git config, and `charter clone` applies it to everything it clones. The plugin denies the
commands that would route around it, which is why a denial there is the rule working
rather than a bug.

### What the task still means to do

Claude Code's task list is per-session: close the terminal and the intent is gone. That's
fine for "run the tests" and useless for "we still owe the billing team a migration".
`charter ws todo "<what>"` records intent against the **workspace**, so it outlives the
session that noticed it — finishing one journals it, abandoning one goes quietly. Each
workspace also carries a **Vision**, one line saying what the whole task is for, which a
fork inherits.

### Coming back to a task cold, or handing it to someone else

Two weeks later a workspace is a pile of repos on branches whose names you no longer
trust. `charter workspace snapshot` writes the repos and their branches into a committed
manifest and `charter workspace restore` rebuilds the whole thing from it, on your machine
or a colleague's. `charter workspace fork` copies a workspace's charter, manifest and
memory so you can branch a task off with its context intact. Workspaces stay private until
you decide otherwise; `charter workspace live` is what makes one shareable.

### Everyone on the team running a slightly different charter

Once a control plane is shared, the version each engineer happens to have installed stops
being a private detail: hooks fire differently, a guard denies on one machine and not the
next, and bug reports stop lining up. It's opt-in — with no pin, charter tracks whatever
you have — but `[charter].version` in `charter.toml` pins one version the way a lockfile
does, and the `SessionStart` hook conforms each machine to it once per session, never
mid-turn. The pin is exact rather than a floor, so it downgrades too; putting a team back
on a known-good release is precisely the case worth automating. `charter version bump`
installs and verifies the target *before* writing the lock, so you can't pin colleagues to
a build you haven't run yourself, and charter only ever shows you that command — it never
moves the pin on its own. A conform that fails (offline, no `uv`) warns and gets out of
the way instead of blocking you, and the drift stays visible in `charter doctor` until
someone deals with it.

### Not being able to see what any of it is doing

Across four repos and three roles, "is anything broken" shouldn't take six commands to
answer. The status line carries the active workspace and its open todos, every clone with
its branch, dirty and ahead/behind markers, CI and open PRs, beside the personas, their
vault health and how much each has remembered — all read from disk, with no git subprocess
and no network on the render path. `charter doctor` preflights the environment before an
agent discovers the problem halfway through a task. `charter trace` shows what actually
happened in a session: guard denials, tool approvals, secret warnings, memory writes. And
`charter persona stats` says whether a role is being dispatched at all, or whether that
work is quietly routing to a generic agent instead.

---

If you've never seen it before, you can go from `uv tool install charter-cp` to a working
control plane in about a minute — see [60 seconds](#60-seconds-from-nothing-to-a-working-control-plane) below.

## Install

charter ships as **two artifacts** — install both. A CLI-only install leaves the plugin's
hooks inert (no session context injection, no golden-rule guard, no auto-save), since the
plugin is what actually wires them into Claude Code.

### Paste this into Claude Code

Both artifacts, installed and checked, without looking anything up:

```
Install charter (https://github.com/diazoxide/charter) for me:

1. CLI: run `uv tool install charter-cp`. charter needs Python 3.11+, which uv can
   fetch for me; fall back to pipx or pip only if uv is missing.
2. Plugin: run `claude plugin marketplace add diazoxide/charter`, then
   `claude plugin install charter@charter`.
3. Run `charter doctor` and show me the output.
4. Do NOT run `charter init` — tell me what it would create and let me pick the
   directory first.

Then tell me to restart this session so the plugin's hooks load.
```

Step 4 is not caution for its own sake: `charter init` makes the directory it runs in a
control plane, writing `charter.toml` and scaffolding `personas/`, `inventory/` and
`workspaces/` (see [docs/control-plane.md](docs/control-plane.md)). Run from an unrelated
project by accident, it would quietly convert it — so the prompt stops before the step that
writes into your working directory, and hands the choice back to you.

The manual steps below are the same install, written out. Reach for them when you want to
know exactly what is happening, or when the prompt does something you did not expect.

### 1. The CLI

```
uv tool install charter-cp     # installs the `charter` command
```

Lead with [`uv`](https://docs.astral.sh/uv/) for a concrete reason, not a preference:
charter requires **Python ≥ 3.11** (it leans on stdlib `tomllib`, which is 3.11+ only),
and stock macOS ships 3.9. `uv tool install` can fetch and manage a suitable Python for
you; `pipx` and `pip` both require one to already be on your `PATH`.

Alternatives, once you have a 3.11+ Python:

```
pipx install charter-cp        # installs the `charter` command
pip install charter-cp
```

Or run it without installing anything:

```
uvx --from charter-cp charter <cmd>
```

> **The package is `charter-cp`; the command is `charter`.** PyPI would not
> allow `charter` as a project name, so the distribution carries a suffix — but
> everything you type, and everything in this README, is `charter`.


### 2. The Claude Code plugin

This repo also ships as a Claude Code plugin — `.claude-plugin/plugin.json` +
`hooks/hooks.json` — installed the way you install any Claude Code plugin from a git repo.
From a shell, which is also what the paste-in prompt above runs:

```
claude plugin marketplace add diazoxide/charter
claude plugin install charter@charter
```

Or inside a session: `/plugin marketplace add diazoxide/charter`, then `/plugin install
charter@charter`. Consult Claude Code's own `claude plugin --help` if that flow has moved
on since this was written.

The plugin loads on the **next** session, so restart after installing. Upgrading the CLI
does not upgrade the plugin — they are two artifacts with two version numbers, pinned to
each other, so `claude plugin update charter@charter` is its own step. A plugin *newer*
than the CLI says so loudly at session start; an older one is quietly supported.

The plugin supplies the pieces that only make sense running *inside* a Claude Code
session: injecting the active persona's memory at session start, the `PreToolUse` guard
that enforces the one-credential rule below, the record-memory nudges, and the
Stop-hook auto-save. **The plugin ships no Python of its own** — every hook it declares
just shells out to the `charter` CLI you installed in step 1, so the CLI must be on
`PATH` first. The CLI works standalone for everything else (`charter clone`, `charter
persona show`, …) with the plugin absent; install the plugin too if you want charter
actively driving a live session, not just scripted from a terminal.

## 60 seconds: from nothing to a working control plane

```
mkdir my-control-plane && cd my-control-plane
charter init --forge github --owner my-org
charter doctor
charter discover
charter clone some-repo
```

- **`charter init`** scaffolds `charter.toml`, the baseline directories
  (`personas/`, `inventory/`, `workspaces/`), a `.gitignore` tuned for the layout, and a
  Claude Code status line — additive and idempotent, so re-running it is always safe.
  `--forge` is `gitlab` (the default) or `github`; `--owner` is the GitLab group or
  GitHub org/user whose repos this control plane tracks. Run inside an existing git repo
  it also *offers* to clone that repo into your first workspace — accept with
  `charter init --clone-this-repo`, because work happens in a workspace, never in the
  plane root.
- **`charter doctor`** preflights the environment (python, git, git identity, the
  forge's CLI and its auth) and tells you exactly what's missing before anything else
  tries to use it.
- **`charter discover`** queries the forge and writes `inventory/repos.json` — the
  durable, git-tracked map of every repo in the group, complete even when nothing is
  cloned yet.
- **`charter clone <repo>`** clones a repo on demand into the active workspace
  (`workspaces/default/<repo>/`), already configured with the one-credential git policy
  below.

## Concepts

- **Control plane** — any directory marked by `charter.toml`. Not a fixed location: `cd`
  anywhere beneath one and commands resolve it by walking up, the way git resolves
  `.git`. See `docs/control-plane.md` for the file in full.
- **Workspace** — an isolated, per-task directory of repo clones
  (`workspaces/<name>/<repo>`), so several tasks can each hold their own repos on their
  own branches without stepping on each other. `default` always exists; `charter
  workspace create <name> --use` starts a new one.
- **Worktree** — a further split *within* one workspace's clone of a repo: several git
  worktrees over one clone (`workspaces/<ws>/.worktrees/<repo>/<piece>`), so parallel
  sub-agents can each work their own branch of the *same* repo without re-cloning it.
- **Persona** — a specialist role identity (`devops`, `qa`, …) with a committed charter,
  its own persistent memory, and a named credential vault — dispatchable as an isolated
  Claude Code sub-agent. This is charter's differentiator; see the worked example below
  and `docs/personas.md`.
- **Memory** — durable notes a persona or workspace records as it works
  (`charter persona remember` / `charter workspace remember`). How far a note travels —
  disk only, committed locally, or pushed to the team — is one setting,
  `[memory].share`, and it **defaults to `local`**: see `docs/control-plane.md`.
- **Vault** — where a persona's credentials live: plaintext JSON at file mode 0600, with
  **no encryption at rest**. What it protects against is different and real — keeping a
  secret value out of an agent's context and transcript. Read `docs/secrets.md` before
  storing anything real in one; the vault is **not a password manager**.

## Worked example: a persona, end to end

```
charter persona create devops --role "DevOps Engineer" \
  --delegate-when "CI/CD pipelines, k8s deploys, cluster access" --with-vault
charter persona use devops
charter persona secret set API_TOKEN --stdin           # value never touches argv/history
charter persona remember "prod kubeconfig lives in the devops vault, key KUBECONFIG"
# …write what the persona owns, then drop the `draft: true` line it was created with:
charter persona sync-agents
```

A new persona starts as `draft: true` and gets **no** generated sub-agent until that
line is removed — an unwritten charter must never become an agent's system prompt.
`charter persona lint`, `charter doctor` and the status-line chip (`⚑`) all say so
meanwhile.

The last step writes `.claude/agents/devops.md` — a generated Claude Code sub-agent
carrying devops's charter, its memory instructions, and a reminder to use the vault
(`exec`/`cp`, never `--reveal`). From here on, any session can hand work to it in an
isolated context instead of guessing with borrowed credentials:

```
Agent(subagent_type: "devops", prompt: "Check whether the prod deployment rolled out cleanly.")
```

The devops sub-agent runs with *its own* vault and *its own* memory — it can read the
`prod kubeconfig` note it (or a teammate) recorded earlier, pull `API_TOKEN` via
`charter persona secret exec`, and never expose the raw value back to the caller. Every
dispatch like this is tallied (agent name + date, never the prompt) so `charter persona
stats` can show whether devops is actually being used, or whether that work is quietly
routing to a generic agent instead. Full format, inheritance, and the memory model:
`docs/personas.md`.

### Feeding a tool that wants a dotenv secrets file

Some tools take a *file* of secrets rather than env vars. `--dotenv` writes one
0600 temp file containing every entry you name, points an env var at its path,
and deletes it when the command exits — so no value is ever printed, stored, or
placed in argv.

```bash
charter secret exec qa \
  --dotenv PLAYWRIGHT_MCP_SECRETS_FILE=APP_USER:platform-user \
  --dotenv PLAYWRIGHT_MCP_SECRETS_FILE=APP_PASS:platform-pass \
  -- npx @playwright/cli@0.1.18 -s=login fill e3 APP_PASS
```

Repeats sharing an env-var name merge into a single file, in flag order.
Different names produce separate files. Defining the same NAME twice under one
ENVVAR is an error (exit code 2).

The value is never typed by the caller: the tool refers to the secret by the
**name** you gave it (`APP_PASS`), and resolves it from the file. Any
value that does appear in captured output is redacted.

`--dotenv` cannot be combined with `--exec` — exec replaces this process, so
the temp file would never be cleaned up. Use `--env` for an exec'd command.

## The one-credential rule

Every git operation charter performs — from any persona, any sub-agent, any repo clone —
authenticates with **that repo's own forge's CLI token, over HTTPS**: `glab` for GitLab,
`gh` for GitHub. Never an SSH key, never commit/tag signing. `charter git-policy --apply`
writes this into a repo's *local* git config (a credential helper, `commit.gpgsign =
false`, and SSH→HTTPS URL rewrites so even a repo whose remote is an SSH URL still
transports over HTTPS); `charter clone` applies it automatically to everything it clones.

This is deliberate, not incidental: an **SSH key prompt or a GPG signer prompt hangs an
autonomous agent** mid-run — there's no human at the keyboard to answer it. One
credential, held by the forge's own CLI, over HTTPS, is the only shape that can never
block on a question nobody is there to answer.

The Claude Code plugin's `PreToolUse` guard **denies** a command that would bypass this
— a raw SSH GitLab/GitHub URL handed to git, `GIT_SSH_COMMAND=`, `-S`/`--gpg-sign`, `ssh
-T git@github.com`. **If you hit one of these denials, that is the rule working, not a
bug** — the message names the fix (usually: nothing, since `charter git-policy --apply`
already configured the repo correctly). Check the credential with `glab auth status` /
`gh auth status`, never `ssh -T`.

## Learn more

- `docs/control-plane.md` — `charter.toml` in full: every key, a self-hosted example, a
  mixed-forge example, and the memory posture (`local`/`commit`/`push`) in detail.
- `docs/forges.md` — what GitLab and GitHub each need, self-hosted hosts, and the rule
  for a repo name that collides across forges.
- `docs/personas.md` — the charter format, the memory model, and dispatching a persona
  as a sub-agent.
- `docs/secrets.md` — exactly what the vault does and does not protect against.

Development: the test suite is stdlib `unittest` — `python3 -m unittest discover -s
tests`. Report issues at [github.com/diazoxide/charter](https://github.com/diazoxide/charter).
