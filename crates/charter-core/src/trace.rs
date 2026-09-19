//! The per-session activity trace — `charter/trace.py`: one JSON object per line under
//! `.charter/persona-state/trace/<session>.jsonl`, appended as things happen.
//!
//! Best effort, always: observability must never break the thing it observes, so a
//! record that cannot be written is dropped silently, and a line that cannot be parsed is
//! skipped on read. It is charter's own state, so it is written the way charter writes its
//! state — the directories it creates at 0700 and the file at 0600, whatever the umask.

use std::path::{Path, PathBuf};

/// The bucket a session's state lives under when there is no session.
pub const NO_SESSION: &str = "nosession";

/// This session's bucket name — `session.bucket`.
///
/// `$CHARTER_SESSION_ID`, then `$CLAUDE_CODE_SESSION_ID`, the first that is set and not
/// empty; stripped, and every character outside `[A-Za-z0-9._-]` dropped, because it
/// becomes a filename. [`NO_SESSION`] when nothing is left.
pub fn bucket(env: &dyn Fn(&str) -> Option<String>) -> String {
    let raw = ["CHARTER_SESSION_ID", "CLAUDE_CODE_SESSION_ID"]
        .iter()
        .find_map(|name| env(name).filter(|v| !v.is_empty()));
    let Some(raw) = raw else {
        return NO_SESSION.to_string();
    };
    let safe: String = crate::memstore::py_strip(&raw)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    if safe.is_empty() {
        NO_SESSION.to_string()
    } else {
        safe
    }
}

/// The trace file of one session.
pub fn file(root: &Path, session: &str) -> PathBuf {
    root.join(".charter/persona-state/trace")
        .join(format!("{session}.jsonl"))
}

/// Append one event — `trace.record`. `fields` are written after `ts` and `event`, in the
/// order given; the timestamp is local, to the second.
pub fn record(
    root: &Path,
    session: &str,
    event: &str,
    fields: &[(&str, &str)],
    stamp: chrono::NaiveDateTime,
) {
    let path = file(root, session);
    let mut rec = serde_json::Map::new();
    rec.insert(
        "ts".into(),
        stamp.format("%Y-%m-%dT%H:%M:%S").to_string().into(),
    );
    rec.insert("event".into(), event.into());
    for (key, value) in fields {
        rec.insert((*key).into(), (*value).into());
    }
    // `contain.json_line`: `json.dumps(rec)` with Python's own separators, and
    // `ensure_ascii`, which is the whole of what keeps one record on one physical line.
    let line = format!(
        "{}\n",
        crate::pyjson::dumps(&serde_json::Value::Object(rec), None, ", ", ": ")
    );
    let _ = append_private(root, &path, line.as_bytes());
}

/// Append `bytes` to a file of charter's own state: every directory it has to make at
/// 0700, the file at 0600 — an existing one tightened too, before a byte is added, because
/// a file charter is putting its own bytes into is charter's whatever its history.
///
/// Contained first. charter does not ask here; the plane's own gate costs nothing and the
/// trace sits in a directory a commit cannot reach but a link can still be left in.
pub fn append_private(root: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    crate::contain::writable(root, path)
        .map_err(|r| std::io::Error::new(std::io::ErrorKind::PermissionDenied, r.to_string()))?;
    if let Some(parent) = path.parent() {
        private_mkdir(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = out.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    std::io::Write::write_all(&mut out, bytes)
}

/// Create `dir` and every missing level above it at 0700 — `config.private_mkdir`. A
/// directory that already exists is left exactly as it is.
pub fn private_mkdir(dir: &Path) -> std::io::Result<()> {
    let mut missing = Vec::new();
    let mut at = dir;
    while !at.exists() {
        missing.push(at.to_path_buf());
        match at.parent() {
            Some(parent) if parent != at => at = parent,
            _ => break,
        }
    }
    for level in missing.into_iter().rev() {
        match std::fs::create_dir(&level) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&level, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

/// This session's events attributed to `persona`, the last `n` of them (`0` for all) —
/// `trace.for_persona`. A line that is not a JSON object is skipped.
pub fn for_persona(
    root: &Path,
    session: &str,
    persona: &str,
    n: i64,
) -> Vec<serde_json::Map<String, serde_json::Value>> {
    let path = file(root, session);
    if !crate::memstore::readable_file(root, &path) {
        return Vec::new();
    }
    let Some(text) = crate::memstore::read_text(&path) else {
        return Vec::new();
    };
    let events: Vec<serde_json::Map<String, serde_json::Value>> =
        crate::mdsection::split_lines(&text)
            .into_iter()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter_map(|value| match value {
                serde_json::Value::Object(map) => Some(map),
                _ => None,
            })
            .filter(|map| map.get("persona").and_then(|v| v.as_str()) == Some(persona))
            .collect();
    if n == 0 {
        return events;
    }
    // `evs[-n:]`: the last n, and for a negative n everything after the first |n|.
    let len = events.len() as i64;
    let from = if n > 0 {
        (len - n).max(0)
    } else {
        (-n).min(len)
    };
    events.into_iter().skip(from as usize).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn the_bucket_is_charters_session_id_made_safe_for_a_filename() {
        assert_eq!(
            bucket(&env(&[("CHARTER_SESSION_ID", "fixture-session-1")])),
            "fixture-session-1"
        );
        assert_eq!(bucket(&env(&[("CHARTER_SESSION_ID", " a/b:c ")])), "abc");
        assert_eq!(
            bucket(&env(&[
                ("CHARTER_SESSION_ID", ""),
                ("CLAUDE_CODE_SESSION_ID", "x.y")
            ])),
            "x.y",
            "an empty id is no id, and the next rung answers"
        );
        assert_eq!(bucket(&env(&[])), NO_SESSION);
        assert_eq!(bucket(&env(&[("CHARTER_SESSION_ID", "///")])), NO_SESSION);
    }

    #[test]
    fn a_record_is_one_line_in_pythons_json_with_the_keys_in_the_order_written() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        let stamp = "2026-05-04T11:32:17".parse().unwrap();

        record(
            dir.path(),
            "s1",
            "memory",
            &[
                ("persona", "devops"),
                ("scope", "own"),
                ("title", "é\u{2028}x"),
            ],
            stamp,
        );

        assert_eq!(
            std::fs::read_to_string(file(dir.path(), "s1")).unwrap(),
            "{\"ts\": \"2026-05-04T11:32:17\", \"event\": \"memory\", \"persona\": \"devops\", \"scope\": \"own\", \"title\": \"\\u00e9\\u2028x\"}\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_trace_is_private_whatever_mode_it_had() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        let path = file(dir.path(), "s1");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        record(
            dir.path(),
            "s1",
            "memory",
            &[],
            "2026-05-04T11:32:17".parse().unwrap(),
        );

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn a_persona_reads_back_only_its_own_events_the_last_n_of_them() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        let path = file(dir.path(), "s1");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "{\"event\": \"a\", \"persona\": \"devops\"}\nnot json\n[1]\n{\"event\": \"b\", \"persona\": \"qa\"}\n{\"event\": \"c\", \"persona\": \"devops\"}\n",
        )
        .unwrap();

        let events = |n| -> Vec<String> {
            for_persona(dir.path(), "s1", "devops", n)
                .iter()
                .map(|e| e["event"].as_str().unwrap().to_string())
                .collect()
        };

        assert_eq!(events(0), vec!["a", "c"]);
        assert_eq!(events(1), vec!["c"]);
        assert_eq!(events(-1), vec!["c"], "evs[1:]");
    }
}
