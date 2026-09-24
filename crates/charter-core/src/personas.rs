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
    /// **Read by [`frontmatter`], the one reader**, and collapsed by [`meta`] exactly as
    /// `persona.parse`'s `dict(pairs)` collapses it — so a key written twice answers with
    /// the LAST line, which is what every other consumer of a definition already gets.
    ///
    /// This used to be a THIRD reading of the format (`frontmatter_value`), and it lost a
    /// persona's role two ways that charter does not (charter-app #67): it returned at the
    /// first frontmatter line without a colon where charter skips it, so a blank line or a
    /// stray comment above `role:` answered `None`; and it matched the opening fence on a
    /// TRIMMED line where charter tests `text.startswith("---")`, so an indented `  ---`
    /// opened a frontmatter block here and none in charter.
    pub fn role(&self) -> Option<String> {
        let path = self.dir.join("persona.md");
        self.readable(&path).ok()?;
        let text = std::fs::read_to_string(&path).ok()?;
        meta(&frontmatter(&text), "role").map(str::to_string)
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
    load_with_charter(root, name).map(|(pairs, _)| pairs)
}

/// [`load`], and the charter body beside the pairs — `persona.load`'s `meta` and `charter`,
/// from one read of the file so the two cannot describe different versions of it.
pub fn load_with_charter(root: &Path, name: &str) -> Option<(Vec<(String, String)>, String)> {
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
    memstore::read_text(&path).map(|text| (frontmatter(&text), charter_body(&text)))
}

/// The charter below the frontmatter, stripped — `persona._frontmatter`'s second answer.
///
/// Python splits on the first two `---` (`text.split("---", 2)`), wherever the second one
/// falls, and takes everything after it; a text that does not open with `---`, or has no
/// second one, is all body. The same cut as [`frontmatter`], so the pairs and the body are
/// never read off two different boundaries.
pub fn charter_body(text: &str) -> String {
    let after = text
        .strip_prefix("---")
        .and_then(|rest| rest.find("---").map(|end| &rest[end + 3..]))
        .unwrap_or(text);
    memstore::py_strip(after).to_string()
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
/// is three places for the table to go stale separately — with the failure showing up as one
/// charter escaping a character another prints, on a report line, which is exactly what the
/// function is for.
///
/// # This is NOT [`crate::pyrepr::repr_str`], and folding it into one is a bug, not a cleanup
///
/// Every refusal in the module below quotes its value between `'…'` that the SENTENCE
/// supplies — `"no persona '{}' (…)"` — and `one_line` puts nothing round the value. `repr`
/// supplies its own quotes and chooses between `'` and `"` by what the value holds. So the
/// obvious-looking tidy, "these both escape invisible characters, use the one in `pyrepr`",
/// changes `no persona 'x'` into `no persona ''x''` for every ordinary name and into
/// `no persona '"it's"'` for one holding an apostrophe. Those sentences are compared BYTE FOR
/// BYTE against Python's in the differential, and charter's Python spells them with
/// `one_line` here, not with `!r`.
///
/// Two smaller differences that outlive the quoting, for whoever revisits this:
///
/// * `one_line` **clips** at `DISPLAY_LIMIT` with a `…`, because a persona name is
///   attacker-chosen and a refusal is one row of a TUI. `repr` never clips — a refusal that
///   quotes back a truncated value is not Python's refusal.
/// * `one_line` leaves the **backslash** alone and escapes by general category; `repr`
///   doubles the backslash and escapes by `str.isprintable`, which is also false for every
///   unassigned codepoint. They are close and they are not the same set, and the day they
///   are made the same set is the day one of them stops being a port of its Python.
///
/// What the two DO share is the source of "which characters are which": one generated file,
/// `crate::tui::tables`, so the divergence stays the deliberate one and does not quietly
/// acquire an accidental one on top.
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
                "persona '{name}' does not load from {} (its frontmatter does not parse)",
                relative(file)
            ));
        }
        return Some(format!(
            "no persona '{name}' (add it: write personas/{name}/persona.md)"
        ));
    }
    let parent = ancestor_that_does_not_load(root, name)?;
    Some(format!(
        "persona '{name}' inherits from '{parent}', which does not load from {} (its frontmatter does not parse)",
        relative(def_path(root, &parent))
    ))
}

