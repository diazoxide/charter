//! `workspaces/<ws>/workspace.json` — the committed workspace manifest.
//!
//! The file carries a digest taken over the rest of the document, under
//! [`crate::names::GENERATED_KEY`]: `charter_generated` until the plane is migrated (the file is
//! committed, and a teammate on an older build reads only that key), `purlis_generated` after,
//! when a stamp renames the old key in place (V93g, V93i). Either key is read. It is
//! how charter tells its own writes from a hand's: a document whose digest matches is
//! charter's to maintain, and anything else it leaves byte for byte alone. A Rust writer
//! that computed the digest differently would present every file as hand-edited, and
//! charter's automatic writers would quietly stop touching them.

use crate::pyjson;

use crate::names::GENERATED_KEY;

/// The key the digest is written under in the plane at `plane`. Every spelling of
/// [`GENERATED_KEY`] is read, and none is taken into the digest.
pub fn key_for(plane: &std::path::Path) -> &'static str {
    GENERATED_KEY.writes_for(plane)
}

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

/// The digest charter stores under [`key_for`]: sha256 over the document without that
/// key, serialised as `json.dumps(body, sort_keys=True)`.
pub fn digest(doc: &serde_json::Value) -> String {
    use sha2::Digest as _;

    let body: serde_json::Value = match doc {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .filter(|(k, _)| !GENERATED_KEY.recognises(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        other => other.clone(),
    };
    let mut hasher = sha2::Sha256::new();
    hasher.update(pyjson::dumps_sorted(&body).as_bytes());
    crate::extension::hex(&hasher.finalize())
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
    // The newest spelling present is the stamp (V93e): an old key beside a purlis one is a
    // leftover, never a second chance to match.
    let stored = GENERATED_KEY
        .spellings()
        .find_map(|key| doc.get(key))
        .and_then(|v| v.as_str());
    match stored {
        Some(stored) if stored == digest(&doc) => Ownership::Charter,
        _ => Ownership::Operator,
    }
}

/// Stamp `doc` with its digest under `key` (one of [`GENERATED_KEY`]'s spellings, the plane's
/// [`key_for`]) when `stamped`, and leave it alone otherwise.
///
/// The stamp takes the place of the first spelling of the key already there, so a document
/// charter wrote keeps its key order, one a hand wrote keeps the position it chose (Python's
/// `dict` assignment), and one stamped before the rename has `charter_generated` renamed in
/// place rather than a second key added once the plane is migrated. Any other spelling is dropped. With no key there, the
/// stamp goes last.
pub fn stamp(doc: &mut serde_json::Value, stamped: bool, key: &str) {
    if !stamped {
        return;
    }
    let digest = serde_json::Value::String(digest(doc));
    let Some(map) = doc.as_object_mut() else {
        return;
    };
    let mut out = serde_json::Map::with_capacity(map.len() + 1);
    let mut placed = false;
    for (name, value) in std::mem::take(map) {
        if GENERATED_KEY.recognises(&name) {
            if !placed {
                out.insert(key.to_owned(), digest.clone());
                placed = true;
            }
        } else {
            out.insert(name, value);
        }
    }
    if !placed {
        out.insert(key.to_owned(), digest);
    }
    *map = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stamp is SHA-256 over `json.dumps(body, sort_keys=True)` without the stamp itself,
    /// pinned to Python's `hashlib` over `'{"a": "x", "b": 1}'`: a manifest stamped before a
    /// dependency bump must still read as charter's after it.
    #[test]
    fn a_stamp_is_the_known_sha256_of_the_sorted_body() {
        let doc = serde_json::json!({"b": 1, "a": "x", "charter_generated": "stale"});
        assert_eq!(
            digest(&doc),
            "385820f0096fd558f4091319e7fa742cebf877dc3baca180981889f1c40eca84"
        );
    }

    #[test]
    fn a_stamp_that_no_longer_matches_the_body_makes_the_manifest_the_operators() {
        // `charter/workspace.py:manifest_owner`: "charter" only when the stored digest equals
        // the digest of the body as it is NOW. A hand edit after charter's write leaves the old
        // stamp in place, and that file is the operator's — the automatic writers leave it be.
        let mut doc = serde_json::json!({"name": "alpha", "repos": ["widget"]});
        doc["charter_generated"] = serde_json::Value::String(digest(&doc));
        assert_eq!(ownership(Some(&doc.to_string())), Ownership::Charter);

        doc["repos"] = serde_json::json!(["widget", "gadget"]);
        assert_eq!(ownership(Some(&doc.to_string())), Ownership::Operator);
    }

    // ---- the rename (V93i): the digest is recognised under both keys, and moves ---------- //

    /// A manifest charter stamped before the rename: the key is `charter_generated`, and the
    /// digest is the known one over the body without it.
    fn stamped_before_the_rename() -> String {
        r#"{"name": "alpha", "charter_generated": "STAMP", "repos": []}"#.replace(
            "STAMP",
            &digest(&serde_json::json!({"name": "alpha", "repos": []})),
        )
    }

    #[test]
    fn a_manifest_stamped_under_the_old_key_is_still_charters() {
        assert_eq!(
            ownership(Some(&stamped_before_the_rename())),
            Ownership::Charter
        );
    }

    #[test]
    fn a_manifest_stamped_under_the_purlis_key_is_charters() {
        let mut doc = serde_json::json!({"name": "alpha", "repos": []});
        let stamp = digest(&doc);
        doc["purlis_generated"] = serde_json::Value::String(stamp);
        assert_eq!(ownership(Some(&doc.to_string())), Ownership::Charter);

        doc["repos"] = serde_json::json!(["widget"]);
        assert_eq!(ownership(Some(&doc.to_string())), Ownership::Operator);
    }

    #[test]
    fn stamping_moves_the_old_key_to_the_purlis_key_in_place_and_ownership_carries_over() {
        let old: serde_json::Value = serde_json::from_str(&stamped_before_the_rename()).unwrap();
        let mut doc = old.clone();
        stamp(&mut doc, true, GENERATED_KEY.write);
        let keys: Vec<&str> = doc
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["name", "purlis_generated", "repos"], "{doc}");
        assert_eq!(doc["purlis_generated"], old["charter_generated"]);
        assert_eq!(ownership(Some(&doc.to_string())), Ownership::Charter);
    }

    #[test]
    fn with_both_keys_the_purlis_one_is_read_and_stamping_leaves_one() {
        let body = serde_json::json!({"name": "alpha"});
        let good = digest(&body);
        // The purlis key wins: a stale old key beside a matching purlis one is still charter's…
        let both = format!(
            r#"{{"name":"alpha","charter_generated":"stale","purlis_generated":"{good}"}}"#
        );
        assert_eq!(ownership(Some(&both)), Ownership::Charter);
        // …and a matching old key beside a stale purlis one is not.
        let both = format!(
            r#"{{"name":"alpha","charter_generated":"{good}","purlis_generated":"stale"}}"#
        );
        assert_eq!(ownership(Some(&both)), Ownership::Operator);

        let mut doc: serde_json::Value = serde_json::from_str(&both).unwrap();
        stamp(&mut doc, true, GENERATED_KEY.write);
        let keys: Vec<&str> = doc
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["name", "purlis_generated"], "{doc}");
        assert_eq!(ownership(Some(&doc.to_string())), Ownership::Charter);
    }

    #[test]
    fn a_plane_not_yet_migrated_stamps_under_charters_key_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("charter.toml"), "").unwrap();
        let text = stamped_before_the_rename();
        let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
        stamp(&mut doc, true, key_for(tmp.path()));
        assert_eq!(
            doc,
            serde_json::from_str::<serde_json::Value>(&text).unwrap()
        );

        // A fresh manifest there is stamped under charter's key too.
        let mut fresh = serde_json::json!({"name": "beta"});
        stamp(&mut fresh, true, key_for(tmp.path()));
        assert!(fresh.get("charter_generated").is_some(), "{fresh}");
        assert!(fresh.get("purlis_generated").is_none(), "{fresh}");
        assert_eq!(ownership(Some(&fresh.to_string())), Ownership::Charter);

        std::fs::write(tmp.path().join("purlis.toml"), "").unwrap();
        assert_eq!(key_for(tmp.path()), "purlis_generated");
    }

    #[test]
    fn an_unstamped_write_keeps_a_hands_document_as_it_is() {
        let mut doc = serde_json::json!({"name": "alpha", "charter_generated": "mine"});
        let before = doc.clone();
        stamp(&mut doc, false, GENERATED_KEY.write);
        assert_eq!(doc, before);
    }
}
