//! This machine's local project, moved out of the config home into the data home (#1670).
//!
//! The first run used to make the local project in purlis's folder in the config home
//! (`<config>/local-plane`). A chat's sandbox denies writing that whole folder, because it holds
//! the person's own powers (ADR 0067 §5, class 3), so no sandboxed chat could start in a project
//! inside it. The local project is made in the data home now (`<data>/local-project`,
//! [`crate::firstrun::local_plane`]), where nothing a chat is denied sits above it, and a machine
//! that has one in the old place has it moved there at the app's next launch ([`at_launch`]).
//!
//! # How a move is made, and why no file is lost
//!
//! - **One rename**, so the project is whole in one place or the other, never split: a move
//!   across file systems is refused by the system, and the project is left where it is.
//! - **A pointer is left at the old place** (a link to the new one) the moment the rename is
//!   made, so anything that still names the old path finds the project.
//! - **Then what names the old path is pointed at the new one**: the git links between each clone
//!   and its worktrees, the record of the chats that were open, and this machine's remembered
//!   projects and windows.
//! - **The pointer goes only once every one of those is done** and the project reads as a
//!   project in its new place. Until then it stays, and the next launch finishes the rest.
//!
//! # What is never moved
//!
//! - A project something works in: a purlis window holding it, or any process of this user whose
//!   working folder is inside it. It is left where it is, and the launch says so once
//!   ([`Moved::Left`]). It moves at a later launch, once nothing works in it.
//! - A project whose new place is taken: two folders are never merged.

use std::path::{Path, PathBuf};

/// The local project's folder's old name, in purlis's folder in the config home.
pub const OLD_NAME: &str = "local-plane";

/// Where this machine keeps the two places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    /// The folder the config home's folder sits in (`~/.config`).
    pub config_root: PathBuf,
    /// purlis's data home ([`crate::datahome`]).
    pub data_home: PathBuf,
}

impl Places {
    /// Where the local project used to be made: in purlis's folder in the config home.
    pub fn old(&self) -> PathBuf {
        crate::machine::dir(&self.config_root).join(OLD_NAME)
    }

    /// Where it is made now ([`crate::firstrun::local_plane`]).
    pub fn current(&self) -> PathBuf {
        crate::firstrun::local_plane(&self.data_home)
    }

    /// Every spelling of the old place a record may hold: as written, and as the system
    /// resolves its parent (the project was recorded resolved).
    fn old_spellings(&self) -> Vec<PathBuf> {
        let old = self.old();
        let mut out = vec![old.clone()];
        if let (Some(parent), Some(name)) = (old.parent(), old.file_name())
            && let Ok(real) = parent.canonicalize()
            && !out.contains(&real.join(name))
        {
            out.push(real.join(name));
        }
        out
    }

    /// The new place as the system resolves it, else as written.
    fn new_real(&self) -> PathBuf {
        let new = self.current();
        new.canonicalize().unwrap_or(new)
    }
}

/// What a move does to the file system, so a test can stand in for it.
pub struct Seams<'a> {
    /// Why something works in the project at this path, or `None` when nothing does.
    pub in_use: &'a dyn Fn(&Path) -> Option<String>,
    /// Rename a folder, as `std::fs::rename` does.
    pub rename: &'a dyn Fn(&Path, &Path) -> std::io::Result<()>,
}

/// What came of looking for a local project in the old place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Moved {
    /// There is none there: a machine made after the move, or one already moved.
    Nothing,
    /// Moved, everything that named the old place now names the new one, and the pointer is gone.
    Moved { from: PathBuf, to: PathBuf },
    /// Moved, and the pointer at the old place is kept, because something that names the old
    /// place could not be pointed at the new one yet. The next launch tries again.
    PointerKept {
        from: PathBuf,
        to: PathBuf,
        why: String,
    },
    /// Left where it is, and why.
    Left { at: PathBuf, why: String },
}

