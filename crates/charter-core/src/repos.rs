//! The clones a workspace holds, and what git says about each one.
//!
//! Read-only and local-only. Nothing here asks a forge anything — that is `cistate`, which
//! reads a cache and never fetches. A panel is drawn every time a workspace is focused, so
//! the only thing it is allowed to wait for is a disk.
//!
//! # Three states, never two
//!
//! A `git status` that failed writes nothing to stdout, and `""` read as "clean" is how
//! Python's status line renders an unreadable tree as clean and in sync
//! (`charter/statusline.py:_run_state` answers all-false-and-zero on *any* exception —
//! a timeout, a deleted directory and a genuinely clean repo are byte-identical). That same
//! codebase has `charter/gitstate.py` precisely to forbid it, and this follows `gitstate`:
//! a tree charter could not read says so, in git's own words, and carries no counts at all.
//!
//! # Where the clones come from
//!
//! The directory, not `workspace.json`. A manifest records *membership* — which repos this
//! workspace means to hold — and a clone somebody made by hand is still a clone. Python
//! answers the same way (`charter/workspace.py:clones`), and the plane root itself is
//! deliberately never a row (ADR 0008): it is the plane, not a repo you work in.

use std::path::{Path, PathBuf};

use crate::contain;
use crate::workspaces::Plane;
use crate::worktree::confine::{self, Outside};
use crate::worktree::git;

/// One clone under a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// The directory's name, which is how charter addresses the clone.
    pub name: String,
    /// The path that was **checked**, which is the one a caller hands to git. Returning the
    /// checked path rather than letting the caller rebuild it is `within_workspace`'s own
    /// rule: a string checked and a different string used is how a link gets laundered.
    pub path: PathBuf,
}

/// What a workspace's directory holds, and what charter would not look at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clones {
    pub repos: Vec<Repo>,
    /// Names charter refused, with the reason, so a row that is missing is never merely
    /// missing. Swallowing these is how a redirected clone reads as "no repos here".
    pub refused: Vec<(String, String)>,
}

