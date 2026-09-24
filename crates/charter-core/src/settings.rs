//! A plane's settings files, as the Project settings tab reads, checks and writes them
//! (charter-app#252).
//!
//! Two files, and the difference between them is who sees it:
//!
//! - **Shared** is `charter.toml`: committed, so every clone of the plane has it.
//! - **Local** is `charter.local.toml`: gitignored, this machine's alone. Harness profiles live
//!   there (ADR 0022).
//!
//! **Nothing here has rules of its own about what a file may say.** A save is refused for
//! exactly what the next read of that file would refuse, in the sentences that read uses:
//! [`crate::doctor`]'s `charter.toml` row for the Shared file (the loader's own parse and
//! schema refusals, and the settings it reads as absent) and [`crate::profiles`] for the
//! Local one — asked of the text about to be written rather than of the file on disk
//! ([`crate::profiles::derive_from`]) — and, in either file, `[extensions]` is asked of the one
//! reader of it, [`crate::extension::project::refusals`], as `[harness_plugins]` is of
//! [`crate::harness_plugin::refusals`]. Two more refusals are a writer's, because only a writer
//! can cause them: a Local file git would commit ([`crate::profiles::ignore_check_before_writing`],
//! whose sentences are the loader's too), and a secret-shaped value in either file
//! ([`crate::secretshape::secret_kind`], the classifier the leak guard and `charter save` use).
//!
//! **Edits keep the file the operator wrote.** A form's change is applied with `toml_edit`, which
//! keeps every comment, blank line, key order and spacing it did not touch, and replaces a value
//! in place with the decoration it had — so changing one key changes one line.

use std::path::{Path, PathBuf};

use crate::profiles::{self, COMMITTED_FILE, LOCAL_FILE};

/// Which of a plane's two settings files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// `charter.toml` — committed; the team sees it.
    Shared,
    /// `charter.local.toml` — gitignored; this machine only.
    Local,
}

impl Which {
    /// The file's name, at the plane's root.
    pub fn file(self) -> &'static str {
        match self {
            Self::Shared => COMMITTED_FILE,
            Self::Local => LOCAL_FILE,
        }
    }

    fn path(self, root: &Path) -> PathBuf {
        root.join(self.file())
    }
}

/// One settings file as it stands, and what charter says about it now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    /// `charter.toml` or `charter.local.toml`.
    pub file: &'static str,
    /// Whether it is there. A Local file that is not is created by the first save.
    pub exists: bool,
    /// Its text, or empty when it is not there.
    pub text: String,
    /// What saving it as it stands would be refused for — what charter already ignores in it.
    pub refusals: Vec<String>,
}

/// One step of the way to a key: a table's key, or an array of tables' place (`[[forge]]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Key(String),
    Index(usize),
}

/// A value a form can write. Every key charter documents in either file is one of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Integer(i64),
    Bool(bool),
    List(Vec<String>),
}

/// One change: set the key at `path` to `value`, or remove it when `value` is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub path: Vec<Step>,
    pub value: Option<Value>,
}

/// `which` of the plane at `root`, and what charter says about it as it stands.
pub fn read(root: &Path, which: Which) -> Result<Read, String> {
    let (exists, text) = on_disk(root, which)?;
    let refusals = refusals(root, which, &text);
    Ok(Read {
        file: which.file(),
        exists,
        text,
        refusals,
    })
}

/// Whether the file is there and its text, or why it could not be read.
fn on_disk(root: &Path, which: Which) -> Result<(bool, String), String> {
    match std::fs::read(which.path(root)) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(|text| (true, text))
            .map_err(|_| {
                format!(
                    "{} is not UTF-8 text, so charter cannot show it",
                    which.file()
                )
            }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((false, String::new())),
        Err(e) => Err(format!(
            "{} could not be read ({})",
            which.file(),
            crate::shown::short(&e.to_string())
        )),
    }
}

/// Everything charter would refuse in `text` as `which` of the plane at `root`, each as the
/// sentence the reader that refuses it says. Empty when the text may be saved.
pub fn refusals(root: &Path, which: Which, text: &str) -> Vec<String> {
    let mut out = read_refusals(root, which, text);
    out.extend(writer_refusals(root, which, text));
    out
}

