---
version: unreleased
headline: a clone inside a workspace now carries **every** harness's in-repo surface, not Claude Code's — `.opencode/agent/` and `opencode.json` for opencode, `.codex/skills/` for Codex, each spelled by the harness rather than by charter
---

The entry above gave a clone at `workspaces/<ws>/<repo>/` the plane's layer. What it
carried was two paths, spelled out in `workspace.py`:

```python
WALKUP_DIRS = (".claude/agents", ".claude/skills")
```

That was recorded as a **stated limit** with the measured facts beside it, which is honest.
An honest limit is still a limit: an operator on opencode or Codex got a workspace with
none of the plane's agents or skills, and no row anywhere said so.

## The three surfaces, each measured against the installed binary

| harness | carried into a checkout | binary |
|---|---|---|
| Claude Code | `.claude/agents`, `.claude/skills` | 2.1.259 |
| opencode | `.opencode/agent`, `opencode.json` | 1.18.23 |
| Codex | `.codex/skills` | 0.147.0 |

Measured with real sessions, because a **management CLI is not a session** and answers for
the wrong thing — the trap that made the first survey of this conclude Codex had no in-repo
surface at all:

- **opencode's config.** Malformed JSON at `<repo>/opencode.json` fails the run outright —
  `Error: Config file at <repo>/opencode.json is not valid JSON(C)` — and a control repo
  without one runs clean.
- **opencode's agents.** A sentinel at `<repo>/.opencode/agent/probe.md` is a project
  agent; the control repo has none.
- **Codex's skills.** A sentinel at `<repo>/.codex/skills/<name>/SKILL.md` reaches a
  `codex exec` session's context with **zero tool calls**; the control repo answers `NONE`.
- **Codex's config, which is the one that is NOT carried.** A malformed project
  `.codex/config.toml` causes no error at all — Codex ignores it. Mirroring it would put a
  file in somebody's repo that nothing reads: charter's writing looking exactly like wiring
  and being inert, which is the failure shape this repo has already paid for twice.

## The spelling belongs to the harness

`Harness.inherited_paths` sits beside `layer` and `layer_note`, which the `session layer`
row added for exactly this shape, and `workspace.py` asks the registry. A test parses
`_inherited_files` and `_guest_files` with `ast` and fails if their **code** names any
harness or any harness path — the same pin `check_session_layer` already carries, one verb
over. Reporting a harness under another's discovery rules and *writing* another harness's
files are the same mistake; only the second one lands on disk.

It is a separate member from `layer` rather than derived from it, and the reason is what
each can honestly say. `layer` answers *"would a session started here find it, by which
rule"*, and charter has measured exactly two rules — cwd-only, and walk up to the git root.
opencode resolves `opencode.json` at the repository **root**, which is neither; declaring it
as a walking part would report a layer as reachable from an intermediate directory opencode
never reads there. This member answers something narrower that *is* measured for all three:
which of the plane's in-repo paths a nested checkout stops seeing.

## Everything the guest contract already promised still holds

The paths are new; the restraints are not, and each is re-asked of them:

- exact paths in the checkout's `.git/info/exclude`, never a directory glob — your own
  `.codex/` or `opencode.json` stays visible in your own `git status`;
- idempotent re-wiring, so the block never doubles;
- a file charter did not generate is never overwritten — and not hidden either. The guest's
  own `opencode.json` is the sharpest case in the set: a file at the repository root that a
  repo is quite likely to have, and rewriting it would change how their opencode runs;
- `.charter-generated` records a hash of each, so `charter doctor` still tells `stale` from
  `foreign`, and a stale copy is refreshed rather than left for the next upgrade;
- removing the workspace removes them, pruning the directories that emptied.

**`CLAUDE.md` is still deliberately left behind**, and so is any equivalent. It walks up on
the same rule, so the gap is real for it too — and it is the one file a repo of its own is
most likely to have opinions about. A guest hides its own files; it does not narrate the
host's.

A workspace **directory** gets none of this and needs none of it: every one of these paths
is resolved from inside the plane's own repository, so the plane's copies already reach it
and a second copy would shadow the first. The ceiling `charter harness list` names for
opencode and Codex — that a workspace directory is not a config scope, so two workspaces on
one machine cannot be made to differ — is untouched and still true.
