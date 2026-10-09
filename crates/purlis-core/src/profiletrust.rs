//! `.charter/harness-profiles-launched.json` — which harness profiles the operator has
//! approved running.
//!
//! This file decides whether a command runs, so every unreadable state means **ask again**.
//! A missing file, a malformed one, an entry that is not a fingerprint, a link, a FIFO, a
//! planted giant: each reads as "no record", never as approval. Treating silence as a yes is
//! the one state this record exists to keep out.
//!
//! **Where it is a boundary, and where it is not.** In a project that turns the sandbox on, a
//! sandboxed chat is denied writing this record, the declarations' record beside it,
//! `charter.local.toml` and `harnesses/` ([`crate::sandbox::Denied::of`], ADR 0067 §5, #1458),
//! so an approval recorded there is one the person gave in the window. Where the sandbox is
//! off, or for a chat started without it, a chat that can edit `charter.local.toml` can edit
//! this record too, so the ask catches a command the operator did not change themselves
//! *unless whatever changed it also forged the record*: it closes the accident and the careless
//! edit, and nothing that runs unsandboxed should be built as though it were more.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// The file, under the plane's state directory.
pub const RECORD: &str = "harness-profiles-launched.json";

/// What is recorded for a profile: its kind, its command and its environment, **as declared,
/// before `~` is expanded**.
///
/// The expansion is deliberately not recorded. The file is what an edit changes, and a home
/// directory that moved would otherwise make every profile read as *changed* and ask again
/// about a command nobody touched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub kind: String,
    pub command: Vec<String>,
    /// Sorted by name, as charter stores a profile's environment.
    pub env: BTreeMap<String, String>,
}

impl Fingerprint {
    fn to_json(&self) -> serde_json::Value {
        let mut env = serde_json::Map::new();
        for (name, value) in &self.env {
            env.insert(name.clone(), serde_json::Value::String(value.clone()));
        }
        let mut doc = serde_json::Map::new();
        doc.insert("kind".into(), serde_json::Value::String(self.kind.clone()));
        doc.insert(
            "command".into(),
            serde_json::Value::Array(
                self.command
                    .iter()
                    .map(|word| serde_json::Value::String(word.clone()))
                    .collect(),
            ),
        );
        doc.insert("env".into(), serde_json::Value::Object(env));
        serde_json::Value::Object(doc)
    }

