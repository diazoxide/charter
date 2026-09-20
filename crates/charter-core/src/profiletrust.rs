//! `.charter/harness-profiles-launched.json` — which harness profiles the operator has
//! approved running.
//!
//! This file decides whether a command runs, so every unreadable state means **ask again**.
//! A missing file, a malformed one, an entry that is not a fingerprint, a link, a FIFO, a
//! planted giant: each reads as "no record", never as approval. Treating silence as a yes is
//! the one state this record exists to keep out.
//!
//! **And it is not a boundary**, which `charter/profiletrust.py` says at full volume and this
//! module did not. A chat that can edit `charter.local.toml` can edit this record too — both
//! sit under paths no guard covers — so the ask catches a command the operator did not change
//! themselves *unless whatever changed it also forged the record*. That is worth having: it
//! closes the accident and the careless edit, and it is the difference between a command that
//! ran unseen and one that was read out loud first. It is not a defence against an agent that
//! set out to forge it, and nothing here should be built as though it were.

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

fn path(root: &Path) -> PathBuf {
    root.join(".charter").join(RECORD)
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
    let nothing = || serde_json::Value::Object(serde_json::Map::new());
    let Ok(mut open) = crate::contain::open_no_link(root, &path(root)) else {
        return nothing();
    };
    // `fstat` of the descriptor the read will use, not of the name: the two cannot be
    // handed different files.
    let Ok(found) = open.metadata() else {
        return nothing();
    };
    if found.len() > MAX_BYTES {
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
    // This file records which commands the operator approved RUNNING, so a link that moves
    // it out of the plane moves the consent with it. `contain::writable`'s data roots do not
    // cover it — `.charter` is not one, and Python does not gate this write either — so the
    // rule is spelled out here: the record stays under this plane's own `.charter`.
    // The record itself, not only the directory holding it. Gating `.charter` left this
    // safe because `private_dir` refuses a symlinked state directory and the write ends in
    // a `rename` — true, and one level shallower than the thing opened.
    let state = root.join(".charter");
    if !crate::contain::within_plane(root, &path(root))
        || !crate::contain::within_plane(root, &state)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "'{}' resolves outside the plane — charter will not record consent to run a \
                 command through a link that leaves it",
                state.display()
            ),
        ));
    }
    let mut doc = read(root);
    // One object keyed by profile name, read and rewritten whole, so approving one profile
    // today does not make another ask again tomorrow. An existing key keeps its position.
    if let Some(map) = doc.as_object_mut() {
        map.insert(name.to_string(), print.to_json());
    }
    let dir = path(root)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.to_path_buf());
    private_dir(&dir)?;
    write_private(&path(root), crate::pyjson::dumps_indent2(&doc).as_bytes())
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
                    "'{}' is a symlink; charter will not follow one here",
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

/// Write the record atomically, 0600: it records the operator's consent to run a command.
///
/// The mode is set on the TEMP file, because a rename carries the source's mode onto the
/// target rather than the other way round.
pub(crate) fn write_private(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = target.parent().unwrap_or(Path::new("."));
    let name = target
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    // pid AND a per-call tag, as charter's `config.temp_beside` does: the pid separates two
    // processes, the tail separates two writers inside one — Tauri runs commands on a thread
    // pool — and a pid the kernel has recycled.
    let temp = dir.join(format!(
        "{name}.{}.{}.tmp",
        std::process::id(),
        crate::workspaces::scratch_tag()
    ));

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = options
        .open(&temp)
        .and_then(|mut f| std::io::Write::write_all(&mut f, bytes))
        .and_then(|()| std::fs::rename(&temp, target));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
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
pub fn approval_needed(root: &Path, p: &crate::profiles::Profile) -> Option<Approval> {
    if p.source == crate::profiles::Source::BuiltIn {
        return None;
    }
    match last_launched(root, &p.name) {
        None => Some(Approval::New),
        Some(was) if was == fingerprint(p) => None,
        Some(_) => Some(Approval::Changed),
    }
}
