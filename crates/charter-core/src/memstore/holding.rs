//! A workspace's memory and todos, used through the store held by descriptor (V74, #1064).
//!
//! [`super`]'s operations take a store by its path, which a link planted in the workspace can
//! redirect between the check and the write. When that path is a workspace's store,
//! `<project>/workspaces/<ws>/<store>` (or a directory directly in it, its `archive/`), each
//! operation hands over to its twin here, which holds the store through [`crate::held`] and
//! touches nothing by path again. The rules are [`super`]'s: the same names, the same index
//! lines, the same refusals in the same words, and the same path gates asked first, so a link
//! out of the project is refused in the sentence it always was. What is new is that a link
//! that stays inside the project, to another workspace or a persona, is no longer followed
//! either.
//!
//! The MCP tools write through [`write_in`] too, so a memory or a todo is written one way
//! whoever writes it, and the index line goes with the file or neither stays (#1058).

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use crate::active::Place;
use crate::held::{Make, Store, Unheld, Who};

use super::{
    Base, EditRefused, Found, INDEX, INDEX_FALLBACK, MAX_BYTES, Unread, entry_name, gate,
    index_line, index_rewritten, listed_in, md_name, memory_body, no_such, title_in,
};

/// A workspace's store, named by the path a caller gave: where it is, and what it is.
pub(super) struct Spot<'a> {
    /// The path the caller named it by, which every path handed back is built from.
    dir: &'a Path,
    root: &'a Path,
    ws: String,
    store: String,
    /// The directories below the store, when one of them was named: `archive`.
    subs: Vec<String>,
}

/// How a store named by a path is reached.
pub(super) enum Reach<'a> {
    /// A workspace's store, or a directory in one: held by descriptor.
    Held(Spot<'a>),
    /// A path below `<root>/workspaces/` that is not a workspace's store spelled plainly: a
    /// `..` in it, or one too short to name a store. Refused, never used by path, so the hold
    /// does not rest on how a caller built the path.
    Refused(io::Error),
    /// Anything else (a persona's store, charter's own state): reached by path, as before.
    ByPath,
}

/// How `dir` is reached: held when it is `<root>/workspaces/<ws>/<store>` or a directory in
/// one, refused when it is any other path below `<root>/workspaces/`.
pub(super) fn reach<'a>(root: &'a Path, dir: &'a Path) -> Reach<'a> {
    let Ok(below) = dir.strip_prefix(root) else {
        return Reach::ByPath;
    };
    let mut components = below.components();
    if components.next() != Some(std::path::Component::Normal("workspaces".as_ref())) {
        return Reach::ByPath;
    }
    let refused = || {
        Reach::Refused(refused(format!(
            "{} is not a workspace's store, so nothing was done",
            crate::shown::readable(&below.to_string_lossy(), super::PATH_LIMIT)
        )))
    };
    let Some(parts) = components
        .map(|part| match part {
            std::path::Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Option<Vec<&str>>>()
    else {
        return refused();
    };
    // Each part is one plain name (a `Normal` component): never `..`, `.` or a separator.
    // A name charter would not give a workspace is still held, not refused: `doctor` reads the
    // store of a directory a chat made under `workspaces/` with any name (#353).
    let [ws, store, subs @ ..] = parts.as_slice() else {
        return refused();
    };
    Reach::Held(Spot {
        dir,
        root,
        ws: (*ws).to_owned(),
        store: (*store).to_owned(),
        subs: subs.iter().map(|sub| (*sub).to_owned()).collect(),
    })
}

/// A refusal of the held layer, as the `io::Error` [`super`]'s callers read.
fn refused(why: String) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, why)
}

/// A store whose lock another process would not let go of in time.
fn busy(why: String) -> io::Error {
    io::Error::new(io::ErrorKind::WouldBlock, why)
}

/// Whether `e` is the held layer refusing a link, rather than the filesystem refusing a look.
fn is_refusal(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::PermissionDenied && e.raw_os_error().is_none()
}

