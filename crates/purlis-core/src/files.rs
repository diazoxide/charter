//! A branch's files: its tree, one folder at a time, and one file of it (RC-5, FM-1; ADR 0081
//! §1, ADR 0084 §3). The file manager's deep module (#1103): the explorer's tree and the light
//! editor both read a branch through here, and nothing else in the window reads one.
//!
//! **The window names a branch, never a directory.** It asks by workspace, repo and piece — or
//! by workspace and repo alone for the repo's own folder, its clone (#948) — the names
//! [`worktree::list`] and the workspace's listing answer with, and this module finds the
//! folder: so whatever can reach the command cannot point it at a directory of its choosing. A
//! file or a folder is named by its path relative to that folder, and is read only while it
//! stays inside it.
//!
//! **Nothing the repo's configuration names runs.** Every branch is a folder an agent can write
//! (ADR 0084 §2), and every git call here goes through `worktree::git`'s hardened runner, which
//! turns off the fsmonitor and hooks on the command line.
//!
//! **What opens is what a review can show**: a file git tracks, or one it does not track and
//! does not ignore. The tree names an ignored file, so the operator can see that a build output
//! exists, but marks it ignored, and [`open`] refuses it.
//!
//! **Read only.** Nothing here writes.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::worktree::{self, git};
use crate::youreditor::{self, Editor, Launch, NotLaunched};

mod find;
pub use find::{Finder, Found, Hit, Named, Place, branches, find};
// What a branch changed (FM-4): its own file, so the calls other tickets add here stay apart.
mod status;
pub use status::{AheadBehind, Change, MARKED, Mark, Rolled, Status};
// Where to listen for those changes (FM-4).
mod watch;
pub use watch::{ASKED, KNOWN, Root, root};
// Both read in a bounded child of charter's own binary (FM-4, D-88h).
mod reader;
pub use reader::{
    AT_ONCE, Answer, Ask, GRACE, MEMORY, OUTPUT, READ_ARG, READ_FAILED, Reader, ahead_behind,
    serve_if_asked, status, was_busy,
};
// The one diff engine (RC-2): every comparison, read in the same bounded child.
mod compare;
pub use compare::{
    COUNTED, Compared, Comparison, FileChange, FileDiff, Head, Hunk, Lines, MemberCompared, Shown,
    Sides, WhatChanged, compare, compare_change, compare_file, what_changed,
};
mod search;
pub use search::{
    BadQuery, Ended, FILE_LINES, FILL, FileHits, HitLine, LINE_CHARS, LONGEST_LINE, LONGEST_QUERY,
    PAGE_TIME, Part, Search, SearchOptions, Searched, search,
};

/// The largest file the preview draws, in bytes: 2 MiB (V86 F4, FM-2).
///
/// A file past it is answered with its size, and the window offers your editor instead (ADR
/// 0081 §3). The preview is for reading; a log or a generated bundle that size is not read line
/// by line, and holding it would cost the web content process (ADR 0086 row M2) for nothing
/// anyone reads.
///
/// **It bounds an image's bytes, not what they decode to**: a quarter-megabyte PNG can declare a
/// canvas of gigabytes. That is [`MOST_PIXELS`]'s to bound.
pub const LARGEST: u64 = 2 * 1024 * 1024;

/// The most pixels an image the preview draws may declare: 40 megapixels (FM-2 review).
///
/// The window decodes an image to four bytes a pixel, and draws it on a canvas of the same size,
/// so this keeps one preview to some 320 MB of the web content process at worst — a phone
/// camera's photo still fits. Past it the image is said by its size ([`Opened::HugeImage`]).
/// The size is read from the image's header, never by decoding it.
pub const MOST_PIXELS: u64 = 40_000_000;

/// How much of a file is looked at to call it binary: git's own heuristic, a NUL byte in the
/// first 8,000 bytes (`buffer_is_binary` in git's `xdiff-interface.c`).
const SNIFF: usize = 8000;

/// The most entries one folder level answers with, after sorting: a folder of a hundred thousand
/// generated files (`target/debug/deps`, `node_modules/.pnpm`) is drawn as its first 5,000 and
/// a count of the rest, never as a tree too long to draw.
pub const SHOWN: usize = 5000;

/// The most folders [`folders`] resolves in one call: what one window can have open and
/// watched at once.
pub const WATCHED: usize = 256;

/// Which branch's files: a piece of a repo, or — with no piece — the repo's own folder, its
/// clone (#948). Named, never given as a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Branch<'a> {
    pub ws: &'a str,
    pub repo: &'a str,
    pub piece: Option<&'a str>,
}

impl<'a> Branch<'a> {
    /// A piece of `repo`: a folder of its own under the workspace's worktree root.
    pub fn piece(ws: &'a str, repo: &'a str, piece: &'a str) -> Self {
        Self {
            ws,
            repo,
            piece: Some(piece),
        }
    }

    /// The repo's own folder in the workspace, whichever branch it has checked out.
    pub fn repo(ws: &'a str, repo: &'a str) -> Self {
        Self {
            ws,
            repo,
            piece: None,
        }
    }

    /// What the branch is called in a sentence: its folder's name.
    fn called(&self) -> &'a str {
        self.piece.unwrap_or(self.repo)
    }
}

/// What opening a file of a branch found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// Text, to draw. Bytes that are not UTF-8 are drawn as U+FFFD: the light editor only reads
    /// here, so nothing is written back through the replacement.
    Text { text: String },
    /// An image, known by its first bytes and never by its name: its media type and its bytes
    /// (FM-2). PNG, JPEG, GIF and WebP — the raster kinds a web view decodes. An SVG is text,
    /// and opens as text.
    Image { mime: &'static str, data: Vec<u8> },
    /// An image whose header declares more than [`MOST_PIXELS`]: its type and declared size,
    /// and none of its bytes.
    HugeImage {
        mime: &'static str,
        width: u32,
        height: u32,
    },
    /// A file git would call binary, by its size. Drawn as a sentence, never as text.
    Binary { bytes: u64 },
    /// A file past [`LARGEST`], by its size.
    TooLarge { bytes: u64 },
}

/// What one entry of a folder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folder,
    File,
    /// A symbolic link, never followed by the tree. It opens only when it resolves to another
    /// file the branch offers.
    Link,
}

/// One entry of a folder of a branch, as the tree draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its name in the folder, not its path.
    pub name: String,
    pub kind: Kind,
    /// Whether git ignores it — or it is git's own `.git`. Drawn dimmed, and only when the
    /// operator asks for ignored files.
    pub ignored: bool,
    /// Why it does not open, as the sentence the window shows; `None` for a file or link that
    /// opens and for a folder that expands. An ignored entry carries its reason too.
    pub refused: Option<String>,
}

/// Why a branch or one of its files did not open, as the sentence the window shows.
///
/// Every sentence names a branch's **folder** and never a piece, a worktree or a clone (ADR
/// 0072 §3/§4, CONTEXT.md): these reach the window and nothing else.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("{}", .0.in_window())]
    Piece(#[from] worktree::Refusal),
    #[error("{repo} in workspace '{ws}' has no branch folder called '{piece}'")]
    NoSuchPiece {
        ws: String,
        repo: String,
        piece: String,
    },
    #[error("'{0}' is not a path inside the branch's folder")]
    NotInPiece(String),
    #[error("'{0}' is not in the branch's folder any more")]
    NotThere(String),
    #[error(
        "'{0}' is not one of the branch's files: the light editor opens what git tracks there \
         and what it does not ignore"
    )]
    NotOffered(String),
    #[error("'{0}' is not a file")]
    NotAFile(String),
    /// "Show what changed" for a file the branch did not change (FM-11).
    #[error("'{0}' is not a file this branch changed against its base")]
    NotChanged(String),
    #[error("'{0}' is not a folder")]
    NotAFolder(String),
    /// The branch's own folder is a link (#1143): placed, it would be what the link leads to.
    #[error("the folder of '{0}' is a link, and purlis places no folder a link leads to")]
    FolderIsALink(String),
    #[error(transparent)]
    Editor(#[from] NotLaunched),
    #[error("purlis could not read '{what}': {why}")]
    Unreadable { what: String, why: String },
    /// A read of the branch, in the bounded reader, that failed: its sentence.
    #[error("{0}")]
    Read(String),
}

