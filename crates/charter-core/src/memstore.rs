//! The per-file memory store: workspace memory, persona memory and todos all use it.
//!
//! A memory is one Markdown file named after its title, listed in a `MEMORY.md` index
//! beside it. The filename is part of the format — `ws todo done <slug>` closes a todo by
//! its file stem — so the slug rule is reproduced exactly rather than approximated.

/// Is `c` whitespace to Python's `str.strip()`?
///
/// **Not `char::is_whitespace`.** Python drives `strip()` off `str.isspace()`, which is true
/// for the four separator controls U+001C–U+001F; Rust's `White_Space` property is false for
/// all four. The difference is not cosmetic here: `write` refuses an empty body so that a
/// failed substitution cannot land a secret in a memory file, and a body of only U+001F is
/// empty to Python and not to Rust.
pub fn is_python_space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{1f}')
}

/// `str.strip()`.
pub fn py_strip(text: &str) -> &str {
    text.trim_matches(is_python_space)
}

/// `str.rstrip()` — what a table row is trimmed with, so a row whose last column is empty
/// carries no trailing spaces into whatever reads it back.
pub fn py_rstrip(text: &str) -> &str {
    text.trim_end_matches(is_python_space)
}

/// The longest a memory title may be: it becomes a heading, an index row and part of a
/// filename.
pub const TITLE_MAX: usize = 72;

/// The index every store keeps beside its files.
pub const INDEX: &str = "MEMORY.md";

#[cfg(unix)]
mod holding;
#[cfg(unix)]
pub(crate) use holding::{NoGates, rewrite_index, write_in};

/// A store's lock, taken by [`lock_store`] and let go when this is dropped.
pub(crate) enum StoreLock {
    /// [`crate::rewrite::Lock`], on a store reached by path.
    Path(#[allow(dead_code)] crate::rewrite::Lock),
    /// A workspace's store, held and locked by descriptor for a bounded wait.
    #[cfg(unix)]
    Held(#[allow(dead_code)] crate::held::Store),
    /// A workspace's store that is not there yet: nothing to take turns on.
    None,
}

/// Take `dir`'s lock as every writer of a store takes it. A workspace's store is locked through
/// the store held by descriptor, and a lock another process keeps for longer than the bound is
/// refused in a sentence rather than waited on for ever (`WouldBlock`); any other store waits,
/// as [`crate::rewrite::Lock::on`] does.
pub(crate) fn lock_store(
    root: &std::path::Path,
    dir: &std::path::Path,
) -> std::io::Result<StoreLock> {
    #[cfg(unix)]
    match holding::reach(root, dir) {
        holding::Reach::Held(spot) => {
            return match spot.held(crate::held::Make::Nothing)? {
                Some(store) => store
                    .locked()
                    .map(StoreLock::Held)
                    .map_err(|why| std::io::Error::new(std::io::ErrorKind::WouldBlock, why)),
                None => Ok(StoreLock::None),
            };
        }
        holding::Reach::Refused(why) => return Err(why),
        holding::Reach::ByPath => {}
    }
    Ok(StoreLock::Path(crate::rewrite::Lock::on(dir)))
}

/// Whether `dir` is a workspace's store, which every operation here reaches through the store
/// held by descriptor rather than by its path (V74): never on a platform without that.
pub(crate) fn held_store(root: &std::path::Path, dir: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        !matches!(holding::reach(root, dir), holding::Reach::ByPath)
    }
    #[cfg(not(unix))]
    {
        let _ = (root, dir);
        false
    }
}

/// The filename stem charter derives from a title.
///
/// Every run of characters outside `[a-z0-9]` becomes one `-`, the ends are trimmed, and
/// only then is the result cut to 48 characters — so a cut can leave a trailing `-`, and
/// charter keeps it.
///
/// **And the stem has to travel** (charter-app#96). A note titled "NUL" slugs to `nul`, and
/// a persona memory carries no `YYYYMMDD-HHMMSS-` prefix, so the file is `nul.md` — the null
/// device on Windows, where the write succeeds and the note is gone. The fix is here rather
/// than a refusal because this name is charter's own derivation and not one the operator
/// typed: the brief for #96 is that charter must not silently rename a WORKSPACE, and it has
/// always chosen this filename itself. A declared difference from the frozen Python, which
/// writes `nul.md`.
pub fn slug(title: &str) -> String {
    let lowered = title.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut in_run = false;
    for ch in lowered.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            in_run = false;
        } else if !in_run {
            out.push('-');
            in_run = true;
        }
    }
    let trimmed = out.trim_matches('-');
    // The cut is by characters and comes last, so it can leave the trailing `-` that
    // trimming had just removed from the end of the whole string.
    let cut: String = trimmed.chars().take(48).collect();
    if cut.is_empty() {
        return "note".to_string();
    }
    // The alphabet above is `[a-z0-9-]` with the ends trimmed, so of everything
    // `contain::mintable` refuses only a DOS device stem can reach here — `nul`, `con`,
    // `com1`. `-note` rather than a number, because a number is what the collision loop in
    // `write` appends and this is not a collision.
    if crate::contain::mintable(&cut).is_err() {
        return format!("{cut}-note");
    }
    cut
}

/// The title `write` derives from a body: its first non-blank line, stripped, capped at 72.
pub fn title_of(text: &str) -> String {
    // One strip, on the line: charter's reader derives the same string, and a second
    // spelling of "first line, stripped, capped" is how the two come to disagree.
    for line in crate::mdsection::split_lines(text) {
        let line = py_strip(line);
        if !line.is_empty() {
            return line.chars().take(TITLE_MAX).collect();
        }
    }
    String::new()
}

/// Create the store directory and its `MEMORY.md` (with `header`) when absent; return the
/// index path.
pub fn ensure_index(
    root: &std::path::Path,
    dir: &std::path::Path,
    header: &str,
) -> std::io::Result<std::path::PathBuf> {
    let index = dir.join(INDEX);
    // The FILE, not the directory above it: a committed `MEMORY.md -> outside` escaped a
    // gate that only asked about the store.
    gate(root, &index)?;
    // The directory too, and before it is made: the index being contained does not stop
    // `create_dir_all` walking a symlinked store out of the plane.
    gate(root, dir)?;
    #[cfg(unix)]
    match holding::reach(root, dir) {
        holding::Reach::Held(spot) => return holding::ensure_index(&spot, header),
        holding::Reach::Refused(e) => return Err(e),
        holding::Reach::ByPath => {}
    }
    std::fs::create_dir_all(dir)?;
    if !index.exists() {
        let header = if header.ends_with('\n') {
            header.to_string()
        } else {
            format!("{header}\n")
        };
        // Exclusive: `exists()` is false for a dangling link, and a plain write would
        // have created whatever that link named.
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&index)
        {
            Ok(mut f) => std::io::Write::write_all(&mut f, header.as_bytes())?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Ok(index)
}

/// Write one memory file into `dir` and, unless `index` is false, append it to the index.
#[allow(clippy::too_many_arguments)]
pub fn write(
    root: &std::path::Path,
    dir: &std::path::Path,
    text: &str,
    title: Option<&str>,
    timestamped: bool,
    kind: &str,
    index: bool,
    stamp: chrono::NaiveDateTime,
) -> std::io::Result<std::path::PathBuf> {
    let text = py_strip(text);
    if text.is_empty() {
        // charter raises here, and the reason is not tidiness: a secret must never reach a
        // memory file, and an empty body is how a failed substitution arrives.
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "empty memory",
        ));
    }
    let title = match title {
        Some(t) => py_strip(t).chars().take(TITLE_MAX).collect::<String>(),
        None => title_of(text),
    };
    // BEFORE the mkdir. `create_dir_all` through a symlinked store creates directories
    // outside the plane, and a gate that runs after the side effect is no gate: this made
    // `memory/` and `todos/` appear beside an operator's files.
    gate(root, dir)?;
    // A workspace's store is held by descriptor from here on (V74).
    #[cfg(unix)]
    match holding::reach(root, dir) {
        holding::Reach::Held(spot) => {
            return holding::write(&spot, text, &title, timestamped, kind, index, stamp);
        }
        holding::Reach::Refused(e) => return Err(e),
        holding::Reach::ByPath => {}
    }
    // charter's own state — the ephemeral quadrant under `.charter/` — is private whatever
    // the umask: directories it makes at 0700 and the file at 0600 (`config.mkdir_for`,
    // `config.write_for`). A committed store is the operator's to mode, and is left to it.
    let private = under_state(root, dir);
    if private {
        crate::trace::private_mkdir(dir)?;
    } else {
        std::fs::create_dir_all(dir)?;
    }

    // Held from choosing the name to appending the index line (SI-9d). The append is `O_APPEND`
    // on the index's inode, and an edit's retitle replaces that inode with what it read before
    // the append: without the store's lock a line appended in between went with the old inode
    // (120 lines of 400 in a stress run). And two writes of one title in one second both chose
    // the same free name, and the second replaced the first.
    let _held = crate::rewrite::Lock::on(dir);
    let prefix = if timestamped {
        stamp.format("%Y%m%d-%H%M%S-").to_string()
    } else {
        String::new()
    };
    let mut n = 1;
    let mut path = dir.join(entry_name(&prefix, &title, n));
    // Same title in the same second: keep both, numbered, as charter does.
    while path.exists() {
        n += 1;
        path = dir.join(entry_name(&prefix, &title, n));
    }

    let body = memory_body(&title, kind, text, stamp);
    // The chosen name too: the collision loop stops on the first name nothing occupies,
    // and a dangling link occupies nothing.
    gate(root, &path)?;
    // And the index, before a byte is written: charter checks both targets up front, so a
    // refused index leaves the store exactly as it was rather than holding a memory file
    // nothing indexes.
    if index {
        gate(root, &dir.join(INDEX))?;
    }
    // Replaced whole (#434): a link planted at the chosen name
    // after the gate above answered is refused or replaced, never written through, and a
    // crash leaves no half-written memory. Charter's own state is 0600 whatever the file
    // had; a committed store keeps the operator's mode.
    let mode = if private {
        crate::rewrite::Mode::Private
    } else {
        crate::rewrite::Mode::Kept
    };
    // Gated from the store itself: `gate` has answered for the directories above it, and a
    // link among them that stays inside the plane is followed, as it always was.
    crate::rewrite::replace(dir, &path, body.as_bytes(), mode)?;
    if index {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if let Err(e) = index_append(root, &dir.join(INDEX), &name, &title) {
            // The file and its line go together, or neither stays (#1058): refused at the
            // append itself, the memory just written is taken away again.
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
    }
    Ok(path)
}

/// The `n`th name a memory titled `title` may take: `<prefix><slug>.md`, then `-2`, `-3`… for
/// the same title in the same second. One spelling for every writer of a store, the CLI's and
/// the MCP tools' alike.
pub(crate) fn entry_name(prefix: &str, title: &str, n: usize) -> String {
    let base = slug(title);
    if n <= 1 {
        format!("{prefix}{base}.md")
    } else {
        format!("{prefix}{base}-{n}.md")
    }
}

/// What an index is made with when its store's own header was not written first.
pub(crate) const INDEX_FALLBACK: &str = "# Memory Index\n\n";

