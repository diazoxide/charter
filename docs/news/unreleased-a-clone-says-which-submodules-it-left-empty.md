---
version: unreleased
headline: A clone says which submodules it left empty, and a sync stops calling it up to date
---

*"After `charter clone`, `git submodule status` shows a leading `-` and the submodule
directory is empty, so EVERY one of those targets fails on line one with a bare 'no such
file or directory'. Nothing in the clone output hints that a submodule was skipped."*
(#817)

## What was measured

A throwaway plane, a repo carrying one `dev-scripts` submodule, and the real
`commands._clone_one`:

```
clone status: ok
--- charter output during clone ---
(nothing)
--- ls dest ---            ['.git', '.gitmodules', 'README.md', 'dev-scripts']
--- ls dest/dev-scripts --- []
--- git submodule status --- '-038cbf71a398dadefbaad26e157b70ade2e2f2db dev-scripts\n'
--- what a dev target would hit ---
rc= 127 stderr= sh: dev-scripts/docker-build.sh: No such file or directory
```

`git clone` does not fetch submodules unless asked, and charter never asked. The clone is
a success by every measure charter had, and the tree is missing the scripts every build
target calls.

**`charter sync` was the same silence carrying a stronger claim.** On a clone whose
upstream moved one submodule pointer and added a second submodule:

```
✓ ws/sync_clone: up to date on main
--- git submodule status after sync ---
+038cbf71a398dadefbaad26e157b70ade2e2f2db dev-scripts (heads/main)
-bb34671f3cf1e95aa8ce95d575f561c96065809b extra-tools
--- extra-tools on disk? --- True []
```

A fast-forward moves the *gitlink* and never touches the submodule's own checkout, so the
commonest outcome of a sync is a branch that is up to date beside a submodule that is not.
The branch half of that tick was true, which is exactly why it had to be said differently
rather than followed by a warning — a tick is what an operator scans for and stops reading
at.

## charter says, and does not do

`clone`, `sync` and `status` now name every submodule with nothing checked out, and every
one left behind the commit the branch records, with the one command that fixes both:

```
! super: 1 submodule(s) recorded but not initialised (dev-scripts) — nothing is checked
  out there, so anything that runs from them fails with 'no such file or directory'.
  charter does not fetch them: a submodule URL comes out of the cloned repo's own
  .gitmodules and can point anywhere, and charter's token-only policy does not reach a
  submodule fetch. Yours to run: git -C workspaces/ws/super submodule update --init --recursive
```

The issue asked for either initialising or reporting. **Reporting, and the second reason
is the one that settles it.**

A submodule URL comes out of `.gitmodules` — a file inside the repo charter has just
cloned — and can name any host, recursively. `_https_url` already refuses to hand `git
clone` a string charter did not build (#335, where `ext::sh -c '…'` is a transport that
runs a command); fetching whatever `.gitmodules` names would put that string back one
layer down, where that allowlist cannot see it.

**And charter could not have done it under its own rule anyway.** Golden rule 0 is written
with `git config --local`, and **`git clone` does not read the local config of the
repository it is standing in** — system, global and `-c` only. A submodule fetch *is* a
nested `git clone`. Measured on git 2.50.1, against a superproject whose submodule cannot
be fetched at all without the config under test:

```
LOCAL  protocol.file.allow in the superproject -> submodule init rc = 1   (never read)
-c     on the command line                     -> submodule init rc = 0
GLOBAL protocol.file.allow                     -> submodule init rc = 0
LOCAL  submodule.<name>.url override           -> submodule init rc = 0
```

The last line is the asymmetry: the *parent* resolves the URL from local config, so a
`submodule.<name>.url` override works; the *child* consumes `credential.helper` and
`url.<https>.insteadOf`, and its config search skips the local file that holds them. So a
submodule fetch runs without charter's credential helper and without its SSH→HTTPS
rewrite, whoever starts it — and a charter
that initialised submodules for you would be fetching outside its own credential policy,
quietly, with your token in reach. `docs/git-policy.md` states that boundary now, because
it was true before this release too and nothing said so.

## Two things in the report that measurement contradicted

**The guard is not what denies a submodule init.** #817 reports that `git-policy` says
"all repos are token-only" while a submodule init "is correctly denied by the PreToolUse
guard, which reads as a guard bug". The guard reads the command line and never
`.gitmodules`, and it was asked directly:

```
allowed  git submodule update --init --recursive
allowed  git -C workspaces/ws/super submodule update --init --recursive
allowed  git config submodule.dev-scripts.url https://gitlab.com/eg/dev-scripts.git
DENIED   git config submodule.dev-scripts.url git@gitlab.com:eg/dev-scripts.git
```

Nothing stops the init. What fires is the *manual workaround* — typing the SSH URL while
configuring the override — and there the guard is right, and the HTTPS form works. (This
was confirmed the hard way: the first attempt to build a fixture for this ran into that
denial from a shell command of its own.)

**`gl-refresh` does not share the gap**, so it is deliberately unchanged. It runs no git
command that touches a working tree — it asks each clone's forge for the open change and
last CI and writes the cache the status line reads (`glstate.refresh`).

## What this changes that was not a defect

A submodule left behind is an unstaged change to the gitlink, so `charter status` already
said `dirty` for it and said nothing about why. The row now names the cause — and that
matters more than it reads, because `_sync_one` **skips a dirty tree**: a submodule nobody
was told about silently stops that repo being synced at all.

`git fetch` recurses into submodules on demand, so `charter sync` was already fetching
submodule objects; what it never did was check anything out. Nothing here changes that.

## Verification

Reproduced first, in a throwaway plane with `config.ROOT` and `config.STATE_DIR` asserted
into a scratch directory, never against a real one. 21 new cases: submodules planted by
hand as index gitlinks (a recorded submodule needs no network, no second repository and no
`protocol.file.allow`), and real `git submodule add` fixtures for the two states that
genuinely need a checked-out submodule.

Thirteen hand-mutations, all killed: dropping the `.gitmodules` short-circuit, collapsing
the two drift marks onto one, removing the report from `clone`, from `sync` and from each
half of the `status` row, flattening the remedy path, and trimming `--init --recursive`
off the remedy. One was killed for the wrong reason — splitting the git output on every
space rather than the first survives any fixture whose submodule paths are single words —
so two cases now use a submodule path with a space in it, and that mutation is measured
where it is made.

Nothing to adopt.
