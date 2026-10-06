//! The Bash guard's arm for git on the PROJECT from a sandboxed chat outside its root folder: a
//! command that would take one of git's lock files in the project's own repository (#1345).
//!
//! A sandboxed chat writes inside its own folder. A workspace chat's folder is inside the
//! project's working tree, but the project's `.git` is not inside the chat's folder, so `git
//! add`, `git commit`, `git fetch` and every other command that takes `index.lock` or a ref's
//! lock stop on `Operation not permitted`, and git's own message names a lock file and nothing
//! else. An agent reading it retries, deletes the "stale" lock, or concludes the repository is
//! broken. None of that is the answer: the app saves and syncs the project.
//!
//! **A mistake guard, not the boundary.** The sandbox already refuses the write; this arm only
//! says why before the command runs, and what to do instead. So it speaks only where it is sure:
//!
//! - **the chat was given a sandbox**, as the app says in the chat's own environment
//!   ([`crate::sandbox::chat_is_sandboxed`]) — never because the project's policy is on, which
//!   a chat opted out, a system with no backend, or a harness started from a terminal is not;
//! - **the chat's folder is not the project's root folder**, which a chat there may write;
//! - **git will use the project's repository**: the one repository git names, from `--git-dir`
//!   or `GIT_DIR` where one is given and from discovery at the last `-C` otherwise
//!   ([`repository`]);
//! - and it fails OPEN wherever it cannot place the command: a `cd` on the line, a wrapper's own
//!   chdir, a shell expansion in what names the repository.
//!
//! **Brokering is the other answer, and a later one.** Saving and syncing the project from a
//! chat through the app, as `save` will be (#1333), would let these run. Until then a sentence
//! that names who does it is the honest answer.

use std::path::{Path, PathBuf};

use crate::shellwrap::{self, base_lower};
use crate::{gitconfig, shellseg};

/// The trace reason this arm is tallied under.
pub const REASON: &str = "project-git-lock";

/// The git subcommands that take a lock file in the repository they act on: the index's or a
/// ref's. Read-only ones (`status`, `log`, `diff`, `show`) work from a sandboxed chat and are
/// not here; `branch` and `tag` are not either, because their listing forms are read-only and
/// the arm only speaks when it is sure.
const LOCKING: [&str; 18] = [
    "add",
    "am",
    "checkout",
    "cherry-pick",
    "commit",
    "fetch",
    "gc",
    "merge",
    "mv",
    "pull",
    "push",
    "rebase",
    "reset",
    "revert",
    "rm",
    "stash",
    "switch",
    "update-index",
];

/// Whether `git <sub> <post…>` takes a lock: a [`LOCKING`] subcommand, except the forms that
/// only read — `stash list` and `stash show`, and the dry runs of `push` (`--dry-run`, `-n`),
/// `fetch` and `commit` (`--dry-run`; their `-n` is `--no-tags` and `--no-verify`, which write).
/// `add -n` is not one: it takes the index's lock all the same (#1345's own probe).
fn locks(sub: &str, post: &[String]) -> bool {
    if !LOCKING.contains(&sub) {
        return false;
    }
    let has = |flag: &str| post.iter().any(|arg| arg == flag);
    match sub {
        "stash" => !matches!(post.first().map(String::as_str), Some("list" | "show")),
        "push" => !(has("--dry-run") || has("-n")),
        "fetch" | "commit" => !has("--dry-run"),
        _ => true,
    }
}

/// What the chat is told, for `git <sub>`.
pub fn said(sub: &str) -> String {
    format!(
        "this chat runs sandboxed in a folder below the project's root, and its sandbox does not \
         let it write the project's own git folder, so `git {sub}` would stop on one of git's \
         lock files (Operation not permitted). That lock file is not stale, and removing it will \
         not help. The app saves and syncs the project: by itself where auto-save is on, and \
         otherwise when the operator presses Save in the title bar or saves from the Saving \
         view. Leave the project's commits, pushes and fetches to it, and tell the operator \
         when something is ready to save. Read-only git (`git status`, `git log`, `git diff`) \
         works here."
    )
}