// ---------------------------------------------------------------------------------------
// What a persona SAYS about itself: the keys `charter persona show` puts on screen, read
// through the one frontmatter reader above and merged down the `extends:` chain exactly as
// `persona.resolve` merges them.

/// Which vault a persona uses, as its own definition declares it.
///
/// **Three answers and not two**, because "holds no credentials" and "nobody said" are
/// different facts and rounding the second down to the first is the shape this repo keeps
/// refusing elsewhere. `charter/persona.py` makes the same split — `declares_no_vault` is
/// its own function beside `vault_of`, and its docstring says why: *"Use `declares_no_vault`
/// to tell 'none, deliberately' from 'none, unexamined'."*
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Vault {
    /// `vault: <name>`, inherited-inclusive. **The NAME, and never anything inside it.**
    Named(String),
    /// `vault: none` — the reserved value, declared by a persona that holds no credentials.
    DeclaredNone,
    /// No `vault:` anywhere up the chain.
    ///
    /// **This is not "no vault".** charter's own `vault_of` falls back to the vault registry
    /// — a vault tagged with this persona's name in `vaults.json` — and that registry has no
    /// Rust reader yet. Whoever shows this must say "not declared" and not "none", or the
    /// answer claims a persona holds no credentials on the strength of a file nothing read.
    Undeclared,
}

/// What a persona's definition says about it, with its `extends:` chain applied.
///
/// The keys are the ones `charter persona show` prints, minus the two this cannot answer
/// (the memory counts, and the charter body — neither is a fact about the persona that a
/// panel row is asking for). Everything here comes through [`load`], which is the one
/// frontmatter reader (#67); nothing re-parses the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Details {
    pub name: String,
    /// `role:`, or `None` where the definition declares none.
    pub role: Option<String>,
    /// `delegate-when:` — what makes a persona findable, and what a router reads.
    pub delegate_when: Option<String>,
    /// `tools:`, the union down the chain in the order a parent declared them first.
    pub tools: Vec<String>,
    pub vault: Vault,
    /// The `extends:` chain, child first. One name long for a persona that extends nothing.
    pub lineage: Vec<String>,
    /// The definition file, relative to the plane where it is inside it.
    pub file: String,
}

/// The effective frontmatter of `name` and everything it extends — `persona.resolve`'s
/// `meta` and its `tools`, for the keys [`Details`] carries.
///
/// Root first so a child's line wins, and **a blank value never overwrites**: `persona.resolve`
/// skips a pair whose value is empty (`if k in (…) or not v: continue`), so a child that
/// writes `role:` with nothing after it inherits its parent's role rather than erasing it.
///
/// `tools` is unioned rather than overwritten, parent's first, deduplicated — which is what
/// `tools_of`'s "including inherited ones (union across the `extends` chain)" means.
fn effective(root: &Path, chain: &[String]) -> (Vec<(String, String)>, Vec<String>) {
    // **Appended rather than overwritten, because [`meta`] already reads the LAST pair.** That
    // is `dict(pairs)`'s rule for one file, and walking root first makes it the chain's rule
    // too: a child's line is pushed after its parent's and answers instead of it.
    let mut declared: Vec<(String, String)> = Vec::new();
    let mut tools: Vec<String> = Vec::new();
    for ancestor in chain.iter().rev() {
        let Some(pairs) = load(root, ancestor) else {
            continue;
        };
        for (key, value) in &pairs {
            // `tools` is merged below; `extends` is the chain itself and is not shown; the
            // other two are lists charter merges and nothing here reads. A blank value is
            // skipped, so a child that writes `role:` with nothing after it inherits.
            if matches!(key.as_str(), "tools" | "agent-tools" | "uses" | "extends")
                || value.is_empty()
            {
                continue;
            }
            declared.push((key.clone(), value.clone()));
        }
        for tool in csv(meta(&pairs, "tools").unwrap_or_default()) {
            if !tools.contains(&tool) {
                tools.push(tool);
            }
        }
    }
    (declared, tools)
}