/// What the readers of the file refuse in `text`: the doctor's `charter.toml` row for Shared,
/// the profiles loader for Local.
fn read_refusals(root: &Path, which: Which, text: &str) -> Vec<String> {
    let mut out = match which {
        Which::Shared => shared_refusals(root, text),
        Which::Local => local_refusals(root, text),
    };
    // Either file may hold `[extensions]` (charter-app#253), and it is read by one reader in
    // both, so it is refused in that reader's words in both.
    out.extend(crate::extension::project::refusals(text, which.file()));
    // And `[harness_plugins]` (charter-app#274), the same way.
    out.extend(crate::harness_plugin::refusals(text, which.file()));
    // And `[theme]` (charter-app#273), read by one reader in both too.
    out.extend(crate::extension::project::theme::refusals(
        text,
        which.file(),
    ));
    out
}

/// What only a writer can cause, and so what stops every save however long the file has held
/// it: a Local file git would commit, and a secret in either file.
fn writer_refusals(root: &Path, which: Which, text: &str) -> Vec<String> {
    let mut out = Vec::new();
    if which == Which::Local {
        let check = profiles::ignore_check_before_writing(root);
        if !check.passes() {
            out.push(check.reason);
        }
    }
    if let Some(kind) = crate::secretshape::secret_kind(text) {
        out.push(secret_refusal(which, kind));
    }
    out
}

/// The reasons the set refused a table or a profile in `file`, and the doctor's findings about
/// `cfg`, as one sentence each — the finding's summary, then what to do.
fn said(root: &Path, set: &profiles::ProfileSet, file: &str, cfg: &toml::Table) -> Vec<String> {
    set.refused
        .iter()
        .filter(|refused| refused.source == file)
        .map(|refused| refused.reason.clone())
        .chain(
            crate::doctor::config_findings(root, cfg, set)
                .into_iter()
                .map(|(summary, detail)| format!("{summary} — {detail}")),
        )
        .collect()
}

/// The Shared file's: the doctor's `charter.toml` row, every finding rather than the first,
/// and a profile table that belongs in the Local file.
fn shared_refusals(root: &Path, text: &str) -> Vec<String> {
    let cfg = match crate::doctor::Config::parse(&Which::Shared.path(root), text.into()) {
        crate::doctor::Config::Read(cfg) => cfg,
        crate::doctor::Config::Malformed(why) | crate::doctor::Config::Refused(why) => {
            return vec![why];
        }
    };
    let set = profiles::current_of(profiles::derive_from(
        Some(text),
        profiles::read_local(root),
    ));
    said(root, &set, COMMITTED_FILE, &cfg)
}

/// The Local file's: every profile the loader would refuse, and a `default` that names none it
/// keeps.
fn local_refusals(root: &Path, text: &str) -> Vec<String> {
    let committed = std::fs::read_to_string(Which::Shared.path(root)).ok();
    let set = profiles::current_of(profiles::derive_from(
        committed.as_deref(),
        Ok(Some(text.to_owned())),
    ));
    let default = text
        .parse::<toml::Table>()
        .map(|cfg| only_the_default(&cfg))
        .unwrap_or_default();
    said(root, &set, LOCAL_FILE, &default)
}

/// Just `[harness] default` of `cfg`, so the doctor's findings are asked about that key alone.
/// Anything else at the top of the Local file is the profiles loader's to refuse, and it does,
/// with its own sentence (`[<key>] in charter.local.toml is not read`).
fn only_the_default(cfg: &toml::Table) -> toml::Table {
    let mut out = toml::Table::new();
    if let Some(default) = cfg
        .get("harness")
        .and_then(toml::Value::as_table)
        .and_then(|harness| harness.get("default"))
    {
        let mut harness = toml::Table::new();
        harness.insert("default".to_owned(), default.clone());
        out.insert("harness".to_owned(), toml::Value::Table(harness));
    }
    out
}

/// A secret-shaped value, by its KIND and never its text: this sentence is drawn in the window,
/// and one that quoted the credential would show it to whoever is looking.
fn secret_refusal(which: Which, kind: &str) -> String {
    let who = match which {
        Which::Shared => "it is committed, so every clone of this plane would carry the secret",
        Which::Local => "it is a plain file any program on this machine can read",
    };
    format!(
        "{} looks like it holds a secret ({kind}), so nothing was saved — {who}. Keep the \
         value in a vault and name it where it is needed as vault:<vault>/<key>.",
        which.file()
    )
}