/// The refusal for `cmd`, run in `cwd` by a sandboxed chat whose folder is `chat_dir`, in the
/// project at `root`: `(subcommand, sentence)`, or `None`.
///
/// The caller has already asked whether the chat was given a sandbox, and found its folder
/// (`toolgate::Plane::chat_dir`); this asks everything else.
pub fn refusal(cmd: &str, cwd: &str, root: &str, chat_dir: &Path) -> Option<(String, String)> {
    if !shellwrap::prefilter_text(cmd).contains("git") {
        return None;
    }
    let root_real = std::fs::canonicalize(root).ok()?;
    let chat_real = std::fs::canonicalize(chat_dir).ok()?;
    // The root's own chat writes the root, `.git` with it; a folder outside the project is not
    // a chat this project starts.
    if chat_real == root_real || !chat_real.starts_with(&root_real) {
        return None;
    }
    let project_git = real_git_dir(&root_real)?;
    let segments = shellseg::joined_argv(cmd)?;
    let argvs: Vec<Vec<String>> = segments.iter().map(|s| s.argv.clone()).collect();
    let exported = shellwrap::exported_env(&argvs);
    for (segment, before) in segments.iter().zip(exported.iter()) {
        let call = shellwrap::split_env_chdir(&segment.argv);
        let program = base_lower(&call.prog);
        // Where the rest of the line runs is no longer `cwd`: fail open.
        if matches!(program.as_str(), "cd" | "pushd" | "popd") {
            return None;
        }
        if program != "git" || !call.chdir.is_empty() {
            continue;
        }
        let (pre, rest) = shellwrap::git_globals(&call.argv);
        let Some((sub, post)) = rest.split_first() else {
            continue;
        };
        if !locks(sub, post) {
            continue;
        }
        // Earlier exports first, then this invocation's own assignments, which override them.
        let mut env = before.clone();
        env.extend(call.env.iter().cloned());
        if repository(Path::new(cwd), &pre, &env).as_ref() == Some(&project_git) {
            return Some((sub.clone(), said(sub)));
        }
    }
    None
}

/// Whether a word names a place only the shell knows: an expansion this arm cannot follow.
fn unplaceable(word: &str) -> bool {
    word.contains(['$', '`', '~'])
}

/// The one repository git will use, run in `cwd` with its global options `pre` and the
/// environment assignments `env` (`NAME=value`, later ones winning), as the kernel names it.
///
/// `--git-dir` or `GIT_DIR` where one is given — the option winning over the variable, as in
/// git — and otherwise the repository git discovers from the directory the last `-C` leaves it
/// in. A work tree (`--work-tree`, `GIT_WORK_TREE`) moves no lock: the index and the refs are
/// the git directory's. `None` where a word naming the place is a shell expansion, or the
/// place does not resolve.
fn repository(cwd: &Path, pre: &[String], env: &[String]) -> Option<PathBuf> {
    let mut git_dir: Option<String> = env
        .iter()
        .rev()
        .find_map(|assign| assign.strip_prefix("GIT_DIR="))
        .map(str::to_owned);
    let mut here = cwd.to_path_buf();
    let mut i = 0;
    while i < pre.len() {
        let token = pre[i].as_str();
        if token == "-C" {
            let value = pre.get(i + 1)?;
            if unplaceable(value) {
                return None;
            }
            if !value.is_empty() {
                here = here.join(value);
            }
            i += 2;
            continue;
        }
        if let Some(value) = token.strip_prefix("--git-dir=") {
            git_dir = Some(value.to_owned());
            i += 1;
            continue;
        }
        if token == "--git-dir" {
            git_dir = Some(pre.get(i + 1)?.clone());
            i += 2;
            continue;
        }
        i += if shellwrap::GIT_VALUE_OPTS.contains(&token) {
            2
        } else {
            1
        };
    }
    match git_dir.filter(|dir| !dir.is_empty()) {
        Some(dir) if unplaceable(&dir) => None,
        Some(dir) => std::fs::canonicalize(here.join(dir)).ok(),
        None => real_git_dir(&here),
    }
}

