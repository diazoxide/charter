//! A commit the app makes for a sandboxed chat in the branch folder it stands in: **a brokered
//! git action** (ADR 0067 §2 as amended on 2026-10-08, #1055, ruling V99h).
//!
//! A linked worktree keeps its index, its HEAD and its objects in its clone's `.git`, outside
//! the folder a chat may write, so `git add` and `git commit` are refused there. The worktree's
//! git directory is not opened to the chat. `purlis worktree commit` hands the app a message
//! and what to stage ([`crate::hookwire::CommitAsk`]), and the app stages and commits, outside
//! the sandbox.
//!
//! # What the app takes from the ask, and what it does not
//!
//! The message and the paths. **Where** is the app's own record of the chat, never the ask:
//! the folder the chat stands in must be one purlis cut (`<workspace>/.worktrees/<repo>/<piece>`),
//! its `.git` file must name that clone's worktree and be named back ([`link`]), and every git
//! call is given that git directory and work tree. **Which branch** is the one that folder is
//! on. **Who** is the identity the repository's own config gives, or the person's, and the
//! trailers are the chat's (ADR 0074).
//!
//! # What git may run
//!
//! Nothing but git. No global or system config is read, hooks and the file-system monitor are
//! off, and git starts no git of its own inside a submodule. A repository whose config names
//! a program is refused before any git runs in it ([`super::runs_a_program`]); so is one with
//! a `config.worktree` in the worktree's git directory, one with an `objects/info/alternates`,
//! and one whose config signs its commits. Nothing else of the repository is refused: its
//! `info/attributes` is read as git reads it, and can name no program without a config that
//! defines one. So the repository's own `pre-commit` does not run, and the answer says so
//! each time one is there. purlis's own scan of what the commit would publish does run, here.
//!
//! # What staging reads
//!
//! The chat wrote the folder. A path is taken only as a name inside it: relative, no `..`, no
//! `.git`, none reached through a link. git stores a link as a link and never reads what it
//! points at. Every call is held to the checked git directory wherever in the folder the
//! chat stands, so nothing is found again from the folder after the checks. What was staged
//! is then read back, and the commit is unstaged and refused where it would record another
//! repository (a gitlink), change `.gitmodules`, add a name that differs from another only
//! by case, or hold a file its attributes mark for a content filter: the person's filters
//! (an LFS one, most often) live in the config this commit does not read, so the file would
//! be stored as it is on disk where their own git would store the filtered form.
//!
//! # One at a time, and never left half done
//!
//! One commit runs in a branch folder at a time. Before it stages, the app leaves a note in
//! the worktree's git directory, which no chat writes, and takes it away when it has
//! committed or put the index back. An app that stopped in between is found out by the next
//! ask: what is staged is what that note says the app staged, so it is unstaged and the ask
//! carries on. Staged changes that are not the app's are someone's, and are left alone.
//!
//! # What it never does
//!
//! Amend, reset, rebase, merge, push, or commit on another branch or in another folder. The
//! ask has no word for any of them.

use std::path::{Component, Path, PathBuf};

use super::Asker;
use crate::hookwire::{Answer, CommitAsk, Stage};
use crate::repocmd::Say;
use crate::worktree::{self, git, link};

/// The longest message taken, in bytes: far past any commit message, and under what one ask
/// carries.
pub const MOST_MESSAGE: usize = 32 * 1024;

/// The most paths one ask names.
pub const MOST_PATHS: usize = 1000;

/// The longest one path, in bytes: the longest a path may be on the systems purlis runs on.
pub const MOST_PATH: usize = 4096;

/// What every refusal of a spelling the command does not have says.
pub const COMMITS_ONLY: &str = "purlis commits for a sandboxed chat and does nothing else to \
     history: it does not amend, reset, rebase, merge or push. Make a new commit, or ask the \
     person to run the command in their own terminal";

/// What a chat that is not sandboxed is told: it needs no broker.
pub const PLAIN_GIT: &str = "this chat is not sandboxed, so `git add` and `git commit` work \
     here as they are: use them";

/// How long git may take to stage: it reads every named file, so a read's deadline is not
/// its deadline.
pub const STAGING: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// How long git may take to write the commit.
pub const COMMITTING: std::time::Duration = std::time::Duration::from_secs(2 * 60);