/// `persona._csv_list`: the value's outer brackets dropped, split at commas, each part
/// stripped, and an empty part thrown away.
///
/// The brackets are stripped as CHARACTERS from both ends, which is what Python's
/// `str.strip("[]")` does — so `[a, b]`, `a, b` and `]a, b[` all answer with the same two.
fn csv(value: &str) -> Vec<String> {
    value
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(memstore::py_strip)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// What a persona says about itself, or the sentence saying why charter will not answer.
///
/// The refusal is [`name_refusal`]'s, so a panel, the CLI and a hook all refuse a name in
/// the same words — which is the whole point of that function existing (#1057, #1059).
/// One persona's memory store, which is the same per-file store the workspaces use.
///
/// **A path and not an opened thing**, so the two readers below each apply their own
/// containment check to what they are about to touch — [`memstore::read_files`] refuses a
/// directory that resolves out of the plane and [`crate::workspaces::read_store`] refuses it
/// again per entry. A committed symlink at `personas/<name>/memory` is the shape both are for.
fn memory_dir(root: &Path, name: &str) -> PathBuf {
    root.join("personas").join(name).join("memory")
}

/// How many memories this persona holds, counted **without reading one**.
///
/// It is the count a panel row carries, so it runs once per persona on the path that draws the
/// window, and the whole point of it is that it is a `read_dir` and no file opens. Reading them
/// is [`memories`], which happens when a reader asks to see them and not before.
///
/// A store charter cannot look at counts zero, because the count is decoration on a row and the
/// row is about the persona. A reader who opens it gets the refusal in charter's own words.
pub fn memory_count(root: &Path, name: &str) -> usize {
    memstore::read_files(root, &memory_dir(root, name)).0.len()
}

/// This persona's memories, in the store's own order, or charter's own sentence for a name it
/// will not answer about.
///
/// The same [`crate::workspaces::read_store`] the todo list comes out of — one store, one
/// reader, and a memory that arrives with the title, the stamp and the body a todo does.
pub fn memories(root: &Path, name: &str) -> Result<Vec<crate::workspaces::Entry>, String> {
    if let Some(refused) = name_refusal(root, name) {
        return Err(refused);
    }
    crate::workspaces::read_store(root, &memory_dir(root, name)).map_err(|why| why.to_string())
}

pub fn details(root: &Path, name: &str) -> Result<Details, String> {
    if let Some(refused) = name_refusal(root, name) {
        return Err(refused);
    }
    let chain = lineage(root, name);
    let (declared, tools) = effective(root, &chain);
    let value = |key: &str| meta(&declared, key).map(str::to_string);
    let vault = match meta(&declared, "vault") {
        // `persona.vault_of`: the reserved `none` is a declaration, not a name.
        Some(NO_VAULT) => Vault::DeclaredNone,
        // A blank value never reaches here, so anything else is a name.
        Some(named) => Vault::Named(named.to_string()),
        None => Vault::Undeclared,
    };
    let file = def_path(root, name);
    Ok(Details {
        name: name.to_string(),
        role: value("role"),
        delegate_when: value("delegate-when"),
        tools,
        vault,
        lineage: chain,
        file: file
            .strip_prefix(root)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| file.to_string_lossy().into_owned()),
    })
}

