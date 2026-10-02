//! The landing log: `workspaces/<ws>/changes/log/<host>.jsonl` (ADR 0060 §2;
//! `docs/plane-format.md`, "the landing log").
//!
//! One line per landing charter made: *charter merged this commit, for this change*. It is
//! never committed, one file per machine, appended `O_APPEND` with no lock, and unioned by
//! `.gitattributes` should it ever be. The present tense is not stored: a member is landed when
//! the forge reports its request merged **and** the default branch still contains the commit
//! its line names. The forge alone cannot see a revert, and the log alone cannot see a browser
//! merge.
//!
//! A reader skips every line whose key set is not exactly [`LOG_FIELDS`], so a hand edit or a
//! half-written line costs only itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::store;
use crate::contain;

/// Every key a line carries, and nothing else: the Python charter's `LOG_FIELDS`, in the
/// sorted order the line is written in.
pub const LOG_FIELDS: [&str; 6] = ["change", "head", "merge", "number", "repo", "ts"];

/// The directory, relative to the change store.
pub const LOG_DIRNAME: &str = "log";

/// The last landing declared for each member, by repo.
pub type Landings = BTreeMap<String, Landing>;

/// One landing charter made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landing {
    /// When, as UTC ISO-8601 seconds.
    pub ts: String,
    /// The change's slug.
    pub change: String,
    /// The member that landed.
    pub repo: String,
    /// The request charter merged.
    pub number: u64,
    /// The commit the merge made on the target branch.
    pub merge: String,
    /// The member's head commit the checks passed on, and the merge was pinned to.
    pub head: String,
}

impl Landing {
    /// A line about `repo` of `change`, made at `when`.
    pub fn new(
        change: &str,
        repo: &str,
        number: u64,
        head: &str,
        merge: &str,
        when: DateTime<Utc>,
    ) -> Landing {
        Landing {
            ts: crate::dispatch::stamp(when),
            change: change.to_string(),
            repo: repo.to_string(),
            number,
            merge: merge.to_string(),
            head: head.to_string(),
        }
    }

    fn to_value(&self) -> Value {
        json!({
            "ts": self.ts,
            "change": self.change,
            "repo": self.repo,
            "number": self.number,
            "merge": self.merge,
            "head": self.head,
        })
    }

    /// A line read back, when it is one: an object with exactly [`LOG_FIELDS`], each of its
    /// type.
    fn of(value: &Value) -> Option<Landing> {
        let line = value.as_object()?;
        let mut keys: Vec<&str> = line.keys().map(String::as_str).collect();
        keys.sort_unstable();
        if keys != LOG_FIELDS {
            return None;
        }
        let text = |key: &str| line.get(key)?.as_str().map(str::to_string);
        Some(Landing {
            ts: text("ts")?,
            change: text("change")?,
            repo: text("repo")?,
            number: line.get("number")?.as_u64()?,
            merge: text("merge")?,
            head: text("head")?,
        })
    }
}

/// `workspaces/<ws>/changes/log/`.
pub fn log_dir(plane: &Path, ws: &str) -> PathBuf {
    store::dir(plane, ws).join(LOG_DIRNAME)
}

/// `workspaces/<ws>/changes/log/<host>.jsonl`: this machine's log.
pub fn log_path(plane: &Path, ws: &str, host: &str) -> PathBuf {
    log_dir(plane, ws).join(format!("{host}.jsonl"))
}

/// Append `landing` to `host`'s log: keys sorted, ASCII only, as the Python charter wrote it.
/// `None` when containment refuses the path or the disk does.
pub fn append(plane: &Path, ws: &str, host: &str, landing: &Landing) -> Option<PathBuf> {
    crate::dispatch::append(&log_path(plane, ws, host), plane, &landing.to_value())
}

