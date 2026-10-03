//! What a branch changed, file by file and rolled up onto its folders (FM-4, #1107; #1103 V86
//! F6): the tree's markers, and what "Changed only" collapses it to.
//!
//! **Against the branch it was cut from**, committed or not. The base is the one fact charter
//! records about a piece, `branch.<branch>.charterBase` in the clone's config (ADR 0027,
//! `worktree::add`), and the changes are counted from where the branch left it (`git
//! merge-base`), so what the base gained since is not the branch's change. A branch with no
//! recorded base — the repo's own folder, a branch cut by plain git — is marked against its
//! last commit: what is not committed yet.
//!
//! **Two git reads, both through the hardened runner**, which turns off the fsmonitor and
//! hooks on the command line:
//! - `git status --porcelain=v2 -z`: what is not committed, and the files git does not track
//!   and does not ignore;
//! - `git diff --name-status -z` from the base to the working tree, with renames found, no
//!   external diff and no text conversion.
//!
//! **Neither runs a filter driver the repo's config names.** Both reads compare file content,
//! and to compare a file whose time moved and whose size did not, git runs the clean filter its
//! attributes name. A repo's config is anything working in the branch can write, and the app
//! reads this on its own after every write, so each driver named at repo scope (the clone's
//! config, its worktree config, and whatever those include) is blanked on the command line for
//! these two reads: the content passes through unfiltered, and nothing runs. A driver whose name
//! cannot be spelled as a `-c` key is refused rather than left on. The operator's own drivers
//! (global and system scope, Git LFS's among them) are left as they are. What remains: a driver
//! added between the listing and the read, and every other git call charter makes (#810).
//!
//! **The recorded base is read, never trusted.** Anything working in the branch can write the
//! clone's config, so the value must be a branch name charter would hand git, or a commit
//! named by its sha; it is resolved to a commit before git sees it as anything but data, and a
//! base that does not resolve is treated as none.
//!
//! **Every path answered is a plain path inside the branch**: relative, no `..`, never git's
//! own. A renamed file's source is a name, shown and never followed. Nothing here reads a file.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::{Branch, Refused, base_of, inside};
use crate::worktree::{self, git, name};

/// The most changes one status answers with, sorted by path: a branch that generated a
/// hundred thousand files is marked as its first and a count of the rest. Folders roll up all
/// of them.
pub const MARKED: usize = 10_000;

/// What a branch did to one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mark {
    /// Its content or its kind changed.
    Changed,
    /// It is new: git does not have it at the base.
    Added,
    /// It is gone.
    Deleted,
    /// It moved here from another path.
    Renamed,
}

/// One path the branch changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Its path relative to the branch's folder.
    pub path: String,
    pub mark: Mark,
    /// Where a renamed file came from: a name, never followed.
    pub from: Option<String>,
    /// Whether the change is not committed yet; `false` is one the branch committed.
    pub uncommitted: bool,
}

/// One folder holding changes: the mark its changes share, or [`Mark::Changed`] when they are
/// of more than one kind, and how many it holds at any depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rolled {
    /// Its path relative to the branch's folder; `""` is the branch's own.
    pub folder: String,
    pub mark: Mark,
    pub count: usize,
}

/// What a branch changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// Sorted by path, the first [`MARKED`].
    pub changes: Vec<Change>,
    /// Every folder holding a change, sorted by path.
    pub folders: Vec<Rolled>,
    /// How many changes past [`MARKED`] are not in `changes`.
    pub more: usize,
    /// The branch the changes are counted against, or `None` when they are counted against the
    /// last commit.
    pub base: Option<String>,
}