impl std::fmt::Display for Moved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Moved::Nothing => f.write_str("no local project in the config home to move"),
            Moved::Moved { from, to } => write!(
                f,
                "the local project moved from {} to {}, where a sandboxed chat can start",
                from.display(),
                to.display()
            ),
            Moved::PointerKept { from, to, why } => write!(
                f,
                "the local project moved from {} to {}; a link at the old place still points \
                 there, because {why}. purlis finishes the move at its next launch",
                from.display(),
                to.display()
            ),
            Moved::Left { at, why } => write!(
                f,
                "the local project stays at {} for now, because {why}. A sandboxed chat cannot \
                 start there; purlis moves it at a launch when nothing works in it",
                at.display()
            ),
        }
    }
}

/// Move the local project from the config home to the data home, or finish a move a launch
/// started (see the module).
pub fn run(places: &Places, seams: &Seams<'_>) -> Moved {
    let old = places.old();
    let new = places.current();
    let olds = places.old_spellings();
    match std::fs::symlink_metadata(&old) {
        Err(_) => return Moved::Nothing,
        Ok(meta) if meta.file_type().is_symlink() => {
            // A pointer a launch left: the rename was made, and only the rest is to finish. A
            // link that points anywhere else is not purlis's, and is left alone.
            if std::fs::read_link(&old).ok().as_deref() != Some(new.as_path()) {
                return Moved::Nothing;
            }
            return finish(places, &olds);
        }
        Ok(meta) if !meta.is_dir() => return Moved::Nothing,
        Ok(_) => {}
    }
    let left = |why: String| Moved::Left {
        at: old.clone(),
        why,
    };
    if std::fs::symlink_metadata(&new).is_ok() {
        return left(format!(
            "{} is there already, and purlis never merges two folders; keep the one you want \
             and move the other away",
            new.display()
        ));
    }
    if let Some(why) = crate::datahome::refusal(&new) {
        return left(why);
    }
    if let Some(why) = (seams.in_use)(&old) {
        return left(why);
    }
    if let Some(parent) = new.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return left(format!("{} could not be made ({e})", parent.display()));
    }
    if let Err(e) = (seams.rename)(&old, &new) {
        return left(format!("it could not be moved to {} ({e})", new.display()));
    }
    if let Err(e) = link(&new, &old) {
        // Without the pointer, a record still naming the old place would find nothing: the
        // rename is put back, so the project is exactly where it was.
        return match (seams.rename)(&new, &old) {
            Ok(()) => left(format!(
                "no link could be left at the old place ({e}), so the move was put back"
            )),
            Err(back) => Moved::PointerKept {
                from: old.clone(),
                to: new.clone(),
                why: format!(
                    "no link could be left at the old place ({e}) and the move could not be put \
                     back ({back}); the project is whole at {}",
                    new.display()
                ),
            },
        };
    }
    finish(places, &olds)
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

#[cfg(not(unix))]
fn link(_target: &Path, _at: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("links are made only on unix"))
}

/// Point what names the old place at the new one, then take the pointer away.
fn finish(places: &Places, olds: &[PathBuf]) -> Moved {
    let from = places.old();
    let to = places.current();
    let real = places.new_real();
    let mut failed: Vec<String> = Vec::new();
    if !to.is_dir() {
        failed.push(format!("{} is not there", to.display()));
    }
    failed.extend(git_links(&to, olds, &real));
    if let Err(e) = reopen_record(&to, olds, &real) {
        failed.push(format!(
            "the record of its open chats could not be rewritten ({e})"
        ));
    }
    if let Err(e) = machine_store(&places.config_root, olds, &real) {
        failed.push(format!(
            "this machine's remembered projects could not be rewritten ({e})"
        ));
    }
    if failed.is_empty()
        && let Err(e) = std::fs::remove_file(&from)
    {
        failed.push(format!(
            "the link at the old place could not be taken away ({e})"
        ));
    }
    if failed.is_empty() {
        Moved::Moved { from, to }
    } else {
        Moved::PointerKept {
            from,
            to,
            why: failed.join("; "),
        }
    }
}

/// `path` with whichever of `olds` it starts with replaced by `new`; `None` when it starts with
/// none of them.
fn rebased(path: &Path, olds: &[PathBuf], new: &Path) -> Option<PathBuf> {
    olds.iter().find_map(|old| {
        path.strip_prefix(old).ok().map(|rest| {
            if rest.as_os_str().is_empty() {
                new.to_path_buf()
            } else {
                new.join(rest)
            }
        })
    })
}

