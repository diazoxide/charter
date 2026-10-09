//! **The one diff engine** (RC-2, #704; ADR 0084 §1–§2, R1, R4): every comparison charter
//! shows — a chat's branch against its base, any two refs, what is not committed, a cross-repo
//! change's members — is computed here, and nothing else in charter computes one. The
//! explorer's change markers ([`super::status`]) are this engine's file list without the line
//! counts, and the Review tab and "Show what changed" (RC-4, FM-11) draw what it answers.
//!
//! **A comparison is a base and a head in one repo** (ADR 0084 §1). [`Comparison`] names R1's
//! kinds; [`compare`] resolves one to its two [`Sides`] and answers the file list, and
//! [`compare_file`] answers one file's hunks between the sides the list was computed at, so a
//! large comparison paints its list first and a file's lines when its row is drawn (§2).
//!
//! **Read by gitoxide in the bounded reader's child, starting no program** (V88a, D-88f,
//! D-88h; amending ADR 0084 §2's "runs git as a program", which predates them). Every repo a
//! comparison reads is one an agent can write, so it is opened as [`super::status`] opens a
//! branch — the config read once and cut to the keys a read needs — and read in a child of
//! charter's own binary that is killed past a deadline and a memory cap. No diff driver, text
//! conversion, filter, external diff or pager the repo names can run, because nothing here
//! runs anything: the lines are compared in-process with git's own algorithm (Myers, then
//! git's indent heuristic), and a file's working-tree content is read through a descriptor held
//! from the project's root, one component at a time, following no link.
//!
//! **What git answers is the reference** (RC-2's acceptance): each kind's file list, marks,
//! renames, line counts and hunks are checked against `git diff` on fixtures
//! (`tests/a_comparison_matches_git_diff.rs`).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use super::status::{Change, MARKED, Mark, Opened, open, plain, recorded_base};
use super::{Branch, Refused};
use crate::worktree::name;

/// What to compare (R1). Each kind names its two sides (ADR 0084 §1's table).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Comparison {
    /// (a) The branch checked out in the folder, from where it left its base to its last
    /// commit. Its base is the one charter recorded when it cut the branch, else the repo's
    /// default branch.
    Branch,
    /// (a) with what is not committed yet: from where the branch left its base to the working
    /// tree. A branch with no base recorded is compared from its last commit. The explorer's
    /// markers, and a file's "Show what changed" (FM-11).
    BranchAndUncommitted,
    /// (a) for a branch the repo holds by name, checked out or not: a cross-repo change's
    /// member (e).
    NamedBranch { branch: String },
    /// (b) Any two refs: from where they meet (their merge base) to `to`, or from `from`
    /// itself when `exact`.
    Refs {
        from: String,
        to: String,
        exact: bool,
    },
    /// (c) What is not committed: the folder's last commit to its working tree, untracked
    /// files git does not ignore included, the index never touched.
    Uncommitted,
}

/// The head of a comparison.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Head {
    /// A commit, by its full hex id.
    Commit(String),
    /// The branch folder's files as they are on disk.
    WorkingTree,
}

/// The two sides a comparison was resolved to: what [`compare_file`] is handed back, so a
/// file's hunks are of the same comparison as the list, however the branch moved since.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Sides {
    /// The base commit, by its full hex id; `None` for nothing (a branch with no commit yet).
    pub base: Option<String>,
    pub head: Head,
}

/// How many lines a file's change added and removed, as `git diff --numstat` counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Lines {
    pub added: u64,
    pub removed: u64,
}

/// One file of a comparison.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileChange {
    /// Its path relative to the branch's folder, on the head's side.
    pub path: String,
    pub mark: Mark,
    /// Where a renamed file came from: a name, never followed.
    pub from: Option<String>,
    /// Lines added and removed; `None` when either side is binary or past [`COUNTED`].
    pub lines: Option<Lines>,
    /// Whether git would call either side binary: a NUL in its first 8,000 bytes.
    pub binary: bool,
    /// Whether the change is not committed yet (a working-tree head only).
    pub uncommitted: bool,
}

/// A comparison's file list: the first step (ADR 0084 §2).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Compared {
    /// The sides it was computed at: hand them to [`compare_file`].
    pub sides: Sides,
    /// The base's name — a branch, a ref as given, or a commit — or `None` when there is none.
    pub base: Option<String>,
    /// Sorted by path, the first [`MARKED`].
    pub files: Vec<FileChange>,
    /// How many files past [`MARKED`] are not in `files`.
    pub more: usize,
}

/// One hunk, in `git diff -U0`'s numbers: lines counted from 1, and a side with no lines naming
/// the line the hunk comes after (`0` for the top). The window's `GitHunk` (`hunks.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
}

/// One file of a comparison, the second step (ADR 0084 §2): both sides and git's hunks.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileDiff {
    /// Text: each side (empty where the file is absent) and the hunks between them. Bytes that
    /// are not UTF-8 are drawn as U+FFFD; the hunks are git's, computed on the bytes.
    Text {
        base: String,
        head: String,
        hunks: Vec<Hunk>,
    },
    /// A side git would call binary: drawn as a sentence (R3's previews are RC-6's).
    Binary,
    /// A side past [`super::LARGEST`], by its size.
    TooLarge { bytes: u64 },
}

/// The most bytes of one side a file list counts the lines of: 16 MiB. Past it the file is
/// listed without counts, which keeps one comparison inside the reader's memory cap.
pub const COUNTED: u64 = 16 * 1024 * 1024;

/// How much of a side is looked at to call it binary: git's `buffer_is_binary`.
const SNIFF: usize = 8000;

/// A comparison of `branch`, read by `reader`: its sides and its file list (see the module).
pub fn compare(
    reader: &super::Reader,
    plane: &Path,
    branch: Branch<'_>,
    comparison: &Comparison,
) -> Result<Compared, Refused> {
    match reader.ask(plane, branch, super::Ask::Compare(comparison.clone()))? {
        super::Answer::Compared(compared) => Ok(compared),
        _ => Err(Refused::Read("the reader answered something else".into())),
    }
}

