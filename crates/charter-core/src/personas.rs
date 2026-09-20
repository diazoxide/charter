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
    plane_root: PathBuf,
}

impl Persona {
    pub(crate) fn at(dir: PathBuf, name: String, plane_root: PathBuf) -> Self {
        Self {
            dir,
            name,
            plane_root,
        }
    }

    /// Refuse this persona if its directory resolves out of the plane — a committed symlink
    /// at `personas/<legal-name>` redirects every write to it.
    /// Refuse a path that resolves out of the plane, asked of the path being TOUCHED.
    fn writable(&self, path: &Path) -> io::Result<()> {
        crate::contain::writable(&self.plane_root, path).map_err(refusal)
    }

    fn readable(&self, path: &Path) -> io::Result<()> {
        crate::contain::readable(&self.plane_root, path).map_err(refusal)
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
        self.writable(&self.dir.join("memory"))?;
        self.writable(&self.dir.join("refs"))?;
        let who = self.who();
        create_absent(
            &self.plane_root,
            &self.dir.join("memory"),
            memstore::INDEX,
            &index_header(&who),
        )?;
        create_absent(
            &self.plane_root,
            &self.dir.join("refs"),
            "README.md",
            &refs_readme(&who),
        )
    }

    /// This persona's memories, by filename.
    pub fn memories(&self) -> io::Result<Vec<crate::workspaces::Entry>> {
        let dir = self.dir.join("memory");
        self.readable(&dir)?;
        crate::workspaces::read_store(&self.plane_root, &dir)
    }

    /// Record one durable fact. Slug-only filename, `persistent`, indexed.
    ///
    /// **No index header is scaffolded here**, because charter's `persona.remember` writes
    /// straight through `memstore.write`: a persona whose `memory/` has no `MEMORY.md` yet
    /// (`charter init` leaves a `.gitkeep`) gets the store's generic `# Memory Index`
    /// header, not the persona's. [`Persona::scaffold_memory`] is the header's writer.
    pub fn remember(&self, text: &str, stamp: chrono::NaiveDateTime) -> io::Result<PathBuf> {
        self.writable(&self.dir.join("memory"))?;
        let dir = self.dir.join("memory");
        // `timestamped: false` — a persona memory is addressed by its slug, so the name
        // carries no `YYYYMMDD-HHMMSS-` prefix.
        memstore::write(
            &self.plane_root,
            &dir,
            text,
            None,
            false,
            "persistent",
            true,
            stamp,
        )
    }

