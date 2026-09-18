//! `.charter/harness-profiles-launched.json` — which harness profiles the operator has
//! approved running.
//!
//! This file decides whether a command runs, so every unreadable state means **ask again**.
//! A missing file, a malformed one, an entry that is not a fingerprint: each reads as "no
//! record", never as approval. Treating silence as a yes is the one state this record exists
//! to keep out.

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

/// Every recorded profile, or an empty document.
pub fn read(root: &Path) -> serde_json::Value {
    // Unreadable, malformed and missing all read the same: an empty document, so everything
    // declared asks again.
    std::fs::read_to_string(path(root))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()))
}

/// What `name` was last approved as, or `None` when there is no record of it.
pub fn last_launched(root: &Path, name: &str) -> Option<Fingerprint> {
    Fingerprint::from_json(read(root).get(name)?)
}

/// Write `name` down as launched, keeping every other profile's record.
pub fn record_launched(root: &Path, name: &str, print: &Fingerprint) -> io::Result<()> {
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
fn private_dir(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
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
fn write_private(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = target.parent().unwrap_or(Path::new("."));
    let name = target
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let temp = dir.join(format!("{name}.{}.tmp", std::process::id()));

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