/// One file of a comparison of `branch` at `sides` (from [`compare`]), read by `reader`: its
/// path on the head's side, and `from`, its path on the base's side when it was renamed.
pub fn compare_file(
    reader: &super::Reader,
    plane: &Path,
    branch: Branch<'_>,
    sides: &Sides,
    path: &str,
    from: Option<&str>,
) -> Result<FileDiff, Refused> {
    let ask = super::Ask::CompareFile {
        sides: sides.clone(),
        path: path.to_string(),
        from: from.map(str::to_string),
    };
    match reader.ask(plane, branch, ask)? {
        super::Answer::FileDiff(diff) => Ok(diff),
        _ => Err(Refused::Read("the reader answered something else".into())),
    }
}

/// One file of a branch against the branch's base: what "Show what changed" draws (FM-11).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WhatChanged {
    /// The file as the branch's file list has it: its mark, and where it came from if renamed.
    pub change: FileChange,
    /// The base's name, as [`Compared::base`]; `None` when compared against the last commit.
    pub base: Option<String>,
    /// Its lines and hunks, or the kind that is drawn as a sentence instead.
    pub diff: FileDiff,
}

/// One file of `branch`, by its path relative to the branch's folder, compared against the
/// branch's base with what is not committed yet ([`Comparison::BranchAndUncommitted`], the
/// explorer's markers' own comparison): "Show what changed" (FM-11, #1103 story 21).
///
/// **The path is confined first, as every file command confines one** ([`super::named`]):
/// refused when it is empty, absolute or walks up, names git's folder in any case, or reaches
/// its file through a link; nothing is read for it then. Then the comparison's file list is
/// read, and a file not in it is refused with [`Refused::NotChanged`], never answered with an
/// empty diff. Its hunks are read at the sides the list was read at, so the two agree however
/// the branch moved between them.
///
/// **Only the comparison is read in the reader's child.** Confining the path finds the branch's
/// folder as every file command does, in this process: `git worktree list` for a piece, `git
/// rev-parse` for a repo's own folder (#1189 moves that into the reader).
pub fn what_changed(
    reader: &super::Reader,
    plane: &Path,
    branch: Branch<'_>,
    path: &str,
) -> Result<WhatChanged, Refused> {
    let relative = super::named(plane, branch, path)?;
    let compared = compare(reader, plane, branch, &Comparison::BranchAndUncommitted)?;
    let Some(change) = compared.files.into_iter().find(|one| one.path == relative) else {
        if compared.more > 0 {
            return Err(Refused::Read(format!(
                "purlis compares the first {MARKED} files this branch changed, and '{path}' is \
                 not among them"
            )));
        }
        return Err(Refused::NotChanged(path.to_string()));
    };
    let diff = compare_file(
        reader,
        plane,
        branch,
        &compared.sides,
        &change.path,
        change.from.as_deref(),
    )?;
    Ok(WhatChanged {
        change,
        base: compared.base,
        diff,
    })
}

/// One member of a cross-repo change, compared (e).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MemberCompared {
    pub repo: String,
    pub branch: String,
    /// The member's branch compared as (a), or why it could not be.
    pub compared: Result<Compared, String>,
}

/// A cross-repo change compared (R1 (e), ADR 0084 §1): one comparison per member, as (a) for the
/// member's branch in the workspace's clone of its repo, in the order its `needs` give (ADR
/// 0060): a member after every member it needs, and otherwise in the record's order.
pub fn compare_change(
    reader: &super::Reader,
    plane: &Path,
    ws: &str,
    slug: &str,
) -> Result<Vec<MemberCompared>, Refused> {
    let record = crate::change::store::read(plane, ws, slug)
        .map_err(|e| Refused::Read(format!("purlis could not read the change {slug}: {e}")))?;
    let mut out = Vec::new();
    for member in needs_order(&record.members) {
        let comparison = Comparison::NamedBranch {
            branch: member.branch.clone(),
        };
        let compared = compare(reader, plane, Branch::repo(ws, &member.repo), &comparison)
            .map_err(|refused| refused.to_string());
        out.push(MemberCompared {
            repo: member.repo.clone(),
            branch: member.branch.clone(),
            compared,
        });
    }
    Ok(out)
}