/// A memory file's whole text, as [`write()`] writes it.
pub(crate) fn memory_body(
    title: &str,
    kind: &str,
    text: &str,
    stamp: chrono::NaiveDateTime,
) -> String {
    format!(
        "# {title}\n\n_{} · {kind}_\n\n{text}\n",
        stamp.format("%Y-%m-%d %H:%M")
    )
}

/// Is `path` in the plane's state directory, `.charter/` or `.purlis/` — charter's own, and
/// private? Either spelling, whichever is in use: both are private.
fn under_state(root: &std::path::Path, path: &std::path::Path) -> bool {
    crate::names::STATE_DIR
        .spellings()
        .any(|state| path.starts_with(root.join(state)))
}

/// Append one `- [title](file)` line ([`index_line`]). Order is write order: charter never
/// sorts this file.
///
/// Takes no lock itself: [`write()`], [`unarchive`], `curate::apply_safe` and a workspace's
/// legacy `notes.md` line (`Workspace::index_legacy_memo`) call it holding the store's
/// [`crate::rewrite::Lock`], which a second `Lock::on` in the same process would wait for.
pub fn index_append(
    root: &std::path::Path,
    index: &std::path::Path,
    filename: &str,
    title: &str,
) -> std::io::Result<()> {
    gate(root, index)?;
    #[cfg(unix)]
    if let Some(dir) = index.parent() {
        match holding::reach(root, dir) {
            holding::Reach::Held(spot) => {
                gate(root, dir)?;
                return holding::index_append(&spot, filename, title);
            }
            holding::Reach::Refused(e) => return Err(e),
            holding::Reach::ByPath => {}
        }
    }
    if let Some(parent) = index.parent()
        && std::fs::symlink_metadata(index).is_err()
    {
        gate(root, parent)?;
        std::fs::create_dir_all(parent)?;
    }
    let line = format!("{}\n", index_line(title, filename));
    // Neither open follows a link at the index (#420): `create_new` is `O_EXCL`, which never
    // does, and the append is `O_NOFOLLOW`. Containment answered for the path a moment ago; a
    // link swapped in since is refused rather than written through.
    let mut fresh = std::fs::OpenOptions::new();
    fresh.write(true).create_new(true);
    match crate::contain::nofollow(&mut fresh).open(index) {
        Ok(mut f) => {
            std::io::Write::write_all(&mut f, format!("{INDEX_FALLBACK}{line}").as_bytes())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let mut more = std::fs::OpenOptions::new();
            more.append(true);
            let mut f = crate::contain::nofollow(&mut more).open(index)?;
            // Opened without waiting (`nofollow` is non-blocking too), and only a file takes
            // the line.
            if !f.metadata()?.is_file() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("{} is not a file", index.display()),
                ));
            }
            std::io::Write::write_all(&mut f, line.as_bytes())
        }
        Err(e) => Err(e),
    }
}

/// Find a memory file by its full filename or by a bare slug — the lookup for a name a person
/// TYPED (`ws todo done write-the-migration`, `charter workspace archive <slug>`).
///
/// The exact name wins; otherwise the one file whose name ends `-<ident>.md`, which is how a
/// timestamped store is addressed by the slug alone. **More than one such file is refused**
/// (`InvalidInput`, naming each) rather than answered with the first in sorted order, as
/// charter's resolver answered: `deploy` of a store holding `a-deploy.md` and `prod-deploy.md`
/// names neither, and taking one deleted or moved a memory nobody named (SI-9d). `NotFound`
/// when nothing matches.
///
/// **Only for a typed name.** The operations the window calls with a slug it was handed —
/// [`open`], [`edit`], [`archive_one`], [`unarchive`] — take the exact name and nothing else
/// (`resolve_exact`): a tab whose memory was archived by something else must find it gone,
/// not find `prod-deploy.md` under `deploy`.
///
/// **Every candidate is an entry of the listing** (`memstore.resolve`, #336): contained, a
/// regular file, within the bound, and never the index. Without that `forget` reached a FIFO,
/// a directory named `x.md` or a link out of the plane by naming it — the same read arriving by
/// a shorter route — and on a filesystem that ignores case, `memory` reached `MEMORY.md`.
///
/// **Stricter than charter in one place, on purpose.** charter trusts a non-link below a
/// store it has checked, because a listed entry cannot have moved relative to its
/// directory. A NAME is not a listed entry: `../../<elsewhere>` is not a link and walks
/// out all the same. This asks containment of every path, so a traversing ident resolves
/// to nothing here even where charter's resolver would hand it back.
pub fn resolve(
    root: &std::path::Path,
    dir: &std::path::Path,
    ident: &str,
) -> std::io::Result<std::path::PathBuf> {
    resolve_among(root, &[dir], ident)
}

/// [`resolve`] over several directories: an exact name in any of them first, in the order
/// given; otherwise the files whose name ends `-<ident>.md` in the first directory that has any
/// — one is the answer, more is refused.
fn resolve_among(
    root: &std::path::Path,
    dirs: &[&std::path::Path],
    ident: &str,
) -> std::io::Result<std::path::PathBuf> {
    if let Some(exact) = dirs.iter().find_map(|dir| resolve_exact(root, dir, ident)) {
        return Ok(exact);
    }
    let suffix = format!("-{}", md_name(ident));
    let tails: Vec<std::path::PathBuf> = dirs
        .iter()
        .map(|dir| {
            read_files(root, dir)
                .0
                .into_iter()
                .filter(|p| {
                    p.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .ends_with(&suffix)
                })
                .collect::<Vec<_>>()
        })
        .find(|found| !found.is_empty())
        .unwrap_or_default();
    match tails.as_slice() {
        [] => Err(no_such(ident)),
        [one] => Ok(one.clone()),
        many => Err(invalid(&format!(
            "'{ident}' is the end of more than one memory's name — {}; name one in full",
            many.iter()
                .map(|p| p.file_stem().unwrap_or_default().to_string_lossy())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// What `unarchive` is told to undo an archive of the memory `name` that landed at `archived`:
/// its stem there, and ` --as <name's stem>` when archiving had to number it — without which
/// the Undo a command prints restores `foo-2`, not `foo` (SI-9d).
pub fn undo_args(name: &str, archived: &std::path::Path) -> String {
    let stem = archived.file_stem().unwrap_or_default().to_string_lossy();
    let own = name.strip_suffix(".md").unwrap_or(name);
    if stem == own {
        stem.into_owned()
    } else {
        format!("{stem} --as {own}")
    }
}

/// Where a typed slug is looked for by [`typed_name`]: the store, or the store and its archive
/// in the order the verb wants them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typed {
    /// `edit`: the store only.
    Stored,
    /// `archive`: the store, then `archive/` — a memory already archived is where it is.
    Archiving,
    /// `unarchive`: `archive/`, then the store — a memory already back is where it is.
    Restoring,
}

/// The exact filename a slug a person typed on the command line names, for the verb `typed` —
/// which the exact operations ([`edit`], [`archive_one`], [`unarchive`]) are then handed.
///
/// [`resolve`]'s rule across the directories `typed` names: an exact name in any of them wins,
/// then the one file whose name ends `-<slug>.md` in the first of them that has any, and more
/// than one there is refused. So `archive <slug>` takes the store's entry over an older one of
/// the same name already archived, and an exact name in the archive beats a tail in the store:
/// `archive foo` twice finds `foo.md` archived the second time, and never goes on to
/// `old-foo.md` (SI-9d).
pub fn typed_name(
    root: &std::path::Path,
    dir: &std::path::Path,
    ident: &str,
    typed: Typed,
) -> std::io::Result<String> {
    one_segment(ident)?;
    let archive = dir.join(ARCHIVE);
    let dirs: &[&std::path::Path] = match typed {
        Typed::Stored => &[dir],
        Typed::Archiving => &[dir, &archive],
        Typed::Restoring => &[&archive, dir],
    };
    let found = resolve_among(root, dirs, ident)?;
    Ok(found
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned())
}

/// The comparable words in `text` — `memstore.wordset`'s tokenizer.
///
/// Lowercased, split on runs of non-word characters, and **only words longer than three
/// characters** survive. Both near-duplicate callers must tokenize identically or the shared
/// threshold means two different things.
pub fn wordset(text: &str) -> std::collections::BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| w.chars().count() > 3)
        .map(str::to_string)
        .collect()
}

/// A memory's content with its `# title` and `_stamp_` lines dropped and whitespace
/// normalised — the basis for duplicate comparison.
pub fn comparable_body(text: &str) -> String {
    let kept: Vec<&str> = crate::mdsection::split_lines(text)
        .into_iter()
        .filter(|line| !line.starts_with("# "))
        .filter(|line| {
            let t = line.trim();
            !(t.starts_with('_') && t.ends_with('_') && t.contains('·'))
        })
        .collect();
    kept.join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Above this share of words in common, two todos are "the same intent".
pub const DUPLICATE_THRESHOLD: f64 = 0.5;

/// The title of a stored todo that already says this, or `None`.
///
/// Duplicate INTENT is worse than duplicate memory: closing one of a near-identical pair
/// leaves its twin looking outstanding, so the list starts lying about what is left. charter
/// warns and skips rather than merging, so the writer learns it is already there.
///
/// Jaccard — intersection over UNION. Dividing by the longer side looks equivalent and is
/// not: on text this short a single shared word is half the content, and "first thing" and
/// "second thing" scored 0.5.
pub fn duplicate_of(root: &std::path::Path, dir: &std::path::Path, text: &str) -> Option<String> {
    crate::contain::readable(root, dir).ok()?;
    let words = wordset(text);
    if words.is_empty() {
        return None;
    }
    #[cfg(unix)]
    match holding::reach(root, dir) {
        holding::Reach::Held(spot) => return holding::duplicate_of(&spot, text),
        holding::Reach::Refused(e) => {
            return {
                let _ = e;
                None
            };
        }
        holding::Reach::ByPath => {}
    }
    // Sorted, as charter's `files()` is: which of two near-duplicates is named must not
    // depend on directory order.
    let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        if path.extension().is_none_or(|e| e != "md") || path.file_name()? == INDEX {
            continue;
        }
        // Each ENTRY, before it is read. The filters above are an extension, a name and a
        // parse — none of them containment — so a committed `leak.md -> /outside/secret.md`
        // was read here, its heading echoed by `ws todo <text>`'s refusal, and the rest of
        // it used as the Jaccard oracle. Same gate `read_store` applies, for the same
        // reason: in charter this listing IS the gate (`memstore.files`).
        if crate::contain::readable(root, &path).is_err() {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(title) = duplicate_in(text, [raw.as_str()]) {
            return Some(title);
        }
    }
    None
}

/// The title of the first of `stored` (each a stored todo's whole text) that already says
/// `text`, by [`duplicate_of`]'s rule, or `None`.
pub(crate) fn duplicate_in<'a>(
    text: &str,
    stored: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let words = wordset(text);
    if words.is_empty() {
        return None;
    }
    for raw in stored {
        let title = title_of_stored(raw);
        let other = wordset(&format!("{title} {}", comparable_body(raw)));
        if other.is_empty() {
            continue;
        }
        let shared = words.intersection(&other).count() as f64;
        let union = words.union(&other).count() as f64;
        if shared / union >= DUPLICATE_THRESHOLD {
            return Some(title);
        }
    }
    None
}

/// The fewest words two TITLES must share before their overlap says anything about them —
/// `todos._MIN_SHARED_WORDS`. [`same_work`] only.
const MIN_SHARED_WORDS: usize = 3;

/// The first of `titles` that names the same work as `subject`, a todo's first line —
/// `todos._same_work`.
///
/// **Below `MIN_SHARED_WORDS` (three) shared words, identity decides**: `Update the README` and
/// `Update the README file` share two words, which cannot tell "retitled by a word" from
/// "different work sharing a word", so they are two todos unless they read the same once
/// case and runs of whitespace are set aside. Past it, Jaccard against
/// [`DUPLICATE_THRESHOLD`], as [`duplicate_of`] measures.
pub fn same_work<'a>(subject: &str, titles: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let words = wordset(subject);
    let mine = normal(subject);
    titles.into_iter().find(|title| {
        let other = wordset(title);
        let shared = words.intersection(&other).count();
        if shared < MIN_SHARED_WORDS {
            return mine == normal(title);
        }
        shared as f64 / words.union(&other).count() as f64 >= DUPLICATE_THRESHOLD
    })
}

/// `todos._normal`: case and runs of whitespace are not differences between two titles.
/// `to_lowercase` where Python has `casefold`; the two part only on a few letters such as
/// `ß`, where this reads two titles as different that Python reads as one.
fn normal(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The `# ` heading of a stored memory.
fn title_of_stored(raw: &str) -> String {
    crate::mdsection::split_lines(raw)
        .into_iter()
        .find_map(|line| line.strip_prefix("# "))
        .map(|rest| py_strip(rest).to_string())
        .unwrap_or_default()
}

/// Delete one memory and its index line. The index is rewritten as the surviving lines
/// joined with `\n` plus one trailing `\n` — and nothing at all when none survive.
///
/// `NotFound` when nothing matched, which a caller turns into its own sentence, and
/// `InvalidInput` for a slug the ends of two files' names share ([`resolve`]).
///
/// Under the store's [`crate::rewrite::Lock`], as every other rewrite of its index is: a drop
/// that read the index while an edit's retitle was replacing it would put back the line the
/// retitle changed, or lose the one it wrote.
pub fn forget(root: &std::path::Path, dir: &std::path::Path, ident: &str) -> std::io::Result<()> {
    // The slug is untrusted: `../../../victim` resolved out of the plane and `remove_file`
    // took it. charter refuses a slug that is not one path segment (#339).
    // One check, on the name with any `.md` taken off: `../../victim.md` is a legal
    // FILENAME and not a legal slug, and the first spelling of this let it through.
    if !crate::contain::segment_ok(ident.strip_suffix(".md").unwrap_or(ident)) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("'{ident}' is not the slug of one todo"),
        ));
    }
    // And never the index, which the lookup below cannot return but the refusal should name.
    one_segment(ident)?;
    #[cfg(unix)]
    match holding::reach(root, dir) {
        holding::Reach::Held(spot) => return holding::forget(&spot, ident),
        holding::Reach::Refused(e) => return Err(e),
        holding::Reach::ByPath => {}
    }
    let _held = crate::rewrite::Lock::on(dir);
    let file = resolve(root, dir, ident)?;
    gate(root, &file)?;
    std::fs::remove_file(&file)?;
    drop_index_line(
        root,
        dir,
        &file.file_name().unwrap_or_default().to_string_lossy(),
    );
    Ok(())
}

