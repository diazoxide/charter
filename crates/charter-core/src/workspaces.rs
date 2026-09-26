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

/// A name that cannot name a workspace or a persona this plane contains.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NameError {
    #[error("no workspace '{0}'")]
    Workspace(String),
    #[error("no persona '{0}'")]
    Persona(String),
}

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
    ///
    /// The listing a caller wants when it is asking which workspaces it can OPEN — the tab
    /// strip, a name lookup. A caller that REPORTS on the plane's workspaces asks
    /// [`Plane::read_workspaces`] instead, because an entry the filesystem will not describe
    /// is left out of this one and is not nothing.
    pub fn workspaces(&self) -> io::Result<Vec<String>> {
        self.read_workspaces().map(|(names, _)| names)
    }

    /// The workspace names AND every entry under `workspaces/` whose kind the filesystem
    /// will not tell, with the errno — `charter/workspace.py:read_workspaces` (charter #1043).
    ///
    /// **One listing, two answers, because a command that counts what it read and names what
    /// it did not must not take them from two reads that disagree.** `Path.is_dir` has no
    /// third answer: for an entry charter may not `stat` it raised on Python 3.11–3.13 and
    /// answered False on 3.14, so a workspace under a `workspaces/` at mode 666 cost a
    /// `doctor` row on one interpreter and vanished from it on the other. Here the same
    /// entry would simply not be a directory, which is the 3.14 half of that bug.
    ///
    /// A `workspaces/` that is not there holds nothing. One that is there and cannot be
    /// LISTED raises, as `Path.iterdir` does: nothing under it can be counted, so the caller
    /// names the directory itself rather than any entry.
    pub fn read_workspaces(&self) -> io::Result<(Vec<String>, crate::memstore::Unread)> {
        let mut names: Vec<String> = Vec::new();
        let mut unread: crate::memstore::Unread = Vec::new();
        for path in read_dir_sorted(&self.root.join("workspaces"))? {
            // charter's own (`.worktrees/`), never a workspace — and asked before the
            // filesystem is, exactly as Python asks it.
            if file_name(&path).starts_with('.') {
                continue;
            }
            match std::fs::metadata(&path) {
                Ok(found) if !found.is_dir() => {}
                // A real clone, not a worktree. Git draws the line: a clone's `.git` is a
                // DIRECTORY, a linked worktree's `.git` is a FILE holding `gitdir:` — so a
                // worktree under `workspaces/` is still a workspace.
                Ok(_) if path.join(".git").is_dir() => {}
                Ok(_) => names.push(file_name(&path).to_string()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => unread.push((path, e.raw_os_error())),
            }
        }
        names.sort();
        Ok((names, unread))
    }

    /// One persona of this plane, by name. `_shared` names the store every persona reads.
    ///
    /// Checked before it is joined onto a path, for the same reason a workspace name is.
    pub fn persona(&self, name: &str) -> Result<crate::personas::Persona, NameError> {
        if !crate::contain::persona_name_ok(name) {
            return Err(NameError::Persona(name.to_string()));
        }
        Ok(crate::personas::Persona::at(
            self.root.join("personas").join(name),
            name.to_string(),
            self.root.clone(),
        ))
    }

    /// The workspace a path sits in, or `None` for a path outside `workspaces/`.
    ///
    /// This is how the app files one of its own chats under a workspace: a chat is app state,
    /// not a plane file, so what relates the two is where the chat is working.
    pub fn workspace_of(&self, path: &Path) -> Option<String> {
        let workspaces = self.root.join("workspaces");
        // Compared after resolving both sides where the filesystem will: a plane reached
        // through a symlink (`/tmp` is one on macOS) would otherwise never match.
        let base = workspaces.canonicalize().unwrap_or(workspaces);
        let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let name = target
            .strip_prefix(&base)
            .ok()?
            .components()
            .next()?
            .as_os_str()
            .to_string_lossy()
            .to_string();
        // A name that is not a workspace this plane has is not one: a chat's cwd is not a
        // reason to invent one.
        self.workspaces()
            .ok()?
            .into_iter()
            .find(|known| *known == name)
    }

    /// The plane's default persona — `[persona] default` in `charter.toml` — or `None`.
    pub fn default_persona(&self) -> Option<String> {
        // A hand-edited `charter.toml` that does not parse is not an error here: the sidebar
        // still draws, and `charter doctor` is what reports the file.
        let text = std::fs::read_to_string(self.root.join(crate::plane::MANIFEST)).ok()?;
        let doc: toml::Table = text.parse().ok()?;
        doc.get("persona")?
            .as_table()?
            .get("default")?
            .as_str()
            .map(str::to_string)
    }

    /// Whether `name` is LIVE — un-ignored in the plane's `.gitignore` managed block, so its
    /// memory is committed and shared (`workspace.live_workspaces`).
    pub fn is_live(&self, name: &str) -> bool {
        const BEGIN: &str =
            "# >>> charter live workspaces (managed by `charter workspace live`) >>>";
        const END: &str = "# <<< charter live workspaces <<<";
        let Ok(text) = std::fs::read_to_string(self.root.join(".gitignore")) else {
            return false;
        };
        let mut inside = false;
        for line in crate::mdsection::split_lines(&text) {
            let line = memstore::py_strip(line);
            if line == BEGIN {
                inside = true;
            } else if line == END {
                inside = false;
            } else if inside
                && let Some(rest) = line.strip_prefix("!/workspaces/")
                && let Some((live, tail)) = rest.split_once('/')
                && !live.is_empty()
                && tail.starts_with("workspace.json")
                && live == name
            {
                return true;
            }
        }
        false
    }

    /// One workspace of this plane, by name.
    ///
    /// The name is checked BEFORE it is joined onto a path. `Path::join` throws the prefix
    /// away when handed an absolute path and `..` walks out of the plane, so an unchecked
    /// name here is a write anywhere on the filesystem.
    pub fn workspace(&self, name: &str) -> Result<Workspace, NameError> {
        if !crate::contain::workspace_name_ok(name) {
            return Err(NameError::Workspace(name.to_string()));
        }
        Ok(Workspace {
            dir: self.root.join("workspaces").join(name),
            name: name.to_string(),
            plane_root: self.root.clone(),
        })
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
    plane_root: PathBuf,
}

impl Workspace {
    /// Refuse this workspace if its directory resolves out of the plane.
    ///
    /// Asked before every write rather than once at construction: a link can be created,
    /// or repointed, between one command and the next.
    /// Refuse a path that resolves out of the plane's data directories.
    ///
    /// Asked of the PATH BEING TOUCHED, not of the workspace directory above it: a symlink
    /// at `workspace.md`, or at `memory/`, redirects the write just as one at the workspace
    /// does, and checking only the directory left both open.
    fn writable(&self, path: &Path) -> io::Result<()> {
        crate::contain::writable(&self.plane_root, path).map_err(refusal)
    }

    /// The same, before a read: a committed link with a legal name otherwise PRINTS a file
    /// from outside the plane.
    fn readable(&self, path: &Path) -> io::Result<()> {
        crate::contain::readable(&self.plane_root, path).map_err(refusal)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The `## Vision` body, or `""` when it is unset, still the placeholder, or unreadable.
    pub fn vision(&self) -> String {
        let path = self.dir.join("workspace.md");
        if self.readable(&path).is_err() {
            // charter answers "" for a charter it refuses, rather than printing it.
            return String::new();
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
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
        let path = self.dir.join("workspace.json");
        if self.readable(&path).is_err() {
            return (None, manifest::Ownership::Absent);
        }
        let text = std::fs::read_to_string(&path).ok();
        let ownership = manifest::ownership(text.as_deref());
        let doc = text.as_deref().and_then(|t| serde_json::from_str(t).ok());
        (doc, ownership)
    }

    /// `workspace.json`'s text, `None` when it is not there — or why it could not be read,
    /// a path that resolves out of the plane included.
    pub fn manifest_text(&self) -> io::Result<Option<String>> {
        let path = self.dir.join("workspace.json");
        self.readable(&path)?;
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Create `workspace.md` from the template when it is absent. An existing file is never
    /// overwritten — only its `## Vision` body is ever replaced.
    pub fn scaffold_charter(&self) -> io::Result<()> {
        let path = self.dir.join("workspace.md");
        self.writable(&path)?;
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
        self.writable(&self.dir.join("workspace.md"))?;
        self.scaffold_charter()?;
        let path = self.dir.join("workspace.md");
        let current = std::fs::read_to_string(&path)?;
        std::fs::write(&path, mdsection::replace(&current, "Vision", text))
    }

    /// Record one todo as its own timestamp-prefixed file, and index it.
    pub fn add_todo(&self, text: &str, stamp: chrono::NaiveDateTime) -> io::Result<PathBuf> {
        self.writable(&self.dir.join("todos"))?;
        let dir = self.dir.join("todos");
        memstore::ensure_index(
            &self.plane_root,
            &dir,
            &TODOS_HEADER.replace("{name}", &self.name),
        )?;
        memstore::write(
            &self.plane_root,
            &dir,
            text,
            None,
            true,
            "persistent",
            true,
            stamp,
        )
    }

    /// Record a todo, unless it has no words or an open todo is already about the same work.
    ///
    /// **The one rule a terminal and the window both record by** (SI-3): `charter ws todo
    /// "<text>"` and the Todos panel's box call this and nothing lower. Duplicate intent is
    /// worse than duplicate memory: closing one of a near-identical pair leaves its twin
    /// looking outstanding, so the list starts lying about what is left. It is refused rather
    /// than merged, naming the todo it repeats ([`memstore::duplicate_of`]).
    pub fn record_todo(
        &self,
        text: &str,
        stamp: chrono::NaiveDateTime,
    ) -> Result<PathBuf, RecordRefused> {
        if text.trim().is_empty() {
            return Err(RecordRefused::Empty);
        }
        let dir = self.dir.join("todos");
        if let Some(dup) = memstore::duplicate_of(&self.plane_root, &dir, text) {
            return Err(RecordRefused::AlreadyListed(dup));
        }
        self.add_todo(text, stamp).map_err(RecordRefused::Io)
    }

    /// Drop a todo without closing it: the file and its index line go, and **nothing is
    /// journalled** — it was abandoned, not done. A slug that is not one path segment is
    /// refused before anything is looked up ([`memstore::forget`]).
    pub fn forget_todo(&self, slug: &str) -> io::Result<()> {
        let dir = self.dir.join("todos");
        self.writable(&dir)?;
        memstore::forget(&self.plane_root, &dir, slug)
    }

    /// The title of an open todo about the same work as `text`, or `None` —
    /// `todos.duplicate_of(…, by_title=True)`, the rule `charter handoff` records by.
    ///
    /// First lines against titles, never the whole text: every handoff todo ends in the same
    /// provenance sentence (`handoff::todo_text`), which would otherwise read as agreement.
    /// Careful rather than catching, because a false match drops a real handoff's todo. The
    /// rule is [`memstore::same_work`].
    ///
    /// A store that cannot be read has no duplicate, so the todo is written — and the
    /// write, which asks the same questions, is where a real failure is said.
    pub fn todo_for_the_same_work(&self, text: &str) -> Option<String> {
        let open = self.todos().ok()?;
        memstore::same_work(
            &memstore::title_of(text),
            open.iter().map(|todo| todo.title.as_str()),
        )
        .map(str::to_owned)
    }

    /// Close a todo: write its closing memory into the journal, then delete the todo file
    /// and its index line. There is no state field — a closed todo is a deleted file.
    pub fn close_todo(&self, slug: &str, stamp: chrono::NaiveDateTime) -> io::Result<()> {
        self.writable(&self.dir.join("todos"))?;
        let dir = self.dir.join("todos");
        let path = memstore::resolve(&self.plane_root, &dir, slug).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, format!("no such todo: {slug}"))
        })?;
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let title = read_store(&self.plane_root, &dir)?
            .into_iter()
            .find(|e| e.slug == stem)
            .map(|e| e.title)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("no such todo: {slug}"))
            })?;
        // The journal entry goes in FIRST: charter writes the trace, then deletes the todo,
        // so a failure leaves the todo open rather than closed with nothing recorded.
        self.remember(&format!("Closed todo: {title}"), stamp)?;
        memstore::forget(&self.plane_root, &dir, slug)
    }

    /// Record one durable fact in the workspace's journal.
    pub fn remember(&self, text: &str, stamp: chrono::NaiveDateTime) -> io::Result<PathBuf> {
        self.remember_titled(text, None, stamp)
    }

    /// Record one durable fact under a title of the caller's, or the text's first line.
    ///
    /// A title of `""` is no title, as charter reads `if title`; one of only spaces is a
    /// title, and strips to nothing — charter's own reading of `--title "  "`.
    pub fn remember_titled(
        &self,
        text: &str,
        title: Option<&str>,
        stamp: chrono::NaiveDateTime,
    ) -> io::Result<PathBuf> {
        self.writable(&self.dir.join("memory"))?;
        let dir = self.dir.join("memory");
        let index = memstore::ensure_index(
            &self.plane_root,
            &dir,
            &WS_MEMORY_HEADER.replace("{name}", &self.name),
        )?;
        // A legacy `notes.md` is grandfathered into the index, so a pre-v2 workspace's memo
        // stays discoverable (`workspace.scaffold_memory`).
        if dir.join("notes.md").exists()
            && memstore::readable_file(&self.plane_root, &index)
            && !memstore::read_text(&index).is_some_and(|t| t.contains("(notes.md)"))
        {
            memstore::index_append(&self.plane_root, &index, "notes.md", "Task memo (legacy)")?;
        }
        memstore::write(
            &self.plane_root,
            &dir,
            text,
            title.filter(|t| !t.is_empty()),
            true,
            "persistent",
            true,
            stamp,
        )
    }

    /// Write `workspace.json`, stamping the digest last and replacing the file atomically.
    ///
    /// `charter_generated` is inserted rather than appended when it is already there, so a
    /// document charter wrote keeps its key order and one a hand wrote keeps the position it
    /// chose — which is what Python's `dict` assignment does.
    pub fn write_manifest(&self, doc: &serde_json::Value) -> io::Result<()> {
        self.write_manifest_as(doc, true)
    }

    /// [`Self::write_manifest`], stamping the digest only when `stamped`: a writer that must not
    /// make a hand's manifest charter's — the Workspace settings tab's save — writes one it found
    /// unstamped, unstamped (charter-app#280).
    pub fn write_manifest_as(&self, doc: &serde_json::Value, stamped: bool) -> io::Result<()> {
        self.writable(&self.dir.join("workspace.json"))?;
        let mut doc = doc.clone();
        let digest = manifest::digest(&doc);
        if let Some(map) = doc.as_object_mut()
            && stamped
        {
            map.insert(manifest::KEY.to_string(), serde_json::Value::String(digest));
        }
        std::fs::create_dir_all(&self.dir)?;
        // Whole or not at all: one of this file's readers is `git add`, so half a manifest is
        // not a glitch somebody re-runs past — it is half a manifest a teammate pulls. Gated
        // from the workspace's own directory, which `writable` has just answered for: a link
        // to the manifest itself is refused rather than replaced.
        crate::rewrite::replace(
            &self.dir,
            &self.dir.join("workspace.json"),
            crate::pyjson::dumps_indent2(&doc).as_bytes(),
            crate::rewrite::Mode::Kept,
        )
    }

    /// Create the workspace's memory index when it has none — the per-file DB's `MEMORY.md`.
    ///
    /// A legacy `notes.md` is grandfathered into the index, so a pre-v2 workspace's memo
    /// stays discoverable. `workspace.scaffold_memory`.
    pub fn scaffold_memory(&self) -> io::Result<PathBuf> {
        let dir = self.dir.join("memory");
        self.writable(&dir)?;
        let index = memstore::ensure_index(
            &self.plane_root,
            &dir,
            &WS_MEMORY_HEADER.replace("{name}", &self.name),
        )?;
        if dir.join("notes.md").exists()
            && memstore::readable_file(&self.plane_root, &index)
            && !memstore::read_text(&index).is_some_and(|t| t.contains("(notes.md)"))
        {
            memstore::index_append(&self.plane_root, &index, "notes.md", "Task memo (legacy)")?;
        }
        Ok(index)
    }

    /// Create the workspace's manifest if it has none. Never touches one that is there.
    ///
    /// The file is committed *precisely so a teammate can restore someone else's workspace*,
    /// so a workspace has one from birth rather than wherever somebody happened to run
    /// `snapshot`.
    ///
    /// A new workspace's manifest says `repos: []`, which is a true and useful statement. A
    /// workspace that already has clones records them as MEMBERSHIP — a name each, and
    /// deliberately no branch: recording a branch here would record whatever happens to be
    /// checked out at that instant, and a manifest branch carries `snapshot`'s enforce-push
    /// promise, which a writer that runs without being asked cannot make.
    ///
    /// **Swallows its own failure**, like Python's: this runs from a launch path where
    /// raising would cost the operator their tab, and a manifest that could not be written
    /// leaves the workspace exactly as it was for `structure_status` to go on reporting.
    pub fn scaffold_manifest(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        author: &str,
    ) -> io::Result<()> {
        if self.manifest().1 != manifest::Ownership::Absent {
            return Ok(());
        }
        self.write_manifest(&self.birth_manifest(now, author))
    }

    /// The document [`Self::scaffold_manifest`] writes, unwritten: the clones already here,
    /// and when and by whom.
    pub fn birth_manifest(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        author: &str,
    ) -> serde_json::Value {
        let members: Vec<serde_json::Value> = crate::repos::clones(&self.plane_root, &self.name)
            .map(|found| {
                found
                    .repos
                    .iter()
                    .map(|repo| serde_json::json!({"name": repo.name}))
                    .collect()
            })
            .unwrap_or_default();
        serde_json::json!({
            "name": self.name,
            "description": "",
            "repos": members,
            "updated_at": now.format("%Y-%m-%dT%H:%M:%S+00:00").to_string(),
            "updated_by": author,
        })
    }

    /// The open todos. There is no state field: a closed todo is a deleted file.
    pub fn todos(&self) -> io::Result<Vec<Entry>> {
        let dir = self.dir.join("todos");
        self.readable(&dir)?;
        read_store(&self.plane_root, &dir)
    }

    /// The workspace's memories, oldest first — the filename stamp orders them.
    pub fn memories(&self) -> io::Result<Vec<Entry>> {
        let dir = self.dir.join("memory");
        self.readable(&dir)?;
        read_store(&self.plane_root, &dir)
    }
}

