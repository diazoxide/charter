//! The recorded oracle corpora under `fixtures/corpora/`, read the one way every replay test
//! reads them.
//!
//! Each shell-reader and plane-root replay stands on TWO recordings: the curated rows (every
//! command line a Python docstring names as a bypass that shipped, plain JSON Lines) and a
//! frozen subset of the seeded fuzz (`*-generated.jsonl.gz`). Both were answered once by the
//! Python charter pinned at 50d31dc, on 2026-09-23, and are this app's contract from then on —
//! `fixtures/corpora/README.md` says how they were chosen and how to change an answer.
//!
//! A row comes back with WHERE it was recorded (`file:line`), so a failure names the row to
//! look at, not only the command, which a generated corpus can hold twice under different
//! probes.

use std::io::Read;
use std::path::PathBuf;

use serde_json::Value;

/// One recorded row and where it was recorded.
pub struct Row {
    /// `file:line`, 1-based, as an editor opens it.
    pub at: String,
    pub row: Value,
}

fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/corpora")
        .join(name)
}

fn rows(name: &str, text: &str) -> Vec<Row> {
    text.split('\n')
        .enumerate()
        .filter(|(_, l)| !l.is_empty())
        .map(|(i, l)| Row {
            at: format!("{name}:{}", i + 1),
            row: serde_json::from_str(l)
                .unwrap_or_else(|err| panic!("{name}:{}: not one JSON object: {err}", i + 1)),
        })
        .collect()
}

/// A plain `.jsonl` corpus.
///
/// Split on `\n` and nothing else: a recorded command can hold U+2028 and friends, which are
/// ordinary characters inside a JSON string and line breaks to `str::lines`' cousins elsewhere.
pub fn jsonl(name: &str) -> Vec<Row> {
    let p = path(name);
    let text = std::fs::read_to_string(&p)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", p.display()));
    rows(name, &text)
}

/// A gzipped `.jsonl.gz` corpus — the frozen fuzz subsets, which are an order of magnitude
/// smaller compressed and are only ever read whole.
pub fn jsonl_gz(name: &str) -> Vec<Row> {
    let p = path(name);
    let file =
        std::fs::File::open(&p).unwrap_or_else(|err| panic!("cannot open {}: {err}", p.display()));
    let mut text = String::new();
    flate2::read::GzDecoder::new(file)
        .read_to_string(&mut text)
        .unwrap_or_else(|err| panic!("cannot decompress {}: {err}", p.display()));
    rows(name, &text)
}

/// The shell reader's curated rows followed by its frozen fuzz subset.
///
/// Unused by the plane-root replay, which includes this module too.
#[allow(dead_code)]
pub fn shellseg() -> Vec<Row> {
    let mut all = jsonl("shellseg-oracle.jsonl");
    let generated = jsonl_gz("shellseg-generated.jsonl.gz");
    assert!(
        generated.len() >= 2000,
        "the frozen fuzz subset has {} rows — it was cut, not answered",
        generated.len()
    );
    all.extend(generated);
    all
}