/// The note the app leaves in a worktree's git directory while it commits there: empty until
/// staging ends, then the id of the tree it staged.
const NOTE: &str = "purlis-brokered-commit";

/// The repository's hooks a commit would have run, which a brokered one does not.
const HOOKS: [&str; 4] = [
    "pre-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
];

/// Commits for `asker` in the project at `root` what `ask` names, reading the person's
/// identity from `identity` and nothing else of their config, and answers the new commit, or
/// one sentence saying why not.
pub fn answer(root: &Path, asker: &Asker, ask: &CommitAsk, identity: &git::Isolated) -> Answer {
    match commit(root, asker, ask, identity) {
        Ok(lines) => Answer::Said {
            lines: super::fitted(lines, super::SAID_AT_MOST),
            code: 0,
        },
        Err(why) => Answer::No { why },
    }
}

fn commit(
    root: &Path,
    asker: &Asker,
    ask: &CommitAsk,
    identity: &git::Isolated,
) -> Result<Vec<Say>, String> {
    if !sandboxed(root, asker)? {
        return Err(PLAIN_GIT.to_owned());
    }
    message_ok(&ask.message)?;
    if let Stage::Paths(paths) = &ask.stage {
        paths_ok(paths)?;
    }
    let here = Here::of(root, asker.cwd.as_deref())?;
    if let Stage::Paths(paths) = &ask.stage {
        for path in paths {
            through_no_link(&here.cwd, path)?;
        }
    }
    // One commit at a time in a branch folder: a second ask waits for the first.
    let folder = lock_of(&here.git_dir);
    let _held = folder
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Every call below reads the checked git directory, and no config but the repository's.
    let plain = git::Isolated::default().pinned(&here.top, &here.git_dir);
    git::isolated(&plain, || -> Result<Vec<Say>, String> {
        super::runs_a_program(&here.clone)?;
        made(root, asker.chat, ask, &here, identity)
    })
}

/// The lock of the branch folder whose git directory is `git_dir`, for this process's life.
fn lock_of(git_dir: &Path) -> std::sync::Arc<std::sync::Mutex<()>> {
    static FOLDERS: std::sync::LazyLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, std::sync::Arc<std::sync::Mutex<()>>>>,
    > = std::sync::LazyLock::new(Default::default);
    FOLDERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .entry(git_dir.to_path_buf())
        .or_default()
        .clone()
}