/// The branch's files: every file git tracks, and every one it does not track and does not
/// ignore, as paths relative to the branch's folder, sorted.
///
/// What `git ls-files` answers, which is what a review of the branch can show: an ignored
/// build output is not offered.
pub fn list(plane: &Path, branch: Branch<'_>) -> Result<Vec<String>, Refused> {
    files_in(&folder_of(plane, branch)?, branch)
}

/// One folder level of a branch: its first [`SHOWN`] entries, and how many more it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub entries: Vec<Entry>,
    /// How many entries past [`SHOWN`] are not in `entries`.
    pub more: usize,
}

/// One folder of the branch, one level deep: what is in it, folders first, each kind sorted
/// the way a person reads names (`2` before `10`), the first [`SHOWN`] of them. `""` is the
/// branch's own folder.
///
/// **Nothing below it is read.** One directory listing and one `git check-ignore` per call, so
/// a branch of a hundred thousand files costs what its top folder holds until a folder is
/// expanded (#1103 story 2).
///
/// **The folder is listed through a descriptor**, opened one component at a time with no link
/// followed, so a folder swapped for a link between the check and the listing is refused rather
/// than listed (on Windows, where there is no such open, its identity is checked again after
/// the listing).
///
/// The folder is refused when it leaves the branch, is reached through a link, or is git's.
/// Each entry says whether git ignores it and, when it does not open, why: git's own `.git`,
/// a link out of the branch or to an ignored file, something that is not a file (a FIFO, a
/// socket), anything inside another repository nested in this one, and an ignored file.
pub fn tree(plane: &Path, branch: Branch<'_>, folder: &str) -> Result<Folder, Refused> {
    tree_in(&base_of(plane, branch)?, branch, folder)
}

/// [`tree`] in a branch whose folder is already resolved to `base`.
fn tree_in(base: &Path, branch: Branch<'_>, folder: &str) -> Result<Folder, Refused> {
    let relative = folder_in(base, folder)?;
    let base = base.to_path_buf();
    let dir = base.join(&relative);
    let unreadable = |e: std::io::Error| Refused::Unreadable {
        what: if folder.is_empty() {
            branch.called().to_string()
        } else {
            folder.to_string()
        },
        why: e.to_string(),
    };
    let mut listed = listing(&base, &relative).map_err(|e| {
        if swapped(&e) {
            // A link where the folder was checked to be: swapped since.
            Refused::NotInPiece(folder.to_string())
        } else if e.kind() == std::io::ErrorKind::NotFound {
            Refused::NotThere(folder.to_string())
        } else {
            unreadable(e)
        }
    })?;
    listed.sort_by(|a, b| {
        (a.kind != Kind::Folder)
            .cmp(&(b.kind != Kind::Folder))
            .then_with(|| natural(&a.name.to_string_lossy(), &b.name.to_string_lossy()))
    });
    let more = listed.len().saturating_sub(SHOWN);
    listed.truncate(SHOWN);

    // A repository nested in this one — a submodule, or a clone inside the branch — is
    // git's business, not this branch's: its folder is drawn, and nothing in it opens. A
    // submodule the index records but that was never checked out has no `.git` on disk, so
    // the index is asked too: git refuses outright any question about a path inside one.
    let mut nested = nested_repo(&dir, &base);
    let mut entries = Vec::with_capacity(listed.len());
    // What git is asked about, from the branch's folder, and which entry each is.
    let mut asked: Vec<(PathBuf, usize)> = Vec::new();
    // Links whose target is asked about, with the target's path.
    let mut targets: Vec<(PathBuf, usize)> = Vec::new();
    for one in listed {
        let at = entries.len();
        let Some(name) = one.name.to_str().map(str::to_owned) else {
            entries.push(Entry {
                name: one.name.to_string_lossy().into_owned(),
                kind: one.kind,
                ignored: false,
                refused: Some("purlis opens only a file whose name is UTF-8".to_string()),
            });
            continue;
        };
        // git's own folder is hidden with what git ignores: in every branch, and never a file
        // of it.
        let gits = name == ".git";
        let mut refused = if gits {
            Some("git's own, not a file of the branch".to_string())
        } else if nested {
            Some("inside another git repository, whose files purlis does not open here".into())
        } else if one.kind == Kind::File && !one.file {
            Some("not a file: only files open here".to_string())
        } else {
            None
        };
        if refused.is_none() {
            asked.push((relative.join(&name), at));
            if one.kind == Kind::Link {
                match link_target(&base, &dir.join(&name)) {
                    Ok(target) => targets.push((target, at)),
                    Err(why) => refused = Some(why),
                }
            }
        }
        entries.push(Entry {
            name,
            kind: one.kind,
            ignored: gits,
            refused,
        });
    }

    // Every folder the answers below depend on: this one and those above it, and those above
    // each link's target. Asked of the index once, for the submodules among them.
    let mut above: Vec<PathBuf> = prefixes(&relative);
    for (target, _) in &targets {
        if let Some(parent) = target.parent() {
            above.extend(prefixes(parent));
        }
    }
    let submodules = gitlinks(&base, &above, branch)?;
    let in_submodule = |path: &Path| prefixes(path).iter().any(|one| submodules.contains(one));
    if !nested && in_submodule(&relative) {
        nested = true;
    }
    if nested {
        for (_, at) in asked.drain(..) {
            entries[at].refused =
                Some("inside another git repository, whose files purlis does not open here".into());
        }
        targets.clear();
    }
    targets.retain(|(target, at)| {
        let inside = target.parent().is_some_and(in_submodule);
        if inside {
            entries[*at].refused =
                Some("a link out of the branch's folder, so purlis does not follow it".into());
        }
        !inside
    });

    let all: Vec<&Path> = asked
        .iter()
        .chain(targets.iter())
        .map(|(path, _)| path.as_path())
        .collect();
    let ignored = ignored_of(&base, &all, branch)?;
    let (of_entries, of_targets) = ignored.split_at(asked.len());
    for ((_, at), ignored) in asked.iter().zip(of_entries) {
        if *ignored {
            let entry = &mut entries[*at];
            entry.ignored = true;
            // A folder is not opened, only listed: an ignored one still expands, so the
            // operator can see a build output exists, and each file in it is refused.
            if entry.kind != Kind::Folder {
                entry.refused = Some("ignored by git, so purlis does not open it".to_string());
            }
        }
    }
    for ((_, at), ignored) in targets.iter().zip(of_targets) {
        let entry = &mut entries[*at];
        if *ignored && entry.refused.is_none() {
            entry.refused = Some("a link to a file git ignores, so purlis does not open it".into());
        }
    }
    Ok(Folder { entries, more })
}