    /// The `role:` line of `persona.md`'s frontmatter, if it has one.
    ///
    /// Not YAML: charter's frontmatter is line-based with no parser, no quote stripping and
    /// no nesting, so this reads it the same way.
    pub fn role(&self) -> Option<String> {
        let path = self.dir.join("persona.md");
        self.readable(&path).ok()?;
        let text = std::fs::read_to_string(&path).ok()?;
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
fn create_absent(root: &Path, dir: &Path, name: &str, body: &str) -> io::Result<()> {
    // Both, and the FILE is the one that matters. Gating only the directory left this safe
    // by accident — `create_new` refuses to follow a link at the name — and "safe because
    // of the flag two lines down" is the shape that has produced five rounds of findings.
    crate::contain::writable(root, dir).map_err(refusal)?;
    crate::contain::writable(root, &dir.join(name)).map_err(refusal)?;
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

// ---------------------------------------------------------------------------------------
// Which names a command may act on: `persona.name_refusal`, and the definition loader it
// asks. One answer for every command that takes a persona name, so no two of them can
// come to refuse the same name in different words (#1057, #1059).

/// `^[a-z0-9][a-z0-9._-]*$` — a name charter could have minted. `_shared` is not one:
/// charter reaches that store through a flag, never by name.
pub fn valid_name(name: &str) -> bool {
    name != SHARED && crate::contain::persona_name_ok(name)
}

/// The definition file: `personas/<name>/persona.md`, else the legacy flat
/// `personas/<name>.md`, else the first — where a writer would create one.
pub fn def_path(root: &Path, name: &str) -> PathBuf {
    let dir_layout = root.join("personas").join(name).join("persona.md");
    if dir_layout.exists() {
        return dir_layout;
    }
    let flat = root.join("personas").join(format!("{name}.md"));
    if flat.exists() {
        return flat;
    }
    dir_layout
}

/// The frontmatter's `key: value` pairs, in file order — `persona._frontmatter`.
///
/// Line-based, no quote stripping and no nesting: the text between the first two `---`,
/// stripped, each line holding a `:` split at the first one, both halves stripped, and a
/// pair kept only when its key is not empty.
pub fn frontmatter(text: &str) -> Vec<(String, String)> {
    let Some(rest) = text.strip_prefix("---") else {
        return Vec::new();
    };
    let Some(end) = rest.find("---") else {
        return Vec::new();
    };
    let block = memstore::py_strip(&rest[..end]);
    crate::mdsection::split_lines(block)
        .into_iter()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            let key = memstore::py_strip(key);
            (!key.is_empty()).then(|| (key.to_string(), memstore::py_strip(value).to_string()))
        })
        .collect()
}

/// A persona's own definition, loaded — its frontmatter pairs — or `None` when it does
/// not load: not a name, no file, a file charter will not read (out of the plane, not a
/// regular file, past the bound), or one that is not UTF-8 text.
pub fn load(root: &Path, name: &str) -> Option<Vec<(String, String)>> {
    if !valid_name(name) {
        return None;
    }
    let path = def_path(root, name);
    if !path.exists() {
        return None;
    }
    let parent = path.parent()?;
    if crate::contain::readable(root, parent).is_err() || !memstore::readable_file(root, &path) {
        return None;
    }
    memstore::read_text(&path).map(|text| frontmatter(&text))
}

/// The last value a definition gives `key` — `dict(pairs)`, where a later line wins.
fn meta<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .rev()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// The inheritance chain, child first, stopping at the first persona that does not load
/// or at a cycle — `persona.lineage`.
pub fn lineage(root: &Path, name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = Some(name.to_string());
    while let Some(cur) = current.take() {
        if cur.is_empty() || out.contains(&cur) {
            break;
        }
        let Some(pairs) = load(root, &cur) else {
            break;
        };
        out.push(cur);
        current = meta(&pairs, "extends")
            .map(|v| memstore::py_strip(v).to_string())
            .filter(|v| !v.is_empty());
    }
    out
}

/// The persona up `name`'s `extends:` chain whose definition is there and does not load
/// — `persona.ancestor_that_does_not_load`. An absent parent, a non-name and a cycle are
/// not this: each has its own sentence in `lint`.
pub fn ancestor_that_does_not_load(root: &Path, name: &str) -> Option<String> {
    let chain = lineage(root, name);
    let last = chain.last()?;
    let pairs = load(root, last)?;
    let parent = meta(&pairs, "extends").unwrap_or_default().to_string();
    if chain.contains(&parent) || !valid_name(&parent) {
        return None;
    }
    def_path(root, &parent).exists().then_some(parent)
}

/// A value as one line of a report — `contain.one_line`, at the ordinary budget.
///
/// **One implementation, in [`crate::shown`], rather than a copy here.** This module, `doctor`
/// and `news` all need the same answer, and three copies of "which characters have no glyph"
/// is three places for the Cf table to go stale separately — with the failure showing up as
/// one charter escaping a character another prints, on a report line, which is exactly what
/// the function is for.
pub fn one_line(value: &str) -> String {
    crate::shown::line(value)
}

/// Why a command must not act on the persona `name`, or `None` when this plane defines it
/// — `persona.name_refusal`. Four sentences for four fixes: blank, outside the alphabet,
/// defined nowhere (with the create hint), and a definition that is there and does not
/// load — itself, or the parent it inherits from.
pub fn name_refusal(root: &Path, name: &str) -> Option<String> {
    if !name.is_empty() && memstore::py_strip(name).is_empty() {
        return Some(format!(
            "no persona '{}' (a persona name is never only whitespace)",
            one_line(name)
        ));
    }
    if !valid_name(name) {
        return Some(format!(
            "invalid persona name '{}' (lowercase letters, digits, '.', '_', '-')",
            one_line(name)
        ));
    }
    let relative = |path: PathBuf| {
        path.strip_prefix(root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned())
    };
    if load(root, name).is_none() {
        let file = def_path(root, name);
        if file.exists() {
            return Some(format!(
                "persona '{name}' does not load from {} (see why: charter persona lint {name})",
                relative(file)
            ));
        }
        return Some(format!(
            "no persona '{name}' (create it: charter persona create {name})"
        ));
    }
    let parent = ancestor_that_does_not_load(root, name)?;
    Some(format!(
        "persona '{name}' inherits from '{parent}', which does not load from {} (see why: charter persona lint {parent})",
        relative(def_path(root, &parent))
    ))
}

/// The title `persona remember` records — `(title or text.splitlines()[0]).strip()[:72]`.
///
/// A title given as `""` is no title; one given as spaces is a title of nothing, which the
/// store then derives from the text. The trace records THIS string, so it is its own
/// function rather than whatever the store happened to write.
pub fn memory_title(text: &str, title: Option<&str>) -> String {
    let chosen = match title.filter(|t| !t.is_empty()) {
        Some(t) => t,
        None => crate::mdsection::split_lines(text)
            .into_iter()
            .next()
            .unwrap_or_default(),
    };
    memstore::py_strip(chosen)
        .chars()
        .take(memstore::TITLE_MAX)
        .collect()
}

/// A containment refusal as an IO error.
fn refusal(refused: crate::contain::Refused) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, refused.to_string())
}