/// A value found in a file: one a form can write, or one only the raw view changes — a float,
/// a date, a list that is not all text — as TOML writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Value(Value),
    Other(String),
}

/// Every value in `text` and the path to it, in file order — or `None` when it is not TOML.
///
/// A table is walked into, and so is an array of tables (`[[forge]]`), by place. What a form
/// shows is read from this, so the window needs no TOML parser of its own, and a key no form
/// knows yet is already here for the one that will.
pub fn fields(text: &str) -> Option<Vec<(Vec<Step>, Found)>> {
    let table: toml::Table = text.parse().ok()?;
    let mut out = Vec::new();
    flatten(&table, &mut Vec::new(), &mut out);
    Some(out)
}

fn flatten(table: &toml::Table, at: &mut Vec<Step>, out: &mut Vec<(Vec<Step>, Found)>) {
    for (key, value) in table {
        at.push(Step::Key(key.clone()));
        match value {
            toml::Value::Table(inner) => flatten(inner, at, out),
            toml::Value::Array(items)
                if !items.is_empty() && items.iter().all(toml::Value::is_table) =>
            {
                for (i, item) in items.iter().enumerate() {
                    at.push(Step::Index(i));
                    if let Some(inner) = item.as_table() {
                        flatten(inner, at, out);
                    }
                    at.pop();
                }
            }
            leaf => out.push((at.clone(), found(leaf))),
        }
        at.pop();
    }
}

fn found(value: &toml::Value) -> Found {
    match value {
        toml::Value::String(text) => Found::Value(Value::Text(text.clone())),
        toml::Value::Integer(n) => Found::Value(Value::Integer(*n)),
        toml::Value::Boolean(b) => Found::Value(Value::Bool(*b)),
        toml::Value::Array(items) if items.iter().all(toml::Value::is_str) => {
            Found::Value(Value::List(
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect(),
            ))
        }
        other => Found::Other(other.to_string()),
    }
}

/// `text` with `edits` applied, keeping everything they do not touch as it was written.
///
/// A missing table on the way is created; a key that is there has its value replaced in place,
/// keeping the spacing and the comment beside it. Refused, rather than guessed at, when the
/// text is not TOML or a step goes through something that is not a table.
pub fn edited(text: &str, edits: &[Edit]) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| {
        format!(
            "the file is not valid TOML ({}), so a form cannot change it — fix it in the raw \
                 view",
            crate::shown::short(e.message())
        )
    })?;
    for edit in edits {
        apply(doc.as_table_mut(), edit)?;
    }
    Ok(doc.to_string())
}

/// What a path is called in a sentence: `memory.share`, `forge[0].host`.
fn dotted(path: &[Step]) -> String {
    let mut out = String::new();
    for step in path {
        match step {
            Step::Key(key) if out.is_empty() => out.push_str(key),
            Step::Key(key) => {
                out.push('.');
                out.push_str(key);
            }
            Step::Index(at) => out.push_str(&format!("[{at}]")),
        }
    }
    out
}