/// Whether a listing failed because a component is no longer the folder that was checked:
/// `ELOOP` where the open met a link, `ENOTDIR` where `O_DIRECTORY` met one first (macOS) or
/// met a file.
#[cfg(unix)]
fn swapped(e: &std::io::Error) -> bool {
    [rustix::io::Errno::LOOP, rustix::io::Errno::NOTDIR]
        .iter()
        .any(|errno| e.raw_os_error() == Some(errno.raw_os_error()))
}

#[cfg(not(unix))]
fn swapped(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::Other
}

/// One entry of a folder as it was listed, before anything is asked of git.
struct Listed {
    name: std::ffi::OsString,
    kind: Kind,
    /// Whether it is a regular file: a FIFO or a socket is listed as a file that does not open.
    file: bool,
}

/// What `relative` holds, listed through a descriptor opened from the branch's folder one
/// component at a time, following no link (`O_NOFOLLOW | O_DIRECTORY`): a component swapped
/// for a link since it was checked fails the open with `ELOOP`.
#[cfg(unix)]
fn listing(base: &Path, relative: &Path) -> std::io::Result<Vec<Listed>> {
    use rustix::fs::{AtFlags, CWD, FileType, Mode, OFlags};
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut fd = rustix::fs::openat(CWD, base, flags, Mode::empty())?;
    for step in relative.components() {
        let Component::Normal(name) = step else {
            continue;
        };
        fd = rustix::fs::openat(&fd, name, flags, Mode::empty())?;
    }
    let mut out = Vec::new();
    for found in rustix::fs::Dir::read_from(&fd)? {
        let found = found?;
        let bytes = found.file_name().to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        let mut kind = found.file_type();
        if kind == FileType::Unknown {
            // Not every file system says in the listing; one that does not is asked.
            let Ok(stat) = rustix::fs::statat(&fd, found.file_name(), AtFlags::SYMLINK_NOFOLLOW)
            else {
                // Gone between the listing and here.
                continue;
            };
            kind = FileType::from_raw_mode(stat.st_mode);
        }
        use std::os::unix::ffi::OsStrExt as _;
        out.push(Listed {
            name: std::ffi::OsStr::from_bytes(bytes).to_os_string(),
            kind: match kind {
                FileType::Symlink => Kind::Link,
                FileType::Directory => Kind::Folder,
                _ => Kind::File,
            },
            file: kind == FileType::RegularFile,
        });
    }
    Ok(out)
}

/// A branch's folder, held open: what [`open_inside`] opens a file relative to, so no folder
/// above the file — the branch's own, or one of its ancestors up to the project — is looked up
/// by path again once it was opened.
#[cfg(unix)]
pub(crate) struct Held(rustix::fd::OwnedFd);

/// Where there is no descriptor to hold: the folder's resolved path, every open checked again.
#[cfg(not(unix))]
pub(crate) struct Held(PathBuf);

/// The folder `folder` names under `root`, opened from `root` one component at a time, following
/// no link (`O_NOFOLLOW | O_DIRECTORY`): an ancestor of the branch's folder swapped for a link —
/// a workspace's `.worktrees/<repo>`, which its chats can write — fails the open with `ELOOP` or
/// `ENOTDIR` rather than leading elsewhere. `root` is the project's resolved root, which the
/// operator chose; `folder` must lie under it.
#[cfg(unix)]
fn hold_folder(root: &Path, folder: &Path) -> std::io::Result<Held> {
    use rustix::fs::{CWD, Mode, OFlags};
    let relative = folder
        .strip_prefix(root)
        .map_err(|_| std::io::Error::other("the branch's folder is not inside the project"))?;
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut fd = rustix::fs::openat(CWD, root, flags, Mode::empty())?;
    for step in relative.components() {
        match step {
            Component::Normal(name) => fd = rustix::fs::openat(&fd, name, flags, Mode::empty())?,
            Component::CurDir => {}
            _ => return Err(std::io::Error::other("not a plain path inside the project")),
        }
    }
    Ok(Held(fd))
}

#[cfg(not(unix))]
fn hold_folder(root: &Path, folder: &Path) -> std::io::Result<Held> {
    let resolved = std::fs::canonicalize(folder)?;
    if !resolved.starts_with(root) || resolved != folder {
        return Err(std::io::Error::other(
            "the branch's folder is not the one checked",
        ));
    }
    Ok(Held(resolved))
}

/// The file `relative` names in the held folder, opened from it one component at a time,
/// following no link (`O_NOFOLLOW`): a folder on the way swapped for a link since the walk saw
/// it fails the open with `ELOOP` or `ENOTDIR` rather than being followed, and so does the file
/// itself. Opened non-blocking, so a FIFO answers at once rather than waiting for a writer; the
/// caller checks it is a regular file.
#[cfg(unix)]
fn open_inside(held: &Held, relative: &Path) -> std::io::Result<std::fs::File> {
    use rustix::fs::{Mode, OFlags};
    let (folder, leaf) = folder_inside(held, relative)?;
    let at = folder.as_ref().unwrap_or(&held.0);
    let file = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let opened = rustix::fs::openat(at, leaf, file, Mode::empty())?;
    Ok(std::fs::File::from(opened))
}

