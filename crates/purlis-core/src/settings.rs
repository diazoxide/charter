//! A plane's settings files, as the Settings tab reads, checks and writes them
//! (charter-app#252).
//!
//! Two files, and the difference between them is who sees it:
//!
//! - **Shared** is `charter.toml`: committed, so every clone of the plane has it.
//! - **Local** is `charter.local.toml`: gitignored, this machine's alone. Harness profiles live
//!   there (ADR 0022).
//!
//! A third holder of settings is each workspace's `workspace.json`, whose `settings` sit between
//! the two (charter-app#280): [`workspace`], read and written by the same readers' rules.
//!
//! **Every reader of the two files' layers reads them through [`layer_text`]**, which leaves out
//! a Local file git would carry (charter-app#308). This tab is the one place such a file is still
//! shown, with the reason it is not read.
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

pub mod collection;
pub mod forges;
pub mod harness_profiles;
pub mod hosts;
pub mod workspace;

/// Where an answer came from: which layer of the Shared/Workspace/Local overlay decided it
/// (ADR 0048). Every reader of the overlay answers with it: [`crate::extension::project`],
/// [`crate::planesave`] and the rest (charter-app#309).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// No file said: the machine's answer, or the setting's declared default.
    Default,
    /// `charter.toml`.
    Shared,
    /// The `settings` of the workspace's `workspace.json` (charter-app#280).
    Workspace,
    /// `charter.local.toml`.
    Local,
}

impl Source {
    /// The file this source is, or `None` for [`Source::Default`]. A workspace's is
    /// `workspace.json`; which workspace's, [`crate::extension::project::Choices::workspace_file`] says.
    pub fn file(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Shared => Some(COMMITTED_FILE),
            Self::Workspace => Some(crate::settings::workspace::FILE),
            Self::Local => Some(LOCAL_FILE),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Shared => "shared",
            Self::Workspace => "workspace",
            Self::Local => "local",
        }
    }
}

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

    /// The file at `root`, under the name the plane has it by (RN-2a): a write lands in the
    /// file that is there, never beside it.
    fn path(self, root: &Path) -> PathBuf {
        root.join(self.file_at(root))
    }

    /// The file's name at `root`: [`Self::file`] until the plane has it under its purlis name.
    pub fn file_at(self, root: &Path) -> &'static str {
        let name = match self {
            Self::Shared => crate::names::PLANE_MANIFEST,
            Self::Local => crate::names::LOCAL_SETTINGS,
        };
        name.spelling_at(root, Path::is_file)
    }

    /// The other of the two.
    pub fn other(self) -> Self {
        match self {
            Self::Shared => Self::Local,
            Self::Local => Self::Shared,
        }
    }
}

/// One layer as its readers are handed it by [`layer_text`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerText {
    /// The file's text.
    Text(String),
    /// No file, or one that cannot be read: it says nothing, and there is nothing to say about it.
    Nothing,
    /// A Local file git would carry, left out. `why` is [`profiles::ignore_check`]'s sentence,
    /// which is also what the Settings tab says at the Project level about it ([`read`]).
    /// `text` is what the file says, which no reader takes — only asks whether it said anything
    /// in its own table, so a group says the file was left out only where it would have decided
    /// something (charter-app#319).
    LeftOut { why: String, text: String },
}

impl LayerText {
    /// The text a reader reads, or `None`.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Nothing | Self::LeftOut { .. } => None,
        }
    }

    /// Why the file is there and was not read, when that is so.
    pub fn left_out(&self) -> Option<&str> {
        match self {
            Self::LeftOut { why, .. } => Some(why),
            Self::Text(_) | Self::Nothing => None,
        }
    }

    /// The sentence a reader keeps, when the file was left out **and** its `said` — the reader's
    /// own parse of the left-out text — says something: a group whose table the file does not
    /// touch has nothing that was not applied, and says nothing (charter-app#319).
    pub fn left_out_where(&self, said: impl FnOnce(&toml::Table) -> bool) -> Option<String> {
        match self {
            Self::LeftOut { why, text } => text
                .parse::<toml::Table>()
                .is_ok_and(|top| said(&top))
                .then(|| why.clone()),
            Self::Text(_) | Self::Nothing => None,
        }
    }
}