/// Remove `filename`'s line from the store's index — the store's only TRUNCATING write.
///
/// Refuses by doing nothing, as charter's `_drop_index_line` does and for its reason: this
/// runs after the memory file has already been removed or moved, so an error here would
/// report a failure for work that succeeded. An index charter declines to touch keeps its
/// stale line, which is drift the next `optimize` names. What it declines: an index that
/// is not there, a store that resolves out of the plane, and an index that is not a plain,
/// contained, bounded file — pointed at a credential store, a rewrite destroys it outright.
fn drop_index_line(root: &std::path::Path, dir: &std::path::Path, filename: &str) {
    rewrite_index_lines(root, dir, filename, None);
}

/// An index's text with every line that links `filename` dropped: the surviving lines joined
/// with `\n` plus one trailing `\n`, and nothing at all when none survive.
pub(crate) fn index_dropping(text: &str, filename: &str) -> String {
    let needle = format!("({filename})");
    let kept: Vec<&str> = crate::mdsection::split_lines(text)
        .into_iter()
        .filter(|line| match leading_link(line) {
            Some((_, file)) => file != filename,
            // A line in a shape charter never writes is read as charter always read it.
            None => !line.contains(&needle),
        })
        .collect();
    if kept.is_empty() {
        String::new()
    } else {
        format!("{}\n", kept.join("\n"))
    }
}

