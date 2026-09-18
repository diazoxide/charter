# M1.4 — a worktree per writing chat: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A chat that writes to a repo gets its own git worktree on its own branch, cut and listed by the Rust core, with the branch on its sidebar row and merging back an explicit operator action.

**Architecture:** A new `charter_core::worktree` module drives the **git binary** through a small runner that clears the repository-local git environment and applies a timeout. It records nothing of its own: the worktree registry is git's, and the one fact charter must remember — a piece's base branch — lives in the clone's git config. Every path is gated by `contain::writable` on the exact path created or removed. The CLI and the Tauri layer both call the same core functions, so a refusal reads identically in a terminal and in the app.

**Tech Stack:** Rust (stable, `rust-toolchain.toml`), `clap` derive for the CLI, `tauri-specta` for typed IPC, React + TypeScript for the UI, WebdriverIO for scenario tests, `tests/differential/run.py` with Python charter as the test oracle.

**Spec:** `docs/superpowers/specs/2026-09-18-worktree-per-chat-design.md` (in the charter repo). Its reasoning is `docs/adr/0027-git-is-the-only-registry-for-a-chats-worktree.md`; the merge-back shape is bound by `docs/adr/0020-there-is-no-cross-repo-merge-loop.md`.

## Global Constraints

- **Repo:** `charter-app` (diazoxide/charter-app), checked out at `workspaces/ide/charter-app`. Read its `AGENTS.md` first.
- **Rust on PATH:** `export PATH="/opt/homebrew/opt/rustup/bin:$PATH"`.
- **No Python in any shipped path** (spec decision 14). Python appears only as the differential oracle in `tests/differential/`.
- **The core never depends on Tauri or the UI.** `charter-core` is plain Rust.
- **No `unsafe`** — `unsafe_code = "forbid"` workspace-wide.
- **Nothing parses harness output to decide anything** (spec decision 12). Reading `git worktree list --porcelain` is not that; it is a documented machine format.
- **Clippy runs with `-D warnings`.** Run what CI runs before pushing: `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo deny check`.
- **A test is only trusted once it has been seen to fail for the right reason.** Every task below runs the test and reads the failure before the implementation is written.
- **Green CI is not evidence.** It was green through every round of M1.1, which shipped four containment holes. Each containment guard is hand-mutated (Task 11) to prove a test goes red.
- **Never kill processes by name** (`pkill`/`killall`): other chats run as `claude`. Kill only a PID this work spawned.
- **Three pull requests**, in order: Tasks 1–7 (core and CLI), Tasks 8–10 (Tauri and UI), Tasks 11–13 (mutation audit, scenario, differential). An independent opus reviewer reads each before merge, and when it reports a fix, re-read the pushed head rather than trusting the claim.

## Prerequisite

**M1.2 must be merged before Task 8.** Tasks 1–7 touch none of M1.2's files and may start immediately. M1.2 is in flight on branch `m1.2-wiring`; it adds `charter_core::wiring`, whose `wired_or_refusal(profile, cwd, root)` decides whether charter's plugin is actually in effect at a chat's working directory. Task 9 depends on the chat-start dialogue M1.2 introduces.

## File Structure

| Path | Responsibility |
| --- | --- |
| `crates/charter-core/src/worktree/mod.rs` | The public API: `add`, `list`, `remove`, `merge`, `publish`, and the guards each one runs. |
| `crates/charter-core/src/worktree/name.rs` | Name rules and the slug: `piece_name_ok`, `branch_name_ok`, `slug`. |
| `crates/charter-core/src/worktree/porcelain.rs` | Parsing `git worktree list --porcelain` into rows, and filtering to a workspace's root. |
| `crates/charter-core/src/worktree/git.rs` | The runner: argv, cleared environment, timeout, captured output. Nothing here decides anything. |
| `crates/charter-core/src/contain.rs` | *Modified* — gains `repo_name_ok`. |
| `crates/charter-core/src/lib.rs` | *Modified* — `pub mod worktree;`. |
| `crates/charter-cli/src/main.rs` | *Modified* — the `wt` subcommand. |
| `crates/charter-core/tests/nothing_escapes_a_worktree.rs` | The adversarial containment suite. |
| `crates/charter-core/tests/a_worktree_keeps_the_work_in_it.rs` | The removal guards. |
| `crates/charter-core/tests/merging_back_is_fast_forward_only.rs` | `merge` and `publish` behaviour. |
| `crates/charter-core/tests/support/repo.rs` | Shared test helper: a throwaway git repo with pinned author and dates. |
| `crates/charter-cli/tests/wt_takes_no_all_flag.rs` | ADR 0020's parser refusal. |
| `app/src-tauri/src/worktrees.rs` | Tauri commands, calling the core. |
| `app/src/Worktree.tsx` | The start dialogue's worktree section and the row's branch/labels. |
| `app/e2e/specs/worktree.e2e.ts` | The scenario test. |
| `tests/differential/run.py` | *Modified* — an output-comparison scenario kind. |

`worktree/` is a directory rather than one file because the four concerns have genuinely different reasons to change: name rules change when an alphabet does, the parser when git's format does, the runner when process handling does, and `mod.rs` when a guard does. `session.rs` in this repo is 1802 lines and is the example not to follow.

---

### Task 1: `repo_name_ok`, and the piece and branch name rules

**Files:**
- Modify: `crates/charter-core/src/contain.rs` (add `repo_name_ok` beside `workspace_name_ok`)
- Create: `crates/charter-core/src/worktree/name.rs`
- Create: `crates/charter-core/src/worktree/mod.rs` (module declarations only, this task)
- Modify: `crates/charter-core/src/lib.rs`

**Interfaces:**
- Consumes: `contain::segment_ok` (exists).
- Produces: `contain::repo_name_ok(&str) -> bool`; `worktree::name::piece_name_ok(&str) -> bool`; `worktree::name::branch_name_ok(&str) -> Result<(), BadBranch>`; `worktree::name::slug(&str) -> Option<String>`.

- [ ] **Step 1: Write the failing tests**

In `crates/charter-core/src/contain.rs`, inside the existing `mod tests`:

```rust
#[test]
fn a_repo_name_is_a_contained_name_in_charters_own_alphabet() {
    assert!(repo_name_ok("charter-app"));
    assert!(repo_name_ok("my-repo.v2"));
    for name in ["../escape", "/abs", "a/b", "a\\b", "C:x", "", ".", "..",
                 ".hidden", "-leading", "a b", "é", "alpha\0evil"] {
        assert!(!repo_name_ok(name), "{name:?} must not name a repo");
    }
}
```

In a new `crates/charter-core/src/worktree/name.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_piece_name_is_one_contained_entry_that_is_never_an_argument() {
        assert!(piece_name_ok("fix-login"));
        assert!(piece_name_ok("m1.4"));
        for name in ["", ".", "..", "a/b", "/abs", ".hidden", "-force",
                     "--force", "a b", "alpha\0evil"] {
            assert!(!piece_name_ok(name), "{name:?} must not name a piece");
        }
    }

    #[test]
    fn a_branch_name_that_would_be_an_argument_is_refused_before_git_sees_it() {
        // `git check-ref-format --branch --upload-pack=...` is the injection, so
        // delegating this one rule to git would not prevent it.
        assert_eq!(branch_name_ok("-force"), Err(BadBranch::LooksLikeAFlag));
        assert_eq!(branch_name_ok("--upload-pack=evil"), Err(BadBranch::LooksLikeAFlag));
        assert_eq!(branch_name_ok(""), Err(BadBranch::Empty));
        assert_eq!(branch_name_ok("a\0b"), Err(BadBranch::Nul));
        // Everything else is git's own ref grammar, checked by git, not here.
        assert_eq!(branch_name_ok("feature/spi-schema"), Ok(()));
    }

    #[test]
    fn a_chat_name_becomes_a_piece_name_or_charter_asks_for_another() {
        assert_eq!(slug("Fix login").as_deref(), Some("fix-login"));
        assert_eq!(slug("🔥 hotfix").as_deref(), Some("hotfix"));
        assert_eq!(slug("../../etc/passwd").as_deref(), Some("etc-passwd"));
        assert_eq!(slug("  --force  ").as_deref(), Some("force"));
        assert_eq!(slug(&"a".repeat(80)).as_deref(), Some(&*"a".repeat(40)));
        // Nothing survives: charter asks rather than inventing a name.
        assert_eq!(slug("🔥🔥🔥"), None);
        assert_eq!(slug("..."), None);
        assert_eq!(slug(""), None);
    }

    #[test]
    fn every_slug_it_produces_is_a_name_the_gate_accepts() {
        // The slug is a convenience; the gate is the containment. This pins that the
        // convenience can never hand the gate something it would have to refuse.
        for raw in ["Fix login", "🔥 hotfix", "../../etc/passwd", "  --force  ",
                    "...trailing", "-lead", "a/b/c", &"z".repeat(200)] {
            if let Some(s) = slug(raw) {
                assert!(piece_name_ok(&s), "slug({raw:?}) = {s:?} must be a legal piece");
            }
        }
    }
}
```