/// The folder holding `relative`'s last component, opened from the held folder one component
/// at a time following no link (`O_NOFOLLOW | O_DIRECTORY`), and that last component: `None`
/// for the held folder itself. What [`open_inside`] and [`link_inside`] both walk.
#[cfg(unix)]
fn folder_inside<'p>(
    held: &Held,
    relative: &'p Path,
) -> std::io::Result<(Option<rustix::fd::OwnedFd>, &'p std::ffi::OsStr)> {
    use rustix::fs::{Mode, OFlags};
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut steps: Vec<&std::ffi::OsStr> = relative
        .components()
        .filter_map(|step| match step {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    let Some(leaf) = steps.pop() else {
        return Err(std::io::Error::other("no file named"));
    };
    let mut fd: Option<rustix::fd::OwnedFd> = None;
    for name in steps {
        let at = fd.as_ref().unwrap_or(&held.0);
        fd = Some(rustix::fs::openat(at, name, flags, Mode::empty())?);
    }
    Ok((fd, leaf))
}

/// The target of the link `relative` names in the held folder, as git stores a link: `None`
/// when it is not a link. The folders on the way are opened one at a time following no link,
/// and the link itself is read, never followed.
#[cfg(unix)]
fn link_inside(held: &Held, relative: &Path) -> Option<Vec<u8>> {
    let (folder, leaf) = folder_inside(held, relative).ok()?;
    let at = folder.as_ref().unwrap_or(&held.0);
    let target = rustix::fs::readlinkat(at, leaf, Vec::new()).ok()?;
    Some(target.into_bytes())
}

/// [`link_inside`] where there is no descriptor to hold: the link read by path, its folders
/// checked to be no link.
#[cfg(not(unix))]
fn link_inside(held: &Held, relative: &Path) -> Option<Vec<u8>> {
    let path = held.0.join(relative);
    let mut at = held.0.clone();
    let mut steps: Vec<_> = relative.components().collect();
    steps.pop()?;
    for step in steps {
        at.push(step);
        if std::fs::symlink_metadata(&at)
            .ok()?
            .file_type()
            .is_symlink()
        {
            return None;
        }
    }
    if !std::fs::symlink_metadata(&path)
        .ok()?
        .file_type()
        .is_symlink()
    {
        return None;
    }
    let target = std::fs::read_link(&path).ok()?;
    Some(target.to_string_lossy().replace('\\', "/").into_bytes())
}

/// [`open_inside`] where there is no open that refuses a link: every component is refused when
/// it is a link, and the open's own last component by `O_NOFOLLOW` where there is one.
#[cfg(not(unix))]
fn open_inside(held: &Held, relative: &Path) -> std::io::Result<std::fs::File> {
    crate::contain::open_no_link(&held.0, &held.0.join(relative))
}

/// [`listing`] where there is no open that refuses a link: listed by path, then refused when
/// the folder is not the one checked any more.
#[cfg(not(unix))]
fn listing(base: &Path, relative: &Path) -> std::io::Result<Vec<Listed>> {
    let dir = base.join(relative);
    let mut out = Vec::new();
    for found in std::fs::read_dir(&dir)? {
        let found = found?;
        let Ok(meta) = std::fs::symlink_metadata(found.path()) else {
            continue;
        };
        out.push(Listed {
            name: found.file_name(),
            kind: if meta.file_type().is_symlink() {
                Kind::Link
            } else if meta.is_dir() {
                Kind::Folder
            } else {
                Kind::File
            },
            file: meta.is_file(),
        });
    }
    if std::fs::canonicalize(&dir).ok().as_deref() != Some(dir.as_path()) {
        return Err(std::io::Error::other(
            "the folder changed while it was listed",
        ));
    }
    Ok(out)
}

/// One folder of the branch, resolved, as [`tree`] reads it: what a watch on it is put on, so
/// the tree hears an agent add or remove a file in it. Refused exactly as [`tree`] refuses it.
pub fn folder(plane: &Path, branch: Branch<'_>, folder: &str) -> Result<PathBuf, Refused> {
    folder_of_in(&base_of(plane, branch)?, folder)
}

/// [`folder`] in a branch whose folder is already resolved to `base`.
fn folder_of_in(base: &Path, folder: &str) -> Result<PathBuf, Refused> {
    folder_in(base, folder).map(|relative| base.join(relative))
}

/// [`folder`] for several folders of ONE branch, finding the branch once: one git call for the
/// lot rather than one per folder. At most [`WATCHED`] are resolved; the rest are answered as
/// refused.
pub fn folders(plane: &Path, branch: Branch<'_>, named: &[&str]) -> Vec<Result<PathBuf, Refused>> {
    let base = match base_of(plane, branch) {
        Ok(base) => base,
        Err(refused) => {
            let said = refused.to_string();
            return named
                .iter()
                .map(|_| {
                    Err(Refused::Unreadable {
                        what: branch.called().to_string(),
                        why: said.clone(),
                    })
                })
                .collect();
        }
    };
    named
        .iter()
        .enumerate()
        .map(|(at, folder)| {
            if at >= WATCHED {
                return Err(Refused::Unreadable {
                    what: (*folder).to_string(),
                    why: format!("purlis watches at most {WATCHED} folders at once"),
                });
            }
            folder_in(&base, folder).map(|relative| base.join(relative))
        })
        .collect()
}

/// The branch's folder, found in this process and resolved.
fn base_of(plane: &Path, branch: Branch<'_>) -> Result<PathBuf, Refused> {
    resolved(&folder_of(plane, branch)?, branch)
}

/// A branch's folder, found, as the disk spells it.
fn resolved(folder: &Path, branch: Branch<'_>) -> Result<PathBuf, Refused> {
    std::fs::canonicalize(folder).map_err(|e| Refused::Unreadable {
        what: branch.called().to_string(),
        why: e.to_string(),
    })
}

/// `folder`'s path relative to the resolved `base` (`""` is the branch's own), once every check
/// has passed: a plain relative path, inside the branch, not git's, a folder, and reached
/// through no link.
fn folder_in(base: &Path, folder: &str) -> Result<PathBuf, Refused> {
    let relative = if folder.is_empty() {
        PathBuf::new()
    } else {
        inside(folder)?.to_path_buf()
    };
    // Git's own folder is refused by its name before the disk is asked, so whether something
    // exists inside it is never said.
    if gits(&relative) {
        return Err(Refused::NotInPiece(folder.to_string()));
    }
    let dir = base.join(&relative);
    // The folder as the disk spells it must be the folder as it was named: a link anywhere on
    // the way resolves elsewhere, and is refused rather than followed.
    match std::fs::canonicalize(&dir) {
        Ok(resolved) if resolved == dir => {}
        Ok(_) => return Err(Refused::NotInPiece(folder.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Refused::NotThere(folder.to_string()));
        }
        Err(e) => {
            return Err(Refused::Unreadable {
                what: folder.to_string(),
                why: e.to_string(),
            });
        }
    }
    if names_git(&dir, base) {
        return Err(Refused::NotInPiece(folder.to_string()));
    }
    if !dir.is_dir() {
        return Err(Refused::NotAFolder(folder.to_string()));
    }
    Ok(relative)
}

/// The git call that lists a branch's offered files: what it tracks, and what it does not
/// track and does not ignore.
const LS_FILES: [&str; 6] = [
    "--no-optional-locks",
    "ls-files",
    "-z",
    "--cached",
    "--others",
    "--exclude-standard",
];

/// [`list`], for a folder already found.
fn files_in(folder: &Path, branch: Branch<'_>) -> Result<Vec<String>, Refused> {
    let listed = git::run(folder, &LS_FILES, git::READ).map_err(worktree::Refusal::from)?;
    offered_of(&listed, branch)
}

/// [`files_in`], called off when `stop` is raised (#1137): `Ok(None)` when it was, before git
/// answered. git can sit out its whole deadline in an open — an ignore file planted as a FIFO —
/// and a search a person stopped must not wait on it.
fn files_in_until(
    folder: &Path,
    branch: Branch<'_>,
    stop: &std::sync::atomic::AtomicBool,
) -> Result<Option<Vec<String>>, Refused> {
    let listed =
        git::run_until(folder, &LS_FILES, git::READ, stop).map_err(worktree::Refusal::from)?;
    if listed.code.is_none() && stop.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(None);
    }
    offered_of(&listed, branch).map(Some)
}

/// The offered list in `listed`, git's answer to [`LS_FILES`], sorted and once each.
fn offered_of(listed: &git::Run, branch: Branch<'_>) -> Result<Vec<String>, Refused> {
    if !listed.ok() {
        return Err(Refused::Unreadable {
            what: format!("the files of {}", branch.called()),
            why: listed.err.trim().to_string(),
        });
    }
    let mut files: Vec<String> = listed
        .out
        .split('\0')
        .filter(|one| !one.is_empty())
        .map(str::to_owned)
        .collect();
    // `--cached` lists a file once per stage while a merge is unresolved.
    files.sort();
    files.dedup();
    Ok(files)
}

/// Whether git ignores each of `asked`, by position: the answer at `i` is the path at `i`'s.
///
/// **Matched by position, never by spelling.** git answers `check-ignore --stdin -z -v -n` with
/// one record per path, in the order asked, each four NUL-ended fields: the file holding the
/// pattern, its line, the pattern, and the path. A path git respells — quoted, or composed
/// differently — is still the answer at its own position. A record with a pattern that does
/// not start with `!` is a path git ignores; an empty pattern is one no rule names, and a `!`
/// one is a rule that un-ignores it.
///
/// `git check-ignore` reports a tracked file as not ignored, whatever its patterns say, which is
/// exactly [`list`]'s line. Each path is handed as `./<path>`, so a name that starts with `:`
/// is never read as a pathspec's magic, and as data on the standard input, never as an
/// argument.
///
/// **A name git would compose differently is asked both ways**, and ignored when either
/// spelling is: git composes the names it lists on a system set to (macOS's
/// `core.precomposeUnicode`), and not what it is handed on its standard input.
fn ignored_of(base: &Path, asked: &[&Path], branch: Branch<'_>) -> Result<Vec<bool>, Refused> {
    use unicode_normalization::UnicodeNormalization as _;
    if asked.is_empty() {
        return Ok(Vec::new());
    }
    // Each question, and which path it is about.
    let mut questions: Vec<(String, usize)> = Vec::with_capacity(asked.len());
    for (at, path) in asked.iter().enumerate() {
        let spelled = dotted(path);
        let composed: String = spelled.nfc().collect();
        if composed != spelled {
            questions.push((composed, at));
        }
        questions.push((spelled, at));
    }
    let mut input = Vec::new();
    for (spelled, _) in &questions {
        input.extend_from_slice(spelled.as_bytes());
        input.push(0);
    }
    let unreadable = |why: String| Refused::Unreadable {
        what: format!("the files of {}", branch.called()),
        why,
    };
    let ran = git::run_with_input(
        base,
        &[
            "--no-optional-locks",
            "check-ignore",
            "--stdin",
            "-z",
            "--verbose",
            "--non-matching",
        ],
        input,
        git::READ,
    )
    .map_err(worktree::Refusal::from)?;
    // 0: some are ignored; 1: none is. Anything else is git refusing.
    if !matches!(ran.code, Some(0 | 1)) {
        return Err(unreadable(
            String::from_utf8_lossy(&ran.err).trim().to_string(),
        ));
    }
    let fields: Vec<&[u8]> = ran.out.split(|byte| *byte == 0).collect();
    // Four fields a record, and the empty one after the last NUL.
    if fields.len() != questions.len() * 4 + 1 {
        return Err(unreadable(format!(
            "git answered {} fields for {} paths",
            fields.len().saturating_sub(1),
            questions.len()
        )));
    }
    let mut ignored = vec![false; asked.len()];
    for ((_, at), record) in questions.iter().zip(fields.chunks(4)) {
        let pattern = record[2];
        if !pattern.is_empty() && pattern[0] != b'!' {
            ignored[*at] = true;
        }
    }
    Ok(ignored)
}

/// Where a link inside the branch leads, as a path relative to the branch's folder — or why it
/// does not open: it leaves the branch, leads into git's own folder or another repository, or
/// leads to something that is not a file.
fn link_target(base: &Path, link: &Path) -> Result<PathBuf, String> {
    let away = || "a link out of the branch's folder, so purlis does not follow it".to_string();
    let Ok(resolved) = std::fs::canonicalize(link) else {
        return Err("a link to nothing".to_string());
    };
    if resolved == base || !resolved.starts_with(base) || names_git(&resolved, base) {
        return Err(away());
    }
    if !resolved.is_file() {
        return Err("a link to something that is not a file".to_string());
    }
    let Some(parent) = resolved.parent() else {
        return Err(away());
    };
    if nested_repo(parent, base) {
        return Err(away());
    }
    let target = resolved
        .strip_prefix(base)
        .map(Path::to_path_buf)
        .map_err(|_| away())?;
    // Asked of git by its name, which must be the name: a lossy spelling would be another one.
    if target.to_str().is_none() {
        return Err("a link to a file whose name is not UTF-8, which purlis does not open".into());
    }
    Ok(target)
}

/// `path` and every folder above it inside the branch, as relative paths: `a/b` is `a`, `a/b`.
fn prefixes(path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut at = PathBuf::new();
    for step in path.components() {
        if let Component::Normal(name) = step {
            at.push(name);
            out.push(at.clone());
        }
    }
    out
}

/// Which of `paths` the index records as a submodule (a gitlink, mode `160000`), checked out or
/// not. The paths are handed literally (`--literal-pathspecs`); git's spelling of each answer
/// is composed before it is compared, as git composes the names it lists.
fn gitlinks(
    base: &Path,
    paths: &[PathBuf],
    branch: Branch<'_>,
) -> Result<std::collections::HashSet<PathBuf>, Refused> {
    use unicode_normalization::UnicodeNormalization as _;
    let mut found = std::collections::HashSet::new();
    let asked: Vec<String> = paths
        .iter()
        .filter_map(|path| path.to_str().map(str::to_owned))
        .collect();
    if asked.is_empty() {
        return Ok(found);
    }
    let mut args = vec![
        "--literal-pathspecs",
        "--no-optional-locks",
        "ls-files",
        "--stage",
        "-z",
        "--",
    ];
    args.extend(asked.iter().map(String::as_str));
    let ran = git::run(base, &args, git::READ).map_err(worktree::Refusal::from)?;
    if !ran.ok() {
        return Err(Refused::Unreadable {
            what: format!("the files of {}", branch.called()),
            why: ran.err.trim().to_string(),
        });
    }
    let composed: std::collections::HashMap<String, &PathBuf> = asked
        .iter()
        .zip(paths.iter().filter(|path| path.to_str().is_some()))
        .map(|(spelled, path)| (spelled.nfc().collect(), path))
        .collect();
    for record in ran.out.split('\0').filter(|one| !one.is_empty()) {
        let Some((meta, path)) = record.split_once('\t') else {
            continue;
        };
        if !meta.starts_with("160000 ") {
            continue;
        }
        let path: String = path.nfc().collect();
        if let Some(asked) = composed.get(&path) {
            found.insert((*asked).clone());
        }
    }
    Ok(found)
}

/// Whether `dir`, or a folder between it and the branch's folder, holds a `.git` of its own:
/// a submodule, or a repository cloned inside the branch.
fn nested_repo(dir: &Path, base: &Path) -> bool {
    let mut at = dir;
    while at != base && at.starts_with(base) {
        if std::fs::symlink_metadata(at.join(".git")).is_ok() {
            return true;
        }
        let Some(up) = at.parent() else {
            return false;
        };
        at = up;
    }
    false
}

/// A relative path as `check-ignore` is asked it: `./` and its components joined by `/`.
fn dotted(relative: &Path) -> String {
    format!("./{}", slashed(relative))
}

/// Two names in the order a person reads them: runs of digits by their value, everything else
/// without regard to case, and the exact spelling only to break a tie.
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut run = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        run.push(c);
                        it.next();
                    }
                    run
                };
                let (m, n) = (take(&mut x), take(&mut y));
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let by_value = m.len().cmp(&n.len()).then_with(|| m.cmp(n));
                if by_value != Ordering::Equal {
                    return by_value;
                }
            }
            (Some(c), Some(d)) => {
                let by_letter = c.to_lowercase().cmp(d.to_lowercase());
                if by_letter != Ordering::Equal {
                    return by_letter;
                }
                x.next();
                y.next();
            }
        }
    }
}

