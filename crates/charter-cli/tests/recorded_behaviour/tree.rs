//! A side's tree as data: what the fixture says to lay down, and what a run left behind.

use std::collections::BTreeMap;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde_json::{Map, Value, json};

use crate::fixture::Blobs;
use crate::text::Tokens;

/// One entry of a tree, with only what the comparison reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// A directory. Its mode is laid down when the fixture says so, and never compared — the
    /// differential compared file modes only.
    Dir {
        mode: Option<u32>,
    },
    File {
        mode: u32,
        bytes: Vec<u8>,
    },
    Symlink(String),
    Fifo {
        mode: u32,
    },
    Other,
}

impl Node {
    /// The node as the tree comparison sees it: a directory is a directory.
    pub fn compared(&self) -> Node {
        match self {
            Node::Dir { .. } => Node::Dir { mode: None },
            other => other.clone(),
        }
    }

    /// A fixture entry: `{"kind": …}` with the contents inline, as base64, or by blob.
    pub fn from_json(value: &Value, blobs: &Blobs) -> Node {
        let mode = |v: &Value| v["mode"].as_u64().map(|m| m as u32);
        match value["kind"].as_str() {
            Some("dir") => Node::Dir { mode: mode(value) },
            Some("file") => {
                let bytes = if let Some(text) = value["text"].as_str() {
                    text.as_bytes().to_vec()
                } else if let Some(b64) = value["b64"].as_str() {
                    decode(b64)
                } else if let Some(sha) = value["blob"].as_str() {
                    blobs.get(sha)
                } else {
                    panic!("a file entry with no contents: {value}")
                };
                Node::File {
                    mode: mode(value).expect("a file's mode"),
                    bytes,
                }
            }
            Some("symlink") => Node::Symlink(
                value["target"]
                    .as_str()
                    .expect("a symlink's target")
                    .to_owned(),
            ),
            Some("fifo") => Node::Fifo {
                mode: mode(value).expect("a fifo's mode"),
            },
            _ => Node::Other,
        }
    }

    /// The node as a fixture entry — the recorder's own shape, so a blessed file is the file
    /// the recorder would have written.
    pub fn to_json(&self, blobs: &mut Blobs) -> Value {
        match self {
            Node::Dir { mode } => match mode {
                Some(m) => json!({"kind": "dir", "mode": m}),
                None => json!({"kind": "dir"}),
            },
            Node::File { mode, bytes } => {
                let mut entry = Map::new();
                entry.insert("kind".into(), json!("file"));
                entry.insert("mode".into(), json!(mode));
                match std::str::from_utf8(bytes) {
                    Ok(text) if text.len() <= 2048 => {
                        entry.insert("text".into(), json!(text));
                    }
                    _ => {
                        entry.insert("blob".into(), json!(blobs.put(bytes)));
                    }
                }
                Value::Object(entry)
            }
            Node::Symlink(target) => json!({"kind": "symlink", "target": target}),
            Node::Fifo { mode } => json!({"kind": "fifo", "mode": mode}),
            Node::Other => json!({"kind": "other"}),
        }
    }
}

pub fn decode(b64: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("valid base64 in the fixture")
}

/// Paths relative to a root, to what is there.
pub type Tree = BTreeMap<String, Node>;

/// Every entry under `root`, by path relative to it, with the side's own directory in any
/// contents or link target spelled `<side>`.
///
/// The recorder's `snapshot`, rule for rule: `pins/` is not the Rust side's; git's sample
/// hooks are inert and were never recorded; a directory the run may not list is recorded and
/// not entered.
pub fn snapshot(root: &Path, tokens: &Tokens) -> Tree {
    let mut out = Tree::new();
    walk(root, root, tokens, &mut out);
    out
}