/// The git half: stages and commits what `ask` names in the checked folder `here`, for chat
/// `chat` of the project at `root`. Run with the thread's git held to `here`'s git directory.
fn made(
    root: &Path,
    chat: u32,
    ask: &CommitAsk,
    here: &Here,
    identity: &git::Isolated,
) -> Result<Vec<Say>, String> {
    let theirs = identity.pinned(&here.top, &here.git_dir);
    here.borrows_nothing()?;
    let top = here.top.as_path();
    if said(
        top,
        &[
            "config",
            "--local",
            "--type=bool",
            "--get",
            "commit.gpgsign",
        ],
    )?
    .is_some_and(|value| value == "true")
    {
        return Err(
            "this repository signs its commits (`commit.gpgsign`), and purlis does not \
             sign for a sandboxed chat: a signer is a program that may ask for a \
             passphrase. Commit this one from your own terminal"
                .to_owned(),
        );
    }
    let branch = said(top, &["symbolic-ref", "--quiet", "--short", "HEAD"])?.ok_or(
        "this folder is not on a branch (a detached HEAD), so there is no branch to \
         commit on",
    )?;
    here.mid_nothing()?;
    let before = ran(
        top,
        &["diff", "--cached", "--quiet", "--ignore-submodules=none"],
    )?;
    let mut lines = Vec::new();
    match before.code {
        Some(0) => here.forget(),
        Some(1) if here.left_by_an_interrupted_commit() => {
            unstage(top);
            here.forget();
            lines.push(Say::Info(
                "an earlier commit for this chat was interrupted after it staged; what it \
                 had staged was unstaged first, and the files are as they were"
                    .to_owned(),
            ));
        }
        Some(1) => {
            return Err(
                "this folder has staged changes that purlis did not stage for a chat. Commit \
                 or unstage them in your own terminal first"
                    .to_owned(),
            );
        }
        _ => {
            return Err(format!(
                "git could not read this folder: {}",
                first_line(&before.err)
            ));
        }
    }
    here.note("")?;
    // From here the index may hold what the ask named: anything that stops the commit puts
    // it back and takes the note away.
    let staged = stage(here, &ask.stage)
        .and_then(|()| {
            let tree = said(top, &["write-tree"])?.unwrap_or_default();
            here.note(&tree)
        })
        .and_then(|()| staged(top))
        .and_then(|staged| {
            if staged.is_empty() {
                return Err(
                    "there is nothing to commit: no named path differs from the branch's \
                     last commit"
                        .to_owned(),
                );
            }
            if let Some(why) = records_another_repository(&staged) {
                return Err(why);
            }
            case_twin(top, &staged).map_or(Ok(()), Err)?;
            filtered(top, &staged).map_or(Ok(()), Err)?;
            scanned(top)?;
            Ok(staged)
        });
    let staged = match staged {
        Ok(staged) => staged,
        Err(why) => {
            unstage(top);
            here.forget();
            return Err(why);
        }
    };
    for hook in here.hooks_not_run() {
        lines.push(Say::Warn(format!(
            "this repository's {hook} hook was not run: for a sandboxed chat, purlis \
             commits with hooks off"
        )));
    }
    let trailers = crate::provenance::trailers_for(root, top, chat);
    let message = crate::provenance::append(&ask.message, &trailers, None);
    // The repository's own identity where it has one, else the person's.
    let own = said(top, &["config", "--local", "--get", "user.name"])?.is_some()
        && said(top, &["config", "--local", "--get", "user.email"])?.is_some();
    let committed = git::within((!own).then_some(&theirs), || {
        git::run_with_input(
            top,
            &[
                "-c",
                "gc.auto=0",
                "-c",
                "maintenance.auto=false",
                "commit",
                "--quiet",
                "--no-gpg-sign",
                "--cleanup=whitespace",
                "--file=-",
            ],
            message.into_bytes(),
            COMMITTING,
        )
    })
    .map_err(|e| e.to_string());
    let committed = match committed {
        Ok(committed) => committed,
        Err(why) => {
            unstage(top);
            here.forget();
            return Err(why);
        }
    };
    if committed.code != Some(0) {
        unstage(top);
        here.forget();
        if committed.code.is_none() {
            return Err(ran_out("write the commit", COMMITTING));
        }
        return Err(format!(
            "git did not make the commit: {}",
            first_line(&String::from_utf8_lossy(&committed.err))
        ));
    }
    here.forget();
    let id = said(top, &["rev-parse", "HEAD"])?.unwrap_or_default();
    lines.insert(
        0,
        Say::Done(format!(
            "committed {id} on {branch}: {} file(s)",
            staged.len()
        )),
    );
    Ok(lines)
}

/// Whether `asker` runs sandboxed: on a harness, not started without the sandbox, in a project
/// where one is in force. A manifest that cannot be read may turn it on, so it refuses.
pub fn sandboxed(root: &Path, asker: &Asker) -> Result<bool, String> {
    if !asker.harnessed || asker.unsandboxed {
        return Ok(false);
    }
    let plane = crate::sandbox::Plane::read(root);
    if plane.unreadable() {
        return Err(crate::sandbox::NotStarted::PlaneUnreadable.to_string());
    }
    let locks = crate::sandbox::policy::Locks::of(root);
    Ok(plane.in_force(&locks).is_some())
}

/// Why `message` is not one to commit, or nothing.
pub fn message_ok(message: &str) -> Result<(), String> {
    if message.trim().is_empty() {
        return Err("the commit message is empty".to_owned());
    }
    if message.len() > MOST_MESSAGE {
        return Err(format!(
            "the commit message is longer than {MOST_MESSAGE} bytes"
        ));
    }
    if message.contains('\0') {
        return Err("the commit message holds a NUL byte".to_owned());
    }
    Ok(())
}