/// How deep below the project a clone's `.git` is looked for: `workspaces/<ws>/<repo>/.git` is
/// four, and a persona's or a nested layout one more.
const CLONE_DEPTH: usize = 5;

/// Every git repository's `.git` folder in `root`, to [`CLONE_DEPTH`], never through a link.
fn git_dirs(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut todo = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if !kind.is_dir() {
                continue;
            }
            let path = entry.path();
            if entry.file_name() == ".git" {
                found.push(path);
            } else if depth + 1 < CLONE_DEPTH {
                todo.push((path, depth + 1));
            }
        }
    }
    found
}

/// The two links between each repository in `root` and its linked worktrees, pointed at the new
/// place (what `git worktree repair` writes): each worktree's `gitdir` record in the repository,
/// and the `.git` file in the worktree. What could not be rewritten, in words.
fn git_links(root: &Path, olds: &[PathBuf], new: &Path) -> Vec<String> {
    let mut failed = Vec::new();
    for git in git_dirs(root) {
        let Ok(worktrees) = std::fs::read_dir(git.join("worktrees")) else {
            continue;
        };
        for worktree in worktrees.flatten() {
            let record = worktree.path().join("gitdir");
            let Ok(text) = std::fs::read_to_string(&record) else {
                continue;
            };
            let named = PathBuf::from(text.trim_end());
            let Some(moved) = rebased(&named, olds, new) else {
                continue;
            };
            if let Err(e) = std::fs::write(&record, format!("{}\n", moved.display())) {
                failed.push(format!("{} could not be rewritten ({e})", record.display()));
                continue;
            }
            // The worktree's own `.git` file, now in the new place, names the record back.
            let Ok(text) = std::fs::read_to_string(&moved) else {
                continue;
            };
            let Some(back) = text.trim_end().strip_prefix("gitdir: ") else {
                continue;
            };
            if let Some(back) = rebased(Path::new(back), olds, new)
                && let Err(e) = std::fs::write(&moved, format!("gitdir: {}\n", back.display()))
            {
                failed.push(format!("{} could not be rewritten ({e})", moved.display()));
            }
        }
    }
    failed
}

/// The record of the chats that were open, with each folder in the old place named in the new.
fn reopen_record(root: &Path, olds: &[PathBuf], new: &Path) -> std::io::Result<()> {
    // A record this purlis cannot use is read as nothing open anyway: nothing to point.
    let Ok(Some(mut record)) = crate::reopen::read_strictly(root) else {
        return Ok(());
    };
    let mut changed = false;
    for chat in &mut record.chats {
        if let Some(moved) = chat.cwd.as_deref().and_then(|cwd| rebased(cwd, olds, new)) {
            chat.cwd = Some(moved);
            changed = true;
        }
    }
    if changed {
        crate::reopen::write(root, &record)?;
    }
    Ok(())
}

/// This machine's remembered projects and windows, with the old place named as the new one.
/// Its approval and its pin go with it: the project is the same files, moved by purlis.
fn machine_store(config_root: &Path, olds: &[PathBuf], new: &Path) -> std::io::Result<()> {
    let store = crate::machine::read(config_root);
    // A store that cannot be read may name the old place: the pointer stays until it can.
    if let Some(why) = store.unreadable {
        return Err(std::io::Error::other(why));
    }
    let names_it = store
        .store
        .recents
        .iter()
        .map(|recent| &recent.plane)
        .chain(store.store.windows.iter().flat_map(|window| &window.planes))
        .any(|plane| rebased(plane, olds, new).is_some());
    if !names_it {
        return Ok(());
    }
    crate::machine::update(config_root, |store| {
        for recent in &mut store.recents {
            if let Some(moved) = rebased(&recent.plane, olds, new) {
                recent.plane = moved;
            }
        }
        for window in &mut store.windows {
            for plane in &mut window.planes {
                if let Some(moved) = rebased(plane, olds, new) {
                    *plane = moved;
                }
            }
        }
    })
    .map(|_| ())
}