/// charter could not look at the workspace at all.
#[derive(Debug, thiserror::Error)]
pub enum Trouble {
    #[error(transparent)]
    Outside(#[from] Outside),
    #[error("could not read workspace '{ws}' ({why})")]
    Unreadable { ws: String, why: String },
}

/// The clones directly under `workspaces/<ws>`, sorted, with what was refused beside them.
pub fn clones(plane: &Path, ws: &str) -> Result<Clones, Trouble> {
    // The workspace directory is the anchor, and it is asked for rather than assumed: a
    // committed `workspaces/<legal-name> -> elsewhere` travels to every machine that clones
    // the plane, and everything below it would then be judged against the wrong place.
    let dir = confine::workspace_dir(plane, ws)?;
    let reader = std::fs::read_dir(&dir).map_err(|why| Trouble::Unreadable {
        ws: ws.to_string(),
        why: why.to_string(),
    })?;
    let mut names: Vec<String> = reader
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();

    let mut found = Clones::default();
    for name in names {
        // charter's own directories under a workspace all start with a dot — `.worktrees`,
        // `.claude`, `.charter-generated` — and none of them is a repo.
        if name.starts_with('.') {
            continue;
        }
        if !contain::repo_name_ok(&name) {
            found.refused.push((
                name.clone(),
                format!("'{name}' is not a name charter will take for a repo"),
            ));
            continue;
        }
        // The path git would be pointed at, gated as **itself** and never as its parent: a
        // link AT `workspaces/<ws>/<repo>` redirects the read while the directory above it
        // is perfectly ordinary, so a check on the parent sees nothing to object to.
        let path = match confine::within_workspace(plane, ws, &dir.join(&name)) {
            Ok(checked) => checked,
            Err(why) => {
                found.refused.push((name, why.to_string()));
                continue;
            }
        };
        match git_at(&path) {
            Git::Directory => found.repos.push(Repo { name, path }),
            // A linked worktree, or a directory that is not a repository at all. Neither is a
            // mistake and neither is a clone, so neither is worth a line in the panel.
            Git::NotAClone => {}
            // Said, not dropped. A repo that quietly disappears from the panel reads as "this
            // workspace has one fewer repo", which is the same class of lie as a blank CI
            // cell — and the operator is the only one who can tell whether the link is theirs.
            Git::Link => found.refused.push((
                name.clone(),
                format!(
                    "'{name}/.git' is a symlink, and charter will not run git through one: \
                     the repository it acts on would not be the one inside this workspace. A \
                     `.git` charter created is never a link"
                ),
            )),
        }
    }
    Ok(found)
}

/// What sits where a clone's `.git` would be.
///
/// Three answers and not two, for the same reason `state_of` has three: "not a clone" and
/// "a clone charter will not touch" are different facts, and only one of them is worth
/// telling the operator about.
enum Git {
    /// A directory — which is how git draws the line. A linked worktree's `.git` is a FILE
    /// holding `gitdir:`, so this admits clones and nothing else.
    Directory,
    /// A gitfile, or nothing at all. An ordinary shape, and not a clone.
    NotAClone,
    /// A link. Charter refuses it and says so.
    Link,
}

/// Asked with `symlink_metadata`, so a `.git` that is a *link* to a directory elsewhere is
/// seen as the link it is. Python asks `is_dir()`, which follows it — and the whole point of
/// gating the repo path is not to then run git against a repository outside the boundary
/// that was just checked. A `.git` charter created is never a link.
fn git_at(path: &Path) -> Git {
    match std::fs::symlink_metadata(path.join(".git")) {
        Ok(found) if found.file_type().is_symlink() => Git::Link,
        Ok(found) if found.is_dir() => Git::Directory,
        _ => Git::NotAClone,
    }
}

/// The repos `workspace.json` names, in the order it lists them.
///
/// Membership, not presence: a name here with no clone on disk is a repo the workspace means
/// to hold that nobody has cloned yet, which is worth drawing. The names come out of a file
/// an operator edits, so each is checked before it is shown and one that is not a name is
/// dropped rather than drawn.
pub fn declared(plane: &Plane, ws: &str) -> Vec<String> {
    let Ok(workspace) = plane.workspace(ws) else {
        return Vec::new();
    };
    let (Some(doc), _) = workspace.manifest() else {
        return Vec::new();
    };
    doc.get("repos")
        .and_then(|repos| repos.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("name")?.as_str())
                .filter(|name| contain::repo_name_ok(name))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Where a tree's HEAD is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    Branch(String),
    /// A branch git has named but which holds no commit yet.
    Unborn(String),
    /// Not on a branch. The short commit, or `""` when git would not name it.
    Detached(String),
}

/// What git said about one tree. Every field here is only ever set from a `git status` that
/// **succeeded** — there is no value in this struct that means "charter does not know".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeState {
    pub head: Head,
    /// The upstream branch git names, where the branch has one.
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Changed files git is tracking.
    pub tracked: u32,
    /// Files git is not tracking.
    pub untracked: u32,
}

impl TreeState {
    pub fn clean(&self) -> bool {
        self.tracked == 0 && self.untracked == 0
    }
}

/// charter could not read the tree — which is not the same as the tree being clean.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error(
    "charter could not read the working tree at {path} — {why}. That is not the same as it \
     being clean"
)]
pub struct Unreadable {
    pub path: String,
    pub why: String,
}

/// `git status --porcelain=v1 --branch` on one tree, through the hardened runner.
///
/// The runner is `worktree::git`, and it is not optional: `status` is the verb that runs the
/// **fsmonitor**, which is a program named by config — including a config in a `HOME` an
/// attacker set. `git -c core.fsmonitor=false` on the command line beats every config file,
/// and a `Command::new("git")` here would walk straight past it.
pub fn state_of(tree: &Path) -> Result<TreeState, Unreadable> {
    let unreadable = |why: String| Unreadable {
        path: tree.display().to_string(),
        why,
    };
    let seen = git::run(tree, &["status", "--porcelain=v1", "--branch"], git::READ)
        .map_err(|err| unreadable(err.to_string()))?;
    if !seen.ok() {
        let why = match seen.code {
            // The deadline passed, so git was killed and said nothing. Its own sentence,
            // because "exited " with no code is not a diagnosis.
            None => format!("git did not answer within {} seconds", git::READ.as_secs()),
            Some(code) => match first_line(&seen.err) {
                Some(said) => said,
                None => format!("git status exited {code}"),
            },
        };
        return Err(unreadable(why));
    }
    let mut state = parse(&seen.out).ok_or_else(|| unreadable("git named no branch".into()))?;
    // Only for a detached HEAD, which is rare: `--branch` says `## HEAD (no branch)` and
    // names no commit, and "detached at <sha>" is the part an operator acts on.
    if state.head == Head::Detached(String::new()) {
        state.head = Head::Detached(commit_at(tree));
    }
    Ok(state)
}