/// What the branch changed against the branch it was cut from, committed or not, with each
/// folder rolling up what it holds. What git ignores is never marked.
pub fn status(plane: &Path, branch: Branch<'_>) -> Result<Status, Refused> {
    let base = base_of(plane, branch)?;
    let unreadable = |why: String| Refused::Unreadable {
        what: format!("the changes of {}", branch.called()),
        why,
    };

    let filters_off = repo_filters_off(&base).map_err(unreadable)?;
    let mut asked: Vec<&str> = filters_off.iter().map(String::as_str).collect();
    asked.extend(git::with_untracked_cache(
        &base,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=all",
        ],
    ));
    let loose = read(&base, &asked).map_err(unreadable)?;
    let loose = uncommitted(&loose);

    let (since, named) = match recorded_base(&base) {
        Some((sha, named)) => (Some(sha), Some(named)),
        None => (head(&base), None),
    };
    // A branch with no commit yet has nothing to count from: what is loose is all there is.
    let mut changes: BTreeMap<String, Change> = BTreeMap::new();
    if let Some(since) = since {
        let mut asked: Vec<&str> = filters_off.iter().map(String::as_str).collect();
        asked.extend([
            "--no-optional-locks",
            "diff",
            "--name-status",
            "-z",
            "-M",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=all",
            &since,
            "--",
        ]);
        let diffed = read(&base, &asked).map_err(unreadable)?;
        for change in diffed_changes(&diffed) {
            changes.insert(change.path.clone(), change);
        }
    }
    for (path, kind) in &loose {
        match changes.get_mut(path) {
            Some(change) => change.uncommitted = true,
            // Untracked, or tracked and loose in a way the diff did not see: the diff from the
            // base names every tracked change, so this is what it cannot, a file git does not
            // track yet.
            None if *kind == Loose::Untracked => {
                changes.insert(
                    path.clone(),
                    Change {
                        path: path.clone(),
                        mark: Mark::Added,
                        from: None,
                        uncommitted: true,
                    },
                );
            }
            None => {}
        }
    }
    let all: Vec<Change> = changes.into_values().collect();
    let folders = rolled_up(&all);
    let more = all.len().saturating_sub(MARKED);
    let mut changes = all;
    changes.truncate(MARKED);
    Ok(Status {
        changes,
        folders,
        more,
        base: named,
    })
}

/// `-c` settings that blank every filter driver the repo's own config names (local and
/// worktree scope, and what they include): its clean, smudge and process programs emptied, and
/// not required, so git passes content through and runs nothing. A driver whose name cannot be
/// spelled as a `-c` key — it holds `=` — is refused, never left on.
fn repo_filters_off(base: &Path) -> Result<Vec<String>, String> {
    let listed = read_allowing_none(
        base,
        &["config", "-z", "--show-scope", "--get-regexp", "^filter\\."],
    )?;
    let mut names = BTreeSet::new();
    let mut fields = listed.split(|byte| *byte == 0);
    while let Some(scope) = fields.next() {
        let Some(entry) = fields.next() else { break };
        // The operator's own config is theirs, not the repo's.
        if matches!(scope, b"global" | b"system") {
            continue;
        }
        let key = entry
            .split(|byte| *byte == b'\n')
            .next()
            .unwrap_or_default();
        let key = String::from_utf8_lossy(key);
        let Some(name) = key
            .strip_prefix("filter.")
            .and_then(|rest| rest.rsplit_once('.'))
            .map(|(name, _)| name.to_string())
        else {
            continue;
        };
        if name.contains('=') || name.contains('\n') || name.contains('\0') {
            return Err(format!(
                "the repo's git config names a filter program, '{name}', that charter cannot \
                 turn off, so it does not read the branch's changes"
            ));
        }
        names.insert(name);
    }
    Ok(names
        .into_iter()
        .flat_map(|name| {
            ["clean=", "smudge=", "process=", "required=false"]
                .map(|setting| ["-c".to_string(), format!("filter.{name}.{setting}")])
        })
        .flatten()
        .collect())
}

/// [`read`], where git's exit 1 is an empty answer: `git config --get-regexp` finding nothing.
fn read_allowing_none(base: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let ran = git::run_with_input(base, args, Vec::new(), git::READ)
        .map_err(|e| worktree::Refusal::from(e).in_window())?;
    match ran.code {
        Some(0) => Ok(ran.out),
        Some(1) => Ok(Vec::new()),
        None => Err("git did not answer in time".to_string()),
        Some(_) => Err(String::from_utf8_lossy(&ran.err).trim().to_string()),
    }
}