/// The git directory git discovers from `dir`, resolved.
fn real_git_dir(dir: &Path) -> Option<PathBuf> {
    let found = gitconfig::git_dir_at(&dir.to_string_lossy())?;
    std::fs::canonicalize(found).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project with a `.git` of its own, a workspace folder inside it, and a clone of its own
    /// in that workspace. A `.git` directory is all git's discovery looks for.
    struct Project {
        _dir: tempfile::TempDir,
        root: PathBuf,
    }

    impl Project {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = std::fs::canonicalize(dir.path()).unwrap();
            std::fs::create_dir_all(root.join(".git")).unwrap();
            std::fs::create_dir_all(root.join("workspaces/ide/repo/.git")).unwrap();
            Self { _dir: dir, root }
        }

        fn root(&self) -> String {
            self.root.display().to_string()
        }

        fn workspace(&self) -> String {
            self.root.join("workspaces/ide").display().to_string()
        }

        fn clone(&self) -> String {
            self.root.join("workspaces/ide/repo").display().to_string()
        }

        /// The refusal for `cmd` from the sandboxed workspace chat, run in `cwd`.
        fn asked_from_workspace(&self, cmd: &str, cwd: &str) -> Option<(String, String)> {
            refusal(cmd, cwd, &self.root(), Path::new(&self.workspace()))
        }
    }

    #[test]
    fn git_that_takes_a_lock_on_the_project_from_a_workspace_chat_is_refused_with_who_does_it() {
        let p = Project::new();
        for (cmd, sub) in [
            ("git add -n .", "add"),
            ("git fetch", "fetch"),
            ("git fetch -n", "fetch"),
            ("git commit -m 'x'", "commit"),
            ("git commit -n -m 'x'", "commit"),
            ("git stash", "stash"),
            ("git push", "push"),
        ] {
            let (said_sub, why) = p
                .asked_from_workspace(cmd, &p.workspace())
                .unwrap_or_else(|| panic!("{cmd} was let through"));
            assert_eq!(said_sub, sub);
            assert!(why.contains("The app saves and syncs the project"), "{why}");
            assert!(why.contains("Save in the title bar"), "{why}");
            assert!(why.contains("Saving view"), "{why}");
            assert!(why.contains(&format!("`git {sub}`")), "{why}");
        }
        let named = format!("git -C {} fetch", p.root());
        assert!(
            p.asked_from_workspace(&named, &p.workspace()).is_some(),
            "{named}"
        );
        let by_dir = format!("GIT_DIR={}/.git git commit -m x", p.root());
        assert!(
            p.asked_from_workspace(&by_dir, &p.clone()).is_some(),
            "{by_dir}"
        );
    }

    #[test]
    fn read_only_git_and_dry_runs_are_let_through() {
        let p = Project::new();
        for cmd in [
            "git status",
            "git log -1",
            "git diff",
            "git stash list",
            "git push --dry-run",
            "git push -n",
            "git fetch --dry-run",
            "git commit --dry-run",
            "ls",
        ] {
            assert_eq!(p.asked_from_workspace(cmd, &p.workspace()), None, "{cmd}");
        }
    }

    #[test]
    fn git_on_a_workspaces_own_clone_is_let_through_however_it_is_named() {
        let p = Project::new();
        let ws = p.workspace();
        let clone = p.clone();
        assert_eq!(p.asked_from_workspace("git commit -m x", &clone), None);
        for cmd in [
            format!("git -C {clone} fetch"),
            format!("git --git-dir={clone}/.git --work-tree={clone} commit -m x"),
            format!("git --git-dir {clone}/.git commit -m x"),
            format!("GIT_DIR={clone}/.git git commit -m x"),
            format!("export GIT_DIR={clone}/.git; git commit -m x"),
            format!(
                "git -C {} -C ide/repo commit -m x",
                p.root.join("workspaces").display()
            ),
        ] {
            assert_eq!(p.asked_from_workspace(&cmd, &ws), None, "{cmd}");
        }
    }

    #[test]
    fn the_root_folders_own_chat_is_let_through() {
        let p = Project::new();
        assert_eq!(
            refusal("git add .", &p.root(), &p.root(), &p.root),
            None,
            "the root's chat writes the root"
        );
    }

    #[test]
    fn a_line_it_cannot_place_is_let_through_rather_than_guessed_at() {
        let p = Project::new();
        let cd = format!("cd {} && git commit -m x", p.clone());
        assert_eq!(p.asked_from_workspace(&cd, &p.workspace()), None, "{cd}");
        for cmd in [
            "git -C $REPO fetch",
            "GIT_DIR=$X git commit -m x",
            "git --git-dir=~/r/.git commit -m x",
        ] {
            assert_eq!(p.asked_from_workspace(cmd, &p.workspace()), None, "{cmd}");
        }
    }
}
