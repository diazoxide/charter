//! A git worktree per chat: cut, listed, merged back and removed through the git binary.
//!
//! **Git is the only registry** (ADR 0027). Nothing here writes charter state. Every listing
//! is `git worktree list --porcelain`, so a worktree made by hand with plain git is visible
//! and one removed by hand cannot leave charter reporting a tree that is not there.
//!
//! The one fact charter must remember — the branch a piece was cut from, which
//! `git merge-base` cannot name, because a commit two branches both reach names neither —
//! lives in git too, as `branch.<branch>.charterBase` in the clone's config.

pub mod confine;
pub mod git;
pub mod name;
pub mod porcelain;

use std::path::{Path, PathBuf};

use confine::within_workspace;

/// The directory under a workspace holding every clone's worktrees.
pub const DIR_NAME: &str = ".worktrees";

/// The config key holding the branch a piece was cut from.
fn base_key(branch: &str) -> String {
    format!("branch.{branch}.charterBase")
}

/// What charter refused, with the sentence the operator sees.
#[derive(Debug, thiserror::Error)]
pub enum Refusal {
    #[error("'{0}' does not name a workspace this plane contains")]
    BadWorkspace(String),
    #[error("'{0}' does not name a repo (letters, digits, '.', '_', '-', and not leading)")]
    BadRepo(String),
    #[error(
        "'{0}' does not name a piece (letters, digits, '.', '_', '-', starting with a letter or digit)"
    )]
    BadPiece(String),
    #[error("{0}")]
    BadBranch(#[from] name::BadBranch),
    #[error("'{0}' is not a name git will accept for a branch")]
    BadBranchName(String),
    #[error(transparent)]
    Outside(#[from] confine::Outside),
    #[error(transparent)]
    GitUnavailable(#[from] git::GitUnavailable),
    #[error(
        "this plane relocates its worktree root ([plane] worktrees = {0:?}), and the app does \
         not follow that yet. Use `charter wt add` from the Python charter, or unset [plane] \
         worktrees to keep worktrees in the plane"
    )]
    Relocated(String),
    #[error("'{0}' is not a git repository. Clone it into this workspace first: charter clone {0}")]
    NotARepo(String),
    #[error(
        "branch '{branch}' already exists in {repo}. Reuse it: charter wt add {repo} <piece> \
         --branch {branch}   (or pick another piece name)"
    )]
    BranchTaken { repo: String, branch: String },
    #[error(
        "no worktree '{piece}' for {repo} in workspace '{ws}'. See what exists: charter wt list {repo}"
    )]
    NoSuchPiece {
        ws: String,
        repo: String,
        piece: String,
    },
    #[error(
        "'{piece}' has uncommitted changes — refusing to remove. Commit them, or discard with --force"
    )]
    Dirty { piece: String },
    #[error(
        "could not determine whether '{piece}' holds uncommitted changes — refusing to \
         remove. Check the worktree by hand, or discard with --force"
    )]
    DirtUnknown { piece: String },
    #[error(
        "'{piece}' has {count} commit(s) that exist nowhere else — refusing to remove. Push \
         the branch or merge it, or discard with --force"
    )]
    WouldLoseWork { piece: String, count: u32 },
    #[error("git {what} failed:\n{err}")]
    GitRefused { what: String, err: String },
    #[error("could not {what} {path}: {why}")]
    Io {
        what: &'static str,
        path: String,
        why: String,
    },
}

/// This workspace's worktree root: `workspaces/<ws>/.worktrees`.
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

/// A plane that moves its worktree root elsewhere is refused by name, not followed.
fn relocation_refusal(plane: &Path) -> Result<(), Refusal> {
    if let Some(declared) = std::env::var_os("CHARTER_WORKTREES") {
        return Err(Refusal::Relocated(declared.to_string_lossy().into_owned()));
    }
    let Ok(text) = std::fs::read_to_string(plane.join("charter.toml")) else {
        return Ok(());
    };
    let Ok(doc) = text.parse::<toml::Table>() else {
        return Ok(());
    };
    match doc
        .get("plane")
        .and_then(|p| p.as_table())
        .and_then(|p| p.get("worktrees"))
        .and_then(|w| w.as_str())
    {
        Some(declared) => Err(Refusal::Relocated(declared.to_string())),
        None => Ok(()),
    }
}