- [ ] **Step 2: Run the tests and read the failures**

```bash
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
cd workspaces/ide/charter-app
cargo test -p charter-core contain::tests::a_repo_name 2>&1 | tail -20
```

Expected: the crate does not compile — `repo_name_ok` not found, `worktree` module missing. That is the right failure.

- [ ] **Step 3: Implement**

In `contain.rs`, beside `workspace_name_ok`:

```rust
/// Can `name` name a repo cloned into a workspace?
///
/// The same rule as a workspace's, and separate from it on purpose: a repo name arrives
/// from `inventory/repos.json`, which is written from what a FORGE reported, while a
/// workspace name is one charter minted. They are equal today; the day a forge name needs
/// a wider alphabet, the two must be able to move apart without the other following.
pub fn repo_name_ok(name: &str) -> bool {
    segment_ok(name) && alphabet_ok(name)
}
```

Create `crates/charter-core/src/worktree/name.rs`:

```rust
//! What may name a piece, a branch, and what a chat's name becomes.
//!
//! The slug here is a CONVENIENCE. The containment is `piece_name_ok` and the
//! `contain::writable` call at the point of use — a test pins that the slug can never hand
//! them a name they would have to refuse, so the two can never drift into a gap.

use crate::contain;

/// Can `name` name a piece — one directory under `.worktrees/<repo>/`?
///
/// Charter's alphabet, and no leading `-`: a piece name reaches `git worktree add` as a
/// path, and a path that starts with `-` is an OPTION by the time git parses its argv.
pub fn piece_name_ok(name: &str) -> bool {
    contain::segment_ok(name)
        && !name.starts_with('-')
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
}

/// Why a branch name is not one charter will hand to git.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BadBranch {
    #[error("a branch name may not be empty")]
    Empty,
    #[error(
        "a branch name may not start with '-': git would read it as an option rather than \
         as a name"
    )]
    LooksLikeAFlag,
    #[error("a branch name may not hold a NUL")]
    Nul,
}

/// The rules charter enforces on a branch name — and only those.
///
/// **Deliberately three rules and not a ref grammar.** Git already has `check-ref-format`,
/// it is the definition, and reimplementing it here would drift from the git the operator
/// runs. What git cannot protect us from is the first rule: by the time git sees the name it
/// is argv, so `--upload-pack=…` is an option and not a bad ref. That one is charter's.
pub fn branch_name_ok(name: &str) -> Result<(), BadBranch> {
    if name.is_empty() {
        return Err(BadBranch::Empty);
    }
    if name.starts_with('-') {
        return Err(BadBranch::LooksLikeAFlag);
    }
    if name.contains('\0') {
        return Err(BadBranch::Nul);
    }
    Ok(())
}

/// The longest piece name charter will mint from a chat's name.
const MAX_SLUG: usize = 40;

/// A chat's name as a piece name, or `None` when nothing legal survives.
///
/// `None` is not a failure to handle quietly: charter asks for a name rather than inventing
/// one, because a piece name is a directory and a branch the operator has to live with.
pub fn slug(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len().min(MAX_SLUG));
    let mut pending_dash = false;
    for c in name.chars() {
        let keep = if c.is_ascii_alphanumeric() {
            Some(c.to_ascii_lowercase())
        } else if c == '_' {
            Some('_')
        } else {
            None
        };
        match keep {
            // A separator is remembered rather than written, so a run of them collapses and
            // a trailing one is never emitted at all.
            None => pending_dash = !out.is_empty(),
            Some(c) => {
                if pending_dash && out.len() < MAX_SLUG {
                    out.push('-');
                    pending_dash = false;
                }
                if out.len() == MAX_SLUG {
                    break;
                }
                out.push(c);
            }
        }
    }
    // A leading non-alphanumeric cannot happen (nothing is written before the first one),
    // so the only way to fail the gate is to be empty.
    if out.is_empty() { None } else { Some(out) }
}
```

Create `crates/charter-core/src/worktree/mod.rs`:

```rust
//! A git worktree per chat: cut, listed, merged back and removed through the git binary.
//!
//! **Git is the only registry** (ADR 0027). Nothing here writes charter state. Every listing
//! is `git worktree list --porcelain`, so a worktree made by hand with plain git is visible
//! and one removed by hand cannot leave charter reporting a tree that is not there.

pub mod git;
pub mod name;
pub mod porcelain;
```

In `lib.rs`, add `pub mod worktree;` in alphabetical position (after `workspaces`? no — before it: `worktree` sorts before `workspaces`).

- [ ] **Step 4: Run the tests**

```bash
cargo test -p charter-core contain:: 2>&1 | tail -5
cargo test -p charter-core worktree::name 2>&1 | tail -5
```

Expected: PASS. Then `cargo clippy --all-targets -- -D warnings`.

- [ ] **Step 5: Commit**

```bash
git add crates/charter-core/src/contain.rs crates/charter-core/src/worktree crates/charter-core/src/lib.rs
git commit -m "A repo, a piece and a branch are named before any of them is joined onto a path"
```

---

### Task 2: the git runner

**Files:**
- Create: `crates/charter-core/src/worktree/git.rs`
- Modify: `crates/charter-core/Cargo.toml` (add `wait-timeout`)

**Interfaces:**
- Produces: `worktree::git::Run { code: Option<i32>, out: String, err: String }`; `worktree::git::run(dir: &Path, args: &[&str], timeout: Duration) -> Result<Run, GitUnavailable>`; `worktree::git::LOCAL: Duration` (5 s) and `NETWORK: Duration` (120 s).

- [ ] **Step 1: Write the failing test**

Create `crates/charter-core/src/worktree/git.rs` with tests at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repository_local_git_environment_cannot_redirect_a_call() {
        // Inherited from a hook, GIT_DIR points `git -C <clone>` at ANOTHER repository. The
        // runner clears them, so this call answers about `dir` and not about `elsewhere`.
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q", "."], LOCAL).unwrap();
        run(elsewhere.path(), &["init", "-q", "."], LOCAL).unwrap();
        // SAFETY of the test, not the code: set and removed within one test process.
        unsafe { std::env::set_var("GIT_DIR", elsewhere.path().join(".git")) };

        let seen = run(dir.path(), &["rev-parse", "--absolute-git-dir"], LOCAL).unwrap();

        unsafe { std::env::remove_var("GIT_DIR") };
        let seen = std::fs::canonicalize(seen.out.trim()).unwrap();
        let wanted = std::fs::canonicalize(dir.path().join(".git")).unwrap();
        assert_eq!(seen, wanted, "the call must answer about the directory it was given");
    }

    #[test]
    fn a_git_that_never_returns_is_given_up_on_rather_than_waited_for() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q", "."], LOCAL).unwrap();
        // `git cat-file --batch` reads stdin forever when nothing closes it.
        let answer = run(dir.path(), &["cat-file", "--batch"], Duration::from_millis(300));
        assert!(
            matches!(answer, Ok(Run { code: None, .. })),
            "a timed-out call reports no exit code rather than blocking: {answer:?}"
        );
    }

    #[test]
    fn a_failed_call_carries_its_code_and_its_stderr_without_anyone_reading_the_english() {
        let dir = tempfile::tempdir().unwrap();
        let answer = run(dir.path(), &["rev-parse", "--git-dir"], LOCAL).unwrap();
        assert_eq!(answer.code, Some(128));
        assert!(!answer.err.is_empty());
    }
}
```

- [ ] **Step 2: Run it and read the failure**

```bash
cargo test -p charter-core worktree::git 2>&1 | tail -20
```

Expected: does not compile — `run`, `Run`, `LOCAL` undefined.

- [ ] **Step 3: Implement**

Add to `crates/charter-core/Cargo.toml` under `[dependencies]`:

```toml
# Waiting with a deadline, which `std::process` does not offer. A hung git — a credential
# prompt, an NFS stall — would otherwise block a Tauri command forever.
wait-timeout = "0.2"
```

`crates/charter-core/src/worktree/git.rs`:

```rust
//! Running the git binary. Nothing here decides anything.
//!
//! Charter drives git through its binary rather than a library (ADR 0027), so this is the
//! one place that spawns it, and the three hazards of doing so are handled here and nowhere
//! else: an inherited environment that redirects the call, a prompt that never comes back,
//! and a child that outlives the answer.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