/// One file of the branch, by its path relative to the branch's folder.
///
/// Only a path [`list`] offers opens, and through a link only a file it offers too. Refused as
/// well when the path is empty, absolute, walks up, has a `.git` component, or resolves
/// (through a link) to somewhere outside the branch. A link to another offered file of the same
/// branch opens, as `CLAUDE.md` linked to `AGENTS.md` does in many repos.
pub fn open(plane: &Path, branch: Branch<'_>, path: &str) -> Result<Opened, Refused> {
    open_in(&base_of(plane, branch)?, branch, path)
}

/// [`open`] in a branch whose folder is already resolved to `base`.
fn open_in(base: &Path, branch: Branch<'_>, path: &str) -> Result<Opened, Refused> {
    let resolved = locate(base, branch, path)?;
    // The resolved path has no link on it, so the open refuses one planted since.
    let mut file = crate::contain::open_no_link(base, &resolved).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Refused::NotThere(path.to_string())
        } else {
            Refused::Unreadable {
                what: path.to_string(),
                why: e.to_string(),
            }
        }
    })?;
    let unreadable = |e: std::io::Error| Refused::Unreadable {
        what: path.to_string(),
        why: e.to_string(),
    };
    let meta = file.metadata().map_err(unreadable)?;
    if !meta.is_file() {
        return Err(Refused::NotAFile(path.to_string()));
    }
    let bytes = meta.len();
    if bytes > LARGEST {
        return Ok(Opened::TooLarge { bytes });
    }
    let mut read = Vec::with_capacity(bytes as usize);
    // Bounded, so a file that grew since the size was read is not read past the limit.
    file.by_ref()
        .take(LARGEST + 1)
        .read_to_end(&mut read)
        .map_err(unreadable)?;
    if read.len() as u64 > LARGEST {
        return Ok(Opened::TooLarge {
            bytes: read.len() as u64,
        });
    }
    if let Some(mime) = image_kind(&read) {
        // Its size as its header declares it, without decoding: a header that says none is not
        // an image the window is handed to decode.
        match imagesize::blob_size(&read) {
            Ok(size) if size.width > 0 && size.height > 0 => {
                let pixels = (size.width as u64).saturating_mul(size.height as u64);
                if pixels <= MOST_PIXELS {
                    return Ok(Opened::Image { mime, data: read });
                }
                let side = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
                return Ok(Opened::HugeImage {
                    mime,
                    width: side(size.width),
                    height: side(size.height),
                });
            }
            _ => {
                return Ok(Opened::Binary {
                    bytes: read.len() as u64,
                });
            }
        }
    }
    if read[..read.len().min(SNIFF)].contains(&0) {
        return Ok(Opened::Binary {
            bytes: read.len() as u64,
        });
    }
    Ok(Opened::Text {
        text: String::from_utf8_lossy(&read).into_owned(),
    })
}