    fn from_json(value: &serde_json::Value) -> Option<Self> {
        let doc = value.as_object()?;
        Some(Self {
            kind: doc.get("kind")?.as_str()?.to_string(),
            command: doc
                .get("command")?
                .as_array()?
                .iter()
                .map(|word| word.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()?,
            env: doc
                .get("env")?
                .as_object()?
                .iter()
                .map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                .collect::<Option<BTreeMap<_, _>>>()?,
        })
    }
}

#[cfg(test)]
fn path(root: &Path) -> PathBuf {
    path_of(root, RECORD)
}

fn path_of(root: &Path, record: &str) -> PathBuf {
    crate::names::state(root).join(record)
}

/// The most this record may be. It is one object of short fingerprints, it is read whole,
/// and it is read on the path that decides whether a chat starts — so a planted giant here
/// is a launch that never finishes, not a slow one.
const MAX_BYTES: u64 = 1 << 20;

/// Every recorded profile, or an empty document.
///
/// **Gated, where it used to be a bare `read_to_string`** (M3). This is the file that
/// decides whether charter shows a command line and asks before running it, and it was the
/// one reader of charter's own state in this crate with no gate at all — while its own
/// writer, two functions down, spends a paragraph on why the write is contained. Three
/// things are asked, and each has a reason the writer's paragraph does not cover:
///
/// - **`open_no_link`**, so a link at `.charter/` or at the record itself cannot make this
///   answer out of a file that is not this plane's. `within_plane` would answer the same
///   question a moment earlier; the flag answers it at the instant of the open.
/// - **a plain file**, because a FIFO here is not a slow read, it is a permanent one —
///   `approval_needed` is on the app's startup path, so the app would hang before there is
///   a window or a tray to kill it from. `O_NONBLOCK` is what makes the open return;
///   this is what makes charter decline the thing it returned.
/// - **a bound**, for the same reason `reopen`'s record has one.
///
/// Every one of those, and a malformed or missing file, reads as an EMPTY document — so
/// everything declared asks again. That direction is the whole of this module's rule, and
/// it is why gating here can only ever add questions, never remove one.
pub fn read(root: &Path) -> serde_json::Value {
    read_record(root, RECORD)
}

/// [`read`] of any consent record under `.charter/`, with every one of its gates: this one, or
/// the harness declarations' ([`crate::harness_declaration::APPROVED`]).
fn read_record(root: &Path, record: &str) -> serde_json::Value {
    let nothing = || serde_json::Value::Object(serde_json::Map::new());
    let Ok(mut open) = crate::contain::open_no_link(root, &path_of(root, record)) else {
        return nothing();
    };
    // `fstat` of the descriptor the read will use, not of the name: the two cannot be
    // handed different files.
    let Ok(found) = open.metadata() else {
        return nothing();
    };
    if !found.file_type().is_file() || found.len() > MAX_BYTES {
        return nothing();
    }
    let mut text = String::new();
    if std::io::Read::read_to_string(&mut open, &mut text).is_err() {
        return nothing();
    }
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(nothing)
}

/// What `name` was last approved as, or `None` when there is no record of it.
pub fn last_launched(root: &Path, name: &str) -> Option<Fingerprint> {
    Fingerprint::from_json(read(root).get(name)?)
}

/// Write `name` down as launched, keeping every other profile's record.
pub fn record_launched(root: &Path, name: &str, print: &Fingerprint) -> io::Result<()> {
    record(root, RECORD, name, print.to_json())
}

/// Write `name` down as `entry` in the consent record `record`, keeping every other entry.
fn record(root: &Path, record: &str, name: &str, entry: serde_json::Value) -> io::Result<()> {
    let path = |root: &Path| path_of(root, record);
    // This file records which commands the operator approved RUNNING, so a link that moves
    // it out of the plane moves the consent with it. `contain::writable`'s data roots do not
    // cover it — `.charter` is not one, and Python does not gate this write either — so the
    // rule is spelled out here: the record stays under this plane's own `.charter`.
    // The record itself, not only the directory holding it. Gating `.charter` left this
    // safe because `private_dir` refuses a symlinked state directory and the write ends in
    // a `rename` — true, and one level shallower than the thing opened.
    let state = crate::names::state(root);
    if !crate::contain::within_plane(root, &path(root))
        || !crate::contain::within_plane(root, &state)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "'{}' resolves outside the plane — purlis will not record consent to run a \
                 command through a link that leaves it",
                state.display()
            ),
        ));
    }
    let mut doc = read_record(root, record);
    // One object keyed by name, read and rewritten whole, so approving one profile today
    // does not make another ask again tomorrow. An existing key keeps its position.
    if let Some(map) = doc.as_object_mut() {
        map.insert(name.to_string(), entry);
    }
    let dir = path(root)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.to_path_buf());
    private_dir(&dir)?;
    // 0600: it records the operator's consent to run a command. The walk is rooted at the
    // plane, so the temp file the bytes land on is gated as well as the record (#113).
    crate::rewrite::replace(
        root,
        &path(root),
        crate::pyjson::dumps_indent2(&doc).as_bytes(),
        crate::rewrite::Mode::Private,
    )
}

/// Create the state directory, private to the operator.
pub(crate) fn private_dir(dir: &Path) -> io::Result<()> {
    // `symlink_metadata`, not `is_dir`: the latter is true for a symlink TO a directory, so
    // the early return followed the link instead of refusing it.
    match std::fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() => return Ok(()),
        Ok(meta) if meta.is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "'{}' is a symlink; purlis will not follow one here",
                    dir.display()
                ),
            ));
        }
        _ => {}
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    // Windows has no mode to set; the directory's ACL is inherited.
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Why a profile is being asked about before its command runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    /// Nothing has recorded this profile.
    New,
    /// It was recorded, and what it runs is not what was recorded.
    Changed,
}

impl Approval {
    /// The word a refusal or a prompt says, as the Python charter says it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Changed => "changed",
        }
    }
}

/// What is recorded for `p`: its kind, its command and its environment, as DECLARED.
pub fn fingerprint(p: &crate::profiles::Profile) -> Fingerprint {
    Fingerprint {
        kind: p.kind.clone(),
        command: p.command.clone(),
        env: p.env.iter().cloned().collect(),
    }
}

/// Whether `p` must be shown and approved before its command runs, and why.
///
/// **A built-in never asks.** Its command comes out of charter's own registry rather than
/// out of a file, so what a chat could have written decides nothing about it — and a
/// question that never carries risk is one an operator learns to answer yes to without
/// reading. A profile the file DECLARES with a built-in's name (`[harness.claude]`) is a
/// declaration and asks with the rest: the name is the built-in's, the command is the file's.
///
/// **And a profile on a project's harness declaration asks for the declaration** (ADR 0073
/// §5, V24b): the program it names and the words charter adds to it come out of a committed
/// file, so the operator approves the declaration on this machine once, and again after any
/// change to it ([`declaration_approval_needed`]). The profile `harnesses/<name>.toml` gives
/// has nothing else to approve; a local profile of that kind asks for both.
pub fn approval_needed(root: &Path, p: &crate::profiles::Profile) -> Option<Approval> {
    profile_approval_needed(root, p)
        .or_else(|| declaration_approval_needed(root, p).map(|(why, _)| why))
}

