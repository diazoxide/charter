//! Where records live: `workspaces/<ws>/changes/<slug>.json`, one file each.
//!
//! **Not created by scaffold.** `wscmd::meta_paths` hands `changes/` to git only when it holds
//! a record, so the directory is made by the first `charter change create`, and
//! `changes/log/` by the first landing.
//!
//! Every read and write is gated by containment first ([`contain::readable`],
//! [`contain::writable`]): a committed `changes -> ../../elsewhere` travels to every machine
//! that clones the plane, and would otherwise redirect both.

use std::path::{Path, PathBuf};

use super::record::{Record, RecordError, name_ok};
use crate::{contain, shown};

/// The directory, relative to the workspace, holding the records.
pub const DIRNAME: &str = "changes";

/// `workspaces/<ws>/changes/`. `ws` is a workspace name the caller has already checked.
pub fn dir(plane: &Path, ws: &str) -> PathBuf {
    plane.join("workspaces").join(ws).join(DIRNAME)
}

/// The record's path, or a refusal when `slug` is not a change name. Asked here and not only
/// at creation: a slug can arrive from a hand edit or another machine.
pub fn path_for(plane: &Path, ws: &str, slug: &str) -> Result<PathBuf, RecordError> {
    if !name_ok(slug) {
        return Err(RecordError(format!(
            "{} is not a change name (letters, digits, '.', '_', '-'; must not start with a \
             dot or a dash). This names a file in the plane and a branch in every member, so \
             it is refused rather than rewritten.",
            shown::short(slug)
        )));
    }
    Ok(dir(plane, ws).join(format!("{slug}.json")))
}

/// Is there a record for `slug`? False for a name that is not a change name.
pub fn exists(plane: &Path, ws: &str, slug: &str) -> bool {
    path_for(plane, ws, slug).is_ok_and(|p| p.symlink_metadata().is_ok())
}

/// The record for `slug`, validated, or what is wrong with it.
pub fn read(plane: &Path, ws: &str, slug: &str) -> Result<Record, RecordError> {
    let path = path_for(plane, ws, slug)?;
    contain::readable(plane, &path).map_err(|e| RecordError(e.to_string()))?;
    let text = std::fs::read_to_string(&path)
        .map_err(|e| RecordError(format!("change '{slug}': cannot be read ({e})")))?;
    Record::parse(&text, slug)
}

/// Why a write did not happen.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The record cannot be true: a refusal of the request.
    #[error(transparent)]
    Record(#[from] RecordError),
    /// A path charter must not write, or the disk said no: an error about the plane.
    #[error("{0}")]
    Plane(String),
}

/// Write `record` as its own file, validated first, whole or not at all.
pub fn write(plane: &Path, ws: &str, record: &Record) -> Result<PathBuf, WriteError> {
    record.validate()?;
    let path = path_for(plane, ws, &record.change)?;
    let parent = dir(plane, ws);
    contain::writable(plane, &path).map_err(|e| WriteError::Plane(e.to_string()))?;
    std::fs::create_dir_all(&parent).map_err(|e| WriteError::Plane(e.to_string()))?;
    crate::rewrite::replace(
        &parent,
        &path,
        record.to_json().as_bytes(),
        crate::rewrite::Mode::Kept,
    )
    .map_err(|e| WriteError::Plane(format!("could not write {}: {e}", path.display())))?;
    Ok(path)
}

/// Delete the record for `slug`. Nothing else: no landing-log line, no branch, no request.
/// `Ok(None)` when there was none.
pub fn forget(plane: &Path, ws: &str, slug: &str) -> Result<Option<PathBuf>, String> {
    let path = path_for(plane, ws, slug).map_err(|e| e.to_string())?;
    if path.symlink_metadata().is_err() {
        return Ok(None);
    }
    contain::writable(plane, &path).map_err(|e| e.to_string())?;
    std::fs::remove_file(&path).map_err(|e| format!("could not delete {}: {e}", path.display()))?;
    Ok(Some(path))
}

/// Every record in a workspace, and beside them what could not be read.
#[derive(Debug, Default)]
pub struct Listing {
    /// Sorted by slug.
    pub records: Vec<Record>,
    /// `(file stem, why)` for each record charter would not act on. Reported, never dropped.
    pub refused: Vec<(String, String)>,
    /// A `changes/` that exists and could not be listed. Never read as "none".
    pub unread: Option<Unread>,
}

/// A directory charter could not look at: where, the OS error number when there was one, and
/// the sentence to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unread {
    pub path: PathBuf,
    pub errno: Option<i32>,
    pub why: String,
}

/// Every record in the workspace. A `changes/` that does not exist is no changes; one that
/// could not be listed is said, not read as none (charter-plane #1084).
pub fn read_all(plane: &Path, ws: &str) -> Listing {
    let d = dir(plane, ws);
    let mut listing = Listing::default();
    if d.symlink_metadata().is_err() {
        return listing;
    }
    if let Err(e) = contain::readable(plane, &d) {
        listing.unread = Some(Unread {
            path: d,
            errno: None,
            why: e.to_string(),
        });
        return listing;
    }
    let names = match std::fs::read_dir(&d) {
        Ok(reader) => {
            let mut names: Vec<String> = reader
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
        Err(e) => {
            listing.unread = Some(Unread {
                why: format!("could not list {}: {e}", d.display()),
                errno: e.raw_os_error(),
                path: d,
            });
            return listing;
        }
    };
    for name in names {
        let Some(slug) = name.strip_suffix(".json") else {
            continue;
        };
        match read(plane, ws, slug) {
            Ok(record) => listing.records.push(record),
            Err(e) => listing.refused.push((slug.to_string(), e.to_string())),
        }
    }
    listing
}