/// The deadline for a call that only touches this machine.
pub const LOCAL: Duration = Duration::from_secs(5);

/// The deadline for a call that crosses a network. `publish` is the only one.
pub const NETWORK: Duration = Duration::from_secs(120);

/// The git variables that would point a call at another repository, or at another index.
///
/// Cleared on EVERY call, not only where it seemed to matter. These are inherited by any
/// process a hook starts, and a hook is exactly where charter's own binary runs.
const REDIRECTING: [&str; 6] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
];

/// What one git call answered. `code` is `None` when the deadline passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub code: Option<i32>,
    pub out: String,
    pub err: String,
}

impl Run {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

/// git itself could not be started.
#[derive(Debug, thiserror::Error)]
#[error("charter could not run git: {0}. Install git, or put it on PATH")]
pub struct GitUnavailable(std::io::Error);

/// Run `git -C <dir> <args>`, with a deadline.
pub fn run(dir: &Path, args: &[&str], timeout: Duration) -> Result<Run, GitUnavailable> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    for var in REDIRECTING {
        cmd.env_remove(var);
    }
    // A credential prompt inside a subprocess the UI cannot show is an infinite hang, and
    // the deadline below would turn it into a failure nobody can explain. Refusing to
    // prompt makes it an error with a cause. Keychain and `gh` helpers are unaffected:
    // they do not use the terminal.
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(GitUnavailable)?;
    let status = child.wait_timeout(timeout).map_err(GitUnavailable)?;
    if status.is_none() {
        // The deadline passed. Kill the child this call spawned — by PID, never by name:
        // other chats on this machine run git too.
        let _ = child.kill();
        let _ = child.wait();
    }
    let out = child.wait_with_output().map_err(GitUnavailable)?;
    Ok(Run {
        code: status.and_then(|s| s.code()),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}
```

> **Note for the implementer:** `wait_with_output` after `wait_timeout` needs the child's
> stdout/stderr still owned by the `Child`. If the borrow checker objects, take the pipes
> with `child.stdout.take()` before waiting and read them on threads — but measure first:
> the simple form compiles on current Rust. Do not silently drop the output to make it build.

- [ ] **Step 4: Run the tests**

```bash
cargo test -p charter-core worktree::git 2>&1 | tail -10
cargo deny check 2>&1 | tail -5
```

Expected: PASS, and `cargo deny` clean for the new dependency.

- [ ] **Step 5: Commit**

```bash
git add crates/charter-core/src/worktree/git.rs crates/charter-core/Cargo.toml Cargo.lock
git commit -m "One place spawns git, and it is the place the three hazards of doing so are handled"
```

---

### Task 3: the porcelain parser and a workspace's listing

**Files:**
- Create: `crates/charter-core/src/worktree/porcelain.rs`
- Modify: `crates/charter-core/src/worktree/mod.rs`

**Interfaces:**
- Consumes: `worktree::git::run`.
- Produces: `porcelain::Row { path: PathBuf, branch: Option<String>, detached: bool, prunable: Option<String>, bare: bool }`; `porcelain::parse(&str) -> Vec<Row>`; `worktree::root_of(plane: &Path, ws: &str) -> PathBuf`; `worktree::path_for(plane: &Path, ws: &str, repo: &str, piece: &str) -> Result<PathBuf, Refusal>`.

- [ ] **Step 1: Write the failing tests**

In `porcelain.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_with_slashes_in_it_survives_being_read() {
        // `rsplit('/')` would leave "spi-schema" and silently lose the rest.
        let rows = parse("worktree /a/b\nHEAD abc\nbranch refs/heads/feature/spi-schema\n\n");
        assert_eq!(rows[0].branch.as_deref(), Some("feature/spi-schema"));
    }

    #[test]
    fn a_worktree_whose_directory_is_gone_is_read_as_prunable_with_its_reason() {
        let rows = parse(
            "worktree /a/gone\nHEAD abc\nbranch refs/heads/x\n\
             prunable gitdir file points to non-existent location\n\n",
        );
        assert_eq!(
            rows[0].prunable.as_deref(),
            Some("gitdir file points to non-existent location")
        );
    }

    #[test]
    fn a_bare_repositorys_own_entry_is_not_read_as_a_tree() {
        // Its `path` is a git directory, not a checkout: treating it as a tree with no
        // `.git` is charter #942, review round 3.
        let rows = parse("worktree /a/bare.git\nbare\n\n");
        assert!(rows[0].bare);
        assert!(rows[0].branch.is_none());
    }

    #[test]
    fn a_detached_head_has_no_branch_and_says_so() {
        let rows = parse("worktree /a/b\nHEAD abc\ndetached\n\n");
        assert!(rows[0].detached);
        assert!(rows[0].branch.is_none());
    }

    #[test]
    fn a_final_record_with_no_trailing_blank_line_is_still_read() {
        let rows = parse("worktree /a/b\nHEAD abc\nbranch refs/heads/x\n");
        assert_eq!(rows.len(), 1);
    }
}
```

- [ ] **Step 2: Run it and read the failure**

```bash
cargo test -p charter-core worktree::porcelain 2>&1 | tail -20
```

Expected: does not compile — `parse` and `Row` undefined.

- [ ] **Step 3: Implement**

```rust
//! `git worktree list --porcelain`, read.
//!
//! This is a documented machine format that git holds stable, which is why charter depends
//! on it — spec decision 12 is about harness output, prose written for a person whose shape
//! nobody promised. Python charter has parsed this since the feature existed
//! (`charter/worktree.py:parse_porcelain`), and the two must agree; the differential in
//! Task 13 is what holds them to it.

use std::path::PathBuf;

/// One worktree, as git reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    /// The branch, with `refs/heads/` removed. `None` when detached or bare.
    pub branch: Option<String>,
    pub detached: bool,
    /// The reason git gives, or an empty string when it gives none. `None` when the tree
    /// is really there.
    ///
    /// **Callers MUST check this before treating `path` as a directory that exists.**
    pub prunable: Option<String>,
    pub bare: bool,
}

pub fn parse(text: &str) -> Vec<Row> {
    let mut out = Vec::new();
    let mut cur: Option<Row> = None;
    for line in text.lines() {
        if line.trim().is_empty() {
            out.extend(cur.take());
            continue;
        }
        let (key, value) = match line.split_once(' ') {
            Some((k, v)) => (k, v),
            None => (line, ""),
        };
        if key == "worktree" {
            out.extend(cur.take());
            cur = Some(Row {
                path: PathBuf::from(value),
                branch: None,
                detached: false,
                prunable: None,
                bare: false,
            });
            continue;
        }
        let Some(row) = cur.as_mut() else { continue };
        match key {
            // Only the prefix is stripped: a branch name legitimately holds slashes, so
            // splitting on the last one would drop everything before it.
            "branch" => row.branch = Some(value.trim_start_matches("refs/heads/").to_string()),
            "detached" => row.detached = true,
            "bare" => row.bare = true,
            "prunable" => row.prunable = Some(value.to_string()),
            _ => {}
        }
    }
    // git's last record is followed by a blank line, but a truncated read is not a reason
    // to lose it.
    out.extend(cur);
    out
}
```

In `mod.rs`, add the path helpers:

```rust
use std::path::{Path, PathBuf};