/// Whether `p`'s own command, as `charter.local.toml` declares it, must be approved.
pub fn profile_approval_needed(root: &Path, p: &crate::profiles::Profile) -> Option<Approval> {
    if p.source != crate::profiles::Source::Local {
        return None;
    }
    match last_launched(root, &p.name) {
        None => Some(Approval::New),
        Some(was) if was == fingerprint(p) => None,
        Some(_) => Some(Approval::Changed),
    }
}

/// Whether the project's harness declaration `p` runs on must be approved, and which one —
/// `None` for a built-in kind, which never asks. Reads the declarations now; a launch asks
/// [`declaration_approval_needed_in`] of the ones it runs.
pub fn declaration_approval_needed(
    root: &Path,
    p: &crate::profiles::Profile,
) -> Option<(Approval, crate::harness_declaration::Declaration)> {
    declaration_approval_needed_in(root, p, &crate::harness_declaration::read(root))
}

/// [`declaration_approval_needed`] of the declarations as `declared` holds them: approved
/// only where the record holds exactly that declaration's digest, all of it.
pub fn declaration_approval_needed_in(
    root: &Path,
    p: &crate::profiles::Profile,
    declared: &crate::harness_declaration::Declarations,
) -> Option<(Approval, crate::harness_declaration::Declaration)> {
    let d = declaration_of(p, declared)?.clone();
    let was = read_record(root, crate::harness_declaration::APPROVED)
        .get(&d.name)
        .and_then(|entry| entry.get("digest"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    match was {
        None => Some((Approval::New, d)),
        Some(was) if was == d.digest => None,
        Some(_) => Some((Approval::Changed, d)),
    }
}

/// The project's declaration `p` runs on, or none for a built-in kind.
fn declaration_of<'a>(
    p: &crate::profiles::Profile,
    declared: &'a crate::harness_declaration::Declarations,
) -> Option<&'a crate::harness_declaration::Declaration> {
    if p.source == crate::profiles::Source::BuiltIn {
        return None;
    }
    declared.projects().find(|d| d.name == p.kind)
}

/// Records the operator's yes to `p`, **only if `shown` is what charter would show for it
/// now** ([`shown`]): its own command where `charter.local.toml` declares it, and the
/// project's harness declaration it runs on, by the digest of the very read that `shown` was
/// checked against. A declaration changed between the dialog and the click is not approved,
/// and nothing is recorded.
///
/// The refusal is a sentence the window shows as it is.
pub fn approve(root: &Path, p: &crate::profiles::Profile, shown: &str) -> Result<(), String> {
    let declared = crate::harness_declaration::read(root);
    let now = shown_in(root, p, &declared);
    if now != shown {
        return Err(format!(
            "profile '{}' changed while you were reading it, so nothing was approved and \
             nothing was started. It now runs: {now}",
            crate::shown::short(&p.name)
        ));
    }
    let could_not = |err: io::Error| {
        format!(
            "purlis could not record that approval ({err}), so it will not start the profile \
             — it would only ask again."
        )
    };
    if p.source == crate::profiles::Source::Local {
        record_launched(root, &p.name, &fingerprint(p)).map_err(could_not)?;
    }
    if let Some(d) = declaration_of(p, &declared) {
        record(
            root,
            crate::harness_declaration::APPROVED,
            &d.name,
            serde_json::json!({ "digest": d.digest }),
        )
        .map_err(could_not)?;
    }
    Ok(())
}

/// What the approval line says for a declaration template with no words: in parentheses, which
/// [`crate::profiles::quote`] always quotes, so a template whose one word is `nothing` (drawn
/// `nothing`) never reads as an empty one (#1014).
pub const NOTHING: &str = "(nothing)";

/// What the approval line says for a declaration with no resume template, in the same form as
/// [`NOTHING`] and for the same reason.
pub const CANNOT_RESUME: &str = "(nothing; it cannot resume)";

