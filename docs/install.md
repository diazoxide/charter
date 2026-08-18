# Install

charter ships as **two artifacts** — install both. A CLI-only install leaves the plugin's
hooks inert (no session context injection, no golden-rule guard, no auto-save), since the
plugin is what actually wires them into Claude Code.

## Paste this into Claude Code

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

## 2. The Claude Code plugin

This repo also ships as a Claude Code plugin — `.claude-plugin/plugin.json` +
`hooks/hooks.json` — installed the way you install any Claude Code plugin from a git repo.
From a shell:

```bash
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
that enforces the [one-credential rule](git-policy.md), the record-memory nudges, and the
Stop-hook auto-save. **The plugin ships no Python of its own** — every hook it declares
just shells out to the `charter` CLI you installed in step 1, so the CLI must be on `PATH`
first. The CLI works standalone for everything else (`charter clone`, `charter persona
show`, …) with the plugin absent; install the plugin too if you want charter actively
driving a live session, not just scripted from a terminal.

## 3. opencode and Codex

Each harness gets **one installed artifact**, the same way Claude Code does. Nothing is
written into the repos you work in.

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

## First control plane## First control plane

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
