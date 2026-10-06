//! The recorded files: one scenario per line, and the larger file contents they name by hash.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::tree::decode;

pub fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

pub fn scenarios_path() -> PathBuf {
    dir().join("recorded/behaviour.jsonl")
}

fn blobs_path() -> PathBuf {
    dir().join("recorded/behaviour-blobs.jsonl.gz")
}

pub fn planes() -> PathBuf {
    dir().join("planes")
}

/// Every scenario, in file order.
pub fn scenarios() -> Vec<Value> {
    let path = scenarios_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("{}:{}: {e}", path.display(), i + 1))
        })
        .collect()
}

/// Write the scenarios back, in the recorder's own serialisation: compact, keys sorted, not
/// ASCII-escaped — so a file nobody changed is written back byte for byte.
pub fn write_scenarios(rows: &[Value]) {
    let mut out = String::new();
    for row in rows {
        out.push_str(&serde_json::to_string(&sorted(row)).expect("a row serialises"));
        out.push('\n');
    }
    std::fs::write(scenarios_path(), out).expect("write the scenarios");
}

/// `value` with every object's keys in order, whatever order the map type keeps.
fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), sorted(&map[k]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// File contents too big or too binary to sit inline in a scenario, by sha256.
#[derive(Default)]
pub struct Blobs {
    by_sha: BTreeMap<String, Vec<u8>>,
    changed: bool,
}

impl Blobs {
    pub fn load() -> Blobs {
        let path = blobs_path();
        let file = std::fs::File::open(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let mut text = String::new();
        flate2::read::GzDecoder::new(file)
            .read_to_string(&mut text)
            .expect("the blobs decompress");
        let mut by_sha = BTreeMap::new();
        for line in text.lines() {
            let row: Value = serde_json::from_str(line).expect("a blob row is JSON");
            let sha = row["sha256"].as_str().expect("a sha").to_owned();
            let bytes = match row["text"].as_str() {
                Some(t) => t.as_bytes().to_vec(),
                None => decode(row["b64"].as_str().expect("a blob's body")),
            };
            by_sha.insert(sha, bytes);
        }
        Blobs {
            by_sha,
            changed: false,
        }
    }

    pub fn get(&self, sha: &str) -> Vec<u8> {
        self.by_sha
            .get(sha)
            .unwrap_or_else(|| panic!("no blob {sha}"))
            .clone()
    }

    pub fn put(&mut self, bytes: &[u8]) -> String {
        let sha: String = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if !self.by_sha.contains_key(&sha) {
            self.by_sha.insert(sha.clone(), bytes.to_vec());
            self.changed = true;
        }
        sha
    }

    /// Write the store back if a bless added to it, keeping only what `rows` still name.
    pub fn save_if_changed(&self, rows: &[Value]) {
        if !self.changed {
            return;
        }
        let mut named = std::collections::BTreeSet::new();
        collect(rows, &mut named);
        let mut text = String::new();
        for (sha, bytes) in &self.by_sha {
            if !named.contains(sha) {
                continue;
            }
            let row = match std::str::from_utf8(bytes) {
                Ok(t) => json!({"sha256": sha, "text": t}),
                Err(_) => {
                    use base64::Engine as _;
                    json!({"sha256": sha, "b64": base64::engine::general_purpose::STANDARD.encode(bytes)})
                }
            };
            text.push_str(&serde_json::to_string(&row).expect("serialises"));
            text.push('\n');
        }
        let file = std::fs::File::create(blobs_path()).expect("write the blobs");
        let mut gz = flate2::GzBuilder::new().write(file, flate2::Compression::best());
        gz.write_all(text.as_bytes()).expect("compress");
        gz.finish().expect("finish");
    }
}

fn collect(rows: &[Value], named: &mut std::collections::BTreeSet<String>) {
    fn walk(v: &Value, named: &mut std::collections::BTreeSet<String>) {
        match v {
            Value::Object(map) => {
                if let Some(Value::String(sha)) = map.get("blob") {
                    named.insert(sha.clone());
                }
                map.values().for_each(|v| walk(v, named));
            }
            Value::Array(items) => items.iter().for_each(|v| walk(v, named)),
            _ => {}
        }
    }
    rows.iter().for_each(|r| walk(r, named));
}

/// Give every entry under `root`, and `root`, the one time `at`.
///
/// The recording laid its plane copies out this way (`frozen-record.py`'s
/// `lay_out_stamped`): the checkout's own times decide which workspace a briefing calls most
/// recent, and they are whatever order git wrote the files in on that machine. One time for
/// all makes the order the implementation's own tie-break, and whatever the scenario's setup
/// lays down afterwards is strictly newer.
pub fn stamp(root: &Path, at: std::time::SystemTime) {
    let times = std::fs::FileTimes::new().set_accessed(at).set_modified(at);
    if let Ok(read) = std::fs::read_dir(root) {
        for entry in read.filter_map(Result::ok) {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                stamp(&path, at);
            } else if entry.file_type().is_ok_and(|t| t.is_file()) {
                std::fs::File::options()
                    .write(true)
                    .open(&path)
                    .and_then(|f| f.set_times(times))
                    .expect("a file's time");
            }
        }
    }
    std::fs::File::open(root)
        .and_then(|f| f.set_times(times))
        .expect("a directory's time");
}

/// Copy a fixture plane, modes and links included, the way `shutil.copytree` did.
pub fn copy_plane(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("the plane's directory");
    for entry in std::fs::read_dir(from).expect("the fixture plane reads") {
        let entry = entry.expect("an entry");
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let kind = entry.file_type().expect("a type");
        if kind.is_symlink() {
            std::os::unix::fs::symlink(std::fs::read_link(&src).expect("a link"), &dst)
                .expect("copy a link");
        } else if kind.is_dir() {
            copy_plane(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).expect("copy a file");
        }
    }
}