/// What the approval dialog shows for `p`, and what [`approve`] checks the click against: the
/// profile's whole line, never clipped at the display limit
/// ([`crate::profiles::display_whole`], #1014), and for a profile on a project's harness
/// declaration **every word that will run** (ruling V66) — the declaration's file and whole
/// digest, its program, and each session template's words — each quoted as a shell would need
/// it typed, so one word with a space never reads as two, and escaped.
///
/// **The kind is on the line too**, as `(kind claude)` after the command and first inside the
/// declaration's parentheses. The approval records it ([`fingerprint`]) and it chooses the
/// harness — the words purlis adds, the sandbox, the guard — so two profiles that differ only
/// in kind never share a line. Parenthesised, a form [`crate::profiles::quote`] always wraps,
/// so no word of the command can read as it.
pub fn shown(root: &Path, p: &crate::profiles::Profile) -> String {
    shown_in(root, p, &crate::harness_declaration::read(root))
}

/// [`shown`] of the declarations as `declared` holds them.
///
/// Beside the words, a warning for each word or flag value that names a file or folder at
/// the project root ([`crate::harness_declaration::names_in_project`]).
pub fn shown_in(
    root: &Path,
    p: &crate::profiles::Profile,
    declared: &crate::harness_declaration::Declarations,
) -> String {
    let line = crate::profiles::display_whole(p);
    let kind = crate::shown::readable(&crate::profiles::quote(&p.kind), usize::MAX);
    let Some(d) = declaration_of(p, declared) else {
        return format!("{line} (kind {kind})");
    };
    let words = |words: &[String]| -> String {
        if words.is_empty() {
            return NOTHING.to_owned();
        }
        words
            .iter()
            .map(|word| crate::shown::readable(&crate::profiles::quote(word), usize::MAX))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut said = format!(
        "{line} (kind {kind}, declared in {}, {}; program {}; a new chat adds {}; a resumed \
         chat adds {}",
        d.file,
        d.digest,
        crate::shown::readable(&crate::profiles::quote(&d.program), usize::MAX),
        words(&d.session.new),
        d.session
            .resume
            .as_deref()
            .map_or_else(|| CANNOT_RESUME.to_owned(), words),
    );
    if let Some(acp) = &d.levels.acp {
        said.push_str(&format!("; its ACP agent runs {}", words(acp)));
    }
    let after_program = d
        .session
        .new
        .iter()
        .chain(d.session.resume.iter().flatten())
        .chain(d.levels.acp.iter().flat_map(|acp| acp.iter().skip(1)));
    let mut warned = Vec::new();
    for warning in
        after_program.filter_map(|word| crate::harness_declaration::names_in_project(root, word))
    {
        if !warned.contains(&warning) {
            warned.push(warning);
        }
    }
    for warning in warned {
        said.push_str(&format!("; warning: {warning}"));
    }
    said.push(')');
    said
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    use std::os::unix::fs::PermissionsExt;

    /// A plane with its state directory already made, as [`record_launched`] leaves it.
    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        private_dir(&dir.path().join(".charter")).unwrap();
        dir
    }

    fn print() -> Fingerprint {
        Fingerprint {
            kind: "claude".into(),
            command: vec!["claude".into()],
            env: BTreeMap::new(),
        }
    }

    /// A link planted AT the record is refused, never followed: consent to run a command
    /// lands in the record or nowhere. (The gates on the temp file and on a swapped-in state
    /// directory are `rewrite`'s, and tested there.)
    #[test]
    fn a_link_at_the_record_itself_is_refused_and_not_written_through() {
        let plane = plane();
        let captured = plane.path().join("captured.json");
        std::os::unix::fs::symlink(&captured, path(plane.path())).unwrap();

        let refused = record_launched(plane.path(), "work", &print());

        assert!(refused.is_err(), "a linked record was replaced or followed");
        assert!(
            !captured.exists(),
            "the record was written through the link"
        );
    }

    /// The mode the record ends at: it holds what the operator approved.
    #[test]
    fn the_record_is_private_to_the_operator() {
        let plane = plane();

        record_launched(plane.path(), "work", &print()).expect("it records");

        let mode = std::fs::metadata(path(plane.path()))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "the record is readable by somebody else"
        );
    }

    /// End to end: what [`record_launched`] wrote is what [`last_launched`] reads back.
    #[test]
    fn what_was_recorded_as_launched_is_what_is_read_back() {
        let plane = plane();
        let print = Fingerprint {
            kind: "claude".into(),
            command: vec!["claude".into()],
            env: BTreeMap::from([(
                "CLAUDE_CONFIG_DIR".to_string(),
                "~/.claude-work".to_string(),
            )]),
        };

        record_launched(plane.path(), "work", &print).expect("it records");

        assert_eq!(last_launched(plane.path(), "work"), Some(print));
    }
}
