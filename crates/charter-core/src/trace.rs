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
/// [`crate::active::session_id`] is the id, and [`NO_SESSION`] is what stands in when there
/// is none. **One reader, asked twice**: the same two variables key the workspace and persona
/// pointers, and M2.9 found this function and its own copy of the rule already written side
/// by side. A sentinel here and `None` there is charter's own split (`session.bucket` versus
/// `session.current`) and is the whole of the difference — the sentinel is a SHARED key, so
/// everything that can represent absence does.
pub fn bucket(env: &dyn Fn(&str) -> Option<String>) -> String {
    crate::active::session_id(env).unwrap_or_else(|| NO_SESSION.to_string())
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

/// [`record`] for fields that are not all text — `trace.record(event, **fields)` where a field
/// is a list, a bool or a number. A `null` field is left out, as Python leaves out `None`.
pub fn record_values(
    root: &Path,
    session: &str,
    event: &str,
    fields: &[(&str, serde_json::Value)],
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
        if !value.is_null() {
            rec.insert((*key).into(), value.clone());
        }
    }
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
    // `O_NOFOLLOW`: a link swapped in after the containment answer is refused, not followed.
    let mut out = crate::contain::nofollow(&mut options).open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = out.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    std::io::Write::write_all(&mut out, bytes)
}

/// Create `dir` and every missing level above it at 0700 — `config.private_mkdir`. A
/// directory that already exists is left exactly as it is.
///
/// **The mode goes on the `mkdir`, and the chmod after it may fail** (M3). Two divergences
/// from `config._mkdir_0700` lived here, and both were in the unsafe direction:
///
/// - `fs::create_dir` asks for 0777, so under the ordinary `umask 022` this directory
///   existed at 0755 between the create and the chmod — and it holds a session's trace,
///   which carries `persona note`'s text. Python has never had that window: `mkdir(mode=)`
///   names the bits outright. `DirBuilder::mode` is the same call. The chmod is kept for
///   Python's own stated reason — mkdir's mode is masked by the umask, so a process under
///   a permissive umask is not guaranteed the bits it asked for.
/// - That chmod propagated with `?`, where Python's is `except OSError: pass`. On a
///   filesystem with no modes — exFAT, some network mounts — charter dropped the record
///   Python writes. Observability may not be the thing that fails.
pub fn private_mkdir(dir: &Path) -> std::io::Result<()> {
    let mut missing = Vec::new();
    let mut at = dir;
    while !at.exists() {
        missing.push(at.to_path_buf());
        // `Path::parent` never answers the path itself, so no guard against a loop is needed.
        match at.parent() {
            Some(parent) => at = parent,
            None => break,
        }
    }
    for level in missing.into_iter().rev() {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&level) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&level, std::fs::Permissions::from_mode(0o700));
        }
    }
    Ok(())
}

/// One trace event as a persona's activity reads it: `ts`, the event padded to ten, then every
/// other field but `persona` as `key=value` — `persona recall`'s and `persona log`'s line,
/// Python's `f"{r['ts']}  {r['event']:10} {extra}"`, a number right-aligned as `:10` aligns it.
pub fn activity_line(act: &serde_json::Map<String, serde_json::Value>) -> String {
    let text = crate::pyrepr::str_json;
    let extra: Vec<String> = act
        .iter()
        .filter(|(k, _)| !matches!(k.as_str(), "ts" | "event" | "persona"))
        .map(|(k, v)| format!("{k}={}", text(v)))
        .collect();
    let ts = act.get("ts").map(text).unwrap_or_default();
    let event = match act.get("event") {
        // A number the way Python prints what `json.loads` read (`1E5` is `100000.0`), and
        // right-aligned, as `:10` aligns a number.
        Some(v @ serde_json::Value::Number(_)) => format!("{:>10}", text(v)),
        Some(v) => format!("{:<10}", text(v)),
        None => format!("{:<10}", ""),
    };
    format!("{ts}  {event} {}", extra.join("  "))
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
    // `evs[-n:]`: the last n, and for a negative n everything after the first |n|.
    let len = events.len() as i64;
    let from = match n.cmp(&0) {
        std::cmp::Ordering::Equal => return events,
        std::cmp::Ordering::Greater => (len - n).max(0),
        std::cmp::Ordering::Less => (-n).min(len),
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

    #[test]
    fn a_record_of_values_keeps_their_types_and_leaves_out_a_null() {
        // `trace.record(event, **fields)` with a list, a bool and a number, and a `None` that
        // Python's record leaves out rather than writing `null`.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();

        record_values(
            dir.path(),
            "s1",
            "persona-use",
            &[
                ("persona", serde_json::json!("devops")),
                ("tools", serde_json::json!(["gh", "kubectl"])),
                ("gone", serde_json::Value::Null),
                ("frozen", serde_json::json!(true)),
                ("count", serde_json::json!(2)),
            ],
            "2026-05-04T11:32:17".parse().unwrap(),
        );

        assert_eq!(
            std::fs::read_to_string(file(dir.path(), "s1")).unwrap(),
            "{\"ts\": \"2026-05-04T11:32:17\", \"event\": \"persona-use\", \"persona\": \"devops\", \"tools\": [\"gh\", \"kubectl\"], \"frozen\": true, \"count\": 2}\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn every_directory_the_trace_makes_is_private() {
        // The END state, which is all a test can see: the 0755 window between `create_dir`
        // and the chmod that used to exist here is a race, and closing it is a matter of
        // which call is made rather than of what is on disk afterwards. So this pins the
        // mode, and `private_mkdir`'s docstring carries the window.
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();

        record(
            dir.path(),
            "s1",
            "memory",
            &[],
            "2026-05-04T11:32:17".parse().unwrap(),
        );

        for level in [
            ".charter",
            ".charter/persona-state",
            ".charter/persona-state/trace",
        ] {
            let mode = std::fs::metadata(dir.path().join(level))
                .unwrap_or_else(|_| panic!("{level} was made"))
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700, "{level}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_level_already_there_is_left_and_one_that_cannot_be_made_is_an_error() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        // A dangling link does not `exist()`, so it is a level to make — and making it answers
        // "already exists", which is somebody else's level and not a failure.
        let dangling = dir.path().join("dangling");
        std::os::unix::fs::symlink(dir.path().join("nowhere"), &dangling).unwrap();
        assert!(private_mkdir(&dangling).is_ok());
        assert!(std::fs::symlink_metadata(&dangling).unwrap().is_symlink());

        // A parent nobody may write into: the error is the answer, not swallowed.
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        let refused = private_mkdir(&locked.join("a/b"));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(refused.is_err());
        assert!(!locked.join("a").exists());
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

    #[cfg(unix)]
    #[test]
    fn a_trace_that_is_a_link_is_refused_and_what_it_points_at_is_left_alone() {
        // Inside the plane, so containment passes: the open's `O_NOFOLLOW` is what refuses.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        let victim = dir.path().join(".charter/persona-state/other.json");
        let path = file(dir.path(), "s1");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&victim, "PRECIOUS\n").unwrap();
        std::os::unix::fs::symlink(&victim, &path).unwrap();

        assert!(append_private(dir.path(), &path, b"row\n").is_err());
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "PRECIOUS\n");
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