/// git's answer as bytes, or why it gave none.
fn read(base: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let ran = git::run_with_input(base, args, Vec::new(), git::READ)
        .map_err(|e| worktree::Refusal::from(e).in_window())?;
    if ran.code != Some(0) {
        let said = String::from_utf8_lossy(&ran.err).trim().to_string();
        return Err(if ran.code.is_none() {
            "git did not answer in time".to_string()
        } else {
            said
        });
    }
    Ok(ran.out)
}

/// The commit HEAD is at, or `None` on a branch with no commit yet.
fn head(base: &Path) -> Option<String> {
    let ran = git::run(
        base,
        &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
        git::READ,
    )
    .ok()?;
    sha(ran.line()).filter(|_| ran.ok())
}

/// Where the branch left its recorded base, and the base's name: `None` when nothing is
/// recorded, the record is not one value, or it does not resolve to a commit HEAD shares.
fn recorded_base(base: &Path) -> Option<(String, String)> {
    let current = git::run(
        base,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        git::READ,
    )
    .ok()?;
    if !current.ok() {
        return None;
    }
    let branch = current.line();
    name::branch_name_ok(branch).ok()?;
    let key = format!("branch.{branch}.charterBase");
    let recorded = git::run(base, &["config", "--get-all", &key], git::READ).ok()?;
    let values: Vec<&str> = recorded
        .out
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect();
    let [value] = values.as_slice() else {
        return None;
    };
    // Resolved to a commit with nothing in the argument git could read as an option: a ref
    // under `refs/heads/`, or a sha.
    let (asked, named) = match value.strip_prefix(worktree::DETACHED_PREFIX) {
        Some(at) => (sha(at)?, at.to_string()),
        None => {
            name::branch_name_ok(value).ok()?;
            (name::as_ref(value), (*value).to_string())
        }
    };
    let resolved = git::run(
        base,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{asked}^{{commit}}"),
        ],
        git::READ,
    )
    .ok()?;
    let at = sha(resolved.line()).filter(|_| resolved.ok())?;
    let forked = git::run(base, &["merge-base", &at, "HEAD"], git::READ).ok()?;
    let forked = sha(forked.line()).filter(|_| forked.ok())?;
    Some((forked, named))
}

/// `text` when it is a commit's sha (or an abbreviation git would take), never anything git
/// could read as an option or a revision expression.
fn sha(text: &str) -> Option<String> {
    let text = text.trim();
    (text.len() >= 4 && text.len() <= 64 && text.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| text.to_string())
}

/// What `git status --porcelain=v2` calls a loose path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Loose {
    Tracked,
    Untracked,
}

/// Every loose path in a `git status --porcelain=v2 -z` answer, a rename's source as well as
/// its target. A path that is not a plain path inside the branch is dropped.
fn uncommitted(out: &[u8]) -> BTreeMap<String, Loose> {
    let mut loose = BTreeMap::new();
    let mut records = out.split(|byte| *byte == 0);
    while let Some(record) = records.next() {
        let Ok(record) = std::str::from_utf8(record) else {
            continue;
        };
        // The fields before the path, by the record's kind (git-status(1), "Porcelain Format
        // Version 2"); the path is everything after them, spaces and all.
        let (fields, kind) = match record.as_bytes().first() {
            Some(b'1') => (8, Loose::Tracked),
            Some(b'2') => (9, Loose::Tracked),
            Some(b'u') => (10, Loose::Tracked),
            Some(b'?') => (1, Loose::Untracked),
            _ => continue,
        };
        let renamed = record.starts_with('2');
        // An untracked folder is answered only for a repository nested in this one, as
        // `nested/`: another repository's business, left out as submodules are.
        if kind == Loose::Untracked && record.ends_with('/') {
            continue;
        }
        if let Some(path) = record.splitn(fields + 1, ' ').nth(fields).and_then(plain) {
            loose.insert(path, kind);
        }
        if renamed {
            // A rename's source is the next NUL-ended field.
            if let Some(from) = records
                .next()
                .and_then(|from| std::str::from_utf8(from).ok())
                .and_then(plain)
            {
                loose.insert(from, Loose::Tracked);
            }
        }
    }
    loose
}