/// The members, each after every member it needs, ties in the record's order. A record that
/// validated has no cycle; one that has is answered in the record's order past the cycle.
fn needs_order(members: &[crate::change::Member]) -> Vec<&crate::change::Member> {
    let mut placed: BTreeSet<&str> = BTreeSet::new();
    let mut out = Vec::with_capacity(members.len());
    while out.len() < members.len() {
        let next = members.iter().find(|m| {
            !placed.contains(m.repo.as_str())
                && m.needs
                    .iter()
                    .all(|n| placed.contains(n.as_str()) || !members.iter().any(|o| &o.repo == n))
        });
        let next = next.or_else(|| members.iter().find(|m| !placed.contains(m.repo.as_str())));
        let Some(next) = next else { break };
        placed.insert(next.repo.as_str());
        out.push(next);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// In the reader's child
// ---------------------------------------------------------------------------------------------

/// A head, resolved.
#[derive(Clone, Copy)]
pub(super) enum At {
    Commit(gix::ObjectId),
    WorkingTree,
}

/// [`compare`], in this process: only the bounded reader's child calls it.
pub(super) fn compare_here(
    plane: &Path,
    branch: Branch<'_>,
    comparison: &Comparison,
) -> Result<Compared, Refused> {
    let unreadable = |why: String| Refused::Unreadable {
        what: format!("the changes of {}", branch.called()),
        why,
    };
    let opened = open(plane, branch)?;
    let (since, head, named) = resolve(plane, branch, &opened, comparison)?;
    let held = match head {
        At::WorkingTree => Some(hold(plane, &opened.base).map_err(unreadable)?),
        At::Commit(_) => None,
    };
    let Changed {
        repo,
        head_tree,
        changes: all,
        unread,
    } = changed(&opened.repo, since, head, held.as_ref()).map_err(unreadable)?;
    let more = all.len().saturating_sub(MARKED);
    let since_tree = tree_of(&repo, since).map_err(unreadable)?;
    let head_tree = repo
        .find_tree(head_tree)
        .map_err(|e| unreadable(e.to_string()))?;
    let mut files = Vec::with_capacity(all.len().min(MARKED));
    for change in all.into_iter().take(MARKED) {
        let old = side_at(
            &repo,
            since_tree.as_ref(),
            change.from.as_deref().unwrap_or(&change.path),
            COUNTED,
        );
        let new = if unread.contains(&change.path) {
            Side::Unreadable
        } else {
            side_at(&repo, Some(&head_tree), &change.path, COUNTED)
        };
        let binary = old.is_binary() || new.is_binary();
        let lines = match (old.bytes(), new.bytes()) {
            (Some(a), Some(b)) if !binary => {
                let (_, added, removed) = line_diff(a, b);
                Some(Lines { added, removed })
            }
            _ => None,
        };
        files.push(FileChange {
            path: change.path,
            mark: change.mark,
            from: change.from,
            lines,
            binary,
            uncommitted: change.uncommitted,
        });
    }
    Ok(Compared {
        sides: Sides {
            base: since.map(|id| id.to_string()),
            head: match head {
                At::Commit(id) => Head::Commit(id.to_string()),
                At::WorkingTree => Head::WorkingTree,
            },
        },
        base: named,
        files,
        more,
    })
}

/// [`compare_file`], in this process: only the bounded reader's child calls it.
pub(super) fn compare_file_here(
    plane: &Path,
    branch: Branch<'_>,
    sides: &Sides,
    path: &str,
    from: Option<&str>,
) -> Result<FileDiff, Refused> {
    let unreadable = |why: String| Refused::Unreadable {
        what: path.to_string(),
        why,
    };
    let Some(path) = plain(path) else {
        return Err(Refused::NotInPiece(path.to_string()));
    };
    let from = match from {
        Some(from) => Some(plain(from).ok_or_else(|| Refused::NotInPiece(from.to_string()))?),
        None => None,
    };
    let opened = open(plane, branch)?;
    let commit = |hex: &str| -> Result<gix::ObjectId, Refused> {
        let full = hex.len() == 40 || hex.len() == 64;
        let id = (full && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| gix::ObjectId::from_hex(hex.as_bytes()).ok())
            .flatten()
            .ok_or_else(|| unreadable(format!("'{hex}' is not a commit")))?;
        opened
            .repo
            .find_commit(id)
            .map_err(|_| unreadable(format!("'{hex}' is not a commit in this repo")))?;
        Ok(id)
    };
    let since = sides.base.as_deref().map(commit).transpose()?;
    let since_tree = tree_of(&opened.repo, since).map_err(unreadable)?;
    let largest = super::LARGEST;
    let old = side_at(
        &opened.repo,
        since_tree.as_ref(),
        from.as_deref().unwrap_or(&path),
        largest,
    );
    let new = match &sides.head {
        Head::Commit(hex) => {
            let tree = tree_of(&opened.repo, Some(commit(hex)?)).map_err(unreadable)?;
            side_at(&opened.repo, tree.as_ref(), &path, largest)
        }
        Head::WorkingTree => {
            // **Only what a review can show** (ADR 0084 §2, as `files::open`): a file git
            // tracks, or one it does not ignore. An ignored `.env` is never read here.
            let shown = super::watch::matters_here(plane, branch, &[path.clone().into()])?;
            if !shown {
                return Err(Refused::NotOffered(path));
            }
            let held = hold(plane, &opened.base).map_err(unreadable)?;
            on_disk(&held, &path, largest).0
        }
    };
    for side in [&old, &new] {
        match side {
            Side::TooLarge(bytes) => return Ok(FileDiff::TooLarge { bytes: *bytes }),
            // Never drawn as an empty file: a side that is there and cannot be read is said.
            Side::Unreadable => {
                return Err(unreadable("it is not a file purlis can read".into()));
            }
            Side::Missing | Side::Bytes(_) => {}
        }
    }
    if old.is_binary() || new.is_binary() {
        return Ok(FileDiff::Binary);
    }
    let (a, b) = (
        old.bytes().unwrap_or_default(),
        new.bytes().unwrap_or_default(),
    );
    let (hunks, _, _) = line_diff(a, b);
    Ok(FileDiff::Text {
        base: String::from_utf8_lossy(a).into_owned(),
        head: String::from_utf8_lossy(b).into_owned(),
        hunks,
    })
}

/// The base and the head `comparison` names in the opened branch, and the base's name.
fn resolve(
    plane: &Path,
    branch: Branch<'_>,
    opened: &Opened,
    comparison: &Comparison,
) -> Result<(Option<gix::ObjectId>, At, Option<String>), Refused> {
    let repo = &opened.repo;
    let what = || format!("the changes of {}", branch.called());
    let refused = |why: String| Refused::Unreadable { what: what(), why };
    let head_commit = || repo.head_commit().ok().map(|commit| commit.id);
    let based = |name: Option<&str>, tip: gix::ObjectId| {
        let recorded = name
            .and_then(|name| opened.recorded.get(name))
            .and_then(|value| recorded_base(repo, value, tip));
        recorded.map(|b| (b.fork, b.named))
    };
    let current = || {
        repo.head_name()
            .ok()
            .flatten()
            .map(|name| name.shorten().to_string())
    };
    match comparison {
        Comparison::Branch => {
            let tip = head_commit().ok_or_else(|| refused("it has no commit yet".into()))?;
            let (fork, named) = based(current().as_deref(), tip)
                .or_else(|| default_base(plane, branch.repo, repo, tip))
                .ok_or_else(|| no_base(branch.called()))?;
            Ok((Some(fork), At::Commit(tip), Some(named)))
        }
        Comparison::BranchAndUncommitted => {
            let tip = head_commit();
            match tip.and_then(|tip| based(current().as_deref(), tip)) {
                Some((fork, named)) => Ok((Some(fork), At::WorkingTree, Some(named))),
                None => Ok((tip, At::WorkingTree, None)),
            }
        }
        Comparison::NamedBranch { branch: wanted } => {
            name::branch_name_ok(wanted).map_err(|e| refused(e.to_string()))?;
            let tip = repo
                .find_reference(name::as_ref(wanted).as_str())
                .ok()
                .and_then(|mut found| found.peel_to_commit().ok())
                .map(|commit| commit.id)
                .ok_or_else(|| refused(format!("it has no branch called '{wanted}'")))?;
            let (fork, named) = based(Some(wanted), tip)
                .or_else(|| default_base(plane, branch.repo, repo, tip))
                .ok_or_else(|| no_base(wanted))?;
            Ok((Some(fork), At::Commit(tip), Some(named)))
        }
        Comparison::Refs { from, to, exact } => {
            let commit = |spec: &str| -> Result<gix::ObjectId, Refused> {
                repo.rev_parse_single(spec)
                    .ok()
                    .and_then(|id| id.object().ok()?.peel_to_commit().ok())
                    .map(|commit| commit.id)
                    .ok_or_else(|| refused(format!("'{spec}' does not name a commit here")))
            };
            let (a, b) = (commit(from)?, commit(to)?);
            let since = if *exact {
                a
            } else {
                repo.merge_base(a, b)
                    .map_err(|_| refused(format!("'{from}' and '{to}' share no history")))?
                    .detach()
            };
            Ok((Some(since), At::Commit(b), Some(from.clone())))
        }
        Comparison::Uncommitted => Ok((head_commit(), At::WorkingTree, None)),
    }
}

fn no_base(called: &str) -> Refused {
    Refused::Read(format!(
        "purlis does not know which branch {called} was cut from, so it has nothing to compare \
         it with. Compare it with a branch you name instead."
    ))
}

/// Where `tip` left the repo's default branch, and that branch's name: the one the project's
/// inventory records for the repo, else the one the clone's `origin/HEAD` names.
fn default_base(
    plane: &Path,
    repo_name: &str,
    repo: &gix::Repository,
    tip: gix::ObjectId,
) -> Option<(gix::ObjectId, String)> {
    let listed = crate::inventory::load(plane, "")
        .ok()
        .map(|doc| crate::inventory::listed(&doc))
        .unwrap_or_default();
    let recorded = crate::inventory::find(&listed, repo_name)
        .and_then(|record| record.get("default_branch")?.as_str())
        .filter(|b| name::branch_name_ok(b).is_ok())
        .map(str::to_owned);
    let candidates: Vec<String> = match recorded {
        Some(b) => vec![
            format!("refs/heads/{b}"),
            format!("refs/remotes/origin/{b}"),
        ],
        None => vec!["refs/remotes/origin/HEAD".to_string()],
    };
    for candidate in candidates {
        let Ok(mut found) = repo.find_reference(candidate.as_str()) else {
            continue;
        };
        let Ok(at) = found.peel_to_commit().map(|commit| commit.id) else {
            continue;
        };
        let named = found.name().shorten().to_string();
        let named = if candidate.ends_with("/HEAD") {
            // What `origin/HEAD` points at, by name, when it is a symbolic ref.
            repo.find_reference(candidate.as_str())
                .ok()
                .and_then(|r| match r.target() {
                    gix::refs::TargetRef::Symbolic(to) => Some(to.shorten().to_string()),
                    gix::refs::TargetRef::Object(_) => None,
                })
                .unwrap_or(named)
        } else {
            named
        };
        // A candidate that shares no history with the branch is not its base: the next one may.
        let Ok(fork) = repo.merge_base(at, tip) else {
            continue;
        };
        return Some((fork.detach(), named));
    }
    None
}

/// What changed between `since` and `head`: the engine's file list without its line counts,
/// which is also the explorer's markers ([`super::status`]), and where to read each side.
pub(super) struct Changed {
    /// The repo both sides are read from. For a working-tree head, the branch's repo with the
    /// working tree's changed files written into memory, never to disk.
    pub(super) repo: gix::Repository,
    /// The head's tree: a commit's, or the working tree's as `git add -A` would stage it.
    pub(super) head_tree: gix::ObjectId,
    /// Sorted by path.
    pub(super) changes: Vec<Change>,
    /// Paths whose working-tree side was not read — past [`COUNTED`], past [`READ_MOST`], or
    /// not a file that reads — and so have no line counts.
    pub(super) unread: BTreeSet<String>,
}

/// The most changed working-tree files read for one comparison: past it, a file is marked by
/// whether it is there and not read, so a branch that generated a hundred thousand files git
/// does not ignore costs the walk git's status makes, not a read of each.
pub const READ_MOST: usize = 20_000;

/// The most bytes of working-tree content one comparison reads and keeps: 128 MiB, an eighth
/// of the reader's memory cap. Every file read is kept (in memory) until the comparison ends,
/// so past this a file is marked by its size, as there and changed, and not read: many large
/// files a branch does not ignore cost the read nothing more, and never its markers.
pub const READ_BUDGET: u64 = 128 * 1024 * 1024;

/// What changed between `since` and `head` (see [`Changed`]), `held` being the branch's folder
/// for a working-tree head.
///
/// **Every kind is one tree against another**, with renames found as `git diff -M` finds them
/// (gitoxide's rewrite tracking, git's 50% similarity and 1,000-file limit). A working-tree
/// head is first made a tree, as `git add -A` would stage it and in memory only: HEAD's tree,
/// with each path git's status reports as changed, untracked or gone read from the folder held
/// open (following no link) and put in or taken out. So a file the branch committed under one
/// name and then moved again in the working tree is one rename, as git shows it.
pub(super) fn changed(
    repo: &gix::Repository,
    since: Option<gix::ObjectId>,
    head: At,
    held: Option<&super::Held>,
) -> Result<Changed, String> {
    let (repo, head_tree, loose, unread) = match head {
        At::Commit(id) => {
            let tree = tree_of(repo, Some(id))?
                .map(|tree| tree.id)
                .ok_or("no tree")?;
            (repo.clone(), tree, BTreeSet::new(), BTreeSet::new())
        }
        At::WorkingTree => working_tree(repo, held.ok_or("the folder is not held")?)?,
    };
    let changes = between_trees(&repo, since, head_tree, &loose)?;
    Ok(Changed {
        repo,
        head_tree,
        changes,
        unread,
    })
}

/// The working tree as a tree, in a copy of `repo` that writes objects to memory: the copy,
/// the tree, every path not committed, and the paths not read.
fn working_tree(
    repo: &gix::Repository,
    held: &super::Held,
) -> Result<
    (
        gix::Repository,
        gix::ObjectId,
        BTreeSet<String>,
        BTreeSet<String>,
    ),
    String,
> {
    let memory = repo.clone().with_object_memory();
    let mut loose: BTreeSet<String> = BTreeSet::new();
    let mut untracked: BTreeSet<String> = BTreeSet::new();
    let mut gone: BTreeSet<String> = BTreeSet::new();
    uncommitted(&memory, &mut loose, &mut untracked, &mut gone)?;
    let start = match memory.head_commit() {
        Ok(commit) => commit.tree_id().map_err(|e| e.to_string())?.detach(),
        Err(_) => gix::ObjectId::empty_tree(memory.object_hash()),
    };
    let mut editor = memory.edit_tree(start).map_err(|e| e.to_string())?;
    let mut unread = BTreeSet::new();
    let mut read = 0usize;
    let mut budget = READ_BUDGET;
    for path in &gone {
        if let Some(plain) = plain(path) {
            editor.remove(plain.as_str()).map_err(|e| e.to_string())?;
        }
    }
    for path in loose.iter().chain(untracked.iter()) {
        let Some(plain) = plain(path) else { continue };
        if gone.contains(path) {
            continue;
        }
        read += 1;
        let (side, kind) = if read > READ_MOST {
            (Side::Unreadable, gix::object::tree::EntryKind::Blob)
        } else {
            on_disk(held, &plain, COUNTED.min(budget))
        };
        if let Side::Bytes(bytes) = &side {
            budget = budget.saturating_sub(bytes.len() as u64);
        }
        let bytes = match side {
            Side::Missing => {
                editor.remove(plain.as_str()).map_err(|e| e.to_string())?;
                continue;
            }
            Side::Bytes(bytes) => bytes,
            Side::TooLarge(_) | Side::Unreadable => {
                // There, and not read: a blob of its own that matches nothing, so it is marked
                // and never taken for a rename.
                unread.insert(plain.clone());
                format!("\0charter: {plain} was not read\0").into_bytes()
            }
        };
        let id = memory
            .write_blob(bytes)
            .map_err(|e| e.to_string())?
            .detach();
        editor
            .upsert(plain.as_str(), kind, id)
            .map_err(|e| e.to_string())?;
    }
    let tree = editor.write().map_err(|e| e.to_string())?.detach();
    loose.extend(untracked);
    loose.extend(gone);
    Ok((memory, tree, loose, unread))
}

/// The difference of `since`'s tree and the tree `head`, file by file, sorted, renames found.
/// A path in `loose` is one not committed yet.
fn between_trees(
    repo: &gix::Repository,
    since: Option<gix::ObjectId>,
    head: gix::ObjectId,
    loose: &BTreeSet<String>,
) -> Result<Vec<Change>, String> {
    let old = tree_of(repo, since)?;
    let new = repo.find_tree(head).map_err(|e| e.to_string())?;
    let options = gix::diff::Options::default().with_rewrites(Some(gix::diff::Rewrites::default()));
    let found = repo
        .diff_tree_to_tree(old.as_ref(), &new, options)
        .map_err(|e| e.to_string())?;
    let mut out: BTreeMap<String, Change> = BTreeMap::new();
    let mut put = |path: String, mark: Mark, from: Option<String>| {
        if let Some(path) = plain(&path) {
            let from = from.and_then(|from| plain(&from));
            let uncommitted =
                loose.contains(&path) || from.as_ref().is_some_and(|from| loose.contains(from));
            out.insert(
                path.clone(),
                Change {
                    path,
                    mark,
                    from,
                    uncommitted,
                },
            );
        }
    };
    use gix::object::tree::diff::ChangeDetached;
    for change in found {
        match change {
            ChangeDetached::Addition {
                location,
                entry_mode,
                ..
            } if is_file(entry_mode) => put(location.to_string(), Mark::Added, None),
            ChangeDetached::Deletion {
                location,
                entry_mode,
                ..
            } if is_file(entry_mode) => put(location.to_string(), Mark::Deleted, None),
            ChangeDetached::Modification {
                location,
                previous_entry_mode,
                entry_mode,
                ..
            } => match (is_file(previous_entry_mode), is_file(entry_mode)) {
                (true, true) => put(location.to_string(), Mark::Changed, None),
                (false, true) => put(location.to_string(), Mark::Added, None),
                (true, false) => put(location.to_string(), Mark::Deleted, None),
                (false, false) => {}
            },
            ChangeDetached::Rewrite {
                source_location,
                location,
                entry_mode,
                copy,
                ..
            } if is_file(entry_mode) => {
                if copy {
                    put(location.to_string(), Mark::Added, None);
                } else {
                    put(
                        location.to_string(),
                        Mark::Renamed,
                        Some(source_location.to_string()),
                    );
                }
            }
            _ => {}
        }
    }
    Ok(out.into_values().collect())
}

/// A blob or a link: what a diff shows. Never a tree, never a submodule's commit.
fn is_file(mode: gix::object::tree::EntryMode) -> bool {
    mode.is_blob_or_symlink()
}

/// What is not committed, as git's status reads it: each path HEAD → index → working tree
/// changed into `loose`, each file git does not track and does not ignore into `untracked`, and
/// each tracked file gone from the working tree into `gone`. Renames are not looked for here:
/// they are found once, between the two trees ([`between_trees`]).
///
/// The untracked files come from a walk of their own ([`untracked_files`]), run beside the
/// status on a second thread as gix's status runs its own walk, so a folder too deep to read
/// costs only itself and not the whole read (#1130).
fn uncommitted(
    repo: &gix::Repository,
    loose: &mut BTreeSet<String>,
    untracked: &mut BTreeSet<String>,
    gone: &mut BTreeSet<String>,
) -> Result<(), String> {
    let walker = repo.clone();
    std::thread::scope(|scope| {
        let walk = scope.spawn(move || untracked_files(&walker));
        let tracked = tracked_changes(repo, loose, gone);
        let walked = walk
            .join()
            .unwrap_or_else(|_| Err("the walk for untracked files stopped".to_string()));
        tracked?;
        untracked.extend(walked?);
        Ok(())
    })
}

/// [`uncommitted`]'s tracked half: git's status with no walk for untracked files.
fn tracked_changes(
    repo: &gix::Repository,
    loose: &mut BTreeSet<String>,
    gone: &mut BTreeSet<String>,
) -> Result<(), String> {
    use gix::status::{Item, index_worktree};
    let items = repo
        .status(gix::progress::Discard)
        .map_err(|e| e.to_string())?
        .untracked_files(gix::status::UntrackedFiles::None)
        .index_worktree_submodules(None)
        .index_worktree_options_mut(|options| options.thread_limit = Some(super::status::threads()))
        .tree_index_track_renames(gix::status::tree_index::TrackRenames::Disabled)
        .into_iter(None)
        .map_err(|e| e.to_string())?;
    for item in items {
        let item = item.map_err(|e| e.to_string())?;
        match item {
            Item::TreeIndex(change) => {
                use gix::diff::index::ChangeRef;
                if let ChangeRef::Rewrite {
                    source_location, ..
                } = &change
                {
                    loose.insert(source_location.to_string());
                }
                loose.insert(change.location().to_string());
            }
            Item::IndexWorktree(index_worktree::Item::Modification {
                rela_path, status, ..
            }) => {
                use gix::status::plumbing::index_as_worktree::{Change as Worktree, EntryStatus};
                match status {
                    EntryStatus::Change(Worktree::Removed) => {
                        gone.insert(rela_path.to_string());
                    }
                    EntryStatus::Change(Worktree::SubmoduleModification(_)) => {}
                    EntryStatus::NeedsUpdate(_) => {}
                    _ => {
                        loose.insert(rela_path.to_string());
                    }
                }
            }
            // No walk was asked for: nothing comes from one.
            Item::IndexWorktree(index_worktree::Item::DirectoryContents { .. })
            | Item::IndexWorktree(index_worktree::Item::Rewrite { .. }) => {}
        }
    }
    Ok(())
}

/// The longest path, in bytes, a call into the file system takes here. Past it an open fails
/// (`ENAMETOOLONG`), and gix's walk reads each folder by its whole path from the working
/// tree's root, so one such folder ends the walk. Windows long paths are taken by the standard
/// library, so there is no limit to keep below there.
#[cfg(target_os = "linux")]
const PATH_MOST: usize = 4096;
#[cfg(all(unix, not(target_os = "linux")))]
const PATH_MOST: usize = 1024;
#[cfg(not(unix))]
const PATH_MOST: usize = usize::MAX;

/// The longest name of one entry in a folder: room for it is kept when a folder is entered.
const NAME_MOST: usize = 255;

/// Whether a folder at `rela` (from the working tree's root, `root_len` bytes long) is too
/// deep to walk: its path, a separator and the longest name in it, with the terminating zero,
/// would not fit in [`PATH_MOST`]. So every folder the walk enters, and every entry it looks at
/// in one, can be named in full.
fn too_deep(root_len: usize, rela: usize, most: usize) -> bool {
    root_len
        .saturating_add(1)
        .saturating_add(rela)
        .saturating_add(1 + NAME_MOST + 1)
        > most
}

/// Each file git does not track and does not ignore, as `git status` lists them, and each
/// untracked folder too deep to walk ([`too_deep`]) as one entry of its own: a mark that says
/// "not read past here", which the working tree takes as a file it could not read. A tracked
/// folder that deep is left to the status of the files git tracks in it.
fn untracked_files(repo: &gix::Repository) -> Result<BTreeSet<String>, String> {
    use gix::dir::entry::{Kind, Status};
    use gix::dir::walk::{Action, Delegate, EmissionMode, ForDeletionMode};
    use gix::dir::{EntryRef, entry};

    struct Untracked {
        root_len: usize,
        found: BTreeSet<String>,
        too_deep: BTreeSet<String>,
    }
    impl Delegate for Untracked {
        fn emit(&mut self, entry: EntryRef<'_>, _: Option<entry::Status>) -> Action {
            if entry.status == Status::Untracked {
                let path = entry.rela_path.to_string();
                if matches!(entry.disk_kind, Some(Kind::File | Kind::Symlink))
                    || self.too_deep.contains(&path)
                {
                    self.found.insert(path);
                }
            }
            Action::Continue(())
        }

        fn can_recurse(
            &mut self,
            entry: EntryRef<'_>,
            for_deletion: Option<ForDeletionMode>,
            worktree_root_is_repository: bool,
        ) -> bool {
            if too_deep(self.root_len, entry.rela_path.len(), PATH_MOST) {
                self.too_deep.insert(entry.rela_path.to_string());
                return false;
            }
            entry.status.can_recurse(
                entry.disk_kind,
                entry.pathspec_match,
                for_deletion,
                worktree_root_is_repository,
            )
        }
    }

    let Some(root) = repo.workdir() else {
        return Ok(BTreeSet::new());
    };
    let index = repo.index_or_empty().map_err(|e| e.to_string())?;
    let options = repo
        .dirwalk_options()
        .map_err(|e| e.to_string())?
        .emit_untracked(EmissionMode::Matching);
    let mut delegate = Untracked {
        root_len: root.as_os_str().len(),
        found: BTreeSet::new(),
        too_deep: BTreeSet::new(),
    };
    repo.dirwalk(
        &index,
        None::<&str>,
        &std::sync::atomic::AtomicBool::new(false),
        options,
        &mut delegate,
    )
    .map_err(|e| e.to_string())?;
    Ok(delegate.found)
}

/// The tree of `commit`, or `None` for none.
fn tree_of<'r>(
    repo: &'r gix::Repository,
    commit: Option<gix::ObjectId>,
) -> Result<Option<gix::Tree<'r>>, String> {
    commit
        .map(|id| {
            repo.find_commit(id)
                .and_then(|commit| commit.tree())
                .map_err(|e| e.to_string())
        })
        .transpose()
}