/// Why `paths` are not names inside the chat's folder, or nothing: each relative, with no
/// `..`, no `.git` at any depth in any case, no control character, and none too long.
///
/// A name that looks like an option (`-n`, `--amend`) is a name: paths reach git as data on
/// its standard input, never as arguments.
pub fn paths_ok(paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("no path was named to stage".to_owned());
    }
    if paths.len() > MOST_PATHS {
        return Err(format!(
            "more than {MOST_PATHS} paths were named: stage a folder, or commit in parts"
        ));
    }
    for path in paths {
        let shown = shown(path);
        if path.is_empty() {
            return Err("an empty path was named".to_owned());
        }
        if path.len() > MOST_PATH {
            return Err(format!("the path {shown} is longer than {MOST_PATH} bytes"));
        }
        if path.chars().any(char::is_control) {
            return Err(format!("the path {shown} holds a control character"));
        }
        let outside = || {
            format!(
                "the path {shown} is not inside this chat's folder: name paths relative to \
                 where the chat stands, with no `..`"
            )
        };
        if Path::new(path).is_absolute() || path.starts_with(['/', '\\']) {
            return Err(outside());
        }
        for part in Path::new(path).components() {
            match part {
                Component::Normal(name) => {
                    if name.to_string_lossy().eq_ignore_ascii_case(".git") {
                        return Err(format!(
                            "the path {shown} names a `.git`, which is never committed"
                        ));
                    }
                }
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(outside());
                }
            }
        }
    }
    Ok(())
}

/// Why `path`, below `cwd`, is reached through a link, or nothing. Its last part may be a
/// link: git stores that as a link.
pub fn through_no_link(cwd: &Path, path: &str) -> Result<(), String> {
    let mut at = cwd.to_path_buf();
    let parts: Vec<Component<'_>> = Path::new(path)
        .components()
        .filter(|part| !matches!(part, Component::CurDir))
        .collect();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        at.push(part);
        if std::fs::symlink_metadata(&at).is_ok_and(|found| found.file_type().is_symlink()) {
            return Err(format!(
                "the path {} is reached through a link ({}), and purlis stages nothing \
                 through a link",
                shown(path),
                at.display()
            ));
        }
    }
    Ok(())
}

/// `path` for a sentence: quoted, and cut where it is long.
fn shown(path: &str) -> String {
    let short: String = path.chars().take(120).collect();
    if short.len() < path.len() {
        format!("{short:?}…")
    } else {
        format!("{short:?}")
    }
}

/// The folder the commit is made in, as the app derived and checked it.
#[derive(Debug)]
struct Here {
    /// The branch folder's top, as the kernel names it.
    top: PathBuf,
    /// Where the chat stands, in it.
    cwd: PathBuf,
    /// The worktree's git directory, in its clone's `.git/worktrees`.
    git_dir: PathBuf,
    /// The clone's `.git`.
    common: PathBuf,
    /// The clone it was cut from.
    clone: PathBuf,
}

impl Here {
    /// The branch folder `cwd` is in, where it is one purlis cut in the project at `root`
    /// whose `.git` file names its clone's worktree and nothing else.
    fn of(root: &Path, cwd: Option<&Path>) -> Result<Self, String> {
        let cwd = cwd.ok_or(
            "the app has no record of where this chat stands, so it will not commit for it",
        )?;
        let not_a_branch_folder = || {
            "this chat does not stand in a branch folder purlis cut \
             (`<workspace>/.worktrees/<repo>/<piece>`), so the app will not commit for it. \
             In a clone, `git add` and `git commit` work as they are"
                .to_owned()
        };
        let piece = worktree::locate(root, cwd).ok_or_else(not_a_branch_folder)?;
        let (top, git_dir) = match link::of(cwd, link::Check::Whole) {
            Ok(link::Link::Linked { top, git_dir }) => (top, git_dir),
            Ok(_) => return Err(not_a_branch_folder()),
            Err(why) => return Err(why.to_owned()),
        };
        let recorded = root
            .join("workspaces")
            .join(&piece.workspace)
            .join(&piece.repo);
        let (Some(common), Some(clone)) = (link::worktree_of(&git_dir), link::cut_from(&top))
        else {
            return Err(not_a_branch_folder());
        };
        if !super::same_dir(&clone, &recorded) || !super::same_dir(&clone.join(".git"), &common) {
            return Err(link::CHANGED.to_owned());
        }
        Ok(Self {
            cwd: std::fs::canonicalize(cwd).map_err(|_| not_a_branch_folder())?,
            top,
            git_dir,
            common,
            clone,
        })
    }