/// The directory under a workspace holding every clone's worktrees.
pub const DIR_NAME: &str = ".worktrees";

/// This workspace's worktree root: `workspaces/<ws>/.worktrees`.
///
/// In-plane only. A plane that relocates its root (`[plane] worktrees`) is refused by name
/// before this is ever called — see `relocation_refusal`.
pub fn root_of(plane: &Path, ws: &str) -> PathBuf {
    plane.join("workspaces").join(ws).join(DIR_NAME)
}

/// Where a piece lives, with every component checked BEFORE it is joined.
pub fn path_for(plane: &Path, ws: &str, repo: &str, piece: &str) -> Result<PathBuf, Refusal> {
    if !crate::contain::workspace_name_ok(ws) {
        return Err(Refusal::BadWorkspace(ws.to_string()));
    }
    if !crate::contain::repo_name_ok(repo) {
        return Err(Refusal::BadRepo(repo.to_string()));
    }
    if !name::piece_name_ok(piece) {
        return Err(Refusal::BadPiece(piece.to_string()));
    }
    Ok(root_of(plane, ws).join(repo).join(piece))
}
```

Define `Refusal` in `mod.rs` as a `thiserror` enum; the implementer adds variants as later tasks need them, each with the sentence naming its repair.

- [ ] **Step 4: Run the tests**

```bash
cargo test -p charter-core worktree:: 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/charter-core/src/worktree
git commit -m "git's own listing, read the way Python reads it, including the records that are not trees"
```

---

### Task 4: the throwaway-repo test helper

**Files:**
- Create: `crates/charter-core/tests/support/mod.rs`
- Create: `crates/charter-core/tests/support/repo.rs`

**Interfaces:**
- Produces: `support::repo::plane_with_clone(&str) -> Fixture`, where `Fixture` holds the `TempDir`, `plane: PathBuf`, `ws: String`, `repo: String`, `clone: PathBuf`, and a `commit(&self, message: &str)` that makes a commit with pinned author and date.

- [ ] **Step 1: Write the helper and a test that proves it is deterministic**

```rust
//! A plane with one clone in it, built from nothing, for tests that need real git.
//!
//! **Pinned author and dates**, so two runs of the same steps produce the same commit shas.
//! Task 13's differential compares what two implementations did to one repository, and that
//! comparison is only meaningful if the repository itself is reproducible.

use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Fixture {
    _dir: tempfile::TempDir,
    pub plane: PathBuf,
    pub ws: String,
    pub repo: String,
    pub clone: PathBuf,
}

const WHO: [(&str, &str); 6] = [
    ("GIT_AUTHOR_NAME", "charter tests"),
    ("GIT_AUTHOR_EMAIL", "tests@example.invalid"),
    ("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00"),
    ("GIT_COMMITTER_NAME", "charter tests"),
    ("GIT_COMMITTER_EMAIL", "tests@example.invalid"),
    ("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00"),
];

pub fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    for (k, v) in WHO {
        cmd.env(k, v);
    }
    // A developer's own `init.defaultBranch`, hooks or templates must not reach a fixture.
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null");
    cmd.env("GIT_CONFIG_SYSTEM", "/dev/null");
    let out = cmd.output().expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

pub fn plane_with_clone(repo: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let plane = dir.path().to_path_buf();
    std::fs::write(plane.join("charter.toml"), "schema = 1\n").unwrap();
    let ws = "alpha".to_string();
    let clone = plane.join("workspaces").join(&ws).join(repo);
    std::fs::create_dir_all(&clone).unwrap();
    git(&clone, &["init", "-q", "-b", "main", "."]);
    std::fs::write(clone.join("README.md"), "one\n").unwrap();
    git(&clone, &["add", "README.md"]);
    git(&clone, &["commit", "-q", "-m", "one"]);
    Fixture { _dir: dir, plane, ws, repo: repo.to_string(), clone }
}

impl Fixture {
    /// A commit in the clone, or in any tree under it.
    pub fn commit(&self, tree: &Path, message: &str) {
        std::fs::write(tree.join(message), message).unwrap();
        git(tree, &["add", "-A"]);
        git(tree, &["commit", "-q", "-m", message]);
    }
}
```

- [ ] **Step 2: Prove the pinning works**

In `crates/charter-core/tests/nothing_escapes_a_worktree.rs` (created here, filled in Task 5):

```rust
mod support;

#[test]
fn the_fixture_repository_is_reproducible_or_the_differential_means_nothing() {
    let a = support::repo::plane_with_clone("thing");
    let b = support::repo::plane_with_clone("thing");
    let sha = |f: &support::repo::Fixture| {
        String::from_utf8(support::repo::git(&f.clone, &["rev-parse", "HEAD"]).stdout).unwrap()
    };
    assert_eq!(sha(&a), sha(&b), "two builds of the fixture must be the same commit");
}
```

- [ ] **Step 3: Run it**

```bash
cargo test -p charter-core --test nothing_escapes_a_worktree 2>&1 | tail -10
```

Expected: PASS. If it fails, the pinning is incomplete — fix it here, because Task 13 depends on it.

- [ ] **Step 4: Commit**

```bash
git add crates/charter-core/tests/support crates/charter-core/tests/nothing_escapes_a_worktree.rs
git commit -m "A throwaway repo a test builds, reproducible down to the commit sha"
```

---

### Task 5: `add`, and the adversarial suite that comes first

**Files:**
- Modify: `crates/charter-core/src/worktree/mod.rs`
- Modify: `crates/charter-core/tests/nothing_escapes_a_worktree.rs`

**Interfaces:**
- Consumes: `path_for`, `git::run`, `contain::writable`, `name::branch_name_ok`.
- Produces: `worktree::add(plane, ws, repo, piece, branch: Option<&str>) -> Result<Added, Refusal>` where `Added { path: PathBuf, branch: String, base: Base, warnings: Vec<String> }` and `Base { Branch(String), Detached(String) }`.

- [ ] **Step 1: Write the adversarial tests FIRST**

Append to `crates/charter-core/tests/nothing_escapes_a_worktree.rs`:

```rust
use charter_core::worktree;

#[test]
fn a_hostile_repo_or_piece_name_never_reaches_a_path() {
    let f = support::repo::plane_with_clone("thing");
    for bad in ["..", "../..", "a/b", "a\\b", "/etc", "C:x", "", ".", "-force", ".hidden"] {
        assert!(
            worktree::path_for(&f.plane, &f.ws, bad, "piece").is_err(),
            "repo {bad:?} must not build a path"
        );
        assert!(
            worktree::path_for(&f.plane, &f.ws, &f.repo, bad).is_err(),
            "piece {bad:?} must not build a path"
        );
    }
}

#[test]
fn a_worktrees_directory_that_is_a_link_out_of_the_plane_is_refused() {
    // A COMMITTED link travels to every machine that clones the plane, so this is not a
    // local mistake: it redirects every worktree this workspace cuts.
    let f = support::repo::plane_with_clone("thing");
    let outside = tempfile::tempdir().unwrap();
    let root = worktree::root_of(&f.plane, &f.ws);
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(outside.path(), &root).unwrap();

    let refusal = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect_err("a link that leaves the plane is refused");

    assert!(format!("{refusal}").contains("outside"), "{refusal}");
    assert!(
        !outside.path().join("thing").exists(),
        "and nothing was created out there"
    );
}

#[test]
fn a_repo_directory_under_the_root_that_is_a_link_out_is_refused_too() {
    // The gate one level shallower than the write is the shape four of the five M1.1 holes
    // had: checking `.worktrees` and not `.worktrees/<repo>` leaves this open.
    let f = support::repo::plane_with_clone("thing");
    let outside = tempfile::tempdir().unwrap();
    let root = worktree::root_of(&f.plane, &f.ws);
    std::fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join(&f.repo)).unwrap();

    let refusal = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect_err("a link that leaves the plane is refused at any depth");

    assert!(format!("{refusal}").contains("outside"), "{refusal}");
    assert!(!outside.path().join("piece").exists());
}

