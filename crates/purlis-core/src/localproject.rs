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
//! - **A journal says a move was started** (`<config>/purlis/local-project-move`), written before
//!   the rename and taken away after the pointer, holding the folder's identity (its device and
//!   inode, which a rename keeps). A launch that died between the rename and the pointer finds
//!   the old place empty and the journal there: it puts the pointer back only when the folder in
//!   the new place is that same folder, and finishes. The approval and pin of the old place go to
//!   the new one only through the pointer or the journal, both in the config home a chat may not
//!   write, so they go only with the same files.
//! - **One launch at a time**: the move holds a lock beside the machine store for its whole
//!   length, so two launches never move or rewrite at once.
//! - **Nothing a chat wrote is followed.** The project's files are a chat's to write, so every
//!   file the move reads or rewrites in it is reached through no link and no `..`
//!   ([`crate::contain::no_link_on_the_way`]) and replaced whole ([`crate::rewrite::replace`]).
//!   A worktree outside the project, or a clone outside it whose worktree is inside, is never
//!   written: the pointer is kept and the log names the `git worktree repair` to run.
//!
//! # A chat's harness conversation
//!
//! A harness that keeps its conversations under the folder a chat ran in (Claude Code keeps
//! them in `~/.claude/projects/<the folder, spelled with dashes>/`) does not find a conversation
//! from the old place when the chat resumes in the new one, since the folder has another name.
//! Nothing is deleted: the conversation's file stays where the harness wrote it. A resume that
//! is not found starts the chat fresh with its session record in its briefing, as any lost
//! conversation does ([`crate::sessionresume::NotResumed::HarnessLostIt`]). Keeping the pointer
//! would not help: a chat started at the old spelling runs in the folder the system resolves,
//! the new one, and the old spelling is inside the config home a sandbox refuses.
//!
//! # What is never moved
//!
//! - A project something works in: a purlis window holding it, or any process of this user whose
//!   working folder is inside it. It is left where it is, and the launch says so once
//!   ([`Moved::Left`]). It moves at a later launch, once nothing works in it.
//! - A project whose new place is taken: two folders are never merged.
//! - A project whose new place would be inside a repository, a project, or the config home.

use std::path::{Path, PathBuf};

use crate::rewrite::Mode;

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

    /// The journal of a move that was started ([`MOVE_JOURNAL`]).
    fn journal(&self) -> PathBuf {
        crate::machine::dir(&self.config_root).join(MOVE_JOURNAL)
    }

    /// The new place as the system resolves it, else as written.
    fn new_real(&self) -> PathBuf {
        let new = self.current();
        new.canonicalize().unwrap_or(new)
    }
}

/// The journal of a move started and not yet finished, beside the machine store: the moved
/// folder's identity, so a launch that died before the pointer was made puts it back only for
/// the same folder.
const MOVE_JOURNAL: &str = "local-project-move";

