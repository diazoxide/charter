//! Looking at the plane's directories with three answers rather than two.
//!
//! `Path::exists` and `Path::is_dir` answer "no" for a path charter was not allowed to look
//! at, which is the #1043 family in one method call: a directory at mode 000 reads as absent,
//! so the row describing it reads as healthy. Every question here answers yes, no, or "could
//! not tell" with the errno — Python's `workspace._existence`, `_directory`,
//! `read_directory` and `read_files` — and each row names what it could not tell beside its
//! verdict.

use std::io;
use std::path::{Path, PathBuf};

use super::{PATH_DISPLAY_LIMIT, one_line};

/// Something charter could not look at, and the errno the look met.
pub(super) type Unread = (PathBuf, Option<i32>);

/// `ELOOP` on the platforms charter runs on — the one errno whose remedy differs.
const ELOOP: i32 = if cfg!(target_os = "linux") { 40 } else { 62 };

/// Python's `except (FileNotFoundError, NotADirectoryError)`: the path is simply not there.
fn absent(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotFound || e.raw_os_error() == Some(not_a_directory())
}

const fn not_a_directory() -> i32 {
    20 // ENOTDIR on Linux and macOS alike
}

/// `_existence`: is anything at `p` — `Some(true)`, `Some(false)`, or `None` with the errno.
pub(super) fn existence(p: &Path, follow: bool) -> (Option<bool>, Option<i32>) {
    let looked = if follow {
        std::fs::metadata(p)
    } else {
        std::fs::symlink_metadata(p)
    };
    match looked {
        Ok(_) => (Some(true), None),
        Err(e) if absent(&e) => (Some(false), None),
        Err(e) => (None, e.raw_os_error()),
    }
}

/// `_directory`: is `p`, through a link, a directory — with the errno when it cannot be told.
pub(super) fn directory(p: &Path) -> (Option<bool>, Option<i32>) {
    match std::fs::metadata(p) {
        Ok(meta) => (Some(meta.is_dir()), None),
        Err(e) if absent(&e) => (Some(false), None),
        Err(e) => (None, e.raw_os_error()),
    }
}

/// `read_directory`: the entries of `d` that `keep` keeps, sorted, and each it could not
/// tell about. A `d` that is not there has no entries; one that cannot be LISTED is `Err`,
/// because nothing under it can be counted.
pub(super) fn read_directory(
    d: &Path,
    keep: impl Fn(&Path) -> (Option<bool>, Option<i32>),
) -> io::Result<(Vec<PathBuf>, Vec<Unread>)> {
    let listing = match std::fs::read_dir(d) {
        Ok(listing) => listing,
        Err(e) if absent(&e) => return Ok((Vec::new(), Vec::new())),
        Err(e) => return Err(e),
    };
    let mut entries: Vec<PathBuf> = listing
        .map(|entry| entry.map(|e| e.path()))
        .collect::<io::Result<_>>()?;
    entries.sort();
    let mut kept = Vec::new();
    let mut unread = Vec::new();
    for p in entries {
        match keep(&p) {
            (None, code) => unread.push((p, code)),
            (Some(true), _) => kept.push(p),
            (Some(false), _) => {}
        }
    }
    Ok((kept, unread))
}

/// `contain.within_data`: does `path` resolve inside one of the plane's data directories.
pub(super) fn within_data(root: &Path, path: &Path) -> bool {
    crate::contain::readable(root, path).is_ok()
}

/// A file charter will read whole is bounded — `contain.MAX_BYTES`.
const MAX_BYTES: u64 = 1_048_576;

/// The data directories, as a refusal names them.
const DATA_ROOTS: &str = "persona-state, personas, workspaces";

fn path_field(p: &Path) -> String {
    one_line(&p.display().to_string(), PATH_DISPLAY_LIMIT)
}