#[test]
fn a_branch_name_that_is_an_argument_never_reaches_gits_argv() {
    let f = support::repo::plane_with_clone("thing");
    for bad in ["-force", "--upload-pack=/bin/sh", "--exec=evil"] {
        let refusal = worktree::add(&f.plane, &f.ws, &f.repo, "piece", Some(bad))
            .expect_err("a name git would read as an option is refused");
        assert!(format!("{refusal}").contains("option"), "{refusal}");
    }
}

#[test]
fn a_branch_name_git_will_not_accept_is_refused_by_git_and_reported_as_a_name() {
    let f = support::repo::plane_with_clone("thing");
    for bad in ["a..b", "a b", "x/", "a~1", "@{", "refs/heads/../evil"] {
        assert!(
            worktree::add(&f.plane, &f.ws, &f.repo, "piece", Some(bad)).is_err(),
            "{bad:?} must not become a branch"
        );
    }
}

#[test]
fn a_plane_that_relocates_its_worktree_root_is_refused_by_name() {
    let f = support::repo::plane_with_clone("thing");
    std::fs::write(
        f.plane.join("charter.toml"),
        "schema = 1\n[plane]\nworktrees = \"../charter.worktrees\"\n",
    )
    .unwrap();

    let refusal = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect_err("charter does not follow a relocated root yet");

    let said = format!("{refusal}");
    assert!(said.contains("[plane] worktrees"), "{said}");
    assert!(said.contains("unset"), "the sentence names the repair: {said}");
}

#[test]
fn a_directory_that_is_not_a_git_repository_is_refused_with_the_repair() {
    let f = support::repo::plane_with_clone("thing");
    let bare = f.plane.join("workspaces").join(&f.ws).join("notarepo");
    std::fs::create_dir_all(&bare).unwrap();

    let refusal = worktree::add(&f.plane, &f.ws, "notarepo", "piece", None).unwrap_err();

    assert!(format!("{refusal}").contains("not a git repository"), "{refusal}");
}

#[test]
fn a_branch_that_already_exists_is_refused_and_the_reuse_is_named() {
    let f = support::repo::plane_with_clone("thing");
    support::repo::git(&f.clone, &["branch", "taken"]);

    let refusal = worktree::add(&f.plane, &f.ws, &f.repo, "taken", None).unwrap_err();

    assert!(format!("{refusal}").contains("--branch taken"), "{refusal}");
}

#[test]
fn a_dirty_clone_warns_and_still_cuts_the_piece() {
    // The tree a worktree exists to escape is usually dirty. Python warns here and refuses
    // only where dirt can be destroyed; charter-app does the same.
    let f = support::repo::plane_with_clone("thing");
    std::fs::write(f.clone.join("scratch.txt"), "wip\n").unwrap();

    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();

    assert!(added.path.is_dir());
    assert!(
        added.warnings.iter().any(|w| w.contains("uncommitted")),
        "the warning is said: {:?}",
        added.warnings
    );
}

#[test]
fn a_piece_records_the_branch_it_was_cut_from() {
    let f = support::repo::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    assert_eq!(added.base, worktree::Base::Branch("main".into()));

    let recorded = support::repo::git(&f.clone, &["config", "branch.piece.charterBase"]);
    assert_eq!(String::from_utf8(recorded.stdout).unwrap().trim(), "main");
}

#[test]
fn a_piece_cut_from_a_detached_head_records_the_commit_and_says_so() {
    let f = support::repo::plane_with_clone("thing");
    let head = String::from_utf8(
        support::repo::git(&f.clone, &["rev-parse", "--short", "HEAD"]).stdout,
    )
    .unwrap();
    support::repo::git(&f.clone, &["checkout", "-q", "--detach"]);

    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();

    assert_eq!(added.base, worktree::Base::Detached(head.trim().to_string()));
}
```

- [ ] **Step 2: Run them and read every failure**

```bash
cargo test -p charter-core --test nothing_escapes_a_worktree 2>&1 | tail -30
```

Expected: does not compile — `worktree::add` undefined. Once it compiles, each test must fail for its own reason before its guard is written. Do not write all the guards at once and then run: add one guard, watch one test go green.

- [ ] **Step 3: Implement `add`**

In `mod.rs`. The order of the guards is the specification:

```rust
pub fn add(
    plane: &Path,
    ws: &str,
    repo: &str,
    piece: &str,
    branch: Option<&str>,
) -> Result<Added, Refusal> {
    relocation_refusal(plane)?;                      // before any path is built
    let path = path_for(plane, ws, repo, piece)?;    // every name checked as a string
    let branch = branch.unwrap_or(piece);
    name::branch_name_ok(branch)?;                   // charter's one rule: not an option

    // The EXACT path charter will create, and the exact parent it will mkdir. Not the
    // directory above them.
    let parent = path.parent().expect("a piece path has a parent");
    crate::contain::writable(plane, parent)?;
    crate::contain::writable(plane, &path)?;

    let clone = clone_dir(plane, ws, repo)?;         // and that it IS a git repository
    // git validates the rest of the ref grammar; charter does not reimplement it.
    if !git::run(&clone, &["check-ref-format", "--branch", branch], git::LOCAL)?.ok() {
        return Err(Refusal::BadBranchName(branch.to_string()));
    }
    if branch_exists(&clone, branch)? {
        return Err(Refusal::BranchTaken { repo: repo.into(), branch: branch.into() });
    }

    let base = head_of(&clone)?;
    let mut warnings = Vec::new();
    match dirt(&clone)? {
        Dirt::Unknown(why) => warnings.push(format!("{repo}: {why}")),
        Dirt::Dirty => warnings.push(format!(
            "{repo} has uncommitted changes — they stay in the clone and are NOT carried \
             into the worktree."
        )),
        Dirt::Clean => {}
    }

    std::fs::create_dir_all(parent).map_err(|e| Refusal::Io(parent.into(), e))?;
    let created = git::run(
        &clone,
        &["worktree", "add", &path.display().to_string(), "-b", branch],
        git::LOCAL,
    )?;
    if !created.ok() {
        // The check above is not the mutex — git is. A racer that won between them leaves a
        // failure indistinguishable from a broken repo unless charter LOOKS again, and a
        // cause charter read out of git's English is one ADR 0009 forbids it to name.
        if let Some(holder) = holder_of(&clone, &path, branch)? {
            return Err(Refusal::Taken { piece: piece.into(), holder });
        }
        return Err(Refusal::GitRefused { what: "worktree add".into(), err: created.err });
    }

    // Recorded on the BRANCH, which `--branch` makes a different string from the piece.
    // In git, because git is the only registry (ADR 0027).
    if let Base::Branch(b) = &base {
        let _ = git::run(
            &clone,
            &["config", &format!("branch.{branch}.charterBase"), b],
            git::LOCAL,
        );
    }

    warnings.extend(submodule_drift(&path)?);
    Ok(Added { path, branch: branch.to_string(), base, warnings })
}
```

The implementer writes `relocation_refusal`, `clone_dir`, `branch_exists`, `head_of`, `dirt`, `holder_of` and `submodule_drift` as small private functions in the same file, each with a doc comment saying what it reads and why. `dirt` returns three states, never a `bool`: a `git status` that failed writes nothing to stdout, and `bool("")` is false, so every way the call can fail would read as *clean, nothing to lose* — that is charter #917.

- [ ] **Step 4: Run the suite**

```bash
cargo test -p charter-core --test nothing_escapes_a_worktree 2>&1 | tail -20
cargo clippy --all-targets -- -D warnings
```

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/charter-core/src/worktree crates/charter-core/tests
git commit -m "A piece is cut only where charter can say the path lands, and never from a name git would read as an option"
```

---

### Task 6: `list`, `remove`, and the guards that keep work

**Files:**
- Modify: `crates/charter-core/src/worktree/mod.rs`
- Create: `crates/charter-core/tests/a_worktree_keeps_the_work_in_it.rs`

**Interfaces:**
- Produces: `worktree::list(plane, ws, repo) -> Result<Vec<Piece>, Refusal>` where `Piece { piece: String, path: PathBuf, branch: Option<String>, prunable: Option<String>, wired: bool }`; `worktree::remove(plane, ws, repo, piece, force: bool, delete_branch: bool) -> Result<Removed, Refusal>`.

- [ ] **Step 1: Write the failing tests**

