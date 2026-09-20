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
pub mod listing;
pub mod name;
pub mod porcelain;

use std::path::{Path, PathBuf};

use confine::within_workspace;

/// The directory under a workspace holding every clone's worktrees.
pub const DIR_NAME: &str = ".worktrees";

/// How a detached-HEAD base is written into the record, so it cannot be mistaken for a
/// branch of that name.
pub const DETACHED_PREFIX: &str = "detached:";

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
        "could not determine whether '{piece}' holds commits that exist nowhere else — \
         refusing to remove. Check the worktree by hand, or discard with --force"
    )]
    UniqueUnknown { piece: String },
    #[error("could not read the state of '{what}' ({why}), so charter will not act on it")]
    Unreadable { what: String, why: String },
    #[error(
        "'{branch}' was cut, but charter could not record the branch it came from ({why}), so \
         `charter wt merge` would not know where to land it. The worktree is there; record it \
         by hand: git -C <clone> config --replace-all branch.{branch}.charterBase <base>"
    )]
    BaseNotRecorded { branch: String, why: String },
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
    // Checked here and not only in `path_for`: `list` and `clone_dir` are public entry
    // points that never call it, and `repo` was being joined straight on — so
    // `list(plane, ws, "../beta/repo")` ran git in another workspace's clone.
    if !crate::contain::workspace_name_ok(ws) {
        return Err(Refusal::BadWorkspace(ws.to_string()));
    }
    if !crate::contain::repo_name_ok(repo) {
        return Err(Refusal::BadRepo(repo.to_string()));
    }
    let clone = plane.join("workspaces").join(ws).join(repo);
    let clone = within_workspace(plane, ws, &clone)?;
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

/// [`Dirt`] for one tree, for a caller outside this module.
///
/// `charter workspace remove` asks the same question `wt remove` does, and it has to get the
/// same answer: a workspace that deleted a worktree `wt remove` would have refused over is
/// charter#91 again, one command along.
pub fn dirt_of(tree: &Path) -> Dirt {
    dirt(tree)
}

/// How many commits this tree holds that no other ref reaches, or `None` when git would not
/// say — the second half of the same shared guard as [`dirt_of`].
pub fn unique_commits_of(tree: &Path, branch: Option<&str>) -> Option<u32> {
    // A `Refusal` here means git could not be run at all, which is the same "charter does not
    // know" the `Ok(None)` inside stands for — and the caller's sentence for both is "could
    // not be checked for unique commits".
    unique_commits(tree, branch).ok().flatten()
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
    // A failed `rev-parse` used to become `Detached("")`, which `merge` then reported as
    // "on a detached HEAD at " — an empty sha and the wrong diagnosis for a broken tree.
    if !sha.ok() || sha.line().is_empty() {
        return Err(Refusal::Unreadable {
            what: tree.display().to_string(),
            why: if sha.err.trim().is_empty() {
                "git could not read HEAD".into()
            } else {
                sha.err.trim().to_string()
            },
        });
    }
    Ok(Base::Detached(sha.line().to_string()))
}