/// The short commit a tree's HEAD sits on, or `""` when git will not name it.
///
/// Asked only for a detached HEAD. A `rev-parse` that fails does not make the tree
/// unreadable — `status` already answered — so the commit is simply not named.
fn commit_at(tree: &Path) -> String {
    // The `sha.ok()` guard cannot be told from `true` by any test (cargo-mutants reports it
    // as a survivor, excluded in `.cargo/mutants.toml`): `rev-parse --short HEAD` writes
    // nothing to stdout when it fails — measured on an unborn branch, where plain `rev-parse
    // HEAD` echoes `HEAD` but `--short` prints nothing — so a failed run's `line()` is the
    // same `""` the other arm returns. The guard is kept because that is git's behaviour and
    // not its contract.
    match git::run(tree, &["rev-parse", "--short", "HEAD"], git::READ) {
        Ok(sha) if sha.ok() => sha.line().to_string(),
        _ => String::new(),
    }
}

/// The first non-blank line of git's own stderr, which is the sentence worth repeating.
fn first_line(err: &str) -> Option<String> {
    err.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// `git status --porcelain=v1 --branch`, whose first line is `## <branch>` and whose others
/// are one changed path each.
///
/// The runner gives git `LC_ALL=C`, so the English matched below is the English git writes.
fn parse(out: &str) -> Option<TreeState> {
    let mut lines = out.lines();
    let (head, upstream, ahead, behind) = head_line(lines.next()?.strip_prefix("## ")?);
    let mut tracked = 0u32;
    let mut untracked = 0u32;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        // `??` is the only status code for a path git is not tracking. Everything else is a
        // change to something it is, which is the distinction `doctor` asks about.
        if line.starts_with("??") {
            untracked = untracked.saturating_add(1);
        } else {
            tracked = tracked.saturating_add(1);
        }
    }
    Some(TreeState {
        head,
        upstream,
        ahead,
        behind,
        tracked,
        untracked,
    })
}

