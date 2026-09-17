//! `workspaces/<ws>/workspace.json` — the committed workspace manifest.
//!
//! The file carries a `charter_generated` digest taken over the rest of the document. It is
//! how charter tells its own writes from a hand's: a document whose digest matches is
//! charter's to maintain, and anything else it leaves byte for byte alone. A Rust writer
//! that computed the digest differently would present every file as hand-edited, and
//! charter's automatic writers would quietly stop touching them.

use crate::pyjson;

/// The key holding the digest, and the one key the digest is not taken over.
pub const KEY: &str = "charter_generated";

/// Who last wrote a manifest, as charter decides it.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Ownership {
    /// The digest matches the body: charter wrote it and may rewrite it.
    Charter,
    /// Present but the digest does not match, or the file does not parse: a hand wrote it.
    Operator,
    /// No manifest at all.
    Absent,
}

/// The digest charter stores in `charter_generated`: sha256 over the document without that
/// key, serialised as `json.dumps(body, sort_keys=True)`.
pub fn digest(doc: &serde_json::Value) -> String {
    use sha2::Digest as _;

    let body: serde_json::Value = match doc {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .filter(|(k, _)| k.as_str() != KEY)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        other => other.clone(),
    };
    let mut hasher = sha2::Sha256::new();
    hasher.update(pyjson::dumps_sorted(&body).as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Who owns the manifest whose text this is, or `Absent` for no file.
pub fn ownership(text: Option<&str>) -> Ownership {
    let Some(text) = text else {
        return Ownership::Absent;
    };
    // A present-but-unparseable file is the operator's: charter will not overwrite what it
    // cannot read.
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(text) else {
        return Ownership::Operator;
    };
    match doc.get(KEY).and_then(|v| v.as_str()) {
        Some(stored) if stored == digest(&doc) => Ownership::Charter,
        _ => Ownership::Operator,
    }
}