/// The image kind `bytes` start as, by the signature each format opens with; `None` for anything
/// else. Only the signature is trusted, never a name: the window decodes what this answers.
fn image_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// A file or folder of a branch, placed on disk for an act outside the window (FM-10): its
/// path copied, revealed in the operating system's file manager, or a shell tab started in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// Its resolved absolute path: the branch's folder as the disk spells it, joined with the
    /// path, which no link is on.
    pub absolute: PathBuf,
    /// Its path relative to the branch's folder, as git's file list spells one.
    pub relative: String,
    /// Whether it is a folder.
    pub folder: bool,
}

/// One file or folder of the branch, by its path relative to the branch's folder, placed on
/// disk (FM-10): what Copy path, Reveal and a shell tab act on, so a path that leaves the
/// window is always one the core resolved.
///
/// **No link is followed**, whether it leads out of the branch or to another of its files: the
/// path as the disk resolves it must be the path as it was named. Refused, too, when the path
/// is empty, absolute, walks up, or is or is inside a `.git`. An ignored file or folder is
/// placed like any other: the tree draws it when asked, and nothing here reads its bytes.
pub fn place(plane: &Path, branch: Branch<'_>, path: &str) -> Result<Placed, Refused> {
    place_in(&base_of(plane, branch)?, path)
}

/// [`place`] in a branch whose folder is already resolved to `base`.
fn place_in(base: &Path, path: &str) -> Result<Placed, Refused> {
    let relative = slashed(inside(path)?);
    // Git's own folder is refused by its name before the disk is asked (see `folder_in`).
    if relative.is_empty() || gits(Path::new(&relative)) {
        return Err(Refused::NotInPiece(path.to_string()));
    }
    let absolute = base.join(&relative);
    match std::fs::canonicalize(&absolute) {
        Ok(resolved) if resolved == absolute => {}
        Ok(_) => return Err(Refused::NotInPiece(path.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Refused::NotThere(path.to_string()));
        }
        Err(e) => {
            return Err(Refused::Unreadable {
                what: path.to_string(),
                why: e.to_string(),
            });
        }
    }
    if names_git(&absolute, base) {
        return Err(Refused::NotInPiece(path.to_string()));
    }
    let folder = absolute.is_dir();
    Ok(Placed {
        absolute,
        relative,
        folder,
    })
}

/// **The branch's own folder**, placed on disk as [`place`] places a folder under it (#1143):
/// what the branch row's Copy absolute path, Reveal and shell tab act on. [`place`] keeps
/// refusing the empty path, so a caller that names a file never gets the whole folder by
/// leaving the name out; this is asked for the folder by name.
///
/// The folder the workspace or git names must be the folder itself: **refused when it is a
/// link**, whatever it leads to, and when it resolves to anything of git's (a `.git` on its
/// path). Answered as the disk spells it, with an empty `relative`: a path relative to the
/// branch's folder means nothing for the folder itself.
pub fn place_branch_folder(plane: &Path, branch: Branch<'_>) -> Result<Placed, Refused> {
    place_named_folder(&folder_of(plane, branch)?, branch)
}

/// [`place_branch_folder`] for the folder `named` as the workspace or git names it.
fn place_named_folder(named: &Path, branch: Branch<'_>) -> Result<Placed, Refused> {
    let said = || branch.called().to_string();
    match std::fs::symlink_metadata(named) {
        Ok(meta) if meta.file_type().is_symlink() => return Err(Refused::FolderIsALink(said())),
        Ok(meta) if !meta.is_dir() => return Err(Refused::NotAFolder(said())),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Refused::NotThere(said()));
        }
        Err(e) => {
            return Err(Refused::Unreadable {
                what: said(),
                why: e.to_string(),
            });
        }
    }
    let absolute = std::fs::canonicalize(named).map_err(|e| Refused::Unreadable {
        what: said(),
        why: e.to_string(),
    })?;
    // A worktree's own `.git` is a file beside its files, never its folder: a folder that
    // resolves into a `.git` is git's, the clone's git directory or inside it.
    if gits(&absolute) {
        return Err(Refused::NotInPiece(said()));
    }
    Ok(Placed {
        absolute,
        relative: String::new(),
        folder: true,
    })
}

/// One file, folder or link of the branch by its path relative to the branch's folder, said as
/// git's file list spells one (FM-10): what Copy relative path copies.
///
/// **Nothing is followed, the leaf included**: its folder is resolved as [`folder`] resolves one
/// — inside the branch, through no link, not git's — and the leaf itself is only looked at
/// (`symlink_metadata`), so a link's row copies the link's own path. Refused, as [`place`]
/// refuses, for an empty, absolute or walking-up path, and for git's own folder.
pub fn named(plane: &Path, branch: Branch<'_>, path: &str) -> Result<String, Refused> {
    spelled_inside(path)?;
    named_in(&base_of(plane, branch)?, path)
}

/// `path` as git's file list spells it, refused as [`named`] refuses a path by its spelling
/// alone — empty, absolute, walking up, or naming git's folder — before any disk is looked at.
pub(crate) fn spelled_inside(path: &str) -> Result<String, Refused> {
    let relative = inside(path)?;
    let spelled = slashed(relative);
    if spelled.is_empty() || gits(relative) {
        return Err(Refused::NotInPiece(path.to_string()));
    }
    Ok(spelled)
}

/// [`named`] in a branch whose folder is already resolved to `base`: by the app's process
/// through [`base_of`], or by the reader's child through the status read's own opening (#1189).
pub(crate) fn named_in(base: &Path, path: &str) -> Result<String, Refused> {
    let spelled = spelled_inside(path)?;
    let (parent, leaf) = match spelled.rsplit_once('/') {
        Some((parent, leaf)) => (parent, leaf),
        None => ("", spelled.as_str()),
    };
    let folder = folder_in(base, parent).map_err(|refused| match refused {
        Refused::NotInPiece(_) => Refused::NotInPiece(path.to_string()),
        Refused::NotThere(_) | Refused::NotAFolder(_) => Refused::NotThere(path.to_string()),
        other => other,
    })?;
    match std::fs::symlink_metadata(base.join(folder).join(leaf)) {
        Ok(_) => Ok(spelled),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(Refused::NotThere(path.to_string()))
        }
        Err(e) => Err(Refused::Unreadable {
            what: path.to_string(),
            why: e.to_string(),
        }),
    }
}

