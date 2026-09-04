# Install

**One command installs charter.**

```bash
uv tool install charter-cp
```

That is the CLI, and the CLI is the front door: it is what sits on your `PATH`, what every
hook charter declares shells out to, and what writes `.claude/settings.json`. Claude Code's
charter plugin is the *second* artifact, and you do not install it by hand — `charter init`
installs it for the plane it creates, and `charter doctor --fix` installs it for a plane
that already exists. The inverse shape, a plugin that bootstraps a CLI, would have to guess
at a Python environment it does not own.

Nothing installs itself. `init` and `doctor --fix` are the only two commands that install
anything, and both are commands you typed; `charter workspace list` will never install
software as a side effect of answering a question.

## Paste this into Claude Code

Installed and checked, without looking anything up:

```
Install charter (https://github.com/diazoxide/charter) for me:

1. Run `uv tool install charter-cp`. charter needs Python 3.11+, which uv can
   fetch for me; fall back to pipx or pip only if uv is missing.
2. Run `charter doctor` and show me the output. Its `plugin install` row will say
   Claude Code's charter plugin is missing — that is expected here, because the
   plugin is installed per control plane and there is no plane yet.
3. Do NOT run `charter init` — tell me what it would create and let me pick the
   directory first. `charter init` is what installs the plugin, for the plane it
   creates.

Then tell me to restart this session once the plane exists, so the plugin's hooks load.
```

Step 3 is not caution for its own sake: `charter init` makes the directory it runs in a
control plane, writing `charter.toml` and scaffolding `personas/`, `inventory/` and
`workspaces/` (see [control-plane.md](control-plane.md)). Run from an unrelated project by
accident, it would quietly convert it — so the prompt stops before the step that writes
into your working directory, and hands the choice back to you.

The manual steps below are the same install, written out. Reach for them when you want to
know exactly what is happening, or when the prompt does something you did not expect.

## 1. The CLI

```bash
uv tool install charter-cp     # installs the `charter` command
```