/// A store that could not be held, as an `io::Error`: the filesystem's own error when it
/// refused a look (which a reader names as unread), a refusal otherwise.
fn unheld(unheld: Unheld) -> io::Error {
    match unheld.errno {
        Some(errno) => io::Error::from_raw_os_error(errno),
        None => refused(unheld.why),
    }
}

impl Spot<'_> {
    /// The store, held, made as `make` says, and the directory below it that was named.
    pub(super) fn held(&self, make: Make) -> io::Result<Option<Store>> {
        let place = Place::Workspace(self.ws.clone());
        let Some(mut store) =
            Store::hold(self.root, &place, &self.store, make, Who::Operator).map_err(unheld)?
        else {
            return Ok(None);
        };
        for sub in &self.subs {
            store = match store.sub(sub, make != Make::Nothing).map_err(unheld)? {
                Some(below) => below,
                None => return Ok(None),
            };
        }
        Ok(Some(store))
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
}

/// The memory files of a held store, sorted: every plain `*.md` file but the index, within
/// the bound on one file — what [`super::read_files`] lists by path. A link, a file with
/// another name, a FIFO or a directory is not a memory.
fn md_names(store: &Store) -> io::Result<Vec<String>> {
    let mut out = Vec::new();
    for name in store.files().map_err(io::Error::other)? {
        if name == INDEX || !name.ends_with(".md") {
            continue;
        }
        if store
            .entry(&name)
            .map_err(io::Error::other)?
            .is_some_and(|entry| entry.plain() && entry.size <= MAX_BYTES)
        {
            out.push(name);
        }
    }
    Ok(out)
}

/// [`super::read_files`] for a workspace's store.
pub(super) fn read_files(spot: &Spot) -> (Vec<PathBuf>, Unread) {
    match spot.held(Make::Nothing).and_then(|held| match held {
        Some(store) => md_names(&store),
        None => Ok(Vec::new()),
    }) {
        Ok(names) => (
            names.iter().map(|name| spot.path(name)).collect(),
            Vec::new(),
        ),
        // A store reached through a link holds nothing and says nothing, as one that resolves
        // out of the project always did.
        Err(e) if is_refusal(&e) => (Vec::new(), Vec::new()),
        // One the filesystem would not let charter look at, or list, is unread, and named.
        Err(e) => (Vec::new(), vec![(spot.dir.to_path_buf(), e.raw_os_error())]),
    }
}

/// [`super::read_entries`] for a workspace's store: each memory read through the held store.
pub(super) fn read_entries(spot: &Spot) -> (Vec<Found>, Unread) {
    let store = match spot.held(Make::Nothing) {
        Ok(Some(store)) => store,
        Ok(None) => return (Vec::new(), Vec::new()),
        Err(e) if is_refusal(&e) => return (Vec::new(), Vec::new()),
        Err(e) => return (Vec::new(), vec![(spot.dir.to_path_buf(), e.raw_os_error())]),
    };
    let names = match md_names(&store) {
        Ok(names) => names,
        Err(e) => return (Vec::new(), vec![(spot.dir.to_path_buf(), e.raw_os_error())]),
    };
    let found = names
        .into_iter()
        .filter_map(|name| {
            let text = store.read(&name).ok()??;
            let path = spot.path(&name);
            let title = title_in(&path, &text);
            Some(Found { path, title, text })
        })
        .collect();
    (found, Vec::new())
}

/// The files the held store's index lists ([`super::listed`]).
fn listed_held(store: &Store) -> BTreeSet<String> {
    match store.entry(INDEX) {
        Ok(Some(entry)) if entry.plain() && entry.size <= MAX_BYTES => {}
        _ => return BTreeSet::new(),
    }
    match store.read(INDEX) {
        Ok(Some(text)) => listed_in(&text),
        _ => BTreeSet::new(),
    }
}

/// [`super::listed`] for a workspace's store.
pub(super) fn listed(spot: &Spot) -> BTreeSet<String> {
    match spot.held(Make::Nothing) {
        Ok(Some(store)) => listed_held(&store),
        _ => BTreeSet::new(),
    }
}