/// One side of one file.
enum Side {
    /// Not on this side.
    Missing,
    /// Its bytes.
    Bytes(Vec<u8>),
    /// Past the size it is read to, by its size.
    TooLarge(u64),
    /// Not a file that can be read as one (a FIFO, a socket, an unreadable one).
    Unreadable,
}

impl Side {
    fn bytes(&self) -> Option<&[u8]> {
        match self {
            Side::Missing => Some(&[]),
            Side::Bytes(bytes) => Some(bytes),
            Side::TooLarge(_) | Side::Unreadable => None,
        }
    }

    fn is_binary(&self) -> bool {
        match self {
            Side::Bytes(bytes) => bytes[..bytes.len().min(SNIFF)].contains(&0),
            _ => false,
        }
    }
}

/// `path` in `tree`: a blob's bytes, or a link's target as git stores it.
fn side_at(repo: &gix::Repository, tree: Option<&gix::Tree<'_>>, path: &str, most: u64) -> Side {
    let Some(entry) = tree.and_then(|tree| tree.lookup_entry_by_path(path).ok().flatten()) else {
        return Side::Missing;
    };
    if !entry.mode().is_blob_or_symlink() {
        return Side::Missing;
    }
    let id = entry.object_id();
    match repo.find_header(id) {
        Ok(header) if header.size() > most => return Side::TooLarge(header.size()),
        Ok(_) => {}
        Err(_) => return Side::Unreadable,
    }
    match repo.find_blob(id) {
        Ok(blob) => Side::Bytes(blob.data.clone()),
        Err(_) => Side::Unreadable,
    }
}