/// A `vault:` value meaning "deliberately nothing" — `persona.NO_VAULT`.
pub const NO_VAULT: &str = "none";

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
            "no persona 'nope' (add it: write personas/nope/persona.md)"
        );
        assert_eq!(
            name_refusal(dir.path(), "broken").unwrap(),
            "persona 'broken' does not load from personas/broken/persona.md (its frontmatter does not parse)"
        );
        assert_eq!(
            name_refusal(dir.path(), "child").unwrap(),
            "persona 'child' inherits from 'broken', which does not load from personas/broken/persona.md (its frontmatter does not parse)"
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

    /// The divergence `one_line`'s docs call deliberate, asserted so a "cleanup" goes red.
    ///
    /// charter's Python spells these refusals with `contain.one_line` inside quotes the
    /// sentence supplies, NOT with an f-string's `!r`. Swapping in `pyrepr::repr_str` — which
    /// escapes almost the same characters, which is what makes the swap look safe — doubles
    /// the quotes on every one of them, and the differential compares these sentences byte
    /// for byte.
    #[test]
    fn a_refusal_quotes_a_name_with_one_line_and_not_with_repr() {
        let dir = plane();

        let refusal = name_refusal(dir.path(), "it's").unwrap();
        assert!(
            refusal.starts_with("invalid persona name 'it's'"),
            "the sentence supplies the quotes: {refusal}"
        );
        // What `repr` would have put there instead, spelled out rather than described.
        assert_eq!(crate::pyrepr::repr_str("it's"), "\"it's\"");

        // And `one_line` still escapes what could forge a second line of the report.
        assert_eq!(one_line("two\nlines"), "two\\x0alines");
        // Where `repr` writes the short escape and adds its own quotes.
        assert_eq!(crate::pyrepr::repr_str("two\nlines"), "'two\\nlines'");
    }

    /// The role charter reads for a persona whose definition is `text`.
    fn role_of(text: &str) -> Option<String> {
        let dir = plane();
        persona(dir.path(), "devops", text);
        Persona::at(
            dir.path().join("personas").join("devops"),
            "devops".to_string(),
            dir.path().to_path_buf(),
        )
        .role()
    }

    /// charter-app #67. `persona._frontmatter` SKIPS a frontmatter line with no colon and
    /// keeps walking; the reader this replaced returned at the first one, so everything
    /// below a blank line or a stray comment was invisible.
    #[test]
    fn a_frontmatter_line_that_is_not_a_pair_is_skipped_and_the_walk_goes_on() {
        // The issue's reproduction, exactly.
        assert_eq!(role_of("---\n\nrole: ops\n---\n").as_deref(), Some("ops"));
        assert_eq!(
            role_of("---\n# not a pair\nrole: ops\n---\n").as_deref(),
            Some("ops"),
            "charter has no comments; the line is simply not a pair, and it is skipped"
        );
        assert_eq!(
            role_of("---\nname: devops\n  wrapped onto a continuation line\nrole: ops\n---\n")
                .as_deref(),
            Some("ops"),
            "the YAML habit a continuation line is: charter walks past it"
        );
        assert_eq!(
            role_of("---\nno-colon-here\n---\n"),
            None,
            "and a block that never declares one still has no role"
        );
    }

    /// `persona._frontmatter` opens on `text.startswith("---")`, which an INDENTED fence
    /// does not satisfy. The reader this replaced compared the line trimmed, so `  ---`
    /// opened a block here and opened none in charter.
    #[test]
    fn the_opening_fence_is_the_first_three_bytes_and_not_a_trimmed_line() {
        assert_eq!(role_of("---\nrole: ops\n---\n").as_deref(), Some("ops"));
        assert_eq!(role_of("  ---\nrole: ops\n---\n"), None);
        assert_eq!(role_of("\n---\nrole: ops\n---\n"), None);
        assert_eq!(
            role_of("---\nname: devops\n---\nrole: in the body\n"),
            None,
            "below the closing fence is body, and the body is not read for keys"
        );
    }

    /// `persona.parse` is `dict(pairs)`: two lines carrying one key collapse to the LAST.
    /// Every other consumer of a definition already reads it that way ([`meta`]); this one
    /// used to answer with the first line it saw.
    #[test]
    fn a_role_written_twice_answers_with_the_last_line_as_a_dict_does() {
        assert_eq!(
            role_of("---\nrole: first\nrole: second\n---\n").as_deref(),
            Some("second")
        );
    }

    /// Split at the FIRST colon, both halves stripped — so a value holding a colon keeps it.
    #[test]
    fn a_role_is_split_at_the_first_colon_and_keeps_the_rest_of_the_line() {
        assert_eq!(
            role_of("---\nrole:   ops: and then some  \n---\n").as_deref(),
            Some("ops: and then some")
        );
        assert_eq!(
            role_of("---\nrole:\n---\n").as_deref(),
            Some(""),
            "a declared role of nothing is declared, and is not no role at all"
        );
    }
}

