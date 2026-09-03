---
version: unreleased
headline: a clone inside a workspace gets charter's layer too — written into the clone and hidden in that checkout's own `.git/info/exclude`, so your repo's `git status` never sees it
---

A workspace can hold a clone at `workspaces/<ws>/<repo>`, and that clone is a **separate
git repository**. The entry above closed the gap for `workspaces/<ws>/` and stated the
clone as a permanent limit: *"in a clone you cannot delegate to a persona."* It is closed
now, in the same release, and this is the argument for it.

## The clone is where the gap is widest

Three things reach a session by three different routes, and only in a clone do all three
fail at once:

```
                        plane root   workspaces/<ws>/   workspaces/<ws>/<repo>/
settings.json           yes          NO  (fixed above)  NO
.claude/agents/         yes          yes (walks up)     NO  (git boundary)
.claude/skills/         yes          yes (walks up)     NO  (git boundary)
```

Claude Code reads project settings from the session's working directory and does not walk
up — that is the row #850 fixed. Agents and skills *do* walk up, and **stop at a git
boundary**. A workspace directory is not one; a clone's root is. So the directory that
already had two thirds of the layer got the third, and the directory that had none of it
kept none.

`enabledPlugins` alone does not close it. The plugin carries charter's own skills and
cannot carry *this* plane's `.claude/agents/` — those are generated from `personas/` by
`persona sync-agents` and are as local to a plane as its personas are. Without them a chat
in a clone loads charter and still cannot delegate to a persona.

## The constraint that shapes the answer

The clone is **not charter's repository**. Files written there are untracked noise in the
operator's own `git status`, in a tree charter does not own — and charter must not edit a
tracked `.gitignore` in somebody else's repo, because that edit travels to their teammates
in a commit.

So the layer is registered in the clone's **`.git/info/exclude`**. That file is
per-checkout, is never committed, and is not itself tracked: charter's files stay invisible
in the clone's status without charter touching anything the operator's repo tracks or
anything their teammates receive. It is the one place a guest may write.

## What that buys, and what it costs

Measured against real `git`, not against charter's model of it: `git status` in the clone is
clean after wiring, `git add -A` stages nothing charter wrote, and the clone's `.gitignore`
is byte-identical.

- **The block is idempotent.** An `ensure` runs on every launch; appending would leave
  `git status` clean while `info/exclude` grew without bound — the failure nobody would
  ever look for. The block is delimited and rewritten in place, and an unterminated one
  (somebody deleted the end marker) is replaced rather than doubled.
- **It lists files, never a directory.** `/.claude/settings.json`, not `.claude/`. Your own
  untracked files under `.claude/` stay visible to you; charter silently making somebody's
  work invisible in their own repo would be a worse failure than the noise this prevents.
- **A path charter did not generate is never touched.** The `.charter-generated` sidecar
  records a hash of what charter wrote, so `charter doctor` can tell `stale` from
  `foreign`. A foreign file is not rewritten, and — the quieter half of the same restraint
  — it is not hidden either. A path you take over stops being hidden on the next wire.
- **`.git/info/exclude` gets a `doctor` row of its own.** Its absence is the whole failure
  this design guards against: every file can be current and your status still full of
  charter. A row nothing reports is a guarantee nothing keeps.

**Every harness's surface is carried, not Claude Code's.** Everything that writes, marks,
hides and removes works from whatever a harness declares, so a harness registered tomorrow
is covered the day it declares one. The list of *what a git boundary cuts off* used to be
two Claude Code paths spelled out in `workspace.py`; it is now `Harness.inherited_paths`,
measured against the installed binaries — `.claude/agents` and `.claude/skills` (2.1.259),
`.opencode/agent` and `opencode.json` (1.18.23), `.codex/skills` (0.147.0). See *a clone
carries every harness's layer*, in this same release.

**CLAUDE.md is deliberately not carried in.** It walks up on the same rule, so the gap is
real for it too — and it is the one file a repo of its own is most likely to have opinions
about. Dropping the plane's project instructions into somebody else's repo, to be read
there as that repo's instructions, is a claim of a different size from mirroring a settings
key. A guest may hide its own files; it does not narrate the host's.

## Removal, and why it is not `rm -rf`

`charter workspace remove` is a `shutil.rmtree`, which takes charter's generated files with
the workspace — and that would be the whole story if the exclude file lived inside it.

For a **linked worktree** it does not. Git treats `info/` as shared between a repo and its
worktrees, so the file git actually reads is the **main repo's**, somewhere else on disk
entirely. Measured: a pattern written to `.git/worktrees/<name>/info/exclude` is read by
nobody. Removal therefore unwires first, and charter resolves the real git dir — through
the `gitdir:` pointer file and then the `commondir` hop — rather than assuming `<root>/.git/`
is a directory. That assumption does not fail loudly: `mkdir -p` cheerfully creates the
path beside the `.git` *file*, the write succeeds, and the operator's status fills up with
charter anyway.

## To adopt

```
charter workspace reinit --all
```

`charter clone` wires new checkouts as it makes them, and says so once per checkout —
charter has just written into a repo you own, and a mechanism whose entire visible
signature is its own absence is one nobody can find when they want it gone.