/// The branch's folder, held open from the project's root following no link (see
/// [`super::hold_folder`]).
pub(super) fn hold(plane: &Path, base: &Path) -> Result<super::Held, String> {
    let root = std::fs::canonicalize(plane).map_err(|e| e.to_string())?;
    super::hold_folder(&root, base).map_err(|e| e.to_string())
}

/// `path` as it is on disk in the held folder, and the kind of entry git would stage it as: a
/// regular file's bytes (executable or not), or a link's target, as git compares them. Opened
/// one component at a time following no link, non-blocking, so a folder swapped for a link is
/// not followed and a FIFO is not waited on.
fn on_disk(held: &super::Held, path: &str, most: u64) -> (Side, gix::object::tree::EntryKind) {
    use gix::object::tree::EntryKind;
    let relative = Path::new(path);
    if let Some(target) = super::link_inside(held, relative) {
        return (Side::Bytes(target), EntryKind::Link);
    }
    let mut file = match super::open_inside(held, relative) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (Side::Missing, EntryKind::Blob);
        }
        Err(_) => return (Side::Unreadable, EntryKind::Blob),
    };
    let Ok(meta) = file.metadata() else {
        return (Side::Unreadable, EntryKind::Blob);
    };
    let kind = if executable(&meta) {
        EntryKind::BlobExecutable
    } else {
        EntryKind::Blob
    };
    if !meta.is_file() {
        return (Side::Unreadable, kind);
    }
    if meta.len() > most {
        return (Side::TooLarge(meta.len()), kind);
    }
    let mut read = Vec::with_capacity(meta.len() as usize);
    if file.by_ref().take(most + 1).read_to_end(&mut read).is_err() {
        return (Side::Unreadable, kind);
    }
    if read.len() as u64 > most {
        return (Side::TooLarge(read.len() as u64), kind);
    }
    (Side::Bytes(read), kind)
}