/// [`super::ensure_index`] for a workspace's store: the store and its index made, with
/// `header`, when they are not there. The index's path.
pub(super) fn ensure_index(spot: &Spot, header: &str) -> io::Result<PathBuf> {
    let store = spot.held(Make::All)?.ok_or_else(|| {
        refused(format!(
            "workspaces/{}/{} could not be made",
            spot.ws, spot.store
        ))
    })?;
    let header = if header.ends_with('\n') {
        header.to_owned()
    } else {
        format!("{header}\n")
    };
    store.create(INDEX, header.as_bytes()).map_err(refused)?;
    Ok(spot.path(INDEX))
}

/// [`super::index_append`] for a workspace's store. Takes no lock, as that does not.
pub(super) fn index_append(spot: &Spot, filename: &str, title: &str) -> io::Result<()> {
    let store = spot.held(Make::All)?.ok_or_else(|| {
        refused(format!(
            "workspaces/{}/{} could not be made",
            spot.ws, spot.store
        ))
    })?;
    let line = format!("{}\n", index_line(title, filename));
    store
        .append(INDEX, INDEX_FALLBACK, line.as_bytes())
        .map_err(refused)
}

/// [`super::write`] for a workspace's store, from the point the store's own gate answered:
/// `text` already stripped and `title` already chosen.
#[allow(clippy::too_many_arguments)]
pub(super) fn write(
    spot: &Spot,
    text: &str,
    title: &str,
    timestamped: bool,
    kind: &str,
    index: bool,
    stamp: chrono::NaiveDateTime,
) -> io::Result<PathBuf> {
    let store = spot.held(Make::All)?.ok_or_else(|| {
        refused(format!(
            "workspaces/{}/{} could not be made",
            spot.ws, spot.store
        ))
    })?;
    let _held = store.lock().map_err(busy)?;
    let name = write_in(&store, spot, text, title, timestamped, kind, index, stamp)?;
    Ok(spot.path(&name))
}

/// Write one memory into `store`, which the caller holds and has locked, and index it: the
/// one write of a workspace's memory or todos, for the operator's commands and the MCP tools
/// alike. The file name it took.
///
/// The index is checked before a byte is written, and the file is removed again when its
/// line cannot be appended, so a memory never stays on disk that its index does not list
/// (#1058). `gates` asks the path gates [`super::write`] asks, so a link out of the project is
/// refused in the sentence it always was; the MCP tools have none to ask, having refused every
/// link in the store already.
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_in(
    store: &Store,
    gates: &dyn Gates,
    text: &str,
    title: &str,
    timestamped: bool,
    kind: &str,
    index: bool,
    stamp: chrono::NaiveDateTime,
) -> io::Result<String> {
    let prefix = if timestamped {
        stamp.format("%Y%m%d-%H%M%S-").to_string()
    } else {
        String::new()
    };
    let body = memory_body(title, kind, text, stamp);
    if index {
        gates.gate(INDEX)?;
        if let Some(entry) = store.entry(INDEX).map_err(refused)?
            && !entry.plain()
        {
            return Err(refused(store.not_plain(INDEX, entry.kind)));
        }
    }
    let mut n = 1;
    let file = loop {
        let file = entry_name(&prefix, title, n);
        match store.entry(&file).map_err(refused)? {
            // Something is there: the next number, as a name that exists takes the next one.
            // A link is never written through; one out of the project is refused in its own
            // words, as it always was.
            Some(entry) => {
                if entry.link() {
                    gates.gate(&file)?;
                }
            }
            None => {
                gates.gate(&file)?;
                if store
                    .create_whole(&file, body.as_bytes())
                    .map_err(refused)?
                {
                    break file;
                }
            }
        }
        n += 1;
    };
    if index {
        let line = format!("{}\n", index_line(title, &file));
        if let Err(why) = store.append(INDEX, INDEX_FALLBACK, line.as_bytes()) {
            // The file and its line go together, or neither stays.
            let _ = store.remove(&file);
            return Err(refused(why));
        }
    }
    Ok(file)
}

/// The path gates a held write still asks before it touches a name, so a link out of the
/// project is refused in the words it always was.
pub(crate) trait Gates {
    fn gate(&self, name: &str) -> io::Result<()>;
}

