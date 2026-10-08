//! **The pairs the person said never to** (#1503): `app/dispatch-never.json`, a file of its own
//! beside this machine's other records of the project.
//!
//! # Why a file of its own
//!
//! Every other record of dispatch on this machine is an allow, so one that does not read is
//! read as empty and nothing is let through by it. A never is the one deny: read as empty, it
//! lets through what the person refused. So it is kept where nothing else is written:
//!
//! - **No other record's fault unreads it.** A key of `app/sandbox.json` this build cannot
//!   read costs that file its grants, and costs this one nothing.
//! - **No older build rewrites it.** A build from before this file drops the keys of
//!   `app/sandbox.json` it does not know at its next chat start. It never opens this file.
//!
//! # It fails closed
//!
//! [`read`] answers [`Nevers::Unread`] for a file that is there and is not the shape below, a
//! link, or anything this process cannot read. **That is never "no nevers".** While it is the
//! answer, no grant covers any pair ([`crate::dispatchgrant::Covers::Unread`]): the person is
//! asked, and told why, and a chat nobody is at is refused. No file at all is no nevers.
//!
//! **A write never drops what it could not read**: [`add`] and [`lift`] refuse on a file that
//! does not read, with [`unread_said`], and leave its bytes as they are. Mending it is the
//! person's: fix the file, or delete it, which says never to nothing.
//!
//! # The shape
//!
//! ```json
//! {"never": [{"asking": "steward", "target": "devops"}]}
//! ```
//!
//! Each entry is matched as it is spelled. An entry that is not two strings makes the whole
//! file unread, since what it meant to refuse is unknown. A key this build does not know is
//! kept as it is.
//!
//! # Where it is, and who writes it
//!
//! Under `app/` in the project's state folder, which the integrity class denies every
//! sandboxed chat writing ([`crate::sandbox::Class::Integrity`]). The app writes it on the
//! person's press. A chat that runs without the sandbox runs as the person and can write it,
//! as it can write anything of theirs; nothing here is a boundary against that chat.
//!
//! # What an earlier build left
//!
//! One dev build kept nevers as `dispatch_never` in `app/sandbox.json`. [`read`] moves what is
//! there into this file the first time it finds any, and counts them until the move is done.

use std::io;
use std::path::{Path, PathBuf};

/// The file, relative to the project's state folder ([`path`]).
pub const IN_STATE: &str = "app/dispatch-never.json";

/// The key that holds the pairs.
const KEY: &str = "never";

/// The file in the project at `root`.
pub fn path(root: &Path) -> PathBuf {
    crate::names::state(root).join(IN_STATE)
}

/// What [`read`] answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nevers {
    /// The pairs the person said never to, each as asking persona and target persona, as the
    /// file spells them. Empty where there is no file.
    Read(Vec<(String, String)>),
    /// There is a record and it does not read: what the person refused is unknown.
    Unread,
}

/// One pair of an entry, or `None` for an entry that is not `{asking, target}` strings.
fn pair_of(entry: &serde_json::Value) -> Option<(String, String)> {
    let entry = entry.as_object()?;
    Some((
        entry.get("asking")?.as_str()?.to_owned(),
        entry.get("target")?.as_str()?.to_owned(),
    ))
}

/// A list of entries as pairs, or `None` where it is not a list or holds one that is no pair.
pub(crate) fn pairs_of(list: &serde_json::Value) -> Option<Vec<(String, String)>> {
    list.as_array()?.iter().map(pair_of).collect()
}

/// The pairs of the file, each as asking persona and target persona.
type Pairs = Vec<(String, String)>;

/// The file's whole object, with every key this build does not know.
type Whole = serde_json::Map<String, serde_json::Value>;

/// `text` as the file's object and its pairs, or `None` where it is not this shape.
fn parsed(text: &str) -> Option<(Whole, Pairs)> {
    let serde_json::Value::Object(whole) = serde_json::from_str(text).ok()? else {
        return None;
    };
    let pairs = match whole.get(KEY) {
        None => Vec::new(),
        Some(list) => pairs_of(list)?,
    };
    Some((whole, pairs))
}

/// The file as it is on disk, with nothing moved into it.
fn on_disk(root: &Path) -> Nevers {
    let path = path(root);
    match std::fs::symlink_metadata(&path) {
        Err(none) if none.kind() == io::ErrorKind::NotFound => Nevers::Read(Vec::new()),
        // Never through a link, and never a folder or a device.
        Ok(there) if there.is_file() => std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| parsed(&text))
            .map_or(Nevers::Unread, |(_, pairs)| Nevers::Read(pairs)),
        _ => Nevers::Unread,
    }
}