/// Whether a purlis window holds the project at `at`, or a process of this user works in it;
/// fail closed: a process list that cannot be read reads as in use.
pub fn in_use(at: &Path) -> Option<String> {
    if let Some(socket) = crate::renamelocal::busy::beside(at)
        .into_iter()
        .find(|socket| crate::renamelocal::busy::answers(socket))
    {
        return Some(format!(
            "a purlis window has it open ({} answers)",
            socket.display()
        ));
    }
    let real = at.canonicalize().unwrap_or_else(|_| at.to_path_buf());
    match working_folders() {
        Ok(found) => found
            .into_iter()
            .find(|(_, cwd)| cwd.starts_with(&real) || cwd.starts_with(at))
            .map(|(what, _)| format!("{what} works in it")),
        Err(why) => Some(format!(
            "purlis could not tell whether anything works in it ({why})"
        )),
    }
}

/// Every other process of this user, by its pid, and the folder it works in.
#[cfg(target_os = "linux")]
fn working_folders() -> Result<Vec<(String, PathBuf)>, String> {
    let me = std::process::id();
    let uid = rustix::process::getuid().as_raw();
    let entries =
        std::fs::read_dir("/proc").map_err(|e| format!("/proc could not be read ({e})"))?;
    Ok(entries
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            if pid == me {
                return None;
            }
            let owner = std::os::unix::fs::MetadataExt::uid(&entry.metadata().ok()?);
            if owner != uid {
                return None;
            }
            let cwd = std::fs::read_link(entry.path().join("cwd")).ok()?;
            Some((format!("process {pid}"), cwd))
        })
        .collect())
}

/// Every other process of this user, by its pid, and the folder it works in: `lsof`'s answer.
#[cfg(target_os = "macos")]
fn working_folders() -> Result<Vec<(String, PathBuf)>, String> {
    let me = std::process::id().to_string();
    let uid = rustix::process::getuid().as_raw().to_string();
    let mut lsof = std::process::Command::new("/usr/sbin/lsof");
    lsof.args(["-nP", "-a", "-d", "cwd", "-u", &uid, "-F", "pn"]);
    let out = crate::forklock::output(&mut lsof)
        .map_err(|e| format!("the process list could not be read ({e})"))?;
    Ok(lsof_cwds(&String::from_utf8_lossy(&out.stdout), &me))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn working_folders() -> Result<Vec<(String, PathBuf)>, String> {
    Err("this system's process list is not read".to_owned())
}

/// `lsof -F pn`'s answer, without the process `me`: a `p<pid>` line, then its `n<folder>`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn lsof_cwds(listing: &str, me: &str) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut pid: Option<&str> = None;
    for line in listing.lines() {
        if let Some(found) = line.strip_prefix('p') {
            pid = Some(found);
        } else if let (Some(folder), Some(pid)) = (line.strip_prefix('n'), pid)
            && pid != me
        {
            out.push((format!("process {pid}"), PathBuf::from(folder)));
        }
    }
    out
}

/// [`run`] on this machine, with the real file system, at the app's launch: `None` when there
/// was nothing to move, else what came of it, for the app's log and its doctor.
pub fn at_launch() -> Option<Moved> {
    let config_root = crate::machine::config_root()?;
    let data_home = crate::datahome::root()?;
    let moved = run(
        &Places {
            config_root,
            data_home,
        },
        &Seams {
            in_use: &in_use,
            rename: &|from, to| std::fs::rename(from, to),
        },
    );
    (moved != Moved::Nothing).then_some(moved)
}

/// Why the project at `root` is the local project still in the config home, for the doctor's
/// row; `None` for any other project.
pub fn still_in_the_config_home(root: &Path, config_root: &Path) -> Option<String> {
    let old = crate::machine::dir(config_root).join(OLD_NAME);
    let is_dir = std::fs::symlink_metadata(&old).is_ok_and(|meta| meta.is_dir());
    let same = old.canonicalize().ok()? == root.canonicalize().ok()?;
    (is_dir && same).then(|| {
        format!(
            "this project is in purlis's config home ({}), which a sandboxed chat may not \
             write, so no sandboxed chat starts here",
            old.display()
        )
    })
}

#[cfg(test)]
#[path = "localproject_tests.rs"]
mod tests;