    /// Why the repository reads objects or settings from somewhere `runs_a_program` did not
    /// look, or nothing: a per-worktree config, or objects borrowed from another repository.
    fn borrows_nothing(&self) -> Result<(), String> {
        for (file, what) in [
            (
                self.git_dir.join("config.worktree"),
                "a configuration of this worktree's own",
            ),
            (
                self.common.join("objects/info/alternates"),
                "objects borrowed from another repository",
            ),
        ] {
            if std::fs::symlink_metadata(&file).is_ok() {
                return Err(format!(
                    "this repository has {what} ({}), which the app does not read for a chat, \
                     so it will not commit here. Commit from your own terminal",
                    file.display()
                ));
            }
        }
        Ok(())
    }

    /// Why the folder is in the middle of something a commit would conclude, or nothing.
    fn mid_nothing(&self) -> Result<(), String> {
        for (entry, what) in [
            ("MERGE_HEAD", "a merge"),
            ("CHERRY_PICK_HEAD", "a cherry-pick"),
            ("REVERT_HEAD", "a revert"),
            ("rebase-merge", "a rebase"),
            ("rebase-apply", "a rebase"),
            ("BISECT_LOG", "a bisect"),
        ] {
            if std::fs::symlink_metadata(self.git_dir.join(entry)).is_ok() {
                return Err(format!(
                    "this folder is in the middle of {what}, which a commit here would \
                     conclude. Finish or abort it in your own terminal first"
                ));
            }
        }
        Ok(())
    }

    /// Leaves the app's note in the worktree's git directory: `tree` is what it staged, or
    /// nothing while it is staging.
    fn note(&self, tree: &str) -> Result<(), String> {
        std::fs::write(self.git_dir.join(NOTE), tree).map_err(|e| {
            format!(
                "the app could not write in this folder's git directory ({e}), so it did not commit"
            )
        })
    }

    /// Takes the note away.
    fn forget(&self) {
        let _ = std::fs::remove_file(self.git_dir.join(NOTE));
    }

    /// Whether what is staged is what an interrupted commit of the app's left: its note is
    /// there, and says it was still staging or names the tree the index holds now.
    fn left_by_an_interrupted_commit(&self) -> bool {
        let Ok(noted) = std::fs::read_to_string(self.git_dir.join(NOTE)) else {
            return false;
        };
        let noted = noted.trim();
        noted.is_empty()
            || said(&self.top, &["write-tree"])
                .ok()
                .flatten()
                .is_some_and(|now| now == noted)
    }

    /// The repository's own hooks a commit would have run, which this one does not.
    fn hooks_not_run(&self) -> Vec<&'static str> {
        let dir = said(&self.top, &["config", "--local", "--get", "core.hooksPath"])
            .ok()
            .flatten()
            .map_or_else(|| self.common.join("hooks"), |named| self.top.join(named));
        HOOKS
            .into_iter()
            .filter(|name| runs(&dir.join(name)))
            .collect()
    }
}

/// Whether `hook` is a file git would run.
fn runs(hook: &Path) -> bool {
    let Ok(found) = std::fs::metadata(hook) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        found.is_file() && found.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        found.is_file()
    }
}

/// One git call in `dir`, or the sentence saying git could not be run.
fn ran(dir: &Path, args: &[&str]) -> Result<git::Run, String> {
    git::run(dir, args, git::READ).map_err(|e| e.to_string())
}

/// What one git call printed, where it succeeded and printed something.
fn said(dir: &Path, args: &[&str]) -> Result<Option<String>, String> {
    let run = ran(dir, args)?;
    Ok(run
        .ok()
        .then(|| run.line().trim().to_owned())
        .filter(|line| !line.is_empty()))
}