/// One index line, `- [title](file)`, with `\\`, `[` and `]` in the title escaped (SI-9d): a
/// title holding `](b.md)` would otherwise read as the end of its own line's link, and a retitle
/// of `b.md` rewrote the line of the memory whose title mentioned it. Escaped, the title is
/// the text a Markdown reader shows.
pub fn index_line(title: &str, filename: &str) -> String {
    let mut escaped = String::with_capacity(title.len());
    for c in title.chars() {
        if matches!(c, '\\' | '[' | ']') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    format!("- [{escaped}]({filename})")
}

/// The link that closes a line's leading `- [..]` element: the byte just past its `)`, and the
/// file it links. `None` for a line that does not start `- [` or whose element never closes.
///
/// The element closes at the first `]` directly followed by `(` that is not inside a bracket
/// pair the title opened, an escaped character never counting ([`index_line`]). So a line an
/// older charter wrote with its title as it was, `- [see [x](b.md) here](a.md)`, still links
/// `a.md`. An older unbalanced title that itself holds `](` is the one shape this cannot read
/// back; charter has escaped every title it wrote since.
fn leading_link(line: &str) -> Option<(usize, &str)> {
    let rest = line.strip_prefix("- [")?;
    let bytes = rest.as_bytes();
    let mut depth = 1usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'[' => depth += 1,
            b']' if depth == 1 && bytes.get(i + 1) == Some(&b'(') => {
                let open = i + 2;
                let close = open + rest[open..].find(')')?;
                let start = line.len() - rest.len();
                return Some((start + close + 1, &rest[open..close]));
            }
            b']' if depth > 1 => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

/// Rewrite the index lines that link `filename`: dropped when `title` is `None`, retitled in
/// place when it is `Some` — `- [{title}]({filename})` up to and including the link, whatever
/// followed the link kept (ADR 0065). The one truncating writer of an index, so a retitle
/// declines exactly what a deletion declines, and for the same reasons.
fn rewrite_index_lines(
    root: &std::path::Path,
    dir: &std::path::Path,
    filename: &str,
    title: Option<&str>,
) {
    let index = dir.join(INDEX);
    if !index.exists() || gate(root, dir).is_err() || !readable_file(root, &index) {
        return;
    }
    let Some(text) = read_text(&index) else {
        return;
    };
    let Some(body) = index_rewritten(&text, filename, title) else {
        return;
    };
    // Replaced whole and never through a link (#434): a link swapped in after
    // `readable_file` answered is refused or replaced rather than truncated through. Gated
    // from the store, which `gate` answered for above. A committed index keeps its mode, as
    // the in-place write kept it; one under `.charter/` is charter's own, and 0600.
    let mode = if under_state(root, dir) {
        crate::rewrite::Mode::Private
    } else {
        crate::rewrite::Mode::Kept
    };
    let _ = crate::rewrite::replace(dir, &index, body.as_bytes(), mode);
}

/// An index's `text` with the lines that link `filename` dropped (`title` is `None`) or
/// retitled in place — [`rewrite_index_lines`]'s rule, for a store held by path or by
/// descriptor. `None` when a retitle changes nothing, so nothing is written.
fn index_rewritten(text: &str, filename: &str, title: Option<&str>) -> Option<String> {
    let Some(title) = title else {
        return Some(index_dropping(text, filename));
    };
    let retitled: Vec<String> = crate::mdsection::split_lines(text)
        .into_iter()
        .map(|line| match leading_link(line) {
            Some((end, file)) if file == filename => {
                format!("{}{}", index_line(title, filename), &line[end..])
            }
            _ => line.to_string(),
        })
        .collect();
    if retitled
        .iter()
        .zip(crate::mdsection::split_lines(text))
        .all(|(new, old)| new == old)
    {
        return None;
    }
    Some(if retitled.is_empty() {
        String::new()
    } else {
        format!("{}\n", retitled.join("\n"))
    })
}

// ---------------------------------------------------------------------------------------
// Reading a store the way `charter recall` reads it: `memstore.read_files`, `entries`,
// `search`, `memory_date`, `body`, `duplicates` and the index-drift check. Every entry of
// every store passes ONE gate, `readable_file`, which is `contain.file_refusal`'s question.

/// A path charter could not look at, and the errno the look met. `(path, None)` when the
/// filesystem gave no number.
pub type Unread = Vec<(std::path::PathBuf, Option<i32>)>;

/// The bound on one plane file charter reads whole (`contain.MAX_BYTES`). Set where
/// nothing an editor produces can reach it, so it never fires on anything a person wrote.
pub const MAX_BYTES: u64 = 1_048_576;

/// How much of a path a sentence repeats back — `contain.PATH_DISPLAY_LIMIT`. A clipped
/// path is one a reader cannot go to.
pub const PATH_LIMIT: usize = 1024;

/// What clears a path charter could not check — `workspace.uncheckable_fix`: a symlink loop
/// names the link, because no permission bit is in its way; anything else is read as a
/// refusal, which read access clears.
pub fn uncheckable_fix(code: Option<i32>, path: &str, place: &str) -> String {
    if crate::recall::is_loop(code) {
        format!("fix the symlink loop at {path}")
    } else {
        format!("restoring read access to {place} clears this")
    }
}

/// The one sentence for a path charter could not look at — `workspace.cannot_check`, with
/// the full stop `say_unread` adds.
///
/// Named from the plane's root and made READABLE first. The path is often a filename a chat
/// wrote: printed as it was, a newline in one writes a line of its own into whatever reads
/// this, and an escape reaches the terminal (charter #1084).
pub fn cannot_check(root: &std::path::Path, path: &std::path::Path, code: Option<i32>) -> String {
    let shown = crate::shown::readable(&path.to_string_lossy(), PATH_LIMIT);
    let named = crate::shown::readable(
        &path.strip_prefix(root).unwrap_or(path).to_string_lossy(),
        PATH_LIMIT,
    );
    format!(
        "{named} cannot be checked — {}.",
        uncheckable_fix(code, &shown, "it")
    )
}

/// May charter READ `path` as one plane file? `contain.file_refusal`, answered as a yes.
///
/// Three questions, each of which has let something through on its own before (#336):
/// - it resolves inside the plane's data directories — a committed `leak.md ->
///   /elsewhere/secret.md` is not a memory, and reading it is how `duplicate_of` leaked a
///   heading and a similarity oracle for the rest of the file;
/// - it is a regular file once any link is followed — a FIFO blocks the read for ever and
///   a device never ends;
/// - it is no larger than [`MAX_BYTES`].
///
/// Containment is asked of every path, link or not. charter asks it of links only,
/// because an entry LISTED from a checked directory cannot have moved; a path built from a
/// name can (`resolve`), and this is the one gate both reach.
pub fn readable_file(root: &std::path::Path, path: &std::path::Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    meta.is_file() && meta.len() <= MAX_BYTES && crate::contain::readable(root, path).is_ok()
}

/// The memory files in `dir` charter may read, sorted, and beside them each it could not
/// look at — `memstore.read_files` (#1084).
///
/// A store that is not there, or that resolves out of the plane, holds nothing and says
/// nothing: the first is empty and the second is containment's refusal. A store that is
/// there and cannot be LISTED is itself unread. An entry whose own `lstat` is refused is
/// unread; an entry the gate refuses is simply not a memory.
///
/// **A directory it could not look at is not an empty one.** "No memories match" of a
/// search that never looked reads as the fact not existing, which is why the caller names
/// what is in the second list.
pub fn read_files(
    root: &std::path::Path,
    dir: &std::path::Path,
) -> (Vec<std::path::PathBuf>, Unread) {
    if crate::contain::readable(root, dir).is_err() {
        return (Vec::new(), Vec::new());
    }
    #[cfg(unix)]
    match holding::reach(root, dir) {
        holding::Reach::Held(spot) => return holding::read_files(&spot),
        holding::Reach::Refused(e) => {
            return {
                let _ = e;
                (Vec::new(), Vec::new())
            };
        }
        holding::Reach::ByPath => {}
    }
    let reader = match std::fs::read_dir(dir) {
        Ok(reader) => reader,
        Err(e) if is_absent(&e) => return (Vec::new(), Vec::new()),
        Err(e) => return (Vec::new(), vec![(dir.to_path_buf(), e.raw_os_error())]),
    };
    let mut paths = Vec::new();
    for entry in reader {
        match entry {
            Ok(entry) => paths.push(entry.path()),
            Err(e) => return (Vec::new(), vec![(dir.to_path_buf(), e.raw_os_error())]),
        }
    }
    paths.sort();
    let mut found = Vec::new();
    let mut unread = Vec::new();
    for path in paths {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name == INDEX || !name.ends_with(".md") {
            continue;
        }
        if readable_file(root, &path) {
            found.push(path);
            continue;
        }
        // Refused: asked why only here, so a readable store pays no second `lstat`. An entry
        // whose `lstat` itself is refused is one charter could not look at, not one it refused.
        if let Err(e) = std::fs::symlink_metadata(&path)
            && !is_absent(&e)
        {
            unread.push((path, e.raw_os_error()));
        }
    }
    (found, unread)
}

/// `FileNotFoundError` or `NotADirectoryError` — the two answers charter reads as "not there".
///
/// Shared, because "is this path gone" must have ONE answer across the core: read as gone, an
/// EACCES or an EIO makes charter write where it cannot see, and every other errno proves
/// nothing about the file.
pub fn is_absent(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

/// A file's text as Python's `Path.read_text()` gives it: UTF-8, with every `\r\n` and lone
/// `\r` read as `\n` — text mode's universal newlines, which decide where a `^` in a
/// multi-line pattern can match. `None` for a file that cannot be read or is not UTF-8.
///
/// charter crashes on a memory file that is not UTF-8 (its readers catch `OSError`, and
/// `UnicodeDecodeError` is not one — charter#1142). This skips it, as it skips a file it
/// cannot read.
pub fn read_text(path: &std::path::Path) -> Option<String> {
    text_of(std::fs::read(path).ok()?)
}

/// A store file's bytes as [`read_text`] reads them: UTF-8, with line endings made `\n`.
pub(crate) fn text_of(bytes: Vec<u8>) -> Option<String> {
    let text = String::from_utf8(bytes).ok()?;
    if text.contains('\r') {
        Some(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Some(text)
    }
}

/// One memory as a reader sees it: where it is, its title, and its whole text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: std::path::PathBuf,
    /// The first `# ` line with the two characters and the ends taken off, else the stem.
    pub title: String,
    pub text: String,
}

/// The title a reader gives a memory: the first line starting `# `, the rest of it
/// stripped, else the file's stem — `memstore._entries_of`.
pub fn title_in(path: &std::path::Path, text: &str) -> String {
    crate::mdsection::split_lines(text)
        .into_iter()
        .find_map(|line| line.strip_prefix("# "))
        .map(|rest| py_strip(rest).to_string())
        .unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
}

/// Every readable memory in `dir`, and what could not be looked at — `memstore.read_entries`.
/// A file that fails to read after the listing is skipped, as charter skips it.
pub fn read_entries(root: &std::path::Path, dir: &std::path::Path) -> (Vec<Found>, Unread) {
    #[cfg(unix)]
    if crate::contain::readable(root, dir).is_ok() {
        match holding::reach(root, dir) {
            holding::Reach::Held(spot) => return holding::read_entries(&spot),
            holding::Reach::Refused(_) => return (Vec::new(), Vec::new()),
            holding::Reach::ByPath => {}
        }
    }
    let (files, unread) = read_files(root, dir);
    let found = files
        .into_iter()
        .filter_map(|path| {
            let text = read_text(&path)?;
            let title = title_in(&path, &text);
            Some(Found { path, title, text })
        })
        .collect();
    (found, unread)
}

/// The entries of every one of `dirs`, in order, with what could not be read added to
/// `unread` — `memstore.gather` with a list to name them in.
pub fn gather(
    root: &std::path::Path,
    dirs: &[std::path::PathBuf],
    unread: &mut Unread,
) -> Vec<Found> {
    let mut out = Vec::new();
    for dir in dirs {
        let (found, missed) = read_entries(root, dir);
        out.extend(found);
        unread.extend(missed);
    }
    out
}

/// Words carrying no signal, dropped so they cannot outvote the term that mattered —
/// `memstore._STOPWORDS`, word for word.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "if", "then", "than", "that", "this", "these", "those",
    "of", "in", "on", "at", "to", "for", "from", "by", "with", "without", "as", "is", "are", "was",
    "were", "be", "been", "being", "it", "its", "it's", "do", "does", "did", "done", "have", "has",
    "had", "you", "your", "we", "our", "i", "my", "me", "they", "them", "their", "he", "she",
    "his", "her", "not", "no", "yes", "so", "such", "can", "could", "should", "would", "will",
    "shall", "may", "might", "must", "about", "into", "over", "under", "again", "more", "most",
    "some", "any",
];

/// Is `c` a word character to Python's `re` (`\w`): alphanumeric or `_`.
///
/// Rust's `is_alphanumeric` and Python's `str.isalnum` agree on every letter and digit a
/// memory is written in; they part on the spacing marks of some Indic scripts, which Rust
/// counts as alphabetic and Python does not. `wordset` has always carried the same residue.
pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `re.split(r"\W+", text.lower())` without the empty strings at either end.
fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !is_word(c))
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// The query tokens worth scoring — `memstore._terms`. Two characters is the floor (`S3`,
/// `CI`, `db` are real searches); a stopword is dropped. Order and repeats are kept, so a
/// word typed twice counts twice, as it does in charter.
pub fn terms(query: &str) -> Vec<String> {
    tokens(query)
        .into_iter()
        .filter(|t| t.chars().count() >= 2 && !STOPWORDS.contains(&t.as_str()))
        .collect()
}

/// The tokens [`terms`] discarded, so a caller can tell "nothing matched" from "nothing
/// was searched for".
pub fn dropped_terms(query: &str) -> Vec<String> {
    tokens(query)
        .into_iter()
        .filter(|t| t.chars().count() < 2 || STOPWORDS.contains(&t.as_str()))
        .collect()
}

/// How many times `\b<term>\w*` matches in `hay` — a term found at the START of a word.
///
/// Every character of a term is a word character (it came out of a `\W+` split), so the
/// boundary before it is exactly "the character before is not a word character, or there
/// is none". A match then runs to the end of its word, so the next can only start in a
/// later word: counting starts is counting matches.
fn word_prefix_count(hay: &str, term: &str) -> usize {
    let mut count = 0;
    let mut prev: Option<char> = None;
    for (i, c) in hay.char_indices() {
        if prev.is_none_or(|p| !is_word(p)) && hay[i..].starts_with(term) {
            count += 1;
        }
        prev = Some(c);
    }
    count
}

/// Keyword-rank the memories of `dirs` against `query` — `memstore.search`.
///
/// A title hit weighs three, a hit anywhere in the text one. Best first; ties by the path
/// as a string, which is how charter breaks them. What could not be read goes to `unread`
/// and the rest is still searched.
pub fn search(
    root: &std::path::Path,
    dirs: &[std::path::PathBuf],
    query: &str,
    limit: usize,
    unread: &mut Unread,
) -> Vec<(Found, usize)> {
    if terms(query).is_empty() {
        return Vec::new();
    }
    rank(gather(root, dirs, unread), query, limit)
}

/// [`search`]'s ranking of memories already read: a title hit weighs three, a hit anywhere in
/// the text one, best first, ties by the path as a string.
pub fn rank(found: Vec<Found>, query: &str, limit: usize) -> Vec<(Found, usize)> {
    let terms = terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(usize, String, Found)> = Vec::new();
    for found in found {
        let low = found.text.to_lowercase();
        let title = found.title.to_lowercase();
        let score: usize = terms
            .iter()
            .map(|t| 3 * word_prefix_count(&title, t) + word_prefix_count(&low, t))
            .sum();
        if score > 0 {
            let key = found.path.to_string_lossy().into_owned();
            scored.push((score, key, found));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(score, _, found)| (found, score))
        .collect()
}

/// The date a memory was recorded — `memstore.memory_date`.
///
/// The in-body `_YYYY-MM-DD…_` stamp first: the FIRST line anywhere that starts `_` and a
/// date followed by a space or a `T`. Only the first such line is asked — charter's
/// `re.search` stops there, so a stamp that does not parse falls straight to the filename
/// rather than to a later stamp. Then a `YYYYMMDD-` filename prefix. `None` for neither.
pub fn memory_date(text: &str, filename: &str) -> Option<chrono::NaiveDate> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    for start in starts {
        let line = &text[start..];
        let Some(date) = stamp_at(line) else {
            continue;
        };
        if let Some(day) = iso_date(date) {
            return Some(day);
        }
        break;
    }
    let digits: Vec<char> = filename.chars().take(9).collect();
    if digits.len() == 9 && digits[..8].iter().all(char::is_ascii_digit) && digits[8] == '-' {
        let n = |range: std::ops::Range<usize>| -> Option<u32> {
            digits[range].iter().collect::<String>().parse().ok()
        };
        return chrono::NaiveDate::from_ymd_opt(n(0..4)? as i32, n(4..6)?, n(6..8)?)
            .filter(|d| (1..=9999).contains(&chrono::Datelike::year(d)));
    }
    None
}

/// `_(\d{4}-\d{2}-\d{2})[ T]` at the start of `line`, giving the date part.
///
/// `\d` is any decimal digit to Python, not only ASCII, and a stamp spelled in other
/// digits still MATCHES there and then fails to parse. So a numeral outside ASCII is
/// accepted here as a digit and refused by [`iso_date`], which puts the date on the same
/// path charter's does: the stamp is found, it does not parse, and the filename decides.
fn stamp_at(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('_')?;
    let mut end = 0;
    for (n, c) in rest.chars().enumerate() {
        let ok = if n == 4 || n == 7 {
            c == '-'
        } else {
            c.is_ascii_digit() || (!c.is_ascii() && c.is_numeric())
        };
        if !ok {
            return None;
        }
        end += c.len_utf8();
        if n == 9 {
            break;
        }
    }
    let date = &rest[..end];
    if date.chars().count() != 10 {
        return None;
    }
    matches!(rest[end..].chars().next(), Some(' ' | 'T')).then_some(date)
}

/// `date.fromisoformat` for `YYYY-MM-DD` in ASCII digits, year 1 to 9999.
fn iso_date(text: &str) -> Option<chrono::NaiveDate> {
    let b = text.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i32 = text.get(0..4)?.parse().ok()?;
    let m: u32 = text.get(5..7)?.parse().ok()?;
    let d: u32 = text.get(8..10)?.parse().ok()?;
    if !text.get(0..4)?.bytes().all(|c| c.is_ascii_digit())
        || !text.get(5..7)?.bytes().all(|c| c.is_ascii_digit())
        || !text.get(8..10)?.bytes().all(|c| c.is_ascii_digit())
        || y < 1
    {
        return None;
    }
    chrono::NaiveDate::from_ymd_opt(y, m, d)
}

/// A memory's content with its `# title` and `_stamp_` lines dropped and every run of
/// whitespace one space, stripped and lowercased — `memstore.body`, the key two memories
/// are EXACT duplicates by.
///
/// Whitespace is Python's (`\s`, `str.strip`), which includes U+001C–U+001F; Rust's does
/// not, and a body differing only in those would be two facts to one and one to the other.
pub fn body(text: &str) -> String {
    let kept: Vec<&str> = crate::mdsection::split_lines(text)
        .into_iter()
        .filter(|line| !line.starts_with("# ") && !is_stamp_line(py_strip(line)))
        .collect();
    let joined = kept.join(" ");
    let mut out = String::with_capacity(joined.len());
    let mut in_space = false;
    for c in joined.chars() {
        if is_python_space(c) {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    py_strip(&out).to_lowercase()
}

/// `^_.*·.*_\s*$` on a stripped line: starts and ends with `_`, a `·` between.
pub(crate) fn is_stamp_line(line: &str) -> bool {
    let Some(inner) = line.strip_prefix('_') else {
        return false;
    };
    inner.ends_with('_') && inner[..inner.len() - 1].contains('·')
}

/// Near-duplicate pairs among the memories of `dirs`, by Jaccard word overlap at or above
/// `threshold` — `memstore.duplicates`. Strongest first; pairs in listing order within a
/// score. Each pair is `(score, first, second)` as indexes into the returned entries.
pub fn duplicates(entries: &[Found], threshold: f64) -> Vec<(f64, usize, usize)> {
    let sets: Vec<std::collections::BTreeSet<String>> =
        entries.iter().map(|e| wordset(&e.text)).collect();
    let mut out = Vec::new();
    for (i, a) in sets.iter().enumerate() {
        for (j, b) in sets.iter().enumerate().skip(i + 1) {
            if a.is_empty() || b.is_empty() {
                continue;
            }
            let shared = a.intersection(b).count() as f64;
            let union = a.union(b).count() as f64;
            let jaccard = shared / union;
            if jaccard >= threshold {
                out.push((jaccard, i, j));
            }
        }
    }
    // Stable, as Python's sort is: equal scores keep their listing order.
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// The filenames the index links to — `memstore._listed`. Empty when charter may not
/// read the index, which makes every file read as unindexed: true, since nothing charter
/// is willing to read indexes them.
///
/// **A line is read as a retitle and a drop read it** (SI-9e): a line starting `- [` lists the
/// file its leading element links ([`leading_link`]) and nothing else in it, so a title that
/// mentions `(b.md)` does not list `b.md` — which had [`unarchive`] skip `b`'s line and left it
/// unindexed. That link is still read by charter's pattern ([`index_links`]), and so is the
/// whole of a line of any other shape.
///
/// Asked with the WRITE rule (`index_refusal`), as charter asks it: an absent index is no
/// defect — a fresh persona has none — while a dangling link out of the plane is absent
/// and hostile at once.
pub fn listed(root: &std::path::Path, dir: &std::path::Path) -> std::collections::BTreeSet<String> {
    let index = dir.join(INDEX);
    if gate(root, dir).is_err() || gate(root, &index).is_err() {
        return std::collections::BTreeSet::new();
    }
    #[cfg(unix)]
    match holding::reach(root, dir) {
        holding::Reach::Held(spot) => return holding::listed(&spot),
        holding::Reach::Refused(e) => {
            return {
                let _ = e;
                std::collections::BTreeSet::new()
            };
        }
        holding::Reach::ByPath => {}
    }
    if std::fs::symlink_metadata(&index).is_ok() && !readable_file(root, &index) {
        return std::collections::BTreeSet::new();
    }
    let Some(text) = read_text(&index) else {
        return std::collections::BTreeSet::new();
    };
    listed_in(&text)
}

/// The files an index's `text` lists, by [`listed`]'s reading.
fn listed_in(text: &str) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for line in crate::mdsection::split_lines(text) {
        match leading_link(line) {
            // The link still has to be one charter's pattern reads as a memory's file: a hand's
            // `- [docs](https://…)` lists nothing, as it never did.
            Some((_, file)) => out.extend(index_links(&format!("({file})"))),
            None => out.extend(index_links(line)),
        }
    }
    out
}

/// `\(([A-Za-z0-9][\w.-]*\.md)\)`, every non-overlapping match, left to right.
///
/// `)` is outside the class, so a match's closing `)` is the first character after the
/// run of class characters that follows `(` — the run has to end in `.md` and hold more
/// than just that. Reading it that way needs no backtracking and gives what `findall` does.
fn index_links(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let in_class = |c: char| is_word(c) || c == '.' || c == '-';
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '(' {
            i += 1;
            continue;
        }
        let start = i + 1;
        if !chars.get(start).is_some_and(char::is_ascii_alphanumeric) {
            i += 1;
            continue;
        }
        let mut end = start + 1;
        while end < chars.len() && in_class(chars[end]) {
            end += 1;
        }
        let run: String = chars[start..end].iter().collect();
        if chars.get(end) == Some(&')') && run.chars().count() > 3 && run.ends_with(".md") {
            out.push(run);
            i = end + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// What the index and the directory disagree about — `memstore.index_drift`: links to a
/// file that is not there (`dangling`) and files no link names (`unindexed`), each sorted.
///
/// `Err` with what could not be looked at when the store itself could not be listed, which
/// charter raises as `CannotCheck` rather than reporting drift it did not measure.
pub fn index_drift(
    root: &std::path::Path,
    dir: &std::path::Path,
) -> Result<(Vec<String>, Vec<String>), Unread> {
    let (files, unread) = read_files(root, dir);
    if !unread.is_empty() {
        return Err(unread);
    }
    let actual: std::collections::BTreeSet<String> = files
        .iter()
        .map(|p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let listed = listed(root, dir);
    Ok((
        listed.difference(&actual).cloned().collect(),
        actual.difference(&listed).cloned().collect(),
    ))
}

/// Move one memory into `<dir>/archive/` and drop its index line — `memstore.archive`, a
/// reversible retire that keeps the file. The path it landed at, or `None` when nothing
/// was archived.
///
/// [`archive_one`] answered as an `Option`, so `optimize`'s collapse and the window's Delete
/// move a file by one rule (ADR 0065).
pub fn archive(
    root: &std::path::Path,
    dir: &std::path::Path,
    ident: &str,
) -> Option<std::path::PathBuf> {
    // A move, and only a move: "already archived" is `archive_one`'s success, and would have
    // `optimize` report a collapse it did not make (SI-9d).
    match archive_moving(root, dir, ident) {
        Ok((path, true)) => Some(path),
        _ => None,
    }
}

/// The directory a store's retired memories are moved into.
pub const ARCHIVE: &str = "archive";

/// Move one memory into `<dir>/archive/` and drop its index line; the path it is at now.
///
/// `archive` is the fifth fixed name in the store: `rename` follows a link on the
/// destination DIRECTORY exactly as `open` follows one on a file, so a committed
/// `archive -> elsewhere` would turn a retire into a move out of the plane. It is gated
/// before anything is made or moved.
///
/// A name that is taken gets `-2`, then `-2-3`, then `-2-3-4`: charter numbers the stem
/// of the name it just tried, not the original, and the files it leaves are named that way.
///
/// **The exact name, and no other** (SI-9d): `foo` is `foo.md`, never `old-foo.md`. A name a
/// person typed is made exact first ([`typed_name`]).
///
/// **Safe to repeat.** A memory that is not in the store and IS in `archive/` under that name
/// has already been archived, and that is where it is: `Ok`, nothing moved. `NotFound` only
/// when it is in neither, and `InvalidInput` for a slug that is not one path segment.
pub fn archive_one(
    root: &std::path::Path,
    dir: &std::path::Path,
    ident: &str,
) -> std::io::Result<std::path::PathBuf> {
    archive_moving(root, dir, ident).map(|(path, _)| path)
}

/// [`archive_one`], and whether it moved anything: `false` when the memory was already in
/// `archive/`.
fn archive_moving(
    root: &std::path::Path,
    dir: &std::path::Path,
    ident: &str,
) -> std::io::Result<(std::path::PathBuf, bool)> {
    one_segment(ident)?;
    #[cfg(unix)]
    match holding::reach(root, dir) {
        holding::Reach::Held(spot) => return holding::archive(&spot, ident),
        holding::Reach::Refused(e) => return Err(e),
        holding::Reach::ByPath => {}
    }
    let dest_dir = dir.join(ARCHIVE);
    let _held = crate::rewrite::Lock::on(dir);
    let Some(file) = resolve_exact(root, dir, ident) else {
        return resolve_exact(root, &dest_dir, ident)
            .map(|already| (already, false))
            .ok_or_else(|| no_such(ident));
    };
    gate(root, &dest_dir)?;
    std::fs::create_dir_all(&dest_dir)?;
    let name = file
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut dest = dest_dir.join(&name);
    let mut n = 2;
    while dest.exists() {
        let stem = dest
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let suffix = dest
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        dest = dest_dir.join(format!("{stem}-{n}{suffix}"));
        n += 1;
    }
    gate(root, &file)?;
    gate(root, &dest)?;
    std::fs::rename(&file, &dest)?;
    drop_index_line(root, dir, &name);
    Ok((dest, true))
}

/// Move one memory from `<dir>/archive/` back into the store and append its index line; the
/// path it is at now.
///
/// `ident` names the file in `archive/` exactly (SI-9d), as its filename or its stem. It is restored under
/// its own name, or under `restore_as` — how an undo puts back a memory [`archive_one`] had to
/// number. The index line is `- [{title}]({filename})`, the title read as every reader reads
/// one ([`title_in`]), and it is appended only when the index does not already list the file.
///
/// **Safe to repeat**, and never over anything. A name the store already holds while the
/// archived file is still there is refused with `AlreadyExists` and nothing moves; a memory
/// that is back in the store and no longer in `archive/` has already been restored: `Ok`.
/// `NotFound` when it is in neither.
pub fn unarchive(
    root: &std::path::Path,
    dir: &std::path::Path,
    ident: &str,
    restore_as: Option<&str>,
) -> std::io::Result<std::path::PathBuf> {
    one_segment(ident)?;
    if let Some(name) = restore_as {
        one_segment(name)?;
    }
    #[cfg(unix)]
    match holding::reach(root, dir) {
        holding::Reach::Held(spot) => return holding::unarchive(&spot, ident, restore_as),
        holding::Reach::Refused(e) => return Err(e),
        holding::Reach::ByPath => {}
    }
    let src_dir = dir.join(ARCHIVE);
    let _held = crate::rewrite::Lock::on(dir);
    let Some(file) = resolve_exact(root, &src_dir, ident) else {
        // Already back is the exact name it would have come back under, and only that: a
        // store file whose name merely ends in it was never archived (SI-9d).
        return resolve_exact(root, dir, restore_as.unwrap_or(ident)).ok_or_else(|| no_such(ident));
    };
    let name = match restore_as {
        Some(name) => md_name(name),
        None => file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    };
    let dest = dir.join(&name);
    // `exists` is false for a dangling link, and `rename` would replace one; it is gated
    // below, and a live file of that name is the operator's and is never replaced.
    if std::fs::symlink_metadata(&dest).is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!(
                "the store already holds {name}, so {} stays archived",
                file.file_name().unwrap_or_default().to_string_lossy()
            ),
        ));
    }
    let index = dir.join(INDEX);
    gate(root, &file)?;
    gate(root, &dest)?;
    gate(root, &index)?;
    let title = read_text(&file)
        .map(|text| title_in(&dest, &text))
        .unwrap_or_else(|| {
            dest.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
    std::fs::rename(&file, &dest)?;
    if !listed(root, dir).contains(&name) {
        index_append(root, &index, &name, &title)?;
    }
    Ok(dest)
}

/// What an edit is checked against (ADR 0065).
#[derive(Debug, Clone, Copy)]
pub enum Base<'a> {
    /// The memory file's whole text as the caller read it. A file that differs on disk now is
    /// refused as [`EditRefused::Stale`].
    Read(&'a str),
    /// Write whatever the file holds now — the window's Overwrite, after a stale refusal.
    Overwrite,
}

/// Why an edit wrote nothing.
#[derive(Debug)]
pub enum EditRefused {
    /// The file changed on disk since the caller read it. Nothing was written; the window
    /// offers Reload or Overwrite.
    Stale,
    /// Anything else: `NotFound` for a slug the store does not hold, `InvalidInput` for a slug
    /// that is a path, an empty body or a title with a line break, and whatever the gate or
    /// the filesystem said.
    Io(std::io::Error),
}

impl std::fmt::Display for EditRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stale => {
                f.write_str("the memory changed on disk since it was read, so nothing was saved")
            }
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EditRefused {}

impl From<std::io::Error> for EditRefused {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// One line of a memory file's top: its index into the file's lines, and the line.
pub(crate) type TopLine<'a> = Option<(usize, &'a str)>;

/// Where a stored memory's heading and stamp lines are — the one reading of a memory file's
/// top that the reader (`workspaces::parse_entry`) and [`edit`] share.
///
/// The heading is the first `# ` line anywhere, as charter reads a title. The stamp is taken
/// from where the store writes it and nowhere else: the first line that is not blank after the
/// heading (after the start of the file when there is none), and only when that line has a
/// stamp's shape ([`is_stamp_line`]); it is given stripped.
pub(crate) fn top_lines<'a>(lines: &[&'a str]) -> (TopLine<'a>, TopLine<'a>) {
    let heading = lines
        .iter()
        .enumerate()
        .find(|(_, line)| line.starts_with("# "))
        .map(|(i, line)| (i, *line));
    let after_heading = heading.map_or(0, |(i, _)| i + 1);
    let stamp = lines
        .iter()
        .enumerate()
        .skip(after_heading)
        .find(|(_, line)| !py_strip(line).is_empty())
        .map(|(i, line)| (i, py_strip(line)))
        .filter(|(_, line)| is_stamp_line(line));
    (heading, stamp)
}

/// One memory file's path and whole text, exactly as it is on disk — the text an edit is
/// later checked against ([`Base::Read`]).
pub fn open(
    root: &std::path::Path,
    dir: &std::path::Path,
    ident: &str,
) -> std::io::Result<(std::path::PathBuf, String)> {
    one_segment(ident)?;
    #[cfg(unix)]
    match holding::reach(root, dir) {
        holding::Reach::Held(spot) => return holding::open(&spot, ident),
        holding::Reach::Refused(e) => return Err(e),
        holding::Reach::ByPath => {}
    }
    let file = resolve_exact(root, dir, ident).ok_or_else(|| no_such(ident))?;
    let text = std::fs::read_to_string(&file)?;
    Ok((file, text))
}

/// Rewrite one memory in place with a new title and text; the path, which does not change.
///
/// ADR 0065 and `docs/plane-format.md` → *Editing and archiving a memory*: the filename is
/// kept (the slug was minted from the first title and is how every command names it), the
/// stamp line the file had is kept verbatim, the title is stripped and capped at
/// [`TITLE_MAX`] (empty: the text's first line, as [`write`] derives one), and every index
/// line linking the file is retitled where it stands.
///
/// Checked against `base` and written under the store's [`crate::rewrite::Lock`], so nothing
/// that also takes it lands between "unchanged since it was read" and the rename.
pub fn edit(
    root: &std::path::Path,
    dir: &std::path::Path,
    ident: &str,
    title: &str,
    text: &str,
    base: Base,
) -> Result<std::path::PathBuf, EditRefused> {
    one_segment(ident)?;
    let text = py_strip(text);
    if text.is_empty() {
        // `write`'s refusal, for `write`'s reason: an empty body is how a failed substitution
        // arrives, and a secret must never reach a memory file.
        return Err(invalid("empty memory").into());
    }
    if title.contains(['\n', '\r']) {
        return Err(invalid("a memory's title is one line").into());
    }
    let title: String = py_strip(title).chars().take(TITLE_MAX).collect();
    let title = if title.is_empty() {
        title_of(text)
    } else {
        title
    };
    gate(root, dir)?;
    #[cfg(unix)]
    match holding::reach(root, dir) {
        holding::Reach::Held(spot) => return holding::edit(&spot, ident, &title, text, base),
        holding::Reach::Refused(e) => return Err(e.into()),
        holding::Reach::ByPath => {}
    }
    let _held = crate::rewrite::Lock::on(dir);
    let file = resolve_exact(root, dir, ident).ok_or_else(|| no_such(ident))?;
    let now = std::fs::read_to_string(&file)?;
    if let Base::Read(read) = base
        && read != now
    {
        return Err(EditRefused::Stale);
    }
    let lines = crate::mdsection::split_lines(&now);
    let body = match top_lines(&lines).1 {
        Some((_, stamp)) => format!("# {title}\n\n{stamp}\n\n{text}\n"),
        None => format!("# {title}\n\n{text}\n"),
    };
    gate(root, &file)?;
    gate(root, &dir.join(INDEX))?;
    let mode = if under_state(root, dir) {
        crate::rewrite::Mode::Private
    } else {
        crate::rewrite::Mode::Kept
    };
    crate::rewrite::replace(dir, &file, body.as_bytes(), mode)?;
    rewrite_index_lines(
        root,
        dir,
        &file.file_name().unwrap_or_default().to_string_lossy(),
        Some(&title),
    );
    Ok(file)
}

/// `name` without a journal's `YYYYMMDD-HHMMSS-` prefix: the name a memory has in any store,
/// and what a move asks a target store to be free of.
pub(crate) fn unstamped(name: &str) -> &str {
    let bytes = name.as_bytes();
    let prefixed = bytes.len() > 16
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'-'
        && bytes[9..15].iter().all(u8::is_ascii_digit)
        && bytes[15] == b'-';
    if prefixed { &name[16..] } else { name }
}

/// The name a memory called `name`, whose whole text is `text`, takes in a store whose names
/// are `timestamped` or not (KN-3): its name without the journal's prefix in a persona's store;
/// in a journal, its own name when it has a prefix already, else the prefix of its own stamp
/// line (`_YYYY-MM-DD HH:MM · kind_`, seconds `00`), else of `now` for a file with none. So a
/// journal lists it where it was recorded rather than where it was moved.
///
/// **A memory moved out of a journal and back has its first name again to the minute, not to
/// the second.** The stamp line, which is all the memory carries with it, holds minutes, so
/// `20260302-091437-a-fact.md` comes back as `20260302-091400-a-fact.md`; its text is the same
/// byte for byte. Keeping the seconds would mean writing them into the file or keeping the
/// journal's prefix in a persona's store, and either changes what a persona's memory is.
pub(crate) fn moved_name(
    name: &str,
    text: &str,
    timestamped: bool,
    now: chrono::NaiveDateTime,
) -> String {
    let bare = unstamped(name);
    if !timestamped {
        return bare.to_owned();
    }
    if bare != name {
        return name.to_owned();
    }
    let lines = crate::mdsection::split_lines(text);
    let stamped = top_lines(&lines)
        .1
        .and_then(|(_, line)| {
            let inner = line.strip_prefix('_')?;
            let when = inner.split(" · ").next()?;
            chrono::NaiveDateTime::parse_from_str(py_strip(when), "%Y-%m-%d %H:%M").ok()
        })
        .unwrap_or(now);
    format!("{}{bare}", stamped.format("%Y%m%d-%H%M%S-"))
}

/// Why a move found its target store taken.
fn taken(to: &std::path::Path, root: &std::path::Path, dest: &str) -> std::io::Error {
    let store = to.strip_prefix(root).unwrap_or(to);
    std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!(
            "{} already holds a memory named '{}', so nothing moved",
            crate::shown::readable(&store.to_string_lossy(), PATH_LIMIT),
            unstamped(dest).strip_suffix(".md").unwrap_or(dest)
        ),
    )
}

/// Move the memory named exactly `ident` from the store `from` into the store `to`, whose names
/// are `timestamped` or not; the path it is at now (KN-3).
///
/// **Whole, or not at all.** The file is renamed, never copied, so no copy is left behind; its
/// text is not touched, so its title and its stamp go with it. Its index line is appended in
/// `to` (an index made with `header` when `to` has none) and dropped in `from`. A refusal moves
/// nothing, and an append that fails after the rename renames the file back.
///
/// The name it takes is [`moved_name`]'s. Refused with `AlreadyExists` when `to` holds a memory
/// of that name once a journal's prefix is taken off either, `NotFound` when `from` does not
/// hold `ident`, `InvalidInput` for a slug that is a path or two stores that are one, and
/// `PermissionDenied` for a store or a name a link takes out of the project. Both stores are
/// locked for the whole move, in the order of their paths.
pub fn move_one(
    root: &std::path::Path,
    from: &std::path::Path,
    ident: &str,
    to: &std::path::Path,
    timestamped: bool,
    header: &str,
    now: chrono::NaiveDateTime,
) -> std::io::Result<std::path::PathBuf> {
    one_segment(ident)?;
    if from == to {
        return Err(invalid("a memory is moved to another store, not its own"));
    }
    gate(root, from)?;
    gate(root, to)?;
    #[cfg(unix)]
    {
        holding::move_one(root, from, ident, to, timestamped, header, now)
    }
    #[cfg(not(unix))]
    {
        move_by_path(root, from, ident, to, timestamped, header, now)
    }
}

/// [`move_one`] by path, where no store is held by descriptor.
#[cfg(not(unix))]
fn move_by_path(
    root: &std::path::Path,
    from: &std::path::Path,
    ident: &str,
    to: &std::path::Path,
    timestamped: bool,
    header: &str,
    now: chrono::NaiveDateTime,
) -> std::io::Result<std::path::PathBuf> {
    let file = resolve_exact(root, from, ident).ok_or_else(|| no_such(ident))?;
    let name = file
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let (_first, _second) = if from < to {
        (crate::rewrite::Lock::on(from), crate::rewrite::Lock::on(to))
    } else {
        (crate::rewrite::Lock::on(to), crate::rewrite::Lock::on(from))
    };
    let text = std::fs::read_to_string(&file)?;
    let dest = moved_name(&name, &text, timestamped, now);
    let target = to.join(&dest);
    let held = read_files(root, to).0;
    if std::fs::symlink_metadata(&target).is_ok()
        || held.iter().any(|p| {
            unstamped(&p.file_name().unwrap_or_default().to_string_lossy()) == unstamped(&dest)
        })
    {
        return Err(taken(to, root, &dest));
    }
    let index = to.join(INDEX);
    if let Ok(meta) = std::fs::symlink_metadata(&index)
        && !meta.is_file()
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("{} is not a file", index.display()),
        ));
    }
    gate(root, &file)?;
    gate(root, &target)?;
    gate(root, &index)?;
    let title = title_in(&target, &text);
    // Made only now, once every check has passed: a refusal leaves no empty store.
    std::fs::create_dir_all(to)?;
    std::fs::rename(&file, &target)?;
    if !index.exists() {
        let _ = std::fs::write(&index, header);
    }
    if let Err(e) = index_append(root, &index, &dest, &title) {
        let _ = std::fs::rename(&target, &file);
        return Err(e);
    }
    drop_index_line(root, from, &name);
    Ok(target)
}

