# guard-paths
PR: https://github.com/diazoxide/charter/pull/497
Branch: guard-paths-follow-the-command
Weaker: None. Verified by a differential rather than by reading: both checkouts (`git archive origin/main` into a temp tree, and this worktree) were run over one byte-identical fixture plane — real git repo with a real upstream and two unpushed commits, a `co`/`wipe` alias pair, a fabricated vault file, and a workspace clone — across 7533 (command, cwd) decisions covering git subcommand × option × operand × global-prefix × trailing-separator, the env-prefixed forms, alias forms, `cd` forms, and reader × path. Keys aligned exactly (0 key-only-main, 0 key-only-branch). Result: 0 inputs where origin/main denies and this branch allows; 1040 newly denied, all in the four intended families (`--git-dir`/`--work-tree`/`GIT_DIR`/`GIT_WORK_TREE` routes, `-C` after the subcommand, alias-resolved resets, and the walk guard). No `cat`/`head`/`sed`/`awk`/`less`/`xxd` decision changed in either direction. Harness: scratchpad/guardpaths/{build_fixture.py,decide.py,diff.py}. The one regression that DID exist was found this way and fixed before commit: the walk predicate's guarded-entry names matched `.charter/vaults.json` (the registry) by `startswith`, newly refusing `ag TOKEN .charter/vaults.json` — #443's false positive returning through the other predicate. Fixed with an exact-name/prefix split, and covered by a new test that uses a walker, since the existing registry test uses `cat` and structurally could not have caught it.

## Bypass

Yes — two, both reproduced end to end against git 2.50.1 with real commit loss.

(1) `GIT_WORK_TREE=<any-other-dir> git reset --hard origin/main`, or `git --work-tree=<any-other-dir> reset --hard origin/main`, typed IN the plane root: destroyed both unpushed commits in the plane root, no refusal. `origin/main` denies this. Same with `checkout`/`switch`/`-b`/`--orphan`/aliases — 40 inputs measured.

(2) `git --git-dir <plane>/.git/hooks/.. reset --hard origin/main` from a workspace clone: destroyed both unpushed commits in the plane root, no refusal. Also `git --git-dir=<plane>/.git/refs/.. checkout feature`, `git --git-dir=<plane>/.git/objects/.. switch -c neu`, and `GIT_DIR=<plane>/.git/refs/.. git checkout feature` — all allowed, while the plain `<plane>/.git` spelling of the same path is denied.

## Blocking

### 1

(B) WEAKER THAN origin/main — charter/hooks.py:1437-1442 (`_git_target`). When `--work-tree`/`GIT_WORK_TREE` is present without `--git-dir`/`GIT_DIR`, the cwd is dropped from the subject list, but git still discovers the repository whose refs move from the cwd. Standing in the plane root: `git --work-tree=<elsewhere> checkout feature`, `git --work-tree=.. switch -c neu`, `git --work-tree=<elsewhere> reset --hard origin/main`, `GIT_WORK_TREE=<elsewhere> git reset --hard origin/main` — 40 measured inputs — are DENIED on origin/main and ALLOWED on this branch. Verified with git 2.50.1: `git --work-tree=<elsewhere> reset --hard origin/main` in the plane root destroyed two unpushed commits with no refusal. Fix: `if work_tree is not None: out.append(_at(work_tree))` then `if work_tree is None or git_dir is None: out.append(here)`. Verified: closes all 40, zero change to the other 25119 decisions.

### 2

(A) #477 STILL REPRODUCES — charter/hooks.py:1443-1449 (`_git_target`). `gd.parent` is computed lexically before `resolve()`, so a git dir whose last component is `..` never resolves to the plane root. From a workspace clone, all ALLOWED: `git --git-dir=<plane>/.git/refs/.. checkout feature`, `git --git-dir=<plane>/.git/objects/.. switch -c neu`, `git --git-dir <plane>/.git/hooks/.. reset --hard origin/main`, `GIT_DIR=<plane>/.git/refs/.. git checkout feature` — while `git --git-dir=<plane>/.git checkout feature`, one dot-segment away and the same inode, is denied. Verified with git 2.50.1: the reset form destroyed two unpushed commits in the plane root, guard silent. Fix: `out.extend((gd, gd / ".."))` so the caller's `resolve()` does the collapsing. Verified: closes all four, zero change to the other 25119 decisions.

### 3

(C) NARROW IN THE SAME COMMIT — tests/test_plane_root_checkout_is_two_commands.py:929-931. The pinned row `("DENY", "clone", "git --work-tree {root} checkout feature", "git itself refuses --work-tree without a git dir; charter refuses AHEAD of git, deliberately, rather than depending on git continuing to")` is factually wrong: git 2.50.1 accepts it and uses the discovered git dir (verified — `git --work-tree=<plane> checkout -f main` from a clone ran fine and would have written the clone's branch content into the plane root's working tree). That false belief is exactly what produced the (B) regression above, so the sentence has to be corrected alongside the fix. Also verify-or-narrow after the fix: `_git_target`'s "A list, not a single path, because more than one directory can be the subject", the news entry's "answers with every subject the command has", and docs/hooks.md:86 "The route does not change the verdict either" — all three overreach as the code stands.