/// The first line of what git said, for a sentence.
fn first_line(text: &str) -> String {
    let line = text.lines().find(|line| !line.trim().is_empty());
    line.unwrap_or("it gave no reason").trim().to_owned()
}

/// What a call git did not finish in time is answered with.
fn ran_out(what: &str, within: std::time::Duration) -> String {
    format!(
        "git did not {what} within {} minutes, so it was stopped and nothing was committed",
        within.as_secs() / 60
    )
}

/// Stages what `stage` names. Paths reach git on its standard input, NUL-separated and
/// literal, so none is read as an option or as pathspec magic.
fn stage(here: &Here, stage: &Stage) -> Result<(), String> {
    let (code, err) = match stage {
        Stage::Tracked => {
            let run = git::run(&here.top, &["add", "--update", "--", "."], STAGING)
                .map_err(|e| e.to_string())?;
            (run.code, run.err)
        }
        Stage::Paths(paths) => {
            let run = git::run_with_input(
                &here.cwd,
                &[
                    "--literal-pathspecs",
                    "add",
                    "--pathspec-from-file=-",
                    "--pathspec-file-nul",
                ],
                paths.join("\0").into_bytes(),
                STAGING,
            )
            .map_err(|e| e.to_string())?;
            (run.code, String::from_utf8_lossy(&run.err).into_owned())
        }
    };
    match code {
        Some(0) => Ok(()),
        None => Err(ran_out("finish staging", STAGING)),
        Some(_) => Err(format!("git did not stage that: {}", first_line(&err))),
    }
}

/// Puts the index back to the branch's last commit: what the ask staged is unstaged, and the
/// files are left as they are. The index held nothing else when staging began.
fn unstage(top: &Path) {
    let _ = git::run(top, &["reset", "--quiet"], git::READ);
}

/// One staged change: the mode it would be committed with, how it differs, and its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged {
    /// The mode before, as git writes it (`100644`, `120000`, `160000`, `000000`).
    pub was: String,
    /// The mode after.
    pub mode: String,
    /// git's letter: `A`, `M`, `D`, `T`.
    pub status: char,
    pub path: String,
}

/// What is staged in `top` against its last commit.
fn staged(top: &Path) -> Result<Vec<Staged>, String> {
    let run = git::run_with_input(
        top,
        &[
            "diff",
            "--cached",
            "--raw",
            "-z",
            "--no-renames",
            "--no-abbrev",
            "--ignore-submodules=none",
        ],
        Vec::new(),
        git::READ,
    )
    .map_err(|e| e.to_string())?;
    if run.code != Some(0) {
        return Err(format!(
            "git could not read what was staged: {}",
            first_line(&String::from_utf8_lossy(&run.err))
        ));
    }
    parse_raw(&run.out).ok_or_else(|| "git's list of what was staged could not be read".to_owned())
}

/// `git diff --raw -z --no-renames`: `:<was> <mode> <sha> <sha> <status>\0<path>\0`, repeated.
pub fn parse_raw(out: &[u8]) -> Option<Vec<Staged>> {
    let mut fields = out.split(|byte| *byte == 0);
    let mut all = Vec::new();
    while let Some(head) = fields.next() {
        if head.is_empty() {
            continue;
        }
        let head = std::str::from_utf8(head).ok()?.strip_prefix(':')?;
        let mut words = head.split(' ');
        let (was, mode) = (words.next()?, words.next()?);
        let status = words.nth(2)?.chars().next()?;
        let path = fields.next().filter(|path| !path.is_empty())?;
        let path = String::from_utf8(path.to_vec()).ok()?;
        all.push(Staged {
            was: was.to_owned(),
            mode: mode.to_owned(),
            status,
            path,
        });
    }
    Some(all)
}

/// git's mode for an entry that names a commit of another repository.
const GITLINK: &str = "160000";

