//! Which sub-agents are running right now — the dispatch half of `charter/inflight.py`.
//!
//! `pretooluse-dispatch` writes one record as a `Task`/`Agent` call goes out and
//! `posttooluse-dispatch` removes it when the call returns, so between the two the records are
//! the sub-agents still working. The one reader is `pretooluse-dispatch` itself: a persona that
//! writes code (`dispatch-isolation: worktree`) dispatched while another agent is still running
//! shares a working tree with it, and the operator is asked first.
//!
//! **A record is presumed dead after [`PRESUMED_DEAD_SECS`]** — a sub-agent that crashed or was
//! interrupted never reaches its `PostToolUse`, and a record that never expires would ask about
//! a peer that is long gone on every dispatch for the rest of the day. Records older than
//! [`PRUNE_SECS`] are deleted whenever the directory is read.
//!
//! The tmux frame's turn markers (`chat-turns/`) and the clone/refresh kinds live in the same
//! Python module and are not here: the app has its own record of which chat is working
//! (`hookwire`), and no hook in charter-app starts a clone or a refresh.

use std::path::{Path, PathBuf};

use crate::hookstate::State;

/// `inflight.PRESUMED_DEAD_SECONDS` — thirty minutes.
pub const PRESUMED_DEAD_SECS: f64 = 30.0 * 60.0;

/// `inflight.PRUNE_SECONDS` — a day.
pub const PRUNE_SECS: f64 = 24.0 * 60.0 * 60.0;

/// The only kind charter-app writes — `inflight.DISPATCH`.
pub const DISPATCH: &str = "dispatch";

/// `<state>/dispatch-inflight`.
pub fn dir(state: &State) -> PathBuf {
    state.dir().join("dispatch-inflight")
}

/// `_safe_name`: every character outside `[A-Za-z0-9._-]` becomes `_`, at most 64.
fn safe_name(agent: &str) -> String {
    agent
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect()
}