/// The clone a piece is cut from, checked to be one.
fn clone_dir(plane: &Path, ws: &str, repo: &str) -> Result<PathBuf, Refusal> {
    let clone = plane.join("workspaces").join(ws).join(repo);
    within_workspace(plane, ws, &clone)?;
    let seen = git::run(&clone, &["rev-parse", "--git-dir"], git::READ)?;
    if !seen.ok() {
        return Err(Refusal::NotARepo(repo.to_string()));
    }
    Ok(clone)
}

/// Whether a tree holds uncommitted changes — in three states, never two.
///
/// A `git status` that failed writes nothing to stdout, and `""` read as "clean" is how every
/// way this call can fail reached the guards that gate a deletion as *nothing to lose*
/// (charter #917). A `bool` cannot carry the difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dirt {
    Clean,
    Dirty,
    Unknown,
}

fn dirt(tree: &Path) -> Dirt {
    match git::run(tree, &["status", "--porcelain"], git::READ) {
        Ok(seen) if seen.ok() => {
            if seen.out.trim().is_empty() {
                Dirt::Clean
            } else {
                Dirt::Dirty
            }
        }
        _ => Dirt::Unknown,
    }
}

/// The base a new piece gets: a branch, or a commit when HEAD is detached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base {
    Branch(String),
    Detached(String),
}

fn head_of(tree: &Path) -> Result<Base, Refusal> {
    let branch = git::run(tree, &["branch", "--show-current"], git::READ)?;
    if branch.ok() && !branch.line().is_empty() {
        return Ok(Base::Branch(branch.line().to_string()));
    }
    let sha = git::run(tree, &["rev-parse", "--short", "HEAD"], git::READ)?;
    Ok(Base::Detached(sha.line().to_string()))
}

fn branch_exists(clone: &Path, branch: &str) -> Result<bool, Refusal> {
    let seen = git::run(
        clone,
        &["rev-parse", "--verify", "--quiet", &name::as_ref(branch)],
        git::READ,
    )?;
    Ok(seen.ok())
}

/// A piece, cut.
#[derive(Debug, Clone)]
pub struct Added {
    pub path: PathBuf,
    pub branch: String,
    pub base: Base,
    pub warnings: Vec<String>,
}

/// What a worktree charter cut does not have, said where the CLI operator will see it.
///
/// Python's `charter wt add` wires the guest layer before it prints `enter: cd <path> &&
/// claude` (charter #951). This one does not, and the verb is spelled the same, so it says so
/// rather than letting the operator infer a guarantee from a command name.
pub fn unwired_warning() -> String {
    "this worktree has no charter layer: no persona agents, no ask/deny rules, no \
     $CHARTER_HARNESS. A harness started here runs without them. Wire it with the Python \
     charter: charter reinit"
        .to_string()
}