#[cfg(test)]
mod name_tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        dir
    }

    fn persona(root: &Path, name: &str, text: &str) {
        std::fs::create_dir_all(root.join("personas").join(name)).unwrap();
        std::fs::write(root.join("personas").join(name).join("persona.md"), text).unwrap();
    }

    #[test]
    fn a_persona_with_a_definition_is_one_a_command_may_act_on() {
        let dir = plane();
        persona(dir.path(), "devops", "---\nrole: ops\n---\nbody\n");

        assert_eq!(name_refusal(dir.path(), "devops"), None);
    }

    #[test]
    fn each_refusal_is_the_sentence_charter_gives() {
        let dir = plane();
        persona(dir.path(), "child", "---\nextends: broken\n---\n");
        std::fs::create_dir_all(dir.path().join("personas/broken")).unwrap();
        std::fs::write(dir.path().join("personas/broken/persona.md"), b"\xff\xfe").unwrap();

        assert_eq!(
            name_refusal(dir.path(), " ").unwrap(),
            "no persona ' ' (a persona name is never only whitespace)"
        );
        assert_eq!(
            name_refusal(dir.path(), "Bad").unwrap(),
            "invalid persona name 'Bad' (lowercase letters, digits, '.', '_', '-')"
        );
        assert_eq!(
            name_refusal(dir.path(), "_shared").unwrap(),
            "invalid persona name '_shared' (lowercase letters, digits, '.', '_', '-')"
        );
        assert_eq!(
            name_refusal(dir.path(), "a\nb").unwrap(),
            "invalid persona name 'a\\x0ab' (lowercase letters, digits, '.', '_', '-')"
        );
        assert_eq!(
            name_refusal(dir.path(), "nope").unwrap(),
            "no persona 'nope' (create it: charter persona create nope)"
        );
        assert_eq!(
            name_refusal(dir.path(), "broken").unwrap(),
            "persona 'broken' does not load from personas/broken/persona.md (see why: charter persona lint broken)"
        );
        assert_eq!(
            name_refusal(dir.path(), "child").unwrap(),
            "persona 'child' inherits from 'broken', which does not load from personas/broken/persona.md (see why: charter persona lint broken)"
        );
    }

    #[test]
    fn a_definition_linked_out_of_the_plane_does_not_load() {
        let dir = plane();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("persona.md"), "---\nrole: x\n---\n").unwrap();
        std::fs::create_dir_all(dir.path().join("personas/evil")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("persona.md"),
            dir.path().join("personas/evil/persona.md"),
        )
        .unwrap();

        assert!(load(dir.path(), "evil").is_none());
    }

    #[test]
    fn a_cycle_ends_the_chain_rather_than_looping() {
        let dir = plane();
        persona(dir.path(), "a", "---\nextends: b\n---\n");
        persona(dir.path(), "b", "---\nextends: a\n---\n");

        assert_eq!(lineage(dir.path(), "a"), vec!["a", "b"]);
        assert_eq!(name_refusal(dir.path(), "a"), None);
    }

    #[test]
    fn the_recorded_title_is_the_first_line_unless_one_was_given() {
        assert_eq!(memory_title("first\nsecond", None), "first");
        assert_eq!(
            memory_title("first", Some("")),
            "first",
            "empty is no title"
        );
        assert_eq!(memory_title("first", Some("  T  ")), "T");
        assert_eq!(
            memory_title("first", Some("   ")),
            "",
            "spaces are a title of nothing"
        );
    }
}