/// The text of `which` as every reader of its layer takes it — [`LayerText::Text`] only when the
/// file says something charter may use (charter-app#308, ADR 0048).
///
/// A file that is not there, or cannot be read, says nothing. **And so does a Local file git
/// would carry** — tracked, or not ignored, or one git could not say about
/// ([`profiles::ignore_check`]): the Local file is this machine's say only while git leaves it
/// alone, and one that reaches every clone would change plane policy with no trace in git. The
/// profiles loader has refused its profiles on that check since ADR 0022; this is the same check
/// for every other table in the file, so `[extensions]`, `[theme]` and `[harness_plugins]` are
/// not read from it either.
///
/// **Every reader of the two files reads them here**, and nowhere else, so a reader added later
/// cannot forget the check. The Settings tab shows the file's text whatever this says,
/// with the check's sentence among its refusals ([`read`]), so what is not applied is said there
/// with its fix. **And a reader keeps that sentence** ([`LayerText::LeftOut`], charter-app#319)
/// when the file said something in its table: each group of a settings tab that shows what is in
/// force says it where the file would have decided something, so a value set in Local and not
/// applied is never shown without its reason — in the same words, from the same check.
///
/// A Local file that is there costs one `git status` of that one path; an absent one costs none.
pub fn layer_text(root: &Path, which: Which) -> LayerText {
    let Ok(text) = std::fs::read_to_string(which.path(root)) else {
        return LayerText::Nothing;
    };
    if which == Which::Local {
        let check = profiles::ignore_check(root);
        if !check.passes() {
            return LayerText::LeftOut {
                why: check.reason,
                text,
            };
        }
    }
    LayerText::Text(text)
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
        file: which.file_at(root),
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
                    "{} is not UTF-8 text, so purlis cannot show it",
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
    out.into_iter().map(|said| named_at(root, &said)).collect()
}

/// `said`, a sentence a reader wrote naming the project's files by their old names, naming them
/// `shared` and `local` instead: the names they have in this project (#1340).
pub fn named_as(said: &str, shared: &str, local: &str) -> String {
    // The local name first: the committed name is not part of it, and a new local name never
    // holds the old committed one.
    whole_name(&whole_name(said, LOCAL_FILE, local), COMMITTED_FILE, shared)
}