fn branch_exists(clone: &Path, branch: &str) -> Result<bool, Refusal> {
    // `show-ref --verify`, not `rev-parse --verify`: the latter still applies git's revision
    // grammar, so `refs/heads/@` RESOLVES and a branch named `@` is reported as existing when
    // it does not. `show-ref --verify` asks only whether the ref is there.
    let seen = git::run(
        clone,
        &["show-ref", "--verify", "--quiet", &name::as_ref(branch)],
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
/// claude` (charter #951), and since M1.x so does this one — so this sentence is now what a
/// cut says only when the wire did **not** land. The verb is spelled the same as Python's, so
/// a tree that is missing the layer says so rather than letting the operator infer a
/// guarantee from a command name.
pub fn unwired_warning(why: &str) -> String {
    format!(
        "this worktree has no charter layer: no persona agents, no ask/deny rules, no \
         $CHARTER_HARNESS. A harness started here runs without them. {why}"
    )
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

    // The confinement checks sit HERE, immediately before the write, and not at the top of
    // the function. Five git subprocesses run above — a window of tens of milliseconds, in
    // which a racer that swapped `.worktrees/<repo>` for a symlink won 8 attempts out of 8.
    // What remains is the structural check-then-open race `contain.rs` documents as accepted;
    // what was here before was a race anyone could win at leisure.
    // One call, not two. `no_link_on_the_way` walks EVERY component below the anchor, so the
    // path check already covers `.worktrees` and `.worktrees/<repo>`; a separate parent check
    // is a call that cannot fail on its own, and a guard no mutation can turn red is a guard
    // nothing is testing.
    let path = within_workspace(plane, ws, &path)?;
    let parent = path
        .parent()
        .expect("a piece path has a parent")
        .to_path_buf();
    std::fs::create_dir_all(&parent).map_err(|why| Refusal::Io {
        what: "create",
        path: parent.display().to_string(),
        why: why.to_string(),
    })?;
    // Untimed: this checks out a tree, and a killed `worktree add` leaves the registration
    // written and the checkout half-done.
    //
    // No `--` separator: `git worktree add` does not take one, and does not need one here.
    // The path is absolute and the piece name cannot start with `-`, so neither argument can
    // be read as an option.
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
    // with `--replace-all`, because a key already holding two values cannot be overwritten by
    // a plain `git config` (exit 5).
    //
    // A detached HEAD records its sha, so `merge` can say what happened at cut time instead
    // of reporting the piece as one charter never cut.
    let recorded = match &base {
        Base::Branch(b) => b.clone(),
        Base::Detached(sha) => format!("{DETACHED_PREFIX}{sha}"),
    };
    let wrote = git::run(
        &clone,
        &["config", "--replace-all", &base_key(&branch), &recorded],
        git::READ,
    )?;
    if !wrote.ok() {
        // Not dropped with `let _`: this is the one fact ADR 0027 says charter must remember,
        // and without it `merge` is permanently unavailable for this branch. The worktree
        // exists, so the message says so and names the repair rather than pretending.
        return Err(Refusal::BaseNotRecorded {
            branch,
            why: wrote.err.trim().to_string(),
        });
    }

    // The layer, written into the tree that was just cut and hidden in the exclude the clone
    // reads. A cut that could not be wired is still a cut — the tree and the branch exist and
    // saying otherwise would be a lie — so this is a warning naming the repair rather than a
    // refusal that would have to undo a checkout.
    let layered = crate::guest::wire(plane, &path);
    if !layered.complete() {
        warnings.push(unwired_warning(&layered.refusal(&path)));
    }
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
    // symlink since is a path git would resolve somewhere else. The CHECKED path is what git
    // is given below — checking one string and passing another is how a link is laundered.
    let path = within_workspace(plane, ws, &path)?;
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
        // The path git REPORTED, re-checked: it comes from `.git/worktrees/<id>/gitdir`,
        // which anything inside the clone can write, and it is the one git will act on.
        let stale_path = within_workspace(plane, ws, &stale.path)?;
        let cleared = git::run(
            &clone,
            &[
                "worktree",
                "remove",
                "--",
                &stale_path.display().to_string(),
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
                // Its own refusal: "could not determine whether this holds uncommitted
                // changes" is not what failed, and telling the operator the wrong thing about
                // a guard that stopped a deletion is worse than saying nothing.
                return Err(Refusal::UniqueUnknown {
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
    /// Whether charter's harness layer is in this tree.
    ///
    /// Since M1.x a worktree charter cuts is wired as it is cut, so `false` is no longer the
    /// ordinary state — it is a tree cut by plain git, or one whose wire did not land. A chat
    /// started in such a tree runs without the plane's ask/deny rules, without its persona's
    /// agents and without `$CHARTER_HARNESS`, so the UI says so on the row and
    /// [`crate::start::ready`] writes the layer or refuses.
    ///
    /// Read from the tree rather than remembered, so a worktree the Python wired reads as
    /// wired and one that becomes wired later stops showing the label with no code to delete.
    pub wired: bool,
}

/// This workspace's pieces for one repo.
///
/// Filtered to registrations under this workspace's worktree root: git also reports the clone
/// itself, a bare repository's own entry, and every worktree registered anywhere else on the
/// machine, none of which are charter's — and without the filter one of them could be handed
/// to `remove`.
pub fn list(plane: &Path, ws: &str, repo: &str) -> Result<Vec<Piece>, Refusal> {
    relocation_refusal(plane)?;
    let clone = clone_dir(plane, ws, repo)?;
    let listed = git::run(&clone, &["worktree", "list", "--porcelain"], git::READ)?;
    if !listed.ok() {
        // An empty list used to be returned for this, so "git did not answer" and "there are
        // no pieces" were the same answer — and `remove`'s stale path turned that into "no
        // such piece".
        return Err(Refusal::Unreadable {
            what: format!("the worktrees of {repo}"),
            why: listed.err.trim().to_string(),
        });
    }
    let root = within_workspace(plane, ws, &root_of(plane, ws).join(repo))?;
    let base = match std::fs::canonicalize(&root) {
        Ok(base) => base,
        // Nothing has been cut for this repo yet: the root does not exist, so there are no
        // pieces. That is a real empty answer, unlike the one above.
        Err(_) => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    for row in porcelain::parse(&listed.out) {
        // No separate `bare` skip: a bare repository's reported path is its git directory,
        // which the filter below already excludes because it is never under this workspace's
        // worktree root. A second check for it could never fail on its own.
        //
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
        let record = crate::guest::marker_at(&resolved);
        let wired = row.prunable.is_none() && !record.is_empty();
        out.push(Piece {
            piece,
            path: row.path,
            branch: row.branch,
            prunable: row.prunable,
            wired,
        });
    }
    // One git call for the whole listing, and only where something claims a layer.
    //
    // A `.charter-generated` git TRACKS is content some cloned repository committed, and says
    // nothing about this tree: without this, any repo carrying one would make every piece of
    // it read as wired, silencing the `unwired` label on a repo the operator merely cloned.
    // But it is a fact about the REPOSITORY, not about each piece — and `worktree_of_chat`
    // runs on every sidebar render, so asking per piece is a subprocess per row. Asked once,
    // here, and only when the answer could change something.
    //
    // A call that failed to run at all is not evidence the layer is charter's either, so only
    // a clear "tracked" takes the label away.
    if out.iter().any(|p| p.wired) && crate::guest::tracked(&clone, crate::guest::MARKER) {
        for piece in &mut out {
            piece.wired = false;
        }
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
    let path = within_workspace(plane, ws, &path)?;
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
        [one] if one.starts_with(DETACHED_PREFIX) => {
            let sha = one.trim_start_matches(DETACHED_PREFIX);
            return Err(Refusal::GitRefused {
                what: "merge".into(),
                err: format!(
                    "'{piece}' was cut from a detached HEAD at {sha}, so there is no branch to \
                     merge it into. Merge it yourself, or record a base: git -C {} config \
                     --replace-all {} <branch>",
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

    // Three states, not two: an unreadable tree is not a dirty one, and reporting a timed-out
    // `git status` as "has uncommitted changes" sends the operator looking for changes that
    // are not there.
    match dirt(&path) {
        Dirt::Clean => {}
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
    }
    match dirt(&clone) {
        Dirt::Clean => {}
        Dirt::Dirty => {
            return Err(Refusal::Dirty {
                piece: repo.to_string(),
            });
        }
        Dirt::Unknown => {
            return Err(Refusal::DirtUnknown {
                piece: repo.to_string(),
            });
        }
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
    if was == now {
        // git reports "Already up to date" at exit 0 when there was nothing to land. Charter
        // reporting that as a successful merge is the same lie the `@` case produced one
        // level down, so it is caught here as well as prevented there.
        return Err(Refusal::GitRefused {
            what: "merge".into(),
            err: format!(
                "'{branch}' had nothing to land in {base}: {repo} is already at {now}. Nothing \
                 was merged"
            ),
        });
    }
    Ok(Merged { was, now, branch })
}

/// Which piece a path is standing in, or `None` for a path outside every worktree.
///
/// **Path arithmetic, and no subprocess.** This is asked for every chat on every sidebar
/// render, and fifty rows times a `git` call is the kind of cost that shows up as the app
/// feeling slow. Python's `worktree.locate` is filesystem-only for the same reason
/// (`charter/worktree.py:37`), and the layout it reads is a contract `docs/plane-format.md`
/// records as stable.
///
/// It answers where the path SITS, not whether charter cut it: naming the piece is enough to
/// ask git the questions that need git.
pub fn locate(plane: &Path, path: &Path) -> Option<Located> {
    let here = std::fs::canonicalize(path).ok()?;
    let workspaces = std::fs::canonicalize(plane.join("workspaces")).ok()?;
    let rest = here.strip_prefix(&workspaces).ok()?;
    let parts: Vec<&str> = rest
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or_default())
        .collect();
    // `<ws>/.worktrees/<repo>/<piece>`, and anything deeper is inside that piece.
    match parts.as_slice() {
        [ws, dir, repo, piece, ..] if *dir == DIR_NAME => Some(Located {
            workspace: (*ws).to_string(),
            repo: (*repo).to_string(),
            piece: (*piece).to_string(),
        }),
        _ => None,
    }
}

/// Where a path sits, when it sits in a piece.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub workspace: String,
    pub repo: String,
    pub piece: String,
}

#[cfg(test)]
mod locate_tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let here = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(here.join("workspaces/alpha/.worktrees/thing/piece/deep/er"))
            .unwrap();
        std::fs::create_dir_all(here.join("workspaces/alpha/thing")).unwrap();
        dir
    }

    #[test]
    fn a_path_in_a_piece_names_its_workspace_repo_and_piece() {
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        let found = locate(&root, &root.join("workspaces/alpha/.worktrees/thing/piece")).unwrap();

        assert_eq!(found.workspace, "alpha");
        assert_eq!(found.repo, "thing");
        assert_eq!(found.piece, "piece");
    }

    #[test]
    fn a_path_deep_inside_a_piece_still_names_it() {
        // A chat's cwd is wherever the operator left it, not the piece's root.
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        let found = locate(
            &root,
            &root.join("workspaces/alpha/.worktrees/thing/piece/deep/er"),
        )
        .unwrap();

        assert_eq!(found.piece, "piece");
    }

    #[test]
    fn a_path_in_the_clone_itself_is_in_no_piece() {
        // The shared checkout is exactly what a piece exists to keep a chat out of, so it
        // must never read as one.
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        assert_eq!(locate(&root, &root.join("workspaces/alpha/thing")), None);
        assert_eq!(locate(&root, &root.join("workspaces/alpha")), None);
        assert_eq!(locate(&root, &root), None);
    }

    #[test]
    fn a_path_outside_the_plane_is_in_no_piece() {
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let outside = tempfile::tempdir().unwrap();

        assert_eq!(locate(&root, outside.path()), None);
    }

    #[test]
    fn a_worktrees_root_with_nothing_under_it_names_nothing() {
        let dir = plane();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        assert_eq!(
            locate(&root, &root.join("workspaces/alpha/.worktrees")),
            None
        );
        assert_eq!(
            locate(&root, &root.join("workspaces/alpha/.worktrees/thing")),
            None
        );
    }
}