impl Gates for Spot<'_> {
    fn gate(&self, name: &str) -> io::Result<()> {
        gate(self.root, &self.path(name))
    }
}

/// No path to gate: the MCP tools refuse every link in a store before they write.
pub(crate) struct NoGates;

impl Gates for NoGates {
    fn gate(&self, _name: &str) -> io::Result<()> {
        Ok(())
    }
}

/// The memory `ident` names in `names`: its exact name, or the one whose name ends
/// `-<ident>.md` ([`super::resolve`]'s rule).
fn resolve_in(names: &[String], ident: &str) -> io::Result<String> {
    let exact = md_name(ident);
    if names.contains(&exact) {
        return Ok(exact);
    }
    let suffix = format!("-{exact}");
    let tails: Vec<&String> = names.iter().filter(|n| n.ends_with(&suffix)).collect();
    match tails.as_slice() {
        [] => Err(no_such(ident)),
        [one] => Ok((*one).clone()),
        many => Err(super::invalid(&format!(
            "'{ident}' is the end of more than one memory's name — {}; name one in full",
            many.iter()
                .map(|n| n.strip_suffix(".md").unwrap_or(n))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Rewrite the held store's index lines that link `file`: dropped, or retitled. Declines by
/// doing nothing, as [`super::rewrite_index_lines`] does: the memory has already moved.
pub(crate) fn rewrite_index(store: &Store, file: &str, title: Option<&str>) {
    match store.entry(INDEX) {
        Ok(Some(entry)) if entry.plain() && entry.size <= MAX_BYTES => {}
        _ => return,
    }
    let Ok(Some(text)) = store.read(INDEX) else {
        return;
    };
    if let Some(body) = index_rewritten(&text, file, title) {
        let _ = store.replace(INDEX, body.as_bytes());
    }
}

/// [`super::forget`] for a workspace's store, from the point its slug was checked.
pub(super) fn forget(spot: &Spot, ident: &str) -> io::Result<()> {
    let Some(store) = spot.held(Make::Nothing)? else {
        return Err(no_such(ident));
    };
    let _held = store.lock().map_err(busy)?;
    let file = resolve_in(&md_names(&store)?, ident)?;
    store.remove(&file).map_err(refused)?;
    rewrite_index(&store, &file, None);
    Ok(())
}

/// One memory's raw text, as it is on disk: `None` when it is not there.
fn raw(store: &Store, name: &str) -> io::Result<String> {
    store
        .read_raw(name)
        .map_err(refused)?
        .ok_or_else(|| no_such(name))
}

/// [`super::open`] for a workspace's store.
pub(super) fn open(spot: &Spot, ident: &str) -> io::Result<(PathBuf, String)> {
    let Some(store) = spot.held(Make::Nothing)? else {
        return Err(no_such(ident));
    };
    let name = md_name(ident);
    if !md_names(&store)?.contains(&name) {
        return Err(no_such(ident));
    }
    let text = raw(&store, &name)?;
    Ok((spot.path(&name), text))
}

/// [`super::edit`] for a workspace's store, from the point the text and title were checked.
pub(super) fn edit(
    spot: &Spot,
    ident: &str,
    title: &str,
    text: &str,
    base: Base,
) -> Result<PathBuf, EditRefused> {
    let Some(store) = spot.held(Make::Nothing)? else {
        return Err(no_such(ident).into());
    };
    let _held = store.lock().map_err(busy)?;
    let name = md_name(ident);
    if !md_names(&store)?.contains(&name) {
        return Err(no_such(ident).into());
    }
    let now = raw(&store, &name)?;
    if let Base::Read(read) = base
        && read != now
    {
        return Err(EditRefused::Stale);
    }
    let lines = crate::mdsection::split_lines(&now);
    let body = match super::top_lines(&lines).1 {
        Some((_, stamp)) => format!("# {title}\n\n{stamp}\n\n{text}\n"),
        None => format!("# {title}\n\n{text}\n"),
    };
    spot.gate(&name)?;
    spot.gate(INDEX)?;
    store.replace(&name, body.as_bytes()).map_err(refused)?;
    rewrite_index(&store, &name, Some(title));
    Ok(spot.path(&name))
}

/// [`super::archive_moving`] for a workspace's store.
pub(super) fn archive(spot: &Spot, ident: &str) -> io::Result<(PathBuf, bool)> {
    let archived = |name: &str| spot.dir.join(super::ARCHIVE).join(name);
    let Some(store) = spot.held(Make::Nothing)? else {
        return Err(no_such(ident));
    };
    let _held = store.lock().map_err(busy)?;
    let name = md_name(ident);
    if !md_names(&store)?.contains(&name) {
        return match store.sub(super::ARCHIVE, false).map_err(unheld)? {
            Some(archive) if md_names(&archive)?.contains(&name) => Ok((archived(&name), false)),
            _ => Err(no_such(ident)),
        };
    }
    gate(spot.root, &spot.dir.join(super::ARCHIVE))?;
    let archive = store
        .sub(super::ARCHIVE, true)
        .map_err(unheld)?
        .ok_or_else(|| no_such(ident))?;
    // A name that is taken gets `-2`, then `-2-3`: the stem of the name just tried is numbered.
    let mut dest = name.clone();
    let mut n = 2;
    while archive.entry(&dest).map_err(refused)?.is_some() {
        let stem = dest.strip_suffix(".md").unwrap_or(&dest).to_owned();
        let suffix = if dest.ends_with(".md") { ".md" } else { "" };
        dest = format!("{stem}-{n}{suffix}");
        n += 1;
    }
    spot.gate(&name)?;
    gate(spot.root, &archived(&dest))?;
    store.rename_into(&name, &archive, &dest).map_err(refused)?;
    rewrite_index(&store, &name, None);
    Ok((archived(&dest), true))
}

/// [`super::unarchive`] for a workspace's store, from the point its names were checked.
pub(super) fn unarchive(spot: &Spot, ident: &str, restore_as: Option<&str>) -> io::Result<PathBuf> {
    let Some(store) = spot.held(Make::Nothing)? else {
        return Err(no_such(ident));
    };
    let _held = store.lock().map_err(busy)?;
    let wanted = md_name(ident);
    let archive = store.sub(super::ARCHIVE, false).map_err(unheld)?;
    let in_archive = match &archive {
        Some(archive) => md_names(archive)?.contains(&wanted),
        None => false,
    };
    let (Some(archive), true) = (archive, in_archive) else {
        // Already back is the exact name it would have come back under, and only that.
        let back = md_name(restore_as.unwrap_or(ident));
        return if md_names(&store)?.contains(&back) {
            Ok(spot.path(&back))
        } else {
            Err(no_such(ident))
        };
    };
    let name = match restore_as {
        Some(name) => md_name(name),
        None => wanted.clone(),
    };
    if store.entry(&name).map_err(refused)?.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("the store already holds {name}, so {wanted} stays archived"),
        ));
    }
    gate(spot.root, &spot.dir.join(super::ARCHIVE).join(&wanted))?;
    spot.gate(&name)?;
    spot.gate(INDEX)?;
    let dest = spot.path(&name);
    let title = match archive.read(&wanted) {
        Ok(Some(text)) => title_in(&dest, &text),
        _ => name.strip_suffix(".md").unwrap_or(&name).to_owned(),
    };
    archive
        .rename_into(&wanted, &store, &name)
        .map_err(refused)?;
    if !listed_held(&store).contains(&name) {
        let line = format!("{}\n", index_line(&title, &name));
        store
            .append(INDEX, INDEX_FALLBACK, line.as_bytes())
            .map_err(refused)?;
    }
    Ok(dest)
}

/// [`super::duplicate_of`] for a workspace's store.
pub(super) fn duplicate_of(spot: &Spot, text: &str) -> Option<String> {
    let store = spot.held(Make::Nothing).ok()??;
    for name in md_names(&store).ok()? {
        let Ok(Some(raw)) = store.read(&name) else {
            continue;
        };
        if let Some(title) = super::duplicate_in(text, [raw.as_str()]) {
            return Some(title);
        }
    }
    None
}