/// The lock one launch holds for the whole of a move, beside the machine store.
const MOVE_LOCK: &str = "local-project-move.lock";

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
    // Held for the whole move: a second launch waits, then finds it moved or finishes it.
    let _one_at_a_time = crate::machine::Lock::named(&places.config_root, MOVE_LOCK);
    let old = places.old();
    let new = places.current();
    let olds = places.old_spellings();
    match std::fs::symlink_metadata(&old) {
        Err(_) => return after_a_rename(places, &olds),
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
    // The folder is still in the old place, so a journal there is of a move whose rename was
    // never made.
    let _ = std::fs::remove_file(places.journal());
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
    if let Some(why) = in_the_config_home(places, &new) {
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
    let Some(id) = folder_id(&old) else {
        return left("its folder's identity could not be read".to_owned());
    };
    if let Err(e) = write_journal(places, id) {
        return left(format!("the move's journal could not be written ({e})"));
    }
    if let Err(e) = (seams.rename)(&old, &new) {
        let _ = std::fs::remove_file(places.journal());
        return left(format!("it could not be moved to {} ({e})", new.display()));
    }
    if let Err(e) = link(&new, &old) {
        // Without the pointer, a record still naming the old place would find nothing: the
        // rename is put back, so the project is exactly where it was.
        return match (seams.rename)(&new, &old) {
            Ok(()) => {
                let _ = std::fs::remove_file(places.journal());
                left(format!(
                    "no link could be left at the old place ({e}), so the move was put back"
                ))
            }
            // The journal stays: the next launch puts the pointer back and finishes.
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

/// Why `new` is in purlis's folder in the config home, which a chat's sandbox denies whole: a
/// data home set there would move the project from one denied place to another.
fn in_the_config_home(places: &Places, new: &Path) -> Option<String> {
    let config = crate::machine::dir(&places.config_root);
    let config = config.canonicalize().unwrap_or(config);
    let parent = new.parent()?;
    let resolved = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    resolved.starts_with(&config).then(|| {
        format!(
            "{} is in purlis's config home, where a sandboxed chat cannot start either",
            new.display()
        )
    })
}

/// The old place is empty: nothing to move, unless a launch died between the rename and the
/// pointer. Then the journal is there, and the folder in the new place is the one it names: the
/// pointer is put back and the move finished. Anything else is not this move's.
fn after_a_rename(places: &Places, olds: &[PathBuf]) -> Moved {
    let journal = places.journal();
    let Ok(text) = std::fs::read_to_string(&journal) else {
        return Moved::Nothing;
    };
    let new = places.current();
    let same = std::fs::symlink_metadata(&new).is_ok_and(|meta| meta.is_dir())
        && folder_id(&new).is_some_and(|id| text.trim() == journal_line(id));
    if !same {
        let _ = std::fs::remove_file(&journal);
        return Moved::Nothing;
    }
    if let Err(e) = link(&new, &places.old()) {
        return Moved::PointerKept {
            from: places.old(),
            to: new.clone(),
            why: format!(
                "the link at the old place could not be put back ({e}); the project is whole at \
                 {}",
                new.display()
            ),
        };
    }
    finish(places, olds)
}

/// A folder's device and inode, which a rename on one file system keeps.
#[cfg(unix)]
fn folder_id(at: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(at).ok()?;
    meta.is_dir().then(|| (meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn folder_id(_at: &Path) -> Option<(u64, u64)> {
    None
}

fn journal_line((dev, ino): (u64, u64)) -> String {
    format!("{dev} {ino}")
}

fn write_journal(places: &Places, id: (u64, u64)) -> std::io::Result<()> {
    let journal = places.journal();
    let dir = journal
        .parent()
        .expect("the journal is in purlis's folder")
        .to_path_buf();
    crate::rewrite::replace(
        &dir,
        &journal,
        format!("{}\n", journal_line(id)).as_bytes(),
        crate::rewrite::Mode::Private,
    )
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

#[cfg(not(unix))]
fn link(_target: &Path, _at: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("links are made only on unix"))
}

/// Point what names the old place at the new one, then take the pointer away. Every step is
/// safe to make again, so a launch that died in the middle is finished by the next.
fn finish(places: &Places, olds: &[PathBuf]) -> Moved {
    let from = places.old();
    let to = places.current();
    let real = places.new_real();
    let mut failed: Vec<String> = Vec::new();
    if !to.is_dir() {
        failed.push(format!("{} is not there", to.display()));
    } else {
        failed.extend(git_links(&real, olds));
    }
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
        && e.kind() != std::io::ErrorKind::NotFound
    {
        failed.push(format!(
            "the link at the old place could not be taken away ({e})"
        ));
    }
    if failed.is_empty() {
        // Best effort: a journal left behind makes the next launch finish again, which changes
        // nothing.
        let _ = std::fs::remove_file(places.journal());
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
/// none of them, or walks up with a `..` below it (never a path git or purlis writes).
fn rebased(path: &Path, olds: &[PathBuf], new: &Path) -> Option<PathBuf> {
    olds.iter().find_map(|old| {
        let rest = path.strip_prefix(old).ok()?;
        if rest
            .components()
            .any(|step| !matches!(step, std::path::Component::Normal(_)))
        {
            return None;
        }
        Some(if rest.as_os_str().is_empty() {
            new.to_path_buf()
        } else {
            new.join(rest)
        })
    })
}

/// How deep below the project a clone's `.git` is looked for: `workspaces/<ws>/<repo>/.git` is
/// four, a worktree under `workspaces/<ws>/.worktrees/<repo>/<name>/.git` six.
const CLONE_DEPTH: usize = 6;

/// The largest git link file the move reads: one path and a prefix.
const LINK_FILE_MAX: u64 = 64 * 1024;

/// Every `.git` in `root`, to [`CLONE_DEPTH`], never through a link: the folders (a clone) and
/// the files (a worktree or a submodule).
fn git_entries(root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let (mut folders, mut files) = (Vec::new(), Vec::new());
    let mut todo = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            // Of the entry itself: a link is neither a folder nor a file here.
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if entry.file_name() == ".git" {
                if kind.is_dir() {
                    folders.push(path);
                } else if kind.is_file() {
                    files.push(path);
                }
            } else if kind.is_dir() && depth + 1 < CLONE_DEPTH {
                todo.push((path, depth + 1));
            }
        }
    }
    (folders, files)
}

/// The file at `path` in `root`, reached through no link and no `..`, when it is a plain file
/// of a link's size.
fn read_inside(root: &Path, path: &Path) -> Option<String> {
    crate::contain::no_link_on_the_way(root, path).ok()?;
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > LINK_FILE_MAX {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// A file outside the project, read and never written, when it is a plain file of a link's
/// size.
fn read_outside(path: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > LINK_FILE_MAX {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// Whether `path` is in the project, in the old place or the new one.
fn in_the_project(path: &Path, olds: &[PathBuf], new: &Path) -> bool {
    path.starts_with(new) || olds.iter().any(|old| path.starts_with(old))
}

/// The two links between each repository in the project at `root` (as it resolves) and its
/// linked worktrees, pointed at the new place (what `git worktree repair` writes): each
/// worktree's `gitdir` record in the repository, and the `.git` file in the worktree. Each is
/// rewritten only where it still names the old place, so a second run changes nothing. A
/// worktree outside the project, and a clone outside it with a worktree inside, are never
/// written: each still naming the old place is said, and keeps the pointer. What could not be
/// pointed, in words.
fn git_links(root: &Path, olds: &[PathBuf]) -> Vec<String> {
    let mut failed = Vec::new();
    let (folders, files) = git_entries(root);
    for git in folders {
        let worktrees = git.join("worktrees");
        if crate::contain::no_link_on_the_way(root, &worktrees).is_err() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&worktrees) else {
            continue;
        };
        for worktree in entries.flatten() {
            if !worktree.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let record = worktree.path().join("gitdir");
            let Some(text) = read_inside(root, &record) else {
                continue;
            };
            let named = PathBuf::from(text.trim_end());
            let at = match rebased(&named, olds, root) {
                Some(moved) => {
                    let line = format!("{}\n", moved.display());
                    if let Err(e) =
                        crate::rewrite::replace(root, &record, line.as_bytes(), Mode::Kept)
                    {
                        failed.push(format!("{} could not be rewritten ({e})", record.display()));
                        continue;
                    }
                    moved
                }
                None => named,
            };
            if let Some(why) = worktree_back(root, olds, &at) {
                failed.push(why);
            }
        }
    }
    // A worktree in the project of a clone outside it: that clone's record names this one.
    for file in files {
        let Some(text) = read_inside(root, &file) else {
            continue;
        };
        let Some(admin) = text.trim_end().strip_prefix("gitdir: ").map(PathBuf::from) else {
            continue;
        };
        if !admin.is_absolute() || in_the_project(&admin, olds, root) {
            continue;
        }
        let names_old = read_outside(&admin.join("gitdir"))
            .is_some_and(|back| rebased(Path::new(back.trim_end()), olds, root).is_some());
        if names_old {
            failed.push(format!(
                "the clone whose worktree record is {} still names its worktree at the old \
                 place; run `git worktree repair {}` in that clone",
                admin.display(),
                file.parent().unwrap_or(root).display()
            ));
        }
    }
    failed
}

/// The worktree's own `.git` file at `at`, naming its record back, pointed at the new place
/// when it is in the project; said when it is outside and still names the old place.
fn worktree_back(root: &Path, olds: &[PathBuf], at: &Path) -> Option<String> {
    if !at.starts_with(root) {
        let names_old = read_outside(at).is_some_and(|text| {
            text.trim_end()
                .strip_prefix("gitdir: ")
                .is_some_and(|back| rebased(Path::new(back), olds, root).is_some())
        });
        return names_old.then(|| {
            format!(
                "the worktree at {} is outside the project and still names it at the old \
                 place; run `git worktree repair` in it",
                at.parent().unwrap_or(at).display()
            )
        });
    }
    let text = read_inside(root, at)?;
    let back = text.trim_end().strip_prefix("gitdir: ")?;
    let moved = rebased(Path::new(back), olds, root)?;
    let line = format!("gitdir: {}\n", moved.display());
    crate::rewrite::replace(root, at, line.as_bytes(), Mode::Kept)
        .err()
        .map(|e| format!("{} could not be rewritten ({e})", at.display()))
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
/// fail closed: a process list that cannot be read, or that does not show this process, reads
/// as in use, and so does any purlis answering in the fallback folders, which are not named by
/// the project they serve.
pub fn in_use(at: &Path) -> Option<String> {
    let bases = [
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        Some(std::env::temp_dir()),
    ];
    in_use_with(at, &bases.into_iter().flatten().collect::<Vec<_>>())
}

/// [`in_use`], with the fallback folders a window's socket may be in handed in.
fn in_use_with(at: &Path, fallbacks: &[PathBuf]) -> Option<String> {
    if let Some(socket) = crate::renamelocal::busy::beside(at)
        .into_iter()
        .find(|socket| crate::renamelocal::busy::answers(socket))
    {
        return Some(format!(
            "a purlis window has it open ({} answers)",
            socket.display()
        ));
    }
    // A window whose project's path is too long for a socket answers in a fallback folder,
    // which does not say which project it holds: any one answering may hold this one.
    if let Some(socket) = fallbacks
        .iter()
        .flat_map(|base| crate::renamelocal::busy::in_fallback(base))
        .find(|socket| crate::renamelocal::busy::answers(socket))
    {
        return Some(format!(
            "a running purlis answers on {}, and may have it open",
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
    if std::fs::symlink_metadata(format!("/proc/{me}/cwd")).is_err() {
        return Err("/proc does not show this process".to_owned());
    }
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
    if !out.status.success() {
        return Err(format!(
            "the process list could not be read (lsof ended with {})",
            out.status
        ));
    }
    lsof_cwds(&String::from_utf8_lossy(&out.stdout), &me)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn working_folders() -> Result<Vec<(String, PathBuf)>, String> {
    Err("this system's process list is not read".to_owned())
}

/// `lsof -F pn`'s answer, without the process `me`: a `p<pid>` line, then its `n<folder>`. A
/// listing that does not show `me` shows nothing reliably, and is refused.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn lsof_cwds(listing: &str, me: &str) -> Result<Vec<(String, PathBuf)>, String> {
    let mut out = Vec::new();
    let mut pid: Option<&str> = None;
    let mut shows_me = false;
    for line in listing.lines() {
        if let Some(found) = line.strip_prefix('p') {
            pid = Some(found);
            shows_me |= found == me;
        } else if let (Some(folder), Some(pid)) = (line.strip_prefix('n'), pid)
            && pid != me
        {
            out.push((format!("process {pid}"), PathBuf::from(folder)));
        }
    }
    if !shows_me {
        return Err(
            "the process list could not be read (it does not show this process)".to_owned(),
        );
    }
    Ok(out)
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