/// `contain._not_plane_data`.
fn not_plane_data(path: &Path, verb: &str) -> String {
    let target = crate::contain::resolved(path).unwrap_or_else(|| path.to_path_buf());
    format!(
        "'{}' resolves to '{}', outside the directories a control plane keeps its data in \
         ({DATA_ROOTS}). A committed symlink there redirects the {verb}, so charter follows a \
         link that lands inside them and refuses one that leaves",
        path_field(path),
        path_field(&target)
    )
}

/// An OS error as CPython's `strerror` says it — Rust's text without its `(os error N)`.
fn strerror(e: &io::Error) -> String {
    let text = e.to_string();
    match text.rfind(" (os error ") {
        Some(at) if text.ends_with(')') => text[..at].to_owned(),
        _ => text,
    }
}

/// An `OSError` about `path` as Python prints one — `[Errno 13] Permission denied: '/p'` —
/// which is what a row's `not checked (…)` quotes when a directory could not be listed.
pub(super) fn py_os_error(e: &io::Error, path: &Path) -> String {
    let shown = path.display().to_string();
    let quoted = if shown.contains('\'') && !shown.contains('"') {
        format!("\"{shown}\"")
    } else {
        format!("'{}'", shown.replace('\\', "\\\\").replace('\'', "\\'"))
    };
    match e.raw_os_error() {
        Some(code) => format!("[Errno {code}] {}: {quoted}", strerror(e)),
        None => e.to_string(),
    }
}

fn unreadable(path: &Path, e: &io::Error) -> String {
    format!(
        "'{}' cannot be examined ({})",
        path_field(path),
        one_line(&strerror(e), PATH_DISPLAY_LIMIT)
    )
}

/// What a thing that is not a regular file is — `contain._KINDS`.
fn kind_of(meta: &std::fs::Metadata) -> &'static str {
    let t = meta.file_type();
    if t.is_dir() {
        return "a directory";
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if t.is_fifo() {
            return "a FIFO";
        }
        if t.is_socket() {
            return "a socket";
        }
        if t.is_char_device() {
            return "a character device";
        }
        if t.is_block_device() {
            return "a block device";
        }
    }
    "not a file"
}

/// `contain._path_refusal` on the WRITE side: why charter must not write the file at
/// `path`, or `None`. One `lstat`, then a link's containment, then what it is and how big.
///
/// The write rule and not the read one, because the only caller is the index refusal: a
/// path that is not there is the ordinary case — the file is about to be created — while a
/// DANGLING link is absent and hostile at once, which is why the link's containment is
/// asked before anything is followed.
fn write_path_refusal(root: &Path, path: &Path) -> Option<String> {
    const VERB: &str = "write";
    let mut meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => return Some(unreadable(path, &e)),
    };
    if meta.file_type().is_symlink() {
        if !within_data(root, path) {
            return Some(not_plane_data(path, VERB));
        }
        meta = match std::fs::metadata(path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
            Err(e) => return Some(unreadable(path, &e)),
        };
    }
    if !meta.is_file() {
        return Some(format!(
            "'{}' is not a regular file (it is {}). Charter opens plane data at names a \
             committed file can occupy, so an entry that blocks or never ends would take the \
             {VERB} with it",
            path_field(path),
            kind_of(&meta)
        ));
    }
    if meta.len() > MAX_BYTES {
        return Some(format!(
            "'{}' is {} bytes, over the {MAX_BYTES}-byte bound on one plane file. Nothing a \
             memory, todo, ref or persona charter is meant to hold comes near that, so this is \
             a defect in the file rather than a limit to raise",
            path_field(path),
            meta.len()
        ));
    }
    None
}

/// `contain.dir_refusal`: a directory that resolves outside the plane's data is not listed.
pub(super) fn dir_refusal(root: &Path, dir: &Path, verb: &str) -> Option<String> {
    if within_data(root, dir) {
        None
    } else {
        Some(not_plane_data(dir, verb))
    }
}