#[cfg(test)]
mod detail_tests {
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

    /// Everything a reader is shown, off the shape the fixture plane's `devops` has.
    #[test]
    fn a_persona_says_its_role_when_to_delegate_to_it_its_tools_and_its_vault() {
        let dir = plane();
        persona(
            dir.path(),
            "devops",
            "---\nname: devops\nrole: DevOps Engineer\nvault: devops\ntools: kubectl, glab\n\
             delegate-when: CI/CD pipelines, k8s deploys\n---\n\n# DevOps Engineer\nbody\n",
        );

        let shown = details(dir.path(), "devops").expect("it loads");

        assert_eq!(shown.name, "devops");
        assert_eq!(shown.role.as_deref(), Some("DevOps Engineer"));
        assert_eq!(
            shown.delegate_when.as_deref(),
            Some("CI/CD pipelines, k8s deploys")
        );
        assert_eq!(shown.tools, ["kubectl", "glab"]);
        assert_eq!(shown.vault, Vault::Named("devops".to_string()));
        assert_eq!(shown.lineage, ["devops"]);
        assert_eq!(shown.file, "personas/devops/persona.md");
    }

    /// The three answers about a vault, and the reason there are three: a persona that
    /// DECLARES it holds no credentials is not the same as one nobody has said anything
    /// about — charter's own `vault_of` falls back to the vault registry for the second,
    /// and nothing in Rust reads that registry yet.
    #[test]
    fn a_declared_none_is_not_the_same_answer_as_nothing_declared() {
        let dir = plane();
        persona(
            dir.path(),
            "steward",
            "---\nrole: Steward\nvault: none\n---\n",
        );
        persona(dir.path(), "release", "---\nrole: Release\n---\n");
        persona(dir.path(), "forge", "---\nrole: Forge\nvault: forge\n---\n");

        assert_eq!(
            details(dir.path(), "steward").unwrap().vault,
            Vault::DeclaredNone
        );
        assert_eq!(
            details(dir.path(), "release").unwrap().vault,
            Vault::Undeclared
        );
        assert_eq!(
            details(dir.path(), "forge").unwrap().vault,
            Vault::Named("forge".to_string())
        );
    }

    /// `persona.resolve`, root first: a child's line wins, and a line with nothing after the
    /// colon is skipped rather than written — so a child that writes `role:` empty keeps its
    /// parent's role instead of erasing it.
    #[test]
    fn a_child_overrides_its_parent_and_a_blank_value_overrides_nothing() {
        let dir = plane();
        persona(
            dir.path(),
            "base",
            "---\nrole: Base\nvault: shared-vault\ndelegate-when: anything at all\n---\n",
        );
        persona(
            dir.path(),
            "child",
            "---\nextends: base\nrole:\nvault: child-vault\n---\n",
        );

        let shown = details(dir.path(), "child").expect("it loads");

        assert_eq!(
            shown.role.as_deref(),
            Some("Base"),
            "a blank `role:` erased the parent's"
        );
        assert_eq!(shown.vault, Vault::Named("child-vault".to_string()));
        assert_eq!(shown.delegate_when.as_deref(), Some("anything at all"));
        assert_eq!(shown.lineage, ["child", "base"], "child first");
    }