pub fn add(
    plane: &Path,
    ws: &str,
    repo: &str,
    piece: &str,
    branch: Option<&str>,
) -> Result<Added, Refusal> {
    relocation_refusal(plane)?;
    let path = path_for(plane, ws, repo, piece)?;
    let asked = branch.unwrap_or(piece);
    name::branch_name_ok(asked)?;

    // The exact path charter will create, and the exact parent it will mkdir — both, and both
    // anchored on the workspace rather than on the plane.
    let parent = path.parent().expect("a piece path has a parent");
    within_workspace(plane, ws, parent)?;
    within_workspace(plane, ws, &path)?;

    let clone = clone_dir(plane, ws, repo)?;

    // `--branch` RESOLVES as well as validating: measured, `@{-1}` prints the previously
    // checked-out branch and exits 0. The name charter uses, records and reports is the one
    // git PRINTED, never the one it was handed.
    let checked = git::run(&clone, &["check-ref-format", "--branch", asked], git::READ)?;
    if !checked.ok() {
        return Err(Refusal::BadBranchName(asked.to_string()));
    }
    let branch = checked.line().to_string();
    name::branch_name_ok(&branch)?;

    if branch_exists(&clone, &branch)? {
        return Err(Refusal::BranchTaken {
            repo: repo.to_string(),
            branch,
        });
    }

    let base = head_of(&clone)?;
    let mut warnings = Vec::new();
    match dirt(&clone) {
        Dirt::Dirty => warnings.push(format!(
            "{repo} has uncommitted changes — they stay in the clone and are NOT carried into \
             the worktree."
        )),
        Dirt::Unknown => warnings.push(format!(
            "{repo}: charter could not read whether the clone is clean."
        )),
        Dirt::Clean => {}
    }

    std::fs::create_dir_all(parent).map_err(|why| Refusal::Io {
        what: "create",
        path: parent.display().to_string(),
        why: why.to_string(),
    })?;
    // Untimed: this checks out a tree, and a killed `worktree add` leaves the registration
    // written and the checkout half-done.
    //
    // No `--` separator: `git worktree add` does not take one, and it does not need one here.
    // The path is absolute (it is built from the plane root) and the piece name cannot start
    // with `-`, so neither argument can be read as an option.
    let created = git::run_untimed(
        &clone,
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            &path.display().to_string(),
        ],
    )?;
    if !created.ok() {
        return Err(Refusal::GitRefused {
            what: "worktree add".into(),
            err: created.err,
        });
    }

    // Recorded on the BRANCH, which `--branch` makes a different string from the piece, and
    // with `--replace-all`, because a key already holding two values cannot be overwritten
    // by a plain `git config` (exit 5).
    if let Base::Branch(b) = &base {
        let _ = git::run(
            &clone,
            &["config", "--replace-all", &base_key(&branch), b],
            git::READ,
        );
    }

    warnings.push(unwired_warning());
    Ok(Added {
        path,
        branch,
        base,
        warnings,
    })
}

/// A piece, removed.
#[derive(Debug, Clone)]
pub struct Removed {
    pub branch: Option<String>,
    pub was_stale: bool,
}

pub fn remove(
    plane: &Path,
    ws: &str,
    repo: &str,
    piece: &str,
    force: bool,
    delete_branch: bool,
) -> Result<Removed, Refusal> {
    relocation_refusal(plane)?;
    let path = path_for(plane, ws, repo, piece)?;
    // Asked NOW, not remembered from when the piece was created: a path that has become a
    // symlink since is a path git would resolve somewhere else.
    within_workspace(plane, ws, &path)?;
    let clone = clone_dir(plane, ws, repo)?;

    let exists = path.symlink_metadata().is_ok();
    if !exists {
        // A registration whose directory is gone has no tree to check and nothing to lose.
        let stale = list(plane, ws, repo)?
            .into_iter()
            .find(|p| p.piece == piece && p.prunable.is_some());
        let Some(stale) = stale else {
            return Err(Refusal::NoSuchPiece {
                ws: ws.to_string(),
                repo: repo.to_string(),
                piece: piece.to_string(),
            });
        };
        let cleared = git::run(
            &clone,
            &[
                "worktree",
                "remove",
                "--",
                &stale.path.display().to_string(),
            ],
            git::READ,
        )?;
        if !cleared.ok() {
            return Err(Refusal::GitRefused {
                what: "worktree remove".into(),
                err: format!(
                    "{}\nSomething exists at that path again, so git will not clear the stale \
                     registration. Clear it yourself: git -C {} worktree prune",
                    cleared.err.trim(),
                    clone.display()
                ),
            });
        }
        return Ok(Removed {
            branch: stale.branch,
            was_stale: true,
        });
    }

    let branch = match head_of(&path) {
        Ok(Base::Branch(b)) => Some(b),
        _ => None,
    };

    if !force {
        match dirt(&path) {
            Dirt::Dirty => {
                return Err(Refusal::Dirty {
                    piece: piece.to_string(),
                });
            }
            Dirt::Unknown => {
                return Err(Refusal::DirtUnknown {
                    piece: piece.to_string(),
                });
            }
            Dirt::Clean => {}
        }
        match unique_commits(&path, branch.as_deref())? {
            None => {
                return Err(Refusal::DirtUnknown {
                    piece: piece.to_string(),
                });
            }
            Some(0) => {}
            Some(count) => {
                return Err(Refusal::WouldLoseWork {
                    piece: piece.to_string(),
                    count,
                });
            }
        }
    }

    let mut argv = vec!["worktree", "remove"];
    if force {
        argv.push("--force");
    }
    argv.push("--");
    let shown = path.display().to_string();
    argv.push(&shown);
    let done = git::run(&clone, &argv, git::READ)?;
    if !done.ok() {
        return Err(Refusal::GitRefused {
            what: "worktree remove".into(),
            err: done.err,
        });
    }

    if let (true, Some(b)) = (delete_branch, &branch) {
        let flag = if force { "-D" } else { "-d" };
        let _ = git::run(&clone, &["branch", flag, "--", b], git::READ);
    }
    Ok(Removed {
        branch,
        was_stale: false,
    })
}

