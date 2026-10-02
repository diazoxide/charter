//! `work links`: the work link log's lines charter skipped, and alias cycles a merge made
//! (ADR 0088 §2, §3).
//!
//! **Shown only when there is something to say.** A project nothing has linked or promoted has
//! no log, and a healthy one has nothing to report; a row on every run would be furniture.

use super::{Doctor, Row};
use crate::work::log;

const NAME: &str = "work links";

pub(super) fn work_links(d: &Doctor) -> Option<Row> {
    if !d.has_plane {
        return None;
    }
    let folded = log::fold(&d.root);
    let cycles = folded.cycles();
    if !cycles.is_empty() {
        let keys: Vec<String> = cycles.iter().map(ToString::to_string).collect();
        return Some(Row::fail(
            NAME,
            format!("an alias cycle: {}", keys.join(", ")),
            "Two devices' logs, merged, say each key stands for the other. Readers stop before \
             the first key they would repeat. The newest alias is the one to undo: remove its \
             line from workspaces/<ws>/work/<device>.jsonl and save.",
        ));
    }
    let skipped = folded.skipped();
    (skipped > 0).then(|| {
        Row::warn(
            NAME,
            format!("{skipped} line(s) skipped: not one of the four closed key sets"),
            "A line of workspaces/<ws>/work/*.jsonl that charter did not write, or a newer \
             charter wrote. It is read as absent; nothing else in the log is affected.",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doctor(root: &std::path::Path) -> Doctor {
        std::fs::write(root.join(crate::plane::MANIFEST), "schema = 1\n").unwrap();
        Doctor::at(root, root, true, false)
    }

    #[test]
    fn a_project_with_a_clean_log_or_none_has_no_row() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(work_links(&doctor(tmp.path())).is_none());
    }

    #[test]
    fn a_skipped_line_warns_and_a_cycle_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = log::dir_for(tmp.path(), "alpha");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("01K6H0Z8Y3V1N3G4QK0A9T5B7C.jsonl");
        std::fs::write(&file, "{\"v\":1}\n").unwrap();
        let row = work_links(&doctor(tmp.path())).expect("a row");
        assert_eq!(row.status, super::super::Status::Warn);
        assert!(
            row.detail.starts_with("1 line(s) skipped"),
            "{}",
            row.detail
        );

        let a = "todo:alpha/x";
        let b = "github:github.com/o/r#1";
        let line = |from: &str, to: &str| {
            format!(
                "{{\"v\":1,\"ts\":\"2026-10-02T08:00:00Z\",\"op\":\"alias\",\"from\":\"{from}\",\
                 \"to\":\"{to}\",\"cause\":\"moved\"}}\n"
            )
        };
        std::fs::write(&file, format!("{}{}", line(a, b), line(b, a))).unwrap();
        let row = work_links(&doctor(tmp.path())).expect("a row");
        assert_eq!(row.status, super::super::Status::Fail);
        assert!(
            row.detail.contains(a) && row.detail.contains(b),
            "{}",
            row.detail
        );
    }
}