fn walk(root: &Path, dir: &Path, tokens: &Tokens, out: &mut Tree) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<_> = read.filter_map(Result::ok).map(|e| e.file_name()).collect();
    names.sort();
    for name in names {
        let path = dir.join(&name);
        let rel = path
            .strip_prefix(root)
            .expect("under the root")
            .to_string_lossy()
            .into_owned();
        if rel == "pins" || rel.starts_with("pins/") {
            continue;
        }
        let in_hooks = dir.file_name().is_some_and(|n| n == "hooks")
            && dir
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|n| n.to_string_lossy().ends_with(".git"));
        if in_hooks && name.to_string_lossy().ends_with(".sample") {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        let mode = meta.mode() & 0o7777;
        let kind = meta.file_type();
        let node = if kind.is_symlink() {
            let target = std::fs::read_link(&path).expect("a link reads");
            Node::Symlink(tokens.paths(&target.to_string_lossy()))
        } else if kind.is_dir() {
            if mode & 0o500 == 0o500 {
                walk(root, &path, tokens, out);
            }
            Node::Dir { mode: Some(mode) }
        } else if kind.is_file() {
            let bytes = std::fs::read(&path).unwrap_or_default();
            Node::File {
                mode,
                bytes: tokens.bytes(&bytes),
            }
        } else if is_fifo(&meta) {
            Node::Fifo { mode }
        } else {
            Node::Other
        };
        out.insert(rel, node);
    }
}

fn is_fifo(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::FileTypeExt;
    meta.file_type().is_fifo()
}

/// `delta.set` and `delta.remove` applied to `base`.
pub fn applied(base: &Tree, delta: &Value, blobs: &Blobs) -> Tree {
    let mut out = base.clone();
    for rel in delta["remove"].as_array().into_iter().flatten() {
        out.remove(rel.as_str().expect("a path"));
    }
    for (rel, entry) in delta["set"].as_object().into_iter().flatten() {
        out.insert(rel.clone(), Node::from_json(entry, blobs));
    }
    out
}

/// What turns `before` into `after`, in the fixture's shape.
pub fn delta(before: &Tree, after: &Tree, blobs: &mut Blobs) -> Value {
    let mut set = Map::new();
    for (rel, node) in after {
        if before.get(rel) != Some(node) {
            set.insert(rel.clone(), node.to_json(blobs));
        }
    }
    let remove: Vec<Value> = before
        .keys()
        .filter(|rel| !after.contains_key(*rel))
        .map(|rel| json!(rel))
        .collect();
    json!({"remove": remove, "set": set})
}

/// Lay a delta down on disk under `side`: removals, then every entry in path order. The
/// directory modes are returned rather than set, for [`close`] to apply once the caller has
/// looked at the tree — a directory closed to its owner cannot be read afterwards.
pub fn lay_down(side: &Path, delta: &Value, blobs: &Blobs, tokens: &Tokens) -> Vec<(PathBuf, u32)> {
    let mut removals: Vec<&str> = delta["remove"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| v.as_str().expect("a path"))
        .collect();
    removals.sort_by_key(|rel| std::cmp::Reverse(rel.len()));
    for rel in removals {
        let path = side.join(rel);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() => {
                let _ = std::fs::remove_dir_all(&path);
            }
            Ok(_) => {
                let _ = std::fs::remove_file(&path);
            }
            Err(_) => {}
        }
    }
    let mut modes: Vec<(PathBuf, u32)> = Vec::new();
    for (rel, entry) in delta["set"].as_object().into_iter().flatten() {
        let path = side.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("a parent directory");
        }
        let node = Node::from_json(entry, blobs);
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if meta.is_dir() && !matches!(node, Node::Dir { .. }) {
                std::fs::remove_dir_all(&path).expect("clear the way");
            } else if !meta.is_dir() {
                std::fs::remove_file(&path).expect("clear the way");
            }
        }
        match node {
            Node::Dir { mode } => {
                std::fs::create_dir_all(&path).expect("a directory");
                if let Some(mode) = mode {
                    modes.push((path, mode));
                }
            }
            Node::File { mode, bytes } => {
                std::fs::write(&path, tokens.unbytes(&bytes)).expect("a file");
                if let Some(secs) = entry["mtime"].as_u64() {
                    // A time the scenario's setup set on purpose: a crashed lock's age.
                    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
                    std::fs::File::options()
                        .write(true)
                        .open(&path)
                        .and_then(|f| f.set_modified(at))
                        .expect("a file's time");
                }
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                    .expect("a file's mode");
            }
            Node::Symlink(target) => {
                std::os::unix::fs::symlink(tokens.unpaths(&target), &path).expect("a link");
            }
            Node::Fifo { mode } => {
                let made = std::process::Command::new("mkfifo")
                    .arg("-m")
                    .arg(format!("{mode:o}"))
                    .arg(&path)
                    .status()
                    .expect("mkfifo runs");
                assert!(made.success(), "mkfifo {}", path.display());
            }
            Node::Other => panic!("the fixture asks for an entry of no kind at {rel}"),
        }
    }
    modes
}