/// One file of the branch, handed to your editor at `line` (RC-20, ADR 0081 §3): what the app
/// launches, a URL in the editor's own scheme or `$VISUAL`/`$EDITOR` as a program with its
/// arguments ([`youreditor::launch`]).
///
/// **The path is checked exactly as [`open`] checks it**, so what the light editor would refuse
/// is never handed on either: only a file the list offers, inside the branch, through no link
/// that leaves it. The editor is given the file's resolved, absolute path. A file past the
/// light editor's size is handed on like any other: it is what your editor is for.
pub fn in_your_editor(
    plane: &Path,
    branch: Branch<'_>,
    path: &str,
    line: u32,
    editor: Editor,
    var: &dyn Fn(&str) -> Option<String>,
) -> Result<Launch, Refused> {
    in_your_editor_in(&base_of(plane, branch)?, branch, path, line, editor, var)
}

/// [`in_your_editor`] in a branch whose folder is already resolved to `base`.
fn in_your_editor_in(
    base: &Path,
    branch: Branch<'_>,
    path: &str,
    line: u32,
    editor: Editor,
    var: &dyn Fn(&str) -> Option<String>,
) -> Result<Launch, Refused> {
    let resolved = locate(base, branch, path)?;
    // A submodule is offered by its path and is a folder; a FIFO is no file to edit.
    if !resolved.is_file() {
        return Err(Refused::NotAFile(path.to_string()));
    }
    Ok(youreditor::launch(editor, &resolved, line, var)?)
}

/// The resolved file `path` names in the branch whose folder is resolved to `base`, once every
/// check has passed: a plain relative path, offered by the list, inside the branch, not git's,
/// and through a link only to another offered file.
fn locate(base: &Path, branch: Branch<'_>, path: &str) -> Result<PathBuf, Refused> {
    let relative = inside(path)?;
    // **Only what the list offers opens** (ADR 0084 §2, ADR 0052): a file git tracks, or one
    // it does not track and does not ignore. An ignored `.env`, or a secret materialised into
    // the branch, is not something a review shows, and `piece_file` hands the window no
    // value the vault keeps out of it. One more git call per open.
    let offered = files_in(base, branch)?;
    let offers = |relative: &Path| offered.binary_search(&slashed(relative)).is_ok();
    if !offers(relative) {
        return Err(Refused::NotOffered(path.to_string()));
    }
    let resolved = match std::fs::canonicalize(base.join(relative)) {
        Ok(resolved) => resolved,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Refused::NotThere(path.to_string()));
        }
        Err(e) => {
            return Err(Refused::Unreadable {
                what: path.to_string(),
                why: e.to_string(),
            });
        }
    };
    if resolved == base || !resolved.starts_with(base) || names_git(&resolved, base) {
        return Err(Refused::NotInPiece(path.to_string()));
    }
    // A link opens only a file the list offers too: one to an ignored file is refused.
    if !resolved.strip_prefix(base).is_ok_and(offers) {
        return Err(Refused::NotOffered(path.to_string()));
    }
    Ok(resolved)
}

/// **The file commands, on a branch whose folder the bounded reader found** (#1189, D-1189-7).
///
/// [`root`] asks the reader's child for the branch's folder ([`Ask::Root`]), found and checked
/// as the status read finds it, so the process asking starts no git to find it. Each command
/// below then confines its path and reads against that folder in this process, refused exactly
/// as its free-standing namesake refuses: the app's commands read a branch this way, and the
/// command line and the tests keep the free-standing ones, which find the folder here.
impl Root {
    /// The folder as the disk spells it now: the reader's answer, resolved again here.
    fn base(&self) -> Result<PathBuf, Refused> {
        resolved(self.path(), self.branch())
    }

    /// [`tree`], on this branch.
    pub fn tree(&self, folder: &str) -> Result<Folder, Refused> {
        tree_in(&self.base()?, self.branch(), folder)
    }

    /// [`open`], on this branch.
    pub fn open(&self, path: &str) -> Result<Opened, Refused> {
        open_in(&self.base()?, self.branch(), path)
    }

    /// [`folder`], on this branch.
    pub fn folder(&self, folder: &str) -> Result<PathBuf, Refused> {
        folder_of_in(&self.base()?, folder)
    }

    /// [`place`], on this branch.
    pub fn place(&self, path: &str) -> Result<Placed, Refused> {
        place_in(&self.base()?, path)
    }

    /// [`named`], on this branch.
    pub fn named(&self, path: &str) -> Result<String, Refused> {
        named_in(&self.base()?, path)
    }