/// The last landing declared for each member of `slug`, by its `ts`, read from every host's
/// log: a change worked from two machines is still one change. A line that is not a landing,
/// and a file or directory containment refuses, is skipped: this is bookkeeping beside git.
pub fn landings(plane: &Path, ws: &str, slug: &str) -> Landings {
    let dir = log_dir(plane, ws);
    let mut out = BTreeMap::new();
    if contain::readable(plane, &dir).is_err() {
        return out;
    }
    let Ok(listing) = std::fs::read_dir(&dir) else {
        return out;
    };
    let mut files: Vec<PathBuf> = listing
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    files.sort();
    let mut lines: Vec<Landing> = Vec::new();
    for file in files {
        let Ok(text) = contain::read_text_no_link(plane, &file) else {
            continue;
        };
        lines.extend(
            text.lines()
                .filter_map(|raw| serde_json::from_str::<Value>(raw).ok())
                .filter_map(|value| Landing::of(&value))
                .filter(|landing| landing.change == slug),
        );
    }
    // A stable sort by `ts`, so the latest landing of a member wins.
    lines.sort_by(|a, b| a.ts.cmp(&b.ts));
    for landing in lines {
        out.insert(landing.repo.clone(), landing);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
        (dir, root)
    }

    fn at(second: u32) -> DateTime<Utc> {
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 10, 2, 9, 0, second).unwrap()
    }

    /// The line on disk is the plane format's: exactly `LOG_FIELDS`, written in that order,
    /// ASCII only, one line ending in a newline.
    #[test]
    fn the_log_line_matches_log_fields() {
        let (_d, root) = plane();
        let landing = Landing::new("api-2", "widgét", 7, "a".repeat(40).as_str(), "b1", at(5));
        let path = append(&root, "alpha", "laptop", &landing).expect("written");
        assert_eq!(path, root.join("workspaces/alpha/changes/log/laptop.jsonl"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            format!(
                "{{\"change\": \"api-2\", \"head\": \"{}\", \"merge\": \"b1\", \"number\": 7, \
                 \"repo\": \"widg\\u00e9t\", \"ts\": \"2026-10-02T09:00:05+00:00\"}}\n",
                "a".repeat(40)
            )
        );
        let read: Value = serde_json::from_str(text.trim_end()).unwrap();
        let mut keys: Vec<&str> = read
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, LOG_FIELDS);
    }

    /// Every host's file is read, the latest line per member wins, and a line with a key too
    /// many, a key too few or the wrong type is skipped.
    #[test]
    fn the_latest_landing_per_member_is_read_from_every_host_and_a_line_that_is_not_one_skipped() {
        let (_d, root) = plane();
        append(
            &root,
            "alpha",
            "b-host",
            &Landing::new("api-2", "web", 3, "h1", "m1", at(1)),
        );
        append(
            &root,
            "alpha",
            "a-host",
            &Landing::new("api-2", "web", 4, "h2", "m2", at(2)),
        );
        append(
            &root,
            "alpha",
            "a-host",
            &Landing::new("other", "web", 9, "h9", "m9", at(3)),
        );
        let log = log_path(&root, "alpha", "a-host");
        let mut text = std::fs::read_to_string(&log).unwrap();
        text.push_str("{\"change\": \"api-2\", \"head\": \"h\", \"merge\": \"m\", \"number\": 1, \"repo\": \"svc\", \"ts\": \"x\", \"state\": \"landed\"}\n");
        text.push_str("{\"change\": \"api-2\", \"head\": \"h\", \"merge\": \"m\", \"number\": \"1\", \"repo\": \"svc\", \"ts\": \"x\"}\n");
        text.push_str("{\"change\": \"api-2\", \"merge\": \"m\", \"number\": 1, \"repo\": \"svc\", \"ts\": \"x\"}\n");
        text.push_str("not json\n");
        std::fs::write(&log, text).unwrap();

        let found = landings(&root, "alpha", "api-2");
        assert_eq!(found.keys().collect::<Vec<_>>(), vec!["web"]);
        assert_eq!(
            (found["web"].number, found["web"].merge.as_str()),
            (4, "m2")
        );
    }

    /// A log directory that leads out of the plane is neither read nor written.
    #[test]
    fn a_log_directory_that_links_out_of_the_plane_is_neither_read_nor_written() {
        let (_d, root) = plane();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.join("workspaces/alpha/changes")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("workspaces/alpha/changes/log"))
            .unwrap();
        std::fs::write(
            outside.path().join("x.jsonl"),
            "{\"change\": \"api-2\", \"head\": \"h\", \"merge\": \"m\", \"number\": 1, \"repo\": \"svc\", \"ts\": \"x\"}\n",
        )
        .unwrap();
        assert!(landings(&root, "alpha", "api-2").is_empty());
        assert_eq!(
            append(
                &root,
                "alpha",
                "h",
                &Landing::new("api-2", "svc", 1, "h", "m", at(1))
            ),
            None
        );
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 1);
    }
}