/// `said` with each `old` that stands as a whole file name replaced by `new`: one no name
/// character runs into on either side, so a host or a path a person wrote that only holds it
/// (`charter.toml.example.com`, `mycharter.toml`) is quoted as written. A sentence's own full
/// stop after it still ends the name, and a folder before it is the file's own.
fn whole_name(said: &str, old: &str, new: &str) -> String {
    let part = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_');
    let mut out = String::with_capacity(said.len());
    let mut rest = said;
    while let Some(at) = rest.find(old) {
        let (before, from) = rest.split_at(at);
        let after = &from[old.len()..];
        let led = before
            .chars()
            .next_back()
            .is_some_and(|c| part(c) || c == '.');
        let mut next = after.chars();
        let runs_on = match next.next() {
            Some('.') => next.next().is_some_and(part),
            Some(c) => part(c),
            None => false,
        };
        out.push_str(before);
        out.push_str(if led || runs_on { old } else { new });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// `said`, naming the project's two files as the project at `root` has them (#1340).
pub fn named_at(root: &Path, said: &str) -> String {
    named_as(
        said,
        Which::Shared.file_at(root),
        Which::Local.file_at(root),
    )
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
    // And `[sandbox]` (ADR 0067), which only the Shared file may hold, and only as `on`.
    out.extend(crate::sandbox::refusals(text, which.file()));
    // And so may `[plane]` and `[repos]` (charter-app#292), read by `planesave` in both.
    out.extend(crate::planesave::refusals(
        text,
        which == Which::Local,
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
    // As typed, and as charter reads it (#1304): the one answer the project save gives a
    // staged charter.toml, so the editor and the save refuse the same files.
    if let Some(kind) =
        crate::secretshape::kind_as_read(Some(crate::secretshape::Structured::Toml), text)
    {
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
    let mut out = said(root, &set, COMMITTED_FILE, &cfg);
    out.extend(names_nothing(root, &cfg));
    out
}

/// `[persona] default` naming no persona in this project (ST-1, #1225), as `[harness] default`'s
/// is said: the Settings tab draws it as a picker over what is here, and a value set by hand that
/// names nothing is shown with this. Charter already reads such a default as none
/// ([`crate::active::plane_default_persona`]), and the alerts call it the front door.
///
/// Not `[workspace] default` (D-ST1-1, amended): a declared default workspace is one charter
/// treats as there and makes where it is first used (`wscmd::select`), so naming one not made
/// yet is no refusal. A value shaped like a secret is never quoted back.
fn names_nothing(root: &Path, cfg: &toml::Table) -> Vec<String> {
    let named = |table: &str| {
        cfg.get(table)
            .and_then(toml::Value::as_table)
            .and_then(|t| t.get("default"))
            .map(|v| crate::memstore::py_strip(&crate::pyrepr::str_toml(v)).to_owned())
            .filter(|v| !v.is_empty())
            // Never said back when it is shaped like a secret: the secret's own refusal, by
            // its kind, is the one sentence about it.
            .filter(|v| crate::secretshape::kind_as_read(None, v).is_none())
    };
    let mut out = Vec::new();
    if let Some(persona) = named("persona")
        && !crate::alerts::persona_exists(root, &persona)
    {
        out.push(format!(
            "[persona] default = \"{}\" names no persona in this project, so purlis reads it \
             as no default. Pick one that is here, or make it.",
            crate::shown::short(&persona)
        ));
    }
    out
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

/// A value found in a file: one a form can write, or one only Edit as TOML changes — a float,
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
    Some(fields_of(&table))
}

/// [`fields`], of a table already read — a workspace's settings, read as the table they mirror.
pub(crate) fn fields_of(table: &toml::Table) -> Vec<(Vec<Step>, Found)> {
    let mut out = Vec::new();
    flatten(table, &mut Vec::new(), &mut out);
    out
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
        other => Found::Other(crate::pyrepr::toml_09_datetimes(other).to_string()),
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
            "the file is not valid TOML ({}), so a form cannot change it — fix it under Edit \
                 as TOML",
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
///
/// The check and the write are one act: both happen under [`crate::rewrite::Lock`], which
/// every other writer of these files takes too, so nothing can land between "unchanged since
/// the tab read it" and the rename that would overwrite it (#357).
pub fn save(root: &Path, which: Which, base: Option<&str>, text: &str) -> Result<(), Vec<String>> {
    let _held = crate::rewrite::Lock::on(root);
    let (exists, now) = unchanged(root, which, base)?;
    let refused = refused_of(root, which, exists.then_some(now.as_str()), text);
    if !refused.is_empty() {
        return Err(refused);
    }
    write(root, which, text).map_err(|e| vec![not_written(which, &e)])
}

/// The file as it is on disk — whether it is there, and its text — when it is still what the
/// caller read (`base`, `None`: it was not there); otherwise why nothing is written.
pub(crate) fn unchanged(
    root: &Path,
    which: Which,
    base: Option<&str>,
) -> Result<(bool, String), Vec<String>> {
    let (exists, now) = on_disk(root, which).map_err(|why| vec![why])?;
    if (exists, now.as_str()) != (base.is_some(), base.unwrap_or_default()) {
        return Err(vec![format!(
            "{} changed on disk since this tab read it, so nothing was saved. Read it again, \
             then make the change again.",
            which.file()
        )]);
    }
    Ok((exists, now))
}

/// Why `text` may not replace `now` (`None`: no file yet) as `which`.
///
/// What the file already holds is not this write's to answer for: an edit to one key is not
/// refused because another key was already being ignored. A secret and a Local file git would
/// commit are, whatever the file held before. So is text that does not parse: the raw editor is
/// how a broken file is mended, and a half-mended one is never written (SE-19).
fn refused_of(root: &Path, which: Which, now: Option<&str>, text: &str) -> Vec<String> {
    let parses = text.parse::<toml::Table>().is_ok();
    let standing = now
        .filter(|_| parses)
        .map_or_else(Vec::new, |now| read_refusals(root, which, now));
    let mut refused: Vec<String> = read_refusals(root, which, text)
        .into_iter()
        .filter(|why| !standing.contains(why))
        .collect();
    refused.extend(writer_refusals(root, which, text));
    refused
}

fn not_written(which: Which, e: &std::io::Error) -> String {
    format!(
        "{} could not be written ({}), so nothing was saved.",
        which.file(),
        crate::shown::short(&e.to_string())
    )
}

/// **Move the values at `paths` into `to`, out of the other file** (SE-18, V89d): what the
/// Settings tab's "Shared / Only on this machine" choice does. Each value leaves the file it is
/// in and is set in `to`, as it was written — a value `to` already held there is replaced, in
/// place. A table the move empties goes with it.
///
/// `from_base` and `to_base` are the two files as the caller read them (`None`: not there), and
/// a file changed since is not written, as [`save`]'s. **Both files or neither**: each new text
/// is checked as [`save`] checks it before anything is written; then `to` is written first and
/// the other file after, so the value is never in neither file; and if that second write fails,
/// `to` is put back as it was (taken away again when the move created it).
pub fn move_keys(
    root: &Path,
    to: Which,
    from_base: Option<&str>,
    to_base: Option<&str>,
    paths: &[Vec<Step>],
) -> Result<(), Vec<String>> {
    let from = to.other();
    let _held = crate::rewrite::Lock::on(root);
    let (from_exists, from_now) = unchanged(root, from, from_base)?;
    let (to_exists, to_now) = unchanged(root, to, to_base)?;
    let (from_text, to_text) = moved(&from_now, &to_now, from, paths).map_err(|why| vec![why])?;
    let mut refused = refused_of(
        root,
        from,
        from_exists.then_some(from_now.as_str()),
        &from_text,
    );
    refused.extend(refused_of(
        root,
        to,
        to_exists.then_some(to_now.as_str()),
        &to_text,
    ));
    if !refused.is_empty() {
        return Err(refused);
    }
    write(root, to, &to_text).map_err(|e| vec![not_written(to, &e)])?;
    if let Err(e) = write(root, from, &from_text) {
        let mut why = vec![not_written(from, &e)];
        let back = if to_exists {
            write(root, to, &to_now)
        } else {
            std::fs::remove_file(to.path(root))
        };
        if let Err(e) = back {
            why.push(format!(
                "{} could not be put back as it was ({}): the value is now in both files.",
                to.file(),
                crate::shown::short(&e.to_string())
            ));
        }
        return Err(why);
    }
    Ok(())
}

/// The two texts once the values at `paths` have left `from_text` (the file `from`) and been
/// set in `to_text`.
fn moved(
    from_text: &str,
    to_text: &str,
    from: Which,
    paths: &[Vec<Step>],
) -> Result<(String, String), String> {
    let parse = |text: &str, which: Which| {
        text.parse::<toml_edit::DocumentMut>()
            .map_err(|e: toml_edit::TomlError| {
                format!(
                    "{} is not valid TOML ({}), so nothing can be moved — fix it under Edit as TOML",
                    which.file(),
                    crate::shown::short(e.message())
                )
            })
    };
    let mut out = parse(from_text, from)?;
    let mut into = parse(to_text, from.other())?;
    for path in paths {
        let Some((Step::Key(last), way)) = path.split_last() else {
            return Err(format!("{} does not end in a key", dotted(path)));
        };
        let nothing = || {
            format!(
                "{} is not in {}, so there is nothing to move.",
                dotted(path),
                from.file()
            )
        };
        let table = walk(out.as_table_mut(), way, path, false)?.ok_or_else(nothing)?;
        let value = match table.get(last) {
            Some(toml_edit::Item::Value(value)) if !value.is_inline_table() => value.clone(),
            None | Some(toml_edit::Item::None) => return Err(nothing()),
            Some(_) => {
                return Err(format!(
                    "{} is a table, so it is moved under Edit as TOML",
                    dotted(path)
                ));
            }
        };
        table.remove(last);
        prune(out.as_table_mut(), way);
        let Some(target) = walk(into.as_table_mut(), way, path, true)? else {
            unreachable!("a missing table is created");
        };
        match target.get_mut(last) {
            Some(toml_edit::Item::Value(old)) if !old.is_inline_table() => {
                let mut fresh = value;
                *fresh.decor_mut() = old.decor().clone();
                *old = fresh;
            }
            None | Some(toml_edit::Item::None) => {
                let mut fresh = value;
                fresh.decor_mut().clear();
                target.insert(last, toml_edit::value(fresh));
            }
            Some(_) => {
                return Err(format!(
                    "{} is a table in {}, so a value cannot be moved over it",
                    dotted(path),
                    from.other().file()
                ));
            }
        }
    }
    Ok((out.to_string(), into.to_string()))
}

/// Takes out each table on `way` the move left with nothing in it, innermost first.
fn prune(root: &mut toml_edit::Table, way: &[Step]) {
    for depth in (1..=way.len()).rev() {
        let (Some((Step::Key(name), above)), true) = (
            way[..depth].split_last(),
            way[..depth].iter().all(|step| matches!(step, Step::Key(_))),
        ) else {
            return;
        };
        let mut table: &mut toml_edit::Table = root;
        for step in above {
            let Step::Key(key) = step else { return };
            match table.get_mut(key).and_then(toml_edit::Item::as_table_mut) {
                Some(inner) => table = inner,
                None => return,
            }
        }
        let empty = table
            .get(name)
            .and_then(toml_edit::Item::as_table_like)
            .is_some_and(toml_edit::TableLike::is_empty);
        if !empty {
            return;
        }
        table.remove(name);
    }
}

fn write(root: &Path, which: Which, text: &str) -> std::io::Result<()> {
    // A new file is 0600: nobody else on this machine has any business reading this plane's
    // settings, and the Local file is this user's by definition. An existing one keeps its mode.
    crate::rewrite::replace(
        root,
        &which.path(root),
        text.as_bytes(),
        crate::rewrite::Mode::KeptOrPrivate,
    )
}

#[cfg(test)]
mod tests;