/// Every `*.md` directly in a memory store, `MEMORY.md` excepted, sorted by filename.
pub(crate) fn read_store(plane_root: &Path, dir: &Path) -> io::Result<Vec<Entry>> {
    crate::contain::readable(plane_root, dir).map_err(refusal)?;
    let mut entries: Vec<Entry> = Vec::new();
    for path in read_dir_sorted(dir)? {
        if !path.is_file() || path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        if file_name(&path) == "MEMORY.md" {
            continue;
        }
        // Each ENTRY too: a single file in the store can be a link out of the plane.
        if crate::contain::readable(plane_root, &path).is_err() {
            continue;
        }
        // An entry charter cannot read is SKIPPED, not fatal. Python's `_entries_of` does
        // the same, and the reason is the sidebar: propagating here turned one chmod-000
        // file in one workspace into a window with no workspaces in it at all.
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
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
///
/// **The stamp is taken from where the store writes it and nowhere else**: the first line that
/// is not blank after the title (after the start of the file when there is no title), and only
/// when that line has a stamp's shape — `memstore.body`'s `^_.*·.*_$`. It used to be the first
/// line anywhere that began with `_`, so in a memory written by hand, with no stamp line, the
/// first `_emphasis_` or `__init__.py` of the body was read as its stamp — and drawn as the
/// memory's date, and handed to an extension as one (charter-app#212's review).
///
/// Python has no reader to agree with here: its store parses a memory's DATE
/// (`memstore.memory_date`, [`crate::memstore::memory_date`]), which wants a date after the
/// underscore and never takes a body line's words, and no command prints a stamp string.
fn parse_entry(slug: &str, text: &str) -> Entry {
    let lines = crate::mdsection::split_lines(text);
    // The first `# ` line anywhere, and `[2..]` ONCE — a stored title of `# Hello` reads as
    // `# Hello`, not `Hello`, because charter takes `ln[2:].strip()`.
    let heading = lines
        .iter()
        .enumerate()
        .find_map(|(i, line)| line.strip_prefix("# ").map(|rest| (i, rest)));
    let title = heading
        .map(|(_, rest)| crate::memstore::py_strip(rest).to_string())
        .unwrap_or_default();
    let after_heading = heading.map_or(0, |(i, _)| i + 1);
    let stamped = lines
        .iter()
        .enumerate()
        .skip(after_heading)
        .find(|(_, line)| !crate::memstore::py_strip(line).is_empty())
        .map(|(i, line)| (i, crate::memstore::py_strip(line)))
        .filter(|(_, line)| crate::memstore::is_stamp_line(line));
    let (stamp, body_from) = match stamped {
        Some((i, line)) => (
            line.trim_matches('_')
                .split(" · ")
                .next()
                .unwrap_or_default()
                .to_string(),
            i + 1,
        ),
        None => (String::new(), 0),
    };
    let body = lines
        .into_iter()
        .skip(body_from)
        .collect::<Vec<_>>()
        .join("\n");
    let body = crate::memstore::py_strip(&body).to_string();
    Entry {
        slug: slug.to_string(),
        // A file with no heading is named by its stem, as charter names one.
        title: if title.is_empty() {
            slug.to_string()
        } else {
            title
        },
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

/// Twelve hex characters no other writer in this process will produce, so the temp name is
/// unique per CALL and not merely per process.
///
/// charter's own temp is `.charter-generated.<target>.<pid>.<12 random hex>.tmp`
/// ([`crate::rewrite::replace`]): the pid separates two
/// processes (#893 — two commands scaffolding one workspace used to share a single
/// `workspace.json.tmp`) and the random half separates two writers inside one, which threads
/// in a single process are.
pub(crate) fn scratch_tag() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let mixed = nanos ^ (NEXT.fetch_add(1, Ordering::Relaxed) << 32);
    format!("{:012x}", mixed & 0xffff_ffff_ffff)
}

/// Why [`Workspace::record_todo`] recorded nothing.
#[derive(Debug)]
pub enum RecordRefused {
    /// The text has no words.
    Empty,
    /// An open todo is already about the same work: its title.
    AlreadyListed(String),
    /// The store could not be written.
    Io(io::Error),
}

impl std::fmt::Display for RecordRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("a todo needs some words to say what is to be done"),
            // CONTAINED and one line: the title is the `# ` heading of a file on disk, and a
            // handed-off todo's title can be a model's prose.
            Self::AlreadyListed(title) => write!(
                f,
                "already on the list: {}",
                crate::personas::one_line(title)
            ),
            Self::Io(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for RecordRefused {}

/// A containment refusal as an IO error, so every caller handles one kind of failure.
fn refusal(refused: crate::contain::Refused) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, refused.to_string())
}

#[cfg(test)]
mod entry_tests {
    use super::*;

    #[test]
    fn a_memory_the_store_wrote_has_its_stamp_and_its_body() {
        let entry = parse_entry(
            "fact",
            "# A fact\n\n_2026-09-22 10:00 · persistent_\n\nThe body.\n",
        );
        assert_eq!(entry.title, "A fact");
        assert_eq!(entry.stamp, "2026-09-22 10:00");
        assert_eq!(entry.body, "The body.");
    }

    #[test]
    fn a_body_line_that_starts_with_an_underscore_is_never_read_as_the_stamp() {
        // No stamp line: the first `_` line is the body's, and it stays the body's.
        let entry = parse_entry(
            "db",
            "---\nname: db\n---\n# prod db\n\nnotes\n\n_the prod password is hunter2_\n",
        );
        assert_eq!(entry.stamp, "");
        assert!(entry.body.contains("hunter2"), "{entry:?}");

        let entry = parse_entry("py", "# layout\n\n__init__.py re-exports the loader\n");
        assert_eq!(entry.stamp, "", "{entry:?}");
    }

    #[test]
    fn a_stamp_shaped_line_further_down_the_body_is_not_the_stamp() {
        let entry = parse_entry(
            "late",
            "# late\n\nfirst words\n\n_2026-09-22 10:00 · persistent_\n",
        );
        assert_eq!(entry.stamp, "");
    }

    #[test]
    fn a_memory_with_no_heading_is_stamped_from_its_first_line() {
        let entry = parse_entry("bare", "_2026-09-22 10:00 · persistent_\n\nbody\n");
        assert_eq!(entry.title, "bare");
        assert_eq!(entry.stamp, "2026-09-22 10:00");
        assert_eq!(entry.body, "body");
    }
}