```rust
mod support;
use charter_core::worktree;

#[test]
fn a_piece_with_uncommitted_changes_is_not_removed() {
    let f = support::repo::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    std::fs::write(added.path.join("wip.txt"), "unsaved\n").unwrap();

    let refusal =
        worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap_err();

    assert!(format!("{refusal}").contains("uncommitted"), "{refusal}");
    assert!(added.path.is_dir(), "and the tree is still there");
}

#[test]
fn a_piece_holding_commits_that_exist_nowhere_else_is_not_removed() {
    let f = support::repo::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    f.commit(&added.path, "work");

    let refusal =
        worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap_err();

    assert!(format!("{refusal}").contains("nowhere else"), "{refusal}");
    assert!(added.path.is_dir());
}

#[test]
fn a_piece_that_is_only_as_far_as_its_base_has_nothing_to_lose_and_goes() {
    // "Has no upstream" fires on a piece created a minute ago with nothing to lose, and a
    // guard that fires on the harmless common case is how `--force` becomes a habit
    // (charter #104). The rule is commits reachable from NO OTHER REF.
    let f = support::repo::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();

    worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap();

    assert!(!added.path.exists());
}

#[test]
fn a_tree_charter_could_not_read_is_not_a_tree_charter_clears_for_deletion() {
    // Before charter #917 an unreadable tree fell through to `git worktree remove` as clean.
    let f = support::repo::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    // A held index lock makes `git status` fail without saying the tree is clean.
    std::fs::write(added.path.join(".git.broken"), "").unwrap();
    let gitfile = added.path.join(".git");
    std::fs::write(&gitfile, "gitdir: /nonexistent/nowhere\n").unwrap();

    let refusal =
        worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap_err();

    assert!(format!("{refusal}").contains("could not determine"), "{refusal}");
}

#[test]
fn forcing_is_how_the_operator_says_to_discard_it() {
    let f = support::repo::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    f.commit(&added.path, "work");

    worktree::remove(&f.plane, &f.ws, &f.repo, "piece", true, false).unwrap();

    assert!(!added.path.exists());
}

#[test]
fn removing_a_piece_keeps_its_branch() {
    let f = support::repo::plane_with_clone("thing");
    worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();

    worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap();

    let branches = String::from_utf8(
        support::repo::git(&f.clone, &["branch", "--list", "piece"]).stdout,
    )
    .unwrap();
    assert!(branches.contains("piece"), "a branch costs nothing; a deleted one costs a hunt");
}

#[test]
fn a_removal_never_deletes_anything_outside_the_workspace() {
    // The tree is replaced by a link out AFTER it was cut, so the gate is asked about the
    // path at the moment of removal and not about the path as it was created.
    let f = support::repo::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let treasure = outside.path().join("treasure.txt");
    std::fs::write(&treasure, "keep me\n").unwrap();
    std::fs::remove_dir_all(&added.path).unwrap();
    std::os::unix::fs::symlink(outside.path(), &added.path).unwrap();

    let _ = worktree::remove(&f.plane, &f.ws, &f.repo, "piece", true, false);

    assert!(treasure.exists(), "nothing outside the workspace may be removed");
}

#[test]
fn a_registration_whose_directory_is_gone_is_cleared_without_pretending_to_check_a_tree() {
    let f = support::repo::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    std::fs::remove_dir_all(&added.path).unwrap();

    let removed = worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false).unwrap();

    assert!(removed.was_stale);
    let listed = worktree::list(&f.plane, &f.ws, &f.repo).unwrap();
    assert!(listed.iter().all(|p| p.piece != "piece"));
}

#[test]
fn a_worktree_somebody_made_elsewhere_is_not_mistaken_for_one_of_ours() {
    let f = support::repo::plane_with_clone("thing");
    let elsewhere = tempfile::tempdir().unwrap();
    support::repo::git(
        &f.clone,
        &["worktree", "add", &elsewhere.path().join("theirs").display().to_string(), "-b", "theirs"],
    );

    let listed = worktree::list(&f.plane, &f.ws, &f.repo).unwrap();

    assert!(listed.is_empty(), "only what is under this workspace's root: {listed:?}");
}
```

- [ ] **Step 2: Run them and read the failures**

```bash
cargo test -p charter-core --test a_worktree_keeps_the_work_in_it 2>&1 | tail -30
```

- [ ] **Step 3: Implement `list` and `remove`**

`list` runs `git worktree list --porcelain` in the clone, parses with `porcelain::parse`, and keeps only rows whose **resolved** path starts with the resolved `root_of(plane, ws).join(repo)`. `wired` is `path.join(".charter-generated").exists()` — one `exists()` per row, no subprocess.

`remove` in order: `relocation_refusal`; `path_for`; **`contain::writable(plane, &path)` on the exact path, asked now and not remembered from `add`**; if the directory is absent, look for a `prunable` row and take the stale path, which skips every tree check because there is no tree; otherwise, unless `force`, refuse on `Dirt::Unknown`, on `Dirt::Dirty`, and on `unique_commits(&path)? > 0`, where `unique_commits` is `rev-list --count HEAD --exclude=<branch> --branches --remotes` and `None` (git could not answer) is itself a refusal. Capture the branch with `head_of(&path)` **before** the tree disappears, since `--branch` makes it differ from the piece name. Then `git worktree remove [--force] <path>`. Delete the branch only when asked.

- [ ] **Step 4: Run the tests, then the whole crate**

```bash
cargo test -p charter-core 2>&1 | tail -10
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 5: Commit**

```bash
git add crates/charter-core/src/worktree crates/charter-core/tests
git commit -m "Nothing removes a piece that holds work, and nothing removes anything outside the workspace"
```

---

### Task 7: `merge`, `publish`, and the CLI that refuses `--all`

**Files:**
- Modify: `crates/charter-core/src/worktree/mod.rs`
- Create: `crates/charter-core/tests/merging_back_is_fast_forward_only.rs`
- Modify: `crates/charter-cli/src/main.rs`
- Create: `crates/charter-cli/tests/wt_takes_no_all_flag.rs`

**Interfaces:**
- Produces: `worktree::merge(plane, ws, repo, piece) -> Result<Merged, Refusal>`; `worktree::publish(plane, ws, repo, piece) -> Result<Published, Refusal>`; CLI `charter wt {add,list,remove,merge,publish}`.

- [ ] **Step 1: Write the failing tests**

```rust
mod support;
use charter_core::worktree;

#[test]
fn a_piece_that_fast_forwards_lands_in_the_clone() {
    let f = support::repo::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    f.commit(&added.path, "work");

    let merged = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap();

    let head = String::from_utf8(
        support::repo::git(&f.clone, &["rev-parse", "HEAD"]).stdout,
    ).unwrap();
    assert_eq!(head.trim(), merged.now);
    assert_ne!(merged.now, merged.was, "the clone's HEAD moved");
}

#[test]
fn a_piece_that_does_not_fast_forward_is_refused_and_the_repair_is_in_the_worktree() {
    let f = support::repo::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    f.commit(&added.path, "theirs");
    f.commit(&f.clone, "mine");            // main moves on
    let before = String::from_utf8(
        support::repo::git(&f.clone, &["rev-parse", "HEAD"]).stdout,
    ).unwrap();

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();

    let said = format!("{refusal}");
    assert!(said.contains("does not fast-forward"), "{said}");
    assert!(said.contains(&added.path.display().to_string()),
            "the repair runs in the worktree, where the conflicts belong: {said}");
    let after = String::from_utf8(
        support::repo::git(&f.clone, &["rev-parse", "HEAD"]).stdout,
    ).unwrap();
    assert_eq!(before, after, "and the clone's HEAD did not move");
}

#[test]
fn a_clone_that_is_not_on_the_recorded_base_is_refused() {
    let f = support::repo::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    f.commit(&added.path, "work");
    support::repo::git(&f.clone, &["switch", "-q", "-c", "elsewhere"]);

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();

    let said = format!("{refusal}");
    assert!(said.contains("cut from 'main'"), "{said}");
    assert!(said.contains("switch main"), "the repair is named: {said}");
}