#[cfg(unix)]
fn executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    meta.permissions().mode() & 0o100 != 0
}

#[cfg(not(unix))]
fn executable(_: &std::fs::Metadata) -> bool {
    false
}

/// git's line diff of `old` and `new`: Myers, then git's indent heuristic for where an
/// ambiguous hunk sits (git's defaults, `diff.algorithm` and `diff.indentHeuristic`, which a
/// branch's config cannot change here). Lines keep their line ends, so a last line that gains
/// one is a changed line, as git shows it. The hunks in `-U0`'s numbers, and the lines added and
/// removed.
fn line_diff(old: &[u8], new: &[u8]) -> (Vec<Hunk>, u64, u64) {
    use gix::diff::blob::{Algorithm, Diff, InternedInput};
    let input = InternedInput::new(old, new);
    let mut diff = Diff::compute(Algorithm::Myers, &input);
    diff.postprocess_lines(&input);
    let hunks = diff
        .hunks()
        .map(|hunk| {
            let old_lines = hunk.before.end - hunk.before.start;
            let new_lines = hunk.after.end - hunk.after.start;
            Hunk {
                old_start: hunk.before.start + u32::from(old_lines > 0),
                old_lines,
                new_start: hunk.after.start + u32::from(new_lines > 0),
                new_lines,
            }
        })
        .collect();
    (
        hunks,
        u64::from(diff.count_additions()),
        u64::from(diff.count_removals()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_walked_only_while_its_longest_entry_still_fits_the_path_limit() {
        // A 20-byte root, `/` and a folder: the folder, `/`, a 255-byte name and the zero.
        let fits = 1024 - 20 - 1 - (1 + 255 + 1);
        assert!(!too_deep(20, fits, 1024));
        assert!(too_deep(20, fits + 1, 1024));
        assert!(!too_deep(20, 0, 1024));
        // Nothing wraps past the limit, however long a path is.
        assert!(too_deep(usize::MAX, usize::MAX, usize::MAX - 1));
    }

    /// A chain of folders under `at/top`, each named by 200 bytes, longer in all than
    /// `longer_than` bytes, with a file at its bottom. Built by renames, so no call is handed a
    /// path longer than a few hundred bytes.
    fn deep_chain(at: &Path, longer_than: usize) {
        let top = at.join("top");
        std::fs::create_dir(&top).unwrap();
        std::fs::write(top.join("bottom.txt"), "at the bottom\n").unwrap();
        let (mut length, mut n) = (0, 0);
        while length <= longer_than {
            let wrapper = at.join("wrapper");
            std::fs::create_dir(&wrapper).unwrap();
            let name = format!("{n:03}{}", "d".repeat(197));
            std::fs::rename(&top, wrapper.join(&name)).unwrap();
            std::fs::rename(&wrapper, &top).unwrap();
            length += name.len() + 1;
            n += 1;
        }
    }

    /// #1130: the walk for untracked files used to end at the first folder too long to name,
    /// and the whole status with it. In a repository with a separate git directory, as a
    /// branch's folder is.
    #[test]
    fn a_folder_chain_past_the_path_limit_is_one_untracked_entry_and_the_walk_goes_on() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let work = base.join("work");
        let git_dir = base.join("work.git");
        let made = crate::testgit::run(
            &base,
            &[
                "init",
                "-q",
                "-b",
                "main",
                "--separate-git-dir",
                &git_dir.display().to_string(),
                &work.display().to_string(),
            ],
        );
        assert!(made.ok(), "{}", made.err);
        std::fs::write(work.join("tracked.txt"), "one\n").unwrap();
        assert!(crate::testgit::run(&work, &["add", "tracked.txt"]).ok());
        assert!(crate::testgit::run(&work, &["commit", "-q", "-m", "one"]).ok());
        // Past Linux's 4,096 bytes, and so past macOS's 1,024 too.
        deep_chain(&work, 4096 + work.as_os_str().len());
        // Walked after `top`: the walk has to come back from the chain to find it.
        std::fs::write(work.join("zz-new.txt"), "new\n").unwrap();
        std::fs::write(work.join("tracked.txt"), "two\n").unwrap();

        let repo = gix::open(&work).unwrap();
        let (mut loose, mut untracked, mut gone) = Default::default();
        uncommitted(&repo, &mut loose, &mut untracked, &mut gone)
            .expect("the status degrades rather than fails");

        assert_eq!(loose, BTreeSet::from(["tracked.txt".to_string()]));
        assert!(gone.is_empty(), "{gone:?}");
        assert!(untracked.contains("zz-new.txt"), "{untracked:?}");
        let in_chain: Vec<&String> = untracked
            .iter()
            .filter(|path| path.starts_with("top/"))
            .collect();
        assert_eq!(in_chain.len(), 1, "{untracked:?}");
        assert!(!in_chain[0].ends_with("bottom.txt"), "{untracked:?}");
        // The folder named is one the walk could still have read, and the first it could not
        // enter: its parent fits the limit with room for a name, and it does not.
        let root_len = work.as_os_str().len();
        assert!(too_deep(root_len, in_chain[0].len(), PATH_MOST));
        let parent = in_chain[0].rsplit_once('/').unwrap().0;
        assert!(!too_deep(root_len, parent.len(), PATH_MOST));
        assert_eq!(untracked.len(), 2, "{untracked:?}");
    }

    #[test]
    fn a_line_that_gains_its_line_end_is_a_changed_line_as_git_shows_it() {
        let (hunks, added, removed) = line_diff(b"a\nb", b"a\nb\nc\n");
        assert_eq!((added, removed), (2, 1));
        assert_eq!(
            hunks,
            [Hunk {
                old_start: 2,
                old_lines: 1,
                new_start: 2,
                new_lines: 2
            }]
        );
    }

    /// git's own answer, `git diff --no-index -U0` with its default `diff.indentHeuristic`: the
    /// repeated block is placed after the first `b`, not after the second `a` as Myers alone
    /// places it (`-c diff.indentHeuristic=false` answers `@@ -5,0 +6,3 @@`).
    #[test]
    fn an_ambiguous_hunk_sits_where_gits_indent_heuristic_puts_it() {
        let (hunks, ..) = line_diff(b"1\n2\na\n\nb\n3\n4\n", b"1\n2\na\n\nb\na\n\nb\n3\n4\n");
        assert_eq!(
            hunks,
            [Hunk {
                old_start: 4,
                old_lines: 0,
                new_start: 5,
                new_lines: 3
            }]
        );
    }

    #[test]
    fn a_pure_insertion_names_the_line_it_comes_after() {
        let (hunks, ..) = line_diff(b"a\nb\n", b"top\na\nb\n");
        assert_eq!(
            hunks,
            [Hunk {
                old_start: 0,
                old_lines: 0,
                new_start: 1,
                new_lines: 1
            }]
        );
    }

    #[test]
    fn members_come_after_the_members_they_need_and_otherwise_in_the_records_order() {
        use crate::change::Member;
        let member = |repo: &str, needs: &[&str]| Member {
            repo: repo.to_string(),
            branch: "change/x".to_string(),
            needs: needs.iter().map(|n| n.to_string()).collect(),
        };
        let members = [
            member("app", &["api", "schema"]),
            member("api", &["schema"]),
            member("docs", &[]),
            member("schema", &[]),
        ];
        let order: Vec<&str> = needs_order(&members)
            .iter()
            .map(|m| m.repo.as_str())
            .collect();
        assert_eq!(order, ["docs", "schema", "api", "app"]);
    }
}