/// Every change in a `git diff --name-status -z` answer: a status, then one path — or two for
/// a rename or a copy, its source first. A path that is not a plain path inside the branch is
/// dropped.
fn diffed_changes(out: &[u8]) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut fields = out
        .split(|byte| *byte == 0)
        .map(|field| std::str::from_utf8(field).ok());
    while let Some(Some(status)) = fields.next() {
        let Some(letter) = status.chars().next() else {
            break;
        };
        let paired = matches!(letter, 'R' | 'C');
        let first = fields.next().flatten();
        let (path, from) = if paired {
            (fields.next().flatten(), first)
        } else {
            (first, None)
        };
        let Some(path) = path.and_then(plain) else {
            continue;
        };
        let (mark, from) = match letter {
            'A' => (Mark::Added, None),
            'D' => (Mark::Deleted, None),
            'R' => match from.and_then(plain) {
                Some(from) => (Mark::Renamed, Some(from)),
                None => (Mark::Added, None),
            },
            // A copy leaves its source where it was: the copy is new.
            'C' => (Mark::Added, None),
            _ => (Mark::Changed, None),
        };
        changes.push(Change {
            path,
            mark,
            from,
            uncommitted: false,
        });
    }
    changes
}

/// `path` when it is a plain path inside the branch: relative, no `..`, and never git's own.
fn plain(path: &str) -> Option<String> {
    let relative = inside(path).ok()?;
    let git_s = relative.components().any(|step| step.as_os_str() == ".git");
    (!git_s && !path.contains('\\')).then(|| path.to_string())
}

/// Every folder holding a change, with the mark its changes share and how many it holds.
fn rolled_up(changes: &[Change]) -> Vec<Rolled> {
    let mut folders: BTreeMap<String, (BTreeSet<Mark>, usize)> = BTreeMap::new();
    for change in changes {
        let mut above = Some(change.path.as_str());
        while let Some(path) = above {
            let parent = path.rfind('/').map(|at| &path[..at]);
            let folder = parent.unwrap_or("");
            let entry = folders.entry(folder.to_string()).or_default();
            entry.0.insert(change.mark);
            entry.1 += 1;
            above = parent;
        }
    }
    folders
        .into_iter()
        .map(|(folder, (marks, count))| Rolled {
            folder,
            mark: match marks.iter().collect::<Vec<_>>().as_slice() {
                [one] => **one,
                _ => Mark::Changed,
            },
            count,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_git_answers_that_would_leave_the_branch_or_reach_git_is_dropped() {
        let out = b"A\0../out.txt\0M\0.git/config\0D\0sub/.git/HEAD\0R100\0../away\0in.txt\0M\0/etc/passwd\0A\0ok.txt\0";

        let changes = diffed_changes(out);

        let paths: Vec<(&str, Mark, Option<&str>)> = changes
            .iter()
            .map(|one| (one.path.as_str(), one.mark, one.from.as_deref()))
            .collect();
        assert_eq!(
            paths,
            [("in.txt", Mark::Added, None), ("ok.txt", Mark::Added, None)]
        );
    }

    #[test]
    fn a_loose_path_with_spaces_and_a_renames_source_are_both_read() {
        let out = b"1 .M N... 100644 100644 100644 abc abc a b.txt\0\
2 R. N... 100644 100644 100644 abc abc R100 new name.rs\0old name.rs\0\
? ../escape\0? fresh.txt\0";

        let loose = uncommitted(out);

        let paths: Vec<&str> = loose.keys().map(String::as_str).collect();
        assert_eq!(
            paths,
            ["a b.txt", "fresh.txt", "new name.rs", "old name.rs"]
        );
        assert_eq!(loose["fresh.txt"], Loose::Untracked);
    }

    #[test]
    fn a_recorded_base_is_a_sha_only_when_it_is_nothing_but_hex() {
        assert_eq!(sha("0123abcd"), Some("0123abcd".to_string()));
        assert_eq!(sha("--output=x"), None);
        assert_eq!(sha("HEAD~1"), None);
        assert_eq!(sha("abc"), None);
    }
}