/// Could `ident` be a memory's slug? One path segment once any `.md` is taken off —
/// `../../victim.md` is a legal FILENAME and not a legal slug (#339) — and not the store's
/// index: `MEMORY` is `MEMORY.md`, which `archive MEMORY` moved away whole (SI-9d).
///
/// The window asks it too, so a row it cannot act on runs nothing, and a new memory's tab is
/// keyed by a slug it refuses (`memories::DRAFT`).
pub fn slug_ok(ident: &str) -> bool {
    crate::contain::segment_ok(ident.strip_suffix(".md").unwrap_or(ident))
        && md_name(ident) != INDEX
}

/// [`slug_ok`], as the refusal every memory operation gives.
fn one_segment(ident: &str) -> std::io::Result<()> {
    if slug_ok(ident) {
        Ok(())
    } else {
        Err(invalid(&format!("'{ident}' is not the slug of one memory")))
    }
}

/// `ident` as a filename: `x` and `x.md` both name `x.md`.
pub(crate) fn md_name(ident: &str) -> String {
    if ident.ends_with(".md") {
        ident.to_string()
    } else {
        format!("{ident}.md")
    }
}

/// The file in `dir` named exactly `ident` (with `.md` added), when the listing holds it — so
/// never the index, whatever the filesystem's case rule, and never an entry the listing's gate
/// refuses ([`read_files`]).
fn resolve_exact(
    root: &std::path::Path,
    dir: &std::path::Path,
    ident: &str,
) -> Option<std::path::PathBuf> {
    let name = md_name(ident);
    read_files(root, dir)
        .0
        .into_iter()
        .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy() == name))
}

