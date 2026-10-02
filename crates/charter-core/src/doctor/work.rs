//! `work links`: the work link log's lines charter skipped, alias cycles a merge made, and todos
//! a promote aliased to an issue but did not get to close (ADR 0088 §2, §3, §5).
//!
//! **It reports and changes nothing.** The close a crash cut short is finished by the next
//! `charter ws todo` in that workspace, which is what the row says to run.
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
    let unclosed = promoted_but_open(d, &folded);
    if let Some(ws) = unclosed.first().map(|(ws, _, _)| ws.clone()) {
        let named: Vec<String> = unclosed
            .iter()
            .map(|(ws, title, to)| format!("{ws}: '{title}' → {to}"))
            .collect();
        return Some(Row::warn(
            NAME,
            format!("promoted but still open: {}", named.join("; ")),
            format!(
                "A promote stopped after its alias, so every reader already treats the todo as \
                 the issue. `charter ws todo -w {ws}` closes it, journalled as promote would have."
            ),
        ));
    }
    let skipped = folded.skipped();
    (skipped > 0).then(|| {
        let files: Vec<String> = folded
            .skipped_in()
            .iter()
            .map(|(file, n)| format!("{n} in workspaces/{}", file.replacen('/', "/work/", 1)))
            .collect();
        Row::warn(
            NAME,
            format!(
                "{skipped} line(s) skipped, not one of the four closed key sets: {}",
                files.join(", ")
            ),
            "Lines charter did not write, or a newer charter wrote. Each is read as absent; \
             nothing else in the log is affected.",
        )
    })
}

/// Every open todo whose key already has an alias: `(workspace, title, where it went)`.
fn promoted_but_open(d: &Doctor, folded: &log::Fold) -> Vec<(String, String, String)> {
    let plane = crate::workspaces::Plane::open(&d.root);
    let Ok(names) = plane.workspaces() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for name in names {
        let Ok(ws) = plane.workspace(&name) else {
            continue;
        };
        for todo in ws.todos().unwrap_or_default() {
            let Ok(key) = crate::work::TrackerKey::todo(&name, &todo.slug) else {
                continue;
            };
            if folded.is_aliased(&key) {
                out.push((name.clone(), todo.title, folded.resolve(&key).to_string()));
            }
        }
    }
    out
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
        assert_eq!(
            row.detail,
            "1 line(s) skipped, not one of the four closed key sets: 1 in \
             workspaces/alpha/work/01K6H0Z8Y3V1N3G4QK0A9T5B7C.jsonl"
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

    #[test]
    fn a_todo_promoted_but_left_open_is_reported_with_where_it_went() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = crate::workspaces::Plane::open(tmp.path())
            .workspace("alpha")
            .unwrap();
        let stamp = chrono::NaiveDate::from_ymd_opt(2026, 10, 2)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap();
        let path = ws.add_todo("Port the picker", stamp).unwrap();
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let d = doctor(tmp.path());
        log::append_alias(
            tmp.path(),
            "alpha",
            "01K6H0Z8Y3V1N3G4QK0A9T5B7C",
            stamp.and_utc(),
            crate::work::TrackerKey::todo("alpha", &stem).unwrap(),
            crate::work::TrackerKey::parse("github:github.com/o/r#1").unwrap(),
            log::Cause::Promoted,
        )
        .unwrap();
        let row = work_links(&d).expect("a row");
        assert_eq!(row.status, super::super::Status::Warn);
        assert_eq!(
            row.detail,
            "promoted but still open: alpha: 'Port the picker' → github:github.com/o/r#1"
        );
        assert!(
            row.hint.contains("charter ws todo -w alpha"),
            "{}",
            row.hint
        );
    }
}