/// Why `staged` would record another repository, or nothing: a gitlink entry (a folder with a
/// `.git` of its own, which git stages as a submodule), or a change to `.gitmodules`, which
/// tells a later git where to fetch one from and what to run on it.
pub fn records_another_repository(staged: &[Staged]) -> Option<String> {
    for change in staged {
        if change.mode == GITLINK || change.was == GITLINK {
            return Some(format!(
                "{} is a repository of its own (it holds a `.git`), and purlis does not \
                 commit one inside another for a chat. Remove its `.git`, or leave it out",
                shown(&change.path)
            ));
        }
        let name = change.path.rsplit('/').next().unwrap_or(&change.path);
        if name.eq_ignore_ascii_case(".gitmodules") {
            return Some(format!(
                "{} declares submodules, and purlis does not commit a change to it for a \
                 chat. Commit it from your own terminal",
                shown(&change.path)
            ));
        }
    }
    None
}

/// Why a path `staged` adds differs from another tracked path only by case, or nothing. On a
/// file system that ignores case the two are one file, and a checkout writes one over the other.
fn case_twin(top: &Path, staged: &[Staged]) -> Option<String> {
    if !staged.iter().any(|change| change.status == 'A') {
        return None;
    }
    let listed = git::run_with_input(top, &["ls-files", "-z"], Vec::new(), git::READ).ok()?;
    let tracked: Vec<String> = listed
        .out
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect();
    twin_among(&tracked, staged)
}

/// [`case_twin`], over the tracked paths.
pub fn twin_among(tracked: &[String], staged: &[Staged]) -> Option<String> {
    let mut folded: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    for path in tracked {
        if let Some(other) = folded.insert(path.to_lowercase(), path.as_str())
            && other != path.as_str()
            && staged.iter().any(|change| {
                change.status == 'A' && (change.path == *path || change.path == other)
            })
        {
            return Some(format!(
                "{} and {} differ only by case, and purlis does not add such a pair for a \
                 chat. Rename one",
                shown(other),
                shown(path)
            ));
        }
    }
    None
}

/// Why a path `staged` adds or changes is marked for a content filter, or nothing.
fn filtered(top: &Path, staged: &[Staged]) -> Option<String> {
    let paths: Vec<&str> = staged
        .iter()
        .filter(|change| change.status != 'D')
        .map(|change| change.path.as_str())
        .collect();
    if paths.is_empty() {
        return None;
    }
    let asked = git::run_with_input(
        top,
        &["check-attr", "-z", "--stdin", "filter"],
        paths.join("\0").into_bytes(),
        git::READ,
    );
    match asked {
        Ok(run) if run.code == Some(0) => filter_named(&run.out).map(|(path, filter)| {
            format!(
                "{} is marked for a content filter (`filter={filter}` in its attributes). A \
                 commit made for a sandboxed chat reads none of the person's filters, so the \
                 file would be stored as it is on disk. The person commits a filtered file \
                 from their own terminal",
                shown(&path)
            )
        }),
        _ => Some("git could not read the attributes of what was staged".to_owned()),
    }
}

/// The first path `git check-attr -z filter` answers a filter for, and the filter:
/// `<path>\0filter\0<value>\0`, repeated, where no filter is `unspecified` or `unset`.
pub fn filter_named(out: &[u8]) -> Option<(String, String)> {
    let fields: Vec<&[u8]> = out.split(|byte| *byte == 0).collect();
    fields.as_chunks::<3>().0.iter().find_map(|answer| {
        let value = String::from_utf8_lossy(answer[2]).into_owned();
        (!matches!(value.as_str(), "unspecified" | "unset"))
            .then(|| (String::from_utf8_lossy(answer[0]).into_owned(), value))
    })
}

/// purlis's scan of what the staged change would publish (ADR 0074): the check a chat's own
/// `pre-commit` makes, made here.
fn scanned(top: &Path) -> Result<(), String> {
    let scan = crate::diffscan::checked(top)
        .map_err(|why| format!("the commit could not be scanned for secrets ({why})"))?;
    if scan.changes_the_allowlist {
        return Err(
            "the commit changes the scan's allowlist, which only a commit made outside a chat \
             may change. Leave the file out; the person reviews and commits an entry"
                .to_owned(),
        );
    }
    if scan.refused.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{}. Fix what the scan found and commit again; `purlis scan --explain` names the \
         entry that would let a finding through",
        crate::diffscan::summary(crate::diffscan::Stopped::Commit, top, &scan.refused)
    ))
}

#[cfg(test)]
mod tests;