fn no_such(ident: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("no such memory: {ident}"),
    )
}

fn invalid(why: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, why.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every expectation here came from charter itself: `from charter.memstore import slug`.

    #[test]
    fn a_title_becomes_its_lowercase_words_joined_by_single_dashes() {
        assert_eq!(slug("Hello World"), "hello-world");
        assert_eq!(slug("MiXeD CaSe 123"), "mixed-case-123");
        assert_eq!(
            slug("The API returns 418 on Mondays"),
            "the-api-returns-418-on-mondays"
        );
    }

    #[test]
    fn dashes_at_the_ends_are_trimmed_before_anything_else() {
        assert_eq!(slug("  --Trailing dashes--  "), "trailing-dashes");
    }

    #[test]
    fn a_title_with_nothing_sluggable_in_it_is_called_note() {
        assert_eq!(slug("!!!"), "note");
        assert_eq!(slug(""), "note");
    }

    #[test]
    fn every_non_ascii_letter_is_a_separator_not_a_letter() {
        // Python lowercases first, then replaces anything outside [a-z0-9]; the leading
        // run is trimmed, the inner ones are not.
        assert_eq!(slug("Ünïcödé fäncy"), "n-c-d-f-ncy");
    }

    #[test]
    fn the_cut_to_48_happens_after_trimming_so_it_can_leave_a_trailing_dash() {
        assert_eq!(slug(&"a".repeat(60)), "a".repeat(48));
        assert_eq!(slug(&"x ".repeat(30)), "x-".repeat(24));
    }

    #[test]
    fn a_title_whose_filename_would_be_a_device_gets_one_that_travels() {
        // charter-app#96. A persona memory carries no timestamp prefix, so `nul.md` is the
        // whole filename — and on Windows that is the null device: the write succeeds, and
        // the note the operator just wrote is gone. A declared difference from the frozen
        // Python, which writes `nul.md`.
        assert_eq!(slug("NUL"), "nul-note");
        assert_eq!(slug("con"), "con-note");
        assert_eq!(slug("COM1"), "com1-note");
        assert_eq!(slug("Aux"), "aux-note");
        assert_eq!(slug("lpt9"), "lpt9-note");
        // The alphabet already made `com1.txt` into `com1-txt`, whose stem is not a device,
        // and a name that merely begins like one was never one.
        assert_eq!(slug("com1.txt"), "com1-txt");
        assert_eq!(slug("console"), "console");
        assert_eq!(slug("com10"), "com10");
    }

    #[test]
    fn every_filename_this_store_mints_travels() {
        // The pairing `worktree::name` keeps between its slug and its gate, kept here too:
        // the derivation is a convenience and the rule is the containment, and a test is
        // what stops the two drifting into a gap.
        for title in [
            "NUL",
            "con",
            "COM\u{00b9}",
            "Ship the widget",
            "!!!",
            "",
            "\u{00dc}n\u{00ef}c\u{00f6}d\u{00e9} f\u{00e4}ncy",
            &"a".repeat(60),
        ] {
            let name = format!("{}.md", slug(title));
            assert!(
                crate::contain::mintable(&name).is_ok(),
                "a memory titled {title:?} is filed as {name:?}, which does not travel"
            );
        }
    }
}