fn seconds(t: std::time::SystemTime) -> f64 {
    t.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn mtime(path: &Path) -> Option<f64> {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .map(seconds)
}

/// A record's kind; a record with none is a dispatch, which is every record older charters
/// wrote.
fn kind_of(rec: &serde_json::Value) -> &str {
    rec.get("kind")
        .and_then(serde_json::Value::as_str)
        .filter(|k| !k.is_empty())
        .unwrap_or(DISPATCH)
}

/// The dispatches still running, by agent name, sorted — `inflight.still_running()`.
///
/// Prunes what is past [`PRUNE_SECS`] on the way, as charter's reader does.
pub fn still_running(state: &State, now: f64) -> Vec<String> {
    let Ok(reader) = std::fs::read_dir(dir(state)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in reader.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(mt) = mtime(&path) else { continue };
        if now - mt > PRUNE_SECS {
            let _ = state.remove(&path);
            continue;
        }
        let Some(rec) = state
            .read_text(&path)
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        else {
            continue;
        };
        if kind_of(&rec) != DISPATCH {
            continue;
        }
        let started = rec
            .get("ts")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(mt);
        if now - started > PRESUMED_DEAD_SECS {
            continue;
        }
        let name = rec
            .get("agent")
            .and_then(serde_json::Value::as_str)
            .filter(|a| !a.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                path.file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
        out.push(name);
    }
    out.sort();
    out
}

/// Record that a dispatch to `agent` is starting — `inflight.start`. Returns the record's
/// token (its file stem), or `None` when nothing was written.
pub fn start(state: &State, agent: &str, now: f64) -> Option<String> {
    use std::io::Write;
    let agent = crate::memstore::py_strip(agent);
    if agent.is_empty() {
        return None;
    }
    let d = dir(state);
    state.mkdir(&d).ok()?;
    let token = format!(
        "{}.{}",
        safe_name(agent),
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let path = d.join(format!("{token}.json"));
    // `create_new` is `O_CREAT | O_EXCL`, which refuses a name that is already there — a link
    // included — so the one thing `mkstemp` promises is kept: this record is a new file.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).ok()?;
    let body = format!(
        "{{\"agent\": {}, \"kind\": \"{DISPATCH}\", \"ts\": {}}}",
        crate::pyjson::dumps_sorted(&serde_json::Value::String(agent.to_string())),
        py_float(now)
    );
    file.write_all(body.as_bytes()).ok()?;
    Some(token)
}

/// A float as Python's `repr` writes one at this magnitude: shortest round-trip digits, and a
/// `.0` on a whole number — `time.time()` pinned to a whole second is `1777894337.0`, not
/// `1777894337`.
fn py_float(value: f64) -> String {
    let text = format!("{value}");
    if value.is_finite() && !text.contains(['.', 'e', 'E']) {
        format!("{text}.0")
    } else {
        text
    }
}

/// Remove one running record for `agent` — `inflight.finish(agent)`: the OLDEST of those still
/// presumed alive, else the oldest of any age.
pub fn finish(state: &State, agent: &str, now: f64) {
    let agent = crate::memstore::py_strip(agent);
    if agent.is_empty() {
        return;
    }
    let prefix = format!("{}.", safe_name(agent));
    let Ok(reader) = std::fs::read_dir(dir(state)) else {
        return;
    };
    let mut matches: Vec<(f64, PathBuf)> = reader
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            name.starts_with(&prefix) && name.ends_with(".json") && name.len() > prefix.len() + 5
        })
        .filter(|p| {
            state
                .read_text(p)
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .is_some_and(|rec| kind_of(&rec) == DISPATCH)
        })
        .filter_map(|p| mtime(&p).map(|m| (m, p)))
        .collect();
    matches.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let running: Vec<&(f64, PathBuf)> = matches
        .iter()
        .filter(|(m, _)| now - m <= PRESUMED_DEAD_SECS)
        .collect();
    let target = running
        .first()
        .map(|(_, p)| p.clone())
        .or_else(|| matches.first().map(|(_, p)| p.clone()));
    if let Some(path) = target {
        let _ = state.remove(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> f64 {
        seconds(std::time::SystemTime::now())
    }

    fn state() -> (tempfile::TempDir, State) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        (dir, State::of(&root))
    }

    #[test]
    fn a_dispatch_runs_from_start_to_finish() {
        let (_d, state) = state();
        assert!(still_running(&state, now()).is_empty());
        start(&state, "devops", now()).unwrap();
        start(&state, "web", now()).unwrap();
        assert_eq!(still_running(&state, now()), vec!["devops", "web"]);
        finish(&state, "devops", now());
        assert_eq!(still_running(&state, now()), vec!["web"]);
        finish(&state, "nobody", now());
        assert_eq!(still_running(&state, now()), vec!["web"]);
    }

    #[test]
    fn a_record_past_half_an_hour_is_presumed_dead() {
        let (_d, state) = state();
        start(&state, "devops", now() - PRESUMED_DEAD_SECS - 5.0).unwrap();
        assert!(still_running(&state, now()).is_empty());
    }

    #[test]
    fn a_record_is_written_the_way_charter_writes_one() {
        let (_d, state) = state();
        let token = start(&state, "dev ops", 1.5).unwrap();
        assert!(token.starts_with("dev_ops."));
        let text = std::fs::read_to_string(dir(&state).join(format!("{token}.json"))).unwrap();
        assert_eq!(
            text,
            "{\"agent\": \"dev ops\", \"kind\": \"dispatch\", \"ts\": 1.5}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir(&state)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }
    }

    /// Writes `body` as the record `name` with its mtime at `mtime`, a whole second.
    fn record_at(state: &State, name: &str, body: &str, mtime: u64) -> PathBuf {
        std::fs::create_dir_all(dir(state)).unwrap();
        let path = dir(state).join(name);
        std::fs::write(&path, body).unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime))
            .unwrap();
        path
    }

    #[test]
    fn a_record_is_presumed_dead_after_thirty_minutes_and_pruned_after_a_day() {
        assert_eq!(PRESUMED_DEAD_SECS, 1800.0);
        assert_eq!(PRUNE_SECS, 86_400.0);
    }

    #[test]
    fn a_record_with_an_empty_kind_is_a_dispatch_and_another_kind_is_not() {
        let (_d, state) = state();
        let t = now();
        record_at(
            &state,
            "a.1.json",
            &format!(r#"{{"agent": "a", "kind": "", "ts": {t}}}"#),
            t as u64,
        );
        record_at(
            &state,
            "b.1.json",
            &format!(r#"{{"agent": "b", "kind": "clone", "ts": {t}}}"#),
            t as u64,
        );
        assert_eq!(still_running(&state, t), vec!["a"]);
    }

    #[test]
    fn a_record_exactly_at_either_age_is_still_counted() {
        let (_d, state) = state();
        let mtime: u64 = 1_700_000_000;
        let now = mtime as f64 + PRUNE_SECS;
        // Written a day ago to the second, started thirty minutes ago to the second: neither
        // bound is passed, so it is running and its file stays.
        let at_both = record_at(
            &state,
            "edge.1.json",
            &format!(
                r#"{{"agent": "edge", "ts": {}}}"#,
                py_float(now - PRESUMED_DEAD_SECS)
            ),
            mtime,
        );
        // Started a second before that: presumed dead, and not pruned for it.
        let dead = record_at(
            &state,
            "dead.1.json",
            &format!(
                r#"{{"agent": "dead", "ts": {}}}"#,
                py_float(now - PRESUMED_DEAD_SECS - 1.0)
            ),
            mtime,
        );
        // Written a second before a day ago: pruned, whatever its own clock says.
        let old = record_at(
            &state,
            "old.1.json",
            &format!(r#"{{"agent": "old", "ts": {}}}"#, py_float(now)),
            mtime - 1,
        );
        // Written two hours ago and started now: running, not pruned.
        let fresh = record_at(
            &state,
            "fresh.1.json",
            &format!(r#"{{"agent": "fresh", "ts": {}}}"#, py_float(now)),
            now as u64 - 7200,
        );

        assert_eq!(still_running(&state, now), vec!["edge", "fresh"]);
        assert!(at_both.exists() && dead.exists() && fresh.exists());
        assert!(!old.exists(), "a record past a day is pruned");
    }

    #[test]
    fn finish_takes_the_oldest_record_still_alive_before_an_older_dead_one() {
        // `inflight.finish`: the OLDEST of those still presumed alive, else the oldest of any
        // age. A dead record is older than every live one, so "oldest" alone would take it.
        let (_d, state) = state();
        let now: u64 = 1_700_000_000;
        let body = r#"{"agent": "devops", "kind": "dispatch"}"#;
        let dead = record_at(&state, "devops.dead.json", body, now - 3600);
        let alive = record_at(&state, "devops.alive.json", body, now - 60);
        let newer = record_at(&state, "devops.newer.json", body, now - 10);

        finish(&state, "devops", now as f64);
        assert!(dead.exists() && newer.exists());
        assert!(
            !alive.exists(),
            "finish took a record other than the oldest live one"
        );

        // With nothing alive left but `newer`, and then nothing alive at all, the oldest goes.
        finish(&state, "devops", now as f64);
        assert!(!newer.exists());
        finish(&state, "devops", now as f64);
        assert!(!dead.exists());
    }

    #[test]
    fn finish_never_takes_a_record_whose_name_is_only_the_prefix() {
        // `devops..json` starts with `devops.` and ends with `.json`, and names no record `start`
        // could have written: `start` always puts a token between the two.
        let (_d, state) = state();
        let t = now();
        let bare = record_at(
            &state,
            "devops..json",
            &format!(r#"{{"agent": "devops", "kind": "dispatch", "ts": {t}}}"#),
            t as u64,
        );
        finish(&state, "devops", t);
        assert!(bare.exists(), "finish removed a record start never names");

        let token = start(&state, "devops", t).unwrap();
        finish(&state, "devops", t);
        assert!(!dir(&state).join(format!("{token}.json")).exists());
        assert!(bare.exists());
    }
}