Lead with [`uv`](https://docs.astral.sh/uv/) for a concrete reason, not a preference:
charter requires **Python ≥ 3.11** (it leans on stdlib `tomllib`, which is 3.11+ only), and
stock macOS ships 3.9. `uv tool install` can fetch and manage a suitable Python for you;
`pipx` and `pip` both require one to already be on your `PATH`.

Alternatives, once you have a 3.11+ Python:

```bash
pipx install charter-cp        # installs the `charter` command
pip install charter-cp
```

Or run it without installing anything:

```bash
uvx --from charter-cp charter <cmd>
```

> **The package is `charter-cp`; the command is `charter`.** PyPI would not allow
> `charter` as a project name, so the distribution carries a suffix — but everything you
> type, and everything in the docs, is `charter`.

## 2. The Claude Code plugin — charter installs it

This repo also ships as a Claude Code plugin — `.claude-plugin/plugin.json` +
`hooks/hooks.json` — and you do not type the commands for it:

```bash
charter init          # creates a control plane, and installs the plugin for it
charter doctor --fix  # installs it into a plane that already exists
```

**Per plane, at `project` scope, and that is a feature rather than an accident.** Claude
Code keeps a cache holding every version of a plugin at once, so a plane's pinned version
is the plugin's and not the binary's: two planes on one laptop can sit on different
charters without fighting. A machine-wide install would collapse that to one version, and
would put charter's hooks into repositories you never pointed charter at.

`charter doctor` reports the plugin's absence on its own `plugin install` row and names
`charter doctor --fix` as the remedy. It **warns** rather than failing: a plane that
declares `charter hook pretooluse` in its own `.claude/settings.json` is guarded with no
plugin at all, and a CLI-only install is supported — `charter clone`, `charter persona
show` and the rest work with no plugin anywhere.

The same row says so when the plugin is installed and **disabled**, because an installed
plugin loads nothing until it is enabled. charter will not enable it for you — `claude
plugin enable charter@charter --scope project` is yours to run, since turning a plugin off
is a choice and charter does not revert a deliberate edit.

By hand, if you would rather, or if `charter doctor --fix` could not (an old `claude`, no
network):

```bash
claude plugin marketplace add diazoxide/charter
claude plugin install charter@charter --scope project
```

Or inside a session: `/plugin marketplace add diazoxide/charter`, then `/plugin install
charter@charter`. Consult Claude Code's own `claude plugin --help` if that flow has moved
on since this was written.

The plugin loads on the **next** session, so restart after installing. Upgrading the CLI
does not upgrade the plugin — they are two artifacts with two version numbers, pinned to
each other, so `claude plugin update charter@charter` is its own step, and skew between
them is normal rather than exceptional. `charter doctor` **refuses** on skew, with a row
naming the exact command; charter's hooks only **warn**, at session start and nowhere else.
That asymmetry is deliberate: a hook that hard-failed on a version mismatch would brick
every tool call in the session, turning a cosmetic difference into an outage. A plugin
*newer* than the CLI says so loudly at session start; an older one is quietly supported.

The plugin supplies the pieces that only make sense running *inside* a Claude Code
session: injecting the active persona's memory at session start, the `PreToolUse` guard
that enforces the [one-credential rule](git-policy.md), the record-memory nudges, and the
Stop-hook auto-save. **The plugin ships no Python of its own** — every hook it declares
just shells out to the `charter` CLI you installed in step 1, which is why the CLI is the
artifact you install and the plugin is the one it installs for you.

## 3. opencode and Codex

Each harness gets **one installed artifact**, the same way Claude Code does. Nothing is
written into the repos you work in.

**Only Claude Code's artifact is one charter installs for you, and the difference is
scope.** The Claude Code plugin is installed per project, so `charter init` installing it
touches exactly the plane you just asked charter to create. Codex's wiring lives only in
`~/.codex/config.toml` — a machine-global file, in force for every repository on the
machine — so charter writes it only when you run `charter harness install codex`, where
running the command *is* the consent. `charter doctor` reports the gap and stops there;
one documented one-time edit is a smaller cost than charter silently rewriting a
machine-wide config, and unlike the plugin that file is not something a charter upgrade
has to keep in lockstep.

**opencode — `charter init` does it.** The plugin goes to `~/.config/opencode/plugin/`
(`$XDG_CONFIG_HOME` is honoured), which opencode reads for every project, along with a
`/charter` command and the session context it reads at startup. `charter doctor`'s
`harness` row reports it, and `charter reinit` reinstalls if you ever delete it.

Earlier charters wrote a plugin into every clone and worktree instead, because opencode
does not search parent directories for *project* plugins. It does read the config dir, so
that was a lot of files in other people's repositories answering a question that did not
need asking. If you have `.opencode/` directories lying around in clones, they are inert
and safe to delete.

**Codex — the plugin, plus one line.** Codex installs the same plugin charter ships for
Claude Code, through `codex plugin`. That covers every hook. The one thing a plugin cannot
do is tell a shell which harness it is, so:

```bash
charter harness install codex
```

writes exactly that into `~/.codex/config.toml`:

```toml
[shell_environment_policy]
set = { CHARTER_HARNESS = "codex" }
```

If it finds hooks declared in that file it **refuses and says so**. An earlier charter
wrote them there before the plugin route was known, and both sets are trusted and both
run — charter fires twice on every SessionStart, UserPromptSubmit and Bash call. Nothing
is wrong; everything is doubled, which is harder to notice. Delete charter's block from
`config.toml` and keep the plugin.

## 4. The dev channel — trying main without cutting a release

**Opt-in, per control plane, one key.** Absent, nothing here happens and you track
published releases as before.

```toml
[update]
channel = "dev"     # "stable" (the default) | "dev"
```

A dev build is installed straight from git — never from PyPI:

```bash
uv tool install --force git+https://github.com/diazoxide/charter@main
```

`charter update` runs exactly that for you on a plane that declares the channel, and then
does the two things a bare install does not: it moves this harness's charter artifact, and
it force-refreshes the Claude Code plugin (see below for why that needs forcing).

**Dev builds are never published, and that is the design rather than a limitation.** PyPI
forbids local version identifiers, so a real dev release would have to burn `0.52.0.dev1`,
`.dev2`, … permanently, at a rate of hundreds a month, irreversibly. And publishing on
every push would mean running the release workflow — which holds `id-token: write` — on
every merge, multiplying exactly the exposure that workflow is already being narrowed
about. CI verifies the git install on every push to `main` instead: same coverage, no
publish, no token.

**Which channel you are on is on the status line.** The brand chip reads `⬢ charter 0.51.0
dev`, so `↑a1b2c3d` beside it can only mean one thing — main moved. `charter --version`
answers the other half, which is what you are actually running:

```
$ charter --version
charter 0.51.0+dev (main @ a1b2c3d)      # a git install
charter 0.51.0                            # a published one
```

That is read from the dist-info's PEP 610 `direct_url.json`, which a VCS install writes and
a PyPI install does not — so it is the install itself talking, not a number somebody
remembered to stamp.

**Nothing installs itself.** When main is ahead, charter *nudges*; you run `charter
update`. Auto-installing unreviewed merges is committed content reaching execution without
a moment of consent, which is the one thing charter will not do to you.

**A plane cannot ask for both a pin and the dev channel.** `[charter] version` names a
published release the whole team conforms to; `main` has no such number. Declare both and
charter installs neither, and says so at session start.

**Going back** is one command — `charter update --to 0.51.0` installs the published release
without editing anything — or delete the `[update]` block and run `charter update`.

### The plugin needs forcing, and only on this channel

`claude plugin update charter@charter` compares **version strings**, and the plugin's
version moves once per release. The marketplace is a git clone of `main` that Claude Code
re-fetches on its own, so between releases the clone moves and the installed copy does not,
both still say `0.51.0`, and the update command correctly reports there is nothing to do.
Measured on one machine: 45 files apart, `skills/secrets/SKILL.md` and
`skills/browser/SKILL.md` among them.

Hooks are unaffected — `hooks/hooks.json` invokes the `charter` on your `PATH`, so hook
behaviour follows the CLI. **Skills are the part that goes stale**, and skills are text the
model loads.

So `charter update` on the dev channel uninstalls and reinstalls the plugin, which is the
only mechanism that repopulates a version-keyed cache directory. And `charter doctor` now
compares the two by **content** on *both* channels — a `plugin files` row that names the
digests and the files that differ, because a version number that is frozen by design cannot
answer the question.

## First control plane

```bash
mkdir my-control-plane && cd my-control-plane
charter init --forge github --owner my-org
charter doctor
charter discover
charter clone some-repo
```

`--forge` is `gitlab` (the default) or `github`; `--owner` is the GitLab group or GitHub
org/user whose repos this control plane tracks. Run inside an existing git repo, `init`
also *offers* to clone that repo into your first workspace — accept with `charter init
--clone-this-repo`, because work happens in a workspace, never in the plane root.
