---
version: unreleased
headline: `charter doctor` grows a `session layer` row that answers whether a session started in this directory can see charter's layer — naming which of settings, skills+agents or the status line is missing, which discovery rule decided, and whether the directory is trusted enough for any of it to run
---

An operator opened a chat in a workspace directory. **No skills, no agents, no plugin** —
and nothing in `charter doctor` said why. Every row in that report was telling the truth.
None of them could have carried the answer, because the answer is not one fact.

Measured on Claude Code 2.1.259:

| artefact | how it is found |
| --- | --- |
| `.claude/settings.json` | the session's **own** directory. No walk-up. |
| `.claude/agents/`, `.claude/skills/` | walk **up**, stopping at the **git root**. |
| `CLAUDE.md` | walks up, and is **not** git-bounded. |

Three rules. So a chat can have charter's prose and none of charter's machinery, which is
exactly what "it felt half-configured" meant, and no single row could say so.

## The row

```
✓  session layer  /plane/workspaces/fleet/api — what a session started here would find in the repo
      ↳ claude-code: settings ✗ — project settings are read from the session's OWN
        directory and the host does not walk up, so nothing above it is in force;
        skills+agents ✗ — skills and agents DO walk up, but the walk stops at the git
        root — anything charter wrote above that boundary is out of reach, while
        CLAUDE.md walks up and is NOT git-bounded, which is why a session like this reads
        as half-configured rather than empty; status line ✗ — `statusLine` is a key in
        that same settings file, so it is cwd-only too
      ↳ trust: /plane/workspaces/fleet/api is a git root of its own, so it carries its own
        trust acceptance — until that is given, hooks or the status line do not run here
        whatever any settings file declares. `guard seen` cannot answer it: that state
        lives under <plane>/.charter, so it is per PLANE and a sighting there says nothing
        about this directory
```

**A fact, never a verdict — OK even when nothing reaches.** A chat can be rooted in a
directory that reaches none of this and be exactly where it belongs — a clone charter has
not wired yet, or one whose harness declares no layer parts. A row that warned there is a
row operators learn to skip. Whatever is missing *and* fixable already warns where it can
name its own remedy — `workspace layer` for a generated file gone stale, `plane-root guard`
for the guard.

This paragraph read *"charter deliberately writes nothing inside `workspaces/<ws>/<repo>/`"*.
Two entries in this same release retract it: *a clone inside a workspace gets the layer*
writes it there, and *a clone inside a workspace carries every harness's in-repo surface*
decides what it carries. The argument is unchanged; it now rests on what a session would
find rather than on charter having written nothing.

It is about the **layer**, not the plugin, so it does not reopen `check_guard_wired`'s
reasoning: *"Whether the plane runs as a plugin is an implementation detail. Whether the
guard fires is the fact the operator needs."* That stands, in its own two rows.

## Declared is not fired

Claude Code gates hook execution **and** the status line on the directory being trusted,
globally — the gate takes no argument saying which settings source declared them. So

* *"a file this session reads declares `charter hook pretooluse`"*, and
* *"charter's guard will fire here"*

are two different facts, and an untrusted directory answers **yes** to the first and **no**
to the second.

Trust is inherited up to the git root. `workspaces/<ws>/` is inside the plane's own
repository and rides its acceptance — the `+` button and every workspace tab are fine, and
the row says nothing about them. A clone at `workspaces/<ws>/<repo>` and a linked worktree
have a git root of their own and need their own.

**Charter asks the question it owns.** `git rev-parse --show-toplevel`, and no
host-private state. Reading `hasTrustDialogAccepted` out of `~/.claude.json` would need two
things nobody has measured: that a *missing* project entry means "not trusted" rather than
"never opened", and that the flag charter reads is the one the host actually gates on
across versions. Reading absence as refusal warns at planes that are fine. Naming a
condition is weaker than returning a verdict, and it cannot be wrong in the direction that
matters.

And it says why `guard seen` cannot stand in: `guardseen` state lives under
`config.STATE_DIR`, so that row answers **per plane**. A guard that fired in the plane last
week still shows a recent sighting for a session rooted in a clone where nothing has ever
dispatched.

## The rules live on the harness

`Harness.layer` — a `LayerPart` per artefact, carrying its paths, whether it walks, and the
measured sentence a miss is reported with. Written into `doctor` they would have been the
hardcoded-literal-per-harness failure `harness/registry.py` exists to end: a fourth harness
would be reported under Claude Code's discovery rules by default, which is *verifying a
proxy instead of the fact* — #168, #177, #261 and #851 in one line. A test parses
`check_session_layer` and fails if it names a harness or a harness path itself.

## A retraction, measured

Charter had recorded that opencode and Codex have **no project-level config at all**. That
is too broad, and re-measuring against real sessions — not management CLIs — says so:

* **Codex 0.147.0** ignores a project `.codex/config.toml`, as recorded. But it **does**
  read `.codex/skills/` from the project: a sentinel skill at
  `<repo>/.codex/skills/<name>/SKILL.md` reached the model's context with **zero tool
  calls**, where a control repository without one did not.
* **opencode 1.18.23** reads an `opencode.json` at the **repository root** — a malformed
  one fails the run outright, `Error: Config file at <repo>/opencode.json is not valid
  JSON(C)` — and reads `.opencode/agent/` from the project.

The trap is worth writing down, because it caught the measurement three times: **a
management CLI is not a session.** `codex mcp list` and `claude plugin marketplace list`
both ignore project config, so probing with them yields a confident false negative. And a
model asked whether it can see a file will cheerfully go and `sed` the file you just named
— so the trace, not the answer, is the evidence.

So all three harnesses have an in-repo surface, and the row says so for each of them rather
than implying the surface does not exist. This entry went on to say charter *writes* one for
Claude Code only, and by the end of this release that is no longer true either: every
harness declares its own `inherited_paths` — `.claude/agents` and `.claude/skills`,
`.opencode/agent`, `.codex/skills` — and a clone inside a workspace carries all of them (*a
clone inside a workspace carries every harness's in-repo surface*). What is still Claude
Code's alone is the generated `.claude/settings.json` per workspace **directory**, because
the other two have no per-workspace config scope to hold one. Charter still writes
**nothing** into `~/.config/opencode/` or `~/.codex/config.toml`: it
writes no machine-global state on the operator's behalf, which is a decision and not an
oversight.

The two `workspace-scope` deficits kept their conclusion and lost their reason. Neither
harness can hold **per-workspace** config, and that is still true — a workspace directory
is not a config scope for either — but *"config is machine-global"* and *"no project-level
config exists at all"* were the wrong grounds for it, and a reader following either
sentence would have stopped looking for a surface that is there. `charter doctor`'s aside,
`docs/harnesses.md`, `docs/workspaces.md` and the earlier entry that carried the same claim
all say the narrower thing now.