/// Apply directory modes deepest first, so a directory closed to its owner is closed last.
pub fn close(mut modes: Vec<(PathBuf, u32)>) {
    modes.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
    for (path, mode) in modes {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
            .expect("a directory's mode");
    }
}

/// Open every directory under `root` to its owner again, so the scratch tree can be removed.
pub fn unseal(root: &Path) {
    let Ok(meta) = std::fs::symlink_metadata(root) else {
        return;
    };
    if !meta.is_dir() {
        return;
    }
    if meta.mode() & 0o700 != 0o700 {
        let _ = std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o755));
    }
    if let Ok(read) = std::fs::read_dir(root) {
        for entry in read.filter_map(Result::ok) {
            unseal(&entry.path());
        }
    }
}

/// `tree` without the entries under any of `ignore`, directories compared by kind alone.
pub fn compared(tree: &Tree, ignore: &[String]) -> Tree {
    tree.iter()
        .filter(|(rel, _)| !ignored(rel, ignore))
        .map(|(rel, node)| (rel.clone(), node.compared()))
        .collect()
}

pub fn ignored(rel: &str, ignore: &[String]) -> bool {
    ignore.iter().any(|k| {
        rel == k
            || rel
                .strip_prefix(k.as_str())
                .is_some_and(|r| r.starts_with('/'))
    })
}

/// The entries of `tree` under `prefix/`, re-rooted there.
pub fn under(tree: &Tree, prefix: &str) -> Tree {
    tree.iter()
        .filter_map(|(rel, node)| {
            rel.strip_prefix(prefix)
                .and_then(|r| r.strip_prefix('/'))
                .map(|r| (r.to_owned(), node.clone()))
        })
        .collect()
}

/// Every way two trees differ, as lines to print.
pub fn differences(want: &Tree, got: &Tree) -> Vec<String> {
    let mut out = Vec::new();
    for (rel, node) in want {
        match got.get(rel) {
            None => out.push(format!("    missing: {rel} ({})", describe(node))),
            Some(other) if other != node => out.push(format!(
                "    differs: {rel} — recorded {}, now {}",
                describe(node),
                describe(other)
            )),
            Some(_) => {}
        }
    }
    for (rel, node) in got {
        if !want.contains_key(rel) {
            out.push(format!("    unexpected: {rel} ({})", describe(node)));
        }
    }
    out
}

fn describe(node: &Node) -> String {
    match node {
        Node::Dir { .. } => "dir".into(),
        Node::File { mode, bytes } => match std::str::from_utf8(bytes) {
            Ok(text) if text.len() <= 400 => format!("file {mode:o} {text:?}"),
            _ => format!("file {mode:o}, {} bytes", bytes.len()),
        },
        Node::Symlink(target) => format!("symlink -> {target}"),
        Node::Fifo { mode } => format!("fifo {mode:o}"),
        Node::Other => "other".into(),
    }
}
