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
    // charter's own state — the ephemeral quadrant under `.charter/` — is private whatever
    // the umask: directories it makes at 0700 and the file at 0600 (`config.mkdir_for`,
    // `config.write_for`). A committed store is the operator's to mode, and is left to it.
    let private = under_state(root, dir);
    if private {
        crate::trace::private_mkdir(dir)?;
    } else {
        std::fs::create_dir_all(dir)?;
    }

    let prefix = if timestamped {
        stamp.format("%Y%m%d-%H%M%S-").to_string()
    } else {
        String::new()
    };
    let base = slug(&title);
    let mut path = dir.join(format!("{prefix}{base}.md"));
    let mut n = 2;
    // Same title in the same second: keep both, numbered, as charter does.
    while path.exists() {
        path = dir.join(format!("{prefix}{base}-{n}.md"));
        n += 1;
    }

    let body = format!(
        "# {title}\n\n_{} · {kind}_\n\n{text}\n",
        stamp.format("%Y-%m-%d %H:%M")
    );
    // The chosen name too: the collision loop stops on the first name nothing occupies,
    // and a dangling link occupies nothing.
    gate(root, &path)?;
    // And the index, before a byte is written: charter checks both targets up front, so a
    // refused index leaves the store exactly as it was rather than holding a memory file
    // nothing indexes.
    if index {
        gate(root, &dir.join(INDEX))?;
    }
    // Replaced whole through the walk from `root` (#434): a link planted at the chosen name
    // after the gate above answered is refused or replaced, never written through, and a
    // crash leaves no half-written memory. Charter's own state is 0600 whatever the file
    // had; a committed store keeps the operator's mode.
    let mode = if private {
        crate::rewrite::Mode::Private
    } else {
        crate::rewrite::Mode::Kept
    };
    crate::rewrite::replace(root, &path, body.as_bytes(), mode)?;
    if index {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        index_append(root, &dir.join(INDEX), &name, &title)?;
    }
    Ok(path)
}

/// Is `path` in the plane's state directory, `.charter/` — charter's own, and private?
fn under_state(root: &std::path::Path, path: &std::path::Path) -> bool {
    path.starts_with(root.join(".charter"))
}

/// Append one `- [title](file)` line. Order is write order: charter never sorts this file.
pub fn index_append(
    root: &std::path::Path,
    index: &std::path::Path,
    filename: &str,
    title: &str,
) -> std::io::Result<()> {
    gate(root, index)?;
    if let Some(parent) = index.parent()
        && std::fs::symlink_metadata(index).is_err()
    {
        gate(root, parent)?;
        std::fs::create_dir_all(parent)?;
    }
    let line = format!("- [{title}]({filename})\n");
    // Neither open follows a link at the index (#420): `create_new` is `O_EXCL`, which never
    // does, and the append is `O_NOFOLLOW`. Containment answered for the path a moment ago; a
    // link swapped in since is refused rather than written through.
    let mut fresh = std::fs::OpenOptions::new();
    fresh.write(true).create_new(true);
    match crate::contain::nofollow(&mut fresh).open(index) {
        Ok(mut f) => {
            std::io::Write::write_all(&mut f, format!("# Memory Index\n\n{line}").as_bytes())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let mut more = std::fs::OpenOptions::new();
            more.append(true);
            let mut f = crate::contain::nofollow(&mut more).open(index)?;
            std::io::Write::write_all(&mut f, line.as_bytes())
        }
        Err(e) => Err(e),
    }
}

