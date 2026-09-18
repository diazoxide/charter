//! Reading a plane: its workspaces, their visions, todos and memories, and its personas.
//!
//! Every rule here is charter's, including the ones that look arbitrary: an entry under
//! `workspaces/` whose name starts with `.` is charter's own and never a workspace, and one
//! holding a `.git` is a clone somebody dropped in the wrong directory. Listings are sorted,
//! because `workspace.read_directory` sorts and the sidebar's order is the CLI's order.

use std::io;
use std::path::{Path, PathBuf};

use crate::{manifest, mdsection, memstore};

/// `workspace.md`'s template, with `{name}` and `{vision}` to fill. Taken from
/// `charter/workspace.py:_CHARTER_TEMPLATE` verbatim — the file is committed and
/// hand-edited, so a byte that differs is a diff in the operator's repository.
const CHARTER_TEMPLATE: &str = "# {name}\n\n> **Living charter** for this workspace — its north star and shared context.\n> Keep it current as the work evolves (edit this file, or `charter workspace vision \"…\"`).\n> It's committed + shared for LIVE workspaces, and a fork inherits it — so anyone\n> can pick up the task with full context. Never put secrets here (vault only).\n\n## Vision\n\n{vision}\n\n## Context & decisions\n\n<!-- Key facts, constraints, and design/architecture decisions found while working —\n     the durable \"why\", not a chronological log. Grow this as you learn. -->\n\n_Nothing yet._\n\n## Glossary\n\n<!-- Task/domain vocabulary so a teammate or a fork isn't lost: `term` — definition. -->\n\n_Nothing yet._\n\n## Log\n\nChronological \"what was done\" lives in the task memo — `memory/notes.md`\n(append with `charter workspace note \"…\"`).\n";

/// The vision body charter writes when no vision is set, and reads back as "unset".
pub const VISION_PLACEHOLDER: &str = "_Not set yet — describe the goal: what are we building or fixing, and why? Set it with `charter workspace vision \"…\"` (or edit this file)._";

/// The header `memory/MEMORY.md` is created with, `{name}` to fill.
const WS_MEMORY_HEADER: &str = "# {name} — task memory\n\nOne file per memory — a small, programmatically-explorable DB, not a single log to\nmerge-conflict on. Files are timestamp-prefixed, so this index (and the directory) list chronologically. **Committed + shared** for LIVE workspaces. Write with `charter workspace remember \"…\"`, search with `charter workspace recall [--query …]`, drop one with `charter workspace forget <slug>`. Never put secrets here (vault only).\n";

/// The header `todos/MEMORY.md` is created with, `{name}` to fill.
const TODOS_HEADER: &str = "# Todos — workspace `{name}`\n\nOne line per todo; each links a file holding one thing this task still means to do.\nOpen or done — and done removes it, leaving its trace in the journal instead.\n";

/// A control plane, rooted at the directory holding `charter.toml`.
#[derive(Debug, Clone)]
pub struct Plane {
    root: PathBuf,
}

/// One memory or todo file: the store is the same for both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The file stem — what `ws todo done <slug>` closes a todo by.
    pub slug: String,
    /// The `# ` heading.
    pub title: String,
    /// The `_<date> <time> · <kind>_` line's date, as written.
    pub stamp: String,
    /// Everything under the stamp line, trimmed.
    pub body: String,
}

impl Plane {
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The names under `workspaces/`, sorted. A plane with no `workspaces/` has none.
    pub fn workspaces(&self) -> io::Result<Vec<String>> {
        let mut names: Vec<String> = read_dir_sorted(&self.root.join("workspaces"))?
            .into_iter()
            .filter(|p| p.is_dir())
            .filter(|p| !file_name(p).starts_with('.'))
            .filter(|p| !p.join(".git").exists())
            .map(|p| file_name(&p).to_string())
            .collect();
        names.sort();
        Ok(names)
    }

    pub fn workspace(&self, name: &str) -> Workspace {
        Workspace {
            dir: self.root.join("workspaces").join(name),
            name: name.to_string(),
        }
    }