/// Commits reachable from HEAD and from no other ref — the work that would cease to exist.
///
/// `None` when git could not answer, which callers treat as a refusal. "Has no upstream" is
/// the wrong test: it fires on a piece created a minute ago with nothing to lose, and a guard
/// that fires on the harmless common case is how `--force` becomes a habit (charter #104).
fn unique_commits(tree: &Path, branch: Option<&str>) -> Result<Option<u32>, Refusal> {
    let exclude;
    let mut argv = vec!["rev-list", "--count", "HEAD", "--not"];
    if let Some(b) = branch {
        // The pattern is read RELATIVE to refs/heads before `--branches`, so it is the bare
        // name: `refs/heads/<b>` would silently exclude nothing, and the failure mode is the
        // guard reporting zero unique commits for work that is genuinely unique.
        exclude = format!("--exclude={b}");
        argv.push(&exclude);
    }
    argv.push("--branches");
    argv.push("--remotes");
    let seen = git::run(tree, &argv, git::READ)?;
    if !seen.ok() {
        return Ok(None);
    }
    Ok(seen.line().trim().parse::<u32>().ok())
}

/// One piece of this workspace, as git reports it.
#[derive(Debug, Clone)]
pub struct Piece {
    pub piece: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub prunable: Option<String>,
}