/// Find a memory file by its full filename or by a bare slug.
///
/// The direct name wins; otherwise any file whose name ends `-<ident>.md`, which is how a
/// timestamped store is addressed by the slug alone (`ws todo done write-the-migration`).
/// First match in sorted order, as charter's `files()` gives them.
///
/// **Both halves go through the entry gate** (`memstore.resolve`, #336). The direct hit
/// asks the filesystem rather than the listing, so it is asked the listing's question in
/// full: contained, a regular file, within the bound. Without it `forget` reached a FIFO,
/// a directory named `x.md` or a link out of the plane by naming it — the same read
/// arriving by a shorter route. The suffix half IS the listing, so what it cannot see this
/// cannot return.
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
) -> Option<std::path::PathBuf> {
    let name = if ident.ends_with(".md") {
        ident.to_string()
    } else {
        format!("{ident}.md")
    };
    let direct = dir.join(&name);
    if crate::contain::readable(root, dir).is_ok() && readable_file(root, &direct) {
        return Some(direct);
    }
    let suffix = format!("-{name}");
    let (files, _unread) = read_files(root, dir);
    files.into_iter().find(|p| {
        let n = p.file_name().unwrap_or_default().to_string_lossy();
        n == name || n.ends_with(&suffix)
    })
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
        let title = title_of_stored(&raw);
        let other = wordset(&format!("{title} {}", comparable_body(&raw)));
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
/// `NotFound` when nothing matched, which a caller turns into its own sentence.
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
    let Some(file) = resolve(root, dir, ident) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no such memory: {ident}"),
        ));
    };
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
    let index = dir.join(INDEX);
    if !index.exists() || gate(root, dir).is_err() || !readable_file(root, &index) {
        return;
    }
    let Some(text) = read_text(&index) else {
        return;
    };
    let needle = format!("({filename})");
    let kept: Vec<&str> = crate::mdsection::split_lines(&text)
        .into_iter()
        .filter(|line| !line.contains(&needle))
        .collect();
    let body = if kept.is_empty() {
        String::new()
    } else {
        format!("{}\n", kept.join("\n"))
    };
    // Replaced whole, through the walk and never through a link (#434): a link swapped in
    // after `readable_file` answered is refused rather than truncated through. The index
    // keeps its mode, as the in-place write kept it.
    let _ = crate::rewrite::replace(root, &index, body.as_bytes(), crate::rewrite::Mode::Kept);
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
    let bytes = std::fs::read(path).ok()?;
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
    let terms = terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(usize, String, Found)> = Vec::new();
    for found in gather(root, dirs, unread) {
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
/// Asked with the WRITE rule (`index_refusal`), as charter asks it: an absent index is no
/// defect — a fresh persona has none — while a dangling link out of the plane is absent
/// and hostile at once.
pub fn listed(root: &std::path::Path, dir: &std::path::Path) -> std::collections::BTreeSet<String> {
    let index = dir.join(INDEX);
    let mut out = std::collections::BTreeSet::new();
    if gate(root, dir).is_err() || gate(root, &index).is_err() {
        return out;
    }
    if std::fs::symlink_metadata(&index).is_ok() && !readable_file(root, &index) {
        return out;
    }
    let Some(text) = read_text(&index) else {
        return out;
    };
    out.extend(index_links(&text));
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
/// `archive` is the fifth fixed name in the store: `rename` follows a link on the
/// destination DIRECTORY exactly as `open` follows one on a file, so a committed
/// `archive -> elsewhere` would turn a retire into a move out of the plane. Refused by
/// doing nothing, like every other "nothing was archived", rather than raised out of a
/// half-finished batch.
///
/// A name that is taken gets `-2`, then `-2-3`, then `-2-3-4`: charter numbers the stem
/// of the name it just tried, not the original, and the files it leaves are named that way.
pub fn archive(
    root: &std::path::Path,
    dir: &std::path::Path,
    ident: &str,
) -> Option<std::path::PathBuf> {
    let file = resolve(root, dir, ident)?;
    let dest_dir = dir.join("archive");
    gate(root, &dest_dir).ok()?;
    std::fs::create_dir_all(&dest_dir).ok()?;
    let name = file.file_name()?.to_string_lossy().into_owned();
    let mut dest = dest_dir.join(&name);
    let mut n = 2;
    while dest.exists() {
        let stem = dest.file_stem()?.to_string_lossy().into_owned();
        let suffix = dest
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        dest = dest_dir.join(format!("{stem}-{n}{suffix}"));
        n += 1;
    }
    gate(root, &file).ok()?;
    std::fs::rename(&file, &dest).ok()?;
    drop_index_line(root, dir, &name);
    Some(dest)
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

        assert_eq!(resolve(dir.path(), &store, "leak"), None);
        assert_eq!(resolve(dir.path(), &store, "leak.md"), None);
        assert_eq!(
            resolve(dir.path(), &store, "real"),
            Some(store.join("real.md"))
        );
    }
}