/// `contain.write_refusal`: the parent first — when `memory/` is the link, `MEMORY.md` inside
/// it is an ordinary file with nothing to object to — then the file itself.
pub(super) fn write_refusal(root: &Path, path: &Path) -> Option<String> {
    let parent = path.parent().unwrap_or(path);
    dir_refusal(root, parent, "write").or_else(|| write_path_refusal(root, path))
}

/// `workspace.unread_name`: a path charter could not check, named from the plane's root when
/// it is inside it, and contained — it is often a filename a chat wrote.
pub(super) fn unread_name(root: &Path, path: &Path) -> String {
    let named = match path.strip_prefix(root) {
        Ok(rel) => rel.display().to_string(),
        Err(_) => path.display().to_string(),
    };
    crate::shown::readable(&named, PATH_DISPLAY_LIMIT)
}

/// `workspace.uncheckable_fix`: a loop names the link, because no permission is in the way
/// of one; anything else is read as a refusal.
fn uncheckable_fix(code: Option<i32>, shown: &str) -> String {
    if code == Some(ELOOP) {
        format!("fix the symlink loop at {shown}")
    } else {
        "restoring read access to it clears this".to_owned()
    }
}

/// `workspace.cannot_check`: `<path> cannot be checked — <what clears it>`, no full stop.
pub(super) fn cannot_check(root: &Path, path: &Path, code: Option<i32>) -> String {
    let shown = crate::shown::readable(&path.display().to_string(), PATH_DISPLAY_LIMIT);
    format!(
        "{} cannot be checked — {}",
        unread_name(root, path),
        uncheckable_fix(code, &shown)
    )
}

/// `doctor._beside_unread`: `row`, saying beside its verdict every path it could not check —
/// and never OK while one stands. Beside and not instead: the verdict is still true of what
/// was read (#1014).
pub(super) fn beside_unread(root: &Path, mut row: super::Row, unread: &[Unread]) -> super::Row {
    if unread.is_empty() {
        return row;
    }
    let named: Vec<String> = unread.iter().map(|(p, _)| unread_name(root, p)).collect();
    let cannot = format!(
        "{}.",
        unread
            .iter()
            .map(|(p, code)| cannot_check(root, p, *code))
            .collect::<Vec<_>>()
            .join("; ")
    );
    if row.status == super::Status::Ok {
        row.status = super::Status::Warn;
    }
    row.detail = format!("{}; {} cannot be checked", row.detail, named.join(", "));
    row.hint = if row.hint.is_empty() {
        cannot
    } else {
        format!("{}   {cannot}", row.hint)
    };
    row
}

/// `workspace.read_workspaces`: the workspaces under `workspaces/`, and each directory there
/// whose kind the filesystem will not tell. A leading dot is charter's own (`.worktrees/`),
/// and a clone sitting directly under `workspaces/` is not a workspace.
pub(super) fn read_workspaces(root: &Path) -> io::Result<(Vec<String>, Vec<Unread>)> {
    let keep = |d: &Path| {
        let hidden = d
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if hidden {
            return (Some(false), None);
        }
        match directory(d) {
            (Some(true), code) => (Some(!is_clone(d)), code),
            other => other,
        }
    };
    let (found, unread) = read_directory(&root.join("workspaces"), keep)?;
    let names = found
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    Ok((names, unread))
}

/// A clone, not a worktree: git itself draws the line — a clone's `.git` is a DIRECTORY.
fn is_clone(p: &Path) -> bool {
    directory(&p.join(".git")).0 == Some(true)
}

/// `workspace.read_clones`: the clones in one workspace, and each entry whose `.git` charter
/// cannot `stat`. A workspace that cannot be listed is `Err`.
pub(super) fn read_clones(root: &Path, ws: &str) -> io::Result<(Vec<PathBuf>, Vec<Unread>)> {
    read_directory(&root.join("workspaces").join(ws), |d| {
        directory(&d.join(".git"))
    })
}