    /// `tools_of`: the UNION across the chain, in the order a parent declared them first,
    /// deduplicated — not the child's line replacing the parent's.
    #[test]
    fn tools_are_the_union_down_the_chain_with_the_parents_first() {
        let dir = plane();
        persona(dir.path(), "base", "---\ntools: gh, git\n---\n");
        persona(
            dir.path(),
            "child",
            "---\nextends: base\ntools: git, kubectl\n---\n",
        );

        assert_eq!(
            details(dir.path(), "child").unwrap().tools,
            ["gh", "git", "kubectl"]
        );
    }

    /// `_csv_list` strips `[` and `]` as CHARACTERS off both ends, so the bracketed spelling
    /// a YAML habit produces answers with the same list as the bare one.
    #[test]
    fn a_bracketed_tools_list_is_the_same_list() {
        let dir = plane();
        persona(dir.path(), "a", "---\ntools: [gh, glab]\n---\n");
        persona(dir.path(), "b", "---\ntools: gh, glab\n---\n");
        persona(dir.path(), "c", "---\ntools: gh, , glab,\n---\n");

        let tools = |name: &str| details(dir.path(), name).unwrap().tools;
        assert_eq!(tools("a"), ["gh", "glab"]);
        assert_eq!(tools("b"), ["gh", "glab"]);
        assert_eq!(tools("c"), ["gh", "glab"], "an empty part is thrown away");
    }

    /// The refusal is [`name_refusal`]'s, word for word: one sentence per fix, wherever a
    /// persona name is taken.
    #[test]
    fn a_name_this_plane_does_not_define_is_refused_in_charters_own_words() {
        let dir = plane();

        assert_eq!(
            details(dir.path(), "nope").unwrap_err(),
            "no persona 'nope' (add it: write personas/nope/persona.md)"
        );
        assert_eq!(
            details(dir.path(), "Bad").unwrap_err(),
            "invalid persona name 'Bad' (lowercase letters, digits, '.', '_', '-')"
        );
        assert_eq!(
            details(dir.path(), "_shared").unwrap_err(),
            "invalid persona name '_shared' (lowercase letters, digits, '.', '_', '-')"
        );
    }

    /// The legacy flat layout is a persona too, and the file a reader is told about is the
    /// one charter would open.
    #[test]
    fn the_file_named_is_the_one_charter_reads_including_the_flat_layout() {
        let dir = plane();
        std::fs::create_dir_all(dir.path().join("personas")).unwrap();
        std::fs::write(
            dir.path().join("personas/flat.md"),
            "---\nrole: Flat\n---\n",
        )
        .unwrap();

        let shown = details(dir.path(), "flat").expect("it loads");

        assert_eq!(shown.file, "personas/flat.md");
        assert_eq!(shown.role.as_deref(), Some("Flat"));
    }

    /// A definition below the closing fence is BODY, and the body is never read for keys —
    /// so nothing a persona writes into its charter can arrive on a panel as a vault name.
    #[test]
    fn nothing_below_the_frontmatter_becomes_one_of_these_keys() {
        let dir = plane();
        persona(
            dir.path(),
            "a",
            "---\nrole: Real\n---\nvault: not-a-vault\ntools: kubectl\n",
        );

        let shown = details(dir.path(), "a").expect("it loads");

        assert_eq!(shown.vault, Vault::Undeclared);
        assert_eq!(shown.tools, [] as [String; 0]);
    }

    /// A key written twice collapses to the LAST line, which is what `dict(pairs)` does and
    /// what every other consumer of a definition already gets.
    #[test]
    fn a_key_written_twice_answers_with_the_last_line() {
        let dir = plane();
        persona(
            dir.path(),
            "a",
            "---\nvault: first\nvault: second\nrole: one\nrole: two\n---\n",
        );

        let shown = details(dir.path(), "a").expect("it loads");

        assert_eq!(shown.vault, Vault::Named("second".to_string()));
        assert_eq!(shown.role.as_deref(), Some("two"));
    }
}
