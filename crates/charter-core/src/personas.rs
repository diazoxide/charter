//! Personas: their charter file and their memory.
//!
//! A persona's memory is the same per-file store the workspaces use, with two differences
//! that matter to a reader: the filenames carry no timestamp prefix (a persona memory is
//! addressed by its slug alone, which is how `forget` and `recall` name one), and the index
//! header names the persona — or says "shared (all personas)" for the one every persona
//! reads.

use std::io;
use std::path::{Path, PathBuf};

use crate::memstore;

/// The directory holding the memory and refs every persona shares.
pub const SHARED: &str = "_shared";

/// One persona's directory.
#[derive(Debug, Clone)]
pub struct Persona {
    dir: PathBuf,
    name: String,
}

impl Persona {
    pub(crate) fn at(dir: PathBuf, name: String) -> Self {
        Self { dir, name }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// How the index and the refs README name this persona.
    fn who(&self) -> String {
        if self.name == SHARED {
            "shared (all personas)".to_string()
        } else {
            self.name.clone()
        }
    }

    /// Create the committed `memory/` and `refs/` with the files that make them useful: an
    /// index to read, and a README saying what the directory is for.
    pub fn scaffold_memory(&self) -> io::Result<()> {
        let who = self.who();
        create_absent(
            &self.dir.join("memory"),
            memstore::INDEX,
            &index_header(&who),
        )?;
        create_absent(&self.dir.join("refs"), "README.md", &refs_readme(&who))
    }

    /// This persona's memories, by filename.
    pub fn memories(&self) -> io::Result<Vec<crate::workspaces::Entry>> {
        crate::workspaces::read_store(&self.dir.join("memory"))
    }

    /// Record one durable fact. Slug-only filename, `persistent`, indexed.
    pub fn remember(&self, text: &str, stamp: chrono::NaiveDateTime) -> io::Result<PathBuf> {
        let dir = self.dir.join("memory");
        memstore::ensure_index(&dir, &index_header(&self.who()))?;
        // `timestamped: false` — a persona memory is addressed by its slug, so the name
        // carries no `YYYYMMDD-HHMMSS-` prefix.
        memstore::write(&dir, text, None, false, "persistent", true, stamp)
    }

    /// The `role:` line of `persona.md`'s frontmatter, if it has one.
    ///
    /// Not YAML: charter's frontmatter is line-based with no parser, no quote stripping and
    /// no nesting, so this reads it the same way.
    pub fn role(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.dir.join("persona.md")).ok()?;
        frontmatter_value(&text, "role")
    }
}

/// The header a persona's memory index is created with.
pub(crate) fn index_header(who: &str) -> String {
    format!(
        "# Memory Index — {who}\n\nOne line per memory; each links a file holding a single durable fact.\nWritten by the persona as it learns; committed and shared.\n"
    )
}

/// The README a persona's refs directory is created with.
pub(crate) fn refs_readme(who: &str) -> String {
    format!(
        "# References — {who}\n\nCurated docs, links, and snippets this role collects. Committed and shared. Never store secrets here — those live only in the vault.\n"
    )
}

/// Write `name` into `dir` only when nothing is at that name.
///
/// Exclusive, so a dangling symlink at the name is refused by the kernel rather than
/// followed — `exists()` answers false for one, and a plain write would have created
/// whatever it pointed at.
fn create_absent(dir: &Path, name: &str, body: &str) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(name))
    {
        Ok(mut f) => std::io::Write::write_all(&mut f, body.as_bytes()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

/// One `key: value` line from the leading `---` block.
///
/// charter's frontmatter is deliberately not YAML: no parser, no quote stripping, no
/// nesting, no comments. So this is the same reading — split once on the first colon, trim,
/// and stop at the closing fence.
fn frontmatter_value(text: &str, key: &str) -> Option<String> {
    let mut lines = crate::mdsection::split_lines(text).into_iter();
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        if line.trim() == "---" {
            return None;
        }
        let (found, value) = line.split_once(':')?;
        if found.trim() == key {
            return Some(value.trim().to_string());
        }
    }
    None
}