    /// The plane's personas: a directory holding `persona.md` whose name does not start with
    /// `_`, plus the legacy flat `personas/<name>.md` files. Sorted, and deduplicated where a
    /// name has both.
    pub fn personas(&self) -> io::Result<Vec<String>> {
        let dir = self.root.join("personas");
        let mut names: Vec<String> = Vec::new();
        for path in read_dir_sorted(&dir)? {
            let name = file_name(&path).to_string();
            let is_persona = if path.is_dir() {
                !name.starts_with('_') && path.join("persona.md").is_file()
            } else {
                // The legacy layout: one flat file per persona, README excepted.
                path.extension().is_some_and(|e| e == "md")
                    && !name.eq_ignore_ascii_case("readme.md")
            };
            if is_persona {
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or(name);
                if !names.contains(&stem) {
                    names.push(stem);
                }
            }
        }
        names.sort();
        Ok(names)
    }
}

/// One workspace's directory.
#[derive(Debug, Clone)]
pub struct Workspace {
    dir: PathBuf,
    name: String,
}

impl Workspace {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The `## Vision` body, or `""` when it is unset, still the placeholder, or unreadable.
    pub fn vision(&self) -> String {
        let text = std::fs::read_to_string(self.dir.join("workspace.md")).unwrap_or_default();
        let body = mdsection::section_body(&text, "Vision");
        // charter reads its own placeholder as "no vision", by prefix.
        if body.starts_with("_Not set yet") {
            String::new()
        } else {
            body
        }
    }

    /// `workspace.json`, and who owns it.
    pub fn manifest(&self) -> (Option<serde_json::Value>, manifest::Ownership) {
        let text = std::fs::read_to_string(self.dir.join("workspace.json")).ok();
        let ownership = manifest::ownership(text.as_deref());
        let doc = text.as_deref().and_then(|t| serde_json::from_str(t).ok());
        (doc, ownership)
    }

    /// Create `workspace.md` from the template when it is absent. An existing file is never
    /// overwritten — only its `## Vision` body is ever replaced.
    pub fn scaffold_charter(&self) -> io::Result<()> {
        let path = self.dir.join("workspace.md");
        if path.exists() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.dir)?;
        let body = CHARTER_TEMPLATE
            .replace("{name}", &self.name)
            .replace("{vision}", VISION_PLACEHOLDER);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => std::io::Write::write_all(&mut f, body.as_bytes()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Set the `## Vision` body, creating the charter first when it is missing.
    pub fn set_vision(&self, text: &str) -> io::Result<()> {
        self.scaffold_charter()?;
        let path = self.dir.join("workspace.md");
        let current = std::fs::read_to_string(&path)?;
        std::fs::write(&path, mdsection::replace(&current, "Vision", text))
    }

    /// Record one todo as its own timestamp-prefixed file, and index it.
    pub fn add_todo(&self, text: &str, stamp: chrono::NaiveDateTime) -> io::Result<PathBuf> {
        let dir = self.dir.join("todos");
        memstore::ensure_index(&dir, &TODOS_HEADER.replace("{name}", &self.name))?;
        memstore::write(&dir, text, None, true, "persistent", true, stamp)
    }

    /// Close a todo: write its closing memory into the journal, then delete the todo file
    /// and its index line. There is no state field — a closed todo is a deleted file.
    pub fn close_todo(&self, slug: &str, stamp: chrono::NaiveDateTime) -> io::Result<()> {
        let dir = self.dir.join("todos");
        let path = memstore::resolve(&dir, slug).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, format!("no such todo: {slug}"))
        })?;
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let title = read_store(&dir)?
            .into_iter()
            .find(|e| e.slug == stem)
            .map(|e| e.title)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("no such todo: {slug}"))
            })?;
        // The journal entry goes in FIRST: charter writes the trace, then deletes the todo,
        // so a failure leaves the todo open rather than closed with nothing recorded.
        self.remember(&format!("Closed todo: {title}"), stamp)?;
        memstore::forget(&dir, slug)
    }

    /// Record one durable fact in the workspace's journal.
    pub fn remember(&self, text: &str, stamp: chrono::NaiveDateTime) -> io::Result<PathBuf> {
        let dir = self.dir.join("memory");
        memstore::ensure_index(&dir, &WS_MEMORY_HEADER.replace("{name}", &self.name))?;
        memstore::write(&dir, text, None, true, "persistent", true, stamp)
    }

    /// Write `workspace.json`, stamping the digest last and replacing the file atomically.
    ///
    /// `charter_generated` is inserted rather than appended when it is already there, so a
    /// document charter wrote keeps its key order and one a hand wrote keeps the position it
    /// chose — which is what Python's `dict` assignment does.
    pub fn write_manifest(&self, doc: &serde_json::Value) -> io::Result<()> {
        let mut doc = doc.clone();
        let digest = manifest::digest(&doc);
        if let Some(map) = doc.as_object_mut() {
            map.insert(manifest::KEY.to_string(), serde_json::Value::String(digest));
        }
        std::fs::create_dir_all(&self.dir)?;
        replace_atomically(
            &self.dir.join("workspace.json"),
            crate::pyjson::dumps_indent2(&doc).as_bytes(),
        )
    }

    /// The open todos. There is no state field: a closed todo is a deleted file.
    pub fn todos(&self) -> io::Result<Vec<Entry>> {
        read_store(&self.dir.join("todos"))
    }

    /// The workspace's memories, oldest first — the filename stamp orders them.
    pub fn memories(&self) -> io::Result<Vec<Entry>> {
        read_store(&self.dir.join("memory"))
    }
}

