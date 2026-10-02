//! Pending landings: `workspaces/<ws>/changes/log/pending/<host>.jsonl`, the evidence that
//! charter started a landing (#472, D-472a).
//!
//! `charter change land` appends a line **before** it asks the forge to merge or to queue a
//! member's request: the change, the member, the request, the head its checks passed on, and
//! how (`direct` or `queue`). A later `land` that finds the request merged at that head records
//! the landing in the landing log; a request merged with no such line, by a person in the
//! browser or before a queue existed, is never recorded as charter's.
//!
//! The line is updated by appending another: `refused` when the forge refused the call, so a
//! later merge of the same head by somebody else is not taken for charter's; `merge-later` when
//! GitLab set the request to merge later and charter could not undo it, which doctor names.
//! The latest line per member wins.
//!
//! It sits under `changes/log/`, which the LIVE block re-ignores, so it is never committed. It
//! is a directory of its own, so no reader of the landing log (this charter's or the Python
//! one's, both of which read `log/*.jsonl`) ever reads it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::landing::log_dir;
use crate::contain;

/// The directory, relative to the landing log's.
pub const PENDING_DIRNAME: &str = "pending";

/// How the landing was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// Merged now.
    Direct,
    /// Put in the target branch's merge queue or merge train.
    Queue,
}

impl Via {
    fn word(self) -> &'static str {
        match self {
            Via::Direct => "direct",
            Via::Queue => "queue",
        }
    }
}

/// Where the landing stands, as charter last saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Charter asked the forge, and has not seen the merge yet.
    Asked,
    /// The forge refused; nothing charter asked for will merge.
    Refused,
    /// GitLab set it to merge later and charter could not undo that.
    MergeLater,
}

impl Stage {
    fn word(self) -> &'static str {
        match self {
            Stage::Asked => "asked",
            Stage::Refused => "refused",
            Stage::MergeLater => "merge-later",
        }
    }
}

/// One line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub ts: String,
    pub change: String,
    pub repo: String,
    pub number: u64,
    pub head: String,
    pub via: Via,
    pub stage: Stage,
}

/// The latest pending line for each member, by repo.
pub type Pendings = BTreeMap<String, Pending>;

impl Pending {
    /// A line about `repo` of `change`, made at `when`.
    pub fn new(
        change: &str,
        repo: &str,
        number: u64,
        head: &str,
        via: Via,
        stage: Stage,
        when: DateTime<Utc>,
    ) -> Pending {
        Pending {
            ts: crate::dispatch::stamp(when),
            change: change.to_string(),
            repo: repo.to_string(),
            number,
            head: head.to_string(),
            via,
            stage,
        }
    }

    /// This line moved on to `stage`, at `when`.
    pub fn at(&self, stage: Stage, when: DateTime<Utc>) -> Pending {
        Pending {
            ts: crate::dispatch::stamp(when),
            stage,
            ..self.clone()
        }
    }

    /// Whether this is charter's landing of request `number` at `head`, still standing.
    pub fn started(&self, number: u64, head: &str) -> bool {
        self.number == number && self.head == head && self.stage != Stage::Refused
    }

    fn to_value(&self) -> Value {
        json!({
            "ts": self.ts,
            "change": self.change,
            "repo": self.repo,
            "number": self.number,
            "head": self.head,
            "via": self.via.word(),
            "stage": self.stage.word(),
        })
    }

    fn of(value: &Value) -> Option<Pending> {
        let line = value.as_object()?;
        if line.len() != 7 {
            return None;
        }
        let text = |key: &str| line.get(key)?.as_str().map(str::to_string);
        let via = match text("via")?.as_str() {
            "direct" => Via::Direct,
            "queue" => Via::Queue,
            _ => return None,
        };
        let stage = match text("stage")?.as_str() {
            "asked" => Stage::Asked,
            "refused" => Stage::Refused,
            "merge-later" => Stage::MergeLater,
            _ => return None,
        };
        Some(Pending {
            ts: text("ts")?,
            change: text("change")?,
            repo: text("repo")?,
            number: line.get("number")?.as_u64()?,
            head: text("head")?,
            via,
            stage,
        })
    }
}

/// `workspaces/<ws>/changes/log/pending/`.
pub fn pending_dir(plane: &Path, ws: &str) -> PathBuf {
    log_dir(plane, ws).join(PENDING_DIRNAME)
}

/// Append `line` to `host`'s file. `None` when containment or the disk refused it.
pub fn append(plane: &Path, ws: &str, host: &str, line: &Pending) -> Option<PathBuf> {
    let path = pending_dir(plane, ws).join(format!("{host}.jsonl"));
    crate::dispatch::append(&path, plane, &line.to_value())
}

/// The latest pending line for each member of `slug`, from every host's file.
pub fn pendings(plane: &Path, ws: &str, slug: &str) -> Pendings {
    let dir = pending_dir(plane, ws);
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
    let mut lines: Vec<Pending> = Vec::new();
    for file in files {
        let Ok(text) = contain::read_text_no_link(plane, &file) else {
            continue;
        };
        lines.extend(
            text.lines()
                .filter_map(|raw| serde_json::from_str::<Value>(raw).ok())
                .filter_map(|value| Pending::of(&value))
                .filter(|p| p.change == slug),
        );
    }
    lines.sort_by(|a, b| a.ts.cmp(&b.ts));
    for line in lines {
        out.insert(line.repo.clone(), line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(second: u32) -> DateTime<Utc> {
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 10, 2, 9, 0, second).unwrap()
    }

    /// The latest line per member wins, a refused one no longer stands, and none of it is read
    /// as a landing.
    #[test]
    fn the_latest_pending_line_wins_and_is_never_a_landing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
        let asked = Pending::new("api-2", "web", 7, "h1", Via::Queue, Stage::Asked, at(1));
        append(&root, "alpha", "laptop", &asked).unwrap();
        assert!(pendings(&root, "alpha", "api-2")["web"].started(7, "h1"));
        assert!(!pendings(&root, "alpha", "api-2")["web"].started(7, "h2"));
        append(&root, "alpha", "laptop", &asked.at(Stage::Refused, at(2))).unwrap();
        assert!(!pendings(&root, "alpha", "api-2")["web"].started(7, "h1"));
        assert!(super::super::landing::landings(&root, "alpha", "api-2").is_empty());
    }
}