/// What a write is refused with on a record that does not read, and what the person is told.
pub fn unread_said(root: &Path) -> String {
    let path = path(root);
    let shown = path
        .strip_prefix(root)
        .unwrap_or(&path)
        .display()
        .to_string();
    format!(
        "purlis could not read the list of pairs you said never to ({shown} in this project), \
         so it changed nothing there and no dispatch grant counts until it reads. Fix that \
         file, or delete it to say never to nothing."
    )
}

/// Reads the file, changes its pairs with `how` and writes it back, under purlis's lock on its
/// directory. **Refuses a file that does not read**, and leaves it as it is.
fn change(root: &Path, how: impl FnOnce(&mut Vec<(String, String)>)) -> io::Result<()> {
    let unread = || io::Error::new(io::ErrorKind::InvalidData, unread_said(root));
    if on_disk(root) == Nevers::Unread {
        return Err(unread());
    }
    let path = path(root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::rewrite::update(root, &path, |now| {
        let (mut whole, mut pairs) = match now {
            None => (serde_json::Map::new(), Vec::new()),
            Some(text) => parsed(text).ok_or_else(unread)?,
        };
        how(&mut pairs);
        whole.insert(
            KEY.to_owned(),
            pairs
                .iter()
                .map(|(asking, target)| serde_json::json!({"asking": asking, "target": target}))
                .collect(),
        );
        serde_json::to_string_pretty(&whole)
            .map(|text| Some(format!("{text}\n")))
            .map_err(io::Error::other)
    })
    .map(|_| ())
}

/// **The pairs the person said never to in the project at `root`**, or that the record does
/// not read. What an earlier build left in `app/sandbox.json` is moved here first, and is
/// counted either way.
pub fn read(root: &Path) -> Nevers {
    let Some(left) = crate::sandbox::local::legacy_dispatch_nevers(root) else {
        return Nevers::Unread;
    };
    let moved = left.is_empty()
        || (change(root, |pairs| {
            for one in &left {
                if !pairs.contains(one) {
                    pairs.push(one.clone());
                }
            }
        })
        .is_ok()
            && crate::sandbox::local::drop_legacy_dispatch_nevers(root).is_ok());
    match on_disk(root) {
        Nevers::Unread => Nevers::Unread,
        Nevers::Read(mut pairs) => {
            if !moved {
                for one in left {
                    if !pairs.contains(&one) {
                        pairs.push(one);
                    }
                }
            }
            Nevers::Read(pairs)
        }
    }
}

/// Records the person's never for `asking` to `target`: the grant Notice's "Never for this
/// pair", never a chat. Refused where the record does not read.
pub fn add(root: &Path, asking: &str, target: &str) -> io::Result<()> {
    if read(root) == Nevers::Unread {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            unread_said(root),
        ));
    }
    let pair = (asking.to_owned(), target.to_owned());
    change(root, |pairs| {
        if !pairs.contains(&pair) {
            pairs.push(pair);
        }
    })
}

/// Lifts that never: Settings, never a chat. Answers whether there was one. Refused where the
/// record does not read.
pub fn lift(root: &Path, asking: &str, target: &str) -> io::Result<bool> {
    let Nevers::Read(now) = read(root) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            unread_said(root),
        ));
    };
    if !now.iter().any(|(from, to)| from == asking && to == target) {
        return Ok(false);
    }
    change(root, |pairs| {
        pairs.retain(|(from, to)| !(from == asking && to == target));
    })?;
    Ok(true)
}

/// **Renames a persona in every never** (#1504): a persona renamed through purlis keeps what
/// the person refused of it, as asking persona and as target. A never that would name one
/// persona twice is kept as it then reads, and one said twice is kept once. Refused where the
/// record does not read, and then nothing is written.
pub fn rename(root: &Path, from: &str, to: &str) -> io::Result<()> {
    let Nevers::Read(now) = read(root) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            unread_said(root),
        ));
    };
    if !now
        .iter()
        .any(|(asking, target)| asking == from || target == from)
    {
        return Ok(());
    }
    let name = |one: &str| if one == from { to } else { one }.to_owned();
    change(root, |pairs| {
        let mut out: Vec<(String, String)> = Vec::new();
        for (asking, target) in pairs.drain(..) {
            let one = (name(&asking), name(&target));
            if !out.contains(&one) {
                out.push(one);
            }
        }
        *pairs = out;
    })
}

#[cfg(test)]
#[path = "dispatchnever_tests.rs"]
mod tests;