    /// [`in_your_editor`], on this branch.
    pub fn in_your_editor(
        &self,
        path: &str,
        line: u32,
        editor: Editor,
        var: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Launch, Refused> {
        in_your_editor_in(&self.base()?, self.branch(), path, line, editor, var)
    }

    /// [`place_branch_folder`], on this branch: the folder by the name the workspace gives it,
    /// refused when that name is a link, and refused as gone when it is no longer the folder the
    /// reader found.
    pub fn place_branch_folder(&self) -> Result<Placed, Refused> {
        let branch = self.branch();
        let Branch { ws, repo, piece } = branch;
        let named = match piece {
            Some(piece) => worktree::path_for(self.plane(), ws, repo, piece)?,
            None => self.plane().join("workspaces").join(ws).join(repo),
        };
        let placed = place_named_folder(&named, branch)?;
        if placed.absolute != self.base()? {
            return Err(Refused::NotThere(branch.called().to_string()));
        }
        Ok(placed)
    }
}

/// [`named`], the branch's folder found by `reader`'s child ([`root`]): Copy relative path in the
/// app (#1189). The path is refused by its spelling before the reader is asked, as [`named`]
/// refuses it before it finds the folder.
pub fn named_by(
    reader: &Reader,
    plane: &Path,
    branch: Branch<'_>,
    path: &str,
) -> Result<String, Refused> {
    spelled_inside(path)?;
    root(reader, plane, branch)?.named(path)
}

/// The branch's folder, found by `reader`'s child when there is one ([`root`]), else in this
/// process ([`folder_of`]): how ⌘P and ⌘⇧F find each branch of their scope (#1189).
fn found(reader: Option<&Reader>, plane: &Path, branch: Branch<'_>) -> Result<PathBuf, Refused> {
    match reader {
        Some(reader) => root(reader, plane, branch).map(|root| root.path().to_path_buf()),
        None => folder_of(plane, branch),
    }
}

/// The branch's folder: a piece found the way the Explorer finds it, among the pieces git has;
/// the repo's own folder only where git says that folder is a repository's top.
fn folder_of(plane: &Path, branch: Branch<'_>) -> Result<PathBuf, Refused> {
    let Branch { ws, repo, piece } = branch;
    let Some(piece) = piece else {
        // `clone_dir` checks the names before joining them, keeps the folder inside the
        // workspace, and asks git whether it is a repository at all.
        let clone = worktree::clone_dir(plane, ws, repo)?;
        // A folder git answers for only because the project around it is a repository is not
        // a repo of this workspace: listing it would list the project's own files.
        let top = git::run(&clone, &["rev-parse", "--show-toplevel"], git::READ)
            .map_err(worktree::Refusal::from)?;
        let at_top = top.ok()
            && std::fs::canonicalize(top.line()).ok() == std::fs::canonicalize(&clone).ok();
        if !at_top {
            return Err(worktree::Refusal::NotARepo(repo.to_string()).into());
        }
        return Ok(clone);
    };
    // The names are checked before anything is joined or run.
    worktree::path_for(plane, ws, repo, piece)?;
    worktree::list(plane, ws, repo)?
        .into_iter()
        .find(|one| one.piece == piece && one.prunable.is_none())
        .map(|one| one.path)
        .ok_or_else(|| Refused::NoSuchPiece {
            ws: ws.to_string(),
            repo: repo.to_string(),
            piece: piece.to_string(),
        })
}

/// A path relative to the branch, with nothing in it that could leave: no root, no `..`, not
/// empty.
fn inside(path: &str) -> Result<&Path, Refused> {
    let relative = Path::new(path);
    let plain = !path.is_empty()
        && relative
            .components()
            .all(|step| matches!(step, Component::Normal(_) | Component::CurDir));
    if plain {
        Ok(relative)
    } else {
        Err(Refused::NotInPiece(path.to_string()))
    }
}

/// Whether a resolved path is, or is inside, a `.git` entry at any depth: the branch's own
/// (in a worktree `.git` is a file naming the clone's git directory) or a nested repo's. Either
/// is git's, not the branch's.
fn names_git(resolved: &Path, base: &Path) -> bool {
    resolved.strip_prefix(base).is_ok_and(gits)
}

/// Whether a path relative to the branch has a `.git` component, in any ASCII case: on a
/// case-insensitive folder (macOS, Windows, a Linux casefold or FAT mount) `.GIT` is git's too.
fn gits(relative: &Path) -> bool {
    relative.components().any(|step| {
        step.as_os_str()
            .as_encoded_bytes()
            .eq_ignore_ascii_case(b".git")
    })
}

/// A relative path as git's file list spells it: its plain components joined by `/`.
fn slashed(relative: &Path) -> String {
    relative
        .components()
        .filter_map(|step| match step {
            Component::Normal(name) => Some(name.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::natural;
    use std::cmp::Ordering;

    /// #1189: a reader whose child answers every ask with `answer` (the child's JSON), so a test
    /// sees which folder a command reads, and that nothing but the reader found it.
    #[cfg(unix)]
    pub(super) fn reader_answering(answer: &str) -> (tempfile::TempDir, super::Reader) {
        let dir = tempfile::tempdir().unwrap();
        let said = dir.path().join("answer");
        std::fs::write(&said, format!("\u{1e}charter-read\u{1e}{answer}\n")).unwrap();
        let script = format!("cat >/dev/null; cat '{}'", said.display());
        let reader = super::Reader::new(
            "/bin/sh".into(),
            ["-c", &script, super::READ_ARG].map(std::ffi::OsString::from),
        );
        (dir, reader)
    }

    /// A project holding the repo `alpha/thing` as a plain folder, with `a.txt` and `src/`.
    #[cfg(unix)]
    fn project() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        let folder = plane.join("workspaces/alpha/thing");
        std::fs::create_dir_all(folder.join("src")).unwrap();
        std::fs::write(folder.join("a.txt"), "a\n").unwrap();
        (dir, plane, folder)
    }

    #[cfg(unix)]
    fn found_at(folder: &std::path::Path) -> String {
        serde_json::json!({ "Ok": { "Root": { "base": folder, "refs": [] } } }).to_string()
    }

    const REPO: super::Branch<'static> = super::Branch {
        ws: "alpha",
        repo: "thing",
        piece: None,
    };

    /// #1189: a branch the reader refuses is refused in the reader's own sentence.
    #[cfg(unix)]
    #[test]
    fn a_branch_the_reader_refuses_is_refused_in_its_sentence() {
        let (_dir, plane, _folder) = project();
        let said = "thing in workspace 'alpha' has no branch folder called 'piece'";
        let (_answer, reader) = reader_answering(&serde_json::json!({ "Err": said }).to_string());

        let refused = super::root(&reader, &plane, REPO).unwrap_err();

        assert_eq!(refused.to_string(), said);
    }

    /// #1189: the file commands confine and read against the folder the reader found.
    #[cfg(unix)]
    #[test]
    fn the_file_commands_read_the_folder_the_reader_found() {
        let (_dir, plane, folder) = project();
        let (_answer, reader) = reader_answering(&found_at(&folder));

        let root = super::root(&reader, &plane, REPO).unwrap();

        assert_eq!(root.named("a.txt").unwrap(), "a.txt");
        assert_eq!(root.place("src").unwrap().absolute, folder.join("src"));
        assert_eq!(root.folder("src").unwrap(), folder.join("src"));
        assert_eq!(root.place_branch_folder().unwrap().absolute, folder);
        assert_eq!(
            root.named("../a.txt").unwrap_err().to_string(),
            "'../a.txt' is not a path inside the branch's folder"
        );
        assert_eq!(
            root.folder("a.txt").unwrap_err().to_string(),
            "'a.txt' is not a folder"
        );
    }

    /// #1189: the branch's own folder is placed by the name the workspace gives it: a link
    /// there is refused even when the reader found the folder it leads to.
    #[cfg(unix)]
    #[test]
    fn a_branch_folder_named_by_a_link_is_refused_whatever_the_reader_found() {
        let (_dir, plane, folder) = project();
        let real = folder.with_file_name("real");
        std::fs::rename(&folder, &real).unwrap();
        std::os::unix::fs::symlink(&real, &folder).unwrap();
        let (_answer, reader) = reader_answering(&found_at(&real));

        let root = super::root(&reader, &plane, REPO).unwrap();

        assert_eq!(
            root.place_branch_folder().unwrap_err().to_string(),
            "the folder of 'thing' is a link, and purlis places no folder a link leads to"
        );
    }

    /// #1189: a folder the reader found that is not the one the workspace names is not placed.
    #[cfg(unix)]
    #[test]
    fn a_branch_folder_that_is_not_the_one_the_reader_found_is_not_placed() {
        let (_dir, plane, folder) = project();
        let elsewhere = folder.join("src");
        let (_answer, reader) = reader_answering(&found_at(&elsewhere));

        let root = super::root(&reader, &plane, REPO).unwrap();

        assert_eq!(
            root.place_branch_folder().unwrap_err().to_string(),
            "'thing' is not in the branch's folder any more"
        );
    }

    /// The race a check-then-list leaves open, closed: a folder that is a link by the time it
    /// is listed is refused by the open, not followed. (The swap itself is not timed here; the
    /// listing is handed the link directly, which is what the swap would leave.)
    #[cfg(unix)]
    #[test]
    fn a_folder_that_is_a_link_when_listed_is_refused_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("elsewhere.txt"), "").unwrap();
        std::os::unix::fs::symlink(outside.path(), base.join("src")).unwrap();

        let refused = super::listing(&base, std::path::Path::new("src"))
            .err()
            .expect("a link is not listed");

        assert!(super::swapped(&refused), "{refused}");
    }

    #[test]
    fn names_sort_as_a_person_reads_them() {
        let mut names = vec![
            "file10.txt",
            "File2.txt",
            "file1.txt",
            "a",
            "B",
            "file02.txt",
        ];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(
            names,
            [
                "a",
                "B",
                "file1.txt",
                "File2.txt",
                "file02.txt",
                "file10.txt"
            ]
        );
        assert_eq!(natural("x", "x"), Ordering::Equal);
    }
}