/// The `## …` line: a branch, its upstream, and how far apart they are.
fn head_line(rest: &str) -> (Head, Option<String>, u32, u32) {
    if rest == "HEAD (no branch)" {
        return (Head::Detached(String::new()), None, 0, 0);
    }
    // `[` is a character `git check-ref-format` refuses, so " [" can never be part of a
    // branch name and this split cannot cut one in half.
    let (names, counts) = match rest.split_once(" [") {
        Some((names, counts)) => (names, counts.strip_suffix(']').unwrap_or(counts)),
        None => (rest, ""),
    };
    // `..` is refused in a ref name too, so `...` separates the pair and never splits one.
    let (name, upstream) = match names.split_once("...") {
        Some((name, up)) => (name, Some(up.to_string())),
        None => (names, None),
    };
    let mut ahead = 0u32;
    let mut behind = 0u32;
    for part in counts.split(", ") {
        if let Some(count) = part.strip_prefix("ahead ") {
            ahead = count.parse().unwrap_or(0);
        } else if let Some(count) = part.strip_prefix("behind ") {
            behind = count.parse().unwrap_or(0);
        }
    }
    // A branch with no commit on it yet: git says so in words instead of naming a ref. Both
    // spellings, because the older one is what a machine with git before 2.11 writes and a
    // plane travels.
    for said in ["No commits yet on ", "Initial commit on "] {
        if let Some(branch) = name.strip_prefix(said) {
            return (Head::Unborn(branch.to_string()), upstream, ahead, behind);
        }
    }
    (Head::Branch(name.to_string()), upstream, ahead, behind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(out: &str) -> TreeState {
        parse(out).expect("the first line names a head")
    }

    #[test]
    fn a_branch_with_an_upstream_carries_both_names() {
        let state = read("## main...origin/main\n");

        assert_eq!(state.head, Head::Branch("main".into()));
        assert_eq!(state.upstream.as_deref(), Some("origin/main"));
        assert!(state.clean());
    }

    #[test]
    fn a_branch_with_no_upstream_has_none_rather_than_an_empty_one() {
        let state = read("## work\n");

        assert_eq!(state.head, Head::Branch("work".into()));
        assert_eq!(state.upstream, None);
    }

    #[test]
    fn how_far_apart_the_two_are_is_read_from_the_bracket() {
        let state = read("## main...origin/main [ahead 2, behind 13]\n");

        assert_eq!((state.ahead, state.behind), (2, 13));
    }

    #[test]
    fn a_slash_in_a_branch_name_survives_both_splits() {
        // `feature/x` is ordinary, and a split that took the first `/` would halve it.
        let state = read("## feature/x...origin/feature/x [ahead 1]\n");

        assert_eq!(state.head, Head::Branch("feature/x".into()));
        assert_eq!(state.upstream.as_deref(), Some("origin/feature/x"));
        assert_eq!((state.ahead, state.behind), (1, 0));
    }

    #[test]
    fn a_detached_head_is_not_a_branch_called_head() {
        let state = read("## HEAD (no branch)\n");

        assert_eq!(state.head, Head::Detached(String::new()));
        assert_eq!(state.upstream, None);
    }

    #[test]
    fn a_branch_with_no_commit_on_it_yet_is_unborn_and_not_a_branch_of_that_sentence() {
        for out in ["## No commits yet on main\n", "## Initial commit on main\n"] {
            assert_eq!(read(out).head, Head::Unborn("main".into()), "{out:?}");
        }
    }

    #[test]
    fn tracked_changes_and_untracked_files_are_counted_apart() {
        let state = read("## main\n M src/a.rs\nA  src/b.rs\n?? scratch\n?? other\n");

        assert_eq!((state.tracked, state.untracked), (2, 2));
        assert!(!state.clean());
    }

    #[test]
    fn output_with_no_branch_line_is_not_a_state_charter_invents() {
        // A `status` whose first line is not `## ` is one charter does not understand, and
        // "understood nothing" must not read as "clean on no branch".
        assert_eq!(parse(""), None);
        assert_eq!(parse("?? scratch\n"), None);
    }

    #[test]
    fn the_sentence_repeated_is_the_first_line_git_actually_wrote() {
        // `charter/gitstate.py:said`: the first non-blank line, stripped once, or nothing.
        // Blank lines before it are skipped, the advice after it is not repeated.
        assert_eq!(
            first_line("\n   \n  fatal: not a git repository  \nhint: run git init\n").as_deref(),
            Some("fatal: not a git repository")
        );
        assert_eq!(first_line(""), None);
        assert_eq!(first_line("\n \t \n"), None);
    }

    #[test]
    fn a_detached_head_is_named_by_the_short_commit_git_gives_it() {
        // "detached at <sha>" is the part an operator acts on, so it is git's own short sha
        // and not merely something non-empty.
        let dir = tempfile::tempdir().unwrap();
        let tree = std::fs::canonicalize(dir.path()).unwrap();
        let fixture = |args: &[&str]| {
            let mut command = std::process::Command::new("git");
            command
                .args(args)
                .current_dir(&tree)
                .env_clear()
                .env("PATH", "/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin")
                .env("HOME", &tree)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_AUTHOR_NAME", "Fixture")
                .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
                .env("GIT_COMMITTER_NAME", "Fixture")
                .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid");
            let out = crate::forklock::output(&mut command).expect("git runs");
            assert!(out.status.success(), "git {args:?}");
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        fixture(&["init", "-q", "-b", "main", "."]);
        fixture(&["commit", "-q", "--allow-empty", "-m", "one"]);
        fixture(&["checkout", "-q", "--detach"]);

        assert_eq!(commit_at(&tree), fixture(&["rev-parse", "--short", "HEAD"]));
    }
}