fn apply(root: &mut toml_edit::Table, edit: &Edit) -> Result<(), String> {
    let Some((Step::Key(last), way)) = edit.path.split_last() else {
        return Err(format!("{} does not end in a key", dotted(&edit.path)));
    };
    // Removing a key from a table that is not there is already done.
    let Some(table) = walk(root, way, &edit.path, edit.value.is_some())? else {
        return Ok(());
    };
    match &edit.value {
        None => {
            table.remove(last);
        }
        Some(value) => {
            let mut fresh = to_edit(value);
            match table.get_mut(last) {
                // In place, with the spacing and the comment the old value had.
                Some(toml_edit::Item::Value(old)) if !old.is_inline_table() => {
                    *fresh.decor_mut() = old.decor().clone();
                    *old = fresh;
                }
                None | Some(toml_edit::Item::None) => {
                    table.insert(last, toml_edit::value(fresh));
                }
                // A whole section is never replaced by one value.
                Some(_) => {
                    return Err(format!(
                        "{} is a table, so a form cannot set it to one value",
                        dotted(&edit.path)
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The table `steps` lead to from `table` — a `[section]` or an inline `{ … }`, which is how a
/// profile's `env` is usually written — creating a missing one when `creating`, or `None`
/// when it is missing and not to be created. `full` is the whole path, for the sentence.
fn walk<'t>(
    table: &'t mut dyn toml_edit::TableLike,
    steps: &[Step],
    full: &[Step],
    creating: bool,
) -> Result<Option<&'t mut dyn toml_edit::TableLike>, String> {
    let Some(step) = steps.first() else {
        return Ok(Some(table));
    };
    let here = full.len() - steps.len();
    let named = |upto: usize| dotted(&full[..=upto]);
    let Step::Key(key) = step else {
        return Err(format!("{} is not a list of tables", dotted(&full[..here])));
    };
    if table.get(key).is_none() {
        if !creating {
            return Ok(None);
        }
        table.insert(key, toml_edit::table());
    }
    let Some(item) = table.get_mut(key) else {
        unreachable!("the key was there or has just been inserted");
    };
    // `[[forge]]`: the next step says which block.
    if let toml_edit::Item::ArrayOfTables(blocks) = item {
        return match steps.get(1) {
            Some(Step::Index(at)) => match blocks.get_mut(*at) {
                Some(block) => walk(block, &steps[2..], full, creating),
                None => Err(format!("{} is not there", named(here + 1))),
            },
            _ => Err(format!("{} is a list of tables; say which", named(here))),
        };
    }
    match item.as_table_like_mut() {
        Some(inner) => walk(inner, &steps[1..], full, creating),
        None => Err(format!("{} is not a table, so it has no keys", named(here))),
    }
}

fn to_edit(value: &Value) -> toml_edit::Value {
    match value {
        Value::Text(text) => text.as_str().into(),
        Value::Integer(n) => (*n).into(),
        Value::Bool(b) => (*b).into(),
        Value::List(items) => items
            .iter()
            .map(String::as_str)
            .collect::<toml_edit::Array>()
            .into(),
    }
}

/// Write `text` as `which` of the plane at `root` — or say every reason it was not written.
///
/// `base` is the file as the caller read it (`None`: it was not there). A file that has changed
/// since is not overwritten: the tab would be writing over an edit it never saw. The write is
/// whole or not at all — a temp file beside it, then one rename — keeps the mode an existing
/// file had, and makes a new one readable by this user alone.
pub fn save(root: &Path, which: Which, base: Option<&str>, text: &str) -> Result<(), Vec<String>> {
    let (exists, now) = on_disk(root, which).map_err(|why| vec![why])?;
    if (exists, now.as_str()) != (base.is_some(), base.unwrap_or_default()) {
        return Err(vec![format!(
            "{} changed on disk since this tab read it, so nothing was saved. Read it again, \
             then make the change again.",
            which.file()
        )]);
    }
    // What the file already holds is not this save's to answer for: an edit to one key is not
    // refused because another key was already being ignored. A secret and a Local file git
    // would commit are, whatever the file held before.
    let standing = if exists {
        read_refusals(root, which, &now)
    } else {
        Vec::new()
    };
    let mut refused: Vec<String> = read_refusals(root, which, text)
        .into_iter()
        .filter(|why| !standing.contains(why))
        .collect();
    refused.extend(writer_refusals(root, which, text));
    if !refused.is_empty() {
        return Err(refused);
    }
    write(root, which, text).map_err(|e| {
        vec![format!(
            "{} could not be written ({}), so nothing was saved.",
            which.file(),
            crate::shown::short(&e.to_string())
        )]
    })
}

fn write(root: &Path, which: Which, text: &str) -> std::io::Result<()> {
    use std::io::Write;

    let path = which.path(root);
    crate::contain::no_link_on_the_way(root, &path)?;
    let mode = std::fs::metadata(&path).ok().map(|m| m.permissions());
    let mut temp = tempfile::NamedTempFile::new_in(root)?;
    temp.write_all(text.as_bytes())?;
    temp.as_file().sync_all()?;
    if let Some(permissions) = mode {
        temp.as_file().set_permissions(permissions)?;
    }
    // A new file keeps `NamedTempFile`'s 0600: nobody else on this machine has any business
    // reading this plane's settings, and the Local file is this user's by definition.
    temp.persist(&path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests;