#[cfg(test)]
mod write_tests {
    use super::*;

    // The expectations here are what `charter.memstore.write` produced in a temp plane with
    // `stamp=datetime(2026, 3, 2, 9, 14)`.

    fn stamp() -> chrono::NaiveDateTime {
        "2026-03-02T09:14:00".parse().unwrap()
    }

    /// A real plane, because the store gate asks whether a path is inside one.
    fn store() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
        let store = dir.path().join("workspaces/alpha/todos");
        ensure_index(
            dir.path(),
            &store,
            "# Todos — workspace `alpha`\n\nOne line per todo; each links a file holding one thing this task still means to do.\nOpen or done — and done removes it, leaving its trace in the journal instead.\n",
        )
        .unwrap();
        (dir, store)
    }

    #[test]
    fn a_timestamped_memory_is_named_for_its_second_and_its_title() {
        let (tmp, store) = store();

        let path = write(
            tmp.path(),
            &store,
            "Review the rollout plan",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();

        assert_eq!(
            path.file_name().unwrap(),
            "20260302-091400-review-the-rollout-plan.md"
        );
    }

    #[test]
    fn a_memory_file_is_a_heading_a_stamp_line_and_the_body() {
        let (tmp, store) = store();

        let path = write(
            tmp.path(),
            &store,
            "Review the rollout plan",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Review the rollout plan\n\n_2026-03-02 09:14 · persistent_\n\nReview the rollout plan\n"
        );
    }

    #[test]
    fn the_title_of_a_multi_line_body_is_its_first_line_and_the_body_keeps_the_rest() {
        let (tmp, store) = store();

        let path = write(
            tmp.path(),
            &store,
            "Multi line\nbody here",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();

        assert_eq!(path.file_name().unwrap(), "20260302-091400-multi-line.md");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Multi line\n\n_2026-03-02 09:14 · persistent_\n\nMulti line\nbody here\n"
        );
    }

    #[test]
    fn two_memories_with_one_title_in_one_second_are_both_kept() {
        let (tmp, store) = store();

        write(
            tmp.path(),
            &store,
            "Review the rollout plan",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();
        let second = write(
            tmp.path(),
            &store,
            "Review the rollout plan",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();

        assert_eq!(
            second.file_name().unwrap(),
            "20260302-091400-review-the-rollout-plan-2.md"
        );
    }

    #[test]
    fn the_index_gains_one_line_per_memory_in_the_order_they_were_written() {
        let (tmp, store) = store();

        write(
            tmp.path(),
            &store,
            "Review the rollout plan",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();
        write(
            tmp.path(),
            &store,
            "Review the rollout plan",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();
        write(
            tmp.path(),
            &store,
            "Multi line\nbody here",
            None,
            true,
            "persistent",
            true,
            stamp(),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(store.join("MEMORY.md")).unwrap(),
            "# Todos — workspace `alpha`\n\nOne line per todo; each links a file holding one thing this task still means to do.\nOpen or done — and done removes it, leaving its trace in the journal instead.\n- [Review the rollout plan](20260302-091400-review-the-rollout-plan.md)\n- [Review the rollout plan](20260302-091400-review-the-rollout-plan-2.md)\n- [Multi line](20260302-091400-multi-line.md)\n"
        );
    }

    #[test]
    fn an_empty_body_is_refused_because_a_secret_must_never_be_written_here() {
        let (tmp, store) = store();

        assert!(
            write(
                tmp.path(),
                &store,
                "   \n\n ",
                None,
                true,
                "persistent",
                true,
                stamp()
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod duplicate_tests {
    use super::*;

    // Oracles from `charter.memstore.wordset` / `.body` and `charter.todos._same_text`.

    #[test]
    fn only_words_longer_than_three_characters_count() {
        assert_eq!(
            wordset("Review the rollout plan")
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["plan", "review", "rollout"],
            "`the` is three characters and is dropped"
        );
        assert_eq!(
            wordset("a ab abc abcd abcde")
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["abcd", "abcde"]
        );
    }

    #[test]
    fn a_word_is_unicode_so_an_accented_one_is_not_split_apart() {
        assert_eq!(
            wordset("café déjà-vu naïve")
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["café", "déjà", "naïve"]
        );
    }

    #[test]
    fn the_comparable_body_drops_the_heading_and_the_stamp_and_collapses_space() {
        assert_eq!(
            comparable_body(
                "# Title here\n\n_2026-03-02 09:14 · persistent_\n\nSome   body\ntext\n"
            ),
            "some body text"
        );
    }
}

/// Refuse a path that resolves out of the plane's data directories.
fn gate(root: &std::path::Path, path: &std::path::Path) -> std::io::Result<()> {
    crate::contain::writable(root, path)
        .map_err(|r| std::io::Error::new(std::io::ErrorKind::PermissionDenied, r.to_string()))
}

#[cfg(test)]
mod gate_tests {
    use super::*;

    fn stamp() -> chrono::NaiveDateTime {
        "2026-03-02T09:14:00".parse().unwrap()
    }

    fn plane() -> (tempfile::TempDir, std::path::PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
        let store = dir.path().join("personas/devops/memory");
        std::fs::create_dir_all(&store).unwrap();
        (dir, store, tempfile::tempdir().unwrap())
    }

    #[test]
    fn a_refused_index_leaves_no_memory_file_behind() {
        // Both targets are gated before a byte is written, as charter's `write` gates them:
        // a memory file on disk that nothing indexes is the half-write this prevents.
        let (dir, store, outside) = plane();
        std::fs::write(outside.path().join("idx"), "PRECIOUS\n").unwrap();
        std::os::unix::fs::symlink(outside.path().join("idx"), store.join(INDEX)).unwrap();

        let refused = write(
            dir.path(),
            &store,
            "A fact",
            None,
            false,
            "persistent",
            true,
            stamp(),
        );

        assert_eq!(
            refused.unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
        assert!(!store.join("a-fact.md").exists(), "no memory file was left");
        assert_eq!(
            std::fs::read_to_string(outside.path().join("idx")).unwrap(),
            "PRECIOUS\n"
        );
    }

    /// `write` into `store` with its index, after `plant` made the index refuse the line.
    #[cfg(unix)]
    fn refused_at_the_append(plant: impl FnOnce(&std::path::Path)) {
        let (dir, store, _outside) = plane();
        std::fs::write(store.join("fact.md"), "PRECIOUS\n").unwrap();
        plant(&store);

        let refused = write(
            dir.path(),
            &store,
            "A fact",
            None,
            false,
            "persistent",
            true,
            stamp(),
        );

        assert!(refused.is_err(), "{refused:?}");
        assert!(
            std::fs::symlink_metadata(store.join("a-fact.md")).is_err(),
            "a memory file nothing indexes was left"
        );
        assert_eq!(
            std::fs::read_to_string(store.join("fact.md")).unwrap(),
            "PRECIOUS\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_index_linked_inside_the_store_refuses_the_line_and_leaves_no_memory_file() {
        // #1058: the link passes containment, so the refusal comes at the append itself.
        refused_at_the_append(|store| {
            std::os::unix::fs::symlink(store.join("fact.md"), store.join(INDEX)).unwrap();
        });
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_index_refuses_the_line_and_leaves_no_memory_file() {
        use std::os::unix::fs::PermissionsExt;
        refused_at_the_append(|store| {
            std::fs::write(store.join(INDEX), "# Memory\n").unwrap();
            std::fs::set_permissions(store.join(INDEX), std::fs::Permissions::from_mode(0o444))
                .unwrap();
        });
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_at_the_index_refuses_the_line_at_once_and_leaves_no_memory_file() {
        // Opened without waiting for a reader: a FIFO must never hold the write for ever.
        refused_at_the_append(|store| {
            let made = crate::forklock::status(
                std::process::Command::new("mkfifo").arg(store.join(INDEX)),
            )
            .expect("mkfifo runs");
            assert!(made.success());
        });
    }

    #[cfg(unix)]
    #[test]
    fn an_index_that_is_a_link_is_refused_and_what_it_points_at_is_left_alone() {
        // A link to another file in the same store passes containment; the open refuses it.
        let (dir, store, _outside) = plane();
        std::fs::write(store.join("fact.md"), "PRECIOUS\n").unwrap();
        std::os::unix::fs::symlink(store.join("fact.md"), store.join(INDEX)).unwrap();

        assert!(index_append(dir.path(), &store.join(INDEX), "a.md", "A").is_err());
        assert_eq!(
            std::fs::read_to_string(store.join("fact.md")).unwrap(),
            "PRECIOUS\n"
        );

        // A dangling one is refused too: the header is not written through it.
        let gone = store.join("gone.md");
        std::fs::remove_file(store.join(INDEX)).unwrap();
        std::os::unix::fs::symlink(&gone, store.join(INDEX)).unwrap();
        assert!(index_append(dir.path(), &store.join(INDEX), "a.md", "A").is_err());
        assert!(!gone.exists(), "nothing was created where the link points");
    }

    /// Plant a link to `at_target` at the path being replaced, in the window between the
    /// gate and the rename — where an in-place write would have followed it. Returns whether
    /// the window was ever reached, so a writer that bypasses `rewrite` cannot pass vacuously.
    #[cfg(unix)]
    fn plant_in_the_window(
        at_target: std::path::PathBuf,
    ) -> (
        std::rc::Rc<std::cell::Cell<bool>>,
        crate::rewrite::hook::Unset,
    ) {
        let fired = std::rc::Rc::new(std::cell::Cell::new(false));
        let seen = std::rc::Rc::clone(&fired);
        let unset = crate::rewrite::hook::set(move |target, _temp| {
            seen.set(true);
            let _ = std::fs::remove_file(target);
            std::os::unix::fs::symlink(&at_target, target)
        });
        (fired, unset)
    }

    #[cfg(unix)]
    #[test]
    fn a_link_planted_at_a_new_memorys_name_is_never_written_through() {
        // #434: the name is gated, then written. A link that lands in between used to take
        // the whole memory to wherever it pointed.
        let (dir, store, outside) = plane();
        let theirs = outside.path().join("theirs");
        std::fs::write(&theirs, "THEIRS\n").unwrap();
        let (fired, _unset) = plant_in_the_window(theirs.clone());

        let path = write(
            dir.path(),
            &store,
            "A fact",
            None,
            false,
            "persistent",
            false,
            stamp(),
        )
        .unwrap();

        assert!(fired.get(), "the memory went through rewrite::replace");
        assert_eq!(std::fs::read_to_string(&theirs).unwrap(), "THEIRS\n");
        assert!(!path.is_symlink(), "the link was replaced, not followed");
    }

    #[test]
    fn a_memory_write_that_dies_before_its_rename_leaves_no_memory_behind() {
        let (dir, store, _outside) = plane();
        let _killed = crate::rewrite::hook::set(|_, _| Err(std::io::Error::other("killed")));

        let refused = write(
            dir.path(),
            &store,
            "A fact",
            None,
            false,
            "persistent",
            false,
            stamp(),
        );

        assert!(refused.is_err());
        assert_eq!(
            std::fs::read_dir(&store).unwrap().count(),
            0,
            "no temp, no memory"
        );
    }

    #[test]
    fn forgetting_a_memory_that_dies_while_dropping_its_line_leaves_the_index_whole() {
        // #434: the index is replaced whole, so a crash mid-rewrite leaves the old index,
        // stale line and all, rather than an empty or cut-short one.
        let (dir, store, _outside) = plane();
        std::fs::write(store.join("a.md"), "# A\n").unwrap();
        let index = "# Memory Index\n\n- [A](a.md)\n- [B](b.md)\n";
        std::fs::write(store.join(INDEX), index).unwrap();
        let _killed = crate::rewrite::hook::set(|_, _| Err(std::io::Error::other("killed")));

        forget(dir.path(), &store, "a").unwrap();

        assert_eq!(std::fs::read_to_string(store.join(INDEX)).unwrap(), index);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_planted_at_the_index_while_a_line_is_dropped_is_never_truncated_through() {
        let (dir, store, outside) = plane();
        std::fs::write(store.join("a.md"), "# A\n").unwrap();
        std::fs::write(store.join(INDEX), "# Memory Index\n\n- [A](a.md)\n").unwrap();
        let theirs = outside.path().join("theirs");
        std::fs::write(&theirs, "THEIRS\n").unwrap();
        let (fired, _unset) = plant_in_the_window(theirs.clone());

        forget(dir.path(), &store, "a").unwrap();

        assert!(fired.get(), "the index went through rewrite::replace");
        assert_eq!(std::fs::read_to_string(&theirs).unwrap(), "THEIRS\n");
        assert_eq!(
            std::fs::read_to_string(store.join(INDEX)).unwrap(),
            "# Memory Index\n\n"
        );
    }

    #[test]
    fn a_new_index_gets_its_header_and_then_its_lines_in_order() {
        let (dir, store, _outside) = plane();
        index_append(dir.path(), &store.join(INDEX), "a.md", "A").unwrap();
        index_append(dir.path(), &store.join(INDEX), "b.md", "B").unwrap();
        assert_eq!(
            std::fs::read_to_string(store.join(INDEX)).unwrap(),
            "# Memory Index\n\n- [A](a.md)\n- [B](b.md)\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_memory_under_the_state_directory_is_private() {
        // The ephemeral quadrant is charter's own state: 0700 directories and a 0600 file,
        // whatever the umask. A committed store is left to the operator's modes.
        use std::os::unix::fs::PermissionsExt;
        let (dir, _store, _outside) = plane();
        let scratch = dir
            .path()
            .join(".charter/persona-state/ephemeral/s1/devops");

        let path = write(
            dir.path(),
            &scratch,
            "Scratch",
            None,
            false,
            "ephemeral",
            false,
            stamp(),
        )
        .unwrap();

        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&scratch), 0o700);
        assert_eq!(mode(scratch.parent().unwrap()), 0o700);
    }

    #[test]
    fn a_name_is_resolved_only_to_a_file_the_listing_would_read() {
        // The direct hit asks the listing's whole question. `is_file()` alone let a link out
        // of the plane, named, reach `forget`.
        let (dir, store, outside) = plane();
        std::fs::write(outside.path().join("secret.md"), "# Secret\n").unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.md"), store.join("leak.md"))
            .unwrap();
        std::fs::write(store.join("real.md"), "# Real\n").unwrap();

        assert!(resolve(dir.path(), &store, "leak").is_err());
        assert!(resolve(dir.path(), &store, "leak.md").is_err());
        assert_eq!(
            resolve(dir.path(), &store, "real").unwrap(),
            store.join("real.md")
        );
    }
}