#[test]
fn a_dirty_clone_is_refused_because_a_merge_would_write_into_it() {
    let f = support::repo::plane_with_clone("thing");
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    f.commit(&added.path, "work");
    std::fs::write(f.clone.join("README.md"), "changed\n").unwrap();

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();
    assert!(format!("{refusal}").contains("uncommitted"), "{refusal}");
}

#[test]
fn a_piece_cut_from_a_detached_head_says_why_it_cannot_be_merged() {
    let f = support::repo::plane_with_clone("thing");
    support::repo::git(&f.clone, &["checkout", "-q", "--detach"]);
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    f.commit(&added.path, "work");

    let refusal = worktree::merge(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();

    assert!(format!("{refusal}").contains("detached HEAD"), "{refusal}");
}

#[test]
fn publishing_without_an_origin_is_refused_rather_than_guessed_at() {
    let f = support::repo::plane_with_clone("thing");
    worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();

    let refusal = worktree::publish(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();

    assert!(format!("{refusal}").contains("no 'origin'"), "{refusal}");
}

#[test]
fn publishing_pushes_the_branch_and_sets_its_upstream() {
    let f = support::repo::plane_with_clone("thing");
    let remote = tempfile::tempdir().unwrap();
    support::repo::git(remote.path(), &["init", "-q", "--bare", "."]);
    support::repo::git(&f.clone, &["remote", "add", "origin", &remote.path().display().to_string()]);
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    f.commit(&added.path, "work");

    worktree::publish(&f.plane, &f.ws, &f.repo, "piece").unwrap();

    let there = String::from_utf8(
        support::repo::git(remote.path(), &["branch", "--list", "piece"]).stdout,
    ).unwrap();
    assert!(there.contains("piece"));
}

#[test]
fn a_diverged_publish_is_refused_by_git_and_charter_does_not_force_it() {
    let f = support::repo::plane_with_clone("thing");
    let remote = tempfile::tempdir().unwrap();
    support::repo::git(remote.path(), &["init", "-q", "--bare", "."]);
    support::repo::git(&f.clone, &["remote", "add", "origin", &remote.path().display().to_string()]);
    let added = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None).unwrap();
    f.commit(&added.path, "first");
    worktree::publish(&f.plane, &f.ws, &f.repo, "piece").unwrap();
    let pushed = String::from_utf8(
        support::repo::git(remote.path(), &["rev-parse", "piece"]).stdout,
    ).unwrap();
    // Rewrite history in the piece so the push no longer fast-forwards.
    support::repo::git(&added.path, &["reset", "-q", "--hard", "HEAD~1"]);
    f.commit(&added.path, "second");

    let refusal = worktree::publish(&f.plane, &f.ws, &f.repo, "piece").unwrap_err();

    assert!(format!("{refusal}").contains("rejected"), "{refusal}");
    let still = String::from_utf8(
        support::repo::git(remote.path(), &["rev-parse", "piece"]).stdout,
    ).unwrap();
    assert_eq!(pushed, still, "charter never force-pushes; the rejection is the human");
}
```

And `crates/charter-cli/tests/wt_takes_no_all_flag.rs`:

```rust
//! ADR 0020, one command down from where it was written: the flag does not exist, and the
//! parser refuses it. Not a flag defaulting off, and not one behind a confirmation.

#[test]
fn neither_merge_nor_publish_takes_an_all_flag() {
    for verb in ["merge", "publish"] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_charter"))
            .args(["wt", verb, "thing", "--all"])
            .output()
            .expect("the binary runs");
        assert!(!out.status.success(), "`wt {verb} --all` must not be accepted");
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(
            said.contains("unexpected argument") || said.contains("--all"),
            "the parser refuses it by name: {said}"
        );
    }
}
```

- [ ] **Step 2: Run them and read the failures**

```bash
cargo test -p charter-core --test merging_back_is_fast_forward_only 2>&1 | tail -30
cargo test -p charter-cli 2>&1 | tail -20
```

- [ ] **Step 3: Implement**

`merge`: `relocation_refusal`; `path_for`; read the base from `git config branch.<branch>.charterBase` in the clone — absent means charter did not cut this piece and the refusal says so; refuse a `Base::Detached`; refuse `dirt(&path)` other than `Clean`; refuse `dirt(&clone)` other than `Clean`; refuse when `head_of(&clone)` is not the recorded base, naming `git -C <clone> switch <base>`; then `git -C <clone> merge --ff-only <branch>`. On failure, refuse with the worktree-side repair. **Never pushes.**

`publish`: refuse when `git remote get-url origin` fails; then `git -C <worktree> push --set-upstream origin <branch>` with `git::NETWORK`. No `--force`, no `--force-with-lease`, ever. On success, derive a compare URL from the remote when it is a recognisable GitHub or GitLab URL and say nothing when it is not.

CLI in `main.rs`, following the existing `clap` derive style:

```rust
/// Worktrees: one per chat that writes to a repo.
#[command(subcommand, alias = "worktree")]
Wt(WtCommand),