/// Every `*.md` directly in a memory store, `MEMORY.md` excepted, sorted by filename.
fn read_store(dir: &Path) -> io::Result<Vec<Entry>> {
    let mut entries: Vec<Entry> = Vec::new();
    for path in read_dir_sorted(dir)? {
        if !path.is_file() || path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        if file_name(&path) == "MEMORY.md" {
            continue;
        }
        let text = std::fs::read_to_string(&path)?;
        entries.push(parse_entry(
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
            &text,
        ));
    }
    Ok(entries)
}

/// A memory file's three parts: `# <title>`, `_<stamp> · <kind>_`, then the body.
fn parse_entry(slug: &str, text: &str) -> Entry {
    let mut title = String::new();
    let mut stamp = String::new();
    let mut body_from = 0usize;
    for (i, line) in text.lines().enumerate() {
        if i == 0 {
            title = line.trim_start_matches("# ").trim().to_string();
        } else if stamp.is_empty() && line.starts_with('_') {
            stamp = line
                .trim_matches('_')
                .split(" · ")
                .next()
                .unwrap_or_default()
                .to_string();
            body_from = i + 1;
        }
    }
    let body = text
        .lines()
        .skip(body_from)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    Entry {
        slug: slug.to_string(),
        title,
        stamp,
        body,
    }
}

/// A directory's entries, sorted by name. A directory that is not there has none — charter
/// creates a store lazily, so "no todos yet" and "no `todos/`" are the same answer.
fn read_dir_sorted(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let reader = match std::fs::read_dir(dir) {
        Ok(reader) => reader,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut paths: Vec<PathBuf> = reader
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .map(|e| e.path())
        .collect();
    paths.sort();
    Ok(paths)
}

fn file_name(path: &Path) -> std::borrow::Cow<'_, str> {
    path.file_name().unwrap_or_default().to_string_lossy()
}

/// Write `bytes` to `path` through a temp file beside it, then rename.
///
/// One of this file's readers is `git add`, so half a manifest is not a glitch somebody
/// re-runs past — it is half a manifest a teammate pulls. The temp name carries the pid
/// because two commands scaffolding one workspace at once used to share one temp file.
fn replace_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = file_name(path);
    let temp = dir.join(format!("{name}.{}.tmp", std::process::id()));
    // The mode the file ends up with is the temp file's: `rename` carries the source's,
    // so it is created here under the umask rather than with a private mode.
    std::fs::write(&temp, bytes)?;
    match std::fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temp);
            Err(e)
        }
    }
}