/// This workspace's pieces for one repo.
///
/// Filtered to registrations under this workspace's worktree root: git also reports the clone
/// itself, a bare repository's own entry, and every worktree registered anywhere else on the
/// machine, none of which are charter's — and without the filter one of them could be handed
/// to `remove`.
pub fn list(plane: &Path, ws: &str, repo: &str) -> Result<Vec<Piece>, Refusal> {
    let clone = clone_dir(plane, ws, repo)?;
    let listed = git::run(&clone, &["worktree", "list", "--porcelain"], git::READ)?;
    if !listed.ok() {
        return Ok(Vec::new());
    }
    let base = match std::fs::canonicalize(root_of(plane, ws).join(repo)) {
        Ok(base) => base,
        Err(_) => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    for row in porcelain::parse(&listed.out) {
        if row.bare {
            continue;
        }
        // `prunable` means the directory is gone, so it cannot be canonicalised; compare the
        // path git reported as it stands in that case.
        let resolved = std::fs::canonicalize(&row.path).unwrap_or_else(|_| row.path.clone());
        if !resolved.starts_with(&base) {
            continue;
        }
        let Some(piece) = resolved
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
        else {
            continue;
        };
        out.push(Piece {
            piece,
            path: row.path,
            branch: row.branch,
            prunable: row.prunable,
        });
    }
    Ok(out)
}

/// A piece, landed.
#[derive(Debug, Clone)]
pub struct Merged {
    pub was: String,
    pub now: String,
    pub branch: String,
}

pub fn merge(plane: &Path, ws: &str, repo: &str, piece: &str) -> Result<Merged, Refusal> {
    relocation_refusal(plane)?;
    let path = path_for(plane, ws, repo, piece)?;
    within_workspace(plane, ws, &path)?;
    let clone = clone_dir(plane, ws, repo)?;

    let branch = match head_of(&path)? {
        Base::Branch(b) => b,
        Base::Detached(sha) => {
            return Err(Refusal::GitRefused {
                what: "merge".into(),
                err: format!(
                    "'{piece}' is on a detached HEAD at {sha}, so there is no branch to merge. \
                     Give it one: git -C {} switch -c <name>",
                    path.display()
                ),
            });
        }
    };

    // `--get-all`, never `--get`: a key can hold more than one value, `--get` returns the
    // LAST with exit 0 and no warning, and `.git/config` is writable by anything in the
    // clone — so a second line is somebody else choosing what charter merges into.
    let recorded = git::run(
        &clone,
        &["config", "--get-all", &base_key(&branch)],
        git::READ,
    )?;
    let values: Vec<&str> = recorded
        .out
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect();
    let base = match values.as_slice() {
        [] => {
            return Err(Refusal::GitRefused {
                what: "merge".into(),
                err: format!(
                    "the base '{branch}' was cut from was not recorded, so charter does not \
                     know what to merge it into. (A piece cut by the Python charter has no \
                     such record.) Merge it yourself, or record it: git -C {} config \
                     {} <branch>",
                    clone.display(),
                    base_key(&branch)
                ),
            });
        }
        [one] => (*one).to_string(),
        many => {
            return Err(Refusal::GitRefused {
                what: "merge".into(),
                err: format!(
                    "'{}' holds more than one value ({}), so charter cannot say which base \
                     '{branch}' was cut from. Fix it: git -C {} config --replace-all {} \
                     <branch>",
                    base_key(&branch),
                    many.join(", "),
                    clone.display(),
                    base_key(&branch)
                ),
            });
        }
    };

    if dirt(&path) != Dirt::Clean {
        return Err(Refusal::Dirty {
            piece: piece.to_string(),
        });
    }
    if dirt(&clone) != Dirt::Clean {
        return Err(Refusal::Dirty {
            piece: repo.to_string(),
        });
    }
    match head_of(&clone)? {
        Base::Branch(on) if on == base => {}
        other => {
            return Err(Refusal::GitRefused {
                what: "merge".into(),
                err: format!(
                    "'{piece}' was cut from '{base}', and {repo} is on {other:?}. Switch it \
                     back first: git -C {} switch {base}",
                    clone.display()
                ),
            });
        }
    }

    let was = git::run(&clone, &["rev-parse", "HEAD"], git::READ)?
        .line()
        .to_string();
    // Fully qualified: `@` is a legal branch name and `git merge --ff-only @` resolves HEAD,
    // printing "Already up to date" at exit 0 while landing nothing.
    let merged = git::run_untimed(
        &clone,
        &["merge", "--ff-only", "--", &name::as_ref(&branch)],
    )?;
    if !merged.ok() {
        return Err(Refusal::GitRefused {
            what: "merge".into(),
            err: format!(
                "'{branch}' does not fast-forward into {base} — {base} has moved on.\n  Update \
                 the piece where its conflicts belong:\n    git -C {} merge {base}",
                path.display()
            ),
        });
    }
    let now = git::run(&clone, &["rev-parse", "HEAD"], git::READ)?
        .line()
        .to_string();
    Ok(Merged { was, now, branch })
}