#[derive(Subcommand)]
enum WtCommand {
    /// Cut a piece: a worktree on its own branch.
    Add { repo: String, piece: String, #[arg(long)] branch: Option<String>,
          #[command(flatten)] common: Common },
    /// This workspace's pieces for a repo.
    List { repo: String, #[command(flatten)] common: Common },
    /// Remove a piece. Refuses to throw away work.
    Remove { repo: String, piece: String, #[arg(long)] force: bool,
             #[arg(long)] delete_branch: bool, #[command(flatten)] common: Common },
    /// Land a piece in the clone, fast-forward only. Never pushes.
    Merge { repo: String, piece: String, #[command(flatten)] common: Common },
    /// Push a piece's branch. Never merges.
    Publish { repo: String, piece: String, #[command(flatten)] common: Common },
}
```

`clap` refuses an unknown `--all` on its own; the test pins that it stays refused.

- [ ] **Step 4: Run everything CI runs**

```bash
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo deny check
```

- [ ] **Step 5: Commit and open PR 1**

```bash
git add -A crates
git commit -m "Merging back is one repo at a time, fast-forward only, and publishing never forces"
```

Open the pull request; dispatch an independent opus reviewer over the **pushed head**; when it reports a fix, re-read the pushed head rather than trusting the claim.

---

### Task 8: Tauri commands

**Prerequisite: M1.2 merged.**

**Files:**
- Create: `app/src-tauri/src/worktrees.rs`
- Modify: `app/src-tauri/src/lib.rs` (register in `collect_commands!`)
- Modify: `app/src/bindings.ts` (generated — never edited by hand)

**Interfaces:**
- Consumes: `charter_core::worktree::{add, list, remove, merge, publish}`.
- Produces: commands `worktree_add`, `worktree_list`, `worktree_remove`, `worktree_merge`, `worktree_publish`, each `Result<T, String>`; `#[derive(serde::Serialize, specta::Type)] struct PieceRow { piece, path, branch, wired, stale }`.

- [ ] **Step 1: Write the failing test**

In `app/src-tauri/src/worktrees.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn a_refusal_reaches_the_ui_as_the_same_sentence_the_cli_prints() {
        // One message constant, two call sites. A UI that rewords a refusal is a UI whose
        // users cannot search for the sentence they were shown.
        let plane = tempfile::tempdir().unwrap();
        let core = charter_core::worktree::add(plane.path(), "..", "r", "p", None)
            .unwrap_err()
            .to_string();
        let through = super::worktree_add(plane.path().display().to_string(),
                                          "..".into(), "r".into(), "p".into(), None)
            .unwrap_err();
        assert_eq!(core, through);
    }
}
```

- [ ] **Step 2: Run it**

```bash
cargo test -p charter-app 2>&1 | tail -20
```

Expected: does not compile — the module does not exist.

- [ ] **Step 3: Implement**

Thin wrappers. Each converts `Refusal` with `.to_string()` and does no other work: every decision stays in the core, so the CLI and the app cannot diverge.

- [ ] **Step 4: Regenerate bindings and check they are current**

```bash
cargo test -p charter-app -- --ignored     # writes app/src/bindings.ts
cargo test -p charter-app                  # the test that fails when it is out of date
```

- [ ] **Step 5: Commit**

```bash
git add app/src-tauri/src app/src/bindings.ts
git commit -m "The app asks the core, and repeats its refusal unchanged"
```

---

### Task 9: the start dialogue's worktree section and the sidebar row

**Files:**
- Create: `app/src/Worktree.tsx`
- Create: `app/src/Worktree.test.tsx`
- Modify: `app/src/Sidebar.tsx` (the `ChatRow`)
- Modify: whichever component M1.2 introduced for starting a chat

**Interfaces:**
- Consumes: `worktree_add`, `worktree_list` from `bindings.ts`; the chat-start props M1.2 defines.
- Produces: `<WorktreeChoice repos={...} chatName={...} onChange={...} />`.

- [ ] **Step 1: Write the failing tests**

```tsx
test("the piece name is the chat's name, slugged, and the operator can change it", async () => {
  render(<WorktreeChoice repos={["thing"]} chatName="Fix login" onChange={noop} />);
  expect(screen.getByLabelText("Branch")).toHaveValue("fix-login");
});

test("a workspace with no repo cannot ask for a worktree", () => {
  render(<WorktreeChoice repos={[]} chatName="Fix login" onChange={noop} />);
  expect(screen.getByLabelText("Give this chat its own worktree")).toBeDisabled();
});

test("one repo is chosen already, and several are not", () => {
  const { rerender } = render(<WorktreeChoice repos={["thing"]} chatName="x" onChange={noop} />);
  expect(screen.getByLabelText("Repo")).toHaveValue("thing");
  rerender(<WorktreeChoice repos={["thing", "other"]} chatName="x" onChange={noop} />);
  expect(screen.getByLabelText("Repo")).toHaveValue("");
});

test("a chat on a worktree shows its branch", () => {
  render(<ChatRow chat={{ ...base, branch: "fix-login" }} />);
  expect(screen.getByText("fix-login")).toBeInTheDocument();
});

test("a worktree with no charter layer says so, and a stale one says that", () => {
  render(<ChatRow chat={{ ...base, branch: "b", wired: false, stale: false }} />);
  expect(screen.getByText("unwired")).toBeInTheDocument();
  cleanup();
  render(<ChatRow chat={{ ...base, branch: "b", wired: true, stale: true }} />);
  expect(screen.getByText("stale")).toBeInTheDocument();
});
```

> **Do not query by `role="tab"` or any ARIA role this app uses for its own tabs** — the
> sidebar already uses `role="tab"` for workspaces, and a role query there matches the wrong
> element. Query by test id or by text.

- [ ] **Step 2: Run them**

```bash
cd app && npm test 2>&1 | tail -20
```

- [ ] **Step 3: Implement** the component and the row, wiring the checkbox default to on when `repos.length > 0`.

- [ ] **Step 4: Run the front-end checks**

```bash
cd app && npm run typecheck && npm run lint && npm run format:check && npm test
```

- [ ] **Step 5: Commit**

```bash
git add app/src
git commit -m "A chat says which branch it is on, and whether charter is actually in it"
```

---

### Task 10: relaunch with the worktree gone

**Files:**
- Modify: `crates/charter-core/src/reopen.rs`
- Modify: `app/src-tauri/src/chats.rs` (`put_back`)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_chat_whose_worktree_is_gone_is_dropped_rather_than_reopened_in_the_clone() {
    // A chat that believes it is on its own branch, silently resuming in the SHARED
    // checkout, is the parallel-conflict failure this milestone exists to remove.
    let record = Record { version: VERSION, chats: vec![Chat {
        cwd: Some("/nonexistent/worktree".into()), ..chat("one")
    }]};
    let put = Chats::new().put_back(&record, size());
    assert!(put.is_empty());
}
```

- [ ] **Step 2–4:** run it, implement the drop with a line in the launch report, run again.

- [ ] **Step 5: Commit and open PR 2.** Reviewer over the pushed head.

---

### Task 11: the mutation audit

**Files:** none changed permanently. This task produces a record, not a diff.

Green CI is not evidence — it was green through every round of M1.1. For each guard below, make the mutation **by hand**, run the named test, confirm it fails for the right reason, then revert.

- [ ] Gate the **parent** instead of the exact path in `add` → `a_repo_directory_under_the_root_that_is_a_link_out_is_refused_too` must fail.
- [ ] Drop the `contain::writable` call in `remove` → `a_removal_never_deletes_anything_outside_the_workspace` must fail.
- [ ] Make `dirt` return `Clean` when `git status` fails → `a_tree_charter_could_not_read_is_not_a_tree_charter_clears_for_deletion` must fail.
- [ ] Make `unique_commits` return `0` when git could not answer → the same suite must fail.
- [ ] Remove the leading-`-` rule from `branch_name_ok` → `a_branch_name_that_is_an_argument_never_reaches_gits_argv` must fail.
- [ ] Drop `REDIRECTING` from the runner → `a_repository_local_git_environment_cannot_redirect_a_call` must fail.
- [ ] Change `merge` to `--no-ff` → `a_piece_that_does_not_fast_forward_is_refused…` must fail.
- [ ] Add `--force` to `publish` → `a_diverged_publish_is_refused_by_git_and_charter_does_not_force_it` must fail.
- [ ] Accept `--all` on `merge` → `neither_merge_nor_publish_takes_an_all_flag` must fail.

**A mutation that does not turn a test red is a missing test, not a passing guard.** Write the test before moving on. Record the nine results in the PR description.

> Run `cargo test` with no stale build in the way. A hand-mutation check that reads green
> from a cached artifact is a false negative, and this project has been fooled by exactly
> that before.

---

### Task 12: the scenario test

**Files:**
- Create: `app/e2e/specs/worktree.e2e.ts`

- [ ] **Step 1: Write the spec** — build a throwaway git repo in a temp plane, start the app, start a chat with the worktree box ticked, assert the row shows the branch, run the merge action, assert the clone's HEAD moved.
- [ ] **Step 2–4:** run `npm run e2e`, watch it fail, implement any missing test id, run again.
- [ ] **Step 5: Commit.**

> The app under WebdriverIO is **one process for the whole run**: a spec that quits takes the
> run with it. This spec must not quit the app. Anything needing a relaunch belongs in
> `npm run e2e:relaunch`.

---

### Task 13: the narrowed differential

**Files:**
- Modify: `tests/differential/run.py`
- Create: `tests/differential/scenarios/worktree_add.*` (following the existing scenario layout)

> `tests/differential/run.py` is also modified on M1.2's branch (+117 lines). **Rebase on
> merged main before starting this task** and read that change first; do not resolve a
> conflict by reverting either side.

- [ ] **Step 1: Write the scenario** — both implementations cut a piece in a reproducible repo; the compared artifact is captured stdout: `git worktree list --porcelain` with the plane's path normalised, plus `git rev-parse HEAD` and the branch.
- [ ] **Step 2: Run it and watch it fail** for a real reason (the Rust side not yet registered as a scenario).
- [ ] **Step 3: Implement** the output-comparison scenario kind: where a tree comparison asks "are these trees equal", this one asks "are these captured outputs equal", and says which line differs.
- [ ] **Step 4: Run both**

```bash
cargo build -p charter-cli
tests/differential/run.py                       # every scenario
tests/differential/run.py --scenario worktree_add
```

- [ ] **Step 5: Commit and open PR 3.** Reviewer over the pushed head.

---

## Self-review

**Spec coverage.** Walked the design doc section by section: the layout and relocation refusal (Tasks 3, 5), containment and the slug (Tasks 1, 5), the git runner's three hazards (Task 2), all five verbs (Tasks 5–7), the UI (Tasks 8–9), relaunch (Task 10), the adversarial and mutation work (Tasks 5, 6, 11), the scenario (Task 12), the differential (Task 13). One design-doc statement has no task and needs none: the CLI has no live-chat knowledge, which is an absence rather than a behaviour — the app-side guard is part of Task 8's wrapper and is asserted in Task 12's scenario.

**Placeholders.** None: every step names its files, its command, and the expected failure. Task 6's implementation is described rather than written out in full, and that is the one place a reader may want more — the tests in Step 1 are the specification there, and each guard's reasoning is in the design doc.

**Type consistency.** `Refusal` is one enum across all five verbs; `Base` is used by `add` and read by `merge`; `Piece` as produced by `list` is what `PieceRow` serialises; `Dirt` is three-state everywhere and never a `bool`.
